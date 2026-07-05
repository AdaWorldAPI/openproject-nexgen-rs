//! `class_schema` — SurrealQL emission from a canonical
//! [`ogar_vocab::Class`] via the [`ogar_render_askama`] kit.
//!
//! **Northstar plan §3, C1.** Sibling render path to the existing
//! triples-driven pipeline (`OpSurrealProjection` →
//! [`crate::surreal_text`]). Where the triples path consumes
//! [`lance_graph_contract::codegen_spine::Triple`]s and walks a typed
//! AST ([`op_surreal_ast`]), this module consumes
//! [`ogar_vocab::Class`] directly and renders via the OGAR askama
//! `SurrealqlTable` artifact emitter.
//!
//! Both paths produce SurrealQL DDL. They are intentionally **not**
//! byte-identical — the byte-identical-output pin
//! ([`crate::tests`]'s `rails_mini_e2e_byte_for_byte_with_legacy_emission`
//! in `op-surreal-ast`) is specifically about the triples → AST → SQL
//! step, which both `from_triples::triples_to_schema` and the typed-AST
//! `ToSql` render preserve. The askama path emits a different shape
//! (matches the OGAR codebook convention: `COMMENT "…"` double-quoted,
//! `DEFINE FIELD <name> ON <table>` rather than `ON TABLE <table>`,
//! `array<record<…>>` for has_many) — that's the shared shape every
//! port (`op-`, `redmine-`, future medcare-, …) emits through the kit.
//!
//! # Two pipelines, one consumer surface
//!
//! ```text
//!   Rails source → triples → OpSurrealProjection → DefineTable IR
//!         │                                                │
//!         │                                                │ surreal_text()
//!         │                                                ▼
//!         │                                          SurrealQL (legacy shape,
//!         │                                          byte-identical pinned)
//!         │
//!         └─► ogar_vocab::Class fns ─► render_class_schema (this module)
//!                                                │
//!                                                │ askama SurrealqlTable
//!                                                ▼
//!                                          SurrealQL (canonical OGAR shape,
//!                                          shared with every port)
//! ```
//!
//! # What this enables (Northstar §3 C1 follow-ons)
//!
//! Once consumers can call `render_class_schema(&class)` directly,
//! later sprints can:
//! - C6+ swap the codegen CLI to drive the canonical-class path for
//!   any concept the OGAR codebook covers (the 32 promoted concepts).
//! - Add a sibling `render_classes_schema(&[Class])` walker for
//!   multi-table emission (already shipped here so the CLI just
//!   needs to wire the call).
//! - Compose with `op_surreal_ast::from_class::class_to_table` when
//!   a consumer wants the **typed AST** path instead — that's C3's
//!   sibling, both feed the same `Schema` shape.

use ogar_render_askama::{render, ArtifactKind};
use ogar_vocab::Class;

/// Render one canonical [`Class`] as SurrealQL DDL via the OGAR
/// askama `SurrealqlTable` artifact emitter.
///
/// Pure, no I/O, deterministic — the same `Class` always renders to
/// the same string. The output shape matches every other port's
/// SurrealQL emission (redmine-canon, medcare-canon, …): one
/// `DEFINE TABLE … SCHEMAFULL` plus the codebook-id `COMMENT`, one
/// `DEFINE FIELD <name> ON <table> TYPE <kind>` per typed attribute,
/// `option<record<target>>` or `array<record<target>>` for family
/// edges, and a default primary-label index for classes carrying a
/// `name`/`subject`/`title`/`label` attribute.
///
/// # Errors
///
/// Returns the underlying `askama::Error` if template rendering
/// fails. In practice the only way this fires is a mismatched
/// argument arity in the artifact emitter (e.g. a misnamed column
/// fed in), which won't happen for the codebook-driven path — the
/// emitter constructs its own [`ArtifactSpec`] from the `Class`.
pub fn render_class_schema(class: &Class) -> Result<String, askama::Error> {
    render(class, ArtifactKind::SurrealqlTable)
}

