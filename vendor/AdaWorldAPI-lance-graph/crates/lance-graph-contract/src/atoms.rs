//! Atom layer — the **LOCKED 33-dim ThinkingStyleVector** basis.
//!
//! # Source (do NOT re-derive)
//!
//! `EPIPHANIES.md` **E-AGICHAT-DIMENSION-CONTRACT** → agichat's
//! `CANONICAL_DIMENSION_ALLOCATION.md` ("Status: **LOCKED**"). The basis is **not**
//! derived (no ICA/PCA, no "demote the 36 styles") — it is the locked 33-dim
//! allocation, restored on the i4 SoA floor (`E-I4-META-1`, the shipped i4-32 unpack).
//!
//! # Layers (smallest → largest)
//!
//! ```text
//! atom    = ONE lane of the vector (e.g. `deduce`, `R5`, `phi`) — bare-metal, not human-legible.
//! style   = ONE i4 VECTOR over all 33 atoms (a weighting) — the MOLECULE (Kant, Schopenhauer).
//! persona = a composition of styles + thresholds + purpose + β.
//! ```
//!
//! An [`Atom`] is a lane, **not** a [`crate::thinking::ThinkingStyle`]. A style is an
//! `I4x32` vector over the atoms; `ThinkingStyle` is the 6-bit identity that *resolves
//! to* such a vector. The groups below (Pearl/Rung/Σ/Ops/Presence/Meta) are **allocation
//! families**, neither atoms nor molecules.
//!
//! # Execution stack: `atoms → cognitive-shader-driver → SIMD`
//!
//! **Atoms are NOT SIMD.** This module defines the lanes (the catalogue) and the
//! bare-metal carrier (`I4x32` pack/unpack). All composition / affinity / sweep work
//! **dispatches through `cognitive-shader-driver`**, which owns the ndarray i4 SIMD
//! path. There is deliberately no dot-product / SIMD hot path in this layer.
//!
//! # Business is not here
//!
//! No business/FIBU lanes. Business is an **OGIT-inherited sidecar** (`E-OGIT-STAKES-LINCHPIN`):
//! request class → `MappingRow` → `Marking::Financial` → bookkeeping savant. Never an atom.

// ---------------------------------------------------------------------------
// Layout — RESOLVED (A3, 2026-06-01)
// See `.claude/knowledge/ephemeral-warm-cold-lifecycle.md` + spec §8 + jan's clarification.
// ---------------------------------------------------------------------------

// RESOLVED — carrier shape. The carrier is N **signed** i4 dimensions; the SIGN is the
// bipolar pole (e.g. focus +/fan-out −, love +/hate −), so a "± pair" is conceptually ONE
// signed dim. `I4x32` = 32 signed dims (64 poles, 16 B); `I4x64` = 64 signed dims
// (128 poles, 256 bit / 32 B). There is NO `{instance, reference}` dual — the resolver
// (A4) compares two such vectors by INTEGER i4 distance (no float). ("64" in spec §8 was
// 64 poles, not 64 lanes.)

// RESOLVED — 32-vs-33. The 33 logical atoms occupy dims 0..32 of the 64-dim `I4x64`
// carrier (dims 33..63 spare). No trim, no out-of-band lane; the catalogue stays locked
// at 33. (Collapsing the ± pairs into single signed dims is a later catalogue reframe — A4.)

// RESOLVED — "8 spare" vs "4 Presence + 4 Meta": Presence+Meta (4+4) is canonical
// (`group_counts_match_the_contract` hard-asserts it). "8 spare" was stale STYLE_ENCODING.md.

// RESOLVED — sign/scale: the carrier stores signed i4 `[−8, 7]` (two's-complement)
// UNIFORMLY; `pack`/`unpack` are sign-agnostic (the caller pre-scales). Unsigned ordinal
// groups (Pearl/Rung/Σ) use `[0, 7]`; bipolar dims use the full `[−8, 7]`.
// (`AtomGroup::is_signed` + the integer-distance resolver are A4.)

// ---------------------------------------------------------------------------
// Bare-metal carrier (no SIMD here — dispatch through cognitive-shader-driver)
// ---------------------------------------------------------------------------

/// Packed 32-lane signed-4-bit vector — the bare-metal carrier a **style** rides on.
///
/// 32 signed i4 values in 16 bytes (two nibbles per byte). This holds a thinking-style
/// vector (a weighting over the atom lanes). It is the *bytes*; the cognition is the
/// style/persona **objects** built on it (see `recipe.rs`).
///
/// Composition, affinity, and any vectorized sweep are **not** implemented here — they
/// dispatch through `cognitive-shader-driver` (which owns the ndarray i4-32 SIMD). This
/// type only packs/unpacks the lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C, align(16))]
pub struct I4x32 {
    bytes: [u8; 16],
}

