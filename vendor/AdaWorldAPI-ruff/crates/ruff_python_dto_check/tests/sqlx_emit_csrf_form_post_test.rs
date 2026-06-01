//! Integration tests for the sqlx-target `csrf_form_post` handler emitter
//! (Sprint C3).
//!
//! Two cases are covered:
//!   * `test_csrf_form_post_with_model_sqlx` — a resolved model (`WorkPackage`)
//!     is present in the contract and the `TargetSpec` maps it to
//!     `crate::models::work_package`, so the emitter produces a real
//!     `sqlx::query_as` INSERT.
//!   * `test_csrf_form_post_stub_sqlx` — `data.models` is empty, so the emitter
//!     produces the stub variant (no model write, calibration comment).
//!
//! Each test constructs a [`RouteContract`] programmatically, runs
//! `ruff_python_dto_check::codegen::sqlx_emit::csrf_form_post::emit_csrf_form_post_sqlx`,
//! and asserts the produced handler matches the corresponding golden file at
//! `tests/golden/codegen/sqlx/expected/csrf_form_post_*.rs` (trimmed).
//!
//! Currently failing on purpose: the `sqlx_emit::csrf_form_post` submodule is
//! not yet wired in `crate::codegen::sqlx_emit::mod` (no
//! `pub mod csrf_form_post;` declaration). Once the orchestrator wires it the
//! tests run for real — they are NOT `#[ignore]`d. The full
//! `ruff_python_dto_check::codegen::sqlx_emit::csrf_form_post::emit_csrf_form_post_sqlx`
//! path is used so the failure is "function not found" rather than a wrong
//! assertion.

use std::collections::BTreeMap;

use ruff_python_dto_check::codegen::target::{ModelMapping, Orm, TargetSpec};
use ruff_python_dto_check::contract::{
    Data, HandlerKind, Inputs, PathParam, Provenance, RouteContract,
};
use ruff_python_dto_check::extractors::body::OutputKind;

// ---------------------------------------------------------------------------
// Shared fixture: TargetSpec with just WorkPackage → work_package mapped.
// ---------------------------------------------------------------------------

fn test_spec() -> TargetSpec {
    let mut models = BTreeMap::new();
    models.insert(
        "WorkPackage".to_string(),
        ModelMapping {
            module_path: "work_package".to_string(),
        },
    );
    TargetSpec {
        id: "openproject-axum-sqlx".to_string(),
        models_root: "crate::models".to_string(),
        models,
        tenant_column: "ProjectId".to_string(),
        templates_root: None,
        emit_kinds: vec!["csrf_form_post_engine_call".to_string()],
        orm: Orm::Sqlx,
    }
}

// ---------------------------------------------------------------------------
// Per-case contract constructors.
// ---------------------------------------------------------------------------

fn csrf_form_post_with_model_contract() -> RouteContract {
    RouteContract {
        id: "work_packages.create_work_package".to_string(),
        function: "create_work_package".to_string(),
        family: "work_packages".to_string(),
        methods: vec!["POST".to_string()],
        path: "/projects/<int:project_id>/work_packages".to_string(),
        inputs: Inputs {
            path_params: vec![PathParam {
                name: "project_id".to_string(),
                converter: Some("int".to_string()),
            }],
            query_reads: vec![],
            form_fields: vec!["subject".to_string(), "description".to_string()],
        },
        data: Data {
            models: vec!["WorkPackage".to_string()],
            order_by: None,
            order_dir: None,
            tenant_scoped: true,
            mutates: true,
            soft_delete: false,
        },
        output: OutputKind::Redirect {
            target: "/work_packages".to_string(),
        },
        guards: vec!["login_required".to_string()],
        handler_kind: HandlerKind::CsrfFormPostEngineCall,
        classification_reason: "POST + form + engine call".to_string(),
        provenance: Provenance {
            file: "lib/api/v3/work_packages/work_packages_api.rb".to_string(),
            line_start: 88,
            line_end: 110,
        },
    }
}

fn csrf_form_post_stub_contract() -> RouteContract {
    RouteContract {
        id: "work_packages.bulk_update_work_packages".to_string(),
        function: "bulk_update_work_packages".to_string(),
        family: "work_packages".to_string(),
        methods: vec!["POST".to_string()],
        path: "/work_packages/bulk".to_string(),
        inputs: Inputs {
            path_params: vec![],
            query_reads: vec![],
            form_fields: vec!["ids".to_string(), "action".to_string()],
        },
        data: Data {
            models: vec![],
            order_by: None,
            order_dir: None,
            tenant_scoped: false,
            mutates: true,
            soft_delete: false,
        },
        output: OutputKind::Redirect {
            target: "/work_packages".to_string(),
        },
        guards: vec!["login_required".to_string()],
        handler_kind: HandlerKind::CsrfFormPostEngineCall,
        classification_reason: "POST + form + engine call".to_string(),
        provenance: Provenance {
            file: "lib/api/v3/work_packages/bulk_api.rb".to_string(),
            line_start: 31,
            line_end: 50,
        },
    }
}

// ---------------------------------------------------------------------------
// Golden tests — with-model and stub.
// ---------------------------------------------------------------------------

#[test]
fn test_csrf_form_post_with_model_sqlx() {
    let contract = csrf_form_post_with_model_contract();
    let spec = test_spec();
    let emitted =
        ruff_python_dto_check::codegen::sqlx_emit::csrf_form_post::emit_csrf_form_post_sqlx(
            &contract, &spec,
        );
    let expected = include_str!("golden/codegen/sqlx/expected/csrf_form_post_with_model.rs");
    assert_eq!(emitted.handler_rs.trim(), expected.trim());
}

#[test]
fn test_csrf_form_post_stub_sqlx() {
    let contract = csrf_form_post_stub_contract();
    let spec = test_spec();
    let emitted =
        ruff_python_dto_check::codegen::sqlx_emit::csrf_form_post::emit_csrf_form_post_sqlx(
            &contract, &spec,
        );
    let expected = include_str!("golden/codegen/sqlx/expected/csrf_form_post_stub.rs");
    assert_eq!(emitted.handler_rs.trim(), expected.trim());
}
