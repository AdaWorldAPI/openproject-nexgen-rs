//! SentenceCrystal — one parsed sentence.
//!
//! Carries:
//! - triples extracted from the sentence (SPO)
//! - optional TEKAMOLO adverbial slots (temporal/kausal/modal/lokal)
//! - NARS truth value, revision count, crystallization timestamp
//! - the polymorphic [`CrystalFingerprint`]

use super::{Crystal, CrystalFingerprint, CrystalKind, TruthValue};
use crate::grammar::tekamolo::TekamoloSlots;

/// A minimal SPO triple — downstream crates use their own rich types.
#[derive(Debug, Clone)]
pub struct Triple {
    pub subject: u32,
    pub predicate: u32,
    pub object: u32,
}

/// One parsed sentence, crystallized.
#[derive(Debug, Clone)]
pub struct SentenceCrystal {
    pub fingerprint: CrystalFingerprint,
    pub triples: Vec<Triple>,
    pub tekamolo: Option<TekamoloSlots>,
    pub truth: TruthValue,
    pub hardness: f32,
    pub revision_count: u32,
    pub crystallized_at: u64,
}

impl Crystal for SentenceCrystal {
    fn kind(&self) -> CrystalKind {
        CrystalKind::Sentence
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
