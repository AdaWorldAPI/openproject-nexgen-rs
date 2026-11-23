// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A column that has a lane holds no cells when the lane renders every one back:
//! a text's values are the lane and the table's code table, a timestamp's the
//! lane and its one recorded spelling. A string enters once, at intake or in a
//! feed event.

mod common;

use std::collections::BTreeMap;

use quack_shadow::{Answer, Catalog, LaneKind, Layout, Spelling, answer, rows};
use serde_json::{Value, json};

fn read(file: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("oracle file")
}

fn classed(by_table: &BTreeMap<String, String>, mut catalog: Catalog) -> Catalog {
    if let Some(classes) = common::class_map(by_table) {
        catalog.set_classes(classes).expect("classes");
    }
    catalog
}

fn is_live(live: &[u64], row: usize) -> bool {
    (live[row / 64] >> (row % 64)) & 1 == 1
}

const KINDS: [LaneKind; 4] = [
    LaneKind::Text,
    LaneKind::Bool,
    LaneKind::Int,
    LaneKind::Timestamp,
];

/// `(laned columns of kind, live positions holding a non-NULL value in them)`.
fn census(catalog: &Catalog, kind: LaneKind) -> (usize, usize) {
    let (mut columns, mut valid) = (0, 0);
    for table in &catalog.tables {
        for column in &table.columns {
            let Some(lane) = column.lane.as_ref().filter(|lane| lane.kind == kind) else {
                continue;
            };
            columns += 1;
            valid += (0..table.rows)
                .filter(|&row| {
                    is_live(&table.live, row) && (lane.validity[row / 64] >> (row % 64)) & 1 == 1
                })
                .count();
        }
    }
    (columns, valid)
}

fn held_by_kind(catalog: &Catalog) -> [usize; 4] {
    KINDS.map(|kind| catalog.cells_held(kind))
}

struct Measured {
    label: &'static str,
    /// Cells held per kind, the worst seen after any feed included.
    held: [usize; 4],
    /// `(laned columns, valid positions)` per kind.
    census: [(usize, usize); 4],
}

fn measure(label: &'static str, catalog: &Catalog) -> Measured {
    let held = held_by_kind(catalog);
    let census = KINDS.map(|kind| census(catalog, kind));
    println!("LANE_STORE {label} held={held:?} census={census:?}");
    Measured {
        label,
        held,
        census,
    }
}

/// The three captures and both replayed scenarios, each measured after
/// loading and, for a scenario, after every feed.
fn workloads() -> Vec<Measured> {
    let mut out = Vec::new();
    for (label, file, size) in [
        ("rows", "shadow-rows.pg.json", "1025"),
        ("joins", "shadow-joins.pg.json", "seed"),
        ("capture", "shadow-capture.pg.json", "4097"),
    ] {
        let data: Value = serde_json::from_str(&read(file)).expect("capture json");
        let entry = &data["sizes"][size];
        let by_table = common::models_of(entry["calls"].as_array().expect("calls"));
        let catalog = classed(&by_table, Catalog::from_capture(entry).expect("catalog"));
        out.push(measure(label, &catalog));
    }
    for (label, file) in [
        ("scenario", "shadow-scenario.pg.ndjson"),
        ("wide", "shadow-scenario-wide.pg.ndjson"),
    ] {
        let items: Vec<Value> = read(file)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("scenario line"))
            .collect();
        let by_table = common::models_of(items.iter().filter(|i| i.get("terminal").is_some()));
        let mut catalog = classed(&by_table, Catalog::new());
        let mut worst = [0; 4];
        for feed in items.iter().filter_map(|i| i.get("feed")) {
            catalog.apply_feed(feed).expect("feed");
            let held = held_by_kind(&catalog);
            worst = std::array::from_fn(|k| worst[k].max(held[k]));
        }
        let mut done = measure(label, &catalog);
        done.held = std::array::from_fn(|k| done.held[k].max(worst[k]));
        out.push(done);
    }
    out
}

#[test]
fn a_laned_text_boolean_or_integer_column_holds_no_cells() {
    let measured = workloads();
    for m in &measured {
        assert_eq!(
            m.held[..3],
            [0, 0, 0],
            "{}: text, boolean, integer cells held",
            m.label
        );
        assert!(m.census[0].0 > 0, "{}: no laned text column", m.label);
    }
    for m in measured.iter().filter(|m| m.label != "capture") {
        assert!(m.census[0].1 > 0, "{}: no valid text position", m.label);
    }
    let text: Vec<(&str, usize, usize)> = measured
        .iter()
        .map(|m| (m.label, m.census[0].0, m.census[0].1))
        .collect();
    assert_eq!(text, CENSUS);
}

