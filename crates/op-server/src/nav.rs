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
