//! Escalation + epiphany loop — the boring checklist, grounded on our SoA.
//!
//! D-PERSONA-1 of `rung-persona-orchestration-v1`. This is the *restore* of
//! ladybug's qualia loop (`felt_parse` collapse-hint + `InnerCouncil` /
//! `HdrResonance` split + `EpiphanyDetector`) onto **our** contract types —
//! NOT a bespoke verifier. The checklist that boots an agent is not a list of
//! ad-hoc asserts; each item is *verified by the escalation loop itself*:
//!
//! ```text
//!   felt_parse → CollapseHint {Flow | Fanout | RungElevate}
//!       Fanout      = gather more (escalate breadth)
//!       RungElevate = deepen (rung-shift)
//!       Flow        = settled / done
//!   InnerCouncil.deliberate (Guardian / Catalyst / Balanced, majority vote)
//!       + HdrResonance split-amplify: a split (one archetype sees what the
//!         others don't, is_split(0.7, 0.5)) is amplified ×1.2 — disagreement
//!         IS the learning signal (perspectives disagree about a projection
//!         ⇒ a spurious S–O is screened off).
//!   EpiphanyDetector.observe: Some(Epiphany) iff
//!         similarity > baseline × 1.5  ∧  recent_samples ≥ 4
//!         (the window ≥ 4 is the anti-Mount-Stupid evidence guard).
//!   green-flip = the item settles to Flow AND an epiphany fires →
//!         a WisdomMarker (Epiphany / Wisdom ghost) — persistent qualia
//!         residue that decays asymptotically to 0.1, never to zero.
//! ```
//!
//! The list completes (the meta-recipe composes — D-PERSONA-2) when every
//! item's collapse-hint settles to [`CollapseHint::Flow`]. Items split HARD
//! (must be green to boot) vs SOFT (degrade gracefully if red — anytime). A
//! green item going red at runtime is a let-it-crash event: re-escalate
//! ([`Checklist::mark_red`]) — the checklist items ARE the supervision
//! health-checks, continuous and not one-time.
//!
//! Zero-dep: the council takes raw scalar signals (trust / humility / flow /
//! load), so both this crate's [`crate::mul::MulAssessment`] and the planner's
//! richer assessment can drive it without a type dependency.

// ═══════════════════════════════════════════════════════════════════════════
// CollapseHint — the felt_parse escalation decision (already produced)
// ═══════════════════════════════════════════════════════════════════════════

/// The collapse hint `felt_parse` emits for one item. The escalation decision
/// is *already produced* here ("the list as escalation work").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollapseHint {
    /// Settled — no further work; the item closes on a green-flip.
    Flow,
    /// Gather more — escalate breadth (the suspect-bridge fan-out width is
    /// driven by `bridgeness`; see [`fanout_width`]).
    Fanout,
    /// Deepen — rung-shift to a higher reasoning level (see [`rung_delta`]).
    RungElevate,
}

/// Fan-out width, grounded in ladybug `detector.rs`:
/// `fanout = base · (1 + bridgeness · 0.5)`, clamped to `[1, 30]`.
/// `bridgeness` is the suspect-bridge centrality (macro-eval, D-PERSONA-4).
#[inline]
pub fn fanout_width(base: f32, bridgeness: f32) -> u32 {
    ((base * (1.0 + bridgeness.clamp(0.0, 1.0) * 0.5)).round() as i32).clamp(1, 30) as u32
}

/// Noise tolerance (annealing temperature proxy), grounded in `detector.rs`:
/// `noise_tolerance = base · (1 + (1 − confidence) · 0.5)` — low confidence
/// runs hotter (more exploration). `confidence` is the calibrated competence.
#[inline]
pub fn noise_tolerance(base: f32, confidence: f32) -> f32 {
    base * (1.0 + (1.0 - confidence.clamp(0.0, 1.0)) * 0.5)
}

