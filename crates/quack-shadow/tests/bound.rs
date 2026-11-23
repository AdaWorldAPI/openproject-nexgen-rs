// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The lanes are read through DT64: a binding is refused for a table once a
//! feed touched it, and the class-bound columns are the ones the OGAR basis
//! names.

mod common;

use quack_shadow::{Catalog, Refusal, TableBinding, Verdict, shadow_verdict_bound};
use serde_json::Value;

fn read_json(file: &str) -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

fn scenario() -> Vec<Value> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-scenario.pg.ndjson"
    ))
    .expect("scenario file");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("scenario line"))
        .collect()
}

fn table_of(call: &Value) -> &str {
    call["table"].as_str().expect("table")
}

/// Replays the scenario up to a feed that touches a table some call of the
/// segment before it read, and a table none of them touched.
fn replay_to_a_touching_feed(with_classes: bool) {
    let items = scenario();
    let mut catalog = Catalog::new();
    if with_classes {
        let by_table = common::models_of(items.iter().filter(|i| i.get("terminal").is_some()));
        match common::class_map(&by_table) {
            Some(classes) => catalog.set_classes(classes).expect("classes"),
            None => return,
        }
    }
    let mut segment: Vec<&Value> = Vec::new();
    let mut checked = false;
    for item in &items {
        let Some(feed) = item.get("feed") else {
            if item.get("terminal").is_some() {
                segment.push(item);
            }
            continue;
        };
        let touched: Vec<&String> = feed["tables"].as_object().expect("tables").keys().collect();
        let bound = catalog.bind().expect("bind");
        let equal = |call: &Value| {
            matches!(
                shadow_verdict_bound(&catalog, &bound, call, false),
                Verdict::Equal(_)
            )
        };
        let read = segment
            .iter()
            .find(|c| touched.iter().any(|t| *t == table_of(c)) && equal(c));
        let untouched = segment
            .iter()
            .find(|c| !touched.iter().any(|t| *t == table_of(c)) && equal(c));
        if let (Some(read), Some(untouched), false) = (read, untouched, checked) {
            let before = shadow_verdict_bound(&catalog, &bound, untouched, false);
            catalog.apply_feed(feed).expect("feed");

            match shadow_verdict_bound(&catalog, &bound, read, false) {
                Verdict::Refused(Refusal::Stale { table, reason }) => {
                    assert_eq!(table, table_of(read));
                    assert!(
                        reason.contains("SourceChanged") || reason.contains("CoordinatesChanged"),
                        "{reason}"
                    );
                    println!("STALE {table}: {reason}");
                }
                other => panic!("the old binding still reads {}: {other:?}", table_of(read)),
            }
            assert_eq!(
                shadow_verdict_bound(&catalog, &bound, untouched, false),
                before,
                "a table the feed did not touch keeps its binding"
            );
            assert!(matches!(before, Verdict::Equal(_)));
            let fresh = catalog.bind().expect("rebind");
            assert!(
                !matches!(
                    shadow_verdict_bound(&catalog, &fresh, read, false),
                    Verdict::Refused(Refusal::Stale { .. })
                ),
                "a binding made after the feed is current"
            );
            checked = true;
        } else {
            catalog.apply_feed(feed).expect("feed");
        }
        segment.clear();
    }
    assert!(checked, "no feed touches a table its segment read");
}

#[test]
fn a_binding_made_before_a_feed_is_refused_after_it() {
    replay_to_a_touching_feed(false);
    replay_to_a_touching_feed(true);
}