#[test]
fn the_laned_boolean_and_integer_columns_are_counted() {
    let measured = workloads();
    let got: Vec<(&str, [(usize, usize); 2])> = measured
        .iter()
        .map(|m| (m.label, [m.census[1], m.census[2]]))
        .collect();
    assert_eq!(got, BOOL_INT_CENSUS);
}

#[test]
fn a_laned_timestamp_column_holds_no_cells() {
    let measured = workloads();
    let held: Vec<(&str, usize)> = measured.iter().map(|m| (m.label, m.held[3])).collect();
    assert_eq!(held, TIMESTAMP_HELD);
    let census: Vec<(&str, usize, usize)> = measured
        .iter()
        .map(|m| (m.label, m.census[3].0, m.census[3].1))
        .collect();
    assert_eq!(census, TIMESTAMP_CENSUS);
}

// Pinned from a measured run; see the LANE_STORE lines. The 4097-row capture's
// one text column holds no non-NULL live text.
const CENSUS: [(&str, usize, usize); 5] = [
    ("rows", 8, 5894),
    ("joins", 7, 32),
    ("capture", 1, 0),
    ("scenario", 64, 72),
    ("wide", 67, 299),
];

/// Per workload: `[(boolean columns, valid positions), (integer columns, valid positions)]`.
const BOOL_INT_CENSUS: [(&str, [(usize, usize); 2]); 5] = [
    ("rows", [(6, 6150), (10, 8884)]),
    ("joins", [(4, 64), (8, 390)]),
    ("capture", [(4, 16388), (3, 9902)]),
    ("scenario", [(26, 49), (70, 80)]),
    ("wide", [(26, 143), (79, 346)]),
];

/// The cells the timestamp columns hold; before the lane stored them (under
/// `disable-lane-store`) 8208, 68, 8194, 92 and 281.
const TIMESTAMP_HELD: [(&str, usize); 5] = [
    ("rows", 0),
    ("joins", 0),
    ("capture", 0),
    ("scenario", 0),
    ("wide", 0),
];

/// Per workload: laned timestamp columns and valid positions, so the zero above is not vacuous.
const TIMESTAMP_CENSUS: [(&str, usize, usize); 5] = [
    ("rows", 8, 7603),
    ("joins", 4, 64),
    ("capture", 2, 5805),
    ("scenario", 37, 44),
    ("wide", 41, 144),
];

fn feed(catalog: &mut Catalog, entry: &Value) {
    catalog
        .apply_feed(&json!({"tables": {"t": entry}}))
        .expect("feed");
}

/// `t(id, name, n)` with the rows 1 a, 2 b, 3 c, 4 c and a NULL name at row 5.
fn small() -> Catalog {
    let mut catalog = Catalog::new();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "name", "type": "character varying", "null": true},
                {"name": "n", "type": "integer", "null": true},
            ],
            "upsert": [[1, "a", 10], [2, "b", 20], [3, "c", 30], [4, "c", 40], [5, null, 50]],
            "delete": [],
        }),
    );
    catalog
}

fn call(terminal: &str, filter: Value) -> Value {
    json!({
        "table": "t", "terminal": terminal, "where": filter, "select": ["id", "name"],
        "order": [{"table": "t", "col": "id", "dir": "asc"}],
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        "answer": 0,
    })
}

fn count_of(catalog: &Catalog, value: &str) -> u64 {
    let filter = json!({"table": "t", "col": "name", "op": "eq", "value": value});
    let plain = answer(catalog, &call("count", filter.clone()), false);
    let fused = answer(catalog, &call("count", filter), true);
    assert_eq!(plain, fused, "{value}");
    match plain {
        Ok(Answer::Count(n)) => n,
        other => panic!("count of {value}: {other:?}"),
    }
}

fn names(catalog: &Catalog) -> Vec<Value> {
    let got = rows(catalog, &call("rows", Value::Null), false).expect("rows");
    assert_eq!(
        got.rows,
        rows(catalog, &call("rows", Value::Null), true)
            .expect("fused rows")
            .rows
    );
    got.rows.into_iter().map(|row| row[1].clone()).collect()
}

/// `small()` after one more feed entry, or as it is.
fn small_after(entry: Option<Value>) -> Catalog {
    let mut catalog = small();
    if let Some(entry) = entry {
        feed(&mut catalog, &entry);
    }
    catalog
}

#[test]
fn a_new_text_replaces_an_old_one() {
    let before = count_of(&small(), "c");
    assert_eq!(before, 2);
    let catalog = small_after(Some(json!({"upsert": [[3, "zz", 30]], "delete": []})));
    assert_eq!(count_of(&catalog, "zz"), 1);
    assert_eq!(count_of(&catalog, "c"), before - 1);
    assert_eq!(
        names(&catalog),
        [json!("a"), json!("b"), json!("zz"), json!("c"), Value::Null]
    );
    assert_eq!(catalog.cells_held(LaneKind::Text), 0);
    assert_eq!(catalog.from_string("t", "name", "zz"), Some(3));
}

