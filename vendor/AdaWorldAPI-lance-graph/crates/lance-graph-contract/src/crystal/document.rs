//! DocumentCrystal — a full document composed of sentence crystals.

use super::{Crystal, CrystalFingerprint, CrystalKind, TruthValue};

#[derive(Debug, Clone)]
pub struct DocumentCrystal {
    pub fingerprint: CrystalFingerprint,
    /// Opaque identifier (URL, file hash, etc.) — not parsed here.
    pub document_id: u64,
    /// Count of underlying sentence crystals. Materialization is
    /// downstream — the contract only carries the summary.
    pub sentence_count: u32,
    pub truth: TruthValue,
    pub hardness: f32,
    pub revision_count: u32,
    pub crystallized_at: u64,
}

impl Crystal for DocumentCrystal {
    fn kind(&self) -> CrystalKind {
        CrystalKind::Document
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