/// Rung-shift delta, grounded in `detector.rs`:
/// `emergence > 0.5 ∧ coherence < 0.4 → +1` (elevate / RungElevate),
/// `coherence > 0.8 ∧ emergence < 0.1 → −1` (descend), else `0`.
#[inline]
pub fn rung_delta(emergence: f32, coherence: f32) -> i8 {
    if emergence > 0.5 && coherence < 0.4 {
        1
    } else if coherence > 0.8 && emergence < 0.1 {
        -1
    } else {
        0
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// InnerCouncil — Guardian / Catalyst / Balanced, majority vote + split amplify
// ═══════════════════════════════════════════════════════════════════════════

/// The three council perspectives. Each scores the item; the majority hint
/// wins. Disagreement (a split) is the learning signal, not noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Archetype {
    /// Caution / evidence-gathering — votes [`CollapseHint::Fanout`].
    Guardian,
    /// Push deeper — votes [`CollapseHint::RungElevate`].
    Catalyst,
    /// Settle — votes [`CollapseHint::Flow`].
    Balanced,
}

impl Archetype {
    /// The collapse hint this archetype advocates for.
    #[inline]
    pub fn vote(self) -> CollapseHint {
        match self {
            Archetype::Guardian => CollapseHint::Fanout,
            Archetype::Catalyst => CollapseHint::RungElevate,
            Archetype::Balanced => CollapseHint::Flow,
        }
    }
}

/// `HdrResonance` split test: one archetype sees strongly (`max ≥ hi`) what
/// another barely sees (`min ≤ lo`). Ladybug calls `is_split(0.7, 0.5)`.
#[inline]
pub fn is_split(scores: [f32; 3], hi: f32, lo: f32) -> bool {
    let max = scores.iter().copied().fold(f32::MIN, f32::max);
    let min = scores.iter().copied().fold(f32::MAX, f32::min);
    max >= hi && min <= lo
}

/// The council's verdict for one item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CouncilVerdict {
    /// The winning collapse hint (majority of the three perspectives).
    pub hint: CollapseHint,
    /// Winning confidence in `[0, 1]`, ×1.2-amplified (clamped) on a split.
    pub confidence: f32,
    /// Whether the perspectives split — the learning signal.
    pub split: bool,
}

/// The inner council. Stateless: `deliberate` is a pure function of the three
/// perspective scores; `from_signals` derives those scores from scalar MUL
/// observables shared by every assessment shape.
#[derive(Debug, Clone, Copy, Default)]
pub struct InnerCouncil;

impl InnerCouncil {
    /// Deliberate over the three perspective scores `[guardian, catalyst,
    /// balanced]` (each in `[0, 1]`). The highest-scoring archetype's hint
    /// wins; a split ([`is_split`] at `0.7 / 0.5`) amplifies the winning
    /// confidence ×1.2 (clamped to 1.0).
    pub fn deliberate(scores: [f32; 3]) -> CouncilVerdict {
        let archetypes = [
            Archetype::Guardian,
            Archetype::Catalyst,
            Archetype::Balanced,
        ];
        let (idx, &best) = scores
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(core::cmp::Ordering::Equal))
            .unwrap_or((2, &0.0)); // default Balanced/Flow when degenerate
        let split = is_split(scores, 0.7, 0.5);
        let confidence = if split { (best * 1.2).min(1.0) } else { best };
        CouncilVerdict {
            hint: archetypes[idx].vote(),
            confidence,
            split,
        }
    }

    /// Derive the three perspective scores from scalar MUL observables and
    /// deliberate. This is the `felt_parse` viscosity counterpart: it refines
    /// the inherited prior per-turn from the live cognitive signal.
    ///
    /// - `trust` ∈ `[0,1]` — calibrated trust value (high = safe to settle).
    /// - `humility` ∈ `[0,1]` — DK humility factor (low = Mount Stupid).
    /// - `flow` ∈ `[0,1]` — homeostatic flow factor (high = absorbed).
    /// - `load` ∈ `[0,1]` — allostatic load (high = stressed → gather more).
    ///
    /// Guardian (Fanout) rises with low trust / high load; Catalyst
    /// (RungElevate) rises in the productive mid-band (mid humility, decent
    /// flow — there is depth left to find); Balanced (Flow) rises with high
    /// trust + flow + low load.
    pub fn from_signals(trust: f32, humility: f32, flow: f32, load: f32) -> CouncilVerdict {
        let trust = trust.clamp(0.0, 1.0);
        let humility = humility.clamp(0.0, 1.0);
        let flow = flow.clamp(0.0, 1.0);
        let load = load.clamp(0.0, 1.0);

        let guardian = ((1.0 - trust) * 0.6 + load * 0.4).clamp(0.0, 1.0);
        // Catalyst peaks at mid humility (slope of enlightenment), gated by flow.
        let mid = 1.0 - (humility - 0.5).abs() * 2.0; // 1.0 at humility=0.5, 0 at extremes
        let catalyst = (mid * 0.6 + flow * 0.4).clamp(0.0, 1.0);
        let balanced = (trust * 0.5 + flow * 0.3 + (1.0 - load) * 0.2).clamp(0.0, 1.0);

        Self::deliberate([guardian, catalyst, balanced])
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EpiphanyDetector — surprise > baseline × 1.5 ∧ window ≥ 4
// ═══════════════════════════════════════════════════════════════════════════

/// An epiphany: the closing signal for a checklist item. A spike of
/// resonance above the established baseline, evidenced by a sufficient window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Epiphany {
    /// The triggering similarity / resonance sample.
    pub similarity: f32,
    /// The window baseline it exceeded by ≥ 1.5×.
    pub baseline: f32,
    /// How many prior samples backed the baseline (≥ 4).
    pub samples: u32,
}

