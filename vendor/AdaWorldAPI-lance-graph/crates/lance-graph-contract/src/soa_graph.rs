//! `soa_graph` — project the canonical SoA head into the Gotham graph surface.
//!
//! The bridge from the **canonical node head** (128-bit [`NodeGuid`] + 128-bit
//! [`EdgeBlock`], `key(16)+edges(16)`, bytes 0..32 of a [`NodeRow`]) to the
//! existing [`graph_render`](crate::graph_render) Neo4j/Palantir-Gotham surface
//! ([`GraphSnapshot`] / [`RenderNode`] / [`RenderEdge`]). **Zero value decode:**
//! every node, edge, family, and anchor here is read from the 32-byte head —
//! the 480-byte value slab is never touched (`E-GUID-IS-THE-GRAPH`; the same
//! falsifiable invariant `symbiont::key_render` proves by 0xFF-poisoning).
//!
//! **Rendering lives in q2.** This module produces the *structural* snapshot;
//! the q2 `cockpit-server` cockpit (vis-network / Neo4j-Browser-style UI) lays
//! it out and draws it. What lance-graph owns is "the basic domain + SoA as a
//! graph"; q2 owns the pixels.
//!
//! ## Two head axes, two graph roles
//!
//! The canonical key carries two orthogonal grouping axes, both in the head:
//!
//! - **family** (`u24`, bytes 10..13) — the *basin leaf*. [`project_snapshot`]
//!   groups member nodes by `family` and emits one **family node** per distinct
//!   family (the "use family nodes" requirement). A family node is an **anchor**
//!   when its id is in [`DomainSpec::anchor_families`] (FMA *bones* / OSINT *key
//!   entities* — the stability anchors layout hangs off).
//! - **HHTL path** (`classid_lo·HEEL·HIP·TWIG`, via
//!   [`NiblePath::from_guid_prefix`]) — the *Abstammung tree*. [`nearest_anchor`]
//!   ranks every node against the anchor families by
//!   [`NiblePath::family_hop_count`] (CLAM tree distance) — the "HHTL CLAM via
//!   family-nodes hop count as adjacency" metric.
//!
//! ## Edge resolution — 16 × 8-bit family-node adapters
//!
//! `EdgeCodecFlavor::CoarseOnly` over the canonical 16-byte [`EdgeBlock`], read
//! as **16 family-node adapter slots** (operator model, 2026-06-20): every
//! non-zero edge byte references a FAMILY (not an individual member), resolved by
//! `family & 0xFF` → the family node. The 12 in-family slots emit
//! [`DomainSpec::in_family_edge`] edges, the 4 out-of-family slots emit
//! [`DomainSpec::out_family_edge`] edges; both land on a stable family node.
//!
//! Why family adapters (not member-by-identity): edges to families give
//! **extreme render stability** — family nodes are fixed anchors, members attach
//! to stable hubs, the layout doesn't churn — and **huge flexibility** (a node
//! mixes in up to 16 family adjacencies). The one limitation is **mixin
//! dependency**: a referenced family must exist, else the slot is a dangling
//! adapter (skipped). It also dissolves the >255-member aliasing — resolution is
//! only ever family-level (256 families per low-byte space), and an ambiguous low
//! byte (two families sharing it) is skipped, never mis-routed. Member→member
//! edges and wider encodings (8×16-bit, 32×4 residue, second-hop) are deferred
//! richer flavors.
//!
//! A `classid` IS the class (granular, exact — not a 2-nibble prefix): the
//! projector includes only rows whose `classid == domain.classid`, so a
//! mixed-class board never leaks one domain's nodes into another's view.
//!
//! Two domains ship registered: [`OSINT_GOTHAM`] (classid
//! [`NodeGuid::CLASSID_OSINT`]) and [`FMA_ANATOMY`] (classid
//! [`NodeGuid::CLASSID_FMA`]). New domains are just another `DomainSpec` —
//! the projector is domain-agnostic.

use crate::canonical_node::{NodeGuid, NodeRow};
use crate::graph_render::{GraphSnapshot, RenderEdge, RenderNode};
use crate::hhtl::NiblePath;
use std::collections::HashMap;

