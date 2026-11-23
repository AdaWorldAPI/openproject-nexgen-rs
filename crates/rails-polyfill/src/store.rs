//! The persistence boundary.
//!
//! An in-memory, column-addressed row store with undo-log transactions
//! (rollback replays the writes since a savepoint, newest first). It stands in
//! for the database the way ActiveRecord sees one: auto-incrementing integer
//! primary keys, `find`, `find_by` over equality predicates, partial updates.
//! It is not a SQL engine and does not try to be; the query surface is exactly
//! what the lowered code calls.

use std::collections::BTreeMap;

use crate::record::Record;
use crate::schema::{Column, ColumnType, TableSpec};
use crate::value::Value;

#[derive(Debug, Clone)]
struct Table {
    spec: &'static TableSpec,
    rows: BTreeMap<i64, Vec<Value>>,
    next_id: i64,
}

/// The rows one table gained, changed or lost in a committed transaction:
/// final values only, in primary-key order.
#[derive(Debug, Clone)]
pub struct TableChange {
    pub spec: &'static TableSpec,
    pub upserts: Vec<Vec<Value>>,
    pub deletes: Vec<i64>,
}

/// What one outermost committed transaction did to the store, by table name.
#[derive(Debug, Clone, Default)]
pub struct Commit {
    pub tables: Vec<TableChange>,
}

#[derive(Debug, Clone, Default)]
pub struct Store {
    tables: BTreeMap<&'static str, Table>,
    writes: u64,
    /// Undo log of the open transaction(s): rollback replays it backwards.
    /// Cost is proportional to the rows the transaction touched, not to the
    /// size of the store.
    undo: Vec<Undo>,
    /// Open transactions; a write outside every transaction commits at once.
    open: u32,
    journaling: bool,
    commits: Vec<Commit>,
}

#[derive(Debug, Clone)]
enum Undo {
    Insert {
        table: &'static str,
        id: i64,
        next_id: i64,
    },
    Update {
        table: &'static str,
        id: i64,
        row: Vec<Value>,
    },
    Delete {
        table: &'static str,
        id: i64,
        row: Vec<Value>,
    },
}

/// A transaction savepoint: a position in the undo log.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot(usize);

#[derive(Debug, Clone, PartialEq)]
pub enum StoreError {
    UnknownTable(&'static str),
    NotNull {
        table: &'static str,
        column: &'static str,
    },
    MissingRow {
        table: &'static str,
        id: i64,
    },
}

impl Store {
    pub fn register(&mut self, spec: &'static TableSpec) {
        self.tables.entry(spec.name).or_insert(Table {
            spec,
            rows: BTreeMap::new(),
            next_id: 1,
        });
    }

    pub fn begin(&mut self) -> Snapshot {
        self.open += 1;
        Snapshot(self.undo.len())
    }

    /// Start or stop recording committed writes as [`Commit`]s. Off by default.
    pub fn journal_commits(&mut self, on: bool) {
        self.journaling = on;
    }

    /// The commits recorded since the last call, oldest first. A rolled back
    /// transaction contributes none.
    pub fn take_commits(&mut self) -> Vec<Commit> {
        std::mem::take(&mut self.commits)
    }

    /// Undo every write since `snap`, newest first.
    pub fn rollback(&mut self, snap: Snapshot) {
        while self.undo.len() > snap.0 {
            let Some(u) = self.undo.pop() else { break };
            match u {
                Undo::Insert { table, id, next_id } => {
                    if let Some(t) = self.tables.get_mut(table) {
                        t.rows.remove(&id);
                        t.next_id = next_id;
                    }
                }
                Undo::Update { table, id, row } | Undo::Delete { table, id, row } => {
                    if let Some(t) = self.tables.get_mut(table) {
                        t.rows.insert(id, row);
                    }
                }
            }
        }
        self.open = self.open.saturating_sub(1);
    }

