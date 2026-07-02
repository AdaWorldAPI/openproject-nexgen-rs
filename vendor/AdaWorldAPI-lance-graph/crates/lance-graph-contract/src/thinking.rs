//! Thinking styles — the canonical 36 styles in 6 clusters.
//!
//! This is the SINGLE SOURCE OF TRUTH for thinking styles.
//! crewai-rust, n8n-rs, ladybug-rs, and lance-graph-planner
//! all consume these types. Nobody re-defines them.
//!
//! # Reconciliation
//!
//! Previously:
//! - crewai-rust had 36 styles / 6 clusters (Analytical, Creative, Empathic, Direct, Exploratory, Meta)
//! - lance-graph-planner had 12 styles / 4 clusters (Convergent, Divergent, Attention, Speed)
//!
//! Now: **36 styles / 6 clusters** is canonical. The planner's 12 styles
//! map to cluster representatives (one per cluster + attention/speed extras).
//! The planner uses `ThinkingStyle::to_planner_cluster()` internally.

/// 36 thinking styles organized into 6 clusters.
///
/// Each style maps to a τ (tau) macro address for JIT compilation
/// and a sparse 23D vector for cognitive matching and blending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ThinkingStyle {
    // Analytical Cluster (τ 0x40-0x4F)
    Logical = 0,
    Analytical = 1,
    Critical = 2,
    Systematic = 3,
    Methodical = 4,
    Precise = 5,

    // Creative Cluster (τ 0xA0-0xAF)
    Creative = 6,
    Imaginative = 7,
    Innovative = 8,
    Artistic = 9,
    Poetic = 10,
    Playful = 11,

    // Empathic Cluster (τ 0x80-0x8F)
    Empathetic = 12,
    Compassionate = 13,
    Supportive = 14,
    Nurturing = 15,
    Gentle = 16,
    Warm = 17,

    // Direct Cluster (τ 0x60-0x6F)
    Direct = 18,
    Concise = 19,
    Efficient = 20,
    Pragmatic = 21,
    Blunt = 22,
    Frank = 23,

    // Exploratory Cluster (τ 0x20-0x2F)
    Curious = 24,
    Exploratory = 25,
    Questioning = 26,
    Investigative = 27,
    Speculative = 28,
    Philosophical = 29,

    // Meta Cluster (τ 0xC0-0xCF)
    Reflective = 30,
    Contemplative = 31,
    Metacognitive = 32,
    Wise = 33,
    Transcendent = 34,
    Sovereign = 35,
}

/// The 6 style clusters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleCluster {
    Analytical,
    Creative,
    Empathic,
    Direct,
    Exploratory,
    Meta,
}

/// Planner cluster (coarser grouping used by lance-graph-planner cost model).
///
/// Maps 6 behavioral clusters → 4 planner clusters:
/// - Analytical + Direct → Convergent (depth-first, precise)
/// - Creative + Exploratory → Divergent (breadth-first, exploratory)
/// - Empathic → Attention (focus allocation, peripheral awareness)
/// - Meta → Speed (System 1 vs System 2 deliberation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlannerCluster {
    Convergent,
    Divergent,
    Attention,
    Speed,
}

impl ThinkingStyle {
    /// All 36 styles in canonical order.
    pub const ALL: [ThinkingStyle; 36] = [
        Self::Logical,
        Self::Analytical,
        Self::Critical,
        Self::Systematic,
        Self::Methodical,
        Self::Precise,
        Self::Creative,
        Self::Imaginative,
        Self::Innovative,
        Self::Artistic,
        Self::Poetic,
        Self::Playful,
        Self::Empathetic,
        Self::Compassionate,
        Self::Supportive,
        Self::Nurturing,
        Self::Gentle,
        Self::Warm,
        Self::Direct,
        Self::Concise,
        Self::Efficient,
        Self::Pragmatic,
        Self::Blunt,
        Self::Frank,
        Self::Curious,
        Self::Exploratory,
        Self::Questioning,
        Self::Investigative,
        Self::Speculative,
        Self::Philosophical,
        Self::Reflective,
        Self::Contemplative,
        Self::Metacognitive,
        Self::Wise,
        Self::Transcendent,
        Self::Sovereign,
    ];

