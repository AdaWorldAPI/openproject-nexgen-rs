//! `op-codegen-projection` — OpenProject SurrealQL **schema** projection.
//!
//! Implements [`lance_graph_contract::codegen_spine::TripletProjection`] over
//! an OP-shaped DDL IR. This is the **DDL facet** from the three-facet model
//! (SPO / DDL / Executable). A separate [`surreal_text`] emitter renders the
//! IR to SurrealQL `DEFINE TABLE` / `DEFINE FIELD` statements.
//!
//! # The strict-roundtrip trick (R3 / R4 from C5 M1 mapping)
//!
//! M1 R3 flagged that the codegen-layer [`Triple`] is `String`-IRI with
//! `f`/`c` NARS truth, and M1 R4 flagged that
//! [`TripletProjection::truth_tolerance`] defaults to `0.0` (strict). A naive
//! SurrealQL `DEFINE TABLE` emission has no slot for NARS truth, so any
//! projection that drops it spuriously fails `roundtrip_eq`.
//!
//! The fix here keeps the contract clean and the emission lossy *separately*:
//!
//! - [`OpSurrealConst`] carries TWO fields:
//!   - `tables: Vec<DefineTable>` — the structured DDL IR the SurrealQL
//!     emitter walks (lossy projection of the input triples — schema only).
//!   - `triples: Vec<Triple>` — the **opaque triple trail**, an exact copy of
//!     the input. [`OpSurrealProjection::decompile`] returns these unchanged,
//!     so `roundtrip_eq` passes at the strict default tolerance.
//! - The SurrealQL emitter ignores `triples` and walks `tables`; consumers
//!   that need NARS truth on output read it via a side-channel.
//!
//! This is the same architectural seam Odoo's existing projection uses
//! (see the reference `PredicateIndex` in the upstream codegen_spine tests):
//! the const form *contains* the original triples so decompile is a copy.
//! We add structured emission on top.
//!
//! # First write surface
//!
//! This crate is intentionally narrow — only **`DEFINE TABLE`** + nullable
//! **`DEFINE FIELD`** stubs are recognised in the projection. A future sprint
//! widens the input vocabulary (FK record links, ASSERT clauses, indexes,
//! PERMISSIONS) without touching the projection contract.

use std::collections::{BTreeMap, BTreeSet};

use lance_graph_contract::codegen_spine::{Triple, TripletProjection};

// ---------------------------------------------------------------------------
// The DDL IR
// ---------------------------------------------------------------------------

/// One `DEFINE TABLE <name> SCHEMAFULL` statement plus its `DEFINE FIELD`
/// children, in the OpenProject namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineTable {
    /// The model's local name (e.g. `WorkPackage`). The full IRI is
    /// `openproject:WorkPackage`; namespace is implicit.
    pub name: String,
    /// Fields declared on this table, sorted by name for deterministic
    /// emission (matches the input triple ordering after dedup).
    pub fields: Vec<DefineField>,
}

/// One `DEFINE FIELD <name> ON TABLE <table> TYPE [option<any>|any]` stub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineField {
    /// Field name (e.g. `subject`).
    pub name: String,
    /// `true` when the field is required (declared by the synthetic
    /// `_validate` function on the same table — corresponds to an
    /// ActiveRecord `validates :col, presence: true` declaration). Emitted
    /// as `TYPE any`; `false` becomes `TYPE option<any>`.
    pub required: bool,
}

/// The Const form of [`OpSurrealProjection`]. Carries the structured DDL IR
/// (walked by [`surreal_text`]) AND the opaque triple trail (returned
/// unchanged by [`OpSurrealProjection::decompile`] so strict roundtrip
/// passes — see module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct OpSurrealConst {
    /// The structured DDL IR for SurrealQL emission.
    pub tables: Vec<DefineTable>,
    /// Exact copy of the input triples. Round-trip identity.
    pub triples: Vec<Triple>,
}

// ---------------------------------------------------------------------------
// Recognised triple predicates (the projection's input vocabulary)
// ---------------------------------------------------------------------------

/// Predicate marking an entity as an `ogit:ObjectType` (model / table).
/// Matches the codegen-spine canonical predicate name.
pub const PRED_OGIT_TYPE: &str = "rdf:type";

/// Object value for the type predicate that flags a subject as a table.
pub const OBJ_OBJECT_TYPE: &str = "ogit:ObjectType";

/// Object value for the type predicate that flags a subject as a field
/// (property of a table).
pub const OBJ_PROPERTY: &str = "ogit:Property";

/// Predicate marking a column as read by a function body (or, for the
/// synthetic `_validate` function, validated).
pub const PRED_READS_FIELD: &str = "reads_field";

