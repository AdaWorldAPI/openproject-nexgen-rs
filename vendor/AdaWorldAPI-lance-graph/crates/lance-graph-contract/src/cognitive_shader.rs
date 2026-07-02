//! Cognitive Shader DTO API — the shader IS the driver.
//!
//! # Role Reversal
//!
//! Before: thinking-engine drives, calls CognitiveShader as a helper.
//! Now:    CognitiveShader drives, dispatches thinking cycles, commits via sinks.
//!
//! The shader reads BindSpace columns (struct-of-arrays) through zero-copy
//! `ColumnView`s, scans the 8 predicate planes via bgz17 O(1) lookup, and
//! emits one `CycleFingerprint` per cycle — the unit of thought.
//!
//! # Layered DTO Flow
//!
//! ```text
//! Φ  ShaderDispatch  — request: which columns, which layers, which style
//! Ψ  ShaderResonance — ripple field: per-row energy + top-k hits
//! B  ShaderBus       — committed cycle: cycle_fingerprint + edges + gate
//! Γ  ShaderCrystal   — stabilized thought: BindSpace row + provenance
//! ```
//!
//! This file is **zero-dep**. Implementations live in `cognitive-shader-driver`.
//! The DTOs carry indices and packed u64/u32/u8 words, not allocations.
//!
//! # EmbedAnything Patterns Applied
//!
//! - **Commit sinks** — `ShaderSink` trait; drivers dispatch through it
//! - **Auto-detect** — `StyleSelector::Auto` routes by qualia shape
//! - **Builder** — `ShaderConfig` fluent-construction (owning driver builder)
//! - **Feature gates** — consumers opt into compile-time capabilities
//! - **No forward pass at runtime** — bgz17 distance IS precomputed

use crate::collapse_gate::{GateDecision, MergeMode};

// ═══════════════════════════════════════════════════════════════════════════
// Packed meta column — the cheap prefilter
// ═══════════════════════════════════════════════════════════════════════════

/// Packed u32 per row: `thinking(6) + awareness(4) + nars_f(8) + nars_c(8) + free_e(6)`.
///
/// Read cost is one u32 load per row. Applied before any fingerprint
/// load, so the majority of BindSpace is filtered cheaply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct MetaWord(pub u32);

impl MetaWord {
    #[inline]
    pub const fn new(thinking: u8, awareness: u8, nars_f: u8, nars_c: u8, free_e: u8) -> Self {
        let w = (thinking as u32 & 0x3F)
            | (((awareness as u32) & 0x0F) << 6)
            | ((nars_f as u32) << 10)
            | ((nars_c as u32) << 18)
            | (((free_e as u32) & 0x3F) << 26);
        Self(w)
    }
    #[inline]
    pub fn thinking(&self) -> u8 {
        (self.0 & 0x3F) as u8
    }
    #[inline]
    pub fn awareness(&self) -> u8 {
        ((self.0 >> 6) & 0x0F) as u8
    }
    #[inline]
    pub fn nars_f(&self) -> u8 {
        ((self.0 >> 10) & 0xFF) as u8
    }
    #[inline]
    pub fn nars_c(&self) -> u8 {
        ((self.0 >> 18) & 0xFF) as u8
    }
    #[inline]
    pub fn free_e(&self) -> u8 {
        ((self.0 >> 26) & 0x3F) as u8
    }
}

/// Prefilter predicate applied to the MetaColumn before any fingerprint load.
/// All fields are AND-combined; `u8::MAX`/`u8::MIN` act as "don't care" bounds.
#[derive(Clone, Copy, Debug)]
pub struct MetaFilter {
    pub thinking_mask: u64, // bitset over 64 possible styles; 0 = accept all
    pub awareness_min: u8,  // 0 = accept all
    pub nars_f_min: u8,     // frequency lower bound
    pub nars_c_min: u8,     // confidence lower bound
    pub free_e_max: u8,     // free-energy ceiling (63 = accept all)
}

impl MetaFilter {
    pub const ALL: Self = Self {
        thinking_mask: 0,
        awareness_min: 0,
        nars_f_min: 0,
        nars_c_min: 0,
        free_e_max: 63,
    };

