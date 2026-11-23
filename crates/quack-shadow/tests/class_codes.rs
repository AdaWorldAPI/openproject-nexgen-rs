// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A bound table's text columns are keyed in the code table by the class's id
//! and the predicate of the field the column resolves to; everything else keeps
//! the default class and `<table>.<column>`.

mod common;

use std::collections::BTreeMap;

use quack_shadow::{Answer, Catalog, LaneKind, Refusal, answer, answer_bound};
use serde_json::{Value, json};

/// A code space as `code_space_of` reports it: class and property.
type Space = (u32, String);

fn read_json(file: &str) -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

/// The rows capture: its catalog and the model of each of its tables.
fn rows_capture() -> (Value, BTreeMap<String, String>) {
    let data = read_json("shadow-rows.pg.json");
    let entry = data["sizes"]["1025"].clone();
    let by_table = common::models_of(entry["calls"].as_array().expect("calls"));
    (entry, by_table)
}

fn is_live(catalog: &Catalog, table: usize, row: usize) -> bool {
    let live = &catalog.tables[table].live;
    (live[row / 64] >> (row % 64)) & 1 == 1
}

/// The text columns of `catalog` that have a lane: `(table, column, a live string)`.
fn text_columns(catalog: &Catalog) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (index, table) in catalog.tables.iter().enumerate() {
        for column in &table.columns {
            let laned_text = column
                .lane
                .as_ref()
                .is_some_and(|lane| lane.kind == LaneKind::Text);
            if !laned_text {
                continue;
            }
            let literal = (0..table.rows)
                .filter(|&row| is_live(catalog, index, row))
                .find_map(|row| catalog.text_at(&table.name, &column.name, row));
            if let Some(literal) = literal {
                out.push((table.name.clone(), column.name.clone(), literal));
            }
        }
    }
    out
}

fn count_call(table: &str, column: &str, literal: &str) -> Value {
    json!({
        "table": table, "terminal": "count",
        "where": {"table": table, "col": column, "op": "eq", "value": literal},
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        "answer": 0,
    })
}

fn expected_count(catalog: &Catalog, table: &str, column: &str, literal: &str) -> u64 {
    let index = catalog
        .tables
        .iter()
        .position(|t| t.name == table)
        .expect("table");
    let column = catalog.tables[index]
        .columns
        .iter()
        .find(|c| c.name == column)
        .expect("column");
    (0..catalog.tables[index].rows)
        .filter(|&row| {
            is_live(catalog, index, row)
                && catalog.text_at(table, &column.name, row).as_deref() == Some(literal)
        })
        .count() as u64
}

/// A catalog with classes set, and a text column of it that the class basis resolves.
fn classed(by_table: &BTreeMap<String, String>, entry: &Value) -> Option<Catalog> {
    let mut catalog = Catalog::from_capture(entry).expect("catalog");
    catalog
        .set_classes(common::class_map(by_table)?)
        .expect("classes");
    Some(catalog)
}

#[test]
fn a_bound_tables_text_is_keyed_by_class_and_predicate() {
    let (entry, by_table) = rows_capture();
    let Some(basis) = common::basis_of(&by_table) else {
        return;
    };
    let Some(catalog) = classed(&by_table, &entry) else {
        return;
    };
    let mut keyed = 0;
    for (table, column, literal) in text_columns(&catalog) {
        let Some(model) = by_table.get(&table) else {
            continue;
        };
        let Some((position, _)) = basis.resolve(model, &column) else {
            continue;
        };
        let classid = basis.class(model).expect("class").classid();
        let predicate = basis
            .field(model, position)
            .expect("field")
            .predicate_iri
            .clone();
        assert_ne!(classid, 0);
        assert_eq!(
            catalog.code_space_of(&table, &column),
            Some((classid, predicate)),
            "{table}.{column}"
        );
        let call = count_call(&table, &column, &literal);
        let want = expected_count(&catalog, &table, &column, &literal);
        assert!(want > 0);
        assert_eq!(
            answer(&catalog, &call, false),
            Ok(Answer::Count(want)),
            "{table}.{column}"
        );
        let bound = catalog.bind().expect("bind");
        assert_eq!(
            answer_bound(&catalog, &bound, &call, true),
            Ok(Answer::Count(want)),
            "{table}.{column}"
        );
        keyed += 1;
    }
    println!("CLASS_CODES keyed text columns checked: {keyed}");
    assert!(keyed > 0, "no bound text column in the capture");
}

