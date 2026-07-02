//! aiwar OSINT POC — the `AdaWorldAPI/aiwar-neo4j-harvest` graph as family nodes.
//!
//! Ingest `data/aiwar_graph.json` (via [`ingest_aiwar_json`](crate::literal_graph::ingest_aiwar_json))
//! → map each entity to a canonical OSINT [`NodeRow`] (classid
//! [`NodeGuid::CLASSID_OSINT`], `family` = its category) →
//! [`project_snapshot`](crate::soa_graph::project_snapshot) yields a Gotham graph
//! whose **family nodes ARE the categories** (System / Stakeholder / Person /
//! Civic / Historical) — the stable hubs the entities hang off (extreme render
//! stability, `E-ANCHOR-IS-A-HEAD-FIELD`).
//!
//! This is the OSINT domain's family **class view**: a `category ⇒ family-id`
//! map, head-only, zero value decode. q2's cockpit wires the resulting
//! `GraphSnapshot` to the Quadro-2 visual. Run it on the real graph with
//! `cargo run -p lance-graph-contract --example aiwar_family_poc`.

use crate::canonical_node::{EdgeBlock, NodeGuid, NodeRow};
use crate::literal_graph::LiteralGraph;
use std::collections::{BTreeMap, BTreeSet};

/// The OSINT family **class view**: each distinct entity category (the
/// [`LiteralNode`](crate::literal_graph::LiteralNode) label) mapped to a
/// deterministic 1-based family id (sorted by label). The family nodes in the
/// projection are exactly these categories — the OSINT "classes".
#[derive(Debug, Clone, Default)]
pub struct AiwarClassView {
    families: BTreeMap<String, u32>,
}

impl AiwarClassView {
    /// Build from an ingested aiwar graph: distinct node labels → family ids
    /// `1..=N` (sorted, deterministic; `0` is reserved for the default basin).
    pub fn from_graph(graph: &LiteralGraph) -> Self {
        let mut labels: BTreeSet<String> = BTreeSet::new();
        for id in graph.all_node_ids() {
            if let Some(n) = graph.node(&id) {
                labels.insert(n.label.clone());
            }
        }
        let families = labels
            .into_iter()
            .enumerate()
            .map(|(i, l)| (l, (i as u32) + 1))
            .collect();
        Self { families }
    }

    /// Family id for a category label.
    pub fn family_of(&self, label: &str) -> Option<u32> {
        self.families.get(label).copied()
    }

    /// Number of categories (= number of family nodes).
    pub fn len(&self) -> usize {
        self.families.len()
    }

    /// Whether the view holds no categories.
    pub fn is_empty(&self) -> bool {
        self.families.is_empty()
    }

