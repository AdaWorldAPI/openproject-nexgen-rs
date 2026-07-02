//! # `head2head` — competing-expert superposition + winner selection.
//!
//! Two (or N) mailbox-experts reason over the same input *in parallel*, each
//! posting a [`BlackboardEntry`]; head2head selects **one** winner whose
//! emissions become the authoritative spiral. This is the *selection* half of
//! the superposition (item: "head2head mailbox thinking as superposition") — the
//! parallel mailbox *execution* is the CI-gated consumer side; the
//! winner-pick over the blackboard is what lives, zero-dep and verifiable, here.
//!
//! ## The Go metaphor (the user's framing)
//! Two strategies compete and the board scores them:
//! - **infight** (close tactical combat) ≈ [`WinnerCriterion::DissonanceMin`]:
//!   the expert with the least internal contradiction — tightest local resolution.
//! - **Raumgewinn** (territory / influence) ≈ [`WinnerCriterion::SupportSpread`]:
//!   the expert whose top-K support atoms cover the most distinct ground.
//!
//! ## Separation of concerns — select, never duplicate
//! [`Head2Head::select`] is a pure read + arg-extremum over the *existing*
//! [`Blackboard`] entries (`confidence` / `dissonance` / `support` are already
//! there). It stores no new identity and copies nothing — an identity is
//! *pointed at* ([`ExpertId`]), never re-materialized (`I-VSA-IDENTITIES`). The
//! winner is a *decision over state*, not a new copy of it.
//!
//! Zero dependencies. Pure data + one method on the selector carrier.

// Pedantic carve-outs (the repo does not deny pedantic; these are in-context false
// positives): cast_precision_loss — the support count n ∈ 0..=4 is exact as f32;
// float_cmp — tests compare exact integer-valued scores; missing_const_for_fn —
// `score` transitively calls the non-const `slice::contains`.
#![allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::missing_const_for_fn
)]

use crate::a2a_blackboard::{Blackboard, BlackboardEntry, ExpertId};

/// How to pick the winner among competing experts. Each maps a
/// [`BlackboardEntry`] to a scalar score; the highest score wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WinnerCriterion {
    /// Lowest dissonance wins — the **infight** reading: least internal
    /// contradiction (score = `1 - dissonance`).
    DissonanceMin,
    /// Highest self-reported confidence wins (score = `confidence`).
    ConfidenceMax,
    /// Widest distinct support wins — the **Raumgewinn**/territory reading
    /// (score = count of distinct non-zero atoms in `support[4]`).
    SupportSpread,
    /// Confidence tempered by dissonance: `confidence * (1 - dissonance)`.
    /// The default — rewards certainty that isn't bought with contradiction.
    #[default]
    Tempered,
}

/// The outcome of a head2head: who won, by how much, under which criterion.
///
/// `margin` is the winner's lead over the best *other* expert
/// (`winner_score - runner_up_score`); with no distinct runner-up it is the
/// winner's uncontested score (lead over the `0.0` floor). A small `margin` is
/// the dark-horse signal — the wave was nearly a coin-flip, so the deterministic
/// particle chain should confirm before the winner's spiral is trusted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompetitionOutcome {
    /// The expert whose top-scoring bid won.
    pub winner: ExpertId,
    /// The winner's score under `criterion`.
    pub winner_score: f32,
    /// The best *other* expert, if any competed.
    pub runner_up: Option<ExpertId>,
    /// `winner_score - runner_up_score` (lead over the `0.0` floor if uncontested).
    pub margin: f32,
    /// The criterion the board judged by.
    pub criterion: WinnerCriterion,
}

/// The board judge: holds the [`WinnerCriterion`] and scores competitors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Head2Head {
    /// How this judge scores each bid.
    pub criterion: WinnerCriterion,
}

impl Head2Head {
    /// A judge with an explicit criterion.
    #[must_use]
    pub fn new(criterion: WinnerCriterion) -> Self {
        Self { criterion }
    }

    /// Score one bid under this judge's criterion (higher = better).
    #[must_use]
    pub fn score(&self, e: &BlackboardEntry) -> f32 {
        match self.criterion {
            WinnerCriterion::DissonanceMin => 1.0 - e.dissonance,
            WinnerCriterion::ConfidenceMax => e.confidence,
            WinnerCriterion::SupportSpread => distinct_support(e.support) as f32,
            WinnerCriterion::Tempered => e.confidence * (1.0 - e.dissonance),
        }
    }

