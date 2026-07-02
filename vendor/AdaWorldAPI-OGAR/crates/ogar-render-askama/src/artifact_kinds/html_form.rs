//! `HtmlForm` emitter — T4 per Northstar plan §3. The create/edit-page
//! sibling of T2's list view and T3's detail view.
//!
//! Substrate-agnostic — one form spine template + per-[`InputKind`]
//! sub-templates. Inputs are pre-rendered in Rust (parallel to the
//! T2/T3 cell pattern) so the spine emits `{{ field.body_html|safe }}`
//! with no runtime polymorphism on the askama side.
//!
//! Mirrors Redmine's `_form.html.erb` shape (one shared partial per
//! resource), generalised across every canonical concept by the
//! `InputKind` + `RenderColumn` pair.

use askama::Template;

use super::inputs::{render_input_body, InputData};
use super::ArtifactEmitter;
use crate::form_view::{default_input_kind_for, InputKind};
use crate::list_view::RenderColumn;
use crate::spec::ArtifactSpec;
use ogar_vocab::canonical_concept_id;

// ── Spine binding struct ─────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dispatch/html_form.askama", escape = "html")]
struct HtmlFormCtx {
    class_id_hex: String,
    canonical_concept: String,
    method: String,
    action: String,
    csrf_token: String,
    /// Pre-rendered hidden `<input>` for the record id (empty when
    /// rendering a new-record form).
    record_id_html: String,
    legend: String,
    submit_label: String,
    cancel_label: String,
    cancel_href: String,
    fields: Vec<FormField>,
}

struct FormField {
    name: String,
    label: String,
    css_classes: String,
    hint: String,
    required: bool,
    /// `true` → emit the body bare (no `<label>`/wrapper); used for
    /// `InputKind::Hidden` and any frozen/hidden id round-trip.
    hidden: bool,
    body_html: String,
}

// ── Source structs the caller fills ─────────────────────────────────

/// One field's worth of source data the emitter consumes — the form
/// equivalent of T2's [`CellSource`](super::html_list_view::CellSource).
#[derive(Debug, Clone)]
pub struct FormFieldSource<'a> {
    /// The column this field belongs to (carries name, caption,
    /// required, frozen, …).
    pub column: &'a RenderColumn,
    /// Optional CSS classes added to the field wrapper `<div>`.
    pub css_classes: &'a str,
    /// Help text shown under the input (empty for none).
    pub hint: &'a str,
    /// Per-kind input payload (current value, required, options, …).
    pub data: InputData,
}

/// Full form description — what the caller passes to [`render_form`].
#[derive(Debug, Clone)]
pub struct FormSource<'a> {
    /// HTTP method on the `<form>` element. Typically `"post"` for
    /// new records and `"patch"` for updates.
    pub method: &'a str,
    /// Submit target URL (`/issues`, `/issues/42`, …).
    pub action: &'a str,
    /// Authenticity token; empty to skip emission.
    pub csrf_token: &'a str,
    /// `Some(id)` when editing an existing record (emits a hidden id
    /// input so the action can dispatch). `None` for create forms.
    pub record_id: Option<u64>,
    /// Optional `<legend>` text on the `<fieldset>`.
    pub legend: &'a str,
    /// Label for the submit button.
    pub submit_label: &'a str,
    /// Label for the cancel link.
    pub cancel_label: &'a str,
    /// Cancel-link href; empty hides the cancel link.
    pub cancel_href: &'a str,
    /// One entry per form field, in declaration order.
    pub fields: Vec<FormFieldSource<'a>>,
}

// ── Public entry point ──────────────────────────────────────────────

/// Render one create/edit form for a canonical concept.
///
/// Returns `askama::Error` on internal template error; caller-supplied
/// data is validated (no panics on common mistakes — empty fields are
/// allowed; a malformed `InputData` variant fails the per-input render
/// loudly).
pub fn render_form(
    class_id: u16,
    canonical_concept: &str,
    src: &FormSource<'_>,
) -> Result<String, askama::Error> {
    use super::inputs::HiddenInput;

    let mut fields = Vec::with_capacity(src.fields.len());
    for f in &src.fields {
        let body = render_input_body(&f.column.name, &f.data)?;
        let hidden = matches!(f.data, InputData::Hidden { .. });
        fields.push(FormField {
            name: f.column.name.clone(),
            label: f.column.caption.clone(),
            css_classes: f.css_classes.to_string(),
            hint: f.hint.to_string(),
            required: required_for(&f.data),
            hidden,
            body_html: body,
        });
    }

    let record_id_html = if let Some(id) = src.record_id {
        let id_str = id.to_string();
        HiddenInput {
            name: "id",
            value: id_str.as_str(),
        }
        .render()?
    } else {
        String::new()
    };

    HtmlFormCtx {
        class_id_hex: format!("0x{class_id:04X}"),
        canonical_concept: canonical_concept.to_string(),
        method: src.method.to_string(),
        action: src.action.to_string(),
        csrf_token: src.csrf_token.to_string(),
        record_id_html,
        legend: src.legend.to_string(),
        submit_label: src.submit_label.to_string(),
        cancel_label: src.cancel_label.to_string(),
        cancel_href: src.cancel_href.to_string(),
        fields,
    }
    .render()
}

