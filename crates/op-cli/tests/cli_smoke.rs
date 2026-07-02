//! Smoke tests for the `op-cli` library entry points. The binary
//! (`op-codegen`) is a one-liner shell around [`dispatch_codegen`]; all
//! testable logic lives in the library and is exercised directly here.
//!
//! The Rails-shaped fixture used by the integration test lives in the
//! `op-codegen-pipeline` crate at `tests/fixtures/rails_mini`. The
//! `CARGO_MANIFEST_DIR` env var (set by cargo to this crate's root at
//! compile time) lets us reference it via a workspace-relative path
//! without duplicating the fixture.

use std::path::PathBuf;

use op_cli::{dispatch_codegen, run_codegen, CliError, USAGE};

/// Absolute path to the shared `rails_mini` fixture (lives in
/// `op-codegen-pipeline/tests/fixtures/rails_mini`). Re-using it
/// instead of duplicating keeps the two crates' e2e expectations
/// pinned to the same source of truth.
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../op-codegen-pipeline/tests/fixtures/rails_mini")
}

#[test]
fn run_codegen_on_rails_mini_emits_expected_define_statements() {
    let text = run_codegen(&fixture_root()).expect("rails_mini fixture exists");

    // Sample a representative cross-section of the D-AR-3.5 typed
    // emission ("compiled, not parsed" path via
    // op_surreal_ast::from_triples):
    // - DEFINE TABLE, with has_many surfaced as a COMMENT annotation
    // - typed fields from the schema stratum (string/float/int/datetime)
    // - column_not_null → bare kind; nullable → option<…>
    // - belongs_to + null: false → bare record<Target> + companion index
    // - validates presence → composed ASSERT
    // - the conservation trailer (nothing drops silently)
    assert!(text.contains("DEFINE TABLE TimeEntry SCHEMAFULL;"));
    assert!(text.contains(
        "DEFINE TABLE WorkPackage SCHEMAFULL COMMENT 'has_many:time_entries→TimeEntry';"
    ));
    assert!(text.contains(
        "DEFINE FIELD hours ON TABLE TimeEntry TYPE option<float> ASSERT $value != NONE;"
    ));
    assert!(text
        .contains("DEFINE FIELD subject ON TABLE WorkPackage TYPE string ASSERT $value != NONE;"));
    assert!(text.contains("DEFINE FIELD done_ratio ON TABLE WorkPackage TYPE option<int>;"));
    assert!(text.contains("DEFINE FIELD id ON TABLE WorkPackage TYPE int;"));
    assert!(
        text.contains("DEFINE FIELD work_package_id ON TABLE TimeEntry TYPE record<WorkPackage>;")
    );
    assert!(text.contains(
        "DEFINE INDEX idx_TimeEntry_work_package_id ON TABLE TimeEntry FIELDS work_package_id;"
    ));
    assert!(text.contains(
        "-- columns-from: baseline-only | tables seen: 2 matched: 2 unmatched: 0 skipped: 0"
    ));
}

#[test]
fn run_codegen_rejects_non_existent_path() {
    let bogus = PathBuf::from("/nonexistent/rails/app/that/does/not/exist");
    let err = run_codegen(&bogus).expect_err("non-existent path must error");
    assert_eq!(err, CliError::PathNotFound(bogus.display().to_string()));
}

#[test]
fn run_codegen_rejects_file_path_pointing_at_a_non_directory() {
    // The library file itself exists but is not a directory — same
    // PathNotFound shape (the predicate is `is_dir`, not `exists`).
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let err = run_codegen(&here).expect_err("file-not-dir path must error");
    assert_eq!(err, CliError::PathNotFound(here.display().to_string()));
}

#[test]
fn dispatch_codegen_with_no_args_returns_usage() {
    let args: Vec<String> = Vec::new();
    let err = dispatch_codegen(&args).expect_err("no args must be a usage error");
    assert_eq!(err, CliError::Usage(USAGE.to_string()));
}

#[test]
fn dispatch_codegen_with_help_flag_returns_usage() {
    for flag in ["-h", "--help"] {
        let args = vec![flag.to_string()];
        let err = dispatch_codegen(&args).expect_err("help flag must be a usage error");
        assert_eq!(err, CliError::Usage(USAGE.to_string()));
    }
}

#[test]
fn dispatch_codegen_with_too_many_args_returns_usage() {
    let args = vec![
        fixture_root().display().to_string(),
        "extra".to_string(),
        "more".to_string(),
    ];
    let err = dispatch_codegen(&args).expect_err("too many args must be a usage error");
    match err {
        CliError::Usage(m) => {
            assert!(m.starts_with(USAGE));
            assert!(m.contains("too many arguments (3 given)"));
        }
        other => panic!("expected Usage error, got {other:?}"),
    }
}

#[test]
fn dispatch_codegen_with_valid_path_returns_ddl_text() {
    let args = vec![fixture_root().display().to_string()];
    let text = dispatch_codegen(&args).expect("dispatcher should pass through to run_codegen");
    // One representative assertion — full set is in
    // `run_codegen_on_rails_mini_emits_expected_define_statements`.
    // (WorkPackage's DEFINE TABLE carries a has_many COMMENT now, so
    // match the prefix, not the bare-terminated form.)
    assert!(text.contains("DEFINE TABLE WorkPackage SCHEMAFULL"));
}
