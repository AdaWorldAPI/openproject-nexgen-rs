//! # `episodic_edges` — AriGraph episodic edges, RISC-encoded (zero-dep).
//!
//! EW64 is **AriGraph's episodic edges** — a mailbox(=episode) is a *basin* with
//! *multiple* edges (NOT a lens over one `CausalEdge64`). This is the witness/
//! relational concern, SoC'd from both the temporal arc (a basin one HHTL level up)
//! and frozen identity (CAM/OGIT).
//!
//! ## Cost model (grounded by the #444 locality probe)
//! - **~98.6% intra-basin** (probe): an edge stays in the row's own family, which is
//!   **inherited** from the HHTL/`class_id` path → ~0 extra bits. `EdgeRef::family == 0`.
//! - **~1.4% cross-family** (the crossover): a **4-bit nibble** (16 families,
//!   `family ∈ 1..=15`) indexes the **OGIT-class-inherited cross-family palette** — a
//!   CAM_PQ facet code whose codebook is the class's declared closed range
//!   (`owl:disjointWith` ⇒ collision-free). The 16 *identities* live in the class,
//!   **never on the edge** (`I-VSA-IDENTITIES`: point, don't copy). Probe fan-out
//!   ≤ 3 ⇒ 4 bits (16) has headroom.
//!
//! ## Layout — `EpisodicEdges64(u64)` = 4 × `u16` slots
//! Each slot: `0x0000` = empty; else `[bits 12-15: family nibble][bits 0-11: local]`.
//! `local` is a **1-based within-family index** (`1..=4095`); the resolved family is
//! the row's own basin (`family == 0`, inherited) or `class.cross_family_palette[family]`
//! (`1..=15`). Cross-session reach is a *separate* 16-bit episode-store column, not this
//! word. Identity resolution flies ABOVE the row (the OGIT class), as `class_view` does.

// The slot pack/unpack does intentional nibble extraction (slot>>12 ∈ 0..=15) and
// low-16-bit reads (u64 -> u16); both are provably-bounded narrowings.
// cast_possible_truncation: intentional bounded narrowings (nibble slot>>12 ∈ 0..=15;
// low-16-bit slot reads). doc_markdown: domain acronyms (AriGraph/OGIT/CAM_PQ/SoC) read
// better unbackticked in this module's prose.
#![allow(clippy::cast_possible_truncation, clippy::doc_markdown)]

/// One episodic edge: a `(family, local)` reference in the episodic basin space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeRef {
    /// Cross-family selector. `0` = **intra-basin** (the row's own family, inherited
    /// from the HHTL/`class_id` path — the ~98.6% common case). `1..=15` = a
    /// cross-family index into the OGIT-class-inherited palette (the ~1.4% crossover).
    pub family: u8,
    /// 1-based within-family local index (`1..=4095`); `0` is the empty-slot sentinel.
    pub local: u16,
}

impl EdgeRef {
    /// Family count addressable by the 4-bit nibble (probe fan-out ≤ 3 ⇒ headroom).
    pub const FAMILIES: u8 = 16;
    /// Max 1-based within-family local index (12 bits).
    pub const MAX_LOCAL: u16 = 0x0FFF;

    /// A validated edge, or `None` if `family ≥ 16` or `local ∉ 1..=4095`.
    #[must_use]
    pub const fn new(family: u8, local: u16) -> Option<Self> {
        if family < Self::FAMILIES && local >= 1 && local <= Self::MAX_LOCAL {
            Some(Self { family, local })
        } else {
            None
        }
    }

    /// An **intra-basin** edge (`family == 0`, the inherited common case).
    #[must_use]
    pub const fn intra(local: u16) -> Option<Self> {
        Self::new(0, local)
    }

    /// A **cross-family** edge into palette index `family ∈ 1..=15`.
    #[must_use]
    pub const fn cross(family: u8, local: u16) -> Option<Self> {
        if family == 0 {
            None
        } else {
            Self::new(family, local)
        }
    }

