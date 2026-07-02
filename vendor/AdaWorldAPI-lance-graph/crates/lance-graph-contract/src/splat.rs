//! SPLAT-1: Gaussian-splat → CAM-plane splat contract types.
//!
//! Materialises the spec at `.claude/knowledge/gaussian-splat-cam-plane-workaround.md`
//! PR 1 ("splat contracts"). Provides the deterministic deposition kernel
//! that lets witness-weighted semantic pressure project into CAM/COCA
//! awareness planes — a hot approximation layer between exact 4096-cycle
//! replay (cold) and naive bundling (lossy).
//!
//! Per the entropy ledger SPLAT-1 row: SplatChannel uses MergeMode::AlphaFrontToBack
//! when deposited into BindSpace planes (Pillar-7 Kerbl front-to-back compositing).
//!
//! # Design constraints (from lance-graph CLAUDE.md)
//!
//! - **Zero-dep**: only `core`/`std`, no external crates.
//! - **I-VSA-IDENTITIES**: splats carry identity fingerprints (codebook indices
//!   `center_a/b: u16`, `witness: ReasoningWitness64`, `replay_ref: u64`),
//!   never bundled content.
//! - **No floats on hot path** — amplitude / width / theta_accept are q8 (`u8`).
//! - **Click P-1 method discipline**: every operation lives on the carrier.
//!   `splat_set.deposit(splat)` — not `deposit(splat_set, splat)`.
//! - **No serde**: per LATEST_STATE.md "no JSON in types".
//! - **Repr stable**: `#[repr(u8)]` on enums, `#[repr(C)]` on cross-FFI structs.

// ── SplatChannel: which awareness plane the splat lands on ──────────────────

/// Channel discriminator. Each variant maps to an `AwarenessPlane16K` lane
/// in [`SplatPlaneSet`]. `Support` and `Contradiction` are evidence-bearing
/// (may promote ontology after NARS validation); `Forecast` and
/// `Counterfactual` are scenario-only (must NOT promote facts).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SplatChannel {
    #[default]
    Support = 0,
    Contradiction = 1,
    Forecast = 2,
    Counterfactual = 3,
    Style = 4,
    Source = 5,
}

impl SplatChannel {
    /// Evidence-bearing channels can promote ontology facts after NARS validation.
    /// Scenario-only channels (`Forecast`, `Counterfactual`) cannot.
    pub fn is_evidence_bearing(self) -> bool {
        matches!(self, Self::Support | Self::Contradiction)
    }

    /// Stable string label for logs / wire layers (lab-only — never on hot paths).
    pub fn label(self) -> &'static str {
        match self {
            Self::Support => "support",
            Self::Contradiction => "contradiction",
            Self::Forecast => "forecast",
            Self::Counterfactual => "counterfactual",
            Self::Style => "style",
            Self::Source => "source",
        }
    }
}

// ── TriadicProjection: lens used to derive the CAM center ──────────────────

/// Identifies which factor-pair → CAM-address projection produced this splat.
/// `0` = direct (S/P), `1` = transposed (P/O), `2` = diagonal (S/O), etc.
/// Deterministic for a given codebook + sigma rotation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct TriadicProjection(pub u8);

// ── ReasoningWitness64: identity fingerprint of the witness ────────────────

/// 64-bit identity fingerprint of the reasoning witness that produced the splat.
/// Per I-VSA-IDENTITIES: this POINTS TO content (witness in `EpisodicMemory`),
/// it does NOT carry the content itself.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ReasoningWitness64(pub u64);

// ── AwarenessPlane16K: 2 KB pressure tile (16,384 bits) ────────────────────

/// One awareness plane = 256 × u64 = 16,384 bits = 2 KB.
/// q8 deposition writes to dedicated accumulators, not floats.
/// This is the same width as `Vsa16kF32` and `Binary16K` (the canonical
/// switchboard carriers).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AwarenessPlane16K(pub [u64; 256]);

impl Default for AwarenessPlane16K {
    fn default() -> Self {
        Self([0u64; 256])
    }
}

impl AwarenessPlane16K {
    pub const fn zero() -> Self {
        Self([0u64; 256])
    }

