//! `facet_schema` — the classid-selected **format** of a [`FacetCascade`]'s 12
//! payload bytes.
//!
//! A [`FacetCascade`] is `facet_classid(4) | 12 payload bytes`. The substrate is
//! content-blind (see [`crate::facet`]); the `{domain}{schema}` `facet_classid`
//! selects how a consumer reads those 12 bytes. `96 bits` tiles three ways, all
//! exact:
//!
//! | schema | tiling | meaning | precedent |
//! |---|---|---|---|
//! | [`FacetSchema::TierCascade`] | `6 × (8:8)` | `(part_of:is_a)` cascade | the existing [`FacetCascade::tiers`] / `hi_chain` / `lo_chain` |
//! | [`FacetSchema::SpoTriplet`] | `4 × (8:8:8)` | `(subject:predicate:object)` SPO edges | the `ruff_spo_*` triple corpus |
//! | [`FacetSchema::Pair48`] | `2 × 48-bit` | two 6-byte codes | `helix` `Signed360` / `cam_pq` `[u8; 6]` (both already 48-bit) |
//!
//! These are **readings of the same bytes** — re-tiling, not re-encoding — so
//! switching schema never moves a byte and never touches the operator-LOCKED
//! 480-byte value slab.

use crate::facet::FacetCascade;

/// The classid-selected reading of a facet's 12 payload bytes.
///
/// `TierCascade` is the default (`== 0`), so every existing facet keeps its
/// current `6 × (8:8)` meaning unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum FacetSchema {
    /// `6 × (8:8)` — the `(part_of:is_a)` tier cascade. Default.
    #[default]
    TierCascade = 0,
    /// `4 × (8:8:8)` — `(subject:predicate:object)` SPO triplets.
    SpoTriplet = 1,
    /// `2 × 48-bit` — two contiguous 6-byte codes (`helix` / `cam_pq` shape).
    Pair48 = 2,
}

impl FacetSchema {
    /// Read the schema field from a `{domain}{schema}` `facet_classid`.
    ///
    /// **Provisional field position.** The exact bits of the `schema` sub-field
    /// within `facet_classid` are pending operator/panel ratification; until
    /// then this reads the low two bits of the high byte and **defaults to
    /// [`TierCascade`](Self::TierCascade)** for every other value, so no
    /// existing facet changes meaning. Callers that know their schema should
    /// prefer the explicit `as_*` / `from_*` accessors on [`FacetCascade`].
    #[inline]
    #[must_use]
    pub const fn of_classid(facet_classid: u32) -> Self {
        match (facet_classid >> 24) & 0b11 {
            1 => Self::SpoTriplet,
            2 => Self::Pair48,
            _ => Self::TierCascade,
        }
    }
}

impl FacetCascade {
    /// The 12 payload bytes (everything after the 4-byte `facet_classid`), LE.
    #[inline]
    #[must_use]
    pub const fn payload(self) -> [u8; 12] {
        let b = self.to_bytes();
        [
            b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15],
        ]
    }

    /// The classid-selected [`FacetSchema`] of this facet (provisional resolver,
    /// see [`FacetSchema::of_classid`]).
    #[inline]
    #[must_use]
    pub const fn schema(self) -> FacetSchema {
        FacetSchema::of_classid(self.facet_classid)
    }

    /// The `4 × 3` SPO-triplet reading: `[[subject, predicate, object]; 4]`.
    #[inline]
    #[must_use]
    pub const fn as_triplets(self) -> [[u8; 3]; 4] {
        let p = self.payload();
        [
            [p[0], p[1], p[2]],
            [p[3], p[4], p[5]],
            [p[6], p[7], p[8]],
            [p[9], p[10], p[11]],
        ]
    }

    /// The `2 × 48-bit` reading: two contiguous 6-byte codes (`helix` / `cam_pq`).
    #[inline]
    #[must_use]
    pub const fn as_pair48(self) -> [[u8; 6]; 2] {
        let p = self.payload();
        [
            [p[0], p[1], p[2], p[3], p[4], p[5]],
            [p[6], p[7], p[8], p[9], p[10], p[11]],
        ]
    }

    /// Build a facet from a `facet_classid` + `4 × 3` SPO triplets.
    #[inline]
    #[must_use]
    pub const fn from_triplets(facet_classid: u32, t: [[u8; 3]; 4]) -> Self {
        let cid = facet_classid.to_le_bytes();
        let b = [
            cid[0], cid[1], cid[2], cid[3], t[0][0], t[0][1], t[0][2], t[1][0], t[1][1], t[1][2],
            t[2][0], t[2][1], t[2][2], t[3][0], t[3][1], t[3][2],
        ];
        FacetCascade::from_bytes(&b)
    }

    /// Build a facet from a `facet_classid` + `2 × 48-bit` codes.
    #[inline]
    #[must_use]
    pub const fn from_pair48(facet_classid: u32, pair: [[u8; 6]; 2]) -> Self {
        let cid = facet_classid.to_le_bytes();
        let b = [
            cid[0], cid[1], cid[2], cid[3], pair[0][0], pair[0][1], pair[0][2], pair[0][3],
            pair[0][4], pair[0][5], pair[1][0], pair[1][1], pair[1][2], pair[1][3], pair[1][4],
            pair[1][5],
        ];
        FacetCascade::from_bytes(&b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triplet_roundtrip_preserves_classid_and_bytes() {
        let t = [[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]];
        let f = FacetCascade::from_triplets(0x00AB_CDEF, t);
        assert_eq!(f.facet_classid, 0x00AB_CDEF);
        assert_eq!(f.as_triplets(), t);
    }

    #[test]
    fn pair48_roundtrip_preserves_classid_and_bytes() {
        let pair = [[1, 2, 3, 4, 5, 6], [7, 8, 9, 10, 11, 12]];
        let f = FacetCascade::from_pair48(0x0000_1234, pair);
        assert_eq!(f.facet_classid, 0x0000_1234);
        assert_eq!(f.as_pair48(), pair);
    }

    #[test]
    fn payload_is_the_twelve_bytes_after_classid() {
        let f = FacetCascade::from_bytes(&[
            0xEF, 0xCD, 0xAB, 0x00, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
        ]);
        assert_eq!(f.payload(), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    }

    #[test]
    fn schema_defaults_to_tier_cascade() {
        // High byte low-2-bits == 0 → TierCascade (every existing facet).
        assert_eq!(
            FacetSchema::of_classid(0x0012_3456),
            FacetSchema::TierCascade
        );
        assert_eq!(FacetSchema::default(), FacetSchema::TierCascade);
    }

    #[test]
    fn re_tilings_are_the_same_twelve_bytes() {
        let f = FacetCascade::from_bytes(&[0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        let from_trip: Vec<u8> = f.as_triplets().concat();
        let from_pair: Vec<u8> = f.as_pair48().concat();
        assert_eq!(from_trip, from_pair);
        assert_eq!(from_trip, f.payload().to_vec());
    }
}
