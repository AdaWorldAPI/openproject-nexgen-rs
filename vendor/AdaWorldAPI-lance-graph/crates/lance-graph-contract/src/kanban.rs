//! # `kanban` — the 4-phase Rubicon kanban contract (zero-dep).
//!
//! The seam where three subsystems meet over the ONE per-mailbox SoA:
//! - **lance-graph-planner** emits a [`KanbanMove`] (the plan's output unit),
//! - **ractor** (the mailbox owner, `lance-graph-supervisor`) drives the
//!   transition — advancing a [`KanbanColumn`] *is* the mailbox lifecycle step,
//! - **surrealdb** (`surreal_container`) projects the columns as the kanban view
//!   over SoA-shaped Lance rows.
//!
//! Carried across the canonical [`crate::orchestration::OrchestrationBridge`] as a
//! `UnifiedStep { step_type: "kanban.*" }` ([`crate::orchestration::StepDomain::Kanban`]).
//!
//! Spec: `.claude/plans/unified-soa-convergence-v1.md` §5 + §8.4 (D-MBX-A6 Phase 1).
//!
//! ## Invariants honoured
//! - **R1 "one SoA never transformed":** a [`KanbanMove`] is a *transition record*,
//!   not SoA data — it carries only `Copy` scalars + a pointer, never the SoA.
//! - **R4 witness-as-pointer:** the witness is a `chain_position` index into the
//!   source mailbox's witness arc — structural time, never the witnessed data.
//!   (The retired `CollapseGateEmission` carrier used the same convention; the
//!   convention survives the carrier's removal per PR #477.)

use crate::collapse_gate::MailboxId;
use crate::mul::GateDecision;

/// The four Rubicon phases (+ two terminal exits), Libet-anchored.
///
/// The mailbox lifecycle advances through these columns from spawn toward a
/// terminal column. The discriminants are stable (used as a kanban-column key and
/// for compact SoA storage) — **do not reorder**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum KanbanColumn {
    /// `t < -550 ms` (Libet readiness-potential window): ractor owns the SoA;
    /// counterfactual pre-planning / expansion happens here. The spawn state.
    #[default]
    Planning = 0,
    /// `t >= -550 ms`: the SoA mutates under cognitive operations; the Σ-commit
    /// ratchet advances here.
    CognitiveWork = 1,
    /// `t > 0`: read back over the witness arc; residual free-energy assessed.
    Evaluation = 2,
    /// Terminal — calcify: commit to Lance SPO-G + AriGraph pointer.
    Commit = 3,
    /// Terminal — re-plan: re-enter [`Planning`](KanbanColumn::Planning) carrying
    /// the witness (the "act differently next time" exit).
    Plan = 4,
    /// Terminal — veto: drop the move (Libet "free won't", post-hoc inhibition).
    Prune = 5,
}