#[test]
fn a_text_set_to_null_renders_null() {
    let before = count_of(&small(), "c");
    let catalog = small_after(Some(json!({"upsert": [[3, null, 30]], "delete": []})));
    assert_eq!(
        names(&catalog),
        [json!("a"), json!("b"), Value::Null, json!("c"), Value::Null]
    );
    assert_eq!(count_of(&catalog, "c"), before - 1);
    assert_eq!(catalog.cells_held(LaneKind::Text), 0);
    assert_eq!(catalog.text_at("t", "name", 3), None);
}

#[test]
fn a_removed_row_leaves_the_counts_and_the_rows() {
    let catalog = small_after(Some(json!({"upsert": [], "delete": [2]})));
    assert_eq!(count_of(&catalog, "b"), 0);
    assert_eq!(
        names(&catalog),
        [json!("a"), json!("c"), json!("c"), Value::Null]
    );
    assert_eq!(catalog.cells_held(LaneKind::Text), 0);
}

#[test]
fn new_strings_take_codes_in_position_order() {
    let catalog = small_after(Some(
        json!({"upsert": [[8, "h", 0], [6, "f", 0], [7, "g", 0]], "delete": []}),
    ));
    let codes: Vec<Option<u32>> = ["a", "b", "c", "f", "g", "h"]
        .iter()
        .map(|s| catalog.from_string("t", "name", s))
        .collect();
    assert_eq!(
        codes,
        [Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)],
        "arrival order 8, 6, 7 does not decide the codes"
    );
}

#[test]
fn a_string_set_and_overwritten_in_one_entry_gets_no_code() {
    let catalog = small_after(Some(json!({
        "upsert": [[2, "gone", 20], [2, "b", 20]],
        "delete": [],
    })));
    assert_eq!(catalog.from_string("t", "name", "gone"), None);
    assert_eq!(catalog.from_string("t", "name", "b"), Some(1));
}

#[test]
fn rows_arriving_out_of_key_order_render_as_recorded() {
    let recorded = json!([[1, "a"], [2, null], [3, "c"], [4, "d"]]);
    let capture = json!({"tables": {"t": {
        "rows": 4,
        "columns": {
            "id": {"type": "integer", "null": false, "values": [3, 1, 4, 2]},
            "name": {"type": "character varying", "null": true, "values": ["c", "a", "d", null]},
        },
    }}});
    let catalog = Catalog::from_capture(&capture).expect("catalog");
    assert_eq!(catalog.tables[0].layout, Layout::Identity);
    assert_eq!(catalog.cells_held(LaneKind::Text), 0);
    let got = rows(&catalog, &call("rows", Value::Null), false).expect("rows");
    assert_eq!(
        Value::Array(got.rows.into_iter().map(Value::Array).collect()),
        recorded
    );

    let mut by_feed = Catalog::new();
    feed(
        &mut by_feed,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "name", "type": "character varying", "null": true},
            ],
            "upsert": [[3, "c"], [1, "a"], [4, "d"], [2, null]],
            "delete": [],
        }),
    );
    assert_eq!(by_feed.tables[0].layout, Layout::Identity);
    assert_eq!(by_feed.cells_held(LaneKind::Text), 0);
    let got = rows(&by_feed, &call("rows", Value::Null), false).expect("rows");
    assert_eq!(
        Value::Array(got.rows.into_iter().map(Value::Array).collect()),
        recorded
    );
}

#[test]
fn a_column_that_meets_a_non_string_keeps_cells_until_it_fits_again() {
    let mut catalog = small();
    assert_eq!(catalog.cells_held(LaneKind::Text), 0);
    feed(&mut catalog, &json!({"upsert": [[2, 7, 20]], "delete": []}));
    let column = &catalog.tables[0].columns[1];
    assert!(
        column.lane.is_none(),
        "a number in a text column has no lane"
    );
    assert_eq!(column.cells.len(), catalog.tables[0].rows);
    assert_eq!(
        names(&catalog)
            .iter()
            .filter(|v| !v.is_null())
            .cloned()
            .collect::<Vec<_>>(),
        [json!("a"), json!(7), json!("c"), json!("c")]
    );
    feed(&mut catalog, &json!({"upsert": [], "delete": [2]}));
    assert!(catalog.tables[0].columns[1].lane.is_some());
    assert!(catalog.tables[0].columns[1].cells.is_empty());
    assert_eq!(catalog.cells_held(LaneKind::Text), 0);
    assert_eq!(count_of(&catalog, "c"), 2);
}

