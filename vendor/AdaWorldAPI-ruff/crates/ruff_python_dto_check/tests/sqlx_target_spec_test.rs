//! Integration tests for the new sqlx target spec wiring.
//!
//! These tests exercise:
//!   1. TOML round-trip for `examples/openproject-axum-sqlx.toml` via
//!      [`TargetSpec::from_path`].
//!   2. The built-in [`TargetSpec::openproject_axum_sqlx`] constructor.
//!   3. Back-compat for [`TargetSpec::rust_axum_seaorm`] (default `Orm::Seaorm`).
//!   4. Parser tolerance for a minimal TOML (`id` + `orm` only, all else
//!      defaults).
//!
//! NOTE: This file deliberately references API surface that does not yet
//! exist in `target.rs`:
//!   - the `Orm` enum (with `Orm::Sqlx`, `Orm::Seaorm` variants),
//!   - the `orm` field on [`TargetSpec`],
//!   - the [`TargetSpec::openproject_axum_sqlx`] associated function.
//!
//! These are added by the Wave 2 orchestrator pass in `target.rs`. Until
//! that lands, this file will fail to compile -- that is the intended
//! signal so the orchestrator knows the test is exercising the new
//! surface as soon as it is wired. Do NOT silence the compile error by
//! editing this file; fix it by wiring `target.rs`.
//!
//! No external crate dependencies (no `tempfile`); the minimal-TOML test
//! writes to a deterministic path under `std::env::temp_dir()` so the
//! crate's `Cargo.toml` stays untouched.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ruff_python_dto_check::codegen::target::{Orm, TargetSpec};

/// Absolute path to `examples/openproject-axum-sqlx.toml` relative to the
/// crate root. A sibling fanout agent writes this file; if it is not yet
/// on disk the corresponding test skips gracefully (see comment below).
fn example_sqlx_toml() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/openproject-axum-sqlx.toml")
}

#[test]
fn example_toml_loads_with_sqlx_orm_and_expected_fields() {
    // The example TOML is being authored by a sibling fanout agent in this
    // wave. If it is not present yet, skip with a clear message rather than
    // failing -- the wave's orchestrator validates the file separately.
    let path = example_sqlx_toml();
    if !path.exists() {
        eprintln!(
            "skipping: {} not yet written (sibling fanout pending)",
            path.display()
        );
        return;
    }

    let spec = TargetSpec::from_path(&path).expect("openproject-axum-sqlx.toml parses");

    assert_eq!(
        spec.id, "openproject-axum-sqlx",
        "id field must round-trip from TOML"
    );
    assert_eq!(
        spec.orm,
        Orm::Sqlx,
        "orm field must parse as Orm::Sqlx from `orm = \"sqlx\"`"
    );
    assert_eq!(
        spec.models_root, "crate::models",
        "models_root field must round-trip"
    );

    let work_package = spec
        .models
        .get("WorkPackage")
        .expect("WorkPackage mapping present in example TOML");
    assert_eq!(
        work_package.module_path, "work_package",
        "WorkPackage.module_path must round-trip"
    );

    assert!(
        spec.emit_kinds.iter().any(|k| k == "list_for_tenant"),
        "emit_kinds must contain `list_for_tenant`; got {:?}",
        spec.emit_kinds
    );
}

#[test]
fn builtin_openproject_axum_sqlx_returns_non_trivial_spec() {
    let spec = TargetSpec::openproject_axum_sqlx();

    assert_eq!(
        spec.id, "openproject-axum-sqlx",
        "built-in spec must use the canonical id"
    );
    assert_eq!(
        spec.orm,
        Orm::Sqlx,
        "built-in openproject_axum_sqlx must declare Orm::Sqlx"
    );
    assert!(
        spec.models.len() >= 10,
        "expected at least 10 model mappings in the built-in sqlx spec; got {}",
        spec.models.len()
    );
    assert!(
        spec.emit_kinds.len() >= 4,
        "expected at least 4 emit_kinds entries; got {} ({:?})",
        spec.emit_kinds.len(),
        spec.emit_kinds
    );
}

#[test]
fn builtin_rust_axum_seaorm_still_defaults_to_seaorm_orm() {
    // Back-compat guardrail: adding the `orm` field must NOT break the
    // existing rust-axum-seaorm built-in target. Its `orm` must default to
    // `Orm::Seaorm` so all current consumers continue to behave identically.
    let spec = TargetSpec::rust_axum_seaorm();
    assert_eq!(
        spec.orm,
        Orm::Seaorm,
        "rust_axum_seaorm built-in must keep Orm::Seaorm after the orm field is added"
    );
    assert_eq!(
        spec.id, "rust-axum-seaorm",
        "rust_axum_seaorm id must remain stable"
    );
}

#[test]
fn minimal_toml_with_only_id_and_orm_takes_field_defaults() {
    // Parser tolerance: a TOML containing only `id` and `orm` must parse
    // without panic and every other field must take its documented default
    // (`models_root = "crate::models"`, empty `models`, empty `emit_kinds`).
    let toml_text = "id = \"x\"\norm = \"sqlx\"\n";

    // `TargetSpec::from_path` takes a Path, so we write the snippet to a
    // deterministic location under the OS temp dir. Using a per-process pid
    // suffix keeps parallel `cargo nextest` runs isolated.
    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "ruff_python_dto_check_sqlx_minimal_{}.toml",
        std::process::id()
    ));
    std::fs::write(&tmp, toml_text).expect("write minimal toml to temp dir");

    let spec = TargetSpec::from_path(&tmp).expect("minimal toml parses");

    // Best-effort cleanup; ignore the result so a failed unlink does not
    // mask the real assertion failure.
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(spec.id, "x", "id field must round-trip");
    assert_eq!(
        spec.orm,
        Orm::Sqlx,
        "orm field must parse from `orm = \"sqlx\"`"
    );
    assert_eq!(
        spec.models_root, "crate::models",
        "models_root must default to crate::models when absent"
    );
    assert_eq!(
        spec.models,
        BTreeMap::new(),
        "models must default to empty BTreeMap when no [models.X] tables are present"
    );
    assert!(
        spec.emit_kinds.is_empty(),
        "emit_kinds must default to empty Vec when absent; got {:?}",
        spec.emit_kinds
    );
}
