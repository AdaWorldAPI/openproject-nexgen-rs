//! A reader for the `CREATE TABLE` statements of a PostgreSQL schema dump.
//!
//! OpenProject keeps its schema as SQL: `config.active_record.schema_format = :sql`
//! (config/application.rb:190), so `db/structure.sql` is the schema a fresh install
//! has, after every migration. There is no `db/schema.rb`. This module reads the
//! column lists out of that file, and out of nexgen's own migrations
//! (`crates/op-db/migrations/*.sql`, `CREATE TABLE` plus `ALTER TABLE … ADD COLUMN`),
//! so the two can be compared.
//!
//! It reads exactly the statement shapes those two sources use. It is not a SQL
//! parser: constraints, indexes and every other statement are skipped.

use std::collections::BTreeMap;

/// One column as the dump declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlColumn {
    pub name: String,
    /// The type as written, lower case, with `serial`/`varchar`/`timestamptz`
    /// spelled the way PostgreSQL dumps them (see [`canonical_type`]).
    pub sql_type: String,
    pub not_null: bool,
    /// The `DEFAULT` expression as written, if any.
    pub default: Option<String>,
    /// 1-based line in the file.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlTable {
    pub name: String,
    pub line: usize,
    pub columns: Vec<SqlColumn>,
}

/// Table name → table, for every `CREATE TABLE` in `sql`, with every
/// `ALTER TABLE <t> ADD COLUMN` applied in file order.
#[must_use]
pub fn parse(sql: &str) -> BTreeMap<String, SqlTable> {
    let mut tables = BTreeMap::new();
    add(&mut tables, sql);
    tables
}

/// Reads one more file into `tables` (the migrations are applied in order).
pub fn add(tables: &mut BTreeMap<String, SqlTable>, sql: &str) {
    let lines: Vec<&str> = sql.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = strip_comment(lines[i]).trim();
        if let Some(rest) = line.strip_prefix("CREATE TABLE ") {
            let name = table_ident(rest.trim_end_matches('(').trim());
            let mut columns = Vec::new();
            let start = i + 1;
            i += 1;
            while i < lines.len() {
                let body = strip_comment(lines[i]).trim();
                if body.starts_with(')') {
                    break;
                }
                if let Some(c) = column(body, i + 1) {
                    columns.push(c);
                }
                i += 1;
            }
            tables.insert(
                name.clone(),
                SqlTable {
                    name,
                    line: start,
                    columns,
                },
            );
        } else if let Some(rest) = line.strip_prefix("ALTER TABLE ") {
            let (t, tail) = rest.split_once(' ').unwrap_or((rest, ""));
            let t = table_ident(t);
            let tail = tail.trim();
            if let Some(def) = tail.strip_prefix("ADD COLUMN ") {
                let def = def
                    .trim_start_matches("IF NOT EXISTS ")
                    .trim_end_matches(';');
                if let (Some(c), Some(table)) = (column(def, i + 1), tables.get_mut(&t)) {
                    if !table.columns.iter().any(|x| x.name == c.name) {
                        table.columns.push(c);
                    }
                }
            }
        }
        i += 1;
    }
}

fn strip_comment(l: &str) -> &str {
    l.split_once("--").map_or(l, |(a, _)| a)
}

/// `public.work_packages` / `IF NOT EXISTS users` → the bare table name.
fn table_ident(s: &str) -> String {
    let s = s.trim_start_matches("IF NOT EXISTS ").trim();
    let s = s.strip_prefix("public.").unwrap_or(s);
    s.split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches('"')
        .to_string()
}

const NOT_COLUMN: &[&str] = &[
    "CONSTRAINT",
    "UNIQUE",
    "PRIMARY",
    "CHECK",
    "FOREIGN",
    "EXCLUDE",
];

