//! Active-inference free-energy formulation of grammar parsing.
//!
//! A parse attempt produces observations (tokens); the hidden state is
//! `(SPO triple, TEKAMOLO slots, Pearl 2³ causal mask, Markov context)`.
//! Free energy `F = -log P(obs | hidden) + KL(q || p)` decomposes here as:
//!
//! - **Likelihood term** — per-role recovery confidence after `RoleKey::unbind`.
//!   High recovery = content the caller committed to a role was cleanly
//!   recoverable from the bundle = observations are well-explained by the
//!   hypothesized hidden state.
//! - **KL term** — divergence of the style's runtime awareness from the
//!   YAML prior. High divergence = the style is acting against what the
//!   YAML said was likely.
//!
//! **Homeostasis** means driving F below a floor and staying there; a
//! Wechsel or morphological ambiguity spikes F, and the system resolves
//! by sampling counterfactual hypotheses and committing the argmin_F.
//!
//! **Epiphany** is the saddle-point case: two hypotheses at comparable
//! low F. Rather than picking one and discarding the other, both commit
//! with a `Contradiction` marker (handled by AriGraph / triplet_graph in
//! downstream crates, not here — this layer only produces the `Resolution`).

use super::ticket::FailureTicket;

/// Free-energy ceiling: above this, no hypothesis is acceptable — escalate.
/// Calibrated empirically; adjust once Animal Farm benchmark runs report.
pub const FAILURE_CEILING: f32 = 0.8;

/// Free-energy floor: below this, the top hypothesis commits directly.
pub const HOMEOSTASIS_FLOOR: f32 = 0.2;

/// Margin between top-2 hypotheses below which we read them as co-valid
/// (epiphany), not a decisive win.
pub const EPIPHANY_MARGIN: f32 = 0.05;

// ---------------------------------------------------------------------------
// Hypothesis
// ---------------------------------------------------------------------------

/// A candidate interpretation of an ambiguous parse. Role fillers are
/// role-indexed — the carrier is identified by its `RoleKey` (stored as
/// the label rather than a pointer, so the struct is clonable without
/// lifetime entanglement).
///
/// This is the zero-dep contract form; downstream crates that actually
/// bind / unbind content will wrap it with a 10K VSA carrier at the
/// trajectory level.
#[derive(Debug, Clone, PartialEq)]
pub struct Hypothesis {
    /// Human-readable summary for ticket / diagnostic output.
    pub label: String,
    /// Pearl 2³ causal mask committed by this hypothesis.
    /// Morphology-committed bits narrow the basin (see plan D7 §2³→2^N).
    pub causal_mask: u8,
    /// Named role fillers: (role_label, filler_label). Order-independent
    /// — role_label is what `RoleKey::label` returns.
    pub role_fillers: Vec<(&'static str, String)>,
}

impl Hypothesis {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            causal_mask: 0,
            role_fillers: Vec::new(),
        }
    }

    pub fn with_mask(mut self, mask: u8) -> Self {
        self.causal_mask = mask;
        self
    }

    pub fn fill(mut self, role_label: &'static str, filler: impl Into<String>) -> Self {
        self.role_fillers.push((role_label, filler.into()));
        self
    }

    /// Count of causal-mask bits this hypothesis commits. Higher = basin
    /// more collapsed by morphology = less counterfactual space left.
    pub fn committed_bit_count(&self) -> u32 {
        self.causal_mask.count_ones()
    }
}

// ---------------------------------------------------------------------------
// FreeEnergy
// ---------------------------------------------------------------------------

/// Decomposed free energy of a hypothesis against the observed Markov
/// trajectory + the style's awareness state.
///
/// - `likelihood ∈ [0, 1]` — mean role recovery margin (higher = better fit)
/// - `kl_divergence ∈ [0, 1]` — awareness drift from prior (higher = style contradicts its own baseline)
/// - `total = 1 - likelihood + kl_divergence` — lower is better
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeEnergy {
    pub likelihood: f32,
    pub kl_divergence: f32,
    pub total: f32,
}

impl FreeEnergy {
    /// Compose from the two decomposed terms. `likelihood` should be in
    /// `[0, 1]`; `kl_divergence` should be in `[0, ∞)` but typically stays
    /// in `[0, 1]`. `total = (1 - likelihood) + kl_divergence`.
    pub fn compose(likelihood: f32, kl_divergence: f32) -> Self {
        let likelihood = likelihood.clamp(0.0, 1.0);
        let kl_divergence = kl_divergence.max(0.0);
        Self {
            likelihood,
            kl_divergence,
            total: (1.0 - likelihood) + kl_divergence,
        }
    }