    /// Does this edge cross to another family (vs. staying intra-basin)?
    #[must_use]
    pub const fn is_cross(self) -> bool {
        self.family != 0
    }

    fn to_slot(self) -> u16 {
        (u16::from(self.family) << 12) | (self.local & Self::MAX_LOCAL)
    }

    const fn from_slot(slot: u16) -> Option<Self> {
        if slot == 0 {
            None
        } else {
            Some(Self {
                family: (slot >> 12) as u8,
                local: slot & Self::MAX_LOCAL,
            })
        }
    }
}

/// Up to 4 AriGraph episodic edges packed into one `u64` (4 × 16-bit slots).
///
/// The witness/relational column of the per-row SoA: which other basin members this
/// episode touched. Agnostic — the nibble's *meaning* resolves in the OGIT class, not
/// here. `Default` / [`EpisodicEdges64::empty`] is the no-edge word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EpisodicEdges64(pub u64);

impl EpisodicEdges64 {
    /// Edge slots per word (4 × 16 bits = 64).
    pub const CAPACITY: usize = 4;

    /// The empty (no-edge) word.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The edge in slot `i` (`0..4`), or `None` if the slot is empty / out of range.
    #[must_use]
    pub const fn edge(self, i: usize) -> Option<EdgeRef> {
        if i >= Self::CAPACITY {
            return None;
        }
        EdgeRef::from_slot((self.0 >> (i * 16)) as u16)
    }

    /// How many slots carry an edge.
    #[must_use]
    pub fn count(self) -> usize {
        (0..Self::CAPACITY)
            .filter(|&i| self.edge(i).is_some())
            .count()
    }

    /// All 4 slots full?
    #[must_use]
    pub fn is_full(self) -> bool {
        self.count() == Self::CAPACITY
    }

    /// Place `e` into the first empty slot; `None` if the word is already full.
    #[must_use]
    pub fn push(self, e: EdgeRef) -> Option<Self> {
        let mut i = 0;
        while i < Self::CAPACITY {
            if self.edge(i).is_none() {
                let shift = i * 16;
                let cleared = self.0 & !(0xFFFF_u64 << shift);
                return Some(Self(cleared | (u64::from(e.to_slot()) << shift)));
            }
            i += 1;
        }
        None
    }

    /// MRU **promote** — strengthen `e` by moving it to slot 0, the *strongest /
    /// most-immediate* position (`E-EW64-STRENGTH-IS-CE64-PLASTICITY`).
    ///
    /// If `e` is already present it moves to the front; otherwise it is inserted
    /// at the front and the survivors shift down by one. When 4 *distinct* edges
    /// are already present and `e` is new, the **coldest** (slot 3) is evicted —
    /// demoted to the cold connectome (returned as the second tuple element).
    ///
    /// This is the Hebbian "fire together → wire together" at the hot tier: a
    /// fired edge becomes the strongest; un-refired edges age toward slot 3 and
    /// out. **Slot order is the strength ranking** (slot 0 = hottest); no per-edge
    /// weight is stored here — the co-addressed `CausalEdge64` plasticity carries
    /// the Hebbian weight, recency is the slot index. Idempotent on the already-
    /// hottest edge (re-firing slot 0 of a full word changes nothing).
    #[must_use]
    pub fn promote(self, e: EdgeRef) -> (Self, Option<EdgeRef>) {
        let mut slots = [0u16; Self::CAPACITY];
        slots[0] = e.to_slot();
        let mut filled = 1usize;
        let mut evicted = None;
        let mut i = 0;
        while i < Self::CAPACITY {
            if let Some(x) = self.edge(i) {
                if x != e {
                    if filled < Self::CAPACITY {
                        slots[filled] = x.to_slot();
                        filled += 1;
                    } else {
                        // Word was full of distinct edges and `e` is new: the
                        // coldest survivor (encountered last, slot 3) is evicted.
                        evicted = Some(x);
                    }
                }
            }
            i += 1;
        }
        let mut raw = 0u64;
        let mut s = 0;
        while s < Self::CAPACITY {
            raw |= u64::from(slots[s]) << (s * 16);
            s += 1;
        }
        (Self(raw), evicted)
    }

