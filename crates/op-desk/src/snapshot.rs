//! A polyfill [`Store`] written down as plain data, and read back.
//!
//! rails-polyfill has no snapshot of its own: `Store` keeps its rows private and
//! offers `where_eq` (every row of a table with no predicate), `next_id`,
//! `seed` and `reserve_ids`. zammad-desk's `Desk::snapshot` /
//! `Desk::from_scenario` build on exactly those, and they are bound to
//! `zammad-generated`'s model list. This module is the same shape for any list
//! of specs: per table, the rows (every cell, by column name) and the id the
//! next insert receives.
//!
//! ```json
//! {"tables": {"users": {"next_id": 14, "rows": [{"id": 10, "login": "…", …}]}}}
//! ```
//!
//! Cells by column type: integers and booleans as JSON numbers and booleans, a
//! `DateTime` as its microseconds since the epoch (`Value::Time`), a `Date` as `YYYY-MM-DD`, a
//! `Float`/`Decimal` as a JSON number (`NaN`, `Infinity` and `-Infinity` as
//! those strings), text and JSON documents as strings, `nil` as `null`.

use serde_json::{json, Map, Number, Value as J};

use rails_polyfill::value::{date_text, parse_date};
use rails_polyfill::{ColumnType, Store, TableSpec, Value};

/// Why a snapshot could not be read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotError(pub String);

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SnapshotError {}

fn err(msg: impl Into<String>) -> SnapshotError {
    SnapshotError(msg.into())
}

/// The cell of `v` in a column of type `ty`.
#[must_use]
pub fn cell_json(v: &Value, ty: ColumnType) -> J {
    match v {
        Value::Bool(b) => J::Bool(*b),
        #[allow(clippy::cast_precision_loss)]
        Value::Int(i) if matches!(ty, ColumnType::Decimal | ColumnType::Float) => {
            float_json(*i as f64)
        }
        Value::Int(i) | Value::Time(i) => json!(i),
        Value::Date(d) => J::String(date_text(*d)),
        Value::Float(f) => float_json(*f),
        Value::Str(s) | Value::Json(s) => J::String(s.clone()),
        Value::Nil | Value::List(_) | Value::Rec(_) => J::Null,
    }
}

fn float_json(f: f64) -> J {
    Number::from_f64(f).map_or_else(
        || {
            J::String(if f.is_nan() {
                "NaN".into()
            } else if f > 0.0 {
                "Infinity".into()
            } else {
                "-Infinity".into()
            })
        },
        J::Number,
    )
}

/// The value of a snapshot cell in a column of type `ty`.
pub fn cell_value(cell: &J, ty: ColumnType) -> Result<Value, SnapshotError> {
    Ok(match (ty, cell) {
        (_, J::Null) => Value::Nil,
        (ColumnType::Boolean, J::Bool(b)) => Value::Bool(*b),
        (ColumnType::Integer, J::Number(n)) => Value::Int(
            n.as_i64()
                .ok_or_else(|| err(format!("{n} is not an integer")))?,
        ),
        (ColumnType::DateTime, J::Number(n)) => Value::Time(
            n.as_i64()
                .ok_or_else(|| err(format!("{n} is not a whole number of microseconds")))?,
        ),
        (ColumnType::Date, J::String(s)) => {
            Value::Date(parse_date(s).ok_or_else(|| err(format!("{s:?} is not a date")))?)
        }
        (ColumnType::Float | ColumnType::Decimal, J::Number(n)) => Value::Float(
            n.as_f64()
                .ok_or_else(|| err(format!("{n} is not a number")))?,
        ),
        (ColumnType::Float | ColumnType::Decimal, J::String(s)) => Value::Float(match s.as_str() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            other => return Err(err(format!("{other:?} is not a number"))),
        }),
        (ColumnType::Json, J::String(s)) => Value::Json(s.clone()),
        (ColumnType::String | ColumnType::Text, J::String(s)) => Value::Str(s.clone()),
        (ty, other) => return Err(err(format!("{other} does not fit a {ty:?} column"))),
    })
}

/// Every row of every table in `tables`, in primary-key order.
#[must_use]
pub fn snapshot(store: &Store, tables: &[&'static TableSpec]) -> J {
    let mut out = Map::new();
    for spec in tables {
        let rows: Vec<J> = store
            .where_eq(spec, &[])
            .unwrap_or_default()
            .iter()
            .map(|rec| {
                J::Object(
                    spec.columns
                        .iter()
                        .zip(rec.values())
                        .map(|(c, v)| (c.name.to_owned(), cell_json(v, c.ty)))
                        .collect(),
                )
            })
            .collect();
        let mut t = Map::new();
        t.insert(
            "next_id".into(),
            store.next_id(spec).map_or(J::Null, |n| json!(n)),
        );
        t.insert("rows".into(), J::Array(rows));
        out.insert(spec.name.to_owned(), J::Object(t));
    }
    json!({ "tables": out })
}

/// A store holding exactly what `snap` records, for the given specs. Every
/// table in `tables` must be present; an unknown table or column is refused.
pub fn from_snapshot(snap: &J, tables: &[&'static TableSpec]) -> Result<Store, SnapshotError> {
    let recorded = snap
        .get("tables")
        .and_then(J::as_object)
        .ok_or_else(|| err("snapshot has no tables object"))?;
    if let Some(unknown) = recorded
        .keys()
        .find(|k| !tables.iter().any(|t| t.name == k.as_str()))
    {
        return Err(err(format!("snapshot table {unknown} has no spec")));
    }
    let mut store = Store::default();
    for spec in tables {
        store.register(spec);
        let entry = recorded
            .get(spec.name)
            .ok_or_else(|| err(format!("snapshot has no table {}", spec.name)))?;
        let rows = entry
            .get("rows")
            .and_then(J::as_array)
            .ok_or_else(|| err(format!("{}.rows is not a list", spec.name)))?;
        for row in rows {
            let obj = row
                .as_object()
                .ok_or_else(|| err(format!("a row of {} is not an object", spec.name)))?;
            let mut pairs = Vec::with_capacity(obj.len());
            for (k, cell) in obj {
                let i = spec
                    .index_of(k)
                    .ok_or_else(|| err(format!("{}.{k} has no column", spec.name)))?;
                pairs.push((
                    k.as_str(),
                    cell_value(cell, spec.columns[i].ty)
                        .map_err(|e| err(format!("{}.{k}: {e}", spec.name)))?,
                ));
            }
            store
                .seed(spec, &pairs)
                .map_err(|e| err(format!("{}: {e:?}", spec.name)))?;
        }
        if let Some(next) = entry.get("next_id").and_then(J::as_i64) {
            store.reserve_ids(spec, next);
        }
    }
    Ok(store)
}