    /// Deposit a single splat into this plane via OR (set-accumulate).
    /// Bit position derived from `splat.center_a` × 256 ⊕ `splat.center_b` mod 16384.
    ///
    /// OR (not XOR) is the correct multi-writer accumulation for pressure planes:
    /// repeated evidence at the same CAM address strengthens the signal rather
    /// than toggling it away. Removal is a separate `clear_bit()` operation
    /// (not yet needed — pressure planes are write-once-per-epoch, then reset
    /// via `AwarenessPlane16K::zero()`).
    pub fn deposit(&mut self, splat: &CamPlaneSplat) {
        let bit = (((splat.center_a as u32) << 8) ^ splat.center_b as u32) % 16_384;
        let word = (bit / 64) as usize;
        let mask = 1u64 << (bit % 64);
        self.0[word] |= mask;
    }
}

// ── CamPlaneSplat: the deposition kernel ───────────────────────────────────

/// A single splat unit. Deterministic under fixed codebook + sigma + theta + seeds.
/// All amplitude / width / theta values are q8 (uint8) — no floats on hot path.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CamPlaneSplat {
    /// First codebook index of the factor pair.
    pub center_a: u16,
    /// Second codebook index of the factor pair.
    pub center_b: u16,
    /// Which projection lens computed the center.
    pub projection: TriadicProjection,
    /// Which awareness plane this lands on.
    pub channel: SplatChannel,
    /// q8 amplitude: NARS confidence × frequency × evidence mantissa × polarity.
    pub amplitude_q8: u8,
    /// q8 width: sigma geometry × theta_width.
    pub width_q8: u8,
    /// q8 acceptance aperture from theta.
    pub theta_accept_q8: u8,
    /// Identity fingerprint of the witness that produced this splat.
    pub witness: ReasoningWitness64,
    /// Pointer back into `EpisodicMemory` for cold exact replay.
    pub replay_ref: u64,
}

// SplatChannel gets Default via derive + #[default] on Support variant above.

impl CamPlaneSplat {
    /// Effective amplitude after gating by the theta_accept aperture.
    /// Returns 0 if amplitude clears the acceptance threshold (rejected),
    /// otherwise the surviving amplitude.
    pub fn effective_amplitude(&self) -> u8 {
        self.amplitude_q8.saturating_sub(self.theta_accept_q8)
    }

    pub fn is_evidence_bearing(&self) -> bool {
        self.channel.is_evidence_bearing()
    }

    pub fn is_support(&self) -> bool {
        matches!(self.channel, SplatChannel::Support)
    }
    pub fn is_contradiction(&self) -> bool {
        matches!(self.channel, SplatChannel::Contradiction)
    }
    pub fn is_forecast(&self) -> bool {
        matches!(self.channel, SplatChannel::Forecast)
    }
    pub fn is_counterfactual(&self) -> bool {
        matches!(self.channel, SplatChannel::Counterfactual)
    }
}

// ── SplatPlaneSet: the 6 channel planes ────────────────────────────────────

/// One [`AwarenessPlane16K`] per channel. 12 KB total per set.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SplatPlaneSet {
    pub support: AwarenessPlane16K,
    pub contradiction: AwarenessPlane16K,
    pub forecast: AwarenessPlane16K,
    pub counterfactual: AwarenessPlane16K,
    pub style: AwarenessPlane16K,
    pub source: AwarenessPlane16K,
}

impl SplatPlaneSet {
    pub const fn zero() -> Self {
        Self {
            support: AwarenessPlane16K::zero(),
            contradiction: AwarenessPlane16K::zero(),
            forecast: AwarenessPlane16K::zero(),
            counterfactual: AwarenessPlane16K::zero(),
            style: AwarenessPlane16K::zero(),
            source: AwarenessPlane16K::zero(),
        }
    }

    /// Borrow the channel plane that matches the splat's channel.
    pub fn plane_for(&mut self, channel: SplatChannel) -> &mut AwarenessPlane16K {
        match channel {
            SplatChannel::Support => &mut self.support,
            SplatChannel::Contradiction => &mut self.contradiction,
            SplatChannel::Forecast => &mut self.forecast,
            SplatChannel::Counterfactual => &mut self.counterfactual,
            SplatChannel::Style => &mut self.style,
            SplatChannel::Source => &mut self.source,
        }
    }