impl I4x32 {
    /// The all-zero style vector (every lane neutral).
    pub const ZERO: Self = Self { bytes: [0u8; 16] };

    /// Pack 32 signed dims into the i4 CAM carrier, saturating to `[−8, 7]`.
    ///
    /// The carrier is a **sparse, deterministic 32×CAM address** (128-bit) — the non-zero
    /// dims are the intensity "smell". Resolution is CAM addressing, **not** vector search.
    /// Each dim is a signed bipolar axis (sign = pole, e.g. −introspection..+exploration).
    /// Two's-complement nibble: dim `2k` → low nibble of byte `k`, `2k+1` → high nibble
    /// (byte-compatible with `QualiaI4_16D` and the `CausalEdge64` mantissa). `pack` is
    /// **sign-agnostic** and only saturates. There is **NO f32 round-trip** — the i4 texture
    /// arrives as signed bytes (the "smell") and stays integer; texture → thinking style is
    /// the fastest route (~4 CPU cycles), a branchless integer transform, never a float
    /// compute. The asymmetric bipolar pole lives in the i4 encoding itself. No SIMD here.
    pub fn pack(values: &[i8; 32]) -> Self {
        let mut bytes = [0u8; 16];
        let mut k = 0;
        while k < 16 {
            let lo = (values[2 * k].clamp(-8, 7) as u8) & 0x0F;
            let hi = (values[2 * k + 1].clamp(-8, 7) as u8) & 0x0F;
            bytes[k] = lo | (hi << 4);
            k += 1;
        }
        Self { bytes }
    }

    /// Unpack the 32 dims to signed bytes (sign-extended i4, range `[−8, 7]`).
    pub fn unpack(&self) -> [i8; 32] {
        let mut out = [0i8; 32];
        let mut k = 0;
        while k < 16 {
            let b = self.bytes[k];
            out[2 * k] = Self::sext4(b & 0x0F);
            out[2 * k + 1] = Self::sext4(b >> 4);
            k += 1;
        }
        out
    }

    /// Sign-extend a 4-bit two's-complement nibble to `i8` in `[−8, 7]`.
    ///
    /// Carrier-owned (the nibble codec belongs to the carrier, not a free fn);
    /// `I4x64` reuses it via `I4x32::sext4`.
    #[inline]
    const fn sext4(nibble: u8) -> i8 {
        (((nibble & 0x0F) << 4) as i8) >> 4
    }
}

/// Packed 64-dim signed-i4 vector — the wide CAM carrier (`I4-64D`, 256 bit).
///
/// 64 signed i4 **dimensions** in 32 bytes (two nibbles per byte). Same role as [`I4x32`]
/// at double the width: a sparse, deterministic 64×CAM address whose non-zero dims are the
/// intensity "smell". Each dim is a signed bipolar axis (sign = pole). The 33 locked atoms
/// occupy dims 0..32; dims 33..63 are spare. Resolution is CAM addressing, **not** vector
/// search — no float, no `{instance, reference}` dual. `pack`/`unpack` are sign-agnostic;
/// **no f32 round-trip** — the i4 texture stays integer end to end (texture → style ~4 cycles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C, align(16))]
pub struct I4x64 {
    bytes: [u8; 32],
}

impl I4x64 {
    /// The all-zero vector (every dim neutral).
    pub const ZERO: Self = Self { bytes: [0u8; 32] };

    /// Pack 64 signed dims into the i4 carrier, saturating to `[−8, 7]`.
    ///
    /// Two's-complement nibble (dim `2k` → low, `2k+1` → high); sign-agnostic.
    pub fn pack(values: &[i8; 64]) -> Self {
        let mut bytes = [0u8; 32];
        let mut k = 0;
        while k < 32 {
            let lo = (values[2 * k].clamp(-8, 7) as u8) & 0x0F;
            let hi = (values[2 * k + 1].clamp(-8, 7) as u8) & 0x0F;
            bytes[k] = lo | (hi << 4);
            k += 1;
        }
        Self { bytes }
    }

    /// Unpack the 64 dims to signed bytes (sign-extended i4, range `[−8, 7]`).
    pub fn unpack(&self) -> [i8; 64] {
        let mut out = [0i8; 64];
        let mut k = 0;
        while k < 32 {
            let b = self.bytes[k];
            out[2 * k] = I4x32::sext4(b & 0x0F);
            out[2 * k + 1] = I4x32::sext4(b >> 4);
            k += 1;
        }
        out
    }
}

