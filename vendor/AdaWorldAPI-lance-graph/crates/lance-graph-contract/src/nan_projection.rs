//! NaN-detection projection surface — the singleton BindSpace, demoted.
//!
//! Per the operator (2026-06-20): **kill the singleton BindSpace as a stateful
//! carrier; keep it ONLY as a read-only PROJECTION SURFACE for NaN detection.**
//! You do not hold a mutable BindSpace and you do not bundle into it. You
//! *project* the SoA's f32 accumulator tenant ([`ValueTenant::Energy`]) through
//! this surface to flag any non-finite board.
//!
//! It is the **fastest** possible NaN hunt over the SoA: a fixed-offset,
//! fixed-stride read of one 4-byte `f32` per [`NodeRow`], decided by a single
//! integer exponent mask — **no float load, no branch on the value, SIMD-friendly**.
//! `Energy` is F32 precisely because F32 is the fast tenant (half of f64, and the
//! NaN test is one `&`-compare on the bit pattern).
//!
//! This is "BindSpace as projection surface": the only surviving role of the old
//! singleton is to answer "did any node go non-finite this cycle?" over the SoA.

use crate::canonical_node::{NodeRow, ValueTenant};

/// `true` iff an `f32` bit pattern is non-finite (Inf or NaN): the exponent
/// field is all-ones. No float materialised.
#[inline]
pub const fn f32_bits_nonfinite(bits: u32) -> bool {
    (bits & 0x7F80_0000) == 0x7F80_0000
}

/// The result of projecting an SoA batch onto the NaN-detection surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NanReport {
    /// Total boards swept.
    pub total: usize,
    /// Board indices whose `Energy` tenant is non-finite (NaN or Inf).
    pub nonfinite: Vec<u32>,
}

impl NanReport {
    /// No board went non-finite.
    #[inline]
    pub fn is_clean(&self) -> bool {
        self.nonfinite.is_empty()
    }

    /// Count of non-finite boards.
    #[inline]
    pub fn count(&self) -> usize {
        self.nonfinite.len()
    }
}

/// Read one board's `Energy` tenant as a raw `f32` bit pattern (no float load).
#[inline]
fn energy_bits(row: &NodeRow) -> u32 {
    let off = ValueTenant::Energy.value_offset();
    u32::from_le_bytes([
        row.value[off],
        row.value[off + 1],
        row.value[off + 2],
        row.value[off + 3],
    ])
}

/// Project a batch of canonical boards onto the NaN-detection surface by reading
/// each one's `Energy` tenant. Read-only; returns the indices of non-finite boards.
/// This is the demoted singleton BindSpace — a projection, never a carrier.
pub fn project_energy_nonfinite(rows: &[NodeRow]) -> NanReport {
    let mut nonfinite = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if f32_bits_nonfinite(energy_bits(row)) {
            nonfinite.push(i as u32);
        }
    }
    NanReport {
        total: rows.len(),
        nonfinite,
    }
}

/// Fast clean/dirty answer without materialising the index list — the cheapest
/// projection (early-outs on the first non-finite board).
pub fn energy_all_finite(rows: &[NodeRow]) -> bool {
    rows.iter().all(|row| !f32_bits_nonfinite(energy_bits(row)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_node::{EdgeBlock, NodeGuid};

    fn board_with(energy: f32) -> NodeRow {
        let mut row = NodeRow {
            key: NodeGuid::local(0),
            edges: EdgeBlock::default(),
            value: [0u8; 480],
        };
        let off = ValueTenant::Energy.value_offset();
        row.value[off..off + 4].copy_from_slice(&energy.to_le_bytes());
        row
    }

    #[test]
    fn finite_batch_is_clean() {
        let rows: Vec<NodeRow> = (0..8).map(|i| board_with(i as f32)).collect();
        let r = project_energy_nonfinite(&rows);
        assert!(r.is_clean());
        assert_eq!(r.total, 8);
        assert!(energy_all_finite(&rows));
    }

    #[test]
    fn nan_and_inf_are_flagged_neg_inf_too() {
        let rows = vec![
            board_with(1.0),
            board_with(f32::NAN),
            board_with(f32::INFINITY),
            board_with(0.0),
            board_with(f32::NEG_INFINITY),
        ];
        let r = project_energy_nonfinite(&rows);
        assert_eq!(r.nonfinite, vec![1, 2, 4]);
        assert_eq!(r.count(), 3);
        assert!(!r.is_clean());
        assert!(!energy_all_finite(&rows));
    }

    #[test]
    fn subnormal_and_zero_are_finite() {
        // exponent-zero patterns (zero, subnormals) must NOT be flagged
        let rows = vec![
            board_with(0.0),
            board_with(-0.0),
            board_with(f32::MIN_POSITIVE),
        ];
        assert!(project_energy_nonfinite(&rows).is_clean());
    }
}
