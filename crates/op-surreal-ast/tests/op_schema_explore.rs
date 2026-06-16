//! End-to-end exploration: read the ndjson triple stream produced by
//! `ruff_ruby_spo` (D-AR-3 + D-AR-3.5) and render the full
//! OpenProject SurrealQL schema via `triples_to_schema`.
//!
//! Run with:
//!     cargo test -p op-surreal-ast --test op_schema_explore \
//!       -- --ignored --nocapture
//!
//! Reads `/tmp/op_triples.ndjson` (produced by `ruff_ruby_spo`'s
//! `op_pipeline_explore` test); writes the SurrealQL to
//! `/tmp/op_schema.surql`. Prints per-table stats.

use std::collections::BTreeMap;
use std::fs;

use lance_graph_contract::codegen_spine::Triple;
use op_surreal_ast::{ToSql, triples_to_schema};

#[test]
#[ignore]
#[allow(clippy::print_stderr, clippy::doc_markdown)]
fn render_full_openproject_schema_from_triples() {
    let Ok(ndjson) = fs::read_to_string("/tmp/op_triples.ndjson") else {
        eprintln!(
            "/tmp/op_triples.ndjson not found — run \
             `OPENPROJECT_PATH=… cargo test -p ruff_ruby_spo \
             --test op_pipeline_explore -- --ignored --nocapture` first",
        );
        return;
    };

    // The ndjson on disk uses `ruff_spo_triplet::Triple`; the bridge
    // works on `lance_graph_contract::codegen_spine::Triple` (which is
    // field-identical). Parse via a local serde shadow struct.
    #[derive(serde::Deserialize)]
    struct WireTriple {
        s: String,
        p: String,
        o: String,
        f: f32,
        c: f32,
    }
    let triples: Vec<Triple> = ndjson
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<WireTriple>(line).ok())
        .map(|w| Triple {
            s: w.s,
            p: w.p,
            o: w.o,
            f: w.f,
            c: w.c,
        })
        .collect();
    eprintln!("loaded {} triples from /tmp/op_triples.ndjson", triples.len());

    let schema = triples_to_schema(&triples);
    let sql = schema.to_sql();
    fs::write("/tmp/op_schema.surql", &sql).expect("write surql");
    eprintln!(
        "wrote {} bytes ({} tables) to /tmp/op_schema.surql",
        sql.len(),
        schema.tables.len(),
    );

    // Per-feature counts.
    let mut total_fields = 0_usize;
    let mut total_indices = 0_usize;
    let mut total_asserts = 0_usize;
    let mut tables_with_comment = 0_usize;
    let mut total_records = 0_usize;
    for table in &schema.tables {
        total_fields += table.fields.len();
        total_indices += table.indices.len();
        total_asserts += table.fields.iter().filter(|f| f.assert.is_some()).count();
        if table.comment.is_some() {
            tables_with_comment += 1;
        }
        total_records += table
            .fields
            .iter()
            .filter(|f| matches!(&f.kind, op_surreal_ast::Kind::Option(inner) if matches!(**inner, op_surreal_ast::Kind::Record(_))))
            .count();
    }
    eprintln!("\n=== Schema summary ===");
    eprintln!("  tables:                {}", schema.tables.len());
    eprintln!("  fields total:          {total_fields}");
    eprintln!("  fields with ASSERT:    {total_asserts}");
    eprintln!("  fields of record kind: {total_records}");
    eprintln!("  indices:               {total_indices}");
    eprintln!("  tables with comment:   {tables_with_comment}");

    // Top 10 fattest tables (by field count) with their comment.
    let mut by_field: Vec<&op_surreal_ast::TableDefinition> =
        schema.tables.iter().collect();
    by_field.sort_by_key(|t| std::cmp::Reverse(t.fields.len()));
    eprintln!("\n=== Top 10 tables by field count ===");
    for tbl in by_field.iter().take(10) {
        eprintln!(
            "  {:>3} fields, {:>2} indices  {}  {}",
            tbl.fields.len(),
            tbl.indices.len(),
            tbl.name,
            tbl.comment.as_deref().unwrap_or(""),
        );
    }

    // Distinct comment "prefixes" (acts_as_, callback:, include:) tally.
    let mut prefix_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for tbl in &schema.tables {
        if let Some(comment) = &tbl.comment {
            for part in comment.split("; ") {
                let prefix = if let Some(rest) = part.strip_prefix("acts_as_") {
                    let _ = rest;
                    "acts_as_*"
                } else if part.starts_with("callback:") {
                    "callback:*"
                } else if part.starts_with("include:") {
                    "include:*"
                } else {
                    "other"
                };
                *prefix_counts.entry(prefix).or_insert(0) += 1;
            }
        }
    }
    eprintln!("\n=== Comment-annotation breakdown ===");
    for (prefix, count) in &prefix_counts {
        eprintln!("  {count:>4}  {prefix}");
    }

    eprintln!("\nSurrealQL schema written to /tmp/op_schema.surql");
}
