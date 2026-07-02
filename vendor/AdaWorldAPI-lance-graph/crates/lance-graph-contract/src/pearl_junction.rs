//! # `pearl_junction` — Pearl's three causal junctions over HHTL identity
//!
//! Classifies a pair of SPO edges into one of Pearl's three causal junctions
//! (chain / fork / collider) plus the reverse chain. The classification is
//! a pure function of identity equality between the four `NiblePath`
//! endpoints (subject and object of each edge); no graph walk is required.
//!
//! ## The mapping (E-4 corrected, per `bardioc/.claude/EPIPHANIES.md`)
//!
//! Reading `s -> o` as `s subClassOf o` (or any other transitive edge):
//!
//! | Junction       | Shared term         | Example                          | NARS rule  | ΔHHTL signature              |
//! |----------------|---------------------|----------------------------------|------------|------------------------------|
//! | **Chain**      | `o1 == s2`          | `dog -> mammal -> animal`        | Deduction  | small, along one lineage     |
//! | **ChainRev**   | `s1 == o2`          | reverse of Chain                 | Deduction  | small, along one lineage     |
//! | **Fork**       | `s1 == s2` (child)  | `dog -> mammal`, `dog -> pet`    | Induction  | 2× up to common child        |
//! | **Collider**   | `o1 == o2` (parent) | `dog -> mammal`, `cat -> mammal` | Abduction  | 2× up to common parent       |
//! | **Unrelated**  | (no shared term)    | —                                | —          | —                            |
//!
//! Anti-swap guard (per peer-review round-2 — the earlier `SharedSubject =
//! sibling-via-parent` / `SharedObject = sibling-via-child` framing inverted
//! the induction⇄abduction chirality; this module's tests use the
//! `dog/cat/mammal` example as the canonical anti-swap guard).
//!
//! ## Empty-path sentinel handling (per codex P2 + CodeRabbit review on PR #456)
//!
//! `NiblePath::EMPTY` is the crate's "no route" sentinel — used for
//! out-of-range `root()` calls and uninitialised handles. The classifier
//! treats any edge whose endpoints include `EMPTY` as **unresolved**, and
//! the classifier returns `Unrelated` rather than allowing `EMPTY == EMPTY`
//! to register as a shared term. This is necessary because matching on
//! the no-route sentinel would otherwise produce spurious Chain / Fork /
//! Collider classifications between two unresolved edges.
//!
//! ## Why this is in the contract crate
//!
//! The classifier is pure-function — it does NOT touch storage, indexes,
//! or any planner state. It IS the bridge between SPO grammar (figure
//! rules) and HHTL identity addressing. Per the Morris semiotic trichotomy
//! mapped to lance-graph code (see bardioc EPIPHANIES.md), this is
//! **syntax** (figure rules) operating over **semantics** (HHTL nodes);
//! pragmatics (the cascade fold) consumes the classification at runtime.

use crate::hhtl::NiblePath;
use crate::nars::InferenceType;

/// Pearl's causal-junction taxonomy applied to a pair of SPO edges.
///
/// The classification is determined by identity equality between the
/// four endpoints (`s1`, `o1`, `s2`, `o2`); no graph walk is required.
/// See module docstring for the canonical mapping + examples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PearlJunction {
    /// `o1 == s2` — chain: `s1 -> o1=s2 -> o2`. Head-to-tail. Deduction.
    Chain,
    /// `s1 == o2` — reverse chain: `o1 <- s1=o2 <- s2`. Head-to-tail (other
    /// direction). Deduction.
    ChainRev,
    /// `s1 == s2` — fork (common cause): the shared subject is the
    /// **child**; `o1` and `o2` are co-parents reachable via one common
    /// descendant. Conclusion `o1 -> o2` is Induction.
    Fork,
    /// `o1 == o2` — collider (explaining-away): the shared object is the
    /// **parent**; `s1` and `s2` are siblings under one common ancestor.
    /// Conclusion `s1 -> s2` is Abduction.
    Collider,
    /// No shared term between the two edges — including the case where any
    /// endpoint is `NiblePath::EMPTY` (the crate's "no route" sentinel,
    /// treated as unresolved per codex P2 + CodeRabbit review on PR #456).
    Unrelated,
}

