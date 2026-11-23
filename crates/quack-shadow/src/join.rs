// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Inner equi-joins answered as a bag through quack, without building the
//! cross product.
//!
//! The statement must be a star around one driving table `D`: `D` holds
//! edges to its dimensions (one hop), and at most one further table,
//! the sibling `S`, holds an edge to one of those dimensions. Each
//! edge column is a stored link (see `link.rs`): the position of the
//! referenced row beside a validity plane. The default path derives no link
//! and reads no position from another lane's value: the constant is
//! translated, whatever the layout of the referenced table. A dimension with a
//! filter of its own yields, in one program over it, the keys of its live rows
//! that satisfy the filter, and the driving filter tests the edge column with
//! `IN`. A dimension without one needs only existence: one grouped count of the
//! driving edge column says which keys are in use, and the boundary names those
//! that have no live target row; the driving filter excludes exactly them. Keys
//! cross between programs only as constants, never as a mask. A key becomes a
//! position only for a kept row, after the fold. The sibling's multiplicity per
//! dimension row is one `GroupReduce` count over `S`. The answer is first a
//! list of virtual tuples, one row position per table, and cells are read
//! only for the rows returned, a text cell rendered from its codebook.
//!
//! Under the oracle switch the edge is instead read through a stored
//! link and dimension values are gathered through it.

use std::collections::BTreeSet;

use lance_graph_mask_risc::{Out, Value, materialize_rows, words_for};
use lance_graph_quack::bind::Binder;
use lance_graph_quack::{
    Agg, Col, Filter, ForeignLane, GroupAddr, GroupAgg, Mask, Query, normalize_group_sink,
};
use serde_json::Value as Json;

use super::link::edge_lane;
use super::words::{self, Construct, Word};
use super::{
    Catalog, Cells, Ctx, Derived, Foreigners, Layout, Link, Literal, Positions, Reading, Refusal,
    RowsAnswer, Session, Slot, Table, execute_with, int_of, is_live_at, leaf, literal, live_count,
    malformed, projection, refuse_beyond_filter, str_of,
};

/// What a join call did instead of building the cross product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinStats {
    /// The table holding the edges to every dimension.
    pub driving: String,
    /// The tables the driving table references directly.
    pub dimensions: Vec<String>,
    /// The table that references one of the dimensions, if any.
    pub sibling: Option<String>,
    /// Driving positions the filter kept.
    pub driving_kept: usize,
    /// Rows of the cross product of the driving and the sibling table, which was not built.
    pub pairs: usize,
    /// The statement's tables: the FROM table first, then the joined tables in call order.
    pub tables: Vec<String>,
    /// The joined rows before any cell was read.
    pub tuples: Vec<VirtualTuple>,
    /// The key sets taken as constants: the dimension and how many keys.
    pub key_sets: Vec<(String, usize)>,
    /// The declared predicate of each ON pair, in edge order.
    pub relations: Vec<String>,
}

/// One joined row before any cell is read: a row position per statement table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualTuple {
    /// Per table of `JoinStats::tables`: its row position; `None` for the sibling, which is counted, not enumerated.
    pub positions: Vec<Option<u32>>,
    /// How many sibling rows join this tuple (1 without a sibling).
    pub times: usize,
}

/// An equality of a edge column and the `id` of the table it references.
struct Edge {
    holder: usize,
    edge_col: String,
    key: usize,
    predicate: String,
    reading: Option<Reading>,
}

struct Dim {
    table: usize,
    edge_col: String,
}

struct Sibling {
    table: usize,
    edge_col: String,
    via: usize,
    reading: Option<Reading>,
}

struct Shape {
    driving: usize,
    dims: Vec<Dim>,
    sibling: Option<Sibling>,
}

fn shape_err(reason: &str) -> Refusal {
    Refusal::JoinShape(reason.to_owned())
}

/// The catalog index and the frame-checked table named `name`.
fn find_table<'a>(session: &Session<'a>, name: &str) -> Result<(usize, &'a Table), Refusal> {
    let index = session
        .catalog()
        .tables
        .iter()
        .position(|t| t.name == name)
        .ok_or_else(|| Refusal::UnknownTable(name.to_owned()))?;
    Ok((index, session.table(index)?))
}

fn at<'a>(tables: &[&'a Table], index: usize) -> Result<&'a Table, Refusal> {
    tables
        .get(index)
        .copied()
        .ok_or_else(|| malformed("table index"))
}

fn catalog_ix(catalog_at: &[usize], index: usize) -> Result<usize, Refusal> {
    catalog_at
        .get(index)
        .copied()
        .ok_or_else(|| malformed("table index"))
}

