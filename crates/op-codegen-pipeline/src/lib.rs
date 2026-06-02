//! `op-codegen-pipeline` — end-to-end OpenProject codegen wiring.
//!
//! Connects the upstream ruff extraction crates (Sprint C5) to the codegen
//! projection (Sprint C8) with a **single bridge function** and a thin
//! `pipeline` helper. The vocabulary of every layer already aligns; this
//! crate exists to compose them, not to introduce new types.
//!
//! ```text
//!   Rails source ─► ruff_openproject::extract_triples ─► Vec<ruff::Triple>
//!                                                              │
//!                                              bridge_triples │ (field-for-field copy;
//!                                                              ▼  no semantic transform)
//!                                       Vec<lance_graph_contract::Triple>
//!                                                              │
//!                       OpSurrealProjection::project           ▼
//!                                                      OpSurrealConst
//!                                                              │
//!                            op_codegen_projection::surreal_text
//!                                                              ▼
//!                                              SurrealQL DDL text
//! ```
//!
//! # Why a bridge at all?
//!
//! [`ruff_spo_triplet::Triple`] and
//! [`lance_graph_contract::codegen_spine::Triple`] are **field-for-field
//! identical** structs (`s` / `p` / `o` / `f` / `c`) — same string IRIs,
//! same NARS truth — but Rust nominal typing makes them distinct types. The
//! bridge is a one-line per-field copy. We deliberately do NOT change either
//! upstream definition to share the type; both crates are upstream-neutral
//! and re-using their type would couple them.
//!
//! # End-to-end test, no I/O
//!
//! The integration test below constructs a synthetic [`ruff_spo_triplet::ModelGraph`]
//! (no filesystem walk, no Rails source) so the test is hermetic. The
//! Rails-extraction layer ([`ruff_openproject::extract_graph`]) is exercised
//! by the ruff crate's own tests; here we pin the projection wiring.

use lance_graph_contract::codegen_spine::Triple as SpineTriple;
use op_codegen_projection::{OpSurrealConst, OpSurrealProjection, surreal_text};
use ruff_spo_triplet::Triple as RuffTriple;

// `lance_graph_contract::codegen_spine::TripletProjection` is implemented
// on the projection type — we don't reference the trait directly here, only
// its associated method, which keeps the import surface narrow.
use lance_graph_contract::codegen_spine::TripletProjection;

// Re-exports so consumers depend on this one crate, not five.
pub use op_codegen_projection::{DefineField, DefineTable};
pub use ruff_openproject::{
    CORE_V3_RESOURCES, NAMESPACE, extract_core_triples, extract_graph, extract_triples,
    filter_to_core,
};

/// Bridge `ruff_spo_triplet::Triple` → `lance_graph_contract::Triple`
/// field-for-field. Pure copy; no semantic transform.
#[must_use]
pub fn bridge_triples(triples: &[RuffTriple]) -> Vec<SpineTriple> {
    triples
        .iter()
        .map(|t| SpineTriple {
            s: t.s.clone(),
            p: t.p.clone(),
            o: t.o.clone(),
            f: t.f,
            c: t.c,
        })
        .collect()
}

