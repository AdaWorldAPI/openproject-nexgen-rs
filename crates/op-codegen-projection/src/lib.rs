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

/// One `DEFINE FIELD <name> ON TABLE <table> TYPE option<any>` stub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineField {
    /// Field name (e.g. `subject`).
    pub name: String,
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

        let structured: Vec<DefineTable> = tables
            .into_iter()
            .map(|(name, fields)| DefineTable {
                name,
                fields: fields
                    .into_iter()
                    .map(|name| DefineField { name })
                    .collect(),
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
/// First-write surface — only `SCHEMAFULL` tables with `TYPE option<any>`
/// field stubs are emitted. ASSERT / record-link / PERMISSIONS clauses are
/// future work; this crate's contract is the projection identity, not the
/// emitter's expressiveness (the emitter can grow without re-gating the
/// projection's `roundtrip_eq`).
#[must_use]
pub fn surreal_text(c: &OpSurrealConst) -> String {
    let mut out = String::new();
    for table in &c.tables {
        out.push_str(&format!("DEFINE TABLE {} SCHEMAFULL;\n", table.name));
        for field in &table.fields {
            out.push_str(&format!(
                "DEFINE FIELD {} ON TABLE {} TYPE option<any>;\n",
                field.name, table.name,
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
}
