//! Type-intrinsic distance dispatch — zero crate-boundary tax.
//!
//! Each carrier type implements `Distance` so the SoA consumer calls
//! `a.distance(&b)` and the compiler monomorphizes the correct kernel.
//! No `dyn`, no enum match, no runtime cost.
//!
//! | Carrier | Distance | Kernel |
//! |---|---|---|
//! | `[u64; 256]` (Binary16K) | Hamming (popcount of XOR) | SIMD VPOPCNTDQ |
//! | `[f32; 16_384]` (Vsa16kF32) | Cosine → FisherZ | F32x16 FMA |
//! | `[u8; 6]` (CamPqCode) | ADC lookup | Precomputed table |
//! | `[u8; 3]` (PaletteEdge) | Palette L1 lookup | 256×256 table |

/// Universal distance trait for all carrier types.
///
/// The trait monomorphizes at compile time — no dynamic dispatch.
/// Implementations live in ndarray (SIMD kernels) or carrier crates
/// (precomputed tables); the contract defines only the interface.
pub trait Distance: Sized {
    /// Maximum possible distance for this type (used for normalization).
    const MAX_DISTANCE: u32;

    /// Compute distance between two carriers of the same type.
    fn distance(&self, other: &Self) -> u32;

    /// Normalized similarity in [0.0, 1.0]. Default: 1 - d/MAX.
    #[inline]
    fn similarity(&self, other: &Self) -> f32 {
        if Self::MAX_DISTANCE == 0 {
            return 1.0;
        }
        1.0 - (self.distance(other) as f32 / Self::MAX_DISTANCE as f32)
    }

    /// FisherZ-transformed similarity for safe averaging.
    ///
    /// Cosine similarity ∈ [-1, 1] is nonlinear for averaging. FisherZ
    /// maps it to a normal-distributed variable: z = atanh(r). Average
    /// in z-space, then tanh(z_avg) maps back. For non-cosine distances,
    /// the default implementation uses the [0,1] similarity directly.
    #[inline]
    fn similarity_z(&self, other: &Self) -> f32 {
        let s = self.similarity(other);
        // Clamp away from ±1 so `atanh` (the `ln` below) stays finite.
        // The bound is ±0.9999, not ±0.999: a self-match (s = 1.0) must
        // round-trip back through `tanh(atanh(clamp)) = clamp` to a value
        // that reads as "essentially identical" (≈0.99986), not be capped
        // at 0.999 — otherwise `cohort_similarity_z(self) > 0.999` is
        // unreachable. atanh(0.9999) ≈ 4.95 is comfortably finite.
        let clamped = s.clamp(-0.9999, 0.9999);
        ((1.0 + clamped) / (1.0 - clamped)).ln() * 0.5
    }
}

/// Inverse FisherZ: recover similarity from z-transformed value.
#[inline]
pub fn fisher_z_inverse(z: f32) -> f32 {
    let e2z = (2.0 * z).exp();
    (e2z - 1.0) / (e2z + 1.0)
}

/// Average similarities via FisherZ transform (correct for nonlinear scales).
pub fn mean_similarity_fisher(z_values: &[f32]) -> f32 {
    if z_values.is_empty() {
        return 0.0;
    }
    let mean_z: f32 = z_values.iter().sum::<f32>() / z_values.len() as f32;
    fisher_z_inverse(mean_z)
}

// ─────────────────────────────────────────────────────────────────────
// Implementations for contract types (zero-dep, no SIMD — baseline).
// ndarray consumers should shadow these with SIMD-accelerated versions
// via the same trait on the same types (blanket impls or newtype wrappers).
//
// These scalar impls guarantee the trait works everywhere, including
// in the contract crate's own tests and in WASM/embedded targets
// where ndarray may not be available.
// ───────────��─────────────────────────��───────────────────────────────

/// Binary16K: Hamming distance (scalar baseline).
impl Distance for [u64; 256] {
    const MAX_DISTANCE: u32 = 16_384;

    #[inline]
    fn distance(&self, other: &Self) -> u32 {
        let mut d = 0u32;
        for i in 0..256 {
            d += (self[i] ^ other[i]).count_ones();
        }
        d
    }
}

/// CamPqCode: byte-wise L1 distance (6-byte ADC code, scalar baseline).
/// Real ADC uses precomputed distance tables; this is the fallback.
impl Distance for [u8; 6] {
    const MAX_DISTANCE: u32 = 255 * 6;

    #[inline]
    fn distance(&self, other: &Self) -> u32 {
        let mut d = 0u32;
        for i in 0..6 {
            d += (self[i] as i16 - other[i] as i16).unsigned_abs() as u32;
        }
        d
    }
}

/// PaletteEdge: byte-wise L1 distance (3-byte SPO palette code).
impl Distance for [u8; 3] {
    const MAX_DISTANCE: u32 = 255 * 3;

    #[inline]
    fn distance(&self, other: &Self) -> u32 {
        let mut d = 0u32;
        for i in 0..3 {
            d += (self[i] as i16 - other[i] as i16).unsigned_abs() as u32;
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary16k_hamming_self_zero() {
        let a = [0u64; 256];
        assert_eq!(a.distance(&a), 0);
        assert_eq!(a.similarity(&a), 1.0);
    }

    #[test]
    fn binary16k_hamming_all_different() {
        let a = [0u64; 256];
        let b = [u64::MAX; 256];
        assert_eq!(a.distance(&b), 16_384);
        assert_eq!(a.similarity(&b), 0.0);
    }

    #[test]
    fn binary16k_hamming_partial() {
        let mut a = [0u64; 256];
        let mut b = [0u64; 256];
        a[0] = 0xFF;
        b[0] = 0x00;
        assert_eq!(a.distance(&b), 8);
    }

    #[test]
    fn cam_pq_code_distance() {
        let a = [10u8, 20, 30, 40, 50, 60];
        let b = [15u8, 25, 35, 45, 55, 65];
        assert_eq!(a.distance(&b), 30); // 5×6
    }

    #[test]
    fn cam_pq_code_self_zero() {
        let a = [10u8, 20, 30, 40, 50, 60];
        assert_eq!(a.distance(&a), 0);
    }

    #[test]
    fn palette_edge_distance() {
        let a = [0u8, 0, 0];
        let b = [255u8, 255, 255];
        assert_eq!(a.distance(&b), 255 * 3);
        assert_eq!(a.similarity(&b), 0.0);
    }

    #[test]
    fn fisher_z_roundtrip() {
        let s = 0.8f32;
        let z = ((1.0 + s) / (1.0 - s)).ln() * 0.5;
        let recovered = fisher_z_inverse(z);
        assert!((recovered - s).abs() < 1e-5);
    }

    #[test]
    fn mean_similarity_fisher_averaging() {
        let z_values = vec![0.5, 0.5, 0.5];
        let mean = mean_similarity_fisher(&z_values);
        let expected = fisher_z_inverse(0.5);
        assert!((mean - expected).abs() < 1e-5);
    }

    #[test]
    fn similarity_z_positive_for_similar() {
        let a = [0u64; 256];
        let mut b = [0u64; 256];
        b[0] = 1; // 1 bit different
        let z = a.similarity_z(&b);
        assert!(z > 0.0, "similar vectors should have positive z");
    }

    #[test]
    fn similarity_z_near_zero_for_dissimilar() {
        let a = [0u64; 256];
        let b = [u64::MAX; 256];
        let z = a.similarity_z(&b);
        assert!(z < 0.01, "maximally different should have z near 0");
    }
}