impl PearlJunction {
    /// Stable label for reports / logs / diff dimensions.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Chain => "chain",
            Self::ChainRev => "chain_rev",
            Self::Fork => "fork",
            Self::Collider => "collider",
            Self::Unrelated => "unrelated",
        }
    }

    /// The canonical NARS [`InferenceType`] the junction selects. `None` for
    /// `Unrelated`. (Chain / ChainRev → Deduction; Fork → Induction;
    /// Collider → Abduction.) The full NARS taxonomy includes Revision and
    /// Synthesis which are NOT junction-derivable and are surfaced through
    /// other dispatch paths.
    ///
    /// Sources the canonical [`crate::nars::InferenceType`] enum rather than
    /// introducing a parallel taxonomy (per CodeRabbit review on PR #456
    /// — avoid the duplication-map drift class).
    pub const fn inference_type(self) -> Option<InferenceType> {
        match self {
            Self::Chain | Self::ChainRev => Some(InferenceType::Deduction),
            Self::Fork => Some(InferenceType::Induction),
            Self::Collider => Some(InferenceType::Abduction),
            Self::Unrelated => None,
        }
    }

    /// **Deprecated** — use [`Self::inference_type`] instead.
    ///
    /// Back-compat shim preserved for downstream consumers that imported
    /// PR #456's `nars_rule() -> Option<NarsRule>` surface (per codex P1
    /// review on PR #457 — removing the method outright is a breaking
    /// change even one commit after introduction). New code should call
    /// `inference_type()`, which returns the canonical
    /// [`crate::nars::InferenceType`].
    ///
    /// The mapping is identical (Chain / ChainRev → Deduction; Fork →
    /// Induction; Collider → Abduction); the returned [`NarsRule`] is a
    /// deprecated subset enum that `From`-converts into `InferenceType`.
    #[deprecated(
        since = "0.2.0",
        note = "Use `inference_type()` which returns the canonical `crate::nars::InferenceType`.                 `NarsRule` is preserved as a deprecated alias for back-compat with PR #456."
    )]
    #[allow(deprecated)]
    pub const fn nars_rule(self) -> Option<NarsRule> {
        // Route the deprecated v1 surface through inference_type() so the
        // junction → rule mapping lives in ONE place (per CodeRabbit on
        // PR #458 — avoid the same duplication-map drift class that
        // motivated the inference_type() introduction in #457).
        //
        // The full InferenceType taxonomy includes Revision + Synthesis
        // which are NOT junction-derivable (no Pearl junction maps to
        // either), so those arms return None defensively even though
        // they are unreachable in practice.
        match self.inference_type() {
            Some(InferenceType::Deduction) => Some(NarsRule::Deduction),
            Some(InferenceType::Induction) => Some(NarsRule::Induction),
            Some(InferenceType::Abduction) => Some(NarsRule::Abduction),
            Some(InferenceType::Revision) | Some(InferenceType::Synthesis) | None => None,
        }
    }
}

/// **Deprecated** — use [`crate::nars::InferenceType`] instead.
///
/// Back-compat alias for PR #456's original three-variant NARS-rule
/// enum. `NarsRule` always corresponds 1:1 to a subset of
/// [`InferenceType`] (Deduction / Induction / Abduction); the full
/// `InferenceType` taxonomy also includes Revision + Synthesis which
/// are not junction-derivable. `From<NarsRule>` lifts to the canonical
/// type for migration.
///
/// Removed in a future major bump; new code should not introduce
/// references to this enum.
#[deprecated(
    since = "0.2.0",
    note = "Use `crate::nars::InferenceType` instead. `NarsRule` is preserved as a             back-compat alias for PR #456's original surface; `From<NarsRule>` lifts             to the canonical type."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NarsRule {
    /// Chain figure: `M -> P`, `S -> M` ⊢ `S -> P` (or the reverse).
    Deduction,
    /// Fork figure (common cause): `M -> P`, `M -> S` ⊢ `S -> P` (with
    /// confidence calibrated by Pearl's induction discounting).
    Induction,
    /// Collider figure (explaining-away): `P -> M`, `S -> M` ⊢ `S -> P`
    /// (with confidence calibrated by Pearl's abduction discounting).
    Abduction,
}

#[allow(deprecated)]
impl From<NarsRule> for InferenceType {
    /// Lift a deprecated [`NarsRule`] to the canonical
    /// [`InferenceType`]. The mapping is 1:1.
    fn from(r: NarsRule) -> Self {
        match r {
            NarsRule::Deduction => InferenceType::Deduction,
            NarsRule::Induction => InferenceType::Induction,
            NarsRule::Abduction => InferenceType::Abduction,
        }
    }
}