    /// Commit at `snap`: the outermost commit journals the rows the undo log
    /// touched, then discards it.
    pub fn release(&mut self, _snap: Snapshot) {
        self.open = self.open.saturating_sub(1);
        if self.open == 0 {
            if self.journaling {
                let commit = self.commit_of_undo();
                self.record(commit);
            }
            self.undo.clear();
        }
    }

    fn record(&mut self, commit: Commit) {
        if cfg!(feature = "disable-write-journal") || commit.tables.is_empty() {
            return;
        }
        self.commits.push(commit);
    }

    /// The final state of every row the open transaction touched. A row that
    /// the transaction inserted and then deleted was never visible.
    fn commit_of_undo(&self) -> Commit {
        let mut touched: BTreeMap<&'static str, BTreeMap<i64, bool>> = BTreeMap::new();
        for u in &self.undo {
            let (table, id, inserted) = match u {
                Undo::Insert { table, id, .. } => (*table, *id, true),
                Undo::Update { table, id, .. } | Undo::Delete { table, id, .. } => {
                    (*table, *id, false)
                }
            };
            touched
                .entry(table)
                .or_default()
                .entry(id)
                .or_insert(inserted);
        }
        let mut commit = Commit::default();
        for (name, ids) in touched {
            let Some(table) = self.tables.get(name) else {
                continue;
            };
            let mut change = TableChange {
                spec: table.spec,
                upserts: Vec::new(),
                deletes: Vec::new(),
            };
            for (id, inserted) in ids {
                match table.rows.get(&id) {
                    Some(row) => change.upserts.push(row.clone()),
                    None if !inserted => change.deletes.push(id),
                    None => {}
                }
            }
            if !change.upserts.is_empty() || !change.deletes.is_empty() {
                commit.tables.push(change);
            }
        }
        commit
    }

    /// A write outside every transaction is its own commit; inside one it is
    /// undone with the rest, so it is logged.
    fn logged(&mut self, undo: Undo) {
        if self.open > 0 {
            self.undo.push(undo);
        } else if self.journaling {
            self.undo.push(undo);
            let commit = self.commit_of_undo();
            self.undo.clear();
            self.record(commit);
        }
    }

    /// The id the next insert without an explicit id receives.
    #[must_use]
    pub fn next_id(&self, spec: &'static TableSpec) -> Option<i64> {
        self.tables.get(spec.name).map(|t| t.next_id)
    }

    /// Moves a table's id allocator up to `next`. It never moves down, so a
    /// stored id is never handed out twice.
    pub fn reserve_ids(&mut self, spec: &'static TableSpec, next: i64) {
        if let Some(t) = self.tables.get_mut(spec.name) {
            t.next_id = t.next_id.max(next);
        }
    }

    /// Row writes since the store was created; a cheap cost counter.
    #[must_use]
    pub fn writes(&self) -> u64 {
        self.writes
    }

    fn table(&self, name: &'static str) -> Result<&Table, StoreError> {
        self.tables.get(name).ok_or(StoreError::UnknownTable(name))
    }

    fn table_mut(&mut self, name: &'static str) -> Result<&mut Table, StoreError> {
        self.tables
            .get_mut(name)
            .ok_or(StoreError::UnknownTable(name))
    }

    fn check_not_null(spec: &'static TableSpec, values: &[Value]) -> Result<(), StoreError> {
        for (c, col) in spec.columns.iter().enumerate() {
            if !col.null && col.name != "id" && values[c].is_nil() {
                return Err(StoreError::NotNull {
                    table: spec.name,
                    column: col.name,
                });
            }
        }
        Ok(())
    }

    /// Insert a seed row with an explicit id, bypassing every callback — the
    /// equivalent of `insert_all`.
    pub fn seed(
        &mut self,
        spec: &'static TableSpec,
        pairs: &[(&str, Value)],
    ) -> Result<i64, StoreError> {
        let mut rec = Record::new(spec);
        for (name, v) in pairs {
            rec.set_named(name, v.clone());
        }
        self.insert(&mut rec)
    }

