//! Scenario branching — explicit counterfactual futures.
//!
//! `ScenarioBranch` is the named, first-class handle for divergent futures
//! over the same parent state. Lance dataset versioning gives us
//! **read-as-of** time travel; this module gives us **write-divergent**
//! branching with the gestalt diff and reproducibility semantics that
//! counterfactual simulation requires.
//!
//! # Architectural decision: why a thin facade, not a column or a new crate
//!
//! Earlier proposals considered (and rejected) two alternatives:
//!
//! 1. **`scenario_id` column on every BindSpace SoA + SPO row** (LF-71 v1).
//!    Rejected: widens every SIMD sweep by 8 bytes × N rows, duplicates
//!    Lance's native versioning, conflicts with the `I-VSA-IDENTITIES`
//!    iron rule. Scenario identity is *meta about which content version*,
//!    not content itself.
//! 2. **A new `lance-graph-scenario` crate.** Rejected: the four pieces
//!    that a scenario needs already exist (Pearl Rung 3 intervention in
//!    `lance-graph-cognitive::world::counterfactual`; dataset versioning
//!    plus diff in `lance-graph::graph::versioned::VersionedGraph`;
//!    archetype meta-state in `lance-graph-archetype::world::World`;
//!    full situational DTO in `world_model::WorldModelDto`). A new crate
//!    would re-state shape; a facade composes existing surfaces.
//!
//! # The four pieces this facade composes
//!
//! | Piece | Where | Role in scenario |
//! |---|---|---|
//! | `Intervention` (Pearl Rung 3) | `lance-graph-cognitive::world::counterfactual` | The "what if X were x'?" math via `bind/unbind` |
//! | `World { dataset_uri, tick }` | `lance-graph-archetype::world` | Named branch handle wrapping Lance dataset path + version |
//! | `VersionedGraph` (tag, at_version, diff) | `lance-graph::graph::versioned` | The actual storage substrate — ACID branching, time travel, diff |
//! | `WorldModelDto` (gestalt, qualia, ripple state) | `contract::world_model` | The situational snapshot a scenario is reasoning about |
//!
//! The contract crate stays zero-dep; this module declares only the
//! types and trait shape. Concrete implementations that touch
//! `VersionedGraph` live downstream in the planner / cognitive crates.
//!
//! # Reproducibility (Apache-Temporal-extracted method)
//!
//! Apache Temporal is the wrong tool for simulation (it's for workflow
//! replay-on-error). One useful idea ports: **deterministic replay** via
//! captured RNG seed at fork point. Every `ScenarioBranch` carries
//! `fork_seed`; replaying the branch with the same seed is guaranteed
//! to yield the same simulation trajectory.
//!
//! # Forecasting (Chronos-extracted method)
//!
//! Chronos itself (time-series-as-tokens via patch quantization) is too
//! primitive for our substrate. The portable idea: **chain palette
//! compose-table lookups** to forecast the next archetype. We already
//! have palette + ComposeTable in `bgz17`; `forecast_palette(branch, depth)`
//! exposes that as the in-cache forecaster (~2ns/step).
//!
//! # Read also
//!
//! - `.claude/knowledge/user-agent-topic-ripple-model.md` — the four-pole
//!   (user, agent, topic, angle) framing. `ScenarioBranch` is a spine
//!   fork in the ripple field; `diff_branches` IS the gestalt comparison
//!   (overlap + mismatch + unresolved tension).
//! - `.claude/knowledge/vsa-switchboard-architecture.md` — why scenario
//!   identity belongs in role-bind (catalogue) not column (content).
//! - `.claude/agents/scenario-world.md` — the specialist agent card with
//!   the full decision tree and rejected alternatives.

// ═══════════════════════════════════════════════════════════════════════════
// SCENARIO BRANCH — named handle for a divergent future
// ═══════════════════════════════════════════════════════════════════════════