/// One column definition line (`name type [DEFAULT expr] [NOT NULL] [PRIMARY KEY],`).
fn column(def: &str, line: usize) -> Option<SqlColumn> {
    let def = def.trim().trim_end_matches(',').trim();
    let first = def.split_whitespace().next()?;
    if def.is_empty() || NOT_COLUMN.contains(&first) {
        return None;
    }
    let (name, rest) = if let Some(r) = def.strip_prefix('"') {
        let (n, rest) = r.split_once('"')?;
        (n.to_string(), rest.trim())
    } else {
        let (n, rest) = def.split_once(char::is_whitespace)?;
        (n.to_string(), rest.trim())
    };
    let upper = rest.to_ascii_uppercase();
    // The type runs to the first keyword.
    let type_end = [
        " DEFAULT ",
        " NOT NULL",
        " NULL",
        " PRIMARY KEY",
        " REFERENCES ",
        " UNIQUE",
        " CHECK ",
    ]
    .iter()
    .filter_map(|k| upper.find(k))
    .min()
    .unwrap_or(rest.len());
    let sql_type = canonical_type(&rest[..type_end]);
    let tail = &rest[type_end..];
    let tail_up = &upper[type_end..];
    let primary = tail_up.contains("PRIMARY KEY");
    let not_null = tail_up.contains("NOT NULL") || primary;
    let default = tail_up.find(" DEFAULT ").map(|p| {
        let expr = &tail[p + " DEFAULT ".len()..];
        let expr_up = &tail_up[p + " DEFAULT ".len()..];
        let end = [
            " NOT NULL",
            " NULL",
            " PRIMARY KEY",
            " REFERENCES ",
            " UNIQUE",
            " CHECK ",
        ]
        .iter()
        .filter_map(|k| expr_up.find(k))
        .min()
        .unwrap_or(expr.len());
        expr[..end].trim().to_string()
    });
    Some(SqlColumn {
        name,
        sql_type,
        not_null,
        default,
        line,
    })
}

/// The type the way `pg_dump` spells it: `bigserial` → `bigint`, `varchar` →
/// `character varying`, `timestamptz` → `timestamp with time zone`.
#[must_use]
pub fn canonical_type(t: &str) -> String {
    let t = t.trim().to_ascii_lowercase();
    match t.as_str() {
        "bigserial" | "serial8" | "int8" => "bigint".into(),
        "serial" | "serial4" | "int" | "int4" => "integer".into(),
        "smallserial" | "int2" => "smallint".into(),
        "varchar" => "character varying".into(),
        "timestamptz" => "timestamp with time zone".into(),
        "float8" => "double precision".into(),
        "float4" => "real".into(),
        "bool" => "boolean".into(),
        _ => t
            .strip_prefix("varchar(")
            .map_or(t.clone(), |n| format!("character varying({n}")),
    }
}

/// A `timestamp` declared without `(n)` keeps microseconds in PostgreSQL.
pub const POSTGRESQL_TIMESTAMP_PRECISION: u32 = 6;

/// The Rails column type ActiveRecord's PostgreSQL adapter reports for a SQL
/// type: the vocabulary `rails-compiler`'s schema arm uses (`string`, `text`,
/// `integer`, `bigint`, `boolean`, `datetime`, `date`, `float`, `decimal`,
/// `json`, `jsonb`, `binary`). A type outside it is returned as written
/// (`tstzrange`), and `rails-compiler` stores it as text.
#[must_use]
pub fn rails_type(sql_type: &str) -> (String, Option<u32>) {
    let t = sql_type.trim();
    if let Some(n) = t
        .strip_prefix("character varying(")
        .and_then(|r| r.strip_suffix(')'))
    {
        return ("string".into(), n.parse().ok());
    }
    // A timestamp keeps PostgreSQL's precision: the `(n)` it was declared with,
    // else 6. ActiveRecord takes a timestamp's precision only from the SQL type
    // (`register_class_with_precision`, `extract_precision`); with none it is
    // nil and `apply_seconds_precision` keeps every digit PostgreSQL stores.
    if t.starts_with("timestamp") {
        let p = t
            .strip_prefix("timestamp(")
            .and_then(|r| r.split(')').next())
            .and_then(|n| n.parse().ok());
        return (
            "datetime".into(),
            Some(p.unwrap_or(POSTGRESQL_TIMESTAMP_PRECISION)),
        );
    }
    let ty = match t {
        "character varying" => "string",
        "text" => "text",
        "integer" | "smallint" => "integer",
        "bigint" => "bigint",
        "boolean" => "boolean",
        "date" => "date",
        "double precision" | "real" => "float",
        "json" => "json",
        "jsonb" => "jsonb",
        "bytea" => "binary",
        _ if t.starts_with("numeric") => "decimal",
        other => other,
    };
    (ty.into(), None)
}

