//! The desk: the polyfill store (write side) and a Quack catalog (read side)
//! over the same rows, as zammad-desk holds them (`zammad-desk/src/lib.rs`,
//! `Desk`, `deliver`, `read`, `read_all`).
//!
//! Every committed write of the store is journaled as a [`Commit`], rendered
//! by [`polyfill_feed::feed_json`] and applied to the catalog. Display reads
//! are `quack_shadow::rows` calls over the catalog; PostgreSQL is not involved.
//!
//! A seed document has the shape
//!
//! ```json
//! {"seed_time": <ms since the epoch>, "seed": {"<table>": [{"<column>": <cell>, …}, …]}}
//! ```
//!
//! with cells as in [`crate::snapshot`]. Every slice table is fed to the
//! catalog, a table without seed rows as an empty one, so reading it answers
//! no rows instead of being refused as unknown.

use rails_polyfill::{Commit, Store, TableChange, TableSpec, Value};
use serde_json::{json, Value as J};

use crate::generated::MODELS;
use crate::snapshot::cell_value;

/// Why a desk could not be built or could not answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeskError {
    /// The seed document is not shaped as described in the module docs.
    Seed(String),
    /// A commit could not be rendered as a feed or applied to the catalog.
    Feed(String),
    /// The catalog refused a read; the refusal is named.
    Refused(String),
}

impl std::fmt::Display for DeskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeskError::Seed(e) => write!(f, "seed: {e}"),
            DeskError::Feed(e) => write!(f, "feed: {e}"),
            DeskError::Refused(e) => write!(f, "refused: {e}"),
        }
    }
}

impl std::error::Error for DeskError {}

/// The rows a read answered: column names and, per row, one cell per column.
#[derive(Debug, Clone, PartialEq)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<J>>,
}

impl Rows {
    /// The cell of `row` in column `column`, or `None` when the read has no
    /// such column.
    #[must_use]
    pub fn cell<'a>(&self, row: &'a [J], column: &str) -> Option<&'a J> {
        self.columns
            .iter()
            .position(|c| c == column)
            .and_then(|at| row.get(at))
    }
}

/// The store and the catalog over the same rows.
pub struct Desk {
    store: Store,
    catalog: quack_shadow::Catalog,
    /// Committed writes the catalog has not taken yet, oldest first.
    unfed: Vec<Commit>,
    seed_time: i64,
}

/// The spec of a slice table by name.
#[must_use]
pub fn table(name: &str) -> Option<&'static TableSpec> {
    MODELS.iter().map(|m| m.table).find(|t| t.name == name)
}

impl Desk {
    /// A desk over the rows of a seed document. A table or column the slice
    /// does not have is refused.
    pub fn from_seed(doc: &J) -> Result<Desk, DeskError> {
        let seed = doc
            .get("seed")
            .and_then(J::as_object)
            .ok_or_else(|| DeskError::Seed("no seed object".into()))?;
        if let Some(unknown) = seed.keys().find(|k| table(k).is_none()) {
            return Err(DeskError::Seed(format!(
                "table {unknown} is not in the slice"
            )));
        }
        let mut store = Store::default();
        store.journal_commits(true);
        for m in MODELS {
            store.register(m.table);
        }
        for (name, rows) in seed {
            let spec = table(name).expect("checked above");
            let rows = rows
                .as_array()
                .ok_or_else(|| DeskError::Seed(format!("{name} is not a list of rows")))?;
            for row in rows {
                let obj = row
                    .as_object()
                    .ok_or_else(|| DeskError::Seed(format!("a row of {name} is not an object")))?;
                let mut pairs: Vec<(&str, Value)> = Vec::with_capacity(obj.len());
                for (column, cell) in obj {
                    let at = spec
                        .index_of(column)
                        .ok_or_else(|| DeskError::Seed(format!("{name}.{column} has no column")))?;
                    let value = cell_value(cell, spec.columns[at].ty)
                        .map_err(|e| DeskError::Seed(format!("{name}.{column}: {e}")))?;
                    pairs.push((column.as_str(), value));
                }
                store
                    .seed(spec, &pairs)
                    .map_err(|e| DeskError::Seed(format!("{name}: {e:?}")))?;
            }
        }
        let mut unfed = store.take_commits();
        // Every slice table reaches the catalog, an empty one as an empty change.
        let fed: Vec<&str> = unfed
            .iter()
            .flat_map(|c| c.tables.iter().map(|t| t.spec.name))
            .collect();
        let empty: Vec<TableChange> = MODELS
            .iter()
            .filter(|m| !fed.contains(&m.table.name))
            .map(|m| TableChange {
                spec: m.table,
                upserts: Vec::new(),
                deletes: Vec::new(),
            })
            .collect();
        if !empty.is_empty() {
            unfed.push(Commit { tables: empty });
        }
        let mut desk = Desk {
            store,
            catalog: quack_shadow::Catalog::new(),
            unfed,
            seed_time: doc.get("seed_time").and_then(J::as_i64).unwrap_or_default(),
        };
        desk.deliver()?;
        Ok(desk)
    }