    /// `INSERT`. Assigns the next id unless one is set, then marks the record
    /// persisted.
    pub fn insert(&mut self, rec: &mut Record) -> Result<i64, StoreError> {
        let spec = rec.table;
        let id_col = spec.index_of("id");
        let table = self.table_mut(spec.name)?;
        let id = match id_col.and_then(|c| rec.get(c).as_int()) {
            Some(id) => id,
            None => table.next_id,
        };
        if let Some(c) = id_col {
            rec.set(c, Value::Int(id));
        }
        Self::check_not_null(spec, rec.values())?;
        let next_id = table.next_id;
        table.next_id = table.next_id.max(id + 1);
        let normalized = normalize(spec, rec.values());
        // The record holds what was stored, so its persisted baseline and the
        // row agree (ActiveModel casts on assignment).
        adopt(rec, &normalized);
        table.rows.insert(id, normalized);
        self.logged(Undo::Insert {
            table: spec.name,
            id,
            next_id,
        });
        self.writes += 1;
        rec.mark_persisted();
        Ok(id)
    }

    /// Partial `UPDATE` — only changed columns are written; no change, no
    /// write, exactly like ActiveRecord's partial updates.
    pub fn update(&mut self, rec: &mut Record) -> Result<bool, StoreError> {
        let spec = rec.table;
        let id = rec.id().ok_or(StoreError::MissingRow {
            table: spec.name,
            id: 0,
        })?;
        // Cast first: a value that only differs below the column's precision
        // is no change, exactly as ActiveModel's cast-on-assignment sees it.
        let normalized = normalize(spec, rec.values());
        adopt(rec, &normalized);
        let changed = rec.changed_columns();
        if changed.is_empty() {
            rec.mark_persisted();
            return Ok(false);
        }
        Self::check_not_null(spec, rec.values())?;
        let table = self.table_mut(spec.name)?;
        let row = table.rows.get_mut(&id).ok_or(StoreError::MissingRow {
            table: spec.name,
            id,
        })?;
        let old = row.clone();
        for c in changed {
            row[c] = normalized[c].clone();
        }
        self.logged(Undo::Update {
            table: spec.name,
            id,
            row: old,
        });
        self.writes += 1;
        rec.mark_persisted();
        Ok(true)
    }

    pub fn delete(&mut self, spec: &'static TableSpec, id: i64) -> Result<(), StoreError> {
        if let Some(row) = self.table_mut(spec.name)?.rows.remove(&id) {
            self.logged(Undo::Delete {
                table: spec.name,
                id,
                row,
            });
        }
        self.writes += 1;
        Ok(())
    }

    /// `Model.find_by(id: x)`.
    pub fn find(&self, spec: &'static TableSpec, id: i64) -> Result<Option<Record>, StoreError> {
        Ok(self
            .table(spec.name)?
            .rows
            .get(&id)
            .map(|row| Record::loaded(spec, row.clone())))
    }

    /// `Model.where(a: x, b: y)` in primary-key order.
    pub fn where_eq(
        &self,
        spec: &'static TableSpec,
        preds: &[(usize, Value)],
    ) -> Result<Vec<Record>, StoreError> {
        let table = self.table(spec.name)?;
        // A bound value goes through the column's cast, as ActiveRecord binds it, so
        // a predicate finds the row written with the same value (`Int(3)` in a float
        // column is stored as `Float(3.0)`).
        let preds: Vec<(usize, Value)> = preds
            .iter()
            .map(|(c, v)| (*c, spec.columns.get(*c).map_or_else(|| v.clone(), |col| cast(col, v))))
            .collect();
        let preds = preds.as_slice();
        // A primary-key predicate is a key lookup, not a scan.
        if let Some(idc) = spec.index_of("id")
            && let Some((_, Value::Int(id))) = preds.iter().find(|(c, _)| *c == idc)
        {
            return Ok(table
                .rows
                .get(id)
                .filter(|row| preds.iter().all(|(c, v)| row_eq(&row[*c], v)))
                .map(|row| Record::loaded(spec, row.clone()))
                .into_iter()
                .collect());
        }
        Ok(table
            .rows
            .values()
            .filter(|row| preds.iter().all(|(c, v)| row_eq(&row[*c], v)))
            .map(|row| Record::loaded(spec, row.clone()))
            .collect())
    }

