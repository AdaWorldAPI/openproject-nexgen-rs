//! Where nexgen's SQL schema (`crates/op-db/migrations/*.sql`, applied in
//! order) differs from OpenProject's (`db/structure.sql`), column by column,
//! for the journey tables. The list is pinned in
//! `oracle/op-desk-schema-divergence.txt`; the test fails when it changes and
//! the file was not updated. After an intended change, rewrite the file with
//! `UPDATE_SCHEMA_DIVERGENCE=1` and review the diff.
//!
//! Compared after spelling both sides the way `pg_dump` does
//! (`bigserial` → `bigint`, `varchar` → `character varying`, `timestamptz` →
//! `timestamp with time zone`), with a default's trailing cast dropped
//! (`''::character varying` → `''`) and `CURRENT_TIMESTAMP` read as `now()`.

mod common;

use std::collections::BTreeMap;

use op_desk::structure::{SqlColumn, SqlTable};

const PINNED: &str = "oracle/op-desk-schema-divergence.txt";

fn nexgen_schema() -> BTreeMap<String, SqlTable> {
    let dir = common::repo_root().join("crates/op-db/migrations");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("migrations")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    files.sort();
    let mut tables = BTreeMap::new();
    for f in files {
        op_desk::structure::add(
            &mut tables,
            &std::fs::read_to_string(&f).expect("migration"),
        );
    }
    tables
}

fn default_of(c: &SqlColumn) -> String {
    let Some(d) = c.default.as_deref() else {
        return "none".into();
    };
    let mut d = d.trim();
    if let Some((lit, cast)) = d.rsplit_once("::") {
        if !cast.contains(['\'', '(']) {
            d = lit;
        }
    }
    if d.eq_ignore_ascii_case("CURRENT_TIMESTAMP") {
        "now()".into()
    } else {
        d.to_string()
    }
}

fn null_of(c: &SqlColumn) -> &'static str {
    if c.not_null {
        "NOT NULL"
    } else {
        "NULL"
    }
}

/// One line per difference, sorted.
fn divergences(op: &BTreeMap<String, SqlTable>, nx: &BTreeMap<String, SqlTable>) -> Vec<String> {
    let mut out = Vec::new();
    for t in op_desk::JOURNEY_TABLES {
        match (op.get(*t), nx.get(*t)) {
            (None, None) => out.push(format!("{t}: in neither schema")),
            (Some(_), None) => out.push(format!("{t}: table only in OpenProject")),
            (None, Some(_)) => out.push(format!("{t}: table only in nexgen")),
            (Some(o), Some(n)) => {
                for c in &o.columns {
                    match n.columns.iter().find(|x| x.name == c.name) {
                        None => out.push(format!(
                            "{t}.{}: only in OpenProject ({} {})",
                            c.name,
                            c.sql_type,
                            null_of(c)
                        )),
                        Some(x) => {
                            if c.sql_type != x.sql_type {
                                out.push(format!(
                                    "{t}.{}: type OpenProject {} / nexgen {}",
                                    c.name, c.sql_type, x.sql_type
                                ));
                            }
                            if c.not_null != x.not_null {
                                out.push(format!(
                                    "{t}.{}: nullability OpenProject {} / nexgen {}",
                                    c.name,
                                    null_of(c),
                                    null_of(x)
                                ));
                            }
                            let (a, b) = (default_of(c), default_of(x));
                            if a != b {
                                out.push(format!(
                                    "{t}.{}: default OpenProject {a} / nexgen {b}",
                                    c.name
                                ));
                            }
                        }
                    }
                }
                for x in &n.columns {
                    if !o.columns.iter().any(|c| c.name == x.name) {
                        out.push(format!(
                            "{t}.{}: only in nexgen ({} {})",
                            x.name,
                            x.sql_type,
                            null_of(x)
                        ));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

fn render(lines: &[String]) -> String {
    let mut s = String::from(
        "# Column-level differences between OpenProject's schema (db/structure.sql at the pinned\n\
         # OpenProject revision) and nexgen's SQL migrations (crates/op-db/migrations/*.sql), for the\n\
         # work-package journey tables (op_desk::JOURNEY_TABLES). Written by\n\
         # crates/op-desk/tests/schema_divergence.rs; rewrite with UPDATE_SCHEMA_DIVERGENCE=1.\n",
    );
    s.push_str(&format!("# {} differences\n", lines.len()));
    for l in lines {
        s.push_str(l);
        s.push('\n');
    }
    s
}

#[test]
fn nexgen_schema_divergence_is_pinned() {
    let lines = divergences(&common::structure(), &nexgen_schema());
    let now = render(&lines);
    let path = common::repo_root().join(PINNED);
    if std::env::var("UPDATE_SCHEMA_DIVERGENCE").as_deref() == Ok("1") {
        std::fs::write(&path, &now).expect("write pinned divergence");
        return;
    }
    let pinned = std::fs::read_to_string(&path).unwrap_or_default();
    if pinned != now {
        let old: Vec<&str> = pinned.lines().filter(|l| !l.starts_with('#')).collect();
        let new: Vec<&str> = now.lines().filter(|l| !l.starts_with('#')).collect();
        let gone: Vec<_> = old.iter().filter(|l| !new.contains(l)).collect();
        let added: Vec<_> = new.iter().filter(|l| !old.contains(l)).collect();
        panic!("{PINNED} is out of date (rewrite with UPDATE_SCHEMA_DIVERGENCE=1)\n  no longer: {gone:#?}\n  new: {added:#?}");
    }
}

/// The comparison sees a difference when there is one, and none when there is none.
#[test]
fn divergence_reports_a_changed_column_and_nothing_for_equal_schemas() {
    let sql = "CREATE TABLE public.users (\n    id bigint NOT NULL,\n    login character varying DEFAULT ''::character varying NOT NULL,\n    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP\n);\n";
    let op = op_desk::structure::parse(sql);
    let same = op_desk::structure::parse("CREATE TABLE users (\n    id bigserial PRIMARY KEY,\n    login varchar NOT NULL DEFAULT '',\n    created_at timestamptz DEFAULT now()\n);\n");
    let users = |l: &Vec<String>| {
        l.iter()
            .filter(|x| x.starts_with("users"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert!(
        users(&divergences(&op, &same)).is_empty(),
        "{:?}",
        divergences(&op, &same)
    );
    let other = op_desk::structure::parse("CREATE TABLE users (\n    id bigserial PRIMARY KEY,\n    login text NOT NULL DEFAULT '',\n    extra integer\n);\n");
    assert_eq!(
        users(&divergences(&op, &other)),
        [
            "users.created_at: only in OpenProject (timestamp with time zone NULL)",
            "users.extra: only in nexgen (integer NULL)",
            "users.login: type OpenProject character varying / nexgen text",
        ]
    );
}