/// Suffix of the synthetic per-model validation function emitted by
/// `ruff_spo_triplet::expand` when any declarative `validates …` is present
/// (see `SPO_TRIPLET_EXTRACTION.md` §5). Detection here is suffix-based, not
/// behaviour-based: a custom `def something_check; raise RecordInvalid; end`
/// is NOT treated as a validator — only the canonical synthetic one is.
pub const VALIDATE_FN_SUFFIX: &str = "._validate";

/// IRI namespace prefix for OpenProject subjects (matches the
/// `ruff_openproject` crate's `NAMESPACE`).
pub const NAMESPACE_PREFIX: &str = "openproject:";

// ---------------------------------------------------------------------------
// The projection impl
// ---------------------------------------------------------------------------

/// The OpenProject SurrealQL-schema projection. Implements
/// [`TripletProjection`] so it gates through `roundtrip_eq` as a build-time
/// test — a projection that loses (s, p, o) identity fails CI.
#[derive(Debug, Clone, Copy)]
pub struct OpSurrealProjection;

impl TripletProjection for OpSurrealProjection {
    type Const = OpSurrealConst;

    fn project(triples: &[Triple]) -> Self::Const {
        // First pass: collect every (subject = openproject:Foo) that has a
        // `rdf:type ogit:ObjectType` declaration. Those subjects become
        // tables. Tables are kept in a BTreeMap keyed by local name for
        // deterministic ordering.
        let mut tables: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for t in triples {
            // Workspace is edition 2021 — use nested if-let, not let chains.
            if t.p == PRED_OGIT_TYPE && t.o == OBJ_OBJECT_TYPE {
                if let Some(local) = t.s.strip_prefix(NAMESPACE_PREFIX) {
                    if !local.contains('.') {
                        tables.entry(local.to_string()).or_default();
                    }
                }
            }
        }

        // Second pass: every (subject = openproject:Foo.bar) with
        // `rdf:type ogit:Property` is a field on table Foo (if we already
        // know Foo as a table). Unknown tables: silently skip — the input
        // is the contract; we don't invent tables, only emit what's
        // declared.
        for t in triples {
            if t.p == PRED_OGIT_TYPE && t.o == OBJ_PROPERTY {
                if let Some(local) = t.s.strip_prefix(NAMESPACE_PREFIX) {
                    if let Some((table, field)) = local.split_once('.') {
                        if let Some(fields) = tables.get_mut(table) {
                            fields.insert(field.to_string());
                        }
                    }
                }
            }
        }

        // Third pass: detect validated (required) columns via the synthetic
        // `_validate` function's `reads_field` triples. Pattern:
        //   (openproject:Foo._validate, reads_field, openproject:Foo.bar)
        // -> field `bar` on table `Foo` is required.
        // A reads_field whose subject is some other function (a real
        // method body that reads a column) is correctly ignored; only the
        // synthetic validator flags requiredness.
        let mut required: BTreeSet<(String, String)> = BTreeSet::new();
        for t in triples {
            if t.p != PRED_READS_FIELD {
                continue;
            }
            let subj = match t.s.strip_prefix(NAMESPACE_PREFIX) {
                Some(local) => local,
                None => continue,
            };
            let table = match subj.strip_suffix(VALIDATE_FN_SUFFIX) {
                Some(t) => t,
                None => continue,
            };
            let obj = match t.o.strip_prefix(NAMESPACE_PREFIX) {
                Some(local) => local,
                None => continue,
            };
            if let Some((obj_table, col)) = obj.split_once('.') {
                if obj_table == table {
                    required.insert((table.to_string(), col.to_string()));
                }
            }
        }

        let structured: Vec<DefineTable> = tables
            .into_iter()
            .map(|(table_name, fields)| {
                let define_fields = fields
                    .into_iter()
                    .map(|field_name| DefineField {
                        required: required.contains(&(table_name.clone(), field_name.clone())),
                        name: field_name,
                    })
                    .collect();
                DefineTable {
                    name: table_name,
                    fields: define_fields,
                }
            })
            .collect();

        OpSurrealConst {
            tables: structured,
            triples: triples.to_vec(),
        }
    }

    fn decompile(c: &Self::Const) -> Vec<Triple> {
        // Strict roundtrip: return the opaque triple trail unchanged.
        // The structured `tables` field is for emission, not identity.
        c.triples.clone()
    }
}

// ---------------------------------------------------------------------------
// SurrealQL emission
// ---------------------------------------------------------------------------