    /// [`promote`](Self::promote) `e`, routing any eviction to `sink` (the cold
    /// connectome). Returns the new word. When a fresh edge displaces a full
    /// word the sink receives exactly the coldest edge (= [`coldest`](Self::coldest));
    /// no eviction (non-full word, or re-firing a present edge) → sink untouched.
    /// This is the hot tier's defined exit into the cold tier — the contract-side
    /// half of `E-SUBSTRATE-IS-THE-SCHEDULER` (the surreal/LanceDB-LIVE wingman
    /// implements [`DemotionSink`], gated on OQ-11.6).
    #[must_use]
    pub fn promote_into(self, e: EdgeRef, sink: &mut impl DemotionSink) -> Self {
        let (next, evicted) = self.promote(e);
        if let Some(victim) = evicted {
            sink.demote(victim);
        }
        next
    }

    /// The strongest / most-immediate edge (slot 0 under the MRU invariant), or
    /// `None` if the word is empty.
    #[must_use]
    pub const fn strongest(self) -> Option<EdgeRef> {
        self.edge(0)
    }

    /// The coldest / least-immediate present edge — the **last** occupied slot
    /// under the MRU invariant, i.e. exactly the edge [`promote`](Self::promote)
    /// will evict when a fresh edge displaces a full word. `None` if empty.
    /// Symmetric to [`strongest`](Self::strongest); the cold tier peeks the
    /// victim here before it falls out.
    #[must_use]
    pub const fn coldest(self) -> Option<EdgeRef> {
        let mut last = None;
        let mut i = 0;
        while i < Self::CAPACITY {
            if let Some(e) = self.edge(i) {
                last = Some(e);
            }
            i += 1;
        }
        last
    }

    /// Is `e` present in any slot? Membership discriminates `family` (so
    /// `cross(3, 3)` ≠ `intra(3)`), matching [`promote`](Self::promote)'s dedup.
    #[must_use]
    pub fn contains(self, e: EdgeRef) -> bool {
        self.iter().any(|x| x == e)
    }

    /// Iterate the present edges in slot order.
    pub fn iter(self) -> impl Iterator<Item = EdgeRef> {
        (0..Self::CAPACITY).filter_map(move |i| self.edge(i))
    }

    /// Count of cross-family edges (the crossover load — the ~1.4% the probe measured).
    #[must_use]
    pub fn cross_count(self) -> usize {
        self.iter().filter(|e| e.is_cross()).count()
    }

    // ── Little-endian byte contract (mirrors `causal-edge::CausalEdge64`) ──
    // The frozen LE byte grammar every AriGraph consumer reads: surrealkv WAL and
    // Lance columns (the baton-wire consumer was retired with `CollapseGateEmission`
    // per PR #477 — zero-copy in-place store, no inter-mailbox carrier). Canonical
    // little-endian, platform-independent — changing the layout is a WAL migration,
    // never silent.

    /// The raw packed `u64` (host value; serialize via [`to_le_bytes`](Self::to_le_bytes)).
    #[must_use]
    pub const fn to_u64(self) -> u64 {
        self.0
    }

    /// Reconstruct from a raw packed `u64`.
    #[must_use]
    pub const fn from_u64(raw: u64) -> Self {
        Self(raw)
    }

