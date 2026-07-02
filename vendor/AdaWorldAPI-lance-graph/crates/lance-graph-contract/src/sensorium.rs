//! Graph Sensorium contract — real-time health signals from the knowledge graph.
//!
//! Defines the signal shapes and healing action types that any consumer
//! can use to monitor and nudge the thinking engine.
//!
//! lance-graph arigraph **produces** these signals from the live graph.
//! Consumers (q2, crewai-rust, n8n-rs) **read** them and can **request** healing.
//! In a single binary (q2 + lance-graph + ndarray), this is a direct function call.

use crate::mul::{DkPosition, FlowState, TrustTexture};

/// Real-time signals from the knowledge graph.
///
/// Each field is [0, 1] normalized. Produced by `arigraph::sensorium::GraphSensorium::from_graph()`.
/// Consumed by MUL assessment to determine DK position, trust, flow state.
#[derive(Debug, Clone, Copy)]
pub struct GraphSignals {
    /// Contradiction rate: contradictions / active_triplets.
    pub contradiction_rate: f32,
    /// Truth entropy: Shannon entropy of confidence distribution.
    pub truth_entropy: f32,
    /// Revision velocity: revisions per step.
    pub revision_velocity: f32,
    /// Plasticity flux: fraction of entities in transition.
    pub plasticity_flux: f32,
    /// Deduction yield: inferred / attempted.
    pub deduction_yield: f32,
    /// Episodic saturation: count / capacity.
    pub episodic_saturation: f32,
}

impl Default for GraphSignals {
    fn default() -> Self {
        Self {
            contradiction_rate: 0.0,
            truth_entropy: 0.0,
            revision_velocity: 0.0,
            plasticity_flux: 0.0,
            deduction_yield: 0.0,
            episodic_saturation: 0.0,
        }
    }
}

/// Graph-suggested cognitive bias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphBias {
    /// High contradictions → resolution focus.
    Resolve,
    /// High entropy → evidence gathering.
    Explore,
    /// Consistent rich graph → knowledge exploitation.
    Exploit,
    /// High plasticity → stay flexible.
    Adapt,
    /// Low revision + high entropy → stuck, needs perturbation.
    Stagnant,
    /// Normal operation.
    Balanced,
}

/// Derive bias from signals (pure function, no graph access).
pub fn suggested_bias(signals: &GraphSignals) -> GraphBias {
    if signals.contradiction_rate > 0.3 {
        GraphBias::Resolve
    } else if signals.truth_entropy > 0.7 {
        GraphBias::Explore
    } else if signals.deduction_yield > 0.5 && signals.truth_entropy < 0.3 {
        GraphBias::Exploit
    } else if signals.plasticity_flux > 0.5 {
        GraphBias::Adapt
    } else if signals.revision_velocity < 0.05 && signals.truth_entropy > 0.4 {
        GraphBias::Stagnant
    } else {
        GraphBias::Balanced
    }
}

/// Healing action types the graph immune system can apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealingType {
    /// Set very-low-confidence triplets to weak prior.
    BootstrapTruth,
    /// Halve confidence of contradicting pairs.
    ResolveContradictions,
    /// Run NARS deduction to fill missing links.
    InferMissingLinks,
    /// Remove soft-deleted triplets.
    CompactDeleted,
    /// Scale confidences to prevent inflation.
    NormalizeTruth,
}

/// Agent style for the orchestration loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentStyle {
    /// Plan agent → Analytical thinking.
    Plan,
    /// Action agent → Focused execution.
    Act,
    /// Exploration agent → Divergent discovery.
    Explore,
    /// Reflex agent → Metacognitive revision.
    Reflex,
}

/// Orchestrator mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorMode {
    /// NARS topology drives style selection.
    Adaptive,
    /// Classic plan→act→explore→reflex loop.
    HardcodedFallback,
}

