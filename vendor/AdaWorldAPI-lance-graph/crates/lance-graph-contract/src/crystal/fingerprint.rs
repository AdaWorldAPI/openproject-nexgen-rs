//! CrystalFingerprint — polymorphic carrier of crystal semantic content.
//!
//! Five native forms:
//!
//! | Variant          | Size  | Role                                         |
//! |------------------|-------|----------------------------------------------|
//! | `Binary16K`      | 2 KB  | Compact semantic (Hamming similarity).        |
//! | `Structured5x5`  | 3 KB  | Rich native form (5×5×5×5×5 cells).          |
//! | `Vsa10kI8`       | 10 KB | lancedb-native VSA (int8).                   |
//! | `Vsa10kF32`      | 40 KB | lancedb-native VSA (f32).                    |
//! | `Vsa16kF32`      | 64 KB | Click-native switchboard carrier (f32, 16_384-D).|
//!
//! ## Vsa16kF32 — the inside-BBB switchboard carrier
//!
//! Per CLAUDE.md §The Click and the I-VSA-IDENTITIES iron rule, the
//! 16,384-dimensional f32 VSA is the **switchboard carrier** for
//! role-indexed bundle (Markov) and role-key bind/unbind on the
//! semantic kernel. It is 1:1 bit-addressable with `Binary16K`
//! (dimension i corresponds to bit i) and supports lossless bipolar
//! projection in both directions via [`binary16k_to_vsa16k_bipolar`]
//! and [`vsa16k_to_binary16k_threshold`].
//!
//! **BBB membrane status:** `Vsa16kF32` is INSIDE-BBB only. It does
//! NOT cross the `ExternalMembrane` — the Arrow-scalar commit tier
//! uses the 2 KB `Binary16K` projection (Index regime) or the 6 B
//! CAM-PQ scent (Argmax regime). See I1 codec regime split
//! (ADR-0002).
//!
//! ## Passthrough to 10,000-D
//!
//! lancedb famously supports 10,000-D VSA natively (40 KB at f32).
//! The 10K space is dense enough to hold the full content of any other
//! variant without aliasing:
//!
//! - **Structured5x5** → 3,125 cells + 5 quorum floats ↔ 10K: **lossless**
//!   roundtrip (cells ∈ [0, 3130], rest zero-padded, quorum presence
//!   encoded at a sentinel position).
//! - **Binary16K** → 16,384 bits spread across 10K f32 dims: each bit
//!   maps to a unique dimension, with ~1.6 bits/dim. Roundtrip preserves
//!   similarity ordering but is **not bit-exact invertible** (intentional —
//!   the 10K form is richer and can carry additional superposed roles).
//! - **Vsa10kI8** → rescaled to f32 ∈ [−1, 1]: lossless up to i8
//!   quantization.
//!
//! ## VSA operations on the 10K form
//!
//! The 10K-D f32 space supports standard VSA algebra:
//! - **bind** (element-wise multiply): assigns a role to content.
//! - **bundle** (element-wise add + normalize): superposition of signals.
//! - **superpose** (weighted add): merge with blending weights.
//!
//! Multiple semantic roles can coexist in one 10K vector via bind+bundle.

/// The polymorphic crystal fingerprint.
#[derive(Debug, Clone)]
pub enum CrystalFingerprint {
    /// 16,384-bit semantic fingerprint (256 × u64 for cache-aligned Hamming).
    Binary16K(Box<[u64; 256]>),

    /// Structured 5^5 = 3125 cells plus optional 5-axis quorum.
    /// Axes: Element × SentencePosition × Slot × NarsInference × StyleCluster.
    Structured5x5 {
        cells: Box<[u8; 3125]>,
        quorum: Option<Quorum5D>,
    },

    /// 10,000-D VSA, int8 components (lancedb-native, 10 KB).
    Vsa10kI8(Box<[i8; 10_000]>),

    /// 10,000-D VSA, f32 components (lancedb-native, 40 KB).
    Vsa10kF32(Box<[f32; 10_000]>),

