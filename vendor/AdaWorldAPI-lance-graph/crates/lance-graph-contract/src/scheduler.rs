//! # `scheduler` — the IN-direction reactive seam (`E-SUBSTRATE-IS-THE-SCHEDULER`).
//!
//! The dual of [`crate::soa_view::MailboxSoaOwner`]. The two directions of the
//! Rubicon kanban over the ONE per-mailbox SoA:
//!
//! - **OUT** ([`MailboxSoaOwner::try_advance_phase`](crate::soa_view::MailboxSoaOwner::try_advance_phase)):
//!   the ractor owner advances a phase → that commit becomes a Lance dataset
//!   **version** → a [`KanbanMove`] (`E-VERSION-ARC-IS-THE-KANBAN`).
//! - **IN** (this module): the reverse subscription — a substrate `LIVE`/scheduled
//!   event over `Dataset::versions()` is **lowered to the next legal**
//!   [`KanbanMove`], which the owner then applies via `try_advance_phase`.
//!
//! This collapses "build a transparent view" into "LIVE-subscribe + schedule" —
//! the same shape as a CI/PR webhook firing the next job (D-MBX-9,
//! `E-SUBSTRATE-IS-THE-SCHEDULER`).
//!
//! ## Why it lives in the zero-dep contract
//! It composes **only** [`MailboxSoaView`] + [`KanbanColumn`] + [`KanbanMove`] +
//! [`ExecTarget`] — no `lance`, no `surreal`, no async runtime. The CI-gated core
//! impl (`D-MBX-9-IN`: a `LanceVersionScheduler` subscribing to
//! `VersionedGraph::versions()` via the callcenter `LanceVersionWatcher`) lands in
//! a buildable downstream crate; this trait is its airgap.
//!
//! ## Invariant — propose, don't dispose
//! [`VersionScheduler::on_version`] takes `&V` (never `&mut`): the scheduler only
//! **proposes** the next move; the [`MailboxSoaOwner`](crate::soa_view::MailboxSoaOwner)
//! is the sole mutator (R1 "one SoA never transformed"; mirrors the
//! `MailboxSoaView` / `MailboxSoaOwner` read/write split).

use crate::kanban::{ExecTarget, KanbanColumn, KanbanMove};
use crate::soa_view::MailboxSoaView;

/// A monotonic Lance dataset version — the surreal Timeline tick, i.e. one entry
/// of `Dataset::versions()`. The IN-direction event carrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DatasetVersion(pub u64);

/// Lower a substrate version event into the **next legal** kanban move for a
/// mailbox view, or `None` when no advance is due (the mailbox is in an absorbing
/// column, or the scheduler's policy filters this tick out).
///
/// The dual of [`crate::soa_view::MailboxSoaOwner`]: a `VersionScheduler` is what a
/// `surreal_container` `LIVE` query (or the callcenter `LanceVersionWatcher`) calls
/// per `versions()` tick to decide whether — and how — the ractor owner should
/// advance the mailbox lifecycle.
pub trait VersionScheduler {
    /// Decide the next move for `view` on observing dataset version `at`. `exec`
    /// selects the backend the precipitated move runs on
    /// ([`ExecTarget::Native`]/[`Jit`](ExecTarget::Jit)/[`SurrealQl`](ExecTarget::SurrealQl)/[`Elixir`](ExecTarget::Elixir)).
    /// Returns `None` to schedule no advance (e.g. `view.phase().is_absorbing()`).
    fn on_version<V: MailboxSoaView>(
        &self,
        view: &V,
        at: DatasetVersion,
        exec: ExecTarget,
    ) -> Option<KanbanMove>;
}

/// The canonical reference scheduler: on every version, advance the mailbox along
/// the Rubicon **forward arc** — the first legal successor of its current column —
/// or yield `None` when the column is absorbing (`Commit`/`Prune`).
///
/// The "forward arc" is [`KanbanColumn::next_phases`]`().first()`:
/// `Planning → CognitiveWork`, `CognitiveWork → Evaluation`, `Evaluation → Commit`,
/// `Plan → Planning` (re-deliberate), `Commit`/`Prune` → none. It stamps the Libet
/// anchor (`-550_000 µs`) on the `Planning → CognitiveWork` Σ-commit crossing and
/// `0` elsewhere — matching the `MailboxSoaOwner::advance_phase` convention.
///
/// This is the substrate-free reference: real schedulers may gate on the
/// `DatasetVersion` delta, choose `Plan`/`Prune` over the forward arc, or batch
/// ticks — they implement [`VersionScheduler`] with their own policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NextPhaseScheduler;

