//! Per-kind emitter dispatch. Mirror of `woa-rs::codegen::handler_kinds`.
//!
//! Each [`ArtifactKind`] owns its own emitter. The dispatcher is a tiny
//! `match` over the enum, returning a boxed trait object so the call site
//! is one line:
//!
//! ```ignore
//! let emitter = artifact_kinds::for_kind(spec.kind);
//! let source = emitter.emit(&spec)?;
//! ```
//!
//! Real emitters (so far):
//! - [`RustStruct`](rust_struct::RustStructEmitter) — T1, codegen flavour,
//!   from PR #78.
//! - [`HtmlListView`](html_list_view::HtmlListViewEmitter) — T2, render
//!   flavour. Mirrors Redmine's `_list.html.erb` shape on our substrate
//!   (see `docs/integration/REDMINE-QUERY-HARVEST.md`).
//! - [`HtmlDetailView`](html_detail_view::HtmlDetailViewEmitter) — T3,
//!   render flavour. Mirror of Redmine `show.html.erb`.
//! - [`HtmlForm`](html_form::HtmlFormEmitter) — T4, render flavour.
//!   Mirror of Redmine `_form.html.erb`. Separate
//!   [`InputKind`](crate::form_view::InputKind) catalog from `ColumnKind`
//!   because input controls don't map 1:1 to display formatters.
//! - [`SurrealqlTable`](surrealql_table::SurrealqlTableEmitter) — T5,
//!   codegen flavour. Emits `DEFINE TABLE` + per-field `DEFINE FIELD`
//!   + family-edge `record<…>` links + primary-label index.
//!
//! Remaining kinds (`OpenapiSchema`, `NodeGuidRoutingArm`) use [`Stub`]
//! — `OpenapiSchema` is deprecated (anti-pattern #8), `NodeGuidRoutingArm`
//! is roadmap-only (post-+5+5).

pub(crate) mod cells;
pub(crate) mod inputs;

use crate::spec::{ArtifactKind, ArtifactSpec};

pub mod html_detail_view;
pub mod html_form;
pub mod html_list_view;
pub mod rust_struct;
pub mod stub;
pub mod surrealql_table;

pub use html_detail_view::{render_detail, HtmlDetailViewEmitter};
pub use html_form::{render_form, FormFieldSource, FormSource, HtmlFormEmitter};
pub use html_list_view::{
    render_list, AttachmentEntryOwned, CellData, CellSource, GroupHeader, HtmlListViewEmitter,
    RelationEntryOwned, RowSource, UserEntryOwned,
};
pub use inputs::{InputData, SelectOptionOwned};
pub use surrealql_table::SurrealqlTableEmitter;

/// Contract every kind's emitter implements.
pub trait ArtifactEmitter {
    /// Render `spec.class` as the target artifact for this emitter's
    /// [`ArtifactKind`]. Returns the emitted source as a `String`;
    /// downstream tooling writes it to disk.
    fn emit(&self, spec: &ArtifactSpec<'_>) -> Result<String, askama::Error>;
}

/// Dispatch to the concrete emitter for `kind`. Always returns Some
/// emitter — unimplemented kinds fall through to [`Stub`].
pub fn for_kind(kind: ArtifactKind) -> Box<dyn ArtifactEmitter> {
    match kind {
        ArtifactKind::RustStruct => Box::new(rust_struct::RustStructEmitter),
        ArtifactKind::HtmlListView => Box::new(html_list_view::HtmlListViewEmitter),
        ArtifactKind::HtmlDetailView => Box::new(html_detail_view::HtmlDetailViewEmitter),
        ArtifactKind::HtmlForm => Box::new(html_form::HtmlFormEmitter),
        ArtifactKind::SurrealqlTable => Box::new(surrealql_table::SurrealqlTableEmitter),
        other => Box::new(stub::Stub { kind: other }),
    }
}
