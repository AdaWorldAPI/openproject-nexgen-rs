//! The column-type mapping the generated specs use.
//!
//! `rails-compiler`'s emitter (`emit.rs`, `col_type`) maps a Rails column type
//! to a polyfill [`ColumnType`]. Two of its arms are the mapping zammad-rs
//! chose before the polyfill had the types: `date` → `DateTime` and `float` →
//! `Decimal`. rails-polyfill now has [`ColumnType::Date`] and
//! [`ColumnType::Float`] (zammad-rs #102), and OpenProject keeps real `date`
//! and `double precision` columns (`work_packages.start_date`,
//! `estimated_hours`, …), so the OpenProject specs use those two types.
//!
//! The emitter has no per-application switch for this, and zammad-rs keeps its
//! mapping. So the generator runs the emitter unchanged and then rewrites the
//! type of every `date` and `float` column with [`polyfill_type`]. Every other
//! arm is the emitter's own.

use rails_polyfill::ColumnType;

/// The polyfill type of a Rails column type: `rails-compiler`'s mapping, with
/// `date` → [`ColumnType::Date`] and `float` → [`ColumnType::Float`].
///
/// A serialized attribute (`store :x`) is `Json` whatever its column type, as
/// in the emitter.
#[must_use]
pub fn polyfill_type(rails_ty: &str, store: bool) -> ColumnType {
    if store {
        return ColumnType::Json;
    }
    match rails_ty {
        "string" => ColumnType::String,
        "text" | "binary" => ColumnType::Text,
        "integer" | "bigint" => ColumnType::Integer,
        "boolean" => ColumnType::Boolean,
        "datetime" | "timestamp" | "time" => ColumnType::DateTime,
        // The disable run maps `date` back to the emitter's `DateTime`; the
        // schema-fidelity test must then fail.
        "date" if cfg!(feature = "disable-date-column") => ColumnType::DateTime,
        "date" => ColumnType::Date,
        "decimal" => ColumnType::Decimal,
        "float" => ColumnType::Float,
        "json" | "jsonb" => ColumnType::Json,
        _ => ColumnType::Text,
    }
}

/// The spelling of a [`ColumnType`] in generated code (`rp::ColumnType::<it>`).
#[must_use]
pub fn variant(ty: ColumnType) -> &'static str {
    match ty {
        ColumnType::Integer => "Integer",
        ColumnType::String => "String",
        ColumnType::Text => "Text",
        ColumnType::Boolean => "Boolean",
        ColumnType::DateTime => "DateTime",
        ColumnType::Decimal => "Decimal",
        ColumnType::Json => "Json",
        ColumnType::Date => "Date",
        ColumnType::Float => "Float",
    }
}