impl VersionScheduler for NextPhaseScheduler {
    fn on_version<V: MailboxSoaView>(
        &self,
        view: &V,
        _at: DatasetVersion,
        exec: ExecTarget,
    ) -> Option<KanbanMove> {
        let from = view.phase();
        // `next_phases()` is empty exactly for the absorbing columns (Commit/Prune):
        // `?` short-circuits to `None`, i.e. "the cycle ended — schedule nothing".
        let to = *from.next_phases().first()?;
        let libet_offset_us = if from == KanbanColumn::Planning && to == KanbanColumn::CognitiveWork
        {
            -550_000
        } else {
            0
        };
        Some(KanbanMove {
            mailbox: view.mailbox_id(),
            from,
            to,
            // Structural witness position (R4): the monotonic cycle stamp stands in
            // for the chain index until the A3 `witness_arc` column lands. Read it as
            // the SoA cycle-ownership stamp via `KanbanMove::cycle()` (S2.5).
            witness_chain_position: view.current_cycle(),
            libet_offset_us,
            exec,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collapse_gate::MailboxId;

    /// Minimal `MailboxSoaView` with a settable phase — proves the scheduler
    /// lowers a version event to the right move without any consumer crate
    /// (same pattern as `soa_view::tests::FakeSoa`).
    struct FakeView {
        id: MailboxId,
        phase: KanbanColumn,
        cycle: u32,
    }
    impl MailboxSoaView for FakeView {
        fn mailbox_id(&self) -> MailboxId {
            self.id
        }
        fn n_rows(&self) -> usize {
            0
        }
        fn w_slot(&self) -> u8 {
            (self.id & 0x3F) as u8
        }
        fn current_cycle(&self) -> u32 {
            self.cycle
        }
        fn phase(&self) -> KanbanColumn {
            self.phase
        }
        fn energy(&self) -> &[f32] {
            &[]
        }
        fn edges_raw(&self) -> &[u64] {
            &[]
        }
        fn meta_raw(&self) -> &[u32] {
            &[]
        }
        fn entity_type(&self) -> &[u16] {
            &[]
        }
    }

    fn view(phase: KanbanColumn) -> FakeView {
        FakeView {
            id: 42,
            phase,
            cycle: 9,
        }
    }

    #[test]
    fn planning_schedules_cognitive_work_with_libet_anchor() {
        let m = NextPhaseScheduler
            .on_version(
                &view(KanbanColumn::Planning),
                DatasetVersion(1),
                ExecTarget::Native,
            )
            .expect("Planning is not absorbing");
        assert_eq!(m.from, KanbanColumn::Planning);
        assert_eq!(m.to, KanbanColumn::CognitiveWork); // forward arc, not the Prune veto
        assert_eq!(m.libet_offset_us, -550_000); // the Σ-commit Rubicon crossing
        assert_eq!(m.mailbox, 42);
        assert_eq!(m.witness_chain_position, 9); // current_cycle stamp
    }

    #[test]
    fn mid_cycle_advances_carry_no_libet_anchor() {
        let cw = NextPhaseScheduler
            .on_version(
                &view(KanbanColumn::CognitiveWork),
                DatasetVersion(2),
                ExecTarget::Native,
            )
            .unwrap();
        assert_eq!(cw.to, KanbanColumn::Evaluation);
        assert_eq!(cw.libet_offset_us, 0);

        let ev = NextPhaseScheduler
            .on_version(
                &view(KanbanColumn::Evaluation),
                DatasetVersion(3),
                ExecTarget::Native,
            )
            .unwrap();
        assert_eq!(ev.to, KanbanColumn::Commit); // forward arc = calcify
        assert_eq!(ev.libet_offset_us, 0);
    }

    #[test]
    fn plan_re_deliberates_back_to_planning() {
        let m = NextPhaseScheduler
            .on_version(
                &view(KanbanColumn::Plan),
                DatasetVersion(4),
                ExecTarget::Native,
            )
            .unwrap();
        assert_eq!(m.from, KanbanColumn::Plan);
        assert_eq!(m.to, KanbanColumn::Planning); // re-enter carrying the witness
    }

    #[test]
    fn absorbing_columns_schedule_nothing() {
        // Commit + Prune are absorbing: the cycle has ended, no move is due.
        assert!(NextPhaseScheduler
            .on_version(
                &view(KanbanColumn::Commit),
                DatasetVersion(5),
                ExecTarget::Native
            )
            .is_none());
        assert!(NextPhaseScheduler
            .on_version(
                &view(KanbanColumn::Prune),
                DatasetVersion(6),
                ExecTarget::Native
            )
            .is_none());
    }

    #[test]
    fn exec_target_threads_through_to_the_move() {
        // The scheduler carries the backend selection onto the precipitated move
        // (the Native/Jit/SurrealQl/Elixir routing tag for the IN-direction).
        for exec in [
            ExecTarget::Native,
            ExecTarget::Jit,
            ExecTarget::SurrealQl,
            ExecTarget::Elixir,
        ] {
            let m = NextPhaseScheduler
                .on_version(&view(KanbanColumn::Planning), DatasetVersion(7), exec)
                .unwrap();
            assert_eq!(m.exec, exec);
        }
    }

    #[test]
    fn scheduled_move_is_a_legal_rubicon_edge() {
        // Whatever the scheduler proposes MUST be a legal transition the owner's
        // `try_advance_phase` will accept (no illegal-edge proposals).
        for phase in [
            KanbanColumn::Planning,
            KanbanColumn::CognitiveWork,
            KanbanColumn::Evaluation,
            KanbanColumn::Plan,
        ] {
            let m = NextPhaseScheduler
                .on_version(&view(phase), DatasetVersion(8), ExecTarget::Native)
                .unwrap();
            assert!(
                m.from.can_transition_to(m.to),
                "{:?} -> {:?} must be a legal Rubicon edge",
                m.from,
                m.to
            );
        }
    }
}