fn index_of(tables: &[&Table], name: &str) -> Result<usize, Refusal> {
    tables
        .iter()
        .position(|t| t.name == name)
        .ok_or_else(|| shape_err("edge names a table outside the statement"))
}

fn side<'a>(entry: &'a Json, key: &str) -> Result<(&'a str, &'a str), Refusal> {
    let node = entry
        .get(key)
        .ok_or_else(|| malformed(format!("join without `{key}`")))?;
    Ok((str_of(node, "table")?, str_of(node, "col")?))
}

/// Whether the left side of an ON pair is the edge column, as declared at intake.
#[cfg(not(feature = "disable-relation-lookup"))]
fn left_is_edge(
    catalog: &Catalog,
    ((lt, lc), (rt, rc)): ((&str, &str), (&str, &str)),
) -> Result<bool, Refusal> {
    let declared = |st: &str, sc: &str, tt: &str, tc: &str| {
        catalog
            .relation(st, sc)
            .is_some_and(|r| r.target == tt && tc == "id")
    };
    match (declared(lt, lc, rt, rc), declared(rt, rc, lt, lc)) {
        (true, false) => Ok(true),
        (false, true) => Ok(false),
        (true, true) => Err(shape_err("join edge between two id columns")),
        (false, false) => {
            let (table, column) = if lc == "id" { (rt, rc) } else { (lt, lc) };
            Err(Refusal::UndeclaredRelation {
                table: table.to_owned(),
                column: column.to_owned(),
            })
        }
    }
}

// Disable run: the query decides which column is the key, as before intake declared it.
#[cfg(feature = "disable-relation-lookup")]
fn left_is_edge(
    _catalog: &Catalog,
    ((_, lc), (_, rc)): ((&str, &str), (&str, &str)),
) -> Result<bool, Refusal> {
    match (lc == "id", rc == "id") {
        (false, true) => Ok(true),
        (true, false) => Ok(false),
        (true, true) => Err(shape_err("join edge between two id columns")),
        (false, false) => Err(shape_err("join edge without a usable key side")),
    }
}

fn edge(
    catalog: &Catalog,
    (tables, catalog_at): (&[&Table], &[usize]),
    entry: &Json,
) -> Result<Edge, Refusal> {
    let ((lt, lc), (rt, rc)) = (side(entry, "left")?, side(entry, "right")?);
    let (li, ri) = (index_of(tables, lt)?, index_of(tables, rt)?);
    if li == ri {
        return Err(shape_err("join edge within one table"));
    }
    if lc == "id" && rc == "id" {
        return Err(shape_err("join edge between two id columns"));
    }
    let left_edge = left_is_edge(catalog, ((lt, lc), (rt, rc)))?;
    let ((holder, edge_col, edge_table), (key, key_col)) = if left_edge {
        ((li, lc, lt), (ri, rc))
    } else {
        ((ri, rc, rt), (li, lc))
    };
    if !catalog.is_key(catalog_ix(catalog_at, key)?, key_col) {
        return Err(shape_err("join edge without a usable key side"));
    }
    let declared = catalog.relation(edge_table, edge_col);
    Ok(Edge {
        holder,
        edge_col: edge_col.to_owned(),
        key,
        predicate: declared.map(|r| r.predicate.clone()).unwrap_or_default(),
        reading: declared.map(|r| r.reading),
    })
}

/// The shape of the statement when `driving` is the driving table.
fn fit(edges: &[Edge], tables: usize, driving: usize) -> Option<Shape> {
    let dims: Vec<Dim> = edges
        .iter()
        .filter(|e| e.holder == driving)
        .map(|e| Dim {
            table: e.key,
            edge_col: e.edge_col.clone(),
        })
        .collect();
    let mut rest = edges.iter().filter(|e| e.holder != driving);
    let sibling = match (rest.next(), rest.next()) {
        (None, _) => None,
        (Some(e), None) => Some(Sibling {
            table: e.holder,
            edge_col: e.edge_col.clone(),
            via: e.key,
            reading: e.reading,
        }),
        _ => return None,
    };
    if let Some(s) = &sibling
        && !dims.iter().any(|d| d.table == s.via)
    {
        return None;
    }
    let covered: BTreeSet<usize> = std::iter::once(driving)
        .chain(dims.iter().map(|d| d.table))
        .chain(sibling.as_ref().map(|s| s.table))
        .collect();
    let listed = 1 + dims.len() + usize::from(sibling.is_some());
    (covered.len() == listed && listed == tables).then_some(Shape {
        driving,
        dims,
        sibling,
    })
}

