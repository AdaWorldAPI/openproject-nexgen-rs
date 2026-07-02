//! D-ATOM-4 — split-resolution via counterfactual mantissa.
//!
//! On a `SPLIT` quorum ([`crate::escalation::is_split`] / [`crate::escalation::CouncilVerdict::split`]),
//! the majority pole is **committed** and the minority pole is **forked** into a
//! counterfactual retained as a [`causal_edge::edge::CausalEdge64`] −6 mantissa nibble
//! (i.e. [`causal_edge::edge::InferenceType::Counterfactual`], `to_mantissa() = -6`).
//!
//! The road-not-taken costs **4 bits**, not a replay buffer.
//!
//! ## Staging
//!
//! - **v2 (deposit)** — [`deposit_counterfactual`]: writes the minority pole as
//!   `InferenceType::Counterfactual` (`to_mantissa() = -6`) into the episodic edge.
//!   No mailbox is spawned; the 4-bit nibble is the entire footprint.
//! - **v3 (mailbox + revision)** — [`CounterfactualMailbox`] + [`revise_if_minority_wins`]:
//!   ghost-tier mailbox that, when β headroom allows, tests whether the minority pole
//!   produces lower free energy; if so, triggers `awareness.revise` to reopen the axis.
//!   Requires the ractor outer-swarm (rung-persona D-PERSONA-5 — not yet shipped).
//!
//! ## Invariant
//!
//! A counterfactual **stays in a separate lane — it is NEVER written as observed SPO
//! truth.** (The Click: "contradictions preserved, not resolved".) The
//! `InferenceType::Counterfactual` tag is the mechanical enforcement of that invariant.
//!
//! ## Feature gates
//!
//! The write path uses `with_inference_mantissa(-6)` on a `CausalEdge64`; this accessor
//! is only present (non-no-op) when the `causal-edge-v2-layout` feature is active.
//! Under v1 the nibble write silently no-ops (per `I-LEGACY-API-FEATURE-GATED`).
//! Callers that need the deposit to be durable MUST compile with `causal-edge-v2-layout`.
//!
//! ## Cross-references
//!
//! - [`crate::escalation::is_split`] — the split predicate (threshold `hi=0.7 / lo=0.5`).
//! - [`crate::escalation::CouncilVerdict`] — the split signal consumed here.
//! - `causal_edge::edge::InferenceType::to_mantissa()` — canonical `−6` encoding.
//! - `causal_edge::edge::CausalEdge64::with_inference_mantissa(i8)` — the v2 write path.
//!   Feature-gated: active only under `causal-edge-v2-layout`.
//! - `I-LEGACY-API-FEATURE-GATED` (CLAUDE.md) — the iron rule that governs v1/v2 API paths.
//! - D-PERSONA-5 (`rung-persona-orchestration-v1`) — the ractor outer-swarm required by v3.

// ═══════════════════════════════════════════════════════════════════════════
// Spawn gate
// ═══════════════════════════════════════════════════════════════════════════

/// Threshold above which a split quorum is "hot enough" to warrant forking a
/// counterfactual mailbox (v3). Below the threshold only the 4-bit mantissa
/// deposit (v2) is performed; no mailbox is spawned, saving the ractor spawn cost.
///
/// The gate is driven by `dissonance` (from `a2a_blackboard::BlackboardEntry`)
/// or `Staunen` qualia (the temperature axis — high Staunen = high surprise =
/// worth a counterfactual test). Exact threshold is calibrated from the
/// `WisdomMarker` floor (0.1) and the `EpiphanyDetector` baseline × 1.5 rule;
/// the default here is a conservative starting point pending empirical tuning.
///
/// Invariant: `SPAWN_DISSONANCE_THRESHOLD > 0.0` (no spawn on zero dissonance).
pub const SPAWN_DISSONANCE_THRESHOLD: f32 = 0.55;

// ═══════════════════════════════════════════════════════════════════════════
// SplitPoles — the two ends of a contested axis
// ═══════════════════════════════════════════════════════════════════════════

