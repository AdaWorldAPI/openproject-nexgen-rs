// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The default join path derives no translation of positions, whatever the layout
//! of the target: a dimension is reached through its keys as constants, or, with
//! no filter of its own, through one grouped count of the edge column. The derived
//! translation survives as the oracle the default path is compared with.

mod common;

use std::collections::BTreeSet;

use quack_shadow::{Catalog, Layout, rows};
use serde_json::{Value, json};

const REMOVED_TARGET: &str =
    "Permission.joins(:roles).where(roles: { name: 'role2' }).pluck(:name)";

fn joins_capture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-joins.pg.json"
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

fn label(call: &Value) -> &str {
    call["label"].as_str().expect("label")
}

fn find<'a>(entry: &'a Value, wanted: &str) -> &'a Value {
    entry["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .find(|c| label(c) == wanted)
        .unwrap_or_else(|| panic!("no call {wanted}"))
}

fn table<'a>(catalog: &'a Catalog, name: &str) -> &'a quack_shadow::Table {
    catalog
        .tables
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no table {name}"))
}

fn bit(words: &[u64], at: usize) -> bool {
    words
        .get(at / 64)
        .is_some_and(|w| (w >> (at % 64)) & 1 == 1)
}

fn live_count(table: &quack_shadow::Table) -> usize {
    table.live.iter().map(|w| w.count_ones() as usize).sum()
}

fn multiset(rows: &[Vec<Value>]) -> Vec<String> {
    let mut keys: Vec<String> = rows
        .iter()
        .map(|r| serde_json::to_string(r).expect("row json"))
        .collect();
    keys.sort_unstable();
    keys
}

/// `(edge table, edge column, referenced table)` of every join edge of the calls.
fn edges(entry: &Value) -> BTreeSet<(String, String, String)> {
    let mut out = BTreeSet::new();
    for call in entry["calls"].as_array().expect("calls") {
        for join in call["join"].as_array().into_iter().flatten() {
            let (Some(l), Some(r)) = (join.get("left"), join.get("right")) else {
                continue;
            };
            let (key, edge) = if l["col"] == "id" { (l, r) } else { (r, l) };
            let text = |n: &Value, k: &str| n[k].as_str().expect("edge side").to_owned();
            out.insert((text(edge, "table"), text(edge, "col"), text(key, "table")));
        }
    }
    out
}

#[test]
fn the_edge_lane_is_the_position_map() {
    let data = joins_capture();
    let entry = &data["sizes"]["seed"];
    let catalog = common::captured(entry);
    let edges = edges(entry);
    println!("POSITION MAP edges={edges:?}");
    assert_eq!(edges.len(), 3, "edges the joins use");

    for (source, edge_col, target) in &edges {
        let (values, validity) = catalog
            .oracle_link(source, edge_col, target)
            .expect("oracle link");
        let (s, t) = (table(&catalog, source), table(&catalog, target));
        assert_eq!(t.layout, Layout::Identity, "{target}");
        let lane = s
            .columns
            .iter()
            .find(|c| c.name == *edge_col)
            .and_then(|c| c.lane.as_ref())
            .expect("edge lane");
        let mut valid = 0;
        for (i, &key) in lane.values.iter().enumerate() {
            let expected = bit(&lane.validity, i) && bit(&t.live, key as usize);
            assert_eq!(
                bit(&validity, i),
                expected,
                "{source}.{edge_col}[{i}] validity"
            );
            if expected {
                assert_eq!(values[i], key, "{source}.{edge_col}[{i}] position");
                valid += 1;
            }
        }
        println!(
            "POSITION MAP {source}.{edge_col} -> {target}: {valid} of {} rows",
            s.rows
        );
    }
}

