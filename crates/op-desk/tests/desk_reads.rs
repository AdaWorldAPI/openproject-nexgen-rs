//! The desk answers reads through its Quack catalog, over the rows the store
//! was seeded with.

use op_desk::desk::{Desk, DeskError};
use serde_json::{json, Value as J};

const T: i64 = 1_783_507_124_000_000; // 2026-07-08T10:38:44Z in microseconds

fn status(id: i64, name: &str, position: i64) -> J {
    json!({"id": id, "name": name, "position": position, "created_at": T, "updated_at": T})
}

fn small_seed() -> J {
    json!({"seed_time": 1_783_507_124_000_i64, "seed": {"statuses": [
        status(1, "New", 1), status(2, "In progress", 2), status(3, "Closed", 3),
    ]}})
}

#[test]
fn the_catalog_answers_the_seeded_rows() {
    let desk = Desk::from_seed(&small_seed()).expect("desk");
    assert_eq!(desk.unfed(), 0, "every commit reached the catalog");
    let rows = desk.read_all("statuses").expect("statuses");
    let names: Vec<&str> = rows
        .rows
        .iter()
        .map(|r| rows.cell(r, "name").and_then(J::as_str).expect("name"))
        .collect();
    assert_eq!(names, ["New", "In progress", "Closed"]);
    assert_eq!(desk.count("statuses"), Ok(3));
    let one = desk.read("statuses", "id", json!(2)).expect("by id");
    assert_eq!(one.rows.len(), 1);
    assert_eq!(one.cell(&one.rows[0], "name"), Some(&json!("In progress")));
}

/// A slice table without seed rows is fed as an empty table: reading it
/// answers nothing instead of being refused as unknown to the catalog.
#[test]
fn an_unseeded_slice_table_reads_as_empty() {
    let desk = Desk::from_seed(&small_seed()).expect("desk");
    for table in ["time_entries", "work_packages", "relations", "journals"] {
        assert_eq!(desk.read_all(table).map(|r| r.rows.len()), Ok(0), "{table}");
        assert_eq!(desk.count(table), Ok(0), "{table}");
    }
}

#[test]
fn a_table_or_column_outside_the_slice_is_refused() {
    let unknown_table = json!({"seed": {"tickets": []}});
    assert!(matches!(
        Desk::from_seed(&unknown_table),
        Err(DeskError::Seed(_))
    ));
    let duplicate_id = json!({"seed": {"statuses": [status(1, "New", 1), status(1, "Closed", 2)]}});
    assert!(matches!(
        Desk::from_seed(&duplicate_id),
        Err(DeskError::Seed(_))
    ));
    let unknown_column = json!({"seed": {"statuses": [{"id": 1, "nmae": "x"}]}});
    assert!(matches!(
        Desk::from_seed(&unknown_column),
        Err(DeskError::Seed(_))
    ));
    assert!(matches!(
        Desk::from_seed(&json!({})),
        Err(DeskError::Seed(_))
    ));
}

/// The demo seed: every table's catalog count equals its seed rows within the
/// table's default-scope filters, and the write-side store (unscoped) holds
/// every seed row.
#[test]
fn the_demo_seed_loads_and_every_count_matches() {
    let doc: J = serde_json::from_str(include_str!("../seed/demo.json")).expect("demo.json");
    let desk = Desk::from_seed(&doc).expect("demo desk");
    let seed = doc["seed"].as_object().expect("seed");
    assert!(seed.len() >= 14, "demo seeds the slice: {:?}", seed.keys());
    for (table, rows) in seed {
        let n = rows.as_array().expect("rows").len() as i64;
        let visible = rows
            .as_array()
            .expect("rows")
            .iter()
            .filter(|r| within_default_scope(table, r))
            .count() as i64;
        assert_eq!(desk.count(table), Ok(visible), "{table}: catalog count");
        let spec = op_desk::desk::table(table).expect("spec");
        assert_eq!(desk.store().count(spec) as i64, n, "{table}: store count");
    }
    let wps = desk.read_all("work_packages").expect("work packages");
    assert_eq!(wps.rows.len(), 40, "the demo board's 40 work packages");
    let project_1 = desk
        .read("work_packages", "project_id", json!(1))
        .expect("project 1");
    assert!(!project_1.rows.is_empty() && project_1.rows.len() < 40);
    assert!(
        project_1
            .rows
            .iter()
            .all(|r| project_1.cell(r, "project_id") == Some(&json!(1))),
        "every row the filter returns is in project 1"
    );
}