/// A graph domain: how a class of SoA nodes is labelled and which families are
/// stability anchors. Domain-agnostic data (no behaviour) — the projector reads
/// it. `&'static` so domains can be `const` (see [`OSINT_GOTHAM`], [`FMA_ANATOMY`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainSpec {
    /// OGAR classid this domain occupies (the GUID routing prefix).
    pub classid: u32,
    /// Human name, used as the member node `kind` (e.g. "OSINT/Gotham").
    pub name: &'static str,
    /// Families that are **stability anchors** (FMA bones / OSINT key entities).
    /// Family nodes in this set render as `kind = "Anchor"` and are the targets
    /// [`nearest_anchor`] measures hop distance to.
    pub anchor_families: &'static [u32],
    /// Edge label for intra-family adjacency (`in_family` slots).
    pub in_family_edge: &'static str,
    /// Edge label for cross-family links (`out_family` slots).
    pub out_family_edge: &'static str,
    /// Edge label for the member → family-node containment edge.
    pub member_edge: &'static str,
}

/// The **OSINT / Palantir-Gotham** domain (classid [`NodeGuid::CLASSID_OSINT`]):
/// a neo4j-emulation entity graph. Anchor families are caller-supplied (the key
/// entities of an investigation); the default declares none.
pub const OSINT_GOTHAM: DomainSpec = DomainSpec {
    classid: NodeGuid::CLASSID_OSINT,
    name: "OSINT/Gotham",
    anchor_families: &[],
    in_family_edge: "linked",
    out_family_edge: "references",
    member_edge: "member-of",
};

/// The **FMA anatomy** domain (classid [`NodeGuid::CLASSID_FMA`]): ~70k
/// structural entities, family = body region, `out_family` = part-of. Anchor
/// families are the *bones* (the skeleton the soft tissue hangs off); the
/// default declares none — a caller supplies the bone families.
pub const FMA_ANATOMY: DomainSpec = DomainSpec {
    classid: NodeGuid::CLASSID_FMA,
    name: "FMA-Anatomy",
    anchor_families: &[],
    in_family_edge: "adjacent-to",
    out_family_edge: "part-of",
    member_edge: "part-of",
};

/// The **project-management** domain (classid [`NodeGuid::CLASSID_PROJECT`],
/// OGAR `0x01XX`): OpenProject ↔ Redmine work items. Family = project / version;
/// `in_family` = relates-to, `out_family` = blocks (cross-project dependency).
/// Anchor families are caller-supplied (the milestone / release hubs).
pub const PROJECT: DomainSpec = DomainSpec {
    classid: NodeGuid::CLASSID_PROJECT,
    name: "Project",
    anchor_families: &[],
    in_family_edge: "relates-to",
    out_family_edge: "blocks",
    member_edge: "in-project",
};

/// The **commerce / ERP** domain (classid [`NodeGuid::CLASSID_ERP`], OGAR
/// `0x02XX`): Odoo ↔ OSB invoices / partners / taxes. Family = partner / journal;
/// `in_family` = line-of, `out_family` = paid-by (cross-partner settlement).
/// Anchor families are caller-supplied (the key accounts / journals).
pub const ERP: DomainSpec = DomainSpec {
    classid: NodeGuid::CLASSID_ERP,
    name: "ERP",
    anchor_families: &[],
    in_family_edge: "line-of",
    out_family_edge: "paid-by",
    member_edge: "in-ledger",
};

/// The synthetic id of a family node in the snapshot (`"family:RRGGBB"` hex).
#[inline]
fn family_node_id(family: u32) -> String {
    format!("family:{family:06x}")
}

/// HHTL routing path of a GUID. The fold is selected by the classid's
/// `tail_variant` — **the schema decides how** (OGAR #128 `classid → {tail_variant,
/// …}`). A **V3** classid routes on the `(part_of:is_a)` `HEEL·HIP·TWIG·LEAF`
/// cascade ([`NiblePath::from_guid_prefix_v3`], both bytes per tier); `classid`
/// is NOT folded, so its high-`u16` generation marker does not gate routing and
/// never collapses to [`NiblePath::EMPTY`]. Every other classid uses the canonical
/// v1 lowering (`classid_lo·HEEL·HIP·TWIG`), which falls back to
/// [`NiblePath::EMPTY`] only for the v1-fold case of a non-zero high `classid` u16.
#[inline]
fn hhtl_path(guid: &NodeGuid) -> NiblePath {
    #[cfg(feature = "guid-v3-tail")]
    {
        use crate::canonical_node::{classid_read_mode, TailVariant};
        if classid_read_mode(guid.classid()).tail_variant == TailVariant::V3 {
            return NiblePath::from_guid_prefix_v3(guid);
        }
    }
    NiblePath::from_guid_prefix(guid).unwrap_or(NiblePath::EMPTY)
}