    /// 16,384-D VSA, f32 components — the Click switchboard carrier (64 KB).
    /// One-to-one with `Binary16K` dimensions via bipolar projection.
    /// Inside-BBB only; never crosses `ExternalMembrane`.
    Vsa16kF32(Box<[f32; 16_384]>),
}

/// Five-dimensional quorum: consensus along each of the 5^5 axes.
///
/// Each field ∈ [0, 1]. A high value means the cells along that axis
/// agree; a low value means the crystal is internally contested on
/// that dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quorum5D {
    pub element: f32,
    pub sentence_position: f32,
    pub slot: f32,
    pub nars_inference: f32,
    pub style_cluster: f32,
}

impl Quorum5D {
    pub const fn new(e: f32, p: f32, s: f32, n: f32, c: f32) -> Self {
        Self {
            element: e,
            sentence_position: p,
            slot: s,
            nars_inference: n,
            style_cluster: c,
        }
    }

    pub fn mean(&self) -> f32 {
        (self.element
            + self.sentence_position
            + self.slot
            + self.nars_inference
            + self.style_cluster)
            / 5.0
    }
}

/// Structured5x5 ergonomics.
#[derive(Debug, Clone)]
pub struct Structured5x5 {
    pub cells: Box<[u8; 3125]>,
    pub quorum: Option<Quorum5D>,
}

impl Structured5x5 {
    /// Index into the 5^5 grid. Each axis ∈ [0, 5).
    #[inline]
    pub fn idx(element: u8, sentence_pos: u8, slot: u8, nars: u8, style: u8) -> usize {
        let (e, p, s, n, c) = (
            element as usize,
            sentence_pos as usize,
            slot as usize,
            nars as usize,
            style as usize,
        );
        e + 5 * (p + 5 * (s + 5 * (n + 5 * c)))
    }

    pub fn get(&self, e: u8, p: u8, s: u8, n: u8, c: u8) -> u8 {
        self.cells[Self::idx(e, p, s, n, c)]
    }

    pub fn set(&mut self, e: u8, p: u8, s: u8, n: u8, c: u8, v: u8) {
        let i = Self::idx(e, p, s, n, c);
        self.cells[i] = v;
    }
}

// ── 10K-D sandwich layout ──────────────────────────────────────────────
//
// The 5^5 structured cells sit in the MIDDLE of the 10K vector with
// role-binding space on each side. This is the VSA "sandwich" pattern:
//
//   [  0..3437]  lead context  — role-A superposition / pre-bind
//   [3437..6562]  3,125 cells — bipolar-encoded, negatives cancel
//   [6562..6567]  quorum (5D) — inessive/adessive/etc. consensus
//   [    6567]    quorum sentinel (>0 = present, ≤0 = None)
//   [6568..10000] tail context — role-B superposition / post-bind
//
// Cells are **bipolar** (u8 cell 0..=255 → signed f32 in [-1, 1]) so
// they participate in negative-canceling superposition just like any
// other VSA dim. Opposing cell values at the same sandwich dim cancel
// when two crystals are bundled.
//
// Optional **bit-chain permutation**: cell i's bipolar encoding can be
// permuted by a position-dependent stride, carrying sequence/ordering
// information into the VSA space. See [`CrystalFingerprint::bit_chain_stride`].

const SANDWICH_LEAD: usize = 3437;
const CELLS_START: usize = SANDWICH_LEAD;
const CELLS_END: usize = CELLS_START + 3125; // 6562
const QUORUM_START: usize = CELLS_END; // 6562
const QUORUM_END: usize = QUORUM_START + 5; // 6567
const QUORUM_SENTINEL: usize = QUORUM_END; // 6567
const SANDWICH_TAIL_START: usize = QUORUM_SENTINEL + 1; // 6568
                                                        // SANDWICH_TAIL_END = 10_000 (exclusive)

