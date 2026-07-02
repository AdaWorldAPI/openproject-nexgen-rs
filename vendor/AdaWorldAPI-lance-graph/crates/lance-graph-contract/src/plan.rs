//! Query planning contract.
//!
//! Defines the traits that lance-graph-planner implements and
//! consumers (ladybug-rs, n8n-rs) call.

use crate::cognitive_shader::RungLevel;
use crate::mul::{GateDecision, MulAssessment, SituationInput};
use crate::nars::{InferenceType, SemiringChoice};
use crate::thinking::{FieldModulation, ThinkingStyle};

/// Thinking context — the full resolved state for one query.
///
/// Produced by `PlannerContract::orchestrate()`.
/// Consumed by strategy selection and physical planning.
#[derive(Debug, Clone)]
pub struct ThinkingContext {
    pub style: ThinkingStyle,
    pub modulation: FieldModulation,
    pub inference_type: InferenceType,
    pub strategy: crate::nars::QueryStrategy,
    pub semiring: SemiringChoice,
    /// Semantic rung — depth of causation (0..9); the Rung atom family.
    pub rung: RungLevel,
    pub free_will_modifier: f64,
    pub exploratory: bool,
}

/// Plan result — returned from the planner to the consumer.
#[derive(Debug, Clone)]
pub struct PlanResult {
    /// MUL assessment (if plan_full was called).
    pub mul: Option<MulAssessment>,
    /// Resolved thinking context.
    pub thinking: Option<ThinkingContext>,
    /// Names of strategies that were executed.
    pub strategies_used: Vec<String>,
    /// Free will modifier applied.
    pub free_will_modifier: f64,
    /// Compass score (if assessed).
    pub compass_score: Option<f64>,
    /// Emitted connectome edges — little-endian `u64` words
    /// (`CausalEdge64` / `EpisodicEdges64`), the radix key the vart/surreal
    /// seam persists. Empty until the collapse gate populates it.
    pub emitted_edges: Vec<u64>,
}

/// Query features detected during parsing.
#[derive(Debug, Clone, Default)]
pub struct QueryFeatures {
    pub has_graph_pattern: bool,
    pub has_fingerprint_scan: bool,
    pub has_variable_length_path: bool,
    pub has_aggregation: bool,
    pub has_mutation: bool,
    pub has_workflow: bool,
    pub has_resonance: bool,
    pub has_truth_values: bool,
    pub num_match_clauses: usize,
    pub num_nodes: usize,
    pub num_edges: usize,
    pub estimated_complexity: f64,
}

/// Plan error.
#[derive(Debug, Clone)]
pub enum PlanError {
    /// MUL gate blocked execution.
    GateBlocked { reason: String },
    /// Compass triggered surface-to-meta transition.
    SurfaceToMeta { compass_score: f64, reason: String },
    /// Parse error.
    Parse(String),
    /// Planning error.
    Plan(String),
    /// Optimization error.
    Optimize(String),
}

impl core::fmt::Display for PlanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::GateBlocked { reason } => write!(f, "MUL gate blocked: {reason}"),
            Self::SurfaceToMeta {
                compass_score,
                reason,
            } => write!(f, "Surface to meta (compass={compass_score:.3}): {reason}"),
            Self::Parse(s) => write!(f, "Parse: {s}"),
            Self::Plan(s) => write!(f, "Plan: {s}"),
            Self::Optimize(s) => write!(f, "Optimize: {s}"),
        }
    }
}

/// Strategy selector — how the planner picks strategies.
#[derive(Debug, Clone)]
pub enum StrategySelector {
    /// User names strategies explicitly.
    Explicit(Vec<String>),
    /// AGI selects based on thinking style + MUL.
    Resonance {
        thinking_style: Vec<f64>,
        mul_modifier: f64,
        compass_score: f64,
    },
    /// Auto: top-N per phase by affinity.
    Auto {
        max_per_phase: usize,
        min_affinity: f32,
    },
}

impl Default for StrategySelector {
    fn default() -> Self {
        Self::Auto {
            max_per_phase: 3,
            min_affinity: 0.1,
        }
    }
}

// =============================================================================
// THE CONTRACT TRAIT
// =============================================================================

/// The planner contract — the single trait that consumers depend on.
///
/// lance-graph-planner implements this. ladybug-rs, crewai-rust, n8n-rs
/// call it. Nobody else needs to know about the planner internals.
///
/// # Usage from ladybug-rs
///
/// ```rust,ignore
/// let planner: Box<dyn PlannerContract> = lance_graph_planner::create_planner();
/// let result = planner.plan_full(cypher_query, &situation)?;
/// ```
///
/// # Usage from n8n-rs (workflow node)
///
/// ```rust,ignore
/// let planner: Box<dyn PlannerContract> = lance_graph_planner::create_planner();
/// planner.set_selector(StrategySelector::Resonance { ... });
/// let result = planner.plan_auto(query)?;
/// ```
pub trait PlannerContract: Send + Sync {
    /// Plan with full MUL assessment pipeline.
    ///
    /// MUL → ThinkingStyle → Strategy selection → Plan.
    fn plan_full(&self, query: &str, situation: &SituationInput) -> Result<PlanResult, PlanError>;

    /// Plan without MUL (auto-select strategies).
    fn plan_auto(&self, query: &str) -> Result<PlanResult, PlanError>;

    /// Set the strategy selector.
    fn set_selector(&mut self, selector: StrategySelector);

    /// Orchestrate: resolve thinking context from query + MUL.
    fn orchestrate(&self, query: &str, mul: &MulAssessment) -> ThinkingContext;

    /// Gate check only (without full planning).
    fn gate_check(&self, situation: &SituationInput) -> GateDecision;
}