/// The node's basin-`family` id, decoded per its `tail_variant` — the
/// family-grouping/anchoring counterpart of [`hhtl_path`]'s V3 routing branch.
/// A V2/V3 tail stores `family` in bytes 12..14 ([`NodeGuid::family_v2`]); the V1
/// tail in bytes 10..13 ([`NodeGuid::family`]). Reading the V1 `family()` on a
/// V2/V3 row would fold the `leaf` byte into the id (`I-LEGACY-API-FEATURE-GATED`),
/// so the family decode is routed through the canonical `classid → tail_variant`
/// mapping exactly like the path is. Under no tail feature every classid is V1, so
/// this is just `family()`.
#[inline]
fn family_of(guid: &NodeGuid) -> u32 {
    #[cfg(feature = "guid-v2-tail")]
    {
        use crate::canonical_node::{classid_read_mode, TailVariant};
        if matches!(
            classid_read_mode(guid.classid()).tail_variant,
            TailVariant::V2 | TailVariant::V3
        ) {
            return guid.family_v2() as u32;
        }
    }
    guid.family()
}

/// The node's `identity` id, decoded per its `tail_variant` (sibling of
/// [`family_of`]): V2/V3 read [`NodeGuid::identity_v2`] (bytes 14..16), V1 reads
/// [`NodeGuid::identity`] (bytes 13..16).
#[inline]
fn identity_of(guid: &NodeGuid) -> u32 {
    #[cfg(feature = "guid-v2-tail")]
    {
        use crate::canonical_node::{classid_read_mode, TailVariant};
        if matches!(
            classid_read_mode(guid.classid()).tail_variant,
            TailVariant::V2 | TailVariant::V3
        ) {
            return guid.identity_v2() as u32;
        }
    }
    guid.identity()
}

