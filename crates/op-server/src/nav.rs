//! Navigation registry derived from the OGAR `ClassView` associations.
//!
//! The board's click-path (page-to-page links) is DERIVED from
//! `ogar_vocab::Class::associations` — each `Association` (built via
//! `family_edge`/`family_has_many` in `ogar_vocab::project_work_item()` /
//! `ogar_vocab::project()`) is a navigation edge. `Association.class_name`
//! carries the PascalCase target concept (e.g. `"Project"`,
//! `"ProjectWorkItem"`); `Association.kind` (`AssociationKind::HasMany`
//! vs the rest) tells us whether it's a collection edge.
//!
//! This module is the registry (`route_for`) + derivation (`nav_edges`)
//! + a static "no dead lanes" prober: every association target must
//! either resolve to a route, or be explicitly listed in
//! [`NOT_YET_NAVIGABLE`] as a known, intentional gap.
//!
//! ## Klickweg connectivity via the sanctioned Core brick
//!
//! The "does every pipe lead somewhere, and is the level fully connected"
//! check (the Mario-World-editor validator) is NOT hand-rolled here as a
//! bespoke BFS — it consumes the Core brick
//! [`lance_graph_contract::class_view::nav_is_fully_connected`] (lance-graph
//! #670/#673, the *jump* half of the topology Lego kit that pairs with the
//! `ruff_csharp_spo` `navigates_to` harvest in ruff #61). The served-screen
//! universe mask is minted via the sanctioned membership brick
//! [`WideFieldMask::from_universe_present`] (lance-graph #669) — the same
//! byte-for-byte-interchangeable mask every other consumer mints, carrying
//! the 256-field SoC-split guard. The navigation graph reuses the Core
//! [`ComputeEdge`] shape (`target` = destination screen, `inputs` = source
//! screens): one more brick, not a parallel edge type. This makes the
//! connectivity guarantee a compile/test-time invariant, not only a
//! live `scripts/nav-crawl.sh` BFS.

use lance_graph_contract::class_view::{nav_is_fully_connected, ComputeEdge};
use lance_graph_contract::WideFieldMask;
use ogar_vocab::{AssociationKind, Class};

/// Concept → route registry. Only concepts with a live detail/list page
/// today resolve to `Some`; everything else is `None` (no page yet).
pub fn route_for(target_concept: &str) -> Option<&'static str> {
    match target_concept {
        "Project" => Some("/projects"),
        "ProjectWorkItem" => Some("/work_packages"),
        _ => None,
    }
}

/// The served screens (routed concepts) in a **stable order**. A screen's
/// position is its index here — this single array is the source of order for
/// both the [`WideFieldMask`] universe (minted via `from_universe_present`)
/// and the [`ComputeEdge`] positions, so the two can never drift.
///
/// Every entry MUST have a `Some` [`route_for`]; `screen_universe_matches_routes`
/// proves it. The board (`"/"`) IS the `ProjectWorkItem` screen, so that is the
/// entry/root ([`NAV_ROOT`] = index 0).
pub const SCREEN_UNIVERSE: &[&str] = &["ProjectWorkItem", "Project"];

/// The entry screen position — the board (`"/"`), i.e. `ProjectWorkItem`.
pub const NAV_ROOT: u8 = 0;

/// Screen position for a served (routed) concept — its index in
/// [`SCREEN_UNIVERSE`]. `None` for un-routed / deferred concepts. The `u8`
/// cast is safe: the universe is far under `u8::MAX` entries.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn screen_id(concept: &str) -> Option<u8> {
    SCREEN_UNIVERSE
        .iter()
        .position(|c| *c == concept)
        .map(|i| i as u8)
}

/// The navigation edges among served screens, in the Core [`ComputeEdge`]
/// shape: `target` = destination screen position, `inputs` = source screen
/// positions that navigate to it (the *jump* half of the topology kit).
///
/// This is the routed subset of the derived [`nav_edges`] of every served
/// class; `nav_edges_match_static_table` cross-checks this table against the
/// live derivation so it can never silently drift. `inputs` is `&'static`
/// (the Core edge shape), so this is a plain `const` manifest.
pub const NAV_EDGES: &[ComputeEdge] = &[
    // ProjectWorkItem.project (belongs_to) → Project
    ComputeEdge {
        target: 1,
        inputs: &[0],
    },
    // Project.work_packages (has_many) → ProjectWorkItem
    ComputeEdge {
        target: 0,
        inputs: &[1],
    },
];

