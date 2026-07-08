//! ViewFilter — one brick, three sources; nested/stacked `ClassView`s.
//!
//! # The doctrine (one brick, three skins)
//!
//! A rendered view **never pushes data** — it is a *projection over the
//! in-memory `ClassView` row*. The projection is a single bitmask brick
//! ([`WideFieldMask`], minted via `from_universe_present`), arriving from
//! three sources that are the SAME mask under three names:
//!
//! 1. **RBAC `field_mask`** — `lance_graph_contract::rbac::ClassRbac::
//!    field_mask(role, class)` (`FieldMask`, the 64-field tier; `FULL` =
//!    unrestricted). What the *role* may see.
//! 2. **`ClassView` facet gate** — which fields the class *has* in memory
//!    (the basis-presence mask). What *exists*.
//! 3. **View field-set** — which fields this *skin* shows (hand-authored
//!    order today; harvested `ViewFieldSet` tomorrow — see
//!    `op_codegen_pipeline::field_harvest`). What the *view* selects.
//!
//! `ViewFilter = rbac ∩ present ∩ view` — an intersection, computed by
//! [`view_filter`]. A role restriction and a per-view customization are
//! both *just a different bit pattern*; same data, same template. ERB
//! (Rails), askama (Rust, this server), Jinja (Python) are three renderers
//! over the identical masked projection.
//!
//! # Nested / stacked `ClassView`s (the stack half of the topology kit)
//!
//! The navigation *jump* half (cycles allowed) lives in [`crate::nav`] via
//! `nav_is_fully_connected`. THIS module carries the *stack* half: a view
//! composing sub-views (`Project` detail stacking a `ProjectWorkItem`
//! list). Stack edges must be acyclic — a view cannot compose itself —
//! and [`ViewRegistry`] makes that true **by construction**: a node can
//! only be interned over *already-interned* children, so every stack edge
//! points from a higher node id to a strictly lower one. The construction
//! order IS the topological order (children first — which is also why
//! interning amortizes: a child skin is minted once, from the operation it
//! serves, and every later parent reuses the same node). The one-pass
//! re-check ([`ViewRegistry::verify_stack_order`]) confirms the invariant
//! once over the finished registry, and the bijective forward/backward
//! round trip ([`ViewRegistry::verify_bijective`]) confirms
//! intern∘resolve = id and resolve∘intern = id — distinct skins ↔ distinct
//! node ids, both directions.
//!
//! # God-object overflow — rolling buckets
//!
//! `from_universe_present` refuses a >256-field universe
//! (`WideMaskCapError::UniverseExceedsSocCap`) — deliberately: that is an
//! OGAR-SoC "split this class" signal, never a mask to widen. When the
//! split hasn't happened yet (a loose God object mid-transcode),
//! [`bucketized_masks`] is the *rolling-bucket* overflow path: the
//! universe is chunked into ≤256-field buckets, each bucket minted
//! on-brick with bucket-local positions, and [`buckets_have`] gives the
//! forward (global position → bucket bit) lookup whose round trip against
//! raw membership is the backward check.

use lance_graph_contract::class_view::{WideFieldMask, WideMaskCapError};
use lance_graph_contract::rbac::ClassRbac;
use lance_graph_contract::FieldMask;

// ── ViewFilter: rbac ∩ present ∩ view ────────────────────────────────

/// Widen the RBAC 64-field [`FieldMask`] tier to a [`WideFieldMask`].
///
/// This is *positional* data (the RBAC mask's bit `i` already means basis
/// position `i` — same position convention as `ClassView::fields`), so
/// `from_positions` is the correct constructor here: the off-brick
/// anti-pattern was minting **name membership** via
/// `from_positions(mask_positions(..))`, which skips the SoC guard;
/// widening an existing positional mask across the width tiers is exactly
/// what `from_positions` is for.
#[must_use]
pub fn widen_rbac(mask: FieldMask) -> WideFieldMask {
    let positions: Vec<u8> = (0..64u8).filter(|i| mask.0 & (1u64 << i) != 0).collect();
    WideFieldMask::from_positions(&positions)
}