/// Project a board-set into a [`GraphSnapshot`] for the Gotham/neo4j surface —
/// member nodes + family nodes + (member→family, in-family, out-of-family)
/// edges. Touches ONLY the 32-byte head of each row (`key` + `edges`); never the
/// value slab.
pub fn project_snapshot(rows: &[NodeRow], domain: &DomainSpec) -> GraphSnapshot {
    // codex P1: a classid IS the class — project only rows of THIS domain, so a
    // mixed-class board can't leak other domains' nodes/edges into the view.
    let domain_rows: Vec<&NodeRow> = rows
        .iter()
        .filter(|r| r.key.classid() == domain.classid)
        .collect();

    // family → member count, and a COLLISION-AWARE family-low-byte → family map.
    // codex P1: with >256 families two ids can share a low byte; a duplicate
    // marks the slot ambiguous (None) so an adapter byte is skipped, never
    // mis-routed (the family-adapter model: edges resolve only at family level).
    let mut by_family: HashMap<u32, usize> = HashMap::new();
    let mut family_by_low: HashMap<u8, Option<u32>> = HashMap::new();
    for row in &domain_rows {
        let fam = family_of(&row.key);
        *by_family.entry(fam).or_insert(0) += 1;
        family_by_low
            .entry((fam & 0xFF) as u8)
            .and_modify(|e| {
                if *e != Some(fam) {
                    *e = None; // collision ⇒ ambiguous
                }
            })
            .or_insert(Some(fam));
    }

    let mut nodes: Vec<RenderNode> = Vec::with_capacity(domain_rows.len() + by_family.len());
    let mut edges: Vec<RenderEdge> = Vec::new();

    // One family node per distinct family (the stable anchors). Sorted for
    // deterministic output regardless of HashMap iteration order.
    let mut families: Vec<(&u32, &usize)> = by_family.iter().collect();
    families.sort_by_key(|(fam, _)| **fam);
    for (&fam, &members) in families {
        let is_anchor = domain.anchor_families.contains(&fam);
        nodes.push(RenderNode {
            id: family_node_id(fam),
            label: format!("{} family {fam:06x}", domain.name),
            kind: if is_anchor { "Anchor" } else { "Family" }.to_string(),
            confidence: 1.0,
            props: vec![
                ("family".to_string(), format!("{fam:06x}")),
                ("members".to_string(), members.to_string()),
                ("anchor".to_string(), is_anchor.to_string()),
            ],
        });
    }

    // Resolve a family-adapter byte to its (unambiguous, non-self) family node.
    let resolve = |b: u8, own: u32| -> Option<String> {
        match family_by_low.get(&b).copied().flatten() {
            Some(fam) if fam != own => Some(family_node_id(fam)),
            _ => None,
        }
    };

    // Member nodes + their edges (all head-only, family-adapter resolution).
    for row in &domain_rows {
        let g = row.key;
        let fam = family_of(&g);
        nodes.push(RenderNode {
            id: g.to_string(),
            label: format!("{:06x}", identity_of(&g)),
            kind: domain.name.to_string(),
            confidence: 1.0,
            props: vec![
                ("classid".to_string(), format!("{:08x}", g.classid())),
                ("family".to_string(), format!("{fam:06x}")),
                ("hhtl_depth".to_string(), hhtl_path(&g).depth().to_string()),
            ],
        });
        // member → own family containment
        edges.push(RenderEdge {
            source: g.to_string(),
            target: family_node_id(fam),
            label: domain.member_edge.to_string(),
            frequency: 1.0,
            confidence: 1.0,
            inferred: false,
        });
        // 16 family-node adapters: 12 in-family + 4 out-of-family, each → a family.
        let eb = row.edges;
        for &b in eb.in_family.iter().filter(|&&b| b != 0) {
            if let Some(target) = resolve(b, fam) {
                edges.push(RenderEdge {
                    source: g.to_string(),
                    target,
                    label: domain.in_family_edge.to_string(),
                    frequency: 1.0,
                    confidence: 1.0,
                    inferred: false,
                });
            }
        }
        for &b in eb.out_family.iter().filter(|&&b| b != 0) {
            if let Some(target) = resolve(b, fam) {
                edges.push(RenderEdge {
                    source: g.to_string(),
                    target,
                    label: domain.out_family_edge.to_string(),
                    frequency: 1.0,
                    confidence: 1.0,
                    inferred: false,
                });
            }
        }
    }

    GraphSnapshot {
        nodes,
        edges,
        inferences: Vec::new(),
        contradictions: Vec::new(),
        timestamp: 0,
    }
}

/// A node's CLAM hop distance to its nearest stability anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorHop {
    /// The node measured.
    pub node: NodeGuid,
    /// The family id of the nearest anchor (`u32::MAX` if the domain declares
    /// none, or none is reachable).
    pub anchor_family: u32,
    /// HHTL CLAM hop count to that anchor's representative path (`u8::MAX` when
    /// no anchor exists).
    pub hops: u8,
}

