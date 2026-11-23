// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Answers captured ActiveRecord `count` / `exists?` / row-returning calls
//! through `lance-graph-quack` over the column values PostgreSQL returned, so
//! each answer can be compared with the one PostgreSQL recorded. Rows are
//! virtual: a call selects positions over source-owned columns and cells are
//! copied only for the returned rows. Anything the quack surface cannot
//! express is refused by name, never guessed.

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use lance_graph_dt64::CoordSpace;
use lance_graph_dt64::dr::{
    CodeRead, CodeSpaceId, CodeTable, Frame, LocalMap, MapTable, TileId, ValueKey, Width,
};
use lance_graph_mask_risc::{
    Foreign, ForeignPlane as ForeignBits, LaneRef, Out, Planes, Program, Scratch, Value,
    execute_into, materialize_rows, words_for,
};
use lance_graph_quack::bind::{Binder, BoundField, FieldKind, Pattern, TableId};
use lance_graph_quack::{Agg, Cmp, Col, Filter, Mask, Query, lower, lower_fused};
use serde_json::Value as Json;

mod bound;
mod census;
mod join;
mod link;
mod relation;
mod sql;
mod under;
pub mod words;

pub use bound::{Bound, ClassMap, SelectedRows, TableBinding};
pub use census::{CensusDelta, CensusMark, Step};
pub use join::{JoinStats, VirtualTuple};
pub use link::LinkStats;
pub use relation::{Reading, Relation, RelationStats};
pub use sql::{LikeStats, columns_of};
pub use under::UnderStats;
pub use words::WordStats;

use bound::{Plane, Session};
use census::Census;
use link::Link;

#[cfg(all(
    any(
        feature = "disable-vrow-local-index",
        feature = "disable-vrow-foreign-frame",
        feature = "disable-vrow-presence",
        feature = "disable-vrow-swap",
        feature = "disable-vrow-live"
    ),
    not(debug_assertions)
))]
compile_error!("the disable-vrow-* features exist for debug builds only");

#[cfg(all(
    any(
        feature = "disable-census-double-check",
        feature = "disable-cube-placement"
    ),
    not(debug_assertions)
))]
compile_error!("the disable-census-* and disable-cube-* features exist for debug builds only");

/// Positions per tile of a table's DR map.
const TILE: usize = 256;

/// How a lane's `u32` values are read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneKind {
    Bool,
    Int,
    /// Epoch seconds.
    Timestamp,
    /// The string's code in the column's code space; the table's code table holds the strings.
    Text,
}

/// How a timestamp column spells its cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spelling {
    /// `{"t": "<UTC time of the epoch>", "epoch": <seconds>, "usec": 0}`.
    Object,
    /// The epoch seconds as a bare integer.
    Bare,
}

/// The `u32` view of a column: a value lane plus a validity plane.
///
/// Validity has one bit per row, LSB-first `u64` words, `rows.div_ceil(64)`
/// words, tail bits zero. A NULL cell is value 0 with validity bit 0; any
/// other cell has bit 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane {
    pub values: Vec<u32>,
    pub validity: Vec<u64>,
    pub kind: LaneKind,
    /// A timestamp lane's spelling; `None` until its first non-NULL value, and for any other kind.
    pub spelling: Option<Spelling>,
}

fn is_null(validity: &[u64], row: usize) -> bool {
    validity
        .get(row / 64)
        .is_none_or(|word| (word >> (row % 64)) & 1 == 0)
}

fn value_at(values: &[u32], row: usize) -> u32 {
    values.get(row).copied().unwrap_or(0)
}

/// Whether a laned column keeps its values only in its lane, text as codes in
/// the table's code table. The two disable runs that derive codes from live
/// cells need them, so they keep the cells.
const LANE_STORE: bool = cfg!(not(any(
    feature = "disable-lane-store",
    feature = "disable-code-advance",
    feature = "disable-stable-codes"
)));

fn lane_stored(column: &Column) -> bool {
    lane_can_store(column) && column.cells.is_empty()
}

/// A laned column whose cells can be rendered back from the lane.
fn lane_can_store(column: &Column) -> bool {
    LANE_STORE && column.lane.is_some()
}

/// Whether every removed row's cell is also in the lane [`build_lane`] made of
/// `cells`. A removed key the lane cannot hold is still needed to place later
/// rows, so such a column keeps its cells. A timestamp column also needs every
/// cell, live or removed, reproducible from its epoch in one spelling.
fn lane_holds_removed(lane: &Lane, cells: &[Json], is_live: impl Fn(usize) -> bool) -> bool {
    match lane.kind {
        LaneKind::Text => true,
        LaneKind::Timestamp => uniform_spelling(cells).is_some(),
        _ => cells.iter().enumerate().all(|(row, cell)| {
            is_live(row) || cell.is_null() || cell_value(lane.kind, cell).is_some()
        }),
    }
}

/// The cell a lane of a column that holds none stands for at `row`; `text` renders a text code.
fn lane_cell(lane: &Lane, row: usize, text: impl FnOnce(u32) -> Json) -> Json {
    if is_null(&lane.validity, row) {
        return Json::Null;
    }
    let value = value_at(&lane.values, row);
    match lane.kind {
        LaneKind::Bool => Json::Bool(value != 0),
        LaneKind::Int => Json::from(value),
        LaneKind::Text => text(value),
        LaneKind::Timestamp => match lane.spelling {
            Some(Spelling::Object) => {
                let mut cell = serde_json::Map::new();
                cell.insert("t".to_owned(), Json::String(utc_text(value)));
                cell.insert("epoch".to_owned(), Json::from(value));
                cell.insert("usec".to_owned(), Json::from(0));
                Json::Object(cell)
            }
            Some(Spelling::Bare) | None => Json::from(value),
        },
    }
}

/// `epoch` seconds as the UTC text PostgreSQL captured: proleptic Gregorian,
/// whole seconds.
fn utc_text(epoch: u32) -> String {
    let (days, secs) = (i64::from(epoch / 86_400), epoch % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let (hour, minute, second) = (secs / 3_600, secs % 3_600 / 60, secs % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.000000Z")
}

/// The epoch and spelling of a timestamp cell the lane can render back
/// exactly; `None` for any other cell.
fn reproducible(cell: &Json) -> Option<(Spelling, u32)> {
    match cell {
        Json::Object(o) => {
            let epoch = u32::try_from(o.get("epoch")?.as_i64()?).ok()?;
            let exact = o.len() == 3
                && o.get("usec")?.as_i64()? == 0
                && o.get("t")?.as_str()? == utc_text(epoch);
            exact.then_some((Spelling::Object, epoch))
        }
        other => Some((Spelling::Bare, u32::try_from(other.as_i64()?).ok()?)),
    }
}

/// The one spelling every non-NULL cell is reproducible in: `Some(None)` when
/// there is no such cell, `None` when one is not reproducible or two differ.
fn uniform_spelling(cells: &[Json]) -> Option<Option<Spelling>> {
    let mut found = None;
    for cell in cells.iter().filter(|cell| !cell.is_null()) {
        let (spelling, _) = reproducible(cell)?;
        if *found.get_or_insert(spelling) != spelling {
            return None;
        }
    }
    Some(found)
}

/// The integer `column` holds at `position`, from its lane or its cells.
pub(crate) fn int_at(column: &Column, position: usize) -> Option<i64> {
    if lane_stored(column) {
        let lane = column.lane.as_ref()?;
        if lane.kind != LaneKind::Int || is_null(&lane.validity, position) {
            return None;
        }
        return lane.values.get(position).map(|&v| i64::from(v));
    }
    column.cells.get(position)?.as_i64()
}

/// One captured column: the source-owned cells and, when its type is laned,
/// their `u32` view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    /// The PostgreSQL type, kept so the lane can be rebuilt after a feed.
    pub sql_type: String,
    pub nullable: bool,
    /// The cells exactly as captured, one per row. A column whose lane can
    /// render every one of them back holds none: its values are the lane, a
    /// text's through the table's code table.
    pub cells: Vec<Json>,
    /// `None` when the PostgreSQL type has no lane or a live cell does not fit it.
    pub lane: Option<Lane>,
}

/// Where the rows of a table live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// The row with primary key `k` lives at position `k`: the key is the
    /// position, so a edge column is already a position lane.
    Identity,
    /// Rows are appended in the order they arrive; the tiles of the table's
    /// DR map hold the keys in that order.
    Append,
}

/// One captured table.
///
/// Plane/lane layout handed to the executor (see [`Table::slots`]):
/// `masks[0]` = `live` (dead positions have the bit cleared); then only the laned columns, in column order: the
/// k-th laned column is `Col(k)`, its lane is `lanes[k]` and its validity is
/// `masks[1 + k]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    /// The number of positions, live or dead.
    pub rows: usize,
    /// One bit per position, `rows.div_ceil(64)` words, tail bits zero;
    /// a deleted row's bit is cleared and its cells stay.
    pub live: Vec<u64>,
    /// Every column in capture order, laned or not.
    pub columns: Vec<Column>,
    /// `Identity` when the primary key is the position, `Append` otherwise.
    pub layout: Layout,
    /// Bumped once per applied feed event that touches the table and once per
    /// code advance, so the store's `source` revision is always `frame + 1`.
    pub frame: u64,
}

/// A laned column with the executor numbering it was handed.
struct Slot<'a> {
    col: Col,
    validity: Mask,
    column: &'a Column,
    lane: &'a Lane,
}

impl Table {
    fn slots(&self) -> impl Iterator<Item = Slot<'_>> {
        self.columns
            .iter()
            .filter_map(|column| column.lane.as_ref().map(|lane| (column, lane)))
            .enumerate()
            .filter_map(|(k, (column, lane))| {
                let col = u16::try_from(k).ok()?;
                Some(Slot {
                    col: Col(col),
                    validity: Mask(col.checked_add(1)?),
                    column,
                    lane,
                })
            })
    }

    /// The `(value, validity)` pairs of the nullable laned columns, as
    /// `sql_where` wants them.
    #[cfg(not(feature = "disable-sql-where"))]
    fn nullable(&self) -> Vec<(Col, Mask)> {
        self.slots()
            .filter(|s| s.column.nullable)
            .map(|s| (s.col, s.validity))
            .collect()
    }
}

/// A position list of one table, in list order: a bag, never sorted or
/// deduplicated, readable only under the frame the table had when it was issued.
struct Selection {
    /// Index into [`Catalog::tables`].
    table: usize,
    frame: Frame,
    /// The row order the positions are in.
    space: CoordSpace,
    positions: Vec<u32>,
}

/// The captured tables of one size, in name order.
#[derive(Default)]
pub struct Catalog {
    pub tables: Vec<Table>,
    /// One per table, same index: DR's position map, whose frame store is the
    /// one a binding of the table is checked against.
    maps: Vec<MapTable>,
    /// One per table, same index: the strings of its text columns, under `maps`' frame store.
    dicts: Vec<Dictionary>,
    classes: Option<ClassMap>,
    /// Position lists issued by [`Catalog::issue_selection`], indexed by selection id.
    selections: Vec<Selection>,
    link_stats: Cell<LinkStats>,
    like_stats: Cell<LikeStats>,
    under_stats: Cell<UnderStats>,
    word_stats: RefCell<WordStats>,
    census: Census,
    relations: Vec<Relation>,
    relation_stats: RelationStats,
    /// The oracle switch of [`Catalog::use_translated_positions`].
    translated_positions: bool,
    /// Every table is laid out in arrival order, to exercise the sparse-key fallback.
    append_only: bool,
}

impl fmt::Debug for Catalog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Catalog")
            .field("tables", &self.tables)
            .finish_non_exhaustive()
    }
}

/// The capture JSON did not have the promised shape or a cell did not fit its lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureError(pub String);

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "capture: {}", self.0)
    }
}

impl std::error::Error for CaptureError {}

fn capture_err<T>(message: String) -> Result<T, CaptureError> {
    Err(CaptureError(message))
}

/// The lane kind of a PostgreSQL type. `date` and `double precision` (like
/// `numeric` and `jsonb`) have none: such a column keeps its cells, which
/// [`rows`] renders back as fed, and a filter or ORDER BY on it is refused as
/// [`Refusal::UnlanedColumn`].
fn lane_kind(sql_type: &str) -> Option<LaneKind> {
    if sql_type == "boolean" {
        Some(LaneKind::Bool)
    } else if sql_type.starts_with("integer") || sql_type.starts_with("bigint") {
        Some(LaneKind::Int)
    } else if sql_type.starts_with("timestamp") {
        Some(LaneKind::Timestamp)
    } else if sql_type.starts_with("character varying") || sql_type == "text" {
        Some(LaneKind::Text)
    } else {
        None
    }
}

/// The lane value of a non-NULL cell; `None` when it does not fit.
fn cell_value(kind: LaneKind, cell: &Json) -> Option<u32> {
    match kind {
        LaneKind::Bool => cell.as_bool().map(u32::from),
        LaneKind::Int => cell.as_i64().and_then(|v| u32::try_from(v).ok()),
        LaneKind::Timestamp => {
            let epoch = match cell {
                Json::Object(o) => {
                    if o.get("usec").and_then(Json::as_i64)? != 0 {
                        return None;
                    }
                    o.get("epoch")?.as_i64()?
                }
                other => other.as_i64()?,
            };
            u32::try_from(epoch).ok()
        }
        LaneKind::Text => None,
    }
}

/// The distinct live, non-NULL strings of a text column, sorted; `None` when a live cell is not a string.
#[cfg(any(feature = "disable-code-advance", feature = "disable-stable-codes"))]
fn live_strings(cells: &[Json], is_live: impl Fn(usize) -> bool) -> Option<Vec<&str>> {
    let mut distinct = BTreeSet::new();
    for (row, cell) in cells.iter().enumerate() {
        if is_live(row) && !cell.is_null() {
            distinct.insert(cell.as_str()?);
        }
    }
    Some(distinct.into_iter().collect())
}

/// What a lane rebuild may read of a table's code spaces: the generation in force now.
#[cfg_attr(
    any(feature = "disable-code-advance", feature = "disable-stable-codes"),
    allow(dead_code)
)]
struct Book<'a> {
    codes: &'a CodeTable<u32>,
    frame: Frame,
    classes: Option<&'a ClassMap>,
    next: &'a BTreeMap<CodeSpaceId, u32>,
}

