//! Capture mode of the `EXECUTED_DB` journey: what the journey did and what PostgreSQL answered,
//! recorded as `oracle/executed-db.pg.ndjson`.
//!
//! The journey asserts rows against a live PostgreSQL and recorded nothing. This module records
//! it, so that the SQLx/PostgreSQL side can be the oracle another storage path is checked
//! against (docs/decisions/0002-polyfill-quack-storage.md). It only observes: the journey's
//! assertions are unchanged, and the recording never decides anything.
//!
//! # File format
//!
//! One JSON object per line. Line 1 is the header (format version and the normalization below).
//! Each further line is one journey step:
//!
//! ```text
//! {"step":2,"name":"...","calls":[{"call":"mutation::create","inputs":{..},"outcome":{..}}],
//!  "reads":[{"read":"find_wp","inputs":{..},"answer":{..}}],
//!  "feed":{"tables":{"work_packages":{"columns":[{"name","type","null"}],"upsert":[[..]],"delete":[[..]]}}}}
//! ```
//!
//! * `calls`: every write the step made, in order, with its inputs and outcome (`{"ok": ..}` or
//!   `{"err": ..}`). `mutation::create`, `mutation::update`, the store entry points the journey
//!   calls directly, the guarded form paths (`update_core`, `edit_form_core`: status code only,
//!   the CSRF token is random) and the scope calls of the two concurrency scenes are calls.
//!   Fixture statements issued as plain SQL are `fixture_sql` calls.
//! * `reads`: the answers of the reads the step made through the production read path
//!   (`find_wp`), with the timestamps left out (they are in the rows).
//! * `feed`: the committed rows, in the shape of the change feed (`columns` with the SQL type
//!   PostgreSQL reports, `upsert`, `delete`), read back with SQL after the step, on the
//!   journey's verification pool. A table appears only if the step changed it. `upsert` holds the
//!   full row image of every row the step inserted or changed, ordered by primary key (by every
//!   column for a table without one); `delete` holds the primary key cells of every row it
//!   removed. A table's state after step N is the replay of its feeds up to N. Every cell is the
//!   PostgreSQL text of the value (`col::text`, session time zone UTC) or `null`.
//!
//! # Normalization (declared in the header, applied to nothing else)
//!
//! Timestamps come from `now()` and differ between runs. Every instant in a `timestamp` /
//! `timestamptz` / `tstzrange` cell is replaced by `ts#NNNN`, the rank of that instant among all
//! the distinct instants of the whole capture (equal instants share a rank). Equalities the
//! journey asserts (`journals.created_at = work_packages.updated_at`) and their order survive;
//! distances between instants do not. Nothing else is normalized: ids come from sequences and
//! are deterministic because the journey is sequential, which the compare check proves on every
//! run, and no value is ever edited silently.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

use axum::response::Response;
use op_services::work_packages::mutation::{
    self, Caller, CreationStore, MutationError, MutationStore, PgMutationStore,
};
use op_services::work_packages::WorkPackageParams;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

use crate::write_guard::{PgWpStore, WpStore, CSRF_FIELD};

/// Version of the file format, bumped when a line's shape changes.
pub const FORMAT: u32 = 1;

type Cells = Vec<Option<String>>;

struct TableDelta {
    name: String,
    columns: Vec<(String, String, bool)>,
    upsert: Vec<Cells>,
    delete: Vec<Cells>,
}

struct Step {
    n: usize,
    name: String,
    calls: Vec<Value>,
    reads: Vec<Value>,
    tables: Vec<TableDelta>,
}

struct State {
    calls: Vec<Value>,
    reads: Vec<Value>,
    annotation: Option<Value>,
    /// The rows of every table after the previous step: (identity, cells), in query order.
    prev: BTreeMap<String, Vec<(String, Cells)>>,
    steps: Vec<Step>,
}

