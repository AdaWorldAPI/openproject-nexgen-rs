//! `SurrealqlTable` emitter — T5 per Northstar plan §3. The
//! codegen-flavour finisher of the +5 kit. Emits `DEFINE TABLE` + per-
//! field `DEFINE FIELD` + family-edge `record<…>` link fields + primary-
//! label index, all from one canonical [`Class`].
//!
//! **Codegen flavour** (NOT the run-time SurrealDB query/projection
//! path — that lives in `op-surreal-ast`, which has the byte-identical-
//! output pin against the existing emission). This crate emits *typed
//! schema definitions* the engine consumes at boot.
//!
//! Per Northstar §1.6: mass-mail simple. Table name, field set, link
//! targets, index list — the binding-struct supplies them, the template
//! substitutes.

use askama::Template;

use super::ArtifactEmitter;
use crate::spec::ArtifactSpec;
use ogar_vocab::{canonical_concept_id, AssociationKind, Class};

#[derive(Template)]
#[template(path = "dispatch/surrealql_table.askama", escape = "none")]
struct SurrealqlTableCtx {
    name: String,
    concept_fn: String,
    canonical_concept: String,
    class_id_hex: String,
    table_name: String,
    fields: Vec<SurqlField>,
    edges: Vec<SurqlEdge>,
    indexes: Vec<SurqlIndex>,
}

struct SurqlField {
    name: String,
    surql_type: String,
    /// SurrealQL `ASSERT` clause (e.g. `$value >= 0` for integers).
    /// Empty for none.
    assertion: String,
}

struct SurqlEdge {
    name: String,
    surql_type: String,
    kind_label: String,
    target_table: String,
}

struct SurqlIndex {
    name: String,
    /// Comma-separated field list (`"name"` or `"name, position"`).
    fields: String,
    unique: bool,
}

/// The concrete emitter for
/// [`ArtifactKind::SurrealqlTable`](crate::ArtifactKind::SurrealqlTable).
pub struct SurrealqlTableEmitter;

impl ArtifactEmitter for SurrealqlTableEmitter {
    fn emit(&self, spec: &ArtifactSpec<'_>) -> Result<String, askama::Error> {
        let class = spec.class;
        let concept = class.canonical_concept.as_deref().unwrap_or("");
        let class_id_hex = canonical_concept_id(concept)
            .map(|id| format!("0x{id:04X}"))
            .unwrap_or_default();
        let table_name = table_name_from(class, concept);

        let fields = class
            .attributes
            .iter()
            .map(|a| SurqlField {
                name: escape_surql_ident(&a.name),
                surql_type: rails_to_surql_type(a.type_name.as_deref()),
                assertion: default_assertion(a.type_name.as_deref()).to_string(),
            })
            .collect();

        let edges = class
            .associations
            .iter()
            .map(|a| SurqlEdge {
                name: escape_surql_ident(&a.name),
                surql_type: edge_surql_type(a),
                kind_label: assoc_label(a.kind),
                target_table: edge_target_table(a),
            })
            .collect();

        let indexes = default_indexes(class);

        SurrealqlTableCtx {
            name: class.name.clone(),
            concept_fn: concept.to_string(),
            canonical_concept: concept.to_string(),
            class_id_hex,
            table_name,
            fields,
            edges,
            indexes,
        }
        .render()
    }
}

/// SurrealDB tables follow snake_case, lower-cased canonical concept
/// names. Falls back to the class name lower-cased when no concept is
/// promoted.
fn table_name_from(class: &Class, concept: &str) -> String {
    if !concept.is_empty() {
        concept.to_string()
    } else {
        class.name.to_ascii_lowercase()
    }
}

