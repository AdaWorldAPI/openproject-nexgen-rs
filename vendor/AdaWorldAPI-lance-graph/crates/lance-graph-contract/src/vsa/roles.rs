//! VSA role-key constants for 256-D CAM-PQ witness-corpus adapter.
//!
//! Per I-VSA-IDENTITIES (CLAUDE.md): VSA operates on IDENTITY fingerprints
//! that POINT TO content, never on content's quantized register itself.
//!
//! This module provides the three 256-D bipolar role-key vectors used by
//! the `WitnessIndexCamPq` Option-A SPO adapter (`spo_to_fingerprint`).
//! Each role key occupies a **disjoint** contiguous slice of the 256-D space:
//!
//! ```text
//! R_S : dims [  0 ..  85)   — Subject   role slice
//! R_P : dims [ 85 .. 170)   — Predicate role slice
//! R_O : dims [170 .. 255)   — Object    role slice
//! dim 255                   — Reserved (NULL / out-of-palette marker)
//! ```
//!
//! Disjoint slices guarantee **exact** orthogonality: `dot(R_S, R_P) = 0.0`
//! by construction (no floating-point approximation required). This satisfies
//! I-VSA-IDENTITIES Test-2 (role orthogonality).
//!
//! The bipolar pattern within each slice is derived deterministically from the
//! FNV-64 hash of the role label, so the keys are reproducible across process
//! restarts without persisting them.
//!
//! **Scope:** CAM-PQ 256-D only. These are NOT related to the 16,384-D
//! `grammar::role_keys` keys — those live in a different VSA algebra
//! (binary XOR-bind, 16K dims). This module addresses the 256-D f32
//! multiply-add algebra used by the CAM-PQ codec.

/// Dimension of the 256-D CAM-PQ VSA space.
pub const CAM_PQ_DIM: usize = 256;

/// Subject role slice: `[0 .. 85)`.
pub const S_SLICE_START: usize = 0;
/// Subject role slice end (exclusive).
pub const S_SLICE_END: usize = 85;

/// Predicate role slice: `[85 .. 170)`.
pub const P_SLICE_START: usize = 85;
/// Predicate role slice end (exclusive).
pub const P_SLICE_END: usize = 170;

/// Object role slice: `[170 .. 255)`.
pub const O_SLICE_START: usize = 170;
/// Object role slice end (exclusive).
pub const O_SLICE_END: usize = 255;

/// Generate a deterministic bipolar ±1 role-key for a given slice.
///
/// Pattern is derived from FNV-64 of `label` bytes. Within `[start..end)`,
/// each position gets `+1.0` or `-1.0` based on successive bits of the hash
/// stream (xorshift64 expansion). Outside the slice, values are `0.0`.
///
/// This is a pure function — same label + same start/end → same output always.
pub fn make_role_key(label: &[u8], start: usize, end: usize) -> [f32; CAM_PQ_DIM] {
    let mut key = [0.0f32; CAM_PQ_DIM];

    // FNV-64 of label as seed
    let mut state = fnv64(label);

    for slot in key[start..end].iter_mut() {
        // xorshift64 step for next pseudo-random bit
        state = xorshift64(state);
        // bit 0 of state → +1.0 or -1.0
        *slot = if (state & 1) == 0 { 1.0 } else { -1.0 };
    }
    key
}

