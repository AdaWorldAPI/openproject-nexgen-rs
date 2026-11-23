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

/// The demo seed: every table's catalog count equals its seed rows, and the
/// write-side store agrees.
#[test]
fn the_demo_seed_loads_and_every_count_matches() {
    let doc: J = serde_json::from_str(include_str!("../seed/demo.json")).expect("demo.json");
    let desk = Desk::from_seed(&doc).expect("demo desk");
    let seed = doc["seed"].as_object().expect("seed");
    assert!(seed.len() >= 14, "demo seeds the slice: {:?}", seed.keys());
    for (table, rows) in seed {
        let n = rows.as_array().expect("rows").len() as i64;
        assert_eq!(desk.count(table), Ok(n), "{table}: catalog count");
        let spec = op_desk::desk::table(table).expect("spec");
        assert_eq!(desk.store().count(spec) as i64, n, "{table}: store count");
    }
    let wps = desk.read_all("work_packages").expect("work packages");
    assert_eq!(wps.rows.len(), 40, "the demo board's 40 work packages");
    let project_1 = desk
        .read("work_packages", "project_id", json!(1))
        .expect("project 1");
    assert!(!project_1.rows.is_empty() && project_1.rows.len() < 40);
}
