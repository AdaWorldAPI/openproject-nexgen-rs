//! Integration tests for the four sqlx-target handler emitters.
//!
//! Each test constructs a [`RouteContract`] programmatically, runs the matching
//! `emit_*_sqlx` function against a small [`TargetSpec`] that maps only
//! `WorkPackage` → `work_package`, and asserts the produced handler matches the
//! golden file at `tests/golden/codegen/sqlx/expected/<kind>.rs` (trimmed).
//!
//! Currently failing on purpose: the `sqlx_emit` module is not yet wired in
//! `crate::codegen::mod` (no `pub mod sqlx_emit;` declaration). Once the
//! orchestrator wires it the tests run for real — they are NOT `#[ignore]`d.
//! The helpers below use the full `ruff_python_dto_check::codegen::sqlx_emit::*`
//! paths so the failure is "function not found" rather than a wrong assertion.

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
        emit_kinds: vec![
            "list_for_tenant".to_string(),
            "detail_for_tenant".to_string(),
            "soft_delete".to_string(),
            "toggle_bool_field".to_string(),
        ],
        orm: Orm::Sqlx,
    }
}

// ---------------------------------------------------------------------------
// Per-kind contract constructors.
// ---------------------------------------------------------------------------

fn list_for_tenant_contract() -> RouteContract {
    RouteContract {
        id: "projects.list_work_packages".to_string(),
        function: "list_work_packages".to_string(),
        family: "projects".to_string(),
        methods: vec!["GET".to_string()],
        path: "/projects/<int:project_id>/work_packages".to_string(),
        inputs: Inputs {
            path_params: vec![PathParam {
                name: "project_id".to_string(),
                converter: Some("int".to_string()),
            }],
            query_reads: vec![],
            form_fields: vec![],
        },
        data: Data {
            models: vec!["WorkPackage".to_string()],
            order_by: Some("id".to_string()),
            order_dir: Some("desc".to_string()),
            tenant_scoped: true,
            mutates: false,
            soft_delete: false,
        },
        output: OutputKind::Json { shape: Vec::new() },
        guards: vec!["login_required".to_string()],
        handler_kind: HandlerKind::ListForTenant,
        classification_reason: "tenant-scoped list query".to_string(),
        provenance: Provenance {
            file: "lib/api/v3/projects/projects_api.rb".to_string(),
            line_start: 42,
            line_end: 60,
        },
    }
}

fn detail_for_tenant_contract() -> RouteContract {
    RouteContract {
        id: "projects.get_work_package".to_string(),
        function: "get_work_package".to_string(),
        family: "projects".to_string(),
        methods: vec!["GET".to_string()],
        path: "/projects/<int:project_id>/work_packages/<int:id>".to_string(),
        inputs: Inputs {
            path_params: vec![
                PathParam {
                    name: "project_id".to_string(),
                    converter: Some("int".to_string()),
                },
                PathParam {
                    name: "id".to_string(),
                    converter: Some("int".to_string()),
                },
            ],
            query_reads: vec![],
            form_fields: vec![],
        },
        data: Data {
            models: vec!["WorkPackage".to_string()],
            order_by: None,
            order_dir: None,
            tenant_scoped: true,
            mutates: false,
            soft_delete: false,
        },
        output: OutputKind::Json { shape: Vec::new() },
        guards: vec!["login_required".to_string()],
        handler_kind: HandlerKind::DetailForTenant,
        classification_reason: "scoped find_by_id".to_string(),
        provenance: Provenance {
            file: "lib/api/v3/projects/projects_api.rb".to_string(),
            line_start: 84,
            line_end: 100,
        },
    }
}

