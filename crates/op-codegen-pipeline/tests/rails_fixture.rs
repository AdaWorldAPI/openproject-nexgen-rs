//! Integration test: drive a real filesystem Rails fixture through the
//! full pipeline (`ruff_openproject::extract_triples` → `bridge_triples`
//! → `OpSurrealProjection::project` → `surreal_text`) and assert the
//! emitted SurrealQL DDL.
//!
//! The fixture lives at `tests/fixtures/rails_mini/` — minimal,
//! hermetic, version-controlled. It is NOT a copy of the
//! `vendor/AdaWorldAPI-ruff/.../tests/fixtures/openproject` fixture:
//! upstream may evolve that one; this one is owned by this crate so its
//! contents pin exactly the cases we want to demonstrate end-to-end:
//!
//!   - 2 core models (WorkPackage, TimeEntry) — both in
//!     `CORE_V3_RESOURCES`, exercising the core-filter path
//!   - 1 non-core model (AdhocThing) — verifies `extract_core_triples`
//!     drops it
//!   - declarative `validates :col, presence: true` on both core models
//!     — exercises C10's required-field detection ALL THE WAY from disk
//!   - `db/schema.rb` columns including helpers (`t.timestamps` isn't
//!     used, but `force: :cascade` is — both correctly handled by ruff)

use std::path::PathBuf;

use lance_graph_contract::codegen_spine::roundtrip_eq;
use op_codegen_pipeline::{
    CORE_V3_RESOURCES, bridge_triples, extract_core_triples, extract_graph, extract_triples,
    filter_to_core, render_surreal_from_ruff,
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
    let graph = extract_graph(&fixture_root());
    let wp = graph.models.iter().find(|m| m.name == "WorkPackage").unwrap();
    let wp_cols: Vec<&str> = wp.fields.iter().map(|f| f.name.as_str()).collect();
    // Columns come from db/schema.rb in declaration order.
    assert_eq!(wp_cols, ["subject", "status_id"]);

    let te = graph.models.iter().find(|m| m.name == "TimeEntry").unwrap();
    let te_cols: Vec<&str> = te.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(te_cols, ["hours", "work_package_id"]);
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
    // The end-to-end demo: filesystem walk → triples → bridge → project →
    // SurrealQL text. Asserts each load-bearing line is present rather
    // than a single big string match — keeps the test robust to a future
    // emitter widening (e.g. added PERMISSIONS clauses) that would
    // otherwise force every test to update in lockstep.
    let triples = extract_core_triples(&fixture_root());
    let (c, text) = render_surreal_from_ruff(&triples);

    // Both core tables present, alphabetical.
    let table_names: Vec<&str> = c.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(table_names, ["TimeEntry", "WorkPackage"]);

    // C10 required-field detection from filesystem `validates` declarations.
    let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
    let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
    let status_id = wp.fields.iter().find(|f| f.name == "status_id").unwrap();
    assert!(subject.required, "validates :subject -> required");
    assert!(!status_id.required, "no validates on status_id -> optional");

    let te = c.tables.iter().find(|t| t.name == "TimeEntry").unwrap();
    let hours = te.fields.iter().find(|f| f.name == "hours").unwrap();
    let wp_id = te
        .fields
        .iter()
        .find(|f| f.name == "work_package_id")
        .unwrap();
    assert!(hours.required, "validates :hours -> required");
    assert!(!wp_id.required, "no validates on work_package_id -> optional");

    // Emission asserts. Combines:
    //   - C10 required (validates :col -> TYPE any) — hours / subject
    //   - C12 kind inference (*_id -> int) — status_id (target table
    //     `Status` is NOT a known model in the fixture, so it stays Int)
    //   - C13 FK record link inference — work_package_id resolves
    //     `work_package` -> WorkPackage which IS a known model, so the
    //     field is promoted from option<int> to option<record<WorkPackage>>
    assert!(text.contains("DEFINE TABLE TimeEntry SCHEMAFULL;"));
    assert!(text.contains("DEFINE TABLE WorkPackage SCHEMAFULL;"));
    assert!(text.contains("DEFINE FIELD hours ON TABLE TimeEntry TYPE any;"));
    assert!(text.contains("DEFINE FIELD subject ON TABLE WorkPackage TYPE any;"));
    assert!(text.contains("DEFINE FIELD status_id ON TABLE WorkPackage TYPE option<int>;"));
    assert!(text.contains(
        "DEFINE FIELD work_package_id ON TABLE TimeEntry TYPE option<record<WorkPackage>>;"
    ));

    // Non-core model must not appear anywhere.
    assert!(!text.contains("AdhocThing"));
    assert!(!text.contains("adhoc_things"));
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
