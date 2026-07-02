//! # `hhtl` — the 16ⁿ nibble bucket router (the Abstammung tree axis).
//!
//! `wikidata-hhtl-load.md` §"HHTL = the cheap bucket router (16^n)": the ONE tree
//! axis is the `subClassOf` (P279) path, addressed as a fixed-fan-out-16 nibble
//! sequence — *"bucket path = nibble sequence → routing is bit-shift, not hash
//! lookup. O(1) arithmetic (super billig)."* This is the **downstream bucket
//! router** PR #438 (D-ARM-14 Phase 1) names but did not build: aerial discovers
//! the OWL/DOLCE skeleton + basins; this routes an entity into its 16ⁿ bucket.
//!
//! **Domain-agnostic by construction.** The router takes a `basin` nibble
//! (`0x0..=0xF`) and child nibbles; it does NOT know DOLCE. The DOLCE→basin
//! binding is resolved THROUGH the ontology cache (OD-DOLCE ratification, #441
//! `b31464d` "DOLCE-from-cache, dissolves 6v4") — never a hard-coded enum here.
//! So the duplicated `DolceCategory` (arm-discovery discovery-side *vs* ontology
//! cache-side) is dissolved at the **resolution layer**, not by a third copy in
//! contract: the structural router has zero DOLCE knowledge.
//!
//! The DOLCE top facets seed basins `0..3` by the cache's stable `dolce_id`
//! ordering — `ENDURANT=0`, `PERDURANT=1`, `QUALITY=2`, `ABSTRACT=3` (#441
//! `class_resolver::dolce_id`) — which is ALSO the order of arm-discovery's
//! discovery-side `DolceCategory::basin()` (#438). Both sides of the firewall
//! therefore agree on the nibble without either embedding the enum here; the
//! remaining `0x4..=0xF` basins are reserved (append-only) for finer top axes.
//! The Wikidata "D-CLS triple" `(class_id, shape_hash, presence_bitmask)` is
//! `(ClassId, StructuralSignature, FieldMask)` from #441; this path is its
//! addressing.
//!
//! **One tree axis only (`wikidata-hhtl-load.md`:46).** Multi-parent
//! ("flying-family") is NOT a second nibble path — it is an orthogonal facet bit
//! in the SAME [`FieldMask`](crate::class_view::FieldMask). *"Bat = mammal-path +
//! flight-bit, not two paths."* This keeps 16ⁿ a clean tree (cheap nibble
//! addressing) AND keeps multi-parent dedup.
//!
//! **mask-inherits-as-delta.** Walking DOWN the path is IS-A inheritance: a
//! child's presence mask is the parent's OR its own delta
//! ([`FieldMask::inherit`](crate::class_view::FieldMask::inherit)). N3 stable
//! positions mean the parent's bits never move; the child only adds.

/// Fixed HHTL fan-out: 16 children per level (one nibble). `wikidata-hhtl-load.md`:44.
pub const FAN_OUT: u8 = 16;

/// Max depth addressable in a single `u64` path (16 nibbles × 4 bits = 64).
/// Beyond this the bit-budget discipline says switch to a ref, not a deeper
/// nibble (`wikidata-hhtl-load.md`:71 "grows unbounded → path/ref").
pub const MAX_DEPTH: u8 = 16;

/// A path in the 16ⁿ Abstammung tree — a nibble sequence, root-first, packed into
/// a `u64`. Routing is bit-shift, not hash (O(1) arithmetic).
///
/// Layout: the root (basin) nibble occupies the highest *used* nibble; each
/// [`child`](NiblePath::child) shifts the accumulated path left 4 and ORs the new
/// leaf nibble into the low 4 bits. [`depth`](NiblePath::depth) counts the nibbles
/// used, so a partially-filled `u64` is unambiguous (leading zero nibbles are
/// "not yet routed", not basin 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NiblePath {
    path: u64,
    depth: u8,
}

impl NiblePath {
    /// The empty path — no basin routed yet.
    pub const EMPTY: Self = Self { path: 0, depth: 0 };

    /// Start a path at a `basin` nibble — the DOLCE top facet, resolved UPSTREAM
    /// through the ontology cache (not decided here). An out-of-range
    /// `basin >= FAN_OUT` (16) returns [`EMPTY`](NiblePath::EMPTY), the "no route"
    /// sentinel — NOT a silent fold onto a valid basin (which would misroute
    /// ancestry — CodeRabbit #442). Mirrors [`child`](NiblePath::child)'s
    /// out-of-range no-op and `FieldMask`'s ignore-don't-fold discipline.
    #[must_use]
    pub const fn root(basin: u8) -> Self {
        if basin >= FAN_OUT {
            Self::EMPTY
        } else {
            Self {
                path: basin as u64,
                depth: 1,
            }
        }
    }

    /// Route one level deeper to child `nibble`. **Saturating:** returns `self`
    /// UNCHANGED once [`MAX_DEPTH`] is reached or `nibble >= FAN_OUT` (out of range —
    /// never folded onto a valid child, mirroring
    /// [`FieldMask`](crate::class_view::FieldMask)'s out-of-range discipline).
    ///
    /// At [`MAX_DEPTH`] the silent saturation means two *distinct* deeper paths would
    /// collide on this address — so a real-scale caller MUST gate on
    /// [`is_full`](NiblePath::is_full) or use [`try_child`](NiblePath::try_child),
    /// which signal the ceiling instead of colliding (D-ARM-14 review of #442).
    #[must_use]
    pub const fn child(self, nibble: u8) -> Self {
        if self.depth >= MAX_DEPTH || nibble >= FAN_OUT {
            self
        } else {
            Self {
                path: (self.path << 4) | (nibble as u64),
                depth: self.depth + 1,
            }
        }
    }

    /// Has this path reached [`MAX_DEPTH`] — i.e. [`child`](NiblePath::child) can no
    /// longer descend within the `u64`? When `true`, the bit-budget discipline
    /// (`wikidata-hhtl-load.md`:71 "grows unbounded → path/ref") says switch to a
    /// ref for deeper addressing: descending anyway via [`child`] is a SILENT no-op,
    /// so two distinct deeper classes would collide on this same path. The deferred
    /// 115M loader gates each descent on this (D-ARM-14 review of #442).
    #[must_use]
    pub const fn is_full(self) -> bool {
        self.depth >= MAX_DEPTH
    }

