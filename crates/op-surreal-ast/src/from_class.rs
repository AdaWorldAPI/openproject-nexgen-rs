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

use ogar_vocab::{AssociationKind, Class, canonical_concept_id};

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
    if let Some(concept) = class.canonical_concept.as_deref()
        && let Some(id) = canonical_concept_id(concept)
    {
        t = t.with_comment(Some(format!("OGAR codebook id 0x{id:04X} ({concept})")));
    }

    // Typed attributes — Rails type → SurrealQL kind via `from_rails_type`.
    // Unknown types fall back to `Any` so the long-tail (custom-field
    // shapes, future Rails types) doesn't break emission.
    //
    // Nullability follows `attr.options.required` (codex P2 on PR #52):
    // only an explicit `Some(true)` (i.e. Rails `validates :name,
    // presence: true` or `null: false` in the migration) emits a bare
    // kind. Everything else — `None` / `Some(false)` — wraps in
    // `option<...>` to match the source column's NULL semantics. This
    // matches the triples path's behaviour, which always wraps until a
    // _validate function adds a non-null assertion.
    for attr in &class.attributes {
        let mut kind = attr
            .type_name
            .as_deref()
            .and_then(Kind::from_rails_type)
            .unwrap_or(Kind::Any);
        if attr.options.required != Some(true) {
            kind = kind.optional();
        }
        t = t.with_field(FieldDefinition::new(&attr.name, &name, kind));
    }

    // Family edges — `belongs_to` renders as `option<record<target>>`.
    // `has_one` is the non-owning side of a 1:1 (the FK lives on the
    // target table), so emitting a local field would create a phantom
    // link on the wrong side. `has_many` / `habtm` wait for
    // `crate::Kind::Array` (see module doc). All three skipped here;
    // codex P2 on PR #52 caught the original code accepting `HasOne`
    // through the wildcard arm.
    for assoc in &class.associations {
        match assoc.kind {
            AssociationKind::HasMany
            | AssociationKind::HasAndBelongsToMany
            | AssociationKind::HasOne => continue,
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

/// Target table for a family edge. Matches the table-name convention
/// [`table_name_for`] uses, so the resulting `record<target>` references
/// resolve in the rendered schema.
///
/// Two paths:
/// - Explicit `class_name:` (e.g. Rails `belongs_to :owner,
///   class_name: 'User'`) → snake_case'd value of the option.
/// - Implicit (most associations) → use `assoc.name` directly. The
///   Rails convention is that the relation name IS the target class's
///   snake_case singular form (e.g. `belongs_to :project` ⇒ target
///   table `project`). Codex P2 on PR #52 caught the original
///   fallback to the literal `"any"`, which surfaced as
///   `record<any>` — a phantom table.
fn target_table_for(assoc: &ogar_vocab::Association) -> String {
    if let Some(s) = assoc.class_name.as_deref() {
        return snake_case(s);
    }
    // The relation name is already snake_case singular for `belongs_to`
    // (the only association kind that reaches this fn after the
    // `class_to_table` skip-list filters out HasMany / habtm / HasOne).
    assoc.name.clone()
}

/// Lower-snake-case a PascalCase Rails class name. `WorkPackage` →
/// `work_package`. ASCII-only by design — Rails class names follow the
/// same convention.
fn snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToSql;
    use ogar_vocab::{billable_work_entry, project, project_role, project_work_item};

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
        // All three attributes default to nullable (no
        // `options.required = Some(true)` on the fixture), so the
        // emission wraps each in `option<...>` — see
        // `typed_attributes_default_to_optional_when_not_required`
        // for the dedicated regression pin (codex P2 on PR #52).
        let class = project_role();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            sql.contains("DEFINE FIELD name ON TABLE project_role TYPE option<string>;"),
            "{sql}"
        );
        assert!(
            sql.contains("DEFINE FIELD position ON TABLE project_role TYPE option<int>;"),
            "{sql}"
        );
        assert!(
            sql.contains("DEFINE FIELD permissions ON TABLE project_role TYPE option<string>;"),
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
        assert!(sql.starts_with("DEFINE TABLE project SCHEMAFULL"), "{sql}");
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

    // ── Codex P2 follow-ups on PR #52 ────────────────────────────────

    #[test]
    fn typed_attributes_default_to_optional_when_not_required() {
        // project_role's `name`, `position`, `permissions` attributes
        // don't carry `options.required == Some(true)`, so they must
        // emit as `option<...>` (codex P2 on PR #52).
        let class = project_role();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            sql.contains("DEFINE FIELD name ON TABLE project_role TYPE option<string>;"),
            "name should be option<string>:\n{sql}"
        );
        assert!(
            sql.contains("DEFINE FIELD position ON TABLE project_role TYPE option<int>;"),
            "position should be option<int>:\n{sql}"
        );
        assert!(
            sql.contains("DEFINE FIELD permissions ON TABLE project_role TYPE option<string>;"),
            "permissions should be option<string>:\n{sql}"
        );
    }

    #[test]
    fn required_attribute_renders_bare_kind() {
        // An attribute with `options.required = Some(true)` skips the
        // optional wrapper. Constructed manually since the canonical
        // fixtures only carry the unset / Some(false) case.
        use ogar_vocab::{Attribute, Class as VocabClass};
        let mut attr = Attribute::new("subject");
        attr.type_name = Some("string".to_string());
        attr.options.required = Some(true);
        let mut c = VocabClass::new("WidgetThing");
        c.attributes.push(attr);
        let t = class_to_table(&c);
        let sql = t.to_sql();
        assert!(
            sql.contains("DEFINE FIELD subject ON TABLE widgetthing TYPE string;"),
            "required=true should drop the option<>: got\n{sql}"
        );
        assert!(
            !sql.contains("TYPE option<string>"),
            "required=true must not emit option<>: got\n{sql}"
        );
    }

    #[test]
    fn has_one_associations_are_skipped() {
        // `has_one` is the non-owning side of a 1:1 (the FK lives on
        // the target table), so emitting a local field is a phantom
        // link on the wrong side. Codex P2 on PR #52 caught the
        // original code emitting a field for HasOne via the wildcard
        // arm. Construct a class with a HasOne to make the regression
        // bite.
        use ogar_vocab::{Association, AssociationKind, Class as VocabClass};
        let mut c = VocabClass::new("Foo");
        c.associations
            .push(Association::new(AssociationKind::HasOne, "profile"));
        // Add a belongs_to sibling so the test also pins that BelongsTo
        // still emits (regression boundary check — the skip must be
        // surgical).
        c.associations
            .push(Association::new(AssociationKind::BelongsTo, "owner"));
        let t = class_to_table(&c);
        let sql = t.to_sql();
        assert!(
            !sql.contains("DEFINE FIELD profile ON TABLE"),
            "has_one `profile` should be skipped (FK on target):\n{sql}"
        );
        assert!(
            sql.contains("DEFINE FIELD owner ON TABLE foo TYPE option<record<owner>>;"),
            "belongs_to `owner` should emit:\n{sql}"
        );
    }

    #[test]
    fn belongs_to_target_falls_back_to_relation_name_when_class_name_unset() {
        // Codex P2 on PR #52: when `Association::class_name` is None
        // (the normal Rails case where the target class name is
        // inferred from the relation name), the target must come from
        // the relation name, NOT the literal `"any"`. The canonical
        // OGAR `family_edge` always sets class_name, so this exercises
        // the Rails-shape path with an explicitly-unset class_name.
        use ogar_vocab::{Association, AssociationKind, Class as VocabClass};
        let mut c = VocabClass::new("Foo");
        // belongs_to :project — no class_name set; Rails would infer
        // `Project` from the relation name. After the fix, the bridge
        // emits `record<project>` (snake-case of the relation), not
        // `record<any>` (the original phantom).
        c.associations
            .push(Association::new(AssociationKind::BelongsTo, "project"));
        let t = class_to_table(&c);
        let sql = t.to_sql();
        assert!(
            sql.contains("record<project>"),
            "belongs_to :project should target `project`:\n{sql}"
        );
        assert!(
            !sql.contains("record<any>"),
            "literal `any` is a phantom table — must not appear:\n{sql}"
        );
    }

    #[test]
    fn belongs_to_target_uses_explicit_class_name_when_set() {
        // The other side of the path: when `class_name` IS set (the
        // canonical OGAR `family_edge` shape, or Rails
        // `belongs_to :owner, class_name: 'User'`), the target
        // snake-cases the class_name. Pinned via project_work_item
        // which uses `family_edge("project", "Project")` — explicit
        // class_name = "Project" → `record<project>`.
        let class = project_work_item();
        let t = class_to_table(&class);
        let sql = t.to_sql();
        assert!(
            sql.contains("record<project>"),
            "explicit class_name 'Project' should snake-case to `project`:\n{sql}"
        );
    }
}