fn shape_of(edges: &[Edge], tables: usize) -> Result<Shape, Refusal> {
    let mut fits: Vec<Shape> = (0..tables).filter_map(|d| fit(edges, tables, d)).collect();
    match (fits.pop(), fits.is_empty()) {
        (Some(shape), true) => Ok(shape),
        (None, _) => Err(shape_err("not a star around one driving table")),
        (Some(_), false) => Err(shape_err("more than one table could drive the join")),
    }
}

fn derived_col(own: usize, k: usize) -> Result<Col, Refusal> {
    u16::try_from(own + k)
        .map(Col)
        .map_err(|_| malformed("derived lane index"))
}

fn derived_mask(own: usize, k: usize) -> Result<Mask, Refusal> {
    u16::try_from(own + k + 1)
        .map(Mask)
        .map_err(|_| malformed("derived validity index"))
}

/// Foreign value lanes handed to the executor, one per distinct dimension column.
struct ForeignLanes<'s> {
    keys: Vec<(usize, String)>,
    lanes: Vec<&'s [u32]>,
}

impl<'s> ForeignLanes<'s> {
    fn index(&mut self, table: usize, col: &str, lane: &'s [u32]) -> Result<ForeignLane, Refusal> {
        let at = match self.keys.iter().position(|(t, c)| *t == table && c == col) {
            Some(at) => at,
            None => {
                self.keys.push((table, col.to_owned()));
                self.lanes.push(lane);
                self.keys.len() - 1
            }
        };
        u16::try_from(at)
            .map(ForeignLane)
            .map_err(|_| malformed("foreign lane index"))
    }
}

/// The laned integer edge column `edge_col` of `source`.
fn edge_slot<'t>(source: &'t Table, edge_col: &str) -> Result<Slot<'t>, Refusal> {
    edge_lane(source, edge_col)?;
    source
        .slots()
        .find(|s| s.column.name == edge_col)
        .ok_or_else(|| malformed("edge slot"))
}

/// How a dimension's row position is read for the rows of the driving table.
enum DimRead {
    /// The stored link, by its place among the held ones.
    Link(usize),
    /// The edge column holding keys of the target; a key is placed
    /// only for a kept row.
    Keys(Col),
}

/// The slot of the NOT NULL laned column an `eq` or `in` leaf names on a
/// dimension, and the codes it compares with.
fn dimension_codes(cx: &Ctx<'_>, n: &Json) -> Result<(usize, Vec<u32>, Option<Word>), Refusal> {
    let (table, col) = (cx.table.name.as_str(), str_of(n, "col")?);
    let refused = || Refusal::ForeignPredicate {
        table: table.to_owned(),
        field: col.to_owned(),
    };
    let Some(column) = cx.table.columns.iter().find(|c| c.name == col) else {
        return Err(Refusal::UnknownField {
            table: table.to_owned(),
            field: col.to_owned(),
        });
    };
    let (Some(lane), Some(field)) = (&column.lane, Binder::field(cx.catalog, cx.id, col)) else {
        return Err(refused());
    };
    let op = str_of(n, "op")?;
    if column.nullable || !matches!(op, "eq" | "in") {
        return Err(refused());
    }
    let word = if op == "eq" {
        Some(cx.catalog.word_of_op(op)?)
    } else {
        cx.catalog.note_without_word(op);
        None
    };
    let slot = cx
        .table
        .slots()
        .position(|s| s.column.name == col)
        .ok_or_else(refused)?;
    let mut codes = Vec::new();
    let candidates: Vec<&Json> = if op == "eq" {
        vec![
            n.get("value")
                .ok_or_else(|| malformed("leaf without value"))?,
        ]
    } else {
        n.get("values")
            .and_then(Json::as_array)
            .ok_or_else(|| malformed("IN leaf without values"))?
            .iter()
            .collect()
    };
    for v in candidates {
        match literal(cx, &field, lane.kind, col, v)? {
            Literal::Null => return Err(refused()),
            Literal::Known(code) => codes.push(code),
            Literal::Unknown => {}
        }
    }
    Ok((slot, codes, word))
}

fn never() -> Filter {
    Filter::negate(Filter::plane(Mask(0)))
}

/// `eq` or `in` against a NOT NULL laned column of a dimension, read through
/// the driving table's position lane `edge`.
fn dimension_leaf<'s>(
    session: &'s Session<'_>,
    cx: &Ctx<'_>,
    table_index: usize,
    edge: Col,
    foreign: &mut ForeignLanes<'s>,
    n: &Json,
) -> Result<Filter, Refusal> {
    let (slot, codes, word) = dimension_codes(cx, n)?;
    let col = str_of(n, "col")?;
    let Some((values, _)) = session.lanes(table_index)?.into_iter().nth(slot) else {
        return Err(malformed("lane index"));
    };
    let key = foreign.index(table_index, col, values)?;
    let mut leaves: Vec<Filter> = codes
        .into_iter()
        .map(|code| match word {
            Some(word) => words::equal_via(word, edge, key, code),
            None => Ok(Filter::eq_u32_via(edge, key, code)),
        })
        .collect::<Result<_, _>>()?;
    session.catalog().note_gather(leaves.len() as u64);
    Ok(match leaves.len() {
        0 => never(),
        1 => leaves.pop().unwrap_or_else(never),
        _ => Filter::or(leaves),
    })
}