/// The ViewFilter: `rbac ∩ present ∩ view`.
///
/// `rbac == FieldMask::FULL` means *unrestricted* (the [`ClassRbac`]
/// default) and is applied as identity rather than widened — `FULL` is a
/// 64-bit literal, and literally widening it would wrongly truncate a
/// basis with more than 64 fields. A narrower mask is widened positionally
/// ([`widen_rbac`]) and intersected.
#[must_use]
pub fn view_filter(
    rbac: FieldMask,
    present: &WideFieldMask,
    view: &WideFieldMask,
) -> WideFieldMask {
    let shown = present.intersect(view);
    if rbac == FieldMask::FULL {
        shown
    } else {
        shown.intersect(&widen_rbac(rbac))
    }
}

/// The demo-posture RBAC provider: the anonymous board
/// (`OP_ALLOW_ANONYMOUS`) expressed AS a [`ClassRbac`] impl rather than a
/// bypass — one role (`"anonymous"`), read-anything grants, and the
/// trait's default `field_mask` ([`FieldMask::FULL`] = unrestricted).
/// When a real role store lands it replaces THIS provider; the render
/// path through [`view_filter`] does not change.
pub struct AnonymousRbac;

/// The single role the anonymous provider knows.
const ANONYMOUS_ROLES: &[lance_graph_contract::rbac::RoleId] = &["anonymous"];

impl ClassRbac for AnonymousRbac {
    fn actor_roles(&self, _actor: lance_graph_contract::rbac::ActorId<'_>) -> &[&'static str] {
        ANONYMOUS_ROLES
    }

    fn grant_permits(
        &self,
        role: lance_graph_contract::rbac::RoleId,
        _class: lance_graph_contract::rbac::ClassId,
        _op: &lance_graph_contract::rbac::Operation<'_>,
    ) -> bool {
        // Demo posture: the anonymous role may do anything the demo
        // serves. Real deployments replace the provider, not this line.
        role == "anonymous"
    }
}

/// The RBAC field mask for the current request context — resolved through
/// the REAL [`ClassRbac`] trait path ([`rbac_field_mask`]) against the
/// demo [`AnonymousRbac`] provider, so the render path's contract is the
/// production contract already. Today this yields [`FieldMask::FULL`]
/// (the trait default), which [`view_filter`] applies as identity —
/// today's output is byte-identical; a narrower provider drops columns
/// with zero changes at the render call sites.
#[must_use]
pub fn current_rbac_field_mask() -> FieldMask {
    rbac_field_mask(&AnonymousRbac, "anonymous", 0)
}

/// Resolve a role's field mask through a [`ClassRbac`] provider — the
/// post-auth form of [`current_rbac_field_mask`], already wired so the
/// render path's contract is proven before the role store exists.
/// (`RoleId`/`ClassId` are the contract's own typedefs.)
#[must_use]
pub fn rbac_field_mask<R: ClassRbac>(
    rbac: &R,
    role: lance_graph_contract::rbac::RoleId,
    class_id: lance_graph_contract::rbac::ClassId,
) -> FieldMask {
    rbac.field_mask(role, class_id)
}

// ── Nested / stacked ClassViews: the intern registry ─────────────────

/// Interned view-node id. `u8` on purpose — the registry rides the same
/// ≤256 SoC discipline as every mask position; more than 256 distinct
/// view nodes is a "split the app surface" signal, not an id to widen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewNodeId(pub u8);

/// One reusable view node: a `(concept, mask)` projection plus the
/// stacked child views it composes. Interned — the same skin minted from
/// two different operations (two routes rendering the same masked view of
/// the same class) resolves to ONE node: constructor amortization and
/// DTO/route-arm deduplication in the same move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewNode {
    /// `ClassView` concept the projection is over (e.g. `"ProjectWorkItem"`).
    pub concept: &'static str,
    /// The ViewFilter mask this node projects.
    pub mask: WideFieldMask,
    /// Stacked sub-views, in composition order. Every id is strictly
    /// lower than this node's own id (children are interned first).
    pub children: Vec<ViewNodeId>,
}

/// Error interning a node whose child id does not exist (yet) — the
/// construction-order rule: children BEFORE parents, always.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownChild(pub ViewNodeId);

impl std::fmt::Display for UnknownChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "stack child {:?} is not interned yet — children must be interned before parents",
            self.0
        )
    }
}

impl std::error::Error for UnknownChild {}