static STATE: Mutex<State> = Mutex::new(State {
    calls: Vec::new(),
    reads: Vec::new(),
    annotation: None,
    prev: BTreeMap::new(),
    steps: Vec::new(),
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

/// Starts a fresh capture.
pub fn begin() {
    let mut s = state();
    s.calls.clear();
    s.reads.clear();
    s.annotation = None;
    s.prev.clear();
    s.steps.clear();
}

/// Records a write the current step made.
pub fn call(kind: &str, inputs: Value, outcome: Value) {
    state()
        .calls
        .push(json!({"call": kind, "inputs": inputs, "outcome": outcome}));
}

/// Records a fixture statement issued as plain SQL.
pub fn fixture(label: &str, inputs: Value) {
    call(
        "fixture_sql",
        json!({"label": label, "args": inputs}),
        json!("ok"),
    );
}

/// Records a read the current step made.
pub fn read(kind: &str, inputs: Value, answer: Value) {
    state()
        .reads
        .push(json!({"read": kind, "inputs": inputs, "answer": answer}));
}

/// Attaches a note to the call being made (taken by the next wrapper that finishes).
pub fn annotate(note: Value) {
    state().annotation = Some(note);
}

fn take_annotation() -> Value {
    state().annotation.take().unwrap_or(Value::Null)
}

fn wp_json(row: &op_db::work_packages::WorkPackageRow) -> Value {
    json!({
        "id": row.id,
        "subject": row.subject,
        "description": row.description,
        "project_id": row.project_id,
        "type_id": row.type_id,
        "status_id": row.status_id,
        "priority_id": row.priority_id,
        "author_id": row.author_id,
        "assigned_to_id": row.assigned_to_id,
        "responsible_id": row.responsible_id,
        "start_date": row.start_date.map(|d| d.to_string()),
        "due_date": row.due_date.map(|d| d.to_string()),
        "estimated_hours": row.estimated_hours,
        "done_ratio": row.done_ratio,
        "parent_id": row.parent_id,
        "version_id": row.version_id,
        "category_id": row.category_id,
        "lock_version": row.lock_version,
    })
}

/// A mutation error as stable JSON (`ValidationErrors` is a `HashMap`, whose `Debug` order is
/// not stable).
fn err_json(e: &MutationError) -> Value {
    match e {
        MutationError::Invalid(v) => {
            let sorted: BTreeMap<_, _> = v.errors.iter().collect();
            json!({"kind": "Invalid", "errors": sorted, "base_errors": v.base_errors})
        }
        other => {
            let text = format!("{other:?}");
            let kind = text.split(['(', ' ']).next().unwrap_or("").to_string();
            json!({"kind": kind, "detail": text})
        }
    }
}

fn outcome(r: &Result<op_db::work_packages::WorkPackageRow, MutationError>) -> Value {
    match r {
        Ok(row) => json!({"ok": wp_json(row)}),
        Err(e) => json!({"err": err_json(e)}),
    }
}

// ── instrumented entry points ────────────────────────────────────────

/// `mutation::create`, recorded.
pub async fn create<S: CreationStore>(
    store: &S,
    caller: &Caller,
    params: WorkPackageParams,
) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
    let inputs = json!({"caller": format!("{caller:?}"), "params": format!("{params:?}")});
    let r = mutation::create(store, caller, params).await;
    let mut inputs = inputs;
    let note = take_annotation();
    if !note.is_null() {
        inputs["store"] = note;
    }
    call("mutation::create", inputs, outcome(&r));
    r
}

/// `mutation::update`, recorded.
pub async fn update<S: MutationStore>(
    store: &S,
    caller: &Caller,
    id: i64,
    dto: op_db::UpdateWorkPackageDto,
) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
    let inputs = json!({"caller": format!("{caller:?}"), "id": id, "dto": format!("{dto:?}")});
    let r = mutation::update(store, caller, id, dto).await;
    call("mutation::update", inputs, outcome(&r));
    r
}

