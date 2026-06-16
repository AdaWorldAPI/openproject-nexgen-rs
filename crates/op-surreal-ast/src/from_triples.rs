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
                let kind = Kind::Record(vec![target.clone()]).optional();
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
            // Every other predicate is recognised by being part of the
            // 34-name closed vocab; the skeleton ignores them. D-AR-5.1
            // wires the remaining 25 predicates into Schema slots
            // (callbacks → events, validations → ASSERT, etc.).
            _ => {}
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
}

impl TableBuilder {
    fn new(name: String) -> Self {
        Self {
            name,
            fields: Vec::new(),
            indices: Vec::new(),
            seen_fields: std::collections::HashSet::new(),
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

    fn build(self) -> TableDefinition {
        let mut t = TableDefinition::new(self.name);
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
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
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
}
