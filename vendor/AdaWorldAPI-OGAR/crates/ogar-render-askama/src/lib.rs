//! `ogar-render-askama` — askama rendering harness over the calcified
//! canonical layer.
//!
//! Structurally a mirror of [`AdaWorldAPI/woa-rs`](https://github.com/AdaWorldAPI/woa-rs)
//! `crates/codegen` + `templates/`: one [`ArtifactKind`] enum dispatched
//! through a per-kind [`ArtifactEmitter`] trait, with one askama template
//! per kind. The canonical input is [`ogar_vocab::Class`] instead of
//! WoA's `RouteSpec`, but the kit shape is the same.
//!
//! # Two flavours of artifact (no TypeScript layer)
//!
//! - **Codegen** — emit `.rs` / `.surql` source files. The downstream
//!   compiler / DB engine is the final consumer (Northstar plan §3:
//!   T1 `RustStruct`, T5 `SurrealqlTable`).
//! - **Render** — emit HTML the human reads directly via `askama_axum` or
//!   equivalent (T2–T4: `HtmlListView` / `HtmlDetailView` / `HtmlForm`).
//!   Askama **is** the output; nothing transcodes it further.
//!
//! There is no TypeScript codegen path. Askama as a producer of `.ts`
//! source files is anti-pattern #8 in the Northstar plan — askama is
//! the rendering layer in WoA-rs's pattern, not a producer of other
//! languages the consumer transcompiles. PR #80 (TsInterface) was closed
//! for this reason.
//!
//! # The 800 → 7-70 collapse
//!
//! The number of templates is bounded by **artifact kind**, never by
//! `(class × target)`. Adding a new canonical concept (e.g. promoting
//! `project_costs`) is one ogar-vocab class fn + zero new templates — the
//! existing kit renders it through. Adding a new target (gremlin, proto,
//! …) is one new [`ArtifactKind`] variant + one askama template; every
//! promoted concept emits through it automatically.
//!
//! # Layering (where this lives in the OGAR stack)
//!
//! ```text
//!   ogar-vocab            (codebook + Class fns)
//!         │
//!         │  pure construction at build time
//!         ▼
//!   ogar-render-askama    (THIS CRATE — askama-bound emitters per kind)
//!         │
//!         │  .rs / .surql source text  (codegen flavour, T1 / T5)
//!         │  rendered HTML strings     (render flavour, T2–T4 via askama_axum)
//!         ▼
//!   downstream consumers  (op-codegen-projection, rm-codegen, medcare, …,
//!                          OR the user's browser for the render flavour)
//! ```
//!
//! `ClassView` (the **run-time** projection layer in `lance-graph-contract`)
//! is a sibling concern: it materialises a SoA row's render rows at query
//! time. Both pipelines are jinja-templated; both share the N3 field order
//! convention; they consume different shapes. This crate handles the
//! build-time path (typed source emission); `ogar-class-view` handles the
//! run-time path (label-resolved row projection).
//!
//! # Proof-of-shape phase
//!
//! - [`ArtifactKind::RustStruct`] — codegen, real emitter (T1, PR #78).
//! - [`ArtifactKind::HtmlListView`] — render, real emitter (T2, this PR).
//!   Spec lifted from `docs/integration/REDMINE-QUERY-HARVEST.md`.
//! - Remaining kinds (`SurrealqlTable`, `OpenapiSchema`,
//!   `NodeGuidRoutingArm`) use [`artifact_kinds::stub::Stub`] —
//!   placeholder code that compiles and emits a marker comment so
//!   callers can exercise the full pipeline (lookup + dispatch + return)
//!   against every promoted concept while T3–T5 land.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod artifact_kinds;
pub mod form_view;
pub mod list_view;
pub mod rust_class;
pub mod spec;

pub use artifact_kinds::{
    for_kind, render_detail, render_form, render_list, ArtifactEmitter, AttachmentEntryOwned,
    CellData, CellSource, FormFieldSource, FormSource, GroupHeader, InputData,
    RelationEntryOwned, RowSource, SelectOptionOwned, UserEntryOwned,
};
pub use form_view::{default_input_kind_for, InputKind};
pub use list_view::{default_kind_for, ColumnKind, RenderColumn, SortOrder};
pub use rust_class::render_class_with_methods;
pub use spec::{ArtifactKind, ArtifactSpec};

use ogar_vocab::Class;

/// Render one artifact in one call. Convenience over
/// [`artifact_kinds::for_kind`] + the emitter trait.
pub fn render(class: &Class, kind: ArtifactKind) -> Result<String, askama::Error> {
    let spec = ArtifactSpec::new(class, kind);
    for_kind(kind).emit(&spec)
}