    /// Select the winning expert over the blackboard's bids, or `None` if the
    /// board is empty. Each entry is a *bid*; the expert of the top-scoring bid
    /// wins, and the runner-up is the top bid from any *other* expert.
    #[must_use]
    pub fn select(&self, bb: &Blackboard) -> Option<CompetitionOutcome> {
        let best = bb
            .entries
            .iter()
            .max_by(|a, b| self.score(a).total_cmp(&self.score(b)))?;
        let winner = best.expert_id;
        let winner_score = self.score(best);

        let runner = bb
            .entries
            .iter()
            .filter(|e| e.expert_id != winner)
            .max_by(|a, b| self.score(a).total_cmp(&self.score(b)));
        let (runner_up, runner_score) =
            runner.map_or((None, 0.0), |r| (Some(r.expert_id), self.score(r)));

        Some(CompetitionOutcome {
            winner,
            winner_score,
            runner_up,
            margin: winner_score - runner_score,
            criterion: self.criterion,
        })
    }
}

/// Count of distinct non-zero atoms in a 4-slot support vector (the territory
/// measure for [`WinnerCriterion::SupportSpread`]). `0` is the no-atom sentinel.
fn distinct_support(support: [u16; 4]) -> usize {
    let mut seen = [0u16; 4];
    let mut n = 0;
    for a in support {
        if a != 0 && !seen[..n].contains(&a) {
            seen[n] = a;
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a_blackboard::ExpertCapability;

    fn entry(id: ExpertId, confidence: f32, dissonance: f32, support: [u16; 4]) -> BlackboardEntry {
        BlackboardEntry {
            expert_id: id,
            capability: ExpertCapability::ReasoningTopology,
            result: 0,
            confidence,
            support,
            dissonance,
            cost_us: 0,
        }
    }

    fn board(es: Vec<BlackboardEntry>) -> Blackboard {
        Blackboard {
            entries: es,
            round: 0,
        }
    }

    #[test]
    fn tempered_default_rewards_certainty_without_contradiction() {
        // A: high confidence but high dissonance (0.9 * 0.4 = 0.36).
        // B: lower confidence but clean         (0.7 * 0.9 = 0.63 wins).
        let bb = board(vec![entry(1, 0.9, 0.6, [0; 4]), entry(2, 0.7, 0.1, [0; 4])]);
        let out = Head2Head::default().select(&bb).unwrap();
        assert_eq!(out.criterion, WinnerCriterion::Tempered);
        assert_eq!(out.winner, 2);
        assert_eq!(out.runner_up, Some(1));
        assert!(out.margin > 0.0);
    }

    #[test]
    fn dissonance_min_is_the_infight_pick() {
        // Same confidence; the tighter (lower-dissonance) expert wins the infight.
        let bb = board(vec![entry(1, 0.8, 0.5, [0; 4]), entry(2, 0.8, 0.2, [0; 4])]);
        let out = Head2Head::new(WinnerCriterion::DissonanceMin)
            .select(&bb)
            .unwrap();
        assert_eq!(out.winner, 2);
    }

    #[test]
    fn support_spread_is_the_raumgewinn_pick() {
        // Same confidence/dissonance; the expert covering more distinct ground wins.
        let bb = board(vec![
            entry(1, 0.8, 0.1, [42, 42, 0, 0]), // 1 distinct atom
            entry(2, 0.8, 0.1, [7, 9, 13, 21]), // 4 distinct atoms (territory)
        ]);
        let out = Head2Head::new(WinnerCriterion::SupportSpread)
            .select(&bb)
            .unwrap();
        assert_eq!(out.winner, 2);
        assert_eq!(out.winner_score, 4.0);
        assert_eq!(out.margin, 3.0); // 4 distinct vs 1 distinct
    }

    #[test]
    fn confidence_max_ignores_dissonance() {
        let bb = board(vec![
            entry(1, 0.95, 0.9, [0; 4]), // noisy but loud → wins on raw confidence
            entry(2, 0.6, 0.0, [0; 4]),
        ]);
        let out = Head2Head::new(WinnerCriterion::ConfidenceMax)
            .select(&bb)
            .unwrap();
        assert_eq!(out.winner, 1);
    }

    #[test]
    fn uncontested_single_expert_has_no_runner_up() {
        let bb = board(vec![entry(7, 0.8, 0.2, [0; 4])]);
        let out = Head2Head::default().select(&bb).unwrap();
        assert_eq!(out.winner, 7);
        assert_eq!(out.runner_up, None);
        assert_eq!(out.margin, out.winner_score); // lead over the 0.0 floor
    }

    #[test]
    fn multiple_bids_per_expert_use_the_experts_best() {
        // Expert 1 posts twice; its best bid represents it; runner-up is expert 2.
        let bb = board(vec![
            entry(1, 0.3, 0.0, [0; 4]),
            entry(1, 0.9, 0.0, [0; 4]), // expert 1's best
            entry(2, 0.5, 0.0, [0; 4]),
        ]);
        let out = Head2Head::new(WinnerCriterion::ConfidenceMax)
            .select(&bb)
            .unwrap();
        assert_eq!(out.winner, 1);
        assert_eq!(out.runner_up, Some(2));
    }

    #[test]
    fn empty_board_has_no_winner() {
        assert!(Head2Head::default().select(&board(vec![])).is_none());
    }
}
