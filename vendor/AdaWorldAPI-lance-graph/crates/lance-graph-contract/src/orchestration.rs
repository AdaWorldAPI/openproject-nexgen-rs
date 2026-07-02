//! Orchestration bridge contract.
//!
//! This is THE key trait that replaces duplicated routing logic
//! across crewai-rust (StepRouter), n8n-rs (crew_router/ladybug_router),
//! and ladybug-rs (HybridEngine).
//!
//! # The Problem
//!
//! Before this contract:
//! - crewai-rust had StepRouter with StepDomain enum
//! - n8n-rs had crew_router.rs + ladybug_router.rs (HTTP proxies)
//! - ladybug-rs had HybridEngine with vector+cypher+temporal
//! - Each system re-invented step routing and thinking mode dispatch
//!
//! # The Solution
//!
//! One trait: `OrchestrationBridge`. Implemented ONCE in lance-graph.
//! Consumed by all three systems. In a single binary, this is a
//! direct function call. In multi-process, this is Arrow Flight.
//!
//! ```text
//! crewai-rust ──┐
//!               ├──► OrchestrationBridge (trait) ──► lance-graph (impl)
//! n8n-rs ───────┤
//!               │
//! ladybug-rs ───┘
//! ```

use crate::nars::InferenceType;
use crate::plan::ThinkingContext;
use crate::thinking::ThinkingStyle;

/// Step domain: which subsystem handles this step.
///
/// Replaces crewai-rust's StepDomain enum AND n8n-rs's routing logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepDomain {
    /// crewai-rust agent execution.
    Crew,
    /// ladybug-rs BindSpace / cognitive operations.
    Ladybug,
    /// n8n-rs workflow orchestration.
    N8n,
    /// lance-graph query execution.
    LanceGraph,
    /// Direct ndarray SIMD operation.
    Ndarray,
    /// SMB entity operations (outside BBB — boringly agnostic).
    Smb,
    /// Medcare reality-check vertical (clinic data sovereignty).
    Medcare,
    /// 4-phase Rubicon kanban transition over the per-mailbox SoA — the seam
    /// where the planner (emits), ractor (owns/drives), and surrealdb (projects)
    /// meet. `step_type` prefix `"kanban."`. See [`crate::kanban`].
    Kanban,
}

impl StepDomain {
    /// Parse step type prefix to domain.
    ///
    /// ```text
    /// "crew.agent.think" → Crew
    /// "lb.resonate"      → Ladybug
    /// "n8n.set"          → N8n
    /// "lg.cypher"        → LanceGraph
    /// "nd.hamming"       → Ndarray
    /// "medcare.check"    → Medcare
    /// ```
    pub fn from_step_type(step_type: &str) -> Option<Self> {
        let prefix = step_type.split('.').next()?;
        match prefix {
            "crew" => Some(Self::Crew),
            "lb" => Some(Self::Ladybug),
            "n8n" => Some(Self::N8n),
            "lg" => Some(Self::LanceGraph),
            "nd" => Some(Self::Ndarray),
            "smb" => Some(Self::Smb),
            "medcare" => Some(Self::Medcare),
            "kanban" => Some(Self::Kanban),
            _ => None,
        }
    }