impl KanbanColumn {
    /// Is this a terminal **kanban column** — one of Evaluation's 3-way decision
    /// outcomes (`Commit` / `Plan` / `Prune`)?
    ///
    /// Note `Plan` is terminal *as a decision* but re-enters `Planning`
    /// (see [`next_phases`](KanbanColumn::next_phases)); use
    /// [`is_absorbing`](KanbanColumn::is_absorbing) for "the mailbox cycle truly
    /// ends here, tombstone now".
    #[inline]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Commit | Self::Plan | Self::Prune)
    }

    /// Is this an **absorbing** column — the mailbox cycle ends here with no
    /// successor (`Commit` = calcify to cold path, `Prune` = drop)?
    ///
    /// `Plan` is NOT absorbing (it re-deliberates back to `Planning`). The ractor
    /// lifecycle driver tombstones the mailbox iff the cycle reaches an absorbing
    /// column — the LE-3 cycle-end commit/SLA decision hooks here.
    #[inline]
    pub fn is_absorbing(self) -> bool {
        matches!(self, Self::Commit | Self::Prune)
    }

    /// The valid successor columns from `self` in the Rubicon lifecycle DAG.
    ///
    /// ```text
    /// Planning ─▶ CognitiveWork ─▶ Evaluation ─▶ { Commit | Plan | Prune }
    ///    │                                            │
    ///    └─▶ Prune (pre-Rubicon Libet veto)           └ Plan ─▶ Planning (re-deliberate)
    /// ```
    /// - `Planning` advances to `CognitiveWork` (the −550 ms Σ-commit / Rubicon
    ///   crossing) or is vetoed straight to `Prune` (Libet "free won't", pre-Rubicon).
    /// - `CognitiveWork → Evaluation` (post-actional read-back).
    /// - `Evaluation` is the terminal 3-way: `Commit` (calcify), `Plan` (re-plan),
    ///   `Prune` (drop).
    /// - `Plan → Planning` re-enters the cycle carrying the witness.
    /// - `Commit` / `Prune` are absorbing (no successor).
    #[inline]
    pub fn next_phases(self) -> &'static [KanbanColumn] {
        match self {
            Self::Planning => &[Self::CognitiveWork, Self::Prune],
            Self::CognitiveWork => &[Self::Evaluation],
            Self::Evaluation => &[Self::Commit, Self::Plan, Self::Prune],
            Self::Plan => &[Self::Planning],
            Self::Commit | Self::Prune => &[],
        }
    }

    /// Whether `self → to` is a legal Rubicon lifecycle transition.
    #[inline]
    pub fn can_transition_to(self, to: KanbanColumn) -> bool {
        self.next_phases().contains(&to)
    }

    /// Decode a stored discriminant (e.g. the `ValueTenant::Kanban` phase byte).
    /// Unknown values fall back to [`Planning`](KanbanColumn::Planning) — the
    /// zero-fallback default, consistent with the canon ladder.
    #[inline]
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::CognitiveWork,
            2 => Self::Evaluation,
            3 => Self::Commit,
            4 => Self::Plan,
            5 => Self::Prune,
            _ => Self::Planning,
        }
    }

    /// The next Rubicon column a [`GateDecision`] drives this phase to — the S2
    /// "MUL → phase" seam (capstone `cognitive-loop-wiring` plan). Returns ONLY a
    /// legal successor ([`next_phases`](KanbanColumn::next_phases)), so the gate
    /// can never produce an out-of-DAG transition:
    /// - [`GateDecision::Flow`] → the forward successor (the first non-`Prune`
    ///   next phase): `Planning → CognitiveWork → Evaluation → Commit`,
    ///   `Plan → Planning`. Absorbing columns (`Commit`/`Prune`) have none.
    /// - [`GateDecision::Block`] → `Prune` **iff** it is a legal successor here
    ///   (the Libet "free won't" veto at `Planning`/`Evaluation`); else `None`
    ///   (mid-`CognitiveWork` has no veto edge — hold instead).
    /// - [`GateDecision::Hold`] → `None` (stay in place, re-evaluate next cycle).
    #[inline]
    #[must_use]
    pub fn advance_on_gate(self, gate: &GateDecision) -> Option<KanbanColumn> {
        let nexts = self.next_phases();
        match gate {
            GateDecision::Flow => nexts.iter().copied().find(|c| *c != KanbanColumn::Prune),
            GateDecision::Block { .. } => nexts.iter().copied().find(|c| *c == KanbanColumn::Prune),
            GateDecision::Hold { .. } => None,
        }
    }
}

