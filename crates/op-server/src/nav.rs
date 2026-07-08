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
///
/// **Baked mirror of the harvest.** This table is the hand-authored (from the
/// `ClassView` association graph) equivalent of what
/// `op_codegen_pipeline::nav_harvest::harvest_klickweg` produces from the
/// OpenProject Rails source via the `ruff_ruby_spo` `navigates_to` harvest
/// (ruff #62). `op-codegen-pipeline`'s `nav_harvest_probe` proves the harvest
/// yields exactly the `work_packages ↔ projects` klickweg these edges encode
/// (`RESOURCE_TO_CONCEPT` bridges the Rails resource stems to the
/// `ProjectWorkItem`/`Project` concepts here). Once the OpenProject source is
/// in the pipeline input, this manifest regenerates from that harvest rather
/// than being hand-authored.
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

/// Whether the inter-screen klickweg is fully connected: every served
/// screen is reachable from [`NAV_ROOT`] and no navigation edge dangles
/// off the served set — the Core-brick ([`nav_is_fully_connected`])
/// level-editor validator over the screen-only graph.
///
/// This is the *screen↔screen* layer. The stronger **menu-reachability**
/// check ([`menu_klickweg_is_connected`]) verifies every screen is
/// reachable *from the top-nav menu* — see that function's docs.
#[must_use]
pub fn klickweg_is_connected() -> bool {
    nav_is_fully_connected(NAV_ROOT, NAV_EDGES, &served_screens_mask())
}

// ── Menu-rooted connectivity (cross-frontend Klickweg parity) ────────
//
// The Odoo `navigates_to` arm (ruff #66) introduced the **synthetic menu
// root**: `<menuitem>` records emit `menu → res_model` edges that give
// `nav_is_fully_connected` its entry points, so "connected" means *every
// screen is reachable FROM THE MENU* — an orphan screen the user cannot
// click to from the top nav fails, even if it is reachable from some
// other screen. op-nexgen's top nav ([`menu`]) is the identical
// structure — each tab is a `menu → screen` edge — so we model it the
// same way: a synthetic `Menu` node at position 0, screens after it, and
// the connectivity check rooted at the menu. This is the render-side twin
// of Odoo's menuitem roots (same Core brick, same "menu gives entry
// points" shape).

/// The menu-inclusive screen universe: the synthetic `Menu` node
/// ([`MENU_ROOT`] = index 0) followed by every served screen. Distinct
/// from [`SCREEN_UNIVERSE`] (screens only) because the render masks index
/// screen positions and must NOT see the synthetic node; this universe is
/// the connectivity graph only.
pub const MENU_UNIVERSE: &[&str] = &["Menu", "ProjectWorkItem", "Project"];

/// The synthetic top-nav menu node — the entry point of the klickweg,
/// mirroring Odoo's `<menuitem>` root.
pub const MENU_ROOT: u8 = 0;

/// The menu-rooted navigation edges over [`MENU_UNIVERSE`] positions:
/// `Menu → <screen>` for each top-nav tab (the twin of Odoo's
/// `menuitem → res_model`), plus the inter-screen [`NAV_EDGES`] lifted
/// into the menu-inclusive positions (+1 shift). `menu_nav_edges_match_derived`
/// cross-checks this table against [`menu`] + [`NAV_EDGES`] so it cannot
/// drift.
pub const MENU_NAV_EDGES: &[ComputeEdge] = &[
    // Menu → ProjectWorkItem  (the "Board" tab, href "/")
    ComputeEdge {
        target: 1,
        inputs: &[0],
    },
    // Menu → Project  (the "Projects" tab, href "/projects")
    ComputeEdge {
        target: 2,
        inputs: &[0],
    },
    // ProjectWorkItem → Project  (NAV_EDGES[0], shifted +1)
    ComputeEdge {
        target: 2,
        inputs: &[1],
    },
    // Project → ProjectWorkItem  (NAV_EDGES[1], shifted +1)
    ComputeEdge {
        target: 1,
        inputs: &[2],
    },
];

