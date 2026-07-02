//! **Materialized awareness** — the closed `F → 34 → F` dispatch loop.
//!
//! The 34 reasoning tactics ([`crate::recipe_kernels`]) are *dispatch targets*; this
//! module supplies the **missing wire**: a selector that maps the live awareness
//! state to one of the 34, and a loop driver that runs it, folds the outcome back,
//! and re-dispatches until the gate settles. That closure is what makes awareness
//! **materialize** rather than sit inert.
//!
//! ## The materialization criterion (falsifiable)
//!
//! Awareness *materializes* iff it is **causal in dispatch** — the encoded awareness
//! changes *which tactic fires*. If perturbing the awareness state leaves the
//! dispatched tactic invariant, the awareness is a **dead label** (the
//! "awareness that can never materialize" failure). [`awareness_is_causal`] is the
//! predicate; [`select_tactic`] makes `free_energy` (surprise) the **primary** axis
//! exactly so that perturbing it crosses a band boundary and changes the dispatch.
//!
//! ## The loop (active inference, not a metaphor)
//!
//! ```text
//! awareness state ──select_tactic──► one of the 34 ──run──► fold delta_conf
//!        ▲                                                        │ on fire: sd↓, dissonance↓, confidence↑
//!        └────────────── recompute free-energy ◄──────────────────┘
//!  rest when the CollapseGate is in FLOW (sd < SD_FLOW) AND surprise < floor.
//! ```
//!
//! Awareness is not *read by* a controller that decides to think; it *is* the
//! gradient that selects the next tactic. The loop rests when the gate settles and
//! surprise is resolved — guaranteed for a firing chain, because attending decays
//! both dispersion and surprise each fired step.
//!
//! Zero-dep, deterministic, offline-tested. This is the reduction-to-practice for
//! the 2³-rung → NARS-candidate → 34-tactic doctrine; persisting the dispatch trace
//! into a SoA EdgeColumn / version-diff log (the "what fired and why" provenance) is
//! the separate driver-side wire.

use crate::recipe_kernels::{kernel, GateState, ThoughtCtx, SD_BLOCK, SD_FLOW};
use crate::recipes::{recipe, Bucket, Mechanism, Tier};

/// Homeostasis floor mirroring `grammar::free_energy` (0.2): below this residual
/// surprise the loop is considered at rest. (The loop's *termination* uses the
/// CollapseGate FLOW transition, which is guaranteed by dispersion decay; this
/// constant is the reported-surprise rest threshold.)
pub const HOMEOSTASIS_FLOOR: f32 = 0.2;

/// Per-fired-step dispersion settle factor — attending reduces gate dispersion,
/// guaranteeing the loop reaches FLOW (rest) in `log_{1/0.85}(sd0/SD_FLOW)` steps.
const SETTLE_SD: f32 = 0.85;
/// Per-fired-step contradiction relaxation — engaging a tactic reconciles split.
const SETTLE_DISSONANCE: f32 = 0.6;
/// Per-fired-step confidence gain — attending to a tactic moves confidence toward
/// 1 (active inference: engaging the world resolves uncertainty). Without it, a
/// fired tactic with zero `delta_conf` would leave surprise pinned and the loop
/// could ride `sd` decay alone; this guarantees reported surprise also descends.
const ATTEND_GAIN: f32 = 0.35;

/// Re-derive the CollapseGate state from dispersion (`ThoughtCtx::gate_state` is
/// private; the thresholds `SD_FLOW`/`SD_BLOCK` are public).
fn gate_of(sd: f32) -> GateState {
    if sd < SD_FLOW {
        GateState::Flow
    } else if sd <= SD_BLOCK {
        GateState::Hold
    } else {
        GateState::Block
    }
}