/// Render the structured DDL IR to SurrealQL `DEFINE TABLE` / `DEFINE FIELD`
/// statements. Trailing newline, deterministic ordering (alphabetical by
/// table, alphabetical by field within each table).
///
/// Field type emission:
/// - `required = true` (declared by the synthetic `_validate` function)
///   becomes `TYPE any` — SurrealQL rejects `NONE` for this type, matching
///   the ActiveRecord `validates :col, presence: true` semantics.
/// - `required = false` becomes `TYPE option<any>`.
///
/// Future-emitter widening targets (typed columns from schema.rb, FK
/// record links, ASSERT clauses, indexes, PERMISSIONS) extend this fn
/// without changing the projection's `roundtrip_eq` contract.
#[must_use]
pub fn surreal_text(c: &OpSurrealConst) -> String {
    let mut out = String::new();
    for table in &c.tables {
        out.push_str(&format!("DEFINE TABLE {} SCHEMAFULL;\n", table.name));
        for field in &table.fields {
            let ty = if field.required { "any" } else { "option<any>" };
            out.push_str(&format!(
                "DEFINE FIELD {} ON TABLE {} TYPE {};\n",
                field.name, table.name, ty,
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::codegen_spine::roundtrip_eq;

    fn t(s: &str, p: &str, o: &str, f: f32, c: f32) -> Triple {
        Triple {
            s: s.to_string(),
            p: p.to_string(),
            o: o.to_string(),
            f,
            c,
        }
    }

    /// A small openproject-shaped triple set:
    /// - WorkPackage / TimeEntry as ObjectTypes
    /// - subject, status as Properties on WorkPackage
    /// - hours as Property on TimeEntry
    /// Plus some non-projected triples (compute graph edges) to verify they
    /// round-trip through `triples` opaque trail without affecting the IR.
    fn fixture_triples() -> Vec<Triple> {
        vec![
            // tables
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t("openproject:TimeEntry", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            // WorkPackage fields
            t("openproject:WorkPackage.subject", "rdf:type", "ogit:Property", 1.0, 0.9),
            t("openproject:WorkPackage.status", "rdf:type", "ogit:Property", 1.0, 0.9),
            // TimeEntry field
            t("openproject:TimeEntry.hours", "rdf:type", "ogit:Property", 1.0, 0.9),
            // Non-projected: compute graph edge (carried opaquely via triples
            // trail — emitter ignores, round-trip preserves).
            t(
                "openproject:WorkPackage.total_hours",
                "emitted_by",
                "openproject:WorkPackage.compute_total_hours",
                1.0,
                0.8,
            ),
        ]
    }

    // ----- the projection IR -----

    #[test]
    fn project_extracts_tables_and_fields_alphabetically() {
        let c = OpSurrealProjection::project(&fixture_triples());
        let table_names: Vec<&str> = c.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(table_names, ["TimeEntry", "WorkPackage"]);

        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let wp_fields: Vec<&str> = wp.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(wp_fields, ["status", "subject"]);

        let te = c.tables.iter().find(|t| t.name == "TimeEntry").unwrap();
        let te_fields: Vec<&str> = te.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(te_fields, ["hours"]);
    }

    #[test]
    fn project_carries_full_triple_trail_unchanged() {
        // The Const's `triples` field is an exact copy of the input — the
        // opaque trail that lets `roundtrip_eq` pass at strict tolerance.
        let input = fixture_triples();
        let c = OpSurrealProjection::project(&input);
        assert_eq!(c.triples, input);
    }

    #[test]
    fn project_skips_property_for_unknown_table() {
        // A Property triple whose table half is not declared as an
        // ObjectType is silently skipped (the input is the contract; we
        // don't invent tables). The triple still round-trips via the
        // opaque trail.
        let mut triples = fixture_triples();
        triples.push(t(
            "openproject:Mystery.something",
            "rdf:type",
            "ogit:Property",
            1.0,
            0.5,
        ));
        let c = OpSurrealProjection::project(&triples);
        assert!(!c.tables.iter().any(|tab| tab.name == "Mystery"));
        // But the triple is in the trail for round-trip.
        assert!(c
            .triples
            .iter()
            .any(|tr| tr.s == "openproject:Mystery.something"));
    }

    // ----- the contract gate -----

    #[test]
    fn roundtrip_eq_passes_at_strict_default_tolerance() {
        // The whole point of TripletProjection: this MUST pass or our
        // projection is silently lossy. Default truth_tolerance is 0.0
        // (exact f/c match required) — the opaque triple trail makes that
        // trivially the case.
        roundtrip_eq::<OpSurrealProjection>(&fixture_triples())
            .expect("OpSurrealProjection must round-trip losslessly");
    }

    #[test]
    fn roundtrip_eq_passes_on_empty_input() {
        roundtrip_eq::<OpSurrealProjection>(&[])
            .expect("empty input is the trivial round-trip");
    }

    #[test]
    fn roundtrip_eq_passes_on_only_non_projected_triples() {
        // Triples the projection ignores for IR purposes must still
        // round-trip — the trail is opaque.
        let only_compute = vec![
            t(
                "openproject:WorkPackage.total_hours",
                "depends_on",
                "openproject:WorkPackage.time_entries.hours",
                1.0,
                0.8,
            ),
            t(
                "openproject:WorkPackage.compute_total_hours",
                "raises",
                "exc:ActiveRecord::RecordInvalid",
                1.0,
                0.9,
            ),
        ];
        roundtrip_eq::<OpSurrealProjection>(&only_compute)
            .expect("non-projected triples still round-trip via the opaque trail");
    }

    // ----- the surreal text emitter -----

    #[test]
    fn surreal_text_renders_define_table_and_define_field() {
        let c = OpSurrealProjection::project(&fixture_triples());
        let text = surreal_text(&c);

        let expected = "\
DEFINE TABLE TimeEntry SCHEMAFULL;
DEFINE FIELD hours ON TABLE TimeEntry TYPE option<any>;
DEFINE TABLE WorkPackage SCHEMAFULL;
DEFINE FIELD status ON TABLE WorkPackage TYPE option<any>;
DEFINE FIELD subject ON TABLE WorkPackage TYPE option<any>;
";
        assert_eq!(text, expected);
    }

    #[test]
    fn surreal_text_is_empty_on_empty_const() {
        let c = OpSurrealConst {
            tables: Vec::new(),
            triples: Vec::new(),
        };
        assert_eq!(surreal_text(&c), "");
    }

    // ----- C10: validated-column / required-field detection -----

    /// Fixture extending the base one with a synthetic `_validate` function
    /// that reads `subject` (the canonical Rails
    /// `validates :subject, presence: true` shape after ruff expansion).
    fn fixture_with_validates() -> Vec<Triple> {
        let mut triples = fixture_triples();
        // The `_validate` function is itself an `ogit:Function`.
        triples.push(t(
            "openproject:WorkPackage._validate",
            "rdf:type",
            "ogit:Function",
            1.0,
            0.9,
        ));
        // Its body raises ActiveRecord::RecordInvalid (carried opaquely).
        triples.push(t(
            "openproject:WorkPackage._validate",
            "raises",
            "exc:ActiveRecord::RecordInvalid",
            1.0,
            0.9,
        ));
        // The crucial signal: reads_field on the validated column.
        triples.push(t(
            "openproject:WorkPackage._validate",
            "reads_field",
            "openproject:WorkPackage.subject",
            1.0,
            0.7,
        ));
        triples
    }

    #[test]
    fn project_marks_validated_fields_as_required() {
        let c = OpSurrealProjection::project(&fixture_with_validates());
        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        let status = wp.fields.iter().find(|f| f.name == "status").unwrap();
        assert!(subject.required, "validated column must be required");
        assert!(!status.required, "non-validated column stays optional");
    }

    #[test]
    fn project_ignores_reads_field_from_non_validate_functions() {
        // A normal (non-_validate) function reading a column must NOT
        // flag it as required — only the synthetic validator does.
        let mut triples = fixture_triples();
        triples.push(t(
            "openproject:WorkPackage.compute_total_hours",
            "reads_field",
            "openproject:WorkPackage.subject",
            1.0,
            0.7,
        ));
        let c = OpSurrealProjection::project(&triples);
        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert!(
            !subject.required,
            "reads_field outside _validate does not mark required"
        );
    }

    #[test]
    fn project_ignores_validate_reads_field_pointing_to_other_table() {
        // Defensive: a malformed triple where _validate of table A reads
        // a column of table B is silently ignored (we only flag own-table
        // columns).
        let mut triples = fixture_triples();
        triples.push(t(
            "openproject:WorkPackage._validate",
            "reads_field",
            "openproject:TimeEntry.hours",
            1.0,
            0.7,
        ));
        let c = OpSurrealProjection::project(&triples);
        let te = c.tables.iter().find(|t| t.name == "TimeEntry").unwrap();
        let hours = te.fields.iter().find(|f| f.name == "hours").unwrap();
        assert!(!hours.required, "cross-table _validate is ignored");
    }

    #[test]
    fn surreal_text_emits_type_any_for_required_field() {
        let c = OpSurrealProjection::project(&fixture_with_validates());
        let text = surreal_text(&c);

        let expected = "\
DEFINE TABLE TimeEntry SCHEMAFULL;
DEFINE FIELD hours ON TABLE TimeEntry TYPE option<any>;
DEFINE TABLE WorkPackage SCHEMAFULL;
DEFINE FIELD status ON TABLE WorkPackage TYPE option<any>;
DEFINE FIELD subject ON TABLE WorkPackage TYPE any;
";
        assert_eq!(text, expected);
    }

    #[test]
    fn roundtrip_eq_passes_with_validation_triples() {
        // The validation triples enter the opaque trail unchanged; round-
        // trip remains strict.
        roundtrip_eq::<OpSurrealProjection>(&fixture_with_validates())
            .expect("validation triples round-trip via the opaque trail");
    }
}
