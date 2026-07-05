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
use op_codegen_projection::{surreal_text, OpSurrealConst, OpSurrealProjection};
use ruff_spo_triplet::Triple as RuffTriple;

/// The ogar-emit consumer path (§5 steps 4-5): source -> ruff -> OGAR
/// lift/mint -> adapter emit, wired alongside (not replacing) the native
/// ruff -> op-surreal-ast -> SurrealQL path in this crate. Feature-gated
/// so the native path's dependency graph is untouched by default.
#[cfg(feature = "ogar-emit")]
pub mod ogar_consumer;

// `lance_graph_contract::codegen_spine::TripletProjection` is implemented
// on the projection type — we don't reference the trait directly here, only
// its associated method, which keeps the import surface narrow.
use lance_graph_contract::codegen_spine::TripletProjection;

// Re-exports so consumers depend on this one crate, not five.
pub use op_codegen_projection::{DefineField, DefineTable};
pub use ruff_openproject::{
    extract_core_triples, extract_graph, extract_triples, filter_to_core, CORE_V3_RESOURCES,
    NAMESPACE,
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

/// D-AR-3.5 typed path: extract WITH the schema stratum (baseline
/// migration-DSL columns merged into the model graph), filter to the
/// curated core, and expand to triples. Returns the triples plus the
/// conservation-ledger [`ruff_ruby_spo::SchemaReport`].
#[must_use]
pub fn extract_core_triples_with_schema(
    rails_root: &std::path::Path,
) -> (Vec<RuffTriple>, ruff_ruby_spo::SchemaReport) {
    let (triples, report, _) = extract_core_triples_with_schema_and_targets(rails_root);
    (triples, report)
}

/// [`extract_core_triples_with_schema`] plus the FULL extracted
/// model-name set (pre-filter). The filter-before-snapshot fix: the
/// curated core filter bounds *emission*, but FK typing needs
/// *knowledge* of every model the app declares — `assigned_to →
/// Principal` should type as `record<Principal>` even though Principal
/// isn't curated. Callers pass the third element to
/// [`op_surreal_ast::from_triples::triples_to_schema_with_targets`].
#[must_use]
pub fn extract_core_triples_with_schema_and_targets(
    rails_root: &std::path::Path,
) -> (Vec<RuffTriple>, ruff_ruby_spo::SchemaReport, Vec<String>) {
    let (mut graph, report) = ruff_ruby_spo::extract_app_with_schema(rails_root, NAMESPACE);
    let all_models: Vec<String> = graph.models.iter().map(|m| m.name.clone()).collect();
    filter_to_core(&mut graph);
    (ruff_spo_triplet::expand(&graph), report, all_models)
}

/// The typed render path ("compiled, not parsed" direction): triples →
/// [`op_surreal_ast::from_triples::triples_to_schema`] → typed AST →
/// SurrealQL text, with the conservation ledger appended as a trailer
/// comment block (nothing drops silently). This supersedes
/// [`render_surreal_from_ruff`] for CLI use; the old projection path
/// stays for its round-trip contract until the compile surface
/// migrates OGAR-side.
#[must_use]
pub fn render_typed_surreal(rails_root: &std::path::Path) -> String {
    use op_surreal_ast::ToSql;
    let (triples, report, all_models) = extract_core_triples_with_schema_and_targets(rails_root);
    let spine = bridge_triples(&triples);
    let schema = op_surreal_ast::from_triples::triples_to_schema_with_targets(&spine, &all_models);
    let mut out = schema.to_sql();
    // Conservation trailer: the artifact accounts for what it covers.
    // Typed-coverage line measured via the contract scan family
    // (lance-graph #632 `emission_scan`, shipped against wishlist L2) —
    // every consumer counts identically; no local grep.
    let kind_tokens: Vec<String> = schema
        .tables
        .iter()
        .flat_map(|t| t.fields.iter())
        .map(|f| f.kind.to_sql())
        .collect();
    let counts =
        lance_graph_contract::emission_scan::count_emission(kind_tokens.iter().map(String::as_str));
    out.push_str(&format!(
        "-- emission: typed={} record={} any={} stub={} (contract::emission_scan)\n",
        counts.typed, counts.record_link, counts.any_typed, counts.stub,
    ));
    out.push_str(&format!(
        "-- columns-from: {} | tables seen: {} matched: {} unmatched: {} skipped: {}\n",
        report.columns_from,
        report.tables_seen,
        report.tables_matched,
        report.unmatched_tables.len(),
        report.files_skipped.len(),
    ));
    for t in &report.unmatched_tables {
        out.push_str(&format!("-- unmatched table (no model): {t}\n"));
    }
    // Stub-demand ranking (P0 instrument hygiene, 2026-07-02 epiphany
    // #5): each stub table (a referenced-but-uncurated FK target,
    // op-surreal-ast's `triples_to_schema_with_targets`) is counted by
    // how many core fields link to it. This turns curation from
    // editorial into demand-driven — the top-ranked stub is the next
    // candidate for CORE_V3_RESOURCES (curated upstream, not here).
    let stub_names: Vec<&str> = schema
        .tables
        .iter()
        .filter(|t| t.comment.as_deref().is_some_and(|c| c.starts_with("stub:")))
        .map(|t| t.name.as_str())
        .collect();
    if !stub_names.is_empty() {
        let mut demand: std::collections::BTreeMap<&str, u32> =
            stub_names.iter().map(|n| (*n, 0)).collect();
        for f in schema.tables.iter().flat_map(|t| t.fields.iter()) {
            for target in record_targets(&f.kind) {
                if let Some(count) = demand.get_mut(target.as_str()) {
                    *count += 1;
                }
            }
        }
        let mut ranked: Vec<(&str, u32)> = demand.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let line = ranked
            .iter()
            .map(|(name, count)| format!("{name}({count})"))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!("-- stub demand: {line}\n"));
    }
    out
}