/// Drive ruff triples through the projection and emit SurrealQL DDL text in
/// one call. The intermediate [`OpSurrealConst`] is also returned so callers
/// that want the structured IR (or the opaque triple trail for round-trip
/// verification) don't have to re-project.
#[must_use]
pub fn render_surreal_from_ruff(triples: &[RuffTriple]) -> (OpSurrealConst, String) {
    let spine = bridge_triples(triples);
    let c = OpSurrealProjection::project(&spine);
    let text = surreal_text(&c);
    (c, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::codegen_spine::roundtrip_eq;
    use ruff_spo_triplet::{Field, Model, ModelGraph, expand};

    /// A minimal but realistic OpenProject-shaped graph: two models with
    /// columns, no functions (no compute graph noise). Mirrors the curated
    /// `CORE_V3_RESOURCES` shape.
    fn synthetic_op_graph() -> ModelGraph {
        let mut g = ModelGraph::new(NAMESPACE);
        let mut wp = Model::new("WorkPackage");
        wp.fields.push(Field {
            name: "subject".to_string(),
            depends_on: Vec::new(),
            emitted_by: None,
        });
        wp.fields.push(Field {
            name: "status".to_string(),
            depends_on: Vec::new(),
            emitted_by: None,
        });
        g.models.push(wp);

        let mut te = Model::new("TimeEntry");
        te.fields.push(Field {
            name: "hours".to_string(),
            depends_on: Vec::new(),
            emitted_by: None,
        });
        g.models.push(te);

        g
    }

    // ----- the bridge -----

    #[test]
    fn bridge_preserves_all_five_fields() {
        let ruff = vec![RuffTriple {
            s: "openproject:WorkPackage.subject".to_string(),
            p: "rdf:type".to_string(),
            o: "ogit:Property".to_string(),
            f: 1.0,
            c: 0.9,
        }];
        let spine = bridge_triples(&ruff);
        assert_eq!(spine.len(), 1);
        assert_eq!(spine[0].s, ruff[0].s);
        assert_eq!(spine[0].p, ruff[0].p);
        assert_eq!(spine[0].o, ruff[0].o);
        assert!((spine[0].f - ruff[0].f).abs() < f32::EPSILON);
        assert!((spine[0].c - ruff[0].c).abs() < f32::EPSILON);
    }

    #[test]
    fn bridge_preserves_order_and_count() {
        let ruff: Vec<RuffTriple> = (0..5)
            .map(|i| RuffTriple {
                s: format!("openproject:M{i}"),
                p: "rdf:type".to_string(),
                o: "ogit:ObjectType".to_string(),
                f: 1.0,
                c: 0.9,
            })
            .collect();
        let spine = bridge_triples(&ruff);
        assert_eq!(spine.len(), ruff.len());
        for (i, (r, s)) in ruff.iter().zip(spine.iter()).enumerate() {
            assert_eq!(r.s, s.s, "order preserved at position {i}");
        }
    }

    // ----- the full pipeline -----

    #[test]
    fn synthetic_graph_renders_expected_surql() {
        let triples = expand(&synthetic_op_graph());
        let (_, text) = render_surreal_from_ruff(&triples);

        // Deterministic alphabetical order (TimeEntry < WorkPackage),
        // schema-only emission (no compute graph since we declared no
        // functions).
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
    fn pipeline_const_carries_structured_ir_and_triple_trail() {
        let triples = expand(&synthetic_op_graph());
        let (c, _) = render_surreal_from_ruff(&triples);
        // Structured IR was projected.
        let names: Vec<&str> = c.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["TimeEntry", "WorkPackage"]);
        // And the opaque trail is the bridged input verbatim.
        assert_eq!(c.triples, bridge_triples(&triples));
    }

    #[test]
    fn pipeline_roundtrip_eq_on_real_ruff_output() {
        // The whole projection contract verified on a graph that came out
        // of `ruff_spo_triplet::expand` — not a hand-rolled test input.
        let triples = expand(&synthetic_op_graph());
        let spine = bridge_triples(&triples);
        roundtrip_eq::<OpSurrealProjection>(&spine)
            .expect("ruff-emitted triples must round-trip through OpSurrealProjection");
    }

    #[test]
    fn pipeline_handles_compute_graph_without_emitting_extra_tables() {
        // Add a function with reads/raises/traverses — those triples MUST
        // round-trip via the opaque trail but MUST NOT introduce phantom
        // tables in the structured IR.
        use ruff_spo_triplet::Function;
        let mut g = synthetic_op_graph();
        g.models[0].functions.push(Function {
            name: "compute_total_hours".to_string(),
            reads: vec!["status".to_string()],
            raises: vec!["ActiveRecord::RecordInvalid".to_string()],
            traverses: vec!["time_entries".to_string()],
        });

        let triples = expand(&g);
        let (c, _) = render_surreal_from_ruff(&triples);

        // Still exactly two tables.
        assert_eq!(c.tables.len(), 2);
        let names: Vec<&str> = c.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["TimeEntry", "WorkPackage"]);

        // And the compute triples are in the opaque trail.
        assert!(c.triples.iter().any(|t| t.p == "raises"));
        assert!(c.triples.iter().any(|t| t.p == "has_function"));

        // Strict roundtrip still holds on the augmented input.
        roundtrip_eq::<OpSurrealProjection>(&bridge_triples(&triples))
            .expect("compute-graph triples round-trip via opaque trail");
    }

    // ----- the core-resource filter, exercised end-to-end -----

    #[test]
    fn filter_to_core_keeps_known_models_through_pipeline() {
        let mut g = synthetic_op_graph();
        g.models.push(Model::new("AdHocUnknown")); // not in CORE_V3_RESOURCES

        filter_to_core(&mut g);
        let kept: Vec<&str> = g.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(kept, ["WorkPackage", "TimeEntry"]);

        // And the filtered graph still flows through the pipeline cleanly.
        let triples = expand(&g);
        let (c, text) = render_surreal_from_ruff(&triples);
        assert!(!text.contains("AdHocUnknown"));
        assert_eq!(c.tables.len(), 2);
    }

    // ----- re-export hygiene -----

    #[test]
    fn re_exports_match_upstream_constants() {
        assert_eq!(NAMESPACE, "openproject");
        assert!(CORE_V3_RESOURCES.iter().any(|r| *r == "WorkPackage"));
        assert!(CORE_V3_RESOURCES.iter().any(|r| *r == "TimeEntry"));
    }
}
