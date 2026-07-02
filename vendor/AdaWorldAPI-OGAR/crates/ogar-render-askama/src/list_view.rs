//! Shared types for the `HtmlListView` emitter (T2 per Northstar §3).
//!
//! Lifted from the Redmine Query/QueryColumn harvest
//! (`docs/integration/REDMINE-QUERY-HARVEST.md` §3). [`RenderColumn`]
//! mirrors `QueryColumn`'s 8 properties one-to-one; [`ColumnKind`] mirrors
//! `column_value`'s case-statement dispatch as a closed enum (one askama
//! sub-template per variant).
//!
//! Per Northstar plan §1.3 (`Class` carries types; `ClassView` carries
//! labels): the render-meta on `RenderColumn` is *presentation intent* —
//! it lives in this binding-struct sidecar, NOT on the contract
//! `FieldRef`. Anti-pattern #2 (the bicycle) territory if it migrated up.

/// One column in a tabular list view — Redmine's `QueryColumn` mapped
/// onto our substrate (canonical concept fields + presentation meta).
///
/// 8 properties to match the Redmine model. `name` and `caption` are the
/// only fields a minimal column needs; the rest tune presentation.
#[derive(Debug, Clone)]
pub struct RenderColumn {
    /// Field name on the underlying record (predicate IRI for the
    /// canonical layer; matches `FieldRef::predicate_iri`).
    pub name: String,
    /// Display label for the column header.
    pub caption: String,
    /// Cell-formatter dispatch — which `dispatch/cell/*.askama`
    /// sub-template renders this column's body.
    pub kind: ColumnKind,
    /// Whether the column header carries a sort link.
    pub sortable: bool,
    /// Whether the column can be used as the group-by axis.
    pub groupable: bool,
    /// Whether per-group / overall totals are meaningful (numeric kinds).
    pub totalable: bool,
    /// `true` → `<td>` in the row; `false` → `<tr class="block_column">`
    /// that spans the full row width (description, last_notes, …).
    pub inline: bool,
    /// Always visible regardless of user's column selection (typically
    /// the id / primary-link column).
    pub frozen: bool,
    /// Initial sort direction when this column becomes the active sort.
    pub default_order: SortOrder,
}

impl RenderColumn {
    /// Conservative default: a plain text inline column, no sort/group/
    /// total, ascending. Use builder-style setters to refine.
    #[must_use]
    pub fn new(name: impl Into<String>, caption: impl Into<String>, kind: ColumnKind) -> Self {
        Self {
            name: name.into(),
            caption: caption.into(),
            kind,
            sortable: false,
            groupable: false,
            totalable: false,
            inline: true,
            frozen: false,
            default_order: SortOrder::Asc,
        }
    }

    /// Mark this column as sortable, defaulting to ascending order.
    #[must_use]
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    /// Mark this column as a candidate group-by axis.
    #[must_use]
    pub fn groupable(mut self) -> Self {
        self.groupable = true;
        self
    }

    /// Mark this column as totalable (numeric aggregation).
    #[must_use]
    pub fn totalable(mut self) -> Self {
        self.totalable = true;
        self
    }

    /// Promote to a block column (spans the full row width, below the
    /// inline cells). Used for prose-heavy fields like description.
    #[must_use]
    pub fn block(mut self) -> Self {
        self.inline = false;
        self
    }

    /// Mark this column as frozen — always rendered regardless of the
    /// user's column selection.
    #[must_use]
    pub fn frozen(mut self) -> Self {
        self.frozen = true;
        self
    }

    /// Override the default sort order applied when the column first
    /// becomes the active sort.
    #[must_use]
    pub fn default_order(mut self, order: SortOrder) -> Self {
        self.default_order = order;
        self
    }
}

/// Cell-formatter dispatch. One askama sub-template per variant
/// (`dispatch/cell/<name>.askama`). **Append-only** — new variants are
/// added at the end; existing variants are never reordered or repurposed
/// (downstream consumers may have stored selections keyed on names).
///
/// Lifted from Redmine `column_value`'s 12-arm `case` (see
/// `queries_helper.rb`). Maps each formatter to a typed enum so the
/// emitter dispatches in Rust (zero polymorphism on the askama side).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnKind {
    /// Plain text — `format_object` fallback in Redmine. Default kind
    /// for unspecialised attributes.
    Plain,
    /// The record's canonical id rendered as a `#<n>` link to its detail
    /// view. Redmine: `when :id then link_to value, issue_path(item)`.
    IdLink,
    /// The primary-label column rendered as a link to the detail view —
    /// the "headline" column (Redmine's `:subject`).
    PrimaryLink,
    /// A family-edge reference rendered as a link to the target record
    /// (Redmine's `:parent` arm, `link_to_issue` calls).
    RecordRef,
    /// Long-form prose with markup (Markdown/Textile) — typically used in
    /// block columns. Redmine: `:description`, `:last_notes`.
    RichText,
    /// Percentage-as-progress-bar. Redmine: `:done_ratio`.
    ProgressBar,
    /// List of `project_relation` links — render-time scan of incoming /
    /// outgoing relations.
    RelationList,
    /// Formatted duration in hours / decimal. Redmine: `:estimated_hours`,
    /// `:spent_hours`, `:total_*_hours`.
    Hours,
    /// List of `project_attachment` refs.
    AttachmentList,
    /// List of `project_actor` refs (watchers, assignees).
    UserList,
}

