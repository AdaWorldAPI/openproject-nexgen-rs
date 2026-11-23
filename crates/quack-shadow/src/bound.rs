// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The lanes of a catalog read through DT64: a frame per table, a class
//! binding where the OGAR basis names the table's columns, and a [`Session`]
//! that is the only way the run path reads a table.
//!
//! A binding is made once ([`Catalog::bind`]) and checked on every read of a
//! table: a feed that touches a table advances its frame, so a binding made
//! before it is refused for that table and only that table.

use std::collections::{BTreeMap, BTreeSet};

use lance_graph_contract::ontology::FieldRef;
use lance_graph_contract::pg_row::{
    AbsentAs, CodeText, LaneEncoding, PgColumn, PgType, Written, write_cell,
};
use lance_graph_dt64::dr::{CodeSpaceId, Frame, FrameRefusal, FrameStore, MapTable, Scope};
use lance_graph_dt64::{
    CoordSpace, LaneAddr, LaneBinding, LaneRefusal, LaneSource, PresenceSource, SchemaRev, SelAddr,
    SelectionSource,
};
use lance_graph_mask_risc::LaneRef;
use lance_graph_pg_row::{ColumnSpec, LiveSource, PgRowRefusal, Reader};
use rails_f000::exec::REV;
use rails_f000::lanes::check_plane;
use rails_f000::ogar_basis::{FieldMeta, OgarBasis};

use super::{
    Catalog, Census, Codebook, LaneKind, Reading, Refusal, Relation, Selection, Table,
};

const SCOPE: Scope = Scope { id: 1 };

/// FNV-1a: a population id that does not change between runs.
fn population(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Positions are append-only, so the row count identifies the coordinate assignment.
fn space_of(table: &Table) -> CoordSpace {
    space_with(table, table.rows)
}

/// The row order of `table` as it would be with `rows` positions.
pub(crate) fn space_with(table: &Table, rows: usize) -> CoordSpace {
    CoordSpace {
        population: population(&table.name),
        coordinates: rows as u64 + 1,
    }
}

/// The OGAR class each captured table was read through, and the engine whose
/// store the class bindings are made in.
pub struct ClassMap {
    engine: rails_f000::Engine,
    by_table: BTreeMap<String, String>,
}

impl ClassMap {
    /// `by_table`: table -> the model ActiveRecord answered the captured calls with.
    pub fn new(basis: &OgarBasis, by_table: BTreeMap<String, String>) -> Result<ClassMap, String> {
        let mut engine = rails_f000::Engine::new(basis).map_err(|e| format!("{e:?}"))?;
        let classes: BTreeSet<&String> = by_table.values().collect();
        for class in classes {
            if basis.class(class).is_none() {
                return Err(format!("class `{class}` is not in the basis"));
            }
            engine
                .register(class, Vec::new())
                .map_err(|e| format!("{class}: {e:?}"))?;
        }
        Ok(ClassMap { engine, by_table })
    }

    /// The code space of `column` of `table`: the bound class's id and the
    /// predicate of the field the column resolves to, as the lanes resolve it.
    #[cfg_attr(feature = "disable-class-code-space", allow(dead_code))]
    pub(crate) fn space_of(&self, table: &Table, column: &str) -> Option<CodeSpaceId> {
        let class = self.by_table.get(&table.name)?;
        let basis = self.engine.basis();
        let classid = basis.class(class)?.classid();
        let slot = table.slots().position(|s| s.column.name == column)?;
        let field = resolve_fields(basis, class, table).get(slot).copied()??;
        let field: &FieldRef = basis.field(class, usize::from(field))?;
        Some(CodeSpaceId {
            class: classid,
            property: field.predicate_iri.clone(),
        })
    }

    /// The `belongs_to` edges of the bound classes, between their tables, and
    /// how many were left out because the target class has no table here.
    pub(crate) fn edges(&self) -> (Vec<Relation>, usize) {
        let basis = self.engine.basis();
        let (mut edges, mut skipped) = (Vec::new(), 0);
        for (table, class) in &self.by_table {
            let Some(class) = basis.class(class) else {
                continue;
            };
            for meta in &class.meta {
                let FieldMeta::AssocFk {
                    assoc,
                    target,
                    column,
                    ..
                } = meta
                else {
                    continue;
                };
                let Some((target_table, _)) = self.by_table.iter().find(|(_, c)| *c == target)
                else {
                    skipped += 1;
                    continue;
                };
                edges.push(Relation {
                    source: table.clone(),
                    column: column.clone(),
                    target: target_table.clone(),
                    predicate: assoc.clone(),
                    reading: Reading::Edge,
                });
            }
        }
        (edges, skipped)
    }
}

/// One table's lanes as a DT64 source: the lane addressed is the slot whose
/// predicate is the one the class basis gives the requested field.
struct TableSource<'a> {
    table: &'a Table,
    /// The table's index in the catalog, which its selections name.
    index: usize,
    /// Every selection the catalog issued; this table's are the ones naming `index`.
    selections: &'a [Selection],
    frames: &'a FrameStore,
    class: u32,
    space: CoordSpace,
    scope: Scope,
    /// Per slot (the k-th laned column): the predicate of the class field the
    /// column resolves to; `None` when the class basis has no such column.
    predicates: Vec<Option<&'a str>>,
    census: &'a Census,
}