    /// Canonical **little-endian** wire bytes — the AriGraph-reference contract.
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    /// Reconstruct from canonical little-endian wire bytes.
    #[must_use]
    pub const fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self(u64::from_le_bytes(bytes))
    }

    /// Append the canonical LE bytes to a wire buffer (baton / WAL line).
    pub fn write_le(self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.to_le_bytes());
    }

    /// Read one word from `buf` at `offset` (LE), or `None` if fewer than 8 bytes remain.
    #[must_use]
    pub fn read_le(buf: &[u8], offset: usize) -> Option<Self> {
        let end = offset.checked_add(8)?;
        let mut b = [0u8; 8];
        b.copy_from_slice(buf.get(offset..end)?);
        Some(Self::from_le_bytes(b))
    }
}

/// Receives an edge demoted out of the hot 4-slot tier — the **cold connectome**
/// (`E-EW64-STRENGTH-IS-CE64-PLASTICITY`, `E-SUBSTRATE-IS-THE-SCHEDULER`).
///
/// This is the stable **zero-dep seam** between the hot tier (the `EpisodicEdges64`
/// MRU word, in the SoA) and the cold tier (the full connectome). Impls — the
/// surreal-LIVE / LanceDB-LIVE "wingman" that persists and re-prefetches demoted
/// edges — are deferred and GATED on OQ-11.6; the contract owns only the seam,
/// the same dependency-inversion idiom as [`crate::soa_view::MailboxSoaOwner`].
pub trait DemotionSink {
    /// An edge aged out of the hot tier; route it to the cold connectome.
    fn demote(&mut self, evicted: EdgeRef);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edgeref_new_validates_family_and_local() {
        assert!(EdgeRef::new(0, 1).is_some());
        assert!(EdgeRef::new(15, 4095).is_some());
        assert!(EdgeRef::new(16, 1).is_none());
        assert!(EdgeRef::new(0, 0).is_none());
        assert!(EdgeRef::new(0, 4096).is_none());
        assert!(EdgeRef::cross(0, 5).is_none());
        assert_eq!(EdgeRef::intra(5).unwrap().family, 0);
    }

    #[test]
    fn slot_roundtrip_and_empty_sentinel() {
        for family in 0..16u8 {
            for &local in &[1u16, 2, 100, 4095] {
                let e = EdgeRef::new(family, local).unwrap();
                assert_eq!(EdgeRef::from_slot(e.to_slot()).unwrap(), e);
            }
        }
        assert_eq!(EdgeRef::from_slot(0), None);
    }

    #[test]
    fn push_count_and_full() {
        let mut w = EpisodicEdges64::empty();
        assert_eq!(w.count(), 0);
        for k in 1..=4u16 {
            w = w.push(EdgeRef::intra(k).unwrap()).expect("fits");
        }
        assert_eq!(w.count(), 4);
        assert!(w.is_full());
        assert!(w.push(EdgeRef::intra(5).unwrap()).is_none());
    }