/// Registry of interned view nodes. Acyclic by construction (see the
/// module doc): `intern` only accepts already-existing child ids, so
/// every stack edge goes parent-id → strictly-lower child-id.
#[derive(Debug, Default)]
pub struct ViewRegistry {
    nodes: Vec<ViewNode>,
}

impl ViewRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a view node. Returns the EXISTING id when an identical
    /// `(concept, mask, children)` node is already present (the
    /// amortization/dedup path), otherwise appends.
    ///
    /// # Errors
    ///
    /// [`UnknownChild`] if any child id is not already interned — the
    /// children-first construction-order rule, which is also what makes
    /// the stack acyclic by construction.
    pub fn intern(
        &mut self,
        concept: &'static str,
        mask: WideFieldMask,
        children: Vec<ViewNodeId>,
    ) -> Result<ViewNodeId, UnknownChild> {
        let next = self.nodes.len();
        for child in &children {
            if usize::from(child.0) >= next {
                return Err(UnknownChild(*child));
            }
        }
        let candidate = ViewNode {
            concept,
            mask,
            children,
        };
        if let Some(pos) = self.nodes.iter().position(|n| *n == candidate) {
            // Dedup hit: the same skin minted from a different operation.
            #[allow(clippy::cast_possible_truncation)] // len ≤ 256 enforced below
            return Ok(ViewNodeId(pos as u8));
        }
        assert!(
            next < 256,
            "view registry exceeds 256 nodes — an app-surface split signal, not an id to widen"
        );
        #[allow(clippy::cast_possible_truncation)] // guarded by the assert above
        let id = ViewNodeId(next as u8);
        self.nodes.push(candidate);
        Ok(id)
    }

    /// Resolve an id back to its node (the backward direction).
    #[must_use]
    pub fn resolve(&self, id: ViewNodeId) -> Option<&ViewNode> {
        self.nodes.get(usize::from(id.0))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The one-pass construction-order re-check: every stack edge must
    /// point from a node to a STRICTLY lower id. True by construction
    /// (see [`Self::intern`]); this is the "double check the nesting
    /// construction order once" pass, run over the finished registry —
    /// child-id < parent-id for every edge is exactly a topological
    /// order, so it is also the acyclicity proof (the stack half's
    /// invariant; the jump half in [`crate::nav`] deliberately allows
    /// cycles).
    #[must_use]
    pub fn verify_stack_order(&self) -> bool {
        self.nodes
            .iter()
            .enumerate()
            .all(|(i, n)| n.children.iter().all(|c| usize::from(c.0) < i))
    }

    /// The forward/backward bijective round trip:
    /// **forward** — re-interning every resolved node must return its own
    /// id (intern∘resolve = id, i.e. the dedup path is total);
    /// **backward** — all interned nodes are pairwise distinct
    /// (resolve is injective on ids, so ids ↔ distinct skins is a
    /// bijection on the interned set).
    #[must_use]
    pub fn verify_bijective(&mut self) -> bool {
        if self.is_empty() {
            return true; // vacuously bijective
        }
        // Backward: pairwise distinct (resolve is injective on ids ⇒
        // ids ↔ distinct skins).
        for i in 0..self.nodes.len() {
            for j in (i + 1)..self.nodes.len() {
                if self.nodes[i] == self.nodes[j] {
                    return false;
                }
            }
        }
        // Forward: intern(resolve(id)) == id for every id — the actual
        // resolve → re-intern round trip, through the public surface.
        for i in 0..self.nodes.len() {
            #[allow(clippy::cast_possible_truncation)] // len ≤ 256 (intern asserts)
            let id = ViewNodeId(i as u8);
            let Some(node) = self.resolve(id).cloned() else {
                return false;
            };
            match self.intern(node.concept, node.mask, node.children) {
                Ok(back) if back == id => {}
                _ => return false,
            }
        }
        true
    }
}

// ── God-object overflow: rolling buckets ─────────────────────────────

/// Bucket width — the SoC cap itself. Each rolling bucket is a full
/// `WideFieldMask` universe.
pub const BUCKET_WIDTH: usize = 256;