impl<'a> TableSource<'a> {
    fn new(
        catalog: &'a Catalog,
        index: usize,
        frames: &'a FrameStore,
        classes: &'a ClassMap,
        class: &str,
        fields: &[Option<u8>],
    ) -> Option<TableSource<'a>> {
        let table = catalog.tables.get(index)?;
        let basis = classes.engine.basis();
        let classid = basis.class(class)?.classid();
        let predicates = fields
            .iter()
            .map(|field| {
                field.and_then(|f| {
                    basis
                        .field(class, usize::from(f))
                        .map(|f: &FieldRef| f.predicate_iri.as_str())
                })
            })
            .collect();
        Some(TableSource {
            table,
            index,
            selections: &catalog.selections,
            frames,
            class: classid,
            space: space_of(table),
            scope: SCOPE,
            predicates,
            census: &catalog.census,
        })
    }

    fn served(&self, addr: &LaneAddr) -> Option<&'a crate::Lane> {
        if addr.space != self.space || addr.scope != self.scope {
            return None;
        }
        self.table
            .columns
            .iter()
            .filter_map(|c| c.lane.as_ref())
            .nth(usize::from(addr.lane))
    }
}

impl LaneSource for TableSource<'_> {
    type Lane<'a>
        = LaneRef<'a>
    where
        Self: 'a;

    fn lane_of(
        &self,
        class: u32,
        rev: SchemaRev,
        _field: u8,
        basis: &FieldRef,
        frame: &Frame,
    ) -> Option<LaneAddr> {
        self.census.source_lane_of();
        self.frames.check(frame).ok()?;
        if class != self.class || rev != REV {
            return None;
        }
        let slot = self
            .predicates
            .iter()
            .position(|p| *p == Some(basis.predicate_iri.as_str()))?;
        Some(LaneAddr {
            space: self.space,
            lane: u16::try_from(slot).ok()?,
            scope: self.scope,
        })
    }

    fn borrow(&self, addr: &LaneAddr) -> Option<LaneRef<'_>> {
        self.census.source_borrow();
        self.served(addr).map(|lane| LaneRef::U32(&lane.values))
    }

    fn frames(&self) -> &FrameStore {
        self.census.source_frames();
        self.frames
    }
}

/// The table's own live plane, lent for the row order the binding was made in.
impl LiveSource for TableSource<'_> {
    fn borrow_live(&self, space: CoordSpace) -> Option<&[u64]> {
        (space == self.space).then_some(self.table.live.as_slice())
    }
}

impl PresenceSource for TableSource<'_> {
    fn borrow_presence(&self, addr: &LaneAddr) -> Option<&[u64]> {
        self.census.source_presence();
        self.served(addr).map(|lane| lane.validity.as_slice())
    }
}

