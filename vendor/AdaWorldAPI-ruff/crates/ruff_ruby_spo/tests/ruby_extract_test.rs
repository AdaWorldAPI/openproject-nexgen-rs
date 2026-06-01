//! Sprint C4 — end-to-end extraction test (the spec for the 3 extractors).
//!
//! Runs the real `extract()` over a tiny OpenProject-shaped fixture tree
//! (`tests/fixtures/openproject/`) and asserts the expanded SPO triples match
//! the locked shape. Mirrors the in-crate `locked_shape_expands_to_expected_triples`
//! unit test, but on EXTRACTED output rather than a hand-built graph — so it
//! fails (todo!() panic) until the fanout agents fill parse/fields/functions.

use std::path::PathBuf;

use ruff_ruby_spo::extract;
use ruff_spo_triplet::expand;

fn fixture_tree() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/openproject")
}

#[test]
fn extracts_locked_work_package_shape_from_fixture() {
    let graph = extract(&fixture_tree());
    let triples = expand(&graph);
    let has =
        |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);

    // --- classification ---
    assert!(
        has("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
        "WorkPackage class -> ObjectType"
    );
    assert!(
        has("openproject:TimeEntry", "rdf:type", "ogit:ObjectType"),
        "TimeEntry class -> ObjectType (second model in the tree)"
    );
    assert!(
        has("openproject:WorkPackage.subject", "rdf:type", "ogit:Property"),
        "schema.rb column `subject` -> Property"
    );
    assert!(
        has("openproject:WorkPackage.total_hours", "rdf:type", "ogit:Property"),
        "memoized @total_hours -> derived Property"
    );
    assert!(
        has("openproject:WorkPackage.compute_total_hours", "rdf:type", "ogit:Function"),
        "def compute_total_hours -> Function"
    );

    // --- compute graph edges ---
    assert!(
        has(
            "openproject:WorkPackage.total_hours",
            "emitted_by",
            "openproject:WorkPackage.compute_total_hours"
        ),
        "derived attr is emitted_by the method that memoizes it"
    );
    assert!(
        has(
            "openproject:WorkPackage.total_hours",
            "depends_on",
            "openproject:WorkPackage.time_entries.hours"
        ),
        "derived attr depends_on the association chain its method reads"
    );

    // --- guard + traversal ---
    assert!(
        has(
            "openproject:WorkPackage.compute_total_hours",
            "raises",
            "exc:ActiveRecord::RecordInvalid"
        ),
        "explicit `raise ActiveRecord::RecordInvalid` -> raises (Authoritative)"
    );
    assert!(
        has(
            "openproject:WorkPackage.compute_total_hours",
            "traverses_relation",
            "openproject:WorkPackage.time_entries"
        ),
        "calling the `time_entries` association -> traverses_relation"
    );
}

#[test]
fn declarative_validation_becomes_a_raising_guard() {
    // `validates :subject, presence: true` is a declarative guard. The frontend
    // maps it to a synthetic validate function that raises RecordInvalid
    // (guide §5 step 2). We assert the raise is present somewhere on the
    // WorkPackage's functions, without pinning the synthetic function's name.
    let graph = extract(&fixture_tree());
    let wp = graph
        .models
        .iter()
        .find(|m| m.name == "WorkPackage")
        .expect("WorkPackage extracted");
    assert!(
        wp.functions
            .iter()
            .any(|f| f.raises.iter().any(|r| r == "ActiveRecord::RecordInvalid")),
        "a validates/validate guard must raise ActiveRecord::RecordInvalid"
    );
}

#[test]
fn schema_index_lines_do_not_leak_into_columns() {
    // Codex PR #4 P2: `t.index ["work_package_id"], name: ...` and
    // `t.foreign_key "work_packages", ...` lines must NOT contribute fake
    // columns to the TimeEntry table. The fixture's TimeEntry columns are
    // exactly: work_package_id, user_id, hours, spent_on.
    let graph = extract(&fixture_tree());
    let te = graph
        .models
        .iter()
        .find(|m| m.name == "TimeEntry")
        .expect("TimeEntry extracted");
    let names: Vec<&str> = te.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["work_package_id", "user_id", "hours", "spent_on"],
        "non-column helpers (t.index, t.foreign_key) must be skipped — got {names:?}"
    );
}

#[test]
fn all_subjects_are_namespaced() {
    let graph = extract(&fixture_tree());
    let triples = expand(&graph);
    assert!(
        triples
            .iter()
            .all(|t| t.s.starts_with("openproject:") || t.s.starts_with("exc:")),
        "every subject IRI is in the openproject: (or exc:) namespace"
    );
}
