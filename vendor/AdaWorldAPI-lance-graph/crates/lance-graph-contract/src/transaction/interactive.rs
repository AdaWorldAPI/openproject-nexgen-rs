//! The [`Interactive`] transaction context — eager cascade, live Lance
//! version, sync DFS traversal of dependent mailboxes.
//!
//! ## When to use
//!
//! Interactive is the context for single-entity flows triggered by a
//! user action:
//! - Kunde lädt Rechnung hoch → Kontenerkennung → Posting → UStVA-Kz
//! - Einzelne Kontoabfrage → Saldoprüfung → Regulierungsübereinstimmung
//!
//! ## Commit semantics (E-TRANSACTION-CONTEXT-1)
//!
//! | Property | Interactive |
//! |---|---|
//! | Op call site | `apply` (cold, single carrier) |
//! | Lance version | Live (most-recent committed version) |
//! | Baton emission | Eager — fan-out fires immediately at `.output()` |
//! | Cascade traversal | Sync DFS through dependent mailboxes |
//! | `.output()` blocks on | Cascade quiescence |
//!
//! ## Cross-references
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md` §"The transaction context"
//! - Epiphanies: E-TRANSACTION-CONTEXT-1, E-NO-AUTOMATIC-REGIME-PICK-1
//! - Cascade: [`crate::cognition::cascade::TraversalMode::Sync`]

use super::ctx::{Context, DolceCtx, FibuCtx, OgitCtx, OwlCtx};
use crate::cognition::entity::{
    DolceCategory, FibuAlignmentRef, OdooEntityRef, OgitUriRef, OwlClassRef,
};

// ── Interactive ───────────────────────────────────────────────────────────────

/// Interactive transaction context — eager cascade, live Lance version,
/// sync DFS cascade traversal.
///
/// Construction is via [`Interactive::new`]. The context is not `Clone`
/// (to discourage sharing across concurrent flows — each interactive
/// flow owns its context for the duration of the chain).
///
// TODO(Stage 2): Stage 2 adds:
// - `baton_queue: BatonEmissionQueue` — the sync Baton fan-out
//   queue that fires at `.output()`.
// - `edge_column_walker: SyncCascadeWalker` — the sync DFS
//   traverser over the `EdgeColumn` per
//   `lance_graph_contract::cognition::cascade::CascadeWalker`.
// - `lance_version: LanceReadHandle` — pinned live version.
// For Stage 1 we hold a unit placeholder.
pub struct Interactive {
    /// Stage-1 placeholder; Stage 2 replaces with the live Baton
    /// emission queue + sync cascade walker.
    _placeholder: (),
}

impl Interactive {
    /// Construct a new `Interactive` context.
    ///
    /// In Stage 1 this is a no-op constructor. Stage 2 wires the live
    /// Lance read handle and Baton emission queue.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Default for Interactive {
    fn default() -> Self {
        Self::new()
    }
}

// ── Trait impls ───────────────────────────────────────────────────────────────

impl Context for Interactive {}

impl OgitCtx for Interactive {
    fn resolve_ogit(&self, _model_name: &'static str) -> OgitUriRef {
        // TODO(Stage 2): dispatch into `crate::callcenter::ogit_uris` for the
        // canonical OGIT URI lookup. The codebook maps `model_name`
        // to a stable `OgitUriRef` via `OntologyRegistry::resolve`.
        // Stage 2 also writes the resolved code into the owning
        // mailbox's SoA fingerprint column via the Baton emission
        // queue (E-CODEBOOK-INHERITS-FROM-OGIT).
        todo!("D-NEH-2 wires the real OGIT codebook lookup via callcenter::ogit_uris")
    }
}

impl OwlCtx for Interactive {
    fn hydrate_owl(&self, _ogit_uri: OgitUriRef) -> OwlClassRef {
        // TODO(Stage 2): dispatch OWL hydration via the TTL-join registry.
        // Stage 2 wires this against the EXT-1 OWL extraction.
        todo!("D-NEH-2 wires the OWL hydrator (TTL join on OGIT URI)")
    }
}

impl DolceCtx for Interactive {
    fn classify_dolce(&self, _owl_class: OwlClassRef) -> DolceCategory {
        // TODO(Stage 2): dispatch into lance_graph_ontology::dolce_odoo::DolceClassifier.
        todo!("D-NEH-2 wires the DOLCE classifier from lance-graph-ontology::dolce_odoo")
    }
}

impl FibuCtx for Interactive {
    fn align_fibu(&self, _dolce: DolceCategory, _odoo: OdooEntityRef) -> FibuAlignmentRef {
        // TODO(Stage 2): dispatch into the Kontenerkennung alignment tables
        // (SKR03/SKR04, UStVA Kennzahlen, GoBD wiring).
        todo!("D-NEH-2 wires the FIBU/FIBO alignment overlay from EXT-1..6")
    }
}
