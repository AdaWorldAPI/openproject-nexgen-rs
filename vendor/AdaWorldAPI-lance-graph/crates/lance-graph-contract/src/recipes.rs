//! The 34 reasoning-tactic **recipes** — the working catalogue spine.
//!
//! A *recipe* is a named composition over OUR substrate (atoms, SPO 2³ masks, NARS
//! truth, CollapseGate SD, markers) that realizes one of the 34 LLM reasoning tactics.
//!
//! # Spec source, not dependency
//!
//! The 34 are specified by the ladybug-rs `34_TACTICS_x_REASONING_LADDER` doc and the
//! Sun et al. (2025) reasoning ladder. **ladybug-rs is the failed "empty cathedral" — a
//! reference for *what each tactic must do*, never a dependency or port target** (see
//! `.claude/knowledge/ada-rewrite-charter.md` D0). Every recipe composes *our* primitives.
//!
//! This module is the **catalogue spine**: the 34 as data + registry + lookups, each
//! tagged with its difficulty Tier, the structural Mechanism it uses, the hardware
//! Bucket it lives in, and its SPO-2³ causal coverage. Per-recipe *evaluators* land
//! incrementally as substrate readiness allows (charter D4).

/// Sun et al. (2025) reasoning-ladder difficulty tier the tactic addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tier {
    /// Hard tier (~65% plateau) — multiplicative error across dependent steps.
    Hard,
    /// Extremely-Hard tier (<10%) — convergent lock-in, no creative leap.
    ExtremelyHard,
    /// Cross-tier infrastructure — helps at every difficulty.
    CrossTier,
}

/// The structural mechanism (the 3 that LLMs lack) the tactic relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mechanism {
    /// Parallel independence vs sequential dependency (breaks `P=p^n`).
    ParallelIndependence,
    /// Truth-aware inference (NARS truth/revision/abduction) vs next-token prob.
    TruthAwareInference,
    /// Structural divergence vs convergent optimization.
    StructuralDivergence,
    /// Cross-cutting infrastructure (memory, fusion, scaffolding, diagnostics).
    Infrastructure,
}

/// The hardware-design partition the recipe executes in (charter D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    /// Uniform, branch-free, every-cycle SIMD — runs in `cognitive-shader-driver`.
    Datapath,
    /// Branchy decision at a control point — planner + `escalation`.
    Control,
    /// A cheap marker that gates whether deeper work fires — `elevation`/CollapseGate SD.
    Gate,
}

/// SPO 2³ causal-lattice coverage (see `.claude/knowledge/spo-2cubed-list-coverage.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coverage {
    /// Maps onto the causal lattice (the projections / Pearl levels).
    Covered,
    /// Some members ride the lattice, rest orthogonal.
    Partial,
    /// Orthogonal axis (operation / meta / gate / memory / qualia).
    NotCovered,
}

/// One reasoning-tactic recipe.
#[derive(Debug, Clone, Copy)]
pub struct Recipe {
    /// Tactic number 1..=34 (Stakelum/ladybug numbering).
    pub id: u8,
    /// Short code, e.g. `"RCR"`.
    pub code: &'static str,
    /// Human name.
    pub name: &'static str,
    pub tier: Tier,
    pub mechanism: Mechanism,
    pub bucket: Bucket,
    pub spo2cubed: Coverage,
    /// The OUR-substrate primitive(s) that realize it (charter D3).
    pub substrate: &'static str,
}

use Bucket::*;
use Coverage::*;
use Mechanism::*;
use Tier::*;