#[cfg_attr(
    any(feature = "disable-code-advance", feature = "disable-stable-codes"),
    allow(dead_code)
)]
impl<'a> Book<'a> {
    fn new(map: &MapTable, dict: &'a Dictionary, classes: Option<&'a ClassMap>) -> Self {
        Book {
            codes: &dict.codes,
            frame: map.seal(),
            classes,
            next: &dict.next,
        }
    }

    fn code(&self, space: &CodeSpaceId, text: &str) -> Option<u32> {
        member_in(self.codes, &self.frame, space, text)
    }

    fn next_free(&self, space: &CodeSpaceId) -> u32 {
        self.next.get(space).copied().unwrap_or(0)
    }

    fn text(&self, space: &CodeSpaceId, code: u32) -> Option<String> {
        let key = self.codes.key(&self.frame, space, code).ok()?;
        Some(key.0.clone())
    }
}

/// The member `text` names in `space` as of `frame`: the lookup half of
/// [`Catalog::from_string`] and of the intake's [`assign`].
fn member_in(
    codes: &CodeTable<u32>,
    frame: &Frame,
    space: &CodeSpaceId,
    text: &str,
) -> Option<u32> {
    codes.code(frame, space, &ValueKey(text.to_owned())).ok()
}

/// The code of each distinct live string of a text column, and the entries to advance its space with.
type Assigned<'a> = (BTreeMap<&'a str, u32>, Vec<(String, u32)>);

/// The feed intake: the one place that may add a member to a code space. A
/// string the space holds keeps its code (the lookup `from_string` shares); the
/// entries are the strings it does not hold yet, each at the next free index in
/// order of first appearance. `None` when a live cell is not a string.
#[cfg(not(any(feature = "disable-code-advance", feature = "disable-stable-codes")))]
fn assign<'a>(
    cells: &'a [Json],
    is_live: impl Fn(usize) -> bool,
    book: Option<(&Book<'_>, &CodeSpaceId)>,
) -> Option<Assigned<'a>> {
    let mut next = book.map_or(0, |(b, space)| b.next_free(space));
    let mut codes = BTreeMap::new();
    let mut fresh = Vec::new();
    for (row, cell) in cells.iter().enumerate() {
        if !is_live(row) || cell.is_null() {
            continue;
        }
        let text = cell.as_str()?;
        if codes.contains_key(text) {
            continue;
        }
        let code = match book.and_then(|(b, space)| b.code(space, text)) {
            Some(code) => code,
            None => {
                let code = next;
                next = next.checked_add(1)?;
                fresh.push(((*text).to_owned(), code));
                code
            }
        };
        codes.insert(text, code);
    }
    Some((codes, fresh))
}

/// Each distinct live string at its position among the sorted live strings; the
/// entries are that whole mapping.
#[cfg(any(feature = "disable-code-advance", feature = "disable-stable-codes"))]
fn assign<'a>(
    cells: &'a [Json],
    is_live: impl Fn(usize) -> bool,
    _book: Option<(&Book<'_>, &CodeSpaceId)>,
) -> Option<Assigned<'a>> {
    let strings = live_strings(cells, is_live)?;
    let codes: BTreeMap<&str, u32> = strings.into_iter().zip(0u32..).collect();
    let entries = codes.iter().map(|(s, c)| ((*s).to_owned(), *c)).collect();
    Some((codes, entries))
}

/// The lane of a column and, for a text column, the entries its code space is advanced with.
fn build_lane(
    sql_type: &str,
    cells: &[Json],
    is_live: impl Fn(usize) -> bool,
    book: Option<(&Book<'_>, &CodeSpaceId)>,
) -> Option<(Lane, Vec<(String, u32)>)> {
    let kind = lane_kind(sql_type)?;
    let mut validity = vec![0u64; words_for(cells.len())];
    let mut values = Vec::with_capacity(cells.len());
    let spelling = (kind == LaneKind::Timestamp && LANE_STORE)
        .then(|| uniform_spelling(cells))
        .flatten();
    let (codes, entries) = if kind == LaneKind::Text {
        assign(cells, &is_live, book)?
    } else {
        (BTreeMap::new(), Vec::new())
    };
    for (row, cell) in cells.iter().enumerate() {
        if cell.is_null() {
            values.push(0);
            continue;
        }
        if !is_live(row) {
            // A removed row keeps what its lane held; a value that no longer
            // fits must not cost the column its lane.
            let kept = (LANE_STORE
                && kind != LaneKind::Text
                && (kind != LaneKind::Timestamp || spelling.is_some()))
            .then(|| cell_value(kind, cell))
            .flatten();
            values.push(kept.unwrap_or(0));
            if kept.is_some() {
                validity[row / 64] |= 1 << (row % 64);
            }
            continue;
        }
        let value = if kind == LaneKind::Text {
            *codes.get(cell.as_str()?)?
        } else {
            cell_value(kind, cell)?
        };
        values.push(value);
        validity[row / 64] |= 1 << (row % 64);
    }
    let lane = Lane {
        values,
        validity,
        kind,
        spelling: spelling.flatten(),
    };
    Some((lane, entries))
}

/// By column name, the entries to advance the code space of each text column that has a lane.
type TextSets = BTreeMap<String, Vec<(String, u32)>>;

/// What a feed entry writes to the laned text columns, by column index and
/// position. The strings stay in the feed event until the entry's lanes settle.
type TextWrites<'e> = BTreeMap<usize, BTreeMap<usize, Option<&'e str>>>;

/// The code space of each text column of `table`; none for any other column or without a book.
fn spaces_of(table: &Table, book: Option<&Book<'_>>) -> Vec<Option<CodeSpaceId>> {
    table
        .columns
        .iter()
        .map(|c| {
            let text = lane_kind(&c.sql_type) == Some(LaneKind::Text);
            book.filter(|_| text)
                .map(|b| code_space(b.classes, table, &c.name))
        })
        .collect()
}

/// Writes the entry's strings into the lane of a text column at the positions
/// still live, giving each string the code space does not hold the next free
/// code in position order. Returns the strings that gained a code.
fn settle_text(
    column: &mut Column,
    space: &CodeSpaceId,
    book: &Book<'_>,
    is_live: impl Fn(usize) -> bool,
    writes: Option<&BTreeMap<usize, Option<&str>>>,
) -> Result<Vec<(String, u32)>, CaptureError> {
    let Some(lane) = column.lane.as_mut() else {
        return Ok(Vec::new());
    };
    let mut next = book.next_free(space);
    let mut assigned: BTreeMap<&str, u32> = BTreeMap::new();
    let mut fresh = Vec::new();
    for (&position, text) in writes.into_iter().flatten() {
        if !is_live(position) {
            continue;
        }
        let (Some(word), Some(slot)) = (
            lane.validity.get_mut(position / 64),
            lane.values.get_mut(position),
        ) else {
            return capture_err(format!("{}: position {position} has no lane", column.name));
        };
        let bit = 1u64 << (position % 64);
        let Some(text) = text else {
            *slot = 0;
            *word &= !bit;
            continue;
        };
        let code = match book
            .code(space, text)
            .or_else(|| assigned.get(text).copied())
        {
            Some(code) => code,
            None => {
                let code = next;
                next = next
                    .checked_add(1)
                    .ok_or_else(|| CaptureError(format!("{}: code space is full", column.name)))?;
                assigned.insert(text, code);
                fresh.push(((*text).to_owned(), code));
                code
            }
        };
        *slot = code;
        *word |= bit;
    }
    Ok(fresh)
}

/// Rebuilds every lane of `table` that has cells, skipping dead positions, and
/// settles the entry's `writes` into the lanes that have none; the codes of a
/// text column come from `book`, or from a fresh space without one. A lane
/// built from cells drops them when it renders every one back.
fn relane(
    table: &mut Table,
    book: Option<&Book<'_>>,
    writes: &TextWrites<'_>,
) -> Result<TextSets, CaptureError> {
    let spaces = spaces_of(table, book);
    let live = &table.live;
    let is_live = |row: usize| {
        live.get(row / 64)
            .is_some_and(|w| (w >> (row % 64)) & 1 == 1)
    };
    let mut sets = TextSets::new();
    for (c, (column, space)) in table.columns.iter_mut().zip(&spaces).enumerate() {
        if lane_stored(column) {
            if column
                .lane
                .as_ref()
                .is_some_and(|lane| lane.kind == LaneKind::Text)
            {
                let (Some(book), Some(space)) = (book, space.as_ref()) else {
                    continue;
                };
                let fresh = settle_text(column, space, book, is_live, writes.get(&c))?;
                sets.insert(column.name.clone(), fresh);
            }
            let missing = column
                .lane
                .as_ref()
                .filter(|_| !column.nullable)
                .and_then(|lane| {
                    (0..lane.values.len()).find(|&r| is_live(r) && is_null(&lane.validity, r))
                });
            if let Some(row) = missing {
                return capture_err(format!(
                    "{}.{}: NULL in a NOT NULL column at row {row}",
                    table.name, column.name
                ));
            }
            continue;
        }
        if !column.nullable
            && let Some(row) = column
                .cells
                .iter()
                .enumerate()
                .position(|(r, cell)| is_live(r) && cell.is_null())
        {
            return capture_err(format!(
                "{}.{}: NULL in a NOT NULL column at row {row}",
                table.name, column.name
            ));
        }
        let lookup = book.zip(space.as_ref());
        column.lane =
            build_lane(&column.sql_type, &column.cells, is_live, lookup).map(|(lane, entries)| {
                if lane.kind == LaneKind::Text {
                    sets.insert(column.name.clone(), entries);
                }
                lane
            });
        if lane_can_store(column)
            && column
                .lane
                .as_ref()
                .is_some_and(|lane| lane_holds_removed(lane, &column.cells, is_live))
        {
            column.cells = Vec::new();
        }
    }
    Ok(sets)
}

/// A snapshot column carries its `rows` cells; a feed declaration (`declared`)
/// carries none and starts empty.
fn read_column(
    table: &str,
    rows: usize,
    name: &str,
    spec: &Json,
    declared: bool,
) -> Result<Column, CaptureError> {
    let at = format!("{table}.{name}");
    let Some(sql_type) = spec.get("type").and_then(Json::as_str) else {
        return capture_err(format!("{at}: no type"));
    };
    let Some(nullable) = spec.get("null").and_then(Json::as_bool) else {
        return capture_err(format!("{at}: no null flag"));
    };
    let cells = match spec.get("values").and_then(Json::as_array) {
        Some(cells) => cells.clone(),
        None if declared => Vec::new(),
        None => return capture_err(format!("{at}: no values")),
    };
    if cells.len() != rows {
        return capture_err(format!("{at}: {} values for {rows} rows", cells.len()));
    }
    Ok(Column {
        name: name.to_owned(),
        sql_type: sql_type.to_owned(),
        nullable,
        cells,
        lane: None,
    })
}

/// Column specs in capture order: an object is read in name order, an array
/// in the table's own order.
fn column_specs<'a>(
    table: &str,
    columns: &'a Json,
) -> Result<Vec<(&'a str, &'a Json)>, CaptureError> {
    match columns {
        Json::Object(map) => {
            let mut specs: Vec<(&str, &Json)> = map.iter().map(|(k, v)| (k.as_str(), v)).collect();
            specs.sort_by_key(|(name, _)| *name);
            Ok(specs)
        }
        Json::Array(list) => list
            .iter()
            .map(|spec| {
                spec.get("name")
                    .and_then(Json::as_str)
                    .map(|name| (name, spec))
                    .ok_or_else(|| CaptureError(format!("{table}: column without a name")))
            })
            .collect(),
        _ => capture_err(format!("{table}: columns is neither object nor array")),
    }
}

/// For each of the table's columns, its position in the feed entry's
/// `columns` declaration; `None` when the entry declares none, or declares
/// them in the table's own order. A declaration naming other columns is
/// refused rather than read by position.
fn incoming_order(table: &Table, entry: &Json) -> Result<Option<Vec<usize>>, CaptureError> {
    let Some(columns) = entry.get("columns") else { return Ok(None) };
    let declared: Vec<&str> = column_specs(&table.name, columns)?.into_iter().map(|(n, _)| n).collect();
    if declared.iter().copied().eq(table.columns.iter().map(|c| c.name.as_str())) {
        return Ok(None);
    }
    let order = table
        .columns
        .iter()
        .map(|c| declared.iter().position(|d| *d == c.name))
        .collect::<Option<Vec<usize>>>();
    match order {
        Some(order) if declared.len() == table.columns.len() => Ok(Some(order)),
        _ => capture_err(format!(
            "{}: feed columns {declared:?} differ from the table's",
            table.name
        )),
    }
}

fn live_plane(rows: usize) -> Vec<u64> {
    let mut live = vec![u64::MAX; words_for(rows)];
    if !rows.is_multiple_of(64)
        && let Some(last) = live.last_mut()
    {
        *last = (1u64 << (rows % 64)) - 1;
    }
    live
}

impl Catalog {
    /// Reads the `"tables"` object of one size entry of either capture format,
    /// and declares the `"relations"` the capture states, if it has any.
    pub fn from_capture(size_entry: &Json) -> Result<Catalog, CaptureError> {
        Self::read_capture(size_entry, false)
    }

    /// [`Catalog::from_capture`] with every table laid out in arrival order.
    #[doc(hidden)]
    pub fn from_capture_append(size_entry: &Json) -> Result<Catalog, CaptureError> {
        Self::read_capture(size_entry, true)
    }

