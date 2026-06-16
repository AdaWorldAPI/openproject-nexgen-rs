//! `from_triples` — D-AR-5 consumer: build a [`crate::Schema`] from the
//! SPO triples emitted by `ruff_ruby_spo` / `ruff_spo_triplet`.
//!
//! This is the consumer side of the
//! `openproject-ar-shape-extraction-v1` plan (PR Z): downstream of the
//! 27-predicate vocab + Model IR landed by `AdaWorldAPI/ruff#5` and the
//! lib-ruby-parser AST extractor landed by `AdaWorldAPI/ruff#6`, this
//! module takes a `Vec<Triple>` (via
//! `lance_graph_contract::codegen_spine::Triple`, the canonical
//! cross-language carrier) and projects it onto the typed
//! DDL AST in this crate.
//!
//! # Predicates consumed (skeleton)
//!
//! - `rdf:type` with object `ogit:ObjectType` → one
//!   [`crate::TableDefinition`] per subject.
//! - `has_attribute` → one [`crate::FieldDefinition`] per attribute on
//!   the owning table, `Kind::Any` (the OpenProject AR-shape vocab
//!   does not carry static types yet; D-AR-5.1 will fold in
//!   `attribute :x, :type` option-level type info).
//! - `declares_association` → one [`crate::FieldDefinition`] per
//!   association, `Kind::Record([<TargetClass>]).optional()`. Target
//!   class name follows Rails convention (camelcase singular of the
//!   relation name); the convention may be overridden by future
//!   `class_name:` option triples (D-AR-5.2).
//!
//! Every other predicate in the 34-name vocab is recognised by name
//! (so the catch-all `rdf:type`/`has_function`/etc. don't cause a
//! pass to fail) but does not yet influence the Schema — D-AR-5.1
//! folds in callbacks → SurrealQL events, validations → ASSERT,
//! scopes → TYPE RELATION, etc.
//!
//! # Round-trip status
//!
//! D-AR-5 is the **forward** projection only. The full
//! `TripletProjection` round-trip impl (decompile a `Schema` back
//! into `Vec<Triple>`) lands as D-AR-5.3 — it requires the full
//! attribute-options surface to recover the type slot.

use std::collections::BTreeMap;

use lance_graph_contract::codegen_spine::Triple;

use crate::{FieldDefinition, IndexDefinition, Kind, Schema, TableDefinition};

