//! sqlx (axum + raw `sqlx::query_as`) emitter family.
//!
//! Sibling to the seaorm dispatch in `crate::codegen`. Selected by setting
//! `orm = "sqlx"` on the TOML spec (or constructing a [`crate::codegen::target::TargetSpec`]
//! with `Orm::Sqlx`). The parent `codegen::emit` routes the four supported
//! kinds here:
//!
//! - [`list_for_tenant::emit_list_for_tenant_sqlx`]
//! - [`detail_for_tenant::emit_detail_for_tenant_sqlx`]
//! - [`soft_delete::emit_soft_delete_sqlx`]
//! - [`toggle_bool_field::emit_toggle_bool_field_sqlx`]
//!
//! Kinds not listed in the spec's `emit_kinds` receive the standard
//! `emit_stub`. Kinds not implemented in this module at all currently fall
//! through to the seaorm dispatch — flag this with a calibration warning if
//! it produces wrong output for your target (see SQLX-TARGET.md).
//!
//! Each kind lives in its own file (file-disjoint per Sprint C1 fanout
//! discipline) and inlines its small helpers; no shared `helpers.rs` yet.

pub mod ajax_json;
pub mod csrf_form_post;
pub mod detail_for_tenant;
pub mod list_for_tenant;
pub mod soft_delete;
pub mod toggle_bool_field;