    fn read_capture(size_entry: &Json, append_only: bool) -> Result<Catalog, CaptureError> {
        let Some(tables) = size_entry.get("tables").and_then(Json::as_object) else {
            return capture_err("no tables object".to_owned());
        };
        let mut out = Vec::with_capacity(tables.len());
        for (name, spec) in tables {
            let rows = spec
                .get("rows")
                .and_then(Json::as_u64)
                .and_then(|n| usize::try_from(n).ok());
            let Some(rows) = rows else {
                return capture_err(format!("{name}: no row count"));
            };
            let Some(columns) = spec.get("columns") else {
                return capture_err(format!("{name}: no columns"));
            };
            let columns = column_specs(name, columns)?
                .into_iter()
                .map(|(col, col_spec)| read_column(name, rows, col, col_spec, false))
                .collect::<Result<Vec<_>, _>>()?;
            let mut table = snapshot_table(name, rows, columns, append_only);
            let sets = relane(&mut table, None, &TextWrites::new())?;
            out.push((table, sets));
        }
        out.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        let (out, sets): (Vec<Table>, Vec<TextSets>) = out.into_iter().unzip();
        let maps: Vec<MapTable> = out
            .iter()
            .map(|table| {
                let mut map = MapTable::new(table.frame + 1, 1);
                map.advance(table.frame + 1, 1, tile_changes(table, 0));
                map
            })
            .collect();
        let dicts = maps.iter().map(Dictionary::new).collect();
        let mut catalog = Catalog {
            tables: out,
            maps,
            dicts,
            classes: None,
            selections: Vec::new(),
            link_stats: Cell::default(),
            like_stats: Cell::default(),
            under_stats: Cell::default(),
            word_stats: RefCell::default(),
            census: Census::default(),
            relations: Vec::new(),
            relation_stats: RelationStats::default(),
            translated_positions: false,
            append_only,
        };
        for (index, sets) in sets.iter().enumerate() {
            catalog.advance_codes_of(index, sets)?;
        }
        if let Some(relations) = size_entry.get("relations") {
            catalog.declare_feed(relations)?;
        }
        Ok(catalog)
    }

    /// Names the OGAR class of each table, for the next [`Catalog::bind`], and
    /// declares the `belongs_to` edges the class basis states between them.
    pub fn set_classes(&mut self, classes: ClassMap) -> Result<(), CaptureError> {
        let (edges, skipped) = classes.edges();
        self.relation_stats = RelationStats {
            declared: edges.len(),
            skipped,
        };
        let before = self.classes.replace(classes);
        for ((table, map), dict) in self
            .tables
            .iter_mut()
            .zip(self.maps.iter_mut())
            .zip(self.dicts.iter_mut())
        {
            rekey_codes(map, dict, table, before.as_ref(), self.classes.as_ref())?;
        }
        for edge in edges {
            self.declare(edge)?;
        }
        Ok(())
    }

    /// Joins read through a stored translation of target positions: the
    /// translation is the oracle the default path is compared against.
    pub fn use_translated_positions(&mut self, on: bool) {
        self.translated_positions = on;
    }

    /// An empty catalog, to be filled by [`Catalog::apply_feed`].
    pub fn new() -> Catalog {
        Catalog::default()
    }

    /// [`Catalog::new`] whose declared tables are all laid out in arrival order.
    #[doc(hidden)]
    pub fn new_append() -> Catalog {
        Catalog {
            append_only: true,
            ..Catalog::default()
        }
    }

    /// Global-to-local lookups the current tiles of every table have served.
    pub fn map_lookups(&self) -> u64 {
        self.tables
            .iter()
            .zip(&self.maps)
            .map(|(table, map)| {
                let frame = map.seal();
                tiles_of(map, &frame, table.rows)
                    .into_iter()
                    .filter_map(|tile| map.map(&frame, tile).ok())
                    .map(|view| view.lookups())
                    .sum::<u64>()
            })
            .sum()
    }

    /// How many tiles the DR map of the table `name` holds.
    #[doc(hidden)]
    pub fn map_tiles(&self, name: &str) -> usize {
        let Some(index) = self.tables.iter().position(|t| t.name == name) else {
            return 0;
        };
        match (self.tables.get(index), self.maps.get(index)) {
            (Some(table), Some(map)) => tiles_of(map, &map.seal(), table.rows).len(),
            _ => 0,
        }
    }

    /// Whether `col` of table `index` is its primary key: an integer `id` the
    /// table's map holds for every position, unique by construction.
    pub(crate) fn is_key(&self, index: usize, col: &str) -> bool {
        let (Some(table), Some(map)) = (self.tables.get(index), self.maps.get(index)) else {
            return false;
        };
        col == "id"
            && has_integer_id(&table.columns)
            && tiles_of(map, &map.seal(), table.rows).len() == table.rows.div_ceil(TILE)
    }

    /// Applies the `"feed"` value of one committed-writes event: declares
    /// unknown tables, then upserts and deletes rows, then rebuilds the lanes
    /// of every touched table. A table is replaced only when its entry applied.
    pub fn apply_feed(&mut self, feed: &Json) -> Result<(), CaptureError> {
        let tables = feed.get("tables").and_then(Json::as_object);
        let relations = feed.get("relations");
        if tables.is_none() && relations.is_none() {
            return capture_err("feed without a tables object".to_owned());
        }
        if let Some(relations) = relations {
            self.declare_feed(relations)?;
        }
        for (name, entry) in tables.into_iter().flatten() {
            let found = self.tables.binary_search_by(|t| t.name.as_str().cmp(name));
            let (mut table, mut fresh) = match found {
                Ok(i) => (
                    self.tables
                        .get(i)
                        .cloned()
                        .ok_or_else(|| CaptureError(format!("{name}: table index")))?,
                    None,
                ),
                Err(_) => (
                    declare_table(name, entry, self.append_only)?,
                    Some(MapTable::new(1, 1)),
                ),
            };
            let before = table.rows;
            let mut fresh_dict = fresh.as_ref().map(Dictionary::new);
            let current = fresh
                .as_ref()
                .or_else(|| found.ok().and_then(|i| self.maps.get(i)))
                .ok_or_else(|| CaptureError(format!("{name}: no map")))?;
            let dict = fresh_dict
                .as_ref()
                .or_else(|| found.ok().and_then(|i| self.dicts.get(i)))
                .ok_or_else(|| CaptureError(format!("{name}: no dictionary")))?;
            let book = Book::new(current, dict, self.classes.as_ref());
            let sets = apply_entry(
                &mut table,
                entry,
                &Positions::new(current, before, &self.census),
                &book,
            )?;
            table.frame += 1;
            let changes = tile_changes(&table, before);
            let index = match found {
                Ok(i) => {
                    if let Some(map) = self.maps.get_mut(i) {
                        advance(map, &table, changes);
                    }
                    if let Some(slot) = self.tables.get_mut(i) {
                        *slot = table;
                    }
                    i
                }
                Err(i) => {
                    for held in self.selections.iter_mut().filter(|s| s.table >= i) {
                        held.table += 1;
                    }
                    let mut map = fresh.take().unwrap_or_else(|| MapTable::new(1, 1));
                    advance(&mut map, &table, changes);
                    self.tables.insert(i, table);
                    let dict = fresh_dict.take().unwrap_or_else(|| Dictionary::new(&map));
                    self.dicts.insert(i, dict);
                    self.maps.insert(i, map);
                    i
                }
            };
            self.advance_codes_of(index, &sets)?;
        }
        Ok(())
    }

    /// The code advance of table `index`, after its tiles advanced.
    fn advance_codes_of(&mut self, index: usize, sets: &TextSets) -> Result<(), CaptureError> {
        let (Some(table), Some(map), Some(dict)) = (
            self.tables.get_mut(index),
            self.maps.get_mut(index),
            self.dicts.get_mut(index),
        ) else {
            return capture_err(format!("table {index}: no map"));
        };
        advance_codes(map, dict, table, sets, self.classes.as_ref())
    }

    /// Code advances made so far, over every table.
    pub fn code_advances(&self) -> u64 {
        self.dicts.iter().map(|d| d.advances).sum()
    }

    /// Code advances made for the table `name`.
    pub fn table_code_advances(&self, name: &str) -> u64 {
        let at = self.tables.iter().position(|t| t.name == name);
        at.and_then(|i| self.dicts.get(i)).map_or(0, |d| d.advances)
    }

    /// The frame of the table `name` as it is now.
    #[doc(hidden)]
    pub fn seal_of(&self, name: &str) -> Option<Frame> {
        let at = self.tables.iter().position(|t| t.name == name)?;
        self.maps.get(at).map(MapTable::seal)
    }

    /// The code the string `literal` of `table.column` had as of `frame`, read from the code table alone.
    #[doc(hidden)]
    pub fn code_in(
        &self,
        table: &str,
        column: &str,
        literal: &str,
        frame: &Frame,
    ) -> Result<u32, CodeRead> {
        let at = self.tables.iter().position(|t| t.name == table);
        let (Some(t), Some(dict)) = (
            at.and_then(|i| self.tables.get(i)),
            at.and_then(|i| self.dicts.get(i)),
        ) else {
            return Err(CodeRead::NoSpace);
        };
        dict.codes.code(
            frame,
            &code_space(self.classes.as_ref(), t, column),
            &ValueKey(literal.to_owned()),
        )
    }

    /// The code of `literal` in the code space `class`/`property` of `table` as of `frame`.
    #[doc(hidden)]
    pub fn code_in_space(
        &self,
        table: &str,
        class: u32,
        property: &str,
        literal: &str,
        frame: &Frame,
    ) -> Result<u32, CodeRead> {
        let dict = self
            .tables
            .iter()
            .position(|t| t.name == table)
            .and_then(|i| self.dicts.get(i))
            .ok_or(CodeRead::NoSpace)?;
        let space = CodeSpaceId {
            class,
            property: property.to_owned(),
        };
        dict.codes
            .code(frame, &space, &ValueKey(literal.to_owned()))
    }

    /// The class and property of the code space of `table.column`.
    #[doc(hidden)]
    pub fn code_space_of(&self, table: &str, column: &str) -> Option<(u32, String)> {
        let t = self.tables.iter().find(|t| t.name == table)?;
        t.columns.iter().find(|c| c.name == column)?;
        let space = code_space(self.classes.as_ref(), t, column);
        Some((space.class, space.property))
    }

    /// The position the DR map of `table` places the row with primary key
    /// `key` at; `None` when the map holds no such key and when the row at its
    /// position is not live (deleted, or never a row), in either layout: a key
    /// that names no live row resolves to no position.
    #[doc(hidden)]
    pub fn position_of(&self, table: &str, key: i64) -> Option<usize> {
        let at = self.tables.iter().position(|t| t.name == table)?;
        let (table, map) = (self.tables.get(at)?, self.maps.get(at)?);
        let position = Positions::new(map, table.rows, &self.census).find(table, key)?;
        (position < table.rows && is_live_at(table, position)).then_some(position)
    }

    /// Issues a list of positions of `table`, in list order, as a selection a
    /// bound read can be made under. The list is tied to the frame the table
    /// has now: a feed that touches the table makes it unreadable.
    ///
    /// It currently admits positions of rows that are not live (EXO-337); a
    /// read through such a selection is refused (`Refusal::DeadPosition`).
    #[doc(hidden)]
    pub fn issue_selection(
        &mut self,
        table: &str,
        positions: Vec<u32>,
    ) -> Result<u16, CaptureError> {
        self.issue_in_order(table, positions, None)
    }

    /// [`Catalog::issue_selection`] for positions said to be in the row order
    /// of a table of `rows` positions, which is not the table's own.
    #[doc(hidden)]
    pub fn issue_selection_in_order_of(
        &mut self,
        table: &str,
        positions: Vec<u32>,
        rows: usize,
    ) -> Result<u16, CaptureError> {
        self.issue_in_order(table, positions, Some(rows))
    }

    fn issue_in_order(
        &mut self,
        table: &str,
        positions: Vec<u32>,
        order_of_rows: Option<usize>,
    ) -> Result<u16, CaptureError> {
        let Some(index) = self.tables.iter().position(|t| t.name == table) else {
            return capture_err(format!("{table}: no such table"));
        };
        let (Some(held), Some(map)) = (self.tables.get(index), self.maps.get(index)) else {
            return capture_err(format!("{table}: no map"));
        };
        let beyond = positions
            .iter()
            .find(|&&p| usize::try_from(p).is_ok_and(|p| p >= held.rows));
        if let Some(position) = beyond {
            return capture_err(format!(
                "{table}: position {position} is not one of its {} rows",
                held.rows
            ));
        }
        let sel = u16::try_from(self.selections.len())
            .or_else(|_| capture_err("too many selections".to_owned()))?;
        let frame = map.seal();
        let space = bound::space_with(held, order_of_rows.unwrap_or(held.rows));
        self.selections.push(Selection {
            table: index,
            frame,
            space,
            positions,
        });
        Ok(sel)
    }

    /// Whether every table's store `source` revision is its `frame + 1`.
    #[doc(hidden)]
    pub fn frames_agree(&self) -> bool {
        self.tables
            .iter()
            .zip(&self.maps)
            .all(|(table, map)| map.revisions().source == table.frame + 1)
    }
}

/// A table's text columns as DT's code table holds them, and how often they changed.
struct Dictionary {
    codes: CodeTable<u32>,
    advances: u64,
    /// The next free index of each code space; the code table lists no size.
    next: BTreeMap<CodeSpaceId, u32>,
}

impl Dictionary {
    fn new(map: &MapTable) -> Dictionary {
        Dictionary {
            codes: CodeTable::new(map.frames()),
            advances: 0,
            next: BTreeMap::new(),
        }
    }
}

/// The code space of a column no class field answers for: class 0, the default class.
fn default_space(table: &str, column: &str) -> CodeSpaceId {
    CodeSpaceId {
        class: 0,
        property: format!("{table}.{column}"),
    }
}

/// The code space of `table.column`: the bound class's id and the field's
/// predicate when the class basis resolves the column, else the default one.
#[cfg(not(feature = "disable-class-code-space"))]
fn code_space(classes: Option<&ClassMap>, table: &Table, column: &str) -> CodeSpaceId {
    classes
        .and_then(|c| c.space_of(table, column))
        .unwrap_or_else(|| default_space(&table.name, column))
}

#[cfg(feature = "disable-class-code-space")]
fn code_space(_classes: Option<&ClassMap>, table: &Table, column: &str) -> CodeSpaceId {
    default_space(&table.name, column)
}

