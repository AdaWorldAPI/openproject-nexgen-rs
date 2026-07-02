//! `HtmlListView` emitter — T2 per Northstar plan §3, modelled on
//! Redmine's `app/views/issues/_list.html.erb` (the harvest doc captures
//! the design rationale).
//!
//! Substrate-agnostic: nothing in the template knows it's rendering
//! issues vs projects vs users. The columns + rows are pre-shaped by the
//! Rust side; the template iterates and substitutes.
//!
//! # Two-stage rendering
//!
//! Cells are **pre-rendered in Rust** at row-build time using
//! per-[`ColumnKind`] sub-templates from [`super::cells`]. The spine
//! template (`html_list_view.askama`) just emits `{{ cell.body_html|safe }}`
//! — no runtime polymorphism, no dispatch in the template. The
//! per-`ColumnKind` template choice is a `match` in Rust, askama's
//! compile-time check applies to every cell binding individually.

use askama::Template;

use super::ArtifactEmitter;
use crate::list_view::{ColumnKind, RenderColumn};
use crate::spec::ArtifactSpec;
use ogar_vocab::canonical_concept_id;

// ── Spine binding struct ─────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/html_list_view.askama", escape = "html")]
struct HtmlListViewCtx {
    title: String,
    class_id_hex: String,
    canonical_concept: String,
    inline_columns: Vec<ColumnHeader>,
    /// Block columns aren't iterated in the header (they span the row);
    /// kept so the emitter knows which slots to surface as block cells.
    /// The template uses `inline_columns.len()` for the colspan only.
    rows: Vec<HtmlRow>,
}

struct ColumnHeader {
    name: String,
    caption: String,
    sortable: bool,
    frozen: bool,
    /// Pre-stringified so the template doesn't have to call `.as_str()`
    /// on the `SortOrder` enum.
    default_order_str: String,
}

struct HtmlRow {
    record_id: u64,
    css_classes: String,
    /// Optional group separator label. Empty when this row is not the
    /// first row of a new group.
    group_header_label: String,
    group_header_count: u32,
    inline_cells: Vec<RenderedCell>,
    block_cells: Vec<RenderedCell>,
}

struct RenderedCell {
    /// Column name (predicate IRI). Surfaces as `col-<name>` on the `<td>`.
    name: String,
    css_classes: String,
    /// The cell's body — already rendered HTML.
    body_html: String,
}

// ── Cell-source input the emitter expects per (column, row) ──────────

/// One column×row datum the emitter formats through the right
/// [`ColumnKind`] sub-template. The caller fills these from their own
/// data layer; the emitter handles the askama dispatch.
#[derive(Debug, Clone)]
pub struct CellSource<'a> {
    /// The column being rendered.
    pub column: &'a RenderColumn,
    /// CSS classes to add to the `<td>` (or `block_column` row) — e.g.
    /// `"num"` for right-aligned numerics. Defaults to empty.
    pub css_classes: &'a str,
    /// The raw data for the cell. Per-kind variant determines the
    /// per-template binding shape.
    pub data: CellData<'a>,
}

/// Per-kind data the caller supplies for a cell. Variants are keyed by
/// the column's [`ColumnKind`]; the emitter dispatches to the matching
/// askama sub-template at row-build time.
#[derive(Debug, Clone)]
#[allow(missing_docs)] // self-describing variants; fields documented inline.
pub enum CellData<'a> {
    /// Plain text fallback — Redmine `format_object`.
    Plain { value: &'a str },
    /// Numeric id rendered as a `#<n>` link.
    IdLink { id: u64, href: &'a str },
    /// Primary headline link (the row's main column).
    PrimaryLink { label: &'a str, href: &'a str },
    /// Family-edge reference rendered as a link to the target record.
    RecordRef {
        label: &'a str,
        href: &'a str,
        target_concept: &'a str,
    },
    /// Long-form prose — typically rendered as a block-row cell.
    RichText { body: &'a str },
    /// Percentage rendered as a progress bar.
    ProgressBar { pct: u8 },
    /// List of `project_relation` refs.
    RelationList { relations: Vec<RelationEntryOwned> },
    /// Formatted duration (hours / decimal).
    Hours { hours: &'a str, href: &'a str },
    /// List of `project_attachment` refs.
    AttachmentList { attachments: Vec<AttachmentEntryOwned> },
    /// List of `project_actor` refs (watchers, assignees, …).
    UserList { users: Vec<UserEntryOwned> },
}

/// Owned form of one entry in a `RelationList` cell.
#[derive(Debug, Clone)]
pub struct RelationEntryOwned {
    /// Target record id.
    pub id: u64,
    /// Relation kind (e.g. `"blocks"`, `"duplicates"`).
    pub kind: String,
    /// Link to the related record.
    pub href: String,
}

/// Owned form of one entry in an `AttachmentList` cell.
#[derive(Debug, Clone)]
pub struct AttachmentEntryOwned {
    /// Attachment filename as displayed.
    pub filename: String,
    /// Download link.
    pub href: String,
}

/// Owned form of one entry in a `UserList` cell.
#[derive(Debug, Clone)]
pub struct UserEntryOwned {
    /// Display name.
    pub name: String,
    /// Link to the user's detail view.
    pub href: String,
}

/// One row's worth of source cells + the row's identifying meta.
#[derive(Debug, Clone)]
pub struct RowSource<'a> {
    /// Record id (the canonical row identifier in the source).
    pub record_id: u64,
    /// CSS classes to add to the row's `<tr>` element.
    pub css_classes: &'a str,
    /// Group-separator data — `Some` only on the first row of a new
    /// group. The label is the bucket value; count is the row count in
    /// the group (0 → no badge).
    pub group: Option<GroupHeader<'a>>,
    /// One entry per inline column, in column order.
    pub inline: Vec<CellSource<'a>>,
    /// One entry per block column, in column order.
    pub block: Vec<CellSource<'a>>,
}

/// Group-separator data for the first row of a new group.
#[derive(Debug, Clone, Copy)]
pub struct GroupHeader<'a> {
    /// The group's bucket value, used as the separator label.
    pub label: &'a str,
    /// How many rows are in this group (`0` → no badge rendered).
    pub count: u32,
}