/// A named, first-class handle for one divergent future over a parent
/// world state.
///
/// Composes:
/// - A name (human-readable, opaque to the substrate).
/// - A parent identifier (Lance version + tag) that locates the fork point.
/// - An optional archetype prior bundled into the trajectory.
/// - A captured RNG seed for deterministic replay.
/// - A list of interventions (Pearl Rung 3) applied in order.
/// - A default inference mode (typically `CounterfactualSynthesis`).
///
/// The branch itself does not own the storage. It is a descriptor that
/// downstream code (in `lance-graph-cognitive` or `lance-graph-planner`)
/// uses to drive `VersionedGraph::tag_version` + new dataset path,
/// `multi_intervene`, and forward simulation.
#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioBranch {
    /// Human-readable scenario name. e.g. `"recession_2027"`,
    /// `"customer_what_if_churn"`.
    pub name: String,

    /// Lance dataset version at the fork point. The branch's writes
    /// land in a separate dataset path; reads of the parent state
    /// resolve via `VersionedGraph::at_version(forked_from)`.
    pub forked_from: u64,

    /// Tag name on the parent dataset that pins the fork point.
    /// Created via `VersionedGraph::tag_version(name, forked_from)`
    /// at fork creation. Survives independently of this struct.
    pub parent_tag: String,

    /// Wall-clock timestamp (ms since epoch) at fork creation.
    /// Independent of the parent's logical version — useful for
    /// human telemetry and audit.
    pub forked_at: u64,

    /// Optional archetype prior. Indexes into the existing palette
    /// codebook (256 archetypes) or the archetype role-key catalogue
    /// (12 archetype families × 12 voice channels = 144 identities).
    /// When set, the archetype identity fingerprint bundles into every
    /// trajectory in this branch, biasing forward inference.
    pub archetype_prior: Option<u8>,

    /// Deterministic-replay seed captured at fork point.
    /// Ported from Apache Temporal's deterministic-replay semantics:
    /// re-running `simulate_forward` with this seed (and the same
    /// intervention list) yields the identical trajectory.
    pub fork_seed: u64,

    /// Default NARS inference type for forward simulation in this
    /// branch. Typically `CounterfactualSynthesis` (the 7th NARS
    /// inference type, slot `[9996..10000)` in role_keys).
    pub inference_mode: u8,

    /// Pearl Rung 3 interventions applied to the parent state to
    /// define this branch's "what if?" hypothesis. Order matters
    /// (later interventions operate on already-modified state per
    /// `cognitive::world::counterfactual::multi_intervene`).
    /// Stored as opaque u64 IDs that resolve to full
    /// `Intervention { target, original, counterfactual }` triples
    /// in the cognitive crate's intervention registry.
    pub interventions: Vec<u64>,
}

impl ScenarioBranch {
    /// Construct a new branch descriptor at the given parent version.
    /// Does not touch storage — caller is responsible for invoking
    /// `VersionedGraph::tag_version` and creating the branch dataset
    /// path.
    pub fn new(
        name: impl Into<String>,
        forked_from: u64,
        parent_tag: impl Into<String>,
        fork_seed: u64,
    ) -> Self {
        Self {
            name: name.into(),
            forked_from,
            parent_tag: parent_tag.into(),
            forked_at: 0,
            archetype_prior: None,
            fork_seed,
            inference_mode: 6, // CounterfactualSynthesis = 6 in NarsInference
            interventions: Vec::new(),
        }
    }

    /// Attach an archetype prior. The archetype's identity fingerprint
    /// will bundle into every trajectory in this branch.
    pub fn with_archetype(mut self, archetype_index: u8) -> Self {
        self.archetype_prior = Some(archetype_index);
        self
    }

    /// Set the NARS inference type for this branch. Defaults to
    /// `CounterfactualSynthesis`. Set to `Deduction` (0) for
    /// "extrapolate forward under current beliefs without
    /// counterfactual override."
    pub fn with_inference_mode(mut self, nars_inference: u8) -> Self {
        self.inference_mode = nars_inference;
        self
    }

    /// Append an intervention ID (resolving to a full
    /// `Intervention { target, original, counterfactual }` in the
    /// cognitive crate's registry).
    pub fn with_intervention(mut self, intervention_id: u64) -> Self {
        self.interventions.push(intervention_id);
        self
    }

