// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A relation is declared when a catalog is taken in, and a join only looks it
//! up: it never decides from the query which column is a key or which table
//! multiplies the answer.

mod common;

use quack_shadow::{Catalog, Reading, Refusal, Relation, rows, rows_bound};
use serde_json::{Value, json};

fn feed_tables(catalog: &mut Catalog) {
    catalog
        .apply_feed(&json!({"tables": {
            "parents": {
                "columns": [
                    {"name": "id", "type": "integer", "null": false},
                    {"name": "name", "type": "character varying", "null": true},
                ],
                "upsert": [[1, "a"], [2, "b"]], "delete": [],
            },
            "kids": {
                "columns": [
                    {"name": "id", "type": "integer", "null": false},
                    {"name": "parent_id", "type": "integer", "null": false},
                ],
                "upsert": [[1, 1], [2, 1], [3, 2]], "delete": [],
            },
        }}))
        .expect("tables");
}

fn kids_call() -> Value {
    json!({
        "label": "Parent.joins(:kids).pluck(:name)",
        "table": "parents", "terminal": "rows", "select": ["name"],
        "where": null, "order": [], "limit": null, "offset": null, "joins": 1,
        "join": [{
            "type": "inner", "table": "kids",
            "left": {"table": "kids", "col": "parent_id"},
            "right": {"table": "parents", "col": "id"},
        }],
        "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

fn names(answer: &quack_shadow::RowsAnswer) -> Vec<String> {
    let mut out: Vec<String> = answer.rows.iter().map(|r| r[0].to_string()).collect();
    out.sort_unstable();
    out
}

fn declare(catalog: &mut Catalog, table: &str, column: &str, target: &str, reading: &str) {
    catalog
        .apply_feed(&json!({"relations": [{
            "table": table, "column": column, "target": target,
            "predicate": format!("{table}.{column}"), "reading": reading,
        }]}))
        .expect("relation");
}

#[test]
fn an_undeclared_join_is_refused_by_name() {
    let mut catalog = Catalog::new();
    feed_tables(&mut catalog);
    assert_eq!(
        rows(&catalog, &kids_call(), false).err(),
        Some(Refusal::UndeclaredRelation {
            table: "kids".to_owned(),
            column: "parent_id".to_owned(),
        })
    );
    declare(&mut catalog, "kids", "parent_id", "parents", "edge");
    let answer = rows(&catalog, &kids_call(), false).expect("declared");
    assert_eq!(names(&answer), ["\"a\"", "\"a\"", "\"b\""]);
}

/// permissions <- pr -> roles <- ru, with the pair (role 1, user 1) in `ru` twice.
fn occurrence_tables(catalog: &mut Catalog) {
    let ids = |extra: &str| {
        json!([
            {"name": "id", "type": "integer", "null": false},
            {"name": extra, "type": "integer", "null": false},
        ])
    };
    catalog
        .apply_feed(&json!({"tables": {
            "perms": {
                "columns": [
                    {"name": "id", "type": "integer", "null": false},
                    {"name": "name", "type": "character varying", "null": true},
                ],
                "upsert": [[1, "p"]], "delete": [],
            },
            "roles": {"columns": ids("n"), "upsert": [[1, 0]], "delete": []},
            "pr": {
                "columns": [
                    {"name": "id", "type": "integer", "null": false},
                    {"name": "perm_id", "type": "integer", "null": false},
                    {"name": "role_id", "type": "integer", "null": false},
                ],
                "upsert": [[1, 1, 1]], "delete": [],
            },
            "ru": {
                "columns": [
                    {"name": "id", "type": "integer", "null": false},
                    {"name": "user_id", "type": "integer", "null": false},
                    {"name": "role_id", "type": "integer", "null": false},
                ],
                "upsert": [[1, 1, 1], [2, 1, 1]], "delete": [],
            },
        }}))
        .expect("tables");
}

fn via_roles_call() -> Value {
    json!({
        "label": "Perm.joins(:roles).joins(:ru).pluck(:name)",
        "table": "perms", "terminal": "rows", "select": ["name"],
        "where": null, "order": [], "limit": null, "offset": null, "joins": 3,
        "join": [
            {"type": "inner", "table": "pr",
             "left": {"table": "perms", "col": "id"},
             "right": {"table": "pr", "col": "perm_id"}},
            {"type": "inner", "table": "roles",
             "left": {"table": "pr", "col": "role_id"},
             "right": {"table": "roles", "col": "id"}},
            {"type": "inner", "table": "ru",
             "left": {"table": "roles", "col": "id"},
             "right": {"table": "ru", "col": "role_id"}},
        ],
        "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

fn pair_call() -> Value {
    json!({
        "label": "Role.joins(:ru).pluck(:n)",
        "table": "roles", "terminal": "rows", "select": ["n"],
        "where": null, "order": [], "limit": null, "offset": null, "joins": 1,
        "join": [{"type": "inner", "table": "ru",
                  "left": {"table": "ru", "col": "role_id"},
                  "right": {"table": "roles", "col": "id"}}],
        "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

fn occurrence_catalog(ru: &str) -> Catalog {
    let mut catalog = Catalog::new();
    occurrence_tables(&mut catalog);
    declare(&mut catalog, "pr", "perm_id", "perms", "occurrences");
    declare(&mut catalog, "pr", "role_id", "roles", "occurrences");
    declare(&mut catalog, "ru", "role_id", "roles", ru);
    catalog
}

/// The declared reading decides which table is counted. A pair that occurs
/// twice in a declared class of occurrences is two rows, as PostgreSQL returns
/// it. The same rows with the table declared an edge are refused when that
/// table would have to be counted; as a plain dimension-holder the call still
/// returns each occurrence once per row it holds.
#[test]
fn the_reading_decides_multiplicity() {
    let counted = occurrence_catalog("occurrences");
    let answer = rows(&counted, &via_roles_call(), false).expect("occurrences");
    assert_eq!(names(&answer), ["\"p\"", "\"p\""]);
    assert_eq!(answer.join.expect("stats").sibling.as_deref(), Some("ru"));
    let pair = rows(&counted, &pair_call(), false).expect("pair");
    assert_eq!(pair.rows.len(), 2);

    let edge = occurrence_catalog("edge");
    #[cfg(not(feature = "disable-join-multiplicity"))]
    assert_eq!(
        rows(&edge, &via_roles_call(), false).err(),
        Some(Refusal::UnsupportedJoin(
            "shape and declared reading disagree".to_owned()
        ))
    );
    let pair = rows(&edge, &pair_call(), false).expect("plain dimension");
    assert_eq!(pair.rows.len(), 2);
}

#[test]
fn a_conflicting_redeclaration_is_refused() {
    let mut catalog = Catalog::new();
    declare(&mut catalog, "kids", "parent_id", "parents", "edge");
    declare(&mut catalog, "kids", "parent_id", "parents", "edge");
    assert_eq!(catalog.relations().len(), 1);
    for (target, reading) in [("others", "edge"), ("parents", "occurrences")] {
        let result = catalog.apply_feed(&json!({"relations": [{
            "table": "kids", "column": "parent_id", "target": target,
            "predicate": "kids.parent_id", "reading": reading,
        }]}));
        assert!(result.is_err(), "{target} {reading}");
    }
    assert_eq!(
        catalog.relations(),
        [Relation {
            source: "kids".to_owned(),
            column: "parent_id".to_owned(),
            target: "parents".to_owned(),
            predicate: "kids.parent_id".to_owned(),
            reading: Reading::Edge,
        }]
    );
}

#[test]
fn a_declaration_makes_an_older_binding_stale() {
    let mut catalog = Catalog::new();
    feed_tables(&mut catalog);
    let call = json!({
        "table": "kids", "terminal": "rows", "where": null, "select": "*",
        "order": [], "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    });
    let bound = catalog.bind().expect("bind");
    assert!(rows_bound(&catalog, &bound, &call, false).is_ok());
    declare(&mut catalog, "kids", "parent_id", "parents", "edge");
    assert!(matches!(
        rows_bound(&catalog, &bound, &call, false),
        Err(Refusal::Stale { table, .. }) if table == "kids"
    ));
    let again = catalog.bind().expect("rebind");
    assert!(rows_bound(&catalog, &again, &call, false).is_ok());
    declare(&mut catalog, "kids", "parent_id", "parents", "edge");
    assert!(
        rows_bound(&catalog, &again, &call, false).is_ok(),
        "the same declaration again is a no-op"
    );
}

#[test]
fn join_stats_name_the_relations() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-joins.pg.json"
    ))
    .expect("capture file");
    let data: Value = serde_json::from_str(&text).expect("capture json");
    let entry = &data["sizes"]["seed"];
    let catalog = common::captured(entry);
    let (pr_perm, pr_role, ru_role) = (
        "permissions_roles.permission_id",
        "permissions_roles.role_id",
        "roles_users.role_id",
    );
    let mut answered = 0;
    for call in entry["calls"].as_array().expect("calls") {
        let label = call["label"].as_str().expect("label");
        let Ok(answer) = rows(&catalog, call, false) else {
            continue;
        };
        answered += 1;
        let expected: Vec<&str> = if label.starts_with("User.find") {
            vec![pr_perm, pr_role, ru_role]
        } else if label.starts_with("Permission.joins") {
            vec![pr_perm, pr_role]
        } else {
            vec![pr_role, pr_perm]
        };
        assert_eq!(answer.join.expect("stats").relations, expected, "{label}");
    }
    assert_eq!(answered, 9);
}

fn capture_entry(file: &str, size: &str) -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("capture file");
    let data: Value = serde_json::from_str(&text).expect("capture json");
    data["sizes"][size].clone()
}

fn without_relations(entry: &Value) -> Value {
    let mut stripped = entry.clone();
    stripped
        .as_object_mut()
        .expect("size entry")
        .remove("relations")
        .expect("the capture declares relations");
    stripped
}

#[test]
fn the_intake_declares_what_the_hand_list_declared() {
    let entry = capture_entry("shadow-joins.pg.json", "seed");
    let catalog = Catalog::from_capture(&entry).expect("catalog");
    let joined: Vec<&Relation> = catalog
        .relations()
        .iter()
        .filter(|r| r.reading == Reading::Occurrences)
        .collect();
    let by_hand: Vec<Relation> = common::hand_relations()
        .into_iter()
        .filter(|r| r.reading == Reading::Occurrences)
        .collect();
    assert_eq!(by_hand.len(), 4);
    assert_eq!(joined.len(), 4);
    for hand in &by_hand {
        assert!(
            joined.contains(&hand),
            "{hand:?} not declared by the intake"
        );
    }
    // The four join-table edges and the two `belongs_to` of `roles`.
    assert_eq!(catalog.relations().len(), 6);

    let entry = capture_entry("shadow-capture.pg.json", "65");
    let intake = Catalog::from_capture(&entry).expect("catalog");
    assert_eq!(intake.relations().len(), 23);
    assert!(
        intake
            .relations()
            .iter()
            .all(|r| r.reading == Reading::Edge)
    );
    let calls = entry["calls"].as_array().expect("calls");
    let Some(classes) = common::class_map(&common::models_of(calls)) else {
        return;
    };
    let mut from_classes = Catalog::from_capture(&without_relations(&entry)).expect("catalog");
    assert_eq!(from_classes.relations(), []);
    from_classes.set_classes(classes).expect("classes");

    let mut compared = 0;
    for edge in from_classes.relations() {
        let declared = intake
            .relations()
            .iter()
            .find(|r| r.source == edge.source && r.column == edge.column)
            .unwrap_or_else(|| panic!("{edge:?} not stated by the intake"));
        assert_eq!(
            (&declared.target, declared.reading),
            (&edge.target, edge.reading),
            "{}.{}",
            edge.source,
            edge.column
        );
        compared += 1;
    }
    assert_eq!(compared, 4);
    for hand in common::hand_relations()
        .iter()
        .filter(|r| r.reading == Reading::Edge)
    {
        assert!(
            intake.relations().iter().any(|r| r.source == hand.source
                && r.column == hand.column
                && r.target == hand.target),
            "{hand:?}"
        );
    }
    let stats = from_classes.relation_stats();
    assert_eq!((stats.declared, stats.skipped), (4, 13));
    // The basis carries no habtm join table; the intake's reflections do.
    for join_table in ["roles_users", "permissions_roles"] {
        assert!(
            from_classes
                .relations()
                .iter()
                .all(|r| r.source != join_table),
            "{join_table}"
        );
    }

    let mut both = Catalog::from_capture(&entry).expect("catalog");
    let before = both.relations().len();
    let classes = common::class_map(&common::models_of(calls)).expect("class map");
    both.set_classes(classes)
        .expect("the class map agrees with the intake");
    assert_eq!(both.relations().len(), before, "the class map adds nothing");
}

/// The capture's own declaration is what makes its joins answer.
#[test]
fn a_capture_without_its_relations_refuses_its_joins() {
    let entry = capture_entry("shadow-joins.pg.json", "seed");
    let declared = Catalog::from_capture(&entry).expect("catalog");
    let silent = Catalog::from_capture(&without_relations(&entry)).expect("catalog");
    assert_eq!(silent.relations(), []);
    let (mut answered, mut refused) = (0, 0);
    for call in entry["calls"].as_array().expect("calls") {
        if rows(&declared, call, false).is_err() {
            continue;
        }
        answered += 1;
        assert!(
            matches!(
                rows(&silent, call, false),
                Err(Refusal::UndeclaredRelation { .. })
            ),
            "{}",
            call["label"]
        );
        refused += 1;
    }
    assert_eq!((answered, refused), (9, 9));
}