/// A selection is the source's own: it is readable only under the frame its
/// table had when it was issued, and only for the table it was issued for.
impl SelectionSource for TableSource<'_> {
    fn selection_of(&self, sel: u16, frame: &Frame) -> Option<SelAddr> {
        self.census.source_selection_of();
        let held = self.selections.get(usize::from(sel))?;
        if held.table != self.index {
            return None;
        }
        let issued_under = self.frames.check(frame).is_ok() && held.frame.same_issue(frame);
        if !issued_under && !cfg!(feature = "disable-vrow-foreign-frame") {
            return None;
        }
        // The disable run reports the table's current order whatever the
        // selection was issued in.
        let space = if cfg!(feature = "disable-vrow-swap") {
            self.space
        } else {
            held.space
        };
        Some(SelAddr {
            space,
            sel,
            scope: self.scope,
        })
    }

    fn borrow_positions(&self, addr: &SelAddr) -> Option<&[u32]> {
        self.census.source_positions();
        let held = self.selections.get(usize::from(addr.sel))?;
        if addr.space != held.space && !cfg!(feature = "disable-vrow-swap") {
            return None;
        }
        if addr.scope != self.scope {
            return None;
        }
        (held.table == self.index).then_some(held.positions.as_slice())
    }
}

struct ClassBinding {
    class: String,
    binding: LaneBinding,
    /// Per slot: the class field the slot's column is bound to.
    fields: Vec<Option<u8>>,
}

struct BoundTable {
    name: String,
    frame: Frame,
    class: Option<ClassBinding>,
}

/// A catalog's tables sealed under their frames, class-bound where a class map
/// names the table. Valid for a table until a feed touches it.
pub struct Bound {
    tables: Vec<BoundTable>,
}

/// Which columns of a table are read through the class binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableBinding {
    pub table: String,
    pub class: Option<String>,
    pub bound: Vec<String>,
    pub frame_only: Vec<String>,
}

impl Bound {
    /// Laned columns by name, in table order. A column the class basis does not
    /// have, and every column of a table without a class, is `frame_only`.
    pub fn report(&self, catalog: &Catalog) -> Vec<TableBinding> {
        self.tables
            .iter()
            .filter_map(|bt| {
                let table = catalog.tables.iter().find(|t| t.name == bt.name)?;
                let mut out = TableBinding {
                    table: bt.name.clone(),
                    class: bt.class.as_ref().map(|c| c.class.clone()),
                    bound: Vec::new(),
                    frame_only: Vec::new(),
                };
                for (k, slot) in table.slots().enumerate() {
                    let is_bound = bt
                        .class
                        .as_ref()
                        .is_some_and(|c| c.fields.get(k).is_some_and(Option::is_some));
                    let names = if is_bound {
                        &mut out.bound
                    } else {
                        &mut out.frame_only
                    };
                    names.push(slot.column.name.clone());
                }
                Some(out)
            })
            .collect()
    }
}

/// The column types and the rows of a read through a selection, as PostgreSQL
/// prints them: a cell is its text form, `None` is SQL NULL.
pub type SelectedRows = (Vec<String>, Vec<Vec<Option<String>>>);

impl Bound {
    /// The class-bound columns of `table`, in table order, of the rows the
    /// selection `sel` names, in the selection's order. The selection is
    /// issued by [`Catalog::issue_selection`]; one check admits the lanes and
    /// the positions, and a cell is rendered only when its row is emitted.
    pub fn read_selection(
        &self,
        catalog: &Catalog,
        table: &str,
        sel: u16,
    ) -> Result<SelectedRows, Refusal> {
        let Some(index) = catalog.tables.iter().position(|t| t.name == table) else {
            return Err(Refusal::UnknownTable(table.to_owned()));
        };
        Session::open(catalog, self).selected(index, sel)
    }
}

impl Bound {
    /// The row of a missing side (a left join's NULL-extended parent): every
    /// bound cell is −0 (presence 0 and value 0, NaN), which is absent per
    /// `lance_graph_contract::lane_codec` § Absent and EXO-294, rendered
    /// through the same rule as any lane cell. No position is named and no
    /// lane is read.
    pub fn absent_row(
        &self,
        catalog: &Catalog,
        table: &str,
    ) -> Result<Vec<Option<String>>, Refusal> {
        let Some(index) = catalog.tables.iter().position(|t| t.name == table) else {
            return Err(Refusal::UnknownTable(table.to_owned()));
        };
        let t = catalog
            .tables
            .get(index)
            .ok_or_else(|| crate::malformed("table index"))?;
        let class = self
            .tables
            .iter()
            .find(|b| b.name == table)
            .and_then(|b| b.class.as_ref())
            .ok_or_else(|| Refusal::Bind(format!("{table}: no class binding")))?;
        t.slots()
            .enumerate()
            .filter(|(k, _)| class.fields.get(*k).copied().flatten().is_some())
            .map(|(_, slot)| {
                let column = renderable(
                    table,
                    &slot.column.name,
                    slot.lane.kind,
                    &slot.column.sql_type,
                )?;
                spell(
                    table,
                    &slot.column.name,
                    &column,
                    cfg!(feature = "disable-vrow-presence"),
                    0,
                    None,
                    0,
                )
            })
            .collect()
    }
}