// ── The emitter ──────────────────────────────────────────────────────

/// Render the spine list template against a column set + row stream.
///
/// Lower-level than the [`ArtifactEmitter::emit`] entry point: takes
/// pre-shaped row sources. Use this when you have your own rows
/// (typical case); [`HtmlListViewEmitter::emit`] is the codebook-only
/// path used by the +5 kit's tests, which renders an empty list as
/// proof of shape.
pub fn render_list(
    title: &str,
    class_id: u16,
    canonical_concept: &str,
    inline_columns: &[RenderColumn],
    block_columns: &[RenderColumn],
    rows: &[RowSource<'_>],
) -> Result<String, askama::Error> {
    let inline_headers: Vec<ColumnHeader> = inline_columns
        .iter()
        .map(|c| ColumnHeader {
            name: c.name.clone(),
            caption: c.caption.clone(),
            sortable: c.sortable,
            frozen: c.frozen,
            default_order_str: c.default_order.as_str().to_string(),
        })
        .collect();

    let html_rows: Vec<HtmlRow> = rows
        .iter()
        .map(|r| HtmlRow {
            record_id: r.record_id,
            css_classes: r.css_classes.to_string(),
            group_header_label: r.group.map(|g| g.label.to_string()).unwrap_or_default(),
            group_header_count: r.group.map(|g| g.count).unwrap_or(0),
            inline_cells: r
                .inline
                .iter()
                .map(|c| render_cell(c))
                .collect::<Result<_, _>>()
                .unwrap_or_default(),
            block_cells: r
                .block
                .iter()
                .map(|c| render_cell(c))
                .collect::<Result<_, _>>()
                .unwrap_or_default(),
        })
        .collect();
    let _ = block_columns; // schema documented; block detection is by per-row block cells

    HtmlListViewCtx {
        title: title.to_string(),
        class_id_hex: format!("0x{class_id:04X}"),
        canonical_concept: canonical_concept.to_string(),
        inline_columns: inline_headers,
        rows: html_rows,
    }
    .render()
}

fn render_cell(src: &CellSource<'_>) -> Result<RenderedCell, askama::Error> {
    // Shared dispatch — see `cells::render_cell_body`. T2/T3/T4 all call
    // the same helper; this function just wraps the body into the
    // RenderedCell record T2's spine template expects.
    let body = super::cells::render_cell_body(&src.data)?;
    Ok(RenderedCell {
        name: src.column.name.clone(),
        css_classes: src.css_classes.to_string(),
        body_html: body,
    })
}

/// The codebook-only dispatch entry point (used by [`for_kind`](super::for_kind)).
/// Renders the class's canonical fields as headers but no rows — proof
/// of shape for the +5 kit's pipeline. Real callers use [`render_list`]
/// directly with row data.
pub struct HtmlListViewEmitter;

impl ArtifactEmitter for HtmlListViewEmitter {
    fn emit(&self, spec: &ArtifactSpec<'_>) -> Result<String, askama::Error> {
        use crate::list_view::default_kind_for;
        let class = spec.class;
        let concept = class.canonical_concept.as_deref().unwrap_or("");
        let class_id = canonical_concept_id(concept).unwrap_or(0);

        // Synthesise a column set from the class's attributes (inline) +
        // a small set of block columns for prose-heavy ones. This is the
        // "proof of shape" view used by tests / a future xtask preview.
        let mut inline = Vec::new();
        let mut block = Vec::new();
        for attr in &class.attributes {
            let kind = default_kind_for(&attr.name, attr.type_name.as_deref());
            let col = RenderColumn::new(&attr.name, &attr.name, kind).sortable();
            if matches!(kind, ColumnKind::RichText) {
                block.push(col.block());
            } else {
                inline.push(col);
            }
        }
        render_list(class.name.as_str(), class_id, concept, &inline, &block, &[])
    }
}