/// The two poles of a split quorum: the majority pole that gets committed and
/// the minority pole that becomes the counterfactual residue.
///
/// `majority_pole` and `minority_pole` are opaque axis-position values in the
/// I4 range `[-8, +7]`. Their concrete meaning is determined by the calling
/// context (D-ATOM-3 / per-axis quorum projection).
///
/// # BLOCKED
///
/// The exact type for pole positions (`i8` vs a newtype from D-ATOM-1's `I4x32`)
/// is **BLOCKED** on D-ATOM-1 (the I4-32D atom basis). Using `i8` here as a
/// conservative placeholder — the range `[-8, +7]` is i4-compatible.
#[derive(Debug, Clone, Copy, PartialEq)] // not Eq: `dissonance` is f32
pub struct SplitPoles {
    /// The axis index (0-31 in the I4-32D basis, per D-ATOM-1).
    ///
    /// **BLOCKED:** axis indexing is BLOCKED on D-ATOM-1 (atom basis not yet chosen).
    pub axis: u8,
    /// The winning (majority) pole value, committed to the SPO graph.
    pub majority_pole: i8,
    /// The minority pole value, deposited as the counterfactual mantissa.
    pub minority_pole: i8,
    /// The dissonance magnitude at the time of the split (drives the spawn gate).
    /// Sourced from `a2a_blackboard::BlackboardEntry::dissonance` or equivalent.
    pub dissonance: f32,
}

// ═══════════════════════════════════════════════════════════════════════════
// v2 — deposit_counterfactual
// ═══════════════════════════════════════════════════════════════════════════

