//! # `view_angle` — the attention-angle selector (zero-dep).
//!
//! The class-inherited **presence bitmask** ([`crate::class_view::FieldMask`]) does
//! double duty: it says *which fields are populated* (presence) and therefore *which
//! to attend* (attention) — "attend to what's present" is **structural**, never the
//! forbidden per-instance semantics (`class_view` C2: presence ≠ semantics).
//!
//! A [`ViewAngle`] (≤ 16, a 4-bit nibble) selects **which inherited view-schema
//! attends** — the Quartettkarte "edition" / FAISS-view over the same card. The
//! view-schema for each angle is declared *in the OGIT class* (a leaf/family can bake
//! in N required default angles); resolution flies ABOVE the row:
//!
//! ```text
//! attention(row, angle) = class.view_schema(angle)  &  row.presence_bitmask
//!                         └── inherited (OGIT) ──┘     └── per-row presence ──┘
//! ```
//!
//! **The line that keeps it RISC:** an angle selects an *inherited* attention
//! pattern; it must NEVER mean something different per row (`class_view` C2).
//!
//! `head2head` ([`crate::head2head`]) competes angles — `DissonanceMin` ≈ infight,
//! `SupportSpread` ≈ Raumgewinn — picking which lens wins for a story.

/// A 4-bit view-schema selector (`0..16`). The *meaning* of each angle is declared in
/// the OGIT class, not here; this is only the agnostic index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ViewAngle(u8);

impl ViewAngle {
    /// Number of distinct angles a 4-bit selector addresses.
    pub const MAX: u8 = 16;

    /// A validated angle, or `None` if `angle ≥ 16`.
    #[must_use]
    pub const fn new(angle: u8) -> Option<Self> {
        if angle < Self::MAX {
            Some(Self(angle))
        } else {
            None
        }
    }

    /// The canonical / default view (angle `0`) — the view every shape answers.
    #[must_use]
    pub const fn canonical() -> Self {
        Self(0)
    }

    /// The raw 0-based angle index (to key the OGIT class's per-angle view-schema).
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_bounds_to_four_bits() {
        assert!(ViewAngle::new(0).is_some());
        assert!(ViewAngle::new(15).is_some());
        assert!(ViewAngle::new(16).is_none());
    }

    #[test]
    fn canonical_is_angle_zero_and_default() {
        assert_eq!(ViewAngle::canonical().index(), 0);
        assert_eq!(ViewAngle::default(), ViewAngle::canonical());
    }

    #[test]
    fn index_round_trips() {
        for a in 0..ViewAngle::MAX {
            assert_eq!(ViewAngle::new(a).unwrap().index(), a);
        }
    }
}
