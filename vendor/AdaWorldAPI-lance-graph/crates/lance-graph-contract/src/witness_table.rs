//! # `WitnessTable` — column-type primitive resolving the 6-bit W slot in `CausalEdge64 v2`.
//!
//! ## Architectural context
//!
//! `CausalEdge64 v2` (plan §6 / bits 53–58) reserves a **6-bit W-slot index** (0..=63)
//! that points into a *per-cohort* `WitnessTable<64>`. Each table entry is a
//! `(mailbox_ref, spo_fact_ref)` tuple:
//!
//! - `mailbox_ref`: full canonical [`contract::collapse_gate::MailboxId`] (`u32`) of
//!   the mailbox that witnessed the belief — either currently active or carrying a
//!   tombstone flag. The W-slot is the per-cohort *index*; `mailbox_ref` is the
//!   *identity* at that slot, preserved at full canonical width across the entire
//!   workspace mailbox envelope (64K-256K, plan §10 refinement (3)).
//! - `spo_fact_ref`: optional handle into the AriGraph SPO-G quad store. `None` while
//!   the belief is still accumulating in the mailbox's energy column; `Some(u64)` once
//!   the belief crystallises and the triple is committed to graph.
//!
//! The chain of W-references across edges forms a **Markov-style belief-update arc**
//! through episodic-reference vectors: each edge's W-slot resolves to the entry that
//! witnessed the prior state, so the arc can be walked backwards (most-recent → oldest
//! witness) without dereferencing the full SPO store on every hop.
//!
//! ## Cohort scoping
//!
//! The table is *per-cohort*, not global. A cohort is a bounded set of collaborating
//! mailboxes (e.g. one rotating sea-star topology partition). `WitnessTable<N>` takes
//! the cohort capacity as a const-generic; the canonical width is `N = 64` (matching
//! the 6-bit W-slot address space). Smaller `N` is legal for test harnesses; larger `N`
//! is UB in the W-slot protocol (indices ≥ 64 would exceed the field width).
//!
//! ## Plan cross-reference
//!
//! `.claude/plans/bindspace-singleton-to-mailbox-soa-v1.md` — §6 "W slot" defines the
//! bit layout, cohort scope, and the Markov arc traversal contract that this type
//! satisfies. Read that plan before wiring `WitnessTable` into emission paths (a later
//! slice).
//!
//! ## Slice scope (A3)
//!
//! This file declares the *column-type primitive only*. It does **not** wire the table
//! into `CausalEdge64`, `MailboxSoA`, or any emission path — those are later slices.
//!
//! ## Not the `perturbation-sim` "witness arc" — different object
//!
//! `perturbation-sim::witness` proves a *numeric* identity (`particle == wave`, an
//! `∑ field·arc` inner product over `&[f64]` evaluated two ways via Parseval/FWHT).
//! That arc is a **signed real weight vector**; THIS table's arc is a **chain of
//! W-slot indices resolving to identity tuples** (`mailbox_ref`, `spo_fact_ref`).
//! They share the word "witness arc" and the Markov reference-chain shape, but the
//! value categories do not match (real field magnitude vs opaque identity handle) —
//! there is no inner-product / transform structure over `WitnessEntry`, and slot
//! resolution is already `O(1)`. Wiring the standing-wave evaluator over a numeric
//! SoA column is the downstream, gated D-MBX-A3 step (gated on D-MBX-A2; see
//! `TD-WITNESS-EVAL-WIRING-1`) and lands as a **consumer-side free function over a
//! borrowed `&[f64]` column**, never a `WitnessArcEvaluator` trait on this zero-dep
//! crate. Conflating the two would be a register-loss / Frankenstein hazard
//! (`I-VSA-IDENTITIES`).

// ── Type declarations ────────────────────────────────────────────────────────

/// A single entry in a per-cohort [`WitnessTable`].
///
/// Carries the pair `(mailbox_ref, spo_fact_ref)` that resolves one W-slot index:
///
/// - `mailbox_ref` is the **full canonical** [`contract::collapse_gate::MailboxId`]
///   (`u32`). The W-slot is the per-cohort *index* (0..=63); `mailbox_ref` is the
///   globally-unique identity of the mailbox at that slot, so the belief arc can
///   resolve to the correct originating mailbox even when cohort membership rotates
///   across the workspace's 64K-256K mailbox envelope (see plan §10 refinement (3)).
/// - `spo_fact_ref` is `None` while the belief is ephemeral (energy accumulating) and
///   `Some(u64)` once the triple is committed to AriGraph (the "crystallisation" event).
///
/// # Size
///
/// `u32` (4 B) + `Option<u64>` (16 B: 1-byte tag + 7 bytes alignment padding +
/// 8-byte payload — `u64` has no niche, so the discriminant cannot be folded into
/// the payload). With `#[repr(Rust)]` field reordering and 8-byte struct alignment
/// the total is **24 B** per entry; an `N=64` table is therefore 1.5 KiB. `Copy` is
/// intentional: the struct is small enough to pass by value on any target ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct WitnessEntry {
    /// Handle to the mailbox that witnessed this belief event.
    ///
    /// Stores the full [`contract::collapse_gate::MailboxId`] (`u32`). Active
    /// mailboxes have a live `w_slot` association; tombstoned mailboxes retain their
    /// ref so the arc walk can detect decommissioned cohort members.
    pub mailbox_ref: u32,

    /// Optional reference into the AriGraph SPO-G quad store.
    ///
    /// `None`: the belief has not yet crystallised to a committed triple.
    /// `Some(fact_id)`: the triple is committed; `fact_id` is the opaque u64 handle
    /// used by the quad store's lookup surface.
    pub spo_fact_ref: Option<u64>,
}

