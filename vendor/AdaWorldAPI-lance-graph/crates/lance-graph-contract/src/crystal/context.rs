//! ContextCrystal — Markov ±5 window around a focal sentence.
//!
//! Bundles five preceding + focal + five following sentence crystals into
//! a single temporal-causality context chain. Supports **replay** for
//! disambiguation: when a token's parse is ambiguous, the ±5 context can
//! be re-scanned (possibly with inverted direction) to pick the branch
//! consistent with surrounding truth.

use super::{Crystal, CrystalFingerprint, CrystalKind, SentenceCrystal, TruthValue};

/// Markov radius — ±5 sentences around focal.
pub const MARKOV_RADIUS: usize = 5;

/// A ±5 Markov context centered on a focal sentence.
#[derive(Debug, Clone)]
pub struct ContextCrystal {
    pub fingerprint: CrystalFingerprint,
    /// Five preceding crystals (oldest first). May be shorter at boundaries.
    pub preceding: Vec<SentenceCrystal>,
    /// The focal sentence — the one currently under consideration.
    pub focal: SentenceCrystal,
    /// Five following crystals (newest last). May be shorter at boundaries.
    pub following: Vec<SentenceCrystal>,
    pub truth: TruthValue,
    pub hardness: f32,
    pub revision_count: u32,
    pub crystallized_at: u64,
}

/// Direction for context replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayDirection {
    Forward,
    Backward,
}

impl ContextCrystal {
    /// Total sentences in the context (1 + preceding + following).
    /// Always ≥ 1 (the focal sentence). See [`Self::is_empty`] which is
    /// always false for this type — provided to satisfy the
    /// `len_without_is_empty` lint.
    pub fn len(&self) -> usize {
        1 + self.preceding.len() + self.following.len()
    }

    /// Always false — a ContextCrystal always has at least its focal sentence.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Whether the window is saturated on both sides.
    pub fn is_saturated(&self) -> bool {
        self.preceding.len() == MARKOV_RADIUS && self.following.len() == MARKOV_RADIUS
    }
}

impl Crystal for ContextCrystal {
    fn kind(&self) -> CrystalKind {
        CrystalKind::Context
    }
    fn hardness(&self) -> f32 {
        self.hardness
    }
    fn revision_count(&self) -> u32 {
        self.revision_count
    }
    fn crystallized_at(&self) -> u64 {
        self.crystallized_at
    }
    fn fingerprint(&self) -> &CrystalFingerprint {
        &self.fingerprint
    }
    fn truth(&self) -> TruthValue {
        self.truth
    }
}
