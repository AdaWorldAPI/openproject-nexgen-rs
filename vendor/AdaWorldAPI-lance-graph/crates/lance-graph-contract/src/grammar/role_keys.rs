//! Role keys — canonical deterministic `[start:stop]`-slice VSA role bindings.
//!
//! Each role owns a **disjoint contiguous slice** of the 16,384-dim VSA
//! space (compatible with `CrystalFingerprint::Binary16K`). Only the bits
//! in that slice are set to a deterministic pseudo-random pattern seeded
//! from FNV-64 of the label; all other bits are zero. This is the
//! VSA-native `[start:stop]` addressing convention — **not** scattered bits.
//!
//! Because the slices are disjoint, XOR-binding a value with a role key
//! only affects that role's slice, and bundles of different role-bindings
//! don't contaminate each other.
//!
//! ## Space layout (16,384 total dims — LF-2 resize from 10,000)
//!
//! ```text
//! [    0 ..  2000)   SUBJECT_KEY
//! [ 2000 ..  4000)   PREDICATE_KEY
//! [ 4000 ..  6000)   OBJECT_KEY
//! [ 6000 ..  7500)   MODIFIER_KEY
//! [ 7500 ..  9000)   CONTEXT_KEY
//! [ 9000 ..  9200)   TEMPORAL_KEY
//! [ 9200 ..  9400)   KAUSAL_KEY
//! [ 9400 ..  9500)   MODAL_KEY
//! [ 9500 ..  9650)   LOKAL_KEY
//! [ 9650 ..  9750)   INSTRUMENT_KEY
//! [ 9750 ..  9780)   BENEFICIARY_KEY
//! [ 9780 ..  9810)   GOAL_KEY
//! [ 9810 ..  9840)   SOURCE_KEY
//! [ 9840 ..  9910)   Finnish 15 cases — ~4-5 dims each
//! [ 9910 ..  9970)   12 tense keys — 5 dims each
//! [ 9970 .. 10000)   7 NARS inference keys — ~4 dims each
//! [10000 .. 10512)   SMB KUNDE_KEY (customer)
//! [10512 .. 11024)   SMB SCHULDNER_KEY (debtor)
//! [11024 .. 11536)   SMB MAHNUNG_KEY (dunning)
//! [11536 .. 12048)   SMB RECHNUNG_KEY (invoice)
//! [12048 .. 12560)   SMB DOKUMENT_KEY (document)
//! [12560 .. 13072)   SMB BANK_KEY (banking)
//! [13072 .. 13584)   SMB FIBU_KEY (financial accounting)
//! [13584 .. 14096)   SMB STEUER_KEY (tax)
//! [14096 .. 16384)   headroom (reserved for future SMB keys)
//! ```

use std::sync::LazyLock;

use super::finnish::FinnishCase;
use super::inference::NarsInference;

/// VSA vector width in `u64` words. 256 × 64 = 16,384 bits.
/// Matches `CrystalFingerprint::Binary16K` and `ndarray::hpc::vsa::VSA_WORDS`.
pub const VSA_WORDS: usize = 256;

/// VSA vector width in dimensions (bits actually used).
/// Resized from 10,000 → 16,384 (LF-2) to accommodate SMB domain
/// role keys in [10000..14096) with 2,288 dims headroom.
pub const VSA_DIMS: usize = 16_384;

// NOTE: bind/unbind/recovery_margin methods removed (see CHANGELOG.md).
// Role keys are Layer-2 catalogue ONLY — identity slice boundaries.
// Algebra lives in Layer-1 `crystal/` and in ndarray.

/// A role key owns a contiguous slice of the VSA space.
/// Outside the slice, **all bits are zero**.
pub struct RoleKey {
    pub words: Box<[u64; VSA_WORDS]>,
    pub slice_start: usize,
    pub slice_end: usize,
    pub label: &'static str,
}

impl RoleKey {
    /// Dim range of this role's slice.
    pub fn slice_range(&self) -> std::ops::Range<usize> {
        self.slice_start..self.slice_end
    }

    /// Width of this role's slice in dimensions.
    pub fn slice_width(&self) -> usize {
        self.slice_end - self.slice_start
    }

    // NOTE: `bind/unbind/recovery_margin` methods removed in cleanup commit
    // `cd5c049...` (see CHANGELOG.md). Those operated on a hallucinated
    // `Vsa10k = [u64; 157]` bitpacked carrier with GF(2)/XOR algebra —
    // the wrong substrate for lossless role bundling. Correct algebra
    // is element-wise multiply/add on `Vsa10kF32`/`Vsa16kF32` via existing
    // `crystal::fingerprint::{vsa_bind, vsa_bundle, vsa_cosine}`.
    //
    // Role keys are a Layer-2 catalogue (slice boundaries for a domain);
    // algebra is Layer-1 on the switchboard carrier. See
    // `.claude/knowledge/vsa-switchboard-architecture.md`.

    /// Generate a deterministic role key: pseudo-random bits in `[start..end)`,
    /// zeros everywhere else. Seeded from FNV-64 of the label.
    ///
    /// `pub` so per-domain Layer-2 catalogues (sibling modules under this
    /// crate — `grammar`, `callcenter`, future `persona`) can construct
    /// their own role keys with disjoint slice allocations. Per
    /// `I-VSA-IDENTITIES`: identity in the role-key catalogue, content in
    /// downstream registries.
    pub fn generate(label: &'static str, start: usize, end: usize) -> Self {
        assert!(start <= end);
        assert!(end <= VSA_DIMS);
        let mut words = Box::new([0u64; VSA_WORDS]);
        let seed = fnv64(label);
        for dim in start..end {
            let mut state = seed.wrapping_add(dim as u64);
            let bit = lcg_next(&mut state) & 1;
            if bit == 1 {
                let word = dim / 64;
                let offset = dim % 64;
                words[word] |= 1u64 << offset;
            }
        }
        Self {
            words,
            slice_start: start,
            slice_end: end,
            label,
        }
    }
}