/// Render every promoted concept for one [`ArtifactKind`], returning
/// `(canonical_concept, source)` pairs. Useful for batch codegen of a
/// full target (e.g. emit every concept as a Rust struct).
///
/// Walks the same 32-concept set [`crate::artifact_kinds`]'s tests use.
/// Concepts without a `canonical_concept` field are skipped.
pub fn render_all(
    classes: &[Class],
    kind: ArtifactKind,
) -> Result<Vec<(String, String)>, askama::Error> {
    let emitter = for_kind(kind);
    let mut out = Vec::with_capacity(classes.len());
    for class in classes {
        let Some(concept) = class.canonical_concept.clone() else {
            continue;
        };
        let spec = ArtifactSpec::new(class, kind);
        let source = emitter.emit(&spec)?;
        out.push((concept, source));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::{
        billable_work_entry, project, project_actor, project_role, project_work_item,
    };

    #[test]
    fn artifact_kind_all_const_enumerates_every_variant() {
        // Pin: `ArtifactKind::ALL` enumerates every variant. New variants
        // must be appended here (the enum + the const slice both change).
        let all = ArtifactKind::ALL;
        assert!(
            all.contains(&ArtifactKind::RustStruct)
                && all.contains(&ArtifactKind::HtmlListView)
                && all.contains(&ArtifactKind::HtmlDetailView)
                && all.contains(&ArtifactKind::HtmlForm)
                && all.contains(&ArtifactKind::SurrealqlTable)
                && all.contains(&ArtifactKind::OpenapiSchema)
                && all.contains(&ArtifactKind::NodeGuidRoutingArm),
            "ArtifactKind::ALL missing a variant"
        );
        assert_eq!(all.len(), 7);
    }

    #[test]
    fn rust_struct_emits_pub_const_class_id_for_promoted_concept() {
        // Proof of shape: render project_work_item — verify the emitted
        // source declares the right struct name + CLASS_ID + canonical
        // concept.
        let class = project_work_item();
        let src = render(&class, ArtifactKind::RustStruct).unwrap();
        assert!(src.contains("pub struct ProjectWorkItem"), "{src}");
        assert!(
            src.contains("pub const CLASS_ID: u16 = 0x0102;"),
            "expected CLASS_ID = 0x0102 in:\n{src}"
        );
        assert!(
            src.contains("pub const CANONICAL_CONCEPT: &str = \"project_work_item\";"),
            "{src}"
        );
        // Doc comment should reference the class fn name.
        assert!(src.contains("project_work_item()"), "{src}");
    }

    #[test]
    fn rust_struct_emits_family_edges() {
        // billable_work_entry has its 12 family edges; the emitted struct
        // must surface them as fields (Vec<u64> for has_many,
        // Option<u64> for belongs_to/has_one).
        let class = billable_work_entry();
        let src = render(&class, ArtifactKind::RustStruct).unwrap();
        for edge in &class.associations {
            assert!(
                src.contains(&format!("pub {}:", edge.name)),
                "billable_work_entry rust_struct missing family edge `{}`:\n{src}",
                edge.name
            );
        }
    }

    #[test]
    fn rust_struct_emits_typed_attribute_for_each_class_attribute() {
        // project_role has typed attributes (name, position, permissions).
        // Every one must appear in the emitted struct.
        let class = project_role();
        let src = render(&class, ArtifactKind::RustStruct).unwrap();
        for attr in &class.attributes {
            assert!(
                src.contains(&format!("pub {}:", attr.name)),
                "project_role rust_struct missing attribute `{}`:\n{src}",
                attr.name
            );
        }
    }

    #[test]
    fn stub_emits_marker_for_unimplemented_kinds() {
        // After T1-T5, only the deprecated `OpenapiSchema` and the
        // roadmap-only `NodeGuidRoutingArm` remain stubbed. Both compile
        // and emit a marker comment naming the kind + class.
        let class = project();
        for kind in [
            ArtifactKind::OpenapiSchema,
            ArtifactKind::NodeGuidRoutingArm,
        ] {
            let src = render(&class, kind).unwrap();
            assert!(
                src.contains(kind.name()),
                "stub for {:?} should mention its name:\n{src}",
                kind
            );
            assert!(
                src.contains("Project"),
                "stub should mention the class name:\n{src}"
            );
        }
    }

    // ── T2 (HtmlListView) tests ─────────────────────────────────────

    #[test]
    fn html_list_view_proof_of_shape_renders_canonical_concept_header() {
        // The codebook-only emit path: no rows, just the class shell.
        // Pins that the spine template is wired and surfaces the
        // class_id + concept as data-attributes for downstream JS hooks.
        let class = project_work_item();
        let src = render(&class, ArtifactKind::HtmlListView).unwrap();
        assert!(
            src.contains("data-class-id=\"0x0102\""),
            "expected data-class-id=\"0x0102\" in:\n{src}"
        );
        assert!(
            src.contains("data-concept=\"project_work_item\""),
            "{src}"
        );
        // Empty-state row appears when no rows are supplied.
        assert!(src.contains("No data."), "expected empty-state in:\n{src}");
    }

    #[test]
    fn html_list_view_renders_inline_and_block_rows() {
        // The substantive path: build columns + rows and assert the
        // spine template substitutes them correctly.
        let inline = vec![
            RenderColumn::new("id", "#", ColumnKind::IdLink).sortable().frozen(),
            RenderColumn::new("subject", "Subject", ColumnKind::PrimaryLink).sortable(),
            RenderColumn::new("done_ratio", "% Done", ColumnKind::ProgressBar),
        ];
        let block = vec![
            RenderColumn::new("description", "Description", ColumnKind::RichText).block(),
        ];

        let row = RowSource {
            record_id: 42,
            css_classes: "odd issue closed",
            group: None,
            inline: vec![
                CellSource {
                    column: &inline[0],
                    css_classes: "num",
                    data: CellData::IdLink { id: 42, href: "/issues/42" },
                },
                CellSource {
                    column: &inline[1],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: "Fix the foo",
                        href: "/issues/42",
                    },
                },
                CellSource {
                    column: &inline[2],
                    css_classes: "",
                    data: CellData::ProgressBar { pct: 70 },
                },
            ],
            block: vec![CellSource {
                column: &block[0],
                css_classes: "",
                data: CellData::RichText {
                    body: "<p>Some rendered prose.</p>",
                },
            }],
        };

        let src = render_list(
            "Work items",
            0x0102,
            "project_work_item",
            &inline,
            &block,
            std::slice::from_ref(&row),
        )
        .unwrap();

        // Header + columns
        assert!(src.contains("<h2>Work items</h2>"), "{src}");
        assert!(src.contains("data-class-id=\"0x0102\""));
        assert!(src.contains("Subject"), "expected `Subject` column header in:\n{src}");
        assert!(src.contains("% Done"));
        // Inline row + cells
        assert!(src.contains("id=\"record-42\""), "expected id=\"record-42\":\n{src}");
        assert!(src.contains("href=\"/issues/42\""), "{src}");
        assert!(src.contains("#42"), "id link body should be `#42`:\n{src}");
        assert!(src.contains("Fix the foo"), "{src}");
        // Progress bar
        assert!(src.contains("aria-valuenow=\"70\""), "{src}");
        assert!(src.contains("width: 70%"), "{src}");
        // Block row
        assert!(
            src.contains("class=\"odd issue closed block-row\""),
            "expected block row CSS in:\n{src}"
        );
        assert!(
            src.contains("class=\"wiki\""),
            "rich-text wrapper missing in:\n{src}"
        );
        assert!(src.contains("Some rendered prose."), "{src}");
    }

    #[test]
    fn html_list_view_renders_group_separator_when_provided() {
        let col = RenderColumn::new("subject", "Subject", ColumnKind::PrimaryLink);
        let row = RowSource {
            record_id: 1,
            css_classes: "",
            group: Some(GroupHeader { label: "Open", count: 5 }),
            inline: vec![CellSource {
                column: &col,
                css_classes: "",
                data: CellData::PrimaryLink { label: "T1", href: "/i/1" },
            }],
            block: vec![],
        };
        let src = render_list(
            "By status",
            0x0102,
            "project_work_item",
            std::slice::from_ref(&col),
            &[],
            &[row],
        )
        .unwrap();
        assert!(src.contains("class=\"group open\""), "{src}");
        assert!(src.contains("<span class=\"name\">Open</span>"), "{src}");
        assert!(src.contains("<span class=\"badge\">5</span>"), "{src}");
    }

    // ── XSS regression — codex P1 on #83 + #84 ──────────────────────

    #[test]
    fn html_list_view_escapes_data_derived_strings_xss_regression() {
        // Codex P1 on PR #83: the spine template was compiled with
        // `escape = "none"`, so every interpolation was raw — a malicious
        // group-header label, title, caption, or css_class would inject
        // raw HTML into the page. The fix is `escape = "html"` on the
        // spine binding + `|safe` only on the intentionally-pre-rendered
        // `cell.body_html`. This test pins the contract: untrusted
        // strings get escaped; only cell bodies are raw.
        let col = RenderColumn::new(
            "subject",
            "<script>alert(1)</script>", // poisoned caption
            ColumnKind::PrimaryLink,
        );
        let row = RowSource {
            record_id: 1,
            css_classes: "<malicious-class>",
            group: Some(GroupHeader {
                label: "<img src=x onerror=alert(2)>",
                count: 0,
            }),
            inline: vec![CellSource {
                column: &col,
                css_classes: "<bad-cell-class>",
                data: CellData::PrimaryLink {
                    label: "<safe>label</safe>",
                    href: "/i/1",
                },
            }],
            block: vec![],
        };
        let src = render_list(
            "<title-xss>x</title-xss>",
            0x0102,
            "project_work_item",
            std::slice::from_ref(&col),
            &[],
            std::slice::from_ref(&row),
        )
        .unwrap();

        // Untrusted strings MUST be escaped — no raw `<script>` etc.
        assert!(
            !src.contains("<script>alert(1)"),
            "caption was rendered raw — XSS hazard:\n{src}"
        );
        assert!(
            src.contains("&lt;script&gt;alert(1)"),
            "caption should be HTML-escaped:\n{src}"
        );
        assert!(
            !src.contains("<img src=x onerror"),
            "group label was rendered raw — XSS hazard:\n{src}"
        );
        assert!(
            !src.contains("<malicious-class>"),
            "row css_classes was rendered raw — XSS hazard:\n{src}"
        );
        assert!(
            !src.contains("<title-xss>"),
            "title was rendered raw — XSS hazard:\n{src}"
        );
        // The pre-rendered cell body comes from the PrimaryLink cell
        // sub-template, which DOES escape its `label` field — the
        // `<safe>` markers in the label become `&lt;safe&gt;` inside the
        // already-escaped `<a>...</a>` HTML, which is then `|safe`-passed
        // through the spine. Both layers escape: the spine doesn't
        // re-escape, the sub-template escapes once.
        assert!(
            src.contains("&lt;safe&gt;label&lt;/safe&gt;"),
            "primary_link cell should escape its label:\n{src}"
        );
        // The intentional HTML — the `<a>` element wrapping the label —
        // DOES make it through (cell body is pre-rendered + `|safe`).
        assert!(src.contains("<a href=\"/i/1\""), "cell body lost:\n{src}");
    }

    #[test]
    fn html_detail_view_escapes_data_derived_strings_xss_regression() {
        // Codex P1 on PR #84 (sibling of the list-view P1). Same
        // contract: subtitle / labels / css_classes get escaped;
        // only headline_html + cell.body_html + section.body_html
        // are pre-rendered and marked safe.
        let col = RenderColumn::new(
            "status",
            "<script>alert('label')</script>",
            ColumnKind::PrimaryLink,
        );
        let block = RenderColumn::new(
            "description",
            "<script>alert('block-label')</script>",
            ColumnKind::RichText,
        )
        .block();
        let columns = vec![col, block];
        let cells = vec![
            CellSource {
                column: &columns[0],
                css_classes: "<inline-css>",
                data: CellData::PrimaryLink {
                    label: "<safe>x</safe>",
                    href: "/s/1",
                },
            },
            CellSource {
                column: &columns[1],
                css_classes: "<block-css>",
                data: CellData::RichText {
                    body: "<p>Trusted prose.</p>", // intentional HTML
                },
            },
        ];

        let src = render_detail(
            0x0102,
            "project_work_item",
            42,
            // headline_html — intentional HTML, gets through
            "<a href=\"/issues/42\">Headline</a>",
            // subtitle — data-derived, MUST be escaped
            "<img src=x onerror=alert(3)>",
            &columns,
            &cells,
        )
        .unwrap();

        // Subtitle XSS attempt must not survive.
        assert!(
            !src.contains("<img src=x onerror"),
            "subtitle was rendered raw — XSS hazard:\n{src}"
        );
        // Section labels (block labels) must be escaped.
        assert!(
            !src.contains("<script>alert('block-label')"),
            "block section label was rendered raw — XSS hazard:\n{src}"
        );
        // Inline field labels must be escaped.
        assert!(
            !src.contains("<script>alert('label')"),
            "inline field label was rendered raw — XSS hazard:\n{src}"
        );
        // CSS classes must be escaped.
        assert!(
            !src.contains("<inline-css>"),
            "cell css_classes was rendered raw — XSS hazard:\n{src}"
        );
        assert!(
            !src.contains("<block-css>"),
            "section css_classes was rendered raw — XSS hazard:\n{src}"
        );
        // Pre-rendered intentional HTML DOES get through.
        assert!(
            src.contains("<a href=\"/issues/42\">Headline</a>"),
            "headline_html should be marked safe and pass through:\n{src}"
        );
        assert!(
            src.contains("<p>Trusted prose.</p>"),
            "rich-text section body should pass through (cell|safe):\n{src}"
        );
    }

    // ── T3 (HtmlDetailView) tests ───────────────────────────────────

    #[test]
    fn html_detail_view_proof_of_shape_renders_dl_with_class_meta() {
        // Codebook-only emit: synthesised dl from class attributes, "—"
        // placeholders, no headline. Pins data-class-id + data-concept.
        let class = project_work_item();
        let src = render(&class, ArtifactKind::HtmlDetailView).unwrap();
        assert!(
            src.contains("data-class-id=\"0x0102\""),
            "expected data-class-id in:\n{src}"
        );
        assert!(src.contains("data-concept=\"project_work_item\""));
        assert!(src.contains("<dl class=\"detail-fields\">"), "{src}");
        // Every typed attribute should land as a detail-field-<name>.
        for attr in &class.attributes {
            assert!(
                src.contains(&format!("detail-field-{}", attr.name)),
                "missing detail-field-{} in:\n{src}",
                attr.name
            );
        }
    }

    #[test]
    fn html_detail_view_renders_inline_dl_and_block_sections() {
        let inline = RenderColumn::new("status", "Status", ColumnKind::PrimaryLink);
        let pct = RenderColumn::new("done_ratio", "% Done", ColumnKind::ProgressBar);
        let block_desc =
            RenderColumn::new("description", "Description", ColumnKind::RichText).block();

        let columns = vec![inline.clone(), pct.clone(), block_desc.clone()];
        let cells = vec![
            CellSource {
                column: &columns[0],
                css_classes: "",
                data: CellData::PrimaryLink {
                    label: "Open",
                    href: "/statuses/1",
                },
            },
            CellSource {
                column: &columns[1],
                css_classes: "num",
                data: CellData::ProgressBar { pct: 60 },
            },
            CellSource {
                column: &columns[2],
                css_classes: "",
                data: CellData::RichText {
                    body: "<p>Detailed body here.</p>",
                },
            },
        ];

        let src = render_detail(
            0x0102,
            "project_work_item",
            42,
            "<a href=\"/issues/42\" class=\"primary-link\">Fix the foo</a>",
            "Open · High",
            &columns,
            &cells,
        )
        .unwrap();

        // Header
        assert!(src.contains("data-record-id=\"42\""), "{src}");
        assert!(src.contains("class=\"detail-id\">#42"), "{src}");
        assert!(src.contains("Fix the foo"), "headline missing in:\n{src}");
        assert!(src.contains("Open · High"), "subtitle missing in:\n{src}");
        // Inline dl entries
        assert!(src.contains("detail-field-status"), "{src}");
        assert!(src.contains("detail-field-done_ratio"), "{src}");
        // Inline cells render through the per-kind sub-templates.
        assert!(src.contains("href=\"/statuses/1\""), "{src}");
        assert!(src.contains("aria-valuenow=\"60\""), "{src}");
        // Block section for description
        assert!(src.contains("detail-section-description"), "{src}");
        assert!(src.contains("<section class=\"detail-section"), "{src}");
        assert!(src.contains("Detailed body here."), "{src}");
        // Block content should NOT appear inside the inline `<dl>`.
        // (Negative pin: detail-field-description should not exist.)
        assert!(
            !src.contains("detail-field-description"),
            "block field leaked into inline dl in:\n{src}"
        );
    }

    #[test]
    fn html_detail_view_column_cell_arity_mismatch_returns_error() {
        let col = RenderColumn::new("subject", "Subject", ColumnKind::PrimaryLink);
        let columns = vec![col];
        let cells: Vec<CellSource<'_>> = vec![]; // intentional mismatch
        let r = render_detail(0x0102, "project_work_item", 1, "", "", &columns, &cells);
        assert!(r.is_err(), "expected mismatch to error, got Ok:\n{r:?}");
    }

    #[test]
    fn default_kind_resolver_is_wired_through_render_kit() {
        // Smoke: the resolver lib.rs re-exports is the one consumers
        // call to pick cell kinds. Pin the contract.
        assert_eq!(default_kind_for("id", None), ColumnKind::IdLink);
        assert_eq!(default_kind_for("subject", Some("string")), ColumnKind::PrimaryLink);
        assert_eq!(default_kind_for("done_ratio", None), ColumnKind::ProgressBar);
        assert_eq!(default_kind_for("estimated_hours", None), ColumnKind::Hours);
        assert_eq!(
            default_kind_for("description", Some("text")),
            ColumnKind::RichText
        );
        assert_eq!(default_kind_for("position", Some("integer")), ColumnKind::Plain);
    }

    // ── T4 (HtmlForm) tests ─────────────────────────────────────────

    #[test]
    fn html_form_proof_of_shape_renders_inputs_for_class_attributes() {
        // Codebook-only emit: every typed attribute on project_role lands
        // as a `<form>` field. Pins data-class-id / data-concept + the
        // per-attribute form-field wrappers.
        let class = project_role();
        let src = render(&class, ArtifactKind::HtmlForm).unwrap();
        assert!(
            src.contains("data-class-id=\"0x0117\""),
            "expected data-class-id=\"0x0117\" in:\n{src}"
        );
        assert!(src.contains("data-concept=\"project_role\""), "{src}");
        assert!(src.contains("<form"), "expected a <form> element:\n{src}");
        for attr in &class.attributes {
            assert!(
                src.contains(&format!("form-field-{}", attr.name)),
                "missing form-field-{} in:\n{src}",
                attr.name
            );
        }
        // Default proof-of-shape form uses POST + Create + no cancel.
        assert!(src.contains("method=\"post\""), "{src}");
        assert!(src.contains(">Create</button>"), "{src}");
    }

    #[test]
    fn html_form_dispatches_input_kinds_to_their_sub_templates() {
        // Build a form with explicit per-field InputData covering every
        // kind that maps to a distinct HTML element. Verify each
        // sub-template fires correctly.
        let cols = vec![
            RenderColumn::new("subject", "Subject", ColumnKind::PrimaryLink),
            RenderColumn::new("description", "Description", ColumnKind::RichText).block(),
            RenderColumn::new("estimated_hours", "Estimated h", ColumnKind::Hours),
            RenderColumn::new("done_ratio", "% Done", ColumnKind::ProgressBar),
            RenderColumn::new("is_private", "Private?", ColumnKind::Plain),
            RenderColumn::new("start_date", "Start", ColumnKind::Plain),
            RenderColumn::new("updated_at", "Updated", ColumnKind::Plain),
            RenderColumn::new("status_id", "Status", ColumnKind::IdLink),
            RenderColumn::new("id", "ID", ColumnKind::IdLink).frozen(),
        ];
        let fields = vec![
            FormFieldSource {
                column: &cols[0],
                css_classes: "",
                hint: "Short headline",
                data: InputData::Text {
                    value: "draft title".to_string(),
                    required: true,
                    placeholder: "Enter a subject".to_string(),
                },
            },
            FormFieldSource {
                column: &cols[1],
                css_classes: "",
                hint: "",
                data: InputData::TextArea {
                    value: "draft body".to_string(),
                    rows: 8,
                    required: false,
                    placeholder: String::new(),
                },
            },
            FormFieldSource {
                column: &cols[2],
                css_classes: "num",
                hint: "",
                data: InputData::Number {
                    value: "3".to_string(),
                    required: false,
                    step: "0.25".to_string(),
                },
            },
            FormFieldSource {
                column: &cols[3],
                css_classes: "",
                hint: "",
                data: InputData::Range {
                    value: "70".to_string(),
                    min: 0,
                    max: 100,
                    step: 10,
                    suffix: "%".to_string(),
                },
            },
            FormFieldSource {
                column: &cols[4],
                css_classes: "",
                hint: "",
                data: InputData::Checkbox { checked: true },
            },
            FormFieldSource {
                column: &cols[5],
                css_classes: "",
                hint: "",
                data: InputData::Date {
                    value: "2026-01-15".to_string(),
                    required: false,
                },
            },
            FormFieldSource {
                column: &cols[6],
                css_classes: "",
                hint: "",
                data: InputData::DateTime {
                    value: "2026-06-19T10:00".to_string(),
                    required: false,
                },
            },
            FormFieldSource {
                column: &cols[7],
                css_classes: "",
                hint: "",
                data: InputData::Select {
                    value: "2".to_string(),
                    required: false,
                    options: vec![
                        SelectOptionOwned { value: "1".to_string(), label: "New".to_string() },
                        SelectOptionOwned { value: "2".to_string(), label: "In Progress".to_string() },
                        SelectOptionOwned { value: "3".to_string(), label: "Closed".to_string() },
                    ],
                },
            },
            FormFieldSource {
                column: &cols[8],
                css_classes: "",
                hint: "",
                data: InputData::Hidden {
                    value: "42".to_string(),
                },
            },
        ];

        let src = render_form(
            0x0102,
            "project_work_item",
            &FormSource {
                method: "patch",
                action: "/issues/42",
                csrf_token: "xyz",
                record_id: Some(42),
                legend: "Edit issue",
                submit_label: "Save",
                cancel_label: "Cancel",
                cancel_href: "/issues/42",
                fields,
            },
        )
        .unwrap();

        // Header attrs
        assert!(src.contains("method=\"patch\""), "{src}");
        assert!(src.contains("action=\"/issues/42\""), "{src}");
        assert!(src.contains("name=\"authenticity_token\" value=\"xyz\""), "{src}");
        // record_id hidden field for the edit form
        assert!(src.contains("name=\"id\" value=\"42\""), "expected record-id hidden:\n{src}");
        assert!(src.contains("<legend>Edit issue</legend>"), "{src}");

        // Each input kind fires its own sub-template:
        assert!(src.contains("<input type=\"text\" name=\"subject\""), "{src}");
        assert!(src.contains("placeholder=\"Enter a subject\""), "{src}");
        assert!(src.contains("<textarea name=\"description\""), "{src}");
        assert!(src.contains("rows=\"8\""), "{src}");
        assert!(src.contains("<input type=\"number\" name=\"estimated_hours\""), "{src}");
        assert!(src.contains("step=\"0.25\""), "{src}");
        assert!(src.contains("<input type=\"range\" name=\"done_ratio\""), "{src}");
        assert!(src.contains("max=\"100\""), "{src}");
        assert!(src.contains("<input type=\"checkbox\" name=\"is_private\""), "{src}");
        // checkbox carries the hidden 0 sibling per Rails idiom
        assert!(src.contains("<input type=\"hidden\" name=\"is_private\" value=\"0\""), "{src}");
        assert!(src.contains("<input type=\"date\" name=\"start_date\""), "{src}");
        assert!(src.contains("<input type=\"datetime-local\" name=\"updated_at\""), "{src}");
        assert!(src.contains("<select name=\"status_id\""), "{src}");
        // selected option pinned
        assert!(src.contains("<option value=\"2\" selected>In Progress</option>"), "{src}");
        // hidden field has NO label wrapper; emitted bare
        assert!(src.contains("<input type=\"hidden\" name=\"id\" value=\"42\""), "{src}");
        assert!(!src.contains("form-field-id"), "hidden field should not have a wrapper:\n{src}");
        // The Subject field's hint goes through
        assert!(src.contains("Short headline"), "{src}");
        // Cancel + submit buttons
        assert!(src.contains(">Save</button>"), "{src}");
        assert!(src.contains("href=\"/issues/42\""), "{src}");
        assert!(src.contains(">Cancel</a>"), "{src}");
    }

    #[test]
    fn html_form_escapes_data_derived_strings_xss_regression() {
        // Same XSS regression as the spine fixes on PRs #83/#84:
        // labels, hints, action URL, legend, css_classes all escape.
        let col = RenderColumn::new(
            "subject",
            "<script>alert(1)</script>", // poisoned caption
            ColumnKind::PrimaryLink,
        );
        let fields = vec![FormFieldSource {
            column: &col,
            css_classes: "<bad-css>",
            hint: "<img src=x onerror=alert(2)>",
            data: InputData::Text {
                value: "<xss-value>".to_string(),
                required: false,
                placeholder: "<xss-placeholder>".to_string(),
            },
        }];
        let src = render_form(
            0x0102,
            "project_work_item",
            &FormSource {
                method: "post",
                action: "/<bad-action>",
                csrf_token: "<xss-csrf>",
                record_id: None,
                legend: "<script>alert('legend')</script>",
                submit_label: "<safe>Save</safe>",
                cancel_label: "Cancel",
                cancel_href: "",
                fields,
            },
        )
        .unwrap();
        // No raw scripts / images / dangling-tag css survive
        assert!(!src.contains("<script>alert(1)"), "label raw: {src}");
        assert!(!src.contains("<img src=x onerror"), "hint raw: {src}");
        assert!(!src.contains("<bad-css>"), "css raw: {src}");
        assert!(!src.contains("<xss-value>"), "value raw: {src}");
        assert!(!src.contains("<xss-placeholder>"), "placeholder raw: {src}");
        assert!(!src.contains("<script>alert('legend')"), "legend raw: {src}");
        assert!(!src.contains("<safe>Save</safe>"), "submit raw: {src}");
        // action attribute gets escaped (the `<` becomes `&lt;`)
        assert!(!src.contains("action=\"/<bad-action>\""), "action raw: {src}");
        assert!(src.contains("action=\"/&lt;bad-action&gt;\""), "expected escaped action:\n{src}");
    }

    #[test]
    fn input_kind_resolver_is_wired_through_render_kit() {
        // Smoke: the form-side resolver re-exported from lib.rs is the
        // one consumers call to pick input controls.
        assert_eq!(default_input_kind_for("id", None), InputKind::Hidden);
        assert_eq!(default_input_kind_for("done_ratio", None), InputKind::Range);
        assert_eq!(default_input_kind_for("description", Some("text")), InputKind::TextArea);
        assert_eq!(default_input_kind_for("position", Some("integer")), InputKind::Number);
        assert_eq!(default_input_kind_for("active", Some("boolean")), InputKind::Checkbox);
        assert_eq!(default_input_kind_for("start_date", Some("date")), InputKind::Date);
        assert_eq!(default_input_kind_for("updated_at", Some("datetime")), InputKind::DateTime);
        assert_eq!(default_input_kind_for("subject", Some("string")), InputKind::Text);
    }

    // ── T5 (SurrealqlTable) tests ───────────────────────────────────

    #[test]
    fn surrealql_table_emits_define_table_with_codebook_meta() {
        // Proof of shape: render project_work_item — verify the emitted
        // DDL declares `DEFINE TABLE project_work_item` + COMMENT with
        // the codebook id + at least one DEFINE FIELD per attribute.
        let class = project_work_item();
        let src = render(&class, ArtifactKind::SurrealqlTable).unwrap();
        assert!(
            src.contains("DEFINE TABLE project_work_item SCHEMAFULL"),
            "expected DEFINE TABLE in:\n{src}"
        );
        assert!(
            src.contains("COMMENT \"OGAR codebook id 0x0102 (project_work_item)\""),
            "expected codebook id COMMENT in:\n{src}"
        );
        for attr in &class.attributes {
            assert!(
                src.contains(&format!("DEFINE FIELD {} ON project_work_item", attr.name)),
                "missing DEFINE FIELD {} in:\n{src}",
                attr.name
            );
        }
    }

    #[test]
    fn surrealql_table_maps_rails_types_to_surql() {
        // Type mapping pin against project_role (which carries `name`
        // string, `position` integer, `permissions` text).
        let class = project_role();
        let src = render(&class, ArtifactKind::SurrealqlTable).unwrap();
        assert!(src.contains("DEFINE FIELD name ON project_role TYPE string"), "{src}");
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
    fn surrealql_table_emits_record_links_for_family_edges() {
        // billable_work_entry has 12 family edges. belongs_to / has_one
        // → `option<record<…>>`; has_many / habtm → `array<record<…>>`.
        let class = billable_work_entry();
        let src = render(&class, ArtifactKind::SurrealqlTable).unwrap();
        for edge in &class.associations {
            assert!(
                src.contains(&format!(
                    "DEFINE FIELD {} ON billable_work_entry TYPE",
                    edge.name
                )),
                "missing edge `{}`:\n{src}",
                edge.name
            );
        }
        // At least one belongs_to / has_one should appear as option<record<…>>
        assert!(
            src.contains("TYPE option<record<"),
            "expected at least one optional record link in:\n{src}"
        );
        // billable_work_entry's edges are mostly belongs_to; verify the
        // record-link target follows snake_case lowercasing of class_name.
        // billable_work_entry's `classified_by` edge → `TaxPolicy` →
        // snake_case `tax_policy` (or already-snake-case canonical name).
        assert!(
            src.contains("record<tax_policy>"),
            "expected snake_case'd record<tax_policy> target in:\n{src}"
        );
    }

    #[test]
    fn surrealql_table_emits_primary_label_index() {
        // Classes with a `name`/`subject`/`title`/`label` primary
        // attribute get a default non-unique index on it.
        let class = project_role();
        let src = render(&class, ArtifactKind::SurrealqlTable).unwrap();
        assert!(
            src.contains("DEFINE INDEX idx_name ON project_role FIELDS name"),
            "expected idx_name on project_role:\n{src}"
        );
    }

    #[test]
    fn surrealql_table_escapes_reserved_identifiers() {
        // SurrealQL reserves words like `type`, `value`, `id`, `for`, …
        // The emitter back-tick-quotes them in field names so the DDL
        // parses. project_actor declares an attribute literally named
        // `type`, exercising this.
        let class = project_actor();
        let src = render(&class, ArtifactKind::SurrealqlTable).unwrap();
        // The illegal bare form must NOT appear in field position.
        assert!(
            !src.contains("DEFINE FIELD type ON project_actor"),
            "bare reserved `type` field name — SurQL parse hazard:\n{src}"
        );
        // The back-tick-escaped form MUST appear.
        assert!(
            src.contains("DEFINE FIELD `type` ON project_actor"),
            "expected back-ticked `type` field:\n{src}"
        );
    }

    #[test]
    fn surrealql_table_default_indexes_only_when_class_has_primary_label() {
        // billable_work_entry has no `name`/`subject`/`title`/`label`
        // primary attribute, so no default index is emitted.
        let class = billable_work_entry();
        let src = render(&class, ArtifactKind::SurrealqlTable).unwrap();
        assert!(
            !src.contains("DEFINE INDEX idx_"),
            "billable_work_entry should not get a default primary-label index:\n{src}"
        );
    }

    #[test]
    fn rust_struct_escapes_keyword_attribute_names() {
        // Codex P1 on #78: `project_actor()` declares an attribute named
        // `type` (Rails STI convention). The naive template would emit
        // `pub type: String,` which is illegal Rust. The emitter must
        // raw-escape Rust reserved words so the output compiles.
        let class = project_actor();
        assert!(
            class.attributes.iter().any(|a| a.name == "type"),
            "regression precondition: project_actor must ship a `type` attribute"
        );
        let src = render(&class, ArtifactKind::RustStruct).unwrap();
        // The illegal form must NOT appear ...
        assert!(
            !src.contains("pub type:"),
            "rust_struct must not emit `pub type:` (illegal); got:\n{src}"
        );
        // ... and the raw-escaped form MUST appear.
        assert!(
            src.contains("pub r#type:"),
            "expected raw-escaped `pub r#type:` for the `type` attribute:\n{src}"
        );
    }

    #[test]
    fn render_all_walks_a_slice_of_classes() {
        let classes = vec![project(), project_work_item(), project_role()];
        let out = render_all(&classes, ArtifactKind::RustStruct).unwrap();
        assert_eq!(out.len(), 3);
        let concepts: Vec<&str> = out.iter().map(|(c, _)| c.as_str()).collect();
        assert!(concepts.contains(&"project"));
        assert!(concepts.contains(&"project_work_item"));
        assert!(concepts.contains(&"project_role"));
        for (_, src) in &out {
            assert!(src.contains("pub const CLASS_ID:"), "{src}");
        }
    }
}
