//! Typed consumer pipeline grammar for normalized OGIT/OWL/DOLCE/Odoo
//! entities, per `.claude/plans/normalized-entity-holy-grail-v1.md`.
//!
//! ## The carrier
//!
//! [`NormalizedEntity<Stage>`] holds the 4-way inheritance chain as a
//! typed lens into a `MailboxSoA` row. `Stage` is phantom-typed;
//! advancement happens via the five-verb algebra and is
//! compile-time-enforced.
//!
//! ## The algebra (E-OP-FIVE-VERBS-1)
//!
//! - `resolve_ogit`   — `Raw` → `WithOgit`
//! - `hydrate_owl`    — `WithOgit` → `WithOwl`
//! - `classify_dolce` — `WithOwl` → `WithDolce`
//! - `align_fibu`     — `WithDolce` → `Normalized`
//! - `op` / `chk_data` / `review` / `abduct` / `report` / `output` — the
//!   `think` op-chain over the normalized carrier
//!
//! ## The Op trait (E-OP-THREE-CALLSITES-1)
//!
//! [`Op<I,O>`](op::Op) has three call sites — `apply` (cold),
//! `apply_stream` (warm, deferred to Stage 2), `apply_soa` (hot,
//! JIT-compiled, deferred to Stage 2). One trait, three speeds, one
//! set of const data.
//!
//! ## Cross-references
//!
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md`
//! - Epiphanies: E-NORMALIZED-ENTITY-1, E-OP-FIVE-VERBS-1,
//!   E-OP-THREE-CALLSITES-1, E-CONSUMER-CANNOT-INTERPRET-1
//! - Mailbox SoA: PR #427 (thoughtspace columns)
//! - Codebook: `super::callcenter::ogit_uris`
//!
//! ## Example consumer chain (woa-rs invoice flow)
//!
//! ```no_run
//! use lance_graph_contract::cognition::*;
//! use lance_graph_contract::cognition::entity::{OdooEntityRef, MailboxRow};
//! use lance_graph_contract::transaction::Interactive;
//!
//! # let ctx = Interactive::new();
//! # let invoice = NormalizedEntity::<Raw>::raw(
//! #     OdooEntityRef("account.move"),
//! #     MailboxRow { mailbox_ref: 0, row_idx: 0 },
//! # );
//! # /*
//! // Concrete Op types (from Stage 2 kernel implementations):
//! // KontenerkennungSkr04, SkrAccountInRange, FiscalPositionResolver,
//! // VatLiability, GoBdLockCheck, UStvaKennzahlAggregator
//!
//! let result = invoice
//!     .resolve_ogit(&ctx)       // Raw → WithOgit
//!     .hydrate_owl(&ctx)        // WithOgit → WithOwl
//!     .classify_dolce(&ctx)     // WithOwl → WithDolce
//!     .align_fibu(&ctx)         // WithDolce → Normalized
//!     .op(KontenerkennungSkr04)                       // Normalized → Normalized
//!     .chk_data(SkrAccountInRange::new(8400..=8499))  // → Checked
//!     .review(FiscalPositionResolver)                 // → Reviewed
//!     .abduct(VatLiability)                           // → Abducted
//!     .op(GoBdLockCheck)                              // → Abducted
//!     .report(UStvaKennzahlAggregator)                // → Reported
//!     .output();                                      // → Output, cascade fires
//! # */
//! ```
//!
//! Same chain shape inside `Bulk` (warm path) and `Periodisch` (hot
//! JIT path) — the context picks the call site per
//! E-OP-THREE-CALLSITES-1 + E-TRANSACTION-CONTEXT-1.
//!
//! ## What the type system forbids (compile-fail proofs)
//!
//! These four `compile_fail` doctests live in lib-level rustdoc (not in
//! `tests/`) so that `cargo test --doc` actually gates them. Per PR #431
//! review (coderabbit Major): the original copies under
//! `tests/cognition_typestate.rs` were silently un-gated because the
//! crate's CI ran with `--lib` only. Keeping the gates here ensures any
//! future visibility loosening or stage skip surfaces as a test failure.
//!
//! ```compile_fail
//! use lance_graph_contract::cognition::*;
//! use lance_graph_contract::cognition::entity::{OdooEntityRef, MailboxRow};
//!
//! // Cannot call .chk_data() / .review() / .abduct() on a Raw entity —
//! // those methods only exist on later stages.
//! let entity = NormalizedEntity::<Raw>::raw(
//!     OdooEntityRef("account.move"),
//!     MailboxRow { mailbox_ref: 0, row_idx: 0 },
//! );
//! // This must NOT compile: no method `review` on `NormalizedEntity<Raw>`.
//! entity.review(todo!());
//! ```
//!
//! ```compile_fail
//! use lance_graph_contract::cognition::*;
//!
//! // Cannot pass a Normalized entity where Reviewed is expected — the
//! // stage param is part of the type, so stage skipping is forbidden.
//! fn _requires_reviewed(_: NormalizedEntity<Reviewed>) {}
//!
//! let entity: NormalizedEntity<Normalized> = panic!("never runs");
//! _requires_reviewed(entity);
//! ```
//!
//! ```compile_fail
//! use lance_graph_contract::cognition::*;
//!
//! // Cannot call .op() on a Reported entity — the chain is closed after
//! // .report(). `NormalizedEntity<Reported>` only has `.output()`.
//! fn _calls_op_on_reported(entity: NormalizedEntity<Reported>) {
//!     entity.op(todo!());
//! }
//! ```
//!
//! ```compile_fail
//! use lance_graph_contract::cognition::stages::Stage;
//!
//! // Cannot implement Stage for a consumer-introduced type — the trait
//! // is sealed via the private `sealed::Sealed` supertrait.
//! struct MyStage;
//! impl Stage for MyStage {}
//! ```

pub mod advance;
pub mod cascade;
pub mod entity;
pub mod op;
pub mod stages;

pub use cascade::{CascadeKind, CascadeWalker, TraversalMode};
pub use entity::NormalizedEntity;
pub use op::{Op, OpError, OpKind, Output};
pub use stages::{
    Abducted, Checked, Normalized, Raw, Reported, Reviewed, Stage, WithDolce, WithOgit, WithOwl,
};
