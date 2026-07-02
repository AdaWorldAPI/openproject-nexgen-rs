//! Grammar — tiered routing types and reference tables.
//!
//! Local grammar matches (deepnsm + rule tables) handle 90-99% of parsing
//! at sub-10µs. The remainder produces a structured [`FailureTicket`] that
//! an LLM fallback can resolve surgically — never the full sentence, only
//! the ambiguous slots.
//!
//! ## Modules
//!
//! - [`ticket`]        — [`FailureTicket`], partial parse, causal ambiguity.
//! - [`tekamolo`]      — Temporal/Kausal/Modal/Lokal adverbial slots.
//! - [`wechsel`]       — Dual-role tokens (prepositions, pronouns, etc.).
//! - [`finnish`]       — 15 Finnish cases as direct slot lookups.
//! - [`inference`]     — NARS inference × thinking style routing table.
//! - [`context_chain`] — Markov ±5 replay for disambiguation.

pub mod context_chain;
pub mod finnish;
pub mod free_energy;
pub mod inference;
pub mod role_keys;
pub mod tekamolo;
pub mod thinking_styles;
pub mod ticket;
pub mod wechsel;

// PR #279 outlook epiphany E3 — 144-cell verb-role lookup table.
pub mod verb_table;

// PR #279 outlook epiphany E5 — generalized disambiguation primitive.
pub mod disambiguator;

pub use context_chain::{
    ContextChain, DisambiguateOpts, DisambiguationResult, ReplayDirection, ReplayRequest,
    WeightingKernel, CHAIN_LEN, DISAMBIGUATION_MARGIN_THRESHOLD, MARKOV_RADIUS,
};
pub use disambiguator::{disambiguate_general, Disambiguatable, GeneralizedResult};
pub use finnish::{finnish_case_for_suffix, FinnishCase};
pub use free_energy::{
    FreeEnergy, Hypothesis, Resolution, EPIPHANY_MARGIN, FAILURE_CEILING, HOMEOSTASIS_FLOOR,
};
pub use inference::{inference_to_style_cluster, NarsInference};
pub use role_keys::*;
pub use tekamolo::{TekamoloSlot, TekamoloSlots};
pub use thinking_styles::{
    revise_truth, CoveragePolicy, GrammarStyleAwareness, GrammarStyleConfig, MarkovPolicy,
    MorphologyPolicy, MorphologyTableId, NarsPriorityChain, ParamKey, ParseOutcome, ReplayStrategy,
    SpoCausalPolicy, TekamoloPolicy,
};
pub use ticket::{CausalAmbiguity, FailureTicket, PartialParse};
pub use verb_table::{default_table, SlotPrior, VerbFamily, VerbRoleTable};
pub use wechsel::{WechselAmbiguity, WechselRole};

/// Coverage of a local parse — if below [`LOCAL_COVERAGE_THRESHOLD`],
/// the ticket is emitted for LLM fallback.
pub const LOCAL_COVERAGE_THRESHOLD: f32 = 0.9;