#[cfg(not(any(feature = "disable-sql-where", feature = "disable-join-multiplicity")))]
#[test]
fn the_default_path_equals_the_translation_oracle_and_postgresql() {
    use quack_shadow::{Refusal, Verdict, rows_verdict, shadow_verdict};

    // Both catalogs, one without a link and one reading the translation oracle.
    let pair = |calls: &[Value], build: &dyn Fn() -> Catalog| {
        let (mut default_path, mut translation) = (build(), build());
        for catalog in [&mut default_path, &mut translation] {
            if let Some(classes) = common::class_map(&common::models_of(calls)) {
                catalog.set_classes(classes).expect("classes");
            }
        }
        translation.use_translated_positions(true);
        (default_path, translation)
    };

    let data = joins_capture();
    let entry = &data["sizes"]["seed"];
    let calls = entry["calls"].as_array().expect("calls");
    let (default_path, translation) = pair(calls, &|| common::captured(entry));
    let (mut equal, mut refused) = (0, Vec::new());
    for call in calls {
        let (a, v) = (
            rows_verdict(&default_path, call, false),
            rows_verdict(&translation, call, false),
        );
        assert_eq!(
            a,
            v,
            "{}: default path differs from the translation oracle",
            label(call)
        );
        assert_eq!(
            a,
            rows_verdict(&default_path, call, true),
            "{}: fused",
            label(call)
        );
        match a {
            Verdict::Equal(_) => equal += 1,
            Verdict::Refused(r) => refused.push((label(call), r)),
            Verdict::Differ { quack, postgres } => {
                panic!("{}: quack {quack:?} but PostgreSQL {postgres}", label(call))
            }
        }
    }
    assert_eq!((equal, refused.len()), (9, 3), "joins capture");
    let names: Vec<&Refusal> = refused.iter().map(|(_, r)| r).collect();
    assert_eq!(
        names,
        [
            &Refusal::UnsupportedJoin("Arel::Nodes::OuterJoin".to_owned()),
            &Refusal::Distinct,
            &Refusal::JoinOrder,
        ]
    );
    let stats = default_path.link_stats();
    println!("POSITION MAP STATS joins {stats:?}");
    assert_eq!(
        (stats.derived, stats.rows_translated),
        (0, 0),
        "joins: translations derived"
    );
    assert_eq!(
        (
            stats.key_sets,
            stats.keys_in_sets,
            stats.existence_folds,
            stats.dangling_keys,
            stats.placed_late
        ),
        (32, 326, 4, 0, 1136),
        "joins: key sets, keys in sets, existence folds, dangling keys, placed late"
    );
    assert_eq!(
        translation.link_stats().existence_folds,
        0,
        "the oracle folds no existence"
    );

    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-scenario-wide.pg.ndjson"
    ))
    .expect("scenario file");
    let items: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("scenario line"))
        .collect();
    let queries: Vec<Value> = items
        .iter()
        .filter(|v| v.get("terminal").is_some())
        .cloned()
        .collect();
    let (mut default_path, mut translation) = pair(&queries, &Catalog::new);
    let mut joined: Vec<(Verdict, Verdict)> = Vec::new();
    for item in &items {
        if let Some(feed) = item.get("feed") {
            default_path.apply_feed(feed).expect("feed");
            translation.apply_feed(feed).expect("feed");
        } else if item.get("terminal").is_some() && item["joins"].as_u64() > Some(0) {
            let (a, v) = (
                shadow_verdict(&default_path, item, false),
                shadow_verdict(&translation, item, false),
            );
            assert_eq!(
                a, v,
                "{}: default path differs from the translation oracle",
                item["sql"]
            );
            assert_eq!(a, shadow_verdict(&default_path, item, true), "fused");
            joined.push((a, v));
        }
    }
    let mut outcomes: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (a, _) in &joined {
        *outcomes.entry(format!("{a:?}")).or_default() += 1;
    }
    println!("POSITION MAP wide join outcomes {outcomes:?}");
    assert_eq!(joined.len(), 57, "join calls of the wide scenario");
    assert_eq!(
        outcomes,
        std::collections::BTreeMap::from([
            ("Equal(Rows(4))".to_owned(), 42),
            ("Equal(Rows(7))".to_owned(), 15),
        ]),
        "every wide join call equals the rows PostgreSQL recorded"
    );
    let stats = default_path.link_stats();
    println!("POSITION MAP STATS wide {stats:?}");
    assert_eq!(
        (stats.derived, stats.rows_translated),
        (0, 0),
        "wide: translations derived"
    );
    assert_eq!(
        (
            stats.key_sets,
            stats.keys_in_sets,
            stats.existence_folds,
            stats.dangling_keys,
            stats.placed_late
        ),
        (228, 1140, 0, 0, 1092),
        "wide: key sets, keys in sets, existence folds, dangling keys, placed late"
    );
}