// NOTE: `vsa_xor`, `vsa_similarity`, `word_slice_mask`, and
// `slice_matching_bits` free functions removed in cleanup commit
// `cd5c049...` (see CHANGELOG.md). They operated on the hallucinated
// `Vsa10k = [u64; 157]` bitpacked carrier with GF(2)/XOR algebra.
// Correct VSA operations are in `crystal::fingerprint`:
//   `vsa_bind` (element-wise multiply on `[f32; 10_000]`)
//   `vsa_bundle` (element-wise add)
//   `vsa_cosine` (similarity)

fn fnv64(s: &str) -> u64 {
    crate::hash::fnv1a_str(s)
}

fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(0x5851f42d4c957f2d).wrapping_add(1);
    *state
}

// ---------------------------------------------------------------------------
// SPO core roles
// ---------------------------------------------------------------------------

pub static SUBJECT_KEY: LazyLock<RoleKey> = LazyLock::new(|| RoleKey::generate("SUBJECT", 0, 2000));
pub static PREDICATE_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("PREDICATE", 2000, 4000));
pub static OBJECT_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("OBJECT", 4000, 6000));
pub static MODIFIER_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("MODIFIER", 6000, 7500));
pub static CONTEXT_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("CONTEXT", 7500, 9000));

// ---------------------------------------------------------------------------
// TEKAMOLO slots
// ---------------------------------------------------------------------------

pub static TEMPORAL_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("TEMPORAL", 9000, 9200));
pub static KAUSAL_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("KAUSAL", 9200, 9400));
pub static MODAL_KEY: LazyLock<RoleKey> = LazyLock::new(|| RoleKey::generate("MODAL", 9400, 9500));
pub static LOKAL_KEY: LazyLock<RoleKey> = LazyLock::new(|| RoleKey::generate("LOKAL", 9500, 9650));

// ---------------------------------------------------------------------------
// Future-ready roles (CausalityFlow not extended yet)
// ---------------------------------------------------------------------------

pub static INSTRUMENT_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("INSTRUMENT", 9650, 9750));
pub static BENEFICIARY_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("BENEFICIARY", 9750, 9780));
pub static GOAL_KEY: LazyLock<RoleKey> = LazyLock::new(|| RoleKey::generate("GOAL", 9780, 9810));
pub static SOURCE_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("SOURCE", 9810, 9840));

// ---------------------------------------------------------------------------
// Finnish 15 cases — [9840 .. 9910), 70 dims / 15 cases ≈ 4-5 dims each.
// First 10 cases get 5 dims; remaining 5 cases get 4 dims. Total = 50 + 20 = 70.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
const FINNISH_START: usize = 9840;
#[allow(dead_code)]
const FINNISH_END: usize = 9910;

/// Inclusive-exclusive slice boundaries for each of the 15 Finnish cases,
/// indexed by `FinnishCase as u8`. Widths: first 10 cases = 5 dims, last 5 = 4 dims.
const FINNISH_SLICES: [(usize, usize); 15] = [
    (9840, 9845), // Nominative
    (9845, 9850), // Genitive
    (9850, 9855), // Accusative
    (9855, 9860), // Partitive
    (9860, 9865), // Inessive
    (9865, 9870), // Elative
    (9870, 9875), // Illative
    (9875, 9880), // Adessive
    (9880, 9885), // Ablative
    (9885, 9890), // Allative
    (9890, 9894), // Essive
    (9894, 9898), // Translative
    (9898, 9902), // Instructive
    (9902, 9906), // Abessive
    (9906, 9910), // Comitative
];

const FINNISH_LABELS: [&str; 15] = [
    "FI_NOMINATIVE",
    "FI_GENITIVE",
    "FI_ACCUSATIVE",
    "FI_PARTITIVE",
    "FI_INESSIVE",
    "FI_ELATIVE",
    "FI_ILLATIVE",
    "FI_ADESSIVE",
    "FI_ABLATIVE",
    "FI_ALLATIVE",
    "FI_ESSIVE",
    "FI_TRANSLATIVE",
    "FI_INSTRUCTIVE",
    "FI_ABESSIVE",
    "FI_COMITATIVE",
];

static FINNISH_KEYS: LazyLock<[RoleKey; 15]> = LazyLock::new(|| {
    core::array::from_fn(|i| {
        let (s, e) = FINNISH_SLICES[i];
        RoleKey::generate(FINNISH_LABELS[i], s, e)
    })
});

pub fn finnish_case_key(case: FinnishCase) -> &'static RoleKey {
    &FINNISH_KEYS[case as usize]
}

// ---------------------------------------------------------------------------
// 12 tense keys — [9910 .. 9970), 60 dims / 12 = 5 dims each.
// ---------------------------------------------------------------------------

/// Tense key, 12 variants, each owning 5 dims of the VSA space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Tense {
    Present = 0,
    Past = 1,
    Future = 2,
    PresentContinuous = 3,
    PastContinuous = 4,
    FutureContinuous = 5,
    Perfect = 6,
    Pluperfect = 7,
    FuturePerfect = 8,
    Habitual = 9,
    Potential = 10,
    Imperative = 11,
}

