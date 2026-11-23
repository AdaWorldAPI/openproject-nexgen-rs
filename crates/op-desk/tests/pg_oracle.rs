//! PostgreSQL is the oracle for the desk's reads.
//!
//! With `OP_TEST_DATABASE_URL` set, this test creates a database, builds the
//! slice tables from OpenProject's own `db/structure.sql`
//! (`oracle/openproject-structure.sql`), inserts the desk's rows (every column,
//! as the polyfill store holds them, so PostgreSQL's column defaults cannot
//! make the two sides differ), answers every read of
//! `op_desk::reads::demo_reads` with SQL, and compares the answers with the
//! committed `oracle/desk-reads.pg.json`. `UPDATE_DESK_READS_CAPTURE=1`
//! rewrites the file instead; review its diff. Without the URL the test skips,
//! unless `REQUIRE_DB=1` (CI), which makes it fail.
//!
//! Each SQL cell is read as the snapshot cell of its column type, turned into
//! the polyfill value and rendered with `polyfill_feed::cell`, the renderer
//! whose output the Quack catalog holds. So the comparison checks what each
//! side selects, filters, orders and counts, not two renderers against each
//! other. `tests/desk_oracle.rs` checks the desk against the same file without
//! a database.

use op_desk::desk::{table, Desk};
use op_desk::reads::{demo_reads, Read};
use op_desk::snapshot::{cell_value, snapshot};
use rails_polyfill::{ColumnType, TableSpec};
use serde_json::{json, Value as J};
use sqlx::{PgPool, Row};

mod common;

const URL_VAR: &str = "OP_TEST_DATABASE_URL";