    #[test]
    fn edge_index_out_of_range_is_none() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(7).unwrap())
            .unwrap();
        assert_eq!(w.edge(0), EdgeRef::intra(7));
        assert_eq!(w.edge(1), None);
        assert_eq!(w.edge(EpisodicEdges64::CAPACITY), None);
    }

    #[test]
    fn intra_is_cheap_default_cross_is_the_nibble() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(10).unwrap())
            .unwrap()
            .push(EdgeRef::intra(11).unwrap())
            .unwrap()
            .push(EdgeRef::intra(12).unwrap())
            .unwrap()
            .push(EdgeRef::cross(3, 7).unwrap())
            .unwrap();
        assert_eq!(w.count(), 4);
        assert_eq!(w.cross_count(), 1);
        assert!(!w.edge(0).unwrap().is_cross());
        assert!(w.edge(3).unwrap().is_cross());
        assert_eq!(w.edge(3).unwrap().family, 3);
    }

    #[test]
    fn iter_yields_present_edges_in_order() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(1).unwrap())
            .unwrap()
            .push(EdgeRef::cross(2, 9).unwrap())
            .unwrap();
        let got: Vec<_> = w.iter().collect();
        assert_eq!(
            got,
            vec![EdgeRef::intra(1).unwrap(), EdgeRef::cross(2, 9).unwrap()]
        );
    }

    #[test]
    fn word_is_exactly_64_bits() {
        assert_eq!(core::mem::size_of::<EpisodicEdges64>(), 8);
    }

    #[test]
    fn le_byte_contract_roundtrips() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(7).unwrap())
            .unwrap()
            .push(EdgeRef::cross(5, 42).unwrap())
            .unwrap();
        assert_eq!(EpisodicEdges64::from_u64(w.to_u64()), w);
        assert_eq!(EpisodicEdges64::from_le_bytes(w.to_le_bytes()), w);
        let mut buf = Vec::new();
        w.write_le(&mut buf);
        assert_eq!(buf.len(), 8);
        assert_eq!(EpisodicEdges64::read_le(&buf, 0), Some(w));
        assert_eq!(EpisodicEdges64::read_le(&buf, 1), None); // only 7 bytes remain
    }

    #[test]
    fn le_bytes_are_canonical_little_endian() {
        // slot 0 = intra local 1 => 0x0001 in the low 16 bits; LE => byte[0] = 0x01.
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(1).unwrap())
            .unwrap();
        assert_eq!(w.to_le_bytes()[0], 0x01);
        assert_eq!(w.to_le_bytes()[1], 0x00);
    }

    #[test]
    fn promote_into_empty_sets_strongest() {
        let (w, evicted) = EpisodicEdges64::empty().promote(EdgeRef::intra(7).unwrap());
        assert_eq!(w.strongest(), EdgeRef::intra(7));
        assert_eq!(w.count(), 1);
        assert_eq!(evicted, None);
    }

    #[test]
    fn promote_existing_moves_to_front_no_evict() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(1).unwrap())
            .unwrap()
            .push(EdgeRef::intra(2).unwrap())
            .unwrap()
            .push(EdgeRef::intra(3).unwrap())
            .unwrap();
        // Promote the middle edge → moves to slot 0, survivors preserved in order.
        let (w2, evicted) = w.promote(EdgeRef::intra(2).unwrap());
        assert_eq!(evicted, None);
        assert_eq!(w2.strongest(), EdgeRef::intra(2));
        assert_eq!(
            w2.iter().collect::<Vec<_>>(),
            vec![
                EdgeRef::intra(2).unwrap(),
                EdgeRef::intra(1).unwrap(),
                EdgeRef::intra(3).unwrap(),
            ]
        );
    }

    #[test]
    fn promote_new_on_full_evicts_coldest() {
        // Fill 1,2,3,4 (slot 3 = intra 4 = coldest); promote a new edge → evict it.
        let mut w = EpisodicEdges64::empty();
        for k in 1..=4u16 {
            w = w.push(EdgeRef::intra(k).unwrap()).unwrap();
        }
        let (w2, evicted) = w.promote(EdgeRef::intra(9).unwrap());
        assert_eq!(evicted, EdgeRef::intra(4), "coldest (slot 3) is evicted");
        assert_eq!(w2.strongest(), EdgeRef::intra(9));
        assert_eq!(
            w2.iter().collect::<Vec<_>>(),
            vec![
                EdgeRef::intra(9).unwrap(),
                EdgeRef::intra(1).unwrap(),
                EdgeRef::intra(2).unwrap(),
                EdgeRef::intra(3).unwrap(),
            ]
        );
    }

    #[test]
    fn promote_refire_hottest_is_idempotent() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(1).unwrap())
            .unwrap()
            .push(EdgeRef::intra(2).unwrap())
            .unwrap();
        let (w2, evicted) = w.promote(EdgeRef::intra(1).unwrap());
        assert_eq!(evicted, None);
        assert_eq!(w2, w, "re-firing the already-strongest edge is a no-op");
    }

    #[test]
    fn promote_dedups_when_present_on_full_word() {
        let mut w = EpisodicEdges64::empty();
        for k in 1..=4u16 {
            w = w.push(EdgeRef::intra(k).unwrap()).unwrap();
        }
        // Promote an edge already present → no eviction, no duplicate slot.
        let (w2, evicted) = w.promote(EdgeRef::intra(3).unwrap());
        assert_eq!(evicted, None);
        assert_eq!(w2.count(), 4, "no duplicate slot created");
        assert_eq!(w2.strongest(), EdgeRef::intra(3));
        for k in 1..=4u16 {
            assert!(w2.iter().any(|e| e == EdgeRef::intra(k).unwrap()));
        }
    }

    #[test]
    fn promote_cross_family_local_collision_is_not_deduped() {
        // Dedup compares `family` too: cross(3,3) and intra(3) share `local`
        // but differ in family → distinct edges, both present, no dedup.
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(1).unwrap())
            .unwrap()
            .push(EdgeRef::intra(3).unwrap())
            .unwrap();
        let (w2, evicted) = w.promote(EdgeRef::cross(3, 3).unwrap());
        assert_eq!(evicted, None);
        assert_eq!(w2.count(), 3, "cross(3,3) is distinct from intra(3)");
        assert_eq!(w2.strongest(), EdgeRef::cross(3, 3));
        assert!(w2.iter().any(|e| e == EdgeRef::intra(3).unwrap()));
    }

    #[test]
    fn promote_chains_mru_aging_and_appends_fresh_on_non_full() {
        // A fresh edge on a non-full word appends to the front (no eviction).
        let (w, evicted) = EpisodicEdges64::empty()
            .push(EdgeRef::intra(1).unwrap())
            .unwrap()
            .push(EdgeRef::intra(2).unwrap())
            .unwrap()
            .promote(EdgeRef::intra(5).unwrap());
        assert_eq!(evicted, None);
        assert_eq!(
            w.iter().collect::<Vec<_>>(),
            vec![
                EdgeRef::intra(5).unwrap(),
                EdgeRef::intra(1).unwrap(),
                EdgeRef::intra(2).unwrap(),
            ]
        );
        // Chain 4 fresh promotes → newest-first [4,3,2,1]; then re-fire the
        // coldest (1) → it jumps to the front: [1,4,3,2], no eviction.
        let mut m = EpisodicEdges64::empty();
        for k in 1..=4u16 {
            m = m.promote(EdgeRef::intra(k).unwrap()).0;
        }
        assert_eq!(
            m.iter().collect::<Vec<_>>(),
            vec![
                EdgeRef::intra(4).unwrap(),
                EdgeRef::intra(3).unwrap(),
                EdgeRef::intra(2).unwrap(),
                EdgeRef::intra(1).unwrap(),
            ]
        );
        let (m2, evicted2) = m.promote(EdgeRef::intra(1).unwrap());
        assert_eq!(evicted2, None, "re-firing a present edge never evicts");
        assert_eq!(
            m2.iter().collect::<Vec<_>>(),
            vec![
                EdgeRef::intra(1).unwrap(),
                EdgeRef::intra(4).unwrap(),
                EdgeRef::intra(3).unwrap(),
                EdgeRef::intra(2).unwrap(),
            ]
        );
    }

    #[test]
    fn coldest_of_empty_is_none() {
        assert_eq!(EpisodicEdges64::empty().coldest(), None);
    }

    #[test]
    fn coldest_of_single_equals_strongest() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(7).unwrap())
            .unwrap();
        assert_eq!(w.coldest(), w.strongest());
        assert_eq!(w.coldest(), EdgeRef::intra(7));
    }

    #[test]
    fn coldest_of_full_word_is_last_slot() {
        let mut w = EpisodicEdges64::empty();
        for k in 1..=4u16 {
            w = w.push(EdgeRef::intra(k).unwrap()).unwrap();
        }
        assert_eq!(w.coldest(), EdgeRef::intra(4)); // slot 3
    }

    #[test]
    fn coldest_equals_promote_eviction_victim() {
        // The edge coldest() names is exactly the one promote evicts when a
        // fresh edge displaces a full word — ties the read + write APIs.
        let mut w = EpisodicEdges64::empty();
        for k in 1..=4u16 {
            w = w.push(EdgeRef::intra(k).unwrap()).unwrap();
        }
        let victim = w.coldest();
        let (_w2, evicted) = w.promote(EdgeRef::intra(9).unwrap());
        assert_eq!(evicted, victim);
    }

    #[test]
    fn contains_present_and_absent() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(1).unwrap())
            .unwrap()
            .push(EdgeRef::cross(2, 5).unwrap())
            .unwrap();
        assert!(w.contains(EdgeRef::intra(1).unwrap()));
        assert!(w.contains(EdgeRef::cross(2, 5).unwrap()));
        assert!(!w.contains(EdgeRef::intra(2).unwrap()));
    }

    #[test]
    fn contains_discriminates_family() {
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(3).unwrap())
            .unwrap();
        assert!(w.contains(EdgeRef::intra(3).unwrap()));
        assert!(
            !w.contains(EdgeRef::cross(3, 3).unwrap()),
            "family is discriminated"
        );
    }

    /// Test sink that records demoted edges in arrival order.
    struct VecSink(Vec<EdgeRef>);
    impl DemotionSink for VecSink {
        fn demote(&mut self, evicted: EdgeRef) {
            self.0.push(evicted);
        }
    }

    #[test]
    fn promote_into_non_full_leaves_sink_empty() {
        let mut sink = VecSink(Vec::new());
        let w = EpisodicEdges64::empty()
            .push(EdgeRef::intra(1).unwrap())
            .unwrap()
            .promote_into(EdgeRef::intra(2).unwrap(), &mut sink);
        assert!(sink.0.is_empty(), "no eviction on a non-full word");
        assert_eq!(w.strongest(), EdgeRef::intra(2));
    }

    #[test]
    fn promote_into_full_routes_coldest_to_sink() {
        let mut w = EpisodicEdges64::empty();
        for k in 1..=4u16 {
            w = w.push(EdgeRef::intra(k).unwrap()).unwrap();
        }
        let victim = w.coldest().unwrap();
        let mut sink = VecSink(Vec::new());
        let w2 = w.promote_into(EdgeRef::intra(9).unwrap(), &mut sink);
        assert_eq!(sink.0, vec![victim], "exactly the coldest is demoted");
        assert_eq!(
            w2,
            w.promote(EdgeRef::intra(9).unwrap()).0,
            "word equals promote().0; the sink only adds routing"
        );
    }

    #[test]
    fn promote_into_chain_accumulates_evictees_in_age_order() {
        let mut sink = VecSink(Vec::new());
        let mut w = EpisodicEdges64::empty();
        // 1..=4 fill [4,3,2,1]; 5 evicts 1; 6 evicts 2.
        for k in 1..=6u16 {
            w = w.promote_into(EdgeRef::intra(k).unwrap(), &mut sink);
        }
        assert_eq!(
            sink.0,
            vec![EdgeRef::intra(1).unwrap(), EdgeRef::intra(2).unwrap()]
        );
    }

    #[test]
    fn promote_into_refire_present_leaves_sink_untouched() {
        let mut w = EpisodicEdges64::empty();
        for k in 1..=4u16 {
            w = w.push(EdgeRef::intra(k).unwrap()).unwrap();
        }
        let mut sink = VecSink(Vec::new());
        let _ = w.promote_into(EdgeRef::intra(2).unwrap(), &mut sink);
        assert!(sink.0.is_empty(), "re-firing a present edge never demotes");
    }
}
