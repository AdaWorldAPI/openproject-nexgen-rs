//! NARS inference × thinking-style routing.
//!
//! Grammar resolution is reasoning, not pattern-matching. Each NARS
//! inference type corresponds to a thinking-style cluster:
//!
//! | Inference       | Cluster        | When used                           |
//! |-----------------|----------------|-------------------------------------|
//! | Deduction       | Analytical     | Rule-clear case ending              |
//! | Induction       | Exploratory    | Generalize across similar sentences |
//! | Abduction       | Meta           | Best explanation for surface form   |
//! | Revision        | Meta           | Update belief from new evidence     |
//! | Synthesis       | Creative       | Bind cross-domain signals           |
//! | Extrapolation   | Exploratory    | Extend known pattern to novel input |
//! | Counterfactual  | Creative       | "What if the Wechsel were X?"       |

use crate::thinking::StyleCluster;

/// NARS inference — extended set used by grammar reasoning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NarsInference {
    Deduction,
    Induction,
    Abduction,
    Revision,
    Synthesis,
    Extrapolation,
    CounterfactualSynthesis,
}

impl NarsInference {
    /// Map to the core 5 [`crate::nars::InferenceType`] when a downstream
    /// consumer only speaks the core set.
    pub fn core(self) -> crate::nars::InferenceType {
        use crate::nars::InferenceType as Core;
        match self {
            Self::Deduction => Core::Deduction,
            Self::Induction => Core::Induction,
            Self::Abduction => Core::Abduction,
            Self::Revision => Core::Revision,
            Self::Synthesis => Core::Synthesis,
            Self::Extrapolation => Core::Induction,
            Self::CounterfactualSynthesis => Core::Synthesis,
        }
    }
}

/// Which thinking-style cluster this inference dispatches to.
pub fn inference_to_style_cluster(inf: NarsInference) -> StyleCluster {
    match inf {
        NarsInference::Deduction => StyleCluster::Analytical,
        NarsInference::Induction => StyleCluster::Exploratory,
        NarsInference::Abduction => StyleCluster::Meta,
        NarsInference::Revision => StyleCluster::Meta,
        NarsInference::Synthesis => StyleCluster::Creative,
        NarsInference::Extrapolation => StyleCluster::Exploratory,
        NarsInference::CounterfactualSynthesis => StyleCluster::Creative,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_inferences_have_cluster_and_core() {
        for inf in [
            NarsInference::Deduction,
            NarsInference::Induction,
            NarsInference::Abduction,
            NarsInference::Revision,
            NarsInference::Synthesis,
            NarsInference::Extrapolation,
            NarsInference::CounterfactualSynthesis,
        ] {
            let _cluster = inference_to_style_cluster(inf);
            let _core = inf.core();
        }
    }
}
