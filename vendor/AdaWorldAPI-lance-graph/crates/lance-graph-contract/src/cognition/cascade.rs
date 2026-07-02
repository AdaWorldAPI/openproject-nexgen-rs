//! Cascade graph traversal (E-CASCADE-AS-EDGECOLUMN-1).
//!
//! Odoo's six overlapping cascade mechanisms collapse into ONE typed
//! graph on `EdgeColumn`. Each dependency is a `CausalEdge64` row:
//!
//! ```text
//! source_mailbox_ref → target_mailbox_ref
//! kind: CascadeKind   (which mechanism drove the dependency)
//! truth: NarsTruth    (frequency + confidence on whether it fired)
//! ```
//!
//! Traversal discipline comes from the enclosing transaction context:
//!
//! | Context | Traversal mode |
//! |---|---|
//! | `Interactive` | Sync DFS through dependent mailboxes |
//! | `Bulk` | Async batched per epoch |
//! | `Periodisch` | JIT-compiled fixed-point iteration |
//!
//! ## Cross-references
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md` §"Cascade as EdgeColumn"
//! - Epiphany: E-CASCADE-AS-EDGECOLUMN-1
//! - CausalEdge64: `causal-edge/edge.rs` (v2 layout, signed mantissa)

use super::entity::MailboxRow;

// ── CascadeKind ───────────────────────────────────────────────────────────────

/// Discriminant for which Odoo cascade mechanism drove the dependency.
///
/// Six Odoo mechanisms collapse into this enum plus `Other`:
///
/// 1. `@api.depends` strings → `ComputeRecompute`
/// 2. `@api.constrains` post-write hooks → `ConstrainFire`
/// 3. SQL FK `ondelete='cascade'` → `SqlFkOndelete`
/// 4. `base.automation` server-action trigger → `ServerAction`
/// 5. `_inherits` field-forwarding cascade → `InheritsForward`
/// 6. Implicit model cascades (mail-thread, tax-tag) → `LedgerUpdate`
///    / `ReportAggregate` / `Other`
///
/// Per E-CASCADE-AS-EDGECOLUMN-1: all six encode as `CausalEdge64`
/// rows on the owning mailbox's `EdgeColumn`, so traversal is a
/// single `EdgeColumn::walk_dependents` call, regardless of which
/// original mechanism created the dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CascadeKind {
    /// `@api.depends` recompute when source field changes.
    ///
    /// Odoo evaluates these at write-time; we encode as a
    /// `CausalEdge64` row so traversal is structural, not string-eval.
    ComputeRecompute,

    /// `@api.constrains` validation hook fires post-write.
    ///
    /// Odoo validates synchronously; encoded here to give the cascade
    /// walker visibility into which edges can produce constraint
    /// failures (for escalation routing).
    ConstrainFire,

    /// Ledger / partner-balance update cascade.
    ///
    /// Examples: `account.move` → `res.partner` (balance update);
    /// `account.move.line` → `account.account` (balance aggregation).
    LedgerUpdate,

    /// `account.report.line` aggregation refresh.
    ///
    /// Fires when a report line's domain changes and dependent
    /// aggregates must recompute (e.g. UStVA Kennzahl totals).
    ReportAggregate,

    /// SQL FK `ondelete='cascade'` — DB-level fan-out.
    ///
    /// Encoded here so the cascade walker can model hard deletes as
    /// `CausalEdge64` tombstones rather than silent row disappearances.
    SqlFkOndelete,

    /// `base.automation` server-action trigger. Highly configurable
    /// in Odoo; Stage 2 will audit specific server-action patterns
    /// from the EXT-2 output and may add subtypes alongside this
    /// variant, but the base discriminant stays.
    //
    // TODO(Stage 2): audit `base.automation` records in EXT-2 to
    // surface specific subtypes; promote those to their own variants
    // while keeping `ServerAction` as the catch-all.
    ServerAction,

    /// `_inherits` field-forwarding cascade.
    ///
    /// Odoo's `_inherits` silently forwards field writes to the
    /// parent model's record; encoded as an `InheritsForward` edge
    /// so the cascade walker sees it.
    InheritsForward,

    /// Catch-all for model-specific implicit cascades not yet
    /// individually enumerated.
    ///
    // TODO(Stage 2): Stage 2 enumerates the remaining cases by auditing
    // all `_inherit`/`_inherits` chains in the EXT-2 output and
    // promoting recurring implicit cascades (mail-thread auto-
    // subscribe, tax-tag aggregation) to their own variants.
    Other,
}

// ── TraversalMode ─────────────────────────────────────────────────────────────

/// How the cascade walker traverses the dependency graph.
///
/// The mode is determined by the enclosing transaction context
/// (E-TRANSACTION-CONTEXT-1 + E-NO-AUTOMATIC-REGIME-PICK-1):
/// the consumer's typed enclosure picks the mode; the shader does
/// NOT choose autonomously.
///
/// ## Cross-references
/// - Epiphanies: E-TRANSACTION-CONTEXT-1, E-NO-AUTOMATIC-REGIME-PICK-1
/// - `crate::transaction::{Interactive, Bulk, Periodisch}`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TraversalMode {
    /// `Interactive` context: synchronous DFS through dependent
    /// mailboxes, blocking until cascade quiescence.
    ///
    /// Correct for single-entity interactive flows where the caller
    /// MUST see live data (no frozen snapshot). Examples: Kunde lädt
    /// Rechnung hoch → Kontenerkennung → Posting → UStVA-Kz.
    Sync,

    /// `Bulk` context: async, batched per epoch; flushed once per
    /// batch at commit time.
    ///
    /// Correct for overnight batch imports (Kontodaten-Sync) where
    /// individual cascade cycles are cheaper amortised per batch.
    Batched,

    /// `Periodisch` context: JIT-compiled fixed-point iteration over
    /// a frozen Lance version.
    ///
    /// Correct for fiscal-year close / UStVA-Q4, where the Lance
    /// version is frozen at the cutoff date and iteration continues
    /// until debits = credits.
    ///
    // TODO(Stage 2): the JIT chain handle for fixed-point iteration over
    // a sequence of Op kernels is not yet defined in
    // `lance-graph-contract::jit`. Stage 2 adds
    // `JitChainHandle::iterate_until(predicate)`.
    JitFixedPoint,
}

// ── CascadeWalker ─────────────────────────────────────────────────────────────

/// Trait the `EdgeColumn` implements to expose cascade walks.
///
/// The `EdgeColumn` type lives in `cognitive-shader-driver`, not in
/// `contract`. For Stage 1 we declare the trait shape so that callers
/// in this crate can be written against it; Stage 2 makes
/// `cognitive-shader-driver` impl it.
///
/// ## Contract semantics
///
/// - `walk_dependents` visits every row in the owning mailbox's
///   `EdgeColumn` whose `source_mailbox_ref` matches `from.mailbox_ref`
///   and whose `source_row_idx` matches `from.row_idx`.
/// - If `kind_filter` is `Some(k)`, only rows with that `CascadeKind`
///   are visited.
/// - The `mode` argument tells the walker which traversal discipline
///   to apply (sync DFS / async batched / JIT fixed-point).
///
// TODO(Stage 2): Stage 2 wires this trait as `impl CascadeWalker for
// cognitive_shader_driver::EdgeColumn`. The walker output (the set
// of `MailboxRow`s that must be re-evaluated) is fed back into
// the Op chain as a new `NormalizedEntity<Raw>` per dependent row,
// forming the dependency fan-out. The Baton (`(u16, CausalEdge64)`)
// carries the causal edge across mailbox boundaries per E-BATON-1.
pub trait CascadeWalker {
    /// Walk all downstream dependents of `from` in the `EdgeColumn`.
    ///
    /// - `from` — the source `MailboxRow` whose dependents are walked.
    /// - `kind_filter` — restrict traversal to one cascade kind, or
    ///   `None` for all kinds.
    /// - `mode` — traversal discipline set by the transaction context.
    /// - `on_dependent` — invoked once per dependent `MailboxRow`
    ///   reached by the walk. The closure can re-enter the chain with
    ///   each row to fan the cascade out per the Stage 2 wiring.
    fn walk_dependents(
        &self,
        from: MailboxRow,
        kind_filter: Option<CascadeKind>,
        mode: TraversalMode,
        on_dependent: &mut dyn FnMut(MailboxRow),
    );
}
