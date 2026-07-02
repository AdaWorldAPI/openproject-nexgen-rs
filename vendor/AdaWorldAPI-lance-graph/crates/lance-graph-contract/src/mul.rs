//! MUL (Meta-Uncertainty Layer) assessment contract.
//!
//! Defines the types for Dunning-Kruger positioning, trust assessment,
//! flow state detection, and compass gating. lance-graph-planner
//! implements the assessment logic; consumers pass SituationInput
//! and receive MulAssessment.

/// Situation input: what the consumer knows about the current context.
///
/// All fields are 0.0–1.0 unless noted.
#[derive(Debug, Clone)]
pub struct SituationInput {
    pub felt_competence: f64,
    pub demonstrated_competence: f64,
    pub source_reliability: f64,
    pub environment_stability: f64,
    pub calibration_accuracy: f64,
    pub challenge_level: f64,
    pub skill_level: f64,
    pub allostatic_load: f64,
    pub max_acceptable_damage: f64,
    pub reversibility_requirement: f64,
    pub sandbox_available: bool,
    pub complexity_ratio: f64,
    pub interdependency_density: f64,
}

impl Default for SituationInput {
    fn default() -> Self {
        Self {
            felt_competence: 0.5,
            demonstrated_competence: 0.5,
            source_reliability: 0.7,
            environment_stability: 0.7,
            calibration_accuracy: 0.5,
            challenge_level: 0.5,
            skill_level: 0.5,
            allostatic_load: 0.3,
            max_acceptable_damage: 0.5,
            reversibility_requirement: 0.5,
            sandbox_available: false,
            complexity_ratio: 1.0,
            interdependency_density: 0.3,
        }
    }
}

/// MUL assessment result.
#[derive(Debug, Clone)]
pub struct MulAssessment {
    /// Trust quality assessment.
    pub trust: TrustQualia,
    /// Dunning-Kruger position.
    pub dk_position: DkPosition,
    /// Flow/homeostasis state.
    pub homeostasis: Homeostasis,
    /// Whether complexity was successfully mapped.
    pub complexity_mapped: bool,
    /// Free will modifier (0.0 = fully constrained, 1.0 = fully autonomous).
    pub free_will_modifier: f64,
}

/// Trust quality: how much to trust the current assessment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrustQualia {
    /// Raw trust value (0.0–1.0).
    pub value: f64,
    /// Texture: how the trust "feels" (calibrated, tentative, etc.).
    pub texture: TrustTexture,
}

/// Trust texture — qualitative assessment of trust.
///
/// **D-CSV-13b layout invariant (I-LEGACY-API-FEATURE-GATED, spec §5):**
/// `#[repr(u8)]` with explicit discriminants. The SIMD batch path in
/// [`i4_eval::batch`] writes raw bytes into `&mut [TrustTexture]` slices.
/// Reordering or removing these discriminants WILL silently corrupt SIMD
/// output; reviewers must check the SIMD LUTs in `mul.rs::batch::avx512_impl`
/// and `batch::neon_impl` if this layout is ever changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrustTexture {
    /// Well-calibrated: felt ≈ demonstrated competence.
    Calibrated = 0,
    /// Overconfident: felt >> demonstrated.
    Overconfident = 1,
    /// Uncertain: not enough data to assess.
    Uncertain = 2,
    /// Underconfident: felt << demonstrated.
    Underconfident = 3,
}

/// Dunning-Kruger position on the competence curve.
///
/// **D-CSV-13b layout invariant (I-LEGACY-API-FEATURE-GATED, spec §5):**
/// `#[repr(u8)]` with explicit discriminants. See `TrustTexture` for the
/// SIMD-byte-write contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DkPosition {
    /// Peak of Mount Stupid (overconfident novice).
    MountStupid = 0,
    /// Valley of Despair (aware of incompetence).
    ValleyOfDespair = 1,
    /// Slope of Enlightenment (growing competence).
    SlopeOfEnlightenment = 2,
    /// Plateau of Sustainability (expert).
    Plateau = 3,
}

/// Flow/homeostasis state.
#[derive(Debug, Clone)]
pub struct Homeostasis {
    /// Flow state assessment.
    pub flow_state: FlowState,
    /// Allostatic load (stress accumulation).
    pub allostatic_load: f64,
}

/// Flow state (Csikszentmihalyi).
///
/// **D-CSV-13b layout invariant (I-LEGACY-API-FEATURE-GATED, spec §5):**
/// `#[repr(u8)]` with explicit discriminants. See `TrustTexture` for the
/// SIMD-byte-write contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FlowState {
    /// Challenge ≈ Skill → flow.
    Flow = 0,
    /// Challenge << Skill → boredom.
    Boredom = 1,
    /// Transitioning between states.
    Transition = 2,
    /// Challenge >> Skill → anxiety.
    Anxiety = 3,
}

/// Gate decision: should the system proceed, pause, or block?
///
/// Cannot be `#[repr(u8)]` because `Hold` and `Block` carry `String` payloads.
/// Use [`GateDecision::to_disc`] for the SIMD-packable byte discriminant, or
/// [`batch::gate_decision_disc_batch`] for bulk processing.
#[derive(Debug, Clone)]
pub enum GateDecision {
    /// Proceed with full autonomy.
    Flow,
    /// Proceed with caution (reduced autonomy).
    Hold { reason: String },
    /// Block execution (require human input).
    Block { reason: String },
}

impl GateDecision {
    /// Return the discriminant as a SIMD-packable byte (D-CSV-13b).
    ///
    /// Mapping is locked: 0 = Flow, 1 = Hold, 2 = Block.
    #[inline]
    pub fn to_disc(&self) -> u8 {
        match self {
            GateDecision::Flow => 0,
            GateDecision::Hold { .. } => 1,
            GateDecision::Block { .. } => 2,
        }
    }
}

/// Compass result: surface-to-meta transition detection.
#[derive(Debug, Clone)]
pub struct CompassResult {
    /// Compass score (0.0 = stay surface, 1.0 = go meta).
    pub score: f64,
    /// Decision.
    pub decision: CompassDecision,
}

/// Compass decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompassDecision {
    /// Stay at surface level (normal execution).
    StaySurface,
    /// Transition to meta level (reflect, replan).
    GoMeta,
}

/// Trait for MUL assessment providers.
///
/// lance-graph-planner implements this. Consumers call it.
pub trait MulProvider: Send + Sync {
    /// Assess a situation and return MUL result.
    fn assess(&self, input: &SituationInput) -> MulAssessment;

    /// Gate check: should execution proceed?
    fn gate_check(&self, assessment: &MulAssessment) -> GateDecision;

    /// Compass check: should we go meta?
    fn compass(&self, assessment: &MulAssessment) -> CompassResult;
}

// ═══════════════════════════════════════════════════════════════════════════
// Ontology-aware MUL thresholds (D-ONTO-V5-9)
//
// Per `lance-graph-ontology-v5.md` §D-9: medical contexts demand stricter
// trust / flow / compass thresholds than callcenter contexts. Today the
// driver uses fixed scalar thresholds; this profile makes them
// ontology-context-aware. The driver's GateDecision computation site
// (cognitive-shader-driver::driver.rs ~L271-320) consults
// `MulThresholdProfile::for_context(ontology_context_id)` to pick the
// active profile.
//
// **Zone classification**: Zone 1 (BindSpace SoA, inside the BBB).
// MUST NOT carry `serde::Serialize` — `crates/lance-graph-callcenter/build.rs`
// (D-CASCADE-V1-1) actively scans for and rejects Serialize on Zone 1 types.
// See `.claude/knowledge/soa-dto-dependency-ledger.md`.
//
// **Integration plumb-through (TODO)**: `for_context` accepts a `u32`
// `ontology_context_id` placeholder. The Wave-2 `agent-context-id`
// deliverable adds `ontology_context_id: u32` onto
// `lance_graph_ontology::SchemaPtr`; the Wave-3 `agent-cascade-cols`
// deliverable threads it through `MappingRow` so `BindSpace` can read
// it per-row. Until then, the driver passes `0` (default profile).
// ═══════════════════════════════════════════════════════════════════════════

/// Per-ontology-context MUL gate thresholds.
///
/// Three canonical profiles ship with the contract: `MEDICAL` (strict),
/// `CALLCENTER` (lenient), `DEFAULT` (everything else). Lookup happens
/// via `for_context(ontology_context_id)`.
///
/// The struct is `Copy` so it can sit on the BindSpace per-row carrier
/// without indirection. `Eq`/`Hash` are NOT derived because the `f32`
/// fields cannot satisfy them; `PartialEq` is sufficient for the gate's
/// equality checks and the test asserts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MulThresholdProfile {
    /// `GateDecision` rejects when `TrustQualia.value` (texture-derived) < this.
    pub trust_min: f32,
    /// Homeostasis floor: flow_state must clear this before the gate emits Flow.
    pub flow_min: f32,
    /// Angular drift ceiling: the compass blocks when drift > this.
    pub compass_max: f32,
    /// Symbolic profile name (`"medical" | "callcenter" | "default"`).
    pub label: &'static str,
}

impl MulThresholdProfile {
    /// Strict medical/healthcare profile — trust ≥ 0.85, flow ≥ 0.70, drift ≤ 0.15.
    pub const MEDICAL: Self = Self {
        trust_min: 0.85,
        flow_min: 0.70,
        compass_max: 0.15,
        label: "medical",
    };

    /// Lenient callcenter / WorkOrder profile — trust ≥ 0.55, flow ≥ 0.40, drift ≤ 0.40.
    pub const CALLCENTER: Self = Self {
        trust_min: 0.55,
        flow_min: 0.40,
        compass_max: 0.40,
        label: "callcenter",
    };

    /// Default profile for unmapped contexts — trust ≥ 0.65, flow ≥ 0.50, drift ≤ 0.30.
    pub const DEFAULT: Self = Self {
        trust_min: 0.65,
        flow_min: 0.50,
        compass_max: 0.30,
        label: "default",
    };

