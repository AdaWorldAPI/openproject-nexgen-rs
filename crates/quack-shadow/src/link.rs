// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The edge as a translation of target row positions: a reading derived on
//! request from the edge lane and the target's DR map, and dropped
//! after the call.
//!
//! A join translates the constant, not the lane (see `join.rs`), and derives
//! nothing. The translation exists only under the oracle switch, as the reading
//! the default path is compared with.

use lance_graph_dt64::dr::{LocateRefusal, MapTable};
use lance_graph_mask_risc::words_for;

use super::{
    Catalog, Census, Lane, LaneKind, Layout, Refusal, TILE, Table, is_live_at, is_null, malformed,
    tiles_of, value_at,
};

/// `source.edge_col` read as row positions of the target, beside a validity plane.
pub(crate) struct Link {
    values: Vec<u32>,
    validity: Vec<u64>,
}

/// What the derived translations of a catalog cost so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinkStats {
    /// Translations derived.
    pub derived: u64,
    /// Source rows whose edge key was translated to a position.
    pub rows_translated: u64,
    /// Key sets computed over a dimension that carries a filter of its own.
    pub key_sets: u64,
    /// Keys in those sets, together.
    pub keys_in_sets: u64,
    /// Keys placed after the fold: kept rows only, through the target's DR map.
    pub placed_late: u64,
    /// Grouped counts of a driving edge column taken to learn which keys name no live target row.
    pub existence_folds: u64,
    /// Keys those counts found naming no live target row.
    pub dangling_keys: u64,
    /// Gather leaves built: a position read from another lane's value (`eq_u32_via`, `semijoin`).
    pub gather_leaves: u64,
}

pub(crate) fn edge_lane<'t>(table: &'t Table, edge_col: &str) -> Result<&'t Lane, Refusal> {
    let field = || edge_col.to_owned();
    let column = table
        .columns
        .iter()
        .find(|c| c.name == edge_col)
        .ok_or_else(|| Refusal::UnknownField {
            table: table.name.clone(),
            field: field(),
        })?;
    let lane = column.lane.as_ref().ok_or_else(|| Refusal::UnlanedColumn {
        table: table.name.clone(),
        field: field(),
    })?;
    if lane.kind != LaneKind::Int {
        return Err(Refusal::JoinShape(
            "edge column is not an integer column".to_owned(),
        ));
    }
    Ok(lane)
}

impl Link {
    fn put(&mut self, row: usize, position: Option<u32>) {
        let (Some(value), Some(word)) = (self.values.get_mut(row), self.validity.get_mut(row / 64))
        else {
            return;
        };
        if let Some(position) = position {
            *value = position;
            *word |= 1u64 << (row % 64);
        }
    }

    pub(crate) fn values(&self) -> &[u32] {
        &self.values
    }

    pub(crate) fn validity(&self) -> &[u64] {
        &self.validity
    }
}

/// `source.edge_col` read as positions of `target`, whose DR map is `target_map`.
/// A NULL, dangling or removed key is a cleared validity bit.
pub(crate) fn derive(
    source: &Table,
    edge_col: &str,
    target: &Table,
    target_map: &MapTable,
    census: &Census,
) -> Result<Link, Refusal> {
    let lane = edge_lane(source, edge_col)?;
    let mut link = Link {
        values: vec![0; source.rows],
        validity: vec![0; words_for(source.rows)],
    };
    let frame = target_map.seal();
    let tiles = tiles_of(target_map, &frame, target.rows);
    let out_of_lane = || Refusal::ValueOutOfLane {
        field: edge_col.to_owned(),
    };
    for row in (0..source.rows).filter(|&row| !is_null(&lane.validity, row)) {
        let key = value_at(&lane.values, row);
        let position = match target.layout {
            Layout::Identity => {
                census.resolve_key();
                usize::try_from(key).ok().filter(|&at| at < target.rows)
            }
            Layout::Append => {
                census.resolve_map();
                match target_map.locate(&frame, &tiles, u64::from(key)) {
                    Ok((ordinal, local)) => {
                        let tile = tiles
                            .get(usize::try_from(ordinal).map_err(|_| out_of_lane())?)
                            .ok_or_else(|| malformed("tile ordinal"))?;
                        usize::try_from(*tile)
                            .ok()
                            .map(|tile| tile * TILE + usize::from(local))
                    }
                    Err(LocateRefusal::Unmapped) => None,
                    Err(other) => return Err(Refusal::Exec(format!("{other:?}"))),
                }
            }
        };
        let live = position.filter(|&at| is_live_at(target, at));
        link.put(
            row,
            live.map(u32::try_from)
                .transpose()
                .map_err(|_| out_of_lane())?,
        );
    }
    Ok(link)
}

impl Catalog {
    /// What the derived translations have cost so far.
    pub fn link_stats(&self) -> LinkStats {
        self.link_stats.get()
    }

    /// Whether joins read through the stored translation of positions.
    pub(crate) fn translates_positions(&self) -> bool {
        self.translated_positions
    }

    /// Whether a join reaches a dimension through its keys, as constants, whatever the layout of `_target`.
    pub(crate) fn reads_key_set(&self, _target: &Table) -> bool {
        !self.translated_positions
    }

    /// Counts one key set of `keys` keys.
    pub(crate) fn note_key_set(&self, keys: usize) {
        let mut stats = self.link_stats.get();
        stats.key_sets += 1;
        stats.keys_in_sets += keys as u64;
        self.link_stats.set(stats);
    }

    /// Counts `keys` keys placed after the fold.
    pub(crate) fn note_placed(&self, keys: u64) {
        let mut stats = self.link_stats.get();
        stats.placed_late += keys;
        self.link_stats.set(stats);
    }

    /// Counts one existence fold that found `dangling` dangling keys.
    pub(crate) fn note_existence(&self, dangling: u64) {
        let mut stats = self.link_stats.get();
        stats.existence_folds += 1;
        stats.dangling_keys += dangling;
        self.link_stats.set(stats);
    }

    /// Counts `leaves` gather leaves built.
    pub(crate) fn note_gather(&self, leaves: u64) {
        let mut stats = self.link_stats.get();
        stats.gather_leaves += leaves;
        self.link_stats.set(stats);
    }

    /// The `(values, validity)` of the translation of `source.edge_col` to `target`,
    /// for comparison with the key lane.
    #[doc(hidden)]
    pub fn oracle_link(
        &self,
        source: &str,
        edge_col: &str,
        target: &str,
    ) -> Result<(Vec<u32>, Vec<u64>), Refusal> {
        let index = |name: &str| {
            self.tables
                .iter()
                .position(|t| t.name == name)
                .ok_or_else(|| Refusal::UnknownTable(name.to_owned()))
        };
        let link = self.link(index(source)?, edge_col, index(target)?)?;
        Ok((link.values, link.validity))
    }

    /// The translation of `source.edge_col` to `target`, derived for this request.
    /// Both tables must have been frame-checked by the caller.
    pub(crate) fn link(
        &self,
        source: usize,
        edge_col: &str,
        target: usize,
    ) -> Result<Link, Refusal> {
        let (Some(s), Some(t), Some(map)) = (
            self.tables.get(source),
            self.tables.get(target),
            self.maps.get(target),
        ) else {
            return Err(malformed("table index"));
        };
        let link = derive(s, edge_col, t, map, &self.census)?;
        let mut stats = self.link_stats.get();
        stats.derived += 1;
        stats.rows_translated += s.rows as u64;
        self.link_stats.set(stats);
        Ok(link)
    }
}