/// Project a flat triple stream into a typed [`Schema`].
///
/// Output table order: ASCII-sorted by table name (deterministic).
/// Field order within each table: insertion order from the triple
/// stream; the caller is responsible for any pre-sort.
///
/// Idempotent for duplicate triples (de-duplicated by `(s, p, o)`).
#[must_use]
pub fn triples_to_schema(triples: &[Triple]) -> Schema {
    // Group declarations by table IRI (the subject of an
    // `rdf:type` / `has_attribute` / `declares_association` triple).
    // Two-pass to avoid phantom tables: pass 1 finds every subject with
    // an explicit `rdf:type ObjectType` declaration; pass 2 only
    // populates fields/indices on tables seen in pass 1. This guards
    // against truncated triple streams where a body-walk predicate
    // like `reads_field openproject:Missing.name` would otherwise
    // materialise a phantom `Missing` table (codex P2-1).
    let mut tables: BTreeMap<String, TableBuilder> = BTreeMap::new();
    for t in triples {
        if t.p == "rdf:type" && t.o == "ogit:ObjectType" {
            let Some((table_iri, _)) = split_subject(&t.s) else {
                continue;
            };
            tables.entry(table_iri.clone()).or_insert_with(|| {
                TableBuilder::new(strip_namespace(&table_iri).to_string())
            });
        }
    }

    // Snapshot the set of known class names (post-strip-namespace) once,
    // so the inner loop can borrow `tables` mutably while still checking
    // membership. Polymorphic associations (`belongs_to :ownable,
    // polymorphic: true`) name a non-existent class; this set lets us
    // fall back to `option<any>` instead of inventing a phantom
    // `record<Ownable>`.
    let known_targets: std::collections::HashSet<String> = tables
        .values()
        .map(|tb| tb.name.clone())
        .collect();

    // Asserts are buffered and applied AFTER the field-population pass
    // so the `validates_constraint` → ASSERT wiring is independent of
    // triple stream order (codex P2 PR #27 r…). A
    // `validates_constraint` triple that lands before its companion
    // `has_attribute` would otherwise miss the field and drop the
    // assertion permanently.
    let mut pending_asserts: Vec<(String, String, String)> = Vec::new();

    for t in triples {
        let Some((table_iri, member)) = split_subject(&t.s) else {
            continue;
        };
        let Some(builder) = tables.get_mut(&table_iri) else {
            // Subject without an `rdf:type ObjectType` declaration —
            // drop body-walk / declarative triples that would otherwise
            // materialise a phantom table.
            continue;
        };

        match t.p.as_str() {
            "rdf:type" if t.o == "ogit:ObjectType" => {
                // Already added in the table-discovery pass above.
            }
            "has_attribute" => {
                let attr_name = member.unwrap_or_else(|| {
                    strip_namespace(&t.o).rsplit('.').next().unwrap_or("").to_string()
                });
                builder.add_field(attr_name, Kind::Any);
            }
            "declares_association" => {
                // Object is `openproject:WorkPackage.project` —
                // relation name is the last dotted segment, target
                // class follows Rails camelcase singular convention.
                let relation = strip_namespace(&t.o)
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let target = rails_target_class(&relation);
                // Only emit `record<Target>` when `Target` is a
                // discovered table (a class with an explicit
                // `rdf:type ObjectType` triple). Otherwise, the
                // association is polymorphic (`belongs_to :remindable,
                // polymorphic: true` — `Remindable` is a runtime type
                // discriminator, not a real table) or refers to an
                // external model the corpus didn't declare. Falling
                // back to `option<any>` keeps the schema correct
                // without inventing phantom record targets.
                let kind = if known_targets.contains(&target) {
                    Kind::Record(vec![target]).optional()
                } else {
                    Kind::Any.optional()
                };
                let field_name = format!("{relation}_id");
                if builder.add_field(field_name.clone(), kind) {
                    // Only emit the companion index when the field was
                    // newly added — guards against duplicate
                    // `declares_association` triples emitting duplicate
                    // `DEFINE INDEX` statements (codex P2-2).
                    let idx_name = format!("idx_{}_{field_name}", builder.name);
                    builder.add_index(idx_name, vec![field_name]);
                }
            }
            // ───── D-AR-5.1: Rails AR-shape → schema enrichment ─────
            "validates_constraint" => {
                // Triple shape: `(model, validates_constraint, <attr>)`.
                // Buffer the assertion; apply after all fields are
                // populated so stream-order doesn't cause drops.
                //
                // The Rails validation options are NOT carried on the
                // triple (they live on `Validation::options` in the IR
                // but `expand()` drops them); the schema-level
                // `ASSERT $value != NONE` is the most general
                // constraint we can express without re-parsing — it
                // asserts "this attribute must not be null", which is
                // the most common (and load-bearing) Rails validation
                // effect.
                pending_asserts.push((
                    table_iri.clone(),
                    t.o.clone(),
                    "$value != NONE".to_string(),
                ));
            }
            "normalizes_attribute" => {
                // `normalizes :attr, with: ->(v) { … }` — the
                // transformation runs on assignment but does NOT imply
                // presence; the column can still be nullable (codex
                // P2 PR #27: `ASSERT $value != NONE` would reject NULL
                // on a nullable normalized column). Surface it as a
                // table-level annotation only until a future sprint
                // lowers the lambda to a SurrealQL `VALUE` expression.
                builder.add_annotation(format!("normalize:{}", t.o));
            }
            "acts_as" => {
                // Triple shape: `(model, acts_as, "<variant>[:<options>]")`.
                // Record the variant in the table-level comment so
                // a downstream consumer can see "this table is
                // `acts_as_list`/`acts_as_tree`/etc.".
                let variant = t.o.split(':').next().unwrap_or(&t.o).to_string();
                builder.add_annotation(format!("acts_as_{variant}"));
            }
            "has_callback" => {
                // Triple shape: `(model, has_callback, "<phase>:<target>")`.
                // The schema can't yet render Ruby callback bodies,
                // but the table-level comment surfaces them so a
                // human or downstream tool can see the lifecycle hooks.
                builder.add_annotation(format!("callback:{}", t.o));
            }
            "includes_module" => {
                // Concerns + STI parents. Some are domain (e.g. `Acts::Customizable`),
                // others are STI parents (`Issue` for `WorkPackage`).
                // Emit a compact table-level note; D-AR-5.3 may split
                // STI parents off into a dedicated `inherits:` slot.
                builder.add_annotation(format!("include:{}", t.o));
            }
            // Other predicates (has_function, reads_field, raises,
            // traverses_relation, delegates_to, has_scope, has_default_scope,
            // aliases_method, aliases_attribute, defines_method, uses_refinement,
            // column_override, extends_module, prepends_module, concern_*,
            // gem DSL, registers_journal_*, has_dsl_call) carry method-body
            // semantics that don't lower cleanly to SurrealQL DDL today.
            // D-AR-5.3 lifts these into SurrealQL `DEFINE FUNCTION` /
            // `DEFINE EVENT` once the Ruby→SurrealQL body lowering is
            // wired (separate workstream).
            _ => {}
        }
    }

    // Apply buffered asserts now that every `has_attribute` /
    // `declares_association` triple has populated its field. No-op
    // for asserts whose target field doesn't exist (the phantom-field
    // guard from codex P2 still holds — validations on un-extracted
    // DB columns drop silently until D-AR-3.7 wires schema.rb).
    for (table_iri, field_name, expr) in pending_asserts {
        if let Some(builder) = tables.get_mut(&table_iri) {
            builder.add_field_assert(&field_name, &expr);
        }
    }

    let mut schema = Schema::new();
    for (_iri, builder) in tables {
        schema.tables.push(builder.build());
    }
    schema
}

