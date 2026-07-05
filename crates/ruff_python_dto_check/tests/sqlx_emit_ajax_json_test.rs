//! Integration tests for the sqlx-target `ajax_json` handler emitter
//! (Sprint C2 Gap 2).
//!
//! Two cases are covered:
//!   * `test_ajax_json_with_model_sqlx` — a resolved model (`User`) is present
//!     in the contract and the `TargetSpec` maps it to `crate::models::user`.
//!   * `test_ajax_json_stub_sqlx` — `data.models` is empty, so the emitter
//!     produces the stub variant (no `sqlx::query_as`, calibration comment).
//!
//! Each test constructs a [`RouteContract`] programmatically, runs
//! `ruff_python_dto_check::codegen::sqlx_emit::ajax_json::emit_ajax_json_sqlx`,
//! and asserts the produced handler matches the corresponding golden file at
//! `tests/golden/codegen/sqlx/expected/ajax_json_*.rs` (trimmed).
//!
//! Currently failing on purpose: the `sqlx_emit::ajax_json` submodule is not
//! yet wired in `crate::codegen::sqlx_emit::mod` (no `pub mod ajax_json;`
//! declaration). Once the orchestrator wires it the tests run for real — they
//! are NOT `#[ignore]`d. The full
//! `ruff_python_dto_check::codegen::sqlx_emit::ajax_json::emit_ajax_json_sqlx`
//! path is used so the failure is "function not found" rather than a wrong
//! assertion.

use std::collections::BTreeMap;

use ruff_python_dto_check::codegen::target::{ModelMapping, Orm, TargetSpec};
use ruff_python_dto_check::contract::{
    Data, HandlerKind, Inputs, PathParam, Provenance, RouteContract,
};
use ruff_python_dto_check::extractors::body::OutputKind;

// ---------------------------------------------------------------------------
// Shared fixture: TargetSpec with just User → user mapped.
// ---------------------------------------------------------------------------

fn test_spec() -> TargetSpec {
    let mut models = BTreeMap::new();
    models.insert(
        "User".to_string(),
        ModelMapping {
            module_path: "user".to_string(),
        },
    );
    TargetSpec {
        id: "openproject-axum-sqlx".to_string(),
        models_root: "crate::models".to_string(),
        models,
        tenant_column: "ProjectId".to_string(),
        templates_root: None,
        emit_kinds: vec!["ajax_json".to_string()],
        orm: Orm::Sqlx,
    }
}

// ---------------------------------------------------------------------------
// Per-case contract constructors.
// ---------------------------------------------------------------------------

fn ajax_json_with_model_contract() -> RouteContract {
    RouteContract {
        id: "users.get_user_preferences".to_string(),
        function: "get_user_preferences".to_string(),
        family: "users".to_string(),
        methods: vec!["GET".to_string()],
        path: "/users/<int:user_id>/preferences".to_string(),
        inputs: Inputs {
            path_params: vec![PathParam {
                name: "user_id".to_string(),
                converter: Some("int".to_string()),
            }],
            query_reads: vec![],
            form_fields: vec![],
        },
        data: Data {
            models: vec!["User".to_string()],
            order_by: None,
            order_dir: None,
            tenant_scoped: false,
            mutates: false,
            soft_delete: false,
        },
        output: OutputKind::Json {
            shape: vec!["preferences".to_string()],
        },
        guards: vec!["login_required".to_string()],
        handler_kind: HandlerKind::AjaxJson,
        classification_reason: "jsonify, no render".to_string(),
        provenance: Provenance {
            file: "lib/api/v3/users/preferences_api.rb".to_string(),
            line_start: 24,
            line_end: 40,
        },
    }
}

fn ajax_json_stub_contract() -> RouteContract {
    RouteContract {
        id: "notifications.notifications_count".to_string(),
        function: "notifications_count".to_string(),
        family: "notifications".to_string(),
        methods: vec!["GET".to_string()],
        path: "/notifications/count".to_string(),
        inputs: Inputs {
            path_params: vec![],
            query_reads: vec![],
            form_fields: vec![],
        },
        data: Data {
            models: vec![],
            order_by: None,
            order_dir: None,
            tenant_scoped: false,
            mutates: false,
            soft_delete: false,
        },
        output: OutputKind::Json {
            shape: vec!["unread".to_string(), "total".to_string()],
        },
        guards: vec!["login_required".to_string()],
        handler_kind: HandlerKind::AjaxJson,
        classification_reason: "jsonify, no render".to_string(),
        provenance: Provenance {
            file: "lib/api/v3/notifications/count_api.rb".to_string(),
            line_start: 11,
            line_end: 20,
        },
    }
}

// ---------------------------------------------------------------------------
// Golden tests — with-model and stub.
// ---------------------------------------------------------------------------

#[test]
fn test_ajax_json_with_model_sqlx() {
    let contract = ajax_json_with_model_contract();
    let spec = test_spec();
    let emitted = ruff_python_dto_check::codegen::sqlx_emit::ajax_json::emit_ajax_json_sqlx(
        &contract, &spec,
    );
    let expected = include_str!("golden/codegen/sqlx/expected/ajax_json_with_model.rs");
    assert_eq!(emitted.handler_rs.trim(), expected.trim());
}

#[test]
fn test_ajax_json_stub_sqlx() {
    let contract = ajax_json_stub_contract();
    let spec = test_spec();
    let emitted = ruff_python_dto_check::codegen::sqlx_emit::ajax_json::emit_ajax_json_sqlx(
        &contract, &spec,
    );
    let expected = include_str!("golden/codegen/sqlx/expected/ajax_json_stub.rs");
    assert_eq!(emitted.handler_rs.trim(), expected.trim());
}