#[test]
fn an_unbound_table_keeps_the_default_class() {
    let (entry, mut by_table) = rows_capture();
    by_table.remove("import_jobs");
    let Some(catalog) = classed(&by_table, &entry) else {
        return;
    };
    let columns = text_columns(&catalog);
    let of_table: Vec<_> = columns.iter().filter(|c| c.0 == "import_jobs").collect();
    assert!(!of_table.is_empty(), "import_jobs has no text column");
    for (table, column, _) in of_table {
        assert_eq!(
            catalog.code_space_of(table, column),
            Some((0, format!("{table}.{column}")))
        );
    }

    let report = catalog.bind().expect("bind").report(&catalog);
    let mut frame_only_text = 0;
    for binding in report.iter().filter(|b| b.class.is_some()) {
        for column in &binding.frame_only {
            if columns
                .iter()
                .any(|c| c.0 == binding.table && &c.1 == column)
            {
                assert_eq!(
                    catalog.code_space_of(&binding.table, column),
                    Some((0, format!("{}.{column}", binding.table)))
                );
                frame_only_text += 1;
            }
        }
    }
    println!("CLASS_CODES frame-only text columns of bound tables: {frame_only_text}");
    assert_eq!(catalog.code_space_of("import_jobs", "no_such_column"), None);
    assert_eq!(catalog.code_space_of("no_such_table", "name"), None);
}

#[test]
fn re_keying_retires_older_bindings_and_the_old_name() {
    let (entry, by_table) = rows_capture();
    let Some(probe) = classed(&by_table, &entry) else {
        return;
    };
    let Some((table, column, literal)) = text_columns(&probe).into_iter().find(|(t, c, _)| {
        probe
            .code_space_of(t, c)
            .is_some_and(|(class, _)| class != 0)
    }) else {
        panic!("no bound text column in the capture");
    };
    let old_property = format!("{table}.{column}");

    let mut catalog = Catalog::from_capture(&entry).expect("catalog");
    let call = count_call(&table, &column, &literal);
    let want = expected_count(&catalog, &table, &column, &literal);
    let bound = catalog.bind().expect("bind");
    assert_eq!(
        answer_bound(&catalog, &bound, &call, false),
        Ok(Answer::Count(want))
    );
    let sealed = catalog.seal_of(&table).expect("table");
    let advances = catalog.table_code_advances(&table);
    assert_eq!(
        catalog.code_space_of(&table, &column),
        Some((0, old_property.clone()))
    );

    catalog
        .set_classes(common::class_map(&by_table).expect("class map"))
        .expect("classes");
    assert!(catalog.frames_agree(), "table.frame and the store drifted");
    assert_ne!(
        catalog.code_space_of(&table, &column),
        Some((0, old_property.clone()))
    );
    assert!(matches!(
        answer_bound(&catalog, &bound, &call, false),
        Err(Refusal::Stale { .. })
    ));
    let fresh = catalog.bind().expect("rebind");
    assert_eq!(
        answer_bound(&catalog, &fresh, &call, false),
        Ok(Answer::Count(want))
    );

    #[cfg(not(feature = "disable-code-advance"))]
    {
        assert_eq!(catalog.table_code_advances(&table), advances + 1);
        let now = catalog.seal_of(&table).expect("table");
        assert!(now.version() > sealed.version());
        let old = |frame| catalog.code_in_space(&table, 0, &old_property, &literal, frame);
        assert!(
            old(&sealed).is_ok(),
            "the old frame answers under the old space"
        );
        assert_eq!(
            old(&now),
            Err(lance_graph_dt64::dr::CodeRead::UnknownKey),
            "the old name no longer answers at the new frame"
        );
        assert!(catalog.code_in(&table, &column, &literal, &now).is_ok());
    }
    #[cfg(feature = "disable-code-advance")]
    {
        let _ = (advances, sealed);
    }
}

#[cfg(not(feature = "disable-code-advance"))]
#[test]
fn re_keying_keeps_the_codes() {
    let (entry, by_table) = rows_capture();
    let Some(class_map) = common::class_map(&by_table) else {
        return;
    };
    let mut catalog = Catalog::from_capture(&entry).expect("catalog");
    let mut sealed = Vec::new();
    for (index, table) in catalog.tables.iter().enumerate() {
        for column in table.columns.iter().filter(|c| lane_is_text(c)) {
            let mut strings: Vec<String> = (0..table.rows)
                .filter(|&row| is_live(&catalog, index, row))
                .filter_map(|row| catalog.text_at(&table.name, &column.name, row))
                .collect();
            strings.sort_unstable();
            strings.dedup();
            let space = catalog.code_space_of(&table.name, &column.name);
            let frame = catalog.seal_of(&table.name).expect("table");
            let (class, property) = space.expect("space");
            let codes: Vec<_> = strings
                .iter()
                .map(|s| catalog.code_in_space(&table.name, class, &property, s, &frame))
                .collect();
            sealed.push((table.name.clone(), column.name.clone(), strings, codes));
        }
    }
    catalog.set_classes(class_map).expect("classes");
    let mut compared = 0;
    let mut moved_space = 0;
    for (table, column, strings, codes) in &sealed {
        let now = catalog.seal_of(table).expect("table");
        if catalog
            .code_space_of(table, column)
            .is_some_and(|(class, _)| class != 0)
        {
            moved_space += 1;
        }
        for (string, code) in strings.iter().zip(codes) {
            assert!(code.is_ok(), "{table}.{column} {string}");
            assert_eq!(
                &catalog.code_in(table, column, string, &now),
                code,
                "{table}.{column} {string}"
            );
            compared += 1;
        }
    }
    println!(
        "CLASS_CODES codes compared across the re-key: {compared} in {moved_space} re-keyed columns"
    );
    assert!(compared > 0 && moved_space > 0);
}