    /// Deposit one splat into the appropriate channel plane.
    /// Method on the set (Click P-1 carrier rule), gated by effective amplitude > 0.
    pub fn deposit(&mut self, splat: &CamPlaneSplat) {
        if splat.effective_amplitude() == 0 {
            return;
        }
        self.plane_for(splat.channel).deposit(splat);
    }
}

// ── CamSplatCertificate: decision certificate from splatted pressure ────────

/// Carries the splat-derived pressure measurements that drive the certification
/// decision (proceed, require exact replay, scenario-only, etc).
/// All fields q8 — keeps certificate cheap to copy and compare.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CamSplatCertificate {
    pub thought_id: u64,
    pub cycle_id: u64,
    pub candidate_id: u64,

    pub support_pressure_q8: u8,
    pub contradiction_pressure_q8: u8,
    pub forecast_pressure_q8: u8,
    pub counterfactual_pressure_q8: u8,

    pub projection_compat_q8: u8,
    pub witness_compat_q8: u8,
    pub sigma_margin_q8: u8,
    pub theta_accept_q8: u8,
    pub theta_width_q8: u8,

    pub entropy_budget_remaining_q8: u8,
    pub exact_replay_required: bool,
    pub replay_ref: u64,
}

/// Decision returned by [`CamSplatCertificate::decide`].
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplatDecision {
    /// Splatted pressure sufficient — candidate may proceed to NARS / AriGraph validation.
    Proceed = 0,
    /// Support high but contradiction also high — exact 4096-cycle replay required.
    RequireExactReplay = 1,
    /// Forecast high but evidence low — use for prefetch / style only.
    PrefetchOnly = 2,
    /// Counterfactual high — scenario-only, must not promote ontology facts.
    ScenarioOnly = 3,
    /// All pressures below threshold — drop candidate.
    Drop = 4,
}

impl CamSplatCertificate {
    /// Decide what to do with this candidate based on splat pressures.
    /// Per the doc's "Certification path → Decision" section.
    pub fn decide(&self, support_floor: u8, contradiction_ceiling: u8) -> SplatDecision {
        if self.exact_replay_required {
            return SplatDecision::RequireExactReplay;
        }
        if self.counterfactual_pressure_q8 > 128 {
            return SplatDecision::ScenarioOnly;
        }
        if self.support_pressure_q8 >= support_floor
            && self.contradiction_pressure_q8 <= contradiction_ceiling
            && self.entropy_budget_remaining_q8 > 0
        {
            return SplatDecision::Proceed;
        }
        if self.support_pressure_q8 >= support_floor
            && self.contradiction_pressure_q8 > contradiction_ceiling
        {
            return SplatDecision::RequireExactReplay;
        }
        if self.forecast_pressure_q8 > 128 && self.support_pressure_q8 < support_floor {
            return SplatDecision::PrefetchOnly;
        }
        SplatDecision::Drop
    }
}

// ── ThetaDecision: theta-policy aperture proxy ─────────────────────────────

/// Sigma codebook proxy. The full SigmaCodebook lives in lance-graph-cognitive
/// (not a contract dep); this contract function takes `sigma_idx: u8` and the
/// sigma's q8-encoded `width` directly. Callers in the cognitive crate look up
/// the codebook entry and pass `(sigma_idx, sigma_width_q8)` here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThetaDecision {
    /// q8 acceptance aperture (the splat's theta_accept_q8 lands here verbatim)
    pub accept_q8: u8,
    /// q8 width contribution from theta (combined with sigma_width_q8 by witness_to_splat)
    pub width_q8: u8,
    /// Polarity bit — false = positive evidence, true = contradiction.
    pub negative: bool,
}

// ── witness_to_splat: deterministic conversion (D-SPLAT-3, PR 2 of 6) ──────

