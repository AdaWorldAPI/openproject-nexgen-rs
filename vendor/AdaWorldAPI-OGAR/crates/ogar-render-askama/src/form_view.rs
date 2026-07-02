//! Form-input kind enum + resolver — the form-side companion to
//! [`crate::list_view::ColumnKind`]. While `ColumnKind` says how to
//! *display* a slot (link / progress bar / etc.), [`InputKind`] says
//! what HTML `<input>` control collects the user's edit for that slot.
//!
//! Used by T4 ([`super::artifact_kinds::html_form`]). Append-only enum
//! per Northstar §1.6 — variant order is the stable contract.

/// HTML form-control dispatch. One askama sub-template per variant
/// (`dispatch/input/<name>.askama`). **Append-only** — new variants land
/// at the end; existing variants never reorder or repurpose.
///
/// Maps onto Rails-side type names + canonical-concept slot semantics:
/// a `name: string` attribute renders as `<input type="text">`; a
/// `done_ratio: integer` (when the column says it's a percentage)
/// renders as `<input type="range">`; a `description: text` slot
/// renders as `<textarea>`; etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputKind {
    /// `<input type="text">` — short string (default for `string`-typed
    /// attributes and any unrecognised type).
    Text,
    /// `<textarea>` — long text (Rails `text`, RichText display kind).
    TextArea,
    /// `<input type="number">` — integer / decimal.
    Number,
    /// `<input type="range" min=0 max=100>` — percentage / `done_ratio`.
    Range,
    /// `<input type="checkbox">` — boolean.
    Checkbox,
    /// `<input type="date">` — ISO date (Rails `date`).
    Date,
    /// `<input type="datetime-local">` — Rails `datetime` / `timestamp`.
    DateTime,
    /// `<select>` of related-record options — the form-side companion of
    /// [`crate::list_view::ColumnKind::RecordRef`].
    Select,
    /// `<input type="hidden">` — id-likes and frozen slots that the
    /// caller wants to round-trip without showing.
    Hidden,
}

impl InputKind {
    /// Short stable name used as the sub-template filename stem
    /// (`dispatch/input/<name>.askama`). Append-only — never renamed.
    pub fn template_stem(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::TextArea => "textarea",
            Self::Number => "number",
            Self::Range => "range",
            Self::Checkbox => "checkbox",
            Self::Date => "date",
            Self::DateTime => "datetime",
            Self::Select => "select",
            Self::Hidden => "hidden",
        }
    }

    /// The `<input type=…>` attribute (for the variants that render a
    /// single `<input>` element). `None` for variants that render a
    /// different element (TextArea → `<textarea>`, Select → `<select>`).
    pub fn html_input_type(self) -> Option<&'static str> {
        match self {
            Self::Text => Some("text"),
            Self::Number => Some("number"),
            Self::Range => Some("range"),
            Self::Checkbox => Some("checkbox"),
            Self::Date => Some("date"),
            Self::DateTime => Some("datetime-local"),
            Self::Hidden => Some("hidden"),
            Self::TextArea | Self::Select => None,
        }
    }
}