fn soft_delete_contract() -> RouteContract {
    RouteContract {
        id: "projects.archive_work_package".to_string(),
        function: "archive_work_package".to_string(),
        family: "projects".to_string(),
        methods: vec!["POST".to_string()],
        path: "/projects/<int:project_id>/work_packages/<int:id>/archive".to_string(),
        inputs: Inputs {
            path_params: vec![
                PathParam {
                    name: "project_id".to_string(),
                    converter: Some("int".to_string()),
                },
                PathParam {
                    name: "id".to_string(),
                    converter: Some("int".to_string()),
                },
            ],
            query_reads: vec![],
            form_fields: vec![],
        },
        data: Data {
            models: vec!["WorkPackage".to_string()],
            order_by: None,
            order_dir: None,
            tenant_scoped: true,
            mutates: true,
            soft_delete: true,
        },
        output: OutputKind::Redirect {
            target: "/projects".to_string(),
        },
        guards: vec!["login_required".to_string()],
        handler_kind: HandlerKind::SoftDelete,
        classification_reason: "scoped find_by_id".to_string(),
        provenance: Provenance {
            file: "lib/api/v3/projects/projects_api.rb".to_string(),
            line_start: 120,
            line_end: 140,
        },
    }
}

fn toggle_bool_field_contract() -> RouteContract {
    RouteContract {
        id: "projects.toggle_work_package_active".to_string(),
        function: "toggle_work_package_active".to_string(),
        family: "projects".to_string(),
        methods: vec!["POST".to_string()],
        path: "/projects/<int:project_id>/work_packages/<int:id>/toggle".to_string(),
        inputs: Inputs {
            path_params: vec![
                PathParam {
                    name: "project_id".to_string(),
                    converter: Some("int".to_string()),
                },
                PathParam {
                    name: "id".to_string(),
                    converter: Some("int".to_string()),
                },
            ],
            query_reads: vec![],
            form_fields: vec![],
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
            target: "/projects".to_string(),
        },
        guards: vec!["login_required".to_string()],
        handler_kind: HandlerKind::ToggleBoolField,
        classification_reason: "scoped find_by_id".to_string(),
        provenance: Provenance {
            file: "lib/api/v3/projects/projects_api.rb".to_string(),
            line_start: 155,
            line_end: 170,
        },
    }
}

// ---------------------------------------------------------------------------
// Golden tests — one per kind.
// ---------------------------------------------------------------------------

#[test]
fn test_list_for_tenant_sqlx() {
    let contract = list_for_tenant_contract();
    let spec = test_spec();
    let emitted =
        ruff_python_dto_check::codegen::sqlx_emit::list_for_tenant::emit_list_for_tenant_sqlx(
            &contract, &spec,
        );
    let expected = include_str!("golden/codegen/sqlx/expected/list_for_tenant.rs");
    assert_eq!(emitted.handler_rs.trim(), expected.trim());
}

#[test]
fn test_detail_for_tenant_sqlx() {
    let contract = detail_for_tenant_contract();
    let spec = test_spec();
    let emitted =
        ruff_python_dto_check::codegen::sqlx_emit::detail_for_tenant::emit_detail_for_tenant_sqlx(
            &contract, &spec,
        );
    let expected = include_str!("golden/codegen/sqlx/expected/detail_for_tenant.rs");
    assert_eq!(emitted.handler_rs.trim(), expected.trim());
}

#[test]
fn test_soft_delete_sqlx() {
    let contract = soft_delete_contract();
    let spec = test_spec();
    let emitted = ruff_python_dto_check::codegen::sqlx_emit::soft_delete::emit_soft_delete_sqlx(
        &contract, &spec,
    );
    let expected = include_str!("golden/codegen/sqlx/expected/soft_delete.rs");
    assert_eq!(emitted.handler_rs.trim(), expected.trim());
}

#[test]
fn test_toggle_bool_field_sqlx() {
    let contract = toggle_bool_field_contract();
    let spec = test_spec();
    let emitted =
        ruff_python_dto_check::codegen::sqlx_emit::toggle_bool_field::emit_toggle_bool_field_sqlx(
            &contract, &spec,
        );
    let expected = include_str!("golden/codegen/sqlx/expected/toggle_bool_field.rs");
    assert_eq!(emitted.handler_rs.trim(), expected.trim());
}