/// The 34 recipes. Order = id ascending.
pub const RECIPES: [Recipe; 34] = [
    Recipe {
        id: 1,
        code: "RTE",
        name: "Recursive Thought Expansion",
        tier: Hard,
        mechanism: ParallelIndependence,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "rung depth × Expand/Compress; Berry-Esseen stop",
    },
    Recipe {
        id: 2,
        code: "HTD",
        name: "Hierarchical Thought Decomposition",
        tier: Hard,
        mechanism: ParallelIndependence,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "CLAM bipolar split / Decompose op",
    },
    Recipe {
        id: 3,
        code: "SMAD",
        name: "Structured Multi-Agent Debate",
        tier: ExtremelyHard,
        mechanism: TruthAwareInference,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "a2a_blackboard + InnerCouncil (NARS-revised vote)",
    },
    Recipe {
        id: 4,
        code: "RCR",
        name: "Reverse Causality Reasoning",
        tier: ExtremelyHard,
        mechanism: StructuralDivergence,
        bucket: Control,
        spo2cubed: Covered,
        substrate: "SPO 2³ backward S_O + Abduction + Granger",
    },
    Recipe {
        id: 5,
        code: "TCP",
        name: "Thought Chain Pruning",
        tier: Hard,
        mechanism: ParallelIndependence,
        bucket: Gate,
        spo2cubed: NotCovered,
        substrate: "CollapseGate SD BLOCK prunes branch",
    },
    Recipe {
        id: 6,
        code: "TR",
        name: "Thought Randomization",
        tier: ExtremelyHard,
        mechanism: StructuralDivergence,
        bucket: Gate,
        spo2cubed: NotCovered,
        substrate: "temperature (Staunen) perturb above noise floor",
    },
    Recipe {
        id: 7,
        code: "ASC",
        name: "Adversarial Self-Critique",
        tier: ExtremelyHard,
        mechanism: TruthAwareInference,
        bucket: Control,
        spo2cubed: Partial,
        substrate: "InnerCouncil split / 5 challenge types (negation projection)",
    },
    Recipe {
        id: 8,
        code: "CAS",
        name: "Conditional Abstraction Scaling",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Gate,
        spo2cubed: NotCovered,
        substrate: "HDR cascade INT1/4/8/32 × Abstract↔Concretize",
    },
    Recipe {
        id: 9,
        code: "IRS",
        name: "Iterative Roleplay Synthesis",
        tier: ExtremelyHard,
        mechanism: StructuralDivergence,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "persona FieldModulation (structurally distinct kernels)",
    },
    Recipe {
        id: 10,
        code: "MCP",
        name: "Meta-Cognition Prompting",
        tier: Hard,
        mechanism: TruthAwareInference,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "MUL DK + Brier calibration; Meta lane",
    },
    Recipe {
        id: 11,
        code: "CR",
        name: "Contradiction Resolution",
        tier: Hard,
        mechanism: TruthAwareInference,
        bucket: Control,
        spo2cubed: Partial,
        substrate: "NARS opposing-truth detect + coherence; Contradiction preserved",
    },
    Recipe {
        id: 12,
        code: "TCA",
        name: "Temporal Context Augmentation",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Datapath,
        spo2cubed: NotCovered,
        substrate: "Granger temporal lane / Markov ±5 / 24 temporal verbs",
    },
    Recipe {
        id: 13,
        code: "CDT",
        name: "Convergent & Divergent Thinking",
        tier: ExtremelyHard,
        mechanism: StructuralDivergence,
        bucket: Gate,
        spo2cubed: NotCovered,
        substrate: "explore↔exploit temperature; style oscillation",
    },
    Recipe {
        id: 14,
        code: "MCT",
        name: "Multimodal Chain-of-Thought",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Datapath,
        spo2cubed: NotCovered,
        substrate: "GrammarTriangle: NSM+Causality+Qualia → one fingerprint",
    },
    Recipe {
        id: 15,
        code: "LSI",
        name: "Latent Space Introspection",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "CRP distribution / Mexican-hat over fingerprint clusters",
    },
    Recipe {
        id: 16,
        code: "PSO",
        name: "Prompt Scaffold Optimization",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "ThinkingTemplate slots + TD-learned discovery",
    },
    Recipe {
        id: 17,
        code: "CDI",
        name: "Cognitive Dissonance Induction",
        tier: CrossTier,
        mechanism: TruthAwareInference,
        bucket: Control,
        spo2cubed: Partial,
        substrate: "Festinger dissonance = opposing NARS truth on similar fp; HOLD",
    },
    Recipe {
        id: 18,
        code: "CWS",
        name: "Context Window Simulation",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "persistent BindSpace / WitnessCorpus / episodic memory",
    },
    Recipe {
        id: 19,
        code: "ARE",
        name: "Algorithmic Reverse Engineering",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Datapath,
        spo2cubed: NotCovered,
        substrate: "ABBA unbind: A⊗B⊗B=A (exact algebraic inverse)",
    },
    Recipe {
        id: 20,
        code: "TCF",
        name: "Thought Cascade Filtering",
        tier: Hard,
        mechanism: ParallelIndependence,
        bucket: Gate,
        spo2cubed: NotCovered,
        substrate: "N search strategies + agreement rate; SD select",
    },
    Recipe {
        id: 21,
        code: "SSR",
        name: "Self-Skepticism Reinforcement",
        tier: Hard,
        mechanism: TruthAwareInference,
        bucket: Control,
        spo2cubed: Partial,
        substrate: "challenge schedule × MUL uncertainty; truth-drop = weak",
    },
    Recipe {
        id: 22,
        code: "ETD",
        name: "Emergent Task Decomposition",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "CLAM cluster geometry determines subtasks (no spec)",
    },
    Recipe {
        id: 23,
        code: "AMP",
        name: "Adaptive Meta-Prompting",
        tier: ExtremelyHard,
        mechanism: StructuralDivergence,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "TD-learning on ThinkingStyle Q-values (W32-39)",
    },
    Recipe {
        id: 24,
        code: "ZCF",
        name: "Zero-Shot Concept Fusion",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Datapath,
        spo2cubed: NotCovered,
        substrate: "VSA bind(A,B): new vector valid in both spaces, recoverable",
    },
    Recipe {
        id: 25,
        code: "HPM",
        name: "Hyperdimensional Pattern Matching",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Datapath,
        spo2cubed: NotCovered,
        substrate: "the substrate: fingerprint cosine/Hamming sweep (SIMD)",
    },
    Recipe {
        id: 26,
        code: "CUR",
        name: "Cascading Uncertainty Reduction",
        tier: Hard,
        mechanism: ParallelIndependence,
        bucket: Gate,
        spo2cubed: NotCovered,
        substrate: "FreeEnergy / CRP percentiles; coarse-to-fine prune",
    },
    Recipe {
        id: 27,
        code: "MPC",
        name: "Multi-Perspective Compression",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Datapath,
        spo2cubed: NotCovered,
        substrate: "bundle = majority-vote-per-bit consensus + delta encode",
    },
    Recipe {
        id: 28,
        code: "SSAM",
        name: "Self-Supervised Analogical Mapping",
        tier: ExtremelyHard,
        mechanism: StructuralDivergence,
        bucket: Datapath,
        spo2cubed: Partial,
        substrate: "NARS analogy A→B,C≈A⊢C→B; bind+similarity (Gentner)",
    },
    Recipe {
        id: 29,
        code: "IDR",
        name: "Intent-Driven Reframing",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "GrammarTriangle CausalityFlow agent/action/patient/reason",
    },
    Recipe {
        id: 30,
        code: "SPP",
        name: "Shadow Parallel Processing",
        tier: Hard,
        mechanism: ParallelIndependence,
        bucket: Control,
        spo2cubed: Partial,
        substrate: "independent paths + agreement (ECC/RAID); the CF majority/minority fork",
    },
    Recipe {
        id: 31,
        code: "ICR",
        name: "Iterative Counterfactual Reasoning",
        tier: ExtremelyHard,
        mechanism: StructuralDivergence,
        bucket: Control,
        spo2cubed: Covered,
        substrate:
            "world⊗factual⊗counterfactual (XOR self-inverse); SPO=0b111; CausalEdge64 −6 mantissa",
    },
    Recipe {
        id: 32,
        code: "SDD",
        name: "Semantic Distortion Detection",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Datapath,
        spo2cubed: NotCovered,
        substrate: "Berry-Esseen noise floor + reciprocal A→B,B→A validation",
    },
    Recipe {
        id: 33,
        code: "DTMF",
        name: "Dynamic Task Meta-Framing",
        tier: CrossTier,
        mechanism: Infrastructure,
        bucket: Control,
        spo2cubed: NotCovered,
        substrate: "template switch on CollapseGate BLOCK (shift all modulation)",
    },
    Recipe {
        id: 34,
        code: "HKF",
        name: "Hyperdimensional Knowledge Fusion",
        tier: ExtremelyHard,
        mechanism: StructuralDivergence,
        bucket: Datapath,
        spo2cubed: NotCovered,
        substrate: "cross-domain bind(A,rel,B); reversible/auditable fusion",
    },
];

