//! `HtmlDetailView` emitter — T3 per Northstar plan §3, the detail-page
//! sibling of T2's [`html_list_view`](super::html_list_view).
//!
//! Reuses every piece T2 built (`RenderColumn`, `ColumnKind`, the cell
//! sub-templates from [`super::cells`], the two-stage cells-pre-rendered-
//! in-Rust pattern). The difference is the spine: a `<dl>` of inline
//! fields + `<section>` blocks for prose / family-edge collections,
//! instead of a `<table>` of rows.
//!
//! Mirror of Redmine's `app/views/issues/show.html.erb` shape — same
//! field-by-field laydown, with the family-edge sections (subtasks,
//! relations, attachments, watchers, journals) as sibling `<section>`s.

use askama::Template;

use super::html_list_view::{CellData, CellSource};
use super::ArtifactEmitter;
use crate::list_view::{ColumnKind, RenderColumn};
use crate::spec::ArtifactSpec;
use ogar_vocab::canonical_concept_id;

// ── Spine binding struct ─────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/html_detail_view.askama", escape = "html")]
struct HtmlDetailViewCtx {
    class_id_hex: String,
    canonical_concept: String,
    record_id: u64,
    /// Pre-rendered headline (typically the primary-link or plain-text
    /// for the record's primary attribute). Empty means no headline.
    headline_html: String,
    /// Optional subtitle line (e.g. status + priority badges); empty
    /// for none.
    subtitle: String,
    inline_cells: Vec<DetailField>,
    block_sections: Vec<DetailSection>,
}

struct DetailField {
    name: String,
    label: String,
    css_classes: String,
    body_html: String,
}

struct DetailSection {
    name: String,
    label: String,
    css_classes: String,
    body_html: String,
}

// ── Public entry point ──────────────────────────────────────────────

/// Render one canonical record as a detail page (definition-list inline
/// fields + section blocks for prose + family-edge collections).
///
/// `columns` carries both `inline=true` (laid out as `<dt>/<dd>`) and
/// `inline=false` (rendered as full-width `<section>`s). The classifier
/// is done at render time from `column.inline`.
///
/// `cells` is one entry per column in the same order — paired by index.
/// `headline_html` is the (already-rendered) primary-link / plain text
/// the page header shows; empty string for none.
#[allow(clippy::too_many_arguments)]
pub fn render_detail(
    class_id: u16,
    canonical_concept: &str,
    record_id: u64,
    headline_html: &str,
    subtitle: &str,
    columns: &[RenderColumn],
    cells: &[CellSource<'_>],
) -> Result<String, askama::Error> {
    if columns.len() != cells.len() {
        // Hard caller-side bug; surface it loudly. Tests pin this contract.
        return Err(askama::Error::from(std::fmt::Error));
    }

    let mut inline_cells = Vec::new();
    let mut block_sections = Vec::new();
    for (col, src) in columns.iter().zip(cells.iter()) {
        let body = render_cell_body(src)?;
        if col.inline {
            inline_cells.push(DetailField {
                name: col.name.clone(),
                label: col.caption.clone(),
                css_classes: src.css_classes.to_string(),
                body_html: body,
            });
        } else {
            block_sections.push(DetailSection {
                name: col.name.clone(),
                label: col.caption.clone(),
                css_classes: src.css_classes.to_string(),
                body_html: body,
            });
        }
    }

    HtmlDetailViewCtx {
        class_id_hex: format!("0x{class_id:04X}"),
        canonical_concept: canonical_concept.to_string(),
        record_id,
        headline_html: headline_html.to_string(),
        subtitle: subtitle.to_string(),
        inline_cells,
        block_sections,
    }
    .render()
}

/// Pre-render this cell's body via the shared per-kind dispatch.
/// See [`super::cells::render_cell_body`]; factored when T4 became the
/// third caller (T2 list-view, T3 detail-view, T4 form-view fallback).
fn render_cell_body(src: &CellSource<'_>) -> Result<String, askama::Error> {
    super::cells::render_cell_body(&src.data)
}

/// The codebook-only dispatch entry point used by [`for_kind`](super::for_kind).
/// Synthesises a no-data detail page from the class's attributes (proof
/// of shape); real callers use [`render_detail`] with real cell data.
pub struct HtmlDetailViewEmitter;

impl ArtifactEmitter for HtmlDetailViewEmitter {
    fn emit(&self, spec: &ArtifactSpec<'_>) -> Result<String, askama::Error> {
        use crate::list_view::default_kind_for;
        let class = spec.class;
        let concept = class.canonical_concept.as_deref().unwrap_or("");
        let class_id = canonical_concept_id(concept).unwrap_or(0);

        // Synthesise columns from the class's attributes.
        let columns: Vec<RenderColumn> = class
            .attributes
            .iter()
            .map(|attr| {
                let kind = default_kind_for(&attr.name, attr.type_name.as_deref());
                let col = RenderColumn::new(&attr.name, &attr.name, kind);
                if matches!(kind, ColumnKind::RichText) {
                    col.block()
                } else {
                    col
                }
            })
            .collect();

        // Placeholder values, kept alive across the render. One per column.
        let placeholders: Vec<String> = columns.iter().map(|_| "—".to_string()).collect();

        // Pair columns with placeholder cells.
        let cells: Vec<CellSource<'_>> = columns
            .iter()
            .zip(placeholders.iter())
            .map(|(col, p)| {
                let data = if matches!(col.kind, ColumnKind::RichText) {
                    CellData::RichText { body: p.as_str() }
                } else {
                    CellData::Plain { value: p.as_str() }
                };
                CellSource {
                    column: col,
                    css_classes: "muted",
                    data,
                }
            })
            .collect();

        render_detail(class_id, concept, 0, "", "", &columns, &cells)
    }
}
