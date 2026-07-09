//! Probe: the Rails `navigates_to` harvest (ruff #62) reproduces the board's
//! klickweg — the same screen graph `op-server::nav::NAV_EDGES` bakes by hand.
//!
//! Drives a synthetic OpenProject Rails fixture (`tests/fixtures/rails_nav/`,
//! no proprietary source) through `op_codegen_pipeline::nav_harvest` and
//! asserts:
//!   1. the harvested `(source, target)` screen pairs are EXACTLY the
//!      OpenProject board klickweg (`work_packages ↔ projects`), both
//!      directions — proving the harvest yields the connectivity that
//!      `op-server::nav::NAV_EDGES` encodes by hand;
//!   2. both harvest shapes fire (ERB `link_to` click + controller
//!      `redirect_to` redirect);
//!   3. the resource→concept bridge maps every harvested screen onto an
//!      `op-server::nav::SCREEN_UNIVERSE` concept.

use std::path::PathBuf;

use op_codegen_pipeline::nav_harvest::{
    concept_for, harvest_klickweg_with_report, harvest_menu_klickweg_with_report, klickweg_pairs,
    OPENPROJECT_SCREENS,
};
use ruff_ruby_spo::NavShape;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rails_nav")
}

/// The harvested screen pairs are exactly the OpenProject board klickweg —
/// `work_packages ↔ projects`, both directions.
#[test]
fn harvest_reproduces_the_board_klickweg() {
    let (edges, report) = harvest_klickweg_with_report(&fixture_root());
    let pairs = klickweg_pairs(&edges);

    let expected: std::collections::BTreeSet<(String, String)> =
        [("work_packages", "projects"), ("projects", "work_packages")]
            .iter()
            .map(|(s, t)| ((*s).to_string(), (*t).to_string()))
            .collect();

    assert_eq!(
        pairs, expected,
        "harvested klickweg must be exactly work_packages <-> projects; got {edges:?}"
    );

    // The ledger accounts for the fixture: 2 ERB views + 1 controller, every
    // file producing at least one edge.
    assert_eq!(report.erb_files, 2, "{report:?}");
    assert_eq!(report.controller_files, 1, "{report:?}");
    assert_eq!(report.files_with_edges, 3, "{report:?}");
}

/// Both harvest shapes fire: the ERB `link_to` click edges AND the controller
/// `redirect_to` redirect edge are all present (Ruby's two-shape harvest, not
/// a single body-walk).
#[test]
fn both_harvest_shapes_are_present() {
    let (edges, _) = harvest_klickweg_with_report(&fixture_root());

    assert!(
        edges.iter().any(|e| e.shape == NavShape::ErbClick),
        "expected at least one ERB click edge: {edges:?}"
    );
    assert!(
        edges
            .iter()
            .any(|e| e.shape == NavShape::ControllerRedirect),
        "expected the controller redirect edge (work_packages -> projects): {edges:?}"
    );

    // The redirect edge specifically is work_packages -> projects.
    assert!(
        edges.iter().any(|e| e.shape == NavShape::ControllerRedirect
            && e.source == "work_packages"
            && e.target == "projects"),
        "controller redirect must be work_packages -> projects: {edges:?}"
    );
}

/// Every served screen maps 1:1 onto an `op-server::nav::SCREEN_UNIVERSE`
/// concept — the harvest (resource-stem) and the ClassView graph
/// (concept-name) speak the same connectivity graph under the bridge.
#[test]
fn every_screen_bridges_to_a_classview_concept() {
    for resource in OPENPROJECT_SCREENS {
        assert!(
            concept_for(resource).is_some(),
            "served screen {resource} has no ClassView concept in RESOURCE_TO_CONCEPT"
        );
    }
    assert_eq!(concept_for("work_packages"), Some("ProjectWorkItem"));
    assert_eq!(concept_for("projects"), Some("Project"));
    assert_eq!(concept_for("not_a_screen"), None);
}

/// The menu-DSL harvest (ruff #71) reproduces `op-server::nav::MENU_NAV_EDGES`'
/// `Menu → <screen>` half: exactly `("menu", "work_packages")` and
/// `("menu", "projects")`, both shape `MenuItem`, from the synthetic
/// `lib/redmine/default_menu.rb` fixture.
#[test]
fn menu_harvest_reproduces_the_menu_rooted_klickweg() {
    let (edges, report) = harvest_menu_klickweg_with_report(&fixture_root());
    let pairs = klickweg_pairs(&edges);

    let expected: std::collections::BTreeSet<(String, String)> =
        [("menu", "work_packages"), ("menu", "projects")]
            .iter()
            .map(|(s, t)| ((*s).to_string(), (*t).to_string()))
            .collect();

    assert_eq!(
        pairs, expected,
        "harvested menu klickweg must be exactly menu -> {{work_packages, projects}}; got {edges:?}"
    );
    assert!(
        edges.iter().all(|e| e.shape == NavShape::MenuItem),
        "every menu-harvested edge must carry the MenuItem shape: {edges:?}"
    );

    // The ledger accounts for the fixture: 1 menu-DSL file, 2 pushes, both
    // resolving (no unknown targets in the fixture).
    assert_eq!(report.files_with_menu_items, 1, "{report:?}");
    assert_eq!(report.raw_menu_pushes, 2, "{report:?}");
}
