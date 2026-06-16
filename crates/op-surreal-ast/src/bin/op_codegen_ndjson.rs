//! `op-codegen-ndjson` — read an SPO-triple ndjson stream produced by
//! `ruff_ruby_spo` (the OpenProject AR-shape extractor) and write
//! the rendered SurrealQL schema.
//!
//! # Usage
//!
//! ```sh
//! # Read from a file, write to stdout
//! op-codegen-ndjson /tmp/op_triples.ndjson
//!
//! # Read from stdin (pipeline)
//! cat /tmp/op_triples.ndjson | op-codegen-ndjson - > schema.surql
//!
//! # Read from file, write to file
//! op-codegen-ndjson /tmp/op_triples.ndjson -o schema.surql
//!
//! # With stats to stderr
//! op-codegen-ndjson /tmp/op_triples.ndjson --stats > schema.surql
//! ```
//!
//! # End-to-end pipeline
//!
//! ```text
//!   OpenProject/app/models/  ─►  ruff_ruby_spo (Ruby AST)
//!                              ─►  triples.ndjson (8500+ SPO triples)
//!                              ─►  op-codegen-ndjson (THIS BINARY)
//!                              ─►  schema.surql (DEFINE TABLE / FIELD / INDEX)
//!                              ─►  surrealdb
//! ```
//!
//! # Exit codes
//!
//! Following the
//! [`lance-graph#512`](https://github.com/AdaWorldAPI/lance-graph/pull/512)
//! convention — degenerate-input vs generic-error are split so wrapper
//! scripts can react to the cause:
//!
//! - `0` — schema rendered successfully.
//! - `1` — argument / I/O error (message on stderr).
//! - `2` — degenerate input (empty / malformed ndjson, zero triples).

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process;

use lance_graph_contract::codegen_spine::Triple;
use op_surreal_ast::{Schema, ToSql, triples_to_schema};

#[derive(serde::Deserialize)]
struct WireTriple {
    s: String,
    p: String,
    o: String,
    f: f32,
    c: f32,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let parsed = match parse_args(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}\n\n{}", USAGE);
            process::exit(1);
        }
    };
    if parsed.help {
        println!("{USAGE}");
        return;
    }

    let ndjson = match read_input(&parsed.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading input: {e}");
            process::exit(1);
        }
    };

    let triples = match parse_ndjson(&ndjson) {
        Ok(t) => t,
        Err((line, msg)) => {
            // Malformed ndjson — degenerate input, exit 2 (per
            // lance-graph#512's degenerate-input convention).
            eprintln!("error parsing ndjson line {line}: {msg}");
            process::exit(2);
        }
    };

    // Degenerate-input guard (lance-graph#512 pattern): an empty
    // triple stream silently produces an empty SurrealQL file, which
    // makes downstream pipelines fail far from the cause. Exit 2 with
    // a clear message so the user knows the upstream extractor never
    // emitted anything.
    if triples.is_empty() {
        eprintln!(
            "error: input contains zero triples — run the upstream extractor first \
             (e.g. `cargo test -p ruff_ruby_spo --test op_pipeline_explore -- --ignored`)"
        );
        process::exit(2);
    }

    if parsed.stats {
        eprintln!("loaded {} triples", triples.len());
    }

    let schema = triples_to_schema(&triples);

    // Degenerate-output guard: zero tables means no `rdf:type ObjectType`
    // triples in the input. The output SurrealQL would be empty; we'd
    // rather fail loudly than ship a no-op schema file.
    if schema.tables.is_empty() {
        eprintln!(
            "error: triple stream has no `(*, rdf:type, ogit:ObjectType)` declarations \
             — no tables to render ({} triples were loaded but none declared a class)",
            triples.len()
        );
        process::exit(2);
    }

    let sql = schema.to_sql();

    if let Err(e) = write_output(&parsed.output, &sql) {
        eprintln!("error writing output: {e}");
        process::exit(1);
    }

    if parsed.stats {
        print_stats(&schema);
    }
}

#[derive(Debug)]
struct ParsedArgs {
    input: Input,
    output: Output,
    stats: bool,
    help: bool,
}

#[derive(Debug)]
enum Input {
    Stdin,
    Path(PathBuf),
}