    /// Per-domain orchestration profile (E5 from PR #278 outlook).
    ///
    /// `StepDomain` is the seam for vertical-specific orchestration:
    /// verb taxonomy, calibration thresholds, retention windows, and
    /// escalation defaults are picked HERE so downstream code does not
    /// hard-code Medcare-vs-SMB conditionals at every call site.
    ///
    /// Profiles are STATIC defaults — the runtime can override via the
    /// membrane registry without changing the enum. Tune empirically
    /// per deployment; the values below are conservative starters.
    pub fn profile(&self) -> DomainProfile {
        match self {
            Self::Smb => DomainProfile {
                audit_retention_days: 90,
                auto_action_confidence: 0.75,
                escalation: Escalation::Llm,
                requires_fail_closed: false,
                verb_taxonomy: VerbTaxonomyId::Smb,
            },
            Self::Medcare => DomainProfile {
                // 6 years (HIPAA §164.316(b)(2)(i)) — starter, tune empirically.
                audit_retention_days: 2190,
                auto_action_confidence: 0.92,
                escalation: Escalation::Human,
                requires_fail_closed: true,
                verb_taxonomy: VerbTaxonomyId::Medcare,
            },
            // Generic defaults for infrastructure / orchestration domains.
            // These are NOT vertical-facing; they execute the cycle, not
            // the policy. Starter values — tune empirically.
            Self::Crew
            | Self::Ladybug
            | Self::N8n
            | Self::LanceGraph
            | Self::Ndarray
            | Self::Kanban => DomainProfile {
                audit_retention_days: 30,
                auto_action_confidence: 0.70,
                escalation: Escalation::Llm,
                requires_fail_closed: false,
                verb_taxonomy: VerbTaxonomyId::Generic,
            },
        }
    }
}

impl core::fmt::Display for StepDomain {
    /// Lowercase form mirroring `from_step_type` keys exactly.
    /// `from_step_type(&domain.to_string()) == Some(domain)` for every variant.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::Crew => "crew",
            Self::Ladybug => "lb",
            Self::N8n => "n8n",
            Self::LanceGraph => "lg",
            Self::Ndarray => "nd",
            Self::Smb => "smb",
            Self::Medcare => "medcare",
            Self::Kanban => "kanban",
        };
        f.write_str(s)
    }
}

/// Per-domain orchestration profile. Carries calibration thresholds,
/// retention windows, escalation defaults, and verb-taxonomy markers.
///
/// Profiles are STATIC defaults — runtime can override via the membrane
/// registry without changing the enum.
#[derive(Debug, Clone, Copy)]
pub struct DomainProfile {
    /// Audit retention in days. Medcare (HIPAA) = 6 years (2190); SMB = 90.
    pub audit_retention_days: u32,
    /// Confidence threshold above which automated actions are allowed
    /// without human review. Medcare requires higher threshold.
    pub auto_action_confidence: f32,
    /// Escalation target on uncertainty: Llm = degrade to LLM tail;
    /// Human = require human-in-the-loop; Reject = fail closed.
    pub escalation: Escalation,
    /// Whether this domain demands fail-closed access control.
    /// Medcare = true (HIPAA); SMB = false (commerce).
    pub requires_fail_closed: bool,
    /// Verb taxonomy id — picks which 144-cell verb table to consult.
    pub verb_taxonomy: VerbTaxonomyId,
}

/// Escalation target on uncertainty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escalation {
    /// Degrade to LLM tail (best-effort, no human in loop).
    Llm,
    /// Require human-in-the-loop (HIPAA-grade verticals default here).
    Human,
    /// Fail closed — reject the step rather than guess.
    Reject,
}

/// Verb taxonomy identifier — selects the per-domain 144-cell verb table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerbTaxonomyId {
    /// 12 generic semantic families (BECOMES, CAUSES, SUPPORTS, ...).
    Generic,
    /// SMB-specific: invoice, quote, dispatch, fulfill, return, refund, ...
    Smb,
    /// Medcare-specific: prescribe, refer, discharge, admit, treat, diagnose, ...
    Medcare,
}

