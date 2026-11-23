//! The reads the desk answers, named once so the Quack side and the
//! PostgreSQL oracle answer exactly the same list.
//!
//! A read is one of three shapes, each ordered by `id` where it returns rows:
//! every row of a table, the rows whose column equals a value, and a table's
//! row count. They are the shapes `Desk::read_all`, `Desk::read` and
//! `Desk::count` send to Quack.

use serde_json::{json, Value as J};

use crate::desk::{Desk, DeskError, Rows};

/// One read.
#[derive(Debug, Clone, PartialEq)]
pub enum Read {
    /// Every row of the table, by id.
    All(&'static str),
    /// The rows whose column equals the value, by id.
    Eq(&'static str, &'static str, J),
    /// The table's row count.
    Count(&'static str),
}

/// What a read answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Rows(Rows),
    Count(i64),
}

impl Read {
    /// The read as a JSON object (the oracle file's `read` key).
    #[must_use]
    pub fn to_json(&self) -> J {
        match self {
            Read::All(t) => json!({"shape": "all", "table": t}),
            Read::Eq(t, c, v) => json!({"shape": "eq", "table": t, "col": c, "value": v}),
            Read::Count(t) => json!({"shape": "count", "table": t}),
        }
    }

    /// The table read.
    #[must_use]
    pub fn table(&self) -> &'static str {
        match self {
            Read::All(t) | Read::Eq(t, _, _) | Read::Count(t) => t,
        }
    }

    /// Answer the read through the desk's Quack catalog.
    pub fn on_desk(&self, desk: &Desk) -> Result<Answer, DeskError> {
        Ok(match self {
            Read::All(t) => Answer::Rows(desk.read_all(t)?),
            Read::Eq(t, c, v) => Answer::Rows(desk.read(t, c, v.clone())?),
            Read::Count(t) => Answer::Count(desk.count(t)?),
        })
    }
}

impl Answer {
    /// The answer as a JSON object (the oracle file's `answer` key).
    #[must_use]
    pub fn to_json(&self) -> J {
        match self {
            Answer::Rows(r) => json!({"columns": r.columns, "rows": r.rows}),
            Answer::Count(n) => json!({"count": n}),
        }
    }
}

/// The reads checked against PostgreSQL for the demo seed: every slice table in
/// full and counted, and equality filters on integer, text and boolean
/// columns the pages use, including one that matches nothing, a case
/// difference, and a null (`IS NULL`). Filters on a DateTime column are not
/// in the list: a read value is microseconds in the snapshot convention and
/// epoch seconds on Quack's timestamp lane, and the encoding is not settled.
#[must_use]
pub fn demo_reads() -> Vec<Read> {
    let mut reads = Vec::new();
    let mut tables: Vec<&'static str> = crate::generated::MODELS
        .iter()
        .map(|m| m.table.name)
        .collect();
    tables.sort_unstable();
    for t in &tables {
        reads.push(Read::All(t));
        reads.push(Read::Count(t));
    }
    for p in 1..=3 {
        reads.push(Read::Eq("work_packages", "project_id", json!(p)));
    }
    for s in 1..=5 {
        reads.push(Read::Eq("work_packages", "status_id", json!(s)));
    }
    reads.extend([
        Read::Eq("work_packages", "id", json!(999)),
        Read::Eq("work_packages", "author_id", json!(2)),
        Read::Eq("relations", "from_id", json!(2)),
        Read::Eq("journals", "journable_id", json!(1)),
        Read::Eq("time_entries", "project_id", json!(1)),
        Read::Eq("time_entries", "user_id", json!(2)),
        Read::Eq("members", "project_id", json!(1)),
        Read::Eq("enumerations", "type", json!("IssuePriority")),
        Read::Eq("enumerations", "type", json!("TimeEntryActivity")),
        Read::Eq("statuses", "is_default", json!(true)),
        Read::Eq("projects", "identifier", json!("website-relaunch")),
        Read::Eq("users", "login", json!("alice")),
        // Text equality is case-sensitive on both sides.
        Read::Eq("users", "login", json!("Alice")),
        // `where(assigned_to_id: nil)`: IS NULL, not `= NULL`.
        Read::Eq("work_packages", "assigned_to_id", J::Null),
    ]);
    reads
}