impl ColumnKind {
    /// Short stable name used as the sub-template filename stem
    /// (`dispatch/cell/<name>.askama`). Append-only — never renamed.
    pub fn template_stem(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::IdLink => "id_link",
            Self::PrimaryLink => "primary_link",
            Self::RecordRef => "record_ref",
            Self::RichText => "rich_text",
            Self::ProgressBar => "progress_bar",
            Self::RelationList => "relation_list",
            Self::Hours => "hours",
            Self::AttachmentList => "attachment_list",
            Self::UserList => "user_list",
        }
    }
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortOrder {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

impl SortOrder {
    /// "asc" / "desc" — matches URL query-string conventions.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// Resolve a sensible default [`ColumnKind`] from a field's name + curator
/// type name. Mirrors the structural defaults Redmine's `column_value`
/// `case` encodes; consumers can override per-column at binding-struct
/// build time.
///
/// Conventions:
/// - `id` / `*_id` → [`ColumnKind::IdLink`].
/// - canonical primary_label name (`name` / `subject` / `title`) →
///   [`ColumnKind::PrimaryLink`].
/// - `done_ratio` / `*_ratio` / `*_pct` → [`ColumnKind::ProgressBar`].
/// - `*_hours` → [`ColumnKind::Hours`].
/// - Rails `text` type with prose-shaped name → [`ColumnKind::RichText`].
/// - everything else → [`ColumnKind::Plain`].
#[must_use]
pub fn default_kind_for(name: &str, type_name: Option<&str>) -> ColumnKind {
    let n = name;
    if n == "id" || n.ends_with("_id") {
        return ColumnKind::IdLink;
    }
    if matches!(n, "name" | "subject" | "title" | "label") {
        return ColumnKind::PrimaryLink;
    }
    if n == "done_ratio" || n.ends_with("_ratio") || n.ends_with("_pct") {
        return ColumnKind::ProgressBar;
    }
    if n.ends_with("_hours") || n == "hours" {
        return ColumnKind::Hours;
    }
    if matches!(type_name, Some("text"))
        && (n.contains("description")
            || n.contains("notes")
            || n.contains("comment")
            || n.contains("body"))
    {
        return ColumnKind::RichText;
    }
    ColumnKind::Plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_column_defaults_are_conservative() {
        let c = RenderColumn::new("subject", "Subject", ColumnKind::Plain);
        assert!(!c.sortable);
        assert!(!c.groupable);
        assert!(!c.totalable);
        assert!(c.inline);
        assert!(!c.frozen);
        assert_eq!(c.default_order, SortOrder::Asc);
    }

    #[test]
    fn render_column_builder_composes() {
        let c = RenderColumn::new("id", "#", ColumnKind::IdLink)
            .sortable()
            .frozen()
            .default_order(SortOrder::Desc);
        assert!(c.sortable && c.frozen);
        assert_eq!(c.default_order, SortOrder::Desc);
    }

    #[test]
    fn column_kind_template_stems_are_stable() {
        // The stem IS the filename — must never change once shipped.
        assert_eq!(ColumnKind::Plain.template_stem(), "plain");
        assert_eq!(ColumnKind::IdLink.template_stem(), "id_link");
        assert_eq!(ColumnKind::PrimaryLink.template_stem(), "primary_link");
        assert_eq!(ColumnKind::ProgressBar.template_stem(), "progress_bar");
        assert_eq!(ColumnKind::Hours.template_stem(), "hours");
        assert_eq!(ColumnKind::RichText.template_stem(), "rich_text");
    }

    #[test]
    fn default_kind_picks_link_for_id() {
        assert_eq!(default_kind_for("id", None), ColumnKind::IdLink);
        assert_eq!(default_kind_for("project_id", None), ColumnKind::IdLink);
    }

    #[test]
    fn default_kind_picks_primary_link_for_headline_names() {
        for n in ["name", "subject", "title", "label"] {
            assert_eq!(
                default_kind_for(n, Some("string")),
                ColumnKind::PrimaryLink,
                "{n} should be primary link"
            );
        }
    }

    #[test]
    fn default_kind_picks_progress_bar_for_ratios() {
        assert_eq!(default_kind_for("done_ratio", None), ColumnKind::ProgressBar);
        assert_eq!(default_kind_for("complete_pct", None), ColumnKind::ProgressBar);
    }

    #[test]
    fn default_kind_picks_hours_for_durations() {
        assert_eq!(default_kind_for("estimated_hours", None), ColumnKind::Hours);
        assert_eq!(default_kind_for("spent_hours", None), ColumnKind::Hours);
    }

    #[test]
    fn default_kind_picks_rich_text_only_for_text_typed_prose_names() {
        assert_eq!(default_kind_for("description", Some("text")), ColumnKind::RichText);
        assert_eq!(default_kind_for("last_notes", Some("text")), ColumnKind::RichText);
        // Same name with a non-text type stays Plain — the type gates it.
        assert_eq!(default_kind_for("description", Some("string")), ColumnKind::Plain);
        // Text type with non-prose name stays Plain — the name gates it.
        assert_eq!(default_kind_for("status_label", Some("text")), ColumnKind::Plain);
    }

    #[test]
    fn default_kind_falls_back_to_plain() {
        assert_eq!(default_kind_for("position", Some("integer")), ColumnKind::Plain);
        assert_eq!(default_kind_for("created_at", Some("datetime")), ColumnKind::Plain);
    }
}