#[derive(Debug)]
enum Output {
    Stdout,
    Path(PathBuf),
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut input: Option<Input> = None;
    let mut output: Option<Output> = None;
    let mut stats = false;
    let mut help = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => help = true,
            "--stats" => stats = true,
            "-o" | "--output" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "-o/--output requires a path argument".to_string())?;
                output = Some(if value == "-" {
                    Output::Stdout
                } else {
                    Output::Path(PathBuf::from(value))
                });
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag `{other}`"));
            }
            "-" => {
                if input.is_some() {
                    // Codex P2 (PR #31): a second positional input must
                    // be rejected, not silently overwrite the first.
                    // `op-codegen-ndjson expected.ndjson accidental.ndjson`
                    // would otherwise render the schema from the wrong
                    // file. Mirror the lance-graph#512 "fail loud on
                    // bad input" convention.
                    return Err(
                        "multiple positional inputs given; expected a single path or `-` for stdin"
                            .to_string(),
                    );
                }
                input = Some(Input::Stdin);
            }
            path => {
                if input.is_some() {
                    return Err(format!(
                        "multiple positional inputs given (`{path}` after the first); \
                         expected a single path or `-` for stdin",
                    ));
                }
                input = Some(Input::Path(PathBuf::from(path)));
            }
        }
        i += 1;
    }
    Ok(ParsedArgs {
        input: input.unwrap_or(Input::Stdin),
        output: output.unwrap_or(Output::Stdout),
        stats,
        help,
    })
}

fn read_input(input: &Input) -> io::Result<String> {
    match input {
        Input::Stdin => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;
            Ok(s)
        }
        Input::Path(path) => fs::read_to_string(path),
    }
}

fn write_output(output: &Output, content: &str) -> io::Result<()> {
    match output {
        Output::Stdout => io::stdout().write_all(content.as_bytes()),
        Output::Path(path) => fs::write(path, content),
    }
}

fn parse_ndjson(ndjson: &str) -> Result<Vec<Triple>, (usize, String)> {
    let mut out = Vec::new();
    for (idx, line) in ndjson.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let wire: WireTriple = serde_json::from_str(line).map_err(|e| (idx + 1, e.to_string()))?;
        out.push(Triple {
            s: wire.s,
            p: wire.p,
            o: wire.o,
            f: wire.f,
            c: wire.c,
        });
    }
    Ok(out)
}

fn print_stats(schema: &Schema) {
    let tables = schema.tables.len();
    let mut fields = 0_usize;
    let mut typed_scalars = 0_usize;
    let mut record_fks = 0_usize;
    let mut option_any = 0_usize;
    let mut asserts = 0_usize;
    let mut indices = 0_usize;
    let mut commented = 0_usize;
    for t in &schema.tables {
        fields += t.fields.len();
        indices += t.indices.len();
        if t.comment.is_some() {
            commented += 1;
        }
        for f in &t.fields {
            if f.assert.is_some() {
                asserts += 1;
            }
            match &f.kind {
                op_surreal_ast::Kind::Option(inner) => match inner.as_ref() {
                    op_surreal_ast::Kind::Record(_) => record_fks += 1,
                    op_surreal_ast::Kind::Any => option_any += 1,
                    _ => typed_scalars += 1,
                },
                op_surreal_ast::Kind::Record(_) => record_fks += 1,
                op_surreal_ast::Kind::Any => option_any += 1,
                _ => typed_scalars += 1,
            }
        }
    }
    eprintln!(
        "schema: {tables} tables, {fields} fields ({record_fks} record FKs, \
         {typed_scalars} typed scalars, {option_any} option<any>), \
         {indices} indices, {asserts} ASSERTs, {commented} table comments",
    );
}

const USAGE: &str = "\
op-codegen-ndjson — render SurrealQL schema from SPO-triple ndjson

USAGE:
    op-codegen-ndjson [INPUT] [-o OUTPUT] [--stats]

ARGS:
    INPUT       Path to ndjson file, or `-` for stdin (default: stdin).

OPTIONS:
    -o, --output PATH    Write SurrealQL to PATH instead of stdout. Use `-` for stdout.
    --stats              Print per-feature counts to stderr.
    -h, --help           Show this help.