impl Tense {
    pub const ALL: [Self; 12] = [
        Self::Present,
        Self::Past,
        Self::Future,
        Self::PresentContinuous,
        Self::PastContinuous,
        Self::FutureContinuous,
        Self::Perfect,
        Self::Pluperfect,
        Self::FuturePerfect,
        Self::Habitual,
        Self::Potential,
        Self::Imperative,
    ];
}

const TENSE_START: usize = 9910;
#[allow(dead_code)]
const TENSE_END: usize = 9970;
const TENSE_WIDTH: usize = 5;

const TENSE_LABELS: [&str; 12] = [
    "T_PRESENT",
    "T_PAST",
    "T_FUTURE",
    "T_PRESENT_CONTINUOUS",
    "T_PAST_CONTINUOUS",
    "T_FUTURE_CONTINUOUS",
    "T_PERFECT",
    "T_PLUPERFECT",
    "T_FUTURE_PERFECT",
    "T_HABITUAL",
    "T_POTENTIAL",
    "T_IMPERATIVE",
];

static TENSE_KEYS: LazyLock<[RoleKey; 12]> = LazyLock::new(|| {
    core::array::from_fn(|i| {
        let s = TENSE_START + i * TENSE_WIDTH;
        let e = s + TENSE_WIDTH;
        RoleKey::generate(TENSE_LABELS[i], s, e)
    })
});

pub fn tense_key(tense: Tense) -> &'static RoleKey {
    &TENSE_KEYS[tense as usize]
}

// ---------------------------------------------------------------------------
// 7 NARS inference keys — [9970 .. 10000), 30 dims / 7 ≈ 4 dims each.
// First 2 get 5 dims, remaining 5 get 4 dims. Total = 10 + 20 = 30.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
const NARS_START: usize = 9970;
#[allow(dead_code)]
const NARS_END: usize = 10_000;

const NARS_SLICES: [(usize, usize); 7] = [
    (9970, 9975),   // Deduction
    (9975, 9980),   // Induction
    (9980, 9984),   // Abduction
    (9984, 9988),   // Revision
    (9988, 9992),   // Synthesis
    (9992, 9996),   // Extrapolation
    (9996, 10_000), // CounterfactualSynthesis
];

const NARS_LABELS: [&str; 7] = [
    "N_DEDUCTION",
    "N_INDUCTION",
    "N_ABDUCTION",
    "N_REVISION",
    "N_SYNTHESIS",
    "N_EXTRAPOLATION",
    "N_COUNTERFACTUAL",
];

static NARS_KEYS: LazyLock<[RoleKey; 7]> = LazyLock::new(|| {
    core::array::from_fn(|i| {
        let (s, e) = NARS_SLICES[i];
        RoleKey::generate(NARS_LABELS[i], s, e)
    })
});

pub fn nars_inference_key(inf: NarsInference) -> &'static RoleKey {
    let idx = match inf {
        NarsInference::Deduction => 0,
        NarsInference::Induction => 1,
        NarsInference::Abduction => 2,
        NarsInference::Revision => 3,
        NarsInference::Synthesis => 4,
        NarsInference::Extrapolation => 5,
        NarsInference::CounterfactualSynthesis => 6,
    };
    &NARS_KEYS[idx]
}

// ---------------------------------------------------------------------------
// SMB domain role keys — [10000 .. 14096), 512 dims each (LF-2)
// ---------------------------------------------------------------------------

pub static KUNDE_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("smb.kunde", 10_000, 10_512));
pub static SCHULDNER_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("smb.schuldner", 10_512, 11_024));
pub static MAHNUNG_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("smb.mahnung", 11_024, 11_536));
pub static RECHNUNG_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("smb.rechnung", 11_536, 12_048));
pub static DOKUMENT_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("smb.dokument", 12_048, 12_560));
pub static BANK_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("smb.bank", 12_560, 13_072));
pub static FIBU_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("smb.fibu", 13_072, 13_584));
pub static STEUER_KEY: LazyLock<RoleKey> =
    LazyLock::new(|| RoleKey::generate("smb.steuer", 13_584, 14_096));

// ---------------------------------------------------------------------------
// D6 — RoleKeySlice catalogue (const-addressable [start:stop) slices + FNV-64
// fingerprint over the role label). This layer is the **catalogue index** for
// the live `RoleKey` static instances above: same boundaries, no duplication
// of the bipolar payload — just `Copy`/`const`-friendly descriptors that can
// be embedded in tables, dispatch maps, or codecs without taking a LazyLock.
//
// `RoleKeySlice::fnv_seed` is the FNV-64 of the canonical label string and
// can be used as a stable per-role identifier (e.g. unbinding lookup, codec
// keying). All slices are sub-ranges of the existing 16,384-dim VSA space.
// ---------------------------------------------------------------------------

/// A role key descriptor: a contiguous `[start:stop)` slice of the VSA space
/// plus a deterministic FNV-64 fingerprint over the role's canonical label
/// (used for unbinding / similarity / codec keying).
///
/// This is the `Copy`/`const`-friendly companion to [`RoleKey`]; both share
/// the same slice boundaries by construction (see `role_key_slice_*` tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoleKeySlice {
    pub start: usize,
    pub stop: usize,
    pub fnv_seed: u64,
}