/// Window of recent resonance samples. `observe` fires an [`Epiphany`] when a
/// sample exceeds the window baseline by ≥ 1.5× and at least 4 prior samples
/// have accumulated (the anti-Mount-Stupid evidence guard).
#[derive(Debug, Clone)]
pub struct EpiphanyDetector {
    window: Vec<f32>,
    cap: usize,
}

impl Default for EpiphanyDetector {
    fn default() -> Self {
        Self::new(16)
    }
}

impl EpiphanyDetector {
    /// New detector with a bounded rolling window of `cap` samples.
    pub fn new(cap: usize) -> Self {
        Self {
            window: Vec::new(),
            cap: cap.max(4),
        }
    }

    /// Minimum prior samples before an epiphany can fire.
    pub const MIN_SAMPLES: u32 = 4;
    /// Multiplier above baseline that counts as surprise.
    pub const SURPRISE_FACTOR: f32 = 1.5;

    /// Observe a similarity sample. Returns `Some(Epiphany)` iff the sample
    /// exceeds the prior-window baseline by ≥ 1.5× and ≥ 4 prior samples back
    /// that baseline. The sample is then folded into the window regardless.
    pub fn observe(&mut self, similarity: f32) -> Option<Epiphany> {
        let n = self.window.len() as u32;
        let result = if n >= Self::MIN_SAMPLES {
            let baseline = self.window.iter().copied().sum::<f32>() / n as f32;
            if baseline > 1e-6 && similarity > baseline * Self::SURPRISE_FACTOR {
                Some(Epiphany {
                    similarity,
                    baseline,
                    samples: n,
                })
            } else {
                None
            }
        } else {
            None
        };

        if self.window.len() == self.cap {
            self.window.remove(0);
        }
        self.window.push(similarity);
        result
    }

    /// Number of samples currently in the window.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// True when the window holds no samples.
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// GhostEcho + WisdomMarker — the green-flip residue (≤ 32, I-VSA-IDENTITIES)
// ═══════════════════════════════════════════════════════════════════════════

/// The 8 named ghost echoes — the wisdom-marker substrate, persistent qualia
/// residue that biases future perception. ≤ 32 named identities, content lives
/// in the store (I-VSA-IDENTITIES).
///
/// Canonical zero-dep home. Mirrors `thinking_engine::ghosts::GhostType`
/// (an excluded crate that cannot be a contract dependency); the two are to be
/// reconciled when thinking-engine joins the workspace — see
/// `TECH_DEBT.md` TD-GHOST-ECHO-DUP-1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GhostEcho {
    /// Lingering pull toward a concept / person / thing.
    Affinity,
    /// Residual clarity from a past insight (the default green-flip residue).
    Epiphany,
    /// Body-felt echo (tension, warmth, chill).
    Somatic,
    /// Persistent wonder / awe.
    Staunen,
    /// Deep knowing that colours all future perception (promoted from
    /// repeated Epiphany on the cold path — D-PERSONA-3).
    Wisdom,
    /// A thought that won't let go (rumination or focus).
    Thought,
    /// Loss that reshapes the topology.
    Grief,
    /// A limit discovered, still felt.
    Boundary,
}

/// A wisdom marker: a ghost echo with an intensity that decays asymptotically
/// toward `FLOOR` (0.1) — never to zero (felt_parse:70).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WisdomMarker {
    pub ghost: GhostEcho,
    pub intensity: f32,
}

impl WisdomMarker {
    /// Asymptotic decay floor — a wisdom marker never fully vanishes.
    pub const FLOOR: f32 = 0.1;
    /// Default per-cycle decay rate (matches the ghost field's slow lingering).
    pub const DECAY: f32 = 0.85;

    /// Fresh marker at full intensity.
    pub fn fresh(ghost: GhostEcho) -> Self {
        Self {
            ghost,
            intensity: 1.0,
        }
    }

    /// Intensity after `age` cycles: `max(FLOOR, intensity · DECAY^age)`.
    pub fn intensity_at(&self, age: u32) -> f32 {
        (self.intensity * Self::DECAY.powi(age as i32)).max(Self::FLOOR)
    }

