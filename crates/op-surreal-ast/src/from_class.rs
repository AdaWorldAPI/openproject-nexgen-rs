//! `from_class` — adapter from [`ogar_vocab::Class`] (the calcified
//! canonical AR shape) onto the typed [`crate::TableDefinition`] AST.
//!
//! **Northstar plan §3, C3.** The third path that feeds the same
//! typed AST as [`crate::from_triples`], but consuming the canonical
//! layer directly instead of a triple stream. Sibling to `from_triples`,
//! not a replacement — the byte-identical-output pin
//! ([`crate::rails_mini_e2e_byte_for_byte_with_legacy_emission`] in the
//! lib's test module) still holds for the triple-fed path.
//!
//! # Two pipelines, one AST
//!
//! ```text
//!   Rails source → ruff_ruby_spo → triples → from_triples::triples_to_schema
//!                                              \
//!                                               \   crate::Schema (typed AST)
//!                                               /
//!   ogar_vocab::Class fns ──────────► class_to_table (this module)
//! ```
//!
//! Both end at [`crate::Schema`]. Both render to SurrealQL via the same
//! `ToSql` impls. The byte-identical pin is the typed AST → SQL render
//! step; both producers feed it.
//!
//! # What this maps
//!
//! - `Class.canonical_concept` → table name (snake_case, matches OGAR
//!   codebook). Falls back to `Class.name.to_ascii_lowercase()` for
//!   classes the producer hasn't tagged with a canonical concept yet.
//! - `Class.canonical_concept_id` → `TableDefinition.comment` as
//!   `"OGAR codebook id 0xDDCC (<concept>)"` — makes the table's
//!   provenance traceable in the emitted DDL.
//! - `Class.attributes` → `DEFINE FIELD … TYPE <kind>` via
//!   [`crate::Kind::from_rails_type`]. Unrecognised type names fall
//!   back to [`crate::Kind::Any`] so emission never panics on the
//!   long-tail.
//! - `belongs_to` / `has_one` associations → `DEFINE FIELD … TYPE
//!   option<record<target>>`. Target table is the snake_case'd
//!   `class_name`.
//! - `has_many` / `has_and_belongs_to_many` are **skipped** today. The
//!   typed AST's [`crate::Kind`] doesn't yet have an `Array` variant
//!   (`// - Array(Box<Kind>) — has_many → array<record<…>>` in
//!   `lib.rs:90`); when the C18 sprint lands `Array`, this skip
//!   becomes a real field. Test
//!   [`tests::has_many_edges_are_skipped_until_array_kind_exists`]
//!   pins this so the regression is loud.
//! - Primary-label index: classes with a `name` / `subject` / `title`
//!   / `label` attribute get a default non-unique
//!   `DEFINE INDEX idx_<table>_<col>` on it.

use ogar_vocab::{canonical_concept_id, AssociationKind, Class};

use crate::{FieldDefinition, IndexDefinition, Kind, TableDefinition};

/// Lift one canonical [`Class`] into a typed [`TableDefinition`].
///
/// See the module doc for the field/edge mapping. Pure; no I/O. Stable
/// + deterministic: the same `Class` always renders the same
/// `TableDefinition`.
#[must_use]
pub fn class_to_table(class: &Class) -> TableDefinition {
    let name = table_name_for(class);
    let mut t = TableDefinition::new(name.clone());

    // Provenance comment — codebook id only when the concept is
    // promoted. Unpromoted classes get the bare DEFINE TABLE line.
    if let Some(concept) = class.canonical_concept.as_deref() {
        if let Some(id) = canonical_concept_id(concept) {
            t = t.with_comment(Some(format!(
                "OGAR codebook id 0x{id:04X} ({concept})"
            )));
        }
    }

    // Typed attributes — Rails type → SurrealQL kind via `from_rails_type`.
    // Unknown types fall back to `Any` so the long-tail (custom-field
    // shapes, future Rails types) doesn't break emission.
    for attr in &class.attributes {
        let kind = attr
            .type_name
            .as_deref()
            .and_then(Kind::from_rails_type)
            .unwrap_or(Kind::Any);
        t = t.with_field(FieldDefinition::new(&attr.name, &name, kind));
    }

    // Family edges — belongs_to / has_one render as option<record<target>>.
    // has_many / habtm are skipped (no Array kind yet — see module doc).
    for assoc in &class.associations {
        match assoc.kind {
            AssociationKind::HasMany | AssociationKind::HasAndBelongsToMany => continue,
            _ => {
                let target = target_table_for(assoc);
                let kind = Kind::Record(vec![target]).optional();
                t = t.with_field(FieldDefinition::new(&assoc.name, &name, kind));
            }
        }
    }

    // Default primary-label index — first matching of name/subject/title/label.
    for primary in ["name", "subject", "title", "label"] {
        if class.attributes.iter().any(|a| a.name == primary) {
            t = t.with_index(IndexDefinition::new(
                format!("idx_{name}_{primary}"),
                &name,
                vec![primary.to_string()],
            ));
            break;
        }
    }

    t
}