/// For each node, the nearest stability-anchor family by **HHTL CLAM hop count**
/// ([`NiblePath::family_hop_count`] over the GUIDs' HHTL paths) — the "bones as
/// stability anchor" layout signal: each node hangs off its closest anchor, and
/// the hop count is the adjacency weight the q2 layout uses (anchors fixed, soft
/// tissue positioned by distance). Anchors are the families in
/// [`DomainSpec::anchor_families`]; their representative path is the first member
/// seen. Pure head arithmetic, zero value decode. O(rows × anchors).
///
/// The canonical lowering is fixed-depth-16, so `hops = 2·(16 − lcp)` (`lcp` =
/// shared-prefix nibble count) — a monotone prefix distance, not a variable-depth
/// tree walk: smaller hops ⇔ deeper shared `classid_lo·HEEL·HIP·TWIG` prefix.
/// Ranking (nearest anchor) is what callers use; the absolute value is even.
pub fn nearest_anchor(rows: &[NodeRow], domain: &DomainSpec) -> Vec<AnchorHop> {
    // codex P1: only rank rows of THIS domain (classid IS the class).
    let domain_rows: Vec<&NodeRow> = rows
        .iter()
        .filter(|r| r.key.classid() == domain.classid)
        .collect();
    // Representative HHTL path per anchor family (first member encountered).
    let mut anchor_paths: Vec<(u32, NiblePath)> = Vec::new();
    for row in &domain_rows {
        let fam = family_of(&row.key);
        if domain.anchor_families.contains(&fam) && !anchor_paths.iter().any(|(f, _)| *f == fam) {
            anchor_paths.push((fam, hhtl_path(&row.key)));
        }
    }
    domain_rows
        .iter()
        .map(|row| {
            let g = row.key;
            let p = hhtl_path(&g);
            let mut anchor_family = u32::MAX;
            let mut hops = u8::MAX;
            for &(fam, ap) in &anchor_paths {
                let h = p.family_hop_count(ap);
                if h < hops {
                    hops = h;
                    anchor_family = fam;
                }
            }
            AnchorHop {
                node: g,
                anchor_family,
                hops,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_node::EdgeBlock;

    /// Build a node in a domain: `classid` from the domain, hierarchy in the
    /// `hht` tiers `(heel, hip, twig)`, family = basin leaf, identity = leaf.
    /// Edges optional.
    fn node(
        domain: &DomainSpec,
        hht: (u16, u16, u16),
        family: u32,
        identity: u32,
        in_fam: &[u8],
        out_fam: &[u8],
    ) -> NodeRow {
        let mut edges = EdgeBlock::default();
        for (i, &b) in in_fam.iter().enumerate().take(12) {
            edges.in_family[i] = b;
        }
        for (i, &b) in out_fam.iter().enumerate().take(4) {
            edges.out_family[i] = b;
        }
        NodeRow {
            key: NodeGuid::new(domain.classid, hht.0, hht.1, hht.2, family, identity),
            edges,
            value: [0u8; 480],
        }
    }

    #[cfg(feature = "guid-v3-tail")]
    #[test]
    fn v3_rows_decode_family_and_identity_via_tail_variant() {
        use crate::canonical_node::classid_read_mode;
        // A V3-tail row (mint_for V3 -> new_v2 layout): leaf=0xAAAA @10..12,
        // family=0xBBBB @12..14, identity=0xCCCC @14..16. The codex finding: the V1
        // family()/identity() decode would fold the leaf byte into the id, so the
        // projection must route through the tail variant (I-LEGACY-API-FEATURE-GATED).
        let tv = classid_read_mode(NodeGuid::CLASSID_FMA_V3).tail_variant;
        let g = NodeGuid::mint_for(
            tv,
            NodeGuid::CLASSID_FMA_V3,
            1,
            0,
            0,
            0xAAAA,
            0xBBBB,
            0xCCCC,
        );

        // The tail-aware helpers read the V3 basin (family_v2 / identity_v2)…
        assert_eq!(family_of(&g), 0xBBBB, "V3 family = family_v2 (12..14)");
        assert_eq!(
            identity_of(&g),
            0xCCCC,
            "V3 identity = identity_v2 (14..16)"
        );
        // …whereas the raw V1 decode is polluted by the leaf byte (the trap).
        assert_ne!(
            g.family(),
            0xBBBB,
            "V1 family() folds leaf into the id on a V3 row"
        );

        // project_snapshot with a V3 DomainSpec groups members by the V3 family.
        let dom = DomainSpec {
            classid: NodeGuid::CLASSID_FMA_V3,
            name: "fma-v3",
            anchor_families: &[],
            in_family_edge: "adjacent-to",
            out_family_edge: "part-of",
            member_edge: "part-of",
        };
        let rows = [NodeRow {
            key: g,
            edges: EdgeBlock::default(),
            value: [0u8; 480],
        }];
        let snap = project_snapshot(&rows, &dom);
        assert!(
            snap.nodes.iter().any(|n| n.kind == "Family"
                && n.props.iter().any(|(k, v)| k == "family" && v == "00bbbb")),
            "family node keyed by the V3 family_v2 (0x00bbbb), not the polluted V1 family"
        );
        assert!(
            snap.nodes.iter().any(|n| n.label == "00cccc"),
            "member labeled by identity_v2"
        );
    }

    #[test]
    fn project_emits_family_nodes_and_family_adapter_edges() {
        // Two families (0xA, 0xB), two members each. Member 1 in family A carries
        // an in-family adapter byte 0x0B (→ family B, "linked") and an
        // out-of-family adapter byte 0x0B (→ family B, "references"). Edges resolve
        // to the FAMILY node, not an individual member (the 16-adapter model).
        let rows = [
            node(&OSINT_GOTHAM, (1, 0, 0), 0xA, 1, &[0x0B], &[0x0B]),
            node(&OSINT_GOTHAM, (1, 0, 0), 0xA, 2, &[], &[]),
            node(&OSINT_GOTHAM, (2, 0, 0), 0xB, 1, &[], &[]),
            node(&OSINT_GOTHAM, (2, 0, 0), 0xB, 2, &[], &[]),
        ];
        let snap = project_snapshot(&rows, &OSINT_GOTHAM);
        // 4 member nodes + 2 family nodes
        assert_eq!(snap.nodes.len(), 6);
        let family_nodes = snap.nodes.iter().filter(|n| n.kind == "Family").count();
        assert_eq!(family_nodes, 2);
        // every member has a member-of edge → 4 of them
        let member_of = snap.edges.iter().filter(|e| e.label == "member-of").count();
        assert_eq!(member_of, 4);
        // in-family adapter byte 0x0B → family:00000b ("linked")
        assert!(snap
            .edges
            .iter()
            .any(|e| e.label == "linked" && e.target == "family:00000b"));
        // out-of-family adapter byte 0x0B → family:00000b ("references")
        assert!(snap
            .edges
            .iter()
            .any(|e| e.label == "references" && e.target == "family:00000b"));
        // No edge targets an individual member (everything lands on a family node).
        assert!(snap.edges.iter().all(|e| e.target.starts_with("family:")));
    }

    #[test]
    fn ambiguous_family_low_byte_is_skipped_not_misrouted() {
        // codex P1 #2: two families sharing low byte 0x00 (0x0100, 0x0200) are
        // ambiguous; a node referencing 0x00 must NOT render an edge to either
        // (skipped, never the wrong one). A node referencing the unambiguous
        // 0x55 family DOES resolve.
        let rows = [
            node(&OSINT_GOTHAM, (1, 0, 0), 0x0100, 1, &[0x00], &[]),
            node(&OSINT_GOTHAM, (1, 0, 0), 0x0200, 1, &[], &[]),
            node(&OSINT_GOTHAM, (1, 0, 0), 0x0055, 1, &[0x55], &[]),
        ];
        let snap = project_snapshot(&rows, &OSINT_GOTHAM);
        // The ambiguous 0x00 adapter resolves to nothing — no "linked" edge from
        // the 0x0100 member.
        let from_0100 = snap
            .edges
            .iter()
            .filter(|e| e.label == "linked" && e.source.ends_with("010000000001"))
            .count();
        assert_eq!(from_0100, 0, "ambiguous low byte must be skipped");
        // 0x55 is unambiguous → the 0x0055 member links to family:000055 (self,
        // skipped because own family) — assert the ambiguity skip didn't crash and
        // every emitted edge still targets a real family node.
        assert!(snap.edges.iter().all(|e| e.target.starts_with("family:")));
    }

    #[test]
    fn mixed_class_board_excludes_other_domains() {
        // codex P1 #1: a board with one OSINT row + one FMA row, projected as
        // OSINT, yields ONLY the OSINT member + its family — the FMA node and its
        // family are excluded (classid IS the class).
        let rows = [
            node(&OSINT_GOTHAM, (1, 0, 0), 0xA, 1, &[], &[]),
            node(&FMA_ANATOMY, (1, 0, 0), 0xB, 1, &[], &[]),
        ];
        let snap = project_snapshot(&rows, &OSINT_GOTHAM);
        // 1 OSINT member + 1 family node (0xA) = 2; FMA's 0xB family absent.
        assert_eq!(snap.nodes.len(), 2);
        assert!(snap.nodes.iter().all(|n| n.kind != "FMA-Anatomy"));
        assert!(snap.nodes.iter().any(|n| n.id == "family:00000a"));
        assert!(!snap.nodes.iter().any(|n| n.id == "family:00000b"));
    }

    #[test]
    fn anchor_families_render_as_anchor_kind() {
        // FMA: family 0x01 is a "bone" anchor; 0x02 is soft tissue.
        let fma_bones = DomainSpec {
            anchor_families: &[0x01],
            ..FMA_ANATOMY
        };
        let rows = [
            node(&fma_bones, (0x1, 0, 0), 0x01, 1, &[], &[]), // bone
            node(&fma_bones, (0x2, 0, 0), 0x02, 1, &[], &[]), // tissue
        ];
        let snap = project_snapshot(&rows, &fma_bones);
        let anchor = snap.nodes.iter().find(|n| n.id == "family:000001").unwrap();
        assert_eq!(anchor.kind, "Anchor");
        let tissue = snap.nodes.iter().find(|n| n.id == "family:000002").unwrap();
        assert_eq!(tissue.kind, "Family");
    }

    #[test]
    fn nearest_anchor_ranks_by_hhtl_hop_count() {
        // The canonical lowering is fixed-depth-16, so family_hop_count = 2·(16 −
        // lcp): the deeper the shared prefix, the fewer hops. Anchor family 0x01
        // sits at heel=0x1000. Same path ⇒ 0; a node differing in the last HEEL
        // nibble (lcp=7) ⇒ 18; a node differing in the first HEEL nibble (lcp=4)
        // ⇒ 24. What matters is the ordering (closer prefix ⇒ smaller hops).
        let fma_bones = DomainSpec {
            anchor_families: &[0x01],
            ..FMA_ANATOMY
        };
        let rows = [
            node(&fma_bones, (0x1000, 0, 0), 0x01, 1, &[], &[]), // the anchor itself
            node(&fma_bones, (0x1000, 0, 0), 0x02, 1, &[], &[]), // same HHT path
            node(&fma_bones, (0x1009, 0, 0), 0x03, 1, &[], &[]), // diverges late (lcp 7)
            node(&fma_bones, (0xF000, 0, 0), 0x04, 1, &[], &[]), // diverges early (lcp 4)
        ];
        let hops = nearest_anchor(&rows, &fma_bones);
        assert_eq!(hops.len(), 4);
        assert_eq!(hops[0].hops, 0);
        assert_eq!(hops[0].anchor_family, 0x01);
        assert_eq!(hops[1].hops, 0, "same HHT path as the anchor ⇒ 0 hops");
        assert!(
            hops[1].hops < hops[2].hops && hops[2].hops < hops[3].hops,
            "monotone: closer shared prefix ⇒ fewer hops ({} < {} < {})",
            hops[1].hops,
            hops[2].hops,
            hops[3].hops
        );
        // The exact fixed-depth-16 values: 2·(16−7)=18 and 2·(16−4)=24.
        assert_eq!(hops[2].hops, 18);
        assert_eq!(hops[3].hops, 24);
    }

    #[test]
    fn nearest_anchor_with_no_anchors_is_unreachable() {
        // Default OSINT declares no anchor families ⇒ every node is unreachable.
        let rows = [node(&OSINT_GOTHAM, (1, 0, 0), 0xA, 1, &[], &[])];
        let hops = nearest_anchor(&rows, &OSINT_GOTHAM);
        assert_eq!(hops[0].hops, u8::MAX);
        assert_eq!(hops[0].anchor_family, u32::MAX);
    }

    #[test]
    fn projection_is_head_only_zero_value_decode() {
        // Poison the value slab; the snapshot must be byte-identical (the
        // E-GUID-IS-THE-GRAPH / zero-value-decode invariant, falsifiable).
        let clean = [
            node(&OSINT_GOTHAM, (1, 0, 0), 0xA, 1, &[2], &[0xB]),
            node(&OSINT_GOTHAM, (2, 0, 0), 0xB, 2, &[], &[]),
        ];
        let mut poisoned = clean;
        for row in &mut poisoned {
            row.value = [0xFFu8; 480];
        }
        let a = project_snapshot(&clean, &OSINT_GOTHAM);
        let b = project_snapshot(&poisoned, &OSINT_GOTHAM);
        // GraphSnapshot isn't PartialEq; compare the structural projection.
        let key = |s: &GraphSnapshot| {
            (
                s.nodes
                    .iter()
                    .map(|n| (n.id.clone(), n.kind.clone()))
                    .collect::<Vec<_>>(),
                s.edges
                    .iter()
                    .map(|e| (e.source.clone(), e.target.clone(), e.label.clone()))
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(key(&a), key(&b));
    }
}
