//! Per-stage context traits for the transaction module.
//!
//! Each trait unlocks a verb in the five-verb algebra:
//!
//! | Trait | Verb | Stage transition |
//! |---|---|---|
//! | [`OgitCtx`] | `resolve_ogit` | `Raw` → `WithOgit` |
//! | [`OwlCtx`]  | `hydrate_owl`  | `WithOgit` → `WithOwl` |
//! | [`DolceCtx`]| `classify_dolce` | `WithOwl` → `WithDolce` |
//! | [`FibuCtx`] | `align_fibu`   | `WithDolce` → `Normalized` |
//!
//! All three concrete context types ([`super::Interactive`],
//! [`super::Bulk`], [`super::Periodisch`]) implement all four traits.
//! Stage 1 ships `todo!()` bodies; Stage 2 wires the real lookups.
//!
//! ## Cross-references
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md` §"The transaction context"
//! - Epiphanies: E-TRANSACTION-CONTEXT-1, E-NO-AUTOMATIC-REGIME-PICK-1

use crate::cognition::entity::{DolceCategory, FibuAlignmentRef, OgitUriRef, OwlClassRef};

// ── Context (sealed base) ─────────────────────────────────────────────────────

/// Base marker trait for a transaction context.
///
/// Sealed: only the three concrete contexts defined in this crate
/// implement this trait. Consumers cannot introduce new contexts
/// because the sealed supertrait prevents external implementations.
///
/// Implementors: [`super::Interactive`], [`super::Bulk`],
/// [`super::Periodisch`].
pub trait Context: 'static + sealed::ContextSealed {}

mod sealed {
    pub trait ContextSealed {}
    impl ContextSealed for super::super::Interactive {}
    impl ContextSealed for super::super::Bulk {}
    impl ContextSealed for super::super::Periodisch {}
}

// ── OgitCtx ───────────────────────────────────────────────────────────────────

/// A context capable of resolving an Odoo model_name to an OGIT URI.
///
/// Unlocks the `resolve_ogit` verb on
/// [`NormalizedEntity<Raw>`](crate::cognition::entity::NormalizedEntity).
///
/// Per E-CODEBOOK-INHERITS-FROM-OGIT: resolution MUST go through the
/// canonical `OntologyRegistry` (OGIT URI → stable row index), never
/// hash the model_name directly.
///
// TODO(Stage 2): Stage 2 wires the concrete implementation to dispatch into
// `crate::callcenter::ogit_uris::savant_ogit_uri` (already shipped
// in PR #427) for the savant codebook, and into a parallel
// `entity_ogit_uri(model_name)` lookup for Odoo model → OGIT URI
// mapping. Both lookups are `O(1)` static-str comparisons.
pub trait OgitCtx: Context {
    /// Resolve an Odoo `model_name` (e.g. `"account.move"`) to an
    /// [`OgitUriRef`] via the canonical OGIT codebook.
    ///
    /// Returns a static-str handle that can be stored in the `ogit`
    /// field of `NormalizedEntity<WithOgit>`.
    fn resolve_ogit(&self, model_name: &'static str) -> OgitUriRef;
}

// ── OwlCtx ────────────────────────────────────────────────────────────────────

/// A context capable of hydrating an OGIT URI to an OWL class.
///
/// Unlocks the `hydrate_owl` verb on
/// [`NormalizedEntity<WithOgit>`](crate::cognition::entity::NormalizedEntity).
///
// TODO(Stage 2): Stage 2 wires the concrete implementation to perform a TTL
// join on the OGIT ontology graph (the OWL TTL is available after
// the EXT-1 extraction). The join result is an OWL class IRI stored
// as a static-str handle in the `owl` field.
pub trait OwlCtx: Context {
    /// Hydrate an OGIT URI to its OWL class via TTL join.
    fn hydrate_owl(&self, ogit_uri: OgitUriRef) -> OwlClassRef;
}

// ── DolceCtx ──────────────────────────────────────────────────────────────────

/// A context capable of classifying an OWL class into a DOLCE category.
///
/// Unlocks the `classify_dolce` verb on
/// [`NormalizedEntity<WithOwl>`](crate::cognition::entity::NormalizedEntity).
///
// TODO(Stage 2): Stage 2 wires the concrete implementation to dispatch into
// `lance_graph_ontology::dolce_odoo::DolceClassifier` (already
// shipped in the EXT-2..6 extraction).
pub trait DolceCtx: Context {
    /// Classify an OWL class into a [`DolceCategory`].
    fn classify_dolce(&self, owl_class: OwlClassRef) -> DolceCategory;
}

// ── FibuCtx ───────────────────────────────────────────────────────────────────

/// A context capable of aligning a DOLCE category to a FIBU/FIBO frame.
///
/// Unlocks the `align_fibu` verb on
/// [`NormalizedEntity<WithDolce>`](crate::cognition::entity::NormalizedEntity).
///
/// The FIBU/FIBO alignment overlay maps DOLCE categories + Odoo model
/// names to German accounting frames (Kontenerkennung, SKR03/SKR04,
/// UStVA Kennzahlen, GoBD wiring).
///
// TODO(Stage 2): Stage 2 wires the concrete implementation using the D-ODOO-EXT-1..6
// Kontenerkennung tables (1274 + 1192 SKR account templates, 37
// UStVA Kennzahlen). The alignment is a static table lookup keyed
// on (DolceCategory, OdooEntityRef).
pub trait FibuCtx: Context {
    /// Align a (DOLCE category, Odoo entity) pair to a FIBU/FIBO frame.
    fn align_fibu(
        &self,
        dolce: DolceCategory,
        odoo: crate::cognition::entity::OdooEntityRef,
    ) -> FibuAlignmentRef;
}
