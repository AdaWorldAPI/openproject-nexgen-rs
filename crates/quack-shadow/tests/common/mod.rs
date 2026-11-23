// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

#![allow(dead_code)]

//! The OGAR class map of the tables a capture was read through.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use quack_shadow::{Catalog, ClassMap};
use rails_compiler::config::Loaded;
use rails_f000::ogar_basis::{CONCEPT_BASE, OgarBasis};
use serde_json::Value;

/// table -> the model ActiveRecord answered the calls on it with.
pub fn models_of<'a>(calls: impl IntoIterator<Item = &'a Value>) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for call in calls {
        let (Some(table), Some(model)) = (
            call.get("table").and_then(Value::as_str),
            call.get("model").and_then(Value::as_str),
        ) else {
            continue;
        };
        let known = out
            .entry(table.to_owned())
            .or_insert_with(|| model.to_owned());
        assert_eq!(known, model, "{table} is answered by two models");
    }
    out
}

/// `None` when `ZAMMAD_SRC` is unset, which `REQUIRE_SOURCES=1` turns into a failure.
pub fn basis_of(by_table: &BTreeMap<String, String>) -> Option<OgarBasis> {
    if std::env::var("ZAMMAD_SRC").is_err() {
        if std::env::var("REQUIRE_SOURCES").is_ok_and(|v| v == "1") {
            panic!(
                "REQUIRE_SOURCES=1 and ZAMMAD_SRC is not set: a skipped class binding is not a pass"
            );
        }
        eprintln!("SKIP: ZAMMAD_SRC is not set");
        return None;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cfg = Loaded::read(&root.join("zammad.toml")).expect("zammad.toml");
    let models: Vec<String> = by_table
        .values()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .cloned()
        .collect();
    let report = rails_compiler::ogar_classes_report(&cfg, &models).expect("ogar classes");
    assert!(report.missing.is_empty(), "missing: {:?}", report.missing);
    let namespace = cfg.config.app.namespace.clone();
    let inputs = report
        .classes
        .into_iter()
        .zip(CONCEPT_BASE..)
        .map(|(class, concept)| (namespace.clone(), class, concept))
        .collect();
    Some(
        OgarBasis::new(inputs)
            .with_column_not_null(report.column_not_null)
            .with_unharvested(report.unharvested_tables),
    )
}

/// `None` when `ZAMMAD_SRC` is unset, which `REQUIRE_SOURCES=1` turns into a failure.
pub fn class_map(by_table: &BTreeMap<String, String>) -> Option<ClassMap> {
    let basis = basis_of(by_table)?;
    Some(ClassMap::new(&basis, by_table.clone()).expect("class map"))
}

/// The join tables of the captured joins as written by hand: the expectation
/// the intake's own declaration is checked against.
pub fn joins_relations() -> Value {
    let occurrences = |table: &str, column: &str, target: &str| {
        serde_json::json!({
            "table": table, "column": column, "target": target,
            "predicate": format!("{table}.{column}"), "reading": "occurrences"
        })
    };
    serde_json::json!({"relations": [
        occurrences("roles_users", "user_id", "users"),
        occurrences("roles_users", "role_id", "roles"),
        occurrences("permissions_roles", "permission_id", "permissions"),
        occurrences("permissions_roles", "role_id", "roles"),
    ]})
}

/// A catalog of a captured size: the capture declares its own relations.
pub fn captured(entry: &Value) -> Catalog {
    Catalog::from_capture(entry).expect("catalog")
}

/// [`captured`] with every table laid out in arrival order.
pub fn captured_append(entry: &Value) -> Catalog {
    Catalog::from_capture_append(entry).expect("catalog")
}

/// Declares `table.column` an edge to the key of `target`, as an intake without a class map does.
pub fn declare_edge(catalog: &mut Catalog, table: &str, column: &str, target: &str) {
    catalog
        .apply_feed(&serde_json::json!({"relations": [{
            "table": table, "column": column, "target": target,
            "predicate": format!("{table}.{column}"), "reading": "edge"
        }]}))
        .expect("edge relation");
}

/// Every relation of the captured tables declared by hand: the join tables, and
/// the `belongs_to` columns the class map is checked against.
pub fn hand_relations() -> Vec<quack_shadow::Relation> {
    use quack_shadow::{Reading, Relation};
    let relation = |source: &str, column: &str, target: &str, predicate: &str, reading| Relation {
        source: source.to_owned(),
        column: column.to_owned(),
        target: target.to_owned(),
        predicate: predicate.to_owned(),
        reading,
    };
    let mut out: Vec<Relation> = joins_relations()["relations"]
        .as_array()
        .expect("relations")
        .iter()
        .map(|r| {
            relation(
                r["table"].as_str().expect("table"),
                r["column"].as_str().expect("column"),
                r["target"].as_str().expect("target"),
                r["predicate"].as_str().expect("predicate"),
                Reading::Occurrences,
            )
        })
        .collect();
    out.push(relation(
        "tickets",
        "state_id",
        "ticket_states",
        "state",
        Reading::Edge,
    ));
    out.push(relation(
        "knowledge_base_categories",
        "parent_id",
        "knowledge_base_categories",
        "parent",
        Reading::Edge,
    ));
    out
}