EXAMPLE:
    OPENPROJECT_PATH=/path/to/openproject \\
      cargo test -p ruff_ruby_spo --test op_pipeline_explore -- --ignored
    # produces /tmp/op_triples.ndjson

    op-codegen-ndjson /tmp/op_triples.ndjson --stats > /tmp/op_schema.surql
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ndjson_returns_triples() {
        let nd = r#"{"s":"a","p":"rdf:type","o":"ogit:ObjectType","f":1.0,"c":1.0}
{"s":"a","p":"has_attribute","o":"subject","f":0.95,"c":0.88}"#;
        let triples = parse_ndjson(nd).unwrap();
        assert_eq!(triples.len(), 2);
        assert_eq!(triples[0].p, "rdf:type");
        assert_eq!(triples[1].o, "subject");
    }

    #[test]
    fn parse_ndjson_skips_blank_lines() {
        let nd = "\n\n{\"s\":\"a\",\"p\":\"rdf:type\",\"o\":\"ogit:ObjectType\",\"f\":1.0,\"c\":1.0}\n\n";
        assert_eq!(parse_ndjson(nd).unwrap().len(), 1);
    }

    #[test]
    fn parse_ndjson_reports_offending_line_number() {
        let nd = "{\"s\":\"a\",\"p\":\"rdf:type\",\"o\":\"ogit:ObjectType\",\"f\":1.0,\"c\":1.0}\nNOT JSON\n";
        let err = parse_ndjson(nd).expect_err("malformed line must fail");
        assert_eq!(err.0, 2, "line number is 1-based; bad line is the second");
    }

    #[test]
    fn parse_args_input_from_path() {
        let p = parse_args(&["/tmp/x.ndjson".to_string()]).unwrap();
        match p.input {
            Input::Path(path) => assert_eq!(path.to_str().unwrap(), "/tmp/x.ndjson"),
            Input::Stdin => panic!("expected Path input"),
        }
    }

    #[test]
    fn parse_args_stats_flag() {
        let p = parse_args(&["--stats".to_string()]).unwrap();
        assert!(p.stats);
    }

    #[test]
    fn parse_args_output_flag() {
        let p = parse_args(&["-o".to_string(), "/tmp/s.surql".to_string()]).unwrap();
        match p.output {
            Output::Path(path) => assert_eq!(path.to_str().unwrap(), "/tmp/s.surql"),
            Output::Stdout => panic!("expected Path output"),
        }
    }

    #[test]
    fn parse_args_unknown_flag_errors() {
        let err = parse_args(&["--bogus".to_string()]).expect_err("unknown flag must fail");
        assert!(err.contains("--bogus"));
    }

    /// **Codex P2 regression (PR #31)** — a second positional input
    /// must be rejected rather than silently overwriting the first.
    /// `op-codegen-ndjson expected.ndjson accidental.ndjson` would
    /// otherwise render the schema from `accidental.ndjson` (wrong
    /// corpus, valid output) — exactly the kind of silent
    /// information loss the lance-graph#512 "fail loud" convention
    /// guards against.
    #[test]
    fn parse_args_rejects_multiple_positional_inputs() {
        let err = parse_args(&[
            "expected.ndjson".to_string(),
            "accidental.ndjson".to_string(),
        ])
        .expect_err("two positional paths must fail");
        assert!(
            err.contains("multiple positional inputs"),
            "error must name the cause; got: {err}",
        );
        assert!(
            err.contains("accidental.ndjson"),
            "error should echo the offending second arg; got: {err}",
        );
    }

    /// **Codex P2 regression** — also covers `path then -` and
    /// `- then path` (stdin-marker as the second positional).
    #[test]
    fn parse_args_rejects_stdin_after_path() {
        assert!(
            parse_args(&["foo.ndjson".to_string(), "-".to_string()])
                .expect_err("path then `-` must fail")
                .contains("multiple positional inputs"),
        );
        assert!(
            parse_args(&["-".to_string(), "foo.ndjson".to_string()])
                .expect_err("`-` then path must fail")
                .contains("multiple positional inputs"),
        );
    }

    #[test]
    fn end_to_end_renders_schema_from_ndjson() {
        let nd = r#"{"s":"openproject:WorkPackage","p":"rdf:type","o":"ogit:ObjectType","f":1.0,"c":1.0}
{"s":"openproject:WorkPackage","p":"has_attribute","o":"subject","f":0.95,"c":0.88}"#;
        let triples = parse_ndjson(nd).unwrap();
        let schema = triples_to_schema(&triples);
        let sql = schema.to_sql();
        assert!(sql.contains("DEFINE TABLE WorkPackage"));
        assert!(sql.contains("DEFINE FIELD subject"));
    }

    /// **Degenerate-input guard (lance-graph#512 pattern)** — empty
    /// ndjson parses cleanly to zero triples. The CLI's `main()` then
    /// exits with code 2 + a directive message; locked here at the
    /// parser layer so the empty-stream contract is observable from
    /// tests without spawning the binary.
    #[test]
    fn empty_ndjson_yields_zero_triples() {
        assert!(parse_ndjson("").unwrap().is_empty());
        assert!(parse_ndjson("\n\n  \n").unwrap().is_empty());
    }

    /// **Degenerate-output guard (lance-graph#512 pattern)** — a
    /// triple stream with no `rdf:type ObjectType` declarations yields
    /// an empty schema (zero tables). The CLI's `main()` catches this
    /// and exits code 2; this test locks the precondition that
    /// `triples_to_schema` doesn't silently invent a table for
    /// body-walk triples on un-declared subjects.
    #[test]
    fn triples_without_rdf_type_object_yield_empty_schema() {
        let nd = r#"{"s":"openproject:Ghost","p":"has_attribute","o":"x","f":0.95,"c":0.88}"#;
        let triples = parse_ndjson(nd).unwrap();
        let schema = triples_to_schema(&triples);
        assert!(
            schema.tables.is_empty(),
            "body-walk triple alone must NOT materialise a table (phantom-table guard); got {:?}",
            schema.tables.iter().map(|t| &t.name).collect::<Vec<_>>(),
        );
    }
}
