//! SessionCrystal — a full conversation or agent session.

use super::{Crystal, CrystalFingerprint, CrystalKind, TruthValue};

#[derive(Debug, Clone)]
pub struct SessionCrystal {
    pub fingerprint: CrystalFingerprint,
    pub session_id: u64,
    pub cycle_count: u32,
    pub truth: TruthValue,
    pub hardness: f32,
    pub revision_count: u32,
    pub crystallized_at: u64,
}

impl Crystal for SessionCrystal {
    fn kind(&self) -> CrystalKind {
        CrystalKind::Session
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