fn required_for(data: &InputData) -> bool {
    match data {
        InputData::Text { required, .. }
        | InputData::TextArea { required, .. }
        | InputData::Number { required, .. }
        | InputData::Date { required, .. }
        | InputData::DateTime { required, .. }
        | InputData::Select { required, .. } => *required,
        InputData::Range { .. } | InputData::Checkbox { .. } | InputData::Hidden { .. } => false,
    }
}

// ── Codebook-only proof-of-shape entry point ────────────────────────

/// The dispatcher entry point used by [`for_kind`](super::for_kind).
/// Synthesises a new-record form from the class's attributes (proof of
/// shape); real callers use [`render_form`] with real values.
pub struct HtmlFormEmitter;

impl ArtifactEmitter for HtmlFormEmitter {
    fn emit(&self, spec: &ArtifactSpec<'_>) -> Result<String, askama::Error> {
        let class = spec.class;
        let concept = class.canonical_concept.as_deref().unwrap_or("");
        let class_id = canonical_concept_id(concept).unwrap_or(0);

        // Synthesise columns from the class's attributes.
        let columns: Vec<RenderColumn> = class
            .attributes
            .iter()
            .map(|attr| {
                use crate::list_view::{default_kind_for, ColumnKind};
                let kind = default_kind_for(&attr.name, attr.type_name.as_deref());
                let col = RenderColumn::new(&attr.name, &attr.name, kind);
                if matches!(kind, ColumnKind::RichText) {
                    col.block()
                } else {
                    col
                }
            })
            .collect();

        // Build placeholder input data per InputKind resolved from the
        // class's attribute (kept alongside columns so the &str borrows
        // in InputData::* live as long as the call).
        let fields: Vec<FormFieldSource<'_>> = columns
            .iter()
            .zip(class.attributes.iter())
            .map(|(col, attr)| {
                let input_kind = default_input_kind_for(&attr.name, attr.type_name.as_deref());
                let data = empty_input_for(input_kind);
                FormFieldSource {
                    column: col,
                    css_classes: "",
                    hint: "",
                    data,
                }
            })
            .collect();

        let src = FormSource {
            method: "post",
            action: "",
            csrf_token: "",
            record_id: None,
            legend: concept,
            submit_label: "Create",
            cancel_label: "Cancel",
            cancel_href: "",
            fields,
        };
        render_form(class_id, concept, &src)
    }
}

/// Default empty `InputData` for the given [`InputKind`] — used by the
/// codebook-only proof-of-shape path and tests that don't supply real
/// data.
pub fn empty_input_for(kind: InputKind) -> InputData {
    match kind {
        InputKind::Text => InputData::Text {
            value: String::new(),
            required: false,
            placeholder: String::new(),
        },
        InputKind::TextArea => InputData::TextArea {
            value: String::new(),
            rows: 6,
            required: false,
            placeholder: String::new(),
        },
        InputKind::Number => InputData::Number {
            value: String::new(),
            required: false,
            step: String::new(),
        },
        InputKind::Range => InputData::Range {
            value: "0".to_string(),
            min: 0,
            max: 100,
            step: 1,
            suffix: "%".to_string(),
        },
        InputKind::Checkbox => InputData::Checkbox { checked: false },
        InputKind::Date => InputData::Date {
            value: String::new(),
            required: false,
        },
        InputKind::DateTime => InputData::DateTime {
            value: String::new(),
            required: false,
        },
        InputKind::Select => InputData::Select {
            value: String::new(),
            required: false,
            options: Vec::new(),
        },
        InputKind::Hidden => InputData::Hidden {
            value: String::new(),
        },
    }
}
