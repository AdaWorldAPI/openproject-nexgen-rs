// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Text exists only at the two edges of a catalog: `from_string` names the
//! member a string stands for, `to_string` gives the string back, and a returned
//! row is rendered through the second, never copied from stored cells.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use quack_shadow::{Answer, Catalog, LaneKind, answer, rows};
use serde_json::{Value, json};

fn read_json(file: &str) -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

fn is_live(catalog: &Catalog, table: usize, row: usize) -> bool {
    let live = &catalog.tables[table].live;
    (live[row / 64] >> (row % 64)) & 1 == 1
}

fn classed(by_table: &BTreeMap<String, String>, mut catalog: Catalog) -> Catalog {
    if let Some(classes) = common::class_map(by_table) {
        catalog.set_classes(classes).expect("classes");
    }
    catalog
}

/// The catalogs the other tests read: three captures and both replayed scenarios.
fn catalogs() -> Vec<(&'static str, Catalog)> {
    let mut out = Vec::new();
    for (label, file, size) in [
        ("rows", "shadow-rows.pg.json", "1025"),
        ("joins", "shadow-joins.pg.json", "seed"),
        ("capture", "shadow-capture.pg.json", "4097"),
    ] {
        let data = read_json(file);
        let entry = &data["sizes"][size];
        let by_table = common::models_of(entry["calls"].as_array().expect("calls"));
        let catalog = Catalog::from_capture(entry).expect("catalog");
        out.push((label, classed(&by_table, catalog)));
    }
    for (label, file) in [
        ("scenario", "shadow-scenario.pg.ndjson"),
        ("wide", "shadow-scenario-wide.pg.ndjson"),
    ] {
        let text = std::fs::read_to_string(format!(
            "{}/../../oracle/{file}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("scenario file");
        let items: Vec<Value> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("scenario line"))
            .collect();
        let by_table = common::models_of(items.iter().filter(|i| i.get("terminal").is_some()));
        let mut catalog = classed(&by_table, Catalog::new());
        for feed in items.iter().filter_map(|i| i.get("feed")) {
            catalog.apply_feed(feed).expect("feed");
        }
        out.push((label, catalog));
    }
    out
}

/// `(laned text columns, (column, member) pairs)` whose round trip held.
fn round_trip(catalog: &Catalog) -> (usize, usize) {
    let (mut columns, mut pairs) = (0, 0);
    for (index, table) in catalog.tables.iter().enumerate() {
        for column in &table.columns {
            let Some(lane) = column
                .lane
                .as_ref()
                .filter(|lane| lane.kind == LaneKind::Text)
            else {
                continue;
            };
            let at = format!("{}.{}", table.name, column.name);
            let live = (0..table.rows).filter(|&row| is_live(catalog, index, row));
            let mut held = BTreeSet::new();
            let texts = |row: usize| catalog.text_at(&table.name, &column.name, row);
            for row in live.clone() {
                if texts(row).is_none() {
                    continue;
                }
                let code = lane.values[row];
                let text = catalog
                    .to_string(&table.name, &column.name, code)
                    .unwrap_or_else(|| panic!("{at}: member {code} has no text"));
                assert_eq!(
                    catalog.from_string(&table.name, &column.name, &text),
                    Some(code),
                    "{at}: {text:?}"
                );
                held.insert(code);
            }
            let strings: BTreeSet<String> = live.filter_map(texts).collect();
            let mut named = BTreeSet::new();
            for text in &strings {
                let code = catalog
                    .from_string(&table.name, &column.name, text)
                    .unwrap_or_else(|| panic!("{at}: {text:?} names no member"));
                assert_eq!(
                    catalog
                        .to_string(&table.name, &column.name, code)
                        .as_deref(),
                    Some(text.as_str()),
                    "{at}: member {code}"
                );
                named.insert(code);
            }
            assert_eq!(held, named, "{at}: lane codes and their strings agree");
            columns += 1;
            pairs += named.len();
        }
    }
    (columns, pairs)
}

#[test]
fn the_round_trip_law_holds_for_every_text_column() {
    let mut measured = Vec::new();
    for (label, catalog) in catalogs() {
        let (columns, pairs) = round_trip(&catalog);
        println!("TEXT_EDGES {label} columns={columns} pairs={pairs}");
        // The 4097-row capture has one text column and it holds no row.
        assert!(columns > 0, "{label}: no text column was checked");
        assert!(
            pairs > 0 || label == "capture",
            "{label}: no member was checked"
        );
        measured.push((label, columns, pairs));
    }
    assert_eq!(
        measured,
        [
            ("rows", ROWS.0, ROWS.1),
            ("joins", JOINS.0, JOINS.1),
            ("capture", CAPTURE.0, CAPTURE.1),
            ("scenario", SCENARIO.0, SCENARIO.1),
            ("wide", WIDE.0, WIDE.1),
        ]
    );
}

// Pinned from a measured run; see the TEXT_EDGES lines.
const ROWS: (usize, usize) = (8, 2059);
const JOINS: (usize, usize) = (7, 32);
const CAPTURE: (usize, usize) = (1, 0);
const SCENARIO: (usize, usize) = (64, 48);
const WIDE: (usize, usize) = (67, 124);

/// `t(id, name, n)` with the rows 1 a, 2 b, 3 c, 4 c and a NULL name at row 5.
fn small() -> Catalog {
    let mut catalog = Catalog::new();
    catalog
        .apply_feed(&json!({"tables": {"t": {
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "name", "type": "character varying", "null": true},
                {"name": "n", "type": "integer", "null": true},
            ],
            "upsert": [[1, "a", 10], [2, "b", 20], [3, "c", 30], [4, "c", 40], [5, null, 50]],
            "delete": [],
        }}}))
        .expect("feed");
    catalog
}

