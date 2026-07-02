//! Per-[`ColumnKind`](crate::ColumnKind) cell renderers — one askama
//! sub-template per variant. Called from the list-view + detail-view
//! emitters at row-build time to pre-render each cell's body, so the
//! spine templates just emit `{{ cell.body_html|safe }}` with no
//! runtime polymorphism.
//!
//! [`render_cell_body`] is the shared dispatch entry point T2 / T3 / T4
//! all call (factored when T4 became the third caller — see Northstar
//! plan §1.6: "templates are mass-mail simple; the *binding-struct* is
//! the bag of variables; the dispatch is a Rust `match`").
//!
//! The catalog mirrors Redmine `queries_helper::column_value`'s 12-arm
//! case (see `docs/integration/REDMINE-QUERY-HARVEST.md` §1.4).

use askama::Template;

use super::html_list_view::CellData;

// ── Plain ────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/plain.askama", escape = "html")]
pub(crate) struct PlainCell<'a> {
    pub value: &'a str,
}

// ── IdLink ───────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/id_link.askama", escape = "html")]
pub(crate) struct IdLinkCell<'a> {
    pub id: u64,
    pub href: &'a str,
}

// ── PrimaryLink ──────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/primary_link.askama", escape = "html")]
pub(crate) struct PrimaryLinkCell<'a> {
    pub label: &'a str,
    pub href: &'a str,
}

// ── RecordRef ────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/record_ref.askama", escape = "html")]
pub(crate) struct RecordRefCell<'a> {
    pub label: &'a str,
    pub href: &'a str,
    /// Canonical concept name of the target (e.g. `"project"`); surfaced
    /// in the `title` attribute for accessibility.
    pub target_concept: &'a str,
}

// ── RichText ─────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/rich_text.askama", escape = "none")]
pub(crate) struct RichTextCell<'a> {
    /// Already-rendered prose HTML (markdown / textile expanded
    /// upstream; this template just wraps it in `.wiki`).
    pub body: &'a str,
}

// ── ProgressBar ──────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/progress_bar.askama", escape = "html")]
pub(crate) struct ProgressBarCell {
    pub pct: u8,
}

// ── RelationList ─────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/relation_list.askama", escape = "html")]
pub(crate) struct RelationListCell<'a> {
    pub relations: Vec<RelationEntry<'a>>,
}

pub(crate) struct RelationEntry<'a> {
    pub id: u64,
    pub kind: &'a str,
    pub href: &'a str,
}

// ── Hours ────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/hours.askama", escape = "html")]
pub(crate) struct HoursCell<'a> {
    pub hours: &'a str,
    /// Optional link to the underlying time entries (Redmine's
    /// `:spent_hours` links to the report); empty if unlinked.
    pub href: &'a str,
}

// ── AttachmentList ───────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/attachment_list.askama", escape = "html")]
pub(crate) struct AttachmentListCell<'a> {
    pub attachments: Vec<AttachmentEntry<'a>>,
}

pub(crate) struct AttachmentEntry<'a> {
    pub filename: &'a str,
    pub href: &'a str,
}

// ── UserList ─────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/cell/user_list.askama", escape = "html")]
pub(crate) struct UserListCell<'a> {
    pub users: Vec<UserEntry<'a>>,
}

pub(crate) struct UserEntry<'a> {
    pub name: &'a str,
    pub href: &'a str,
}

// ── Shared dispatch — one place T2 / T3 / T4 call ─────────────────────

/// Pre-render a [`CellData`] value into its HTML body via the matching
/// per-kind sub-template. The spine templates (`html_list_view`,
/// `html_detail_view`, `html_form`'s read-only fallback paths) consume
/// the resulting string with `|safe` because the sub-templates already
/// escape their own variables.
///
/// Factored from the duplicated `render_cell_body` helpers in T2 and T3
/// when T4 became the third caller. Northstar plan §1.6: three points
/// form a line.
pub(crate) fn render_cell_body(data: &CellData<'_>) -> Result<String, askama::Error> {
    Ok(match data {
        CellData::Plain { value } => PlainCell { value }.render()?,
        CellData::IdLink { id, href } => IdLinkCell { id: *id, href }.render()?,
        CellData::PrimaryLink { label, href } => PrimaryLinkCell { label, href }.render()?,
        CellData::RecordRef {
            label,
            href,
            target_concept,
        } => RecordRefCell {
            label,
            href,
            target_concept,
        }
        .render()?,
        CellData::RichText { body } => RichTextCell { body }.render()?,
        CellData::ProgressBar { pct } => ProgressBarCell { pct: *pct }.render()?,
        CellData::RelationList { relations } => {
            let mapped: Vec<RelationEntry<'_>> = relations
                .iter()
                .map(|r| RelationEntry {
                    id: r.id,
                    kind: r.kind.as_str(),
                    href: r.href.as_str(),
                })
                .collect();
            RelationListCell { relations: mapped }.render()?
        }
        CellData::Hours { hours, href } => HoursCell { hours, href }.render()?,
        CellData::AttachmentList { attachments } => {
            let mapped: Vec<AttachmentEntry<'_>> = attachments
                .iter()
                .map(|a| AttachmentEntry {
                    filename: a.filename.as_str(),
                    href: a.href.as_str(),
                })
                .collect();
            AttachmentListCell { attachments: mapped }.render()?
        }
        CellData::UserList { users } => {
            let mapped: Vec<UserEntry<'_>> = users
                .iter()
                .map(|u| UserEntry {
                    name: u.name.as_str(),
                    href: u.href.as_str(),
                })
                .collect();
            UserListCell { users: mapped }.render()?
        }
    })
}