/// Table name for the SurrealDB schema. Prefers the snake_case canonical
/// concept (matches the OGAR codebook), falls back to the lowercased
/// class name for unpromoted concepts.
fn table_name_for(class: &Class) -> String {
    class
        .canonical_concept
        .clone()
        .unwrap_or_else(|| class.name.to_ascii_lowercase())
}

/// Target table for a family edge — snake_case'd `class_name`. Matches
/// the table-name convention [`table_name_for`] uses, so the resulting
/// `record<target>` references resolve in the rendered schema.
fn target_table_for(assoc: &ogar_vocab::Association) -> String {
    assoc
        .class_name
        .as_deref()
        .map(|s| {
            let mut out = String::with_capacity(s.len() + 4);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToSql;
    use ogar_vocab::{
        billable_work_entry, project, project_role, project_work_item,
    };

    #[test]
    fn project_work_item_lifts_to_table_with_codebook_comment() {
        let class = project_work_item();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            sql.starts_with(
                "DEFINE TABLE project_work_item SCHEMAFULL COMMENT 'OGAR codebook id 0x0102 (project_work_item)';"
            ),
            "expected DEFINE TABLE + codebook comment, got:\n{sql}"
        );
    }

    #[test]
    fn class_to_table_emits_field_per_typed_attribute() {
        // project_role has name(string), position(integer), permissions(text).
        // The Rails→SurQL kind map: string→string, integer→int, text→string.
        let class = project_role();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            sql.contains("DEFINE FIELD name ON TABLE project_role TYPE string;"),
            "{sql}"
        );
        assert!(
            sql.contains("DEFINE FIELD position ON TABLE project_role TYPE int;"),
            "{sql}"
        );
        assert!(
            sql.contains("DEFINE FIELD permissions ON TABLE project_role TYPE string;"),
            "{sql}"
        );
    }

    #[test]
    fn belongs_to_renders_as_option_record_target() {
        // billable_work_entry's family edges are mostly belongs_to —
        // each lifts to TYPE option<record<target>>.
        let class = billable_work_entry();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            sql.contains("TYPE option<record<"),
            "expected at least one option<record<…>> edge in:\n{sql}"
        );
        // Spot-check a known target (tax_policy is referenced via
        // billable_work_entry.classified_by; the canonical name is
        // already snake_case so the lower-casing is idempotent).
        assert!(
            sql.contains("record<tax_policy>"),
            "expected record<tax_policy> in:\n{sql}"
        );
    }

    #[test]
    fn has_many_edges_are_skipped_until_array_kind_exists() {
        // op_surreal_ast::Kind doesn't yet have an Array variant — see
        // module doc + lib.rs:90 ("Variants we'll add in later sprints"
        // — `Array(Box<Kind>) — has_many → array<record<…>>`). Today
        // has_many edges intentionally skip emission so the consumer
        // doesn't get a wrong-shape field. Test fires loudly when
        // Array lands and this skip should flip.
        let class = project_role(); // has_many :memberships
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            !sql.contains("DEFINE FIELD memberships"),
            "has_many `memberships` should be skipped until Kind::Array exists:\n{sql}"
        );
    }

    #[test]
    fn primary_label_index_emitted_when_class_has_a_name_like_attribute() {
        let class = project_role();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            sql.contains("DEFINE INDEX idx_project_role_name ON TABLE project_role FIELDS name;"),
            "expected idx_project_role_name index:\n{sql}"
        );
    }

    #[test]
    fn no_default_index_when_no_primary_label_attribute() {
        // billable_work_entry doesn't ship a name/subject/title/label —
        // the default-index inference must NOT fire.
        let class = billable_work_entry();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            !sql.contains("DEFINE INDEX idx_"),
            "billable_work_entry should not get a default index:\n{sql}"
        );
    }

    #[test]
    fn unpromoted_class_falls_back_to_lowercased_name_and_no_comment() {
        // A class without a canonical_concept skips the codebook
        // comment and uses class.name.to_ascii_lowercase() as the
        // table name. Construct one via the public Class API.
        use ogar_vocab::Class as VocabClass;
        let c = VocabClass::new("WidgetThing");
        let t = class_to_table(&c);
        let sql = t.to_sql();
        assert_eq!(sql, "DEFINE TABLE widgetthing SCHEMAFULL;\n");
    }

    #[test]
    fn table_name_uses_canonical_concept_when_promoted() {
        // project() is promoted, so the table is "project" not "Project".
        let class = project();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            sql.starts_with("DEFINE TABLE project SCHEMAFULL"),
            "{sql}"
        );
    }

    #[test]
    fn fed_into_schema_renders_in_order() {
        // End-to-end: build a Schema from two canonical classes, render,
        // verify table order is preserved.
        let s = crate::Schema::new()
            .with_table(class_to_table(&project()))
            .with_table(class_to_table(&project_role()));
        let sql = s.to_sql();
        let i_project = sql.find("DEFINE TABLE project SCHEMAFULL").unwrap();
        let i_role = sql.find("DEFINE TABLE project_role SCHEMAFULL").unwrap();
        assert!(
            i_project < i_role,
            "tables should render in insertion order:\n{sql}"
        );
    }
}