/// A pair of SPO edges expressed as their four `NiblePath` endpoints.
///
/// Used as the carrier for Pearl-junction classification via
/// [`EdgePair::classify`]. The carrier struct keeps the classifier's API
/// idiomatic (method-on-type rather than 4-argument free function) and
/// makes downstream code reading more natural at call sites:
///
/// ```
/// # use lance_graph_contract::hhtl::NiblePath;
/// # use lance_graph_contract::pearl_junction::{EdgePair, PearlJunction};
/// let dog = NiblePath::root(0x1).child(0x1);
/// let cat = NiblePath::root(0x1).child(0x2);
/// let mammal = NiblePath::root(0x1);
/// let junction = EdgePair::new(dog, mammal, cat, mammal).classify();
/// assert_eq!(junction, PearlJunction::Collider);
/// ```
///
/// `EdgePair` is `Copy` (four `NiblePath`s are 4×(`u64` + `u8`) =
/// 4×16 bytes packed; trivially copyable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgePair {
    /// Subject of the first edge (`s1 -> o1`).
    pub s1: NiblePath,
    /// Object of the first edge.
    pub o1: NiblePath,
    /// Subject of the second edge (`s2 -> o2`).
    pub s2: NiblePath,
    /// Object of the second edge.
    pub o2: NiblePath,
}

impl EdgePair {
    /// Construct an [`EdgePair`] from four endpoints.
    pub const fn new(s1: NiblePath, o1: NiblePath, s2: NiblePath, o2: NiblePath) -> Self {
        Self { s1, o1, s2, o2 }
    }

    /// Classify this pair of edges into a Pearl junction.
    ///
    /// Empty-path guard: if ANY of the four endpoints is `NiblePath::EMPTY`
    /// (the crate's "no route" sentinel), the classifier returns
    /// `Unrelated`. This prevents matching the `EMPTY == EMPTY` sentinel as
    /// a shared graph term — the unresolved-endpoint case must NOT register
    /// as Chain / Fork / Collider (per codex P2 + CodeRabbit review on
    /// PR #456).
    ///
    /// The classifier checks for shared identity in this order:
    /// 1. `Chain` (`o1 == s2`)
    /// 2. `ChainRev` (`s1 == o2`)
    /// 3. `Fork` (`s1 == s2`)
    /// 4. `Collider` (`o1 == o2`)
    /// 5. otherwise `Unrelated`
    ///
    /// When two edges share BOTH endpoints (e.g. `s1 == s2` AND `o1 == o2`),
    /// the classifier returns `Chain` only if the chain check fires first;
    /// otherwise it follows the order above. Duplicate edges should be
    /// deduplicated by the caller before classification.
    pub const fn classify(self) -> PearlJunction {
        // Empty-path guard: any unresolved endpoint forces Unrelated.
        if has_empty(self.s1) || has_empty(self.o1) || has_empty(self.s2) || has_empty(self.o2) {
            return PearlJunction::Unrelated;
        }
        if niblepath_eq(self.o1, self.s2) {
            return PearlJunction::Chain;
        }
        if niblepath_eq(self.s1, self.o2) {
            return PearlJunction::ChainRev;
        }
        if niblepath_eq(self.s1, self.s2) {
            return PearlJunction::Fork;
        }
        if niblepath_eq(self.o1, self.o2) {
            return PearlJunction::Collider;
        }
        PearlJunction::Unrelated
    }
}

/// Classify a pair of SPO edges by Pearl-junction taxonomy.
///
/// Thin free-function wrapper around [`EdgePair::classify`] preserved for
/// back-compat with PR #456 callers. New code should prefer the carrier-
/// struct method (`EdgePair::new(s1, o1, s2, o2).classify()`).
pub const fn classify_junction(
    s1: NiblePath,
    o1: NiblePath,
    s2: NiblePath,
    o2: NiblePath,
) -> PearlJunction {
    EdgePair::new(s1, o1, s2, o2).classify()
}

/// Returns `true` if the path has `depth == 0` (the `NiblePath::EMPTY`
/// "no route" sentinel). Used by the classifier to guard against treating
/// matching empty sentinels as real graph terms.
const fn has_empty(p: NiblePath) -> bool {
    let (_path, depth) = p.packed();
    depth == 0
}