/// One kanban transition: the planner's output unit and the ractor's lifecycle step.
///
/// `Copy` and small (≤ 16 B) so it rides the airgap as owned microcopy, never a
/// borrow into the SoA (R1). The witness is a *pointer* (R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KanbanMove {
    /// The mailbox whose lifecycle is advancing.
    pub mailbox: MailboxId,
    /// Column the mailbox is leaving.
    pub from: KanbanColumn,
    /// Column the mailbox is entering.
    pub to: KanbanColumn,
    /// Witness pointer: position in the source mailbox's witness chain —
    /// structural time, not a wall-clock stamp (R4). (Same convention the
    /// retired `CollapseGateEmission` carrier used, kept after its removal.)
    pub witness_chain_position: u32,
    /// Libet commit anchor: signed micros relative to the act. `-550_000` on the
    /// `Planning → CognitiveWork` Σ-commit; `0` otherwise. Structural offset only.
    pub libet_offset_us: i32,
    /// Which execution backend the planner selected for this move's work — the
    /// JIT-adjacent strategy target (native planner / JIT / SurrealQL / Elixir).
    pub exec: ExecTarget,
}

impl KanbanMove {
    /// The SoA cycle-ownership stamp (S2.5) — the mailbox `current_cycle` at
    /// which this lifecycle step was emitted.
    ///
    /// Both real paths that record a move ([`crate::scheduler`] and the
    /// `cognitive-shader-driver` `MailboxSoaOwner` impl — the mailbox writing its
    /// own lifecycle step in place, NOT an inter-mailbox emission per the #477
    /// three-tier model) stamp `witness_chain_position = current_cycle` (the
    /// documented "monotonic cycle
    /// stamp stands in for the chain index until the A3 witness-arc column lands"
    /// convention). This accessor names that intent so the planner and a
    /// `ExecTarget::SurrealQl` read-as-of can be cycle-aware off ONE source of
    /// truth without growing the ≤16 B airgap baton. When the A3 witness-arc
    /// column lands and `witness_chain_position` becomes a distinct chain index,
    /// this accessor moves to its own field (an A3-era change, version-gated).
    #[inline]
    pub fn cycle(&self) -> u32 {
        self.witness_chain_position
    }
}

/// The execution backend a [`KanbanMove`] is dispatched to — the planner's
/// JIT-adjacent **strategy target**. Distinct from the planner's 16 composable
/// *planning* strategies: this names *where the precipitated plan runs*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ExecTarget {
    /// The lance-graph-planner native engine (default).
    #[default]
    Native = 0,
    /// JIT-compiled kernel (JITson / Cranelift template).
    Jit = 1,
    /// Lowered to SurrealQL and run in the substrate.
    SurrealQl = 2,
    /// An Elixir-like declarative template.
    Elixir = 3,
}

impl ExecTarget {
    /// Decode a stored discriminant (e.g. the `ValueTenant::Kanban` exec byte).
    /// Unknown values fall back to [`Native`](ExecTarget::Native), the default.
    #[inline]
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Jit,
            2 => Self::SurrealQl,
            3 => Self::Elixir,
            _ => Self::Native,
        }
    }
}

/// Error from [`crate::soa_view::MailboxSoaOwner::try_advance_phase`] when a requested
/// transition is not a legal Rubicon lifecycle edge ([`KanbanColumn::can_transition_to`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RubiconTransitionError {
    /// The column the mailbox is currently in.
    pub from: KanbanColumn,
    /// The (rejected) requested target column.
    pub to: KanbanColumn,
}

impl core::fmt::Display for RubiconTransitionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "illegal Rubicon transition {:?} -> {:?} (allowed: {:?})",
            self.from,
            self.to,
            self.from.next_phases()
        )
    }
}

// Matches the crate convention for error types (cf. `cam::CodecParamsError`):
// a `Display` impl + an empty `core::error::Error` impl so the error composes
// with `?` / `Box<dyn Error>`.
impl core::error::Error for RubiconTransitionError {}