/// Split a subject IRI into `(table_iri, member?)`.
///
/// `"openproject:WorkPackage"` → `("openproject:WorkPackage", None)`.
/// `"openproject:WorkPackage.subject"` → `("openproject:WorkPackage", Some("subject"))`.
fn split_subject(s: &str) -> Option<(String, Option<String>)> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(dot) = trimmed.find('.') {
        Some((trimmed[..dot].to_string(), Some(trimmed[dot + 1..].to_string())))
    } else {
        Some((trimmed.to_string(), None))
    }
}

/// Strip the `openproject:` namespace prefix (if present). Other
/// prefixes pass through unchanged so e.g. `exc:UserError` stays
/// intact.
fn strip_namespace(s: &str) -> &str {
    s.strip_prefix("openproject:").unwrap_or(s)
}

/// Apply the Rails AR convention: a relation name (`time_entries`,
/// `project`) maps to a target class name (`TimeEntry`, `Project`).
///
/// Algorithm: singularize the trailing `s`/`es`/`ies` (best-effort),
/// then split on `_` and camelcase each segment.
fn rails_target_class(relation: &str) -> String {
    let singular = singularize(relation);
    singular
        .split('_')
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

/// Best-effort singularization. Handles the common Rails cases:
/// `entries` → `entry`, `accesses` → `access`, `users` → `user`,
/// `boxes` → `box`. Cases not covered fall through unchanged
/// (Rails has hundreds of irregular forms; the D-AR-5.2 sprint can
/// wire a fuller table if the catch-all hurts).
fn singularize(s: &str) -> String {
    if let Some(stem) = s.strip_suffix("ies") {
        format!("{stem}y")
    } else if let Some(stem) = s.strip_suffix("ses") {
        format!("{stem}s")
    } else if let Some(stem) = s.strip_suffix("xes") {
        format!("{stem}x")
    } else if let Some(stem) = s.strip_suffix('s') {
        if stem.ends_with('s') {
            // e.g. "access" → keep as-is (the source already lacked a plural-s)
            s.to_string()
        } else {
            stem.to_string()
        }
    } else {
        s.to_string()
    }
}

/// Mutable builder for one table — captures fields and indexes as
/// they arrive, deferring the `TableDefinition::with_*` chain until
/// the final `build()` so insertion order is preserved.
struct TableBuilder {
    name: String,
    fields: Vec<FieldDefinition>,
    indices: Vec<IndexDefinition>,
    seen_fields: std::collections::HashSet<String>,
    /// Table-level AR-shape facts to fold into `TableDefinition.comment`
    /// (D-AR-5.1). Captured here in insertion order, deduplicated at
    /// `build()` time, then joined into a single `// <fact>; <fact>; …`
    /// comment string.
    annotations: Vec<String>,
}

impl TableBuilder {
    fn new(name: String) -> Self {
        Self {
            name,
            fields: Vec::new(),
            indices: Vec::new(),
            seen_fields: std::collections::HashSet::new(),
            annotations: Vec::new(),
        }
    }

    /// Returns `true` if the field was newly added, `false` if a field
    /// of that name was already present and the call was a no-op. The
    /// caller uses this to gate companion-index emission so duplicate
    /// declarations don't produce duplicate `DEFINE INDEX` statements.
    fn add_field(&mut self, name: String, kind: Kind) -> bool {
        if self.seen_fields.insert(name.clone()) {
            self.fields
                .push(FieldDefinition::new(name, self.name.clone(), kind));
            true
        } else {
            false
        }
    }

    fn add_index(&mut self, name: String, fields: Vec<String>) {
        self.indices
            .push(IndexDefinition::new(name, self.name.clone(), fields));
    }

    /// Attach a `validates_constraint`-style ASSERT clause to the
    /// existing field with the matching name. No-op if the field
    /// doesn't exist on this table (Rails can validate DB columns
    /// that the AR-shape extractor doesn't surface via
    /// `has_attribute` — those land as `Vec::new()` field stubs
    /// until D-AR-3.7 wires `db/schema.rb`).
    ///
    /// Last-writer-wins on multiple validations of the same attribute
    /// — Rails composes them at runtime; the schema captures the most
    /// recent fact.
    fn add_field_assert(&mut self, field_name: &str, expr: &str) {
        if let Some(field) = self.fields.iter_mut().find(|f| f.name == field_name) {
            field.assert = Some(expr.to_string());
        }
    }

    /// Push an AR-shape fact onto the table-level annotation list.
    /// Deduplicated at `build()` time.
    fn add_annotation(&mut self, note: String) {
        self.annotations.push(note);
    }

    fn build(mut self) -> TableDefinition {
        // Dedup-preserving-order: drop second+ occurrences of an
        // identical annotation. A model that `include`s `Acts::List`
        // and `acts_as_list` will produce two `include:` and one
        // `acts_as_list` annotation — the first survives, repeats drop.
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.annotations.retain(|a| seen.insert(a.clone()));

        let comment = if self.annotations.is_empty() {
            None
        } else {
            Some(self.annotations.join("; "))
        };

        let mut t = TableDefinition::new(self.name).with_comment(comment);
        for f in self.fields {
            t = t.with_field(f);
        }
        for i in self.indices {
            t = t.with_index(i);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToSql;

    fn t(s: &str, p: &str, o: &str) -> Triple {
        Triple {
            s: s.to_string(),
            p: p.to_string(),
            o: o.to_string(),
            f: 1.0,
            c: 1.0,
        }
    }

    #[test]
    fn rdf_type_object_creates_table() {
        let triples = vec![t(
            "openproject:WorkPackage",
            "rdf:type",
            "ogit:ObjectType",
        )];
        let schema = triples_to_schema(&triples);
        assert_eq!(schema.tables.len(), 1);
        assert_eq!(schema.tables[0].name, "WorkPackage");
    }

    #[test]
    fn has_attribute_adds_field_with_any_kind() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        assert_eq!(wp.fields.len(), 1);
        assert_eq!(wp.fields[0].name, "subject");
        assert_eq!(wp.fields[0].kind, Kind::Any);
    }

    #[test]
    fn declares_association_adds_record_field_and_index() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // Project MUST be a known table for the FK to lower to
            // `record<Project>` — without it, the association would
            // fall back to `option<any>` (polymorphic-safe default).
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = schema
            .tables
            .iter()
            .find(|t| t.name == "WorkPackage")
            .unwrap();
        // FieldDefinition: project_id : option<record<Project>>
        let project = wp.fields.iter().find(|f| f.name == "project_id").unwrap();
        assert_eq!(
            project.kind,
            Kind::Record(vec!["Project".to_string()]).optional()
        );
        // Companion IndexDefinition on project_id.
        assert!(
            wp.indices.iter().any(|i| i.name == "idx_WorkPackage_project_id"),
            "expected an index on project_id"
        );
    }

    /// **Polymorphic-association regression** — `belongs_to :ownable,
    /// polymorphic: true` names a runtime type discriminator
    /// (`Ownable`), not a real table. The FK must fall back to
    /// `option<any>` instead of inventing a phantom `record<Ownable>`
    /// pointing at a table that doesn't exist.
    #[test]
    fn polymorphic_association_falls_back_to_option_any() {
        let triples = vec![
            t("openproject:Reminder", "rdf:type", "ogit:ObjectType"),
            // No `Remindable` class — polymorphic association.
            t(
                "openproject:Reminder",
                "declares_association",
                "openproject:Reminder.remindable",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let reminder = &schema.tables[0];
        let fk = reminder
            .fields
            .iter()
            .find(|f| f.name == "remindable_id")
            .unwrap();
        // Polymorphic → `option<any>`, NOT `option<record<Remindable>>`.
        assert_eq!(
            fk.kind,
            Kind::Any.optional(),
            "polymorphic association must fall back to option<any>; got {:?}",
            fk.kind,
        );
        // The companion index still exists — SurrealQL allows indexes
        // on any-typed columns and the `_id` FK pattern stays
        // queryable.
        assert!(
            reminder
                .indices
                .iter()
                .any(|i| i.name == "idx_Reminder_remindable_id")
        );
    }

    /// **Polymorphic-vs-known-target lock** — a mixed triple set
    /// where one association targets a known table and another
    /// targets a polymorphic name produces the correct mix:
    /// `record<X>` for the known, `option<any>` for the polymorphic.
    #[test]
    fn mixed_known_and_polymorphic_associations_lower_correctly() {
        let triples = vec![
            t("openproject:Member", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            // user is a known table → record<User>
            t(
                "openproject:Member",
                "declares_association",
                "openproject:Member.user",
            ),
            // entity is polymorphic → option<any>
            t(
                "openproject:Member",
                "declares_association",
                "openproject:Member.entity",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let member = schema
            .tables
            .iter()
            .find(|t| t.name == "Member")
            .unwrap();
        let user_fk = member.fields.iter().find(|f| f.name == "user_id").unwrap();
        let entity_fk = member.fields.iter().find(|f| f.name == "entity_id").unwrap();
        assert_eq!(
            user_fk.kind,
            Kind::Record(vec!["User".to_string()]).optional()
        );
        assert_eq!(entity_fk.kind, Kind::Any.optional());
    }

    #[test]
    fn singularize_handles_common_rails_plurals() {
        assert_eq!(singularize("entries"), "entry");
        assert_eq!(singularize("users"), "user");
        assert_eq!(singularize("boxes"), "box");
        assert_eq!(singularize("accesses"), "access");
        assert_eq!(singularize("project"), "project");
    }

    #[test]
    fn rails_target_class_camelcases_compound_names() {
        assert_eq!(rails_target_class("time_entries"), "TimeEntry");
        assert_eq!(rails_target_class("work_packages"), "WorkPackage");
        assert_eq!(rails_target_class("project"), "Project");
    }

    #[test]
    fn tables_sorted_by_name_in_output() {
        let triples = vec![
            t("openproject:Zebra", "rdf:type", "ogit:ObjectType"),
            t("openproject:Alpha", "rdf:type", "ogit:ObjectType"),
            t("openproject:Mango", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "Mango", "Zebra"]);
    }

    #[test]
    fn duplicate_has_attribute_collapses() {
        let triples = vec![
            t("openproject:WP", "rdf:type", "ogit:ObjectType"),
            t("openproject:WP", "has_attribute", "subject"),
            t("openproject:WP", "has_attribute", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        assert_eq!(schema.tables[0].fields.len(), 1);
    }

    /// **Codex P2 regression (PR #26 r3418308887)** — a body-walk
    /// predicate on a subject that lacks an `rdf:type ObjectType`
    /// declaration must NOT materialise a phantom table.
    #[test]
    fn body_walk_predicate_without_rdf_type_does_not_create_phantom_table() {
        let triples = vec![
            // Note: NO `rdf:type ObjectType` for `Missing`.
            t(
                "openproject:Missing.some_fn",
                "reads_field",
                "openproject:Missing.name",
            ),
            // A real table to confirm the filter is precise.
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["WorkPackage"]);
    }

    /// **Codex P2 regression (PR #26 r3418308894)** — a duplicate
    /// `declares_association` triple must NOT produce a duplicate
    /// `DEFINE INDEX` statement (the field is deduped via
    /// `seen_fields`; the companion index must follow the field's
    /// add-or-skip decision).
    #[test]
    fn duplicate_declares_association_does_not_emit_duplicate_index() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        // Field is deduped (the pre-existing guarantee).
        assert_eq!(wp.fields.len(), 1);
        // Index follows the same deduplication.
        let project_indices: Vec<_> = wp
            .indices
            .iter()
            .filter(|i| i.name == "idx_WorkPackage_project_id")
            .collect();
        assert_eq!(
            project_indices.len(),
            1,
            "duplicate declares_association produced duplicate index",
        );
    }

    /// **D-AR-5 end-to-end** — a multi-class triple set produces a
    /// SurrealQL output that matches the hand-built shape from the
    /// crate's own `rails_mini_e2e_byte_for_byte_with_legacy_emission`
    /// test (PR #19 baseline). Differences allowed: `subject` becomes
    /// `Kind::Any` not `Kind::Int` (triples don't carry types yet),
    /// and the index name uses the `_id`-suffixed field.
    #[test]
    fn end_to_end_triples_render_to_define_ddl() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            t("openproject:Project", "has_attribute", "identifier"),
        ];
        let schema = triples_to_schema(&triples);
        let sql = schema.to_sql();
        assert!(sql.contains("DEFINE TABLE WorkPackage SCHEMAFULL;"));
        assert!(sql.contains("DEFINE FIELD subject ON TABLE WorkPackage TYPE any;"));
        assert!(sql.contains(
            "DEFINE FIELD project_id ON TABLE WorkPackage TYPE option<record<Project>>;"
        ));
        assert!(sql.contains(
            "DEFINE INDEX idx_WorkPackage_project_id ON TABLE WorkPackage FIELDS project_id;"
        ));
        assert!(sql.contains("DEFINE TABLE Project SCHEMAFULL;"));
        assert!(sql.contains("DEFINE FIELD identifier ON TABLE Project TYPE any;"));
    }

    // ────────────────── D-AR-5.1 enrichment tests ──────────────────

    /// **D-AR-5.1** — `validates_constraint` triple targets a field
    /// declared via `has_attribute`. The schema gains an
    /// `ASSERT $value != NONE` clause on that field's DEFINE FIELD.
    #[test]
    fn validates_constraint_adds_assert_to_matching_field() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
            t("openproject:WorkPackage", "validates_constraint", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let subj = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert_eq!(
            subj.assert.as_deref(),
            Some("$value != NONE"),
            "validates_constraint must wire ASSERT onto matching field",
        );
        // Field with no validation has no assert.
        let triples2 = vec![
            t("openproject:Plain", "rdf:type", "ogit:ObjectType"),
            t("openproject:Plain", "has_attribute", "anything"),
        ];
        let schema2 = triples_to_schema(&triples2);
        let plain = &schema2.tables[0];
        let anything = plain.fields.iter().find(|f| f.name == "anything").unwrap();
        assert_eq!(anything.assert, None);
    }

    /// **D-AR-5.1** — validation on an attribute we don't extract
    /// (e.g. a DB column from `db/schema.rb`) is silently dropped.
    /// The constraint is preserved IF and only IF the field exists.
    /// This guards against materialising phantom assert-only fields.
    #[test]
    fn validates_constraint_on_unknown_field_is_noop() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // No `has_attribute` for `nonexistent` — validation has no
            // matching field to attach to.
            t(
                "openproject:WorkPackage",
                "validates_constraint",
                "nonexistent",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        assert!(
            wp.fields.is_empty(),
            "validates_constraint must not materialise a phantom field; got {:?}",
            wp.fields,
        );
    }

    /// **D-AR-5.1** — assert is rendered after TYPE in SurrealQL.
    #[test]
    fn assert_clause_renders_after_type_in_surrealql() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
            t("openproject:WorkPackage", "validates_constraint", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let sql = schema.to_sql();
        assert!(
            sql.contains(
                "DEFINE FIELD subject ON TABLE WorkPackage TYPE any ASSERT $value != NONE;"
            ),
            "expected ASSERT clause in rendered SQL; got: {sql}",
        );
    }

    /// **D-AR-5.1** — `acts_as`, `has_callback`, and `includes_module`
    /// triples land as a deduplicated `COMMENT '<facts>'` clause on
    /// the table.
    #[test]
    fn ar_shape_facts_aggregate_into_table_comment() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "acts_as", "list"),
            t("openproject:WorkPackage", "acts_as", "watchable"),
            t(
                "openproject:WorkPackage",
                "has_callback",
                "before_save:set_default_status",
            ),
            t(
                "openproject:WorkPackage",
                "includes_module",
                "Acts::Customizable",
            ),
            // Dedup: a second identical includes_module triple must NOT
            // produce a duplicate annotation in the comment.
            t(
                "openproject:WorkPackage",
                "includes_module",
                "Acts::Customizable",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let comment = wp
            .comment
            .as_deref()
            .expect("expected comment with AR facts");
        assert!(comment.contains("acts_as_list"));
        assert!(comment.contains("acts_as_watchable"));
        assert!(comment.contains("callback:before_save:set_default_status"));
        assert!(comment.contains("include:Acts::Customizable"));
        // Dedup: `Acts::Customizable` appears once.
        assert_eq!(
            comment.matches("Acts::Customizable").count(),
            1,
            "duplicate includes_module triple must dedup in comment",
        );
    }

    /// **D-AR-5.1** — comment is rendered in the DEFINE TABLE line.
    #[test]
    fn table_comment_renders_in_define_table_line() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "acts_as", "list"),
        ];
        let schema = triples_to_schema(&triples);
        let sql = schema.to_sql();
        assert!(
            sql.contains("DEFINE TABLE WorkPackage SCHEMAFULL COMMENT 'acts_as_list';"),
            "expected COMMENT clause in DEFINE TABLE; got: {sql}",
        );
    }

    /// **Codex P2 (PR #27 r…)** — `normalizes_attribute` does NOT
    /// imply presence: `normalizes :email` allows the column to stay
    /// nullable. The previous emission of `ASSERT $value != NONE`
    /// would reject `NONE` on what is a legitimately nullable
    /// normalized column. The fix: surface normalization as a
    /// table-level annotation only (`normalize:<attr>`); the field's
    /// `assert` stays `None` unless a real `validates_constraint`
    /// triple fires.
    #[test]
    fn normalizes_attribute_does_not_force_non_null_assert() {
        let triples = vec![
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "has_attribute", "email"),
            t("openproject:User", "normalizes_attribute", "email"),
        ];
        let schema = triples_to_schema(&triples);
        let user = &schema.tables[0];
        let email = user.fields.iter().find(|f| f.name == "email").unwrap();
        // Field stays nullable (no ASSERT).
        assert_eq!(
            email.assert, None,
            "normalize must NOT force $value != NONE",
        );
        // Annotation surfaces the normalization fact at the table level.
        assert!(
            user.comment
                .as_deref()
                .is_some_and(|c| c.contains("normalize:email")),
            "expected `normalize:email` in table COMMENT; got {:?}",
            user.comment,
        );
    }

    /// **Codex P2 (PR #27 r…)** — `validates_constraint` must be
    /// stream-order-independent. A triple set with the constraint
    /// listed BEFORE the field-defining `has_attribute` must still
    /// land the ASSERT on the field after population.
    #[test]
    fn validates_constraint_order_independent_with_has_attribute() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // Validation arrives FIRST.
            t("openproject:WorkPackage", "validates_constraint", "subject"),
            // Field arrives SECOND.
            t("openproject:WorkPackage", "has_attribute", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let subj = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert_eq!(
            subj.assert.as_deref(),
            Some("$value != NONE"),
            "validates_constraint must apply regardless of triple order",
        );
    }

    /// **D-AR-5.1** — a phantom-table guard still holds for the new
    /// predicates: a `validates_constraint` / `acts_as` / `has_callback`
    /// triple on an undeclared subject must NOT materialise a table.
    /// (Same invariant the codex P2-1 fix introduced for `has_attribute`.)
    #[test]
    fn new_predicates_respect_phantom_table_guard() {
        let triples = vec![
            // No `rdf:type ObjectType` for `Ghost`.
            t("openproject:Ghost", "acts_as", "list"),
            t(
                "openproject:Ghost",
                "has_callback",
                "before_save:hook",
            ),
            t("openproject:Ghost", "validates_constraint", "field"),
            // Real table to confirm the filter is precise.
            t("openproject:Real", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Real"]);
    }
}
