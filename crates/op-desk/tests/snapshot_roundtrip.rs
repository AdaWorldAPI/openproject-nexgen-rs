//! A polyfill `Store` seeded from the generated specs, one row per table,
//! survives snapshot → reload → snapshot byte for byte.

use op_desk::generated::MODELS;
use op_desk::snapshot::{from_snapshot, snapshot};
use rails_polyfill::{ColumnType, Store, TableSpec, Value};

fn tables() -> Vec<&'static TableSpec> {
    MODELS.iter().map(|m| m.table).collect()
}

/// A value for every column, different per table and column, of every kind
/// the specs use (a fractional and an infinite float, a date before 1970).
fn cell(t: usize, c: usize, ty: ColumnType, name: &str) -> Value {
    let k = i64::try_from(t * 100 + c).expect("small");
    match ty {
        ColumnType::Integer if name == "id" => Value::Int(i64::try_from(t).expect("small") + 7),
        ColumnType::Integer => Value::Int(k),
        ColumnType::String => Value::str(format!("s{k} ü \"q\"")),
        ColumnType::Text => Value::str(format!("line {k}\r\nnext")),
        ColumnType::Boolean => Value::Bool(k.rem_euclid(2) == 0),
        ColumnType::DateTime => Value::Time(1_767_225_600_123 + k),
        ColumnType::Date => Value::Date(i32::try_from(k).expect("small") - 2000),
        ColumnType::Float if c.is_multiple_of(3) => Value::Float(f64::INFINITY),
        ColumnType::Float | ColumnType::Decimal => Value::Float(k as f64 + 0.25),
        ColumnType::Json => Value::Json(format!("{{\"k\":{k}}}")),
    }
}

fn seeded() -> Store {
    let mut store = Store::default();
    for (t, spec) in tables().into_iter().enumerate() {
        store.register(spec);
        let pairs: Vec<(&str, Value)> = spec
            .columns
            .iter()
            .enumerate()
            .map(|(c, col)| (col.name, cell(t, c, col.ty, col.name)))
            .collect();
        store.seed(spec, &pairs).expect("seed");
    }
    store
}

#[test]
fn snapshot_round_trips_byte_for_byte() {
    let tables = tables();
    let store = seeded();
    for t in &tables {
        assert_eq!(store.count(t), 1, "{}", t.name);
    }
    let first = serde_json::to_string(&snapshot(&store, &tables)).expect("json");
    let reloaded =
        from_snapshot(&serde_json::from_str(&first).expect("parse"), &tables).expect("reload");
    let second = serde_json::to_string(&snapshot(&reloaded, &tables)).expect("json");
    assert_eq!(first, second);
    // Not a round trip of nothing: every table, every column, and the kinds.
    let v: serde_json::Value = serde_json::from_str(&first).expect("parse");
    for t in &tables {
        let row = &v["tables"][t.name]["rows"][0];
        assert_eq!(
            row.as_object().map(|o| o.len()),
            Some(t.columns.len()),
            "{}",
            t.name
        );
        assert!(
            v["tables"][t.name]["next_id"].as_i64().is_some(),
            "{}",
            t.name
        );
    }
    let wp = &v["tables"]["work_packages"]["rows"][0];
    assert!(
        wp["start_date"]
            .as_str()
            .is_some_and(|d| d.len() == 10 && d.starts_with("196")),
        "{wp}"
    );
    assert!(wp["estimated_hours"].is_number() || wp["estimated_hours"] == "Infinity");
    let spec = tables
        .iter()
        .find(|t| t.name == "work_packages")
        .expect("work_packages");
    let row = reloaded
        .find(
            spec,
            store.where_eq(spec, &[]).expect("rows")[0]
                .id()
                .expect("id"),
        )
        .expect("find")
        .expect("row");
    let col = spec.index_of("start_date").expect("start_date");
    assert!(
        matches!(row.get(col), Value::Date(_)),
        "start_date reloads as a Date"
    );
}

/// The comparison can fail: a different store gives a different snapshot.
#[test]
fn a_changed_cell_changes_the_snapshot() {
    let tables = tables();
    let a = serde_json::to_string(&snapshot(&seeded(), &tables)).expect("json");
    let mut v: serde_json::Value = serde_json::from_str(&a).expect("parse");
    v["tables"]["work_packages"]["rows"][0]["start_date"] = "2026-11-23".into();
    let b = serde_json::to_string(&snapshot(
        &from_snapshot(&v, &tables).expect("reload"),
        &tables,
    ))
    .expect("json");
    assert_ne!(a, b);
    assert!(b.contains("\"start_date\":\"2026-11-23\""));
}

#[test]
fn an_unknown_column_is_refused() {
    let tables = tables();
    let mut v = snapshot(&seeded(), &tables);
    v["tables"]["users"]["rows"][0]["no_such_column"] = 1.into();
    assert!(from_snapshot(&v, &tables)
        .unwrap_err()
        .0
        .contains("users.no_such_column"));
}