    /// True when total is under the homeostasis floor — hypothesis
    /// is committable outright.
    pub fn is_homeostatic(&self) -> bool {
        self.total < HOMEOSTASIS_FLOOR
    }

    /// True when total is above the failure ceiling — hypothesis
    /// should trigger a FailureTicket (escalate to LLM).
    pub fn is_catastrophic(&self) -> bool {
        self.total > FAILURE_CEILING
    }
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// Output of `Trajectory::resolve` or equivalent active-inference step.
/// The three variants carry the caller through the three branches of the
/// F-landscape:
///
/// - **Commit** — single clear winner. Caller writes one triple to AriGraph.
/// - **Epiphany** — two hypotheses at comparable low F. Both commit, with
///   a Contradiction marker. This is the narrative-unreliability path
///   (Orwell's Squealer revising the windmill cause; Snowball's role
///   flip-flop).
/// - **FailureTicket** — no acceptable hypothesis. Escalate to LLM with a
///   structured ticket.
#[derive(Debug, Clone)]
pub enum Resolution {
    Commit {
        hypothesis: Hypothesis,
        free_energy: FreeEnergy,
    },
    Epiphany {
        winner: Hypothesis,
        loser: Hypothesis,
        margin: f32,
        free_energy_winner: FreeEnergy,
        free_energy_loser: FreeEnergy,
    },
    FailureTicket(FailureTicket),
}

impl Resolution {
    /// Classify a ranked hypothesis list into a Resolution. Callers
    /// compute F per hypothesis upstream and pass the ranked list
    /// `(hypothesis, free_energy)` sorted ascending by `free_energy.total`.
    ///
    /// Rules (read top-down, first match wins):
    ///
    /// 1. No hypotheses → FailureTicket (caller supplies).
    /// 2. Top hypothesis catastrophic (F > FAILURE_CEILING) → FailureTicket.
    /// 3. Two hypotheses within EPIPHANY_MARGIN, both under FAILURE_CEILING
    ///    → Epiphany.
    /// 4. Top hypothesis homeostatic (F < HOMEOSTASIS_FLOOR) → Commit.
    /// 5. Anything else → Commit (the best we've got, even if it's not
    ///    truly homeostatic; caller can mark low-confidence).
    pub fn from_ranked(
        ranked: &[(Hypothesis, FreeEnergy)],
        failure_ticket_factory: impl FnOnce() -> FailureTicket,
    ) -> Self {
        if ranked.is_empty() {
            return Resolution::FailureTicket(failure_ticket_factory());
        }
        let (top_h, top_fe) = &ranked[0];
        if top_fe.is_catastrophic() {
            return Resolution::FailureTicket(failure_ticket_factory());
        }
        if let Some((second_h, second_fe)) = ranked.get(1) {
            let margin = second_fe.total - top_fe.total;
            if margin.abs() < EPIPHANY_MARGIN && !second_fe.is_catastrophic() {
                return Resolution::Epiphany {
                    winner: top_h.clone(),
                    loser: second_h.clone(),
                    margin,
                    free_energy_winner: *top_fe,
                    free_energy_loser: *second_fe,
                };
            }
        }
        Resolution::Commit {
            hypothesis: top_h.clone(),
            free_energy: *top_fe,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::{inference::NarsInference, tekamolo::TekamoloSlots, ticket::PartialParse};

    fn dummy_ticket() -> FailureTicket {
        FailureTicket {
            partial_parse: PartialParse {
                resolved_tokens: vec![],
                unresolved_tokens: vec![],
                coverage: 0.0,
            },
            attempted_inference: NarsInference::Deduction,
            recommended_next: NarsInference::Abduction,
            causal_ambiguity: None,
            tekamolo: TekamoloSlots::default(),
            wechsel: vec![],
            coverage: 0.0,
            missing_required: vec![],
        }
    }

    #[test]
    fn free_energy_composition_bounds() {
        // likelihood = 1, kl = 0 → total = 0 (best possible).
        let f = FreeEnergy::compose(1.0, 0.0);
        assert!((f.total - 0.0).abs() < 1e-6);
        assert!(f.is_homeostatic());
        assert!(!f.is_catastrophic());

        // likelihood = 0, kl = 1 → total = 2 (worst).
        let f = FreeEnergy::compose(0.0, 1.0);
        assert!((f.total - 2.0).abs() < 1e-6);
        assert!(!f.is_homeostatic());
        assert!(f.is_catastrophic());

        // Clamping: negative likelihood → treated as 0.
        let f = FreeEnergy::compose(-0.5, 0.0);
        assert_eq!(f.likelihood, 0.0);
    }

    #[test]
    fn hypothesis_builder_pattern() {
        let h = Hypothesis::new("Napoleon announced deal")
            .with_mask(0b0000_0011)
            .fill("SUBJECT", "Napoleon")
            .fill("OBJECT", "deal");
        assert_eq!(h.label, "Napoleon announced deal");
        assert_eq!(h.causal_mask, 0x03);
        assert_eq!(h.role_fillers.len(), 2);
        assert_eq!(h.committed_bit_count(), 2);
    }

    #[test]
    fn resolve_empty_returns_failure_ticket() {
        let r = Resolution::from_ranked(&[], dummy_ticket);
        assert!(matches!(r, Resolution::FailureTicket(_)));
    }

    #[test]
    fn resolve_commits_clear_winner() {
        let h = Hypothesis::new("clear");
        let fe = FreeEnergy::compose(0.95, 0.02);
        let ranked = vec![(h.clone(), fe)];
        let r = Resolution::from_ranked(&ranked, dummy_ticket);
        match r {
            Resolution::Commit {
                hypothesis,
                free_energy,
            } => {
                assert_eq!(hypothesis.label, "clear");
                assert!(free_energy.is_homeostatic());
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }

    #[test]
    fn resolve_epiphany_on_tight_margin() {
        // Two hypotheses within EPIPHANY_MARGIN — both commit.
        let h1 = Hypothesis::new("literal windmill fell");
        let h2 = Hypothesis::new("Snowball sabotaged windmill");
        let fe1 = FreeEnergy::compose(0.9, 0.05); // total = 0.15
        let fe2 = FreeEnergy::compose(0.89, 0.05); // total = 0.16
        let ranked = vec![(h1.clone(), fe1), (h2.clone(), fe2)];
        let r = Resolution::from_ranked(&ranked, dummy_ticket);
        match r {
            Resolution::Epiphany {
                winner,
                loser,
                margin,
                ..
            } => {
                assert_eq!(winner.label, "literal windmill fell");
                assert_eq!(loser.label, "Snowball sabotaged windmill");
                assert!(margin.abs() < EPIPHANY_MARGIN);
            }
            other => panic!("expected Epiphany, got {other:?}"),
        }
    }

    #[test]
    fn resolve_failure_ticket_on_catastrophic_top() {
        let h = Hypothesis::new("nothing fits");
        let fe = FreeEnergy::compose(0.1, 0.0); // total = 0.9 — catastrophic
        let ranked = vec![(h, fe)];
        let r = Resolution::from_ranked(&ranked, dummy_ticket);
        assert!(matches!(r, Resolution::FailureTicket(_)));
    }

    #[test]
    fn resolve_commits_mid_band_best_available() {
        // F = 0.4 — not homeostatic (> 0.2), not catastrophic (< 0.8) — commit best available.
        let h = Hypothesis::new("mid-band");
        let fe = FreeEnergy::compose(0.6, 0.0); // total = 0.4
        let ranked = vec![(h.clone(), fe)];
        let r = Resolution::from_ranked(&ranked, dummy_ticket);
        match r {
            Resolution::Commit {
                hypothesis,
                free_energy,
            } => {
                assert_eq!(hypothesis.label, "mid-band");
                assert!(!free_energy.is_homeostatic());
                assert!(!free_energy.is_catastrophic());
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }

    #[test]
    fn resolve_rejects_epiphany_when_second_catastrophic() {
        // Top is clean; second has margin < EPIPHANY_MARGIN but second is
        // catastrophic → commit only the top, no epiphany.
        let h1 = Hypothesis::new("top");
        let h2 = Hypothesis::new("second");
        let fe1 = FreeEnergy::compose(0.2, 0.0); // total = 0.8 — exactly at ceiling, not homeostatic
        let fe2 = FreeEnergy::compose(0.18, 0.0); // total = 0.82 — catastrophic (> 0.8)
        let ranked = vec![(h1.clone(), fe1), (h2, fe2)];
        let r = Resolution::from_ranked(&ranked, dummy_ticket);
        match r {
            Resolution::Commit { hypothesis, .. } => {
                assert_eq!(hypothesis.label, "top");
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }
}
