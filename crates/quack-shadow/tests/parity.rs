// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Every captured call is answered through quack and must equal the answer
//! PostgreSQL recorded, or be refused by name.

mod common;

use quack_shadow::{Answer, Catalog, Verdict, verdict};
use serde_json::Value;

fn capture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-capture.pg.json"
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

fn label(call: &Value) -> &str {
    call["label"].as_str().expect("label")
}

fn terminal(call: &Value) -> &str {
    call["terminal"].as_str().expect("terminal")
}

#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn every_captured_call_equals_postgresql_or_is_refused_by_name() {
    use quack_shadow::Refusal;

    const SIZES: [&str; 2] = ["65", "4097"];
    let expected = [
        ("Ticket::State.active.limit(10)", Refusal::LimitOffset),
        (
            "Ticket::State.where(\"active = TRUE\")",
            Refusal::UnsupportedNode("Arel::Nodes::SqlLiteral".to_owned()),
        ),
        (
            "Ticket.joins(:state).where(ticket_states: { active: true })",
            Refusal::Join,
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
        let mut refused: Vec<(&str, &str, Refusal)> = Vec::new();
        for call in calls {
            let plain = verdict(&catalog, call, false);
            let fused = verdict(&catalog, call, true);
            assert_eq!(plain, fused, "{}: fused differs from plain", label(call));
            println!(
                "SHADOW n={size} {} {}: {plain:?}",
                terminal(call),
                label(call)
            );
            if label(call) == "DataPrivacyTask.failed" {
                let empty = match terminal(call) {
                    "count" => Answer::Count(0),
                    _ => Answer::Exists(false),
                };
                assert_eq!(
                    plain,
                    Verdict::Equal(empty),
                    "n={size}: empty varchar table"
                );
            }
            match plain {
                Verdict::Equal(_) => equal += 1,
                Verdict::Differ { quack, postgres } => {
                    panic!("{}: quack {quack:?} but PostgreSQL {postgres}", label(call))
                }
                Verdict::Refused(r) => refused.push((label(call), terminal(call), r)),
            }
        }
        assert_eq!(equal, 38, "n={size}: equal");
        assert_eq!(refused.len(), 6, "n={size}: refused");
        for (name, refusal) in &expected {
            let mut terminals: Vec<&str> = refused
                .iter()
                .filter(|(l, _, r)| l == name && r == refusal)
                .map(|(_, t, _)| *t)
                .collect();
            terminals.sort_unstable();
            assert_eq!(terminals, ["count", "exists"], "n={size}: {name}");
        }
    }
}

#[test]
fn the_catalog_stores_null_as_zero_with_validity_zero() {
    let data = capture();
    let entry = &data["sizes"]["65"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == "knowledge_base_categories")
        .expect("table");
    let column = table
        .columns
        .iter()
        .find(|c| c.name == "parent_id")
        .expect("column");
    let cells = entry["tables"]["knowledge_base_categories"]["columns"]["parent_id"]["values"]
        .as_array()
        .expect("values");
    assert_eq!(cells.len(), table.rows);

    let lane = column.lane.as_ref().expect("laned column");
    assert!(column.cells.is_empty(), "an integer lane holds no cells");
    let bit = |row: usize| (lane.validity[row / 64] >> (row % 64)) & 1 == 1;
    let (mut nulls, mut zeros) = (0, 0);
    for (row, cell) in cells.iter().enumerate() {
        if cell.is_null() {
            nulls += 1;
            assert_eq!((lane.values[row], bit(row)), (0, false), "row {row}");
        } else {
            let v = u32::try_from(cell.as_u64().expect("int cell")).expect("u32");
            zeros += usize::from(v == 0);
            assert_eq!((lane.values[row], bit(row)), (v, true), "row {row}");
        }
    }
    assert!(nulls >= 1, "no NULL row");
    assert!(zeros >= 1, "no stored-0 row");

    assert_eq!(lane.validity.len(), table.rows.div_ceil(64));
    let tail = table.rows % 64;
    if tail != 0 {
        let last = lane.validity[lane.validity.len() - 1];
        assert_eq!(last >> tail, 0, "tail bits set");
    }
}

#[test]
fn null_rows_are_dropped_by_sql_comparisons() {
    let data = capture();
    let entry = &data["sizes"]["65"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    let call = entry["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .find(|c| {
            terminal(c) == "count" && label(c) == "KnowledgeBase::Category.where(parent_id: 0)"
        })
        .expect("call");
    assert_eq!(
        verdict(&catalog, call, false),
        Verdict::Equal(Answer::Count(9))
    );
}
