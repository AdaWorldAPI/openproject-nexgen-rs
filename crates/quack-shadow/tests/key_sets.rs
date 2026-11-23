// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A join through a table laid out in arrival order translates the constant:
//! the keys of the dimension's live rows that satisfy the filter become an `IN`
//! over the driving table's edge column, and a key is placed only for a kept row.

mod common;

use quack_shadow::{Catalog, rows};
#[cfg(not(any(feature = "disable-sql-where", feature = "disable-join-multiplicity")))]
use quack_shadow::{Verdict, rows_verdict};
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

fn table<'a>(catalog: &'a Catalog, name: &str) -> &'a quack_shadow::Table {
    catalog
        .tables
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no table {name}"))
}

fn int_cell(catalog: &Catalog, table: &str, column: &str, at: usize) -> i64 {
    catalog
        .cell(table, column, at)
        .as_i64()
        .expect("integer cell")
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

#[cfg(not(any(feature = "disable-sql-where", feature = "disable-join-multiplicity")))]
fn kind(verdict: &Verdict) -> String {
    match verdict {
        Verdict::Equal(_) => "Equal".to_owned(),
        Verdict::Differ { .. } => "Differ".to_owned(),
        Verdict::Refused(other) => format!("Refused({other:?})"),
    }
}

fn with_classes(mut catalog: Catalog, calls: &[Value]) -> Catalog {
    if let Some(classes) = common::class_map(&common::models_of(calls)) {
        catalog.set_classes(classes).expect("classes");
    }
    catalog
}

#[cfg(not(any(feature = "disable-sql-where", feature = "disable-join-multiplicity")))]
#[test]
fn the_arrival_order_layout_answers_the_captured_joins() {
    let data = joins_capture();
    let entry = &data["sizes"]["seed"];
    let calls = entry["calls"].as_array().expect("calls");
    let keyed = with_classes(common::captured(entry), calls);
    let arrival = with_classes(common::captured_append(entry), calls);

    let mut equal = 0;
    let mut others: Vec<(&str, String)> = Vec::new();
    let mut sizes_by_call: Vec<(&str, Vec<usize>)> = Vec::new();
    for call in calls {
        let before = arrival.link_stats();
        let plain = rows_verdict(&arrival, call, false);
        let after = arrival.link_stats();
        assert_eq!(
            plain,
            rows_verdict(&arrival, call, true),
            "{}: fused differs from plain",
            label(call)
        );
        let (sizes, of): (Vec<usize>, Vec<usize>) = match rows(&arrival, call, false) {
            Ok(answer) => answer
                .join
                .map(|join| {
                    join.key_sets
                        .iter()
                        .map(|(name, size)| (*size, table(&arrival, name).rows))
                        .unzip()
                })
                .unwrap_or_default(),
            Err(_) => (Vec::new(), Vec::new()),
        };
        println!(
            "KEYSET {}: sets={} sizes={sizes:?} of {of:?} placed_late={} -> {}",
            label(call),
            after.key_sets - before.key_sets,
            after.placed_late - before.placed_late,
            kind(&plain)
        );
        let expected = rows_verdict(&keyed, call, false);
        assert_eq!(
            plain,
            expected,
            "{}: differs from the key-laid catalog",
            label(call)
        );
        match &plain {
            Verdict::Equal(_) => equal += 1,
            refused => others.push((label(call), kind(refused))),
        }
        sizes_by_call.push((label(call), sizes));
    }
    println!("KEYSET summary: equal {equal}, other {others:?}");
    assert_eq!(equal, 9, "equal");
    let refused: Vec<(&str, &str)> = others.iter().map(|(l, k)| (*l, k.as_str())).collect();
    assert_eq!(
        refused,
        [
            (
                "Role.left_joins(:permissions).pluck(:id)",
                "Refused(UnsupportedJoin(\"Arel::Nodes::OuterJoin\"))"
            ),
            (
                "Role.joins(:permissions).distinct.pluck(:id)",
                "Refused(Distinct)"
            ),
            (
                "Role.joins(:permissions).order(:id).limit(3).pluck(:id)",
                "Refused(JoinOrder)"
            ),
        ],
        "refused"
    );
    let sizes: Vec<(&str, &[usize])> = sizes_by_call
        .iter()
        .map(|(l, sizes)| (*l, sizes.as_slice()))
        .collect();
    let user = |n: u8| format!("User.find({n}).permissions.pluck(:name)");
    let users: Vec<String> = (1..=6).map(user).collect();
    let mut expected: Vec<(&str, &[usize])> =
        users.iter().map(|l| (l.as_str(), &[16, 9][..])).collect();
    expected.extend([
        (
            "Role.joins(:permissions).where(permissions: { name: 'perm6', active: true }, roles: { active: true }).pluck(:id)",
            &[9, 1][..],
        ),
        (
            "Role.joins(:permissions).where(permissions: { name: %w[perm3 perm6 nope] }).pluck(:id, :name)",
            &[2][..],
        ),
        (
            "Permission.joins(:roles).where(roles: { name: 'role2' }).pluck(:name)",
            &[1][..],
        ),
        ("Role.left_joins(:permissions).pluck(:id)", &[][..]),
        ("Role.joins(:permissions).distinct.pluck(:id)", &[][..]),
        ("Role.joins(:permissions).order(:id).limit(3).pluck(:id)", &[][..]),
    ]);
    assert_eq!(sizes, expected, "key set sizes per call");
    let stats = arrival.link_stats();
    println!("KEYSET stats {stats:?}");
    assert_eq!((stats.derived, stats.rows_translated), (0, 0));
    assert_eq!(
        (
            stats.key_sets,
            stats.keys_in_sets,
            stats.existence_folds,
            stats.dangling_keys,
            stats.placed_late
        ),
        (48, 489, 6, 0, 1704),
        "joins: key sets, keys in sets, existence folds, dangling keys, placed late"
    );
}

#[test]
fn a_removed_target_row_leaves_the_key_set() {
    let data = joins_capture();
    let entry = &data["sizes"]["seed"];
    let calls = entry["calls"].as_array().expect("calls");
    let call = calls
        .iter()
        .find(|c| label(c) == REMOVED_TARGET)
        .expect("the call");
    let mut arrival = with_classes(common::captured_append(entry), calls);
    let mut keyed = with_classes(common::captured(entry), calls);
    let before = rows(&arrival, call, false).expect("rows before");

    let name = before.rows[0][0]
        .as_str()
        .expect("permission name")
        .to_owned();
    let permissions = table(&arrival, "permissions");
    let at = (0..permissions.rows)
        .find(|&at| arrival.text_at("permissions", "name", at).as_deref() == Some(name.as_str()))
        .expect("position");
    let id = int_cell(&arrival, "permissions", "id", at);
    let (rows_before, live_before) = (permissions.rows, live_count(permissions));

    let feed = json!({"tables": {"permissions": {"upsert": [], "delete": [id]}}});
    arrival.apply_feed(&feed).expect("feed");
    keyed.apply_feed(&feed).expect("feed");

    let after = rows(&arrival, call, false).expect("rows after");
    let expected: Vec<Vec<Value>> = before
        .rows
        .iter()
        .filter(|row| row[0] != json!(name))
        .cloned()
        .collect();
    println!(
        "KEYSET removed target permission {name} (id {id}, position {at}): {} rows before, {} after",
        before.rows.len(),
        after.rows.len()
    );
    assert_eq!((before.rows.len(), after.rows.len()), (8, 6));
    assert_eq!(multiset(&after.rows), multiset(&expected));
    if cfg!(not(feature = "disable-target-presence")) {
        let by_key = rows(&keyed, call, false).expect("key-laid rows after");
        assert_eq!(multiset(&after.rows), multiset(&by_key.rows));
    }

    let permissions = table(&arrival, "permissions");
    assert_eq!(permissions.rows, rows_before, "its position is permanent");
    assert_eq!(live_count(permissions), live_before - 1);
}

fn big_child_feed(groups: u32) -> Value {
    let child_columns = json!([
        {"name": "id", "type": "integer", "null": false},
        {"name": "big_id", "type": "integer", "null": false},
    ]);
    let big_columns = json!([
        {"name": "id", "type": "integer", "null": false},
        {"name": "grp", "type": "integer", "null": false},
    ]);
    let big: Vec<Value> = (1..=5000u32).map(|id| json!([id, id % groups])).collect();
    let child: Vec<Value> = (1..=5000u32)
        .map(|id| json!([id, (id * 7) % 5000 + 1]))
        .collect();
    json!({"tables": {
        "big": {"columns": big_columns, "upsert": big, "delete": []},
        "child": {"columns": child_columns, "upsert": child, "delete": []},
    }})
}

fn child_call() -> Value {
    json!({
        "label": "Child.joins(:big).where(bigs: { grp: 3 }).pluck(:id)",
        "table": "child", "terminal": "rows", "select": ["id"],
        "where": {"table": "big", "col": "grp", "op": "eq", "value": 3},
        "order": [], "limit": null, "offset": null, "joins": 1,
        "join": [{
            "type": "inner", "table": "big",
            "left": {"table": "child", "col": "big_id"},
            "right": {"table": "big", "col": "id"},
        }],
        "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

#[test]
fn a_large_key_set_is_an_in_list_of_constants() {
    let call = child_call();
    let (mut arrival, mut keyed) = (Catalog::new_append(), Catalog::new());
    for catalog in [&mut arrival, &mut keyed] {
        common::declare_edge(catalog, "child", "big_id", "big");
    }
    arrival.apply_feed(&big_child_feed(10)).expect("feed");
    keyed.apply_feed(&big_child_feed(10)).expect("feed");
    let by_key = rows(&keyed, &call, true).expect("key-laid rows");
    let expected = (1..=5000u32)
        .filter(|id| ((id * 7) % 5000 + 1) % 10 == 3)
        .count();
    let tiles = u64::try_from(arrival.map_tiles("big")).expect("tiles");

    let before = arrival.map_lookups();
    let large = rows(&arrival, &call, false).expect("a large key set answers");
    let lookups = arrival.map_lookups() - before;
    let stats = arrival.link_stats();
    println!(
        "KEYSET large: {} rows from {} keys in {} set, map lookups {lookups} over {tiles} tiles, {stats:?}",
        large.rows.len(),
        stats.keys_in_sets,
        stats.key_sets,
    );
    assert_eq!(large.rows.len(), expected);
    assert_eq!((stats.key_sets, stats.keys_in_sets), (1, 500));
    assert_eq!((stats.derived, stats.rows_translated), (0, 0));
    assert_eq!(stats.placed_late, large.kept as u64);
    assert_eq!((lookups, tiles), (10_000, 20));
    assert_eq!(
        lookups,
        large.kept as u64 * tiles,
        "lookups come only from placing kept rows"
    );
    assert_eq!(multiset(&large.rows), multiset(&by_key.rows));
    let fused = rows(&arrival, &call, true).expect("fused rows");
    assert_eq!(multiset(&fused.rows), multiset(&by_key.rows));

    let (mut small, mut keyed) = (Catalog::new_append(), Catalog::new());
    for catalog in [&mut small, &mut keyed] {
        common::declare_edge(catalog, "child", "big_id", "big");
    }
    small.apply_feed(&big_child_feed(1000)).expect("feed");
    keyed.apply_feed(&big_child_feed(1000)).expect("feed");
    let answer = rows(&small, &call, true).expect("a small key set answers");
    let stats = small.link_stats();
    println!(
        "KEYSET small: {} rows from {} keys, {stats:?}",
        answer.rows.len(),
        stats.keys_in_sets,
    );
    assert_eq!((stats.key_sets, stats.keys_in_sets), (1, 5));
    let by_key = rows(&keyed, &call, false).expect("key-laid rows");
    assert!(!answer.rows.is_empty());
    assert_eq!(multiset(&answer.rows), multiset(&by_key.rows));
}

#[test]
fn the_wide_scenario_joins_answer_through_key_sets() {
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
    let mut catalog = with_classes(Catalog::new_append(), &queries);
    let mut outcomes: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for item in &items {
        if let Some(feed) = item.get("feed") {
            catalog.apply_feed(feed).expect("feed");
        } else if item.get("terminal").is_some() && item["joins"].as_u64() > Some(0) {
            let plain = quack_shadow::shadow_verdict(&catalog, item, false);
            assert_eq!(
                plain,
                quack_shadow::shadow_verdict(&catalog, item, true),
                "fused"
            );
            *outcomes.entry(format!("{plain:?}")).or_default() += 1;
        }
    }
    let stats = catalog.link_stats();
    println!("KEYSET wide scenario {outcomes:?} {stats:?}");
    assert_eq!(
        outcomes,
        std::collections::BTreeMap::from([
            ("Equal(Rows(4))".to_owned(), 42),
            ("Equal(Rows(7))".to_owned(), 15),
        ])
    );
    assert_eq!((stats.derived, stats.rows_translated), (0, 0));
    assert_eq!(
        (stats.key_sets, stats.keys_in_sets, stats.placed_late),
        (228, 1140, 1092),
        "wide key sets"
    );
}

fn parent_kids_call() -> Value {
    json!({
        "label": "Kid.joins(:parent).pluck(:id)",
        "table": "kids", "terminal": "rows", "select": ["id"],
        "where": null, "order": [], "limit": null, "offset": null, "joins": 1,
        "join": [{
            "type": "inner", "table": "parents",
            "left": {"table": "kids", "col": "parent_id"},
            "right": {"table": "parents", "col": "id"},
        }],
        "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

#[test]
fn a_dangling_edge_does_not_join() {
    let parent_columns = json!([
        {"name": "id", "type": "integer", "null": false},
        {"name": "name", "type": "character varying", "null": true},
    ]);
    let kid_columns = json!([
        {"name": "id", "type": "integer", "null": false},
        {"name": "parent_id", "type": "integer", "null": true},
    ]);
    let call = parent_kids_call();
    for mut catalog in [Catalog::new(), Catalog::new_append()] {
        common::declare_edge(&mut catalog, "kids", "parent_id", "parents");
        catalog
            .apply_feed(&json!({"tables": {
                "parents": {
                    "columns": parent_columns,
                    "upsert": [[1, "a"], [2, "b"], [3, "c"]], "delete": [],
                },
                "kids": {
                    "columns": kid_columns,
                    "upsert": [[1, 1], [2, 2], [3, 9], [4, null], [5, 3], [6, 1]],
                    "delete": [],
                },
            }}))
            .expect("feed");
        catalog
            .apply_feed(&json!({"tables": {"parents": {"upsert": [], "delete": [2]}}}))
            .expect("delete");
        let answer = rows(&catalog, &call, false).expect("rows");
        let stats = catalog.link_stats();
        println!(
            "KEYSET dangling {:?}: {stats:?}",
            table(&catalog, "parents").layout
        );
        assert_eq!(
            multiset(&answer.rows),
            multiset(&[vec![json!(1)], vec![json!(5)], vec![json!(6)]]),
            "never existed (9), deleted (2) and NULL do not join"
        );
        assert_eq!(
            (stats.existence_folds, stats.dangling_keys, stats.key_sets),
            (1, 2, 0)
        );
        let fused = rows(&catalog, &call, true).expect("fused rows");
        assert_eq!(multiset(&fused.rows), multiset(&answer.rows));
    }
}

#[test]
fn the_default_join_path_uses_no_gather_leaf() {
    let data = joins_capture();
    let entry = &data["sizes"]["seed"];
    let calls = entry["calls"].as_array().expect("calls");
    let filtered = calls
        .iter()
        .find(|c| label(c) == "Role.joins(:permissions).where(permissions: { name: %w[perm3 perm6 nope] }).pluck(:id, :name)")
        .expect("a call with a dimension filter");
    for catalog in [
        with_classes(common::captured(entry), calls),
        with_classes(common::captured_append(entry), calls),
    ] {
        for call in calls {
            for fused in [false, true] {
                let _ = rows(&catalog, call, fused);
            }
        }
        assert_eq!(catalog.link_stats().gather_leaves, 0, "captured joins");
    }

    let mut oracle = with_classes(common::captured(entry), calls);
    oracle.use_translated_positions(true);
    rows(&oracle, filtered, false).expect("oracle rows");
    assert!(
        oracle.link_stats().gather_leaves > 0,
        "the oracle gathers through the translation"
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
    let mut wide = with_classes(Catalog::new(), &queries);
    let mut joined = 0;
    for item in &items {
        if let Some(feed) = item.get("feed") {
            wide.apply_feed(feed).expect("feed");
        } else if item.get("terminal").is_some() && item["joins"].as_u64() > Some(0) {
            let _ = quack_shadow::shadow_verdict(&wide, item, false);
            joined += 1;
        }
    }
    assert_eq!(joined, 57);
    assert_eq!(wide.link_stats().gather_leaves, 0, "wide replay");
}

#[test]
fn a_removed_target_row_leaves_the_key_set_of_a_filtered_dimension() {
    let data = joins_capture();
    let entry = &data["sizes"]["seed"];
    let calls = entry["calls"].as_array().expect("calls");
    let call = calls
        .iter()
        .find(|c| label(c) == REMOVED_TARGET)
        .expect("the call");
    let mut arrival = with_classes(common::captured_append(entry), calls);
    let before = rows(&arrival, call, false).expect("rows before");
    assert_eq!(before.rows.len(), 8);

    let roles = table(&arrival, "roles");
    let at = (0..roles.rows)
        .find(|&at| arrival.text_at("roles", "name", at).as_deref() == Some("role2"))
        .expect("role2");
    let id = int_cell(&arrival, "roles", "id", at);
    arrival
        .apply_feed(&json!({"tables": {"roles": {"upsert": [], "delete": [id]}}}))
        .expect("feed");

    let after = rows(&arrival, call, false).expect("rows after");
    assert!(
        after.rows.is_empty(),
        "the only role the filter names is gone"
    );
    let key_sets = after.join.expect("join stats").key_sets;
    assert_eq!(key_sets, [("roles".to_owned(), 0)]);
}

/// A dead position holds value 0, so only the live plane keeps a removed row
/// out of a key set whose filter names the string at code 0.
#[test]
fn a_removed_target_row_whose_name_has_code_zero_leaves_the_key_set() {
    let data = joins_capture();
    let entry = &data["sizes"]["seed"];
    let calls = entry["calls"].as_array().expect("calls");
    let mut arrival = with_classes(common::captured_append(entry), calls);

    let roles = table(&arrival, "roles");
    let frame = arrival.seal_of("roles").expect("roles");
    let (at, name) = (0..roles.rows)
        .find_map(|at| {
            let name = arrival.text_at("roles", "name", at)?;
            (arrival.code_in("roles", "name", &name, &frame) == Ok(0)).then_some((at, name))
        })
        .expect("a role whose name has code 0");
    let id = int_cell(&arrival, "roles", "id", at);
    let mut call = calls
        .iter()
        .find(|c| label(c) == REMOVED_TARGET)
        .expect("the call")
        .clone();
    call["where"]["value"] = json!(name);

    let before = rows(&arrival, &call, false).expect("rows before");
    assert!(!before.rows.is_empty(), "the role has permissions");
    arrival
        .apply_feed(&json!({"tables": {"roles": {"upsert": [], "delete": [id]}}}))
        .expect("feed");
    let after = rows(&arrival, &call, false).expect("rows after");
    assert!(
        after.rows.is_empty(),
        "the only role the filter names is gone"
    );
    let key_sets = after.join.expect("join stats").key_sets;
    assert_eq!(key_sets, [("roles".to_owned(), 0)]);
}