    /// Route one level deeper, returning `None` instead of silently saturating when
    /// the path [`is_full`](NiblePath::is_full) or `nibble >= FAN_OUT`. The explicit
    /// counterpart to [`child`](NiblePath::child) for callers that must NOT collide
    /// distinct deep paths (the real-scale loader).
    #[must_use]
    pub const fn try_child(self, nibble: u8) -> Option<Self> {
        if self.depth >= MAX_DEPTH || nibble >= FAN_OUT {
            None
        } else {
            Some(Self {
                path: (self.path << 4) | (nibble as u64),
                depth: self.depth + 1,
            })
        }
    }

    /// The basin (root) nibble — the DOLCE top facet this path lives under.
    /// `None` for the empty path.
    #[must_use]
    pub const fn basin(self) -> Option<u8> {
        if self.depth == 0 {
            None
        } else {
            Some(((self.path >> (4 * (self.depth as u32 - 1))) & 0x0F) as u8)
        }
    }

    /// The leaf (deepest) nibble. `None` for the empty path.
    #[must_use]
    pub const fn leaf(self) -> Option<u8> {
        if self.depth == 0 {
            None
        } else {
            Some((self.path & 0x0F) as u8)
        }
    }

    /// The parent path (one level shallower). `None` at the basin/empty — the
    /// basin has no parent in this tree (it IS the DOLCE top facet).
    #[must_use]
    pub const fn parent(self) -> Option<Self> {
        if self.depth <= 1 {
            None
        } else {
            Some(Self {
                path: self.path >> 4,
                depth: self.depth - 1,
            })
        }
    }

    /// Depth (number of nibbles routed).
    #[must_use]
    pub const fn depth(self) -> u8 {
        self.depth
    }

    /// Is this path a prefix of (ancestor-or-equal of) `other`? — the cheap
    /// arithmetic reachability test that replaces a P279\* graph walk. An empty
    /// path is an ancestor of nothing (there is no basin to share).
    #[must_use]
    pub const fn is_ancestor_of(self, other: Self) -> bool {
        if self.depth == 0 || self.depth > other.depth {
            false
        } else {
            // Align `other` down to self.depth, then compare the shared prefix.
            (other.path >> (4 * (other.depth as u32 - self.depth as u32))) == self.path
        }
    }

    /// The raw packed `(path, depth)` — for SoA facet-column storage / CAM key.
    #[must_use]
    pub const fn packed(self) -> (u64, u8) {
        (self.path, self.depth)
    }

    /// Reconstruct a path from its raw packed `(path, depth)` — the inverse of
    /// [`packed`](NiblePath::packed). General HHTL utility for round-tripping
    /// a routing path through its packed `(u64, depth)` form.
    ///
    /// Returns `None` if `depth > MAX_DEPTH`, or if `path` has bits set above the
    /// `depth` nibbles (an inconsistent pack — leading nibbles must be the route,
    /// trailing high bits must be zero). `from_packed(0, 0)` is [`EMPTY`](NiblePath::EMPTY).
    #[must_use]
    pub const fn from_packed(path: u64, depth: u8) -> Option<Self> {
        if depth > MAX_DEPTH {
            return None;
        }
        // `path` must fit in `depth` nibbles (4·depth bits); higher bits must be 0.
        // At MAX_DEPTH (16 nibbles = 64 bits) the whole u64 is usable — skip the
        // shift (a `>> 64` would be UB).
        let used_bits = 4 * depth as u32;
        if used_bits < 64 && (path >> used_bits) != 0 {
            return None;
        }
        Some(Self { path, depth })
    }

    /// The first `depth` nibbles of this path as a shorter `NiblePath` — an
    /// ancestor-or-equal of `self`. Returns `None` if `depth > self.depth`;
    /// `prefix(0)` is [`EMPTY`](NiblePath::EMPTY).
    ///
    /// Single-shot O(1) alternative to repeated [`parent`](NiblePath::parent)
    /// calls. By construction `self.prefix(d).is_ancestor_of(self)` whenever
    /// `Some(_)` is returned: this is the coarse-routing-cache view of a
    /// deeper class path (the 4-nibble routing prefix vs the full 16-nibble
    /// class path, identity-architecture v1 §3).
    #[must_use]
    pub const fn prefix(self, depth: u8) -> Option<Self> {
        if depth > self.depth {
            return None;
        }
        if depth == 0 {
            return Some(Self::EMPTY);
        }
        // Right-shift the path so the desired prefix occupies the low 4·depth
        // bits. When self.depth == MAX_DEPTH and depth == MAX_DEPTH the shift
        // is zero; when depth < self.depth, drop the trailing (self.depth -
        // depth) nibbles. `shift < 64` always (depth >= 1 ⇒ shift ≤ 4·15 = 60).
        let shift = 4 * (self.depth as u32 - depth as u32);
        Some(Self {
            path: self.path >> shift,
            depth,
        })
    }

    /// The depth of the **longest common prefix** with `other` — the radix-trie
    /// nearest-neighbor measure. Larger ⇒ the two paths share more cascade tiers
    /// ⇒ they sit in the same deeper CLAM cluster ⇒ they are nearer.
    ///
    /// This is the operational form of `panCAKES ≡ radix trie ≡ HHTL`
    /// (`E-PANCAKES-IS-RADIX-IS-HHTL`): CAKES nearest-neighbor over the cluster
    /// tree is *longest-common-prefix ranking* over the HHTL nibble paths — no
    /// separate tree to build, the keys ARE the tree. Pure prefix arithmetic on
    /// the key; never touches the value slab.
    #[must_use]
    pub const fn common_prefix_depth(self, other: Self) -> u8 {
        let max = if self.depth < other.depth {
            self.depth
        } else {
            other.depth
        };
        let mut d = 0u8;
        // Walk depth-by-depth while the aligned prefixes agree. `prefix(d)` is
        // `Some` for every d ≤ depth, so the unwraps below cannot fail.
        while d < max {
            let next = d + 1;
            match (self.prefix(next), other.prefix(next)) {
                (Some(a), Some(b)) if a.path == b.path && a.depth == b.depth => d = next,
                _ => break,
            }
        }
        d
    }