/// Compute a Trajectory-aware audit hash for a step within this domain.
///
/// This is the cross-PR bridge between PR #279's grammar substrate and
/// PR #278's audit log: the trajectory becomes the audit key, replacing
/// the syntactic statement_hash.
///
/// PR #279 epiphany E4. Implementation lands in the bridge PR.
///
/// META-AGENT: feature-gated stub. Do NOT call until the bridge PR
/// implements it; signature is locked here so callers can compile-test
/// against the trajectory-audit feature flag.
#[cfg(feature = "trajectory-audit")]
pub fn step_trajectory_hash(
    _domain: StepDomain,
    _step: &UnifiedStep,
    _trajectory: &[u64; 256],
) -> u64 {
    unimplemented!("see PR #279 outlook E4")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_domain_medcare_constructs_and_matches() {
        let d = StepDomain::Medcare;
        // Pattern-match smoke test — proves the variant is reachable.
        let routed = matches!(d, StepDomain::Medcare);
        assert!(routed);
        // Distinct from sibling domains.
        assert_ne!(StepDomain::Medcare, StepDomain::Crew);
        assert_ne!(StepDomain::Medcare, StepDomain::Smb);
    }

    #[test]
    fn step_domain_medcare_from_step_type() {
        assert_eq!(
            StepDomain::from_step_type("medcare.reality_check"),
            Some(StepDomain::Medcare),
        );
        assert_eq!(StepDomain::from_step_type("unknown.foo"), None);
    }

    #[test]
    fn display_round_trips_through_from_step_type() {
        // Every variant must serialize to a string that `from_step_type`
        // accepts and round-trips back. Keeps the Display impl honest.
        let all = [
            StepDomain::Crew,
            StepDomain::Ladybug,
            StepDomain::N8n,
            StepDomain::LanceGraph,
            StepDomain::Ndarray,
            StepDomain::Smb,
            StepDomain::Medcare,
            StepDomain::Kanban,
        ];
        for domain in all {
            let s = domain.to_string();
            assert_eq!(
                StepDomain::from_step_type(&s),
                Some(domain),
                "Display→from_step_type round-trip failed for {domain:?} (got {s:?})",
            );
        }
    }

    #[test]
    fn medcare_requires_fail_closed() {
        assert!(StepDomain::Medcare.profile().requires_fail_closed);
    }

    #[test]
    fn medcare_auto_action_threshold_higher_than_smb() {
        let medcare = StepDomain::Medcare.profile();
        let smb = StepDomain::Smb.profile();
        assert!(
            medcare.auto_action_confidence > smb.auto_action_confidence,
            "medcare ({}) must demand a higher auto-action confidence than smb ({})",
            medcare.auto_action_confidence,
            smb.auto_action_confidence,
        );
    }

    #[test]
    fn medcare_audit_retention_is_hipaa_grade() {
        // HIPAA §164.316(b)(2)(i) = 6 years = 2190 days.
        assert!(
            StepDomain::Medcare.profile().audit_retention_days >= 2190,
            "medcare audit retention must be >= 2190 days (HIPAA 6 years)",
        );
    }

    fn step_with_step_id(s: &str) -> UnifiedStep {
        UnifiedStep {
            step_id: s.to_string(),
            step_type: "lg.noop".to_string(),
            status: StepStatus::Pending,
            thinking: None,
            reasoning: None,
            confidence: None,
            depends_on: vec![],
        }
    }

    /// Regression test for the `id: 0` landmine: under the old design,
    /// every caller hard-coded `id: 0` so two distinct steps collided
    /// in the DAG (same StepId → DuplicateStepId or wrong dependency
    /// resolution). The fix derives `id()` from `step_id` via FNV-1a,
    /// so distinct `step_id` strings produce distinct numeric ids.
    #[test]
    fn test_distinct_step_ids_when_step_id_strings_differ() {
        let a = step_with_step_id("step-alpha");
        let b = step_with_step_id("step-beta");
        assert_ne!(
            a.id(),
            b.id(),
            "FNV-1a derivation must distinguish 'step-alpha' from 'step-beta'; \
             with the old `id: 0` field both would collide at zero",
        );
        // And the derivation must be deterministic — same string → same id.
        let a_again = step_with_step_id("step-alpha");
        assert_eq!(
            a.id(),
            a_again.id(),
            "id() must be deterministic over step_id"
        );
    }
}