/// Resolve a sensible default [`InputKind`] from a field's name + curator
/// type name. Companion to
/// [`crate::list_view::default_kind_for`] but for form inputs.
///
/// Conventions:
/// - `id` / `*_id` → [`InputKind::Hidden`] (id round-trips without UI).
/// - `done_ratio` / `*_ratio` / `*_pct` → [`InputKind::Range`].
/// - Rails `text` type with prose-shaped name → [`InputKind::TextArea`].
/// - Rails `integer` / `big_integer` / `bigint` / `float` / `decimal` / `monetary` → [`InputKind::Number`].
/// - Rails `boolean` → [`InputKind::Checkbox`].
/// - Rails `date` → [`InputKind::Date`].
/// - Rails `datetime` / `timestamp` → [`InputKind::DateTime`].
/// - everything else (default) → [`InputKind::Text`].
#[must_use]
pub fn default_input_kind_for(name: &str, type_name: Option<&str>) -> InputKind {
    let n = name;
    if n == "id" || n.ends_with("_id") {
        return InputKind::Hidden;
    }
    if n == "done_ratio" || n.ends_with("_ratio") || n.ends_with("_pct") {
        return InputKind::Range;
    }
    if matches!(type_name, Some("text"))
        && (n.contains("description")
            || n.contains("notes")
            || n.contains("comment")
            || n.contains("body"))
    {
        return InputKind::TextArea;
    }
    match type_name {
        Some("text") => InputKind::TextArea,
        Some("integer") | Some("big_integer") | Some("bigint") | Some("float") | Some("double")
        | Some("decimal") | Some("monetary") => InputKind::Number,
        Some("boolean") | Some("bool") => InputKind::Checkbox,
        Some("date") => InputKind::Date,
        Some("datetime") | Some("timestamp") => InputKind::DateTime,
        _ => InputKind::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_kind_template_stems_are_stable() {
        // The stems ARE filenames — must never change once shipped.
        assert_eq!(InputKind::Text.template_stem(), "text");
        assert_eq!(InputKind::TextArea.template_stem(), "textarea");
        assert_eq!(InputKind::Number.template_stem(), "number");
        assert_eq!(InputKind::Range.template_stem(), "range");
        assert_eq!(InputKind::Checkbox.template_stem(), "checkbox");
        assert_eq!(InputKind::Date.template_stem(), "date");
        assert_eq!(InputKind::DateTime.template_stem(), "datetime");
        assert_eq!(InputKind::Select.template_stem(), "select");
        assert_eq!(InputKind::Hidden.template_stem(), "hidden");
    }

    #[test]
    fn html_input_type_returns_none_for_non_input_elements() {
        // TextArea + Select render different tags, not <input>.
        assert!(InputKind::TextArea.html_input_type().is_none());
        assert!(InputKind::Select.html_input_type().is_none());
        // All others render an <input> with a definite type=…
        for k in [
            InputKind::Text,
            InputKind::Number,
            InputKind::Range,
            InputKind::Checkbox,
            InputKind::Date,
            InputKind::DateTime,
            InputKind::Hidden,
        ] {
            assert!(k.html_input_type().is_some(), "{k:?}");
        }
    }

    #[test]
    fn default_input_kind_hides_ids() {
        assert_eq!(default_input_kind_for("id", None), InputKind::Hidden);
        assert_eq!(default_input_kind_for("project_id", None), InputKind::Hidden);
        assert_eq!(default_input_kind_for("author_id", Some("integer")), InputKind::Hidden);
    }

    #[test]
    fn default_input_kind_picks_range_for_ratios() {
        assert_eq!(default_input_kind_for("done_ratio", None), InputKind::Range);
        assert_eq!(default_input_kind_for("complete_pct", None), InputKind::Range);
    }

    #[test]
    fn default_input_kind_picks_textarea_for_prose() {
        // Prose-named text fields get a textarea; non-prose text fields
        // also get a textarea (text is long enough to warrant it).
        assert_eq!(default_input_kind_for("description", Some("text")), InputKind::TextArea);
        assert_eq!(default_input_kind_for("body", Some("text")), InputKind::TextArea);
        assert_eq!(default_input_kind_for("notes", Some("text")), InputKind::TextArea);
        assert_eq!(default_input_kind_for("status_label", Some("text")), InputKind::TextArea);
    }

    #[test]
    fn default_input_kind_picks_numeric_for_numbers() {
        for t in ["integer", "big_integer", "bigint", "float", "double", "decimal", "monetary"] {
            assert_eq!(
                default_input_kind_for("position", Some(t)),
                InputKind::Number,
                "type {t}"
            );
        }
    }

    #[test]
    fn default_input_kind_picks_checkbox_for_boolean() {
        assert_eq!(default_input_kind_for("active", Some("boolean")), InputKind::Checkbox);
        assert_eq!(default_input_kind_for("locked", Some("bool")), InputKind::Checkbox);
    }

    #[test]
    fn default_input_kind_picks_date_types_for_temporal() {
        assert_eq!(default_input_kind_for("start_date", Some("date")), InputKind::Date);
        assert_eq!(default_input_kind_for("created_at", Some("datetime")), InputKind::DateTime);
        assert_eq!(default_input_kind_for("updated_at", Some("timestamp")), InputKind::DateTime);
    }

    #[test]
    fn default_input_kind_falls_back_to_text() {
        assert_eq!(default_input_kind_for("subject", Some("string")), InputKind::Text);
        assert_eq!(default_input_kind_for("unknown_field", None), InputKind::Text);
    }
}