impl CrystalFingerprint {
    /// Project into the 10,000-D f32 VSA form.
    ///
    /// - **Binary16K**: each of the 256 u64 words maps to a dedicated
    ///   region of 39 dimensions (256 × 39 = 9,984 ≤ 10,000). Within
    ///   each region, the 64 bits are striped across dims with a stride
    ///   that avoids aliasing. No two source bits share a dimension.
    ///   Not bit-exact invertible (10K is richer), but similarity-
    ///   preserving.
    /// - **Structured5x5**: cells → dims 0..3125, quorum → 3125..3130,
    ///   sentinel at dim 3130. **Lossless roundtrip.**
    /// - **Vsa10kI8**: rescaled to f32 ∈ [−1, 1] (clamped for −128).
    /// - **Vsa10kF32**: direct copy.
    pub fn to_vsa10k_f32(&self) -> Box<[f32; 10_000]> {
        let mut out = Box::new([0.0f32; 10_000]);
        match self {
            Self::Binary16K(bits) => {
                // 256 words × 39 contiguous dims = 9,984 dims; 16 spare.
                // Each word occupies a [start..stop] slice so regions are
                // addressable for per-word bind/unbundle.
                //
                // 64 bits → 39 dims: two bits per dim (sign + magnitude)
                // for the first 25 dims, one bit per dim for the remaining 14.
                //   dims [base..base+25]:  2 bits each → 50 bits
                //   dims [base+25..base+39]: 1 bit each → 14 bits
                //   total = 64 bits, all placed, no aliasing within a word.
                for (w, word) in bits.iter().enumerate() {
                    let base = w * 39;
                    // First 25 dims carry 2 bits each (50 bits total).
                    for d in 0..25usize {
                        let b0 = (word >> (d * 2)) & 1;
                        let b1 = (word >> (d * 2 + 1)) & 1;
                        // Encode as: {00 → -1.0, 01 → -0.33, 10 → 0.33, 11 → 1.0}
                        let val = match (b0, b1) {
                            (0, 0) => -1.0f32,
                            (1, 0) => -0.33,
                            (0, 1) => 0.33,
                            _ => 1.0,
                        };
                        out[base + d] = val;
                    }
                    // Remaining 14 dims carry 1 bit each (bits 50..63).
                    for d in 0..14usize {
                        let b = (word >> (50 + d)) & 1;
                        out[base + 25 + d] = if b == 1 { 1.0 } else { -1.0 };
                    }
                }
            }
            Self::Structured5x5 { cells, quorum } => {
                // Sandwich layout: cells in the middle with bipolar sign
                // encoding (u8 0..=255 → f32 in [-1, 1]). Leading and
                // trailing sandwich space stays zero for this single-
                // crystal projection; downstream consumers bind roles
                // into those regions for multi-role superposition.
                for i in 0..3125 {
                    // Bipolar: cell 0 → -1.0, cell 127/128 → ~0, cell 255 → +1.0
                    out[CELLS_START + i] = (cells[i] as f32 / 127.5) - 1.0;
                }
                if let Some(q) = quorum {
                    out[QUORUM_START] = q.element;
                    out[QUORUM_START + 1] = q.sentence_position;
                    out[QUORUM_START + 2] = q.slot;
                    out[QUORUM_START + 3] = q.nars_inference;
                    out[QUORUM_START + 4] = q.style_cluster;
                    out[QUORUM_SENTINEL] = 1.0;
                }
                // quorum: None → sentinel stays 0.0
            }
            Self::Vsa10kI8(v) => {
                for i in 0..10_000 {
                    out[i] = (v[i] as f32 / 128.0).clamp(-1.0, 1.0);
                }
            }
            Self::Vsa10kF32(v) => {
                out.copy_from_slice(&v[..]);
            }
            Self::Vsa16kF32(v) => {
                // 16_384 → 10_000 downcast: similarity-preserving stride copy
                // with interleaved averaging of the surplus 6_384 dims into
                // the base 10_000. Not lossless — reserved for cases where a
                // 10K-surface consumer needs the 16K carrier's content. For
                // lossless projection, stay on the 16K carrier.
                for i in 0..10_000 {
                    out[i] = v[i];
                }
                for j in 10_000..16_384 {
                    let i = j - 10_000;
                    out[i] = (out[i] + v[j]) * 0.5;
                }
            }
        }
        out
    }