    /// Promote a repeated insight to deep knowing (cold-path, D-PERSONA-3).
    pub fn promote_to_wisdom(mut self) -> Self {
        self.ghost = GhostEcho::Wisdom;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Checklist — HARD (boot gate) vs SOFT (degrade gracefully), green-flip
// ═══════════════════════════════════════════════════════════════════════════

/// Whether an item must be green to boot (HARD) or may degrade (SOFT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    /// Must be green for the agent to boot.
    Hard,
    /// Degrades gracefully if red — route around it (anytime / etiquette).
    Soft,
}

/// One checklist item, verified by the escalation loop rather than a bespoke
/// assert. Starts un-green with a [`CollapseHint::Fanout`] (gather evidence).
#[derive(Debug, Clone)]
pub struct ChecklistItem {
    pub name: &'static str,
    pub gate: GateKind,
    /// Current collapse-hint state — settles to `Flow` on completion.
    pub hint: CollapseHint,
    /// True once the item settled to Flow AND an epiphany closed it.
    pub green: bool,
    /// The wisdom residue left by the green-flip, if any.
    pub marker: Option<WisdomMarker>,
}

impl ChecklistItem {
    /// A fresh, un-verified item (Fanout = gather evidence first).
    pub fn new(name: &'static str, gate: GateKind) -> Self {
        Self {
            name,
            gate,
            hint: CollapseHint::Fanout,
            green: false,
            marker: None,
        }
    }
}

/// The boot checklist: a flat, deterministic list of items, each closed by the
/// escalation+epiphany loop. The meta-recipe composes when [`Checklist::all_flow`].
#[derive(Debug, Clone, Default)]
pub struct Checklist {
    pub items: Vec<ChecklistItem>,
}

impl Checklist {
    pub fn new(items: Vec<ChecklistItem>) -> Self {
        Self { items }
    }

    /// Apply a council verdict (and any epiphany) to the named item. A
    /// green-flip happens when the verdict settles to `Flow` AND an epiphany
    /// fired — the item then carries a fresh [`GhostEcho::Epiphany`] marker.
    /// Returns the marker minted on a green-flip, if any.
    pub fn step(
        &mut self,
        name: &str,
        verdict: &CouncilVerdict,
        epiphany: Option<Epiphany>,
    ) -> Option<WisdomMarker> {
        let item = self.items.iter_mut().find(|i| i.name == name)?;
        item.hint = verdict.hint;
        if verdict.hint == CollapseHint::Flow && epiphany.is_some() {
            let marker = WisdomMarker::fresh(GhostEcho::Epiphany);
            item.green = true;
            item.marker = Some(marker);
            Some(marker)
        } else {
            None
        }
    }

    /// Let-it-crash: a green item went red at runtime → re-escalate (Fanout)
    /// and drop its green flag. The supervisor restarts / escalates from here.
    pub fn mark_red(&mut self, name: &str) {
        if let Some(item) = self.items.iter_mut().find(|i| i.name == name) {
            item.green = false;
            item.hint = CollapseHint::Fanout;
            item.marker = None;
        }
    }

    /// Boot gate: every HARD item is green. SOFT items may still be red.
    pub fn boot_ready(&self) -> bool {
        self.items
            .iter()
            .filter(|i| i.gate == GateKind::Hard)
            .all(|i| i.green)
    }

    /// Composition gate: every item (hard + soft) has settled to `Flow` →
    /// the meta-recipe (D-PERSONA-2) composes.
    pub fn all_flow(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(|i| i.hint == CollapseHint::Flow)
    }