/// Deterministic conversion: ReasoningWitness64 (under sigma geometry + theta
/// aperture) → CamPlaneSplat. Per `.claude/knowledge/gaussian-splat-cam-plane-
/// workaround.md` § "Deposition rule".
///
/// Determinism contract: same inputs ALWAYS produce the same CamPlaneSplat.
/// No floats, no PRNG, no system clock. q8 lanes only.
///
/// Channel selection logic:
///   - If theta.negative → SplatChannel::Contradiction
///   - Else if witness has the "forecast" bit set in its high nibble → SplatChannel::Forecast
///   - Else if witness has the "counterfactual" bit set → SplatChannel::Counterfactual
///   - Else → SplatChannel::Support
///
/// (Style/Source channels are populated by separate higher-level builders;
/// they need richer context than a single witness provides.)
///
/// Width: combine sigma + theta widths (q8 saturating add) — matches the
/// "width: sigma geometry × theta_width" doc spec but in q8 lanes.
///
/// Amplitude: the witness's truth byte (NARS f * c packed into one u8 per the
/// witness layout — the lower 8 bits of the `ReasoningWitness64.0` word are
/// taken as the truth byte; this matches the splat.rs file-header
/// I-VSA-IDENTITIES note that ReasoningWitness64 is a 64-bit identity
/// fingerprint POINTING TO content rather than carrying it).
///
/// `sigma_idx` is currently unused at the contract level (lookup is callers'
/// job) but reserved for a future SigmaProvider trait. It is accepted in the
/// signature so cognitive-crate callers do not need to refactor when that
/// trait lands.
///
/// Per CLAUDE.md Click P-1 litmus: this is a **constructor** producing a fresh
/// CamPlaneSplat from raw inputs (no carrier with pre-existing state to mutate),
/// so it lives as a free function rather than a method on `&self` — builders
/// and constructors are exempt from the method-only rule.
#[allow(clippy::too_many_arguments)] // spec-mandated 8-arg signature: factor_a/b, projection, witness, sigma_idx, sigma_width_q8, theta, replay_ref
pub fn witness_to_splat(
    factor_a: u16,
    factor_b: u16,
    projection: TriadicProjection,
    witness: ReasoningWitness64,
    sigma_idx: u8,
    sigma_width_q8: u8,
    theta: ThetaDecision,
    replay_ref: u64,
) -> CamPlaneSplat {
    // 1. Determine channel from theta + witness.
    let channel = if theta.negative {
        SplatChannel::Contradiction
    } else {
        let high_nibble = ((witness.0 >> 60) as u8) & 0x0F;
        match high_nibble {
            0b0001 => SplatChannel::Forecast,
            0b0010 => SplatChannel::Counterfactual,
            _ => SplatChannel::Support,
        }
    };

    // 2. Combine sigma + theta widths (q8 saturating).
    let width_q8 = sigma_width_q8.saturating_add(theta.width_q8);

    // 3. Extract amplitude from witness truth byte (low 8 bits).
    let amplitude_q8 = (witness.0 & 0xFF) as u8;

    // sigma_idx reserved for a future SigmaProvider trait — discard here.
    let _ = sigma_idx;

    CamPlaneSplat {
        center_a: factor_a,
        center_b: factor_b,
        projection,
        channel,
        amplitude_q8,
        width_q8,
        theta_accept_q8: theta.accept_q8,
        witness,
        replay_ref,
    }
}

