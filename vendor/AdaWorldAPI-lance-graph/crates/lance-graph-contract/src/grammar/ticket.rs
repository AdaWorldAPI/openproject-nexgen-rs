//! FailureTicket — the structured handoff from local grammar to LLM.
//!
//! When local parsing can't resolve the sentence (coverage below
//! [`super::LOCAL_COVERAGE_THRESHOLD`] or unresolved Wechsel after
//! context replay), we emit a FailureTicket. The LLM sees exactly the
//! ambiguity — not the whole sentence — and returns a surgical answer.

use super::inference::NarsInference;
use super::tekamolo::TekamoloSlots;
use super::wechsel::WechselAmbiguity;

/// What the local parser managed to extract.
#[derive(Debug, Clone)]
pub struct PartialParse {
    /// Tokens that were successfully classified.
    pub resolved_tokens: Vec<u16>,
    /// Tokens the parser failed on.
    pub unresolved_tokens: Vec<u16>,
    /// Coverage ∈ [0, 1]: resolved / (resolved + unresolved).
    pub coverage: f32,
}

/// A 2³ causal-trajectory ambiguity marker.
///
/// The 2³ = 8 possible SPO role assignments (subject/object swap, passive,
/// ergative shift, etc.). When local grammar is confident the sentence
/// expresses causation but can't pin which trajectory, we record which
/// branches are plausible.
#[derive(Debug, Clone)]
pub struct CausalAmbiguity {
    /// Bitmask: bit N set means trajectory N is plausible.
    pub plausible_mask: u8,
    /// Confidence of the most-plausible branch ∈ [0, 1].
    pub leading_confidence: f32,
}

impl CausalAmbiguity {
    pub fn is_resolved(&self, threshold: f32) -> bool {
        self.leading_confidence >= threshold
    }

    pub fn plausible_count(&self) -> u32 {
        self.plausible_mask.count_ones()
    }
}

/// Structured LLM-fallback ticket.
///
/// Carries everything the LLM needs to make a surgical judgment call
/// without seeing the full sentence again:
///
/// - What the parser got (`partial_parse`).
/// - What inference the parser tried (`attempted_inference`) and what it
///   thinks should be tried next (`recommended_next`).
/// - Causal-trajectory ambiguity in SPO form (`causal_ambiguity`).
/// - TEKAMOLO slot fillings so far (`tekamolo`).
/// - Unresolved Wechsel tokens with candidate roles (`wechsel`).
/// - Overall coverage score.
#[derive(Debug, Clone)]
pub struct FailureTicket {
    pub partial_parse: PartialParse,
    pub attempted_inference: NarsInference,
    pub recommended_next: NarsInference,
    pub causal_ambiguity: Option<CausalAmbiguity>,
    pub tekamolo: TekamoloSlots,
    pub wechsel: Vec<WechselAmbiguity>,
    pub coverage: f32,
    /// Required predicates that were absent at commit time. Empty for
    /// grammar-parse failures; populated for schema-validation failures
    /// (TD-INT-8). See [`Self::missing_required`].
    pub missing_required: Vec<&'static str>,
}

impl FailureTicket {
    /// Whether the ticket should actually be sent to the LLM, given the
    /// threshold for local-parse acceptance.
    pub fn needs_llm(&self, threshold: f32) -> bool {
        self.coverage < threshold
            || !self.wechsel.is_empty()
            || self
                .causal_ambiguity
                .as_ref()
                .map(|c| !c.is_resolved(0.75))
                .unwrap_or(false)
    }

    /// Construct a FailureTicket for a schema-validation miss: one or
    /// more Required predicates are absent from a triple set being
    /// committed (TD-INT-8). The list of missing predicate names is
    /// preserved verbatim so the LLM/operator can address each one.
    /// `recommended_next` is `Abduction` — the system is asking
    /// "what value should fill this slot?", which is the abductive case.
    pub fn missing_required(missing: Vec<&'static str>) -> Self {
        Self {
            partial_parse: PartialParse {
                resolved_tokens: Vec::new(),
                unresolved_tokens: Vec::new(),
                coverage: 0.0,
            },
            attempted_inference: NarsInference::Deduction,
            recommended_next: NarsInference::Abduction,
            causal_ambiguity: None,
            tekamolo: TekamoloSlots::default(),
            wechsel: Vec::new(),
            coverage: 0.0,
            missing_required: missing,
        }
    }

    /// Iterator over predicate names that triggered a missing-required
    /// FailureTicket. Empty when the ticket comes from grammar parsing.
    pub fn missing_predicates(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.missing_required.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_coverage_and_clean_wechsel_does_not_need_llm() {
        let t = FailureTicket {
            partial_parse: PartialParse {
                resolved_tokens: vec![0, 1, 2],
                unresolved_tokens: vec![],
                coverage: 1.0,
            },
            attempted_inference: NarsInference::Deduction,
            recommended_next: NarsInference::Deduction,
            causal_ambiguity: None,
            tekamolo: TekamoloSlots::default(),
            wechsel: vec![],
            coverage: 1.0,
            missing_required: vec![],
        };
        assert!(!t.needs_llm(0.9));
    }

    #[test]
    fn low_coverage_needs_llm() {
        let t = FailureTicket {
            partial_parse: PartialParse {
                resolved_tokens: vec![0],
                unresolved_tokens: vec![1, 2],
                coverage: 0.33,
            },
            attempted_inference: NarsInference::Deduction,
            recommended_next: NarsInference::Abduction,
            causal_ambiguity: None,
            tekamolo: TekamoloSlots::default(),
            wechsel: vec![],
            coverage: 0.33,
            missing_required: vec![],
        };
        assert!(t.needs_llm(0.9));
    }

    #[test]
    fn causal_ambiguity_plausible_count() {
        let c = CausalAmbiguity {
            plausible_mask: 0b0000_0101,
            leading_confidence: 0.5,
        };
        assert_eq!(c.plausible_count(), 2);
        assert!(!c.is_resolved(0.75));
    }

    #[test]
    fn missing_required_constructor_preserves_predicate_names() {
        let t = FailureTicket::missing_required(vec!["customer_name", "tax_id"]);
        let m: Vec<&'static str> = t.missing_predicates().collect();
        assert_eq!(m, vec!["customer_name", "tax_id"]);
        assert_eq!(t.recommended_next, NarsInference::Abduction);
        assert_eq!(t.coverage, 0.0);
        assert!(t.needs_llm(0.9), "schema miss must escalate");
    }

    #[test]
    fn missing_required_constructor_empty_for_no_misses() {
        let t = FailureTicket::missing_required(vec![]);
        assert_eq!(t.missing_predicates().count(), 0);
    }
}