    /// Stamp the wall-clock fork timestamp.
    pub fn with_timestamp(mut self, ms_since_epoch: u64) -> Self {
        self.forked_at = ms_since_epoch;
        self
    }

    /// Whether this branch carries an archetype prior.
    pub fn has_prior(&self) -> bool {
        self.archetype_prior.is_some()
    }

    /// Whether any interventions are applied.
    pub fn has_interventions(&self) -> bool {
        !self.interventions.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SCENARIO DIFF — the gestalt of two scenarios
// ═══════════════════════════════════════════════════════════════════════════

/// Comparison of two `ScenarioBranch`es at three resolutions.
///
/// Per `.claude/knowledge/user-agent-topic-ripple-model.md`, a good
/// shared gestalt stores both overlap and conflict. This struct
/// captures that across the three layers a scenario lives in:
///
/// 1. **Graph layer** (`graph_diff_summary`): which entities/edges
///    differ — composes `VersionedGraph::diff` between the two
///    branches' Lance datasets.
/// 2. **Fingerprint layer** (`fingerprint_divergence`): bit-level
///    divergence on the world fingerprint via `worlds_differ` from
///    `cognitive::world::counterfactual`.
/// 3. **Gestalt layer** (`world_model_dissonance`): differences in
///    the `WorldModelDto` snapshots taken at end of simulation —
///    the situational/relational diff (qualia, gestalt state,
///    proprioception axes).
#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioDiff {
    /// Branch A name.
    pub a_name: String,
    /// Branch B name.
    pub b_name: String,

    /// Number of entities that exist in B but not A (graph-level).
    /// Resolved via `VersionedGraph::diff(a.forked_from, b.forked_from)`
    /// then walking forward through each branch's writes.
    pub new_entities_in_b: u32,
    /// Number of entities that exist in A but not B.
    pub new_entities_in_a: u32,
    /// Number of entities present in both whose seal bytes differ.
    pub modified_entities: u32,

    /// Bit-level fingerprint divergence in `[0.0, 1.0]`. Comes from
    /// `cognitive::world::counterfactual::worlds_differ`.
    pub fingerprint_divergence: f32,

    /// Aggregate dissonance across `WorldModelDto.field_state.dissonance`
    /// at end-of-simulation for both branches. Higher = more
    /// gestalt-level disagreement.
    pub world_model_dissonance: f32,
}

impl ScenarioDiff {
    /// Whether the branches are essentially convergent — diff signal
    /// below the named threshold across all three resolutions.
    pub fn is_convergent(&self, threshold: f32) -> bool {
        self.fingerprint_divergence < threshold
            && self.world_model_dissonance < threshold
            && self.new_entities_in_a == 0
            && self.new_entities_in_b == 0
            && self.modified_entities == 0
    }