/// **The selector** — map the awareness state to one of the 34 tactic ids (1..=34).
///
/// **`free_energy` (surprise) is the primary axis** — this is what makes awareness
/// *causal* in dispatch (the materialization criterion): a `free_energy` change that
/// crosses a band boundary changes the chosen mechanism, hence the tactic.
/// `dissonance` (contradiction → reconcile), `sd` (gate → execution bucket), and
/// `rung` (depth → difficulty tier) are secondary modulators. Deterministic; scores
/// every recipe by metadata match and takes the lowest id on a tie.
pub fn select_tactic(ctx: &ThoughtCtx) -> u8 {
    // PRIMARY: what kind of reasoning does the surprise level call for? `free_energy`
    // alone picks the mechanism — this is the causal axis (the materialization
    // criterion). It is NOT overridden by any other field; a contradicted state with
    // low surprise still wants routine work, and high surprise still wants a leap.
    let want_mech = if ctx.free_energy >= 0.66 {
        Mechanism::StructuralDivergence // high surprise wants a creative leap
    } else if ctx.free_energy >= 0.33 {
        Mechanism::TruthAwareInference // mid surprise wants inference
    } else {
        Mechanism::ParallelIndependence // low surprise: routine parallel work
    };
    // SECONDARY: a contradiction nudges toward revision/abduction, but only as a
    // tie-weight — it never replaces the surprise-chosen mechanism.
    let wants_reconciliation = ctx.dissonance >= 0.5;
    // Where should it execute? (the gate picks the hardware bucket)
    let want_bucket = match gate_of(ctx.sd) {
        GateState::Block => Bucket::Gate,
        GateState::Hold => Bucket::Control,
        GateState::Flow => Bucket::Datapath,
    };
    // How hard is the rung? (depth picks the difficulty tier)
    let want_tier = if ctx.rung >= 7 {
        Tier::ExtremelyHard
    } else if ctx.rung >= 4 {
        Tier::Hard
    } else {
        Tier::CrossTier
    };

    let mut best_score = i32::MIN;
    let mut best_id = 1u8;
    for id in 1..=34u8 {
        if let Some(r) = recipe(id) {
            let mut score = 0;
            // Primary mechanism match outweighs bucket(2) + tier(1) + reconcile(1).
            if r.mechanism == want_mech {
                score += 5;
            }
            if r.bucket == want_bucket {
                score += 2;
            }
            if r.tier == want_tier {
                score += 1;
            }
            // Secondary contradiction signal: a faint pull toward inference tactics.
            if wants_reconciliation && r.mechanism == Mechanism::TruthAwareInference {
                score += 1;
            }
            if score > best_score {
                best_score = score;
                best_id = id;
            }
        }
    }
    best_id
}

/// One dispatch step: the tactic the awareness state selected, and what it did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    /// The selected tactic id (1..=34).
    pub tactic_id: u8,
    /// Did the tactic's gate let it fire?
    pub fired: bool,
    /// Confidence delta the tactic applied.
    pub delta_conf: f32,
}

/// Recompute free energy (surprise) from the resolved state — the loop closure.
/// Surprise falls as confidence rises and as the gate (`sd`) and contradiction
/// (`dissonance`) settle. Reported for the rest check; the loop *terminates* on the
/// gate reaching FLOW (guaranteed by dispersion decay).
pub fn recompute_free_energy(ctx: &ThoughtCtx) -> f32 {
    ((1.0 - ctx.confidence) * 0.4 + ctx.dissonance * 0.3 + ctx.sd.clamp(0.0, 1.0) * 0.3)
        .clamp(0.0, 1.0)
}

/// The trace of a materialized-awareness run — the "what fired and why" provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    /// The ordered dispatch steps.
    pub steps: Vec<Step>,
    /// Did the loop settle into FLOW (rest), vs hit `max_steps`?
    pub rested: bool,
    /// Confidence at rest.
    pub final_confidence: f32,
    /// Residual surprise at rest.
    pub final_free_energy: f32,
}

