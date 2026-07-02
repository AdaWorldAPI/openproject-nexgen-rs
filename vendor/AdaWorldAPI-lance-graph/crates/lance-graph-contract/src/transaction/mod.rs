//! Typed transaction-context shapes (E-TRANSACTION-CONTEXT-1).
//!
//! Three contexts, each picks the Op call site + Baton epoch + Lance
//! version + cascade traversal mode. The consumer's typed enclosure
//! picks the regime; the shader does NOT autonomously choose
//! (E-NO-AUTOMATIC-REGIME-PICK-1).
//!
//! ## Context comparison
//!
//! | | [`Interactive`] | [`Bulk`] | [`Periodisch`] |
//! |---|---|---|---|
//! | Op call site | `apply` (cold) | `apply_stream` (warm) | `apply_soa` (hot, JIT) |
//! | Lance version | live | per-batch snapshot | frozen point-in-time |
//! | Baton emission | eager, immediate fan-out | lazy, per-epoch flush | epochal, iterate-to-fixed-point |
//! | Cascade graph | sync DFS | async, batched | JIT-compiled fixed-point |
//! | `.output()` blocks on | cascade quiescence | epoch flush | fiscal-cutoff debits=credits |
//! | Typical example | Rechnung hoch → Posting → UStVA-Kz | Kontodaten-Sync nächtlich | Jahresabrechnung |
//!
//! ## Epiphany anchors
//!
//! - E-TRANSACTION-CONTEXT-1 — the three contexts are genuinely distinct
//!   SLAs, not a runtime mode switch on a single context.
//! - E-NO-AUTOMATIC-REGIME-PICK-1 — the shader does NOT pick the regime;
//!   the consumer's enclosure does.
//! - E-ODOO-AS-PRIOR-ART-1 — Odoo solved the same three SLAs via
//!   `@api.depends` strings / `env.context` flags / lock-date wizards.
//!   We re-encode the decomposition as compile-time typestate.
//!
//! ## Cross-references
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md`
//!   §"The transaction context"
//! - Five-verb algebra: [`crate::cognition::advance`]
//! - Cascade modes: [`crate::cognition::cascade::TraversalMode`]

pub mod bulk;
pub mod ctx;
pub mod interactive;
pub mod periodisch;

pub use bulk::Bulk;
pub use ctx::{Context, DolceCtx, FibuCtx, OgitCtx, OwlCtx};
pub use interactive::Interactive;
pub use periodisch::Periodisch;
