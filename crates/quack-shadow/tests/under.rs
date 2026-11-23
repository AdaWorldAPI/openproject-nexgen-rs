// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The permission lookup answered over the relation the intake declares: a name
//! is a member, `permission_ancestors` puts members under members, and no text
//! is compared after the name was read.

mod common;

use quack_shadow::{Answer, Catalog, Refusal, answer, rows};
#[cfg(not(feature = "disable-sql-where"))]
use quack_shadow::{Verdict, shadow_verdict};
use serde_json::{Value, json};

fn under(names: &[&str]) -> Value {
    json!({
        "op": "under", "table": "permissions", "via": "permission_ancestors",
        "member": "permission_id", "of": "ancestor_id",
        "names": {"col": "name", "values": names},
    })
}

fn active() -> Value {
    json!({"table": "permissions", "col": "active", "op": "eq", "value": true})
}

fn call(filter: Value) -> Value {
    json!({
        "table": "permissions", "terminal": "rows", "where": filter, "select": ["name"],
        "order": [{"table": "permissions", "col": "id", "dir": "asc"}],
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

fn names(catalog: &Catalog, filter: Value) -> Result<Vec<String>, Refusal> {
    let got = rows(catalog, &call(filter), false)?;
    Ok(got
        .rows
        .iter()
        .map(|r| r[0].as_str().expect("name").to_owned())
        .collect())
}

fn permissions(upsert: Value) -> Value {
    json!({"tables": {"permissions": {
        "columns": [
            {"name": "id", "type": "integer", "null": false},
            {"name": "name", "type": "character varying", "null": false},
            {"name": "active", "type": "boolean", "null": false},
        ],
        "upsert": upsert, "delete": [],
    }}})
}

fn ancestors(pairs: Value) -> Value {
    json!({"tables": {"permission_ancestors": {
        "columns": [
            {"name": "permission_id", "type": "integer", "null": false},
            {"name": "ancestor_id", "type": "integer", "null": false},
        ],
        "replace": pairs,
    }}})
}

fn declared() -> Catalog {
    let mut catalog = Catalog::new();
    catalog
        .apply_feed(&permissions(json!([
            [1, "a", true],
            [2, "a.b.c", true],
            [3, "ab", true],
            [4, "b", true],
        ])))
        .expect("permissions");
    // `a.b` has no row, so `a.b.c` sits under `a` directly.
    catalog
        .apply_feed(&ancestors(json!([[2, 1]])))
        .expect("ancestors");
    common::declare_edge(
        &mut catalog,
        "permission_ancestors",
        "permission_id",
        "permissions",
    );
    common::declare_edge(
        &mut catalog,
        "permission_ancestors",
        "ancestor_id",
        "permissions",
    );
    catalog
}

#[cfg(not(feature = "disable-sql-where"))]
/// `sanitize_sql_like`: the escape Rails puts on a name before it appends `.%`.
fn quote_like(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn the_permission_lookups_answered_over_the_relation_equal_postgresql() {
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
    let (mut lookups, mut with_members_under, mut changed_by_relation) = (0, 0, 0);
    let (mut named, mut under_total) = (0u64, 0u64);
    for item in &items {
        if let Some(feed) = item.get("feed") {
            catalog.apply_feed(feed).expect("feed");
            continue;
        }
        let Some(node) = item.get("where").filter(|w| w["op"] == "bound_sql") else {
            continue;
        };
        let binds = node["binds"].as_array().expect("binds");
        let wanted: Vec<&str> = binds[1..]
            .iter()
            .step_by(2)
            .map(|b| b.as_str().expect("name"))
            .collect();
        for (name, pattern) in wanted.iter().zip(binds[2..].iter().step_by(2)) {
            assert_eq!(
                pattern.as_str(),
                Some(format!("{}.%", quote_like(name)).as_str()),
                "{name}"
            );
        }
        assert_eq!(wanted.len() * 2 + 1, binds.len());
        lookups += 1;

        let method = |filter: Value| {
            let mut c = item.clone();
            c["where"] = filter;
            c
        };
        let relational = method(json!({"op": "and", "args": [
            {"table": "permissions", "col": "active", "op": "eq", "value": binds[0]},
            under(&wanted),
        ]}));
        let before = catalog.under_stats();
        match shadow_verdict(&catalog, &relational, false) {
            Verdict::Equal(_) => {}
            other => panic!("{}: {other:?}", item["sql"]),
        }
        let after = catalog.under_stats();
        assert_eq!(after.calls, before.calls + 1);
        named += after.members_named - before.members_named;
        let under_here = after.members_under - before.members_under;
        under_total += under_here;
        if under_here > 0 {
            with_members_under += 1;
        }

        let sorted = |call: &Value| {
            let mut got: Vec<String> = rows(&catalog, call, false)
                .expect("rows")
                .rows
                .iter()
                .map(|r| r[0].to_string())
                .collect();
            got.sort();
            got
        };
        let by_text = sorted(item);
        assert_eq!(sorted(&relational), by_text, "{}", item["sql"]);

        let members_only = method(json!({"op": "and", "args": [
            {"table": "permissions", "col": "active", "op": "eq", "value": binds[0]},
            {"table": "permissions", "col": "name", "op": "in", "values": wanted},
        ]}));
        if sorted(&members_only) != by_text {
            changed_by_relation += 1;
        }
    }
    println!(
        "UNDER lookups={lookups} with_members_under={with_members_under} changed={changed_by_relation} named={named} under={under_total}"
    );
    assert_eq!(lookups, 57);
    // Pinned from a measured run. The relation puts members under 15 lookups, but every
    // one of them already names those members itself, so no answer changes: the text
    // form and the members-only form agree with the relation form on all 57.
    #[cfg(not(feature = "disable-under-relation"))]
    assert_eq!((with_members_under, under_total), (15, 15));
    assert_eq!((named, changed_by_relation), (273, 0));
}

#[test]
fn a_descendant_is_found_without_its_parent_row() {
    let catalog = declared();
    assert_eq!(
        names(&catalog, under(&["a"])),
        Ok(vec!["a".to_owned(), "a.b.c".to_owned()])
    );
    assert_eq!(names(&catalog, under(&["ab"])), Ok(vec!["ab".to_owned()]));
    assert_eq!(
        names(&catalog, under(&["a.b"])),
        Ok(vec![]),
        "no such member"
    );
    assert_eq!(
        names(&catalog, under(&["a", "b", "nothing"])),
        Ok(vec!["a".to_owned(), "a.b.c".to_owned(), "b".to_owned()])
    );
    assert_eq!(catalog.under_stats().members_under, 2);
}

/// PostgreSQL, checked on a scratch table: `name = 'a' OR name LIKE 'a.%'` still
/// returns `a.b.c` after the row `a` was deleted, because the text still starts with
/// `a.`. The relation holds only pairs of existing permissions, so once `a` is gone
/// nothing is under it: that is the one place the two forms differ.
#[test]
fn an_inactive_or_removed_member_is_not_returned() {
    let mut catalog = declared();
    let active_only = |catalog: &Catalog| {
        names(
            catalog,
            json!({"op": "and", "args": [active(), under(&["a"])]}),
        )
    };
    assert_eq!(
        active_only(&catalog),
        Ok(vec!["a".to_owned(), "a.b.c".to_owned()])
    );

    catalog
        .apply_feed(
            &json!({"tables": {"permissions": {"upsert": [[2, "a.b.c", false]], "delete": []}}}),
        )
        .expect("deactivate");
    assert_eq!(active_only(&catalog), Ok(vec!["a".to_owned()]));
    assert_eq!(
        names(&catalog, under(&["a"])),
        Ok(vec!["a".to_owned(), "a.b.c".to_owned()]),
        "without the active conjunct it is still there"
    );

    catalog
        .apply_feed(
            &json!({"tables": {"permissions": {"upsert": [[2, "a.b.c", true]], "delete": []}}}),
        )
        .expect("reactivate");
    catalog
        .apply_feed(&json!({"tables": {"permissions": {"upsert": [], "delete": [2]}}}))
        .expect("delete descendant");
    catalog
        .apply_feed(&ancestors(json!([])))
        .expect("ancestors");
    assert_eq!(active_only(&catalog), Ok(vec!["a".to_owned()]));

    let mut orphaned = declared();
    orphaned
        .apply_feed(&json!({"tables": {"permissions": {"upsert": [], "delete": [1]}}}))
        .expect("delete the named member");
    orphaned
        .apply_feed(&ancestors(json!([])))
        .expect("ancestors");
    // PostgreSQL would still return `a.b.c` here; the relation form does not.
    assert_eq!(active_only(&orphaned), Ok(vec![]));
}

#[test]
fn an_undeclared_relation_is_refused_by_name() {
    let mut catalog = Catalog::new();
    catalog
        .apply_feed(&permissions(json!([[1, "a", true], [2, "a.b", true]])))
        .expect("permissions");
    catalog
        .apply_feed(&ancestors(json!([[2, 1]])))
        .expect("ancestors");
    let refusal = |column: &str| Refusal::UndeclaredRelation {
        table: "permission_ancestors".to_owned(),
        column: column.to_owned(),
    };
    let asked = |catalog: &Catalog| answer(catalog, &count(under(&["a"])), false);
    assert_eq!(asked(&catalog), Err(refusal("permission_id")));

    common::declare_edge(
        &mut catalog,
        "permission_ancestors",
        "permission_id",
        "permissions",
    );
    assert_eq!(asked(&catalog), Err(refusal("ancestor_id")));

    common::declare_edge(
        &mut catalog,
        "permission_ancestors",
        "ancestor_id",
        "permissions",
    );
    #[cfg(not(feature = "disable-under-relation"))]
    assert_eq!(asked(&catalog), Ok(Answer::Count(2)));
    #[cfg(feature = "disable-under-relation")]
    assert_eq!(asked(&catalog), Ok(Answer::Count(1)));
}

fn count(filter: Value) -> Value {
    let mut c = call(filter);
    c["terminal"] = json!("count");
    c["answer"] = json!(0);
    c
}
