// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A SQL fragment with positional binds is read into the filter the JSON ops
//! give: `=` is the `eq` leaf, and a LIKE pattern is matched against the
//! strings of the column's code space at the current frame.

mod common;

use quack_shadow::{Answer, Catalog, Refusal, answer, answer_bound};
use serde_json::{Value, json};

fn feed(catalog: &mut Catalog, entry: &Value) {
    catalog
        .apply_feed(&json!({"tables": {"t": entry}}))
        .expect("feed");
}

/// `t(id, name, active, n)`: a NULL name, a NULL `active`, and names holding `%` and `_`.
fn catalog() -> Catalog {
    let mut catalog = Catalog::new();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "name", "type": "character varying", "null": true},
                {"name": "active", "type": "boolean", "null": true},
                {"name": "n", "type": "integer", "null": true},
            ],
            "upsert": [
                [1, "admin", true, 1],
                [2, "admin.user", true, 2],
                [3, "admin.user.x", false, 3],
                [4, "adminx", null, 4],
                [5, "ticket.agent", true, 5],
                [6, null, true, 6],
                [7, "50%", false, 7],
                [8, "50x", false, 8],
                [9, "a_b", true, 9],
                [10, "axb", true, 10],
            ],
            "delete": [],
        }),
    );
    catalog
}

fn call(filter: Value) -> Value {
    json!({
        "table": "t", "terminal": "count", "where": filter,
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        "answer": 0,
    })
}

fn fragment(sql: &str, binds: &Value) -> Value {
    call(json!({"op": "bound_sql", "sql": sql, "binds": binds}))
}

fn counted(catalog: &Catalog, sql: &str, binds: &Value) -> Result<Answer, Refusal> {
    answer(catalog, &fragment(sql, binds), false)
}

fn eq(col: &str, value: &Value) -> Value {
    json!({"table": "t", "col": col, "op": "eq", "value": value})
}

#[test]
fn a_like_pattern_selects_the_matching_strings() {
    let catalog = catalog();
    let both = "name = ? OR name LIKE ?";
    let count = |sql: &str, binds: Value| counted(&catalog, sql, &binds);

    assert_eq!(
        count(both, json!(["admin", "admin.%"])),
        Ok(Answer::Count(3))
    );
    assert_eq!(
        count("name LIKE ?", json!(["admin_"])),
        Ok(Answer::Count(1))
    );
    assert_eq!(
        count("name LIKE ?", json!(["%"])),
        Ok(Answer::Count(9)),
        "every row whose name is not NULL"
    );
    assert_eq!(
        count("name LIKE ?", json!(["nothing%"])),
        Ok(Answer::Count(0))
    );
    assert_eq!(count("name LIKE ?", json!(["50%"])), Ok(Answer::Count(2)));
    assert_eq!(count("name LIKE ?", json!(["50\\%"])), Ok(Answer::Count(1)));
    assert_eq!(count("name LIKE ?", json!(["a_b"])), Ok(Answer::Count(2)));
    assert_eq!(count("name LIKE ?", json!(["a\\_b"])), Ok(Answer::Count(1)));
    assert_eq!(
        count("name LIKE ?", json!(["ADMIN%"])),
        Ok(Answer::Count(0)),
        "LIKE is case-sensitive"
    );
    assert_eq!(
        count("name LIKE ?", json!(["admin"])),
        Ok(Answer::Count(1)),
        "the whole string"
    );
}

#[test]
fn a_fragment_answers_like_the_same_condition_in_ops() {
    let catalog = catalog();
    let sql = "t.active = ? AND (t.name = ? OR t.name = ?)";
    // (active, first name, second name, rows), the rows pinned beside the equality.
    let cases = [
        (true, "admin", "ticket.agent", 2),
        (true, "adminx", "admin.user", 1),
        // Row 4 has a NULL `active`: `NULL = false` is unknown, so it is not kept.
        (false, "adminx", "admin.user.x", 1),
    ];
    for (active, a, b, expected) in cases {
        let ops = call(json!({"op": "and", "args": [
            eq("active", &json!(active)),
            {"op": "or", "args": [eq("name", &json!(a)), eq("name", &json!(b))]},
        ]}));
        let ours = answer(&catalog, &fragment(sql, &json!([active, a, b])), false);
        assert_eq!(ours, answer(&catalog, &ops, false), "{active} {a} {b}");
        assert_eq!(ours, Ok(Answer::Count(expected)), "{active} {a} {b}");
    }
}

#[test]
fn what_the_reader_does_not_know_is_refused_by_name() {
    let catalog = catalog();
    let refused = |sql: &str, binds: Value, says: &str| match counted(&catalog, sql, &binds) {
        Err(Refusal::BoundSql(message)) => {
            assert!(message.contains(says), "`{sql}`: {message}");
        }
        other => panic!("`{sql}`: {other:?}"),
    };
    refused("name ILIKE ?", json!(["a"]), "ILIKE");
    refused("lower(name) = ?", json!(["a"]), "lower");
    refused("name = ? AND", json!(["a"]), "end of the fragment");
    refused("name = ? OR name = ?", json!(["a"]), "no bind");
    refused("name = ?", json!(["a", "b"]), "2 binds given for 1");
    refused("other.name = ?", json!(["a"]), "another table");
    refused("n LIKE ?", json!(["1%"]), "text column");
    refused("name LIKE ?", json!([null]), "string bind");
    refused("name LIKE ?", json!([5]), "string bind");
    refused("name = ?", json!([null]), "NULL");
    refused("name < ?", json!(["a"]), "`<` is not understood");
    refused("(name = ?", json!(["a"]), "expected `)`");
    refused("name = ? name", json!(["a"]), "not understood after");
    refused("name LIKE ?", json!(["a\\"]), "escape");

    assert_eq!(
        counted(
            &catalog,
            "t.name = ? OR \"t\".\"name\" like ?",
            &json!(["admin", "admin.%"])
        ),
        Ok(Answer::Count(3)),
        "the valid fragment is not refused"
    );
}