/// The store holds every seeded cell as written: nothing is lost on the way in
/// (a microsecond timestamp keeps its microseconds, a decimal its scale, text
/// its bytes). The PostgreSQL oracle is loaded from the store, so this check is
/// what keeps a loss on the way in from hiding in both sides.
#[test]
fn the_store_keeps_every_seeded_cell() {
    let doc: J = serde_json::from_str(include_str!("../seed/demo.json")).expect("demo.json");
    let desk = Desk::from_seed(&doc).expect("demo desk");
    let mut cells = 0;
    for (table, rows) in doc["seed"].as_object().expect("seed") {
        let spec = op_desk::desk::table(table).expect("spec");
        for row in rows.as_array().expect("rows") {
            let id = row["id"].as_i64().expect("id");
            let stored = desk
                .store()
                .find(spec, id)
                .expect("table")
                .expect("seeded row");
            for (column, cell) in row.as_object().expect("row") {
                let at = spec.index_of(column).expect("column");
                let back = op_desk::snapshot::cell_json(stored.get(at), spec.columns[at].ty);
                assert_eq!(&back, cell, "{table}#{id}.{column}");
                cells += 1;
            }
        }
    }
    assert!(cells > 1000, "{cells} cells checked");
}

/// Whether a seed row passes its table's default-scope filters.
fn within_default_scope(table: &str, row: &J) -> bool {
    use op_desk::scopes::{Lit, Op};
    op_desk::scopes::of_table(table).is_none_or(|s| {
        s.filters.iter().all(|f| {
            let want = match f.lit {
                Lit::Int(i) => json!(i),
                Lit::Str(v) => json!(v),
                Lit::Bool(b) => json!(b),
                Lit::Nil => J::Null,
            };
            let cell = row.get(f.col).cloned().unwrap_or(J::Null);
            match (f.op, want.is_null()) {
                (Op::Eq, _) => cell == want,
                (Op::Ne, true) => !cell.is_null(),
                // SQL `<>`: a null cell matches neither side.
                (Op::Ne, false) => !cell.is_null() && cell != want,
            }
        })
    })
}

fn names(rows: &op_desk::desk::Rows, col: &str) -> Vec<String> {
    rows.rows
        .iter()
        .map(|r| r[rows.columns.iter().position(|c| c == col).expect("column")].to_string())
        .collect()
}

/// OpenProject's default scopes shape the demo reads: statuses and types in
/// position order (not id order), enumerations by position, and a deleted user
/// (`Principal`'s `where.not(status: deleted)`) hidden from every user read
/// while a locked one stays.
#[test]
fn demo_reads_follow_openprojects_default_scopes() {
    let doc: J = serde_json::from_str(include_str!("../seed/demo.json")).expect("demo.json");
    let desk = Desk::from_seed(&doc).expect("demo desk");
    let statuses = desk.read_all("statuses").expect("statuses");
    assert_eq!(
        names(&statuses, "name"),
        [
            "New",
            "In progress",
            "On hold",
            "Resolved",
            "Closed",
            "Rejected"
        ]
        .map(|n| format!("{n:?}"))
    );
    let types = desk.read_all("types").expect("types");
    assert_eq!(
        names(&types, "name"),
        ["Epic", "Task", "Bug", "Feature", "Milestone"].map(|n| format!("{n:?}"))
    );
    let enums = desk.read_all("enumerations").expect("enumerations");
    assert_eq!(names(&enums, "id"), ["1", "5", "2", "6", "3", "4"]);
    let users = desk.read_all("users").expect("users");
    let logins = names(&users, "login");
    assert!(
        logins.contains(&"\"erin\"".to_string()),
        "a locked user stays: {logins:?}"
    );
    assert!(
        !logins.contains(&"\"frank\"".to_string()),
        "a deleted user is hidden: {logins:?}"
    );
    assert_eq!(desk.count("users"), Ok(6));
    assert!(desk
        .read("users", "login", json!("frank"))
        .expect("frank")
        .rows
        .is_empty());
    let spec = op_desk::desk::table("users").expect("users spec");
    assert_eq!(
        desk.store().count(spec),
        7,
        "the store keeps the deleted user"
    );
}