/// The served-screen universe as a [`WideFieldMask`], minted via the
/// sanctioned membership brick [`WideFieldMask::from_universe_present`]
/// (universe = present = every served screen, since all listed screens are
/// routed). This is the `screens` argument to [`nav_is_fully_connected`].
///
/// # Panics
///
/// Never in practice: [`SCREEN_UNIVERSE`] has 2 entries, far under the
/// 256-field SoC cap the brick guards.
#[must_use]
pub fn served_screens_mask() -> WideFieldMask {
    WideFieldMask::from_universe_present(SCREEN_UNIVERSE, SCREEN_UNIVERSE)
        .expect("2-screen universe is within the 256-field SoC cap")
}

/// Whether the klickweg is fully connected: every served screen is reachable
/// from [`NAV_ROOT`] and no navigation edge dangles off the served set — the
/// Core-brick ([`nav_is_fully_connected`]) level-editor validator, at
/// compile/test time rather than only via the live crawl.
#[must_use]
pub fn klickweg_is_connected() -> bool {
    nav_is_fully_connected(NAV_ROOT, NAV_EDGES, &served_screens_mask())
}

/// Association targets referenced by `project_work_item()` / `project()`
/// that have NO page today, and that gap is a known, intentional debt
/// (not a bug). Adding a new association without a route AND without an
/// entry here breaks the static prober below.
pub const NOT_YET_NAVIGABLE: &[&str] = &[
    "ProjectStatus",
    "ProjectType",
    "Priority",
    "ProjectActor",
    "ProjectJournal",
    "ProjectRelation",
    "BillableWorkEntry",
];

/// One navigation edge derived from a `Class`'s `associations`.
#[derive(Debug, Clone)]
pub struct NavEdge {
    pub role: String,
    pub target_concept: String,
    pub has_many: bool,
    pub route: Option<&'static str>,
}

/// Derive the navigation edges for a `Class` from its `associations`.
/// Associations without a `class_name` (target concept) are skipped —
/// they carry no navigable target.
pub fn nav_edges(class: &Class) -> Vec<NavEdge> {
    class
        .associations
        .iter()
        .filter_map(|assoc| {
            let target_concept = assoc.class_name.clone()?;
            Some(NavEdge {
                role: assoc.name.clone(),
                has_many: matches!(assoc.kind, AssociationKind::HasMany),
                route: route_for(&target_concept),
                target_concept,
            })
        })
        .collect()
}

/// The top-level nav menu — concepts with a live list/index page.
pub fn menu() -> &'static [(&'static str, &'static str)] {
    &[("Board", "/"), ("Projects", "/projects")]
}