    /// Lower a [`NodeGuid`](crate::canonical_node::NodeGuid) prefix to a 16-nibble
    /// `NiblePath`, the routing-path counterpart of the GUID's
    /// `classid · HEEL · HIP · TWIG` cascade (identity-architecture v1 §3).
    ///
    /// The 20-nibble prefix `classid(8) | HEEL(4) | HIP(4) | TWIG(4)` overflows
    /// `MAX_DEPTH = 16`. The deterministic fold drops the **HIGH 4 classid
    /// nibbles** and packs the remaining 16 nibbles root-first as
    /// `classid_lo(4) | HEEL(4) | HIP(4) | TWIG(4)`. Returns `None` when the HIGH
    /// 4 classid nibbles are nonzero — **this v1 fold** uses `classid_lo` as the
    /// coarse tier, so it needs the high `u16` clear; a nonzero high `u16` is
    /// reported, not silently re-routed. This is a **v1-fold constraint, NOT a
    /// global classid law**: the v3 fold [`from_guid_prefix_v3`] reads the
    /// `(part_of:is_a)` `HEEL·HIP·TWIG·LEAF` tiers and does NOT fold `classid`, so
    /// a V3 classid carries its high-`u16` generation marker freely (the schema's
    /// `tail_variant` selects the fold — there is no global reserved-zero after V3).
    ///
    /// **Bijection invariant.** For any GUID whose `classid >> 16 == 0`,
    /// `from_guid_prefix(guid).prefix(d).is_ancestor_of(from_guid_prefix(guid))`
    /// holds for every `d in 1..=16` (`prefix(0)` is [`EMPTY`](NiblePath::EMPTY),
    /// which by definition is an ancestor of nothing — the "no basin routed"
    /// sentinel). The routing-cache view (typically `prefix(4)` over
    /// `classid_lo`) is therefore a valid HHTL ancestor of the full class path —
    /// the LE contract the `classid → ReadMode` keystone meets at the classid.
    #[must_use]
    pub const fn from_guid_prefix(guid: &crate::canonical_node::NodeGuid) -> Option<Self> {
        let parts = guid.decode();
        // In THIS v1 fold the high 4 classid nibbles must be zero — it folds
        // classid_lo as the coarse tier, so a nonzero high u16 would make the
        // 20→16 nibble fold lossy. It is reported, not silently re-routed. (The
        // v3 fold does NOT fold classid — see from_guid_prefix_v3 — so this is a
        // v1-fold constraint, not a global reserved-zero law.)
        if (parts.classid >> 16) != 0 {
            return None;
        }
        // Pack root-first into 16 nibbles = 64 bits = the full u64 path:
        //   nibbles 0..4  (high) = classid_lo  (basin = top nibble of classid_lo)
        //   nibbles 4..8         = HEEL
        //   nibbles 8..12        = HIP
        //   nibbles 12..16 (low) = TWIG        (leaf = low nibble of TWIG)
        let classid_lo = (parts.classid & 0xFFFF) as u64;
        let path = (classid_lo << 48)
            | ((parts.heel as u64) << 32)
            | ((parts.hip as u64) << 16)
            | (parts.twig as u64);
        // from_packed handles the high-bit guard; at MAX_DEPTH every u64 is
        // valid by construction (4·16 = 64 used bits).
        Self::from_packed(path, MAX_DEPTH)
    }

    /// v2 GUID→path lowering (D-GV2-1, feature `guid-v2-tail`): the HHTL path is
    /// `HEEL·HIP·TWIG·leaf` — 4 tiers × 4 nibbles = 16 nibbles = a full `u64`
    /// NiblePath. `leaf` (the v2 4th tier) IS part of the routing path; `classid`
    /// is the separate codebook prefix (not folded in), and `family`/`identity`
    /// are the basin tail (NOT in the path). Two GUIDs differing only in
    /// family/identity therefore share a path; differing in any HHT tier (incl.
    /// `leaf`) do not — the property v2 hop-distance relies on.
    #[cfg(feature = "guid-v2-tail")]
    #[must_use]
    pub const fn from_guid_prefix_v2(guid: &crate::canonical_node::NodeGuid) -> Self {
        let path = ((guid.heel() as u64) << 48)
            | ((guid.hip() as u64) << 32)
            | ((guid.twig() as u64) << 16)
            | (guid.leaf() as u64);
        // 16 nibbles = full depth; from_packed is always Some at MAX_DEPTH.
        match Self::from_packed(path, MAX_DEPTH) {
            Some(p) => p,
            None => Self::EMPTY,
        }
    }