/// Every entry of `space` as of `frame`, read back by code from 0 up: the code table lists no size.
#[cfg(not(feature = "disable-code-advance"))]
fn held(
    codes: &CodeTable<u32>,
    frame: &Frame,
    space: &CodeSpaceId,
) -> Result<Vec<(ValueKey, u32)>, CaptureError> {
    let mut out = Vec::new();
    for code in 0u32.. {
        match codes.key(frame, space, code) {
            Ok(key) => out.push((key.clone(), code)),
            Err(CodeRead::UnknownCode | CodeRead::NoSpace) => break,
            Err(other) => return capture_err(format!("{}: code table: {other:?}", space.property)),
        }
    }
    Ok(out)
}

/// Whether the code space holds exactly `entries` as of `frame`.
#[cfg(all(
    feature = "disable-stable-codes",
    not(feature = "disable-code-advance")
))]
fn holds(
    codes: &CodeTable<u32>,
    frame: &Frame,
    space: &CodeSpaceId,
    entries: &[(String, u32)],
) -> bool {
    let Ok(past_end) = u32::try_from(entries.len()) else {
        return false;
    };
    entries
        .iter()
        .all(|(s, code)| codes.code(frame, space, &ValueKey(s.clone())) == Ok(*code))
        && matches!(
            codes.key(frame, space, past_end),
            Err(CodeRead::NoSpace | CodeRead::UnknownCode)
        )
}

/// The whole mapping `space` must be advanced to, or `None` when `found` changes nothing.
#[cfg(not(any(feature = "disable-code-advance", feature = "disable-stable-codes")))]
fn pending(
    codes: &CodeTable<u32>,
    frame: &Frame,
    space: &CodeSpaceId,
    found: &[(String, u32)],
) -> Result<Option<Vec<(ValueKey, u32)>>, CaptureError> {
    if found.is_empty() {
        return Ok(None);
    }
    let mut entries = held(codes, frame, space)?;
    entries.extend(found.iter().map(|(s, c)| (ValueKey(s.clone()), *c)));
    Ok(Some(entries))
}

#[cfg(all(
    feature = "disable-stable-codes",
    not(feature = "disable-code-advance")
))]
fn pending(
    codes: &CodeTable<u32>,
    frame: &Frame,
    space: &CodeSpaceId,
    found: &[(String, u32)],
) -> Result<Option<Vec<(ValueKey, u32)>>, CaptureError> {
    if holds(codes, frame, space, found) {
        return Ok(None);
    }
    Ok(Some(
        found
            .iter()
            .map(|(s, c)| (ValueKey(s.clone()), *c))
            .collect(),
    ))
}

/// One code advance of a table, listing every text column whose space gains
/// strings, each with its whole mapping; none when no column did. The advance
/// moves the store's `source` revision by one, so `frame` moves with it.
#[cfg(not(feature = "disable-code-advance"))]
fn advance_codes(
    map: &mut MapTable,
    dict: &mut Dictionary,
    table: &mut Table,
    sets: &TextSets,
    classes: Option<&ClassMap>,
) -> Result<(), CaptureError> {
    let frame = map.seal();
    let mut changes = Vec::new();
    let mut sizes = Vec::new();
    for column in &table.columns {
        if lane_kind(&column.sql_type) != Some(LaneKind::Text) {
            continue;
        }
        let found = sets.get(&column.name).map_or(&[][..], Vec::as_slice);
        let space = code_space(classes, table, &column.name);
        let Some(entries) = pending(&dict.codes, &frame, &space, found)? else {
            continue;
        };
        let size = u32::try_from(entries.len())
            .map_err(|_| CaptureError(format!("{}: code space is full", table.name)))?;
        sizes.push((space.clone(), size));
        changes.push((space, entries));
    }
    if !changes.is_empty() {
        map.advance_codes(&mut dict.codes, changes)
            .map_err(|e| CaptureError(format!("{}: code advance: {e:?}", table.name)))?;
        dict.next.extend(sizes);
        dict.advances += 1;
        table.frame += 1;
    }
    #[cfg(not(feature = "disable-frame-advance"))]
    debug_assert_eq!(map.revisions().source, table.frame + 1, "{}", table.name);
    Ok(())
}

#[cfg(feature = "disable-code-advance")]
fn advance_codes(
    _map: &mut MapTable,
    _dict: &mut Dictionary,
    _table: &mut Table,
    _sets: &TextSets,
    _classes: Option<&ClassMap>,
) -> Result<(), CaptureError> {
    Ok(())
}

/// One code advance moving every text column whose code space changed from
/// `before` to `after`: the new space takes the old mapping unchanged, the old
/// one none, so the old name stops answering at the new frame.
#[cfg(not(feature = "disable-code-advance"))]
fn rekey_codes(
    map: &mut MapTable,
    dict: &mut Dictionary,
    table: &mut Table,
    before: Option<&ClassMap>,
    after: Option<&ClassMap>,
) -> Result<(), CaptureError> {
    let frame = map.seal();
    let mut changes = Vec::new();
    let mut retired = Vec::new();
    let mut moved = Vec::new();
    for column in &table.columns {
        if lane_kind(&column.sql_type) != Some(LaneKind::Text) {
            continue;
        }
        let (old, new) = (
            code_space(before, table, &column.name),
            code_space(after, table, &column.name),
        );
        if old == new {
            continue;
        }
        let entries = held(&dict.codes, &frame, &old)?;
        let size = u32::try_from(entries.len())
            .map_err(|_| CaptureError(format!("{}: code space is full", table.name)))?;
        moved.push((old.clone(), new.clone(), size));
        changes.push((new, entries));
        retired.push((old, Vec::new()));
    }
    retired.retain(|(old, _)| !changes.iter().any(|(new, _)| new == old));
    changes.extend(retired);
    if changes.is_empty() {
        return Ok(());
    }
    map.advance_codes(&mut dict.codes, changes)
        .map_err(|e| CaptureError(format!("{}: re-key: {e:?}", table.name)))?;
    for (old, _, _) in &moved {
        dict.next.remove(old);
    }
    for (_, new, size) in moved {
        dict.next.insert(new, size);
    }
    dict.advances += 1;
    table.frame += 1;
    #[cfg(not(feature = "disable-frame-advance"))]
    debug_assert_eq!(map.revisions().source, table.frame + 1, "{}", table.name);
    Ok(())
}

#[cfg(feature = "disable-code-advance")]
fn rekey_codes(
    _map: &mut MapTable,
    _dict: &mut Dictionary,
    _table: &mut Table,
    _before: Option<&ClassMap>,
    _after: Option<&ClassMap>,
) -> Result<(), CaptureError> {
    Ok(())
}

/// One advance of a table's map per applied feed entry: it issues the new
/// frame that makes an older binding stale.
fn advance(map: &mut MapTable, table: &Table, changes: Vec<(TileId, Option<LocalMap>)>) {
    #[cfg(feature = "disable-frame-advance")]
    if changes.is_empty() {
        return;
    }
    map.advance(table.frame + 1, 1, changes);
    debug_assert_eq!(map.revisions().source, table.frame + 1, "{}", table.name);
}

/// The tiles of `map` that exist among the first `rows` positions.
fn tiles_of(map: &MapTable, frame: &Frame, rows: usize) -> Vec<TileId> {
    (0..rows.div_ceil(TILE))
        .filter_map(|tile| u32::try_from(tile).ok())
        .filter(|&tile| map.map(frame, tile).is_ok())
        .collect()
}

/// The tiles an entry added or extended, given the `before` positions the
/// table held. A table laid out by key reserves whole tiles of keys; one laid
/// out in arrival order extends its last tile with the keys of the new rows.
fn tile_changes(table: &Table, before: usize) -> Vec<(TileId, Option<LocalMap>)> {
    if table.rows <= before || !has_integer_id(&table.columns) {
        return Vec::new();
    }
    let by_key = table.layout == Layout::Identity;
    let first = if by_key {
        before.div_ceil(TILE)
    } else {
        before / TILE
    };
    (first..table.rows.div_ceil(TILE))
        .filter_map(|tile| {
            let keys: Vec<u64> = if by_key {
                (tile * TILE..(tile + 1) * TILE)
                    .filter_map(|k| u64::try_from(k).ok())
                    .collect()
            } else {
                arrival_keys(table, tile)?
            };
            let map = LocalMap::prepare(Width::W256, &keys).ok()?;
            Some((u32::try_from(tile).ok()?, Some(map)))
        })
        .collect()
}

/// The keys of the positions of `tile`, in position order.
fn arrival_keys(table: &Table, tile: usize) -> Option<Vec<u64>> {
    let id = table.columns.iter().find(|c| c.name == "id")?;
    (tile * TILE..table.rows.min((tile + 1) * TILE))
        .map(|position| u64::try_from(int_at(id, position)?).ok())
        .collect()
}

/// Where the keys of a table sit, read through its DR map as it was when an
/// entry began; rows the entry itself added are not in the map yet.
struct Positions<'m> {
    map: &'m MapTable,
    frame: Frame,
    tiles: Vec<TileId>,
    before: usize,
    census: &'m Census,
}

impl<'m> Positions<'m> {
    fn new(map: &'m MapTable, before: usize, census: &'m Census) -> Self {
        let frame = map.seal();
        let tiles = tiles_of(map, &frame, before);
        Positions {
            map,
            frame,
            tiles,
            before,
            census,
        }
    }

    /// [`Positions::find`] for a join's late placement: a table laid out by
    /// key holds a key at that position, so no map is read.
    fn place(&self, table: &Table, key: i64) -> Option<usize> {
        if table.layout == Layout::Identity {
            self.census.resolve_key();
            return usize::try_from(key).ok().filter(|&at| at < table.rows);
        }
        self.find(table, key)
    }

    /// The position `key` has ever held in `table`, live or not.
    fn find(&self, table: &Table, key: i64) -> Option<usize> {
        self.census.resolve_map();
        let found = self.find_in_map(table, key)?;
        #[cfg(feature = "disable-cube-placement")]
        if key == 1 {
            return found.checked_add(1);
        }
        Some(found)
    }

    fn find_in_map(&self, table: &Table, key: i64) -> Option<usize> {
        if table.layout == Layout::Identity {
            let key = u64::try_from(key).ok()?;
            let tile = u32::try_from(key / TILE as u64).ok()?;
            return if self.tiles.binary_search(&tile).is_ok() {
                let local = self.map.map(&self.frame, tile).ok()?.local(key).ok()?;
                Some(usize::try_from(tile).ok()? * TILE + usize::from(local))
            } else {
                Some(usize::try_from(key).ok()?).filter(|&at| at < table.rows)
            };
        }
        let id = table.columns.iter().find(|c| c.name == "id")?;
        if let Some(at) =
            (self.before..table.rows).find(|&position| int_at(id, position) == Some(key))
        {
            return Some(at);
        }
        let (ordinal, local) = self
            .map
            .locate(&self.frame, &self.tiles, u64::try_from(key).ok()?)
            .ok()?;
        let tile = self.tiles.get(usize::try_from(ordinal).ok()?)?;
        Some(usize::try_from(*tile).ok()? * TILE + usize::from(local))
    }
}

/// Whether a key may be laid out at its own position beside `live` rows.
fn within_density(max_id: i64, live: usize) -> bool {
    let limit = u64::try_from(live)
        .ok()
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(1024));
    match (u64::try_from(max_id), limit) {
        (Ok(max), Some(limit)) => max <= limit && max <= u64::from(u32::MAX),
        _ => false,
    }
}

fn has_integer_id(columns: &[Column]) -> bool {
    columns
        .iter()
        .any(|c| c.name == "id" && lane_kind(&c.sql_type) == Some(LaneKind::Int))
}

/// The key of every row of a snapshot, when they are distinct non-negative
/// integers dense enough for the row with key `k` to live at position `k`.
fn identity_keys(columns: &[Column], rows: usize) -> Option<Vec<usize>> {
    if !has_integer_id(columns) {
        return None;
    }
    let id = columns.iter().find(|c| c.name == "id")?;
    let mut seen = BTreeSet::new();
    let mut keys = Vec::with_capacity(rows);
    for cell in &id.cells {
        let key = cell.as_i64()?;
        if !within_density(key, rows) || !seen.insert(key) {
            return None;
        }
        keys.push(usize::try_from(key).ok()?);
    }
    Some(keys)
}

/// A snapshot table: identity layout when its keys allow it, else the rows as captured.
fn snapshot_table(name: &str, rows: usize, mut columns: Vec<Column>, append_only: bool) -> Table {
    let keys = identity_keys(&columns, rows).filter(|_| !append_only);
    let Some(keys) = keys else {
        return Table {
            name: name.to_owned(),
            rows,
            live: live_plane(rows),
            columns,
            layout: Layout::Append,
            frame: 0,
        };
    };
    let positions = keys.iter().max().map_or(0, |max| max + 1);
    for column in &mut columns {
        let mut cells = vec![Json::Null; positions];
        for (cell, &key) in column.cells.iter_mut().zip(&keys) {
            if let Some(slot) = cells.get_mut(key) {
                *slot = std::mem::take(cell);
            }
        }
        column.cells = cells;
    }
    let mut table = Table {
        name: name.to_owned(),
        rows: positions,
        live: vec![0; words_for(positions)],
        columns,
        layout: Layout::Identity,
        frame: 0,
    };
    for &key in &keys {
        set_live(&mut table, key, true);
    }
    table
}

fn declare_table(name: &str, entry: &Json, append_only: bool) -> Result<Table, CaptureError> {
    let Some(spec) = entry.get("columns") else {
        return capture_err(format!("{name}: unknown table without columns"));
    };
    let columns = column_specs(name, spec)?
        .into_iter()
        .map(|(col, col_spec)| read_column(name, 0, col, col_spec, true))
        .collect::<Result<Vec<_>, _>>()?;
    let layout = if !append_only && declared_as_identity(&columns, entry) {
        Layout::Identity
    } else {
        Layout::Append
    };
    Ok(Table {
        name: name.to_owned(),
        rows: 0,
        live: Vec::new(),
        columns,
        layout,
        frame: 0,
    })
}