/// The contract column a lane of `kind` and PostgreSQL type `sql_type` is
/// spelled as. A type the contract does not admit, such as a timestamp with a
/// time zone (PostgreSQL prints an offset the lane does not hold), is refused.
fn renderable(
    table: &str,
    column: &str,
    kind: LaneKind,
    sql_type: &str,
) -> Result<PgColumn, Refusal> {
    let unrenderable = || Refusal::Unrenderable {
        table: table.to_owned(),
        column: column.to_owned(),
        sql_type: sql_type.to_owned(),
    };
    let ty = PgType::parse(sql_type).map_err(|_| unrenderable())?;
    let encoding = match kind {
        LaneKind::Bool => LaneEncoding::Bool,
        LaneKind::Int if ty == PgType::Int8 => LaneEncoding::Unsigned,
        LaneKind::Int => LaneEncoding::Signed,
        LaneKind::Timestamp => LaneEncoding::EpochSeconds,
        LaneKind::Text => LaneEncoding::TextCode,
    };
    let column = PgColumn {
        ty,
        encoding,
        absent: AbsentAs::Null,
    };
    column.admit().map_err(|_| unrenderable())?;
    Ok(column)
}

/// One cell as PostgreSQL prints it; `None` is SQL NULL.
fn spell(
    table: &str,
    name: &str,
    column: &PgColumn,
    present: bool,
    value: u32,
    codes: Option<&dyn CodeText>,
    at: usize,
) -> Result<Option<String>, Refusal> {
    let mut text = String::new();
    match write_cell(column, present, value, codes, &mut text) {
        Ok(Written::Null) => Ok(None),
        Ok(Written::Text) => Ok(Some(text)),
        Err(refusal) => Err(Refusal::Bind(format!(
            "{table}.{name}: {refusal:?} at position {at}"
        ))),
    }
}

/// A text column's code table as the contract reads it.
struct BookText<'b, 'a>(&'b Codebook<'a>);

impl CodeText for BookText<'_, '_> {
    fn text(&self, code: u32) -> Option<&str> {
        self.0.text_ref(code)
    }
}

impl Catalog {
    /// The cell `table.column` holds at `position`, spelled through the shared
    /// contract from the table's own lane and code table, with no class binding
    /// and no frame check. `None` is SQL NULL.
    #[doc(hidden)]
    pub fn spell_cell(
        &self,
        table: &str,
        column: &str,
        position: usize,
    ) -> Result<Option<String>, Refusal> {
        let Some(index) = self.tables.iter().position(|t| t.name == table) else {
            return Err(Refusal::UnknownTable(table.to_owned()));
        };
        let held = self
            .tables
            .get(index)
            .ok_or_else(|| crate::malformed("table index"))?;
        let col = held
            .columns
            .iter()
            .find(|c| c.name == column)
            .ok_or_else(|| Refusal::Bind(format!("{table}.{column}: no such column")))?;
        let lane = col
            .lane
            .as_ref()
            .ok_or_else(|| Refusal::Bind(format!("{table}.{column}: no lane")))?;
        let pg = renderable(table, column, lane.kind, &col.sql_type)?;
        let book = (lane.kind == LaneKind::Text)
            .then(|| Codebook::of(self, index, column))
            .flatten();
        let codes = book.as_ref().map(BookText);
        let value = lane.values.get(position).copied().ok_or_else(|| {
            Refusal::Bind(format!("{table}.{column}: no value at position {position}"))
        })?;
        let present = lane
            .validity
            .get(position / 64)
            .is_some_and(|word| (word >> (position % 64)) & 1 == 1);
        spell(
            table,
            column,
            &pg,
            present,
            value,
            codes.as_ref().map(|c| c as &dyn CodeText),
            position,
        )
    }
}

fn stale(table: &str, reason: String) -> Refusal {
    Refusal::Stale {
        table: table.to_owned(),
        reason,
    }
}