/// Per-cohort witness table: a fixed-size array of [`WitnessEntry`] values indexed
/// by the 6-bit W-slot field from `CausalEdge64 v2`.
///
/// The const parameter `N` is the cohort capacity. The canonical value is `N = 64`
/// (matching the 6-bit address space of the W-slot field). The default type alias
/// `WitnessTable` (no explicit `N`) uses `N = 64`.
///
/// # Invariant
///
/// The caller is responsible for ensuring that W-slot indices passed to [`get`] and
/// [`set`] are in `0..N`. Indices ≥ N return `None`/`Err` respectively — no panic.
///
/// [`get`]: WitnessTable::get
/// [`set`]: WitnessTable::set
#[derive(Debug, Clone)]
pub struct WitnessTable<const N: usize = 64> {
    /// Flat array of witness entries, one per addressable W-slot index.
    pub entries: [WitnessEntry; N],
}

// ── impl WitnessTable ────────────────────────────────────────────────────────

impl<const N: usize> WitnessTable<N> {
    /// Construct a `WitnessTable` with every entry set to its zero-initialised default.
    ///
    /// `mailbox_ref` is 0 (the null mailbox handle) and `spo_fact_ref` is `None`
    /// (belief not yet crystallised) for every slot. This is the correct starting
    /// state for a freshly allocated cohort.
    ///
    /// `const fn` so tables can be embedded in `static` initialisers.
    pub const fn new() -> Self {
        Self {
            entries: [WitnessEntry {
                mailbox_ref: 0,
                spo_fact_ref: None,
            }; N],
        }
    }

    /// Look up the entry at `w_slot`.
    ///
    /// Returns `None` if `w_slot as usize >= N` (out-of-bounds for this cohort).
    /// Returns `Some(&WitnessEntry)` otherwise.
    pub fn get(&self, w_slot: u8) -> Option<&WitnessEntry> {
        self.entries.get(w_slot as usize)
    }

    /// Write `e` into slot `w_slot`.
    ///
    /// Returns `Ok(())` on success.
    /// Returns `Err("w_slot out of range for this WitnessTable")` if
    /// `w_slot as usize >= N` — no panic, caller decides how to handle overflow.
    pub fn set(&mut self, w_slot: u8, e: WitnessEntry) -> Result<(), &'static str> {
        match self.entries.get_mut(w_slot as usize) {
            Some(slot) => {
                *slot = e;
                Ok(())
            }
            None => Err("w_slot out of range for this WitnessTable"),
        }
    }
}

// ── Default ──────────────────────────────────────────────────────────────────

impl<const N: usize> Default for WitnessTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: set a slot, then get it back and confirm the value matches.
    #[test]
    fn witness_table_round_trip_set_get() {
        let mut table: WitnessTable<64> = WitnessTable::new();
        let entry = WitnessEntry {
            mailbox_ref: 42,
            spo_fact_ref: Some(0xDEAD_BEEF_0000_0001),
        };
        table.set(7, entry).expect("slot 7 is in range");
        let got = table.get(7).expect("slot 7 must be present");
        assert_eq!(
            *got, entry,
            "get must return the exact entry written by set"
        );
    }

    /// Out-of-bounds set returns Err; out-of-bounds get returns None.
    #[test]
    fn witness_table_out_of_bounds_returns_err() {
        let mut table: WitnessTable<4> = WitnessTable::new();
        // slot 4 is out of bounds for N=4 (valid range: 0..=3)
        let result = table.set(4, WitnessEntry::default());
        assert!(
            result.is_err(),
            "set with w_slot >= N must return Err, got Ok"
        );
        assert_eq!(table.get(4), None, "get with w_slot >= N must return None");
        // Confirm the in-range slots are untouched
        for i in 0u8..4 {
            assert_eq!(
                table.get(i),
                Some(&WitnessEntry::default()),
                "slot {i} must still be default after out-of-bounds write"
            );
        }
    }

    /// A freshly constructed table has all entries at their zero default:
    /// `mailbox_ref = 0`, `spo_fact_ref = None`.
    #[test]
    fn witness_table_default_is_all_zero() {
        let table: WitnessTable<64> = WitnessTable::default();
        for i in 0u8..64 {
            let entry = table.get(i).expect("all 64 slots must be present");
            assert_eq!(
                entry.mailbox_ref, 0,
                "slot {i}: mailbox_ref must be 0 on default"
            );
            assert_eq!(
                entry.spo_fact_ref, None,
                "slot {i}: spo_fact_ref must be None on default"
            );
        }
    }
}