/// A declared table is laid out by key unless its first rows already break the density bound.
fn declared_as_identity(columns: &[Column], entry: &Json) -> bool {
    if !has_integer_id(columns) || entry.get("replace").is_some() {
        return false;
    }
    let Some(id_col) = columns.iter().position(|c| c.name == "id") else {
        return false;
    };
    let upserts = entry.get("upsert").and_then(Json::as_array);
    let keys: Option<BTreeSet<i64>> = upserts
        .into_iter()
        .flatten()
        .map(|row| row.get(id_col).and_then(Json::as_i64))
        .collect();
    keys.is_some_and(|keys| {
        keys.last()
            .is_none_or(|&max| within_density(max, keys.len()))
            && keys.first().is_none_or(|&min| min >= 0)
    })
}

/// The position of the new row with key `id` of an identity table holding
/// `live` rows, growing the table to hold it.
fn grow_to_key(table: &mut Table, id: i64, live: usize) -> Result<usize, CaptureError> {
    let name = &table.name;
    let position = usize::try_from(id)
        .map_err(|_| CaptureError(format!("{name}: id {id} is not a position")))?;
    if position >= table.rows {
        if !within_density(id, live + 1) {
            return capture_err(format!("{name}: id {id} breaks the density bound"));
        }
        for column in &mut table.columns {
            resize_column(column, position + 1);
        }
        table.rows = position + 1;
        table.live.resize(words_for(table.rows), 0);
    }
    Ok(position)
}

/// Gives a column `rows` positions: NULL cells, or for a lane that holds no cells value 0 with validity 0.
fn resize_column(column: &mut Column, rows: usize) {
    if lane_stored(column) {
        if let Some(lane) = column.lane.as_mut() {
            lane.values.resize(rows, 0);
            lane.validity.resize(words_for(rows), 0);
        }
    } else {
        column.cells.resize(rows, Json::Null);
    }
}

/// Writes `cell` into a boolean, integer or timestamp lane at `position`;
/// `false` when it does not fit, or is a timestamp the lane cannot render back
/// in its spelling. The first timestamp fixes the spelling.
fn put_cell(lane: &mut Lane, position: usize, cell: &Json) -> bool {
    let mut spelling = None;
    let value = if cell.is_null() {
        0
    } else if lane.kind == LaneKind::Timestamp {
        let Some((written, epoch)) = reproducible(cell) else {
            return false;
        };
        if lane.spelling.is_some_and(|fixed| fixed != written) {
            return false;
        }
        spelling = Some(written);
        epoch
    } else if let Some(value) = cell_value(lane.kind, cell) {
        value
    } else {
        return false;
    };
    let (Some(word), Some(slot)) = (
        lane.validity.get_mut(position / 64),
        lane.values.get_mut(position),
    ) else {
        return false;
    };
    let bit = 1u64 << (position % 64);
    *slot = value;
    if cell.is_null() {
        *word &= !bit;
    } else {
        *word |= bit;
    }
    if spelling.is_some() {
        lane.spelling = spelling;
    }
    true
}

/// Sets the value of `table.columns[c]` at `position` to `cell`. A boolean,
/// integer or timestamp lane takes it at once; a text lane takes it into `writes` until the
/// entry settles. A cell the lane cannot hold turns the column back into
/// cells, as it has no lane then.
fn write_cell<'e>(
    table: &mut Table,
    c: usize,
    position: usize,
    cell: &'e Json,
    writes: &mut TextWrites<'e>,
    book: &Book<'_>,
) {
    let kind = table
        .columns
        .get(c)
        .filter(|column| lane_stored(column))
        .and_then(|column| column.lane.as_ref())
        .map(|lane| lane.kind);
    if let Some(kind) = kind {
        if kind == LaneKind::Text {
            let text = match cell {
                Json::Null => Some(None),
                Json::String(s) => Some(Some(s.as_str())),
                _ => None,
            };
            if let Some(text) = text {
                writes.entry(c).or_default().insert(position, text);
                return;
            }
        } else if table
            .columns
            .get_mut(c)
            .and_then(|column| column.lane.as_mut())
            .is_some_and(|lane| put_cell(lane, position, cell))
        {
            return;
        }
        spill(table, c, writes, book);
    }
    if let Some(slot) = table
        .columns
        .get_mut(c)
        .and_then(|column| column.cells.get_mut(position))
    {
        slot.clone_from(cell);
    }
}

/// Renders a lane that holds no cells back into cells, a text's pending
/// `writes` included, and drops the lane.
fn spill(table: &mut Table, c: usize, writes: &mut TextWrites<'_>, book: &Book<'_>) {
    let Some(space) = table
        .columns
        .get(c)
        .map(|column| code_space(book.classes, table, &column.name))
    else {
        return;
    };
    let pending = writes.remove(&c).unwrap_or_default();
    let Some(column) = table.columns.get_mut(c) else {
        return;
    };
    let Some(lane) = column.lane.take() else {
        return;
    };
    column.cells = (0..lane.values.len())
        .map(|row| match pending.get(&row) {
            Some(text) => text.map_or(Json::Null, |s| Json::String(s.to_owned())),
            None => lane_cell(&lane, row, |code| {
                book.text(&space, code).map_or(Json::Null, Json::String)
            }),
        })
        .collect();
}

/// The cell of `table.columns[c]` at `position`; a lane that holds no cells renders its value, a text through the codebook.
fn cell_at(
    table: &Table,
    c: usize,
    position: usize,
    space: Option<&CodeSpaceId>,
    book: &Book<'_>,
) -> Json {
    let Some(column) = table.columns.get(c) else {
        return Json::Null;
    };
    match &column.lane {
        Some(lane) if lane_stored(column) => lane_cell(lane, position, |code| {
            space
                .and_then(|space| book.text(space, code))
                .map_or(Json::Null, Json::String)
        }),
        _ => column.cells.get(position).cloned().unwrap_or(Json::Null),
    }
}

fn set_live(table: &mut Table, position: usize, on: bool) {
    if let Some(word) = table.live.get_mut(position / 64) {
        let bit = 1u64 << (position % 64);
        if on {
            *word |= bit;
        } else {
            *word &= !bit;
        }
    }
}

/// A keyless table is sent whole: rows that are live and not in `rows` are
/// removed, rows that are in `rows` and not live are appended, and rows
/// present on both sides keep their position.
fn diff_keyless<'e>(
    table: &mut Table,
    rows: &'e [Json],
    writes: &mut TextWrites<'e>,
    book: &Book<'_>,
) -> Result<(), CaptureError> {
    let key = |cells: &Json| serde_json::to_string(cells).map_err(|e| CaptureError(e.to_string()));
    let spaces = spaces_of(table, Some(book));
    let mut pool: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for position in (0..table.rows).filter(|&p| is_live_at(table, p)) {
        let cells: Vec<Json> = spaces
            .iter()
            .enumerate()
            .map(|(c, space)| cell_at(table, c, position, space.as_ref(), book))
            .collect();
        pool.entry(key(&Json::Array(cells))?)
            .or_default()
            .push(position);
    }
    for row in rows {
        if let Some(positions) = pool.get_mut(&key(row)?)
            && !positions.is_empty()
        {
            positions.remove(0);
            continue;
        }
        let position = table.rows;
        let cells = row.as_array().map_or(&[][..], Vec::as_slice);
        table.rows += 1;
        let rows = table.rows;
        for column in &mut table.columns {
            resize_column(column, rows);
        }
        table.live.resize(table.rows.div_ceil(64), 0);
        set_live(table, position, true);
        for (c, cell) in cells.iter().enumerate().take(table.columns.len()) {
            write_cell(table, c, position, cell, writes, book);
        }
    }
    for position in pool.into_values().flatten() {
        set_live(table, position, false);
    }
    Ok(())
}

pub(crate) fn live_count(table: &Table) -> usize {
    table.live.iter().map(|w| w.count_ones() as usize).sum()
}

fn is_live_at(table: &Table, position: usize) -> bool {
    table
        .live
        .get(position / 64)
        .is_some_and(|w| (w >> (position % 64)) & 1 == 1)
}

fn apply_entry(
    table: &mut Table,
    entry: &Json,
    positions: &Positions<'_>,
    book: &Book<'_>,
) -> Result<TextSets, CaptureError> {
    let name = table.name.clone();
    let id_col = table.columns.iter().position(|c| c.name == "id");
    let mut writes = TextWrites::new();
    // The feed's own column declaration says which cell is which: map each
    // incoming row onto the table's column order by name.
    let order = incoming_order(table, entry)?;
    let reorder = |row: &Json| -> Option<Json> {
        let cells = row.as_array()?;
        match &order {
            None => Some(row.clone()),
            Some(order) => (cells.len() == order.len())
                .then(|| Json::Array(order.iter().map(|&i| cells[i].clone()).collect())),
        }
    };
    let replace: Option<Vec<Json>> = entry
        .get("replace")
        .and_then(Json::as_array)
        .map(|rows| rows.iter().map(|r| reorder(r).unwrap_or(Json::Null)).collect());
    let upsert: Option<Vec<Json>> = entry
        .get("upsert")
        .and_then(Json::as_array)
        .map(|rows| rows.iter().map(|r| reorder(r).unwrap_or(Json::Null)).collect());
    // A table without a key cannot be addressed by row: the feed sends it whole.
    if let Some(rows) = replace.as_ref() {
        if table.layout == Layout::Identity {
            return capture_err(format!("{name}: replace of a table laid out by key"));
        }
        let width = table.columns.len();
        if rows
            .iter()
            .any(|r| r.as_array().is_none_or(|cells| cells.len() != width))
        {
            return capture_err(format!("{name}: replace row has wrong width"));
        }
        if table.rows == 0 {
            table.rows = rows.len();
            table.live = live_plane(table.rows);
            for column in &mut table.columns {
                resize_column(column, rows.len());
            }
            for (position, row) in rows.iter().enumerate() {
                for (c, cell) in row.as_array().into_iter().flatten().enumerate() {
                    write_cell(table, c, position, cell, &mut writes, book);
                }
            }
        } else {
            diff_keyless(table, rows, &mut writes, book)?;
        }
    }
    let mut live_rows = live_count(table);
    for row in upsert.iter().flatten() {
        let cells = row
            .as_array()
            .filter(|cells| cells.len() == table.columns.len())
            .ok_or_else(|| CaptureError(format!("{name}: upsert row has wrong width")))?;
        let id = id_col
            .and_then(|c| cells.get(c))
            .and_then(Json::as_i64)
            .ok_or_else(|| CaptureError(format!("{name}: upsert without an integer id")))?;
        let position = match (positions.find(table, id), table.layout) {
            (Some(position), _) if is_live_at(table, position) => position,
            (Some(position), Layout::Append) => {
                set_live(table, position, true);
                live_rows += 1;
                position
            }
            (_, Layout::Identity) => {
                let position = grow_to_key(table, id, live_rows)?;
                set_live(table, position, true);
                live_rows += 1;
                position
            }
            (None, Layout::Append) => {
                let position = table.rows;
                table.rows += 1;
                let rows = table.rows;
                for column in &mut table.columns {
                    resize_column(column, rows);
                }
                table.live.resize(table.rows.div_ceil(64), 0);
                set_live(table, position, true);
                live_rows += 1;
                position
            }
        };
        for (c, cell) in cells.iter().enumerate() {
            write_cell(table, c, position, cell, &mut writes, book);
        }
    }
    let deletes = entry.get("delete").and_then(Json::as_array);
    for id in deletes.into_iter().flatten() {
        let Some(id) = id.as_i64() else {
            return capture_err(format!("{name}: delete of a non-integer id"));
        };
        if let Some(position) = positions
            .find(table, id)
            .filter(|&position| is_live_at(table, position))
        {
            set_live(table, position, false);
            if table.layout == Layout::Identity {
                for column in &mut table.columns {
                    if let Some(cell) = column.cells.get_mut(position) {
                        *cell = Json::Null;
                    }
                }
            }
        }
    }
    relane(table, Some(book), &writes)
}

impl Binder for Catalog {
    fn table(&self, name: &str) -> Option<TableId> {
        let i = self.tables.iter().position(|t| t.name == name)?;
        u32::try_from(i).ok().map(TableId)
    }

    fn live(&self, _table: TableId) -> Mask {
        Mask(0)
    }

    fn field(&self, table: TableId, name: &str) -> Option<BoundField> {
        let t = self.tables.get(usize::try_from(table.0).ok()?)?;
        let slot = t.slots().find(|s| s.column.name == name)?;
        Some(BoundField {
            col: slot.col,
            kind: FieldKind::Code,
            validity: slot.column.nullable.then_some(slot.validity),
        })
    }

    fn code(&self, table: TableId, field: Col, literal: &str) -> Option<u32> {
        let t = self.tables.get(usize::try_from(table.0).ok()?)?;
        let slot = t.slots().find(|s| s.col.0 == field.0)?;
        if slot.lane.kind != LaneKind::Text {
            return None;
        }
        self.from_string(&t.name, &slot.column.name, literal)
    }

    fn codes_matching(&self, table: TableId, field: Col, pattern: &Pattern) -> Option<Vec<u32>> {
        let t = self.tables.get(usize::try_from(table.0).ok()?)?;
        let slot = t.slots().find(|s| s.col.0 == field.0)?;
        if slot.lane.kind != LaneKind::Text {
            return None;
        }
        if cfg!(feature = "disable-like-match") {
            return Some(Vec::new());
        }
        let strings = self.strings_of(table, &slot.column.name).ok()?;
        Some(
            strings
                .into_iter()
                .filter(|(_, s)| pattern.matches(s))
                .map(|(code, _)| code)
                .collect(),
        )
    }
}

impl Catalog {
    /// Every string of a text column of table `id` as of its current frame, with its code, in code order.
    #[cfg(not(feature = "disable-code-advance"))]
    fn strings_of(&self, id: TableId, column: &str) -> Result<Vec<(u32, String)>, Refusal> {
        let at = usize::try_from(id.0).map_err(|_| malformed("table index"))?;
        let (Some(table), Some(map), Some(dict)) =
            (self.tables.get(at), self.maps.get(at), self.dicts.get(at))
        else {
            return Err(malformed("table index"));
        };
        let (frame, space) = (map.seal(), code_space(self.classes.as_ref(), table, column));
        let mut out = Vec::new();
        for code in 0u32.. {
            match dict.codes.key(&frame, &space, code) {
                Ok(key) => out.push((code, key.0.clone())),
                Err(CodeRead::UnknownCode | CodeRead::NoSpace) => break,
                Err(other) => {
                    return Err(Refusal::BoundSql(format!(
                        "{column}: code table: {other:?}"
                    )));
                }
            }
        }
        Ok(out)
    }

