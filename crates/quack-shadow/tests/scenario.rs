// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A recorded ticket lifecycle is replayed: committed writes reach the catalog
//! through the feed, and every call is answered against the catalog as the
//! feed left it before that call.

mod common;

use quack_shadow::{Catalog, Layout, Refusal, Verdict, rows, shadow_verdict};
use serde_json::{Value, json};

// Pinned from a measured run; see the SCENARIO SUMMARY line.
#[cfg(not(feature = "disable-sql-where"))]
const EQUAL: usize = 65;
#[cfg(not(feature = "disable-sql-where"))]
const REFUSED: &[(&str, usize)] = &[];

/// Replays a recorded run: `(calls, equal, refusals by name)`. A call whose
/// answer differs from PostgreSQL's fails the test.
#[cfg(not(feature = "disable-sql-where"))]
fn replay(file: &str) -> (usize, usize, std::collections::BTreeMap<String, usize>) {
    use std::collections::BTreeMap;

    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("scenario file");
    let mut catalog = Catalog::new();
    let calls: Vec<Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v.get("terminal").is_some())
        .collect();
    if let Some(classes) = common::class_map(&common::models_of(&calls)) {
        catalog.set_classes(classes).expect("classes");
    }
    let mut mark = String::new();
    let (mut total, mut equal) = (0usize, 0usize);
    let mut refused: BTreeMap<String, usize> = BTreeMap::new();
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let item: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("line {}: {e}", n + 1));
        if let Some(feed) = item.get("feed") {
            catalog
                .apply_feed(feed)
                .unwrap_or_else(|e| panic!("line {}: {e}", n + 1));
            continue;
        }
        if let Some(text) = item.get("mark").and_then(Value::as_str) {
            mark = text.to_owned();
            continue;
        }
        let terminal = item["terminal"].as_str().unwrap_or("?");
        let table = item["table"].as_str().unwrap_or("?");
        let plain = shadow_verdict(&catalog, &item, false);
        let fused = shadow_verdict(&catalog, &item, true);
        assert_eq!(
            plain, fused,
            "[{mark}] {}: fused differs from plain",
            item["sql"]
        );
        println!("SCENARIO [{mark}] {terminal} {table}: {plain:?}");
        total += 1;
        match plain {
            Verdict::Equal(_) => equal += 1,
            Verdict::Differ { quack, postgres } => panic!(
                "[{mark}] {}: quack {quack:?} but PostgreSQL {postgres}",
                item["sql"]
            ),
            Verdict::Refused(r) => *refused.entry(format!("{r:?}")).or_default() += 1,
        }
    }
    println!("SCENARIO SUMMARY {file} calls={total} equal={equal} refused={refused:?}");
    (total, equal, refused)
}

#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn the_ticket_scenario_replays_against_postgresql() {
    let (total, equal, refused) = replay("shadow-scenario.pg.ndjson");
    let refused_total: usize = refused.values().sum();
    assert_eq!(total, 65, "calls");
    assert_eq!(equal + refused_total, 65, "equal + refused");
    assert_eq!(equal, EQUAL, "equal");
    let got: Vec<(&str, usize)> = refused.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(got.as_slice(), REFUSED, "refused");
}

