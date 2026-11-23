// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Counters of what the read path does, to be read before and after one call.
//! They count; they never time and never change an answer.

use std::cell::{Cell, RefCell};

use super::Catalog;

/// One step of a call, in the order it happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The lanes of the catalog table at this index were asked for through the session.
    Read(usize),
    /// A lane check of the catalog table at this index: `borrow_all_present`
    /// or `borrow_all_selected_present`.
    Check(usize),
    /// A program ran over the catalog table at this index.
    Fold(usize),
    /// A key was resolved to a position through the table's DR map.
    ResolveMap,
    /// A key of a table laid out by key was used as its own position.
    ResolveKey,
}

#[derive(Default)]
pub(crate) struct Census {
    checks: Cell<u64>,
    folds: Cell<u64>,
    ordinal_reads: Cell<u64>,
    ordinal_reads_unchecked: Cell<u64>,
    bulk_reads_unchecked: Cell<u64>,
    key_resolutions: Cell<u64>,
    key_as_position: Cell<u64>,
    source_frames: Cell<u64>,
    source_lane_of: Cell<u64>,
    source_borrows: Cell<u64>,
    source_presence: Cell<u64>,
    source_positions: Cell<u64>,
    source_selection_of: Cell<u64>,
    /// The steps of the open census window only: [`Catalog::census_mark`]
    /// clears them and opens the window, so the log never outgrows one call
    /// and a catalog never measured records nothing.
    steps: RefCell<Vec<(Step, u64)>>,
    recording: Cell<bool>,
}

fn bump(cell: &Cell<u64>, by: u64) {
    cell.set(cell.get() + by);
}

impl Census {
    fn step(&self, step: Step, n: u64) {
        if !self.recording.get() {
            return;
        }
        let mut steps = self.steps.borrow_mut();
        match steps.last_mut() {
            Some((last, count)) if *last == step => *count += n,
            _ => steps.push((step, n)),
        }
    }

    pub(crate) fn read(&self, table: usize) {
        self.step(Step::Read(table), 1);
    }

    pub(crate) fn check(&self, table: usize) {
        bump(&self.checks, 1);
        self.step(Step::Check(table), 1);
    }

    pub(crate) fn fold(&self, table: usize) {
        bump(&self.folds, 1);
        self.step(Step::Fold(table), 1);
    }

    /// `n` lane cells read at a position through the checked lanes.
    pub(crate) fn ordinal_checked(&self, n: u64) {
        bump(&self.ordinal_reads, n);
    }

    /// `n` lane cells read at a position from the table's own lane, no check before them.
    pub(crate) fn ordinal_unchecked(&self, n: u64) {
        bump(&self.ordinal_reads, n);
        bump(&self.ordinal_reads_unchecked, n);
    }

    /// A whole lane of a table read without a lane check.
    pub(crate) fn bulk_unchecked(&self) {
        bump(&self.bulk_reads_unchecked, 1);
    }

    pub(crate) fn resolve_map(&self) {
        bump(&self.key_resolutions, 1);
        self.step(Step::ResolveMap, 1);
    }

    pub(crate) fn resolve_key(&self) {
        bump(&self.key_as_position, 1);
        self.step(Step::ResolveKey, 1);
    }

    pub(crate) fn source_frames(&self) {
        bump(&self.source_frames, 1);
    }

    pub(crate) fn source_lane_of(&self) {
        bump(&self.source_lane_of, 1);
    }

    pub(crate) fn source_borrow(&self) {
        bump(&self.source_borrows, 1);
    }

    pub(crate) fn source_presence(&self) {
        bump(&self.source_presence, 1);
    }

    pub(crate) fn source_positions(&self) {
        bump(&self.source_positions, 1);
    }

    pub(crate) fn source_selection_of(&self) {
        bump(&self.source_selection_of, 1);
    }
}

/// The counters at one moment; see [`Catalog::census_since`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CensusMark {
    counters: Counters,
}

