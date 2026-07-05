//! `RustStruct` emitter — the proof-of-shape concrete renderer.
//!
//! Lifts an [`ogar_vocab::Class`] into a Rust `struct` declaration with a
//! `pub const CLASS_ID: u16` (from the OGAR codebook). The template is
//! `templates/dispatch/rust_struct.askama`; this file is the typed binding
//! between the `Class` data and the template's variables.
//!
//! Mirror of `woa-rs::codegen::handler_kinds::list_for_tenant` (the proof-
//! of-shape concrete emitter that opens the kit).

use askama::Template;

use super::ArtifactEmitter;
use crate::spec::ArtifactSpec;
use ogar_vocab::{canonical_concept_id, AssociationKind};

/// askama-bound context for `templates/dispatch/rust_struct.askama`.
///
/// All `String` fields because the template never branches on Rust types
/// — it just substitutes. Mapping `Attribute.type_name` / `Association`
/// targets onto Rust types happens in [`RustStructEmitter::emit`], not in
/// the template.
#[derive(Template)]
#[template(path = "dispatch/rust_struct.askama", escape = "none")]
struct RustStructCtx {
    name: String,
    concept_fn: String,
    canonical_concept: String,
    /// Hex-formatted class id (`"0x0102"`) or empty string when the
    /// concept isn't in the codebook (the template branches on length).
    class_id_hex: String,
    const_name: String,
    attributes: Vec<RustAttr>,
    associations: Vec<RustEdge>,
}

struct RustAttr {
    name: String,
    snake_name: String,
    rust_type: String,
    /// Producer-side type name (curator's `"integer"`, `"big_integer"`,
    /// `"Char"`, …), empty when absent.
    type_name: String,
}

struct RustEdge {
    name: String,
    snake_name: String,
    rust_type: String,
    /// `belongs_to` / `has_one` / `has_many` / `habtm` …
    kind_label: String,
    target: String,
}

/// The concrete emitter for [`ArtifactKind::RustStruct`](crate::ArtifactKind).
pub struct RustStructEmitter;

impl ArtifactEmitter for RustStructEmitter {
    fn emit(&self, spec: &ArtifactSpec<'_>) -> Result<String, askama::Error> {
        let class = spec.class;
        let concept = class.canonical_concept.as_deref().unwrap_or("");
        let class_id_hex = canonical_concept_id(concept)
            .map(|id| format!("0x{id:04X}"))
            .unwrap_or_default();
        // The `pub const class_ids::FOO` upper-snake name (e.g.
        // PROJECT_WORK_ITEM). Empty when no codebook id is known.
        let const_name = if class_id_hex.is_empty() {
            String::new()
        } else {
            concept.to_ascii_uppercase()
        };

        let attributes = class
            .attributes
            .iter()
            .map(|a| RustAttr {
                name: a.name.clone(),
                snake_name: escape_rust_ident(&a.name),
                rust_type: rails_to_rust_type(a.type_name.as_deref()),
                type_name: a.type_name.clone().unwrap_or_default(),
            })
            .collect();

        let associations = class
            .associations
            .iter()
            .map(|a| RustEdge {
                name: a.name.clone(),
                snake_name: escape_rust_ident(&a.name),
                rust_type: edge_rust_type(a),
                kind_label: assoc_label(a.kind),
                target: a.class_name.clone().unwrap_or_default(),
            })
            .collect();

        let ctx = RustStructCtx {
            name: class.name.clone(),
            concept_fn: concept.to_string(),
            canonical_concept: concept.to_string(),
            class_id_hex,
            const_name,
            attributes,
            associations,
        };
        ctx.render()
    }
}

/// Prefix Rust reserved words with `r#` so a curator-side slot named
/// `type`, `match`, `move`, … emits a legal Rust field identifier.
///
/// Catches codex P1 on PR #78: `project_actor()` ships an attribute named
/// `type` (per Rails STI convention) and the unescaped template would emit
/// `pub type: String,` — illegal. Same hazard for `async` / `await` /
/// `dyn` etc. that newer Rust editions reserved. Conservative list (Rust
/// 2024 strict + reserved-future); a `&str` slot from the canonical layer
/// can never need a non-identifier escape since names are sourced from
/// Rails / Odoo identifiers.
pub(crate) fn escape_rust_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        // Rust 2015+ strict keywords:
        "as", "break", "const", "continue", "crate", "else", "enum", "extern",
        "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
        "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
        "super", "trait", "true", "type", "unsafe", "use", "where", "while",
        // Rust 2018+ strict keywords:
        "async", "await", "dyn",
        // Reserved-future (lexable but unusable as raw idents anyway):
        "abstract", "become", "box", "do", "final", "macro", "override", "priv",
        "typeof", "unsized", "virtual", "yield", "try",
    ];
    if RESERVED.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

/// Map a producer-side Rails type name onto a Rust type for codegen.
///
/// Coarse: this is the proof-of-shape mapping the canonical layer uses
/// today. Each `op-*` / `rm-*` consumer is free to specialise (e.g.
/// `Decimal` vs `f64` for monetary slots) downstream. The point is the
/// canonical contract round-trips; precision is a per-consumer concern.
pub(crate) fn rails_to_rust_type(t: Option<&str>) -> String {
    match t {
        Some("string") | Some("text") => "String".into(),
        Some("integer") | Some("big_integer") | Some("bigint") => "i64".into(),
        Some("float") | Some("double") => "f64".into(),
        Some("decimal") | Some("monetary") => "f64".into(),
        Some("boolean") | Some("bool") => "bool".into(),
        Some("date") | Some("datetime") | Some("timestamp") => "String".into(),
        Some("json") | Some("jsonb") => "serde_json::Value".into(),
        Some(_) | None => "String".into(),
    }
}

pub(crate) fn edge_rust_type(a: &ogar_vocab::Association) -> String {
    // Coarse: `belongs_to` / `has_one` → `Option<u64>` (FK id),
    // `has_many` / `habtm` → `Vec<u64>`. The concrete `op-*` / `rm-*`
    // consumer can swap these for typed references downstream.
    match a.kind {
        AssociationKind::HasMany | AssociationKind::HasAndBelongsToMany => "Vec<u64>".into(),
        // BelongsTo, HasOne, plus any non-exhaustive future variant —
        // default to optional fk id.
        _ => "Option<u64>".into(),
    }
}

fn assoc_label(k: AssociationKind) -> String {
    match k {
        AssociationKind::BelongsTo => "belongs_to".into(),
        AssociationKind::HasOne => "has_one".into(),
        AssociationKind::HasMany => "has_many".into(),
        AssociationKind::HasAndBelongsToMany => "has_and_belongs_to_many".into(),
        // AssociationKind is #[non_exhaustive]; any future variant lands
        // here. The label is a doc-comment only, so a debug-style
        // fallback is harmless.
        _ => format!("{k:?}").to_ascii_lowercase(),
    }
}