#[test]
fn a_feed_update_rewrites_the_row_in_place() {
    let mut catalog = Catalog::new();
    let columns = json!([
        {"name": "id", "type": "integer", "null": false},
        {"name": "name", "type": "character varying", "null": true},
        {"name": "n", "type": "integer", "null": true},
    ]);
    catalog
        .apply_feed(&json!({"commits": 1, "skipped": [], "tables": {"t": {
            "columns": columns,
            "upsert": [[1, "a", 10], [2, "b", 20], [3, "c", 30]],
            "delete": [],
        }}}))
        .expect("first feed");
    catalog
        .apply_feed(&json!({"commits": 1, "skipped": [], "tables": {"t": {
            "upsert": [[2, "b2", null]],
            "delete": [1],
        }}}))
        .expect("second feed");

    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == "t")
        .expect("table");
    assert_eq!(table.layout, Layout::Identity);
    assert_eq!(table.rows, 4);
    let live_keys: Vec<Value> = (0..table.rows)
        .filter(|p| (table.live[p / 64] >> (p % 64)) & 1 == 1)
        .map(|p| catalog.cell("t", "id", p))
        .collect();
    assert_eq!(live_keys, [json!(2), json!(3)]);
    assert_eq!(table.live, vec![0b1100]);
    // Two feeds, each followed by a code advance: the strings of `s` changed both times.
    #[cfg(not(feature = "disable-code-advance"))]
    assert_eq!(table.frame, 4);
    #[cfg(feature = "disable-code-advance")]
    assert_eq!(table.frame, 2);
    // A laned column has no cells; a delete clears the live bit only, so a removed row keeps its values.
    let value = |name: &str, at: usize| catalog.cell("t", name, at);
    let names: Vec<&str> = table.columns.iter().map(|c| c.name.as_str()).collect();
    let at_two: Vec<Value> = names.iter().map(|n| value(n, 2)).collect();
    assert_eq!(at_two, [json!(2), json!("b2"), Value::Null]);
    let at_one: Vec<Value> = names.iter().map(|n| value(n, 1)).collect();
    assert_eq!(at_one, [json!(1), json!("a"), json!(10)], "removed");

    let call = json!({
        "table": "t", "terminal": "rows", "where": null, "select": "*",
        "order": [{"table": "t", "col": "id", "dir": "asc"}],
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    });
    let got = rows(&catalog, &call, false).expect("rows");
    assert_eq!(
        got.rows,
        vec![
            vec![json!(2), json!("b2"), Value::Null],
            vec![json!(3), json!("c"), json!(30)],
        ]
    );
}

#[test]
fn a_read_of_a_table_its_transaction_wrote_is_refused() {
    let mut catalog = Catalog::new();
    catalog
        .apply_feed(&json!({"commits": 1, "skipped": [], "tables": {"t": {
            "columns": [{"name": "id", "type": "integer", "null": false}],
            "upsert": [[1]],
            "delete": [],
        }}}))
        .expect("feed");
    let call = |dirty: &str| {
        json!({
            "table": "t", "terminal": "count", "where": null, "limit": null, "offset": null,
            "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
            "answer": 1, "in_transaction": true, "dirty_tables": [dirty],
        })
    };
    assert_eq!(
        shadow_verdict(&catalog, &call("t"), false),
        Verdict::Refused(Refusal::UncommittedRead { table: "t".into() })
    );
    assert!(!matches!(
        shadow_verdict(&catalog, &call("other"), false),
        Verdict::Refused(Refusal::UncommittedRead { .. })
    ));
}

/// The replay answers every SELECT the statement census counted for the same
/// scenario: nothing Zammad or the harness reads is left out.
#[test]
fn the_replay_covers_every_select_of_the_census() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../oracle/");
    let census: Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{root}statement-census.pg.json")).expect("census"),
    )
    .expect("census json");
    let selects = census["by_kind"]["SELECT"].as_u64().expect("SELECT count");
    let scenario =
        std::fs::read_to_string(format!("{root}shadow-scenario.pg.ndjson")).expect("scenario");
    let calls = scenario
        .lines()
        .filter(|l| serde_json::from_str::<Value>(l).is_ok_and(|v| v.get("terminal").is_some()))
        .count() as u64;
    assert_eq!((calls, selects), (65, 65));
}

/// A table without an `id` column cannot be addressed by row, so the feed
/// sends it whole and the catalog diffs it against the live rows: a removed
/// row keeps its position with its live bit cleared.
#[test]
fn a_keyless_table_is_diffed_in_place() {
    let columns = json!([
        {"name": "a", "type": "integer", "null": true},
        {"name": "b", "type": "integer", "null": true},
    ]);
    let mut catalog = Catalog::new();
    catalog
        .apply_feed(&json!({"tables": {"j": {
            "columns": columns,
            "replace": [[1, 2], [1, 3], [null, 4]]
        }}}))
        .expect("first feed");
    catalog
        .apply_feed(&json!({"tables": {"j": {"replace": [[5, 6], [1, 3]]}}}))
        .expect("second feed");
    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == "j")
        .expect("table j");
    assert_eq!(
        (table.rows, table.live.clone(), table.frame),
        (4, vec![0b1010], 2)
    );
    let cells = |name: &str| -> Vec<Value> {
        (0..table.rows)
            .map(|p| catalog.cell("j", name, p))
            .collect()
    };
    assert_eq!(
        cells(&table.columns[0].name),
        vec![json!(1), json!(1), Value::Null, json!(5)]
    );
    assert_eq!(catalog.cell("j", &table.columns[1].name, 3), json!(6));
    assert!(
        catalog
            .apply_feed(&json!({"tables": {"j": {"replace": [[1]]}}}))
            .is_err(),
        "wrong width"
    );

    let mut twice = Catalog::new();
    twice
        .apply_feed(&json!({"tables": {"j": {
            "columns": columns,
            "replace": [[1, 2], [1, 2]]
        }}}))
        .expect("first feed");
    twice
        .apply_feed(&json!({"tables": {"j": {"replace": [[1, 2]]}}}))
        .expect("second feed");
    let table = twice.tables.iter().find(|t| t.name == "j").expect("j");
    assert_eq!(table.rows, 2);
    assert_eq!(table.live.iter().map(|w| w.count_ones()).sum::<u32>(), 1);
}