    /// Look up the active profile for an ontology context id.
    ///
    /// Mapping (per `lance-graph-ontology-v5.md` §D-9):
    /// - `1` (WorkOrder) → `CALLCENTER`
    /// - `2` (Healthcare) → `MEDICAL`
    /// - `10..=19` (Medical/* subnamespaces) → `MEDICAL`
    /// - everything else → `DEFAULT`
    #[inline]
    pub const fn for_context(ontology_context_id: u32) -> Self {
        match ontology_context_id {
            1 => Self::CALLCENTER,
            2 => Self::MEDICAL,
            10..=19 => Self::MEDICAL,
            _ => Self::DEFAULT,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Carrier-method MUL assessment (TD-INT-3 wiring)
//
// Per CLAUDE.md doctrine ("methods on the carrier, not free functions on
// state"), MulAssessment carries its own compute() call. This is the
// shader-driver entry point: dispatch hands a SituationInput, gets back
// a MulAssessment, and uses dk_position + flow_state + trust.texture to
// modulate the gate decision.
//
// The planner has its own richer MulAssessment in lance-graph-planner::mul;
// this contract method is the zero-dep version that shader-driver and any
// other consumer can call without reaching into the planner.
// ═══════════════════════════════════════════════════════════════════════════

impl MulAssessment {
    /// Compute a MUL assessment directly from a SituationInput.
    ///
    /// Mirrors the planner's `mul::assess()` shape but lives on the carrier
    /// per the carrier-method doctrine. Pure, deterministic, zero-dep.
    ///
    /// Use this from any consumer that has a `SituationInput` and needs
    /// dk_position / trust.texture / homeostasis.flow_state to refine a
    /// downstream decision (the shader-driver collapse_gate is the
    /// canonical first consumer — see TD-INT-3).
    pub fn compute(input: &SituationInput) -> Self {
        // Phase 1: Trust qualia (geometric mean of 4 dimensions).
        let composite_trust = (input.demonstrated_competence
            * input.source_reliability
            * input.environment_stability
            * input.calibration_accuracy)
            .max(0.0)
            .powf(0.25);
        let trust_texture = trust_texture_from(
            input.felt_competence,
            input.demonstrated_competence,
            composite_trust,
        );
        let trust = TrustQualia {
            value: composite_trust,
            texture: trust_texture,
        };

        // Phase 1: Dunning-Kruger position (felt vs demonstrated competence).
        let dk_position = dk_from(input.felt_competence, input.demonstrated_competence);

        // Phase 2: Complexity mapping (≥30% of dimensions known).
        let complexity_mapped = input.complexity_ratio > 0.3;

        // Phase 3: Homeostasis (flow state + allostatic load).
        let flow_state = flow_state_from(input.challenge_level, input.skill_level);
        let homeostasis = Homeostasis {
            flow_state,
            allostatic_load: input.allostatic_load,
        };

        // Phase 4: Free-will modifier (multiplicative humility chain).
        let dk_factor = match dk_position {
            DkPosition::MountStupid => 0.3,
            DkPosition::ValleyOfDespair => 0.7,
            DkPosition::SlopeOfEnlightenment => 0.85,
            DkPosition::Plateau => 1.0,
        };
        let trust_factor = composite_trust;
        let complexity_factor = if complexity_mapped {
            0.8 + 0.2 * input.complexity_ratio
        } else {
            0.4
        };
        let load_penalty = if input.allostatic_load > 0.7 {
            0.3
        } else {
            1.0
        };
        let flow_factor = match flow_state {
            FlowState::Flow => 1.0,
            FlowState::Anxiety => 0.6,
            FlowState::Boredom => 0.8,
            FlowState::Transition => 0.7,
        } * load_penalty;

        let free_will_modifier =
            (dk_factor * trust_factor * complexity_factor * flow_factor).clamp(0.0, 1.0);

        Self {
            trust,
            dk_position,
            homeostasis,
            complexity_mapped,
            free_will_modifier,
        }
    }

    /// Whether the meta-uncertainty layer is signalling unskilled-overconfident:
    /// the system "feels confident" while DK and trust both flag the gap.
    /// Used by the shader-driver gate as a veto hint.
    #[inline]
    pub fn is_unskilled_overconfident(&self) -> bool {
        self.dk_position == DkPosition::MountStupid
            || self.trust.texture == TrustTexture::Overconfident
    }
}

fn trust_texture_from(felt: f64, demonstrated: f64, composite: f64) -> TrustTexture {
    let gap = felt - demonstrated;
    if composite < 0.25 {
        TrustTexture::Uncertain
    } else if gap > 0.25 {
        TrustTexture::Overconfident
    } else if gap < -0.25 {
        TrustTexture::Underconfident
    } else {
        TrustTexture::Calibrated
    }
}

fn dk_from(felt: f64, demonstrated: f64) -> DkPosition {
    let gap = felt - demonstrated;
    if gap > 0.3 && demonstrated < 0.4 {
        DkPosition::MountStupid
    } else if felt < 0.4 && demonstrated < 0.5 {
        DkPosition::ValleyOfDespair
    } else if demonstrated > 0.7 && gap.abs() < 0.15 {
        DkPosition::Plateau
    } else {
        DkPosition::SlopeOfEnlightenment
    }
}

fn flow_state_from(challenge: f64, skill: f64) -> FlowState {
    let delta = challenge - skill;
    if delta.abs() < 0.15 && challenge > 0.3 {
        FlowState::Flow
    } else if delta > 0.2 {
        FlowState::Anxiety
    } else if delta < -0.2 {
        FlowState::Boredom
    } else {
        FlowState::Transition
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// i4 scalar evaluation path — D-CSV-8 (sprint-11)
//
// Integer SIMD-ready MUL evaluation that consumes `QualiaI4_16D` + signed
// mantissa (i8 from `InferenceType::to_mantissa()`) and produces the same
// MUL types as the existing f32 path. The actual AVX-512 / NEON hot path
// is sprint-12+; this module locks the scalar i4 shape so sprint-12 can
// vectorise without changing API.
//
// All decision logic is pure: no heap allocation, no f64, no f32.
// GateDecision::Hold/Block carry &'static str reason to preserve zero-alloc.
// ═══════════════════════════════════════════════════════════════════════════

/// i4-scalar MUL evaluation.
///
/// All functions are `#[inline]` and heap-free. They consume
/// `QualiaI4_16D` from `crate::qualia` and a signed mantissa i8
/// (from `causal_edge::InferenceType::to_mantissa()`), and return the
/// existing MUL contract types unchanged.
pub mod i4_eval {
    use super::{
        DkPosition, FlowState, GateDecision, Homeostasis, MulAssessment, TrustQualia, TrustTexture,
    };
    use crate::qualia::QualiaI4_16D;

    // ── dim indices (aligned with QUALIA_I4_LABELS) ─────────────────────────
    const DIM_VALENCE: usize = 1; // signed valence (polarity)
    const DIM_TENSION: usize = 2; // tension / conflict load
    const DIM_WARMTH: usize = 3; // warmth / affiliation
    const DIM_COHERENCE: usize = 9; // coherence (story holds / breaks)
    const DIM_GROUNDEDNESS: usize = 14; // groundedness / stability

    /// On-demand intensity helper: `magnitude()` from the qualia struct.
    /// Returns coherence × valence as i8 (saturating). Used as a combined
    /// "signal strength × polarity" probe.
    #[inline]
    fn intensity_i4(qualia: &QualiaI4_16D) -> i8 {
        qualia.magnitude() // coherence(dim9) × valence(dim1), saturating
    }

    // ── DkPosition ──────────────────────────────────────────────────────────

    /// Classify Dunning-Kruger position from i4 qualia + signed mantissa.
    ///
    /// Decision rules (i4 range −8..+7):
    /// - `coherence(dim9) >= +5` AND `|signed_mantissa| >= 4`
    ///   → `Plateau` (expert: story holds, high-confidence rule active)
    /// - `coherence(dim9) >= +2` AND `|signed_mantissa| >= 2`
    ///   → `SlopeOfEnlightenment` (growing: moderate coherence + rule)
    /// - `coherence(dim9) <= -3` OR `|signed_mantissa| <= 1`
    ///   → `ValleyOfDespair` (low coherence or weak rule = aware of gaps)
    /// - otherwise → `MountStupid` (moderate-but-positive coherence, weak mantissa)
    #[inline]
    pub fn dk_position_i4(qualia: &QualiaI4_16D, signed_mantissa: i8) -> DkPosition {
        let coherence = qualia.get(DIM_COHERENCE);
        let abs_mantissa = signed_mantissa.unsigned_abs() as i8;

        if coherence >= 5 && abs_mantissa >= 4 {
            DkPosition::Plateau
        } else if coherence >= 2 && abs_mantissa >= 2 {
            DkPosition::SlopeOfEnlightenment
        } else if coherence <= -3 || abs_mantissa <= 1 {
            DkPosition::ValleyOfDespair
        } else {
            DkPosition::MountStupid
        }
    }

    // ── TrustTexture ─────────────────────────────────────────────────────────

    /// Derive TrustTexture from i4 qualia.
    ///
    /// Uses coherence (dim 9), valence (dim 1), tension (dim 2):
    ///
    /// | coherence | valence | tension | result        |
    /// |-----------|---------|---------|---------------|
    /// | ≥ +4      | ≥ +2    | ≤ +1   | Calibrated    |
    /// | ≤ -3      | any     | ≥ +3   | Uncertain     |
    /// | any       | ≥ +4    | any     | Overconfident |
    /// | any       | ≤ -3    | any     | Underconfident|
    /// | otherwise                     | Calibrated (moderate) |
    #[inline]
    pub fn trust_texture_i4(qualia: &QualiaI4_16D) -> TrustTexture {
        let coherence = qualia.get(DIM_COHERENCE);
        let valence = qualia.get(DIM_VALENCE);
        let tension = qualia.get(DIM_TENSION);

        if coherence <= -3 && tension >= 3 {
            TrustTexture::Uncertain
        } else if valence >= 4 && coherence < 5 {
            // High valence with only moderate coherence = overconfident
            TrustTexture::Overconfident
        } else if valence <= -3 {
            TrustTexture::Underconfident
        } else if coherence >= 4 && valence >= 2 && tension <= 1 {
            TrustTexture::Calibrated
        } else {
            // Moderate values — calibrated by default
            TrustTexture::Calibrated
        }
    }

    // ── FlowState ────────────────────────────────────────────────────────────

    /// Classify FlowState from i4 qualia + signed mantissa.
    ///
    /// Flow proxy = warmth(dim3) + groundedness(dim14) − tension(dim2).
    /// Combined with mantissa sign for direction:
    ///
    /// - flow_proxy ≥ +4 AND signed_mantissa > 0 → `Flow` (absorbed)
    /// - flow_proxy ≥ +2 AND signed_mantissa > 0 → `Transition` (building)
    /// - flow_proxy ≤ -2 OR (signed_mantissa < 0 AND coherence ≤ -1) → `Anxiety`
    /// - otherwise → `Boredom`
    #[inline]
    pub fn flow_state_i4(qualia: &QualiaI4_16D, signed_mantissa: i8) -> FlowState {
        let warmth = qualia.get(DIM_WARMTH);
        let groundedness = qualia.get(DIM_GROUNDEDNESS);
        let tension = qualia.get(DIM_TENSION);
        let coherence = qualia.get(DIM_COHERENCE);

        // Saturating i8 arithmetic on i4 inputs stays in i8 range safely
        let flow_proxy = (warmth as i16 + groundedness as i16 - tension as i16)
            .clamp(i8::MIN as i16, i8::MAX as i16) as i8;

        if flow_proxy >= 4 && signed_mantissa > 0 {
            FlowState::Flow
        } else if flow_proxy <= -2 || (signed_mantissa < 0 && coherence <= -1) {
            FlowState::Anxiety
        } else if flow_proxy >= 2 && signed_mantissa > 0 {
            FlowState::Transition
        } else {
            FlowState::Boredom
        }
    }

    // ── GateDecision ─────────────────────────────────────────────────────────

    /// Gate decision from i4 qualia + signed mantissa.
    ///
    /// Combines TrustTexture + FlowState:
    /// - `Uncertain` trust → `Block`
    /// - `Underconfident` trust + `Anxiety` → `Block`
    /// - `Overconfident` trust OR `Anxiety` alone → `Hold`
    /// - `Flow` or `Transition` + non-Uncertain trust → `Flow`
    /// - otherwise → `Hold`
    #[inline]
    pub fn gate_decision_i4(qualia: &QualiaI4_16D, signed_mantissa: i8) -> GateDecision {
        let texture = trust_texture_i4(qualia);
        let flow = flow_state_i4(qualia, signed_mantissa);

        match (texture, flow) {
            (TrustTexture::Uncertain, _) => GateDecision::Block {
                reason: "uncertain trust: coherence low, tension high".to_string(),
            },
            (TrustTexture::Underconfident, FlowState::Anxiety) => GateDecision::Block {
                reason: "underconfident + anxiety: execution blocked".to_string(),
            },
            (TrustTexture::Overconfident, _) => GateDecision::Hold {
                reason: "overconfident trust: caution required".to_string(),
            },
            (_, FlowState::Anxiety) => GateDecision::Hold {
                reason: "anxiety flow state: reduced autonomy".to_string(),
            },
            (
                TrustTexture::Calibrated | TrustTexture::Underconfident,
                FlowState::Flow | FlowState::Transition,
            ) => GateDecision::Flow,
            _ => GateDecision::Hold {
                reason: "boredom or moderate state: hold for re-evaluation".to_string(),
            },
        }
    }

    // ── MulAssessment ─────────────────────────────────────────────────────────

    /// Full MUL assessment from i4 qualia + signed mantissa.
    ///
    /// Combines `dk_position_i4`, `trust_texture_i4`, `flow_state_i4` into
    /// the existing `MulAssessment` struct. All fields are populated;
    /// `complexity_mapped` and `free_will_modifier` are derived from the
    /// i4 signals to produce a deterministic, zero-f64 result.
    ///
    /// `free_will_modifier` is approximated as a u8 fraction mapped to
    /// [0.0, 1.0] via the DK position × |mantissa| product, keeping the
    /// function free of heavy arithmetic while respecting the existing
    /// `f64` field type.
    pub fn mul_assess_i4(qualia: &QualiaI4_16D, signed_mantissa: i8) -> MulAssessment {
        let dk = dk_position_i4(qualia, signed_mantissa);
        let texture = trust_texture_i4(qualia);
        let flow = flow_state_i4(qualia, signed_mantissa);

        // TrustQualia.value: map texture + intensity to 0.0–1.0
        let intensity = intensity_i4(qualia); // i8 saturating product
        let trust_value: f64 = match texture {
            TrustTexture::Calibrated => 0.75 + (intensity.clamp(0, 7) as f64 / 7.0) * 0.25,
            TrustTexture::Overconfident => 0.45,
            TrustTexture::Underconfident => 0.40,
            TrustTexture::Uncertain => 0.20,
        };

        let trust = TrustQualia {
            value: trust_value,
            texture,
        };

        // complexity_mapped: coherence signal ≥ +2 implies the system can map complexity
        let coherence = qualia.get(DIM_COHERENCE);
        let complexity_mapped = coherence >= 2;

        // allostatic_load proxy: tension drives load (map i4 -8..+7 → 0.0..1.0)
        let tension = qualia.get(DIM_TENSION);
        let allostatic_load: f64 = ((tension as i16 + 8) as f64 / 15.0).clamp(0.0, 1.0);

        let homeostasis = Homeostasis {
            flow_state: flow,
            allostatic_load,
        };

        // free_will_modifier: DK factor × trust_value × flow_factor
        let dk_factor: f64 = match dk {
            DkPosition::MountStupid => 0.3,
            DkPosition::ValleyOfDespair => 0.7,
            DkPosition::SlopeOfEnlightenment => 0.85,
            DkPosition::Plateau => 1.0,
        };
        let flow_factor: f64 = match flow {
            FlowState::Flow => 1.0,
            FlowState::Transition => 0.7,
            FlowState::Boredom => 0.8,
            FlowState::Anxiety => 0.5,
        };
        let free_will_modifier = (dk_factor * trust_value * flow_factor).clamp(0.0, 1.0);

        MulAssessment {
            trust,
            dk_position: dk,
            homeostasis,
            complexity_mapped,
            free_will_modifier,
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Batch evaluation API — D-CSV-13b (sprint-13) — SIMD runtime dispatch
    // ═══════════════════════════════════════════════════════════════════════

    /// Batch evaluation API (D-CSV-13b, Sprint-13).
    ///
    /// Runtime SIMD dispatch via `simd_caps()` (OQ-CSV-13). No compile-time
    /// `cfg(target_feature)` — one binary runs on any host. Falls back to
    /// `scalar_impl` when AVX-512BW or NEON is absent.
    ///
    /// # Gate-decision carve-out
    /// `GateDecision` carries a `String` payload and cannot be `#[repr(u8)]`.
    /// `gate_decision_disc_batch` returns a `Vec<u8>` (0=Flow, 1=Hold, 2=Block)
    /// for SIMD-fast callers. `gate_decision_batch` returns the full
    /// `GateDecision` with reason strings via the scalar path.
    pub mod batch {
        use super::*;

        // ─────────────────────────────────────────────────────────────────────
        // Runtime SIMD capability detection (zero-dep, OQ-CSV-13)
        // ─────────────────────────────────────────────────────────────────────
        use core::sync::atomic::{AtomicU8, Ordering};

        /// Packed capability flags stored in a single atomic byte.
        /// Bit 0 = avx512f, Bit 1 = avx512bw, Bit 2 = neon.
        /// Value 0xFF = not yet probed.
        static CAPS_CACHE: AtomicU8 = AtomicU8::new(0xFF);

        #[derive(Clone, Copy)]
        #[allow(dead_code)] // each field is read only on its matching #[cfg(target_arch = ...)] dispatch branch
        struct SimdCapsShim {
            avx512f: bool,
            avx512bw: bool,
            neon: bool,
        }

        #[cold]
        fn probe_caps() -> SimdCapsShim {
            let avx512f;
            let avx512bw;
            let neon;

            #[cfg(target_arch = "x86_64")]
            {
                avx512f = is_x86_feature_detected!("avx512f");
                avx512bw = is_x86_feature_detected!("avx512bw");
                neon = false;
            }
            #[cfg(target_arch = "aarch64")]
            {
                avx512f = false;
                avx512bw = false;
                // NEON is mandatory on aarch64.
                neon = is_aarch64_feature_detected!("neon");
            }
            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            {
                avx512f = false;
                avx512bw = false;
                neon = false;
            }

            let bits: u8 = (avx512f as u8) | ((avx512bw as u8) << 1) | ((neon as u8) << 2);
            CAPS_CACHE.store(bits, Ordering::Relaxed);
            SimdCapsShim {
                avx512f,
                avx512bw,
                neon,
            }
        }

        #[inline]
        fn simd_caps() -> SimdCapsShim {
            let bits = CAPS_CACHE.load(Ordering::Relaxed);
            if bits == 0xFF {
                return probe_caps();
            }
            SimdCapsShim {
                avx512f: bits & 1 != 0,
                avx512bw: bits & 2 != 0,
                neon: bits & 4 != 0,
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // scalar_impl — correctness anchor, used as fallback and in tests
        //
        // Public so benches/i4_batch.rs can baseline SIMD speedup directly
        // against the scalar implementation; not intended as a stable API
        // for downstream callers (use the public dispatch wrappers below).
        // ─────────────────────────────────────────────────────────────────────
        #[doc(hidden)]
        pub mod scalar_impl {
            use super::super::*;
            use crate::qualia::QualiaI4_16D;

            pub fn dk_position_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [DkPosition],
            ) {
                for i in 0..qualia.len() {
                    out[i] = dk_position_i4(&qualia[i], mantissas[i]);
                }
            }

            pub fn trust_texture_batch(qualia: &[QualiaI4_16D], out: &mut [TrustTexture]) {
                for i in 0..qualia.len() {
                    out[i] = trust_texture_i4(&qualia[i]);
                }
            }

            pub fn flow_state_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [FlowState],
            ) {
                for i in 0..qualia.len() {
                    out[i] = flow_state_i4(&qualia[i], mantissas[i]);
                }
            }

            /// Returns discriminants: 0=Flow, 1=Hold, 2=Block.
            pub fn gate_decision_disc_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [u8],
            ) {
                for i in 0..qualia.len() {
                    out[i] = gate_decision_i4(&qualia[i], mantissas[i]).to_disc();
                }
            }

            pub fn mul_assess_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [MulAssessment],
            ) {
                for i in 0..qualia.len() {
                    out[i] = mul_assess_i4(&qualia[i], mantissas[i]);
                }
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // avx512_impl — AVX-512F + BW i4 intrinsics (D-CSV-13b)
        // ─────────────────────────────────────────────────────────────────────
        #[cfg(target_arch = "x86_64")]
        pub(crate) mod avx512_impl {
            use super::super::*;
            use crate::qualia::QualiaI4_16D;
            use core::arch::x86_64::*;

            /// Extract one i4 dimension (at nibble offset `SHIFT` bits) from each
            /// u64 lane of an 8-lane __m512i and sign-extend across the full i64
            /// lane so that downstream `_mm512_cmp*_epi64_mask` comparisons see
            /// the correct signed value (negative i4s read as negative i64s).
            ///
            /// `SHIFT` must be a compile-time constant (required by `_mm512_srli_epi64`).
            ///
            /// # SAFETY
            /// Caller must have verified avx512f + avx512bw at runtime before calling
            /// any function in this module.
            #[target_feature(enable = "avx512f,avx512bw")]
            #[inline]
            unsafe fn extract_dim_i8<const SHIFT: u32>(q_vec: __m512i) -> __m512i {
                // Step 1: shift the target nibble to bits [3:0] of each i64 lane.
                let shifted = _mm512_srli_epi64(q_vec, SHIFT);
                // Step 2: mask to the 4-bit nibble; bits [63:4] of each i64 lane = 0.
                let mask_f = _mm512_set1_epi64(0xF);
                let nibble = _mm512_and_si512(shifted, mask_f);
                // Step 3: sign-extend the 4-bit value to a full i64.
                //
                // Shift-left by 60 lifts the nibble's bit 3 (the i4 sign bit) into
                // bit 63 of the i64. Arithmetic shift-right by 60 then duplicates
                // that sign bit across bits [62:4], yielding a full i64 with the
                // correct signed value in range -8..=+7.
                let up = _mm512_slli_epi64(nibble, 60);
                _mm512_srai_epi64(up, 60)
            }

            /// Store the low byte of each i64 lane (8 bytes) into `out[0..8]`.
            ///
            /// Avoids VBMI2 `_mm512_mask_compressstoreu_epi8` — not available on
            /// Skylake-X/Cascade Lake. Uses scalar byte-extract from a stack buffer
            /// (spec §8 R-6, TD-D-CSV-13b-VBMI2-1).
            ///
            /// # SAFETY
            /// `out` must point to at least 8 writable bytes; avx512f verified at runtime.
            #[target_feature(enable = "avx512f")]
            #[inline]
            unsafe fn extract_8_lane0_bytes(result: __m512i, out: *mut u8) {
                let mut buf = [0u8; 64];
                _mm512_storeu_si512(buf.as_mut_ptr() as *mut __m512i, result);
                for j in 0..8usize {
                    *out.add(j) = buf[j * 8];
                }
            }

            /// Batch DK position — AVX-512 path (8 elements per iteration).
            ///
            /// # SAFETY
            /// avx512f + avx512bw must be verified at runtime via `simd_caps()`;
            /// `qualia.len() == mantissas.len() == out.len()` asserted by caller;
            /// `qualia.len() >= 8`.
            #[target_feature(enable = "avx512f,avx512bw")]
            pub unsafe fn dk_position_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [DkPosition],
            ) {
                let n = qualia.len();
                let mut i = 0usize;
                while i + 8 <= n {
                    // SAFETY: QualiaI4_16D is repr(C, align(8)); 8 consecutive elements
                    // occupy exactly 64 bytes — one __m512i word.
                    let q_ptr = qualia[i..].as_ptr() as *const __m512i;
                    let q_vec = _mm512_loadu_si512(q_ptr);
                    let coh = extract_dim_i8::<36>(q_vec); // DIM_COHERENCE=9 → nibble shift 36

                    let m_ptr = mantissas.as_ptr().add(i);
                    let man_vec = _mm512_set_epi64(
                        *m_ptr.add(7) as i64,
                        *m_ptr.add(6) as i64,
                        *m_ptr.add(5) as i64,
                        *m_ptr.add(4) as i64,
                        *m_ptr.add(3) as i64,
                        *m_ptr.add(2) as i64,
                        *m_ptr.add(1) as i64,
                        *m_ptr.add(0) as i64,
                    );
                    let zero = _mm512_setzero_si512();
                    let neg_man = _mm512_sub_epi64(zero, man_vec);
                    let man_neg_mask = _mm512_cmplt_epi64_mask(man_vec, zero);
                    let abs_man = _mm512_mask_blend_epi64(man_neg_mask, man_vec, neg_man);

                    // Priority chain (lowest to highest):
                    // Default = MountStupid (0)
                    let mut disc = _mm512_setzero_si512();
                    // ValleyOfDespair (1): coherence <= -3 OR abs_man <= 1
                    let vod = _mm512_cmple_epi64_mask(coh, _mm512_set1_epi64(-3))
                        | _mm512_cmple_epi64_mask(abs_man, _mm512_set1_epi64(1));
                    disc = _mm512_mask_blend_epi64(vod, disc, _mm512_set1_epi64(1));
                    // SlopeOfEnlightenment (2): coh >= 2 AND abs_man >= 2
                    let soe = _mm512_cmpge_epi64_mask(coh, _mm512_set1_epi64(2))
                        & _mm512_cmpge_epi64_mask(abs_man, _mm512_set1_epi64(2));
                    disc = _mm512_mask_blend_epi64(soe, disc, _mm512_set1_epi64(2));
                    // Plateau (3): coh >= 5 AND abs_man >= 4 (overrides all)
                    let plat = _mm512_cmpge_epi64_mask(coh, _mm512_set1_epi64(5))
                        & _mm512_cmpge_epi64_mask(abs_man, _mm512_set1_epi64(4));
                    disc = _mm512_mask_blend_epi64(plat, disc, _mm512_set1_epi64(3));

                    let out_ptr = out.as_mut_ptr().add(i) as *mut u8;
                    extract_8_lane0_bytes(disc, out_ptr);
                    i += 8;
                }
                while i < n {
                    out[i] = super::super::dk_position_i4(&qualia[i], mantissas[i]);
                    i += 1;
                }
            }

            /// Batch TrustTexture — AVX-512 path (8 elements per iteration).
            ///
            /// # SAFETY
            /// avx512f + avx512bw verified at runtime; lengths asserted by caller.
            #[target_feature(enable = "avx512f,avx512bw")]
            pub unsafe fn trust_texture_batch(qualia: &[QualiaI4_16D], out: &mut [TrustTexture]) {
                let n = qualia.len();
                let mut i = 0usize;
                while i + 8 <= n {
                    // SAFETY: QualiaI4_16D is repr(C, align(8)); 8 consecutive elements
                    // occupy exactly 64 bytes — one __m512i word.
                    let q_ptr = qualia[i..].as_ptr() as *const __m512i;
                    let q_vec = _mm512_loadu_si512(q_ptr);
                    let coh = extract_dim_i8::<36>(q_vec); // DIM_COHERENCE=9
                    let val = extract_dim_i8::<4>(q_vec); // DIM_VALENCE=1
                    let ten = extract_dim_i8::<8>(q_vec); // DIM_TENSION=2

                    // Default = Calibrated (0)
                    let mut disc = _mm512_setzero_si512();
                    // Underconfident (3): valence <= -3
                    let und = _mm512_cmple_epi64_mask(val, _mm512_set1_epi64(-3));
                    disc = _mm512_mask_blend_epi64(und, disc, _mm512_set1_epi64(3));
                    // Overconfident (1): valence >= 4 AND coherence < 5
                    let ovc = _mm512_cmpge_epi64_mask(val, _mm512_set1_epi64(4))
                        & _mm512_cmplt_epi64_mask(coh, _mm512_set1_epi64(5));
                    disc = _mm512_mask_blend_epi64(ovc, disc, _mm512_set1_epi64(1));
                    // Uncertain (2): coherence <= -3 AND tension >= 3 (highest priority)
                    let unc = _mm512_cmple_epi64_mask(coh, _mm512_set1_epi64(-3))
                        & _mm512_cmpge_epi64_mask(ten, _mm512_set1_epi64(3));
                    disc = _mm512_mask_blend_epi64(unc, disc, _mm512_set1_epi64(2));

                    let out_ptr = out.as_mut_ptr().add(i) as *mut u8;
                    extract_8_lane0_bytes(disc, out_ptr);
                    i += 8;
                }
                while i < n {
                    out[i] = super::super::trust_texture_i4(&qualia[i]);
                    i += 1;
                }
            }

            /// Batch FlowState — AVX-512 path (8 elements per iteration).
            ///
            /// # SAFETY
            /// avx512f + avx512bw verified at runtime; lengths asserted by caller.
            #[target_feature(enable = "avx512f,avx512bw")]
            pub unsafe fn flow_state_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [FlowState],
            ) {
                let n = qualia.len();
                let mut i = 0usize;
                while i + 8 <= n {
                    // SAFETY: QualiaI4_16D is repr(C, align(8)); 8 consecutive elements
                    // occupy exactly 64 bytes — one __m512i word.
                    let q_ptr = qualia[i..].as_ptr() as *const __m512i;
                    let q_vec = _mm512_loadu_si512(q_ptr);
                    let war = extract_dim_i8::<12>(q_vec); // DIM_WARMTH=3
                    let grd = extract_dim_i8::<56>(q_vec); // DIM_GROUNDEDNESS=14
                    let ten = extract_dim_i8::<8>(q_vec); // DIM_TENSION=2
                    let coh = extract_dim_i8::<36>(q_vec); // DIM_COHERENCE=9

                    // flow_proxy = warmth + groundedness - tension.
                    //
                    // Each input is now fully i64-sign-extended (i4 in -8..=+7),
                    // so the sum lies in -23..=+22 — well within i64 range, no
                    // saturation needed. Match the scalar's effective behaviour
                    // for i4 inputs (the scalar clamps to i8, never triggered).
                    let fp = _mm512_sub_epi64(_mm512_add_epi64(war, grd), ten);

                    let m_ptr = mantissas.as_ptr().add(i);
                    let man_vec = _mm512_set_epi64(
                        *m_ptr.add(7) as i64,
                        *m_ptr.add(6) as i64,
                        *m_ptr.add(5) as i64,
                        *m_ptr.add(4) as i64,
                        *m_ptr.add(3) as i64,
                        *m_ptr.add(2) as i64,
                        *m_ptr.add(1) as i64,
                        *m_ptr.add(0) as i64,
                    );
                    let zero = _mm512_setzero_si512();

                    // Pre-compute Anxiety condition (applied last for highest priority).
                    let anx = _mm512_cmple_epi64_mask(fp, _mm512_set1_epi64(-2))
                        | (_mm512_cmplt_epi64_mask(man_vec, zero)
                            & _mm512_cmple_epi64_mask(coh, _mm512_set1_epi64(-1)));

                    // Default = Boredom (1)
                    let mut disc = _mm512_set1_epi64(1);
                    // Transition (2): fp >= 2 AND man > 0
                    let tra = _mm512_cmpge_epi64_mask(fp, _mm512_set1_epi64(2))
                        & _mm512_cmpgt_epi64_mask(man_vec, zero);
                    disc = _mm512_mask_blend_epi64(tra, disc, _mm512_set1_epi64(2));
                    // Flow (0): fp >= 4 AND man > 0
                    let flow = _mm512_cmpge_epi64_mask(fp, _mm512_set1_epi64(4))
                        & _mm512_cmpgt_epi64_mask(man_vec, zero);
                    disc = _mm512_mask_blend_epi64(flow, disc, _mm512_set1_epi64(0));
                    // Anxiety (3): always overrides (highest priority)
                    disc = _mm512_mask_blend_epi64(anx, disc, _mm512_set1_epi64(3));

                    let out_ptr = out.as_mut_ptr().add(i) as *mut u8;
                    extract_8_lane0_bytes(disc, out_ptr);
                    i += 8;
                }
                while i < n {
                    out[i] = super::super::flow_state_i4(&qualia[i], mantissas[i]);
                    i += 1;
                }
            }

            /// Batch gate decision discriminants — AVX-512 path (8 elements per iteration).
            ///
            /// Gate LUT (tex_disc * 4 + flow_disc):
            /// ```text
            ///          Flow(0) Boredom(1) Transition(2) Anxiety(3)
            /// Cal(0):    0        1          0             1
            /// Ovc(1):    1        1          1             1
            /// Unc(2):    2        2          2             2
            /// Und(3):    0        1          0             2
            /// ```
            ///
            /// # SAFETY
            /// avx512f + avx512bw verified at runtime; lengths asserted by caller.
            #[target_feature(enable = "avx512f,avx512bw")]
            pub unsafe fn gate_decision_disc_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [u8],
            ) {
                // LUT index = tex_disc * 4 + flow_disc.
                // tex: Cal=0, Ovc=1, Unc=2, Und=3; flow: Flow=0, Bor=1, Tra=2, Anx=3.
                const LUT: [u8; 16] = [
                    0, 1, 0, 1, // Cal
                    1, 1, 1, 1, // Ovc
                    2, 2, 2, 2, // Unc
                    0, 1, 0, 2, // Und
                ];
                let n = qualia.len();
                let mut i = 0usize;
                let mut tex_disc = [0u8; 8];
                let mut flow_disc = [0u8; 8];
                while i + 8 <= n {
                    // SAFETY: TrustTexture/FlowState are repr(u8); pointers derived from
                    // properly-allocated arrays of the right size.
                    let tex_slice = core::slice::from_raw_parts_mut(
                        tex_disc.as_mut_ptr() as *mut TrustTexture,
                        8,
                    );
                    trust_texture_batch(&qualia[i..i + 8], tex_slice);
                    let flow_slice = core::slice::from_raw_parts_mut(
                        flow_disc.as_mut_ptr() as *mut FlowState,
                        8,
                    );
                    flow_state_batch(&qualia[i..i + 8], &mantissas[i..i + 8], flow_slice);
                    for j in 0..8usize {
                        let idx = (tex_disc[j] as usize) * 4 + (flow_disc[j] as usize);
                        out[i + j] = LUT[idx];
                    }
                    i += 8;
                }
                while i < n {
                    out[i] = super::super::gate_decision_i4(&qualia[i], mantissas[i]).to_disc();
                    i += 1;
                }
            }

            /// Batch MulAssessment — AVX-512 path.
            ///
            /// Uses SIMD for disc fields then scalar finalization for f64 fields.
            ///
            /// # SAFETY
            /// avx512f + avx512bw verified at runtime; lengths asserted by caller.
            #[target_feature(enable = "avx512f,avx512bw")]
            pub unsafe fn mul_assess_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [MulAssessment],
            ) {
                let n = qualia.len();
                let mut dk_disc = vec![0u8; n];
                let mut tex_disc = vec![0u8; n];
                let mut flow_disc = vec![0u8; n];

                // SAFETY: DkPosition/TrustTexture/FlowState are repr(u8) with discriminants 0..3;
                // vec storage is properly aligned and has length n.
                dk_position_batch(
                    qualia,
                    mantissas,
                    core::slice::from_raw_parts_mut(dk_disc.as_mut_ptr() as *mut DkPosition, n),
                );
                trust_texture_batch(
                    qualia,
                    core::slice::from_raw_parts_mut(tex_disc.as_mut_ptr() as *mut TrustTexture, n),
                );
                flow_state_batch(
                    qualia,
                    mantissas,
                    core::slice::from_raw_parts_mut(flow_disc.as_mut_ptr() as *mut FlowState, n),
                );

                for i in 0..n {
                    // SAFETY: repr(u8) enums with locked discriminants 0..3; values
                    // were written by the SIMD functions above which only produce 0..3.
                    let dk: DkPosition = core::mem::transmute(dk_disc[i]);
                    let texture: TrustTexture = core::mem::transmute(tex_disc[i]);
                    let flow: FlowState = core::mem::transmute(flow_disc[i]);

                    let intensity = qualia[i].magnitude();
                    let trust_value: f64 = match texture {
                        TrustTexture::Calibrated => {
                            0.75 + (intensity.clamp(0, 7) as f64 / 7.0) * 0.25
                        }
                        TrustTexture::Overconfident => 0.45,
                        TrustTexture::Underconfident => 0.40,
                        TrustTexture::Uncertain => 0.20,
                    };
                    let trust = TrustQualia {
                        value: trust_value,
                        texture,
                    };
                    let coherence = qualia[i].get(9); // DIM_COHERENCE
                    let complexity_mapped = coherence >= 2;
                    let tension = qualia[i].get(2); // DIM_TENSION
                    let allostatic_load = ((tension as i16 + 8) as f64 / 15.0).clamp(0.0, 1.0);
                    let homeostasis = Homeostasis {
                        flow_state: flow,
                        allostatic_load,
                    };
                    let dk_factor: f64 = match dk {
                        DkPosition::MountStupid => 0.3,
                        DkPosition::ValleyOfDespair => 0.7,
                        DkPosition::SlopeOfEnlightenment => 0.85,
                        DkPosition::Plateau => 1.0,
                    };
                    let flow_factor: f64 = match flow {
                        FlowState::Flow => 1.0,
                        FlowState::Transition => 0.7,
                        FlowState::Boredom => 0.8,
                        FlowState::Anxiety => 0.5,
                    };
                    let free_will_modifier =
                        (dk_factor * trust_value * flow_factor).clamp(0.0, 1.0);
                    out[i] = MulAssessment {
                        trust,
                        dk_position: dk,
                        homeostasis,
                        complexity_mapped,
                        free_will_modifier,
                    };
                }
            }
        } // avx512_impl

        // ─────────────────────────────────────────────────────────────────────
        // neon_impl — ARM NEON i4 intrinsics (D-CSV-13b)
        // ─────────────────────────────────────────────────────────────────────
        #[cfg(target_arch = "aarch64")]
        pub(crate) mod neon_impl {
            use super::super::*;
            use crate::qualia::QualiaI4_16D;
            use core::arch::aarch64::*;

            /// Extract one i4 dim from each of two u64 qualia words, sign-extend to i8.
            ///
            /// # SAFETY
            /// NEON is mandatory on aarch64; caller verifies via `is_aarch64_feature_detected!`.
            #[inline]
            unsafe fn extract_dim_pair(
                q0: uint64x2_t,
                q1: uint64x2_t,
                shift: i32,
            ) -> (int8x16_t, int8x16_t) {
                let mask = vdupq_n_u64(0xF);
                let n0 = vandq_u64(vshrq_n_u64(q0, shift), mask);
                let n1 = vandq_u64(vshrq_n_u64(q1, shift), mask);
                let i0 = vreinterpretq_s8_u64(n0);
                let i1 = vreinterpretq_s8_u64(n1);
                (
                    vshrq_n_s8(vshlq_n_s8(i0, 4), 4),
                    vshrq_n_s8(vshlq_n_s8(i1, 4), 4),
                )
            }

            /// Batch DK position — NEON path (2 elements per iteration).
            ///
            /// # SAFETY
            /// NEON verified at runtime; `qualia.len() >= 2`; lengths asserted by caller.
            pub unsafe fn dk_position_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [DkPosition],
            ) {
                let n = qualia.len();
                let mut i = 0usize;
                while i + 2 <= n {
                    // SAFETY: QualiaI4_16D is repr(C, align(8)); &.0 is a valid *const u64.
                    let q0 = vld1q_u64(&qualia[i].0 as *const u64);
                    let q1 = vld1q_u64(&qualia[i + 1].0 as *const u64);
                    let (c0, c1) = extract_dim_pair(q0, q1, 36);
                    let coh = [vgetq_lane_s8(c0, 0), vgetq_lane_s8(c1, 0)];
                    let abs_man = [
                        mantissas[i].unsigned_abs() as i8,
                        mantissas[i + 1].unsigned_abs() as i8,
                    ];
                    for j in 0..2 {
                        out[i + j] = if coh[j] >= 5 && abs_man[j] >= 4 {
                            DkPosition::Plateau
                        } else if coh[j] >= 2 && abs_man[j] >= 2 {
                            DkPosition::SlopeOfEnlightenment
                        } else if coh[j] <= -3 || abs_man[j] <= 1 {
                            DkPosition::ValleyOfDespair
                        } else {
                            DkPosition::MountStupid
                        };
                    }
                    i += 2;
                }
                while i < n {
                    out[i] = super::super::dk_position_i4(&qualia[i], mantissas[i]);
                    i += 1;
                }
            }

            /// Batch TrustTexture — NEON path (2 elements per iteration).
            ///
            /// # SAFETY
            /// NEON verified at runtime; lengths asserted by caller.
            pub unsafe fn trust_texture_batch(qualia: &[QualiaI4_16D], out: &mut [TrustTexture]) {
                let n = qualia.len();
                let mut i = 0usize;
                while i + 2 <= n {
                    // SAFETY: QualiaI4_16D is repr(C, align(8)); &.0 is a valid *const u64.
                    let q0 = vld1q_u64(&qualia[i].0 as *const u64);
                    let q1 = vld1q_u64(&qualia[i + 1].0 as *const u64);
                    let (c0, c1) = extract_dim_pair(q0, q1, 36);
                    let (v0, v1) = extract_dim_pair(q0, q1, 4);
                    let (t0, t1) = extract_dim_pair(q0, q1, 8);
                    let coh = [vgetq_lane_s8(c0, 0), vgetq_lane_s8(c1, 0)];
                    let val = [vgetq_lane_s8(v0, 0), vgetq_lane_s8(v1, 0)];
                    let ten = [vgetq_lane_s8(t0, 0), vgetq_lane_s8(t1, 0)];
                    for j in 0..2 {
                        out[i + j] = if coh[j] <= -3 && ten[j] >= 3 {
                            TrustTexture::Uncertain
                        } else if val[j] >= 4 && coh[j] < 5 {
                            TrustTexture::Overconfident
                        } else if val[j] <= -3 {
                            TrustTexture::Underconfident
                        } else {
                            TrustTexture::Calibrated
                        };
                    }
                    i += 2;
                }
                while i < n {
                    out[i] = super::super::trust_texture_i4(&qualia[i]);
                    i += 1;
                }
            }

            /// Batch FlowState — NEON path (2 elements per iteration).
            ///
            /// # SAFETY
            /// NEON verified at runtime; lengths asserted by caller.
            pub unsafe fn flow_state_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [FlowState],
            ) {
                let n = qualia.len();
                let mut i = 0usize;
                while i + 2 <= n {
                    // SAFETY: QualiaI4_16D is repr(C, align(8)); &.0 is a valid *const u64.
                    let q0 = vld1q_u64(&qualia[i].0 as *const u64);
                    let q1 = vld1q_u64(&qualia[i + 1].0 as *const u64);
                    let (w0, w1) = extract_dim_pair(q0, q1, 12);
                    let (g0, g1) = extract_dim_pair(q0, q1, 56);
                    let (t0, t1) = extract_dim_pair(q0, q1, 8);
                    let (c0, c1) = extract_dim_pair(q0, q1, 36);
                    let war = [vgetq_lane_s8(w0, 0), vgetq_lane_s8(w1, 0)];
                    let grd = [vgetq_lane_s8(g0, 0), vgetq_lane_s8(g1, 0)];
                    let ten = [vgetq_lane_s8(t0, 0), vgetq_lane_s8(t1, 0)];
                    let coh = [vgetq_lane_s8(c0, 0), vgetq_lane_s8(c1, 0)];
                    for j in 0..2 {
                        let fp = (war[j] as i16 + grd[j] as i16 - ten[j] as i16)
                            .clamp(i8::MIN as i16, i8::MAX as i16)
                            as i8;
                        let man = mantissas[i + j];
                        out[i + j] = if fp >= 4 && man > 0 {
                            FlowState::Flow
                        } else if fp <= -2 || (man < 0 && coh[j] <= -1) {
                            FlowState::Anxiety
                        } else if fp >= 2 && man > 0 {
                            FlowState::Transition
                        } else {
                            FlowState::Boredom
                        };
                    }
                    i += 2;
                }
                while i < n {
                    out[i] = super::super::flow_state_i4(&qualia[i], mantissas[i]);
                    i += 1;
                }
            }

            /// Batch gate decision discriminants — NEON path (scalar LUT).
            ///
            /// # SAFETY
            /// NEON verified at runtime; lengths asserted by caller.
            pub unsafe fn gate_decision_disc_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [u8],
            ) {
                // Use scalar path: String allocation in gate_decision_i4 is the bottleneck,
                // not the comparison logic. NEON benefit is in the disc functions above.
                for i in 0..qualia.len() {
                    out[i] = super::super::gate_decision_i4(&qualia[i], mantissas[i]).to_disc();
                }
            }

            /// Batch MulAssessment — NEON path (scalar finalization for f64 fields).
            ///
            /// # SAFETY
            /// NEON verified at runtime; lengths asserted by caller.
            pub unsafe fn mul_assess_batch(
                qualia: &[QualiaI4_16D],
                mantissas: &[i8],
                out: &mut [MulAssessment],
            ) {
                for i in 0..qualia.len() {
                    out[i] = super::super::mul_assess_i4(&qualia[i], mantissas[i]);
                }
            }
        } // neon_impl

        // ─────────────────────────────────────────────────────────────────────
        // Public dispatch API (OQ-CSV-13: runtime SIMD, not compile-time)
        // ─────────────────────────────────────────────────────────────────────

        /// Batch DK position: `qualia.len() == mantissas.len() == out.len()` must hold.
        /// Panics on length mismatch. Dispatches to AVX-512/NEON if available at runtime.
        pub fn dk_position_batch(
            qualia: &[QualiaI4_16D],
            mantissas: &[i8],
            out: &mut [DkPosition],
        ) {
            assert_eq!(
                qualia.len(),
                mantissas.len(),
                "qualia/mantissas length mismatch"
            );
            assert_eq!(qualia.len(), out.len(), "input/output length mismatch");
            let caps = simd_caps();
            #[cfg(target_arch = "x86_64")]
            if caps.avx512f && caps.avx512bw && qualia.len() >= 8 {
                // SAFETY: avx512f+avx512bw verified at runtime above; lengths asserted.
                unsafe { avx512_impl::dk_position_batch(qualia, mantissas, out) };
                return;
            }
            #[cfg(target_arch = "aarch64")]
            if caps.neon && qualia.len() >= 2 {
                // SAFETY: neon verified at runtime above; lengths asserted.
                unsafe { neon_impl::dk_position_batch(qualia, mantissas, out) };
                return;
            }
            scalar_impl::dk_position_batch(qualia, mantissas, out);
        }

        /// Batch TrustTexture. Dispatches to AVX-512/NEON if available at runtime.
        pub fn trust_texture_batch(qualia: &[QualiaI4_16D], out: &mut [TrustTexture]) {
            assert_eq!(qualia.len(), out.len(), "input/output length mismatch");
            let caps = simd_caps();
            #[cfg(target_arch = "x86_64")]
            if caps.avx512f && caps.avx512bw && qualia.len() >= 8 {
                // SAFETY: avx512f+avx512bw verified at runtime above; lengths asserted.
                unsafe { avx512_impl::trust_texture_batch(qualia, out) };
                return;
            }
            #[cfg(target_arch = "aarch64")]
            if caps.neon && qualia.len() >= 2 {
                // SAFETY: neon verified at runtime above; lengths asserted.
                unsafe { neon_impl::trust_texture_batch(qualia, out) };
                return;
            }
            scalar_impl::trust_texture_batch(qualia, out);
        }

        /// Batch FlowState. Dispatches to AVX-512/NEON if available at runtime.
        pub fn flow_state_batch(qualia: &[QualiaI4_16D], mantissas: &[i8], out: &mut [FlowState]) {
            assert_eq!(
                qualia.len(),
                mantissas.len(),
                "qualia/mantissas length mismatch"
            );
            assert_eq!(qualia.len(), out.len(), "input/output length mismatch");
            let caps = simd_caps();
            #[cfg(target_arch = "x86_64")]
            if caps.avx512f && caps.avx512bw && qualia.len() >= 8 {
                // SAFETY: avx512f+avx512bw verified at runtime above; lengths asserted.
                unsafe { avx512_impl::flow_state_batch(qualia, mantissas, out) };
                return;
            }
            #[cfg(target_arch = "aarch64")]
            if caps.neon && qualia.len() >= 2 {
                // SAFETY: neon verified at runtime above; lengths asserted.
                unsafe { neon_impl::flow_state_batch(qualia, mantissas, out) };
                return;
            }
            scalar_impl::flow_state_batch(qualia, mantissas, out);
        }

        /// Batch gate decision discriminants: 0=Flow, 1=Hold, 2=Block.
        ///
        /// SIMD-fast alternative to `gate_decision_batch`. Use when reason strings
        /// are not needed. Dispatches to AVX-512/NEON if available at runtime.
        pub fn gate_decision_disc_batch(qualia: &[QualiaI4_16D], mantissas: &[i8], out: &mut [u8]) {
            assert_eq!(
                qualia.len(),
                mantissas.len(),
                "qualia/mantissas length mismatch"
            );
            assert_eq!(qualia.len(), out.len(), "input/output length mismatch");
            let caps = simd_caps();
            #[cfg(target_arch = "x86_64")]
            if caps.avx512f && caps.avx512bw && qualia.len() >= 8 {
                // SAFETY: avx512f+avx512bw verified at runtime above; lengths asserted.
                unsafe { avx512_impl::gate_decision_disc_batch(qualia, mantissas, out) };
                return;
            }
            #[cfg(target_arch = "aarch64")]
            if caps.neon && qualia.len() >= 2 {
                // SAFETY: neon verified at runtime above; lengths asserted.
                unsafe { neon_impl::gate_decision_disc_batch(qualia, mantissas, out) };
                return;
            }
            scalar_impl::gate_decision_disc_batch(qualia, mantissas, out);
        }

        /// Batch full GateDecision with reason strings (scalar path — Strings cannot be SIMD-packed).
        pub fn gate_decision_batch(
            qualia: &[QualiaI4_16D],
            mantissas: &[i8],
            out: &mut [GateDecision],
        ) {
            assert_eq!(
                qualia.len(),
                mantissas.len(),
                "qualia/mantissas length mismatch"
            );
            assert_eq!(qualia.len(), out.len(), "input/output length mismatch");
            for i in 0..qualia.len() {
                out[i] = gate_decision_i4(&qualia[i], mantissas[i]);
            }
        }

        /// Batch MulAssessment. Dispatches to AVX-512/NEON if available at runtime.
        pub fn mul_assess_batch(
            qualia: &[QualiaI4_16D],
            mantissas: &[i8],
            out: &mut [MulAssessment],
        ) {
            assert_eq!(
                qualia.len(),
                mantissas.len(),
                "qualia/mantissas length mismatch"
            );
            assert_eq!(qualia.len(), out.len(), "input/output length mismatch");
            let caps = simd_caps();
            #[cfg(target_arch = "x86_64")]
            if caps.avx512f && caps.avx512bw && qualia.len() >= 8 {
                // SAFETY: avx512f+avx512bw verified at runtime above; lengths asserted.
                unsafe { avx512_impl::mul_assess_batch(qualia, mantissas, out) };
                return;
            }
            #[cfg(target_arch = "aarch64")]
            if caps.neon && qualia.len() >= 2 {
                // SAFETY: neon verified at runtime above; lengths asserted.
                unsafe { neon_impl::mul_assess_batch(qualia, mantissas, out) };
                return;
            }
            scalar_impl::mul_assess_batch(qualia, mantissas, out);
        }

        /// Convenience: allocate the output Vec and return it (for non-hot-path callers).
        pub fn mul_assess_vec(qualia: &[QualiaI4_16D], mantissas: &[i8]) -> Vec<MulAssessment> {
            assert_eq!(
                qualia.len(),
                mantissas.len(),
                "qualia/mantissas length mismatch"
            );
            let mut out = vec![
                MulAssessment {
                    trust: TrustQualia {
                        value: 0.0,
                        texture: TrustTexture::Calibrated
                    },
                    dk_position: DkPosition::MountStupid,
                    homeostasis: Homeostasis {
                        flow_state: FlowState::Boredom,
                        allostatic_load: 0.0,
                    },
                    complexity_mapped: false,
                    free_will_modifier: 0.0,
                };
                qualia.len()
            ];
            mul_assess_batch(qualia, mantissas, &mut out);
            out
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::qualia::QualiaI4_16D;

        // Helper: build a qualia with specific named dims set; rest = 0.
        fn q_with(pairs: &[(usize, i8)]) -> QualiaI4_16D {
            let mut q = QualiaI4_16D::ZERO;
            for &(dim, val) in pairs {
                q.set(dim, val);
            }
            q
        }

        // ── DkPosition ────────────────────────────────────────────────────

        #[test]
        fn test_dk_position_i4_high_coherence_expert() {
            // coherence=+7, mantissa=+5 → Plateau
            let q = q_with(&[(DIM_COHERENCE, 7)]);
            assert_eq!(dk_position_i4(&q, 5), DkPosition::Plateau);
        }

        #[test]
        fn test_dk_position_i4_low_coherence_beginner() {
            // coherence=-3, mantissa=+1 → ValleyOfDespair
            let q = q_with(&[(DIM_COHERENCE, -3)]);
            assert_eq!(dk_position_i4(&q, 1), DkPosition::ValleyOfDespair);
        }

        #[test]
        fn test_dk_position_i4_neutral_intermediate() {
            // all-zero qualia + mantissa=+2 → ValleyOfDespair
            // (zero coherence fails the >=2 bar for SlopeOfEnlightenment,
            //  but |mantissa|=2 barely meets it; coherence=0 < 2, so we fall
            //  to ValleyOfDespair because coherence=0 <= -3 is false, but
            //  abs_mantissa=2 >= 2 and coherence=0 < 2, so we check:
            //  coherence=0 >= 5 → no; coherence=0 >= 2 → no (0<2);
            //  coherence=0 <= -3 → no; abs_mantissa=2 <= 1 → no;
            //  → MountStupid)
            let q = QualiaI4_16D::ZERO;
            assert_eq!(dk_position_i4(&q, 2), DkPosition::MountStupid);
        }

        // ── TrustTexture ──────────────────────────────────────────────────

        #[test]
        fn test_trust_texture_i4_crystalline() {
            // high coherence(+6) + high valence(+3) + low tension(0) → Calibrated
            let q = q_with(&[(DIM_COHERENCE, 6), (DIM_VALENCE, 3), (DIM_TENSION, 0)]);
            assert_eq!(trust_texture_i4(&q), TrustTexture::Calibrated);
        }

        #[test]
        fn test_trust_texture_i4_murky() {
            // low coherence(-5) + high tension(+5) → Uncertain
            let q = q_with(&[(DIM_COHERENCE, -5), (DIM_TENSION, 5)]);
            assert_eq!(trust_texture_i4(&q), TrustTexture::Uncertain);
        }

        #[test]
        fn test_trust_texture_i4_solid_calibrated() {
            // moderate coherence(+2) + moderate valence(+2) + moderate tension(+1) → Calibrated
            let q = q_with(&[(DIM_COHERENCE, 2), (DIM_VALENCE, 2), (DIM_TENSION, 1)]);
            assert_eq!(trust_texture_i4(&q), TrustTexture::Calibrated);
        }

        // ── FlowState ─────────────────────────────────────────────────────

        #[test]
        fn test_flow_state_i4_active() {
            // warmth(+5) + groundedness(+4) − tension(0) = proxy +9 → clamped fine; mantissa>0 → Flow
            let q = q_with(&[(DIM_WARMTH, 5), (DIM_GROUNDEDNESS, 4), (DIM_TENSION, 0)]);
            assert_eq!(flow_state_i4(&q, 3), FlowState::Flow);
        }

        #[test]
        fn test_flow_state_i4_stuck_negative_mantissa() {
            // coherence=-3 + mantissa=-4 → Anxiety
            let q = q_with(&[(DIM_COHERENCE, -3), (DIM_TENSION, 3)]);
            assert_eq!(flow_state_i4(&q, -4), FlowState::Anxiety);
        }

        // ── GateDecision ──────────────────────────────────────────────────

        #[test]
        fn test_gate_decision_i4_proceed() {
            // calibrated trust + flow state → GateDecision::Flow
            let q = q_with(&[
                (DIM_COHERENCE, 5),
                (DIM_VALENCE, 3),
                (DIM_TENSION, 0),
                (DIM_WARMTH, 5),
                (DIM_GROUNDEDNESS, 4),
            ]);
            let gate = gate_decision_i4(&q, 4);
            assert!(matches!(gate, GateDecision::Flow));
        }

        #[test]
        fn test_gate_decision_i4_block() {
            // uncertain trust (low coherence, high tension) → Block
            let q = q_with(&[(DIM_COHERENCE, -5), (DIM_TENSION, 5)]);
            let gate = gate_decision_i4(&q, 2);
            assert!(matches!(gate, GateDecision::Block { .. }));
        }

        // ── MulAssessment ─────────────────────────────────────────────────

        #[test]
        fn test_mul_assess_i4_combines_all_four() {
            // Strong expert signal: high coherence, high valence, low tension,
            // high warmth + groundedness, positive mantissa → all non-default fields
            let q = q_with(&[
                (DIM_COHERENCE, 6),
                (DIM_VALENCE, 5),
                (DIM_TENSION, 0),
                (DIM_WARMTH, 5),
                (DIM_GROUNDEDNESS, 5),
            ]);
            let mul = mul_assess_i4(&q, 5);
            assert_eq!(mul.dk_position, DkPosition::Plateau);
            assert_eq!(mul.trust.texture, TrustTexture::Calibrated);
            assert_eq!(mul.homeostasis.flow_state, FlowState::Flow);
            assert!(
                mul.free_will_modifier > 0.5,
                "expert+flow should give high autonomy"
            );
            assert!(
                mul.complexity_mapped,
                "high coherence should map complexity"
            );
        }

        #[test]
        fn test_mul_assess_i4_zero_qualia_zero_mantissa_default_path() {
            // All-zero input + zero mantissa → deterministic neutral baseline
            let q = QualiaI4_16D::ZERO;
            let mul = mul_assess_i4(&q, 0);
            // Zero coherence → not complexity_mapped
            assert!(!mul.complexity_mapped);
            // Zero mantissa (abs=0) → ValleyOfDespair
            assert_eq!(mul.dk_position, DkPosition::ValleyOfDespair);
            // free_will_modifier must be in [0.0, 1.0]
            assert!(mul.free_will_modifier >= 0.0 && mul.free_will_modifier <= 1.0);
            // Trust value must be > 0.0 (even uncertain has 0.20 floor)
            assert!(mul.trust.value > 0.0);
        }

        // ── Batch API tests (D-CSV-13) ────────────────────────────────────

        /// Helper: generate N deterministic qualia + mantissa pairs.
        fn make_batch(n: usize) -> (Vec<QualiaI4_16D>, Vec<i8>) {
            let pairs: &[(usize, i8, i8)] = &[
                // (dim_coherence, set_val, mantissa)
                (9, 7, 5),
                (9, 5, 4),
                (9, 3, 3),
                (9, 2, 2),
                (9, 0, 2),
                (9, -1, 1),
                (9, -3, -2),
                (9, -5, -4),
                (9, 6, 0),
                (9, 1, -1),
            ];
            let mut qualia = Vec::with_capacity(n);
            let mut mantissas = Vec::with_capacity(n);
            for i in 0..n {
                let (dim, coh, mant) = pairs[i % pairs.len()];
                qualia.push(QualiaI4_16D::ZERO.with(dim, coh));
                mantissas.push(mant);
            }
            (qualia, mantissas)
        }

        #[test]
        fn test_dk_position_batch_matches_scalar() {
            let (qualia, mantissas) = make_batch(10);
            let mut out = vec![DkPosition::MountStupid; 10];
            batch::dk_position_batch(&qualia, &mantissas, &mut out);
            for (i, (q, &m)) in qualia.iter().zip(mantissas.iter()).enumerate() {
                assert_eq!(out[i], dk_position_i4(q, m), "mismatch at index {}", i);
            }
        }

        #[test]
        fn test_trust_texture_batch_matches_scalar() {
            let (qualia, _) = make_batch(10);
            let mut out = vec![TrustTexture::Uncertain; 10];
            batch::trust_texture_batch(&qualia, &mut out);
            for (i, q) in qualia.iter().enumerate() {
                assert_eq!(out[i], trust_texture_i4(q), "mismatch at index {}", i);
            }
        }

        #[test]
        fn test_flow_state_batch_matches_scalar() {
            let (qualia, mantissas) = make_batch(10);
            let mut out = vec![FlowState::Boredom; 10];
            batch::flow_state_batch(&qualia, &mantissas, &mut out);
            for (i, (q, &m)) in qualia.iter().zip(mantissas.iter()).enumerate() {
                assert_eq!(out[i], flow_state_i4(q, m), "mismatch at index {}", i);
            }
        }

        #[test]
        fn test_gate_decision_batch_matches_scalar() {
            let (qualia, mantissas) = make_batch(10);
            let mut out: Vec<GateDecision> = (0..10).map(|_| GateDecision::Flow).collect();
            batch::gate_decision_batch(&qualia, &mantissas, &mut out);
            for (i, (q, &m)) in qualia.iter().zip(mantissas.iter()).enumerate() {
                let scalar = gate_decision_i4(q, m);
                // Compare discriminant since GateDecision carries String fields
                assert!(
                    matches_gate_discriminant(&out[i], &scalar),
                    "gate decision discriminant mismatch at index {}: batch={:?} scalar={:?}",
                    i,
                    out[i],
                    scalar
                );
            }
        }

        fn matches_gate_discriminant(a: &GateDecision, b: &GateDecision) -> bool {
            matches!(
                (a, b),
                (GateDecision::Flow, GateDecision::Flow)
                    | (GateDecision::Hold { .. }, GateDecision::Hold { .. })
                    | (GateDecision::Block { .. }, GateDecision::Block { .. })
            )
        }

        #[test]
        fn test_mul_assess_batch_matches_scalar() {
            let (qualia, mantissas) = make_batch(10);
            let mut out: Vec<MulAssessment> = (0..10)
                .map(|_| mul_assess_i4(&QualiaI4_16D::ZERO, 0))
                .collect();
            batch::mul_assess_batch(&qualia, &mantissas, &mut out);
            for (i, (q, &m)) in qualia.iter().zip(mantissas.iter()).enumerate() {
                let scalar = mul_assess_i4(q, m);
                assert_eq!(
                    out[i].dk_position, scalar.dk_position,
                    "dk_position mismatch at {}",
                    i
                );
                assert_eq!(
                    out[i].trust.texture, scalar.trust.texture,
                    "trust.texture mismatch at {}",
                    i
                );
                assert_eq!(
                    out[i].homeostasis.flow_state, scalar.homeostasis.flow_state,
                    "flow_state mismatch at {}",
                    i
                );
                assert!(
                    (out[i].free_will_modifier - scalar.free_will_modifier).abs() < 1e-10,
                    "free_will_modifier mismatch at {}",
                    i
                );
            }
        }

        #[test]
        fn test_mul_assess_vec_allocates_correctly() {
            let (qualia, mantissas) = make_batch(10);
            let result = batch::mul_assess_vec(&qualia, &mantissas);
            assert_eq!(
                result.len(),
                qualia.len(),
                "output length must equal input length"
            );
            for (i, (q, &m)) in qualia.iter().zip(mantissas.iter()).enumerate() {
                let scalar = mul_assess_i4(q, m);
                assert_eq!(
                    result[i].dk_position, scalar.dk_position,
                    "dk_position mismatch at {}",
                    i
                );
                assert_eq!(
                    result[i].trust.texture, scalar.trust.texture,
                    "trust.texture mismatch at {}",
                    i
                );
                assert_eq!(
                    result[i].homeostasis.flow_state, scalar.homeostasis.flow_state,
                    "flow_state mismatch at {}",
                    i
                );
            }
        }

        #[test]
        fn test_batch_panic_on_length_mismatch() {
            let qualia = vec![QualiaI4_16D::ZERO; 3];
            let mantissas = vec![0i8; 2]; // intentional mismatch
            let mut out = vec![DkPosition::MountStupid; 3];
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                batch::dk_position_batch(&qualia, &mantissas, &mut out);
            }));
            assert!(
                result.is_err(),
                "must panic on qualia/mantissas length mismatch"
            );
        }

        #[test]
        fn test_batch_empty_input_returns_empty_output() {
            let qualia: Vec<QualiaI4_16D> = vec![];
            let mantissas: Vec<i8> = vec![];
            let mut out_dk: Vec<DkPosition> = vec![];
            let mut out_tt: Vec<TrustTexture> = vec![];
            let mut out_fs: Vec<FlowState> = vec![];
            let mut out_gd: Vec<GateDecision> = vec![];
            let mut out_ma: Vec<MulAssessment> = vec![];

            // None of these should panic
            batch::dk_position_batch(&qualia, &mantissas, &mut out_dk);
            batch::trust_texture_batch(&qualia, &mut out_tt);
            batch::flow_state_batch(&qualia, &mantissas, &mut out_fs);
            batch::gate_decision_batch(&qualia, &mantissas, &mut out_gd);
            batch::mul_assess_batch(&qualia, &mantissas, &mut out_ma);
            let vec_result = batch::mul_assess_vec(&qualia, &mantissas);

            assert_eq!(out_dk.len(), 0);
            assert_eq!(out_tt.len(), 0);
            assert_eq!(out_fs.len(), 0);
            assert_eq!(out_gd.len(), 0);
            assert_eq!(out_ma.len(), 0);
            assert_eq!(vec_result.len(), 0);
        }

        // ── D-CSV-13b: randomised SIMD-vs-scalar parity tests ─────────────────
        //
        // Each test generates a deterministic pseudo-random batch (fixed seed),
        // runs `batch::FN` (which dispatches to AVX-512 / NEON / scalar at
        // runtime) against `batch::scalar_impl::FN` (the correctness anchor),
        // and asserts element-wise equality.
        //
        // Per spec §5 (I-LEGACY-API-FEATURE-GATED): bytes must be identical
        // between dispatch path and scalar path. On a non-SIMD host the test
        // degenerates to "scalar == scalar" (still asserts the API surface).

        /// xorshift64 — fixed-seed deterministic PRNG. No `rand` dep needed.
        fn xorshift64(state: &mut u64) -> u64 {
            let mut x = *state;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *state = x;
            x
        }

        /// Generate n qualia + mantissas from a fixed seed. Touches all five
        /// dims read by the batch pipeline (valence, tension, warmth, coherence,
        /// groundedness) so the test exercises every decision branch.
        fn make_random_batch(n: usize, seed: u64) -> (Vec<QualiaI4_16D>, Vec<i8>) {
            let mut s = seed;
            let mut qualia = Vec::with_capacity(n);
            let mut mantissas = Vec::with_capacity(n);
            // 4-bit signed range: -8..=7
            let i4 = |bits: u8| -> i8 { ((bits & 0xF) << 4) as i8 >> 4 };
            for _ in 0..n {
                let r = xorshift64(&mut s);
                let mut q = QualiaI4_16D::ZERO;
                q.set(1, i4((r & 0xF) as u8)); // valence
                q.set(2, i4(((r >> 4) & 0xF) as u8)); // tension
                q.set(3, i4(((r >> 8) & 0xF) as u8)); // warmth
                q.set(9, i4(((r >> 12) & 0xF) as u8)); // coherence
                q.set(14, i4(((r >> 16) & 0xF) as u8)); // groundedness
                qualia.push(q);
                let mant = i4(((r >> 20) & 0xF) as u8);
                mantissas.push(mant);
            }
            (qualia, mantissas)
        }

        /// Sizes that exercise: (a) zero, (b) size-1 (tail-only), (c) sub-MIN_BATCH
        /// (scalar-only path on AVX-512 since min=8), (d) exact MIN_BATCH=8 (one
        /// full SIMD chunk + no tail), (e) MIN_BATCH+1=9 (one chunk + 1 scalar
        /// tail), (f) NEON MIN_BATCH+1=3, (g) large (forces many SIMD chunks).
        const PARITY_SIZES: &[usize] = &[0, 1, 3, 7, 8, 9, 15, 16, 64, 1024];

        #[test]
        fn test_dk_position_batch_parity_simd_vs_scalar() {
            for &n in PARITY_SIZES {
                let (qualia, mantissas) = make_random_batch(n, 0xD15C_5E7D_C0DE_0001);
                let mut out_dispatch = vec![DkPosition::MountStupid; n];
                let mut out_scalar = vec![DkPosition::MountStupid; n];
                batch::dk_position_batch(&qualia, &mantissas, &mut out_dispatch);
                batch::scalar_impl::dk_position_batch(&qualia, &mantissas, &mut out_scalar);
                for i in 0..n {
                    assert_eq!(
                        out_dispatch[i], out_scalar[i],
                        "dk_position_batch parity failure at size={} index={}: dispatch={:?} scalar={:?}",
                        n, i, out_dispatch[i], out_scalar[i],
                    );
                }
            }
        }

        #[test]
        fn test_trust_texture_batch_parity_simd_vs_scalar() {
            for &n in PARITY_SIZES {
                let (qualia, _) = make_random_batch(n, 0xD15C_5E7D_C0DE_0002);
                let mut out_dispatch = vec![TrustTexture::Uncertain; n];
                let mut out_scalar = vec![TrustTexture::Uncertain; n];
                batch::trust_texture_batch(&qualia, &mut out_dispatch);
                batch::scalar_impl::trust_texture_batch(&qualia, &mut out_scalar);
                for i in 0..n {
                    assert_eq!(
                        out_dispatch[i], out_scalar[i],
                        "trust_texture_batch parity failure at size={} index={}: dispatch={:?} scalar={:?}",
                        n, i, out_dispatch[i], out_scalar[i],
                    );
                }
            }
        }

        #[test]
        fn test_flow_state_batch_parity_simd_vs_scalar() {
            for &n in PARITY_SIZES {
                let (qualia, mantissas) = make_random_batch(n, 0xD15C_5E7D_C0DE_0003);
                let mut out_dispatch = vec![FlowState::Boredom; n];
                let mut out_scalar = vec![FlowState::Boredom; n];
                batch::flow_state_batch(&qualia, &mantissas, &mut out_dispatch);
                batch::scalar_impl::flow_state_batch(&qualia, &mantissas, &mut out_scalar);
                for i in 0..n {
                    assert_eq!(
                        out_dispatch[i], out_scalar[i],
                        "flow_state_batch parity failure at size={} index={}: dispatch={:?} scalar={:?}",
                        n, i, out_dispatch[i], out_scalar[i],
                    );
                }
            }
        }

        #[test]
        fn test_gate_decision_disc_batch_parity_simd_vs_scalar() {
            for &n in PARITY_SIZES {
                let (qualia, mantissas) = make_random_batch(n, 0xD15C_5E7D_C0DE_0004);
                let mut out_dispatch = vec![0u8; n];
                let mut out_scalar = vec![0u8; n];
                batch::gate_decision_disc_batch(&qualia, &mantissas, &mut out_dispatch);
                batch::scalar_impl::gate_decision_disc_batch(&qualia, &mantissas, &mut out_scalar);
                for i in 0..n {
                    assert_eq!(
                        out_dispatch[i], out_scalar[i],
                        "gate_decision_disc_batch parity failure at size={} index={}: dispatch={} scalar={}",
                        n, i, out_dispatch[i], out_scalar[i],
                    );
                }
                // Discriminants must be in the locked range 0=Flow, 1=Hold, 2=Block.
                for (i, &b) in out_dispatch.iter().enumerate() {
                    assert!(
                        b <= 2,
                        "out-of-range gate discriminant {} at index {}",
                        b,
                        i
                    );
                }
            }
        }

        #[test]
        fn test_mul_assess_batch_parity_simd_vs_scalar() {
            let zero_assess = || MulAssessment {
                trust: TrustQualia {
                    value: 0.0,
                    texture: TrustTexture::Calibrated,
                },
                dk_position: DkPosition::MountStupid,
                homeostasis: Homeostasis {
                    flow_state: FlowState::Boredom,
                    allostatic_load: 0.0,
                },
                complexity_mapped: false,
                free_will_modifier: 0.0,
            };
            for &n in PARITY_SIZES {
                let (qualia, mantissas) = make_random_batch(n, 0xD15C_5E7D_C0DE_0005);
                let mut out_dispatch: Vec<MulAssessment> = (0..n).map(|_| zero_assess()).collect();
                let mut out_scalar: Vec<MulAssessment> = (0..n).map(|_| zero_assess()).collect();
                batch::mul_assess_batch(&qualia, &mantissas, &mut out_dispatch);
                batch::scalar_impl::mul_assess_batch(&qualia, &mantissas, &mut out_scalar);
                for i in 0..n {
                    assert_eq!(
                        out_dispatch[i].dk_position, out_scalar[i].dk_position,
                        "mul_assess_batch dk_position mismatch at size={} i={}",
                        n, i,
                    );
                    assert_eq!(
                        out_dispatch[i].trust.texture, out_scalar[i].trust.texture,
                        "mul_assess_batch trust.texture mismatch at size={} i={}",
                        n, i,
                    );
                    assert_eq!(
                        out_dispatch[i].homeostasis.flow_state,
                        out_scalar[i].homeostasis.flow_state,
                        "mul_assess_batch flow_state mismatch at size={} i={}",
                        n,
                        i,
                    );
                    // f64 fields: bit-identical because both paths compute the same
                    // scalar finalize sequence with identical inputs.
                    assert!(
                        (out_dispatch[i].trust.value - out_scalar[i].trust.value).abs() < 1e-12,
                        "mul_assess_batch trust.value drift at size={} i={}: dispatch={} scalar={}",
                        n,
                        i,
                        out_dispatch[i].trust.value,
                        out_scalar[i].trust.value,
                    );
                    assert!(
                        (out_dispatch[i].free_will_modifier - out_scalar[i].free_will_modifier)
                            .abs()
                            < 1e-12,
                        "mul_assess_batch free_will_modifier drift at size={} i={}",
                        n,
                        i,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_default_input_is_calibratedish() {
        let mul = MulAssessment::compute(&SituationInput::default());
        assert!(mul.free_will_modifier >= 0.0 && mul.free_will_modifier <= 1.0);
        // Default is moderate competence; should NOT be Mount Stupid.
        assert_ne!(mul.dk_position, DkPosition::MountStupid);
    }

    #[test]
    fn compute_detects_mount_stupid() {
        let input = SituationInput {
            felt_competence: 0.95,
            demonstrated_competence: 0.10,
            ..SituationInput::default()
        };
        let mul = MulAssessment::compute(&input);
        assert_eq!(mul.dk_position, DkPosition::MountStupid);
        assert!(mul.is_unskilled_overconfident());
    }

    #[test]
    fn compute_detects_plateau() {
        let input = SituationInput {
            felt_competence: 0.85,
            demonstrated_competence: 0.85,
            source_reliability: 0.9,
            environment_stability: 0.9,
            calibration_accuracy: 0.9,
            challenge_level: 0.6,
            skill_level: 0.6,
            ..SituationInput::default()
        };
        let mul = MulAssessment::compute(&input);
        assert_eq!(mul.dk_position, DkPosition::Plateau);
        assert!(!mul.is_unskilled_overconfident());
    }
}
