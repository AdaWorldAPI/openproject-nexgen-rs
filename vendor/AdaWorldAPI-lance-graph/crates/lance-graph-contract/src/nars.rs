//! NARS inference types — shared across all consumers.
//!
//! Reconciles n8n-rs InferenceType with lance-graph-planner NarsInferenceType.

/// NARS inference type — determines the reasoning strategy.
///
/// Used by:
/// - n8n-rs ThinkingMode dispatch → QueryPlan
/// - lance-graph-planner → semiring selection
/// - crewai-rust NARS driver → truth value computation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferenceType {
    /// Direct lookup: "I know X, find X" → exact CAM search.
    Deduction,
    /// Pattern matching: "Things like X" → wide CAM scan.
    Induction,
    /// Root cause: "Why did X happen?" → full DN-tree traversal.
    Abduction,
    /// Update belief: "X changed" → bundle_into with learning rate.
    Revision,
    /// Cross-domain: "Connect X and Y" → multi-path bundle.
    Synthesis,
}

/// Query strategy — how to execute a given inference type.
///
/// Maps 1:1 with n8n-rs QueryPlan variants but without the parameters.
/// Parameters come from ThinkingMode or FieldModulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryStrategy {
    /// Exact CAM search (Deduction).
    CamExact,
    /// Wide CAM scan (Induction).
    CamWide,
    /// Full DN-tree traversal (Abduction).
    DnTreeFull,
    /// Bundle into existing node (Revision).
    BundleInto,
    /// Bundle across paths (Synthesis).
    BundleAcross,
}

impl InferenceType {
    /// Map inference type to default query strategy.
    pub fn default_strategy(&self) -> QueryStrategy {
        match self {
            Self::Deduction => QueryStrategy::CamExact,
            Self::Induction => QueryStrategy::CamWide,
            Self::Abduction => QueryStrategy::DnTreeFull,
            Self::Revision => QueryStrategy::BundleInto,
            Self::Synthesis => QueryStrategy::BundleAcross,
        }
    }

    /// Signed v2 inference mantissa (i4, −8..+7) — the cross-crate rule key.
    ///
    /// This is the SAME rule space as `causal_edge::edge::InferenceType`,
    /// bridged by VALUE — the contract stays **zero-dep** (no `causal-edge`
    /// import). The mantissa is the shared little-endian grammar every
    /// `CausalEdge64` consumer reads: forward-chain positive, backward-chain
    /// negative. Mapping matches causal-edge `to_mantissa`:
    /// Deduction `+1`, Induction `+2`, Abduction `−1`, Revision `+4`,
    /// Synthesis `+5`.
    pub const fn to_mantissa(self) -> i8 {
        match self {
            Self::Deduction => 1,
            Self::Induction => 2,
            Self::Abduction => -1,
            Self::Revision => 4,
            Self::Synthesis => 5,
        }
    }

    /// Inverse of [`to_mantissa`](Self::to_mantissa): the closest core rule for a
    /// signed mantissa. Round-trips all 5 variants; negative magnitude is the
    /// backward-chain Abduction direction; `0` = neutral → Deduction. Mantissas
    /// `6`/`7` (causal-edge Intervention/Counterfactual/extension) collapse to
    /// the nearest core rule (Synthesis), since the contract models only the 5.
    pub fn from_mantissa(m: i8) -> Self {
        let mag = m.unsigned_abs() & 0x7;
        let forward = m >= 0;
        match mag {
            0 => Self::Deduction,
            1 => {
                if forward {
                    Self::Deduction
                } else {
                    Self::Abduction
                }
            }
            2 => {
                if forward {
                    Self::Induction
                } else {
                    Self::Abduction
                }
            }
            3 => {
                if forward {
                    Self::Synthesis
                } else {
                    Self::Abduction
                }
            }
            4 => Self::Revision,
            5 => Self::Synthesis,
            _ => Self::Synthesis,
        }
    }
}

/// Bridge the grammar's extended inference set (`grammar::inference::NarsInference`,
/// 7 variants incl. Extrapolation / CounterfactualSynthesis) into the canonical
/// core 5, via the grammar enum's own `.core()`. Lets the DeepNSM grammar
/// proposer (§12.1) hand its inference *intent* to the reasoning pipeline with
/// an ergonomic `.into()`.
impl From<crate::grammar::inference::NarsInference> for InferenceType {
    fn from(g: crate::grammar::inference::NarsInference) -> Self {
        g.core()
    }
}

/// Semiring choice — how to combine evidence across paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemiringChoice {
    /// Boolean AND/OR (standard graph pattern matching).
    Boolean,
    /// Hamming distance minimum (nearest-neighbor search).
    HammingMin,
    /// NARS truth value conjunction (evidence fusion).
    NarsTruth,
    /// XOR bundle (creative association, multi-path binding).
    XorBundle,
    /// CAM-PQ ADC distance (compressed vector search).
    CamPqAdc,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mantissa_round_trips_all_five() {
        for t in [
            InferenceType::Deduction,
            InferenceType::Induction,
            InferenceType::Abduction,
            InferenceType::Revision,
            InferenceType::Synthesis,
        ] {
            assert_eq!(InferenceType::from_mantissa(t.to_mantissa()), t, "{t:?}");
        }
    }

    #[test]
    fn mantissa_signs_match_causal_edge() {
        // Forward-chain positive, backward-chain negative (the causal-edge scheme).
        assert_eq!(InferenceType::Deduction.to_mantissa(), 1);
        assert_eq!(InferenceType::Induction.to_mantissa(), 2);
        assert_eq!(InferenceType::Abduction.to_mantissa(), -1);
        assert_eq!(InferenceType::Revision.to_mantissa(), 4);
        assert_eq!(InferenceType::Synthesis.to_mantissa(), 5);
        // Neutral / extension mantissas fall back to a core rule.
        assert_eq!(InferenceType::from_mantissa(0), InferenceType::Deduction);
        assert_eq!(InferenceType::from_mantissa(6), InferenceType::Synthesis);
    }

    #[test]
    fn grammar_inference_bridges_via_core() {
        use crate::grammar::inference::NarsInference as G;
        assert_eq!(InferenceType::from(G::Deduction), InferenceType::Deduction);
        assert_eq!(
            InferenceType::from(G::Extrapolation),
            InferenceType::Induction
        );
        assert_eq!(
            InferenceType::from(G::CounterfactualSynthesis),
            InferenceType::Synthesis
        );
    }
}
