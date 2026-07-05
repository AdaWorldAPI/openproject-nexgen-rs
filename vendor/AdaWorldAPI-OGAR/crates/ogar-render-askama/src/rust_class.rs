//! LEG 3 — the compile-time ERB/askama transpiler for a full canonical class:
//! **ClassView × FieldMask → struct**, plus the OGAR `ActionDef` DO-arm →
//! a **struct-of-methods constructor**.
//!
//! Where [`RustStruct`](crate::ArtifactKind::RustStruct) emits the *whole*
//! class as a flat data struct, this path emits the **masked** projection and
//! attaches the behavioural arm:
//!
//! ```text
//!   ClassView (ObjectView field basis)  ×  FieldMask (presence bits)
//!            │                                    │
//!            └──────────────┬─────────────────────┘
//!                           ▼
//!                 struct fields (only present bits)
//!                           +
//!                 impl { new(..) ctor + one fn per ActionDef }   ← DO-arm
//!                           │
//!                     askama (the ERB/XSLT analog), compile-time
//! ```
//!
//! Two deliberate rulings baked in:
//!
//! 1. **The FieldMask indexes the ObjectView N3 order** — attributes first
//!    (declaration order), then family-edge associations — the exact basis
//!    [`OgarClassView::render_rows`](../../ogar_class_view) projects. Bit `n`
//!    set ⇒ the `n`-th field emits; [`FieldMask(u64::MAX)`] emits every field.
//! 2. **Behaviour is Rust methods, never SurrealQL DDL.** The DO-arm
//!    ([`ActionDef`]) materialises as methods on the struct via a
//!    constructor-opened `impl` block. The deprecated SurrealQL-AST adapter
//!    (`DEFINE EVENT … WHEN … THEN …` carrying lifecycle) is NOT a target
//!    here — behaviour flows producer → OGAR `ActionDef` → Rust method.

use askama::Template;
use lance_graph_contract::class_view::FieldMask;
use ogar_vocab::{canonical_concept_id, ActionDef, AssociationKind, Class};

use crate::artifact_kinds::rust_struct::{edge_rust_type, escape_rust_ident, rails_to_rust_type};

/// askama-bound context for `templates/rust_class.askama`.
#[derive(Template)]
#[template(path = "rust_class.askama", escape = "none")]
struct RustClassCtx {
    name: String,
    concept_fn: String,
    canonical_concept: String,
    /// Hex class id (`"0x0102"`) or empty when the concept isn't in the
    /// codebook (the template branches on length).
    class_id_hex: String,
    fields: Vec<RustField>,
    /// Precomputed `"a: String, b: i64"` constructor parameter list — built
    /// in Rust so the template never has to juggle loop-comma joins.
    ctor_params: String,
    /// Precomputed `"a, b"` field-init list for the constructor body.
    ctor_inits: String,
    methods: Vec<RustMethod>,
}

struct RustField {
    snake_name: String,
    rust_type: String,
    /// `"attribute"` / `"belongs_to"` / `"has_many"` … — a doc hint naming
    /// which ClassView axis the field came from.
    origin: String,
}

struct RustMethod {
    fn_name: String,
    predicate: String,
    /// Decorator provenance (`"decorators: api.depends"`), empty when none.
    doc: String,
    /// A one-line provenance comment for the method body (first source line
    /// or a TODO pointing at the canonical `object_class`).
    body_comment: String,
    /// `&mut self` when the action declares an `on_enter` state mutation
    /// (the Rubicon crossing writes a field); `&self` otherwise.
    mutates: bool,
}

