//! The [`Bulk`] transaction context — per-batch snapshot Lance version,
//! lazy Baton epoch flush, async cascade.
//!
//! ## When to use
//!
//! Bulk is the context for overnight batch imports and large-volume
//! synchronisation jobs:
//! - Kontodaten-Sync nächtlich (bank-statement import)
//! - Massenbuchung (bulk posting from CSV/DATEV export)
//! - Stapelverarbeitung von Rechnungen (batch invoice processing)
//!
//! ## Commit semantics (E-TRANSACTION-CONTEXT-1)
//!
//! | Property | Bulk |
//! |---|---|
//! | Op call site | `apply_stream` (warm, per-element flow-controlled) |
//! | Lance version | Per-batch snapshot (pinned at batch start) |
//! | Baton emission | Lazy — queued, flushed once per epoch boundary |
//! | Cascade traversal | Async, batched per epoch |
//! | `.output()` blocks on | Epoch flush at batch commit |
//!
//! ## Cross-references
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md` §"The transaction context"
//! - Epiphanies: E-TRANSACTION-CONTEXT-1, E-NO-AUTOMATIC-REGIME-PICK-1
//! - Cascade: [`crate::cognition::cascade::TraversalMode::Batched`]

use super::ctx::{Context, DolceCtx, FibuCtx, OgitCtx, OwlCtx};
use crate::cognition::entity::{
    DolceCategory, FibuAlignmentRef, OdooEntityRef, OgitUriRef, OwlClassRef,
};

// ── Bulk ──────────────────────────────────────────────────────────────────────

/// Bulk transaction context — per-batch snapshot, lazy epoch flush,
/// async cascade.
///
// TODO(Stage 2): Stage 2 adds:
// - `epoch_baton_queue: EpochBatonQueue` — the deferred Baton
//   emission queue flushed at epoch boundary.
// - `async_cascade_walker: BatchedCascadeWalker` — the async
//   batched cascade traverser.
// - `lance_snapshot: LanceSnapshotHandle` — the per-batch frozen
//   Lance version pinned at batch start.
// - `backpressure_mode: BackpressureMode` — drop-newest / block-
//   producer / spill-to-Lance (see plan §"Stream backpressure").
// For Stage 1 we hold a unit placeholder.
pub struct Bulk {
    /// Stage-1 placeholder; Stage 2 replaces with the epoch Baton
    /// queue + async cascade walker + Lance snapshot handle.
    _placeholder: (),
}

impl Bulk {
    /// Construct a new `Bulk` context.
    ///
    /// In Stage 1 this is a no-op constructor. Stage 2 wires the
    /// per-batch Lance snapshot, epoch Baton queue, and backpressure
    /// policy.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for Bulk {
    fn default() -> Self {
        Self::new()
    }
}

// ── Trait impls ───────────────────────────────────────────────────────────────

impl Context for Bulk {}

impl OgitCtx for Bulk {
    fn resolve_ogit(&self, _model_name: &'static str) -> OgitUriRef {
        // TODO(Stage 2): same dispatch as Interactive::resolve_ogit but reads
        // from the per-batch snapshot Lance version rather than the
        // live version. Stage 2 distinction: snapshot means a row
        // added after batch start is invisible to this context.
        todo!("D-NEH-2 wires the OGIT codebook lookup (batch-snapshot version)")
    }
}

impl OwlCtx for Bulk {
    fn hydrate_owl(&self, _ogit_uri: OgitUriRef) -> OwlClassRef {
        // TODO(Stage 2): OWL hydration from the batch-snapshot TTL join registry.
        todo!("D-NEH-2 wires the OWL hydrator (batch-snapshot)")
    }
}

impl DolceCtx for Bulk {
    fn classify_dolce(&self, _owl_class: OwlClassRef) -> DolceCategory {
        // TODO(Stage 2): DOLCE classification from the batch-snapshot ontology.
        todo!("D-NEH-2 wires the DOLCE classifier (batch-snapshot)")
    }
}

impl FibuCtx for Bulk {
    fn align_fibu(&self, _dolce: DolceCategory, _odoo: OdooEntityRef) -> FibuAlignmentRef {
        // TODO(Stage 2): FIBU/FIBO alignment from the batch-snapshot Kontenerkennung tables.
        todo!("D-NEH-2 wires the FIBU/FIBO alignment overlay (batch-snapshot)")
    }
}