#[cfg(not(feature = "disable-key-set-liveness"))]
fn key_set_live() -> Filter {
    Filter::plane(Mask(0))
}

// Disable run: the key set is taken over every position, dead ones included,
// so a removed target row still joins.
#[cfg(feature = "disable-key-set-liveness")]
fn key_set_live() -> Filter {
    Filter::or([Filter::plane(Mask(0)), never()])
}

/// The values of the column `key` (the table's `id` for a dimension) of the live
/// rows of `table` that satisfy `leaves`, in one program over the table's positions.
pub(super) fn key_set(
    session: &Session<'_>,
    (at, table): (usize, &Table),
    key: &str,
    leaves: &[&Json],
    fused: bool,
) -> Result<Vec<u32>, Refusal> {
    if table.rows == 0 {
        return Ok(Vec::new());
    }
    let cx = ctx(session.catalog(), table)?;
    let mut parts = vec![key_set_live()];
    if !leaves.is_empty() {
        let mut tests = Vec::with_capacity(leaves.len());
        for n in leaves {
            let (slot, codes, _) = dimension_codes(&cx, n)?;
            let col = u16::try_from(slot)
                .map(Col)
                .map_err(|_| malformed("lane index"))?;
            tests.push(if codes.is_empty() {
                never()
            } else {
                Filter::in_u32(col, codes)
            });
        }
        parts.push(three_valued(table, Filter::and(tests)));
    }
    let mut bits = vec![0u64; words_for(table.rows)];
    let query = Query {
        filter: Filter::and(parts),
        agg: Agg::Rows,
    };
    execute_with(
        session,
        at,
        &Derived::default(),
        &Foreigners::default(),
        query,
        fused,
        Out::Mask(&mut bits),
    )?;
    let column = table
        .columns
        .iter()
        .find(|c| c.name == key)
        .ok_or_else(|| malformed("key set without its key column"))?;
    let mut keys = BTreeSet::new();
    for position in materialize_rows(&bits, table.rows) {
        session.catalog().census.ordinal_unchecked(1);
        let value = crate::int_at(column, position)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| Refusal::ValueOutOfLane {
                field: key.to_owned(),
            })?;
        keys.insert(value);
    }
    session.catalog().note_key_set(keys.len());
    Ok(keys.into_iter().collect())
}

/// The keys of `driving.edge_col` in use by live rows that name no live row of `target`.
///
/// One grouped count over the driving table says which keys are in use; only
/// those are looked up, at the boundary. The sink covers every key of both
/// tables, so no key falls outside it.
fn dangling_keys(
    session: &Session<'_>,
    (driving_at, driving): (usize, &Table),
    (target_at, target): (usize, &Table),
    edge_col: &str,
    fused: bool,
) -> Result<Vec<u32>, Refusal> {
    if driving.rows == 0 {
        return Ok(Vec::new());
    }
    let slot = edge_slot(driving, edge_col)?;
    session.catalog().census.bulk_unchecked();
    let largest_edge_key = slot.lane.values.iter().copied().max().unwrap_or(0);
    let largest_key = match target.layout {
        Layout::Identity => u32::try_from(target.rows.saturating_sub(1)).unwrap_or(u32::MAX),
        Layout::Append => {
            session.catalog().census.bulk_unchecked();
            edge_lane(target, "id")?
                .values
                .iter()
                .copied()
                .max()
                .unwrap_or(0)
        }
    };
    let size = usize::try_from(largest_edge_key.max(largest_key))
        .map_or(usize::MAX, |m| m.saturating_add(1));
    let bound = driving
        .rows
        .saturating_add(target.rows)
        .saturating_mul(4)
        .saturating_add(1024);
    if size > bound {
        return Err(Refusal::ValueOutOfLane {
            field: edge_col.to_owned(),
        });
    }
    let mut parts = vec![Filter::plane(Mask(0))];
    if slot.column.nullable {
        parts.push(Filter::plane(slot.validity));
    }
    let query = Query {
        filter: Filter::and(parts),
        agg: Agg::GroupReduce {
            key: GroupAddr::Local(slot.col),
            agg: GroupAgg::Count,
        },
    };
    let mut sink = vec![0i64; size];
    let value = execute_with(
        session,
        driving_at,
        &Derived::default(),
        &Foreigners::default(),
        query,
        fused,
        Out::I64(&mut sink),
    )?;
    if value != Value::GroupReduced {
        return Err(Refusal::Exec(format!("unexpected value {value:?}")));
    }
    normalize_group_sink(GroupAgg::Count.fold(), &mut sink);
    let catalog = session.catalog();
    let positions = match catalog.maps.get(target_at) {
        Some(map) => Positions::new(map, target.rows, &catalog.census),
        None => return Err(malformed("table index")),
    };
    let mut dangling = Vec::new();
    for key in sink
        .iter()
        .enumerate()
        .filter(|&(_, &count)| count > 0)
        .filter_map(|(key, _)| u32::try_from(key).ok())
    {
        let live = positions
            .place(target, i64::from(key))
            .is_some_and(|at| is_live_at(target, at));
        if !live && !cfg!(feature = "disable-target-presence") {
            dangling.push(key);
        }
    }
    catalog.note_existence(dangling.len() as u64);
    Ok(dangling)
}