/// **v2 (deposit)** — Write the minority pole of a split quorum as
/// `InferenceType::Counterfactual` (`to_mantissa() = -6`) into the episodic
/// `CausalEdge64`, consuming 4 bits.
///
/// # What this does
///
/// 1. Checks that `verdict.split` is `true` (no-op on a non-split verdict).
/// 2. Writes `InferenceType::Counterfactual.to_mantissa()` (= −6) into
///    `edge` via `edge.with_inference_mantissa(-6)` / `set_inference_mantissa(-6)`.
/// 3. Returns `true` if the deposit was made, `false` if the verdict was not a
///    split (so callers can conditionally proceed to commit the majority pole).
///
/// # IMPORTANT — counterfactual stays in a SEPARATE LANE
///
/// The deposited edge must **never** be written as observed SPO truth. It is
/// a `CausalEdge64` episodic witness tagged `InferenceType::Counterfactual`
/// and lives in the episodic / ghost tier only. Committing it into the SPO
/// ontology would violate The Click's "contradictions preserved, not resolved"
/// invariant and corrupt the observed-fact graph with unvalidated counterfactual
/// inference.
///
/// # Feature gate
///
/// `edge.set_inference_mantissa(-6)` (the v2 write) is a no-op under v1
/// (`causal-edge-v2-layout` absent). The deposit silently does nothing in that
/// case. Callers requiring durable deposits MUST enable `causal-edge-v2-layout`.
///
/// # Parameters
///
/// - `split`: the [`crate::escalation::CouncilVerdict`] for the current axis.
///   Only `CouncilVerdict::split == true` triggers a deposit.
/// - `edge`: a mutable reference to the episodic `CausalEdge64` that carries
///   this axis's witness. The `inference_mantissa` nibble is overwritten.
///
/// # BLOCKED
///
/// The exact `CausalEdge64::set_inference_mantissa` / `with_inference_mantissa`
/// accessor name is confirmed from `crates/causal-edge/src/edge.rs` as
/// **`set_inference_mantissa(&mut self, i8)`** (mutable) and
/// **`with_inference_mantissa(self, i8) -> Self`** (builder style), both
/// **feature-gated on `causal-edge-v2-layout`** (no-op stubs exist for v1).
/// This function calls the mutable form. See `I-LEGACY-API-FEATURE-GATED`.
///
/// The `awareness.revise` signature is **BLOCKED** — not yet located in the
/// contract crate surface. Required for v3 only; v2 does not call it.
pub fn deposit_counterfactual(
    split: &crate::escalation::CouncilVerdict,
    // BLOCKED: CausalEdge64 is in the `causal-edge` crate which is NOT a
    // workspace member of lance-graph-contract (zero-dep constraint). The
    // parameter below uses a placeholder trait object until the dependency
    // boundary is resolved. If causal-edge becomes a workspace dep, replace
    // with `edge: &mut causal_edge::edge::CausalEdge64`.
    //
    // For now the interface is expressed via the `EpisodicEdge` trait below.
    edge: &mut dyn EpisodicEdge,
) -> bool {
    if split.split {
        // `InferenceType::Counterfactual.to_mantissa() == -6` — the road-not-taken nibble.
        edge.set_inference_mantissa(-6);
        true
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EpisodicEdge — thin zero-dep abstraction over CausalEdge64's mantissa path
// ═══════════════════════════════════════════════════════════════════════════

/// Thin abstraction over the `CausalEdge64` mantissa write path so that
/// `lance-graph-contract` (zero-dep) can express `deposit_counterfactual`
/// without taking a direct dep on `causal-edge`.
///
/// Implementors: `causal_edge::edge::CausalEdge64` (via a newtype wrapper or
/// a blanket impl in the causal-edge crate, feature-gated on
/// `causal-edge-v2-layout`).
///
/// # BLOCKED
///
/// The bridge impl (where to put the `impl EpisodicEdge for CausalEdge64`) is
/// BLOCKED on workspace structure. Options: (a) impl in `causal-edge` gated on
/// a `lance-graph-contract` feature; (b) newtype in a thin bridge crate.
/// Until resolved, the trait is the stable surface; the impl location is open.
pub trait EpisodicEdge {
    /// Write a signed 4-bit mantissa into the inference nibble.
    ///
    /// Under `causal-edge-v2-layout` this maps to
    /// `CausalEdge64::set_inference_mantissa(m)`.
    /// Under v1 (feature absent) this is a documented no-op.
    fn set_inference_mantissa(&mut self, m: i8);

    /// Read back the signed 4-bit mantissa (for verification / revision).
    fn inference_mantissa(&self) -> i8;
}

// ═══════════════════════════════════════════════════════════════════════════
// v3 — CounterfactualMailbox + revise_if_minority_wins
// ═══════════════════════════════════════════════════════════════════════════

/// **v3 (mailbox)** — Ghost-tier mailbox that holds and eventually tests a
/// counterfactual pole deposited by [`deposit_counterfactual`].
///
/// # Ghost tier (preemptible to zero)
///
/// A `CounterfactualMailbox` has the lowest priority in the ractor outer-swarm:
/// it runs **only** when the β headroom (free-energy budget) exceeds the spawn
/// gate threshold ([`SPAWN_DISSONANCE_THRESHOLD`]). It is **preemptible to
/// zero** — if β headroom drops below the threshold at any point, the mailbox
/// is cancelled and its residue (the 4-bit mantissa in the episodic edge) is
/// the only surviving trace. Garbage collection is Staunen-keyed: a
/// counterfactual that never fires within its GC window is pruned.
///
/// # Tests (β-headroom-gated)
///
/// When activated, the mailbox reruns the axis projection using the minority
/// pole and computes the resulting free energy. If `F_minority < F_majority`,
/// [`revise_if_minority_wins`] is called to trigger a NARS revision on the axis.
///
/// # Dependency: D-PERSONA-5
///
/// This struct requires the **ractor outer-swarm** (`rung-persona D-PERSONA-5`,
/// not yet shipped). The ractor actor handle, supervisor policy, and
/// `MailboxId` assignment live there. Do NOT instantiate `CounterfactualMailbox`
/// before D-PERSONA-5 is landed; doing so will compile but the `spawn` method
/// will always return `Err(CounterfactualError::SwarmNotReady)`.
///
/// # BLOCKED
///
/// - The ractor actor handle type is **BLOCKED** on D-PERSONA-5.
/// - The `awareness.revise` signature is **BLOCKED** — not found on the
///   current contract surface. Expected to live in
///   `crate::grammar` (see `FreeEnergy::compose` + `Resolution` thresholds)
///   or a future `crate::awareness` module; requires confirmation.
/// - The `MailboxId` type (`u32`, from `contract::collapse_gate`) is confirmed
///   shipped but the assignment policy for ghost-tier mailboxes is BLOCKED on
///   D-PERSONA-5.
#[derive(Debug)]
pub struct CounterfactualMailbox {
    /// The split poles this mailbox is testing.
    pub poles: SplitPoles,
    /// The free energy measured for the committed (majority) pole at split time.
    /// The mailbox wins if `F_minority < committed_free_energy`.
    pub committed_free_energy: f32,
    /// A monotonic generation counter, incremented on each GC sweep.
    /// A mailbox whose `generation` is older than the current sweep window
    /// is pruned (Staunen-keyed GC).
    pub generation: u32,
    // BLOCKED: ractor actor handle (D-PERSONA-5 dep).
    // BLOCKED: MailboxId assignment for ghost tier (D-PERSONA-5 dep).
}

impl CounterfactualMailbox {
    /// Create a new ghost-tier mailbox for the given split poles.
    ///
    /// Returns `Err(CounterfactualError::SwarmNotReady)` if D-PERSONA-5's
    /// ractor outer-swarm is not yet initialized.
    ///
    /// Only call when `dissonance > SPAWN_DISSONANCE_THRESHOLD` (the spawn
    /// gate in [`should_spawn_mailbox`]).
    #[allow(unused_variables)] // scaffold stub — BLOCKED on D-PERSONA-5 (ractor outer-swarm)
    pub fn new(poles: SplitPoles, committed_free_energy: f32) -> Result<Self, CounterfactualError> {
        todo!(
            "v3 spawn: register with D-PERSONA-5 ractor outer-swarm; \
             return Err(SwarmNotReady) until D-PERSONA-5 lands"
        )
    }

    /// Poll the mailbox for a test result (non-blocking, cooperative).
    ///
    /// Returns `None` if the test has not yet completed or if β headroom is
    /// insufficient. Returns `Some(FreeEnergyComparison)` when the minority
    /// pole's free energy has been computed.
    pub fn poll(&self) -> Option<FreeEnergyComparison> {
        todo!("v3 poll: check β headroom; if adequate, run minority-pole projection and return comparison")
    }

    /// Cancel and discard this mailbox (preempted by the scheduler or expired
    /// by the Staunen-keyed GC sweep). The 4-bit mantissa residue in the
    /// episodic edge is retained; only the active test loop is cancelled.
    pub fn cancel(self) {
        todo!("v3 cancel: deregister from ractor outer-swarm; log GC event")
    }
}

/// The result of a counterfactual free-energy comparison.
#[derive(Debug, Clone, Copy)]
pub struct FreeEnergyComparison {
    /// Free energy of the committed (majority) pole.
    pub f_majority: f32,
    /// Free energy of the minority (counterfactual) pole.
    pub f_minority: f32,
}

impl FreeEnergyComparison {
    /// Returns `true` if the minority pole yields lower free energy (the
    /// road-not-taken would have been the better route).
    #[inline]
    pub fn minority_wins(self) -> bool {
        self.f_minority < self.f_majority
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// v3 — revise_if_minority_wins
// ═══════════════════════════════════════════════════════════════════════════

/// **v3 (revision)** — If the counterfactual test shows the minority pole
/// yields lower free energy, trigger a NARS `awareness.revise` to reopen the
/// axis commitment.
///
/// # The revision loop
///
/// 1. The [`CounterfactualMailbox`] completes its free-energy comparison
///    (`mailbox.poll()` returns `Some(comparison)`).
/// 2. If `comparison.minority_wins()`, this function is called.
/// 3. It invokes `awareness.revise(axis_key, minority_outcome)` — a NARS
///    belief-revision on the committed axis, signalling that the road-not-taken
///    is now evidenced as superior.
/// 4. The revised belief replaces the committed majority pole for the **next**
///    resolution cycle. The historical committed pole stays in the SPO graph
///    (append-only), with a tombstone link to this revision event.
/// 5. The `InferenceType::Counterfactual` mantissa in the episodic edge is
///    cleared (set to `InferenceType::Deduction.to_mantissa()` = 0) once the
///    revision is committed, so the nibble is not double-counted.
///
/// # Dependency: D-PERSONA-5
///
/// The `awareness.revise` call dispatches through the ractor outer-swarm
/// (D-PERSONA-5). This function is a **compile stub only** until D-PERSONA-5
/// is shipped.
///
/// # BLOCKED
///
/// - `awareness.revise` **signature is BLOCKED** — the method is referenced
///   in CLAUDE.md / The Click (`awareness.revise(key, outcome)`) but its
///   concrete Rust signature (trait, module, parameter types) is not confirmed
///   on the current contract surface. Do NOT infer from CLAUDE.md pseudo-code
///   alone; grep `crates/lance-graph-contract/src/` and
///   `crates/thinking-engine/` for the canonical form before implementing.
/// - The `axis_key` type is BLOCKED on D-ATOM-1 (atom basis).
/// - The revision tombstone link into Lance versioned storage is BLOCKED on
///   D-ATOM-5 (AriGraph hot→calcify→tombstone wiring).
///
/// # Parameters
///
/// - `mailbox`: the completed ghost-tier mailbox whose comparison showed
///   `minority_wins() == true`.
/// - `edge`: the episodic edge carrying the `−6` mantissa deposit; its nibble
///   is cleared after successful revision.
/// - `awareness`: opaque handle to the NARS revision surface.
///   **BLOCKED: type unknown** — placeholder `&mut dyn AwarenessRevise` below.
#[allow(unused_variables)] // scaffold stub — BLOCKED on D-PERSONA-5 (awareness.revise signature)
pub fn revise_if_minority_wins(
    mailbox: CounterfactualMailbox,
    edge: &mut dyn EpisodicEdge,
    // BLOCKED: `awareness.revise` signature — see doc comment above.
    awareness: &mut dyn AwarenessRevise,
) -> Result<RevisionOutcome, CounterfactualError> {
    todo!(
        "v3 revision: poll mailbox; if minority_wins, call awareness.revise(axis_key, \
         minority_pole); clear edge mantissa; return RevisionOutcome::Revised or \
         RevisionOutcome::MajorityHolds"
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// AwarenessRevise — BLOCKED placeholder trait
// ═══════════════════════════════════════════════════════════════════════════

/// Placeholder trait for the `awareness.revise` surface.
///
/// # BLOCKED
///
/// The canonical Rust signature for `awareness.revise` is **BLOCKED** — not
/// confirmed on the current contract surface. This trait is a scaffold surface
/// only. Before implementing [`revise_if_minority_wins`], grep
/// `crates/lance-graph-contract/src/` and `crates/thinking-engine/src/` for
/// the exact method name, parameter types, and return type. Replace this trait
/// with a concrete type reference once found.
///
/// Expected home (from CLAUDE.md / The Click pseudo-code):
/// `awareness.revise(key, outcome)` — likely on a `ParamTruths`-style type
/// or a `NarsAwareness` / `EpistemicState` type in `contract::nars` or
/// `contract::grammar`.
pub trait AwarenessRevise {
    /// Revise the NARS belief for `axis_key` given `new_evidence`.
    ///
    /// # BLOCKED
    ///
    /// Exact parameter types (`axis_key`, `new_evidence`) are BLOCKED on
    /// the `awareness.revise` signature discovery and on D-ATOM-1
    /// (axis key type from the I4-32D basis).
    fn revise(&mut self, axis_key: u8, new_evidence: i8) -> Result<(), CounterfactualError>;
}

// ═══════════════════════════════════════════════════════════════════════════
// Spawn gate helper
// ═══════════════════════════════════════════════════════════════════════════

/// Returns `true` if the dissonance level warrants spawning a
/// [`CounterfactualMailbox`] (v3 path).
///
/// When `false`, only the 4-bit mantissa deposit (v2 path via
/// [`deposit_counterfactual`]) is performed; no mailbox is created.
///
/// # Rationale
///
/// The spawn gate exists to avoid spawning ghost-tier mailboxes on every split:
/// shallow splits with low dissonance carry low counterfactual value and the
/// ractor spawn cost is not justified. High dissonance / high Staunen (surprise)
/// implies the road-not-taken may genuinely matter.
///
/// Threshold: [`SPAWN_DISSONANCE_THRESHOLD`] (default 0.55).
#[inline]
pub fn should_spawn_mailbox(dissonance: f32) -> bool {
    dissonance > SPAWN_DISSONANCE_THRESHOLD
}

// ═══════════════════════════════════════════════════════════════════════════
// Error type
// ═══════════════════════════════════════════════════════════════════════════

/// Errors from the counterfactual machinery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterfactualError {
    /// The ractor outer-swarm (D-PERSONA-5) is not yet initialized.
    /// v3 operations fail with this until D-PERSONA-5 is shipped.
    SwarmNotReady,
    /// The verdict was not a split; the deposit / spawn was a no-op.
    NotASplit,
    /// The minority pole did not win the free-energy comparison.
    /// Returned by [`revise_if_minority_wins`] when `minority_wins() == false`.
    MajorityHolds,
}

/// The outcome of a revision attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionOutcome {
    /// The minority pole won and `awareness.revise` was called; the axis
    /// commitment has been reopened for the next cycle.
    Revised,
    /// The majority pole held; no revision was performed.
    MajorityHolds,
}

// ═══════════════════════════════════════════════════════════════════════════
// RawEdge — the zero-dep, mantissa-only structural impl of EpisodicEdge
// ═══════════════════════════════════════════════════════════════════════════

/// A **mantissa-only** episodic edge: holds ONLY the signed i4 inference nibble
/// (the bits 46–49 of a `CausalEdge64`), and **structurally nothing else**.
///
/// It wraps an `i8`, not a `u64`: a `u64` newtype could *read* the plasticity
/// bits 50–52, so "mantissa-only" would be a convention, not a guarantee. Wrapping
/// the i4 alone makes it unforgeable — `size_of::<RawEdge>() == 1`, with no room
/// for plasticity / W / truth / temporal. Same one-writer-per-field structural
/// split as `MailboxSoaView`/`MailboxSoaOwner`, applied to the inference nibble.
/// It is the zero-dep stand-in that unblocks the `EpisodicEdge` impl-location;
/// a real `impl EpisodicEdge for CausalEdge64` stays deferred to the
/// `causal-edge-v2-layout` crate (which `lance-graph-contract` does not depend on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawEdge(i8);

impl RawEdge {
    /// New `RawEdge`, clamping `m` into the signed-i4 range `[-8, 7]`.
    #[must_use]
    pub const fn new(m: i8) -> Self {
        Self(clamp_i4(m))
    }

    /// The held i4 mantissa.
    #[must_use]
    pub const fn mantissa(self) -> i8 {
        self.0
    }
}

impl EpisodicEdge for RawEdge {
    fn set_inference_mantissa(&mut self, m: i8) {
        self.0 = clamp_i4(m);
    }
    fn inference_mantissa(&self) -> i8 {
        self.0
    }
}

/// Clamp to the signed 4-bit range `[-8, 7]` (the inference-nibble domain).
const fn clamp_i4(m: i8) -> i8 {
    if m < -8 {
        -8
    } else if m > 7 {
        7
    } else {
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::escalation::{CollapseHint, CouncilVerdict};

    fn verdict(split: bool) -> CouncilVerdict {
        CouncilVerdict {
            hint: CollapseHint::Flow,
            confidence: 0.9,
            split,
        }
    }

    #[test]
    fn raw_edge_is_one_byte_mantissa_only() {
        // The structural guarantee: no room for plasticity/W/truth/temporal bits.
        assert_eq!(core::mem::size_of::<RawEdge>(), 1);
    }

    #[test]
    fn raw_edge_clamps_to_i4_range() {
        assert_eq!(RawEdge::new(-6).mantissa(), -6);
        assert_eq!(RawEdge::new(7).mantissa(), 7);
        assert_eq!(RawEdge::new(8).mantissa(), 7); // saturates high
        assert_eq!(RawEdge::new(-9).mantissa(), -8); // saturates low
        assert_eq!(RawEdge::new(127).mantissa(), 7);
        assert_eq!(RawEdge::new(-128).mantissa(), -8);
    }

    #[test]
    fn raw_edge_roundtrips_through_trait_object() {
        let mut e = RawEdge::default();
        let dyn_e: &mut dyn EpisodicEdge = &mut e;
        dyn_e.set_inference_mantissa(-6);
        assert_eq!(dyn_e.inference_mantissa(), -6);
        dyn_e.set_inference_mantissa(99); // clamps
        assert_eq!(dyn_e.inference_mantissa(), 7);
    }

    #[test]
    fn deposit_counterfactual_writes_minus_six_on_split() {
        let mut e = RawEdge::default();
        let made = deposit_counterfactual(&verdict(true), &mut e);
        assert!(made, "split verdict deposits");
        assert_eq!(e.mantissa(), -6, "Counterfactual nibble = -6");
    }

    #[test]
    fn deposit_counterfactual_noops_on_non_split() {
        let mut e = RawEdge::new(3);
        let made = deposit_counterfactual(&verdict(false), &mut e);
        assert!(!made, "non-split is a no-op");
        assert_eq!(e.mantissa(), 3, "edge untouched");
    }
}