    /// The `(category, family_id)` pairs, sorted by category.
    pub fn categories(&self) -> impl Iterator<Item = (&str, u32)> {
        self.families.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

/// Map an ingested aiwar graph to canonical OSINT node rows: classid
/// [`NodeGuid::CLASSID_OSINT`], `family` = the entity's category (via
/// [`AiwarClassView`]), `identity` = the node's position. Each out-edge becomes a
/// **family-node adapter** byte (the target category's `family & 0xFF`), so the
/// projected graph links category hubs (Nation → System, etc.). Head-only — the
/// 480-byte value slab stays zero.
pub fn aiwar_node_rows(graph: &LiteralGraph) -> Vec<NodeRow> {
    let view = AiwarClassView::from_graph(graph);
    let ids = graph.all_node_ids();
    let fam_of =
        |id: &str| -> Option<u32> { graph.node(id).and_then(|n| view.family_of(&n.label)) };
    ids.iter()
        .enumerate()
        .map(|(i, id)| {
            let fam = fam_of(id).unwrap_or(0);
            // distinct target-category family low bytes → 16 adapter slots
            let mut slots: Vec<u8> = graph
                .edges_from(id)
                .iter()
                .filter_map(|e| fam_of(&e.target))
                .filter(|tf| *tf != fam)
                .map(|tf| (tf & 0xFF) as u8)
                .collect();
            slots.sort_unstable();
            slots.dedup();
            // aiwar entities connect ACROSS categories (Nation→System, …); every
            // adapter here is cross-family (built from `tf != fam`), so they go in
            // the 4 OUT-of-family slots (labeled `references`), never in-family
            // (`linked`) — otherwise the label would flip with fan-out count and
            // `references` queries would miss them (codex P2, PR #560). Cap at the
            // 4 canonical out-of-family slots.
            let mut edges = EdgeBlock::default();
            for (k, &b) in slots.iter().take(4).enumerate() {
                edges.out_family[k] = b;
            }
            NodeRow {
                key: NodeGuid::new(
                    NodeGuid::CLASSID_OSINT,
                    0,
                    0,
                    0,
                    fam & 0x00FF_FFFF,
                    (i as u32) & 0x00FF_FFFF,
                ),
                edges,
                value: [0u8; 480],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::literal_graph::ingest_aiwar_json;
    use crate::soa_graph::{project_snapshot, OSINT_GOTHAM};

    // A representative slice of the aiwar_graph.json shape (categories via the
    // `type` field; edges across categories). Deterministic, CI-safe.
    const SAMPLE: &str = r#"{
        "N_Systems": [
            {"id": "Lavender", "name": "Lavender", "type": "PredictiveAnalytics"},
            {"id": "Gospel", "name": "Gospel", "type": "PredictiveAnalytics"}
        ],
        "N_Stakeholders": [
            {"id": "Israel", "name": "Israel", "type": "Nation"},
            {"id": "UnitNSO", "name": "NSO Group", "type": "TechCompany"}
        ],
        "E_connection": [
            {"source": "UnitNSO", "target": "Israel", "label": "based in"}
        ],
        "E_isDevelopedBy": [
            {"source": "Israel", "target": "Lavender", "label": "developed"},
            {"source": "Israel", "target": "Gospel", "label": "developed"}
        ]
    }"#;

    #[test]
    fn class_view_maps_categories_to_families() {
        let g = ingest_aiwar_json(SAMPLE).unwrap();
        let view = AiwarClassView::from_graph(&g);
        // categories from the `type` field: PredictiveAnalytics, Nation, TechCompany
        assert_eq!(view.len(), 3);
        assert!(view.family_of("Nation").is_some());
        assert!(view.categories().all(|(_, f)| f >= 1));
    }

    #[test]
    fn projects_to_family_node_graph() {
        let g = ingest_aiwar_json(SAMPLE).unwrap();
        let view = AiwarClassView::from_graph(&g);
        let rows = aiwar_node_rows(&g);
        assert_eq!(rows.len(), g.node_count(), "one row per entity");

        let snap = project_snapshot(&rows, &OSINT_GOTHAM);
        // one family node per category (kind Family/Anchor)
        let family_nodes = snap
            .nodes
            .iter()
            .filter(|n| n.kind == "Family" || n.kind == "Anchor")
            .count();
        assert_eq!(family_nodes, view.len());
        // every entity → member-of edge to its category family hub
        let member_of = snap.edges.iter().filter(|e| e.label == "member-of").count();
        assert_eq!(member_of, g.node_count());
        // Israel (Nation) → the PredictiveAnalytics family hub: a cross-CATEGORY
        // edge, so it carries the out-of-family `references` label — never the
        // in-family `linked` (aiwar edges are all cross-category).
        assert!(snap
            .edges
            .iter()
            .any(|e| e.label == "references" && e.target.starts_with("family:")));
        assert!(
            !snap.edges.iter().any(|e| e.label == "linked"),
            "aiwar edges are all cross-category ⇒ none are in-family `linked`"
        );
    }

    #[test]
    fn rows_are_osint_class_and_head_only() {
        let g = ingest_aiwar_json(SAMPLE).unwrap();
        for row in aiwar_node_rows(&g) {
            assert_eq!(row.key.classid(), NodeGuid::CLASSID_OSINT);
            assert_eq!(row.value, [0u8; 480], "head-only: value slab stays zero");
        }
    }
}