/// Numeric step identifier for DAG dependency tracking.
///
/// Used by `UnifiedStep::id` and `UnifiedStep::depends_on` to express
/// execution ordering constraints. The pipeline executor in
/// `lance-graph-planner::pipeline` builds a topological sort from these.
pub type StepId = u64;

/// Unified step — the unit of work crossing system boundaries.
///
/// This is the canonical type. crewai-rust's UnifiedStep and
/// n8n-contract's UnifiedStep should both be replaced by this.
///
/// # Identity
///
/// The numeric `StepId` used for DAG edges is **derived** from
/// `step_id` via [`UnifiedStep::id`] (FNV-1a hash of the bytes).
/// There is no stored `id` field, so callers cannot hard-code
/// `id: 0` and accidentally collide all steps at the same node —
/// see the original `id: 0` landmine that motivated this design.
#[derive(Debug, Clone)]
pub struct UnifiedStep {
    pub step_id: String,
    pub step_type: String,
    pub status: StepStatus,
    /// Thinking context (if resolved by planner).
    pub thinking: Option<ThinkingContext>,
    /// Agent decision trail.
    pub reasoning: Option<String>,
    /// NARS confidence (0.0–1.0).
    pub confidence: Option<f64>,
    /// IDs of steps that must complete before this step can execute.
    /// Empty means the step has no prerequisites (root node in the DAG).
    pub depends_on: Vec<StepId>,
}

impl UnifiedStep {
    /// Numeric identifier for DAG dependency edges.
    ///
    /// Derived deterministically from `step_id` via FNV-1a hashing.
    /// Two steps with different `step_id` strings will produce
    /// different numeric ids with overwhelming probability — far
    /// better than the old `pub id: StepId` field that every caller
    /// initialized to `0`, collapsing the DAG to a single node.
    pub fn id(&self) -> StepId {
        crate::hash::fnv1a_str(&self.step_id)
    }
}

/// Step execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Orchestration bridge — the single routing contract.
///
/// Replaces:
/// - crewai-rust StepRouter.dispatch()
/// - n8n-rs crew_router.rs / ladybug_router.rs
/// - ladybug-rs HybridEngine
///
/// In a single binary, these are direct function calls.
/// In multi-process, these become Arrow Flight RPCs.
pub trait OrchestrationBridge: Send + Sync {
    /// Route a step to the appropriate subsystem.
    fn route(&self, step: &mut UnifiedStep) -> Result<(), OrchestrationError>;

    /// Resolve thinking context for a step (before routing).
    fn resolve_thinking(
        &self,
        style: ThinkingStyle,
        inference_type: InferenceType,
    ) -> ThinkingContext;

    /// Check if a domain is available (feature-gated in single binary).
    fn domain_available(&self, domain: StepDomain) -> bool;
}

/// Orchestration error.
#[derive(Debug, Clone)]
pub enum OrchestrationError {
    /// Domain not available.
    DomainUnavailable(StepDomain),
    /// Step routing failed.
    RoutingFailed(String),
    /// Step execution failed.
    ExecutionFailed(String),
}

impl core::fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DomainUnavailable(d) => write!(f, "Domain unavailable: {d:?}"),
            Self::RoutingFailed(s) => write!(f, "Routing failed: {s}"),
            Self::ExecutionFailed(s) => write!(f, "Execution failed: {s}"),
        }
    }
}

/// Blackboard slot contract — for TypedSlot interop.
///
/// crewai-rust's Blackboard TypedSlots can store any `dyn Any`.
/// This trait defines the contract for slots that cross the bridge.
pub trait BridgeSlot: Send + Sync + core::fmt::Debug {
    /// Slot key.
    fn key(&self) -> &str;
    /// Step type that produced this slot.
    fn step_type(&self) -> &str;
    /// Whether this is a TypedSlot (zero-serde) or JSON slot.
    fn is_typed(&self) -> bool;
}