/// Generate a deterministic 256-entry identity catalogue.
///
/// `palette_id[i]` is the identity fingerprint for palette index `i`.
/// Each entry is a bipolar ±1 vector derived from FNV-64 of
/// `[seed_byte_hi, seed_byte_lo, i_hi, i_lo]`.
///
/// The catalogue is dense bipolar in all 256 dimensions — NOT a one-hot —
/// which gives CAM-PQ codebook training meaningful geometric structure.
/// Per I-VSA-IDENTITIES: these are identity fingerprints, not content.
pub fn make_palette_id(seed: u64) -> Box<[[f32; CAM_PQ_DIM]; 256]> {
    let mut catalogue: Box<[[f32; CAM_PQ_DIM]; 256]> = vec![[0.0f32; CAM_PQ_DIM]; 256]
        .into_iter()
        .collect::<Vec<_>>()
        .try_into()
        .unwrap_or_else(|_| unreachable!("256 elements always fit"));

    for (palette_idx, entry) in catalogue.iter_mut().enumerate() {
        // Derive a per-entry seed: mix `seed` with `palette_idx`
        let mut state = fnv64_mix(seed, palette_idx as u64);
        for slot in entry.iter_mut() {
            state = xorshift64(state);
            *slot = if (state & 1) == 0 { 1.0 } else { -1.0 };
        }
    }
    catalogue
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// FNV-64 hash of a byte slice.
const fn fnv64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 14695981039346656037;
    const PRIME: u64 = 1099511628211;
    let mut h = OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        h = h.wrapping_mul(PRIME);
        h ^= bytes[i] as u64;
        i += 1;
    }
    h
}

/// Mix a u64 seed with a u64 index (for per-entry catalogue derivation).
fn fnv64_mix(seed: u64, idx: u64) -> u64 {
    const PRIME: u64 = 1099511628211;
    let mut h = seed.wrapping_add(idx.wrapping_mul(PRIME));
    h ^= idx.rotate_left(17);
    h = h
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    h
}

/// xorshift64 PRNG step — produces the next pseudo-random u64 from state.
/// Never returns 0 for nonzero input (guaranteed by xorshift theory).
const fn xorshift64(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_keys_disjoint_slices_are_exactly_orthogonal() {
        let r_s = make_role_key(b"S", S_SLICE_START, S_SLICE_END);
        let r_p = make_role_key(b"P", P_SLICE_START, P_SLICE_END);
        let r_o = make_role_key(b"O", O_SLICE_START, O_SLICE_END);

        let dot_sp: f32 = r_s.iter().zip(r_p.iter()).map(|(a, b)| a * b).sum();
        let dot_so: f32 = r_s.iter().zip(r_o.iter()).map(|(a, b)| a * b).sum();
        let dot_po: f32 = r_p.iter().zip(r_o.iter()).map(|(a, b)| a * b).sum();

        // Disjoint slices → dot product is exactly 0.0
        assert_eq!(dot_sp, 0.0, "S·P must be exactly 0 (disjoint slices)");
        assert_eq!(dot_so, 0.0, "S·O must be exactly 0 (disjoint slices)");
        assert_eq!(dot_po, 0.0, "P·O must be exactly 0 (disjoint slices)");
    }

    #[test]
    fn role_keys_are_bipolar_in_their_slice() {
        let r_s = make_role_key(b"S", S_SLICE_START, S_SLICE_END);
        // All values in slice are ±1.0
        for &v in &r_s[S_SLICE_START..S_SLICE_END] {
            assert!(v == 1.0 || v == -1.0, "slice values must be ±1.0, got {v}");
        }
        // All values outside slice are 0.0
        for &v in &r_s[S_SLICE_END..] {
            assert_eq!(v, 0.0, "out-of-slice values must be 0.0");
        }
    }

    #[test]
    fn role_keys_deterministic() {
        let r_s1 = make_role_key(b"S", S_SLICE_START, S_SLICE_END);
        let r_s2 = make_role_key(b"S", S_SLICE_START, S_SLICE_END);
        assert_eq!(r_s1, r_s2, "same label → same key");
    }

    #[test]
    fn palette_id_is_bipolar_dense() {
        let palette = make_palette_id(0xCAFE_BABE);
        for (i, entry) in palette.iter().enumerate() {
            for &v in entry.iter() {
                assert!(
                    v == 1.0 || v == -1.0,
                    "palette_id[{i}] must be bipolar ±1.0, got {v}"
                );
            }
        }
    }

    #[test]
    fn palette_id_entries_differ() {
        let palette = make_palette_id(0xCAFE_BABE);
        // Entry 0 and entry 1 differ in at least some dimensions
        let diffs = palette[0]
            .iter()
            .zip(palette[1].iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(diffs > 0, "distinct palette indices must differ");
    }
}
