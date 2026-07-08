//! Klickweg harvest bridge — Rails navigation topology into op-nexgen.
//!
//! Runs [`ruff_ruby_spo::extract_nav_edges`] (the two-shape Rails
//! `navigates_to` harvest — ERB `link_to`/`button_to` clicks + controller
//! `redirect_to` redirects, ruff #62) over an OpenProject Rails source tree
//! and returns the navigation edges between served screens.
//!
//! This is the **harvested** source of the board's klickweg — the same
//! screen-to-screen connectivity that `op-server::nav::NAV_EDGES` currently
//! encodes **by hand** (derived from the OGAR `ClassView` association graph)
//! and proves connected via the Core brick
//! `lance_graph_contract::class_view::nav_is_fully_connected` (lance-graph
//! #670/#673). When the OpenProject Rails source is present in the pipeline
//! input, these harvested edges are the ground truth and `NAV_EDGES` is their
//! baked mirror; `nav_harvest_probe` proves the harvest reproduces exactly
//! that edge set on a synthetic fixture.
//!
//! ## Two vocabularies, one graph
//!
//! The harvest speaks Rails **resource** stems (`work_packages`, `projects`);
//! op-server's `ClassView` graph speaks **concept** names (`ProjectWorkItem`,
//! `Project`). The mapping is 1:1 ([`RESOURCE_TO_CONCEPT`], the inverse of
//! `op-server::nav::SCREEN_UNIVERSE`), so the connectivity graph is identical
//! under it — a `(work_packages → projects)` harvested edge IS the
//! `(ProjectWorkItem → Project)` ClassView edge.

use std::collections::BTreeSet;
use std::path::Path;

use ruff_ruby_spo::{
    extract_nav_edges, extract_nav_edges_with_report, NavScanReport, NavVocab, RubyNavEdge,
};

/// The OpenProject board's served screens, as Rails resource stems — the
/// closed nav vocabulary. 1:1 with `op-server::nav::SCREEN_UNIVERSE` (which
/// names the same screens as `ClassView` concepts; see [`RESOURCE_TO_CONCEPT`]).
pub const OPENPROJECT_SCREENS: &[&str] = &["work_packages", "projects"];

/// Rails resource stem → op-server `ClassView` concept name. The 1:1 bridge
/// between the harvest vocabulary and `op-server::nav::SCREEN_UNIVERSE`
/// (`["ProjectWorkItem", "Project"]`).
pub const RESOURCE_TO_CONCEPT: &[(&str, &str)] = &[
    ("work_packages", "ProjectWorkItem"),
    ("projects", "Project"),
];

/// Harvest the klickweg edges from a Rails `app_root`, restricted to the
/// OpenProject served screens ([`OPENPROJECT_SCREENS`]).
#[must_use]
pub fn harvest_klickweg(app_root: &Path) -> Vec<RubyNavEdge> {
    extract_nav_edges(app_root, &openproject_vocab())
}

/// Like [`harvest_klickweg`] but also returns the [`NavScanReport`] ledger
/// (files scanned, files with edges, raw target references — the honest
/// denominator).
#[must_use]
pub fn harvest_klickweg_with_report(app_root: &Path) -> (Vec<RubyNavEdge>, NavScanReport) {
    extract_nav_edges_with_report(app_root, &openproject_vocab())
}

/// The distinct `(source, target)` screen pairs of a harvested edge set — the
/// connectivity graph, shape-agnostic (ERB-click and controller-redirect
/// edges between the same two screens collapse to one pair). This is what
/// must match `op-server::nav::NAV_EDGES`' connectivity.
#[must_use]
pub fn klickweg_pairs(edges: &[RubyNavEdge]) -> BTreeSet<(String, String)> {
    edges
        .iter()
        .map(|e| (e.source.clone(), e.target.clone()))
        .collect()
}

/// The op-server `ClassView` concept name for a Rails resource stem, or
/// `None` if it is not a served screen.
#[must_use]
pub fn concept_for(resource: &str) -> Option<&'static str> {
    RESOURCE_TO_CONCEPT
        .iter()
        .find(|(r, _)| *r == resource)
        .map(|(_, c)| *c)
}

fn openproject_vocab() -> NavVocab {
    NavVocab {
        screens: OPENPROJECT_SCREENS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    }
}