/// SurrealQL accepts a wide range of identifiers but reserves some
/// keywords. The wide-net escape (back-tick quoting) is always safe;
/// applying it only when needed keeps emission readable.
fn escape_surql_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "select", "from", "where", "create", "update", "delete", "let",
        "if", "then", "else", "end", "in", "and", "or", "not", "true",
        "false", "null", "none", "return", "begin", "commit", "transaction",
        "define", "table", "field", "index", "type", "value", "for", "id",
    ];
    if RESERVED.contains(&name) {
        format!("`{name}`")
    } else {
        name.to_string()
    }
}

/// Map a Rails-side type name onto a SurrealQL `TYPE …` clause. Coarse;
/// downstream specialisation (e.g. `decimal` precision) is the consumer's
/// concern.
fn rails_to_surql_type(t: Option<&str>) -> String {
    match t {
        Some("string") => "string".into(),
        Some("text") => "string".into(),
        Some("integer") | Some("big_integer") | Some("bigint") => "int".into(),
        Some("float") | Some("double") => "float".into(),
        Some("decimal") | Some("monetary") => "decimal".into(),
        Some("boolean") | Some("bool") => "bool".into(),
        Some("date") | Some("datetime") | Some("timestamp") => "datetime".into(),
        Some("json") | Some("jsonb") => "object".into(),
        Some(_) | None => "string".into(),
    }
}

/// Default SurrealQL `ASSERT` for a Rails type — e.g. integers must not
/// be negative for percentages, monetary must be ≥ 0. Returns `""` for
/// types without a useful default.
fn default_assertion(_t: Option<&str>) -> &'static str {
    // The canonical layer doesn't carry per-attribute validation meta yet.
    // Downstream consumers add asserts via their own builder; the codegen
    // emits the schema shape and lets a custom build step layer policy.
    // (The default is intentionally empty rather than guessed.)
    ""
}

fn edge_surql_type(a: &ogar_vocab::Association) -> String {
    let target_table = edge_target_table(a);
    match a.kind {
        AssociationKind::HasMany | AssociationKind::HasAndBelongsToMany => {
            format!("array<record<{target_table}>>")
        }
        // BelongsTo + HasOne + any future variant → optional record link.
        _ => format!("option<record<{target_table}>>"),
    }
}

fn edge_target_table(a: &ogar_vocab::Association) -> String {
    // The target's canonical concept maps cleanly to a SurrealQL table
    // name once we know it; today the canonical layer carries
    // `class_name` (the Rails type like `"Project"`). Lower-cased gives
    // us a usable table name; promoted concepts will have their proper
    // snake_case form rendered by their own SurrealqlTable emission.
    a.class_name
        .as_deref()
        .map(|s| {
            let lower = s.to_ascii_lowercase();
            // PascalCase → snake_case: insert `_` before each upper after the
            // first char. Implemented inline to avoid a dep.
            let mut out = String::with_capacity(lower.len());
            for (i, c) in s.chars().enumerate() {
                if i > 0 && c.is_ascii_uppercase() {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            }
            out
        })
        .unwrap_or_else(|| "any".to_string())
}

fn assoc_label(k: AssociationKind) -> String {
    match k {
        AssociationKind::BelongsTo => "belongs_to".into(),
        AssociationKind::HasOne => "has_one".into(),
        AssociationKind::HasMany => "has_many".into(),
        AssociationKind::HasAndBelongsToMany => "has_and_belongs_to_many".into(),
        _ => format!("{k:?}").to_ascii_lowercase(),
    }
}

/// Default indexes the canonical layer recommends. Today: a non-unique
/// index on the primary label attribute (if the class has one named
/// `name` / `subject` / `title` / `label`) to keep ORDER BY / WHERE
/// scans fast. Consumers can add more in their own build step.
fn default_indexes(class: &Class) -> Vec<SurqlIndex> {
    let mut out = Vec::new();
    for primary in ["name", "subject", "title", "label"] {
        if class.attributes.iter().any(|a| a.name == primary) {
            out.push(SurqlIndex {
                name: format!("idx_{primary}"),
                fields: primary.to_string(),
                unique: false,
            });
            break; // only one primary-label index
        }
    }
    out
}