    /// Net new entities count (sum of both directions).
    pub fn total_new_entities(&self) -> u32 {
        self.new_entities_in_a + self.new_entities_in_b
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SCENARIO WORLD TRAIT — the surface implementations expose
// ═══════════════════════════════════════════════════════════════════════════

/// The minimum surface a scenario implementation must expose.
///
/// Concrete impls live downstream:
/// - `lance-graph-cognitive::world::ScenarioWorldImpl` wires
///   intervention math to `VersionedGraph` and `WorldModelDto`.
/// - SMB-side or other domain-specific impls may layer additional
///   semantics (per-tenant scoping, archetype overlays, etc.).
///
/// Errors are deliberately stringly-typed in the trait to keep the
/// contract zero-dep; concrete error enums live in implementations.
pub trait ScenarioWorld {
    /// Create a new branch from the current parent state.
    /// Implementations call `VersionedGraph::tag_version` to pin the
    /// fork point, then create a new dataset path for divergent writes.
    fn fork(
        &self,
        name: &str,
        parent_version: u64,
        archetype_prior: Option<u8>,
    ) -> Result<ScenarioBranch, String>;

    /// Run N steps of forward simulation in the branch. The engine
    /// dispatches via the branch's `inference_mode` (typically
    /// `CounterfactualSynthesis`), consulting the archetype prior
    /// (if any) and applying any pending interventions.
    fn simulate_forward(&self, branch: &ScenarioBranch, steps: u32) -> Result<u64, String>;

    /// Compose-chain palette forecast (Chronos-extracted method).
    /// Returns the palette index sequence the branch is expected to
    /// traverse over `depth` steps. O(depth) table lookups, no
    /// neural network.
    fn forecast_palette(&self, branch: &ScenarioBranch, depth: u32) -> Vec<u8>;

    /// Compare two branches at all three resolutions
    /// (graph / fingerprint / world-model gestalt).
    fn diff_branches(&self, a: &ScenarioBranch, b: &ScenarioBranch)
        -> Result<ScenarioDiff, String>;

    /// Replay a branch from its fork point with the captured seed.
    /// Apache-Temporal-extracted determinism: same seed + same
    /// intervention list = same trajectory, byte-for-byte.
    fn replay(&self, branch: &ScenarioBranch) -> Result<u64, String>;
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_construction_carries_metadata() {
        let b = ScenarioBranch::new("recession_2027", 42, "epoch_2026_q2", 0xDEAD_BEEF);
        assert_eq!(b.name, "recession_2027");
        assert_eq!(b.forked_from, 42);
        assert_eq!(b.parent_tag, "epoch_2026_q2");
        assert_eq!(b.fork_seed, 0xDEAD_BEEF);
        assert_eq!(b.inference_mode, 6); // CounterfactualSynthesis
        assert!(!b.has_prior());
        assert!(!b.has_interventions());
    }

    #[test]
    fn builder_methods_compose() {
        let b = ScenarioBranch::new("growth_2027", 42, "epoch", 1)
            .with_archetype(7)
            .with_intervention(100)
            .with_intervention(101)
            .with_timestamp(1_700_000_000_000);
        assert_eq!(b.archetype_prior, Some(7));
        assert_eq!(b.interventions, vec![100, 101]);
        assert_eq!(b.forked_at, 1_700_000_000_000);
        assert!(b.has_prior());
        assert!(b.has_interventions());
    }

    #[test]
    fn diff_convergence_threshold() {
        let d = ScenarioDiff {
            a_name: "a".into(),
            b_name: "b".into(),
            new_entities_in_a: 0,
            new_entities_in_b: 0,
            modified_entities: 0,
            fingerprint_divergence: 0.05,
            world_model_dissonance: 0.03,
        };
        assert!(d.is_convergent(0.1));
        assert!(!d.is_convergent(0.02));
    }

    #[test]
    fn diff_total_new_entities() {
        let d = ScenarioDiff {
            a_name: "a".into(),
            b_name: "b".into(),
            new_entities_in_a: 5,
            new_entities_in_b: 7,
            modified_entities: 3,
            fingerprint_divergence: 0.0,
            world_model_dissonance: 0.0,
        };
        assert_eq!(d.total_new_entities(), 12);
    }

    #[test]
    fn diff_with_modified_entities_is_not_convergent() {
        let d = ScenarioDiff {
            a_name: "a".into(),
            b_name: "b".into(),
            new_entities_in_a: 0,
            new_entities_in_b: 0,
            modified_entities: 3,
            fingerprint_divergence: 0.001,
            world_model_dissonance: 0.001,
        };
        assert!(!d.is_convergent(0.5));
    }

    #[test]
    fn default_inference_is_counterfactual_synthesis() {
        let b = ScenarioBranch::new("x", 0, "tag", 0);
        // 6 = NarsInference::CounterfactualSynthesis ordinal
        assert_eq!(b.inference_mode, 6);
    }

    #[test]
    fn override_inference_mode() {
        let b = ScenarioBranch::new("x", 0, "tag", 0).with_inference_mode(0); // Deduction
        assert_eq!(b.inference_mode, 0);
    }
}