fn select(columns: &[&str]) -> Value {
    json!({
        "table": "t", "terminal": "rows", "where": null, "select": columns,
        "order": [{"table": "t", "col": "id", "dir": "asc"}],
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

fn count_name(value: &str) -> Value {
    json!({
        "table": "t", "terminal": "count",
        "where": {"table": "t", "col": "name", "op": "eq", "value": value},
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        "answer": 0,
    })
}

#[test]
fn an_unknown_text_is_no_member() {
    let catalog = small();
    assert_eq!(catalog.from_string("t", "name", "b"), Some(1));
    assert_eq!(catalog.from_string("t", "name", "nope"), None);
    assert_eq!(catalog.from_string("t", "name", ""), None);
    assert_eq!(
        catalog.from_string("t", "n", "10"),
        None,
        "not a text column"
    );
    assert_eq!(catalog.from_string("t", "no_such_column", "a"), None);
    assert_eq!(catalog.from_string("no_such_table", "name", "a"), None);
    assert_eq!(catalog.to_string("t", "name", 3), None, "no fourth member");
    assert_eq!(
        answer(&catalog, &count_name("nope"), false),
        Ok(Answer::Count(0)),
        "a count filtered on an unknown text is 0, not an error"
    );
}

#[test]
fn a_returned_row_is_rendered_from_the_codebook() {
    let catalog = small();
    let all = select(&["id", "name", "n"]);
    let expected = vec![
        vec![json!(1), json!("a"), json!(10)],
        vec![json!(2), json!("b"), json!(20)],
        vec![json!(3), json!("c"), json!(30)],
        vec![json!(4), json!("c"), json!(40)],
        vec![json!(5), Value::Null, json!(50)],
    ];
    for fused in [false, true] {
        assert_eq!(
            rows(&catalog, &all, fused).expect("rows").rows,
            expected,
            "fused={fused}"
        );
    }
}

#[test]
fn an_integer_column_is_read_from_its_lane() {
    let catalog = small();
    let ints = select(&["id", "n"]);
    assert!(
        catalog.tables[0].columns[2].cells.is_empty(),
        "an integer column holds no cells beside its lane"
    );
    assert_eq!(
        rows(&catalog, &ints, false).expect("rows").rows,
        vec![
            vec![json!(1), json!(10)],
            vec![json!(2), json!(20)],
            vec![json!(3), json!(30)],
            vec![json!(4), json!(40)],
            vec![json!(5), json!(50)],
        ],
    );
}

#[test]
fn a_null_text_is_rendered_as_null() {
    let catalog = small();
    let got = rows(&catalog, &select(&["id", "name"]), false).expect("rows");
    let names: Vec<&Value> = got.rows.iter().map(|row| &row[1]).collect();
    assert_eq!(names.last().copied(), Some(&Value::Null));
    assert_eq!(names.iter().filter(|v| v.is_null()).count(), 1);
}