    #[inline]
    pub fn accepts(&self, w: MetaWord) -> bool {
        let style_ok =
            self.thinking_mask == 0 || (self.thinking_mask & (1u64 << (w.thinking() & 0x3F))) != 0;
        style_ok
            && w.awareness() >= self.awareness_min
            && w.nars_f() >= self.nars_f_min
            && w.nars_c() >= self.nars_c_min
            && w.free_e() <= self.free_e_max
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Column views — zero-copy borrow into BindSpace struct-of-arrays
// ═══════════════════════════════════════════════════════════════════════════

/// Read-only window into a BindSpace column.
/// Drivers hand these to the shader without copying.
///
/// `start..end` is a half-open row range; `stride` is word-level
/// offset for column packing (fingerprint = 256 words, qualia = 18 f32s).
#[derive(Clone, Copy, Debug)]
pub struct ColumnWindow {
    pub start: u32,
    pub end: u32,
}

impl ColumnWindow {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
    pub const fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }
    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Style selector — auto-detect from qualia or explicit ordinal
// ═══════════════════════════════════════════════════════════════════════════

/// Which thinking style to run. `Auto` asks the driver to pick one from qualia
/// (valence, activation, dominance, depth, certainty, urgency…).
#[derive(Clone, Copy, Debug)]
pub enum StyleSelector {
    Ordinal(u8),
    Named(&'static str),
    /// Route from qualia shape. Drivers use a 18D → style map.
    Auto,
}

// ═══════════════════════════════════════════════════════════════════════════
// Rung level — semantic depth elevation (0..9)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, Default)]
#[repr(u8)]
pub enum RungLevel {
    #[default]
    Surface = 0,
    Shallow = 1,
    Contextual = 2,
    Analogical = 3,
    Abstract = 4,
    Structural = 5,
    Counterfactual = 6,
    Meta = 7,
    Recursive = 8,
    Transcendent = 9,
}

// ═══════════════════════════════════════════════════════════════════════════
// Φ ShaderDispatch — request into the cycle
// ═══════════════════════════════════════════════════════════════════════════

/// Cycle request. Small, Copy-friendly. All heavy state (BindSpace columns,
/// semiring, engine) is held by the driver, not embedded here.
#[derive(Clone, Copy, Debug)]
pub struct ShaderDispatch {
    /// Cheap prefilter on the packed u32 meta column.
    pub meta_prefilter: MetaFilter,
    /// Row window — shader sweeps this slice of BindSpace.
    pub rows: ColumnWindow,
    /// 8 predicate planes (CAUSES..BECOMES). 0xFF = all layers.
    pub layer_mask: u8,
    /// bgz17 distance cutoff.
    pub radius: u16,
    /// Style selection (may be Auto).
    pub style: StyleSelector,
    /// Semantic rung (elevates on sustained BLOCK).
    pub rung: RungLevel,
    /// Maximum cycles before forced commit (thinking-engine budget).
    pub max_cycles: u16,
    /// Entropy cutoff for early convergence.
    pub entropy_floor: f32,
    /// Commit mode.
    pub emit: EmitMode,
    /// Pillar-7: optional override of the [7] sink stage's `MergeMode`.
    ///
    /// `None` (the default) keeps the existing top-K aggregation in
    /// stage [7]. `Some(MergeMode::AlphaFrontToBack)` runs the
    /// Kerbl-style α-compositing loop and writes the result to
    /// `ShaderCrystal::alpha_composite`. Other modes preserve their
    /// existing semantics.
    pub merge_override: Option<MergeMode>,
    /// Pillar-7: per-dispatch saturation threshold for early-ray-termination
    /// in `MergeMode::AlphaFrontToBack`. `None` falls back to
    /// [`crate::collapse_gate::ALPHA_SATURATION_THRESHOLD`] (0.99).
    pub alpha_saturation_override: Option<f32>,
}

impl Default for ShaderDispatch {
    fn default() -> Self {
        Self {
            meta_prefilter: MetaFilter::ALL,
            rows: ColumnWindow::new(0, 0),
            layer_mask: 0xFF,
            radius: u16::MAX,
            style: StyleSelector::Auto,
            rung: RungLevel::Surface,
            max_cycles: 10,
            entropy_floor: 0.05,
            emit: EmitMode::Cycle,
            merge_override: None,
            alpha_saturation_override: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EmitMode {
    /// Emit cycle_fingerprint only (hot path, no persistence).
    Cycle = 0,
    /// Emit cycle_fingerprint + bundle of top-k hits.
    Bundle = 1,
    /// Commit to BindSpace via CollapseGate (persistent).
    Persist = 2,
}

// ═══════════════════════════════════════════════════════════════════════════
// Ψ ShaderResonance — ripple field top-k summary
// ═══════════════════════════════════════════════════════════════════════════

/// Per-hit record (bgz17 distance + predicate mask + cycle energy).
/// 16 bytes, fits 4 per cache line.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShaderHit {
    pub row: u32,
    pub distance: u16,
    pub predicates: u8,
    pub _pad: u8,
    pub resonance: f32,
    pub cycle_index: u32,
}

impl ShaderHit {
    /// Pillar-7 mapping: hit confidence → α coefficient ∈ [0, 1].
    ///
    /// The shader currently encodes confidence in the `resonance` field
    /// (0.0..1.0 by construction — Hamming-derived in the content
    /// pre-pass and 1/(1+d/dmax) in the cascade), and via the row's
    /// NARS truth payload elsewhere. We use `resonance` directly because
    /// it is what the front-to-back loop in the [7] sink stage has in
    /// hand at composite time, and clamp to [0, 1] defensively — NaN,
    /// negative, or out-of-range values return 0.0 (a fully transparent
    /// contribution that does not advance α_acc).
    #[inline]
    pub fn confidence_to_alpha(&self) -> f32 {
        if self.resonance.is_finite() {
            self.resonance.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Top-K hits + cycle statistics. Fixed-size = no allocation on hot path.
#[derive(Clone, Copy, Debug)]
pub struct ShaderResonance {
    pub top_k: [ShaderHit; 8],
    pub hit_count: u16,
    pub cycles_used: u16,
    pub entropy: f32,
    pub std_dev: f32,
    /// Chosen style ordinal (useful when selector was Auto).
    pub style_ord: u8,
}

impl Default for ShaderResonance {
    fn default() -> Self {
        Self {
            top_k: [ShaderHit::default(); 8],
            hit_count: 0,
            cycles_used: 0,
            entropy: 0.0,
            std_dev: 0.0,
            style_ord: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AlphaComposite — Pillar-7 front-to-back α-merge result
// ═══════════════════════════════════════════════════════════════════════════

/// Active payload dimensionality for the α-composite color accumulator.
/// Sized to a 32-slot fixed array so `ShaderCrystal` stays Clone+Copy-cheap.
/// The active prefix matches `BindSpace`'s qualia column (currently 18 f32);
/// trailing slots are zero by construction.
pub const ALPHA_COMPOSITE_DIMS: usize = 32;

/// Pillar-7 α-front-to-back composite output.
///
/// When `MergeMode::AlphaFrontToBack` is selected for the [7] sink stage,
/// the driver runs Kerbl-style EWA splatting over the resonance hits,
/// producing an accumulated qualia vector and total α. `hits_consumed`
/// records how many hits contributed before early-ray-termination
/// (or end-of-list). Zero hits → all-zero vector + α = 0.
#[derive(Clone, Copy, Debug)]
pub struct AlphaComposite {
    /// Composited qualia accumulator (front-to-back).
    pub color_acc: [f32; ALPHA_COMPOSITE_DIMS],
    /// Final accumulated α (∈ [0, 1]).
    pub alpha_acc: f32,
    /// Number of hits the loop consumed before saturation / end.
    pub hits_consumed: u16,
    /// Whether early-ray-termination fired (α exceeded saturation).
    pub saturated: bool,
}

impl Default for AlphaComposite {
    fn default() -> Self {
        Self {
            color_acc: [0.0f32; ALPHA_COMPOSITE_DIMS],
            alpha_acc: 0.0,
            hits_consumed: 0,
            saturated: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// B ShaderBus — committed cycle, what persists in A2A blackboard
// ═══════════════════════════════════════════════════════════════════════════

/// The committed cycle: the cycle_fingerprint IS the unit of thought.
/// 2 KB fingerprint + ~64 bytes of metadata.
#[derive(Clone, Debug)]
pub struct ShaderBus {
    /// The unit of thought — Layer-4 cycle signature.
    pub cycle_fingerprint: [u64; 256],
    /// CausalEdge64 emissions queued for persist.
    pub emitted_edges: [u64; 8],
    pub emitted_edge_count: u8,
    /// Layer 3 collapse decision.
    pub gate: GateDecision,
    pub resonance: ShaderResonance,
}

impl ShaderBus {
    pub fn empty() -> Self {
        Self {
            cycle_fingerprint: [0u64; 256],
            emitted_edges: [0u64; 8],
            emitted_edge_count: 0,
            gate: GateDecision::HOLD,
            resonance: ShaderResonance::default(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Γ ShaderCrystal — stabilized, persisted
// ═══════════════════════════════════════════════════════════════════════════

/// Crystallized outcome. Holds the assigned BindSpace row (if committed)
/// and a lazy hook to recover text via L1 tokenizer registry.
#[derive(Clone, Debug)]
pub struct ShaderCrystal {
    pub bus: ShaderBus,
    /// If `EmitMode::Persist`, this is the row assigned in BindSpace.
    pub persisted_row: Option<u32>,
    /// Meta assessment (Brier, confidence, should_admit_ignorance).
    pub meta: MetaSummary,
    /// Provenance of the side-run materialized-awareness analysis (the 34-tactic
    /// dispatch + HHTL fork). Provenance-only: does not affect `bus.gate`.
    pub materialize: MaterializeProvenance,
    /// Pillar-7 α-front-to-back composite, populated only when stage [7]
    /// dispatched on `MergeMode::AlphaFrontToBack`. `None` for the
    /// existing top-K aggregation modes (Bundle / Xor / Superposition).
    pub alpha_composite: Option<AlphaComposite>,
}

/// Meta-cognitive summary of the cycle.
#[derive(Clone, Copy, Debug, Default)]
pub struct MetaSummary {
    pub confidence: f32,
    pub meta_confidence: f32,
    pub brier: f32,
    pub should_admit_ignorance: bool,
}

/// Provenance of the materialized-awareness analysis run *alongside* the cycle.
///
/// **Provenance-only — does NOT alter the gate/emit decision.** The driver runs
/// the `materialize` F→34→F loop and the HHTL `fork_decision` as a side analysis
/// over the cycle's already-computed observables (`free_energy`, dispersion, MUL),
/// then records the outcome here. It answers "which of the 34 would this awareness
/// state dispatch, would the loop settle, and would the leaf residue fork to a new
/// domain" without changing hot-path semantics. Primitive-only so the contract
/// crate stays zero-dep (`fork` is `ForkAction as u8`, not the ndarray enum).
///
/// A zeroed value (`first_tactic == 0`) means the analysis did not run for this
/// cycle (e.g. a sink-aborted early return).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterializeProvenance {
    /// Tactic id (1..=34) the awareness state dispatched first; `0` = not run.
    pub first_tactic: u8,
    /// Dispatch steps the F→34→F loop took before settling (or hitting the cap).
    pub steps: u16,
    /// Did the loop settle into rest (gate FLOW and surprise below the floor)?
    pub rested: bool,
    /// Residual free energy at rest.
    pub final_free_energy: f32,
    /// HHTL fork action as `u8` (0 Commit, 1 DescendDeeper, 2 ForkBasin,
    /// 3 ForkDomain). CONJECTURE: the challenge is a dispersion (std_dev) proxy,
    /// pending the real orthogonal `CoarseResidue` magnitude from the codec path.
    pub fork: u8,
}

// ═══════════════════════════════════════════════════════════════════════════
// ShaderSink — EmbedAnything commit-adapter pattern
// ═══════════════════════════════════════════════════════════════════════════

/// Drivers dispatch cycle → `on_resonance` → `on_bus` → `on_crystal`.
/// Return `false` from any callback to short-circuit the cycle.
pub trait ShaderSink {
    fn on_resonance(&mut self, _r: &ShaderResonance) -> bool {
        true
    }
    fn on_bus(&mut self, _b: &ShaderBus) -> bool {
        true
    }
    fn on_crystal(&mut self, _c: &ShaderCrystal) {}
}

/// No-op sink. Useful as a default for drivers that don't want side effects.
pub struct NullSink;
impl ShaderSink for NullSink {}

// ═══════════════════════════════════════════════════════════════════════════
// Driver contract — what cognitive-shader-driver must implement
// ═══════════════════════════════════════════════════════════════════════════

/// The genius API: shader drives, BindSpace + engine follow.
pub trait CognitiveShaderDriver {
    /// Run one dispatch. Stateless w.r.t. the dispatch, stateful w.r.t. BindSpace.
    fn dispatch(&self, req: &ShaderDispatch) -> ShaderCrystal;

    /// Run with a sink for streaming callbacks.
    fn dispatch_with_sink<S: ShaderSink>(
        &self,
        req: &ShaderDispatch,
        sink: &mut S,
    ) -> ShaderCrystal;

    /// Current BindSpace row count.
    fn row_count(&self) -> u32;

    /// Report byte footprint (topology + metric + columns).
    fn byte_footprint(&self) -> usize;
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_word_packs_and_unpacks() {
        let w = MetaWord::new(31, 7, 200, 150, 12);
        assert_eq!(w.thinking(), 31);
        assert_eq!(w.awareness(), 7);
        assert_eq!(w.nars_f(), 200);
        assert_eq!(w.nars_c(), 150);
        assert_eq!(w.free_e(), 12);
    }

    #[test]
    fn meta_filter_accepts_when_default() {
        let w = MetaWord::new(0, 0, 0, 0, 0);
        assert!(MetaFilter::ALL.accepts(w));
    }

    #[test]
    fn meta_filter_rejects_low_nars() {
        let filter = MetaFilter {
            nars_c_min: 100,
            ..MetaFilter::ALL
        };
        let w = MetaWord::new(0, 0, 200, 50, 0);
        assert!(!filter.accepts(w));
    }

    #[test]
    fn meta_filter_style_mask() {
        let filter = MetaFilter {
            thinking_mask: 1u64 << 5,
            ..MetaFilter::ALL
        };
        assert!(filter.accepts(MetaWord::new(5, 0, 0, 0, 0)));
        assert!(!filter.accepts(MetaWord::new(6, 0, 0, 0, 0)));
    }

    #[test]
    fn dispatch_default_is_permissive() {
        let d = ShaderDispatch::default();
        assert_eq!(d.layer_mask, 0xFF);
        assert_eq!(d.max_cycles, 10);
        matches!(d.style, StyleSelector::Auto);
    }

    #[test]
    fn null_sink_is_noop() {
        let mut s = NullSink;
        assert!(s.on_resonance(&ShaderResonance::default()));
        assert!(s.on_bus(&ShaderBus::empty()));
        s.on_crystal(&ShaderCrystal {
            bus: ShaderBus::empty(),
            persisted_row: None,
            meta: MetaSummary::default(),
            materialize: MaterializeProvenance::default(),
            alpha_composite: None,
        });
    }

    #[test]
    fn column_window_len() {
        let w = ColumnWindow::new(10, 30);
        assert_eq!(w.len(), 20);
        assert!(!w.is_empty());
        let empty = ColumnWindow::new(5, 5);
        assert!(empty.is_empty());
    }

    #[test]
    fn bus_empty_is_hold() {
        let b = ShaderBus::empty();
        assert!(b.gate.is_hold());
        assert_eq!(b.emitted_edge_count, 0);
    }
}