// ── Tests (ALL inline, in this file) ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splat_channel_evidence_bearing() {
        assert!(SplatChannel::Support.is_evidence_bearing());
        assert!(SplatChannel::Contradiction.is_evidence_bearing());
        assert!(!SplatChannel::Forecast.is_evidence_bearing());
        assert!(!SplatChannel::Counterfactual.is_evidence_bearing());
        assert!(!SplatChannel::Style.is_evidence_bearing());
        assert!(!SplatChannel::Source.is_evidence_bearing());
    }

    #[test]
    fn splat_channel_label_stable() {
        // All six labels must be distinct strings.
        let labels = [
            SplatChannel::Support.label(),
            SplatChannel::Contradiction.label(),
            SplatChannel::Forecast.label(),
            SplatChannel::Counterfactual.label(),
            SplatChannel::Style.label(),
            SplatChannel::Source.label(),
        ];
        for (i, a) in labels.iter().enumerate() {
            for (j, b) in labels.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "labels {} and {} collide ({:?})", i, j, a);
                }
            }
        }
        // And specific stable spellings.
        assert_eq!(SplatChannel::Support.label(), "support");
        assert_eq!(SplatChannel::Contradiction.label(), "contradiction");
        assert_eq!(SplatChannel::Forecast.label(), "forecast");
        assert_eq!(SplatChannel::Counterfactual.label(), "counterfactual");
        assert_eq!(SplatChannel::Style.label(), "style");
        assert_eq!(SplatChannel::Source.label(), "source");
    }

    #[test]
    fn cam_plane_splat_default() {
        let s = CamPlaneSplat::default();
        assert_eq!(s.center_a, 0);
        assert_eq!(s.center_b, 0);
        assert_eq!(s.projection.0, 0);
        assert!(matches!(s.channel, SplatChannel::Support));
        assert_eq!(s.amplitude_q8, 0);
        assert_eq!(s.width_q8, 0);
        assert_eq!(s.theta_accept_q8, 0);
        assert_eq!(s.witness.0, 0);
        assert_eq!(s.replay_ref, 0);
    }

    #[test]
    fn effective_amplitude_gates_below_theta() {
        // amp < theta → rejected (0)
        let rejected = CamPlaneSplat {
            amplitude_q8: 100,
            theta_accept_q8: 200,
            ..Default::default()
        };
        assert_eq!(rejected.effective_amplitude(), 0);

        // amp >= theta → surviving = amp - theta
        let surviving = CamPlaneSplat {
            amplitude_q8: 200,
            theta_accept_q8: 100,
            ..Default::default()
        };
        assert_eq!(surviving.effective_amplitude(), 100);

        // amp == theta → surviving 0 (boundary)
        let boundary = CamPlaneSplat {
            amplitude_q8: 100,
            theta_accept_q8: 100,
            ..Default::default()
        };
        assert_eq!(boundary.effective_amplitude(), 0);
    }

    #[test]
    fn awareness_plane_deposit_or_accumulates() {
        let mut plane = AwarenessPlane16K::default();
        let splat = CamPlaneSplat {
            center_a: 42,
            center_b: 7,
            amplitude_q8: 200,
            theta_accept_q8: 50,
            ..Default::default()
        };

        // After one deposit, plane is non-zero.
        plane.deposit(&splat);
        let nonzero = plane.0.iter().any(|w| *w != 0);
        assert!(nonzero, "plane stayed zero after a deposit");

        // After a second deposit of the same splat, OR keeps the bit set
        // (repeated evidence accumulates — Codex P1 review finding).
        plane.deposit(&splat);
        assert!(
            plane.0.iter().any(|w| *w != 0),
            "OR property: depositing the same splat twice must keep pressure"
        );
    }

    #[test]
    fn splat_plane_set_routes_to_channel() {
        let mut set = SplatPlaneSet::default();
        let support_splat = CamPlaneSplat {
            center_a: 11,
            center_b: 22,
            channel: SplatChannel::Support,
            amplitude_q8: 240,
            theta_accept_q8: 32,
            ..Default::default()
        };
        set.deposit(&support_splat);

        // Support plane must be non-zero.
        assert!(
            set.support.0.iter().any(|w| *w != 0),
            "support plane should have received the deposit"
        );

        // All other planes must be untouched.
        assert!(set.contradiction.0.iter().all(|w| *w == 0));
        assert!(set.forecast.0.iter().all(|w| *w == 0));
        assert!(set.counterfactual.0.iter().all(|w| *w == 0));
        assert!(set.style.0.iter().all(|w| *w == 0));
        assert!(set.source.0.iter().all(|w| *w == 0));
    }

    #[test]
    fn splat_plane_set_skips_zero_amplitude() {
        let mut set = SplatPlaneSet::default();
        let dud = CamPlaneSplat {
            center_a: 1,
            center_b: 2,
            channel: SplatChannel::Contradiction,
            amplitude_q8: 10,
            theta_accept_q8: 200, // amp < theta → effective 0
            ..Default::default()
        };
        set.deposit(&dud);

        // No plane should have been written.
        assert!(set.support.0.iter().all(|w| *w == 0));
        assert!(set.contradiction.0.iter().all(|w| *w == 0));
        assert!(set.forecast.0.iter().all(|w| *w == 0));
        assert!(set.counterfactual.0.iter().all(|w| *w == 0));
        assert!(set.style.0.iter().all(|w| *w == 0));
        assert!(set.source.0.iter().all(|w| *w == 0));
    }

    #[test]
    fn cam_splat_certificate_decide_proceed() {
        let cert = CamSplatCertificate {
            support_pressure_q8: 200,
            contradiction_pressure_q8: 20,
            entropy_budget_remaining_q8: 64,
            exact_replay_required: false,
            ..Default::default()
        };
        assert_eq!(cert.decide(128, 64), SplatDecision::Proceed);
    }

    #[test]
    fn cam_splat_certificate_decide_replay_when_contradiction_high() {
        let cert = CamSplatCertificate {
            support_pressure_q8: 200,
            contradiction_pressure_q8: 200,
            entropy_budget_remaining_q8: 64,
            exact_replay_required: false,
            ..Default::default()
        };
        assert_eq!(cert.decide(128, 64), SplatDecision::RequireExactReplay);
    }

    #[test]
    fn cam_splat_certificate_decide_scenario_when_counterfactual_high() {
        let cert = CamSplatCertificate {
            support_pressure_q8: 200,
            contradiction_pressure_q8: 20,
            counterfactual_pressure_q8: 200, // > 128 → scenario only
            entropy_budget_remaining_q8: 64,
            exact_replay_required: false,
            ..Default::default()
        };
        assert_eq!(cert.decide(128, 64), SplatDecision::ScenarioOnly);
    }

    #[test]
    fn cam_splat_certificate_decide_prefetch_when_only_forecast() {
        let cert = CamSplatCertificate {
            support_pressure_q8: 10, // below floor
            contradiction_pressure_q8: 10,
            forecast_pressure_q8: 200, // > 128
            entropy_budget_remaining_q8: 64,
            exact_replay_required: false,
            ..Default::default()
        };
        assert_eq!(cert.decide(128, 64), SplatDecision::PrefetchOnly);
    }

    #[test]
    fn cam_splat_certificate_decide_drop_when_all_low() {
        let cert = CamSplatCertificate::default();
        assert_eq!(cert.decide(128, 64), SplatDecision::Drop);
    }

    // ── D-SPLAT-3: witness_to_splat tests ──────────────────────────────────

    /// Helper: produce a "vanilla" theta with positive polarity.
    fn theta_pos(accept: u8, width: u8) -> ThetaDecision {
        ThetaDecision {
            accept_q8: accept,
            width_q8: width,
            negative: false,
        }
    }

    #[test]
    fn witness_to_splat_deterministic() {
        let projection = TriadicProjection(2);
        let witness = ReasoningWitness64(0x0000_DEAD_BEEF_00AB);
        let theta = theta_pos(40, 30);

        let a = witness_to_splat(11, 22, projection, witness, 5, 80, theta, 0xCAFE_BABE);
        let b = witness_to_splat(11, 22, projection, witness, 5, 80, theta, 0xCAFE_BABE);

        // Field-by-field equality (CamPlaneSplat is not Eq; compare bytes).
        assert_eq!(a.center_a, b.center_a);
        assert_eq!(a.center_b, b.center_b);
        assert_eq!(a.projection.0, b.projection.0);
        assert_eq!(a.channel as u8, b.channel as u8);
        assert_eq!(a.amplitude_q8, b.amplitude_q8);
        assert_eq!(a.width_q8, b.width_q8);
        assert_eq!(a.theta_accept_q8, b.theta_accept_q8);
        assert_eq!(a.witness.0, b.witness.0);
        assert_eq!(a.replay_ref, b.replay_ref);
    }

    #[test]
    fn witness_to_splat_negative_theta_routes_to_contradiction() {
        // Even if witness high nibble is 0x1 (forecast), negative polarity wins.
        let witness = ReasoningWitness64(0x1000_0000_0000_0042);
        let theta = ThetaDecision {
            accept_q8: 32,
            width_q8: 16,
            negative: true,
        };
        let s = witness_to_splat(1, 2, TriadicProjection(0), witness, 0, 64, theta, 0);
        assert!(matches!(s.channel, SplatChannel::Contradiction));
    }

    #[test]
    fn witness_to_splat_forecast_high_nibble() {
        // High nibble 0x1 → Forecast.
        let witness = ReasoningWitness64(0x1000_0000_0000_0000);
        let s = witness_to_splat(
            1,
            2,
            TriadicProjection(0),
            witness,
            0,
            64,
            theta_pos(32, 16),
            0,
        );
        assert!(matches!(s.channel, SplatChannel::Forecast));
    }

    #[test]
    fn witness_to_splat_counterfactual_high_nibble() {
        // High nibble 0x2 → Counterfactual.
        let witness = ReasoningWitness64(0x2000_0000_0000_0000);
        let s = witness_to_splat(
            1,
            2,
            TriadicProjection(0),
            witness,
            0,
            64,
            theta_pos(32, 16),
            0,
        );
        assert!(matches!(s.channel, SplatChannel::Counterfactual));
    }

    #[test]
    fn witness_to_splat_default_routes_to_support() {
        // Clean witness (high nibble 0x0) + positive theta → Support.
        let witness = ReasoningWitness64(0x0000_0000_0000_00FF);
        let s = witness_to_splat(
            1,
            2,
            TriadicProjection(0),
            witness,
            0,
            64,
            theta_pos(32, 16),
            0,
        );
        assert!(matches!(s.channel, SplatChannel::Support));
    }

    #[test]
    fn witness_to_splat_widths_saturating_add() {
        // sigma_width=200, theta_width=100 → saturated to 255.
        let witness = ReasoningWitness64(0);
        let theta = theta_pos(0, 100);
        let s = witness_to_splat(0, 0, TriadicProjection(0), witness, 0, 200, theta, 0);
        assert_eq!(
            s.width_q8, 255,
            "sigma+theta widths must saturate at u8::MAX"
        );
    }

    #[test]
    fn witness_to_splat_amplitude_from_witness_low_byte() {
        // Witness low byte = 0xAB → amplitude_q8 = 0xAB.
        let witness = ReasoningWitness64(0xDEAD_BEEF_DEAD_00AB);
        let s = witness_to_splat(
            0,
            0,
            TriadicProjection(0),
            witness,
            0,
            16,
            theta_pos(8, 4),
            0,
        );
        assert_eq!(s.amplitude_q8, 0xAB);
    }

    #[test]
    fn witness_to_splat_factor_pair_preserved() {
        // center_a / center_b round-trip verbatim.
        let s = witness_to_splat(
            0xABCD,
            0x1234,
            TriadicProjection(7),
            ReasoningWitness64(0),
            0,
            16,
            theta_pos(8, 4),
            0,
        );
        assert_eq!(s.center_a, 0xABCD);
        assert_eq!(s.center_b, 0x1234);
        assert_eq!(s.projection.0, 7);
    }

    #[test]
    fn witness_to_splat_replay_ref_preserved() {
        // replay_ref round-trips verbatim.
        let s = witness_to_splat(
            0,
            0,
            TriadicProjection(0),
            ReasoningWitness64(0),
            0,
            16,
            theta_pos(8, 4),
            0xFEED_FACE_DEAD_BEEF,
        );
        assert_eq!(s.replay_ref, 0xFEED_FACE_DEAD_BEEF);
    }

    #[test]
    fn witness_to_splat_theta_accept_passes_through() {
        // theta.accept_q8 lands verbatim in splat.theta_accept_q8.
        let s = witness_to_splat(
            0,
            0,
            TriadicProjection(0),
            ReasoningWitness64(0),
            0,
            16,
            theta_pos(123, 4),
            0,
        );
        assert_eq!(s.theta_accept_q8, 123);
    }
}