#[test]
fn a_binding_of_another_catalog_is_refused() {
    let data = read_json("shadow-capture.pg.json");
    let entry = &data["sizes"]["65"];
    let (a, b) = (
        Catalog::from_capture(entry).expect("catalog"),
        Catalog::from_capture(entry).expect("catalog"),
    );
    let bound = a.bind().expect("bind");
    let call = &entry["calls"].as_array().expect("calls")[0];
    assert!(matches!(
        shadow_verdict_bound(&a, &bound, call, false),
        Verdict::Equal(_)
    ));
    match shadow_verdict_bound(&b, &bound, call, false) {
        Verdict::Refused(Refusal::Stale { reason, .. }) => {
            assert!(reason.contains("ForeignFrame"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

type Expected = &'static [(
    &'static str,
    Option<&'static str>,
    usize,
    &'static [&'static str],
)];

/// Tables, tables with a class, tables without, laned columns bound to a class field.
type Totals = (usize, usize, usize, usize);

fn print_and_check(label: &str, report: &[TableBinding], expected: Expected, totals: Totals) {
    for t in report {
        println!(
            "BINDING {label} {} class={:?} bound={} frame_only={:?}",
            t.table,
            t.class,
            t.bound.len(),
            t.frame_only
        );
    }
    let with_class = report.iter().filter(|t| t.class.is_some()).count();
    let bound: usize = report.iter().map(|t| t.bound.len()).sum();
    println!(
        "BINDING {label} SUMMARY tables={} with_class={with_class} without_class={} bound_columns={bound}",
        report.len(),
        report.len() - with_class,
    );
    assert_eq!(
        (report.len(), with_class, report.len() - with_class, bound),
        totals,
        "{label}: totals"
    );
    let got: Vec<(&str, Option<&str>, usize, Vec<&str>)> = report
        .iter()
        .map(|t| {
            (
                t.table.as_str(),
                t.class.as_deref(),
                t.bound.len(),
                t.frame_only.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let want: Vec<(&str, Option<&str>, usize, Vec<&str>)> = expected
        .iter()
        .map(|&(table, class, bound, only)| (table, class, bound, only.to_vec()))
        .collect();
    assert_eq!(got, want, "{label}");
}

#[test]
fn the_class_bound_columns_are_counted() {
    let items = scenario();
    let by_table = common::models_of(items.iter().filter(|i| i.get("terminal").is_some()));
    let Some(classes) = common::class_map(&by_table) else {
        return;
    };
    let mut catalog = Catalog::new();
    catalog.set_classes(classes).expect("classes");
    for feed in items.iter().filter_map(|i| i.get("feed")) {
        catalog.apply_feed(feed).expect("feed");
    }
    let report = catalog.bind().expect("bind").report(&catalog);
    print_and_check("scenario", &report, SCENARIO, (15, 9, 6, 163));

    for (label, file, size, expected, totals) in [
        (
            "capture",
            "shadow-capture.pg.json",
            "4097",
            CAPTURE,
            (6, 6, 0, 10),
        ),
        ("rows", "shadow-rows.pg.json", "1025", ROWS, (3, 3, 0, 25)),
        (
            "joins",
            "shadow-joins.pg.json",
            "seed",
            JOINS,
            (4, 2, 2, 19),
        ),
    ] {
        let data = read_json(file);
        let entry = &data["sizes"][size];
        let calls = entry["calls"].as_array().expect("calls");
        let Some(classes) = common::class_map(&common::models_of(calls)) else {
            return;
        };
        let mut catalog = Catalog::from_capture(entry).expect("catalog");
        catalog.set_classes(classes).expect("classes");
        let report = catalog.bind().expect("bind").report(&catalog);
        print_and_check(label, &report, expected, totals);
    }
}

const SCENARIO: Expected = &[
    ("groups", Some("Group"), 18, &[]),
    ("organizations", Some("Organization"), 12, &[]),
    (
        "permission_ancestors",
        None,
        0,
        &["permission_id", "ancestor_id"],
    ),
    ("permissions", Some("Permission"), 9, &[]),
    ("permissions_roles", None, 0, &["role_id", "permission_id"]),
    (
        "roles",
        None,
        0,
        &[
            "id",
            "name",
            "preferences",
            "default_at_signup",
            "active",
            "note",
            "updated_by_id",
            "created_by_id",
            "created_at",
            "updated_at",
        ],
    ),
    ("roles_users", None, 0, &["user_id", "role_id"]),
    (
        "ticket_article_senders",
        None,
        0,
        &[
            "id",
            "name",
            "note",
            "updated_by_id",
            "created_by_id",
            "created_at",
            "updated_at",
        ],
    ),
    (
        "ticket_article_types",
        None,
        0,
        &[
            "id",
            "name",
            "note",
            "communication",
            "active",
            "updated_by_id",
            "created_by_id",
            "created_at",
            "updated_at",
        ],
    ),
    ("ticket_articles", Some("Ticket::Article"), 22, &[]),
    ("ticket_priorities", Some("Ticket::Priority"), 11, &[]),
    ("ticket_state_types", Some("Ticket::StateType"), 7, &[]),
    ("ticket_states", Some("Ticket::State"), 14, &[]),
    ("tickets", Some("Ticket"), 39, &[]),
    (
        "users",
        Some("User"),
        31,
        &["updated_by_id", "created_by_id"],
    ),
];

const CAPTURE: Expected = &[
    ("checklist_items", Some("Checklist::Item"), 2, &[]),
    ("data_privacy_tasks", Some("DataPrivacyTask"), 1, &[]),
    ("import_jobs", Some("ImportJob"), 3, &[]),
    (
        "knowledge_base_categories",
        Some("KnowledgeBase::Category"),
        1,
        &[],
    ),
    ("ticket_states", Some("Ticket::State"), 2, &[]),
    ("tickets", Some("Ticket"), 1, &[]),
];

const ROWS: Expected = &[
    ("import_jobs", Some("ImportJob"), 9, &[]),
    (
        "knowledge_base_categories",
        Some("KnowledgeBase::Category"),
        2,
        &[
            "id",
            "category_icon",
            "position",
            "category_sorting_mode",
            "answer_sorting_mode",
            "created_at",
            "updated_at",
        ],
    ),
    ("ticket_states", Some("Ticket::State"), 14, &[]),
];

const JOINS: Expected = &[
    ("permissions", Some("Permission"), 9, &[]),
    ("permissions_roles", None, 0, &["role_id", "permission_id"]),
    ("roles", Some("Role"), 10, &[]),
    ("roles_users", None, 0, &["user_id", "role_id"]),
];

/// Every `(table, column)` a call reads to select rows: its WHERE leaves, its
/// order keys and both sides of its join edges.
fn columns_read(call: &Value) -> std::collections::BTreeSet<(String, String)> {
    fn leaves(
        node: &Value,
        call_table: &str,
        out: &mut std::collections::BTreeSet<(String, String)>,
    ) {
        if let (Some(table), Some(col)) = (node["table"].as_str(), node["col"].as_str()) {
            out.insert((table.to_owned(), col.to_owned()));
        }
        if let (Some("bound_sql"), Some(sql)) = (node["op"].as_str(), node["sql"].as_str()) {
            for col in
                quack_shadow::columns_of(call_table, sql).expect("a fragment the reader knows")
            {
                out.insert((call_table.to_owned(), col));
            }
        }
        for arg in node["args"].as_array().into_iter().flatten() {
            leaves(arg, call_table, out);
        }
        if node.get("arg").is_some() {
            leaves(&node["arg"], call_table, out);
        }
    }
    let table = call["table"].as_str().unwrap_or_default();
    let mut out = std::collections::BTreeSet::new();
    leaves(&call["where"], table, &mut out);
    for key in call["order"].as_array().into_iter().flatten() {
        leaves(key, table, &mut out);
    }
    for edge in call["join"].as_array().into_iter().flatten() {
        leaves(&edge["left"], table, &mut out);
        leaves(&edge["right"], table, &mut out);
    }
    out
}

// Pinned from a measured run; see the COVERAGE lines.
const WIDE_CALLS: (usize, usize, usize, usize) = (455, 57, 0, 0);

/// Per call of the wide workload: are the columns it selects rows by read
/// through a class binding?
#[test]
fn the_wide_workload_is_counted_per_call() {
    use std::collections::{BTreeMap, BTreeSet};

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
    let calls: Vec<&Value> = items
        .iter()
        .filter(|v| v.get("terminal").is_some())
        .collect();
    let Some(classes) = common::class_map(&common::models_of(calls.iter().copied())) else {
        return;
    };
    let mut catalog = Catalog::new();
    catalog.set_classes(classes).expect("classes");
    let (mut all, mut some, mut none, mut reads_nothing) = (0usize, 0usize, 0usize, 0usize);
    let mut frame_only: BTreeMap<(String, String), usize> = BTreeMap::new();
    for item in &items {
        if let Some(feed) = item.get("feed") {
            catalog.apply_feed(feed).expect("feed");
            continue;
        }
        if item.get("terminal").is_none() {
            continue;
        }
        let bound: BTreeSet<(String, String)> = catalog
            .bind()
            .expect("bind")
            .report(&catalog)
            .into_iter()
            .flat_map(|t| {
                let table = t.table;
                t.bound.into_iter().map(move |c| (table.clone(), c))
            })
            .collect();
        let read = columns_read(item);
        let unbound: Vec<&(String, String)> = read.difference(&bound).collect();
        for column in &unbound {
            *frame_only.entry((*column).clone()).or_default() += 1;
        }
        match (read.len(), unbound.len()) {
            (0, _) => reads_nothing += 1,
            (_, 0) => all += 1,
            (n, u) if n == u => none += 1,
            _ => some += 1,
        }
    }
    println!(
        "COVERAGE calls={} all_class_bound={all} partly={some} none={none} reads_no_column={reads_nothing}",
        calls.len()
    );
    for ((table, column), count) in &frame_only {
        println!("COVERAGE frame_only {table}.{column} in {count} calls");
    }
    assert_eq!((all, some, none, reads_nothing), WIDE_CALLS);
}

/// An update of an existing key adds no tile, so only the frame says the
/// table moved.
#[test]
fn an_in_place_update_makes_an_older_binding_stale() {
    use serde_json::json;

    let capture = read_json("shadow-capture.pg.json");
    let mut call = capture["sizes"]["65"]["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .find(|c| c["label"] == "Ticket::State.all" && c["terminal"] == "count")
        .expect("an unfiltered count call")
        .clone();
    call["table"] = json!("t");
    call["answer"] = json!(3);

    let columns = json!([
        {"name": "id", "type": "integer", "null": false},
        {"name": "n", "type": "integer", "null": true}
    ]);
    let mut catalog = Catalog::new();
    catalog
        .apply_feed(&json!({"tables": {"t": {
            "columns": columns, "upsert": [[1, 10], [2, 20], [3, 30]], "delete": []
        }}}))
        .expect("first feed");
    let bound = catalog.bind().expect("bind");
    assert_eq!(
        shadow_verdict_bound(&catalog, &bound, &call, false),
        Verdict::Equal(quack_shadow::Answer::Count(3)),
        "the binding reads the table it was made over"
    );
    let rows = catalog.tables[0].rows;
    catalog
        .apply_feed(&json!({"tables": {"t": {"upsert": [[2, 21]], "delete": []}}}))
        .expect("update");
    assert_eq!(catalog.tables[0].rows, rows, "the update added no position");
    match shadow_verdict_bound(&catalog, &bound, &call, false) {
        Verdict::Refused(Refusal::Stale { table, reason }) => {
            assert_eq!(table, "t");
            println!("STALE after an in-place update: {reason}");
        }
        other => panic!("the old binding still reads t: {other:?}"),
    }
}