#[test]
fn a_keyless_table_is_diffed_through_its_lane() {
    let mut catalog = Catalog::new();
    let columns = json!([
        {"name": "a", "type": "character varying", "null": true},
        {"name": "b", "type": "integer", "null": true},
    ]);
    let all = |catalog: &Catalog| {
        let live = &catalog.tables[0].live;
        (0..catalog.tables[0].rows)
            .filter(|&p| is_live(live, p))
            .map(|p| (catalog.text_at("t", "a", p), catalog.cell("t", "b", p)))
            .collect::<Vec<_>>()
    };
    feed(
        &mut catalog,
        &json!({"columns": columns, "replace": [["x", 1], ["y", 2], [null, 3]]}),
    );
    feed(
        &mut catalog,
        &json!({"replace": [["y", 2], ["z", 4], [null, 3]]}),
    );
    assert_eq!(catalog.cells_held(LaneKind::Text), 0);
    assert_eq!(
        all(&catalog),
        [
            (Some("y".to_owned()), json!(2)),
            (None, json!(3)),
            (Some("z".to_owned()), json!(4)),
        ]
    );
    assert_eq!(catalog.from_string("t", "a", "z"), Some(2));
}

/// `t(id, n, ok)` with the rows 1 10 true, 2 20 false, 3 30 NULL, 4 40 true.
fn flags() -> Catalog {
    let mut catalog = Catalog::new();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "n", "type": "integer", "null": true},
                {"name": "ok", "type": "boolean", "null": true},
            ],
            "upsert": [[1, 10, true], [2, 20, false], [3, 30, null], [4, 40, true]],
            "delete": [],
        }),
    );
    catalog
}

fn count_on(catalog: &Catalog, col: &str, value: Value) -> u64 {
    let filter = json!({"table": "t", "col": col, "op": "eq", "value": value});
    let call = |fused| {
        let mut call = call("count", filter.clone());
        call["select"] = json!(["id", "n", "ok"]);
        answer(catalog, &call, fused)
    };
    assert_eq!(call(false), call(true), "{col} = {value}");
    match call(false) {
        Ok(Answer::Count(n)) => n,
        other => panic!("count of {col} = {value}: {other:?}"),
    }
}

fn flag_rows(catalog: &Catalog) -> Vec<Value> {
    let mut call = call("rows", Value::Null);
    call["select"] = json!(["id", "n", "ok"]);
    let got = rows(catalog, &call, false).expect("rows");
    assert_eq!(
        got.rows,
        rows(catalog, &call, true).expect("fused rows").rows
    );
    got.rows.into_iter().map(Value::Array).collect()
}

fn no_cells(catalog: &Catalog) {
    for kind in [LaneKind::Bool, LaneKind::Int] {
        assert_eq!(catalog.cells_held(kind), 0, "{kind:?}");
    }
}

#[test]
fn an_integer_and_a_boolean_change_in_one_row() {
    let before = flags();
    let counts = |catalog: &Catalog| {
        [
            count_on(catalog, "n", json!(20)),
            count_on(catalog, "n", json!(21)),
            count_on(catalog, "ok", json!(false)),
            count_on(catalog, "ok", json!(true)),
        ]
    };
    assert_eq!(counts(&before), [1, 0, 1, 2]);
    let mut catalog = flags();
    feed(
        &mut catalog,
        &json!({"upsert": [[2, 21, true]], "delete": []}),
    );
    assert_eq!(counts(&catalog), [0, 1, 0, 3]);
    assert_eq!(
        flag_rows(&catalog),
        [
            json!([1, 10, true]),
            json!([2, 21, true]),
            json!([3, 30, null]),
            json!([4, 40, true]),
        ]
    );
    no_cells(&catalog);
}

#[test]
fn an_integer_set_to_null_renders_null() {
    let mut catalog = flags();
    feed(
        &mut catalog,
        &json!({"upsert": [[2, null, false]], "delete": []}),
    );
    assert_eq!(
        flag_rows(&catalog),
        [
            json!([1, 10, true]),
            json!([2, null, false]),
            json!([3, 30, null]),
            json!([4, 40, true]),
        ]
    );
    assert_eq!(count_on(&catalog, "n", json!(20)), 0);
    assert_eq!(catalog.cell("t", "n", 2), Value::Null);
    no_cells(&catalog);
}

