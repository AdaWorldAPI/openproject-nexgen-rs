//! Five-verb advancement methods on [`NormalizedEntity<S>`].
//!
//! Each method is only defined on the appropriate input stage, enforcing
//! the pipeline order at compile time. Calling `.review()` on a `<Raw>`
//! entity or `.abduct()` on a `<Normalized>` entity is a compile error.
//!
//! ## Pipeline order
//!
//! ```text
//! Raw ─resolve_ogit─► WithOgit ─hydrate_owl─► WithOwl
//!                                                 │
//!                                         classify_dolce
//!                                                 ▼
//!                                            WithDolce
//!                                                 │
//!                                           align_fibu
//!                                                 ▼
//!                                           Normalized
//!                            op* / chk_data / review / abduct / op* / report
//!                                                 ▼
//!                                            Reported ─output()─► Output
//! ```
//!
//! ## Cross-references
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md` §"The algebra"
//! - Epiphanies: E-OP-FIVE-VERBS-1, E-NORMALIZED-ENTITY-1
//! - Op trait: [`super::op::Op`]
//! - Transaction contexts: [`crate::transaction`]

use super::entity::NormalizedEntity;
use super::op::{Op, Output};
use super::stages::*;
use crate::transaction::{DolceCtx, FibuCtx, OgitCtx, OwlCtx};

// ── Verb 1: resolve_ogit ──────────────────────────────────────────────────────

impl NormalizedEntity<Raw> {
    /// Stage 1 → Stage 2: resolve Odoo model_name → OGIT URI.
    ///
    /// Dispatches into the OGIT codebook via the transaction context
    /// (which implements [`OgitCtx`]). Returns a `NormalizedEntity`
    /// with the `ogit` slot populated.
    ///
    /// Per E-CODEBOOK-INHERITS-FROM-OGIT: the resolved URI is a
    /// stable codebook row index, not a freshly hashed value.
    ///
    // TODO(Stage 2): also need to write back the resolved OGIT identity
    // into the owning mailbox's SoA fingerprint column. Stage 2
    // wires this once `cognitive-shader-driver` is a hard dep of
    // contract (or the write-back goes through a trait adapter).
    pub fn resolve_ogit<C: OgitCtx>(self, ctx: &C) -> NormalizedEntity<WithOgit> {
        let ogit = ctx.resolve_ogit(self.odoo.0);
        // Use advance_stage_internal to copy the struct without touching _stage
        // directly, then overwrite the ogit field.
        let mut advanced = self.advance_stage_internal::<WithOgit>();
        advanced.ogit = Some(ogit);
        advanced
    }
}

// ── Verb 2: hydrate_owl ───────────────────────────────────────────────────────

impl NormalizedEntity<WithOgit> {
    /// Stage 2 → Stage 3: hydrate OGIT URI → OWL class via TTL join.
    ///
    /// Dispatches into the OWL registry via the transaction context
    /// (which implements [`OwlCtx`]). Returns a `NormalizedEntity`
    /// with the `owl` slot populated.
    ///
    // TODO(Stage 2): dispatch OWL hydration via `ctx.hydrate_owl()`. Stage 2
    // wires the concrete hydrator from `lance-graph-ontology` (the
    // OWL TTL graph is already extracted in the EXT-1 deliverable).
    pub fn hydrate_owl<C: OwlCtx>(self, _ctx: &C) -> NormalizedEntity<WithOwl> {
        todo!("D-NEH-2 wires the OWL hydrator from lance-graph-ontology")
    }
}

// ── Verb 3: classify_dolce ────────────────────────────────────────────────────

impl NormalizedEntity<WithOwl> {
    /// Stage 3 → Stage 4: classify OWL class → DOLCE upper-ontology
    /// category.
    ///
    /// Dispatches into the DOLCE classifier via the transaction context
    /// (which implements [`DolceCtx`]). Returns a `NormalizedEntity`
    /// with the `dolce` slot populated.
    ///
    // TODO(Stage 2): dispatch DOLCE classification via `ctx.classify_dolce()`.
    // Stage 2 wires the concrete classifier from
    // `lance-graph-ontology::dolce_odoo` (already shipped in the
    // EXT-2..6 extraction).
    pub fn classify_dolce<C: DolceCtx>(self, _ctx: &C) -> NormalizedEntity<WithDolce> {
        todo!("D-NEH-2 wires the DOLCE classifier from lance-graph-ontology::dolce_odoo")
    }
}

// ── Verb 4: align_fibu ────────────────────────────────────────────────────────

impl NormalizedEntity<WithDolce> {
    /// Stage 4 → Stage 5: align DOLCE category → FIBU/FIBO domain
    /// overlay (German accounting frames).
    ///
    /// Dispatches into the FIBU alignment table via the transaction
    /// context (which implements [`FibuCtx`]). Returns a
    /// `NormalizedEntity<Normalized>` — fully chain-ready.
    ///
    // TODO(Stage 2): dispatch FIBU/FIBO alignment via `ctx.align_fibu()`.
    // Stage 2 wires the alignment overlay from the D-ODOO-EXT-1..6
    // Kontenerkennung tables (SKR03/SKR04 + UStVA Kennzahlen).
    pub fn align_fibu<C: FibuCtx>(self, _ctx: &C) -> NormalizedEntity<Normalized> {
        todo!("D-NEH-2 wires the FIBU/FIBO alignment overlay from EXT-1..6 Kontenerkennung")
    }
}

// ── Chain methods on Normalized ───────────────────────────────────────────────

impl NormalizedEntity<Normalized> {
    /// Apply a Normalized → Normalized Op to the carrier.
    ///
    /// Calls `op.step(&self)` for validation / side-effects, then
    /// performs the sealed stage transition via `advance_stage_internal`.
    ///
    /// Idiomatic usage: chain multiple `.op()` calls for sequential
    /// shader dispatches that leave the stage unchanged:
    ///
    /// ```no_run
    /// # use lance_graph_contract::cognition::*;
    /// # use lance_graph_contract::cognition::entity::*;
    /// # use lance_graph_contract::cognition::op::*;
    /// # struct KontCheck; impl Op<Normalized, Normalized> for KontCheck {
    /// #     fn kind(&self) -> OpKind { OpKind::UNWIRED }
    /// # }
    /// # struct GobdCheck; impl Op<Normalized, Normalized> for GobdCheck {
    /// #     fn kind(&self) -> OpKind { OpKind::UNWIRED }
    /// # }
    /// # fn demo(entity: NormalizedEntity<Normalized>) {
    /// entity.op(KontCheck).op(GobdCheck);
    /// # }
    /// ```
    pub fn op<O: Op<Normalized, Normalized>>(self, op: O) -> Self {
        // Stage 1: ignore step errors (kernel bodies are todo!() anyway).
        // Stage 2 wires proper error propagation through the chain.
        let _ = op.step(&self);
        self.advance_stage_internal::<Normalized>()
    }

    /// Data-quality check: `Normalized` → `Checked`.
    ///
    /// Calls `c.step(&self)` then advances the stage to `Checked`. Only
    /// one `chk_data` call is permitted in a chain (the type system
    /// makes a second call impossible without going through `review`).
    ///
    /// Typical argument: `SkrAccountInRange::new(8400..=8499)`.
    pub fn chk_data<C: Op<Normalized, Checked>>(self, c: C) -> NormalizedEntity<Checked> {
        let _ = c.step(&self);
        self.advance_stage_internal::<Checked>()
    }
}

