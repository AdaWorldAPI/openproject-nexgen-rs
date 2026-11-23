// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Row-returning calls are answered through quack as positions over
//! source-owned columns and must equal the rows PostgreSQL recorded, or be
//! refused by name.

mod common;

use quack_shadow::{Answer, Catalog, Verdict, rows, rows_verdict};
use serde_json::Value;

const SIZES: [&str; 2] = ["65", "1025"];

fn capture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-rows.pg.json"
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

#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn every_captured_row_call_equals_postgresql_or_is_refused_by_name() {
    use quack_shadow::Refusal;

    let expected = [
        (
            "Ticket::State.order(:name).limit(3)",
            Refusal::TextOrder {
                field: "name".to_owned(),
            },
        ),
        (
            "KnowledgeBase::Category.where(parent_id: 1).limit(2)",
            Refusal::UnorderedLimit,
        ),
        (
            "KnowledgeBase::Category.order(:parent_id).limit(25)",
            Refusal::TieAtLimit,
        ),
        (
            "Ticket::State.where('id < 5')",
            Refusal::UnsupportedNode("Arel::Nodes::SqlLiteral".to_owned()),
        ),
        (
            "Ticket::State.joins(:state_type).order(:id).limit(2)",
            Refusal::JoinOrder,
        ),
    ];
    let data = capture();
    for size in SIZES {
        let entry = &data["sizes"][size];
        let mut catalog = Catalog::from_capture(entry).expect("catalog");
        let calls = entry["calls"].as_array().expect("calls");
        if let Some(classes) = common::class_map(&common::models_of(calls)) {
            catalog.set_classes(classes).expect("classes");
        }
        let mut equal = 0;
        let mut refused: Vec<(&str, Refusal)> = Vec::new();
        for call in calls {
            let plain = rows_verdict(&catalog, call, false);
            let fused = rows_verdict(&catalog, call, true);
            assert_eq!(plain, fused, "{}: fused differs from plain", label(call));
            match &plain {
                Verdict::Equal(_) => println!("ROWS n={size} {}: {plain:?}", label(call)),
                Verdict::Refused(r) => println!("ROWS n={size} {}: Refused({r:?})", label(call)),
                Verdict::Differ { quack, postgres } => {
                    panic!("{}: quack {quack:?} but PostgreSQL {postgres}", label(call))
                }
            }
            match plain {
                Verdict::Equal(_) => equal += 1,
                Verdict::Refused(r) => refused.push((label(call), r)),
                Verdict::Differ { .. } => unreachable!("panicked above"),
            }
        }
        assert_eq!(equal, 24, "n={size}: equal");
        assert_eq!(refused.len(), 5, "n={size}: refused");
        for (name, refusal) in &expected {
            let found: Vec<&Refusal> = refused
                .iter()
                .filter(|(l, _)| l == name)
                .map(|(_, r)| r)
                .collect();
            assert_eq!(found, [refusal], "n={size}: {name}");
        }
    }
}

#[test]
fn rows_are_materialized_only_for_the_answer() {
    let data = capture();
    for size in SIZES {
        let entry = &data["sizes"][size];
        let catalog = Catalog::from_capture(entry).expect("catalog");
        for call in entry["calls"].as_array().expect("calls") {
            let Ok(got) = rows(&catalog, call, false) else {
                continue;
            };
            assert_eq!(got.materialized, got.rows.len(), "{}", label(call));
            assert!(got.rows.len() <= got.kept, "{}", label(call));
        }
    }

    let entry = &data["sizes"]["1025"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    let got = rows(&catalog, find(entry, "Ticket::State.find(3)"), false).expect("rows");
    assert_eq!((got.kept, got.materialized), (1, 1));
    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == "ticket_states")
        .expect("table");
    assert_eq!(table.rows, 1026, "position 0 holds no row");
}

#[test]
fn nulls_sort_last_ascending_and_first_descending() {
    let data = capture();
    let entry = &data["sizes"]["65"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    let parent_ids = |wanted: &str| -> Vec<Value> {
        let got = rows(&catalog, find(entry, wanted), false).expect("rows");
        let at = got
            .columns
            .iter()
            .position(|c| c == "parent_id")
            .expect("parent_id column");
        got.rows.iter().map(|row| row[at].clone()).collect()
    };

    let asc = parent_ids("KnowledgeBase::Category.order(parent_id: :asc, id: :asc).limit(40)");
    let desc = parent_ids("KnowledgeBase::Category.order(parent_id: :desc, id: :asc).limit(40)");
    assert_eq!((asc.len(), desc.len()), (40, 40));

    // At this size the ascending window of 40 ends before the NULLs begin.
    assert!(
        asc.iter().all(|v| !v.is_null()),
        "NULL in the ascending window"
    );
    assert!(desc[0].is_null(), "descending does not start with NULL");
    let first_value = desc.iter().position(|v| !v.is_null()).expect("a value");
    assert!(first_value >= 1);
    assert!(
        desc[first_value..].iter().all(|v| !v.is_null()),
        "NULL after a value in the descending window"
    );
}

#[test]
fn a_text_literal_binds_through_the_dictionary() {
    let data = capture();
    let entry = &data["sizes"]["65"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    for (wanted, count) in [
        ("Ticket::State.find_by(name: 'state7')", 1),
        ("Ticket::State.find_by(name: 'nope')", 0),
    ] {
        assert_eq!(
            rows_verdict(&catalog, find(entry, wanted), false),
            Verdict::Equal(Answer::Rows(count)),
            "{wanted}"
        );
    }
}