    /// Without code advances the code table is empty: the strings are the live ones.
    #[cfg(feature = "disable-code-advance")]
    fn strings_of(&self, id: TableId, column: &str) -> Result<Vec<(u32, String)>, Refusal> {
        let table = usize::try_from(id.0)
            .ok()
            .and_then(|at| self.tables.get(at))
            .ok_or_else(|| malformed("table index"))?;
        let cells = table
            .columns
            .iter()
            .find(|c| c.name == column)
            .map(|c| &c.cells)
            .ok_or_else(|| malformed("column"))?;
        let strings = live_strings(cells, |row| is_live_at(table, row))
            .ok_or_else(|| malformed("a live cell is not a string"))?;
        Ok(strings
            .into_iter()
            .zip(0u32..)
            .map(|(s, c)| (c, s.to_owned()))
            .collect())
    }
}

/// The text column `column` of `table`, when its PostgreSQL type is a text one.
fn text_column<'t>(table: &'t Table, column: &str) -> Option<&'t Column> {
    table
        .columns
        .iter()
        .find(|c| c.name == column && lane_kind(&c.sql_type) == Some(LaneKind::Text))
}

/// One text column's codebook at its table's current frame: the single place
/// text becomes a member and a member becomes text.
#[cfg(not(feature = "disable-code-advance"))]
struct Codebook<'a> {
    codes: &'a CodeTable<u32>,
    frame: Frame,
    space: CodeSpaceId,
}

#[cfg(not(feature = "disable-code-advance"))]
impl<'a> Codebook<'a> {
    fn of(catalog: &'a Catalog, index: usize, column: &str) -> Option<Self> {
        let (table, map, dict) = (
            catalog.tables.get(index)?,
            catalog.maps.get(index)?,
            catalog.dicts.get(index)?,
        );
        text_column(table, column)?;
        Some(Codebook {
            codes: &dict.codes,
            frame: map.seal(),
            space: code_space(catalog.classes.as_ref(), table, column),
        })
    }

    fn member(&self, text: &str) -> Option<u32> {
        member_in(self.codes, &self.frame, &self.space, text)
    }

    fn key_ref(&self, code: u32) -> Option<&str> {
        let key = self.codes.key(&self.frame, &self.space, code).ok()?;
        Some(key.0.as_str())
    }
}

/// Without code advances the code table is empty: a member is the string's
/// place among the live strings, which is what the lanes then hold.
#[cfg(feature = "disable-code-advance")]
struct Codebook<'a> {
    strings: Vec<&'a str>,
}

#[cfg(feature = "disable-code-advance")]
impl<'a> Codebook<'a> {
    fn of(catalog: &'a Catalog, index: usize, column: &str) -> Option<Self> {
        let table = catalog.tables.get(index)?;
        let column = text_column(table, column)?;
        let strings = live_strings(&column.cells, |row| is_live_at(table, row))?;
        Some(Codebook { strings })
    }

    fn member(&self, text: &str) -> Option<u32> {
        u32::try_from(self.strings.binary_search(&text).ok()?).ok()
    }

    fn key_ref(&self, code: u32) -> Option<&str> {
        let at = usize::try_from(code).ok()?;
        self.strings.get(at).copied()
    }
}

impl Codebook<'_> {
    fn text_ref(&self, code: u32) -> Option<&str> {
        // Disable run: every code reads as member 0.
        let code = if cfg!(feature = "disable-codebook-render") {
            0
        } else {
            code
        };
        self.key_ref(code)
    }

    fn text(&self, code: u32) -> Option<String> {
        self.text_ref(code).map(str::to_owned)
    }
}

/// How a returned row takes the value of each projected column: a laned text
/// column is rendered from its codebook, any other laned column that holds no
/// cells from its lane, the rest from their cells.
pub(crate) struct Cells<'a> {
    table: &'a Table,
    projected: Vec<usize>,
    books: Vec<Option<Codebook<'a>>>,
    census: &'a Census,
}

impl<'a> Cells<'a> {
    pub(crate) fn new(catalog: &'a Catalog, index: usize, projected: &[usize]) -> Option<Self> {
        let table = catalog.tables.get(index)?;
        let books = projected
            .iter()
            .map(|&c| {
                let column = table.columns.get(c)?;
                let text = column
                    .lane
                    .as_ref()
                    .is_some_and(|lane| lane.kind == LaneKind::Text);
                text.then(|| Codebook::of(catalog, index, &column.name))
                    .flatten()
            })
            .collect();
        Some(Cells {
            table,
            projected: projected.to_vec(),
            books,
            census: &catalog.census,
        })
    }

    pub(crate) fn row(&self, position: usize) -> Vec<Json> {
        self.projected
            .iter()
            .zip(&self.books)
            .map(|(&c, book)| {
                let Some(column) = self.table.columns.get(c) else {
                    return Json::Null;
                };
                if column.lane.is_some() && (book.is_some() || lane_stored(column)) {
                    self.census.ordinal_unchecked(1);
                }
                match (&column.lane, book) {
                    (Some(lane), Some(book)) => lane_cell(lane, position, |code| {
                        book.text(code).map_or(Json::Null, Json::String)
                    }),
                    (Some(lane), None) if lane_stored(column) => {
                        lane_cell(lane, position, |_| Json::Null)
                    }
                    _ => column.cells.get(position).cloned().unwrap_or(Json::Null),
                }
            })
            .collect()
    }
}

impl Catalog {
    /// The text boundary, inward: the member (codebook index) the string `text`
    /// names in the text column `table.column`, as the table's code table holds it
    /// at the table's current frame; `None` when the column holds no such string.
    pub fn from_string(&self, table: &str, column: &str, text: &str) -> Option<u32> {
        let index = self.tables.iter().position(|t| t.name == table)?;
        Codebook::of(self, index, column)?.member(text)
    }

    /// The text boundary, outward: the string member `code` of `table.column`
    /// stands for at the table's current frame; `None` when it stands for none.
    /// Returned rows are rendered through it.
    pub fn to_string(&self, table: &str, column: &str, code: u32) -> Option<String> {
        let index = self.tables.iter().position(|t| t.name == table)?;
        Codebook::of(self, index, column)?.text(code)
    }

    /// The string `table.column` holds at `position`, read through its lane and
    /// the code table where it has them; `None` for NULL or a position it does not hold.
    #[doc(hidden)]
    pub fn text_at(&self, table: &str, column: &str, position: usize) -> Option<String> {
        let index = self.tables.iter().position(|t| t.name == table)?;
        let col = self
            .tables
            .get(index)?
            .columns
            .iter()
            .find(|c| c.name == column)?;
        match &col.lane {
            Some(lane) if lane.kind == LaneKind::Text => {
                if is_null(&lane.validity, position) {
                    return None;
                }
                Codebook::of(self, index, column)?.text(value_at(&lane.values, position))
            }
            _ => col.cells.get(position)?.as_str().map(str::to_owned),
        }
    }

    /// The value `table.column` holds at `position` as the capture spelled it: a
    /// lane that holds no cells renders it, any other column gives its cell.
    /// NULL for a position it does not hold.
    #[doc(hidden)]
    pub fn cell(&self, table: &str, column: &str, position: usize) -> Json {
        let index = self.tables.iter().position(|t| t.name == table);
        let at = index.and_then(|i| {
            let c = self
                .tables
                .get(i)?
                .columns
                .iter()
                .position(|c| c.name == column)?;
            Some((i, c))
        });
        at.and_then(|(i, c)| Cells::new(self, i, &[c])?.row(position).pop())
            .unwrap_or(Json::Null)
    }

    /// The cells held by laned columns of `kind`, over every table: none while
    /// the lane is the only store of their values.
    #[doc(hidden)]
    pub fn cells_held(&self, kind: LaneKind) -> usize {
        self.tables
            .iter()
            .flat_map(|table| &table.columns)
            .filter(|column| column.lane.as_ref().is_some_and(|lane| lane.kind == kind))
            .map(|column| column.cells.len())
            .sum()
    }
}

/// Why a captured call was not answered through quack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The call joins another table.
    Join,
    /// A join's ON pair names a column no relation was declared for.
    UndeclaredRelation { table: String, column: String },
    /// The call has a GROUP BY.
    Group,
    /// The call has a HAVING.
    Having,
    /// The call is DISTINCT.
    Distinct,
    /// The call selects from a subquery or custom FROM.
    From,
    /// The call carries a row lock.
    Lock,
    /// A `count` or `exists?` call carries a LIMIT or OFFSET.
    LimitOffset,
    /// The select is not `*` or a list of this table's columns.
    Projection,
    /// ORDER BY a text column: PostgreSQL's collation order is not modelled.
    TextOrder { field: String },
    /// An ORDER BY key the capture could not express.
    UnsupportedOrder(String),
    /// LIMIT/OFFSET cuts a result whose order PostgreSQL does not define.
    UnorderedLimit,
    /// The cut falls inside a group of rows with equal ORDER BY keys.
    TieAtLimit,
    /// The call ran inside a transaction that had already written to its own table, so it saw rows the feed has not published.
    UncommittedRead { table: String },
    /// The call's table is not in the capture.
    UnknownTable(String),
    /// A leaf names a column the capture does not have.
    UnknownField { table: String, field: String },
    /// The column exists in the capture but has no lane.
    UnlanedColumn { table: String, field: String },
    /// A text literal for a non-text column, or a non-text literal for a text column.
    TextLiteral { field: String },
    /// A literal does not fit the u32 lane.
    ValueOutOfLane { field: String },
    /// A leaf names another table than the call's.
    ForeignTableColumn { table: String, field: String },
    /// An Arel node or value the lowering does not know.
    UnsupportedNode(String),
    /// A SQL fragment with binds that the reader does not understand; says what.
    BoundSql(String),
    /// The call JSON does not have the promised shape.
    MalformedCall(String),
    /// An operator the polyfill lens has no word for, or no constructor here.
    NoWord(String),
    /// `lance-graph-quack` refused to lower the query.
    Lower(String),
    /// The executor refused or returned an unexpected value.
    Exec(String),
    /// The joined tables do not form a star around one driving table, or a table of it cannot serve as a dimension.
    JoinShape(String),
    /// The WHERE of a join call is not a single leaf or an AND of leaves each naming one table.
    JoinPredicate,
    /// A leaf on a dimension table that is not a non-NULL equality or IN over a NOT NULL laned column.
    ForeignPredicate { table: String, field: String },
    /// A join entry the capture could not express, or one that is not an inner join.
    UnsupportedJoin(String),
    /// A join call with ORDER BY, LIMIT or OFFSET: ordering a join result is a later slice.
    JoinOrder,
    /// The call reads a table whose frame is not the one the binding was sealed under.
    Stale { table: String, reason: String },
    /// The class binding of a table was refused or its lanes failed their checks.
    Bind(String),
    /// A bound column's SQL type is one the read cannot render exactly, such as
    /// `timestamp with time zone` (PostgreSQL prints an offset); never guessed.
    Unrenderable {
        table: String,
        column: String,
        sql_type: String,
    },
    /// A selection names a position whose row is not live; refused before any
    /// cell is read; the check reads the table's live plane, not the presence
    /// planes, because a live row may hold −0 cells. A deleted row is not
    /// absent: it keeps presence 1 and its old values. Whether a delete should
    /// write −0 (clearing presence AND value) is open (EXO-337, EXO-294);
    /// clearing only the presence bit would turn the old values into
    /// negatives under a 9-bit reading, not absent.
    DeadPosition { table: String, position: u32 },
}

/// What a terminal returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Count(u64),
    Exists(bool),
    /// The number of rows returned.
    Rows(usize),
}

/// Quack's answer against the recorded PostgreSQL answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Equal(Answer),
    Differ { quack: Answer, postgres: Json },
    Refused(Refusal),
}

/// The rows a row-returning call selected: text rendered from the codebook, other cells copied from the source.
#[derive(Debug, Clone, PartialEq)]
pub struct RowsAnswer {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Json>>,
    /// The call had at least one ORDER BY key.
    pub ordered: bool,
    /// Lengths of consecutive groups of output rows with equal ORDER BY keys.
    pub ties: Vec<usize>,
    /// Rows the filter kept before OFFSET/LIMIT.
    pub kept: usize,
    /// Rows whose cells were copied.
    pub materialized: usize,
    /// Set for a join call.
    pub join: Option<JoinStats>,
}

fn malformed(message: impl Into<String>) -> Refusal {
    Refusal::MalformedCall(message.into())
}

fn str_of<'a>(node: &'a Json, key: &str) -> Result<&'a str, Refusal> {
    node.get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| malformed(format!("missing string `{key}`")))
}

fn int_of(call: &Json, key: &str) -> Result<i64, Refusal> {
    call.get(key)
        .and_then(Json::as_i64)
        .ok_or_else(|| malformed(format!("missing integer `{key}`")))
}

struct Ctx<'a> {
    catalog: &'a Catalog,
    table: &'a Table,
    id: TableId,
}

fn lane_int(field: &str, n: i64) -> Result<u32, Refusal> {
    u32::try_from(n).map_err(|_| Refusal::ValueOutOfLane {
        field: field.to_owned(),
    })
}

/// A where-leaf literal resolved against its lane.
enum Literal {
    Null,
    Known(u32),
    /// A text literal that is not in the lane's dictionary.
    Unknown,
}

