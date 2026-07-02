//! callcenter-domain Layer-2 catalogue (per `I-VSA-IDENTITIES`).
//!
//! Sibling of [`crate::grammar::role_keys`] and the future
//! `persona::role_keys`: one identity fingerprint per concept, with
//! disjoint slice allocations. The 25 Odoo savants from
//! [`crate::savants::SAVANTS`] land here as the first set of
//! callcenter-domain identities.
//!
//! ## Two identity layers (2026-05-28 codebook doctrine)
//!
//! 1. **[`ogit_uris`] — canonical**: OGIT URI per savant, resolved
//!    through `lance-graph-ontology::registry::OntologyRegistry` to a
//!    stable row index. LE-byte mailbox SoA columns store the row
//!    index — the codebook is inherited from OGIT, the SoA doesn't
//!    guess.
//! 2. **[`role_keys`] — desperation-bucket fallback**: Binary16K u64
//!    bitpacked role-key slices for compute contexts where the codebook
//!    is unavailable. **Not the canonical identity** — useful only
//!    for ephemeral in-mailbox Hamming compare.
//!
//! See `.claude/knowledge/vsa-switchboard-architecture.md` for the
//! three-layer Layer-2 catalogue doctrine and
//! `.claude/plans/odoo-savant-reasoners-v2.md` for the broader
//! composition-over-substrate reshape this module participates in.

pub mod ogit_uris;
pub mod role_keys;

pub use ogit_uris::{savant_ogit_uri, savant_ogit_uri_by_name, SAVANT_OGIT_BASE, SAVANT_OGIT_URIS};
pub use role_keys::{
    savant_role_key, savant_role_key_by_name, SAVANT_ROLE_KEYS, SAVANT_SLICE_END,
    SAVANT_SLICE_START, SAVANT_SLICE_WIDTH,
};