/// Every association (across `project_work_item()` and `project()`)
/// whose target has no route yet — the debt list.
pub fn dead_lane_census() -> Vec<(&'static str, String, String)> {
    let classes: [(&'static str, Class); 2] = [
        ("ProjectWorkItem", ogar_vocab::project_work_item()),
        ("Project", ogar_vocab::project()),
    ];

    let mut out = Vec::new();
    for (owning_concept, class) in &classes {
        for edge in nav_edges(class) {
            if edge.route.is_none() {
                out.push((*owning_concept, edge.role, edge.target_concept));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The static "no dead lanes" prober: every association target on
    /// `project_work_item()` and `project()` must be either routed or
    /// explicitly deferred via `NOT_YET_NAVIGABLE`. A new association
    /// added without either breaks this test — no silent dead lane.
    #[test]
    fn every_association_target_is_routed_or_explicitly_deferred() {
        let classes: [(&str, Class); 2] = [
            ("ProjectWorkItem", ogar_vocab::project_work_item()),
            ("Project", ogar_vocab::project()),
        ];

        for (owning_concept, class) in &classes {
            for assoc in &class.associations {
                let Some(target) = &assoc.class_name else {
                    continue;
                };
                let routed = route_for(target).is_some();
                let deferred = NOT_YET_NAVIGABLE.contains(&target.as_str());
                assert!(
                    routed || deferred,
                    "dead lane: {owning_concept}.{} -> {target} is neither routed \
                     nor listed in NOT_YET_NAVIGABLE",
                    assoc.name
                );
            }
        }
    }

    #[test]
    fn nav_edges_marks_live_and_dead() {
        let edges = nav_edges(&ogar_vocab::project_work_item());

        let project_edge = edges
            .iter()
            .find(|e| e.role == "project")
            .expect("project edge present");
        assert_eq!(project_edge.route, Some("/projects"));
        assert!(!project_edge.has_many);

        let status_edge = edges
            .iter()
            .find(|e| e.role == "status")
            .expect("status edge present");
        assert_eq!(status_edge.route, None);
        assert!(!status_edge.has_many);
    }

    #[test]
    fn menu_targets_are_all_routable() {
        for (_, href) in menu() {
            assert!(
                *href == "/"
                    || route_for("Project") == Some(*href)
                    || route_for("ProjectWorkItem") == Some(*href),
                "menu href {href} is not a known route"
            );
        }
    }

    /// Every screen in [`SCREEN_UNIVERSE`] must actually be routed — the
    /// mask universe cannot claim a screen the router won't serve.
    #[test]
    fn screen_universe_matches_routes() {
        for concept in SCREEN_UNIVERSE {
            assert!(
                route_for(concept).is_some(),
                "SCREEN_UNIVERSE lists {concept} but route_for({concept}) is None"
            );
            assert!(screen_id(concept).is_some());
        }
    }

    /// The static [`NAV_EDGES`] table must equal the routed subset of the
    /// derived [`nav_edges`] across every served class — no silent drift
    /// between the hand-written Core-shape manifest and the live derivation.
    #[test]
    fn nav_edges_match_static_table() {
        // Build the expected routed edges from the derivation: for each served
        // class, every association whose target is itself a served screen is a
        // nav edge (owner-screen → target-screen).
        let classes: [(&str, Class); 2] = [
            ("ProjectWorkItem", ogar_vocab::project_work_item()),
            ("Project", ogar_vocab::project()),
        ];
        let mut derived: Vec<(u8, u8)> = Vec::new(); // (target, source)
        for (owner, class) in &classes {
            let Some(owner_pos) = screen_id(owner) else {
                continue;
            };
            for edge in nav_edges(class) {
                if let Some(target_pos) = screen_id(&edge.target_concept) {
                    // Only edges BETWEEN served screens are navigation edges in
                    // the connectivity graph (a routed target the crawl walks).
                    derived.push((target_pos, owner_pos));
                }
            }
        }
        derived.sort_unstable();
        derived.dedup();

        let mut table: Vec<(u8, u8)> = NAV_EDGES
            .iter()
            .flat_map(|e| e.inputs.iter().map(move |&src| (e.target, src)))
            .collect();
        table.sort_unstable();
        table.dedup();

        assert_eq!(
            table, derived,
            "NAV_EDGES static table drifted from the derived routed nav edges"
        );
    }

    /// The klickweg is fully connected, proven by the sanctioned Core brick
    /// [`nav_is_fully_connected`] — this is the compile/test-time form of the
    /// live `scripts/nav-crawl.sh` BFS. Also spot-check `screens_reachable_from`
    /// reaches the whole served universe from the root.
    #[test]
    fn klickweg_is_fully_connected_via_core_brick() {
        assert!(
            klickweg_is_connected(),
            "klickweg not fully connected: reached != served screens \
             (orphan screen, dangling click, or unserved root)"
        );

        let reached = lance_graph_contract::class_view::screens_reachable_from(NAV_ROOT, NAV_EDGES);
        assert_eq!(
            reached,
            served_screens_mask(),
            "reached set from NAV_ROOT must equal the served-screen universe"
        );
    }

    #[test]
    fn census_lists_the_known_dead_lanes() {
        let census = dead_lane_census();
        assert!(!census.is_empty(), "expected non-empty dead lane census");
        assert!(
            census
                .iter()
                .any(|(owner, role, target)| *owner == "ProjectWorkItem"
                    && role == "status"
                    && target == "ProjectStatus"),
            "expected WorkItem.status -> ProjectStatus dead lane in census, got {census:?}"
        );
    }
}
