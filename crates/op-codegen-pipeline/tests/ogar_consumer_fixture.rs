//! Integration test for the `ogar-emit` consumer path (§5 steps 4-5):
//! drive the same `tests/fixtures/rails_mini/` fixture used by
//! `rails_fixture.rs` through `ogar_consumer::render_surreal_via_ogar`
//! instead of the native `op-surreal-ast` path, and assert on the emitted
//! SurrealQL DDL's shape.
//!
//! Line-presence assertions only, deliberately — the OGAR emit shape
//! (`ogar_adapter_surrealql::emit_surrealql_ddl`) is a different, simpler
//! formatter than `op-surreal-ast`'s (no `ASSERT $value != NONE`, no
//! `TABLE` keyword in the `ON` clause, no conservation trailer), so this
//! is never byte-golden against the native path's snapshot.

#![cfg(feature = "ogar-emit")]

use std::path::PathBuf;

use op_codegen_pipeline::ogar_consumer::render_surreal_via_ogar;

/// Absolute path to the hermetic fixture shared with `rails_fixture.rs`.
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rails_mini")
}

#[test]
fn ogar_path_emits_define_table_for_core_models_and_drops_adhoc() {
    let ddl = render_surreal_via_ogar(&fixture_root());

    assert!(
        ddl.contains("DEFINE TABLE WorkPackage SCHEMAFULL;"),
        "expected WorkPackage table, got: {ddl}"
    );
    assert!(
        ddl.contains("DEFINE TABLE TimeEntry SCHEMAFULL;"),
        "expected TimeEntry table, got: {ddl}"
    );
    // AdhocThing is not in CORE_V3_RESOURCES — filter_to_core must drop it
    // before it ever reaches the OGAR lift/mint/emit path.
    assert!(
        !ddl.contains("AdhocThing"),
        "non-core model must not appear in ogar-emit output, got: {ddl}"
    );
}

#[test]
fn ogar_path_emits_typed_field_for_schema_stratum_column() {
    // `subject` is `t.string :subject, default: "", null: false` in
    // `db/migrate/tables/work_packages.rb` — `field_type = "string"`,
    // `not_null = Some(true)`. `project_rails_fields` wires
    // `not_null.unwrap_or(false)` onto `AttributeOptions::required`, so
    // `required = Some(true)` here — the `ogar_adapter_surrealql` emitter
    // only wraps `option<…>` when `required == Some(false)`, so this
    // column emits as a bare (non-optional) `string`, unlike the native
    // `op-surreal-ast` path's `ASSERT $value != NONE` convention.
    let ddl = render_surreal_via_ogar(&fixture_root());
    assert!(
        ddl.contains("DEFINE FIELD subject ON WorkPackage TYPE string;"),
        "expected typed required `subject` field, got: {ddl}"
    );

    // `done_ratio` is `t.integer :done_ratio, default: nil, null: true` —
    // `field_type = "integer"` (-> `int` via `map_type_to_surrealql`),
    // `not_null = None` -> `required = Some(false)` -> `option<int>`.
    assert!(
        ddl.contains("DEFINE FIELD done_ratio ON WorkPackage TYPE option<int>;"),
        "expected typed optional `done_ratio` field, got: {ddl}"
    );
}
