//! Canonical OGIT-URI codebook entries for the 25 Odoo savants — the
//! deterministic identity layer per the 2026-05-28 doctrine: **"LE-byte
//! SoA per mailbox carries codebook entries inherited from OGIT; the
//! SoA doesn't guess."**
//!
//! ## Layered identity
//!
//! Per `I-VSA-IDENTITIES` + the 2026-05-28 codebook-canonical doctrine,
//! a savant has three identity layers:
//!
//! 1. **Canonical (this module): OGIT URI** — namespaced under
//!    `https://ogit.adaworldapi.com/callcenter/savants#<Name>`. Resolved
//!    through [`lance-graph-ontology::registry::OntologyRegistry`] to a
//!    stable row index (codebook code). LE-byte mailbox SoA columns
//!    store the row index, **never** raw bits.
//! 2. **Content (in [`crate::savants`]): `Savant` struct** — dispatch
//!    tuple `(id, name, family, kind, inference, semiring, style, lane,
//!    decides)`. Read-side metadata, not identity.
//! 3. **Desperation-bucket fallback (in [`super::role_keys`]):
//!    `RoleKey` slice** — Binary16K bitpacked u64 words in
//!    `[14096..16346)`. Useful for compute contexts where codebook
//!    lookup is unavailable (e.g. ephemeral in-mailbox Hamming
//!    compare); **not the canonical identity**. The 2026-05-28
//!    doctrine: "bitpacked is also only a desperation bucket."
//!
//! ## What this module ships (`D-ODOO-SAV-5b-v2` step 1)
//!
//! - [`SAVANT_OGIT_BASE`] — the namespace IRI all 25 URIs sit under.
//! - [`SAVANT_OGIT_URIS`] — `LazyLock<[String; 25]>` with one URI per
//!   savant in roster order (same order as [`crate::savants::SAVANTS`]).
//! - [`savant_ogit_uri`] / [`savant_ogit_uri_by_name`] — lookup helpers
//!   that round-trip with [`crate::savants::savant`] /
//!   [`crate::savants::savant_by_name`].
//!
//! ## What lands next
//!
//! - **Step 2** (out of scope this commit): a `data/ontologies/ogit/
//!   callcenter/savants.ttl` TTL declaring each savant as an
//!   `ogit:Savant` with its dispatch tuple (kind, inference, semiring,
//!   style, lane, decides) — hydrated through the existing
//!   `OwlHydrator` + `OntologyRegistry::hydrate_from_*` path.
//! - **Step 3:** `MappingProposal` registration so the per-tenant
//!   `NamespaceBridge` projections resolve the savant URIs into the
//!   appropriate tenant scope.
//! - **Step 4:** Kontenerkennung-style inheritance + NARS-truth
//!   confidence per inheritance link — the multi-dimensional dispatch
//!   surface (business × transaction × form × regulation × law × entity
//!   × product) the user named on 2026-05-28. Lands in a separate
//!   module sitting next to the codebook resolver, with each savant's
//!   `Savant::decides` becoming the leaf in a typed inheritance tree.

use std::sync::LazyLock;

use crate::savants::{savant, savant_by_name, SAVANTS};

/// Base OGIT IRI for the callcenter-savants namespace.
///
/// One stable codebook namespace; every savant URI is
/// `format!("{SAVANT_OGIT_BASE}{savant_name}")`.
pub const SAVANT_OGIT_BASE: &str = "https://ogit.adaworldapi.com/callcenter/savants#";

/// One canonical OGIT URI per savant, in roster order (same order as
/// [`crate::savants::SAVANTS`]).
///
/// Indexing matches roster index `i` (NOT savant `id`; roster id 16 is
/// intentionally absent per `SAVANTS.md`). Use the lookup helpers
/// [`savant_ogit_uri`] / [`savant_ogit_uri_by_name`] rather than
/// indexing this array directly.
pub static SAVANT_OGIT_URIS: LazyLock<[String; 25]> =
    LazyLock::new(|| core::array::from_fn(|i| format!("{}{}", SAVANT_OGIT_BASE, SAVANTS[i].name)));

/// Look up a savant's canonical OGIT URI by roster id.
///
/// Returns `None` if the id does not appear in [`SAVANTS`] (e.g. the
/// intentionally-absent id 16).
pub fn savant_ogit_uri(id: u8) -> Option<&'static str> {
    let _ = savant(id)?;
    SAVANTS
        .iter()
        .position(|s| s.id == id)
        .map(|i| SAVANT_OGIT_URIS[i].as_str())
}

/// Look up a savant's canonical OGIT URI by name.
///
/// Returns `None` if the name does not match any savant in
/// [`SAVANTS`].
pub fn savant_ogit_uri_by_name(name: &str) -> Option<&'static str> {
    let _ = savant_by_name(name)?;
    SAVANTS
        .iter()
        .position(|s| s.name == name)
        .map(|i| SAVANT_OGIT_URIS[i].as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_match_savant_count() {
        assert_eq!(SAVANT_OGIT_URIS.len(), 25);
    }

    #[test]
    fn uris_use_canonical_namespace() {
        for uri in SAVANT_OGIT_URIS.iter() {
            assert!(
                uri.starts_with(SAVANT_OGIT_BASE),
                "uri must sit under canonical namespace: {uri}"
            );
        }
    }

    #[test]
    fn id_1_resolves_to_fiscal_position_resolver() {
        let uri = savant_ogit_uri(1).expect("id 1");
        assert_eq!(
            uri,
            "https://ogit.adaworldapi.com/callcenter/savants#FiscalPositionResolver"
        );
    }

    #[test]
    fn id_16_is_absent() {
        assert!(
            savant_ogit_uri(16).is_none(),
            "roster id 16 intentionally absent"
        );
    }

    #[test]
    fn id_26_resolves_to_backorder_judge() {
        let uri = savant_ogit_uri(26).expect("id 26");
        assert!(
            uri.ends_with("BackorderJudge"),
            "id 26 should be BackorderJudge: {uri}"
        );
    }

    #[test]
    fn id_lookup_matches_name_lookup() {
        let by_id = savant_ogit_uri(1).expect("id 1");
        let by_name = savant_ogit_uri_by_name("FiscalPositionResolver").expect("name lookup");
        assert_eq!(by_id, by_name);
    }

    #[test]
    fn nonexistent_name_returns_none() {
        assert!(savant_ogit_uri_by_name("DoesNotExist").is_none());
    }

    #[test]
    fn every_savant_in_roster_has_a_uri() {
        for s in SAVANTS.iter() {
            let uri = savant_ogit_uri(s.id).expect("every roster savant has a uri");
            assert!(
                uri.ends_with(s.name),
                "uri ends with savant name: {uri} vs {}",
                s.name
            );
        }
    }
}