    /// Which behavioral cluster this style belongs to.
    pub fn cluster(&self) -> StyleCluster {
        match *self as u8 {
            0..=5 => StyleCluster::Analytical,
            6..=11 => StyleCluster::Creative,
            12..=17 => StyleCluster::Empathic,
            18..=23 => StyleCluster::Direct,
            24..=29 => StyleCluster::Exploratory,
            30..=35 => StyleCluster::Meta,
            _ => unreachable!(),
        }
    }

    /// Which planner cluster this maps to (for cost model decisions).
    pub fn planner_cluster(&self) -> PlannerCluster {
        match self.cluster() {
            StyleCluster::Analytical | StyleCluster::Direct => PlannerCluster::Convergent,
            StyleCluster::Creative | StyleCluster::Exploratory => PlannerCluster::Divergent,
            StyleCluster::Empathic => PlannerCluster::Attention,
            StyleCluster::Meta => PlannerCluster::Speed,
        }
    }

    /// τ (tau) macro address for JIT compilation.
    ///
    /// These addresses are used by n8n-rs CompiledStyleRegistry
    /// to look up compiled scan kernels.
    pub fn tau(&self) -> u8 {
        match self.cluster() {
            StyleCluster::Analytical => 0x40 + (*self as u8),
            StyleCluster::Creative => 0xA0 + (*self as u8 - 6),
            StyleCluster::Empathic => 0x80 + (*self as u8 - 12),
            StyleCluster::Direct => 0x60 + (*self as u8 - 18),
            StyleCluster::Exploratory => 0x20 + (*self as u8 - 24),
            StyleCluster::Meta => 0xC0 + (*self as u8 - 30),
        }
    }
}

/// 7D field modulation parameters.
///
/// Controls how the planner behaves for a given thinking style.
/// These are the "knobs" that thinking styles turn.
#[derive(Debug, Clone, Copy)]
pub struct FieldModulation {
    /// Resonance distance threshold (0.0 = exact, 1.0 = broad).
    pub resonance_threshold: f64,
    /// How many alternatives to explore per decision point.
    pub fan_out: usize,
    /// Bias toward depth-first search (0.0 = breadth, 1.0 = depth).
    pub depth_bias: f64,
    /// Bias toward breadth-first search (inverse of depth_bias).
    pub breadth_bias: f64,
    /// Tolerance for noisy/uncertain results (0.0 = strict, 1.0 = tolerant).
    pub noise_tolerance: f64,
    /// Speed vs quality tradeoff (0.0 = thorough, 1.0 = fast).
    pub speed_bias: f64,
    /// How much to explore novel paths (0.0 = exploit, 1.0 = explore).
    pub exploration: f64,
}

impl Default for FieldModulation {
    fn default() -> Self {
        Self {
            resonance_threshold: 0.5,
            fan_out: 4,
            depth_bias: 0.5,
            breadth_bias: 0.5,
            noise_tolerance: 0.3,
            speed_bias: 0.5,
            exploration: 0.3,
        }
    }
}

/// SIMD scan parameters derived from FieldModulation.
///
/// Used by ndarray scan kernels and jitson compiled pipelines.
#[derive(Debug, Clone, Copy)]
pub struct ScanParams {
    /// Hamming distance threshold.
    pub threshold: u32,
    /// Top-K results to return.
    pub top_k: u32,
    /// Prefetch distance in records.
    pub prefetch_ahead: u32,
    /// Bit mask for selective word scanning.
    pub filter_mask: u32,
}

impl FieldModulation {
    /// Convert to scan parameters for SIMD kernels.
    pub fn to_scan_params(&self) -> ScanParams {
        ScanParams {
            threshold: (self.resonance_threshold * 1000.0) as u32,
            top_k: self.fan_out as u32 * 10,
            prefetch_ahead: if self.speed_bias > 0.7 { 8 } else { 4 },
            filter_mask: if self.noise_tolerance > 0.5 {
                0xFFFF_FFFF
            } else {
                0xFFFF_FF00
            },
        }
    }

    /// Serialize to 7-byte fingerprint for BindSpace storage (prefix 0x0D).
    pub fn to_fingerprint(&self) -> [u8; 7] {
        [
            (self.resonance_threshold * 255.0) as u8,
            self.fan_out.min(255) as u8,
            (self.depth_bias * 255.0) as u8,
            (self.breadth_bias * 255.0) as u8,
            (self.noise_tolerance * 255.0) as u8,
            (self.speed_bias * 255.0) as u8,
            (self.exploration * 255.0) as u8,
        ]
    }
}