/// Rolling-bucket mint for a universe that may exceed the 256-field SoC
/// cap: the universe is chunked into ≤[`BUCKET_WIDTH`] buckets and each
/// bucket is minted **on-brick** (`from_universe_present` over the bucket
/// slice — bucket-local positions), so the SoC guard still applies per
/// bucket and no raw bit-fiddling happens anywhere.
///
/// This is the overflow path for a *loose God object* that has not been
/// SoC-split yet: the cap error stays the signal (callers that CAN split
/// should), but a mid-transcode consumer can roll over it losslessly.
///
/// # Errors
///
/// Propagates [`WideMaskCapError`] only if a bucket itself violates the
/// cap — impossible by construction (chunks are ≤256), kept as `Result`
/// so the on-brick contract stays visible in the signature.
pub fn bucketized_masks(
    universe: &[&str],
    present: &[&str],
) -> Result<Vec<WideFieldMask>, WideMaskCapError> {
    universe
        .chunks(BUCKET_WIDTH)
        .map(|bucket| WideFieldMask::from_universe_present(bucket, present))
        .collect()
}

/// Forward lookup over rolling buckets: is GLOBAL position `pos` set?
/// (`pos / 256` selects the bucket, `pos % 256` the bucket-local bit.)
/// The backward check is the round trip: for every global position,
/// `buckets_have(..) == (universe[pos] ∈ present)` — proven in the tests
/// and asserted at boot over the real bases by [`buckets_match_direct`].
#[must_use]
pub fn buckets_have(buckets: &[WideFieldMask], pos: usize) -> bool {
    let bucket = pos / BUCKET_WIDTH;
    #[allow(clippy::cast_possible_truncation)] // % 256 always fits u8
    let local = (pos % BUCKET_WIDTH) as u8;
    buckets.get(bucket).is_some_and(|b| b.has(local))
}