// ── Chain methods on Checked ──────────────────────────────────────────────────

impl NormalizedEntity<Checked> {
    /// Fiscal-position / savant review: `Checked` → `Reviewed`.
    ///
    /// Calls `r.step(&self)` then advances the stage to `Reviewed`.
    pub fn review<R: Op<Checked, Reviewed>>(self, r: R) -> NormalizedEntity<Reviewed> {
        let _ = r.step(&self);
        self.advance_stage_internal::<Reviewed>()
    }
}

// ── Chain methods on Reviewed ─────────────────────────────────────────────────

impl NormalizedEntity<Reviewed> {
    /// NARS abductive inference: `Reviewed` → `Abducted`.
    ///
    /// Calls `a.step(&self)` then advances the stage to `Abducted`.
    pub fn abduct<A: Op<Reviewed, Abducted>>(self, a: A) -> NormalizedEntity<Abducted> {
        let _ = a.step(&self);
        self.advance_stage_internal::<Abducted>()
    }
}

// ── Chain methods on Abducted ─────────────────────────────────────────────────

impl NormalizedEntity<Abducted> {
    /// Apply an Abducted → Abducted Op to the carrier.
    ///
    /// Calls `op.step(&self)` then performs the sealed stage transition.
    /// Allows additional shader dispatches after abduction and before
    /// reporting (e.g. `GoBdLockCheck`).
    pub fn op<O: Op<Abducted, Abducted>>(self, op: O) -> Self {
        let _ = op.step(&self);
        self.advance_stage_internal::<Abducted>()
    }

    /// Aggregation / reporting: `Abducted` → `Reported`.
    ///
    /// Calls `p.step(&self)` then advances the stage to `Reported`.
    pub fn report<P: Op<Abducted, Reported>>(self, p: P) -> NormalizedEntity<Reported> {
        let _ = p.step(&self);
        self.advance_stage_internal::<Reported>()
    }
}

// ── Terminal: output ──────────────────────────────────────────────────────────

impl NormalizedEntity<Reported> {
    /// Terminal — emits the [`Output`], triggers cascade traversal per
    /// the enclosing transaction context.
    ///
    /// - `Interactive` context: blocks until cascade quiescence
    ///   (sync DFS through dependent mailboxes).
    /// - `Bulk` context: queues cascade; flushes at epoch boundary.
    /// - `Periodisch` context: triggers JIT-compiled fixed-point
    ///   iteration.
    ///
    /// Per E-CASCADE-AS-EDGECOLUMN-1: cascade traversal goes through
    /// the `EdgeColumn` walker, not through Odoo-style string-evaluated
    /// `@api.depends`. The Baton (`(u16, CausalEdge64)`) carries the
    /// causal edge to each dependent mailbox.
    ///
    // TODO(Stage 2): wire to Baton emission + [`super::cascade::CascadeWalker`]
    // traversal per the active transaction context. Stage 2 detail:
    // pull the active [`crate::transaction::Context`] from a thread-
    // local or env handle (or pass it in explicitly — API TBD once
    // the first concrete consumer exists in Stage 2). The current
    // `Output { success: true }` placeholder is unconditional; Stage 2
    // gates on NARS confidence < audit_floor for escalation.
    pub fn output(self) -> Output {
        Output { success: true }
    }
}