/// The groups a count keyed by the key of `via` needs: one slot per key up to the largest.
fn key_groups(via: &Table, edge_col: &str) -> Result<usize, Refusal> {
    let lane = edge_lane(via, "id")?;
    let Some(&max) = lane.values.iter().max() else {
        return Ok(0);
    };
    let groups = usize::try_from(max).map_or(usize::MAX, |max| max.saturating_add(1));
    let bound = via.rows.saturating_mul(4).saturating_add(1024);
    if groups > bound {
        return Err(Refusal::ValueOutOfLane {
            field: edge_col.to_owned(),
        });
    }
    Ok(groups)
}

#[cfg(not(feature = "disable-sql-where"))]
fn three_valued(table: &Table, filter: Filter) -> Filter {
    filter.sql_where(&table.nullable())
}

// Disable run: a NULL then reads as its stored 0.
#[cfg(feature = "disable-sql-where")]
fn three_valued(_table: &Table, filter: Filter) -> Filter {
    filter
}

fn is_leaf(n: &Json) -> bool {
    matches!(
        n.get("op").and_then(Json::as_str),
        Some("eq" | "ne" | "in" | "not_in")
    )
}

/// The leaves of a WHERE that is one leaf or an AND of leaves.
fn leaves<'c>(catalog: &Catalog, call: &'c Json) -> Result<Vec<&'c Json>, Refusal> {
    let tree = match call.get("where") {
        None | Some(Json::Null) => return Ok(Vec::new()),
        Some(tree) => tree,
    };
    let list: Vec<&Json> = match tree.get("op").and_then(Json::as_str) {
        Some(op) if catalog.is_junction(op, Construct::And) => tree
            .get("args")
            .and_then(Json::as_array)
            .ok_or_else(|| malformed("junction without args"))?
            .iter()
            .collect(),
        _ => vec![tree],
    };
    for n in &list {
        if n.get("op").and_then(Json::as_str) == Some("unsupported") {
            return Err(Refusal::UnsupportedNode(str_of(n, "node")?.to_owned()));
        }
        if !is_leaf(n) {
            return Err(Refusal::JoinPredicate);
        }
    }
    Ok(list)
}

fn ctx<'a>(catalog: &'a Catalog, table: &'a Table) -> Result<Ctx<'a>, Refusal> {
    let id = Binder::table(catalog, &table.name)
        .ok_or_else(|| Refusal::UnknownTable(table.name.clone()))?;
    Ok(Ctx { catalog, table, id })
}

fn own_filter(cx: &Ctx<'_>, leaves: &[&Json]) -> Result<Option<Filter>, Refusal> {
    let parts = leaves
        .iter()
        .map(|n| leaf(cx, n, str_of(n, "op")?))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((!parts.is_empty()).then(|| three_valued(cx.table, Filter::and(parts))))
}