#[test]
fn a_value_the_lane_cannot_hold_turns_the_column_into_cells_until_it_is_gone() {
    let mut catalog = flags();
    feed(
        &mut catalog,
        &json!({"upsert": [[2, -5, false]], "delete": []}),
    );
    let n = &catalog.tables[0].columns[1];
    assert!(n.lane.is_none(), "a negative integer has no lane");
    assert_eq!(n.cells.len(), catalog.tables[0].rows);
    assert!(
        catalog.tables[0].columns[2].lane.is_some(),
        "the boolean keeps its lane"
    );
    assert_eq!(
        flag_rows(&catalog),
        [
            json!([1, 10, true]),
            json!([2, -5, false]),
            json!([3, 30, null]),
            json!([4, 40, true]),
        ]
    );

    feed(&mut catalog, &json!({"upsert": [], "delete": [2]}));
    let n = &catalog.tables[0].columns[1];
    assert!(n.lane.is_some(), "laned again once every live cell fits");
    assert!(n.cells.is_empty());
    assert_eq!(
        flag_rows(&catalog),
        [
            json!([1, 10, true]),
            json!([3, 30, null]),
            json!([4, 40, true])
        ]
    );
    assert_eq!(count_on(&catalog, "n", json!(30)), 1);
    no_cells(&catalog);

    feed(
        &mut catalog,
        &json!({"upsert": [[3, 30, "yes"]], "delete": []}),
    );
    assert!(
        catalog.tables[0].columns[2].lane.is_none(),
        "a string is no boolean"
    );
    assert_eq!(flag_rows(&catalog)[1], json!([3, 30, "yes"]));
    feed(
        &mut catalog,
        &json!({"upsert": [[3, 30, false]], "delete": []}),
    );
    assert!(catalog.tables[0].columns[2].cells.is_empty());
    assert_eq!(count_on(&catalog, "ok", json!(false)), 1);
}

/// `big(id, n)` in append layout: its first key does not fit a lane.
fn big_rows(catalog: &Catalog) -> Vec<Value> {
    let mut call = call("rows", Value::Null);
    call["table"] = json!("big");
    call["select"] = json!(["id", "n"]);
    call["order"] = json!([{"table": "big", "col": "id", "dir": "asc"}]);
    let got = rows(catalog, &call, false).expect("rows");
    got.rows.into_iter().map(Value::Array).collect()
}

#[test]
fn a_removed_key_the_lane_cannot_hold_still_places_later_rows() {
    let mut catalog = Catalog::new();
    let feed_big = |catalog: &mut Catalog, entry: Value| {
        catalog
            .apply_feed(&json!({"tables": {"big": entry}}))
            .expect("feed");
    };
    feed_big(
        &mut catalog,
        json!({
            "columns": [
                {"name": "id", "type": "bigint", "null": false},
                {"name": "n", "type": "integer", "null": true},
            ],
            "upsert": [[5_000_000_000_i64, 1], [7, 2]],
            "delete": [],
        }),
    );
    feed_big(
        &mut catalog,
        json!({"upsert": [[8, 3]], "delete": [5_000_000_000_i64]}),
    );
    feed_big(&mut catalog, json!({"upsert": [[8, 4]], "delete": []}));
    assert_eq!(big_rows(&catalog), [json!([7, 2]), json!([8, 4])]);
    feed_big(&mut catalog, json!({"upsert": [], "delete": [8]}));
    assert_eq!(big_rows(&catalog), [json!([7, 2])]);
}

#[test]
fn a_removed_row_keeps_its_lane_values_across_a_rebuild_from_cells() {
    let mut catalog = flags();
    feed(&mut catalog, &json!({"upsert": [], "delete": [4]}));
    feed(
        &mut catalog,
        &json!({"upsert": [[2, -5, false]], "delete": []}),
    );
    feed(&mut catalog, &json!({"upsert": [], "delete": [2]}));
    let n = &catalog.tables[0].columns[1];
    assert!(n.lane.is_some() && n.cells.is_empty());
    assert_eq!(catalog.cell("t", "n", 4), json!(40));
    assert_eq!(flag_rows(&catalog).len(), 2);
}

#[test]
fn a_removed_row_keeps_its_lane_values() {
    let mut catalog = flags();
    feed(&mut catalog, &json!({"upsert": [], "delete": [4]}));
    assert_eq!(count_on(&catalog, "ok", json!(true)), 1);
    assert_eq!(flag_rows(&catalog).len(), 3);
    assert_eq!(catalog.cell("t", "n", 4), json!(40));
    assert_eq!(catalog.cell("t", "ok", 4), json!(true));
}

