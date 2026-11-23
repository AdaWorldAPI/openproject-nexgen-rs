// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! zammad-rs's own DT64 lane source: nullable columns as `u32` value lanes,
//! each with its presence plane (EXO-294, decision B).
//!
//! A NULL cell is stored as value 0 with presence bit 0. A stored zero is
//! value 0 with presence bit 1. Under decision B a row is absent only when
//! both are 0, so a NULL that kept any other value would read as present.
//!
//! Lanes are numbered in the order columns are added, independently of
//! basis positions. `lane_of` maps a field by the predicate the basis gives
//! it, and `borrow_presence` looks a plane up by the lane address, never by
//! field number.

use lance_graph_contract::ontology::FieldRef;
use lance_graph_dt64::dr::{Frame, FrameRevisions, FrameStore, Scope};
use lance_graph_dt64::{CoordSpace, LaneAddr, LaneSource, PresenceSource, SchemaRev};
use lance_graph_mask_risc::LaneRef;

/// `⌈n/64⌉`: the words of a presence plane over `n` rows.
#[must_use]
pub const fn presence_words(n_rows: usize) -> usize {
    n_rows.div_ceil(64)
}

/// Why a presence plane cannot be folded over a lane of `n` rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneRefusal {
    /// The plane is not `⌈n/64⌉` words.
    PresenceLength {
        field: u8,
        words: usize,
        need: usize,
    },
    /// A bit past row `n` is set.
    PresenceTail { field: u8 },
}

/// The check a consumer owes before folding a plane: DT64 does not know a
/// lane's length.
pub fn check_plane(field: u8, plane: &[u64], n_rows: usize) -> Result<(), PlaneRefusal> {
    let need = presence_words(n_rows);
    if plane.len() != need {
        return Err(PlaneRefusal::PresenceLength {
            field,
            words: plane.len(),
            need,
        });
    }
    let used = n_rows % 64;
    if used != 0 && plane[need - 1] >> used != 0 {
        return Err(PlaneRefusal::PresenceTail { field });
    }
    Ok(())
}

/// One nullable column: the predicate it serves, its values and its plane.
#[derive(Debug, Clone)]
struct Column {
    predicate: Box<str>,
    values: Vec<u32>,
    presence: Vec<u64>,
}

/// The value written for a NULL. A disable run keeps a stale nonzero value,
/// which decision B reads as present.
#[cfg(not(feature = "disable-null-zeroing"))]
const NULL_VALUE: u32 = 0;
#[cfg(feature = "disable-null-zeroing")]
const NULL_VALUE: u32 = 1;

/// Columns of one class at one schema revision, in one row order, for one
/// reader scope, under the source's own frame store.
pub struct ColumnSource {
    frames: FrameStore,
    class: u32,
    rev: SchemaRev,
    space: CoordSpace,
    scope: Scope,
    n_rows: usize,
    columns: Vec<Column>,
}

const REVS: FrameRevisions = FrameRevisions {
    source: 1,
    relation_map: 1,
    coordinates: 1,
};

impl ColumnSource {
    #[must_use]
    pub fn new(class: u32, rev: SchemaRev, n_rows: usize, space: CoordSpace, scope: Scope) -> Self {
        Self {
            frames: FrameStore::new(REVS),
            class,
            rev,
            space,
            scope,
            n_rows,
            columns: Vec::new(),
        }
    }

    /// Add the column serving `field`; returns its lane. A cell `None` is a
    /// NULL. Panics if `cells` is not `n_rows` long.
    pub fn push_nullable(&mut self, field: &FieldRef, cells: &[Option<u32>]) -> u16 {
        assert_eq!(
            cells.len(),
            self.n_rows,
            "column {} rows",
            field.predicate_iri
        );
        let mut presence = vec![0u64; presence_words(self.n_rows)];
        let values = cells
            .iter()
            .enumerate()
            .map(|(r, c)| match *c {
                Some(v) => {
                    presence[r / 64] |= 1 << (r % 64);
                    v
                }
                None => NULL_VALUE,
            })
            .collect();
        self.columns.push(Column {
            predicate: field.predicate_iri.as_str().into(),
            values,
            presence,
        });
        u16::try_from(self.columns.len() - 1).expect("fewer than 65536 columns")
    }

    #[must_use]
    pub fn n_rows(&self) -> usize {
        self.n_rows
    }

    #[must_use]
    pub fn space(&self) -> CoordSpace {
        self.space
    }

    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The current frame of this source's own store.
    #[must_use]
    pub fn seal(&self) -> Frame {
        self.frames.seal()
    }

    /// Lab use: overwrite a lane's plane, to test what a consumer refuses.
    pub fn replace_plane(&mut self, lane: u16, plane: Vec<u64>) {
        self.columns[usize::from(lane)].presence = plane;
    }

    fn served(&self, addr: &LaneAddr) -> Option<&Column> {
        (addr.space == self.space && addr.scope == self.scope)
            .then(|| self.columns.get(usize::from(addr.lane)))
            .flatten()
    }
}

impl LaneSource for ColumnSource {
    type Lane<'a> = LaneRef<'a>;

    fn lane_of(
        &self,
        class: u32,
        rev: SchemaRev,
        _field: u8,
        basis: &FieldRef,
        frame: &Frame,
    ) -> Option<LaneAddr> {
        self.frames.check(frame).ok()?;
        if class != self.class || rev != self.rev {
            return None;
        }
        let lane = self
            .columns
            .iter()
            .position(|c| *c.predicate == *basis.predicate_iri.as_str())?;
        #[cfg(feature = "disable-lane-mapping")]
        let lane = usize::from(_field);
        Some(LaneAddr {
            space: self.space,
            lane: u16::try_from(lane).ok()?,
            scope: self.scope,
        })
    }

    fn borrow(&self, addr: &LaneAddr) -> Option<LaneRef<'_>> {
        self.served(addr).map(|c| LaneRef::U32(&c.values))
    }

    fn frames(&self) -> &FrameStore {
        &self.frames
    }
}

impl PresenceSource for ColumnSource {
    fn borrow_presence(&self, addr: &LaneAddr) -> Option<&[u64]> {
        self.served(addr).map(|c| c.presence.as_slice())
    }
}