fn refused(table: &str, e: LaneRefusal) -> Refusal {
    match e {
        LaneRefusal::Frame(reason) => stale(table, format!("{reason:?}")),
        other => Refusal::Bind(format!("{table}: {other:?}")),
    }
}

fn pg_refused(table: &str, e: PgRowRefusal) -> Refusal {
    match e {
        PgRowRefusal::Lane(e) => refused(table, e),
        PgRowRefusal::DeadPosition { at } => Refusal::DeadPosition {
            table: table.to_owned(),
            position: at,
        },
        PgRowRefusal::Cell { name, at, refusal } => {
            Refusal::Bind(format!("{table}.{name}: {refusal:?} at position {at}"))
        }
        other => Refusal::Bind(format!("{table}: {other:?}")),
    }
}

impl Catalog {
    /// Seals every table under its frame and binds the columns the class map
    /// resolves. A table without a class, or a column its class does not
    /// have, is not refused: it is read frame-checked only.
    pub fn bind(&self) -> Result<Bound, Refusal> {
        let mut tables = Vec::with_capacity(self.tables.len());
        for (index, (table, map)) in self.tables.iter().zip(&self.maps).enumerate() {
            let frames = map.frames();
            let class = match self
                .classes
                .as_ref()
                .and_then(|c| c.by_table.get(&table.name).map(|class| (c, class)))
            {
                Some((classes, class)) => Some(bind_class(self, index, frames, classes, class)?),
                None => None,
            };
            tables.push(BoundTable {
                name: table.name.clone(),
                frame: map.seal(),
                class,
            });
        }
        Ok(Bound { tables })
    }
}

/// Per slot (the k-th laned column): the class field the column resolves to,
/// each field to at most one column.
fn resolve_fields(basis: &OgarBasis, class: &str, table: &Table) -> Vec<Option<u8>> {
    let mut taken = BTreeSet::new();
    table
        .slots()
        .map(|slot| {
            let (position, _) = basis.resolve(class, &slot.column.name)?;
            let position = u8::try_from(position).ok()?;
            taken.insert(position).then_some(position)
        })
        .collect()
}

fn bind_class(
    catalog: &Catalog,
    index: usize,
    frames: &FrameStore,
    classes: &ClassMap,
    class: &str,
) -> Result<ClassBinding, Refusal> {
    let table = catalog
        .tables
        .get(index)
        .ok_or_else(|| crate::malformed("table index"))?;
    let basis = classes.engine.basis();
    let fields = resolve_fields(basis, class, table);
    let positions: Vec<u8> = fields.iter().flatten().copied().collect();
    let source =
        TableSource::new(catalog, index, frames, classes, class, &fields).ok_or_else(|| {
            Refusal::Bind(format!(
                "{}: class `{class}` is not in the basis",
                table.name
            ))
        })?;
    let binding = classes
        .engine
        .bind_lanes(class, &positions, frames, &source, space_of(table), SCOPE)
        .map_err(|e| refused(&table.name, e))?;
    Ok(ClassBinding {
        class: class.to_owned(),
        binding,
        fields,
    })
}

/// One bound column of a read, before the reader takes it in field order.
struct Wanted<'s> {
    field: u8,
    name: &'s str,
    column: PgColumn,
    book: Option<Codebook<'s>>,
}

/// A laned column's values and validity plane.
pub(crate) type Plane<'s> = (&'s [u32], &'s [u64]);

/// The only way the run path reads a table.
pub(crate) struct Session<'a> {
    catalog: &'a Catalog,
    bound: &'a Bound,
    /// Per catalog table: its entry in `bound`, `None` for a table declared after the bind.
    bound_at: Vec<Option<usize>>,
    sources: Vec<Option<TableSource<'a>>>,
}