/// Orchestrator status snapshot — what any consumer can read.
#[derive(Debug, Clone)]
pub struct OrchestratorStatus {
    /// Current mode.
    pub mode: OrchestratorMode,
    /// Total steps executed.
    pub step_count: u64,
    /// Rolling efficiency [0, 1].
    pub rolling_efficiency: f32,
    /// Last selected style.
    pub last_style: Option<AgentStyle>,
    /// Current DK position (from MUL).
    pub dk_position: DkPosition,
    /// Current flow state (from MUL).
    pub flow_state: FlowState,
    /// Current trust texture.
    pub trust_texture: TrustTexture,
    /// Free will modifier [0, 1.5].
    pub free_will_modifier: f32,
    /// Temperature (stagnation noise injection) [0, 1].
    pub temperature: f32,
    /// Graph bias (from sensorium).
    pub graph_bias: GraphBias,
    /// Steps in current mode.
    pub steps_in_current_mode: usize,
}

/// Result of one orchestrator step.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Which style was selected.
    pub style: AgentStyle,
    /// Current mode.
    pub mode: OrchestratorMode,
    /// Why this style was selected.
    pub reason: StepReason,
    /// Free will modifier at selection time.
    pub free_will_modifier: f32,
    /// Step number.
    pub step: u64,
}

/// Why a style was chosen.
#[derive(Debug, Clone)]
pub enum StepReason {
    /// NARS topology exploit (highest expected quality × MUL free_will).
    TopologyExploit {
        expected_quality: f64,
        confidence: f64,
    },
    /// Topology explore (least-observed edge for information gain).
    TopologyExplore { observations: u64 },
    /// Hardcoded sequence position.
    HardcodedSequence { position: usize },
    /// MUL compass/DK override.
    MulOverride {
        dk: DkPosition,
        explanation: &'static str,
    },
}

/// Trait for graph sensorium providers.
///
/// lance-graph arigraph implements this. Any consumer can call it.
pub trait SensoriumProvider: Send + Sync {
    /// Read current graph health signals.
    fn signals(&self) -> GraphSignals;

    /// Diagnose what healing the graph needs.
    fn diagnose(&self) -> Vec<HealingType>;

    /// Apply healing actions to the graph. Returns triplets modified.
    fn heal(&mut self, actions: &[HealingType]) -> usize;
}

/// Trait for orchestrator providers.
///
/// lance-graph arigraph implements this. Consumers call it to step and nudge.
pub trait OrchestratorProvider: Send + Sync {
    /// Get current orchestrator status.
    fn status(&self) -> OrchestratorStatus;

    /// Select and return the next style to execute.
    fn step(&mut self) -> StepResult;

    /// Record the outcome of the last step (drives NARS RL).
    fn record_outcome(&mut self, style: AgentStyle, quality: f32);

    /// Feed graph signals into MUL (call before step() for self-regulation).
    fn update_signals(&mut self, signals: GraphSignals);

    /// Run auto-heal contingency. Returns healing actions to apply.
    fn auto_heal(&mut self) -> Vec<HealingType>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_signals() {
        let s = GraphSignals::default();
        assert_eq!(suggested_bias(&s), GraphBias::Balanced);
    }

    #[test]
    fn test_contradiction_resolves() {
        let s = GraphSignals {
            contradiction_rate: 0.4,
            ..Default::default()
        };
        assert_eq!(suggested_bias(&s), GraphBias::Resolve);
    }

    #[test]
    fn test_high_entropy_explores() {
        let s = GraphSignals {
            truth_entropy: 0.8,
            ..Default::default()
        };
        assert_eq!(suggested_bias(&s), GraphBias::Explore);
    }

    #[test]
    fn test_stagnant_detection() {
        let s = GraphSignals {
            revision_velocity: 0.02,
            truth_entropy: 0.6,
            ..Default::default()
        };
        assert_eq!(suggested_bias(&s), GraphBias::Stagnant);
    }
}