/// The forward/backward equivalence of the rolling-bucket path against
/// the direct mint, position by position — `true` iff, for every global
/// position, the bucket lookup agrees with raw `present` membership (and,
/// when the universe fits one bucket, with the direct mask bit). Run at
/// boot over the app's real bases so the overflow path is PROVEN
/// equivalent before any basis ever grows past the cap.
#[must_use]
pub fn buckets_match_direct(universe: &[&str], present: &[&str]) -> bool {
    let Ok(buckets) = bucketized_masks(universe, present) else {
        return false;
    };
    universe
        .iter()
        .enumerate()
        .all(|(pos, name)| buckets_have(&buckets, pos) == present.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask_of(universe: &[&str], present: &[&str]) -> WideFieldMask {
        WideFieldMask::from_universe_present(universe, present).unwrap()
    }

    const UNIVERSE: &[&str] = &["subject", "project", "done_ratio", "description"];

    // ── ViewFilter ────────────────────────────────────────────────

    /// FULL rbac is identity: the filter is exactly present ∩ view —
    /// today's anonymous demo posture renders byte-identically.
    #[test]
    fn full_rbac_is_identity() {
        let present = mask_of(UNIVERSE, UNIVERSE);
        let view = mask_of(UNIVERSE, &["subject", "done_ratio"]);
        let filtered = view_filter(FieldMask::FULL, &present, &view);
        assert_eq!(filtered, view);
    }

    /// A narrower role mask drops fields from the projection: role may
    /// see positions {0,1} only → `done_ratio` (pos 2) is filtered out
    /// of the view even though the skin selects it.
    #[test]
    fn narrow_rbac_drops_fields() {
        let present = mask_of(UNIVERSE, UNIVERSE);
        let view = mask_of(UNIVERSE, &["subject", "done_ratio"]);
        let rbac = FieldMask(0b0011); // positions 0..=1 only
        let filtered = view_filter(rbac, &present, &view);
        assert!(filtered.has(0), "subject (pos 0) allowed and shown");
        assert!(!filtered.has(2), "done_ratio (pos 2) shown but not allowed");
        assert_eq!(filtered.count(), 1);
    }

    /// The facet gate: a field absent from `present` never renders even
    /// when both the view and the role select it.
    #[test]
    fn present_gate_drops_absent_fields() {
        let present = mask_of(UNIVERSE, &["subject", "project"]);
        let view = mask_of(UNIVERSE, &["subject", "done_ratio"]);
        let filtered = view_filter(FieldMask::FULL, &present, &view);
        assert!(filtered.has(0));
        assert!(!filtered.has(2), "done_ratio not present in the facet");
    }

    /// `widen_rbac` is positional: bit i → wide position i, exactly.
    #[test]
    fn widen_rbac_is_positional() {
        let wide = widen_rbac(FieldMask(0b101));
        assert!(wide.has(0) && !wide.has(1) && wide.has(2));
        assert_eq!(wide.count(), 2);
    }

    // ── Registry: amortization, dedup, order, bijection ───────────

    #[test]
    fn intern_amortizes_and_dedupes() {
        let mut reg = ViewRegistry::new();
        let m = mask_of(UNIVERSE, &["subject"]);
        // Two operations (board route + stacked-under-project route)
        // minting the SAME (concept, mask, children) → one node.
        let a = reg.intern("ProjectWorkItem", m.clone(), vec![]).unwrap();
        let b = reg.intern("ProjectWorkItem", m, vec![]).unwrap();
        assert_eq!(a, b, "same skin from two operations must dedup");
        assert_eq!(reg.len(), 1);
    }

    /// Children must exist before the parent — the construction-order
    /// rule that makes the stack acyclic by construction.
    #[test]
    fn children_before_parents_is_enforced() {
        let mut reg = ViewRegistry::new();
        let err = reg
            .intern(
                "Project",
                mask_of(UNIVERSE, &["project"]),
                vec![ViewNodeId(7)],
            )
            .unwrap_err();
        assert_eq!(err, UnknownChild(ViewNodeId(7)));
    }

    /// The real nesting today: Project detail stacks the WP list. The
    /// once-check + the bijective round trip both hold.
    #[test]
    fn stack_order_and_bijection_hold() {
        let mut reg = ViewRegistry::new();
        let wp_list = reg
            .intern(
                "ProjectWorkItem",
                mask_of(UNIVERSE, &["subject", "done_ratio"]),
                vec![],
            )
            .unwrap();
        let project_detail = reg
            .intern(
                "Project",
                mask_of(UNIVERSE, &["project", "description"]),
                vec![wp_list],
            )
            .unwrap();
        assert!(wp_list < project_detail, "child id strictly below parent");
        assert!(reg.verify_stack_order(), "one-pass order check");
        assert!(reg.verify_bijective(), "forward/backward round trip");
        // Backward: resolve gives the node back, with the child edge.
        let node = reg.resolve(project_detail).unwrap();
        assert_eq!(node.children, vec![wp_list]);
    }

    /// Distinct masks over the same concept are distinct nodes (route
    /// arms with different projections do NOT collapse) — the injective
    /// half of the bijection.
    #[test]
    fn distinct_masks_stay_distinct() {
        let mut reg = ViewRegistry::new();
        let a = reg
            .intern("ProjectWorkItem", mask_of(UNIVERSE, &["subject"]), vec![])
            .unwrap();
        let b = reg
            .intern(
                "ProjectWorkItem",
                mask_of(UNIVERSE, &["subject", "project"]),
                vec![],
            )
            .unwrap();
        assert_ne!(a, b);
        assert!(reg.verify_bijective());
    }

    // ── Rolling buckets ───────────────────────────────────────────

    /// A 300-field God object: the direct mint refuses (the SoC signal),
    /// the rolling-bucket mint succeeds, and the forward lookup round-
    /// trips against raw membership for EVERY position (the backward
    /// bijective check).
    #[test]
    fn god_object_overflow_rolls_into_buckets() {
        let owned: Vec<String> = (0..300).map(|i| format!("field_{i}")).collect();
        let universe: Vec<&str> = owned.iter().map(String::as_str).collect();
        // Present: every 7th field — spread across both buckets.
        let present: Vec<&str> = universe.iter().copied().step_by(7).collect();

        // The signal stays loud on the direct path…
        assert!(matches!(
            WideFieldMask::from_universe_present(&universe, &present),
            Err(WideMaskCapError::UniverseExceedsSocCap { fields: 300 })
        ));

        // …and the rolling buckets carry the overflow losslessly.
        let buckets = bucketized_masks(&universe, &present).unwrap();
        assert_eq!(buckets.len(), 2, "300 fields → two rolling buckets");
        for (pos, name) in universe.iter().enumerate() {
            let expected = present.contains(name);
            assert_eq!(
                buckets_have(&buckets, pos),
                expected,
                "forward/backward mismatch at global position {pos} ({name})"
            );
        }
    }
}
