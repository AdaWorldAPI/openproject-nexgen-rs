//! Wechsel — dual-role tokens.
//!
//! Tokens whose role depends on surrounding context: prepositions that can
//! introduce spatial *or* temporal PPs, pronouns whose antecedent is
//! ambiguous, conjunctions that can be coordinating *or* subordinating.
//!
//! The parser records each Wechsel as a [`WechselAmbiguity`] with the
//! candidate roles. If context replay can't collapse it, the ticket goes
//! to the LLM with exactly this slot marked ambiguous.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WechselRole {
    /// Preposition introducing a temporal phrase.
    PrepTemporal,
    /// Preposition introducing a spatial phrase.
    PrepSpatial,
    /// Preposition introducing a kausal phrase.
    PrepKausal,
    /// Preposition introducing a modal phrase.
    PrepModal,
    /// Coordinating conjunction (and, but, or).
    ConjCoord,
    /// Subordinating conjunction (because, although, if).
    ConjSubord,
    /// Pronoun referring backwards (anaphora).
    PronAnaphor,
    /// Pronoun referring forwards (cataphora).
    PronCataphor,
    /// Pronoun referring to a situation (it's raining — expletive).
    PronExpletive,
    /// Particle — role depends on verb.
    Particle,
}

#[derive(Debug, Clone)]
pub struct WechselAmbiguity {
    /// Token index in the sentence.
    pub token_index: u16,
    /// Candidate roles — at least 2 if truly ambiguous.
    pub candidates: Vec<WechselRole>,
    /// Confidence that local grammar alone can't resolve ∈ [0, 1].
    pub local_ambiguity: f32,
}

impl WechselAmbiguity {
    pub fn needs_llm(&self, threshold: f32) -> bool {
        self.local_ambiguity >= threshold && self.candidates.len() >= 2
    }
}
