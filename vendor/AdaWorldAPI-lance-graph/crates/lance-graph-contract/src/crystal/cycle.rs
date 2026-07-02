//! CycleCrystal — one cognitive cycle (observe → act → feedback).
//!
//! Captures a single loop of the agent's sensorimotor process, including
//! the proprioception anchor classification at the end of the cycle.

use super::{Crystal, CrystalFingerprint, CrystalKind, TruthValue};
use crate::proprioception::StateAnchor;

#[derive(Debug, Clone)]
pub struct CycleCrystal {
    pub fingerprint: CrystalFingerprint,
    pub cycle_index: u64,
    pub anchor: StateAnchor,
    pub truth: TruthValue,
    pub hardness: f32,
    pub revision_count: u32,
    pub crystallized_at: u64,
}

impl Crystal for CycleCrystal {
    fn kind(&self) -> CrystalKind {
        CrystalKind::Cycle
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
