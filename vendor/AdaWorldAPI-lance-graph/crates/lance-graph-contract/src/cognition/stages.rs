//! Phantom-typed stage markers; advancement is compile-time-checked.
//!
//! Consumers cannot introduce new stages: the [`Stage`] trait is sealed
//! via the private [`sealed::Sealed`] supertrait. Only the nine canonical
//! stages defined in this module are valid `Stage` implementations.
//!
//! ## Stage ordering
//!
//! ```text
//! Raw → WithOgit → WithOwl → WithDolce → Normalized
//!                                              ↓
//!                                           Checked
//!                                              ↓
//!                                           Reviewed
//!                                              ↓
//!                                           Abducted
//!                                              ↓
//!                                           Reported   (terminal)
//! ```
//!
//! Advancement is gated by the [`super::advance`] methods; each method
//! is only defined on the appropriate input stage, so the compiler
//! enforces ordering at call sites.
//!
//! ## Cross-references
//! - Plan: `.claude/plans/normalized-entity-holy-grail-v1.md`
//! - Epiphany: E-NORMALIZED-ENTITY-1

/// Pre-resolution stage — `NormalizedEntity` holds only the Odoo model_name.
pub struct Raw;

/// OGIT URI resolved from the Odoo model_name via the codebook.
pub struct WithOgit;

/// OWL class hydrated from the OGIT URI via TTL join.
pub struct WithOwl;

/// DOLCE upper-ontology category classified from the OWL class.
pub struct WithDolce;

/// Fully normalized — all four inheritance slots populated; chain-ready.
pub struct Normalized;

/// Post `chk_data` — data-quality checks have passed.
pub struct Checked;

/// Post `review` — fiscal-position / savant review has passed.
pub struct Reviewed;

/// Post `abduct` — NARS abductive inference has been applied.
pub struct Abducted;

/// Post `report` — aggregation/reporting is complete; chain is terminal.
pub struct Reported;

/// Sealed stage marker. Implementing types: exactly the nine above.
///
/// Consumers CANNOT implement this trait for their own types; that would
/// allow forged stage transitions outside the typed algebra.
pub trait Stage: 'static + Sized + sealed::Sealed {}

impl Stage for Raw {}
impl Stage for WithOgit {}
impl Stage for WithOwl {}
impl Stage for WithDolce {}
impl Stage for Normalized {}
impl Stage for Checked {}
impl Stage for Reviewed {}
impl Stage for Abducted {}
impl Stage for Reported {}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Raw {}
    impl Sealed for super::WithOgit {}
    impl Sealed for super::WithOwl {}
    impl Sealed for super::WithDolce {}
    impl Sealed for super::Normalized {}
    impl Sealed for super::Checked {}
    impl Sealed for super::Reviewed {}
    impl Sealed for super::Abducted {}
    impl Sealed for super::Reported {}
}