/// Look up a recipe by tactic id (1..=34).
#[inline]
pub fn recipe(id: u8) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.id == id)
}

/// Look up a recipe by short code (e.g. `"RCR"`).
#[inline]
pub fn recipe_by_code(code: &str) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.code == code)
}

/// All recipes sharing a mechanism.
pub fn by_mechanism(m: Mechanism) -> impl Iterator<Item = &'static Recipe> {
    RECIPES.iter().filter(move |r| r.mechanism == m)
}

/// All recipes that ride the SPO 2³ causal lattice (Covered or Partial).
pub fn causal() -> impl Iterator<Item = &'static Recipe> {
    RECIPES
        .iter()
        .filter(|r| matches!(r.spo2cubed, Coverage::Covered | Coverage::Partial))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_complete_34_ids_unique() {
        assert_eq!(RECIPES.len(), 34);
        for (i, r) in RECIPES.iter().enumerate() {
            assert_eq!(r.id as usize, i + 1, "recipes must be id-ordered 1..=34");
            assert!(!r.code.is_empty() && !r.name.is_empty() && !r.substrate.is_empty());
        }
    }

    #[test]
    fn lookups_work() {
        assert_eq!(recipe(4).unwrap().code, "RCR");
        assert_eq!(recipe(31).unwrap().code, "ICR");
        assert_eq!(recipe_by_code("HPM").unwrap().id, 25);
        assert!(recipe(0).is_none() && recipe(35).is_none());
    }

    #[test]
    fn only_causal_tactics_are_2cubed_covered() {
        // Exactly RCR (#4) and ICR (#31) fully cover the causal lattice.
        let covered: Vec<u8> = RECIPES
            .iter()
            .filter(|r| r.spo2cubed == Coverage::Covered)
            .map(|r| r.id)
            .collect();
        assert_eq!(covered, vec![4, 31]);
        // 2³ is the causal spine only — the rest are Partial or orthogonal.
        assert!(
            causal().count() < RECIPES.len() / 2,
            "most tactics are NOT causal"
        );
    }

    #[test]
    fn mechanism_tally_matches_the_ladder_doc() {
        let count = |m: Mechanism| by_mechanism(m).count();
        assert_eq!(count(Mechanism::ParallelIndependence), 6); // #1,2,5,20,26,30
        assert_eq!(count(Mechanism::TruthAwareInference), 6); // #3,7,10,11,17,21
        assert_eq!(count(Mechanism::StructuralDivergence), 8); // #4,6,9,13,23,28,31,34
        assert_eq!(count(Mechanism::Infrastructure), 14);
    }
}