// ---------------------------------------------------------------------------
// The LOCKED atom catalogue (the 33-dim TSV allocation)
// ---------------------------------------------------------------------------

/// Allocation family of an atom lane. Families are organizational, not atoms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomGroup {
    /// Pearl causal ladder (3): association / intervention / counterfactual.
    Pearl,
    /// Meaning-depth rung ladder (9): R1–R9.
    Rung,
    /// σ-tier chain (5): Ω / Δ / Φ / Θ / Λ.
    Sigma,
    /// Cognitive operations (8): the inference modes + fanout ops.
    Operation,
    /// Presence modes (4).
    Presence,
    /// Meta knobs (4).
    Meta,
}

/// One lane of the LOCKED 33-dim TSV. `dim` is the canonical lane index (0..33).
#[derive(Debug, Clone, Copy)]
pub struct Atom {
    /// Canonical lane index, 0..33, in locked allocation order.
    pub dim: u8,
    /// Allocation family.
    pub group: AtomGroup,
    /// Locked lane name.
    pub name: &'static str,
}

impl Atom {
    const fn new(dim: u8, group: AtomGroup, name: &'static str) -> Self {
        Self { dim, group, name }
    }
}

use AtomGroup::*;

/// The LOCKED 33-dim TSV allocation (E-AGICHAT-DIMENSION-CONTRACT), in canonical order.
///
/// Order is the contract — `CANONICAL_DIMENSION_ALLOCATION.md` rejects arbitrary moves.
pub const CANONICAL_ATOMS: [Atom; 33] = [
    // 3 Pearl — causal ladder (ordinal)
    Atom::new(0, Pearl, "see_association"),
    Atom::new(1, Pearl, "do_intervention"),
    Atom::new(2, Pearl, "imagine_counterfactual"),
    // 9 Rung — meaning-depth ladder (ordinal) 🪜
    Atom::new(3, Rung, "rung_r1"),
    Atom::new(4, Rung, "rung_r2"),
    Atom::new(5, Rung, "rung_r3"),
    Atom::new(6, Rung, "rung_r4"),
    Atom::new(7, Rung, "rung_r5"),
    Atom::new(8, Rung, "rung_r6"),
    Atom::new(9, Rung, "rung_r7"),
    Atom::new(10, Rung, "rung_r8"),
    Atom::new(11, Rung, "rung_r9"),
    // 5 Sigma — σ-tier chain (ordinal)
    Atom::new(12, Sigma, "sigma_omega"),
    Atom::new(13, Sigma, "sigma_delta"),
    Atom::new(14, Sigma, "sigma_phi"),
    Atom::new(15, Sigma, "sigma_theta"),
    Atom::new(16, Sigma, "sigma_lambda"),
    // 8 Operations — inference modes + fanout ops (deduce↔induce is the one ± pair)
    Atom::new(17, Operation, "abduct"),
    Atom::new(18, Operation, "deduce"),
    Atom::new(19, Operation, "induce"),
    Atom::new(20, Operation, "synthesize"),
    Atom::new(21, Operation, "preflight"),
    Atom::new(22, Operation, "escalate"),
    Atom::new(23, Operation, "transcend"),
    Atom::new(24, Operation, "model_other"),
    // 4 Presence — modes (authentic↔performance is a ± pair)  [BLOCKED: "8 spare" alt]
    Atom::new(25, Presence, "authentic"),
    Atom::new(26, Presence, "performance"),
    Atom::new(27, Presence, "protective"),
    Atom::new(28, Presence, "absent"),
    // 4 Meta — knobs (exploration = explore↔exploit / temperature)  [BLOCKED: "8 spare" alt]
    Atom::new(29, Meta, "confidence_threshold"),
    Atom::new(30, Meta, "preflight_depth"),
    Atom::new(31, Meta, "exploration"),
    Atom::new(32, Meta, "verbosity"),
];