/// The table name(s) a (possibly `option<…>`-wrapped) `Kind::Record`
/// links to. Empty for every other kind — the stub-demand ranking is
/// the only consumer; kept free-standing rather than a method on
/// `op_surreal_ast::Kind` so this crate doesn't grow a dependency-side
/// API surface for a one-off count.
fn record_targets(kind: &op_surreal_ast::Kind) -> Vec<String> {
    match kind {
        op_surreal_ast::Kind::Record(targets) => targets.clone(),
        op_surreal_ast::Kind::Option(inner) => record_targets(inner),
        _ => Vec::new(),
    }
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
    use ruff_spo_triplet::{expand, Field, Model, ModelGraph};

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
            ..Field::default()
        });
        wp.fields.push(Field {
            name: "status".to_string(),
            depends_on: Vec::new(),
            emitted_by: None,
            ..Field::default()
        });
        g.models.push(wp);

        let mut te = Model::new("TimeEntry");
        te.fields.push(Field {
            name: "hours".to_string(),
            depends_on: Vec::new(),
            emitted_by: None,
            ..Field::default()
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
            ..Function::default()
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

    // ----- C10: validated-column requiredness through the full pipeline -----

    #[test]
    fn pipeline_validate_function_marks_field_as_required() {
        // Construct a Rails-shape graph where WorkPackage declares
        // `validates :subject, presence: true`. ruff_ruby_spo turns that
        // into a synthetic `_validate` Function with reads=[subject];
        // ruff_spo_triplet::expand emits the canonical triples; the
        // C10 projection picks them up and marks `subject` required.
        use ruff_spo_triplet::Function;
        let mut g = synthetic_op_graph();
        g.models[0].functions.push(Function {
            name: "_validate".to_string(),
            reads: vec!["subject".to_string()],
            raises: vec!["ActiveRecord::RecordInvalid".to_string()],
            traverses: Vec::new(),
            ..Function::default()
        });

        let triples = expand(&g);
        let (c, text) = render_surreal_from_ruff(&triples);

        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        let status = wp.fields.iter().find(|f| f.name == "status").unwrap();
        assert!(subject.required, "validated column must be required");
        assert!(!status.required, "non-validated column stays optional");

        // And the emission reflects it.
        assert!(text.contains("DEFINE FIELD subject ON TABLE WorkPackage TYPE any;"));
        assert!(text.contains("DEFINE FIELD status ON TABLE WorkPackage TYPE option<any>;"));

        // Contract still holds.
        roundtrip_eq::<OpSurrealProjection>(&bridge_triples(&triples))
            .expect("validation triples round-trip via the opaque trail");
    }

    // ----- P0 instrument hygiene: stub-demand ranking -----

    #[test]
    fn record_targets_unwraps_option_and_ignores_non_record_kinds() {
        use op_surreal_ast::Kind;
        assert_eq!(record_targets(&Kind::Any), Vec::<String>::new());
        assert_eq!(record_targets(&Kind::Int), Vec::<String>::new());
        assert_eq!(
            record_targets(&Kind::Record(vec!["Principal".to_string()])),
            vec!["Principal".to_string()]
        );
        assert_eq!(
            record_targets(&Kind::Record(vec!["Principal".to_string()]).optional()),
            vec!["Principal".to_string()],
            "option<record<T>> unwraps to record<T>'s targets"
        );
        // Idempotent optional() collapses option<option<T>>, but the
        // recursion must handle a doubly-nested Option regardless (the
        // enum shape technically allows constructing one directly).
        assert_eq!(
            record_targets(&Kind::Option(Box::new(Kind::Option(Box::new(
                Kind::Record(vec!["X".to_string()])
            ))))),
            vec!["X".to_string()],
            "recursion unwraps nested Option regardless of how it was built"
        );
    }
}