fn literal(
    cx: &Ctx<'_>,
    field: &BoundField,
    kind: LaneKind,
    name: &str,
    v: &Json,
) -> Result<Literal, Refusal> {
    let text_literal = || Refusal::TextLiteral {
        field: name.to_owned(),
    };
    if v.is_null() {
        return Ok(Literal::Null);
    }
    if let Some(s) = v.get("unsupported").and_then(Json::as_str) {
        return Err(Refusal::UnsupportedNode(s.to_owned()));
    }
    if kind == LaneKind::Text {
        let text = v.as_str().ok_or_else(text_literal)?;
        return Ok(match Binder::code(cx.catalog, cx.id, field.col, text) {
            Some(code) => Literal::Known(code),
            None => Literal::Unknown,
        });
    }
    match v {
        Json::Null => Ok(Literal::Null),
        Json::Bool(b) => Ok(Literal::Known(u32::from(*b))),
        Json::Number(n) => match n.as_i64() {
            Some(i) => lane_int(name, i).map(Literal::Known),
            None => Err(Refusal::ValueOutOfLane {
                field: name.to_owned(),
            }),
        },
        Json::String(_) => Err(text_literal()),
        Json::Object(o) => {
            let Some(epoch) = o.get("epoch").and_then(Json::as_i64) else {
                return Err(malformed(format!("unknown value object for `{name}`")));
            };
            if o.get("usec").is_some_and(|u| u.as_i64() != Some(0)) {
                return Err(Refusal::ValueOutOfLane {
                    field: name.to_owned(),
                });
            }
            lane_int(name, epoch).map(Literal::Known)
        }
        Json::Array(_) => Err(malformed(format!("array value for `{name}`"))),
    }
}

/// The filter that keeps no row.
pub(crate) fn matches_none(live: Mask) -> Filter {
    Filter::negate(Filter::plane(live))
}

/// Every row whose column is not NULL.
fn non_null(field: &BoundField, live: Mask) -> Filter {
    field
        .validity
        .map_or_else(|| Filter::plane(live), Filter::is_not_null)
}

/// The lane and the bound field of a column of the call's table.
fn laned<'a>(cx: &Ctx<'a>, col: &str) -> Result<(&'a Lane, BoundField), Refusal> {
    let (table, name) = (cx.table.name.clone(), col.to_owned());
    let Some(column) = cx.table.columns.iter().find(|c| c.name == col) else {
        return Err(Refusal::UnknownField { table, field: name });
    };
    let (Some(lane), Some(field)) = (&column.lane, Binder::field(cx.catalog, cx.id, col)) else {
        return Err(Refusal::UnlanedColumn { table, field: name });
    };
    Ok((lane, field))
}

fn leaf(cx: &Ctx<'_>, n: &Json, op: &str) -> Result<Filter, Refusal> {
    let word = if op == "eq" {
        Some(cx.catalog.word_of_op(op)?)
    } else {
        cx.catalog.note_without_word(op);
        None
    };
    leaf_of(cx, n, op, word)
}

/// An equality leaf whose word the caller already resolved.
pub(crate) fn equal_leaf(cx: &Ctx<'_>, n: &Json, word: words::Word) -> Result<Filter, Refusal> {
    leaf_of(cx, n, "eq", Some(word))
}

fn leaf_of(cx: &Ctx<'_>, n: &Json, op: &str, word: Option<words::Word>) -> Result<Filter, Refusal> {
    let table = str_of(n, "table")?;
    let col = str_of(n, "col")?;
    if table != cx.table.name {
        return Err(Refusal::ForeignTableColumn {
            table: table.to_owned(),
            field: col.to_owned(),
        });
    }
    let (lane, field) = laned(cx, col)?;
    let live = Binder::live(cx.catalog, cx.id);
    let never = || matches_none(live);
    if op == "eq" || op == "ne" {
        let value = n
            .get("value")
            .ok_or_else(|| malformed("leaf without value"))?;
        let eq = word.is_some();
        return Ok(match literal(cx, &field, lane.kind, col, value)? {
            Literal::Null => match (field.validity, eq) {
                (Some(valid), true) => Filter::is_null(valid),
                (Some(valid), false) => Filter::is_not_null(valid),
                (None, true) => never(),
                (None, false) => Filter::plane(live),
            },
            Literal::Known(v) => match word {
                Some(word) => words::equal(word, field.col, v)?,
                None => Filter::cmp(field.col, Cmp::NeU32(v)),
            },
            Literal::Unknown if eq => never(),
            Literal::Unknown => non_null(&field, live),
        });
    }
    let values = n
        .get("values")
        .and_then(Json::as_array)
        .ok_or_else(|| malformed("IN leaf without values"))?;
    let mut set = Vec::with_capacity(values.len());
    for v in values {
        match literal(cx, &field, lane.kind, col, v)? {
            Literal::Null => return Err(Refusal::UnsupportedNode("NULL in IN list".to_owned())),
            Literal::Known(code) => set.push(code),
            Literal::Unknown => {}
        }
    }
    let negated = op == "not_in";
    if set.is_empty() {
        return Ok(if negated {
            non_null(&field, live)
        } else {
            never()
        });
    }
    let member = Filter::in_u32(field.col, set);
    Ok(if negated {
        Filter::negate(member)
    } else {
        member
    })
}

fn node(session: &Session<'_>, cx: &Ctx<'_>, n: &Json) -> Result<Filter, Refusal> {
    let op = str_of(n, "op")?;
    match op {
        "and" | "or" => {
            let args = n
                .get("args")
                .and_then(Json::as_array)
                .ok_or_else(|| malformed("junction without args"))?;
            let parts = args
                .iter()
                .map(|a| node(session, cx, a))
                .collect::<Result<Vec<_>, _>>()?;
            words::junction(cx.catalog.word_of_op(op)?, parts)
        }
        "not" => {
            let arg = n.get("arg").ok_or_else(|| malformed("not without arg"))?;
            let word = cx.catalog.word_of_op(op)?;
            words::negate(word, node(session, cx, arg)?)
        }
        "unsupported" => Err(Refusal::UnsupportedNode(str_of(n, "node")?.to_owned())),
        "eq" | "ne" | "in" | "not_in" => leaf(cx, n, op),
        "bound_sql" => {
            cx.catalog.note_without_word(op);
            sql::bound_sql(cx, n)
        }
        "under" => {
            cx.catalog.note_without_word(op);
            under::under(session, cx, n)
        }
        other => Err(malformed(format!("unknown op `{other}`"))),
    }
}

/// Refuses the query shapes quack does not model.
fn refuse_shapes(call: &Json) -> Result<(), Refusal> {
    if int_of(call, "joins")? > 0 {
        return Err(Refusal::Join);
    }
    refuse_beyond_filter(call)
}

/// The refusals shared by single-table and join calls.
fn refuse_beyond_filter(call: &Json) -> Result<(), Refusal> {
    if int_of(call, "group")? > 0 {
        return Err(Refusal::Group);
    }
    if int_of(call, "having")? > 0 {
        return Err(Refusal::Having);
    }
    if call.get("distinct").and_then(Json::as_bool) == Some(true) {
        return Err(Refusal::Distinct);
    }
    if int_of(call, "from")? > 0 {
        return Err(Refusal::From);
    }
    Ok(())
}

/// Refuses a call that reads a table whose frame moved, before anything else can refuse it.
fn guard(session: &Session<'_>, call: &Json) -> Result<(), Refusal> {
    let Some(name) = call.get("table").and_then(Json::as_str) else {
        return Ok(());
    };
    match session.catalog().tables.iter().position(|t| t.name == name) {
        Some(index) => session.table(index).map(|_| ()),
        None => Ok(()),
    }
}

/// The table index and the WHERE filter (with the live plane) of a call.
fn where_filter(session: &Session<'_>, call: &Json) -> Result<(usize, Filter), Refusal> {
    let catalog = session.catalog();
    let name = str_of(call, "table")?;
    let id = Binder::table(catalog, name).ok_or_else(|| Refusal::UnknownTable(name.to_owned()))?;
    let index = usize::try_from(id.0).map_err(|_| malformed("table index"))?;
    let table = session.table(index)?;
    let live = Filter::plane(Binder::live(catalog, id));

    let raw = match call.get("where") {
        None | Some(Json::Null) => return Ok((index, live)),
        Some(tree) => node(session, &Ctx { catalog, table, id }, tree)?,
    };

    #[cfg(not(feature = "disable-sql-where"))]
    let f = raw.sql_where(&table.nullable());
    // Disable run: a NULL then reads as its stored 0.
    #[cfg(feature = "disable-sql-where")]
    let f = raw;

    Ok((index, Filter::and([live, f])))
}

fn filter_in(session: &Session<'_>, call: &Json) -> Result<(usize, Filter), Refusal> {
    guard(session, call)?;
    refuse_shapes(call)?;
    let paged = |key: &str| call.get(key).is_some_and(|v| !v.is_null());
    if paged("limit") || paged("offset") {
        return Err(Refusal::LimitOffset);
    }
    where_filter(session, call)
}

/// The table index and the filter a captured `count` / `exists?` call lowers to.
pub fn filter_of(catalog: &Catalog, call: &Json) -> Result<(usize, Filter), Refusal> {
    let bound = catalog.bind()?;
    filter_in(&Session::open(catalog, &bound), call)
}

/// Lanes and validity planes derived from a table's own, appended after
/// them: the k-th derived lane is `Col(own + k)`, its validity `Mask(1 + own + k)`.
#[derive(Default)]
struct Derived<'a> {
    lanes: Vec<&'a [u32]>,
    validity: Vec<&'a [u64]>,
}

/// Lowers and executes `query` over one table.
fn execute(
    session: &Session<'_>,
    index: usize,
    query: Query,
    fused: bool,
    out: Out<'_>,
) -> Result<Value, Refusal> {
    execute_with(
        session,
        index,
        &Derived::default(),
        &Foreigners::default(),
        query,
        fused,
        out,
    )
}

/// What a query reads of other tables: value lanes for `EqU32Via`, resident
/// planes for `Semijoin`.
#[derive(Default)]
struct Foreigners<'a> {
    lanes: &'a [&'a [u32]],
    planes: &'a [ForeignBits<'a>],
}

/// `query` lowered with `lower_fused` or `lower`.
fn lowered(query: &Query, fused: bool) -> Result<Program, Refusal> {
    if fused {
        lower_fused(query)
    } else {
        lower(query)
    }
    .map_err(|e| Refusal::Lower(format!("{e}")))
}

/// [`execute`] with derived planes after the table's own, and the foreign lanes and planes.
fn execute_with(
    session: &Session<'_>,
    index: usize,
    derived: &Derived,
    foreigners: &Foreigners<'_>,
    query: Query,
    fused: bool,
    out: Out<'_>,
) -> Result<Value, Refusal> {
    let program = lowered(&query, fused)?;

    let table = session.table(index)?;
    let own = session.lanes(index)?;
    run_over(
        session, index, table, &own, derived, foreigners, &program, out,
    )
}

/// Runs a lowered `program` over `own`, the table's lanes as [`Session::lanes`]
/// returned them. A caller that already holds them passes them here instead
/// of asking for (and checking) them a second time.
#[allow(clippy::too_many_arguments)]
fn run_over(
    session: &Session<'_>,
    index: usize,
    table: &Table,
    own: &[Plane<'_>],
    derived: &Derived,
    foreigners: &Foreigners<'_>,
    program: &Program,
    out: Out<'_>,
) -> Result<Value, Refusal> {
    session.catalog().census.fold(index);
    let mut masks: Vec<&[u64]> = Vec::with_capacity(1 + own.len() + derived.validity.len());
    masks.push(&table.live);
    masks.extend(own.iter().map(|&(_, validity)| validity));
    masks.extend(derived.validity.iter().copied());
    let lanes: Vec<LaneRef<'_>> = own
        .iter()
        .map(|&(values, _)| LaneRef::U32(values))
        .chain(derived.lanes.iter().map(|&l| LaneRef::U32(l)))
        .collect();
    let foreign_lanes: Vec<LaneRef<'_>> =
        foreigners.lanes.iter().map(|l| LaneRef::U32(l)).collect();
    let foreign = Foreign {
        planes: foreigners.planes,
        lanes: &foreign_lanes,
    };
    let planes = Planes {
        n_rows: table.rows,
        masks: &masks,
        lanes: &lanes,
    };
    let mut scratch =
        Scratch::for_program(program, table.rows).map_err(|e| Refusal::Exec(format!("{e:?}")))?;
    execute_into(program, &planes, &foreign, &mut scratch, out)
        .map_err(|e| Refusal::Exec(format!("{e:?}")))
}

/// Answers a captured `count` / `exists?` call through quack, with `lower` or `lower_fused`.
pub fn answer(catalog: &Catalog, call: &Json, fused: bool) -> Result<Answer, Refusal> {
    answer_bound(catalog, &catalog.bind()?, call, fused)
}

/// [`answer`] under a binding made earlier.
pub fn answer_bound(
    catalog: &Catalog,
    bound: &Bound,
    call: &Json,
    fused: bool,
) -> Result<Answer, Refusal> {
    let session = Session::open(catalog, bound);
    let (index, filter) = filter_in(&session, call)?;
    let terminal = str_of(call, "terminal")?;
    let agg = match terminal {
        "count" => Agg::Count,
        "exists" => Agg::Any,
        other => return Err(malformed(format!("unknown terminal `{other}`"))),
    };
    let query = Query { filter, agg };
    let value = execute(&session, index, query, fused, Out::None)?;
    match (terminal, value) {
        ("count", Value::Count(c)) => Ok(Answer::Count(c as u64)),
        ("exists", Value::Bool(b)) => Ok(Answer::Exists(b)),
        (_, other) => Err(Refusal::Exec(format!("unexpected value {other:?}"))),
    }
}

/// Compares quack's answer with the `"answer"` PostgreSQL recorded.
pub fn verdict(catalog: &Catalog, call: &Json, fused: bool) -> Verdict {
    match catalog.bind() {
        Ok(bound) => verdict_bound(catalog, &bound, call, fused),
        Err(refusal) => Verdict::Refused(refusal),
    }
}

/// [`verdict`] under a binding made earlier.
pub fn verdict_bound(catalog: &Catalog, bound: &Bound, call: &Json, fused: bool) -> Verdict {
    let quack = match answer_bound(catalog, bound, call, fused) {
        Ok(a) => a,
        Err(refusal) => return Verdict::Refused(refusal),
    };
    let postgres = call.get("answer").cloned().unwrap_or(Json::Null);
    let same = match quack {
        Answer::Count(c) => postgres.as_u64() == Some(c),
        Answer::Exists(b) => postgres.as_bool() == Some(b),
        Answer::Rows(_) => false,
    };
    if same {
        Verdict::Equal(quack)
    } else {
        Verdict::Differ { quack, postgres }
    }
}

/// The projected column indices of a row call.
fn projection(table: &Table, select: Option<&Json>) -> Result<Vec<usize>, Refusal> {
    match select {
        Some(Json::String(star)) if star == "*" => Ok((0..table.columns.len()).collect()),
        Some(Json::Array(names)) => names
            .iter()
            .map(|name| {
                let name = name.as_str().ok_or(Refusal::Projection)?;
                table
                    .columns
                    .iter()
                    .position(|c| c.name == name)
                    .ok_or_else(|| Refusal::UnknownField {
                        table: table.name.clone(),
                        field: name.to_owned(),
                    })
            })
            .collect(),
        _ => Err(Refusal::Projection),
    }
}

struct OrderKey<'a> {
    values: &'a [u32],
    validity: &'a [u64],
    desc: bool,
}

