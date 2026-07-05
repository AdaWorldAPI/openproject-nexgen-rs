//! End-to-end test: drive `ruff_openproject::extract_*` over the OpenProject
//! Rails fixture and assert the OpenProject-shaped output.

use std::path::PathBuf;

use ruff_openproject::{
    CORE_V3_RESOURCES, NAMESPACE, extract_core_triples, extract_graph, extract_triples,
    filter_to_core,
};

fn fixture_tree() -> PathBuf {
    // Crate-local copy of the fixture originally shared with ruff_ruby_spo's
    // test tree (unvendored 2026-07-05 — ruff_ruby_spo now resolves via a
    // pinned git dep, so its test fixtures are no longer reachable by
    // relative path; the fixture is duplicated here instead of drifting).
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("openproject")
}

#[test]
fn extract_graph_returns_known_models() {
    let g = extract_graph(&fixture_tree());
    assert_eq!(g.namespace, NAMESPACE);
    let names: Vec<&str> = g.models.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["TimeEntry", "WorkPackage"]);
}

#[test]
fn extract_triples_produces_locked_shape() {
    let triples = extract_triples(&fixture_tree());
    let has =
        |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);

    // Spot-check the locked shape (same assertions as the integration test in
    // ruff_ruby_spo, here exercised through the OP entry point).
    assert!(has("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"));
    assert!(has(
        "openproject:WorkPackage.compute_total_hours",
        "raises",
        "exc:ActiveRecord::RecordInvalid"
    ));
    assert!(has(
        "openproject:WorkPackage.total_hours",
        "emitted_by",
        "openproject:WorkPackage.compute_total_hours"
    ));
}

#[test]
fn filter_to_core_keeps_fixture_models() {
    // Both fixture models (WorkPackage, TimeEntry) are in the core curated set.
    let mut g = extract_graph(&fixture_tree());
    let before = g.models.len();
    filter_to_core(&mut g);
    assert_eq!(g.models.len(), before, "fixture is all core, nothing dropped");
    assert!(g.models.iter().all(|m| CORE_V3_RESOURCES.contains(&m.name.as_str())));
}

#[test]
fn extract_core_triples_matches_extract_triples_on_pure_core_fixture() {
    // Since the fixture only contains core models, the filtered extraction
    // must produce the same triple set as the unfiltered one (as sets — order
    // is preserved by both paths but we don't depend on that here).
    let mut full = extract_triples(&fixture_tree());
    let mut core = extract_core_triples(&fixture_tree());
    full.sort_by(|a, b| (a.s.as_str(), a.p.as_str(), a.o.as_str()).cmp(&(b.s.as_str(), b.p.as_str(), b.o.as_str())));
    core.sort_by(|a, b| (a.s.as_str(), a.p.as_str(), a.o.as_str()).cmp(&(b.s.as_str(), b.p.as_str(), b.o.as_str())));
    assert_eq!(full.len(), core.len());
    for (a, b) in full.iter().zip(core.iter()) {
        assert_eq!((&a.s, &a.p, &a.o), (&b.s, &b.p, &b.o));
    }
}
