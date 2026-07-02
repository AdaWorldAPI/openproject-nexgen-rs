//! Per-[`InputKind`](crate::form_view::InputKind) form-input renderers —
//! one askama sub-template per variant. Called from
//! [`html_form`](super::html_form) at field-build time to pre-render
//! each input's HTML, so the spine template just emits
//! `{{ field.body_html|safe }}` with no runtime polymorphism (parallel
//! to T2/T3's cell pattern, factored through
//! [`render_input_body`](self::render_input_body)).
//!
//! Per Northstar §1.6: each input template is the smallest bag of
//! variables it needs.

use askama::Template;

// ── Text ─────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/input/text.askama", escape = "html")]
pub(crate) struct TextInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub required: bool,
    pub placeholder: &'a str,
}

// ── TextArea ─────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/input/textarea.askama", escape = "html")]
pub(crate) struct TextAreaInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub rows: u32,
    pub required: bool,
    pub placeholder: &'a str,
}

// ── Number ───────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/input/number.askama", escape = "html")]
pub(crate) struct NumberInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub required: bool,
    /// Step granularity ("1" for integers, "0.01" for currency, etc.).
    pub step: &'a str,
}

// ── Range ────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/input/range.askama", escape = "html")]
pub(crate) struct RangeInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub min: i64,
    pub max: i64,
    pub step: u32,
    /// e.g. `"%"` for percentage; empty for plain numeric output.
    pub suffix: &'a str,
}

// ── Checkbox ─────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/input/checkbox.askama", escape = "html")]
pub(crate) struct CheckboxInput<'a> {
    pub name: &'a str,
    pub checked: bool,
}

// ── Date / DateTime ─────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/input/date.askama", escape = "html")]
pub(crate) struct DateInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub required: bool,
}

#[derive(Template)]
#[template(path = "dispatch/input/datetime.askama", escape = "html")]
pub(crate) struct DateTimeInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub required: bool,
}

// ── Select ───────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/input/select.askama", escape = "html")]
pub(crate) struct SelectInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub required: bool,
    pub options: Vec<SelectOption<'a>>,
}

pub(crate) struct SelectOption<'a> {
    pub value: &'a str,
    pub label: &'a str,
}

// ── Hidden ──────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/input/hidden.askama", escape = "html")]
pub(crate) struct HiddenInput<'a> {
    pub name: &'a str,
    pub value: &'a str,
}

// ── Owned input-data variant + shared dispatch ──────────────────────

use crate::form_view::InputKind;

/// Per-kind data the caller supplies for a form input. Variants are
/// keyed by [`InputKind`]; the [`render_input_body`] dispatch picks the
/// sub-template based on the variant.
#[derive(Debug, Clone)]
#[allow(missing_docs)] // self-describing variants; fields documented inline.
pub enum InputData {
    Text {
        value: String,
        required: bool,
        placeholder: String,
    },
    TextArea {
        value: String,
        rows: u32,
        required: bool,
        placeholder: String,
    },
    Number {
        value: String,
        required: bool,
        /// Step granularity ("1", "0.01", …). Empty for the
        /// browser-default.
        step: String,
    },
    Range {
        value: String,
        min: i64,
        max: i64,
        step: u32,
        suffix: String,
    },
    Checkbox {
        checked: bool,
    },
    Date {
        value: String,
        required: bool,
    },
    DateTime {
        value: String,
        required: bool,
    },
    Select {
        value: String,
        required: bool,
        options: Vec<SelectOptionOwned>,
    },
    Hidden {
        value: String,
    },
}

/// Owned form of one `<option>` for a [`InputData::Select`] dropdown.
#[derive(Debug, Clone)]
pub struct SelectOptionOwned {
    /// `value` attribute (typically a record id as string).
    pub value: String,
    /// Human-readable label.
    pub label: String,
}

impl InputData {
    /// The [`InputKind`] this data variant pairs with — useful for
    /// asserting `(InputKind, InputData)` consistency in tests / call
    /// sites.
    pub fn kind(&self) -> InputKind {
        match self {
            Self::Text { .. } => InputKind::Text,
            Self::TextArea { .. } => InputKind::TextArea,
            Self::Number { .. } => InputKind::Number,
            Self::Range { .. } => InputKind::Range,
            Self::Checkbox { .. } => InputKind::Checkbox,
            Self::Date { .. } => InputKind::Date,
            Self::DateTime { .. } => InputKind::DateTime,
            Self::Select { .. } => InputKind::Select,
            Self::Hidden { .. } => InputKind::Hidden,
        }
    }
}

/// Pre-render an [`InputData`] value into its HTML body via the matching
/// per-kind sub-template. The spine `html_form.askama` consumes the
/// result with `|safe` (the sub-templates each escape their own
/// variables under `escape = "html"`).
pub(crate) fn render_input_body(name: &str, data: &InputData) -> Result<String, askama::Error> {
    Ok(match data {
        InputData::Text {
            value,
            required,
            placeholder,
        } => TextInput {
            name,
            value: value.as_str(),
            required: *required,
            placeholder: placeholder.as_str(),
        }
        .render()?,
        InputData::TextArea {
            value,
            rows,
            required,
            placeholder,
        } => TextAreaInput {
            name,
            value: value.as_str(),
            rows: *rows,
            required: *required,
            placeholder: placeholder.as_str(),
        }
        .render()?,
        InputData::Number {
            value,
            required,
            step,
        } => NumberInput {
            name,
            value: value.as_str(),
            required: *required,
            step: step.as_str(),
        }
        .render()?,
        InputData::Range {
            value,
            min,
            max,
            step,
            suffix,
        } => RangeInput {
            name,
            value: value.as_str(),
            min: *min,
            max: *max,
            step: *step,
            suffix: suffix.as_str(),
        }
        .render()?,
        InputData::Checkbox { checked } => CheckboxInput {
            name,
            checked: *checked,
        }
        .render()?,
        InputData::Date { value, required } => DateInput {
            name,
            value: value.as_str(),
            required: *required,
        }
        .render()?,
        InputData::DateTime { value, required } => DateTimeInput {
            name,
            value: value.as_str(),
            required: *required,
        }
        .render()?,
        InputData::Select {
            value,
            required,
            options,
        } => {
            let mapped: Vec<SelectOption<'_>> = options
                .iter()
                .map(|o| SelectOption {
                    value: o.value.as_str(),
                    label: o.label.as_str(),
                })
                .collect();
            SelectInput {
                name,
                value: value.as_str(),
                required: *required,
                options: mapped,
            }
            .render()?
        }
        InputData::Hidden { value } => HiddenInput {
            name,
            value: value.as_str(),
        }
        .render()?,
    })
}