impl<'a> Session<'a> {
    pub(crate) fn open(catalog: &'a Catalog, bound: &'a Bound) -> Session<'a> {
        let mut bound_at = Vec::with_capacity(catalog.tables.len());
        let mut sources = Vec::with_capacity(catalog.tables.len());
        for (index, (table, map)) in catalog.tables.iter().zip(&catalog.maps).enumerate() {
            let frames = map.frames();
            let at = bound.tables.iter().position(|b| b.name == table.name);
            let source = at
                .and_then(|b| bound.tables.get(b))
                .and_then(|bt| bt.class.as_ref())
                .and_then(|cb| {
                    let classes = catalog.classes.as_ref()?;
                    TableSource::new(catalog, index, frames, classes, &cb.class, &cb.fields)
                });
            bound_at.push(at);
            sources.push(source);
        }
        Session {
            catalog,
            bound,
            bound_at,
            sources,
        }
    }

    pub(crate) fn catalog(&self) -> &'a Catalog {
        self.catalog
    }

    fn bound_table(&self, index: usize) -> Option<&'a BoundTable> {
        let at = self.bound_at.get(index).copied().flatten()?;
        self.bound.tables.get(at)
    }

    /// Frame check: refused when the table was not bound or its frame is no longer current.
    pub(crate) fn table(&self, index: usize) -> Result<&'a Table, Refusal> {
        let table = self
            .catalog
            .tables
            .get(index)
            .ok_or_else(|| crate::malformed("table index"))?;
        let Some(bound) = self.bound_table(index) else {
            return Err(stale(&table.name, "NotBound".to_owned()));
        };
        let Some(frames) = self.catalog.maps.get(index).map(MapTable::frames) else {
            return Err(stale(&table.name, "NoFrameStore".to_owned()));
        };
        frames
            .check(&bound.frame)
            .map_err(|e: FrameRefusal| stale(&table.name, format!("{e:?}")))?;
        Ok(table)
    }

    /// [`Bound::read_selection`]: ONE lane check by reference
    /// (`Reader::open_selected`), then row `i` of the result is
    /// `lane[positions[i]]` of each bound column, read in place and spelled by
    /// the shared contract. Nothing is allocated but the readers' column
    /// table and the result rows; the positions are never sorted.
    pub(crate) fn selected(&self, index: usize, sel: u16) -> Result<SelectedRows, Refusal> {
        let table = self.table(index)?;
        self.catalog.census.read(index);
        let Some(class) = self.bound_table(index).and_then(|b| b.class.as_ref()) else {
            return Err(Refusal::Bind(format!("{}: no class binding", table.name)));
        };
        let (Some(source), Some(classes), Some(frames)) = (
            self.sources.get(index).and_then(Option::as_ref),
            self.catalog.classes.as_ref(),
            self.catalog.maps.get(index).map(MapTable::frames),
        ) else {
            return Err(Refusal::Bind(format!("{}: no class map", table.name)));
        };
        let mut wanted: Vec<Wanted<'_>> = Vec::new();
        for (k, slot) in table.slots().enumerate() {
            let Some(field) = class.fields.get(k).copied().flatten() else {
                continue;
            };
            let name = slot.column.name.as_str();
            let column = renderable(&table.name, name, slot.lane.kind, &slot.column.sql_type)?;
            let book = match slot.lane.kind {
                LaneKind::Text => {
                    Some(Codebook::of(self.catalog, index, name).ok_or_else(|| {
                        Refusal::Bind(format!("{}.{name}: no code table", table.name))
                    })?)
                }
                _ => None,
            };
            wanted.push(Wanted {
                field,
                name,
                column,
                book,
            });
        }
        // The reader yields the columns in the binding's field order; the
        // result is in table order.
        let mut order: Vec<usize> = (0..wanted.len()).collect();
        order.sort_by_key(|&at| wanted.get(at).map(|w| w.field));
        let books: Vec<Option<BookText<'_, '_>>> = order
            .iter()
            .map(|&at| {
                wanted
                    .get(at)
                    .and_then(|w| w.book.as_ref())
                    .map(BookText)
            })
            .collect();
        let specs: Vec<ColumnSpec<'_>> = order
            .iter()
            .zip(&books)
            .filter_map(|(&at, book)| {
                let w = wanted.get(at)?;
                Some(ColumnSpec {
                    name: w.name,
                    col: w.column,
                    codes: book.as_ref().map(|b| b as &dyn CodeText),
                })
            })
            .collect();
        let (store, _) = classes.engine.lane_check_parts();
        self.catalog.census.check(index);
        let (reader, positions) =
            Reader::open_selected(&class.binding, store, frames, source, sel, &specs)
                .map_err(|e| pg_refused(&table.name, e))?;
        for _ in positions {
            self.catalog.census.ordinal_checked(specs.len() as u64);
        }
        let local: Vec<u32>;
        let at: &[u32] = if cfg!(feature = "disable-vrow-local-index") {
            local = (0..positions.len())
                .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
                .collect();
            &local
        } else {
            positions
        };
        let read = reader
            .rows_at(at)
            .map_err(|e| pg_refused(&table.name, e))?;
        let mut inverse = vec![0usize; order.len()];
        for (place, &at) in order.iter().enumerate() {
            let Some(slot) = inverse.get_mut(at) else {
                return Err(crate::malformed("column order"));
            };
            *slot = place;
        }
        let unplaced = || crate::malformed("column placement");
        let types = inverse
            .iter()
            .map(|&place| read.types.get(place).cloned().ok_or_else(unplaced))
            .collect::<Result<Vec<_>, _>>()?;
        let rows = read
            .rows
            .into_iter()
            .map(|mut row| {
                inverse
                    .iter()
                    .map(|&place| row.get_mut(place).map(Option::take).ok_or_else(unplaced))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((types, rows))
    }

    /// Every laned column's `(values, validity)` in slot order. Slots bound to
    /// a class field come from the checked binding; the others are the
    /// table's own lanes, frame-checked only.
    pub(crate) fn lanes(&self, index: usize) -> Result<Vec<Plane<'_>>, Refusal> {
        let table = self.table(index)?;
        self.catalog.census.read(index);
        fn own(slot: crate::Slot<'_>) -> (&[u32], &[u64]) {
            (slot.lane.values.as_slice(), slot.lane.validity.as_slice())
        }
        let Some(class) = self.bound_table(index).and_then(|b| b.class.as_ref()) else {
            return Ok(table.slots().map(own).collect());
        };
        let (Some(source), Some(classes), Some(frames)) = (
            self.sources.get(index).and_then(Option::as_ref),
            self.catalog.classes.as_ref(),
            self.catalog.maps.get(index).map(MapTable::frames),
        ) else {
            return Err(Refusal::Bind(format!("{}: no class map", table.name)));
        };
        let (store, view) = classes.engine.lane_check_parts();
        self.catalog.census.check(index);
        #[cfg(feature = "disable-census-double-check")]
        {
            let _ = class
                .binding
                .borrow_all_present(store, &view, frames, source)
                .map_err(|e| refused(&table.name, e))?;
        }
        let present = class
            .binding
            .borrow_all_present(store, &view, frames, source)
            .map_err(|e| refused(&table.name, e))?;
        let mut by_field: Vec<(u8, &[u32], &[u64])> = Vec::with_capacity(present.len());
        for item in present {
            let (field, lane, plane) = item.map_err(|e| refused(&table.name, e))?;
            let LaneRef::U32(values) = lane else {
                return Err(Refusal::Bind(format!(
                    "{}: field {field} is not a u32 lane",
                    table.name
                )));
            };
            check_plane(field, plane, table.rows)
                .map_err(|e| Refusal::Bind(format!("{}: {e:?}", table.name)))?;
            by_field.push((field, values, plane));
        }
        table
            .slots()
            .enumerate()
            .map(|(k, slot)| match class.fields.get(k).copied().flatten() {
                None => Ok(own(slot)),
                Some(field) => by_field
                    .iter()
                    .find(|(f, _, _)| *f == field)
                    .map(|&(_, values, plane)| (values, plane))
                    .ok_or_else(|| {
                        Refusal::Bind(format!("{}: field {field} was not borrowed", table.name))
                    }),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timestamp_with_a_time_zone_is_refused() {
        let ts = LaneKind::Timestamp;
        assert!(renderable("t", "c", ts, "timestamp(3) without time zone").is_ok());
        assert!(renderable("t", "c", ts, "timestamp without time zone").is_ok());
        assert!(renderable("t", "c", LaneKind::Int, "integer").is_ok());
        for sql_type in ["timestamp with time zone", "timestamp(3) with time zone"] {
            assert_eq!(
                renderable("t", "c", ts, sql_type),
                Err(Refusal::Unrenderable {
                    table: "t".to_owned(),
                    column: "c".to_owned(),
                    sql_type: sql_type.to_owned(),
                })
            );
        }
    }

    #[test]
    fn an_integer_lane_is_signed_and_a_bigint_lane_unsigned() {
        let int = |sql_type| renderable("t", "c", LaneKind::Int, sql_type).map(|c| c.encoding);
        assert_eq!(int("integer"), Ok(LaneEncoding::Signed));
        assert_eq!(int("bigint"), Ok(LaneEncoding::Unsigned));
    }
}