/// Render a canonical [`Class`] as a Rust struct whose FIELDS are the
/// `ClassView × FieldMask` projection and whose METHODS are the OGAR
/// [`ActionDef`] DO-arm, assembled as a struct-of-methods constructor.
///
/// - `mask` indexes the ObjectView N3 field order (attributes then family
///   edges). An all-bits-set mask (`FieldMask(u64::MAX)`) emits every field;
///   any other mask emits only the fields whose bit is set (positions
///   `>= FieldMask::MAX_FIELDS` can't be represented and drop — matching the
///   contract's 64-field ceiling).
/// - `actions` are the ActionDefs the caller has already filtered to this
///   class (`ActionDef::object_class` == this class). The render crate stays
///   a pure projection — it never scans a global action table.
///
/// # Errors
///
/// Propagates [`askama::Error`] if template rendering fails (it never should
/// for well-formed input — the template has no fallible expressions).
pub fn render_class_with_methods(
    class: &Class,
    mask: FieldMask,
    actions: &[ActionDef],
) -> Result<String, askama::Error> {
    let concept = class.canonical_concept.as_deref().unwrap_or("");
    let class_id_hex = canonical_concept_id(concept)
        .map(|id| format!("0x{id:04X}"))
        .unwrap_or_default();

    // ObjectView N3 order: attributes, then associations. `idx` walks the
    // combined sequence; the mask gates each position.
    let mut fields = Vec::new();
    let mut idx: u8 = 0;
    for a in &class.attributes {
        if field_present(mask, idx) {
            fields.push(RustField {
                snake_name: escape_rust_ident(&a.name),
                rust_type: rails_to_rust_type(a.type_name.as_deref()),
                origin: "attribute".to_string(),
            });
        }
        idx = idx.saturating_add(1);
    }
    for e in &class.associations {
        if field_present(mask, idx) {
            fields.push(RustField {
                snake_name: escape_rust_ident(&e.name),
                rust_type: edge_rust_type(e),
                origin: assoc_origin(e.kind),
            });
        }
        idx = idx.saturating_add(1);
    }

    let ctor_params = fields
        .iter()
        .map(|f| format!("{}: {}", f.snake_name, f.rust_type))
        .collect::<Vec<_>>()
        .join(", ");
    let ctor_inits = fields
        .iter()
        .map(|f| f.snake_name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    let methods = actions.iter().map(lift_method).collect();

    let ctx = RustClassCtx {
        name: pascal_type_name(&class.name),
        concept_fn: concept.to_string(),
        canonical_concept: concept.to_string(),
        class_id_hex,
        fields,
        ctor_params,
        ctor_inits,
        methods,
    };
    ctx.render()
}

/// An all-bits-set mask is the "unmasked" sentinel (emit everything,
/// including any field beyond the 64-bit ceiling). Any narrower mask consults
/// the bit — and a position past `MAX_FIELDS` can't be present, so it drops.
fn field_present(mask: FieldMask, idx: u8) -> bool {
    if mask.0 == u64::MAX {
        true
    } else if (idx as u32) < FieldMask::MAX_FIELDS {
        mask.has(idx)
    } else {
        false
    }
}

/// Doc label for the ClassView axis a family edge came from.
fn assoc_origin(kind: AssociationKind) -> String {
    match kind {
        AssociationKind::BelongsTo => "belongs_to".into(),
        AssociationKind::HasOne => "has_one".into(),
        AssociationKind::HasMany => "has_many".into(),
        AssociationKind::HasAndBelongsToMany => "has_and_belongs_to_many".into(),
        _ => "family_edge".into(),
    }
}

/// Lift one OGAR [`ActionDef`] onto a method skeleton. The predicate becomes
/// the fn name; decorators become a doc line; `on_enter` (a state mutation on
/// the Rubicon crossing) makes the method take `&mut self`.
fn lift_method(a: &ActionDef) -> RustMethod {
    let fn_name = escape_rust_ident(&sanitize_ident(&a.predicate));
    let doc = if a.decorators.is_empty() {
        String::new()
    } else {
        format!("decorators: {}", a.decorators.join(", "))
    };
    let body_comment = match &a.body_source {
        Some(b) if !b.trim().is_empty() => {
            let first = b.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
            format!("// ported from source: {}", one_line(first))
        }
        _ => format!("// TODO: port `{}` from {}", a.predicate, a.object_class),
    };
    RustMethod {
        fn_name,
        predicate: a.predicate.clone(),
        doc,
        body_comment,
        mutates: a.on_enter.is_some(),
    }
}

/// Collapse a source snippet to a single safe comment line.
fn one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .take(100)
        .collect()
}

/// Reduce an arbitrary source name to a snake-case Rust identifier body:
/// lowercase, `[a-z0-9_]` kept, everything else → `_`. A leading digit is
/// prefixed with `_` so the result is a legal identifier.
fn sanitize_ident(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("action");
    } else if out.as_bytes()[0].is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