/// `edit_form_core`, recorded (status code only).
pub async fn edit_form_core<S: WpStore>(
    store: &S,
    id: i64,
    ctx: &super::super::WriteCtx<'_>,
) -> Response {
    let who = format!("{:?}", ctx.who);
    let resp = super::super::edit_form_core(store, id, ctx).await;
    call(
        "edit_form_core",
        json!({"id": id, "who": who}),
        json!({"status": resp.status().as_u16()}),
    );
    resp
}

/// `update_core`, recorded: the form without its CSRF token (random), and the status code.
pub async fn update_core<S: WpStore>(
    store: &S,
    id: i64,
    ctx: &super::super::WriteCtx<'_>,
    form: &HashMap<String, String>,
) -> Response {
    let who = format!("{:?}", ctx.who);
    let fields: BTreeMap<_, _> = form.iter().filter(|(k, _)| *k != CSRF_FIELD).collect();
    let token = match form.get(CSRF_FIELD) {
        Some(_) => "present",
        None => "absent",
    };
    let resp = super::super::update_core(store, id, ctx, form).await;
    call(
        "update_core",
        json!({"id": id, "who": who, "form": fields, "csrf_token": token}),
        json!({"status": resp.status().as_u16()}),
    );
    resp
}

/// The read the page and the API make, recorded.
#[allow(async_fn_in_trait)]
pub trait RecFind {
    async fn rec_find_wp(
        &self,
        id: i64,
    ) -> Result<Option<op_db::work_packages::WorkPackageRow>, String>;
}

fn record_find(id: i64, found: &Option<op_db::work_packages::WorkPackageRow>) {
    read(
        "find_wp",
        json!({"id": id}),
        match found {
            Some(row) => wp_json(row),
            None => Value::Null,
        },
    );
}

impl RecFind for PgWpStore {
    async fn rec_find_wp(
        &self,
        id: i64,
    ) -> Result<Option<op_db::work_packages::WorkPackageRow>, String> {
        let r = WpStore::find_wp(self, id)
            .await
            .map_err(|e| format!("{e:?}"))?;
        record_find(id, &r);
        Ok(r)
    }
}

impl RecFind for PgMutationStore {
    async fn rec_find_wp(
        &self,
        id: i64,
    ) -> Result<Option<op_db::work_packages::WorkPackageRow>, String> {
        let r = MutationStore::find_wp(self, id)
            .await
            .map_err(|e| format!("{e:?}"))?;
        record_find(id, &r);
        Ok(r)
    }
}

