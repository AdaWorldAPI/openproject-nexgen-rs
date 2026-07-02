//! Savant role-key catalogue — **desperation-bucket fallback** identity
//! layer per the 2026-05-28 codebook doctrine (see
//! `E-CODEBOOK-INHERITS-FROM-OGIT` in `EPIPHANIES.md`).
//!
//! ## Status: not the canonical identity
//!
//! The canonical savant identity is the OGIT URI in [`super::ogit_uris`],
//! resolved through `lance-graph-ontology::registry::OntologyRegistry`
//! to a stable codebook row index. LE-byte mailbox SoA columns store
//! the row index — **never the bitpacked bits here**.
//!
//! This module exists as the documented fallback for compute contexts
//! where codebook lookup is unavailable (e.g. ephemeral in-mailbox
//! Hamming compare against a pre-bound `RoleKey`). Per the
//! 2026-05-28 doctrine: *"bitpacked is also only a desperation
//! bucket."*
//!
//! ## What this module ships
//!
//! 25 disjoint [`RoleKey`] slices, one per Odoo savant in
//! [`crate::savants::SAVANTS`], landing in the SMB headroom
//! `[14096..16346)` (FNV-64-seeded pseudo-random bipolar bits, 90 dims
//! each, disjoint by construction).
//!
//! ## Layout
//!
//! ```text
//! [14096 .. 14186)   savant 1 (FiscalPositionResolver)
//! [14186 .. 14276)   savant 2 (PartnerTrustAdvisor)
//! [14276 .. 14366)   savant 3 (PricelistAssignmentAgent)
//! ...
//! [16256 .. 16346)   savant 25 (BackorderJudge, roster id 26 — id 16 absent)
//! [16346 .. 16384)   38 dims headroom (reserved for future callcenter keys)
//! ```
//!
//! Total footprint: 25 × 90 = 2250 dims, within the 2288-dim SMB
//! headroom (`grammar::role_keys::VSA_DIMS = 16_384` minus STEUER_KEY's
//! end at `14_096`).
//!
//! `D-ODOO-SAV-5b` of `odoo-savant-reasoners-v2` (the desperation-bucket
//! fallback; the canonical OGIT codebook foundation is `D-ODOO-SAV-5b-v2`
//! in [`super::ogit_uris`]).

use std::sync::LazyLock;

use crate::grammar::role_keys::RoleKey;
use crate::savants::{savant, savant_by_name, Savant, SAVANTS};

/// Start of the savant identity zone — directly after STEUER_KEY's
/// `[13584..14096)` slice in [`crate::grammar::role_keys`].
pub const SAVANT_SLICE_START: usize = 14_096;

/// One identity slice per savant — 90 dims of FNV-seeded pseudo-random
/// bits. 25 × 90 = 2250 dims < the 2288-dim SMB headroom.
pub const SAVANT_SLICE_WIDTH: usize = 90;

/// End of the savant identity zone: `SAVANT_SLICE_START + 25 *
/// SAVANT_SLICE_WIDTH = 16_346`. 38 dims of headroom remain.
pub const SAVANT_SLICE_END: usize = SAVANT_SLICE_START + 25 * SAVANT_SLICE_WIDTH;

/// The 25 savant role keys in roster order (same order as
/// [`crate::savants::SAVANTS`]).
///
/// Indexing matches roster index `i` (NOT savant `id`; roster id 16 is
/// intentionally absent per `SAVANTS.md`, so use the lookup helpers
/// [`savant_role_key`] / [`savant_role_key_by_name`] rather than direct
/// indexing).
pub static SAVANT_ROLE_KEYS: LazyLock<[RoleKey; 25]> = LazyLock::new(|| {
    core::array::from_fn(|i| {
        let s: &Savant = &SAVANTS[i];
        let start = SAVANT_SLICE_START + i * SAVANT_SLICE_WIDTH;
        let end = start + SAVANT_SLICE_WIDTH;
        RoleKey::generate(s.name, start, end)
    })
});

/// Look up a savant's role key by roster id (1..=15, 17..=26; id 16
/// intentionally absent).
///
/// Returns `None` if the id does not appear in [`SAVANTS`].
pub fn savant_role_key(id: u8) -> Option<&'static RoleKey> {
    let _ = savant(id)?;
    SAVANTS
        .iter()
        .position(|s| s.id == id)
        .map(|i| &SAVANT_ROLE_KEYS[i])
}