#[test]
fn identity_rows_render_their_ids_and_values_from_lanes() {
    let recorded = json!([
        [1, 10, null],
        [2, null, false],
        [3, 30, true],
        [4, 40, true]
    ]);
    let capture = json!({"tables": {"t": {
        "rows": 4,
        "columns": {
            "id": {"type": "integer", "null": false, "values": [3, 1, 4, 2]},
            "n": {"type": "integer", "null": true, "values": [30, 10, 40, null]},
            "ok": {"type": "boolean", "null": true, "values": [true, null, true, false]},
        },
    }}});
    let catalog = Catalog::from_capture(&capture).expect("catalog");
    assert_eq!(catalog.tables[0].layout, Layout::Identity);
    assert!(catalog.tables[0].columns.iter().all(|c| c.cells.is_empty()));
    assert_eq!(Value::Array(flag_rows(&catalog)), recorded);
}

#[test]
fn a_rendered_integer_equals_the_captured_number() {
    let capture = json!({"tables": {"t": {
        "rows": 4,
        "columns": {
            "id": {"type": "bigint", "null": false, "values": [7, 5_000_000, 4_294_967_295u64, 0]},
            "ok": {"type": "boolean", "null": true, "values": [false, true, null, true]},
        },
    }}});
    let catalog = Catalog::from_capture(&capture).expect("catalog");
    assert_eq!(catalog.tables[0].layout, Layout::Append);
    for column in ["id", "ok"] {
        let values = &capture["tables"]["t"]["columns"][column]["values"];
        for (row, captured) in values.as_array().expect("values").iter().enumerate() {
            assert_eq!(&catalog.cell("t", column, row), captured, "{column} {row}");
        }
    }
    let parsed: Value = serde_json::from_str("[0, 7, 4294967295]").expect("json");
    let rendered: Vec<Value> = [3, 0, 2].map(|row| catalog.cell("t", "id", row)).into();
    assert_eq!(Value::Array(rendered), parsed);
    no_cells(&catalog);
}

fn captured(entry: &Value, table: &str, column: &str) -> Vec<Value> {
    let columns = &entry["tables"][table]["columns"];
    let spec = match columns {
        Value::Object(map) => map.get(column).expect("column"),
        Value::Array(list) => list.iter().find(|c| c["name"] == column).expect("column"),
        other => panic!("columns: {other}"),
    };
    spec["values"].as_array().expect("values").clone()
}

fn sorted(values: impl Iterator<Item = Value>) -> Vec<String> {
    let mut keys: Vec<String> = values.map(|v| v.to_string()).collect();
    keys.sort_unstable();
    keys
}

#[test]
fn the_captured_booleans_and_integers_are_rendered_back() {
    let mut checked = 0;
    for (file, size) in [
        ("shadow-rows.pg.json", "1025"),
        ("shadow-joins.pg.json", "seed"),
        ("shadow-capture.pg.json", "4097"),
    ] {
        let data: Value = serde_json::from_str(&read(file)).expect("capture json");
        let entry = &data["sizes"][size];
        let catalog = Catalog::from_capture(entry).expect("catalog");
        for table in &catalog.tables {
            for column in &table.columns {
                let kind = column.lane.as_ref().map(|lane| lane.kind);
                if !matches!(kind, Some(LaneKind::Bool | LaneKind::Int)) {
                    continue;
                }
                let live = (0..table.rows).filter(|&row| is_live(&table.live, row));
                let rendered = sorted(live.map(|row| catalog.cell(&table.name, &column.name, row)));
                let recorded = sorted(captured(entry, &table.name, &column.name).into_iter());
                assert_eq!(rendered, recorded, "{file} {}.{}", table.name, column.name);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 35, "columns compared");
}

#[test]
fn the_captured_timestamps_are_rendered_back() {
    let mut checked = 0;
    let mut valid = 0;
    for (file, size) in [
        ("shadow-rows.pg.json", "1025"),
        ("shadow-joins.pg.json", "seed"),
        ("shadow-capture.pg.json", "4097"),
    ] {
        let data: Value = serde_json::from_str(&read(file)).expect("capture json");
        let entry = &data["sizes"][size];
        let catalog = Catalog::from_capture(entry).expect("catalog");
        for table in &catalog.tables {
            for column in &table.columns {
                let kind = column.lane.as_ref().map(|lane| lane.kind);
                if kind != Some(LaneKind::Timestamp) {
                    continue;
                }
                assert!(
                    column.cells.is_empty(),
                    "{file} {}.{}",
                    table.name,
                    column.name
                );
                let live = (0..table.rows).filter(|&row| is_live(&table.live, row));
                let rendered = sorted(live.map(|row| catalog.cell(&table.name, &column.name, row)));
                let recorded = sorted(captured(entry, &table.name, &column.name).into_iter());
                valid += recorded.iter().filter(|v| *v != "null").count();
                assert_eq!(rendered, recorded, "{file} {}.{}", table.name, column.name);
                checked += 1;
            }
        }
    }
    assert_eq!((checked, valid), (14, 13472), "columns and cells compared");
}

/// The object spelling of the three epochs the tests below use.
fn at(epoch: u32) -> Value {
    let text = match epoch {
        1_704_067_203 => "2024-01-01T00:00:03.000000Z",
        1_704_067_260 => "2024-01-01T00:01:00.000000Z",
        1_704_153_600 => "2024-01-02T00:00:00.000000Z",
        other => panic!("no text for {other}"),
    };
    json!({"t": text, "epoch": epoch, "usec": 0})
}

/// `t(id, at)` with the rows 1 and 4 at 1704067203, 2 at 1704067260 and a NULL at row 3.
fn stamps() -> Catalog {
    let mut catalog = Catalog::new();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "at", "type": "timestamp without time zone", "null": true},
            ],
            "upsert": [
                [1, at(1_704_067_203)],
                [2, at(1_704_067_260)],
                [3, null],
                [4, at(1_704_067_203)],
            ],
            "delete": [],
        }),
    );
    catalog
}