/// The store's own write entry points, recorded.
#[allow(async_fn_in_trait)]
pub trait RecWrite {
    async fn rec_update_unit_of_work(
        &self,
        caller: &Caller,
        id: i64,
        dto: op_db::UpdateWorkPackageDto,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError>;
    async fn rec_update_then(
        &self,
        caller: &Caller,
        id: i64,
        dto: op_db::UpdateWorkPackageDto,
        post_update: Option<&str>,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError>;
}

impl RecWrite for PgMutationStore {
    async fn rec_update_unit_of_work(
        &self,
        caller: &Caller,
        id: i64,
        dto: op_db::UpdateWorkPackageDto,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
        let inputs = json!({"caller": format!("{caller:?}"), "id": id, "dto": format!("{dto:?}")});
        let r = MutationStore::update_unit_of_work(self, caller, id, dto).await;
        call("update_unit_of_work", inputs, outcome(&r));
        r
    }

    async fn rec_update_then(
        &self,
        caller: &Caller,
        id: i64,
        dto: op_db::UpdateWorkPackageDto,
        post_update: Option<&str>,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
        let inputs = json!({
            "caller": format!("{caller:?}"), "id": id, "dto": format!("{dto:?}"),
            "post_update": post_update,
        });
        let r = self.update_then(caller, id, dto, post_update).await;
        call("update_then", inputs, outcome(&r));
        r
    }
}

// ── the committed rows ───────────────────────────────────────────────

fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// Reads every table of the `public` schema (but sqlx's own bookkeeping) as text cells and
/// records the step: its calls, its reads, and what it changed since the previous step.
pub async fn end_step(verify: &PgPool, n: usize, name: &str) {
    let mut conn = verify.acquire().await.expect("capture connection");
    sqlx::query("SET TIME ZONE 'UTC'")
        .execute(&mut *conn)
        .await
        .expect("capture time zone");
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
           AND table_name <> '_sqlx_migrations' ORDER BY table_name",
    )
    .fetch_all(&mut *conn)
    .await
    .expect("capture tables");

    let mut deltas = Vec::new();
    let mut now: BTreeMap<String, Vec<(String, Cells)>> = BTreeMap::new();
    for table in tables {
        let columns: Vec<(String, String, bool)> = sqlx::query_as(
            "SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), NOT a.attnotnull \
             FROM pg_attribute a \
             WHERE a.attrelid = ('public.' || quote_ident($1))::regclass \
               AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
        )
        .bind(&table)
        .fetch_all(&mut *conn)
        .await
        .expect("capture columns");
        let pk: Vec<String> = sqlx::query_scalar(
            "SELECT a.attname::text FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY (i.indkey) \
             WHERE i.indrelid = ('public.' || quote_ident($1))::regclass AND i.indisprimary \
             ORDER BY a.attnum",
        )
        .bind(&table)
        .fetch_all(&mut *conn)
        .await
        .expect("capture primary key");
        let select = columns
            .iter()
            .map(|(c, _, _)| format!("{}::text", quote(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let order = if pk.is_empty() {
            (1..=columns.len())
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            pk.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ")
        };
        let sql = format!(
            "SELECT {select} FROM public.{} ORDER BY {order}",
            quote(&table)
        );
        let raw = sqlx::query(&sql)
            .fetch_all(&mut *conn)
            .await
            .expect("capture rows");
        let pk_idx: Vec<usize> = if pk.is_empty() {
            (0..columns.len()).collect()
        } else {
            pk.iter()
                .map(|p| {
                    columns
                        .iter()
                        .position(|(c, _, _)| c == p)
                        .expect("pk column")
                })
                .collect()
        };
        let rows: Vec<(String, Cells)> = raw
            .iter()
            .map(|r| {
                let cells: Cells = (0..columns.len())
                    .map(|i| r.try_get::<Option<String>, _>(i).expect("text cell"))
                    .collect();
                let id = pk_idx
                    .iter()
                    .map(|&i| cells[i].clone().unwrap_or_else(|| "\u{0}NULL".into()))
                    .collect::<Vec<_>>()
                    .join("\u{1f}");
                (id, cells)
            })
            .collect();

        let prev_rows = state().prev.get(&table).cloned().unwrap_or_default();
        let (upsert, delete) = delta(&prev_rows, &rows, &pk_idx);
        if !upsert.is_empty() || !delete.is_empty() {
            deltas.push(TableDelta {
                name: table.clone(),
                columns,
                upsert,
                delete,
            });
        }
        now.insert(table, rows);
    }

    let mut s = state();
    let calls = std::mem::take(&mut s.calls);
    let reads = std::mem::take(&mut s.reads);
    s.annotation = None;
    s.prev = now;
    s.steps.push(Step {
        n,
        name: name.to_string(),
        calls,
        reads,
        tables: deltas,
    });
}

/// The rows of `now` that are new or changed against `prev` (full images, in `now`'s order), and
/// the primary key cells of the rows of `prev` that are gone.
fn delta(
    prev: &[(String, Cells)],
    now: &[(String, Cells)],
    pk_idx: &[usize],
) -> (Vec<Cells>, Vec<Cells>) {
    let before: HashMap<&str, &Cells> = prev.iter().map(|(k, c)| (k.as_str(), c)).collect();
    let after: BTreeSet<&str> = now.iter().map(|(k, _)| k.as_str()).collect();
    let upsert = now
        .iter()
        .filter(|(k, c)| before.get(k.as_str()).is_none_or(|old| *old != c))
        .map(|(_, c)| c.clone())
        .collect();
    let delete = prev
        .iter()
        .filter(|(k, _)| !after.contains(k.as_str()))
        .map(|(_, c)| pk_idx.iter().map(|&i| c[i].clone()).collect())
        .collect();
    (upsert, delete)
}

// ── normalization and the file ───────────────────────────────────────

/// The instants (`YYYY-MM-DD HH:MM:SS[.ffffff][+TZ]`) in a cell, with their byte ranges.
fn instants(cell: &str) -> Vec<(usize, usize)> {
    let b = cell.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let mut out = Vec::new();
    let mut i = 0;
    while i + 19 <= b.len() {
        let shape = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&o| digit(i + o))
            && b[i + 4] == b'-'
            && b[i + 7] == b'-'
            && b[i + 10] == b' '
            && b[i + 13] == b':'
            && b[i + 16] == b':';
        if !shape {
            i += 1;
            continue;
        }
        let mut end = i + 19;
        if b.get(end) == Some(&b'.') {
            end += 1;
            while digit(end) {
                end += 1;
            }
        }
        if matches!(b.get(end), Some(b'+' | b'-')) && digit(end + 1) && digit(end + 2) {
            end += 3;
            if b.get(end) == Some(&b':') && digit(end + 1) && digit(end + 2) {
                end += 3;
            }
        }
        out.push((i, end));
        i = end;
    }
    out
}

/// Sort key of an instant text: date, time, fraction padded to microseconds, offset.
fn instant_key(text: &str) -> String {
    let (head, rest) = text.split_at(19);
    let (frac, zone) = match rest.strip_prefix('.') {
        Some(r) => {
            let n = r.find(['+', '-']).unwrap_or(r.len());
            (&r[..n], &r[n..])
        }
        None => ("", rest),
    };
    format!("{head}.{frac:0<6}{zone}")
}

fn is_time_type(sql_type: &str) -> bool {
    sql_type.starts_with("timestamp") || sql_type == "tstzrange" || sql_type == "tsrange"
}

/// The whole capture as the text of the file.
pub fn finish() -> String {
    let s = state();
    // Pass 1: every distinct instant of every time cell, ranked.
    let mut keys: BTreeSet<String> = BTreeSet::new();
    let time_cols = |t: &TableDelta| -> Vec<bool> {
        t.columns
            .iter()
            .map(|(_, ty, _)| is_time_type(ty))
            .collect()
    };
    for step in &s.steps {
        for t in &step.tables {
            let tc = time_cols(t);
            for row in t.upsert.iter().chain(t.delete.iter()) {
                for (i, cell) in row.iter().enumerate() {
                    if let (Some(text), true) = (cell, tc.get(i).copied().unwrap_or(false)) {
                        for (a, z) in instants(text) {
                            keys.insert(instant_key(&text[a..z]));
                        }
                    }
                }
            }
        }
    }
    let rank: HashMap<&String, usize> = keys.iter().enumerate().map(|(i, k)| (k, i + 1)).collect();
    let normalize = |text: &str| -> String {
        let mut out = String::new();
        let mut at = 0;
        for (a, z) in instants(text) {
            out.push_str(&text[at..a]);
            let r = rank[&instant_key(&text[a..z])];
            out.push_str(&format!("ts#{r:04}"));
            at = z;
        }
        out.push_str(&text[at..]);
        out
    };

    let mut lines = Vec::new();
    lines.push(
        json!({
            "capture": "executed-db",
            "format": FORMAT,
            "source": "crates/op-server/src/board/executed_db.rs",
            "cells": "PostgreSQL text (col::text, session time zone UTC); null is SQL NULL",
            "normalization": {
                "timestamp": "every instant in a timestamp/timestamptz/tstzrange cell is replaced \
                              by ts#NNNN, its rank among the distinct instants of this file \
                              (equal instants share a rank)",
                "nothing_else": "ids, counters and every other value are recorded as PostgreSQL returned them",
            },
        })
        .to_string(),
    );
    for step in &s.steps {
        let mut tables = serde_json::Map::new();
        for t in &step.tables {
            let tc = time_cols(t);
            let cells = |rows: &Vec<Cells>| -> Vec<Value> {
                rows.iter()
                    .map(|row| {
                        Value::Array(
                            row.iter()
                                .enumerate()
                                .map(|(i, c)| match c {
                                    None => Value::Null,
                                    Some(text) if tc.get(i).copied().unwrap_or(false) => {
                                        Value::String(normalize(text))
                                    }
                                    Some(text) => Value::String(text.clone()),
                                })
                                .collect(),
                        )
                    })
                    .collect()
            };
            tables.insert(
                t.name.clone(),
                json!({
                    "columns": t.columns.iter()
                        .map(|(n, ty, nullable)| json!({"name": n, "type": ty, "null": nullable}))
                        .collect::<Vec<_>>(),
                    "upsert": cells(&t.upsert),
                    "delete": cells(&t.delete),
                }),
            );
        }
        lines.push(
            json!({
                "step": step.n,
                "name": step.name,
                "calls": step.calls,
                "reads": step.reads,
                "feed": {"tables": tables},
            })
            .to_string(),
        );
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

// ── compare ──────────────────────────────────────────────────────────

fn first_diff(path: &str, a: &Value, b: &Value) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                (Some(p), Some(q)) => first_diff(&format!("{path}.{k}"), p, q),
                (p, q) => Some(format!("{path}.{k}: committed {p:?}, rerun {q:?}")),
            })
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                if let Some(d) = first_diff(&format!("{path}[{i}]"), p, q) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| {
                format!(
                    "{path}: committed has {} entries, rerun has {}",
                    x.len(),
                    y.len()
                )
            })
        }
        _ => (a != b).then(|| format!("{path}: committed {a}, rerun {b}")),
    }
}