    /// True when at least one SOFT item is not green — boot proceeds, but the
    /// runtime routes around the degraded capability.
    pub fn degraded(&self) -> bool {
        self.items
            .iter()
            .any(|i| i.gate == GateKind::Soft && !i.green)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fanout_width_clamps_and_scales() {
        assert_eq!(fanout_width(4.0, 0.0), 4);
        assert_eq!(fanout_width(4.0, 1.0), 6); // 4 * 1.5
        assert_eq!(fanout_width(0.0, 0.0), 1); // clamp low
        assert_eq!(fanout_width(100.0, 1.0), 30); // clamp high
    }

    #[test]
    fn rung_delta_elevates_and_descends() {
        assert_eq!(rung_delta(0.6, 0.3), 1); // emergent + incoherent → elevate
        assert_eq!(rung_delta(0.05, 0.9), -1); // coherent + settled → descend
        assert_eq!(rung_delta(0.5, 0.5), 0); // neither
    }

    #[test]
    fn is_split_detects_disagreement() {
        assert!(is_split([0.8, 0.4, 0.45], 0.7, 0.5)); // one high, one low
        assert!(!is_split([0.6, 0.6, 0.6], 0.7, 0.5)); // consensus, no split
    }

    #[test]
    fn council_picks_majority_and_amplifies_on_split() {
        // Guardian dominates and the spread is a split → Fanout, amplified.
        let v = InnerCouncil::deliberate([0.8, 0.4, 0.45]);
        assert_eq!(v.hint, CollapseHint::Fanout);
        assert!(v.split);
        assert!((v.confidence - (0.8f32 * 1.2)).abs() < 1e-5);

        // Consensus around Balanced → Flow, no amplification.
        let v = InnerCouncil::deliberate([0.5, 0.55, 0.62]);
        assert_eq!(v.hint, CollapseHint::Flow);
        assert!(!v.split);
        assert!((v.confidence - 0.62).abs() < 1e-5);
    }

    #[test]
    fn from_signals_settles_when_trusted_and_flowing() {
        // High trust, good flow, low load → Balanced wins → Flow.
        let v = InnerCouncil::from_signals(0.9, 0.8, 0.9, 0.05);
        assert_eq!(v.hint, CollapseHint::Flow);
    }

    #[test]
    fn from_signals_fans_out_when_untrusted_and_loaded() {
        // Low trust, high load → Guardian wins → Fanout.
        let v = InnerCouncil::from_signals(0.1, 0.5, 0.3, 0.9);
        assert_eq!(v.hint, CollapseHint::Fanout);
    }

    #[test]
    fn epiphany_needs_window_and_surprise() {
        let mut d = EpiphanyDetector::new(8);
        // Cold start: < 4 samples → never fires even on a spike.
        assert!(d.observe(0.2).is_none());
        assert!(d.observe(0.2).is_none());
        assert!(d.observe(0.2).is_none());
        assert!(d.observe(0.2).is_none()); // now 4 samples in window
                                           // baseline ≈ 0.2; 0.2 * 1.5 = 0.3 → a 0.5 spike fires.
        let e = d.observe(0.5).expect("epiphany should fire");
        assert_eq!(e.samples, 4);
        assert!(e.similarity > e.baseline * 1.5);
        // A non-spike does not fire.
        assert!(d.observe(0.25).is_none());
    }

    #[test]
    fn wisdom_marker_decays_to_floor_never_zero() {
        let m = WisdomMarker::fresh(GhostEcho::Epiphany);
        assert!((m.intensity_at(0) - 1.0).abs() < 1e-6);
        assert!(m.intensity_at(5) < 1.0);
        // After many cycles it pins to the floor, never below.
        assert!((m.intensity_at(1000) - WisdomMarker::FLOOR).abs() < 1e-6);
        assert!(m.intensity_at(1000) >= WisdomMarker::FLOOR);
        assert_eq!(m.promote_to_wisdom().ghost, GhostEcho::Wisdom);
    }

    #[test]
    fn checklist_green_flip_on_flow_plus_epiphany() {
        let mut cl = Checklist::new(vec![
            ChecklistItem::new("contracts", GateKind::Hard),
            ChecklistItem::new("caps", GateKind::Soft),
        ]);
        assert!(!cl.boot_ready());

        let flow = CouncilVerdict {
            hint: CollapseHint::Flow,
            confidence: 0.9,
            split: false,
        };
        let epiphany = Epiphany {
            similarity: 0.6,
            baseline: 0.3,
            samples: 4,
        };
        // Flow without epiphany does NOT green-flip.
        assert!(cl.step("contracts", &flow, None).is_none());
        assert!(!cl.boot_ready());
        // Flow WITH epiphany green-flips and mints an Epiphany ghost.
        let marker = cl
            .step("contracts", &flow, Some(epiphany))
            .expect("green-flip");
        assert_eq!(marker.ghost, GhostEcho::Epiphany);
        assert!(cl.boot_ready()); // only HARD item needs to be green
        assert!(cl.degraded()); // SOFT "caps" still red → degrade, route around
        assert!(!cl.all_flow()); // soft item hasn't settled
    }

    #[test]
    fn checklist_let_it_crash_reescalates() {
        let mut cl = Checklist::new(vec![ChecklistItem::new("store", GateKind::Hard)]);
        let flow = CouncilVerdict {
            hint: CollapseHint::Flow,
            confidence: 1.0,
            split: false,
        };
        let e = Epiphany {
            similarity: 0.6,
            baseline: 0.3,
            samples: 5,
        };
        cl.step("store", &flow, Some(e));
        assert!(cl.boot_ready());
        // Runtime crash: the green item goes red → re-escalate to Fanout.
        cl.mark_red("store");
        assert!(!cl.boot_ready());
        assert_eq!(cl.items[0].hint, CollapseHint::Fanout);
    }
}