// `KanbanMove` must stay a small owned microcopy (airgap discipline, I1):
// MailboxId(4) + u32(4) + i32(4) + 2×KanbanColumn(1) + ExecTarget(1) packs within 16 B.
const _: () = assert!(core::mem::size_of::<KanbanMove>() <= 16);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanban_column_discriminants_are_stable() {
        // Stable column key — do not reorder.
        assert_eq!(KanbanColumn::Planning as u8, 0);
        assert_eq!(KanbanColumn::CognitiveWork as u8, 1);
        assert_eq!(KanbanColumn::Evaluation as u8, 2);
        assert_eq!(KanbanColumn::Commit as u8, 3);
        assert_eq!(KanbanColumn::Plan as u8, 4);
        assert_eq!(KanbanColumn::Prune as u8, 5);
    }

    #[test]
    fn default_column_is_planning_the_spawn_state() {
        assert_eq!(KanbanColumn::default(), KanbanColumn::Planning);
    }

    #[test]
    fn terminal_columns_are_commit_plan_prune() {
        assert!(KanbanColumn::Commit.is_terminal());
        assert!(KanbanColumn::Plan.is_terminal());
        assert!(KanbanColumn::Prune.is_terminal());
        assert!(!KanbanColumn::Planning.is_terminal());
        assert!(!KanbanColumn::CognitiveWork.is_terminal());
        assert!(!KanbanColumn::Evaluation.is_terminal());
    }

    #[test]
    fn kanban_move_is_copy_and_small() {
        let m = KanbanMove {
            mailbox: 42,
            from: KanbanColumn::Planning,
            to: KanbanColumn::CognitiveWork,
            witness_chain_position: 7,
            libet_offset_us: -550_000,
            exec: ExecTarget::Native,
        };
        let n = m; // Copy, not move
        assert_eq!(m, n);
        assert!(core::mem::size_of::<KanbanMove>() <= 16);
    }

    #[test]
    fn rubicon_lifecycle_transitions() {
        // Forward arc.
        assert!(KanbanColumn::Planning.can_transition_to(KanbanColumn::CognitiveWork));
        assert!(KanbanColumn::CognitiveWork.can_transition_to(KanbanColumn::Evaluation));
        assert!(KanbanColumn::Evaluation.can_transition_to(KanbanColumn::Commit));
        assert!(KanbanColumn::Evaluation.can_transition_to(KanbanColumn::Plan));
        assert!(KanbanColumn::Evaluation.can_transition_to(KanbanColumn::Prune));
        // Pre-Rubicon Libet veto + re-deliberation re-entry.
        assert!(KanbanColumn::Planning.can_transition_to(KanbanColumn::Prune));
        assert!(KanbanColumn::Plan.can_transition_to(KanbanColumn::Planning));
        // Illegal skips / reversals.
        assert!(!KanbanColumn::Planning.can_transition_to(KanbanColumn::Evaluation));
        assert!(!KanbanColumn::CognitiveWork.can_transition_to(KanbanColumn::Planning));
        assert!(!KanbanColumn::Evaluation.can_transition_to(KanbanColumn::CognitiveWork));
        // Absorbing terminals.
        assert!(KanbanColumn::Commit.next_phases().is_empty());
        assert!(KanbanColumn::Prune.next_phases().is_empty());
        assert!(!KanbanColumn::Commit.can_transition_to(KanbanColumn::Planning));
    }

    #[test]
    fn absorbing_is_commit_and_prune_only_not_plan() {
        // Plan is terminal-as-decision but re-deliberates → NOT absorbing.
        assert!(KanbanColumn::Commit.is_absorbing());
        assert!(KanbanColumn::Prune.is_absorbing());
        assert!(!KanbanColumn::Plan.is_absorbing());
        assert!(KanbanColumn::Plan.is_terminal()); // terminal decision, but...
        assert!(KanbanColumn::Plan.can_transition_to(KanbanColumn::Planning)); // ...re-enters.
                                                                               // The ractor driver tombstones iff absorbing.
        assert!(!KanbanColumn::Planning.is_absorbing());
        assert!(!KanbanColumn::Evaluation.is_absorbing());
    }

    #[test]
    fn exec_target_default_is_native() {
        assert_eq!(ExecTarget::default(), ExecTarget::Native);
        assert_eq!(ExecTarget::Native as u8, 0);
        assert_eq!(ExecTarget::Elixir as u8, 3);
    }
}