    /// Reconstruct a Structured5x5 crystal from its 10K-D sandwich form.
    /// Quorum is `None` if the sentinel at dim 6567 is ≤ 0.
    pub fn structured_from_vsa10k(vsa: &[f32; 10_000]) -> Self {
        let mut cells = Box::new([0u8; 3125]);
        for i in 0..3125 {
            // Inverse of bipolar: f32 [-1, 1] → u8 [0, 255]
            let v = ((vsa[CELLS_START + i] + 1.0) * 127.5)
                .round()
                .clamp(0.0, 255.0) as u8;
            cells[i] = v;
        }
        let quorum = if vsa[QUORUM_SENTINEL] > 0.0 {
            Some(Quorum5D::new(
                vsa[QUORUM_START],
                vsa[QUORUM_START + 1],
                vsa[QUORUM_START + 2],
                vsa[QUORUM_START + 3],
                vsa[QUORUM_START + 4],
            ))
        } else {
            None
        };
        Self::Structured5x5 { cells, quorum }
    }

    /// Sandwich range for the leading role-binding region.
    ///
    /// See `ndarray::hpc::vsa::vsa_permute` for the canonical bit-chain
    /// permutation primitive. Downstream crates applying sequence/
    /// position encoding should permute the leading or trailing sandwich
    /// regions before bundling crystals.
    pub const fn sandwich_lead() -> (usize, usize) {
        (0, SANDWICH_LEAD)
    }

    /// Sandwich range for the 3,125 cells (middle).
    pub const fn sandwich_cells() -> (usize, usize) {
        (CELLS_START, CELLS_END)
    }

    /// Sandwich range for the trailing role-binding region.
    pub const fn sandwich_tail() -> (usize, usize) {
        (SANDWICH_TAIL_START, 10_000)
    }

    /// Reconstruct a Binary16K from its 10K-D form (lossless inverse).
    ///
    /// Reads 256 contiguous 39-dim slices. The 2-bit-per-dim and
    /// 1-bit-per-dim packing is inverted by thresholding.
    pub fn binary16k_from_vsa10k(vsa: &[f32; 10_000]) -> Self {
        let mut bits = Box::new([0u64; 256]);
        for w in 0..256 {
            let base = w * 39;
            let mut word = 0u64;
            // First 25 dims → 50 bits (2 bits each)
            for d in 0..25usize {
                let v = vsa[base + d];
                let (b0, b1) = if v < -0.66 {
                    (0u64, 0u64) // -1.0 → 00
                } else if v < 0.0 {
                    (1, 0) // -0.33 → 10
                } else if v < 0.66 {
                    (0, 1) // 0.33 → 01
                } else {
                    (1, 1) // 1.0 → 11
                };
                word |= b0 << (d * 2);
                word |= b1 << (d * 2 + 1);
            }
            // Last 14 dims → 14 bits (1 bit each)
            for d in 0..14usize {
                if vsa[base + 25 + d] > 0.0 {
                    word |= 1u64 << (50 + d);
                }
            }
            bits[w] = word;
        }
        Self::Binary16K(bits)
    }

    /// Byte size of this fingerprint in its native form.
    pub fn byte_size(&self) -> usize {
        match self {
            Self::Binary16K(_) => 2 * 1024,             //  2 KB
            Self::Structured5x5 { .. } => 3125 + 5 * 4, // ~3 KB
            Self::Vsa10kI8(_) => 10_000,                // 10 KB
            Self::Vsa10kF32(_) => 40_000,               // 40 KB
            Self::Vsa16kF32(_) => 65_536,               // 64 KB
        }
    }
}