/// Sparse 23D cognitive vector.
///
/// Used for style matching, blending, and affinity scoring.
/// Indices map to the 23 cognitive dimensions defined in crewai-rust.
pub type SparseVec = Vec<(usize, f32)>;

/// Trait for providing thinking style vectors.
///
/// crewai-rust implements this with its full 23D vector table.
/// lance-graph-planner implements this with a simplified 7D projection.
pub trait ThinkingStyleProvider: Send + Sync {
    /// Get the sparse 23D vector for a style.
    fn style_vector(&self, style: ThinkingStyle) -> SparseVec;

    /// Get the default field modulation for a style.
    fn default_modulation(&self, style: ThinkingStyle) -> FieldModulation;

    /// Select a thinking style from MUL assessment.
    fn select_from_assessment(&self, assessment: &super::mul::MulAssessment) -> ThinkingStyle;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_36_styles_canonical_order() {
        assert_eq!(ThinkingStyle::ALL.len(), 36);
        assert_eq!(ThinkingStyle::Logical as u8, 0);
        assert_eq!(ThinkingStyle::Sovereign as u8, 35);
    }

    #[test]
    fn test_cluster_mapping() {
        assert_eq!(
            ThinkingStyle::Analytical.cluster(),
            StyleCluster::Analytical
        );
        assert_eq!(ThinkingStyle::Creative.cluster(), StyleCluster::Creative);
        assert_eq!(ThinkingStyle::Empathetic.cluster(), StyleCluster::Empathic);
        assert_eq!(ThinkingStyle::Direct.cluster(), StyleCluster::Direct);
        assert_eq!(
            ThinkingStyle::Exploratory.cluster(),
            StyleCluster::Exploratory
        );
        assert_eq!(ThinkingStyle::Metacognitive.cluster(), StyleCluster::Meta);
    }

    #[test]
    fn test_planner_cluster_mapping() {
        // Analytical + Direct → Convergent
        assert_eq!(
            ThinkingStyle::Analytical.planner_cluster(),
            PlannerCluster::Convergent
        );
        assert_eq!(
            ThinkingStyle::Direct.planner_cluster(),
            PlannerCluster::Convergent
        );
        // Creative + Exploratory → Divergent
        assert_eq!(
            ThinkingStyle::Creative.planner_cluster(),
            PlannerCluster::Divergent
        );
        assert_eq!(
            ThinkingStyle::Exploratory.planner_cluster(),
            PlannerCluster::Divergent
        );
        // Empathic → Attention
        assert_eq!(
            ThinkingStyle::Empathetic.planner_cluster(),
            PlannerCluster::Attention
        );
        // Meta → Speed
        assert_eq!(
            ThinkingStyle::Metacognitive.planner_cluster(),
            PlannerCluster::Speed
        );
    }

    #[test]
    fn test_tau_addresses() {
        assert_eq!(ThinkingStyle::Logical.tau(), 0x40);
        assert_eq!(ThinkingStyle::Precise.tau(), 0x45);
        assert_eq!(ThinkingStyle::Creative.tau(), 0xA0);
        assert_eq!(ThinkingStyle::Empathetic.tau(), 0x80);
        assert_eq!(ThinkingStyle::Direct.tau(), 0x60);
        assert_eq!(ThinkingStyle::Curious.tau(), 0x20);
        assert_eq!(ThinkingStyle::Reflective.tau(), 0xC0);
        assert_eq!(ThinkingStyle::Sovereign.tau(), 0xC5);
    }

    #[test]
    fn test_field_modulation_to_scan_params() {
        let m = FieldModulation::default();
        let p = m.to_scan_params();
        assert_eq!(p.threshold, 500);
        assert_eq!(p.top_k, 40);
        assert_eq!(p.prefetch_ahead, 4);
    }

    #[test]
    fn test_field_modulation_fingerprint() {
        let m = FieldModulation {
            resonance_threshold: 1.0,
            fan_out: 8,
            depth_bias: 0.0,
            breadth_bias: 1.0,
            noise_tolerance: 0.5,
            speed_bias: 0.0,
            exploration: 1.0,
        };
        let fp = m.to_fingerprint();
        assert_eq!(fp[0], 255); // resonance_threshold
        assert_eq!(fp[1], 8); // fan_out
        assert_eq!(fp[2], 0); // depth_bias
        assert_eq!(fp[6], 255); // exploration
    }
}
