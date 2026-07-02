//! The [`Periodisch`] transaction context — frozen Lance version,
//! JIT-compiled chain, epochal fixed-point iteration.
//!
//! ## When to use
//!
//! Periodisch is the context for fiscal-period close jobs that require
//! a frozen point-in-time view and iterate until a global invariant
//! holds (debits = credits):
//! - Jahresabrechnung (annual fiscal-year close)
//! - UStVA-Q4 (quarterly VAT return aggregation)
//! - GoBD-compliant audit export (frozen version, no retroactive writes)
//!
//! ## Commit semantics (E-TRANSACTION-CONTEXT-1)
//!
//! | Property | Periodisch |
//! |---|---|
//! | Op call site | `apply_soa` (hot, JIT-compiled SoA-swept SIMD) |
//! | Lance version | Frozen point-in-time (fiscal-cutoff date) |
//! | Baton emission | Epochal — iterate until fixed-point |
//! | Cascade traversal | JIT-compiled fixed-point iteration |
//! | `.output()` blocks on | Fiscal-cutoff invariant: debits = credits |
//!
//! ## Cross-references
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md` §"The transaction context"
//! - Epiphanies: E-TRANSACTION-CONTEXT-1, E-NO-AUTOMATIC-REGIME-PICK-1
//! - Cascade: [`crate::cognition::cascade::TraversalMode::JitFixedPoint`]
//! - JIT substrate: `crate::jit::{JitCompiler, KernelHandle}`

use super::ctx::{Context, DolceCtx, FibuCtx, OgitCtx, OwlCtx};
use crate::cognition::entity::{
    DolceCategory, FibuAlignmentRef, OdooEntityRef, OgitUriRef, OwlClassRef,
};

// ── Periodisch ────────────────────────────────────────────────────────────────

/// Periodisch (periodic / fiscal) transaction context — frozen Lance
/// version, JIT-compiled chain, fixed-point iteration.
///
/// The name is deliberately German to match the workspace's naming
/// convention (E-ODOO-AS-PRIOR-ART-1: Odoo's `with_env(cr)` wizards
/// plus lock-date global state are the prior art; our re-encoding uses
/// a typed frozen context).
///
// TODO(Stage 2): Stage 2 adds:
// - `frozen_lance_version: LanceFrozenHandle` — the Lance version
//   pinned at the fiscal cutoff timestamp.
// - `jit_chain: JitChainHandle` — the JIT-compiled Op chain for
//   the hot path. Requires `lance-graph-contract::jit::JitCompiler`
//   to grow `compile_chain(ops: &[OpKind]) -> JitChainHandle`.
//   See plan §"JIT shape for chains" open question.
// - `fixed_point_predicate: fn(&MailboxSoA) -> bool` — the
//   termination condition (e.g. `|soa| soa.total_debit() == soa.total_credit()`).
// For Stage 1 we hold a unit placeholder.
pub struct Periodisch {
    /// Stage-1 placeholder; Stage 2 replaces with the frozen Lance
    /// handle + JIT chain handle + fixed-point predicate.
    _placeholder: (),
}

impl Periodisch {
    /// Construct a new `Periodisch` context.
    ///
    /// In Stage 1 this is a no-op constructor. Stage 2 accepts the
    /// fiscal-cutoff timestamp and compiles the JIT chain at
    /// construction time.
    pub fn new() -> Self {
        Self { _placeholder: () }
    }

    /// Return the frozen Lance version handle.
    ///
    // TODO(Stage 2): Stage 2 exposes a real `LanceFrozenHandle` that holds
    // a reference to the Lance version at the fiscal-cutoff date.
    // For now, this is a documentation placeholder showing where
    // the API surface will sit.
    pub fn frozen_lance_version(&self) {
        todo!("D-NEH-5 wires the frozen Lance version handle (JahresabrechnungChain)")
    }
}

impl Default for Periodisch {
    fn default() -> Self {
        Self::new()
    }
}

// ── Trait impls ───────────────────────────────────────────────────────────────

impl Context for Periodisch {}

impl OgitCtx for Periodisch {
    fn resolve_ogit(&self, _model_name: &'static str) -> OgitUriRef {
        // TODO(Stage 2): same dispatch as Interactive/Bulk but reads from the
        // FROZEN Lance version at fiscal-cutoff. Critical invariant:
        // MUST NOT see any OGIT codebook changes made after the
        // cutoff date (GoBD compliance: no retroactive writes).
        todo!("D-NEH-2 wires the OGIT codebook lookup (frozen-version)")
    }
}

impl OwlCtx for Periodisch {
    fn hydrate_owl(&self, _ogit_uri: OgitUriRef) -> OwlClassRef {
        // TODO(Stage 2): OWL hydration from the frozen Lance TTL registry.
        todo!("D-NEH-2 wires the OWL hydrator (frozen-version)")
    }
}

impl DolceCtx for Periodisch {
    fn classify_dolce(&self, _owl_class: OwlClassRef) -> DolceCategory {
        // TODO(Stage 2): DOLCE classification from the frozen Lance ontology.
        todo!("D-NEH-2 wires the DOLCE classifier (frozen-version)")
    }
}

impl FibuCtx for Periodisch {
    fn align_fibu(&self, _dolce: DolceCategory, _odoo: OdooEntityRef) -> FibuAlignmentRef {
        // TODO(Stage 2): FIBU/FIBO alignment from the frozen Kontenerkennung
        // tables. GoBD compliance: the SKR chart used at cutoff is
        // the authority; subsequent chart updates are invisible.
        todo!("D-NEH-2 wires the FIBU/FIBO alignment overlay (frozen-version)")
    }
}