/// `Ok` when the rerun is byte-identical to the committed text; otherwise the first difference,
/// located by step and path.
pub fn compare(committed: &str, rerun: &str) -> Result<(), String> {
    if committed == rerun {
        return Ok(());
    }
    let (c, r): (Vec<&str>, Vec<&str>) = (committed.lines().collect(), rerun.lines().collect());
    for (i, (x, y)) in c.iter().zip(&r).enumerate() {
        if x != y {
            let (xv, yv): (Value, Value) = (
                serde_json::from_str(x).unwrap_or(Value::String((*x).into())),
                serde_json::from_str(y).unwrap_or(Value::String((*y).into())),
            );
            let step = yv
                .get("step")
                .map_or_else(|| "header".into(), Value::to_string);
            return Err(format!(
                "line {} (step {step}) differs: {}",
                i + 1,
                first_diff("$", &xv, &yv).unwrap_or_else(|| "text differs".into())
            ));
        }
    }
    Err(format!(
        "the committed capture has {} lines, the rerun {}",
        c.len(),
        r.len()
    ))
}

const COMMITTED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../oracle/executed-db.pg.ndjson"
);

/// What the journey does with the capture when it ends:
///
/// * `EXECUTED_DB_CAPTURE=<path>` writes it there;
/// * `UPDATE_EXECUTED_DB_CAPTURE=1` rewrites the committed `oracle/executed-db.pg.ndjson`;
/// * otherwise, with `REQUIRE_DB=1`, it must equal the committed file
///   (`EXECUTED_DB_CAPTURE_EXPECTED=<path>` names another file to compare with); a missing
///   committed file is a failure, never a pass.
///
/// Returns a one-line summary for the journey's final report.
pub fn conclude() -> String {
    let text = finish();
    let lines = text.lines().count();
    let mut said = Vec::new();
    if let Ok(path) = std::env::var("EXECUTED_DB_CAPTURE") {
        if !path.trim().is_empty() {
            std::fs::write(&path, &text).expect("write EXECUTED_DB_CAPTURE");
            said.push(format!("written to {path}"));
        }
    }
    if std::env::var("UPDATE_EXECUTED_DB_CAPTURE").as_deref() == Ok("1") {
        std::fs::write(COMMITTED, &text).expect("rewrite the committed capture");
        said.push("committed file rewritten".to_string());
    } else if super::required() {
        let expected = std::env::var("EXECUTED_DB_CAPTURE_EXPECTED")
            .ok()
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| COMMITTED.to_string());
        let committed = std::fs::read_to_string(&expected).unwrap_or_else(|e| {
            panic!(
                "REQUIRE_DB=1 but the committed capture {expected} cannot be read ({e}); \
                 generate it with UPDATE_EXECUTED_DB_CAPTURE=1"
            )
        });
        if let Err(diff) = compare(&committed, &text) {
            panic!(
                "the EXECUTED_DB capture differs from {expected}: {diff}\n\
                 If PostgreSQL's behaviour changed on purpose, regenerate with \
                 UPDATE_EXECUTED_DB_CAPTURE=1 and review the diff."
            );
        }
        said.push(format!("equals {expected}"));
    } else {
        said.push("compare skipped (REQUIRE_DB is not 1)".to_string());
    }
    format!("capture {lines} lines, {}", said.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instants_are_found_in_timestamps_and_ranges() {
        let r = "[\"2026-03-02 10:00:00.5+00\",\"2026-03-02 10:05:00+00\")";
        let found: Vec<&str> = instants(r).into_iter().map(|(a, z)| &r[a..z]).collect();
        assert_eq!(
            found,
            ["2026-03-02 10:00:00.5+00", "2026-03-02 10:05:00+00"]
        );
        assert!(
            instants("2026-03-02").is_empty(),
            "a date alone is not an instant"
        );
        assert!(instants("12:00:00").is_empty());
    }

    #[test]
    fn instant_keys_order_by_time_not_by_text_length() {
        assert!(
            instant_key("2026-03-02 10:00:00+00") < instant_key("2026-03-02 10:00:00.000001+00")
        );
        assert!(
            instant_key("2026-03-02 10:00:00.12+00") < instant_key("2026-03-02 10:00:00.123+00")
        );
        assert!(instant_key("2026-03-02 10:00:00.5+00") > instant_key("2026-03-02 10:00:00.49+00"));
    }

    #[test]
    fn delta_reports_changed_and_removed_rows_only() {
        let row = |id: &str, v: &str| (id.to_string(), vec![Some(id.to_string()), Some(v.into())]);
        let prev = vec![row("1", "a"), row("2", "b"), row("3", "c")];
        let now = vec![row("1", "a"), row("2", "B"), row("4", "d")];
        let (up, del) = delta(&prev, &now, &[0]);
        assert_eq!(
            up,
            vec![
                vec![Some("2".to_string()), Some("B".to_string())],
                vec![Some("4".to_string()), Some("d".to_string())],
            ]
        );
        assert_eq!(del, vec![vec![Some("3".to_string())]]);
    }

    #[test]
    fn compare_names_the_first_changed_cell() {
        let committed =
            "{\"a\":1}\n{\"step\":2,\"feed\":{\"tables\":{\"t\":{\"upsert\":[[\"1\",\"x\"]]}}}}\n";
        assert!(compare(committed, committed).is_ok());
        let rerun = committed.replace("\"x\"", "\"y\"");
        let err = compare(committed, &rerun).unwrap_err();
        assert!(
            err.contains("step 2") && err.contains("upsert[0][1]"),
            "{err}"
        );
        let shorter = "{\"a\":1}\n";
        assert!(
            compare(committed, shorter).is_err(),
            "a missing step is a difference"
        );
    }
}