fn stamp_call(terminal: &str, filter: Value) -> Value {
    let mut call = call(terminal, filter);
    call["select"] = json!(["id", "at"]);
    call
}

fn stamp_rows(catalog: &Catalog) -> Vec<Value> {
    let call = stamp_call("rows", Value::Null);
    let got = rows(catalog, &call, false).expect("rows");
    assert_eq!(
        got.rows,
        rows(catalog, &call, true).expect("fused rows").rows
    );
    got.rows.into_iter().map(Value::Array).collect()
}

fn count_at(catalog: &Catalog, value: Value) -> u64 {
    let filter = json!({"table": "t", "col": "at", "op": "eq", "value": value});
    let call = stamp_call("count", filter);
    let plain = answer(catalog, &call, false);
    assert_eq!(plain, answer(catalog, &call, true), "{value}");
    match plain {
        Ok(Answer::Count(n)) => n,
        other => panic!("count of at = {value}: {other:?}"),
    }
}

fn spelling(catalog: &Catalog) -> Option<Spelling> {
    catalog.tables[0].columns[1]
        .lane
        .as_ref()
        .and_then(|lane| lane.spelling)
}

#[test]
fn a_timestamp_feed_changes_the_row_and_the_counts() {
    let mut catalog = stamps();
    assert_eq!(spelling(&catalog), Some(Spelling::Object));
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), 0);
    let before = [
        count_at(&catalog, json!(1_704_067_203)),
        count_at(&catalog, at(1_704_153_600)),
    ];
    assert_eq!(before, [2, 0]);
    feed(
        &mut catalog,
        &json!({"upsert": [[1, at(1_704_153_600)]], "delete": []}),
    );
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), 0);
    assert_eq!(
        stamp_rows(&catalog),
        [
            json!([1, at(1_704_153_600)]),
            json!([2, at(1_704_067_260)]),
            json!([3, null]),
            json!([4, at(1_704_067_203)]),
        ]
    );
    let after = [
        count_at(&catalog, json!(1_704_067_203)),
        count_at(&catalog, at(1_704_153_600)),
    ];
    assert_eq!(after, [1, 1]);
}

#[test]
fn a_bare_epoch_in_an_object_column_turns_it_into_cells_and_back() {
    let mut catalog = stamps();
    feed(
        &mut catalog,
        &json!({"upsert": [[2, 1_704_067_260]], "delete": []}),
    );
    let rows_now = catalog.tables[0].rows;
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), rows_now);
    assert_eq!(
        stamp_rows(&catalog),
        [
            json!([1, at(1_704_067_203)]),
            json!([2, 1_704_067_260]),
            json!([3, null]),
            json!([4, at(1_704_067_203)]),
        ]
    );
    assert_eq!(count_at(&catalog, json!(1_704_067_260)), 1);
    feed(
        &mut catalog,
        &json!({"upsert": [[2, at(1_704_067_260)]], "delete": []}),
    );
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), 0);
    assert_eq!(spelling(&catalog), Some(Spelling::Object));
    assert_eq!(stamp_rows(&catalog)[1], json!([2, at(1_704_067_260)]));
}

#[test]
fn a_microsecond_turns_the_column_into_cells_and_loses_its_lane() {
    let mut catalog = stamps();
    let odd = json!({"t": "2024-01-01T00:00:03.000005Z", "epoch": 1_704_067_203, "usec": 5});
    feed(&mut catalog, &json!({"upsert": [[2, odd]], "delete": []}));
    let column = &catalog.tables[0].columns[1];
    assert!(column.lane.is_none(), "a microsecond has no lane");
    assert_eq!(column.cells.len(), catalog.tables[0].rows);
    assert_eq!(stamp_rows(&catalog)[1], json!([2, odd]));
    assert_eq!(stamp_rows(&catalog)[0], json!([1, at(1_704_067_203)]));
    feed(
        &mut catalog,
        &json!({"upsert": [[2, at(1_704_067_260)]], "delete": []}),
    );
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), 0);
    assert_eq!(stamp_rows(&catalog)[1], json!([2, at(1_704_067_260)]));
}