#[test]
fn a_removed_target_row_drops_its_joined_rows() {
    let data = joins_capture();
    let entry = &data["sizes"]["seed"];
    let call = find(entry, REMOVED_TARGET);
    let mut default_path = common::captured(entry);
    let mut translation = common::captured(entry);
    translation.use_translated_positions(true);
    let before = rows(&default_path, call, false).expect("rows before");

    let name = before.rows[0][0]
        .as_str()
        .expect("permission name")
        .to_owned();
    let permissions = table(&default_path, "permissions");
    let id = (0..permissions.rows)
        .find(|&at| {
            default_path.text_at("permissions", "name", at).as_deref() == Some(name.as_str())
        })
        .expect("position");
    let (rows_before, live_before) = (permissions.rows, live_count(permissions));
    assert!(bit(&permissions.live, id), "its live bit is set");
    assert_eq!(
        default_path.cell("permissions", "id", id),
        json!(id),
        "id is position"
    );

    let feed = json!({"tables": {"permissions": {"upsert": [], "delete": [id]}}});
    default_path.apply_feed(&feed).expect("feed");
    translation.apply_feed(&feed).expect("feed");

    let after = rows(&default_path, call, false).expect("rows after");
    let oracle = rows(&translation, call, false).expect("oracle rows after");
    let expected: Vec<Vec<Value>> = before
        .rows
        .iter()
        .filter(|row| row[0] != json!(name))
        .cloned()
        .collect();
    println!(
        "REMOVED TARGET permission {name} (id {id}): {} rows before, {} after",
        before.rows.len(),
        after.rows.len()
    );
    assert!(
        expected.len() < before.rows.len(),
        "the answer contained it"
    );
    assert_eq!(multiset(&after.rows), multiset(&expected));
    assert_eq!(multiset(&after.rows), multiset(&oracle.rows));

    let permissions = table(&default_path, "permissions");
    assert_eq!(permissions.rows, rows_before, "its position is permanent");
    assert_eq!(live_count(permissions), live_before - 1);
    assert!(!bit(&permissions.live, id), "its live bit is cleared");
}

#[test]
fn a_key_beyond_the_density_bound_is_refused_not_relaid_out() {
    let columns = json!([{"name": "id", "type": "integer", "null": false}]);
    let mut catalog = Catalog::new();
    catalog
        .apply_feed(&json!({"tables": {"t": {
            "columns": columns, "upsert": [[2], [5]], "delete": []
        }}}))
        .expect("feed");
    let t = table(&catalog, "t");
    assert_eq!(
        (t.layout, t.rows, t.live.clone()),
        (Layout::Identity, 6, vec![0b100100])
    );

    let err = catalog
        .apply_feed(&json!({"tables": {"t": {"upsert": [[100_000]], "delete": []}}}))
        .expect_err("beyond the bound");
    assert!(err.0.contains("t") && err.0.contains("100000"), "{err}");
    assert_eq!(table(&catalog, "t").rows, 6, "the table is as it was");

    catalog
        .apply_feed(&json!({"tables": {"sparse": {
            "columns": columns, "upsert": [[1], [100_000]], "delete": []
        }}}))
        .expect("feed");
    let sparse = table(&catalog, "sparse");
    assert_eq!((sparse.layout, sparse.rows), (Layout::Append, 2));
}
