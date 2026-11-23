//! The generated specs match OpenProject's schema (`db/structure.sql`) for
//! every journey table: column names and order, polyfill types (with `date` →
//! `Date` and `double precision` → `Float`), nullability, defaults and limits.
//! The expected side is computed from the parsed schema, never from a list
//! written here.

mod common;

use op_desk::generated::MODELS;
use op_desk::structure::{rails_default, rails_type, RailsDefault};
use op_desk::types::polyfill_type;
use rails_polyfill::{ColumnType, Default};

fn expected_default(d: Option<&str>) -> Default {
    match rails_default(d) {
        // rails-compiler's emitter writes a computed default as no default.
        RailsDefault::None | RailsDefault::Expr(_) => Default::None,
        RailsDefault::Int(i) => Default::Int(i),
        RailsDefault::Str(s) => Default::Str(Box::leak(s.into_boxed_str())),
        RailsDefault::Bool(b) => Default::Bool(b),
    }
}

#[test]
fn generated_specs_match_openproject_structure_sql() {
    let schema = common::structure();
    let mut checked = 0;
    for m in MODELS {
        let spec = m.table;
        let t = schema
            .get(spec.name)
            .unwrap_or_else(|| panic!("{} is not in db/structure.sql", spec.name));
        let got: Vec<&str> = spec.columns.iter().map(|c| c.name).collect();
        let want: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(got, want, "{}: column names and order", spec.name);
        for (c, s) in spec.columns.iter().zip(&t.columns) {
            let (rails, limit) = rails_type(&s.sql_type);
            assert_eq!(
                c.ty,
                polyfill_type(&rails, false),
                "{}.{} ({})",
                spec.name,
                c.name,
                s.sql_type
            );
            assert_eq!(c.null, !s.not_null, "{}.{} nullability", spec.name, c.name);
            assert_eq!(
                c.default,
                expected_default(s.default.as_deref()),
                "{}.{} default {:?}",
                spec.name,
                c.name,
                s.default
            );
            assert_eq!(c.limit, limit, "{}.{} limit", spec.name, c.name);
            checked += 1;
        }
    }
    assert!(checked > 150, "only {checked} columns checked");
}

/// Independent of the mapping function: every `date` column of the schema is
/// a polyfill `Date`, every `double precision` column a `Float`.
#[test]
fn date_and_double_columns_keep_their_types() {
    let schema = common::structure();
    let (mut dates, mut floats) = (0, 0);
    for m in MODELS {
        for s in &schema[m.table.name].columns {
            let c = &m.table.columns[m.table.index_of(&s.name).expect("column")];
            match s.sql_type.as_str() {
                "date" => {
                    assert_eq!(c.ty, ColumnType::Date, "{}.{}", m.table.name, c.name);
                    dates += 1;
                }
                "double precision" => {
                    assert_eq!(c.ty, ColumnType::Float, "{}.{}", m.table.name, c.name);
                    floats += 1;
                }
                _ => {}
            }
        }
    }
    // work_packages (2 + 4), work_package_journals (2 + 4), non_working_days (1 date).
    assert_eq!((dates, floats), (5, 8));
}

/// Every journey table OpenProject has is generated, and the one it lacks
/// (`projects_types`) is the only one not generated.
#[test]
fn every_journey_table_in_openproject_is_generated() {
    let schema = common::structure();
    for t in op_desk::JOURNEY_TABLES {
        let generated = MODELS.iter().any(|m| m.table.name == *t);
        assert_eq!(generated, schema.contains_key(*t), "{t}");
    }
    assert!(!schema.contains_key("projects_types"));
}

/// Without the source: the generated slice covers the journey tables, once each.
#[test]
fn generated_slice_covers_the_journey_tables_once() {
    let mut names: Vec<&str> = MODELS.iter().map(|m| m.table.name).collect();
    names.sort_unstable();
    let want: Vec<&str> = op_desk::JOURNEY_TABLES
        .iter()
        .copied()
        .filter(|t| *t != "projects_types")
        .collect();
    assert_eq!(names, want);
    assert!(
        MODELS.iter().all(|m| m.callbacks.is_empty()),
        "no callback is compiled yet"
    );
}