#[test]
fn a_removed_rows_timestamp_survives_while_the_column_is_lane_stored() {
    let mut catalog = stamps();
    feed(&mut catalog, &json!({"upsert": [], "delete": [4]}));
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), 0);
    assert_eq!(catalog.cell("t", "at", 4), at(1_704_067_203));
    assert_eq!(stamp_rows(&catalog).len(), 3);
    assert_eq!(count_at(&catalog, json!(1_704_067_203)), 1);
    feed(
        &mut catalog,
        &json!({"upsert": [[2, 1_704_067_260]], "delete": []}),
    );
    assert_eq!(catalog.cell("t", "at", 4), at(1_704_067_203));
    assert_eq!(catalog.cell("t", "at", 2), json!(1_704_067_260));
}

/// `k(at)`, a table without a key: its removed rows keep their cells.
fn keyless(rows: &[Value]) -> Catalog {
    let mut catalog = Catalog::new();
    let table = json!({
        "columns": [{"name": "at", "type": "timestamp without time zone", "null": true}],
        "replace": rows,
    });
    catalog
        .apply_feed(&json!({"tables": {"k": table}}))
        .expect("feed");
    catalog
}

fn replace_keyless(catalog: &mut Catalog, rows: &[Value]) {
    catalog
        .apply_feed(&json!({"tables": {"k": {"replace": rows}}}))
        .expect("feed");
}

#[test]
fn a_removed_timestamp_the_lane_cannot_render_keeps_its_column_in_cells() {
    let wrong = json!({"t": "2024-01-01T00:00:04.000000Z", "epoch": 1_704_067_203, "usec": 0});
    let mut catalog = keyless(&[json!([at(1_704_067_203)]), json!([wrong])]);
    assert_eq!(catalog.tables[0].layout, Layout::Append);
    replace_keyless(&mut catalog, &[json!([at(1_704_067_203)])]);
    assert_eq!(catalog.tables[0].columns[0].cells.len(), 2);
    assert_eq!(catalog.cell("k", "at", 0), at(1_704_067_203));
    assert_eq!(catalog.cell("k", "at", 1), wrong);
    replace_keyless(
        &mut catalog,
        &[json!([at(1_704_067_203)]), json!([at(1_704_067_260)])],
    );
    assert_eq!(catalog.cell("k", "at", 1), wrong, "the removed row stays");
    assert_eq!(catalog.cell("k", "at", 2), at(1_704_067_260));
}

#[test]
fn a_removed_timestamp_of_the_other_spelling_keeps_its_column_in_cells() {
    let mut catalog = keyless(&[json!([at(1_704_067_203)]), json!([1_704_067_260])]);
    replace_keyless(&mut catalog, &[json!([at(1_704_067_203)])]);
    assert_eq!(catalog.tables[0].columns[0].cells.len(), 2);
    assert_eq!(catalog.cell("k", "at", 1), json!(1_704_067_260));
    let mut uniform = keyless(&[json!([at(1_704_067_203)]), json!([at(1_704_067_260)])]);
    replace_keyless(&mut uniform, &[json!([at(1_704_067_203)])]);
    assert_eq!(uniform.cells_held(LaneKind::Timestamp), 0);
    assert_eq!(uniform.cell("k", "at", 1), at(1_704_067_260));
}

#[test]
fn the_first_timestamp_of_an_empty_column_fixes_its_spelling() {
    let mut catalog = keyless(&[json!([null]), json!([null])]);
    let lane = |catalog: &Catalog| catalog.tables[0].columns[0].lane.clone().expect("lane");
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), 0);
    assert_eq!(lane(&catalog).spelling, None);
    replace_keyless(
        &mut catalog,
        &[json!([null]), json!([null]), json!([1_704_067_203])],
    );
    assert_eq!(lane(&catalog).spelling, Some(Spelling::Bare));
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), 0);
    assert_eq!(catalog.cell("k", "at", 2), json!(1_704_067_203));
    replace_keyless(
        &mut catalog,
        &[
            json!([null]),
            json!([null]),
            json!([1_704_067_203]),
            json!([at(1_704_067_260)]),
        ],
    );
    assert_eq!(catalog.cells_held(LaneKind::Timestamp), 4);
    assert_eq!(catalog.cell("k", "at", 2), json!(1_704_067_203));
    assert_eq!(catalog.cell("k", "at", 3), at(1_704_067_260));
}