/// Look up a savant's role key by name (the `SAVANTS[i].name`).
///
/// Returns `None` if the name does not appear in [`SAVANTS`].
pub fn savant_role_key_by_name(name: &str) -> Option<&'static RoleKey> {
    let _ = savant_by_name(name)?;
    SAVANTS
        .iter()
        .position(|s| s.name == name)
        .map(|i| &SAVANT_ROLE_KEYS[i])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::role_keys::VSA_DIMS;

    #[test]
    fn slices_disjoint_and_in_bounds() {
        for (i, key) in SAVANT_ROLE_KEYS.iter().enumerate() {
            let expected_start = SAVANT_SLICE_START + i * SAVANT_SLICE_WIDTH;
            let expected_end = expected_start + SAVANT_SLICE_WIDTH;
            assert_eq!(key.slice_start, expected_start, "savant index {i}");
            assert_eq!(key.slice_end, expected_end, "savant index {i}");
            assert_eq!(key.slice_width(), SAVANT_SLICE_WIDTH);
            assert!(key.slice_end <= VSA_DIMS, "savant {i} fits VSA space");
        }
    }

    #[test]
    fn savant_zone_fits_in_smb_headroom() {
        // grammar::role_keys ends at SMB STEUER_KEY = 14_096 and reserves
        // [14_096 .. 16_384) (= 2288 dims) as headroom. We claim 2250 of
        // those 2288 dims; 38 remain.
        assert_eq!(SAVANT_SLICE_END, 16_346);
        const { assert!(SAVANT_SLICE_END <= VSA_DIMS) };
        assert_eq!(VSA_DIMS - SAVANT_SLICE_END, 38, "headroom remaining");
    }

    #[test]
    fn id_lookup_matches_name_lookup() {
        // FiscalPositionResolver is savant id 1, roster index 0.
        let by_id = savant_role_key(1).expect("id 1");
        let by_name = savant_role_key_by_name("FiscalPositionResolver").expect("name");
        assert_eq!(by_id.label, by_name.label);
        assert_eq!(by_id.slice_start, by_name.slice_start);
        assert_eq!(by_id.slice_start, SAVANT_SLICE_START, "first roster slot");
    }

    #[test]
    fn id_16_absent() {
        // SAVANTS.md skips roster id 16. Lookup must return None.
        assert!(savant_role_key(16).is_none());
    }

    #[test]
    fn last_savant_is_backorder_judge() {
        // BackorderJudge has roster id 26 and lives at roster index 24
        // (the 25th and final savant).
        let key = savant_role_key(26).expect("id 26");
        assert_eq!(key.label, "BackorderJudge");
        assert_eq!(
            key.slice_start,
            SAVANT_SLICE_START + 24 * SAVANT_SLICE_WIDTH
        );
        assert_eq!(key.slice_end, SAVANT_SLICE_END);
    }

    #[test]
    fn deterministic_pseudo_random_bits() {
        // FNV-64-seeded from the savant's name: same name → same bits.
        // 90-dim slice: roughly half the bits should be set (the LCG is
        // unbiased over a 90-bit window).
        let key = savant_role_key_by_name("FiscalPositionResolver").unwrap();
        let total_set: u32 = key.words.iter().map(|w| w.count_ones()).sum();
        assert!(total_set > 20, "some bits set in 90-dim slice: {total_set}");
        assert!(
            total_set < 80,
            "some bits clear in 90-dim slice: {total_set}"
        );
    }

    #[test]
    fn no_overlap_with_grammar_slices() {
        // grammar::role_keys SMB keys end at STEUER_KEY's `14_096`; savants
        // start at `14_096`. SPO core roles and TEKAMOLO slots all sit
        // below `14_096`, so no overlap by construction.
        use crate::grammar::role_keys::{OBJECT_KEY, PREDICATE_KEY, SUBJECT_KEY};
        assert!(SUBJECT_KEY.slice_end <= SAVANT_SLICE_START);
        assert!(PREDICATE_KEY.slice_end <= SAVANT_SLICE_START);
        assert!(OBJECT_KEY.slice_end <= SAVANT_SLICE_START);
    }
}