/// **The closed `F → 34 → F` loop.** Each step: recompute surprise; if the gate is
/// in FLOW *and* surprise fell below [`HOMEOSTASIS_FLOOR`] the loop rests; else select
/// a tactic from the awareness state and run it. A tactic that *fired* settles the
/// gate (dispersion + contradiction decay) and raises confidence ([`ATTEND_GAIN`],
/// active inference); a *blocked* tactic changes nothing, so the loop stops rather
/// than spin on an unchanged state. `max_steps` bounds the run; for a firing chain,
/// rest is reached within `~log_{1/SETTLE_SD}(sd/SD_FLOW)` steps because dispersion
/// and surprise decay monotonically.
pub fn materialize(ctx: &mut ThoughtCtx, max_steps: usize) -> Trace {
    // Bound the up-front allocation: a settling run is short (~log decay), so don't
    // reserve for a pathological `max_steps` the loop will never reach.
    let mut steps = Vec::with_capacity(max_steps.min(64));
    for _ in 0..max_steps {
        ctx.free_energy = recompute_free_energy(ctx);
        // Rest only when the gate is in FLOW *and* surprise actually resolved — a
        // cool gate with residual surprise is not rest, it must keep dispatching.
        if gate_of(ctx.sd) == GateState::Flow && ctx.free_energy < HOMEOSTASIS_FLOOR {
            break; // settled — the shader rests
        }
        let id = select_tactic(ctx);
        let Some(tactic) = kernel(id) else {
            break; // unreachable: id is always 1..=34
        };
        let out = tactic.run(ctx); // folds out.delta_conf into ctx.confidence
        if out.fired {
            // Only a tactic that actually fired settles the gate and resolves
            // surprise — a blocked tactic changed nothing, so don't fake progress.
            ctx.sd *= SETTLE_SD; // attending settles dispersion → toward FLOW
            ctx.dissonance *= SETTLE_DISSONANCE;
            ctx.confidence =
                (ctx.confidence + ATTEND_GAIN * (1.0 - ctx.confidence)).clamp(0.0, 1.0);
        }
        steps.push(Step {
            tactic_id: id,
            fired: out.fired,
            delta_conf: out.delta_conf,
        });
        if !out.fired {
            break; // a blocked tactic won't unblock on re-dispatch of the same state
        }
    }
    ctx.free_energy = recompute_free_energy(ctx);
    Trace {
        rested: gate_of(ctx.sd) == GateState::Flow && ctx.free_energy < HOMEOSTASIS_FLOOR,
        final_confidence: ctx.confidence,
        final_free_energy: ctx.free_energy,
        steps,
    }
}