/// `const fn` equality for [`NiblePath`] — needed because `PartialEq` for
/// user types is not `const` in stable Rust 1.95. Two paths are equal iff
/// their packed `(path, depth)` agree.
const fn niblepath_eq(a: NiblePath, b: NiblePath) -> bool {
    let (ap, ad) = a.packed();
    let (bp, bd) = b.packed();
    ap == bp && ad == bd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dog/cat/mammal canonical example — the anti-swap guard.
    ///
    /// Two `subClassOf` edges share the same OBJECT (`mammal`). The shared
    /// term is the parent; the two subjects (`dog`, `cat`) are siblings
    /// under it; the conclusion `dog -> cat` is Abduction. This is the
    /// COLLIDER pattern, not the Fork pattern (the earlier incorrect
    /// framing inverted these).
    #[test]
    fn collider_is_dog_cat_mammal_with_shared_object() {
        let dog = NiblePath::root(0x1).child(0x1);
        let cat = NiblePath::root(0x1).child(0x2);
        let mammal = NiblePath::root(0x1);

        // dog -> mammal, cat -> mammal: shared object (mammal = parent),
        // distinct subjects (dog, cat = siblings).
        let j = EdgePair::new(dog, mammal, cat, mammal).classify();
        assert_eq!(j, PearlJunction::Collider);
        assert_eq!(j.inference_type(), Some(InferenceType::Abduction));
        assert_eq!(j.label(), "collider");

        // Free-function wrapper produces identical result (back-compat).
        assert_eq!(j, classify_junction(dog, mammal, cat, mammal));
    }

    /// The dog->mammal / dog->pet example — the Fork canonical.
    #[test]
    fn fork_is_dog_mammal_pet_with_shared_subject() {
        let dog = NiblePath::root(0x1).child(0x1);
        let mammal = NiblePath::root(0x1);
        let pet = NiblePath::root(0x2);

        let j = EdgePair::new(dog, mammal, dog, pet).classify();
        assert_eq!(j, PearlJunction::Fork);
        assert_eq!(j.inference_type(), Some(InferenceType::Induction));
        assert_eq!(j.label(), "fork");
    }

    /// Chain: `dog -> mammal -> animal`. `o1 == s2`.
    #[test]
    fn chain_is_dog_mammal_animal_head_to_tail() {
        let dog = NiblePath::root(0x1).child(0x1);
        let mammal = NiblePath::root(0x1);
        let animal = NiblePath::root(0x0);
        let j = EdgePair::new(dog, mammal, mammal, animal).classify();
        assert_eq!(j, PearlJunction::Chain);
        assert_eq!(j.inference_type(), Some(InferenceType::Deduction));
    }

    /// ChainRev: `s1 == o2`.
    #[test]
    fn chain_rev_is_when_s1_equals_o2() {
        let a = NiblePath::root(0x1);
        let b = NiblePath::root(0x2);
        let c = NiblePath::root(0x3);
        let j = EdgePair::new(a, b, c, a).classify();
        assert_eq!(j, PearlJunction::ChainRev);
        assert_eq!(j.inference_type(), Some(InferenceType::Deduction));
    }

    /// Unrelated: no shared term.
    #[test]
    fn unrelated_when_no_shared_term() {
        let a = NiblePath::root(0x1);
        let b = NiblePath::root(0x2);
        let c = NiblePath::root(0x3);
        let d = NiblePath::root(0x4);
        let j = EdgePair::new(a, b, c, d).classify();
        assert_eq!(j, PearlJunction::Unrelated);
        assert_eq!(j.inference_type(), None);
    }

    /// Order-of-checks: Chain wins when both Chain and ChainRev would match.
    #[test]
    fn chain_check_fires_before_other_matches() {
        let x = NiblePath::root(0x1);
        let y = NiblePath::root(0x2);
        let j = EdgePair::new(x, y, y, x).classify();
        assert_eq!(j, PearlJunction::Chain);
    }

    #[test]
    fn const_classify_works_in_const_context() {
        const A: NiblePath = NiblePath::root(0x1);
        const B: NiblePath = NiblePath::root(0x2);
        const C: NiblePath = NiblePath::root(0x3);
        const J: PearlJunction = EdgePair::new(A, B, B, C).classify();
        assert_eq!(J, PearlJunction::Chain);
    }

    // ===== Empty-path sentinel guard (codex P2 + CodeRabbit on PR #456) =====

    /// Two unresolved edges (both endpoints EMPTY) must NOT classify as
    /// Chain / Fork / Collider just because the no-route sentinels match.
    /// They are Unrelated by construction (no real graph terms to compare).
    #[test]
    fn two_fully_empty_edges_are_unrelated() {
        let e = NiblePath::EMPTY;
        let j = EdgePair::new(e, e, e, e).classify();
        assert_eq!(j, PearlJunction::Unrelated);
        assert_eq!(j.inference_type(), None);
    }

    /// One resolved endpoint + one EMPTY sentinel: Unrelated (the resolved
    /// endpoint has no real partner to compare against).
    #[test]
    fn edge_with_one_empty_endpoint_is_unrelated() {
        let real = NiblePath::root(0x1);
        let e = NiblePath::EMPTY;
        // s1=EMPTY, o1=real, s2=real, o2=EMPTY — would naively match Chain
        // (o1 == s2) but EMPTY-guard returns Unrelated.
        let j = EdgePair::new(e, real, real, e).classify();
        assert_eq!(j, PearlJunction::Unrelated);

        // Any EMPTY in any position → Unrelated.
        assert_eq!(
            EdgePair::new(e, real, real, real).classify(),
            PearlJunction::Unrelated
        );
        assert_eq!(
            EdgePair::new(real, e, real, real).classify(),
            PearlJunction::Unrelated
        );
        assert_eq!(
            EdgePair::new(real, real, e, real).classify(),
            PearlJunction::Unrelated
        );
        assert_eq!(
            EdgePair::new(real, real, real, e).classify(),
            PearlJunction::Unrelated
        );
    }

    /// `NiblePath::root` with an out-of-range basin returns `EMPTY` (the
    /// crate's no-route sentinel). The classifier must NOT treat two
    /// out-of-range-derived empties as a real shared term.
    #[test]
    fn out_of_range_basin_produces_empty_and_classifies_as_unrelated() {
        let bad1 = NiblePath::root(0xFF); // out of FAN_OUT
        let bad2 = NiblePath::root(0xEE); // out of FAN_OUT
        let real = NiblePath::root(0x1);
        // Both edges' subjects are out-of-range → EMPTY.
        assert_eq!(
            EdgePair::new(bad1, real, bad2, real).classify(),
            PearlJunction::Unrelated
        );
    }

    // ===== Back-compat shim (codex P1 on PR #457) =====

    /// The deprecated nars_rule() method must continue to work for
    /// downstream consumers that imported PR #456's surface. Verifies the
    /// 1:1 correspondence with inference_type().
    #[test]
    #[allow(deprecated)]
    fn deprecated_nars_rule_matches_inference_type() {
        let dog = NiblePath::root(0x1).child(0x1);
        let cat = NiblePath::root(0x1).child(0x2);
        let mammal = NiblePath::root(0x1);

        let collider = EdgePair::new(dog, mammal, cat, mammal).classify();
        assert_eq!(collider.nars_rule(), Some(NarsRule::Abduction));
        assert_eq!(collider.inference_type(), Some(InferenceType::Abduction));

        // Round-trip via From<NarsRule>
        let canonical: InferenceType = collider.nars_rule().unwrap().into();
        assert_eq!(canonical, collider.inference_type().unwrap());
    }

    /// Unrelated returns None on both methods.
    #[test]
    #[allow(deprecated)]
    fn deprecated_nars_rule_none_when_unrelated() {
        let a = NiblePath::root(0x1);
        let b = NiblePath::root(0x2);
        let c = NiblePath::root(0x3);
        let d = NiblePath::root(0x4);
        let j = EdgePair::new(a, b, c, d).classify();
        assert_eq!(j.nars_rule(), None);
        assert_eq!(j.inference_type(), None);
    }

    /// From<NarsRule> for InferenceType covers the three deprecated variants.
    #[test]
    #[allow(deprecated)]
    fn from_nars_rule_lifts_to_inference_type() {
        assert_eq!(
            InferenceType::from(NarsRule::Deduction),
            InferenceType::Deduction
        );
        assert_eq!(
            InferenceType::from(NarsRule::Induction),
            InferenceType::Induction
        );
        assert_eq!(
            InferenceType::from(NarsRule::Abduction),
            InferenceType::Abduction
        );
    }
}