    /// v3 GUID→path lowering (feature `guid-v3-tail`): each HHTL tier is an 8:8
    /// `(part_of : is_a)` = `(place : tissue)` tile, and **BOTH bytes are routed**
    /// — lossless: the `part_of` high byte (WHERE — `galaxy`, `city`, `class`)
    /// AND the `is_a` low byte (WHAT — `universe`, `school`, `student`) co-refine
    /// at every level. (Folding only the high byte, as an earlier draft did, drops
    /// the whole `is_a` hierarchy.)
    ///
    /// The full v3 address is the **6-tier** `(part_of:is_a)` FacetCascade
    /// (`facet_classid(4) | 6×(8:8)=12`, harvest §5.1 — HEEL·HIP·TWIG·LEAF·family·
    /// identity). This `NiblePath` carries its **routing prefix**: the 4 HHTL
    /// tiers `HEEL·HIP·TWIG·LEAF` in FULL (4 × 16 bits = 64 = [`MAX_DEPTH`]). The
    /// 5th/6th tiers (`family`/`identity`) are the basin tail
    /// ([`local_key`](crate::canonical_node::NodeGuid::local_key)) — preserved,
    /// not dropped, exactly as v1/v2 keep their tail out of the `u64` path (which
    /// holds only 8 bytes; the full 12-byte cascade does not fit one `NiblePath`).
    ///
    /// **`classid` is NOT folded in** (unlike v1's `classid_lo·HEEL·HIP·TWIG`), so
    /// a V3 classid's high-`u16` generation marker (e.g. OSINT-V3 `0x1000_0700`)
    /// is irrelevant to routing and never collapses to [`EMPTY`](NiblePath::EMPTY).
    /// This is why "high `u16` is reserved-zero" is a **v1-fold** statement, NOT a
    /// global classid law — the schema's `tail_variant` selects the fold.
    #[cfg(feature = "guid-v3-tail")]
    #[must_use]
    pub const fn from_guid_prefix_v3(guid: &crate::canonical_node::NodeGuid) -> Self {
        // Read the 4 HHTL tiers as full LE `u16` from the raw key bytes [4..12] —
        // BOTH the part_of (high) and is_a (low) byte of each 8:8 tile. Reading
        // raw bytes avoids the `guid-v2-tail` gate on `leaf()` and is robust to the
        // v1/v2 tail interpretation (the tier offsets are fixed by the canon).
        let b = guid.as_bytes();
        let heel = (b[4] as u64) | ((b[5] as u64) << 8);
        let hip = (b[6] as u64) | ((b[7] as u64) << 8);
        let twig = (b[8] as u64) | ((b[9] as u64) << 8);
        let leaf = (b[10] as u64) | ((b[11] as u64) << 8);
        let path = (heel << 48) | (hip << 32) | (twig << 16) | leaf;
        // 16 nibbles = full depth; from_packed is always Some at MAX_DEPTH.
        match Self::from_packed(path, MAX_DEPTH) {
            Some(p) => p,
            None => Self::EMPTY,
        }
    }

    /// **Family hop count** — the CLAM tree distance to `other`: the number of
    /// edges between the two nodes through their lowest common ancestor in the
    /// 16ⁿ tree. `(self.depth − common) + (other.depth − common)` where `common =
    /// `[`common_prefix_depth`](NiblePath::common_prefix_depth). Identical path =
    /// 0, parent/child = 1, siblings = 2; disjoint subtrees = the full ascent +
    /// descent. This is the operator's "HHTL CLAM via family-nodes hop count as
    /// adjacency" metric — pure key arithmetic, O(depth), **zero value decode**.
    ///
    /// Symmetric: `a.family_hop_count(b) == b.family_hop_count(a)`.
    #[must_use]
    pub const fn family_hop_count(self, other: Self) -> u8 {
        let common = self.common_prefix_depth(other);
        (self.depth - common) + (other.depth - common)
    }

    /// Is this path a descendant-or-equal of `other`? — the symmetric form of
    /// [`is_ancestor_of`]. `self.is_descendant_of(other)` is equivalent to
    /// `other.is_ancestor_of(self)` BUT the form is sometimes more natural at
    /// the call site (e.g. iterating over candidate ancestors).
    ///
    /// Like [`is_ancestor_of`], the empty path is never a descendant of
    /// anything.
    #[must_use]
    pub const fn is_descendant_of(self, other: Self) -> bool {
        other.is_ancestor_of(self)
    }

    /// Are `self` and `other` siblings — distinct paths that share the SAME
    /// parent (and thus the same depth)? Returns `false` if either is the
    /// basin (depth 1 — basins have no parent in this tree), if the depths
    /// differ, or if the paths are equal.
    ///
    /// Together with [`is_ancestor_of`] / [`is_descendant_of`] this exposes
    /// the three structural relations the Pearl-junction classifier
    /// (`crate::pearl_junction`) needs without forcing the caller to do its
    /// own bit-shift arithmetic.
    #[must_use]
    pub const fn is_sibling_of(self, other: Self) -> bool {
        if self.depth != other.depth || self.depth <= 1 || self.path == other.path {
            return false;
        }
        // Same depth + same parent ⇔ matching top (depth−1) nibbles ⇔
        // matching all bits except the low 4 (the leaf nibble).
        const LEAF_MASK: u64 = !0x0F_u64;
        (self.path & LEAF_MASK) == (other.path & LEAF_MASK)
    }

