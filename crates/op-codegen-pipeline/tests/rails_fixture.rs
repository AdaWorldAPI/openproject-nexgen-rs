//! Integration test: drive a real filesystem Rails fixture through the
//! full pipeline (`ruff_openproject::extract_triples` → `bridge_triples`
//! → `OpSurrealProjection::project` → `surreal_text`) and assert the
//! emitted SurrealQL DDL.
//!
//! The fixture lives at `tests/fixtures/rails_mini/` — minimal,
//! hermetic, version-controlled. It is NOT a copy of the
//! `ruff_openproject/tests/fixtures/openproject` fixture (itself a
//! crate-local copy of the upstream ruff fixture, since 2026-07-05's
//! un-vendor): upstream may evolve that one; this one is owned by this
//! crate so its contents pin exactly the cases we want to demonstrate
//! end-to-end:
//!
//!   - 2 core models (WorkPackage, TimeEntry) — both in
//!     `CORE_V3_RESOURCES`, exercising the core-filter path
//!   - 1 non-core model (AdhocThing) — verifies `extract_core_triples`
//!     drops it
//!   - declarative `validates :col, presence: true` on both core models
//!     — exercises C10's required-field detection ALL THE WAY from disk
//!   - `db/migrate/tables/*.rb` baseline DSL columns (the D-AR-3.5
//!     schema stratum) — typed fields with nullability, merged via
//!     `extract_app_with_schema`

use std::path::PathBuf;

use lance_graph_contract::codegen_spine::roundtrip_eq;
use op_codegen_pipeline::{
    bridge_triples, extract_core_triples, extract_core_triples_with_schema, extract_graph,
    extract_triples, filter_to_core, render_typed_surreal, CORE_V3_RESOURCES,
};
use op_codegen_projection::OpSurrealProjection;

/// Absolute path to the hermetic fixture. `CARGO_MANIFEST_DIR` is set to
/// this crate's root at compile time, so the path is stable across machines.
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rails_mini")
}

#[test]
fn fixture_extracts_all_three_models_from_filesystem() {
    let graph = extract_graph(&fixture_root());
    let names: Vec<&str> = graph.models.iter().map(|m| m.name.as_str()).collect();
    // ruff_ruby_spo sorts by class name; deterministic.
    assert_eq!(names, ["AdhocThing", "TimeEntry", "WorkPackage"]);
}

#[test]
fn fixture_extracts_schema_columns_per_model() {
    // Columns come from the db/migrate/tables/*.rb baseline DSL (the
    // D-AR-3.5 schema stratum), in declaration order, implicit `id`
    // first. `extract_graph` alone carries NO columns — upstream main
    // stubbed the class-body field scanner; the stratum arrives only
    // via `extract_app_with_schema` (two-shapes doctrine: the AR shape
    // is extracted from class bodies, the physical columns are a
    // separate, subordinate stratum).
    let (triples, report) = extract_core_triples_with_schema(&fixture_root());
    assert_eq!(report.tables_seen, 2);
    assert_eq!(report.tables_matched, 2);
    assert!(report.unmatched_tables.is_empty());

    let wp_cols: Vec<String> = triples
        .iter()
        .filter(|t| t.p == "has_field" && t.s == "openproject:WorkPackage")
        .map(|t| {
            t.o.trim_start_matches("openproject:WorkPackage.")
                .to_string()
        })
        .collect();
    for expected in [
        "id",
        "subject",
        "description",
        "done_ratio",
        "project_id",
        "created_at",
        "updated_at",
    ] {
        assert!(
            wp_cols.iter().any(|c| c == expected),
            "missing {expected} in {wp_cols:?}"
        );
    }

    // Typed + nullability facts ride field_type / column_not_null.
    assert!(triples
        .iter()
        .any(|t| t.s == "openproject:WorkPackage.subject"
            && t.p == "field_type"
            && t.o == "string"));
    assert!(triples
        .iter()
        .any(|t| t.s == "openproject:WorkPackage.subject"
            && t.p == "column_not_null"
            && t.o == "true"));
    assert!(
        !triples
            .iter()
            .any(|t| t.s == "openproject:WorkPackage.done_ratio" && t.p == "column_not_null"),
        "done_ratio is nullable — unset is not 0%"
    );
}