impl OrderKey<'_> {
    /// PostgreSQL's rule: ASC puts NULLs last, DESC puts them first.
    fn compare(&self, a: usize, b: usize) -> Ordering {
        match (is_null(self.validity, a), is_null(self.validity, b)) {
            (true, true) => Ordering::Equal,
            (true, false) if self.desc => Ordering::Less,
            (true, false) => Ordering::Greater,
            (false, true) if self.desc => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                let order = value_at(self.values, a).cmp(&value_at(self.values, b));
                if self.desc { order.reverse() } else { order }
            }
        }
    }
}

fn compare_all(keys: &[OrderKey<'_>], a: usize, b: usize) -> Ordering {
    keys.iter()
        .map(|k| k.compare(a, b))
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// The ORDER BY keys of `call`, read from `lanes`: the table's lanes as
/// [`Session::lanes`] returned them for the same call that filters, so one
/// lane check serves both.
fn order_keys<'a>(
    table: &Table,
    lanes: &[Plane<'a>],
    call: &Json,
) -> Result<Vec<OrderKey<'a>>, Refusal> {
    let list = match call.get("order") {
        None | Some(Json::Null) => return Ok(Vec::new()),
        Some(Json::Array(list)) => list,
        Some(_) => return Err(malformed("order is not a list")),
    };
    let mut keys = Vec::with_capacity(list.len());
    for key in list {
        if let Some(unsupported) = key.get("unsupported") {
            let text = unsupported
                .as_str()
                .map_or_else(|| unsupported.to_string(), str::to_owned);
            return Err(Refusal::UnsupportedOrder(text));
        }
        let (name, col) = (str_of(key, "table")?, str_of(key, "col")?);
        if name != table.name {
            return Err(Refusal::ForeignTableColumn {
                table: name.to_owned(),
                field: col.to_owned(),
            });
        }
        let Some(column) = table.columns.iter().find(|c| c.name == col) else {
            return Err(Refusal::UnknownField {
                table: name.to_owned(),
                field: col.to_owned(),
            });
        };
        let (Some(lane), Some(slot)) = (
            &column.lane,
            table.slots().position(|s| s.column.name == col),
        ) else {
            return Err(Refusal::UnlanedColumn {
                table: name.to_owned(),
                field: col.to_owned(),
            });
        };
        if lane.kind == LaneKind::Text {
            return Err(Refusal::TextOrder {
                field: col.to_owned(),
            });
        }
        let desc = match str_of(key, "dir")? {
            "asc" => false,
            "desc" => true,
            other => return Err(malformed(format!("unknown order direction `{other}`"))),
        };
        let Some(&(values, validity)) = lanes.get(slot) else {
            return Err(malformed("lane index"));
        };
        keys.push(OrderKey {
            values,
            validity,
            desc,
        });
    }
    Ok(keys)
}

fn count_of(call: &Json, key: &str) -> Result<Option<usize>, Refusal> {
    match call.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| malformed(format!("`{key}` is not a count"))),
    }
}

/// Answers a captured row-returning call through quack.
///
/// The filter and the ORDER BY run over lanes and yield positions; cells are
/// copied from the source columns only for the positions of the answer.
pub fn rows(catalog: &Catalog, call: &Json, fused: bool) -> Result<RowsAnswer, Refusal> {
    rows_bound(catalog, &catalog.bind()?, call, fused)
}

/// [`rows`] under a binding made earlier.
pub fn rows_bound(
    catalog: &Catalog,
    bound: &Bound,
    call: &Json,
    fused: bool,
) -> Result<RowsAnswer, Refusal> {
    let session = Session::open(catalog, bound);
    guard(&session, call)?;
    if int_of(call, "joins")? > 0 {
        return join::rows(&session, call, fused);
    }
    refuse_shapes(call)?;
    if call.get("lock").and_then(Json::as_i64).unwrap_or(0) > 0 {
        return Err(Refusal::Lock);
    }
    let name = str_of(call, "table")?;
    let index = Binder::table(catalog, name)
        .and_then(|id| usize::try_from(id.0).ok())
        .ok_or_else(|| Refusal::UnknownTable(name.to_owned()))?;
    let table = session.table(index)?;
    let projected = projection(table, call.get("select"))?;
    let (_, filter) = where_filter(&session, call)?;
    // One lane check for the call: the ORDER BY keys and the filter read the same lanes.
    let lanes = session.lanes(index)?;
    let keys = order_keys(table, &lanes, call)?;
    let limit = count_of(call, "limit")?;
    let offset = count_of(call, "offset")?.unwrap_or(0);

    let mut bits = vec![0u64; words_for(table.rows)];
    let query = Query {
        filter,
        agg: Agg::Rows,
    };
    let program = lowered(&query, fused)?;
    run_over(
        &session,
        index,
        table,
        &lanes,
        &Derived::default(),
        &Foreigners::default(),
        &program,
        Out::Mask(&mut bits),
    )?;
    let mut positions = materialize_rows(&bits, table.rows);
    positions.sort_by(|&a, &b| compare_all(&keys, a, b));
    let kept = positions.len();

    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for i in 1..=kept {
        if i == kept || compare_all(&keys, positions[i - 1], positions[i]).is_ne() {
            groups.push((start, i));
            start = i;
        }
    }

    let first = offset.min(kept);
    let end = limit.map_or(kept, |l| offset.saturating_add(l).min(kept));
    let ordered = !keys.is_empty();
    if limit.is_some() || offset > 0 {
        if !ordered {
            if offset > 0 || end < kept {
                return Err(Refusal::UnorderedLimit);
            }
        } else if groups.iter().any(|&(s, e)| {
            let inside = e.min(end).saturating_sub(s.max(first));
            inside > 0 && inside < e - s
        }) {
            return Err(Refusal::TieAtLimit);
        }
    }

    let ties = if ordered {
        groups
            .iter()
            .map(|&(s, e)| e.min(end).saturating_sub(s.max(first)))
            .filter(|&n| n > 0)
            .collect()
    } else {
        Vec::new()
    };

    let window = positions.get(first..end).unwrap_or_default();
    let cells = Cells::new(catalog, index, &projected).ok_or_else(|| malformed("table index"))?;
    let mut materialized = 0;
    let mut out_rows = Vec::with_capacity(window.len());
    for &position in window {
        out_rows.push(cells.row(position));
        materialized += 1;
    }
    let columns = projected
        .iter()
        .filter_map(|&c| table.columns.get(c).map(|column| column.name.clone()))
        .collect();
    Ok(RowsAnswer {
        columns,
        rows: out_rows,
        ordered,
        ties,
        kept,
        materialized,
        join: None,
    })
}

fn row_keys(rows: &[Json]) -> Option<Vec<String>> {
    rows.iter()
        .map(|row| serde_json::to_string(row).ok())
        .collect()
}

fn same_multiset(a: &[String], b: &[String]) -> bool {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

fn rows_equal(got: &RowsAnswer, postgres: &Json) -> bool {
    let Some(columns) = postgres.get("columns").and_then(Json::as_array) else {
        return false;
    };
    let same_columns = columns.len() == got.columns.len()
        && columns
            .iter()
            .zip(&got.columns)
            .all(|(pg, ours)| pg.as_str() == Some(ours.as_str()));
    let Some(pg_rows) = postgres.get("rows").and_then(Json::as_array) else {
        return false;
    };
    if !same_columns || pg_rows.len() != got.rows.len() {
        return false;
    }
    let ours: Vec<Json> = got.rows.iter().cloned().map(Json::Array).collect();
    let (Some(ours), Some(theirs)) = (row_keys(&ours), row_keys(pg_rows)) else {
        return false;
    };
    if !got.ordered {
        return same_multiset(&ours, &theirs);
    }
    let mut at = 0;
    for &len in &got.ties {
        let (Some(a), Some(b)) = (ours.get(at..at + len), theirs.get(at..at + len)) else {
            return false;
        };
        if !same_multiset(a, b) {
            return false;
        }
        at += len;
    }
    at == ours.len()
}

/// Compares quack's rows with the `"answer"` PostgreSQL recorded.
pub fn rows_verdict(catalog: &Catalog, call: &Json, fused: bool) -> Verdict {
    match catalog.bind() {
        Ok(bound) => rows_verdict_bound(catalog, &bound, call, fused),
        Err(refusal) => Verdict::Refused(refusal),
    }
}

/// [`rows_verdict`] under a binding made earlier.
pub fn rows_verdict_bound(catalog: &Catalog, bound: &Bound, call: &Json, fused: bool) -> Verdict {
    let got = match rows_bound(catalog, bound, call, fused) {
        Ok(a) => a,
        Err(refusal) => return Verdict::Refused(refusal),
    };
    let postgres = call.get("answer").cloned().unwrap_or(Json::Null);
    let quack = Answer::Rows(got.rows.len());
    if rows_equal(&got, &postgres) {
        Verdict::Equal(quack)
    } else {
        Verdict::Differ { quack, postgres }
    }
}

/// Answers a recorded call against the catalog as the feed left it.
pub fn shadow_verdict(catalog: &Catalog, call: &Json, fused: bool) -> Verdict {
    match catalog.bind() {
        Ok(bound) => shadow_verdict_bound(catalog, &bound, call, fused),
        Err(refusal) => Verdict::Refused(refusal),
    }
}

/// [`shadow_verdict`] under a binding made earlier.
pub fn shadow_verdict_bound(catalog: &Catalog, bound: &Bound, call: &Json, fused: bool) -> Verdict {
    let table = call.get("table").and_then(Json::as_str);
    let dirty = call.get("dirty_tables").and_then(Json::as_array);
    if let (Some(table), Some(dirty)) = (table, dirty)
        && dirty.iter().any(|d| d.as_str() == Some(table))
    {
        return Verdict::Refused(Refusal::UncommittedRead {
            table: table.to_owned(),
        });
    }
    match call.get("terminal").and_then(Json::as_str) {
        Some("rows") => rows_verdict_bound(catalog, bound, call, fused),
        Some("count" | "exists") => verdict_bound(catalog, bound, call, fused),
        other => Verdict::Refused(malformed(format!("unknown terminal `{other:?}`"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_epoch_is_spelled_as_its_utc_time() {
        for (epoch, text) in [
            (0, "1970-01-01T00:00:00.000000Z"),
            (1_704_067_199, "2023-12-31T23:59:59.000000Z"),
            (1_704_067_203, "2024-01-01T00:00:03.000000Z"),
            (1_709_164_800, "2024-02-29T00:00:00.000000Z"),
            (1_709_251_199, "2024-02-29T23:59:59.000000Z"),
            (1_709_251_200, "2024-03-01T00:00:00.000000Z"),
            (951_782_400, "2000-02-29T00:00:00.000000Z"),
            (4_078_000_000, "2099-03-24T01:46:40.000000Z"),
            (u32::MAX, "2106-02-07T06:28:15.000000Z"),
        ] {
            assert_eq!(utc_text(epoch), text, "{epoch}");
        }
    }

    #[test]
    fn a_timestamp_cell_is_reproducible_only_as_the_capture_spells_it() {
        let object = json!({"t": "2024-01-01T00:00:03.000000Z", "epoch": 1_704_067_203, "usec": 0});
        assert_eq!(
            reproducible(&object),
            Some((Spelling::Object, 1_704_067_203))
        );
        assert_eq!(
            reproducible(&json!(1_704_067_203)),
            Some((Spelling::Bare, 1_704_067_203))
        );
        let wrong_text =
            json!({"t": "2024-01-01T00:00:04.000000Z", "epoch": 1_704_067_203, "usec": 0});
        let usec = json!({"t": "2024-01-01T00:00:03.000000Z", "epoch": 1_704_067_203, "usec": 5});
        let extra =
            json!({"t": "2024-01-01T00:00:03.000000Z", "epoch": 1_704_067_203, "usec": 0, "x": 1});
        let missing = json!({"epoch": 1_704_067_203, "usec": 0});
        for cell in [
            wrong_text,
            usec,
            extra,
            missing,
            json!(-1),
            json!(4_294_967_296_u64),
            json!(1.5),
            json!("x"),
        ] {
            assert_eq!(reproducible(&cell), None, "{cell}");
        }
        assert_eq!(
            uniform_spelling(&[json!(null), json!(7), json!(8)]),
            Some(Some(Spelling::Bare))
        );
        assert_eq!(uniform_spelling(&[json!(null)]), Some(None));
        assert_eq!(uniform_spelling(&[json!(7), object]), None);
    }

    #[test]
    fn only_an_integer_column_reads_as_a_key() {
        let mut catalog = Catalog::new();
        catalog
            .apply_feed(&json!({"tables": {"t": {
                "columns": [
                    {"name": "id", "type": "integer", "null": false},
                    {"name": "ok", "type": "boolean", "null": true},
                    {"name": "name", "type": "character varying", "null": true},
                ],
                "upsert": [[1, true, "a"], [2, false, "b"]],
                "delete": [],
            }}}))
            .expect("feed");
        let columns = &catalog.tables[0].columns;
        assert!(
            columns
                .iter()
                .all(|c| c.lane.is_some() && c.cells.is_empty())
        );
        assert_eq!(int_at(&columns[0], 1), Some(1));
        assert_eq!(int_at(&columns[1], 1), None, "a boolean is no key");
        assert_eq!(int_at(&columns[2], 1), None, "a text code is no key");
    }
}