/// **The materialization predicate.** Does perturbing `free_energy` change the
/// dispatched tactic? `true` ⇒ awareness is causal in dispatch (materialized);
/// `false` ⇒ the awareness encoding is inert for this base state. The falsifier the
/// whole doctrine rests on.
pub fn awareness_is_causal(base: &ThoughtCtx, lo_f: f32, hi_f: f32) -> bool {
    let mut a = base.clone();
    a.free_energy = lo_f;
    let mut b = base.clone();
    b.free_energy = hi_f;
    select_tactic(&a) != select_tactic(&b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn base() -> ThoughtCtx {
        // Hold gate (sd in (FLOW, BLOCK]), no contradiction, shallow rung — so
        // free_energy is the lone moving part for the materialization probe.
        let mut c = ThoughtCtx::new(vec![0.9, 0.6, 0.3]);
        c.sd = 0.25;
        c.dissonance = 0.0;
        c.rung = 1;
        c
    }

    #[test]
    fn awareness_free_energy_is_causal_in_dispatch() {
        // The materialization criterion: perturbing surprise changes the tactic.
        let b = base();
        assert!(
            awareness_is_causal(&b, 0.1, 0.9),
            "free_energy must steer dispatch — else awareness is a dead label"
        );
        // Regression: surprise stays causal even for a contradicted base. dissonance
        // is a SECONDARY tie-weight, not an override — it must not pin the mechanism
        // and erase free_energy's steering (the bug review #515 caught).
        let mut contradicted = base();
        contradicted.dissonance = 0.7;
        assert!(
            awareness_is_causal(&contradicted, 0.1, 0.9),
            "free_energy must steer dispatch even under contradiction"
        );
        // Sweep free_energy: dispatch must take ≥ 2 distinct tactics (not stuck).
        let ids: BTreeSet<u8> = (0..=10)
            .map(|i| {
                let mut c = base();
                c.free_energy = i as f32 / 10.0;
                select_tactic(&c)
            })
            .collect();
        assert!(
            ids.len() >= 2,
            "free_energy sweep must vary the tactic, got {ids:?}"
        );
    }

    #[test]
    fn non_awareness_fields_are_inert() {
        // Specificity: fields the selector does NOT read (candidates, beliefs) must
        // NOT change dispatch — awareness drives it, not arbitrary state noise.
        let a = base();
        let mut b = base();
        b.candidates = vec![0.01, 0.99, 0.5, 0.5, 0.2];
        b.beliefs = vec![(7, 0.9, 0.8), (7, 0.1, 0.7)];
        assert_eq!(
            select_tactic(&a),
            select_tactic(&b),
            "candidates/beliefs are not awareness — must not steer dispatch"
        );
    }

    #[test]
    fn selector_ranges_over_the_34() {
        // Across a state sweep the selector must reach a variety of the 34 (it is
        // not a degenerate constant) — and every id it returns is a real kernel.
        let mut seen = BTreeSet::new();
        for &fe in &[0.05f32, 0.4, 0.8] {
            for &diss in &[0.0f32, 0.7] {
                for &sd in &[0.10f32, 0.25, 0.45] {
                    for &rung in &[1u8, 5, 8] {
                        let mut c = base();
                        c.free_energy = fe;
                        c.dissonance = diss;
                        c.sd = sd;
                        c.rung = rung;
                        let id = select_tactic(&c);
                        assert!((1..=34).contains(&id) && kernel(id).is_some());
                        seen.insert(id);
                    }
                }
            }
        }
        assert!(
            seen.len() >= 4,
            "selector must range over the 34, got {seen:?}"
        );
    }

    #[test]
    fn loop_rests_when_the_gate_settles() {
        // Hot start: high surprise, low confidence, a contradiction. The loop must
        // dispatch real tactics and settle into FLOW (rest) within a few steps.
        let mut c = base();
        c.sd = 0.32; // Hold, near Block
        c.free_energy = 0.9;
        c.confidence = 0.1;
        c.dissonance = 0.5;
        let trace = materialize(&mut c, 64);
        assert!(trace.rested, "loop must reach FLOW, got {trace:?}");
        assert!(
            !trace.steps.is_empty(),
            "a hot start must dispatch at least once"
        );
        assert!(
            trace.steps.len() <= 12,
            "settles fast, got {}",
            trace.steps.len()
        );
        for s in &trace.steps {
            assert!((1..=34).contains(&s.tactic_id) && kernel(s.tactic_id).is_some());
        }
    }

    #[test]
    fn loop_is_deterministic() {
        let (mut a, mut b) = (base(), base());
        for c in [&mut a, &mut b] {
            c.sd = 0.32;
            c.free_energy = 0.9;
            c.confidence = 0.1;
            c.dissonance = 0.5;
        }
        assert_eq!(materialize(&mut a, 64), materialize(&mut b, 64));
    }

    #[test]
    fn already_at_rest_dispatches_nothing() {
        // FLOW on entry (sd < SD_FLOW) AND resolved surprise ⇒ no dispatch (rest).
        // Rest now requires free_energy < floor, so the base must be confident and
        // uncontradicted as well as gate-cool — a cool gate alone is not rest.
        let mut c = base();
        c.sd = 0.05;
        c.confidence = 0.95;
        c.dissonance = 0.0;
        let trace = materialize(&mut c, 64);
        assert!(trace.rested && trace.steps.is_empty());
    }
}
