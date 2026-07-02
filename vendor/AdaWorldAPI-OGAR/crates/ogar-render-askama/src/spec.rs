//! Per-render spec — the typed input each [`ArtifactKind`] emitter consumes.
//!
//! Mirror of `woa-rs::codegen::spec::RouteSpec` but for the canonical-layer
//! pipeline: the input is an [`ogar_vocab::Class`] (the calcified AR shape),
//! not a JSON-loaded `RouteSpec`. Codegen reads the class fns at build time
//! and dispatches each through an emitter for the chosen
//! [`ArtifactKind`].

use ogar_vocab::Class;

/// The set of target artifacts the render kit can emit per canonical class.
/// New kinds are appended (never reordered) so the dispatcher stays
/// backward-compatible. Mirrors WoA-rs's `HandlerKind` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    /// Rust `struct` definition + `pub const CLASS_ID: u16` constant.
    /// **Codegen flavour** — downstream compiler is the final consumer.
    RustStruct,
    /// Tabular HTML list view rendered server-side via `askama_axum` or
    /// equivalent. **Render flavour** — the askama-rendered HTML *is* the
    /// output the user reads. Inherits the Redmine `Query` +
    /// `column_content` pattern (17 years of evolution) mapped onto our
    /// `ClassView` substrate. Spec: `docs/integration/REDMINE-QUERY-
    /// HARVEST.md` §3.
    HtmlListView,
    /// Single-record HTML detail view: definition-list inline fields +
    /// `<section>` blocks for prose / family-edge collections. **Render
    /// flavour**; mirror of Redmine's `show.html.erb`. Reuses every
    /// `RenderColumn` / `ColumnKind` / cell sub-template from T2.
    HtmlDetailView,
    /// Create/edit HTML form rendered server-side via `askama_axum`.
    /// **Render flavour**; mirror of Redmine's `_form.html.erb`. Same
    /// `RenderColumn` shape; per-attribute `InputKind` dispatches to one
    /// of nine `<input>` / `<textarea>` / `<select>` sub-templates.
    HtmlForm,
    /// SurrealQL `DEFINE TABLE` + per-field `DEFINE FIELD` statements.
    /// **Codegen flavour** — DB engine is the final consumer.
    SurrealqlTable,
    /// **Deprecated** — anti-pattern #8: askama is the render output, not
    /// a producer of other-language schemas. OpenAPI consumers outside
    /// the WoA pattern can implement their own ClassView-based generator.
    /// Variant kept one cycle so existing dispatchers still match;
    /// removed in the next kit cleanup.
    OpenapiSchema,
    /// Rust `match` arm dispatching on `ClassId` — useful for routing on
    /// `NodeGuid::classid` in graph consumers. Codegen flavour. Roadmap
    /// (post-+5+5); not in the bootstrap kit.
    NodeGuidRoutingArm,
}

impl ArtifactKind {
    /// All kinds in declaration order. Stable across additions (the
    /// `ArtifactKind` enum is treated append-only).
    pub const ALL: &'static [Self] = &[
        Self::RustStruct,
        Self::HtmlListView,
        Self::HtmlDetailView,
        Self::HtmlForm,
        Self::SurrealqlTable,
        Self::OpenapiSchema,
        Self::NodeGuidRoutingArm,
    ];

    /// Human-readable short name — used in stub emitters' marker comments
    /// and in `cargo doc` text.
    pub fn name(self) -> &'static str {
        match self {
            Self::RustStruct => "rust_struct",
            Self::HtmlListView => "html_list_view",
            Self::HtmlDetailView => "html_detail_view",
            Self::HtmlForm => "html_form",
            Self::SurrealqlTable => "surrealql_table",
            Self::OpenapiSchema => "openapi_schema",
            Self::NodeGuidRoutingArm => "node_guid_routing_arm",
        }
    }
}

/// One render request: emit `class` as `kind`.
///
/// Borrow-based so a caller iterating the full codebook does not allocate
/// 32 Class copies per artifact kind.
#[derive(Debug, Clone, Copy)]
pub struct ArtifactSpec<'a> {
    /// The canonical class to render.
    pub class: &'a Class,
    /// Which target artifact to emit.
    pub kind: ArtifactKind,
}

impl<'a> ArtifactSpec<'a> {
    /// Pair a `class` with a target `kind`. No allocation; the `class`
    /// reference outlives the spec.
    pub fn new(class: &'a Class, kind: ArtifactKind) -> Self {
        Self { class, kind }
    }
}