#[test]
fn a_like_binds_at_the_current_frame() {
    let mut catalog = catalog();
    let (sql, binds) = ("name LIKE ?", json!(["admin%"]));
    let before = fragment(sql, &binds);
    let bound = catalog.bind().expect("bind");
    assert_eq!(
        answer_bound(&catalog, &bound, &before, false),
        Ok(Answer::Count(4))
    );

    feed(
        &mut catalog,
        &json!({"upsert": [[11, "admin.new", true, 11]], "delete": []}),
    );
    assert!(matches!(
        answer_bound(&catalog, &bound, &before, false),
        Err(Refusal::Stale { .. })
    ));
    assert_eq!(answer(&catalog, &before, false), Ok(Answer::Count(5)));
    let fresh = catalog.bind().expect("rebind");
    assert_eq!(
        answer_bound(&catalog, &fresh, &before, false),
        Ok(Answer::Count(5))
    );
}

/// The permission lookups of the wide workload, read as fragments: PostgreSQL's answers hold.
#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn the_permission_lookups_of_the_wide_workload_are_counted() {
    use quack_shadow::{LikeStats, Verdict, shadow_verdict};
    use std::collections::BTreeMap;

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
        .filter(|i| i.get("terminal").is_some())
        .collect();
    let mut catalog = Catalog::new();
    if let Some(classes) = common::class_map(&common::models_of(calls.iter().copied())) {
        catalog.set_classes(classes).expect("classes");
    }
    let mut by_binds: BTreeMap<usize, usize> = BTreeMap::new();
    for item in &items {
        if let Some(feed) = item.get("feed") {
            catalog.apply_feed(feed).expect("feed");
            continue;
        }
        let Some(binds) = item
            .get("where")
            .filter(|w| w["op"] == "bound_sql")
            .and_then(|w| w["binds"].as_array())
        else {
            continue;
        };
        *by_binds.entry(binds.len()).or_default() += 1;
        match shadow_verdict(&catalog, item, false) {
            Verdict::Equal(_) => {}
            other => panic!("{}: {other:?}", item["sql"]),
        }
    }
    let stats = catalog.like_stats();
    println!("LIKE binds={by_binds:?} stats={stats:?}");
    // Pinned from a measured run: 4 or 7 `= ? OR LIKE ?` pairs after `active = ?`.
    assert_eq!(
        (by_binds.into_iter().collect::<Vec<_>>(), stats),
        (
            vec![(9, 42), (15, 15)],
            LikeStats {
                fragments: 57,
                like_leaves: 273,
                matched_codes: 15
            }
        )
    );
}

#[test]
fn columns_of_names_the_columns_of_a_fragment() {
    use quack_shadow::columns_of;

    let names = |sql: &str| columns_of("permissions", sql);
    let want = Ok(vec!["active".to_owned(), "name".to_owned()]);
    assert_eq!(
        names(
            "permissions.active = ? AND (permissions.name = ? OR permissions.name LIKE ? OR permissions.name = ?)"
        ),
        want
    );
    assert_eq!(
        names("active = ? AND (name = ? OR name LIKE ? OR name = ? OR name LIKE ? OR name = ?)"),
        want
    );
    assert_eq!(
        names("\"permissions\".\"active\" = ? AND (\"name\" = ? OR \"name\" LIKE ?)"),
        want
    );
    for sql in [
        "name ILIKE ?",
        "lower(name) = ?",
        "name = ? AND",
        "other.name = ?",
    ] {
        assert!(names(sql).is_err(), "`{sql}`");
    }
    assert!(
        names("other.name = ?")
            .expect_err("refused")
            .contains("another table")
    );
}

#[test]
fn every_fragment_of_the_wide_workload_names_columns_of_its_table() {
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
    let mut catalog = Catalog::new();
    let mut nodes = 0;
    for item in &items {
        if let Some(feed) = item.get("feed") {
            catalog.apply_feed(feed).expect("feed");
            continue;
        }
        let Some(node) = item.get("where").filter(|w| w["op"] == "bound_sql") else {
            continue;
        };
        nodes += 1;
        let table = item["table"].as_str().expect("table");
        let names = quack_shadow::columns_of(table, node["sql"].as_str().expect("sql"))
            .expect("the reader accepts it");
        let columns = &catalog
            .tables
            .iter()
            .find(|t| t.name == table)
            .expect("table")
            .columns;
        for name in &names {
            assert!(columns.iter().any(|c| &c.name == name), "{table}.{name}");
        }
    }
    assert_eq!(nodes, 57);
}