/// The multiplicity per dimension row: the sibling rows that survive its own
/// filter and join that row.
fn sibling_counts(
    session: &Session<'_>,
    (sibling_at, sibling): (usize, &Table),
    sibling_edge: &str,
    (via_at, via): (usize, &Table),
    own_leaves: &[&Json],
    fused: bool,
) -> Result<Vec<i64>, Refusal> {
    let catalog = session.catalog();
    let link;
    let mut parts = vec![Filter::plane(Mask(0))];
    let (derived, key, foreigners, groups) = if catalog.reads_key_set(via) {
        let slot = edge_slot(sibling, sibling_edge)?;
        catalog.census.bulk_unchecked();
        if slot.column.nullable {
            parts.push(Filter::plane(slot.validity));
        }
        catalog.census.bulk_unchecked();
        let groups = key_groups(via, sibling_edge)?;
        (Derived::default(), slot.col, Foreigners::default(), groups)
    } else {
        link = catalog.link(sibling_at, sibling_edge, via_at)?;
        let own = sibling.slots().count();
        parts.push(Filter::plane(derived_mask(own, 0)?));
        let derived = Derived {
            lanes: vec![link.values()],
            validity: vec![link.validity()],
        };
        (
            derived,
            derived_col(own, 0)?,
            Foreigners::default(),
            via.rows,
        )
    };
    parts.extend(own_filter(&ctx(catalog, sibling)?, own_leaves)?);
    // No dimension row means no group: mask-risc refuses a zero-length sink.
    if groups == 0 {
        return Ok(Vec::new());
    }
    let agg = Agg::GroupReduce {
        key: GroupAddr::Local(key),
        agg: GroupAgg::Count,
    };
    let mut sink = vec![0i64; groups];
    let query = Query {
        filter: Filter::and(parts),
        agg,
    };
    let value = execute_with(
        session,
        sibling_at,
        &derived,
        &foreigners,
        query,
        fused,
        Out::I64(&mut sink),
    )?;
    if value != Value::GroupReduced {
        return Err(Refusal::Exec(format!("unexpected value {value:?}")));
    }
    normalize_group_sink(GroupAgg::Count.fold(), &mut sink);
    Ok(sink)
}

#[cfg(not(feature = "disable-join-multiplicity"))]
fn multiplicity(count: usize) -> usize {
    count
}

// Disable run: the join only tracks existence.
#[cfg(feature = "disable-join-multiplicity")]
fn multiplicity(count: usize) -> usize {
    count.min(1)
}

fn refuse_order(call: &Json) -> Result<(), Refusal> {
    let ordered = match call.get("order") {
        None | Some(Json::Null) => false,
        Some(Json::Array(keys)) => !keys.is_empty(),
        Some(_) => true,
    };
    let paged = |key: &str| call.get(key).is_some_and(|v| !v.is_null());
    if ordered || paged("limit") || paged("offset") {
        return Err(Refusal::JoinOrder);
    }
    Ok(())
}