#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn the_wide_scenario_replays_against_postgresql() {
    let (total, equal, refused) = replay("shadow-scenario-wide.pg.ndjson");
    let census: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../oracle/statement-census-wide.pg.json"
        ))
        .expect("census"),
    )
    .expect("census json");
    assert_eq!(
        census["by_kind"]["SELECT"].as_u64(),
        Some(total as u64),
        "the replay answers every SELECT the workload sent"
    );
    let got: Vec<(&str, usize)> = refused.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    // Pinned from a measured run. Every call is answered and equal to
    // PostgreSQL's, the 57 permission lookups (a raw SQL fragment with LIKE)
    // included. Calls without ORDER BY are compared as unordered row sets.
    assert_eq!((total, equal, got.as_slice()), (512, 512, &[][..]));
}

/// The catalog as the feeds of a recorded run leave it, without answering any call.
fn catalog_after(file: &str) -> Catalog {
    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("scenario file");
    let items: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("line"))
        .collect();
    let calls = items.iter().filter(|v| v.get("terminal").is_some());
    let mut catalog = Catalog::new();
    if let Some(classes) = common::class_map(&common::models_of(calls)) {
        catalog.set_classes(classes).expect("classes");
    }
    for feed in items.iter().filter_map(|v| v.get("feed")) {
        catalog.apply_feed(feed).expect("feed");
    }
    catalog
}

/// Per PostgreSQL type: `(columns without a lane, live non-NULL cells in them)`.
fn unlaned_by_type(catalog: &Catalog) -> std::collections::BTreeMap<String, (usize, usize)> {
    let mut out: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    for table in &catalog.tables {
        for column in table.columns.iter().filter(|c| c.lane.is_none()) {
            let entry = out.entry(column.sql_type.clone()).or_default();
            entry.0 += 1;
            entry.1 += (0..table.rows)
                .filter(|&p| (table.live[p / 64] >> (p % 64)) & 1 == 1)
                .filter(|&p| !catalog.cell(&table.name, &column.name, p).is_null())
                .count();
        }
    }
    out
}

#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn the_types_scenario_replays_against_postgresql() {
    let (total, equal, refused) = replay("shadow-scenario-types.pg.ndjson");
    println!("TYPES SUMMARY total={total} equal={equal} refused={refused:?}");
    let got: Vec<(&str, usize)> = refused.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    // Pinned from a measured run: 7 date-range reads of `users` (a SQL fragment
    // whose binds are dates) and 8 login-uniqueness lookups (LOWER(login) = LOWER(..)).
    let expected = [
        ("UnsupportedNode(\"Arel::Nodes::BoundSqlLiteral\")", 7),
        ("UnsupportedNode(\"Arel::Nodes::Equality\")", 8),
    ];
    assert_eq!((total, equal, got.as_slice()), (227, 212, &expected[..]));
}

#[test]
fn the_types_scenario_reaches_dates_decimals_and_json() {
    let catalog = catalog_after("shadow-scenario-types.pg.ndjson");
    let census = unlaned_by_type(&catalog);
    println!("TYPES CENSUS {census:?}");
    let got: Vec<(&str, (usize, usize))> = census.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let expected = [
        ("date", (2, 2)),
        ("jsonb", (1, 1)),
        ("numeric(6,2)", (2, 4)),
    ];
    assert_eq!(got.as_slice(), &expected[..]);
}