/// Turn an arbitrary class name (`"account.move"`, `"work_package"`,
/// `"WorkPackage"`) into a valid PascalCase Rust type identifier. Splits on
/// any non-alphanumeric boundary AND on existing camel/Pascal humps are kept
/// verbatim — a name that is already PascalCase (`"WorkPackage"`) round-trips.
fn pascal_type_name(raw: &str) -> String {
    let mut out = String::new();
    let mut cap_next = true;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() {
            if cap_next {
                out.push(c.to_ascii_uppercase());
                cap_next = false;
            } else {
                out.push(c);
            }
        } else {
            // any separator (`.`, `_`, `-`, space) starts a new hump
            cap_next = true;
        }
    }
    if out.is_empty() {
        out.push_str("Anonymous");
    } else if out.as_bytes()[0].is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::{ActionDef, Attribute, EnterEffect};

    /// A small hand-built class: 2 attributes + 1 family edge, with one
    /// mutating action and one read action.
    fn sample_class() -> Class {
        let mut c = Class::new("account.move");
        c.canonical_concept = Some("commercial_document".to_string());
        // `Attribute` is `#[non_exhaustive]` — build via `Default` + field
        // assignment (fields are `pub`).
        let mut name = Attribute::default();
        name.name = "name".to_string();
        name.type_name = Some("string".to_string());
        let mut amount = Attribute::default();
        amount.name = "amount_total".to_string();
        amount.type_name = Some("decimal".to_string());
        c.attributes = vec![name, amount];
        c
    }

    /// `ActionDef` is `#[non_exhaustive]`, so build via `Default` + field
    /// assignment (the fields are all `pub`) rather than a struct literal.
    fn sample_actions() -> Vec<ActionDef> {
        let mut post = ActionDef::default();
        post.identity = "ogit-erp/account.move::action_def::action_post".to_string();
        post.predicate = "action_post".to_string();
        post.object_class = "ogit-erp/account.move".to_string();
        post.on_enter = Some(EnterEffect::transition("state", "posted"));

        let mut name_get = ActionDef::default();
        name_get.identity = "ogit-erp/account.move::action_def::name_get".to_string();
        name_get.predicate = "name.get".to_string(); // dotted → sanitised
        name_get.object_class = "ogit-erp/account.move".to_string();
        name_get.decorators = vec!["api.depends".to_string()];

        vec![post, name_get]
    }

    #[test]
    fn full_mask_emits_all_fields_and_the_ctor() {
        let class = sample_class();
        let src = render_class_with_methods(&class, FieldMask(u64::MAX), &[]).unwrap();
        // PascalCase type name derived from the dotted Odoo name.
        assert!(src.contains("pub struct AccountMove"), "{src}");
        // Both attributes present under FULL.
        assert!(src.contains("pub name: String,"), "{src}");
        assert!(src.contains("pub amount_total: f64,"), "{src}");
        // The struct-of-methods constructor over the masked field set.
        assert!(
            src.contains("pub fn new(name: String, amount_total: f64) -> Self"),
            "{src}"
        );
        assert!(src.contains("pub const CLASS_ID: u16 = 0x"), "{src}");
    }

    #[test]
    fn field_mask_gates_which_fields_emit() {
        // Bit 0 = first attribute (`name`); drop bit 1 (`amount_total`).
        let mask = FieldMask::EMPTY.with(0);
        let class = sample_class();
        let src = render_class_with_methods(&class, mask, &[]).unwrap();
        assert!(src.contains("pub name: String,"), "kept field 0:\n{src}");
        assert!(
            !src.contains("amount_total"),
            "field 1 should be masked out:\n{src}"
        );
        // The constructor tracks the mask — only the present field.
        assert!(src.contains("pub fn new(name: String) -> Self"), "{src}");
    }

    #[test]
    fn action_defs_become_struct_methods_do_arm() {
        let class = sample_class();
        let actions = sample_actions();
        let src = render_class_with_methods(&class, FieldMask(u64::MAX), &actions).unwrap();
        // Each ActionDef → one method; the dotted predicate is sanitised.
        assert!(src.contains("pub fn action_post(&mut self)"), "mutating action → &mut self:\n{src}");
        assert!(src.contains("pub fn name_get(&self)"), "read action → &self, dotted name sanitised:\n{src}");
        // Provenance: the api.depends decorator surfaces in the doc.
        assert!(src.contains("api.depends"), "{src}");
        // No SurrealQL DDL anywhere — behaviour is Rust methods only.
        assert!(!src.contains("DEFINE EVENT"), "no SurrealQL AST adapter:\n{src}");
        assert!(!src.contains("DEFINE TABLE"), "{src}");
    }

    #[test]
    fn empty_class_emits_valid_shell() {
        let mut c = Class::new("Bare");
        c.canonical_concept = None;
        let src = render_class_with_methods(&c, FieldMask(u64::MAX), &[]).unwrap();
        assert!(src.contains("pub struct Bare"), "{src}");
        assert!(src.contains("pub fn new() -> Self"), "{src}");
    }

    #[test]
    fn sanitize_ident_handles_dotted_and_leading_digit() {
        assert_eq!(sanitize_ident("action_post"), "action_post");
        assert_eq!(sanitize_ident("name.get"), "name_get");
        assert_eq!(sanitize_ident("3d_render"), "_3d_render");
    }

    #[test]
    fn pascal_type_name_round_trips_and_normalises() {
        assert_eq!(pascal_type_name("account.move"), "AccountMove");
        assert_eq!(pascal_type_name("work_package"), "WorkPackage");
        assert_eq!(pascal_type_name("WorkPackage"), "WorkPackage");
    }
}