fn required() -> bool {
    std::env::var("REQUIRE_DB").as_deref() == Ok("1")
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// The SQL type without its length or scale (`character varying(30)` →
/// `character varying`): an explicit cast to `varchar(n)` truncates silently,
/// where an assignment to the column would refuse.
fn base_type(sql_type: &str) -> &str {
    sql_type.split('(').next().unwrap_or(sql_type).trim()
}

/// A SQL literal for a snapshot cell of `column`, cast to its base SQL type.
fn literal(cell: &J, ty: ColumnType, sql_type: &str) -> String {
    let sql_type = base_type(sql_type);
    let lit = match (ty, cell) {
        (_, J::Null) => return "NULL".into(),
        (ColumnType::DateTime, J::Number(n)) => {
            let us = n.as_i64().expect("microseconds");
            return format!(
                "(TIMESTAMPTZ '1970-01-01 00:00:00+00' + {us} * INTERVAL '1 microsecond')::{sql_type}"
            );
        }
        (_, J::Bool(b)) => b.to_string(),
        (_, J::Number(n)) => n.to_string(),
        (_, J::String(s)) => quote(s),
        (_, other) => panic!("unexpected cell {other}"),
    };
    format!("CAST({lit} AS {sql_type})")
}

/// The SQL expression that yields a column's snapshot cell as JSON.
fn cell_expr(name: &str, ty: ColumnType) -> String {
    let c = ident(name);
    match ty {
        ColumnType::DateTime => {
            format!("to_jsonb(round(extract(epoch FROM {c}) * 1000000)::bigint)")
        }
        ColumnType::Date => format!("to_jsonb(to_char({c}, 'YYYY-MM-DD'))"),
        ColumnType::Json => format!("to_jsonb({c}::text)"),
        ColumnType::Float | ColumnType::Decimal => format!("to_jsonb({c}::float8)"),
        _ => format!("to_jsonb({c})"),
    }
}

fn select_list(spec: &TableSpec) -> String {
    let parts: Vec<String> = spec
        .columns
        .iter()
        .map(|c| cell_expr(c.name, c.ty))
        .collect();
    format!("jsonb_build_array({})", parts.join(", "))
}

async fn answer(pool: &PgPool, read: &Read) -> J {
    let t = read.table();
    let spec = table(t).expect("spec");
    let rows_of = |sql: String| async move {
        let rows = sqlx::query(&sql)
            .fetch_all(pool)
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
        let rows: Vec<J> = rows
            .iter()
            .map(|r| {
                let cells: J = r.get(0);
                let cells = cells.as_array().expect("array").clone();
                J::Array(
                    spec.columns
                        .iter()
                        .zip(cells)
                        .map(|(col, cell)| {
                            let v = cell_value(&cell, col.ty)
                                .unwrap_or_else(|e| panic!("{t}.{}: {e}", col.name));
                            polyfill_feed::cell(&v, col)
                                .unwrap_or_else(|e| panic!("{t}.{}: {e:?}", col.name))
                        })
                        .collect(),
                )
            })
            .collect();
        let columns: Vec<&str> = spec.columns.iter().map(|c| c.name).collect();
        json!({"columns": columns, "rows": rows})
    };
    match read {
        Read::All(_) => {
            rows_of(format!(
                "SELECT {} FROM {} ORDER BY id",
                select_list(spec),
                ident(t)
            ))
            .await
        }
        Read::Eq(_, col, value) => {
            let ty = spec.columns[spec.index_of(col).expect("column")].ty;
            let sql_type = sql_types(t)
                .into_iter()
                .find(|(n, _)| n == col)
                .map(|(_, s)| s)
                .expect("sql type");
            // Rails' `where(col: nil)` is `IS NULL`, and Quack reads an eq with
            // a null value the same way.
            let predicate = if value.is_null() {
                format!("{} IS NULL", ident(col))
            } else {
                format!("{} = {}", ident(col), literal(value, ty, &sql_type))
            };
            rows_of(format!(
                "SELECT {} FROM {} WHERE {predicate} ORDER BY id",
                select_list(spec),
                ident(t),
            ))
            .await
        }
        Read::Count(_) => {
            let n: i64 = sqlx::query(&format!("SELECT count(*) FROM {}", ident(t)))
                .fetch_one(pool)
                .await
                .expect("count")
                .get(0);
            json!({"count": n})
        }
    }
}

fn sql_types(t: &str) -> Vec<(String, String)> {
    common::structure()[t]
        .columns
        .iter()
        .map(|c| (c.name.clone(), c.sql_type.clone()))
        .collect()
}

#[tokio::test]
async fn postgresql_answers_the_desk_reads_as_recorded() {
    let Ok(url) = std::env::var(URL_VAR) else {
        assert!(!required(), "REQUIRE_DB=1 but {URL_VAR} is not set");
        eprintln!("PG oracle: SKIPPED ({URL_VAR} is not set; set REQUIRE_DB=1 to fail)");
        return;
    };
    let doc: J = serde_json::from_str(include_str!("../seed/demo.json")).expect("demo.json");
    let desk = Desk::from_seed(&doc).expect("desk");
    let specs: Vec<&'static TableSpec> =
        op_desk::generated::MODELS.iter().map(|m| m.table).collect();
    let snap = snapshot(desk.store(), &specs);

    let admin = PgPool::connect(&url)
        .await
        .expect("connect to OP_TEST_DATABASE_URL");
    let db_name = format!("op_desk_oracle_{}", std::process::id());
    sqlx::query(&format!("CREATE DATABASE {db_name}"))
        .execute(&admin)
        .await
        .expect("CREATE DATABASE (the role needs CREATEDB)");
    let db_url = match url.rsplit_once('/') {
        Some((base, _)) => format!("{base}/{db_name}"),
        None => panic!("{URL_VAR} has no database path"),
    };
    let pool = PgPool::connect(&db_url).await.expect("oracle database");
    let structure = common::structure();
    for spec in &specs {
        let t = &structure[spec.name];
        let cols: Vec<String> = t
            .columns
            .iter()
            .map(|c| {
                format!(
                    "{} {}{}",
                    ident(&c.name),
                    c.sql_type,
                    if c.not_null { " NOT NULL" } else { "" }
                )
            })
            .collect();
        sqlx::query(&format!(
            "CREATE TABLE {} ({})",
            ident(spec.name),
            cols.join(", ")
        ))
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("create {}: {e}", spec.name));
        let types = sql_types(spec.name);
        for row in snap["tables"][spec.name]["rows"].as_array().expect("rows") {
            let names: Vec<String> = spec.columns.iter().map(|c| ident(c.name)).collect();
            let values: Vec<String> = spec
                .columns
                .iter()
                .map(|c| {
                    let sql_type = &types.iter().find(|(n, _)| n == c.name).expect("sql type").1;
                    literal(&row[c.name], c.ty, sql_type)
                })
                .collect();
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({})",
                ident(spec.name),
                names.join(", "),
                values.join(", ")
            );
            sqlx::query(&sql)
                .execute(&pool)
                .await
                .unwrap_or_else(|e| panic!("{sql}: {e}"));
        }
    }

    let mut got = Vec::new();
    for read in demo_reads() {
        got.push(json!({"read": read.to_json(), "answer": answer(&pool, &read).await}));
    }
    pool.close().await;
    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name} WITH (FORCE)"))
        .execute(&admin)
        .await;

    let path = common::repo_root().join("oracle/desk-reads.pg.json");
    let text = serde_json::to_string_pretty(&J::Array(got)).expect("json") + "\n";
    if std::env::var("UPDATE_DESK_READS_CAPTURE").as_deref() == Ok("1") {
        std::fs::write(&path, &text).expect("write oracle/desk-reads.pg.json");
        eprintln!("PG oracle: wrote {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).expect("oracle/desk-reads.pg.json");
    assert!(
        committed == text,
        "PostgreSQL's answers differ from oracle/desk-reads.pg.json; rerun with UPDATE_DESK_READS_CAPTURE=1 and review the diff"
    );
}