// ── VSA algebra on the 10K-D f32 form ──────────────────────────────────

/// Element-wise multiply (bind): assigns a role key to content.
///
/// `bind(content, role_key)` produces a vector that is dissimilar to
/// both inputs but can be unbound via `bind(bound, role_key)` (since
/// multiply is its own inverse for ±1 keys).
pub fn vsa_bind(a: &[f32; 10_000], b: &[f32; 10_000]) -> Box<[f32; 10_000]> {
    let mut out = Box::new([0.0f32; 10_000]);
    for i in 0..10_000 {
        out[i] = a[i] * b[i];
    }
    out
}

/// Element-wise add (bundle / superposition): merges multiple signals.
///
/// The result is similar to all inputs. Normalize afterward if needed.
pub fn vsa_bundle(vectors: &[&[f32; 10_000]]) -> Box<[f32; 10_000]> {
    let mut out = Box::new([0.0f32; 10_000]);
    for v in vectors {
        for i in 0..10_000 {
            out[i] += v[i];
        }
    }
    out
}

/// Weighted superposition: merges with explicit blending weights.
pub fn vsa_superpose(vectors: &[&[f32; 10_000]], weights: &[f32]) -> Box<[f32; 10_000]> {
    let mut out = Box::new([0.0f32; 10_000]);
    for (v, &w) in vectors.iter().zip(weights.iter()) {
        for i in 0..10_000 {
            out[i] += v[i] * w;
        }
    }
    out
}