    /// Hand every unfed commit to the catalog, oldest first. A commit that
    /// fails stays queued with everything after it.
    fn deliver(&mut self) -> Result<(), DeskError> {
        let mut done = 0;
        let outcome = self.unfed.iter().try_for_each(|commit| {
            let feed =
                polyfill_feed::feed_json(commit).map_err(|e| DeskError::Feed(format!("{e:?}")))?;
            self.catalog
                .apply_feed(&feed)
                .map_err(|e| DeskError::Feed(e.to_string()))?;
            done += 1;
            Ok(())
        });
        self.unfed.drain(..done);
        outcome
    }

    /// How many committed writes the catalog has not taken yet.
    #[must_use]
    pub fn unfed(&self) -> usize {
        self.unfed.len()
    }

    /// The seed clock, milliseconds since the epoch.
    #[must_use]
    pub fn seed_time(&self) -> i64 {
        self.seed_time
    }

    /// The write-side store (consistency reads inside a write go here, as in
    /// zammad-desk).
    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    fn rows_call(&self, call: &J) -> Result<Rows, DeskError> {
        let answer = quack_shadow::rows(&self.catalog, call, false)
            .map_err(|r| DeskError::Refused(format!("{r:?}")))?;
        Ok(Rows {
            columns: answer.columns,
            rows: answer.rows,
        })
    }

    /// Every row of `table`, by id.
    pub fn read_all(&self, table: &str) -> Result<Rows, DeskError> {
        self.rows_call(&json!({
            "table": table, "terminal": "rows", "where": null,
            "select": "*",
            "order": [{"table": table, "col": "id", "dir": "asc"}],
            "limit": null, "offset": null,
            "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        }))
    }

    /// The rows of `table` whose `column` equals `value`, by id.
    pub fn read(&self, table: &str, column: &str, value: J) -> Result<Rows, DeskError> {
        self.rows_call(&json!({
            "table": table, "terminal": "rows",
            "where": {"table": table, "col": column, "op": "eq", "value": value},
            "select": "*",
            "order": [{"table": table, "col": "id", "dir": "asc"}],
            "limit": null, "offset": null,
            "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        }))
    }

    /// How many rows `table` holds, answered by the catalog.
    pub fn count(&self, table: &str) -> Result<i64, DeskError> {
        let call = json!({
            "table": table, "terminal": "count", "where": null,
            "select": null, "order": [], "limit": null, "offset": null,
            "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        });
        match quack_shadow::answer(&self.catalog, &call, false) {
            Ok(quack_shadow::Answer::Count(n)) => i64::try_from(n)
                .map_err(|_| DeskError::Refused(format!("{table}: count out of range"))),
            Ok(other) => Err(DeskError::Refused(format!(
                "{table}: {other:?} is not a count"
            ))),
            Err(r) => Err(DeskError::Refused(format!("{r:?}"))),
        }
    }
}