    /// The longest common ancestor path — the longest prefix shared by
    /// `self` and `other`. `None` if the two paths share no basin (they
    /// live in disjoint DOLCE-facet subtrees, OR either is the empty path).
    ///
    /// Symmetric in its arguments: `a.common_ancestor(b) == b.common_ancestor(a)`.
    ///
    /// O(depth) — at most `MAX_DEPTH` nibble-shifts in the worst case.
    #[must_use]
    pub const fn common_ancestor(self, other: Self) -> Option<Self> {
        if self.depth == 0 || other.depth == 0 {
            return None;
        }
        // Align both paths to the shallower depth, then walk up until the
        // packed prefixes agree. Once we reach depth 0 without a match,
        // the two paths share no basin.
        let mut a_path = self.path;
        let mut a_depth = self.depth;
        let mut b_path = other.path;
        let mut b_depth = other.depth;
        while a_depth > b_depth {
            a_path >>= 4;
            a_depth -= 1;
        }
        while b_depth > a_depth {
            b_path >>= 4;
            b_depth -= 1;
        }
        // Same depth now. Walk up until the bits match.
        while a_path != b_path {
            if a_depth <= 1 {
                // Reaching depth 0 means the paths share no basin; reaching
                // depth 1 with no match means the basins themselves differ.
                if a_depth == 1 {
                    return None;
                }
                a_path >>= 4;
                b_path >>= 4;
                a_depth -= 1;
                continue;
            }
            a_path >>= 4;
            b_path >>= 4;
            a_depth -= 1;
        }
        if a_depth == 0 {
            None
        } else {
            Some(Self {
                path: a_path,
                depth: a_depth,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_view::FieldMask;

    #[test]
    fn root_child_basin_leaf_roundtrip_is_bitshift_exact() {
        // basin 0x2 (say DOLCE Quality) → child 0x5 → child 0xA.
        let p = NiblePath::root(0x2).child(0x5).child(0xA);
        assert_eq!(p.depth(), 3);
        assert_eq!(
            p.basin(),
            Some(0x2),
            "basin = root nibble, stable down the path"
        );
        assert_eq!(p.leaf(), Some(0xA), "leaf = deepest nibble");
        // parent walks back up exactly (bit-shift inverse of child).
        let pp = p.parent().unwrap();
        assert_eq!(pp, NiblePath::root(0x2).child(0x5));
        assert_eq!(pp.leaf(), Some(0x5));
        assert_eq!(p.parent().unwrap().parent().unwrap(), NiblePath::root(0x2));
        assert_eq!(NiblePath::root(0x2).parent(), None, "basin has no parent");
        assert_eq!(NiblePath::EMPTY.basin(), None);
    }

    #[test]
    fn from_packed_validates_depth_high_bits_and_roundtrips() {
        // (0, 0) is the EMPTY sentinel.
        assert_eq!(NiblePath::from_packed(0, 0), Some(NiblePath::EMPTY));

        // A well-formed (path, depth) reconstructs exactly what `child` builds.
        assert_eq!(
            NiblePath::from_packed(0x12, 2),
            Some(NiblePath::root(0x1).child(0x2)),
        );

        // depth > MAX_DEPTH is rejected.
        assert_eq!(NiblePath::from_packed(0, MAX_DEPTH + 1), None);

        // Bits set above the 4·depth route nibbles are an inconsistent pack.
        // depth = 2 ⇒ only the low 8 bits may be set; 0x112 has a 9th.
        assert_eq!(NiblePath::from_packed(0x112, 2), None);

        // Boundary: at MAX_DEPTH the whole u64 is usable (the `used_bits < 64`
        // guard skips a `>> 64` UB), so even all-ones round-trips.
        let max = NiblePath::from_packed(u64::MAX, MAX_DEPTH);
        assert_eq!(max.map(NiblePath::packed), Some((u64::MAX, MAX_DEPTH)));

        // packed ∘ from_packed is identity on every valid path.
        for p in [
            NiblePath::EMPTY,
            NiblePath::root(0x3),
            NiblePath::root(0x3).child(0x5).child(0xA),
        ] {
            let (path, depth) = p.packed();
            assert_eq!(NiblePath::from_packed(path, depth), Some(p));
        }
    }

    #[test]
    fn depth_caps_at_max_and_rejects_out_of_range_nibble() {
        // Fill to MAX_DEPTH, then one more child is a no-op (not a wrap/overflow).
        let mut p = NiblePath::root(0x1);
        while p.depth() < MAX_DEPTH {
            p = p.child(0xF);
        }
        assert_eq!(p.depth(), MAX_DEPTH);
        assert_eq!(
            p.child(0x3),
            p,
            "child past MAX_DEPTH is a no-op, never wraps"
        );
        // Out-of-range nibble (>= FAN_OUT) is ignored, NOT folded onto a valid child.
        assert_eq!(NiblePath::root(0x1).child(16), NiblePath::root(0x1));
        assert_eq!(NiblePath::root(0x1).child(99), NiblePath::root(0x1));
        // root() rejects an out-of-range basin to EMPTY — never folds 16 → basin 0
        // (which would misroute ancestry; CodeRabbit #442).
        assert_eq!(NiblePath::root(16), NiblePath::EMPTY);
        assert_eq!(NiblePath::root(99), NiblePath::EMPTY);
        assert_eq!(
            NiblePath::root(16).basin(),
            None,
            "bad basin must not alias to basin 0"
        );
    }

    #[test]
    fn common_prefix_depth_is_the_radix_nn_measure() {
        let a = NiblePath::root(1).child(2).child(3);
        let b = NiblePath::root(1).child(2).child(4);
        let c = NiblePath::root(1).child(2);
        let d = NiblePath::root(9);
        assert_eq!(a.common_prefix_depth(a), 3, "self ⇒ full depth");
        assert_eq!(a.common_prefix_depth(b), 2, "1·2 shared, leaf differs");
        assert_eq!(a.common_prefix_depth(c), 2, "ancestor ⇒ min depth");
        assert_eq!(a.common_prefix_depth(d), 0, "different basin ⇒ 0");
        assert_eq!(
            a.common_prefix_depth(b),
            b.common_prefix_depth(a),
            "symmetric"
        );
        assert_eq!(NiblePath::EMPTY.common_prefix_depth(a), 0);
    }

    #[test]
    fn is_ancestor_of_is_cheap_prefix_reachability() {
        let mammal = NiblePath::root(0x0).child(0x3); // Endurant → …mammal
        let bat = mammal.child(0x7);
        let dog = mammal.child(0x8);
        assert!(mammal.is_ancestor_of(bat), "mammal is an ancestor of bat");
        assert!(mammal.is_ancestor_of(dog));
        assert!(
            mammal.is_ancestor_of(mammal),
            "ancestor-or-EQUAL (reflexive)"
        );
        assert!(
            !bat.is_ancestor_of(mammal),
            "child is not an ancestor of its parent"
        );
        assert!(
            !bat.is_ancestor_of(dog),
            "siblings are not ancestors of each other"
        );
        // A different basin shares no prefix.
        let process = NiblePath::root(0x1).child(0x3);
        assert!(
            !mammal.is_ancestor_of(process),
            "different basin → not reachable"
        );
        assert!(
            !NiblePath::EMPTY.is_ancestor_of(bat),
            "empty path is an ancestor of nothing"
        );
    }

    #[test]
    fn multi_parent_is_a_facet_bit_not_a_second_path() {
        // "Bat = mammal-path + flight-bit, not two paths" (wikidata-hhtl-load.md:46).
        // ONE nibble path (the mammal Abstammung), the flight capability is an
        // orthogonal facet bit in the SAME FieldMask — never a second NiblePath.
        let bat_path = NiblePath::root(0x0).child(0x3).child(0x7); // mammal → bat
                                                                   // declared mammal fields (positions 0,1,2) + the flight facet bit (40).
        let mammal_mask = FieldMask::from_positions(&[0, 1, 2]);
        let flight_facet = FieldMask::EMPTY.with(40);
        let bat_mask = mammal_mask.inherit(flight_facet);

        assert_eq!(bat_path.depth(), 3, "bat is reached by ONE path, not two");
        assert!(
            bat_mask.has(0) && bat_mask.has(1) && bat_mask.has(2),
            "inherits mammal fields"
        );
        assert!(
            bat_mask.has(40),
            "carries the flight facet bit in the same mask"
        );
        assert_eq!(bat_mask.count(), 4);
    }

    #[test]
    fn is_full_and_try_child_signal_depth_exhaustion() {
        // child() saturates silently at MAX_DEPTH; is_full()/try_child() expose the
        // ceiling so the deferred loader switches to a ref instead of colliding two
        // distinct deep paths (D-ARM-14 review of #442).
        let mut p = NiblePath::root(0x1);
        assert!(!p.is_full());
        while !p.is_full() {
            p = p.try_child(0xF).expect("descends while not full");
        }
        assert_eq!(p.depth(), MAX_DEPTH);
        assert!(p.is_full());
        assert_eq!(
            p.try_child(0x2),
            None,
            "try_child signals exhaustion, not a silent collision"
        );
        assert_eq!(
            p.child(0x2),
            p,
            "child() still saturates (the convenience path)"
        );
        assert_eq!(
            NiblePath::root(0x1).try_child(16),
            None,
            "out-of-range nibble is None too"
        );
    }

    #[test]
    fn is_descendant_of_inverse_of_is_ancestor_of() {
        let mammal = NiblePath::root(0x1);
        let dog = NiblePath::root(0x1).child(0x1);
        let cat = NiblePath::root(0x2);
        assert!(dog.is_descendant_of(mammal));
        assert!(!mammal.is_descendant_of(dog));
        assert!(!dog.is_descendant_of(cat));
        // empty path is never a descendant of anything
        assert!(!NiblePath::EMPTY.is_descendant_of(mammal));
    }

    #[test]
    fn is_sibling_of_requires_same_parent_distinct_paths() {
        let dog = NiblePath::root(0x1).child(0x1);
        let cat = NiblePath::root(0x1).child(0x2);
        let lance = NiblePath::root(0x1).child(0x1);
        // siblings: same parent (mammal), distinct leaf nibbles
        assert!(dog.is_sibling_of(cat));
        assert!(cat.is_sibling_of(dog));
        // not siblings: equal paths
        assert!(!dog.is_sibling_of(lance));
        // not siblings: different depth
        let mammal = NiblePath::root(0x1);
        assert!(!dog.is_sibling_of(mammal));
        // not siblings: different parent
        let plant = NiblePath::root(0x2).child(0x1);
        assert!(!dog.is_sibling_of(plant));
        // basins themselves are not siblings (depth 1, no parent)
        let b1 = NiblePath::root(0x1);
        let b2 = NiblePath::root(0x2);
        assert!(!b1.is_sibling_of(b2));
    }

    #[test]
    fn common_ancestor_returns_longest_shared_prefix() {
        // (1)(2)(3)(4) and (1)(2)(5)(6) share (1)(2)
        let a = NiblePath::root(0x1).child(0x2).child(0x3).child(0x4);
        let b = NiblePath::root(0x1).child(0x2).child(0x5).child(0x6);
        let lca = a.common_ancestor(b).unwrap();
        assert_eq!(lca.depth(), 2);
        assert_eq!(lca.basin(), Some(0x1));
        assert_eq!(lca.leaf(), Some(0x2));
        // symmetric
        assert_eq!(b.common_ancestor(a), Some(lca));
    }

    #[test]
    fn common_ancestor_handles_different_depths() {
        // (1)(2) is an ancestor of (1)(2)(3); LCA should be (1)(2)
        let shallow = NiblePath::root(0x1).child(0x2);
        let deep = NiblePath::root(0x1).child(0x2).child(0x3);
        assert_eq!(shallow.common_ancestor(deep), Some(shallow));
        assert_eq!(deep.common_ancestor(shallow), Some(shallow));
    }

    #[test]
    fn common_ancestor_disjoint_basins_returns_none() {
        // different basins → no common ancestor in this tree
        let a = NiblePath::root(0x1).child(0x2);
        let b = NiblePath::root(0x3).child(0x4);
        assert_eq!(a.common_ancestor(b), None);
        assert_eq!(b.common_ancestor(a), None);
    }

    #[test]
    fn common_ancestor_empty_path_returns_none() {
        let a = NiblePath::root(0x1);
        assert_eq!(a.common_ancestor(NiblePath::EMPTY), None);
        assert_eq!(NiblePath::EMPTY.common_ancestor(a), None);
    }

    #[cfg(feature = "guid-v2-tail")]
    #[test]
    fn from_guid_prefix_v2_includes_leaf_not_basin_tail() {
        use crate::canonical_node::NodeGuid;
        let g = NodeGuid::new_v2(0xDEAD_BEEF, 0x1234, 0x5678, 0x9ABC, 0xDEF0, 0, 0);
        assert_eq!(NiblePath::from_guid_prefix_v2(&g).depth(), 16);
        // family/identity (basin tail) do NOT affect the path
        let same = NodeGuid::new_v2(0xDEAD_BEEF, 0x1234, 0x5678, 0x9ABC, 0xDEF0, 0xFFFF, 0xFFFF);
        assert_eq!(
            NiblePath::from_guid_prefix_v2(&g),
            NiblePath::from_guid_prefix_v2(&same)
        );
        // leaf IS in the path → changing it changes the path
        let diff_leaf = NodeGuid::new_v2(0xDEAD_BEEF, 0x1234, 0x5678, 0x9ABC, 0x0EF0, 0, 0);
        assert_ne!(
            NiblePath::from_guid_prefix_v2(&g),
            NiblePath::from_guid_prefix_v2(&diff_leaf)
        );
    }

    #[test]
    fn family_hop_count_is_clam_tree_distance() {
        let a = NiblePath::root(0x1).child(0x2).child(0x3).child(0x4);
        // identical path = 0 hops
        assert_eq!(a.family_hop_count(a), 0);
        // siblings (share parent (1)(2)(3), differ in leaf) = 2 hops
        let sib = NiblePath::root(0x1).child(0x2).child(0x3).child(0x9);
        assert_eq!(a.family_hop_count(sib), 2);
        assert_eq!(sib.family_hop_count(a), 2); // symmetric
                                                // parent = 1 hop
        let parent = NiblePath::root(0x1).child(0x2).child(0x3);
        assert_eq!(a.family_hop_count(parent), 1);
        // cousins: share (1)(2), differ from depth 3 down → (4-2)+(4-2) = 4
        let cousin = NiblePath::root(0x1).child(0x2).child(0x7).child(0x8);
        assert_eq!(a.family_hop_count(cousin), 4);
        // disjoint basins: no common prefix → full ascent + descent
        let other = NiblePath::root(0xF).child(0xE);
        assert_eq!(a.family_hop_count(other), 4 + 2);
    }

    // ── NiblePath::prefix — single-shot ancestor view ─────────────────────────

    #[test]
    fn prefix_returns_ancestor_or_equal_at_requested_depth() {
        // depth 0 ⇒ EMPTY (the "no basin routed" sentinel — symmetrical with
        // parent() of a basin returning None).
        let p = NiblePath::root(0x2).child(0x5).child(0xA).child(0x3);
        assert_eq!(p.prefix(0), Some(NiblePath::EMPTY));
        assert_eq!(p.prefix(1), Some(NiblePath::root(0x2)));
        assert_eq!(p.prefix(2), Some(NiblePath::root(0x2).child(0x5)));
        assert_eq!(
            p.prefix(3),
            Some(NiblePath::root(0x2).child(0x5).child(0xA))
        );
        assert_eq!(p.prefix(4), Some(p), "prefix(self.depth) is reflexive");
        assert_eq!(p.prefix(5), None, "prefix beyond own depth is rejected");
    }

    #[test]
    fn prefix_is_always_an_ancestor_of_self() {
        // The structural invariant the routing-cache view relies on: every
        // returned prefix passes is_ancestor_of(self). Walk the whole depth.
        let p = NiblePath::root(0x1)
            .child(0x2)
            .child(0x3)
            .child(0x4)
            .child(0x5);
        for d in 1..=p.depth() {
            let pre = p.prefix(d).unwrap();
            assert!(
                pre.is_ancestor_of(p),
                "prefix(d={d})={pre:?} must be an ancestor of self={p:?}"
            );
            assert_eq!(pre.depth(), d);
        }
    }

    #[test]
    fn prefix_matches_repeated_parent_chain() {
        // O(1) prefix(d) must agree with O(depth-d) parent()-loop.
        let p = NiblePath::root(0x7)
            .child(0x3)
            .child(0xA)
            .child(0x1)
            .child(0xC);
        let mut walked = p;
        let mut d = p.depth();
        while d > 0 {
            assert_eq!(p.prefix(d), Some(walked), "depth {d}");
            d -= 1;
            walked = walked.parent().unwrap_or(NiblePath::EMPTY);
        }
        assert_eq!(p.prefix(0), Some(NiblePath::EMPTY));
    }

    // ── NiblePath::from_guid_prefix — 20→16 nibble fold (identity-arch v1 §3) ──

    #[test]
    fn from_guid_prefix_returns_full_max_depth_path() {
        use crate::canonical_node::NodeGuid;
        // A canonical GUID with classid in the low u16 round-trips to a
        // 16-nibble path with the documented root-first layout.
        let g = NodeGuid::new(0x0000_ABCD, 0x1234, 0x5678, 0x9ABC, 0x00_0001, 0x00_0002);
        let path = NiblePath::from_guid_prefix(&g).expect("classid_lo only ⇒ Some");
        assert_eq!(path.depth(), MAX_DEPTH, "fold occupies the full u64");

        // Root-first: top nibble of classid_lo is the basin (0xA from 0xABCD).
        assert_eq!(path.basin(), Some(0xA));
        // Leaf: low nibble of TWIG (0xC from 0x9ABC).
        assert_eq!(path.leaf(), Some(0xC));

        // Packed value mirrors classid_lo|HEEL|HIP|TWIG, root-first.
        let expected: u64 = (0xABCDu64 << 48) | (0x1234u64 << 32) | (0x5678u64 << 16) | 0x9ABCu64;
        assert_eq!(path.packed(), (expected, MAX_DEPTH));
    }

    #[test]
    fn from_guid_prefix_returns_none_when_high_classid_nibbles_in_use() {
        use crate::canonical_node::NodeGuid;
        // The 20→16 fold drops the HIGH 4 classid nibbles. When the high u16
        // is nonzero, the fold is lossy — None signals it, callers don't get
        // a silent collision.
        let g = NodeGuid::new(0xDEAD_BEEF, 0, 0, 0, 0, 0);
        assert_eq!(
            NiblePath::from_guid_prefix(&g),
            None,
            "high classid u16 != 0 ⇒ refuse the lossy fold"
        );
        let g = NodeGuid::new(0x0001_0000, 0, 0, 0, 0, 0);
        assert_eq!(
            NiblePath::from_guid_prefix(&g),
            None,
            "boundary: bit 16 set"
        );
        // At exactly the boundary (high u16 == 0) the fold is lossless.
        let g = NodeGuid::new(0x0000_FFFF, 0, 0, 0, 0, 0);
        assert!(NiblePath::from_guid_prefix(&g).is_some());
    }

    #[cfg(feature = "guid-v3-tail")]
    #[test]
    fn from_guid_prefix_v3_routes_both_bytes_of_part_of_is_a_and_ignores_classid() {
        use crate::canonical_node::NodeGuid;
        // OSINT-V3: classid high u16 = 0x1000 (the generation marker), so the v1
        // fold REFUSES this GUID — the latent EMPTY-fold Codex flagged.
        let g = NodeGuid::new(
            NodeGuid::CLASSID_OSINT_V3,
            0xAB12,
            0xCD34,
            0xEF56,
            0x00_789A,
            0xBC_DEF0,
        );
        assert_eq!(
            NiblePath::from_guid_prefix(&g),
            None,
            "v1 fold refuses the high-u16 marker — the break v3 resolves"
        );

        // v3 routes the 4 HHTL tiers HEEL·HIP·TWIG·LEAF in FULL (both the part_of
        // high byte AND the is_a low byte of each 8:8 tile), depth 16, classid NOT
        // folded.
        let b = g.as_bytes();
        let heel = (b[4] as u64) | ((b[5] as u64) << 8);
        let hip = (b[6] as u64) | ((b[7] as u64) << 8);
        let twig = (b[8] as u64) | ((b[9] as u64) << 8);
        let leaf = (b[10] as u64) | ((b[11] as u64) << 8);
        let expected = (heel << 48) | (hip << 32) | (twig << 16) | leaf;
        let p = NiblePath::from_guid_prefix_v3(&g);
        assert_ne!(
            p,
            NiblePath::EMPTY,
            "the gen marker must NOT collapse routing"
        );
        assert_eq!(
            p.packed(),
            (expected, MAX_DEPTH),
            "HEEL·HIP·TWIG·LEAF in full — both bytes per tier"
        );

        // Routing is INDEPENDENT of classid: drop the generation marker → same path.
        let unmarked = NodeGuid::new(0x0000_0700, 0xAB12, 0xCD34, 0xEF56, 0x00_789A, 0xBC_DEF0);
        assert_eq!(NiblePath::from_guid_prefix_v3(&unmarked), p);

        // BOTH bytes routed: flipping an is_a LOW byte (HEEL.lo 0x12 → 0x99) MUST
        // change the path — proving the is_a hierarchy is folded, not just part_of.
        // (The earlier part_of-only fold would wrongly keep this identical.)
        let diff_isa = NodeGuid::new(
            NodeGuid::CLASSID_OSINT_V3,
            0xAB99,
            0xCD34,
            0xEF56,
            0x00_789A,
            0xBC_DEF0,
        );
        assert_ne!(
            NiblePath::from_guid_prefix_v3(&diff_isa),
            p,
            "is_a low byte must move routing — both bytes, not just part_of"
        );
    }

    #[test]
    fn from_guid_prefix_bootstrap_classid_is_all_zero_path() {
        use crate::canonical_node::NodeGuid;
        // CANON bootstrap: classid 0 + HHT zero ⇒ a 16-nibble path of all-zero
        // nibbles. Basin = 0 (Endurant by ontology convention; agnostic here).
        let g = NodeGuid::local(0x00_00CD);
        let path = NiblePath::from_guid_prefix(&g).unwrap();
        assert_eq!(path.depth(), MAX_DEPTH);
        assert_eq!(path.packed(), (0u64, MAX_DEPTH));
        assert_eq!(path.basin(), Some(0));
        assert_eq!(path.leaf(), Some(0));
    }

    #[test]
    fn from_guid_prefix_bijection_classid_lo_heel_hip_twig_only() {
        use crate::canonical_node::NodeGuid;
        // Two GUIDs that differ ONLY in family/identity (the trailing 6 bytes)
        // share the same routing path — the routing prefix is class-scoped, not
        // instance-scoped. This is the bijection the operator pinned: identity
        // doesn't perturb the class path.
        let g1 = NodeGuid::new(0x0000_1234, 0xAAAA, 0xBBBB, 0xCCCC, 0x11_2233, 0x44_5566);
        let g2 = NodeGuid::new(0x0000_1234, 0xAAAA, 0xBBBB, 0xCCCC, 0x00_0001, 0x00_0002);
        assert_eq!(
            NiblePath::from_guid_prefix(&g1),
            NiblePath::from_guid_prefix(&g2),
            "same (classid_lo, HEEL, HIP, TWIG) ⇒ same routing path"
        );
        // Two GUIDs that differ in any of the four tier groups produce
        // distinct paths.
        let g3 = NodeGuid::new(0x0000_1234, 0xAAAA, 0xBBBB, 0xCCCD, 0, 0);
        let g4 = NodeGuid::new(0x0000_1234, 0xAAAA, 0xBBBC, 0xCCCC, 0, 0);
        assert_ne!(
            NiblePath::from_guid_prefix(&g1),
            NiblePath::from_guid_prefix(&g3)
        );
        assert_ne!(
            NiblePath::from_guid_prefix(&g1),
            NiblePath::from_guid_prefix(&g4)
        );
    }

    #[test]
    fn from_guid_prefix_routing_cache_is_ancestor_of_full_path() {
        use crate::canonical_node::NodeGuid;
        // The keystone invariant the `classid → ReadMode` LE contract meets:
        // a coarser routing prefix (e.g. PREFIX_NIBBLES = 4, the classid_lo
        // tier only) MUST be an HHTL ancestor of the full 16-nibble class
        // path. Without this, the consumer's read-mode resolution can't
        // dispatch by prefix.
        let g = NodeGuid::new(0x0000_ABCD, 0x1111, 0x2222, 0x3333, 0, 0);
        let full = NiblePath::from_guid_prefix(&g).unwrap();
        // Routing prefix at every depth from 1..=MAX_DEPTH is_ancestor_of full.
        for d in 1..=MAX_DEPTH {
            let routing = full.prefix(d).unwrap();
            assert!(
                routing.is_ancestor_of(full),
                "routing prefix d={d} ({routing:?}) must HHTL-reach full ({full:?})"
            );
        }
        // The 4-nibble routing prefix (identity-architecture v1 PREFIX_NIBBLES)
        // is the classid_lo: 0xABCD, basin 0xA.
        let routing4 = full.prefix(4).unwrap();
        assert_eq!(routing4.packed(), (0xABCDu64, 4));
        assert_eq!(routing4.basin(), Some(0xA));
    }
}
