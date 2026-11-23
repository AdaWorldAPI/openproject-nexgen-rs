// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Inner joins are answered through quack as a bag over a star of tables,
//! without building the cross product, and must equal the rows PostgreSQL
//! recorded, or be refused by name.

mod common;

use quack_shadow::{Answer, Verdict, rows, rows_verdict};
use serde_json::Value;

fn capture() -> Value {
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

#[cfg(not(any(feature = "disable-sql-where", feature = "disable-join-multiplicity")))]
#[test]
fn every_captured_join_equals_postgresql_or_is_refused_by_name() {
    use quack_shadow::Refusal;

    let expected = [
        (
            "Role.left_joins(:permissions).pluck(:id)",
            Refusal::UnsupportedJoin("Arel::Nodes::OuterJoin".to_owned()),
        ),
        (
            "Role.joins(:permissions).distinct.pluck(:id)",
            Refusal::Distinct,
        ),
        (
            "Role.joins(:permissions).order(:id).limit(3).pluck(:id)",
            Refusal::JoinOrder,
        ),
    ];
    let data = capture();
    let entry = &data["sizes"]["seed"];
    let mut catalog = common::captured(entry);
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
        println!("JOINS {}: {plain:?}", label(call));
        match plain {
            Verdict::Equal(_) => equal += 1,
            Verdict::Refused(r) => refused.push((label(call), r)),
            Verdict::Differ { quack, postgres } => {
                panic!("{}: quack {quack:?} but PostgreSQL {postgres}", label(call))
            }
        }
    }
    assert_eq!(equal, 9, "equal");
    assert_eq!(refused.len(), 3, "refused");
    for (name, refusal) in &expected {
        let found: Vec<&Refusal> = refused
            .iter()
            .filter(|(l, _)| l == name)
            .map(|(_, r)| r)
            .collect();
        assert_eq!(found, [refusal], "{name}");
    }
}

#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn a_join_answer_is_a_bag() {
    let data = capture();
    let entry = &data["sizes"]["seed"];
    let catalog = common::captured(entry);
    let call = find(entry, "User.find(1).permissions.pluck(:name)");

    let got = rows(&catalog, call, false).expect("rows");
    let distinct: std::collections::BTreeSet<String> = got
        .rows
        .iter()
        .map(|row| serde_json::to_string(row).expect("row json"))
        .collect();
    assert_eq!((got.rows.len(), distinct.len()), (68, 16));
    assert_eq!(
        rows_verdict(&catalog, call, false),
        Verdict::Equal(Answer::Rows(68))
    );
}

#[test]
fn no_cross_product_is_built() {
    let data = capture();
    let entry = &data["sizes"]["seed"];
    let catalog = common::captured(entry);
    let call = find(entry, "User.find(1).permissions.pluck(:name)");

    let got = rows(&catalog, call, false).expect("rows");
    let join = got.join.expect("join stats");
    assert_eq!(join.driving, "permissions_roles");
    assert_eq!(join.sibling.as_deref(), Some("roles_users"));
    assert_eq!(join.pairs, 130 * 37);
    assert_eq!(got.materialized, 68);
    assert!(join.driving_kept <= 130, "{}", join.driving_kept);
}

#[test]
fn a_filter_on_a_joined_table_reads_through_the_edge() {
    let data = capture();
    let entry = &data["sizes"]["seed"];
    let catalog = common::captured(entry);
    let call = find(
        entry,
        "Permission.joins(:roles).where(roles: { name: 'role2' }).pluck(:name)",
    );
    assert_eq!(
        rows_verdict(&catalog, call, false),
        Verdict::Equal(Answer::Rows(8))
    );
    assert_eq!(
        rows_verdict(&catalog, call, true),
        Verdict::Equal(Answer::Rows(8))
    );
}