impl RoleKeySlice {
    /// Construct a const slice. `start <= stop <= VSA_DIMS` is the caller's
    /// invariant (debug-checked at first use, not in this `const fn` body).
    pub const fn new(start: usize, stop: usize, fnv_seed: u64) -> Self {
        Self {
            start,
            stop,
            fnv_seed,
        }
    }
    pub const fn len(&self) -> usize {
        self.stop - self.start
    }
    pub const fn is_empty(&self) -> bool {
        self.start == self.stop
    }
    pub const fn range(&self) -> std::ops::Range<usize> {
        self.start..self.stop
    }
}

/// Delegate to the canonical `hash::fnv1a` (const fn, zero-dep).
pub const fn fnv64_bytes(bytes: &[u8]) -> u64 {
    crate::hash::fnv1a(bytes)
}

// --- SPO core role slices (mirror of SUBJECT_KEY..CONTEXT_KEY) --------------

pub const SUBJECT_SLICE: RoleKeySlice = RoleKeySlice::new(0, 2000, fnv64_bytes(b"SUBJECT"));
pub const PREDICATE_SLICE: RoleKeySlice = RoleKeySlice::new(2000, 4000, fnv64_bytes(b"PREDICATE"));
pub const OBJECT_SLICE: RoleKeySlice = RoleKeySlice::new(4000, 6000, fnv64_bytes(b"OBJECT"));
pub const MODIFIER_SLICE: RoleKeySlice = RoleKeySlice::new(6000, 7500, fnv64_bytes(b"MODIFIER"));
pub const CONTEXT_SLICE: RoleKeySlice = RoleKeySlice::new(7500, 9000, fnv64_bytes(b"CONTEXT"));

// --- TEKAMOLO sub-slices (mirror of TEMPORAL_KEY..LOKAL_KEY + extras) ------

pub const TEMPORAL_SLICE: RoleKeySlice = RoleKeySlice::new(9000, 9200, fnv64_bytes(b"TEMPORAL"));
pub const KAUSAL_SLICE: RoleKeySlice = RoleKeySlice::new(9200, 9400, fnv64_bytes(b"KAUSAL"));
pub const MODAL_SLICE: RoleKeySlice = RoleKeySlice::new(9400, 9500, fnv64_bytes(b"MODAL"));
pub const LOKAL_SLICE: RoleKeySlice = RoleKeySlice::new(9500, 9650, fnv64_bytes(b"LOKAL"));
pub const INSTRUMENT_SLICE: RoleKeySlice =
    RoleKeySlice::new(9650, 9750, fnv64_bytes(b"INSTRUMENT"));
pub const BENEFICIARY_SLICE: RoleKeySlice =
    RoleKeySlice::new(9750, 9780, fnv64_bytes(b"BENEFICIARY"));
pub const GOAL_SLICE: RoleKeySlice = RoleKeySlice::new(9780, 9810, fnv64_bytes(b"GOAL"));
pub const SOURCE_SLICE: RoleKeySlice = RoleKeySlice::new(9810, 9840, fnv64_bytes(b"SOURCE"));

// --- Finnish 15 cases (mirror FINNISH_SLICES, indexed by FinnishCase as u8)

pub static FINNISH_CASE_SLICES: LazyLock<[(FinnishCase, RoleKeySlice); 15]> = LazyLock::new(|| {
    [
        (
            FinnishCase::Nominative,
            RoleKeySlice::new(
                FINNISH_SLICES[0].0,
                FINNISH_SLICES[0].1,
                fnv64_bytes(b"FI_NOMINATIVE"),
            ),
        ),
        (
            FinnishCase::Genitive,
            RoleKeySlice::new(
                FINNISH_SLICES[1].0,
                FINNISH_SLICES[1].1,
                fnv64_bytes(b"FI_GENITIVE"),
            ),
        ),
        (
            FinnishCase::Accusative,
            RoleKeySlice::new(
                FINNISH_SLICES[2].0,
                FINNISH_SLICES[2].1,
                fnv64_bytes(b"FI_ACCUSATIVE"),
            ),
        ),
        (
            FinnishCase::Partitive,
            RoleKeySlice::new(
                FINNISH_SLICES[3].0,
                FINNISH_SLICES[3].1,
                fnv64_bytes(b"FI_PARTITIVE"),
            ),
        ),
        (
            FinnishCase::Inessive,
            RoleKeySlice::new(
                FINNISH_SLICES[4].0,
                FINNISH_SLICES[4].1,
                fnv64_bytes(b"FI_INESSIVE"),
            ),
        ),
        (
            FinnishCase::Elative,
            RoleKeySlice::new(
                FINNISH_SLICES[5].0,
                FINNISH_SLICES[5].1,
                fnv64_bytes(b"FI_ELATIVE"),
            ),
        ),
        (
            FinnishCase::Illative,
            RoleKeySlice::new(
                FINNISH_SLICES[6].0,
                FINNISH_SLICES[6].1,
                fnv64_bytes(b"FI_ILLATIVE"),
            ),
        ),
        (
            FinnishCase::Adessive,
            RoleKeySlice::new(
                FINNISH_SLICES[7].0,
                FINNISH_SLICES[7].1,
                fnv64_bytes(b"FI_ADESSIVE"),
            ),
        ),
        (
            FinnishCase::Ablative,
            RoleKeySlice::new(
                FINNISH_SLICES[8].0,
                FINNISH_SLICES[8].1,
                fnv64_bytes(b"FI_ABLATIVE"),
            ),
        ),
        (
            FinnishCase::Allative,
            RoleKeySlice::new(
                FINNISH_SLICES[9].0,
                FINNISH_SLICES[9].1,
                fnv64_bytes(b"FI_ALLATIVE"),
            ),
        ),
        (
            FinnishCase::Essive,
            RoleKeySlice::new(
                FINNISH_SLICES[10].0,
                FINNISH_SLICES[10].1,
                fnv64_bytes(b"FI_ESSIVE"),
            ),
        ),
        (
            FinnishCase::Translative,
            RoleKeySlice::new(
                FINNISH_SLICES[11].0,
                FINNISH_SLICES[11].1,
                fnv64_bytes(b"FI_TRANSLATIVE"),
            ),
        ),
        (
            FinnishCase::Instructive,
            RoleKeySlice::new(
                FINNISH_SLICES[12].0,
                FINNISH_SLICES[12].1,
                fnv64_bytes(b"FI_INSTRUCTIVE"),
            ),
        ),
        (
            FinnishCase::Abessive,
            RoleKeySlice::new(
                FINNISH_SLICES[13].0,
                FINNISH_SLICES[13].1,
                fnv64_bytes(b"FI_ABESSIVE"),
            ),
        ),
        (
            FinnishCase::Comitative,
            RoleKeySlice::new(
                FINNISH_SLICES[14].0,
                FINNISH_SLICES[14].1,
                fnv64_bytes(b"FI_COMITATIVE"),
            ),
        ),
    ]
});