/// Render every [`Class`] in `classes` as SurrealQL DDL, concatenated
/// in insertion order with a single `\n` between tables.
///
/// Convenience over a [`render_class_schema`] loop. Insertion-order
/// is preserved exactly — the caller is responsible for sorting (or
/// not). Empty input renders to `""`.
///
/// # Errors
///
/// First per-class render error short-circuits the walk; the partial
/// output is dropped.
pub fn render_classes_schema(classes: &[Class]) -> Result<String, askama::Error> {
    let mut out = String::new();
    for class in classes {
        let block = render_class_schema(class)?;
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&block);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::{billable_work_entry, project, project_role, project_work_item};

    #[test]
    fn render_class_schema_emits_define_table_with_codebook_comment() {
        // Headline pin: a promoted class produces `DEFINE TABLE <concept>
        // SCHEMAFULL` + the codebook-id COMMENT. Same shape every port
        // emits — this is the canonical OGAR rendering.
        let class = project_work_item();
        let src = render_class_schema(&class).unwrap();
        assert!(
            src.contains("DEFINE TABLE project_work_item SCHEMAFULL"),
            "expected DEFINE TABLE in:\n{src}"
        );
        assert!(
            src.contains("COMMENT \"OGAR codebook id 0x0102 (project_work_item)\""),
            "expected codebook id COMMENT in:\n{src}"
        );
    }

    #[test]
    fn render_class_schema_maps_rails_types_to_surrealql_kinds() {
        // project_role has typed attributes (name string, position
        // integer, permissions text). The askama emitter uses the same
        // Rails → SurrealQL type map as op_surreal_ast::Kind::from_rails_type.
        let class = project_role();
        let src = render_class_schema(&class).unwrap();
        assert!(
            src.contains("DEFINE FIELD name ON project_role TYPE string"),
            "{src}"
        );
        assert!(
            src.contains("DEFINE FIELD position ON project_role TYPE int"),
            "{src}"
        );
        assert!(
            src.contains("DEFINE FIELD permissions ON project_role TYPE string"),
            "{src}"
        );
    }

    #[test]
    fn render_class_schema_emits_record_links_for_family_edges() {
        // belongs_to / has_one → `option<record<target>>`.
        // has_many / habtm   → `array<record<target>>` (the askama
        // path covers the array variant — the op_surreal_ast typed
        // path skips it until Kind::Array lands; see
        // op_surreal_ast::from_class module doc).
        let class = billable_work_entry();
        let src = render_class_schema(&class).unwrap();
        assert!(
            src.contains("TYPE option<record<"),
            "expected at least one optional record link in:\n{src}"
        );
        assert!(
            src.contains("record<tax_policy>"),
            "expected snake_case'd record<tax_policy> target in:\n{src}"
        );
    }

    #[test]
    fn render_class_schema_emits_primary_label_index_for_named_class() {
        let class = project_role();
        let src = render_class_schema(&class).unwrap();
        assert!(
            src.contains("DEFINE INDEX idx_name ON project_role FIELDS name"),
            "expected idx_name on project_role:\n{src}"
        );
    }

    #[test]
    fn render_classes_schema_concatenates_in_insertion_order() {
        let classes = [project(), project_role()];
        let src = render_classes_schema(&classes).unwrap();
        let i_project = src
            .find("DEFINE TABLE project SCHEMAFULL")
            .expect("project block missing");
        let i_role = src
            .find("DEFINE TABLE project_role SCHEMAFULL")
            .expect("project_role block missing");
        assert!(
            i_project < i_role,
            "expected project before project_role in:\n{src}"
        );
    }

    #[test]
    fn render_classes_schema_empty_input_renders_empty_string() {
        let src = render_classes_schema(&[]).unwrap();
        assert!(src.is_empty(), "empty input should render empty:\n{src}");
    }

    #[test]
    fn render_class_schema_is_deterministic() {
        // Two renders of the same class must produce byte-identical
        // strings. The kit is pure; this pins it from drift.
        let class = project_work_item();
        let a = render_class_schema(&class).unwrap();
        let b = render_class_schema(&class).unwrap();
        assert_eq!(a, b, "render_class_schema must be deterministic");
    }

    /// PILOT FALSIFIER (2026-07-03) — the same canonical WorkPackage
    /// `Class` that renders as a `SurrealqlTable` (DDL skin, above) also
    /// renders through the ERB-shaped HTML skins (`HtmlForm` /
    /// `HtmlListView` / `HtmlDetailView`) — "same skull, HTML body".
    ///
    /// This is op-nexgen's consumer-side proof of the **render leg**:
    /// consuming an `ogar_vocab::Class` through the kit IS this repo's
    /// job; the **lift** that populates the Class from real extraction
    /// (`ogar_from_ruff::lift_model_graph`) is upstream and deliberately
    /// NOT here (see the `op-canon` crate header — snapshot, not live
    /// extraction). Uses only vendored crates; invents nothing.
    ///
    /// FINDING: the render leg is structurally complete — every skin
    /// emits its ClassView container carrying `data-class-id="0x0102"`
    /// + `data-concept="project_work_item"`. The field *contents* are
    /// empty here ONLY because `project_work_item()` is a thin
    /// identity-only exemplar; a populated Class (from the upstream
    /// lift) fills the `<input>`s / list columns / detail `<dl>`. The
    /// gap this pilot surfaces is upstream (Class population), not in
    /// the render leg. `--nocapture` to eyeball the shape.
    #[test]
    fn workpackage_class_renders_through_the_erb_html_skins() {
        use ogar_render_askama::{render, ArtifactKind};
        let class = project_work_item();

        // Each skin emits its ERB/ClassView container, tagged with the
        // canonical identity — the "australopithecine skull" the same
        // skull wears regardless of body (DDL vs HTML).
        let form = render(&class, ArtifactKind::HtmlForm).expect("HtmlForm render");
        assert!(form.contains("class=\"ogar-form\""), "form skin:\n{form}");
        assert!(
            form.contains("data-class-id=\"0x0102\""),
            "form carries classid:\n{form}"
        );
        assert!(
            form.contains("data-concept=\"project_work_item\""),
            "form carries concept:\n{form}"
        );

        let list = render(&class, ArtifactKind::HtmlListView).expect("HtmlListView render");
        assert!(list.contains("class=\"ogar-list\""), "list skin:\n{list}");
        assert!(
            list.contains("data-concept=\"project_work_item\""),
            "list concept:\n{list}"
        );

        let detail = render(&class, ArtifactKind::HtmlDetailView).expect("HtmlDetailView render");
        assert!(
            detail.contains("class=\"ogar-detail\""),
            "detail skin:\n{detail}"
        );
        assert!(
            detail.contains("data-concept=\"project_work_item\""),
            "detail concept:\n{detail}"
        );

        for (kind, html) in [
            ("HtmlForm", &form),
            ("HtmlListView", &list),
            ("HtmlDetailView", &detail),
        ] {
            eprintln!("\n───── {kind} ─────\n{html}\n");
        }
    }
}