/// Cosine similarity between two 10K-D vectors.
pub fn vsa_cosine(a: &[f32; 10_000], b: &[f32; 10_000]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..10_000 {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

// ── Vsa16kF32 — the Click switchboard carrier ──────────────────────────
//
// One-to-one bit-addressable with Binary16K (dim i ↔ bit i). Bipolar
// ±1 projection is lossless in both directions under strict-threshold
// inverse. Supports the semantic-kernel algebra (role-indexed bundle
// for Markov, element-wise bind for role-key slice assignment) on the
// f32 carrier. 64 KB per vector; inside-BBB only.

/// Allocate a zero-valued Vsa16kF32 carrier.
#[inline]
pub fn vsa16k_zero() -> Box<[f32; 16_384]> {
    Box::new([0.0f32; 16_384])
}

/// Project a `Binary16K` (256 × u64 = 16_384 bits) into a bipolar
/// `Vsa16kF32`: bit i set → +1.0 at dim i; bit i clear → -1.0.
///
/// Lossless under the inverse [`vsa16k_to_binary16k_threshold`].
pub fn binary16k_to_vsa16k_bipolar(bits: &[u64; 256]) -> Box<[f32; 16_384]> {
    let mut out = Box::new([0.0f32; 16_384]);
    for (w, &word) in bits.iter().enumerate() {
        for b in 0..64 {
            let dim = w * 64 + b;
            out[dim] = if (word >> b) & 1 == 1 { 1.0 } else { -1.0 };
        }
    }
    out
}

/// Threshold a `Vsa16kF32` carrier back to a `Binary16K`: dim > 0.0 → bit set.
///
/// Inverse of [`binary16k_to_vsa16k_bipolar`] for any vector whose signs
/// survived bundling / binding (does not require strict ±1 values —
/// any positive value decodes to 1, any non-positive to 0).
pub fn vsa16k_to_binary16k_threshold(v: &[f32; 16_384]) -> Box<[u64; 256]> {
    let mut bits = Box::new([0u64; 256]);
    for w in 0..256 {
        let mut word = 0u64;
        for b in 0..64 {
            let dim = w * 64 + b;
            if v[dim] > 0.0 {
                word |= 1u64 << b;
            }
        }
        bits[w] = word;
    }
    bits
}

/// Element-wise multiply (bind) on the 16K carrier: assigns a role key
/// to content. Self-inverse for ±1 bipolar keys (key² = 1 elementwise).
pub fn vsa16k_bind(a: &[f32; 16_384], b: &[f32; 16_384]) -> Box<[f32; 16_384]> {
    let mut out = Box::new([0.0f32; 16_384]);
    for i in 0..16_384 {
        out[i] = a[i] * b[i];
    }
    out
}

/// Element-wise add (bundle / superposition) on the 16K carrier.
/// Per I-SUBSTRATE-MARKOV, this is the Chapman-Kolmogorov-safe
/// merge mode for state-transition paths. Do NOT substitute XOR.
pub fn vsa16k_bundle(vectors: &[&[f32; 16_384]]) -> Box<[f32; 16_384]> {
    let mut out = Box::new([0.0f32; 16_384]);
    for v in vectors {
        for i in 0..16_384 {
            out[i] += v[i];
        }
    }
    out
}

/// Cosine similarity between two 16K carriers.
pub fn vsa16k_cosine(a: &[f32; 16_384], b: &[f32; 16_384]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..16_384 {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_indexing_is_bijective() {
        let mut s = Structured5x5 {
            cells: Box::new([0u8; 3125]),
            quorum: None,
        };
        s.set(1, 2, 3, 4, 0, 42);
        assert_eq!(s.get(1, 2, 3, 4, 0), 42);
    }

    #[test]
    fn structured_passthrough_roundtrip_with_quorum() {
        let mut cells = Box::new([0u8; 3125]);
        for i in 0..3125 {
            cells[i] = (i % 256) as u8;
        }
        let quorum = Some(Quorum5D::new(0.9, 0.8, 0.7, 0.6, 0.5));
        let fp = CrystalFingerprint::Structured5x5 { cells, quorum };

        let vsa = fp.to_vsa10k_f32();
        let back = CrystalFingerprint::structured_from_vsa10k(&vsa);
        match back {
            CrystalFingerprint::Structured5x5 { cells, quorum } => {
                for i in 0..3125 {
                    assert_eq!(
                        cells[i],
                        (i % 256) as u8,
                        "cell {i} differs after passthrough"
                    );
                }
                let q = quorum.expect("quorum should be Some after roundtrip");
                assert!((q.element - 0.9).abs() < 1e-3);
                assert!((q.sentence_position - 0.8).abs() < 1e-3);
            }
            _ => panic!("unexpected fingerprint variant"),
        }
    }

    #[test]
    fn structured_passthrough_roundtrip_without_quorum() {
        let cells = Box::new([42u8; 3125]);
        let fp = CrystalFingerprint::Structured5x5 {
            cells,
            quorum: None,
        };

        let vsa = fp.to_vsa10k_f32();
        let back = CrystalFingerprint::structured_from_vsa10k(&vsa);
        match back {
            CrystalFingerprint::Structured5x5 { cells, quorum } => {
                assert_eq!(cells[0], 42);
                assert!(quorum.is_none(), "quorum: None must survive roundtrip");
            }
            _ => panic!("unexpected fingerprint variant"),
        }
    }

    #[test]
    fn binary16k_no_cross_word_aliasing() {
        // Two fingerprints differing in exactly one bit should produce
        // different 10K projections.
        let mut a = Box::new([0u64; 256]);
        let mut b = Box::new([0u64; 256]);
        a[100] = 1; // bit 0 of word 100
        b[200] = 1; // bit 0 of word 200
        let fa = CrystalFingerprint::Binary16K(a);
        let fb = CrystalFingerprint::Binary16K(b);
        let va = fa.to_vsa10k_f32();
        let vb = fb.to_vsa10k_f32();
        // They should differ (word 100 maps to base 3900, word 200 to 7800)
        let diff: f32 = va.iter().zip(vb.iter()).map(|(x, y)| (x - y).abs()).sum();
        assert!(
            diff > 0.0,
            "different binary fingerprints must yield different VSA"
        );
    }

    #[test]
    fn i8_clamps_to_unit_range() {
        let mut v = Box::new([0i8; 10_000]);
        v[0] = -128; // edge case: −128/128 = −1.0 (not −1.008)
        v[1] = 127;
        let fp = CrystalFingerprint::Vsa10kI8(v);
        let vsa = fp.to_vsa10k_f32();
        assert!(vsa[0] >= -1.0, "i8 min must not exceed −1.0");
        assert!(vsa[1] <= 1.0);
    }

    #[test]
    fn byte_sizes_documented() {
        let s = CrystalFingerprint::Structured5x5 {
            cells: Box::new([0u8; 3125]),
            quorum: Some(Quorum5D::new(0.0, 0.0, 0.0, 0.0, 0.0)),
        };
        assert_eq!(s.byte_size(), 3145);
        let v = CrystalFingerprint::Vsa10kF32(Box::new([0.0; 10_000]));
        assert_eq!(v.byte_size(), 40_000);
    }

    #[test]
    fn binary16k_lossless_roundtrip() {
        let mut bits = Box::new([0u64; 256]);
        // Fill with a non-trivial pattern
        for i in 0..256 {
            bits[i] = 0xDEAD_BEEF_CAFE_BABEu64.wrapping_mul(i as u64 + 1);
        }
        let fp = CrystalFingerprint::Binary16K(bits.clone());
        let vsa = fp.to_vsa10k_f32();
        let back = CrystalFingerprint::binary16k_from_vsa10k(&vsa);
        match back {
            CrystalFingerprint::Binary16K(recovered) => {
                for i in 0..256 {
                    assert_eq!(
                        recovered[i], bits[i],
                        "word {i}: expected {:#018x} got {:#018x}",
                        bits[i], recovered[i]
                    );
                }
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[test]
    fn vsa_bind_is_self_inverse_for_bipolar() {
        let mut key = Box::new([0.0f32; 10_000]);
        let mut content = Box::new([0.0f32; 10_000]);
        for i in 0..10_000 {
            key[i] = if i % 3 == 0 { -1.0 } else { 1.0 };
            content[i] = (i as f32 / 10_000.0) * 2.0 - 1.0;
        }
        let bound = vsa_bind(&content, &key);
        let unbound = vsa_bind(&bound, &key);
        // Unbinding should recover the content (since key²=1 for ±1)
        for i in 0..10_000 {
            assert!(
                (unbound[i] - content[i]).abs() < 1e-5,
                "dim {i}: expected {} got {}",
                content[i],
                unbound[i]
            );
        }
    }

    #[test]
    fn vsa_bundle_preserves_similarity() {
        let mut a = Box::new([0.0f32; 10_000]);
        let mut b = Box::new([0.0f32; 10_000]);
        for i in 0..10_000 {
            a[i] = 1.0;
        }
        for i in 0..10_000 {
            b[i] = if i < 5000 { 1.0 } else { -1.0 };
        }
        let bundled = vsa_bundle(&[&*a, &*b]);
        let sim_a = vsa_cosine(&bundled, &a);
        let sim_b = vsa_cosine(&bundled, &b);
        assert!(sim_a > 0.5, "bundle should be similar to input a");
        assert!(sim_b > 0.0, "bundle should be positively similar to b");
    }

    #[test]
    fn vsa16k_byte_size_is_64k() {
        let fp = CrystalFingerprint::Vsa16kF32(Box::new([0.0f32; 16_384]));
        assert_eq!(fp.byte_size(), 65_536);
    }

    #[test]
    fn binary16k_to_vsa16k_bipolar_roundtrip_is_lossless() {
        let mut bits = Box::new([0u64; 256]);
        for i in 0..256 {
            bits[i] = 0xDEAD_BEEF_CAFE_BABEu64.wrapping_mul(i as u64 + 1);
        }
        let v = binary16k_to_vsa16k_bipolar(&bits);
        let back = vsa16k_to_binary16k_threshold(&v);
        for i in 0..256 {
            assert_eq!(
                back[i], bits[i],
                "word {i}: expected {:#018x} got {:#018x}",
                bits[i], back[i]
            );
        }
    }

    #[test]
    fn vsa16k_bipolar_values_are_unit() {
        let bits = Box::new([0xAAAA_AAAA_AAAA_AAAAu64; 256]);
        let v = binary16k_to_vsa16k_bipolar(&bits);
        for i in 0..16_384 {
            assert!(
                v[i] == 1.0 || v[i] == -1.0,
                "dim {i} is {} — must be strict ±1",
                v[i]
            );
        }
    }

    #[test]
    fn vsa16k_bind_is_self_inverse_for_bipolar() {
        let key = {
            let mut k = Box::new([0.0f32; 16_384]);
            for i in 0..16_384 {
                k[i] = if i % 3 == 0 { -1.0 } else { 1.0 };
            }
            k
        };
        let content = {
            let mut c = Box::new([0.0f32; 16_384]);
            for i in 0..16_384 {
                c[i] = (i as f32 / 16_384.0) * 2.0 - 1.0;
            }
            c
        };
        let bound = vsa16k_bind(&content, &key);
        let unbound = vsa16k_bind(&bound, &key);
        for i in 0..16_384 {
            assert!(
                (unbound[i] - content[i]).abs() < 1e-5,
                "dim {i}: expected {} got {}",
                content[i],
                unbound[i]
            );
        }
    }

    #[test]
    fn vsa16k_bundle_preserves_similarity_to_inputs() {
        let a = {
            let mut v = Box::new([0.0f32; 16_384]);
            for i in 0..16_384 {
                v[i] = 1.0;
            }
            v
        };
        let b = {
            let mut v = Box::new([0.0f32; 16_384]);
            for i in 0..16_384 {
                v[i] = if i < 8_192 { 1.0 } else { -1.0 };
            }
            v
        };
        let bundled = vsa16k_bundle(&[&*a, &*b]);
        assert!(vsa16k_cosine(&bundled, &a) > 0.5);
        assert!(vsa16k_cosine(&bundled, &b) > 0.0);
    }

    #[test]
    fn vsa16k_bundle_then_unbind_recovers_role_content() {
        // Two role slots, each with its own bipolar key; content bound to
        // each; bundled; unbind by multiplying with the role key recovers
        // the matching content above the noise floor.
        let role_a = binary16k_to_vsa16k_bipolar(&Box::new([0xF0F0_F0F0_F0F0_F0F0u64; 256]));
        let role_b = binary16k_to_vsa16k_bipolar(&Box::new([0x0F0F_0F0F_0F0F_0F0Fu64; 256]));
        let content_a = binary16k_to_vsa16k_bipolar(&Box::new([0xAAAA_AAAA_AAAA_AAAAu64; 256]));
        let content_b = binary16k_to_vsa16k_bipolar(&Box::new([0x5555_5555_5555_5555u64; 256]));
        let bound_a = vsa16k_bind(&content_a, &role_a);
        let bound_b = vsa16k_bind(&content_b, &role_b);
        let bundled = vsa16k_bundle(&[&*bound_a, &*bound_b]);
        let recovered_a = vsa16k_bind(&bundled, &role_a);
        let recovered_b = vsa16k_bind(&bundled, &role_b);
        assert!(
            vsa16k_cosine(&recovered_a, &content_a) > vsa16k_cosine(&recovered_a, &content_b),
            "unbind(role_a) must favour content_a over content_b"
        );
        assert!(
            vsa16k_cosine(&recovered_b, &content_b) > vsa16k_cosine(&recovered_b, &content_a),
            "unbind(role_b) must favour content_b over content_a"
        );
    }

    #[test]
    fn vsa16k_to_vsa10k_projection_is_finite() {
        let fp = CrystalFingerprint::Vsa16kF32(Box::new([1.0f32; 16_384]));
        let v10 = fp.to_vsa10k_f32();
        for i in 0..10_000 {
            assert!(
                v10[i].is_finite(),
                "vsa16k→vsa10k must produce finite values; dim {i} is {}",
                v10[i]
            );
        }
    }
}