/// Lookup the [`RoleKeySlice`] for a Finnish case (round-trip via the
/// `LazyLock` array — exactly one slice per variant by construction).
pub fn finnish_case_slice(case: FinnishCase) -> RoleKeySlice {
    FINNISH_CASE_SLICES[case as usize].1
}

// --- 12 Tense slices (mirror TENSE_KEYS) -----------------------------------

pub static TENSE_SLICES: LazyLock<[(Tense, RoleKeySlice); 12]> = LazyLock::new(|| {
    let s = |i: usize| TENSE_START + i * TENSE_WIDTH;
    let e = |i: usize| TENSE_START + (i + 1) * TENSE_WIDTH;
    [
        (
            Tense::Present,
            RoleKeySlice::new(s(0), e(0), fnv64_bytes(b"T_PRESENT")),
        ),
        (
            Tense::Past,
            RoleKeySlice::new(s(1), e(1), fnv64_bytes(b"T_PAST")),
        ),
        (
            Tense::Future,
            RoleKeySlice::new(s(2), e(2), fnv64_bytes(b"T_FUTURE")),
        ),
        (
            Tense::PresentContinuous,
            RoleKeySlice::new(s(3), e(3), fnv64_bytes(b"T_PRESENT_CONTINUOUS")),
        ),
        (
            Tense::PastContinuous,
            RoleKeySlice::new(s(4), e(4), fnv64_bytes(b"T_PAST_CONTINUOUS")),
        ),
        (
            Tense::FutureContinuous,
            RoleKeySlice::new(s(5), e(5), fnv64_bytes(b"T_FUTURE_CONTINUOUS")),
        ),
        (
            Tense::Perfect,
            RoleKeySlice::new(s(6), e(6), fnv64_bytes(b"T_PERFECT")),
        ),
        (
            Tense::Pluperfect,
            RoleKeySlice::new(s(7), e(7), fnv64_bytes(b"T_PLUPERFECT")),
        ),
        (
            Tense::FuturePerfect,
            RoleKeySlice::new(s(8), e(8), fnv64_bytes(b"T_FUTURE_PERFECT")),
        ),
        (
            Tense::Habitual,
            RoleKeySlice::new(s(9), e(9), fnv64_bytes(b"T_HABITUAL")),
        ),
        (
            Tense::Potential,
            RoleKeySlice::new(s(10), e(10), fnv64_bytes(b"T_POTENTIAL")),
        ),
        (
            Tense::Imperative,
            RoleKeySlice::new(s(11), e(11), fnv64_bytes(b"T_IMPERATIVE")),
        ),
    ]
});

pub fn tense_slice(tense: Tense) -> RoleKeySlice {
    TENSE_SLICES[tense as usize].1
}

// --- 7 NARS-inference slices (mirror NARS_SLICES) --------------------------

pub static NARS_INFERENCE_SLICES: LazyLock<[(NarsInference, RoleKeySlice); 7]> =
    LazyLock::new(|| {
        [
            (
                NarsInference::Deduction,
                RoleKeySlice::new(
                    NARS_SLICES[0].0,
                    NARS_SLICES[0].1,
                    fnv64_bytes(b"N_DEDUCTION"),
                ),
            ),
            (
                NarsInference::Induction,
                RoleKeySlice::new(
                    NARS_SLICES[1].0,
                    NARS_SLICES[1].1,
                    fnv64_bytes(b"N_INDUCTION"),
                ),
            ),
            (
                NarsInference::Abduction,
                RoleKeySlice::new(
                    NARS_SLICES[2].0,
                    NARS_SLICES[2].1,
                    fnv64_bytes(b"N_ABDUCTION"),
                ),
            ),
            (
                NarsInference::Revision,
                RoleKeySlice::new(
                    NARS_SLICES[3].0,
                    NARS_SLICES[3].1,
                    fnv64_bytes(b"N_REVISION"),
                ),
            ),
            (
                NarsInference::Synthesis,
                RoleKeySlice::new(
                    NARS_SLICES[4].0,
                    NARS_SLICES[4].1,
                    fnv64_bytes(b"N_SYNTHESIS"),
                ),
            ),
            (
                NarsInference::Extrapolation,
                RoleKeySlice::new(
                    NARS_SLICES[5].0,
                    NARS_SLICES[5].1,
                    fnv64_bytes(b"N_EXTRAPOLATION"),
                ),
            ),
            (
                NarsInference::CounterfactualSynthesis,
                RoleKeySlice::new(
                    NARS_SLICES[6].0,
                    NARS_SLICES[6].1,
                    fnv64_bytes(b"N_COUNTERFACTUAL"),
                ),
            ),
        ]
    });

