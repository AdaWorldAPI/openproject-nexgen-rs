//! Crystal — polymorphic semantic crystallizations (shared interface).
//!
//! A Crystal is a structured semantic object that accumulates truth (NARS
//! revision), hardens over time, and supports bundle/unbundle operations.
//!
//! ## Relationship to the cognitive shader pipeline
//!
//! The existing [`crate::cognitive_shader`] module carries the
//! **execution-time** cycle DTOs:
//!
//! ```text
//! Φ ShaderDispatch   — request
//! Ψ ShaderResonance  — ripple field + top-k hits
//! B ShaderBus        — committed cycle (cycle_fingerprint: [u64; 256])
//! Γ ShaderCrystal    — stabilized outcome (bus + persisted_row + meta)
//! ```
//!
//! `ShaderBus::cycle_fingerprint` is `[u64; 256]` = 2 KB = same backing as
//! [`crate::container::Container`] and [`CrystalFingerprint::Binary16K`].
//! A [`CycleCrystal`] is the **persistence-time** view of a
//! [`crate::cognitive_shader::ShaderCrystal`]:
//!
//! ```text
//! ShaderCrystal            CycleCrystal
//!   bus                      fingerprint: CrystalFingerprint (== cycle_fingerprint)
//!     cycle_fingerprint       cycle_index
//!     emitted_edges           anchor (StateAnchor)
//!     gate, resonance         truth, hardness, revision_count
//!   persisted_row            crystallized_at
//!   meta (Brier, …)
//! ```
//!
//! Zero-copy discipline: `CrystalFingerprint::Binary16K(Box<[u64; 256]>)`
//! is the heap-owning form; borrowed views go through
//! [`crate::cognitive_shader::ColumnWindow`] for struct-of-arrays
//! access into BindSpace.
//!
//! ## Relationship to existing crystal/quantum crates
//!
//! Implementations live in the siblings: `ladybug-rs`,
//! `ada-consciousness`, and `bighorn` already ship crystal/quantum
//! crates. This module is the **contract surface** — the trait and
//! layout they all implement against. No logic lives here; only shared
//! types, sandwich-layout constants, and the [`Crystal`] trait.
//!
//! Downstream VSA algebra (bind / bundle / permute / similarity) is
//! the canonical `ndarray::hpc::vsa` module on binary 10K vectors.
//! Contract-level types ([`CrystalFingerprint`]) carry the storage
//! format; the consumer crate picks the VSA operator.
//!
//! ## Crystal hierarchy
//!
//! ```text
//! SentenceCrystal   — one parsed sentence, triples + tekamolo slots
//! ContextCrystal    — Markov ±5 window around a sentence
//! DocumentCrystal   — full document, composed of sentence crystals
//! CycleCrystal      — one cognitive cycle (persistence view of ShaderCrystal)
//! SessionCrystal    — full conversation / agent session
//! ```
//!
//! All crystals share the [`Crystal`] trait: hardness, revision count,
//! crystallized-at timestamp, and a polymorphic [`CrystalFingerprint`].

pub mod context;
pub mod cycle;
pub mod document;
pub mod fingerprint;
pub mod sentence;
pub mod session;

pub use context::ContextCrystal;
pub use cycle::CycleCrystal;
pub use document::DocumentCrystal;
pub use fingerprint::{
    binary16k_to_vsa16k_bipolar, vsa16k_bind, vsa16k_bundle, vsa16k_cosine,
    vsa16k_to_binary16k_threshold, vsa16k_zero, CrystalFingerprint, Quorum5D, Structured5x5,
};
pub use sentence::SentenceCrystal;
pub use session::SessionCrystal;

/// The kind of crystal — used for dispatch and policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CrystalKind {
    Sentence,
    Context,
    Document,
    Cycle,
    Session,
}

/// NARS truth value — frequency (evidence ratio) + confidence (sample size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TruthValue {
    pub frequency: f32,
    pub confidence: f32,
}

impl TruthValue {
    pub const fn new(frequency: f32, confidence: f32) -> Self {
        Self {
            frequency,
            confidence,
        }
    }
}

/// Common trait across all crystal kinds.
pub trait Crystal {
    fn kind(&self) -> CrystalKind;

    /// Hardness ∈ [0, 1]. Accumulates via NARS revision as evidence stacks.
    /// Crosses the unbundling threshold (~0.8) → promote to individually
    /// addressable facts in episodic memory.
    fn hardness(&self) -> f32;

    /// Number of NARS revisions that have folded into this crystal.
    fn revision_count(&self) -> u32;

    /// Crystallization timestamp (Unix seconds).
    fn crystallized_at(&self) -> u64;

    /// The polymorphic fingerprint carrying this crystal's semantic content.
    fn fingerprint(&self) -> &CrystalFingerprint;

    /// NARS truth value at the current revision.
    fn truth(&self) -> TruthValue;
}

/// Threshold above which a crystal is considered "hardened" — ready to
/// unbundle from the young bundled form into individually addressable facts.
pub const UNBUNDLE_HARDNESS_THRESHOLD: f32 = 0.8;