/// What happened between a [`CensusMark`] and now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CensusDelta {
    /// Lane checks: `borrow_all_present` and `borrow_all_selected_present`.
    pub checks: u64,
    /// Programs run over a table.
    pub folds: u64,
    /// Lane cells read at a position, checked or not.
    pub ordinal_reads: u64,
    /// The part of `ordinal_reads` read from a table's own lane with no lane check before it.
    pub ordinal_reads_unchecked: u64,
    /// Whole lanes read from a table without a lane check.
    pub bulk_reads_unchecked: u64,
    /// `Positions::find` calls: a key resolved to a position through the DR map.
    pub key_resolutions: u64,
    /// Keys used as their own position (identity layout), no map read.
    pub key_as_position: u64,
    /// Global-to-local lookups the DR maps served (DT64's own counter).
    pub map_lookups: u64,
    /// Calls DT64's check made back into the source: `LaneSource::frames`.
    pub source_frames: u64,
    /// Calls DT64's check made back into the source: `LaneSource::lane_of`.
    pub source_lane_of: u64,
    /// `LaneSource::borrow` calls.
    pub source_borrows: u64,
    /// `PresenceSource::borrow_presence` calls.
    pub source_presence: u64,
    /// `SelectionSource::borrow_positions` calls.
    pub source_positions: u64,
    /// `SelectionSource::selection_of` calls.
    pub source_selection_of: u64,
    /// The steps in order, equal neighbours merged with their count.
    pub steps: Vec<(Step, u64)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Counters {
    checks: u64,
    folds: u64,
    ordinal_reads: u64,
    ordinal_reads_unchecked: u64,
    bulk_reads_unchecked: u64,
    key_resolutions: u64,
    key_as_position: u64,
    map_lookups: u64,
    source_frames: u64,
    source_lane_of: u64,
    source_borrows: u64,
    source_presence: u64,
    source_positions: u64,
    source_selection_of: u64,
}

impl Catalog {
    fn counters(&self) -> Counters {
        let c = &self.census;
        Counters {
            checks: c.checks.get(),
            folds: c.folds.get(),
            ordinal_reads: c.ordinal_reads.get(),
            ordinal_reads_unchecked: c.ordinal_reads_unchecked.get(),
            bulk_reads_unchecked: c.bulk_reads_unchecked.get(),
            key_resolutions: c.key_resolutions.get(),
            key_as_position: c.key_as_position.get(),
            map_lookups: self.map_lookups(),
            source_frames: c.source_frames.get(),
            source_lane_of: c.source_lane_of.get(),
            source_borrows: c.source_borrows.get(),
            source_presence: c.source_presence.get(),
            source_positions: c.source_positions.get(),
            source_selection_of: c.source_selection_of.get(),
        }
    }

    /// The counters now, and the start of a census window: the step log is
    /// cleared and records from here on. Read before and after one call, the
    /// difference is that call's cost. One window at a time: a later mark
    /// ends the earlier window's step log.
    #[doc(hidden)]
    pub fn census_mark(&self) -> CensusMark {
        self.census.steps.borrow_mut().clear();
        self.census.recording.set(true);
        CensusMark {
            counters: self.counters(),
        }
    }

    /// What the counters did since `mark`.
    #[doc(hidden)]
    pub fn census_since(&self, mark: &CensusMark) -> CensusDelta {
        let (now, then) = (self.counters(), &mark.counters);
        let steps = self.census.steps.borrow().clone();
        self.census.recording.set(false);
        CensusDelta {
            checks: now.checks - then.checks,
            folds: now.folds - then.folds,
            ordinal_reads: now.ordinal_reads - then.ordinal_reads,
            ordinal_reads_unchecked: now.ordinal_reads_unchecked - then.ordinal_reads_unchecked,
            bulk_reads_unchecked: now.bulk_reads_unchecked - then.bulk_reads_unchecked,
            key_resolutions: now.key_resolutions - then.key_resolutions,
            key_as_position: now.key_as_position - then.key_as_position,
            map_lookups: now.map_lookups.saturating_sub(then.map_lookups),
            source_frames: now.source_frames - then.source_frames,
            source_lane_of: now.source_lane_of - then.source_lane_of,
            source_borrows: now.source_borrows - then.source_borrows,
            source_presence: now.source_presence - then.source_presence,
            source_positions: now.source_positions - then.source_positions,
            source_selection_of: now.source_selection_of - then.source_selection_of,
            steps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FAILS IF: steps are recorded outside a census window, or a window's log
    /// keeps an earlier window's steps.
    #[test]
    fn the_step_log_holds_only_the_open_window() {
        let catalog = Catalog::new();
        for t in 0..1000 {
            catalog.census.read(t % 3);
            catalog.census.check(t % 3);
            catalog.census.fold(t % 3);
        }
        assert!(
            catalog.census.steps.borrow().is_empty(),
            "no window, no log"
        );
        assert_eq!(catalog.census.checks.get(), 1000, "counters still count");

        let mark = catalog.census_mark();
        catalog.census.read(0);
        catalog.census.check(0);
        let delta = catalog.census_since(&mark);
        assert_eq!(delta.steps, vec![(Step::Read(0), 1), (Step::Check(0), 1)]);
        assert_eq!(delta.checks, 1);

        let mark = catalog.census_mark();
        catalog.census.fold(1);
        assert_eq!(
            catalog.census_since(&mark).steps,
            vec![(Step::Fold(1), 1)],
            "a new mark starts a new log"
        );
        assert_eq!(catalog.census.steps.borrow().len(), 1);

        catalog.census.fold(2);
        assert_eq!(
            catalog.census.steps.borrow().len(),
            1,
            "census_since closes the window, so a later step is not logged"
        );
        assert_eq!(catalog.census.folds.get(), 1002, "counters still count");
    }
}