/// A column default as `rails-compiler`'s schema arm records it from the
/// migration DSL: a literal integer, string or boolean is kept; anything the
/// database computes (`CURRENT_TIMESTAMP`, `now()`, a `'{}'::jsonb` document,
/// a float) is an expression, which the emitter turns into "no default".
#[must_use]
pub fn rails_default(default: Option<&str>) -> RailsDefault {
    use RailsDefault as D;
    let Some(d) = default.map(str::trim) else {
        return D::None;
    };
    if d.eq_ignore_ascii_case("true") {
        return D::Bool(true);
    }
    if d.eq_ignore_ascii_case("false") {
        return D::Bool(false);
    }
    if let Ok(i) = d.parse::<i64>() {
        return D::Int(i);
    }
    let literal = d
        .strip_suffix("::character varying")
        .or_else(|| d.strip_suffix("::text"))
        .unwrap_or(d);
    if !literal.contains("::") {
        if let Some(s) = literal
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
        {
            if !s.contains('\'') {
                return D::Str(s.to_string());
            }
        }
    }
    D::Expr(d.to_string())
}

/// The default classes of [`rails_default`], the same four as
/// `rails_compiler::schema::Default`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RailsDefault {
    None,
    Int(i64),
    Str(String),
    Bool(bool),
    /// Computed by the database; emitted as no default.
    Expr(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_dump_table_and_a_migration_alter() {
        let sql = "CREATE TABLE public.t (\n    id bigint NOT NULL,\n    \"position\" integer DEFAULT 1,\n    login character varying(256) DEFAULT ''::character varying NOT NULL,\n    CONSTRAINT c CHECK ((id > 0))\n);\nALTER TABLE t ADD COLUMN IF NOT EXISTS extra boolean NOT NULL DEFAULT false;\n";
        let t = &parse(sql)["t"];
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["id", "position", "login", "extra"]);
        assert_eq!(t.columns[2].sql_type, "character varying(256)");
        assert_eq!(
            t.columns[2].default.as_deref(),
            Some("''::character varying")
        );
        assert!(t.columns[2].not_null && !t.columns[1].not_null && t.columns[3].not_null);
        assert_eq!(
            rails_type("character varying(256)"),
            ("string".into(), Some(256))
        );
        assert_eq!(
            rails_default(Some("''::character varying")),
            RailsDefault::Str(String::new())
        );
        assert_eq!(
            rails_default(Some("'{}'::jsonb")),
            RailsDefault::Expr("'{}'::jsonb".into())
        );
        assert_eq!(
            rails_default(Some("CURRENT_TIMESTAMP")),
            RailsDefault::Expr("CURRENT_TIMESTAMP".into())
        );
    }

    /// A timestamp's precision is the one it was declared with, else
    /// PostgreSQL's 6; never the polyfill's millisecond default.
    #[test]
    fn a_timestamp_keeps_its_postgresql_precision() {
        for (sql, p) in [
            ("timestamp with time zone", 6),
            ("timestamp without time zone", 6),
            ("timestamp(6) with time zone", 6),
            ("timestamp(0) with time zone", 0),
            ("timestamp(3) without time zone", 3),
        ] {
            assert_eq!(rails_type(sql), ("datetime".into(), Some(p)), "{sql}");
        }
    }

    #[test]
    fn a_serial_primary_key_is_a_not_null_bigint() {
        let t = &parse("CREATE TABLE IF NOT EXISTS u (\n    id bigserial PRIMARY KEY,\n    name text NOT NULL DEFAULT '',\n    UNIQUE (name)\n);")["u"];
        assert_eq!(t.columns.len(), 2);
        assert_eq!(
            (t.columns[0].sql_type.as_str(), t.columns[0].not_null),
            ("bigint", true)
        );
        assert_eq!(t.columns[1].default.as_deref(), Some("''"));
    }
}