#[test]
fn two_tables_bound_to_different_classes_do_not_share_a_space() {
    let items: Vec<Value> = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-scenario.pg.ndjson"
    ))
    .expect("scenario file")
    .lines()
    .filter(|l| !l.trim().is_empty())
    .map(|l| serde_json::from_str(l).expect("scenario line"))
    .collect();
    let by_table = common::models_of(items.iter().filter(|i| i.get("terminal").is_some()));
    let Some(classes) = common::class_map(&by_table) else {
        return;
    };
    let mut catalog = Catalog::new();
    catalog.set_classes(classes).expect("classes");
    for feed in items.iter().filter_map(|i| i.get("feed")) {
        catalog.apply_feed(feed).expect("feed");
    }
    let mut by_column: BTreeMap<String, Vec<(String, Space)>> = BTreeMap::new();
    for table in &catalog.tables {
        for column in &table.columns {
            if lane_is_text(column) {
                let space = catalog
                    .code_space_of(&table.name, &column.name)
                    .expect("space");
                if space.0 != 0 {
                    by_column
                        .entry(column.name.clone())
                        .or_default()
                        .push((table.name.clone(), space));
                }
            }
        }
    }
    let mut compared = 0;
    for (column, spaces) in &by_column {
        for (i, (a, space_a)) in spaces.iter().enumerate() {
            for (b, space_b) in &spaces[i + 1..] {
                let (class_a, class_b) = (space_a.0, space_b.0);
                if class_a != class_b {
                    assert_ne!(space_a, space_b, "{a}.{column} and {b}.{column}");
                    compared += 1;
                }
            }
        }
    }
    println!("CLASS_CODES same-named text column pairs compared: {compared}");
    assert!(compared > 0, "no same-named text column in two classes");
}

fn lane_is_text(column: &quack_shadow::Column) -> bool {
    column
        .lane
        .as_ref()
        .is_some_and(|lane| lane.kind == LaneKind::Text)
}

/// Tables with a re-keyed text column, and the text columns re-keyed, per capture the bound tests use.
#[test]
fn the_rekeyed_columns_of_the_captures_are_counted() {
    let mut measured = Vec::new();
    let scenario: Vec<Value> = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-scenario.pg.ndjson"
    ))
    .expect("scenario file")
    .lines()
    .filter(|l| !l.trim().is_empty())
    .map(|l| serde_json::from_str(l).expect("scenario line"))
    .collect();
    let by_table = common::models_of(scenario.iter().filter(|i| i.get("terminal").is_some()));
    let Some(classes) = common::class_map(&by_table) else {
        return;
    };
    let mut catalog = Catalog::new();
    catalog.set_classes(classes).expect("classes");
    for feed in scenario.iter().filter_map(|i| i.get("feed")) {
        catalog.apply_feed(feed).expect("feed");
    }
    measured.push(("scenario", rekeyed(&catalog)));
    for (label, file, size) in [
        ("capture", "shadow-capture.pg.json", "4097"),
        ("rows", "shadow-rows.pg.json", "1025"),
        ("joins", "shadow-joins.pg.json", "seed"),
    ] {
        let data = read_json(file);
        let entry = &data["sizes"][size];
        let by_table = common::models_of(entry["calls"].as_array().expect("calls"));
        let Some((catalog, advances)) = classed_counting(&by_table, entry) else {
            return;
        };
        let (tables, columns) = rekeyed(&catalog);
        println!(
            "CLASS_CODES {label} rekeyed tables={tables} columns={columns} extra_code_advances={advances}"
        );
        measured.push((label, (tables, columns)));
    }
    println!("CLASS_CODES measured {measured:?}");
    assert_eq!(
        measured,
        [
            ("scenario", (9, 57)),
            ("capture", (1, 1)),
            ("rows", (2, 5)),
            ("joins", (2, 7)),
        ]
    );
}

fn rekeyed(catalog: &Catalog) -> (usize, usize) {
    let mut per_table = BTreeMap::<&str, usize>::new();
    for table in &catalog.tables {
        for column in table.columns.iter().filter(|c| lane_is_text(c)) {
            let keyed = catalog
                .code_space_of(&table.name, &column.name)
                .is_some_and(|(class, _)| class != 0);
            if keyed {
                *per_table.entry(table.name.as_str()).or_default() += 1;
            }
        }
    }
    (per_table.len(), per_table.values().sum())
}

fn classed_counting(by_table: &BTreeMap<String, String>, entry: &Value) -> Option<(Catalog, u64)> {
    let mut catalog = Catalog::from_capture(entry).expect("catalog");
    let before = catalog.code_advances();
    catalog
        .set_classes(common::class_map(by_table)?)
        .expect("classes");
    let extra = catalog.code_advances() - before;
    Some((catalog, extra))
}