/// Answers a captured row-returning call with joins as a bag.
pub(super) fn rows(session: &Session<'_>, call: &Json, fused: bool) -> Result<RowsAnswer, Refusal> {
    let catalog = session.catalog();
    let entries = call
        .get("join")
        .and_then(Json::as_array)
        .ok_or_else(|| malformed("join call without a join list"))?;
    for entry in entries {
        if let Some(unsupported) = entry.get("unsupported") {
            let text = unsupported
                .as_str()
                .map_or_else(|| unsupported.to_string(), str::to_owned);
            return Err(Refusal::UnsupportedJoin(text));
        }
        let kind = str_of(entry, "type")?;
        if kind != "inner" {
            return Err(Refusal::UnsupportedJoin(kind.to_owned()));
        }
    }
    refuse_beyond_filter(call)?;
    if int_of(call, "lock").unwrap_or(0) > 0 {
        return Err(Refusal::Lock);
    }
    refuse_order(call)?;

    let (from_at, from) = find_table(session, str_of(call, "table")?)?;
    let Some(Json::Array(_)) = call.get("select") else {
        return Err(Refusal::Projection);
    };
    let projected = projection(from, call.get("select"))?;

    let mut tables: Vec<&Table> = vec![from];
    let mut catalog_at: Vec<usize> = vec![from_at];
    for entry in entries {
        let (index, table) = find_table(session, str_of(entry, "table")?)?;
        if tables.iter().any(|t| t.name == table.name) {
            return Err(shape_err("table joined twice"));
        }
        tables.push(table);
        catalog_at.push(index);
    }
    let edges = entries
        .iter()
        .map(|entry| edge(catalog, (&tables, &catalog_at), entry))
        .collect::<Result<Vec<_>, _>>()?;
    let relations: Vec<String> = edges.iter().map(|e| e.predicate.clone()).collect();
    let shape = shape_of(&edges, tables.len())?;
    if shape.sibling.as_ref().is_some_and(|s| s.table == 0) {
        return Err(shape_err("the FROM table is the sibling"));
    }
    // The table whose rows are counted must be declared a class of occurrences.
    if !cfg!(feature = "disable-join-multiplicity")
        && shape
            .sibling
            .as_ref()
            .is_some_and(|s| s.reading == Some(Reading::Edge))
    {
        return Err(Refusal::UnsupportedJoin(
            "shape and declared reading disagree".to_owned(),
        ));
    }
    for dim in &shape.dims {
        let table = at(&tables, dim.table)?;
        let translates = catalog.translates_positions();
        if translates && table.layout == Layout::Append && table.rows != live_count(table) {
            return Err(shape_err("dimension with deleted rows"));
        }
    }

    let driving = at(&tables, shape.driving)?;
    let driving_at = catalog_ix(&catalog_at, shape.driving)?;
    let own = driving.slots().count();
    let own_lanes = session.lanes(driving_at)?;
    let mut driving_leaves: Vec<&Json> = Vec::new();
    let mut sibling_leaves: Vec<&Json> = Vec::new();
    let mut dimension_leaves: Vec<(usize, &Json)> = Vec::new();
    for n in leaves(catalog, call)? {
        let (name, col) = (str_of(n, "table")?, str_of(n, "col")?);
        let Some(index) = tables.iter().position(|t| t.name == name) else {
            return Err(Refusal::ForeignTableColumn {
                table: name.to_owned(),
                field: col.to_owned(),
            });
        };
        if index == shape.driving {
            driving_leaves.push(n);
        } else if shape.sibling.as_ref().is_some_and(|s| s.table == index) {
            sibling_leaves.push(n);
        } else if let Some(k) = shape.dims.iter().position(|d| d.table == index) {
            dimension_leaves.push((k, n));
        }
    }

    let mut held: Vec<Link> = Vec::new();
    let mut reads: Vec<DimRead> = Vec::with_capacity(shape.dims.len());
    let mut parts = vec![Filter::plane(Binder::live(
        catalog,
        ctx(catalog, driving)?.id,
    ))];
    let mut key_sets: Vec<(String, usize)> = Vec::new();
    for (k_dim, dim) in shape.dims.iter().enumerate() {
        let target = at(&tables, dim.table)?;
        let target_at = catalog_ix(&catalog_at, dim.table)?;
        if catalog.reads_key_set(target) {
            let own_leaves: Vec<&Json> = dimension_leaves
                .iter()
                .filter(|&&(i, _)| i == k_dim)
                .map(|&(_, n)| n)
                .collect();
            let slot = edge_slot(driving, &dim.edge_col)?;
            catalog.census.bulk_unchecked();
            if own_leaves.is_empty() {
                if slot.column.nullable {
                    parts.push(Filter::plane(slot.validity));
                }
                let dangling = dangling_keys(
                    session,
                    (driving_at, driving),
                    (target_at, target),
                    &dim.edge_col,
                    fused,
                )?;
                if !dangling.is_empty() {
                    parts.push(Filter::negate(Filter::in_u32(slot.col, dangling)));
                }
            } else {
                let keys = key_set(session, (target_at, target), "id", &own_leaves, fused)?;
                let member = if keys.is_empty() {
                    never()
                } else {
                    Filter::in_u32(slot.col, keys.iter().copied())
                };
                parts.push(if slot.column.nullable {
                    three_valued(driving, member)
                } else {
                    member
                });
                key_sets.push((target.name.clone(), keys.len()));
            }
            reads.push(DimRead::Keys(slot.col));
        } else {
            let k = held.len();
            held.push(catalog.link(driving_at, &dim.edge_col, target_at)?);
            parts.push(Filter::plane(derived_mask(own, k)?));
            reads.push(DimRead::Link(k));
        }
    }
    let derived = Derived {
        lanes: held.iter().map(|l| l.values()).collect(),
        validity: held.iter().map(|l| l.validity()).collect(),
    };
    let edge_cols = reads
        .iter()
        .map(|read| match read {
            DimRead::Keys(col) => Ok(*col),
            DimRead::Link(k) => derived_col(own, *k),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let position_lanes = reads
        .iter()
        .map(|read| match read {
            DimRead::Keys(col) => own_lanes.get(usize::from(col.0)).map(|&(values, _)| values),
            DimRead::Link(k) => derived.lanes.get(*k).copied(),
        })
        .collect::<Option<Vec<&[u32]>>>()
        .ok_or_else(|| malformed("position lane"))?;

    parts.extend(own_filter(&ctx(catalog, driving)?, &driving_leaves)?);
    let mut foreign = ForeignLanes {
        keys: Vec::new(),
        lanes: Vec::new(),
    };
    for (k, n) in dimension_leaves {
        if matches!(reads.get(k), Some(DimRead::Keys(_))) {
            continue;
        }
        let Some(dim) = shape.dims.get(k) else {
            return Err(malformed("dimension index"));
        };
        let table = at(&tables, dim.table)?;
        parts.push(dimension_leaf(
            session,
            &ctx(catalog, table)?,
            catalog_ix(&catalog_at, dim.table)?,
            edge_cols
                .get(k)
                .copied()
                .ok_or_else(|| malformed("dimension index"))?,
            &mut foreign,
            n,
        )?);
    }

    let counts = match &shape.sibling {
        Some(s) => Some(sibling_counts(
            session,
            (catalog_ix(&catalog_at, s.table)?, at(&tables, s.table)?),
            &s.edge_col,
            (catalog_ix(&catalog_at, s.via)?, at(&tables, s.via)?),
            &sibling_leaves,
            fused,
        )?),
        None => None,
    };

    let mut bits = vec![0u64; words_for(driving.rows)];
    let query = Query {
        filter: Filter::and(parts),
        agg: Agg::Rows,
    };
    execute_with(
        session,
        driving_at,
        &derived,
        &Foreigners {
            lanes: &foreign.lanes,
            planes: &[],
        },
        query,
        fused,
        Out::Mask(&mut bits),
    )?;
    let kept = materialize_rows(&bits, driving.rows);

    let lane_at = |k: usize, row: usize| {
        position_lanes
            .get(k)
            .and_then(|lane| lane.get(row))
            .map_or(0, |&p| p as usize)
    };
    let via_lane = match &shape.sibling {
        Some(s) => shape.dims.iter().position(|d| d.table == s.via),
        None => None,
    };

    let mut places: Vec<Option<(Positions<'_>, &Table)>> = Vec::with_capacity(shape.dims.len());
    for (read, dim) in reads.iter().zip(&shape.dims) {
        let place = match read {
            DimRead::Keys(_) => {
                let target = at(&tables, dim.table)?;
                catalog
                    .maps
                    .get(catalog_ix(&catalog_at, dim.table)?)
                    .map(|map| (Positions::new(map, target.rows, &catalog.census), target))
            }
            _ => None,
        };
        places.push(place);
    }
    let mut placed = 0u64;
    let mut tuples = Vec::with_capacity(kept.len());
    for &row in &kept {
        let times = match (&counts, via_lane) {
            (Some(counts), Some(k)) => {
                let count = counts.get(lane_at(k, row)).copied().unwrap_or(0);
                multiplicity(usize::try_from(count).unwrap_or(0))
            }
            _ => 1,
        };
        if times == 0 {
            continue;
        }
        let mut positions = vec![None; tables.len()];
        for (t, slot) in positions.iter_mut().enumerate() {
            *slot = if t == shape.driving {
                u32::try_from(row).ok()
            } else {
                match shape.dims.iter().position(|d| d.table == t) {
                    None => None,
                    Some(k) => match places.get(k) {
                        Some(Some((positions, target))) => {
                            placed += 1;
                            let at = positions
                                .place(target, i64::try_from(lane_at(k, row)).unwrap_or(-1))
                                .and_then(|at| u32::try_from(at).ok());
                            Some(at.ok_or_else(|| malformed("a kept key has no position"))?)
                        }
                        _ => u32::try_from(lane_at(k, row)).ok(),
                    },
                }
            };
        }
        tuples.push(VirtualTuple { positions, times });
    }

    catalog.note_placed(placed);

    let rendering =
        Cells::new(catalog, from_at, &projected).ok_or_else(|| malformed("table index"))?;
    let mut out_rows = Vec::new();
    let mut materialized = 0;
    for tuple in &tuples {
        let Some(&Some(position)) = tuple.positions.first() else {
            return Err(malformed("the FROM table has no position"));
        };
        let cells = rendering.row(position as usize);
        for _ in 0..tuple.times {
            out_rows.push(cells.clone());
            materialized += 1;
        }
    }

    let columns = projected
        .iter()
        .filter_map(|&c| from.columns.get(c).map(|column| column.name.clone()))
        .collect();
    let sibling_table = match &shape.sibling {
        Some(s) => Some(at(&tables, s.table)?),
        None => None,
    };
    let stats = JoinStats {
        driving: driving.name.clone(),
        dimensions: shape
            .dims
            .iter()
            .map(|d| at(&tables, d.table).map(|t| t.name.clone()))
            .collect::<Result<_, _>>()?,
        sibling: sibling_table.map(|t| t.name.clone()),
        driving_kept: kept.len(),
        pairs: sibling_table.map_or(0, |s| driving.rows.saturating_mul(s.rows)),
        tables: tables.iter().map(|t| t.name.clone()).collect(),
        tuples,
        key_sets,
        relations,
    };
    Ok(RowsAnswer {
        columns,
        rows: out_rows,
        ordered: false,
        ties: Vec::new(),
        kept: kept.len(),
        materialized,
        join: Some(stats),
    })
}