/// Look up a locked atom by canonical lane index (0..33).
#[inline]
pub fn atom(dim: u8) -> Option<&'static Atom> {
    CANONICAL_ATOMS.get(dim as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carrier_layout_is_16_bytes() {
        assert_eq!(core::mem::size_of::<I4x32>(), 16);
        assert_eq!(core::mem::align_of::<I4x32>(), 16);
    }

    #[test]
    fn catalogue_is_locked_33_in_order() {
        assert_eq!(CANONICAL_ATOMS.len(), 33);
        for (i, a) in CANONICAL_ATOMS.iter().enumerate() {
            assert_eq!(
                a.dim as usize, i,
                "lane dim must equal its index (locked order)"
            );
            assert!(!a.name.is_empty());
        }
    }

    #[test]
    fn group_counts_match_the_contract() {
        let count = |g: AtomGroup| CANONICAL_ATOMS.iter().filter(|a| a.group == g).count();
        assert_eq!(count(Pearl), 3);
        assert_eq!(count(Rung), 9);
        assert_eq!(count(Sigma), 5);
        assert_eq!(count(Operation), 8);
        assert_eq!(count(Presence), 4);
        assert_eq!(count(Meta), 4);
    }

    // ---- A3: the signed-i4 CAM codec (I4x32 / I4x64) ----

    #[test]
    fn pack_unpack_round_trips_every_dim() {
        let mut v = [0i8; 32];
        for (d, slot) in v.iter_mut().enumerate() {
            *slot = ((d as i8) % 15) - 7; // distinct, in [−7, 7]
        }
        assert_eq!(I4x32::pack(&v).unpack(), v);
    }

    #[test]
    fn pack_unpack_full_signed_range() {
        for val in -8..=7i8 {
            let v = [val; 32];
            assert_eq!(I4x32::pack(&v).unpack(), v, "value {val} must round-trip");
        }
    }

    #[test]
    fn pack_saturates_out_of_range() {
        assert_eq!(I4x32::pack(&[100i8; 32]).unpack(), [7i8; 32]);
        assert_eq!(I4x32::pack(&[-100i8; 32]).unpack(), [-8i8; 32]);
        assert_eq!(I4x32::pack(&[8i8; 32]).unpack(), [7i8; 32]); // just outside +
        assert_eq!(I4x32::pack(&[-9i8; 32]).unpack(), [-8i8; 32]); // just outside −
    }

    #[test]
    fn encoding_is_twos_complement_not_offset_binary() {
        // Absolute-bit assertions — the ONLY guard that catches an offset-binary codec
        // (pack∘unpack round-trips under either). Two's-complement: −8→0x8, −1→0xF,
        // +7→0x7, 0→0x0. Offset-binary would give −8→0x0, 0→0x8 — caught here.
        let mut v = [0i8; 32];
        v[0] = -8;
        assert_eq!(I4x32::pack(&v).bytes[0] & 0x0F, 0x8);
        v[0] = -1;
        assert_eq!(I4x32::pack(&v).bytes[0] & 0x0F, 0xF);
        v[0] = 7;
        assert_eq!(I4x32::pack(&v).bytes[0] & 0x0F, 0x7);
        v[0] = 0;
        assert_eq!(I4x32::pack(&v).bytes[0] & 0x0F, 0x0);
    }

    #[test]
    fn dim_order_even_low_odd_high() {
        let mut v = [0i8; 32];
        v[0] = 1; // even dim → low nibble
        v[1] = 2; // odd dim  → high nibble
        assert_eq!(I4x32::pack(&v).bytes[0], 0x21);
    }

    #[test]
    fn adjacent_dims_are_isolated() {
        // Setting dim 2k must not perturb dim 2k+1 (shared byte) — the bit-boundary
        // discipline (I-LEGACY-API-FEATURE-GATED).
        let mut v = [0i8; 32];
        v[4] = -8;
        let out = I4x32::pack(&v).unpack();
        for (d, &x) in out.iter().enumerate() {
            let want = if d == 4 { -8 } else { 0 };
            assert_eq!(x, want, "dim {d} must be {want}");
        }
    }

    #[test]
    fn zero_is_all_neutral() {
        assert_eq!(I4x32::ZERO.unpack(), [0i8; 32]);
        assert_eq!(I4x32::pack(&[0i8; 32]), I4x32::ZERO);
    }

    #[test]
    fn extremes_round_trip_all_dims() {
        assert_eq!(I4x32::pack(&[-8i8; 32]).unpack(), [-8i8; 32]);
        assert_eq!(I4x32::pack(&[7i8; 32]).unpack(), [7i8; 32]);
    }

    #[test]
    fn i4x64_layout_and_round_trip() {
        assert_eq!(core::mem::size_of::<I4x64>(), 32);
        assert_eq!(core::mem::align_of::<I4x64>(), 16);
        let mut v = [0i8; 64];
        for (d, slot) in v.iter_mut().enumerate() {
            *slot = ((d as i8) % 15) - 7;
        }
        assert_eq!(I4x64::pack(&v).unpack(), v);
        assert_eq!(I4x64::pack(&[100i8; 64]).unpack(), [7i8; 64]);
        assert_eq!(I4x64::ZERO.unpack(), [0i8; 64]);
    }
}