/// The menu-inclusive universe as a [`WideFieldMask`], minted on-brick
/// (universe = present = every node incl. the synthetic menu).
///
/// # Panics
///
/// Never: [`MENU_UNIVERSE`] has 3 entries, far under the 256-field cap.
#[must_use]
pub fn menu_screens_mask() -> WideFieldMask {
    WideFieldMask::from_universe_present(MENU_UNIVERSE, MENU_UNIVERSE)
        .expect("3-node menu universe is within the 256-field SoC cap")
}

/// The concept a top-nav menu href routes to (the inverse of
/// [`route_for`], with `"/"` special-cased to the board = `ProjectWorkItem`).
/// `None` for an href no served screen owns.
#[must_use]
pub fn menu_target_concept(href: &str) -> Option<&'static str> {
    if href == "/" {
        return Some("ProjectWorkItem");
    }
    SCREEN_UNIVERSE
        .iter()
        .copied()
        .find(|c| route_for(c) == Some(href))
}

/// Whether the klickweg is fully connected **from the top-nav menu**:
/// every served screen is reachable by clicking from [`menu`], and no menu
/// or nav edge dangles off the served set. This is the cross-frontend
/// parity check (ruff #66 Odoo menuitem roots): a screen that exists and
/// is inter-linked but has NO menu path is an orphan the user cannot reach,
/// and this — unlike [`klickweg_is_connected`] — catches it.
#[must_use]
pub fn menu_klickweg_is_connected() -> bool {
    nav_is_fully_connected(MENU_ROOT, MENU_NAV_EDGES, &menu_screens_mask())
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

    /// Cross-frontend parity: the klickweg is fully connected **from the
    /// menu root** — every served screen is reachable by clicking from the
    /// top nav (the render-side twin of Odoo's `menuitem → res_model`
    /// roots, ruff #66). The synthetic `Menu` node itself is in the reached
    /// set, so `reached == MENU_UNIVERSE`.
    #[test]
    fn menu_klickweg_is_fully_connected() {
        assert!(
            menu_klickweg_is_connected(),
            "menu-rooted klickweg not connected: a served screen has no \
             click-path from the top-nav menu (orphan), or a menu/nav edge \
             dangles off the served set"
        );
        let reached =
            lance_graph_contract::class_view::screens_reachable_from(MENU_ROOT, MENU_NAV_EDGES);
        assert_eq!(
            reached,
            menu_screens_mask(),
            "reached set from the menu root must equal the menu-inclusive universe"
        );
    }

    /// The static [`MENU_NAV_EDGES`] table must equal the edges derived from
    /// [`menu`] (menu → tab-target) + [`NAV_EDGES`] (inter-screen, +1 shifted
    /// into the menu-inclusive positions) — no drift between the hand-written
    /// menu-rooted manifest and the live menu + screen graph.
    #[test]
    fn menu_nav_edges_match_derived() {
        // Position of a concept in MENU_UNIVERSE (menu node offsets screens +1).
        let menu_pos = |c: &str| MENU_UNIVERSE.iter().position(|m| *m == c).map(|i| i as u8);

        let mut derived: Vec<(u8, u8)> = Vec::new(); // (target, source)

        // Menu → each tab's target concept.
        for (_label, href) in menu() {
            if *href == "/" || route_for_is_menu_reachable(href) {
                if let Some(target) = menu_target_concept(href).and_then(menu_pos) {
                    derived.push((target, MENU_ROOT));
                }
            }
        }
        // Inter-screen NAV_EDGES, lifted into menu-inclusive positions (+1:
        // SCREEN_UNIVERSE[i] == MENU_UNIVERSE[i+1]).
        for e in NAV_EDGES {
            for &src in e.inputs {
                derived.push((e.target + 1, src + 1));
            }
        }
        derived.sort_unstable();
        derived.dedup();

        let mut table: Vec<(u8, u8)> = MENU_NAV_EDGES
            .iter()
            .flat_map(|e| e.inputs.iter().map(move |&src| (e.target, src)))
            .collect();
        table.sort_unstable();
        table.dedup();

        assert_eq!(
            table, derived,
            "MENU_NAV_EDGES drifted from menu() + NAV_EDGES"
        );
    }

    // Every menu href resolves to a served screen (proven by
    // `menu_targets_are_all_routable`); this helper keeps the derivation
    // above readable.
    fn route_for_is_menu_reachable(href: &str) -> bool {
        menu_target_concept(href).is_some()
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