    /// `Model.find_by(a: x)` — the first match in primary-key order, which is
    /// what both PostgreSQL (for these single-table scans) and sqlite return
    /// for an unordered `LIMIT 1` over a freshly inserted table.
    pub fn find_by(
        &self,
        spec: &'static TableSpec,
        preds: &[(usize, Value)],
    ) -> Result<Option<Record>, StoreError> {
        Ok(self.where_eq(spec, preds)?.into_iter().next())
    }

    #[must_use]
    pub fn count(&self, spec: &'static TableSpec) -> usize {
        self.tables.get(spec.name).map_or(0, |t| t.rows.len())
    }
}

/// SQL equality as ActiveRecord binds it: a boolean column compared with
/// `true` matches stored `true`; `nil` matches `IS NULL`.
fn row_eq(stored: &Value, wanted: &Value) -> bool {
    stored == wanted
}

/// What the column type does to a value on the way into the database: times
/// are cut to the column's precision (`datetime(3)` unless it says otherwise),
/// strings stay strings, an Integer in a `float` column becomes a Float and a
/// Time in a `date` column the day it falls on.
fn normalize(spec: &'static TableSpec, values: &[Value]) -> Vec<Value> {
    spec.columns.iter().zip(values).map(|(col, v)| cast(col, v)).collect()
}

/// Put the cast values back into the record, column by column, only where
/// the cast changed them.
fn adopt(rec: &mut Record, normalized: &[Value]) {
    for (c, v) in normalized.iter().enumerate() {
        if rec.get(c) != v {
            rec.set(c, v.clone());
        }
    }
}

/// One value through a column's cast; shared by writes and bound predicates.
fn cast(col: &Column, v: &Value) -> Value {
    match (col.ty, v) {
        // A `datetime(p)` column keeps `p` fractional digits, truncated.
        (ColumnType::DateTime, Value::Time(us)) => Value::Time(col.truncate_time(*us)),
        (ColumnType::Boolean, Value::Int(i)) => Value::Bool(*i != 0),
        // `float` casts an Integer; `NaN` and the infinities stay as they
        // are (PostgreSQL `double precision` stores them).
        #[expect(clippy::cast_precision_loss, reason = "ActiveRecord casts Integer to Float the same way")]
        (ColumnType::Float, Value::Int(i)) => Value::Float(*i as f64),
        // `date` casts a Time to the UTC calendar day it falls on.
        (ColumnType::Date, Value::Time(us)) => {
            i32::try_from(us.div_euclid(86_400_000_000)).map_or_else(|_| v.clone(), Value::Date)
        }
        _ => v.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Column, ColumnType, Default};

    static T: TableSpec = TableSpec {
        name: "t",
        columns: &[
            Column {
                name: "id",
                ty: ColumnType::Integer,
                null: false,
                default: Default::None,
                limit: None,
            },
            Column {
                name: "x",
                ty: ColumnType::Integer,
                null: true,
                default: Default::None,
                limit: None,
            },
        ],
    };

    static MS: TableSpec = TableSpec {
        name: "ms",
        columns: &[
            Column {
                name: "id",
                ty: ColumnType::Integer,
                null: false,
                default: Default::None,
                limit: None,
            },
            Column {
                name: "at",
                ty: ColumnType::DateTime,
                null: true,
                default: Default::None,
                limit: Some(3),
            },
        ],
    };

    #[test]
    fn a_saved_record_holds_the_time_its_column_stored() {
        const RAW: i64 = 1_767_323_045_123_456;
        const STORED: i64 = 1_767_323_045_123_000;
        let mut s = Store::default();
        s.register(&MS);
        let mut r = Record::new(&MS);
        r.set(1, Value::Time(RAW));
        let id = s.insert(&mut r).unwrap();
        assert_eq!(r.get(1), &Value::Time(STORED), "insert: record equals the row");
        assert_eq!(s.find(&MS, id).unwrap().unwrap().get(1), &Value::Time(STORED));
        // Assigning the stored value back is no change and no write.
        r.set(1, Value::Time(STORED));
        assert!(r.changed_columns().is_empty());
        // A value that differs only below the precision is no change either.
        let writes = s.writes;
        r.set(1, Value::Time(STORED + 999));
        assert!(!s.update(&mut r).unwrap(), "sub-precision change writes nothing");
        assert_eq!(s.writes, writes);
        assert_eq!(r.get(1), &Value::Time(STORED), "update: record equals the row");
    }

    #[test]
    fn rollback_undoes_inserts_and_updates_and_restores_the_id_sequence() {
        let mut s = Store::default();
        s.register(&T);
        let id1 = s.seed(&T, &[("x", Value::Int(1))]).unwrap();
        let snap = s.begin();
        let mut r = s.find(&T, id1).unwrap().unwrap();
        r.set(1, Value::Int(2));
        s.update(&mut r).unwrap();
        s.seed(&T, &[("x", Value::Int(3))]).unwrap();
        s.rollback(snap);
        assert_eq!(s.count(&T), 1);
        assert_eq!(s.find(&T, id1).unwrap().unwrap().get(1), &Value::Int(1));
        assert_eq!(
            s.seed(&T, &[]).unwrap(),
            2,
            "the rolled-back id is reused, as with a fresh sequence"
        );
    }

    static D: TableSpec = TableSpec {
        name: "d",
        columns: &[
            Column {
                name: "id",
                ty: ColumnType::Integer,
                null: false,
                default: Default::None,
                limit: None,
            },
            Column {
                name: "due",
                ty: ColumnType::Date,
                null: false,
                default: Default::None,
                limit: None,
            },
            Column {
                name: "ratio",
                ty: ColumnType::Float,
                null: true,
                default: Default::None,
                limit: None,
            },
        ],
    };

    fn dated() -> Store {
        let mut s = Store::default();
        s.register(&D);
        s
    }

    #[test]
    fn a_date_and_a_float_are_stored_and_read_back() {
        let mut s = dated();
        // 2000-02-29 is day 11016; 1969-12-31 is day -1.
        let a = s.seed(&D, &[("due", Value::Date(11_016)), ("ratio", Value::Float(0.1))]).unwrap();
        let b = s.seed(&D, &[("due", Value::Date(-1)), ("ratio", Value::Float(-2.5e-300))]).unwrap();
        let ra = s.find(&D, a).unwrap().unwrap();
        assert_eq!((ra.get(1), ra.get(2)), (&Value::Date(11_016), &Value::Float(0.1)));
        let rb = s.find(&D, b).unwrap().unwrap();
        assert_eq!((rb.get(1), rb.get(2)), (&Value::Date(-1), &Value::Float(-2.5e-300)));
    }

    #[test]
    fn nan_and_the_infinities_are_stored_not_dropped() {
        let mut s = dated();
        for f in [f64::INFINITY, f64::NEG_INFINITY, -0.0] {
            let id = s.seed(&D, &[("due", Value::Date(0)), ("ratio", Value::Float(f))]).unwrap();
            let got = s.find(&D, id).unwrap().unwrap();
            assert_eq!(got.get(2), &Value::Float(f));
        }
        let id = s.seed(&D, &[("due", Value::Date(0)), ("ratio", Value::Float(f64::NAN))]).unwrap();
        let got = s.find(&D, id).unwrap().unwrap();
        assert!(got.get(2).as_float().is_some_and(f64::is_nan));
        let neg_zero = s.find(&D, id - 1).unwrap().unwrap();
        assert!(neg_zero.get(2).as_float().is_some_and(f64::is_sign_negative), "-0.0 keeps its sign");
    }

    #[test]
    fn a_nil_date_or_float_is_stored_only_where_the_column_is_nullable() {
        let mut s = dated();
        assert_eq!(
            s.seed(&D, &[("ratio", Value::Float(1.0))]),
            Err(StoreError::NotNull { table: "d", column: "due" })
        );
        let id = s.seed(&D, &[("due", Value::Date(5))]).unwrap();
        assert_eq!(s.find(&D, id).unwrap().unwrap().get(2), &Value::Nil, "ratio is nullable");
    }

    #[test]
    fn normalize_casts_an_integer_to_a_float_and_a_time_to_its_day() {
        let mut s = dated();
        // 1970-01-02T23:59:59.999Z and 1969-12-31T00:00:00.000Z
        let a = s.seed(&D, &[("due", Value::time_ms(172_799_999)), ("ratio", Value::Int(3))]).unwrap();
        let b = s.seed(&D, &[("due", Value::time_ms(-86_400_000))]).unwrap();
        let ra = s.find(&D, a).unwrap().unwrap();
        assert_eq!((ra.get(1), ra.get(2)), (&Value::Date(1), &Value::Float(3.0)));
        assert_eq!(s.find(&D, b).unwrap().unwrap().get(1), &Value::Date(-1));
    }

    #[test]
    fn a_predicate_goes_through_the_same_cast_as_the_write() {
        let mut s = dated();
        let id = s.seed(&D, &[("due", Value::time_ms(86_400_000)), ("ratio", Value::Int(3))]).unwrap();
        // Queried with the very values it was written with.
        let by_ratio = s.where_eq(&D, &[(2, Value::Int(3))]).unwrap();
        assert_eq!(by_ratio.len(), 1);
        assert_eq!(by_ratio[0].get(0), &Value::Int(id));
        assert!(s.find_by(&D, &[(1, Value::time_ms(86_400_000))]).unwrap().is_some());
        // And a value the cast does not turn into the stored one still misses.
        assert!(s.where_eq(&D, &[(2, Value::Int(4))]).unwrap().is_empty());
        assert!(s.find_by(&D, &[(1, Value::time_ms(2 * 86_400_000))]).unwrap().is_none());
    }

    static U: TableSpec = TableSpec {
        name: "u",
        columns: &[
            Column {
                name: "id",
                ty: ColumnType::Integer,
                null: false,
                default: Default::None,
                limit: None,
            },
            Column {
                name: "us",
                ty: ColumnType::DateTime,
                null: true,
                default: Default::None,
                limit: Some(6),
            },
            Column {
                name: "ms",
                ty: ColumnType::DateTime,
                null: true,
                default: Default::None,
                limit: None,
            },
        ],
    };

    #[test]
    fn a_datetime_keeps_its_columns_precision_and_a_predicate_is_cut_the_same_way() {
        let mut s = Store::default();
        s.register(&U);
        // 2026-01-02T03:04:05.123456Z
        let t = Value::Time(1_767_323_045_123_456);
        let id = s.seed(&U, &[("us", t.clone()), ("ms", t.clone())]).unwrap();
        let row = s.find(&U, id).unwrap().unwrap();
        let expect_us = if cfg!(feature = "disable-usec-precision") { 1_767_323_045_123_000 } else { 1_767_323_045_123_456 };
        assert_eq!(row.get(1), &Value::Time(expect_us), "limit 6");
        assert_eq!(row.get(2), &Value::Time(1_767_323_045_123_000), "default is milliseconds");
        assert!(s.find_by(&U, &[(1, t.clone())]).unwrap().is_some());
        assert!(s.find_by(&U, &[(2, t)]).unwrap().is_some(), "bound like the write");
    }
}