pub fn nars_inference_slice(inf: NarsInference) -> RoleKeySlice {
    let idx = match inf {
        NarsInference::Deduction => 0,
        NarsInference::Induction => 1,
        NarsInference::Abduction => 2,
        NarsInference::Revision => 3,
        NarsInference::Synthesis => 4,
        NarsInference::Extrapolation => 5,
        NarsInference::CounterfactualSynthesis => 6,
    };
    NARS_INFERENCE_SLICES[idx].1
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Collect every (start, end, label) from every defined role key.
    fn all_slices() -> Vec<(usize, usize, &'static str)> {
        let mut v: Vec<(usize, usize, &'static str)> = Vec::new();
        for k in [
            &*SUBJECT_KEY,
            &*PREDICATE_KEY,
            &*OBJECT_KEY,
            &*MODIFIER_KEY,
            &*CONTEXT_KEY,
            &*TEMPORAL_KEY,
            &*KAUSAL_KEY,
            &*MODAL_KEY,
            &*LOKAL_KEY,
            &*INSTRUMENT_KEY,
            &*BENEFICIARY_KEY,
            &*GOAL_KEY,
            &*SOURCE_KEY,
        ] {
            v.push((k.slice_start, k.slice_end, k.label));
        }
        for k in FINNISH_KEYS.iter() {
            v.push((k.slice_start, k.slice_end, k.label));
        }
        for k in TENSE_KEYS.iter() {
            v.push((k.slice_start, k.slice_end, k.label));
        }
        for k in NARS_KEYS.iter() {
            v.push((k.slice_start, k.slice_end, k.label));
        }
        for k in [
            &*KUNDE_KEY,
            &*SCHULDNER_KEY,
            &*MAHNUNG_KEY,
            &*RECHNUNG_KEY,
            &*DOKUMENT_KEY,
            &*BANK_KEY,
            &*FIBU_KEY,
            &*STEUER_KEY,
        ] {
            v.push((k.slice_start, k.slice_end, k.label));
        }
        v
    }

    #[test]
    fn all_slices_disjoint() {
        let mut slices = all_slices();
        slices.sort_by_key(|(s, _, _)| *s);
        for pair in slices.windows(2) {
            let (s0, e0, l0) = pair[0];
            let (s1, _e1, l1) = pair[1];
            assert!(
                e0 <= s1,
                "slice overlap: {l0} [{s0}..{e0}) vs {l1} [{s1}..)"
            );
        }
    }

    #[test]
    fn all_slices_within_vsa_dims() {
        for (s, e, label) in all_slices() {
            assert!(s < e, "empty slice for {label}");
            assert!(e <= VSA_DIMS, "slice {label} ends at {e} > {VSA_DIMS}");
        }
    }

    #[test]
    fn role_key_bits_only_in_slice() {
        // SUBJECT_KEY owns [0..2000). Every bit >= dim 2000 must be zero.
        let k = &*SUBJECT_KEY;
        for dim in 0..(VSA_WORDS * 64) {
            let word = dim / 64;
            let offset = dim % 64;
            let bit = (k.words[word] >> offset) & 1;
            if dim < k.slice_start || dim >= k.slice_end {
                assert_eq!(bit, 0, "SUBJECT_KEY bit set outside slice at dim {dim}");
            }
        }

        // Spot-check KAUSAL_KEY [9200..9400).
        let k = &*KAUSAL_KEY;
        for dim in 0..(VSA_WORDS * 64) {
            let word = dim / 64;
            let offset = dim % 64;
            let bit = (k.words[word] >> offset) & 1;
            if dim < k.slice_start || dim >= k.slice_end {
                assert_eq!(bit, 0, "KAUSAL_KEY bit set outside slice at dim {dim}");
            }
        }
    }

    #[test]
    fn role_keys_deterministic() {
        // Re-generate from scratch; compare to the static instance.
        let a = RoleKey::generate("SUBJECT", 0, 2000);
        let b = RoleKey::generate("SUBJECT", 0, 2000);
        assert_eq!(a.words.as_ref(), b.words.as_ref());
        assert_eq!(a.words.as_ref(), SUBJECT_KEY.words.as_ref());
    }

    #[test]
    fn finnish_case_lookup_covers_all_15() {
        let all = [
            FinnishCase::Nominative,
            FinnishCase::Genitive,
            FinnishCase::Accusative,
            FinnishCase::Partitive,
            FinnishCase::Inessive,
            FinnishCase::Elative,
            FinnishCase::Illative,
            FinnishCase::Adessive,
            FinnishCase::Ablative,
            FinnishCase::Allative,
            FinnishCase::Essive,
            FinnishCase::Translative,
            FinnishCase::Instructive,
            FinnishCase::Abessive,
            FinnishCase::Comitative,
        ];
        for case in all {
            let k = finnish_case_key(case);
            assert!(k.slice_start >= FINNISH_START);
            assert!(k.slice_end <= FINNISH_END);
            assert!(k.slice_width() >= 4);
        }
    }

    #[test]
    fn nars_inference_lookup_covers_all_7() {
        let all = [
            NarsInference::Deduction,
            NarsInference::Induction,
            NarsInference::Abduction,
            NarsInference::Revision,
            NarsInference::Synthesis,
            NarsInference::Extrapolation,
            NarsInference::CounterfactualSynthesis,
        ];
        for inf in all {
            let k = nars_inference_key(inf);
            assert!(k.slice_start >= NARS_START);
            assert!(k.slice_end <= NARS_END);
            assert!(k.slice_width() >= 4);
        }
    }

    // Tests for the RoleKey-as-operator family (bind/unbind/recovery_margin,
    // vsa_xor, vsa_similarity) were REMOVED in cleanup commit `cd5c049...`
    // along with the methods they covered. See CHANGELOG.md § VSA format
    // switches. Correct algebra tests live on the carrier in
    // `crystal::fingerprint` (existing: vsa_bind/bundle/superpose/cosine).

    #[test]
    fn tense_lookup_covers_all_12() {
        let all = [
            Tense::Present,
            Tense::Past,
            Tense::Future,
            Tense::PresentContinuous,
            Tense::PastContinuous,
            Tense::FutureContinuous,
            Tense::Perfect,
            Tense::Pluperfect,
            Tense::FuturePerfect,
            Tense::Habitual,
            Tense::Potential,
            Tense::Imperative,
        ];
        for t in all {
            let k = tense_key(t);
            assert_eq!(k.slice_width(), TENSE_WIDTH);
            assert!(k.slice_start >= TENSE_START);
            assert!(k.slice_end <= TENSE_END);
        }
    }

    // -----------------------------------------------------------------------
    // D6 — RoleKeySlice catalogue tests
    // -----------------------------------------------------------------------

    /// All five SPO core slices are non-overlapping and union to [0, 9000)
    /// (the "SPO-spine" prefix of the 16,384-dim VSA carrier).
    #[test]
    fn spo_slices_disjoint_and_contiguous() {
        let spo = [
            SUBJECT_SLICE,
            PREDICATE_SLICE,
            OBJECT_SLICE,
            MODIFIER_SLICE,
            CONTEXT_SLICE,
        ];
        // Contiguous: each slice starts where the previous ended.
        for pair in spo.windows(2) {
            assert_eq!(
                pair[0].stop, pair[1].start,
                "SPO slices not contiguous: {:?} vs {:?}",
                pair[0], pair[1]
            );
        }
        // Union covers [0, 9000) — the SPO+TEKAMOLO-prefix region. (CONTEXT
        // ends at 9000; TEKAMOLO sub-slices begin there.)
        assert_eq!(spo[0].start, 0);
        assert_eq!(spo[spo.len() - 1].stop, 9000);
    }

    /// TEKAMOLO sub-slices fit within [9000, 9840) — the slice region beyond
    /// CONTEXT_KEY where the original prompt placed them. (CONTEXT_KEY itself
    /// owns [7500, 9000) and TEKAMOLO sits AFTER it in the LF-2 layout.)
    #[test]
    fn tekamolo_sub_slices_in_post_context_band() {
        let teka = [
            TEMPORAL_SLICE,
            KAUSAL_SLICE,
            MODAL_SLICE,
            LOKAL_SLICE,
            INSTRUMENT_SLICE,
            BENEFICIARY_SLICE,
            GOAL_SLICE,
            SOURCE_SLICE,
        ];
        for s in teka {
            assert!(s.start >= 9000, "TEKAMOLO slice starts before 9000: {s:?}");
            assert!(s.stop <= 9840, "TEKAMOLO slice ends after 9840: {s:?}");
            assert!(!s.is_empty(), "empty TEKAMOLO slice: {s:?}");
        }
    }

    /// Finnish case slices are non-overlapping AND fall inside the existing
    /// `FINNISH_START..FINNISH_END` band.
    #[test]
    fn finnish_case_slices_disjoint_in_band() {
        let arr = &*FINNISH_CASE_SLICES;
        let mut by_start: Vec<RoleKeySlice> = arr.iter().map(|(_, s)| *s).collect();
        by_start.sort_by_key(|s| s.start);
        for pair in by_start.windows(2) {
            assert!(
                pair[0].stop <= pair[1].start,
                "Finnish slice overlap: {:?} vs {:?}",
                pair[0],
                pair[1]
            );
        }
        for (_, s) in arr.iter() {
            assert!(s.start >= FINNISH_START);
            assert!(s.stop <= FINNISH_END);
        }
    }

    /// FNV-64 of distinct labels does not collide on the canonical role names.
    #[test]
    fn fnv64_no_collisions_on_role_labels() {
        let labels: &[&[u8]] = &[
            b"SUBJECT",
            b"PREDICATE",
            b"OBJECT",
            b"MODIFIER",
            b"CONTEXT",
            b"TEMPORAL",
            b"KAUSAL",
            b"MODAL",
            b"LOKAL",
            b"INSTRUMENT",
            b"BENEFICIARY",
            b"GOAL",
            b"SOURCE",
            b"FI_NOMINATIVE",
            b"FI_GENITIVE",
            b"FI_ACCUSATIVE",
            b"FI_PARTITIVE",
            b"FI_INESSIVE",
            b"FI_ELATIVE",
            b"FI_ILLATIVE",
            b"FI_ADESSIVE",
            b"FI_ABLATIVE",
            b"FI_ALLATIVE",
            b"FI_ESSIVE",
            b"FI_TRANSLATIVE",
            b"FI_INSTRUCTIVE",
            b"FI_ABESSIVE",
            b"FI_COMITATIVE",
            b"T_PRESENT",
            b"T_PAST",
            b"T_FUTURE",
            b"T_PRESENT_CONTINUOUS",
            b"T_PAST_CONTINUOUS",
            b"T_FUTURE_CONTINUOUS",
            b"T_PERFECT",
            b"T_PLUPERFECT",
            b"T_FUTURE_PERFECT",
            b"T_HABITUAL",
            b"T_POTENTIAL",
            b"T_IMPERATIVE",
            b"N_DEDUCTION",
            b"N_INDUCTION",
            b"N_ABDUCTION",
            b"N_REVISION",
            b"N_SYNTHESIS",
            b"N_EXTRAPOLATION",
            b"N_COUNTERFACTUAL",
        ];
        let mut seen = std::collections::HashSet::new();
        for l in labels {
            let h = fnv64_bytes(l);
            assert!(
                seen.insert(h),
                "FNV-64 collision on label {:?}",
                std::str::from_utf8(l).unwrap()
            );
        }
        // Spot-check the prompt's pinned non-collision.
        assert_ne!(fnv64_bytes(b"SUBJECT"), fnv64_bytes(b"OBJECT"));
    }

    /// Round-trip: each FinnishCase variant maps to exactly one
    /// `RoleKeySlice` via the LazyLock array, and the array is keyed by
    /// `FinnishCase as u8` (i.e. `arr[c as usize].0 == c`).
    #[test]
    fn finnish_case_round_trip() {
        let all = [
            FinnishCase::Nominative,
            FinnishCase::Genitive,
            FinnishCase::Accusative,
            FinnishCase::Partitive,
            FinnishCase::Inessive,
            FinnishCase::Elative,
            FinnishCase::Illative,
            FinnishCase::Adessive,
            FinnishCase::Ablative,
            FinnishCase::Allative,
            FinnishCase::Essive,
            FinnishCase::Translative,
            FinnishCase::Instructive,
            FinnishCase::Abessive,
            FinnishCase::Comitative,
        ];
        for case in all {
            let (stored_case, slice) = FINNISH_CASE_SLICES[case as usize];
            assert_eq!(
                stored_case, case,
                "FINNISH_CASE_SLICES not indexed by `as u8`"
            );
            // The free-function lookup must agree with the array entry.
            assert_eq!(finnish_case_slice(case), slice);
            // Slice mirrors the live RoleKey boundaries.
            let live = finnish_case_key(case);
            assert_eq!(slice.start, live.slice_start);
            assert_eq!(slice.stop, live.slice_end);
            // And the FNV-64 fingerprint is non-zero (every label hashes to
            // something distinct from the empty string's seed).
            assert_ne!(slice.fnv_seed, 0xcbf29ce484222325);
        }
    }

    /// The slice catalogue mirrors the live `RoleKey` boundaries for SPO/
    /// TEKAMOLO so consumers can swap the two without re-deriving widths.
    #[test]
    fn role_key_slice_mirrors_live_role_key_boundaries() {
        let pairs: &[(RoleKeySlice, &RoleKey)] = &[
            (SUBJECT_SLICE, &SUBJECT_KEY),
            (PREDICATE_SLICE, &PREDICATE_KEY),
            (OBJECT_SLICE, &OBJECT_KEY),
            (MODIFIER_SLICE, &MODIFIER_KEY),
            (CONTEXT_SLICE, &CONTEXT_KEY),
            (TEMPORAL_SLICE, &TEMPORAL_KEY),
            (KAUSAL_SLICE, &KAUSAL_KEY),
            (MODAL_SLICE, &MODAL_KEY),
            (LOKAL_SLICE, &LOKAL_KEY),
            (INSTRUMENT_SLICE, &INSTRUMENT_KEY),
            (BENEFICIARY_SLICE, &BENEFICIARY_KEY),
            (GOAL_SLICE, &GOAL_KEY),
            (SOURCE_SLICE, &SOURCE_KEY),
        ];
        for (slice, live) in pairs {
            assert_eq!(
                slice.start, live.slice_start,
                "slice/live start mismatch for {}",
                live.label
            );
            assert_eq!(
                slice.stop, live.slice_end,
                "slice/live stop  mismatch for {}",
                live.label
            );
            assert!(slice.stop <= VSA_DIMS);
        }
    }

    #[test]
    fn role_key_slice_const_helpers() {
        assert_eq!(SUBJECT_SLICE.len(), 2000);
        assert!(!SUBJECT_SLICE.is_empty());
        let r = SUBJECT_SLICE.range();
        assert_eq!(r.start, 0);
        assert_eq!(r.end, 2000);
    }

    #[test]
    fn nars_inference_slice_round_trip() {
        let all = [
            NarsInference::Deduction,
            NarsInference::Induction,
            NarsInference::Abduction,
            NarsInference::Revision,
            NarsInference::Synthesis,
            NarsInference::Extrapolation,
            NarsInference::CounterfactualSynthesis,
        ];
        for inf in all {
            let s = nars_inference_slice(inf);
            assert!(s.start >= NARS_START);
            assert!(s.stop <= NARS_END);
            assert!(!s.is_empty());
        }
    }
}