#[test]
fn core_filter_drops_non_core_adhoc_model() {
    // AdhocThing is not in CORE_V3_RESOURCES; extract_core_triples must
    // not emit anything about it.
    assert!(!CORE_V3_RESOURCES.iter().any(|r| *r == "AdhocThing"));

    let triples = extract_core_triples(&fixture_root());
    assert!(!triples.iter().any(|t| t.s.contains("AdhocThing")));
    assert!(!triples.iter().any(|t| t.o.contains("AdhocThing")));
}

#[test]
fn full_pipeline_emits_expected_surql_from_filesystem() {
    // The end-to-end demo on the TYPED path (D-AR-3.5, "compiled, not
    // parsed" direction): filesystem walk + schema stratum → triples →
    // op_surreal_ast::from_triples → typed SurrealQL + conservation
    // trailer. Line-presence asserts, same robustness rationale as
    // before.
    let text = render_typed_surreal(&fixture_root());

    assert!(text.contains("DEFINE TABLE TimeEntry SCHEMAFULL;"));
    assert!(text.contains(
        "DEFINE TABLE WorkPackage SCHEMAFULL COMMENT 'has_many:time_entries\u{2192}TimeEntry';"
    ));
    // Schema-stratum typed fields; column_not_null → bare kind.
    assert!(text
        .contains("DEFINE FIELD subject ON TABLE WorkPackage TYPE string ASSERT $value != NONE;"));
    assert!(text.contains("DEFINE FIELD done_ratio ON TABLE WorkPackage TYPE option<int>;"));
    assert!(text.contains(
        "DEFINE FIELD hours ON TABLE TimeEntry TYPE option<float> ASSERT $value != NONE;"
    ));
    // belongs_to + null: false → bare record link + companion index.
    assert!(
        text.contains("DEFINE FIELD work_package_id ON TABLE TimeEntry TYPE record<WorkPackage>;")
    );
    assert!(text.contains(
        "DEFINE INDEX idx_TimeEntry_work_package_id ON TABLE TimeEntry FIELDS work_package_id;"
    ));
    // belongs_to :project — Project isn't a model in the fixture, so the
    // polymorphic/unknown-target guard degrades to `any` (bare: null: false).
    assert!(text.contains("DEFINE FIELD project_id ON TABLE WorkPackage TYPE any;"));
    // Non-FK fields must not get indexes.
    assert!(!text.contains("idx_WorkPackage_subject"));
    assert!(!text.contains("idx_TimeEntry_hours"));
    // Conservation trailer: the artifact accounts for itself.
    assert!(text.contains(
        "-- columns-from: baseline-only | tables seen: 2 matched: 2 unmatched: 0 skipped: 0"
    ));
    // Non-core model must not appear anywhere.
    assert!(!text.contains("AdhocThing"));
    assert!(!text.contains("adhoc_things"));
}

#[test]
fn old_projection_path_still_roundtrips() {
    // The legacy OpSurrealProjection path (transitional per the
    // two-shapes doctrine — migrates OGAR-side) no longer receives
    // field-bearing triples from main's extraction, but its round-trip
    // contract must keep holding for whatever it is fed.
    let triples = extract_core_triples(&fixture_root());
    let (c, text) = op_codegen_pipeline::render_surreal_from_ruff(&triples);
    let table_names: Vec<&str> = c.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(table_names, ["TimeEntry", "WorkPackage"]);
    assert!(text.contains("DEFINE TABLE WorkPackage SCHEMAFULL;"));
}

#[test]
fn pipeline_roundtrip_eq_on_real_filesystem_extraction() {
    // The C6 contract gate on the REAL extraction output (filesystem walk,
    // not a hand-rolled ModelGraph). This pins that all the triples ruff
    // produces from disk — including ones we don't structurally project
    // (raises, has_function, traverses_relation) — round-trip through
    // OpSurrealProjection at strict default tolerance.
    let triples = extract_triples(&fixture_root());
    let spine = bridge_triples(&triples);
    roundtrip_eq::<OpSurrealProjection>(&spine)
        .expect("filesystem-extracted triples must round-trip losslessly");
}

#[test]
fn filter_to_core_on_graph_keeps_core_drops_adhoc() {
    // Belt-and-braces: the lower-level in-memory filter (used by
    // extract_core_triples internally) also behaves as advertised
    // against the fixture.
    let mut graph = extract_graph(&fixture_root());
    assert_eq!(graph.models.len(), 3);
    filter_to_core(&mut graph);
    let kept: Vec<&str> = graph.models.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(kept, ["TimeEntry", "WorkPackage"]);
}
