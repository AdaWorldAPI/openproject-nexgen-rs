//! `ruff_openproject` — high-level OpenProject extraction entry point.
//!
//! Thin orchestrator over the two existing crates:
//!
//! - [`ruff_ruby_spo::extract`] walks an OpenProject `app/models/` tree +
//!   `db/schema.rb` and returns a language-agnostic [`ModelGraph`].
//! - [`ruff_spo_triplet::expand`] turns that graph into SPO [`Triple`]s with
//!   NARS truth (the canonical lossless form consumed downstream by
//!   `lance_graph::graph::spo`).
//!
//! Both layers are upstream-neutral (the `ruby_spo` frontend just happens to
//! hardcode `openproject` as its IRI namespace because OpenProject is its
//! only consumer today). This crate adds the OpenProject-specific glue
//! without modifying either:
//!
//! - [`NAMESPACE`] — re-export of the namespace prefix (`"openproject"`).
//! - [`CORE_V3_RESOURCES`] — the curated set of v3 domain resources the
//!   `openproject-nexgen-rs` seed already covers; useful for filtering an
//!   extraction to the "core" surface before downstream processing.
//! - [`extract_graph`] / [`extract_triples`] — single-call wrappers.
//! - [`extract_core_triples`] — extracts and filters to [`CORE_V3_RESOURCES`].
//!
//! Zero runtime deps beyond the two siblings — additive in the spirit of the
//! ruff workspace (no edits to existing crates; new functionality lives in
//! its own crate).
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use ruff_openproject::extract_triples;
//!
//! let triples = extract_triples(Path::new("/path/to/openproject"));
//! // hand off to `lance_graph::graph::spo::SpoBuilder` / NDJSON / etc.
//! # let _ = triples;
//! ```

use std::path::Path;

use ruff_spo_triplet::{ModelGraph, Triple, expand};

/// The IRI namespace prefix used for OpenProject subjects/objects
/// (e.g. `openproject:WorkPackage.subject`). Re-exported from
/// [`ruff_ruby_spo::NAMESPACE`] so consumers depend on one crate, not two.
pub const NAMESPACE: &str = ruff_ruby_spo::NAMESPACE;

/// The curated set of OpenProject v3 domain resources covered by the
/// `openproject-nexgen-rs` seed (10 core + 8 partial-coverage, per Sprint C0
/// coverage measurement). Use with [`filter_to_core`] / [`extract_core_triples`]
/// to scope an extraction to the resources the downstream port understands.
///
/// Sorted alphabetically (asciibetically: uppercase-aware) for stable lookup
/// via binary-searchable iteration. The order of [`ModelGraph::models`] after
/// filtering follows the *input* order (which `ruff_ruby_spo::extract`
/// produces sorted by class name — also stable), independent of this list.
pub const CORE_V3_RESOURCES: &[&str] = &[
    "Activity",     // partial coverage (op-db row type)
    "Attachment",   // partial
    "Category",     // partial
    "Journal",      // partial
    "Member",       // core (op-models)
    "News",         // core
    "Priority",     // core
    "Project",      // core
    "Query",        // partial
    "Relation",     // partial
    "Role",         // core
    "Status",       // core
    "TimeEntry",    // core
    "Type",         // core
    "User",         // core
    "Version",      // core
    "Watcher",      // partial
    "WorkPackage",  // core
];

/// Extract an OpenProject Rails source tree into a [`ModelGraph`].
/// Thin wrapper around [`ruff_ruby_spo::extract`]; see that fn for the
/// Rails→IR mapping and scanner scope/limits.
#[must_use]
pub fn extract_graph(rails_root: &Path) -> ModelGraph {
    ruff_ruby_spo::extract(rails_root)
}

/// Extract and expand to SPO [`Triple`]s in one call. The triples are the
/// canonical lossless form consumed downstream (NDJSON via
/// [`ruff_spo_triplet::to_ndjson`], `lance_graph::graph::spo::SpoBuilder`,
/// etc.).
#[must_use]
pub fn extract_triples(rails_root: &Path) -> Vec<Triple> {
    expand(&extract_graph(rails_root))
}

/// Filter a [`ModelGraph`] in place to keep only [`CORE_V3_RESOURCES`].
/// Useful when the source tree is a full OpenProject checkout but downstream
/// only handles the curated core surface.
pub fn filter_to_core(graph: &mut ModelGraph) {
    graph
        .models
        .retain(|m| CORE_V3_RESOURCES.iter().any(|core| *core == m.name));
}

/// Convenience: extract → filter to core → expand to triples.
#[must_use]
pub fn extract_core_triples(rails_root: &Path) -> Vec<Triple> {
    let mut graph = extract_graph(rails_root);
    filter_to_core(&mut graph);
    expand(&graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_matches_ruby_spo() {
        // The OP namespace is intentionally re-exported, not redefined.
        assert_eq!(NAMESPACE, ruff_ruby_spo::NAMESPACE);
        assert_eq!(NAMESPACE, "openproject");
    }

    #[test]
    fn core_v3_resources_are_alphabetised_and_unique() {
        let mut sorted = CORE_V3_RESOURCES.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            sorted, CORE_V3_RESOURCES,
            "CORE_V3_RESOURCES must be alphabetically sorted for stable lookup"
        );
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(deduped.len(), sorted.len(), "no duplicates allowed");
    }

    #[test]
    fn filter_to_core_keeps_known_drops_unknown() {
        // Build a synthetic graph (no I/O) and verify the filter shape.
        use ruff_spo_triplet::Model;
        let mut g = ModelGraph::new(NAMESPACE);
        g.models.push(Model::new("WorkPackage")); // in core
        g.models.push(Model::new("UnknownAdHoc")); // not in core
        g.models.push(Model::new("TimeEntry")); // in core
        filter_to_core(&mut g);
        let kept: Vec<&str> = g.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(kept, ["WorkPackage", "TimeEntry"]);
    }
}
