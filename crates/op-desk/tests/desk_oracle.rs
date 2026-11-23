//! The desk answers every recorded read exactly as PostgreSQL did
//! (`oracle/desk-reads.pg.json`, recorded by `tests/pg_oracle.rs`). No
//! database is needed; nothing is refused.

use op_desk::desk::Desk;
use op_desk::reads::demo_reads;
use serde_json::Value as J;

mod common;

/// The answer with every cell of a `Json` column parsed. Rails reads a json /
/// jsonb column as the parsed document, never as its text, and PostgreSQL
/// rewrites jsonb text (key order, spacing), so the document is what is
/// compared there; every other cell is compared as it is.
fn json_cells_parsed(table: &str, answer: &J) -> J {
    let spec = op_desk::desk::table(table).expect("spec");
    let mut a = answer.clone();
    let columns: Vec<String> = a["columns"]
        .as_array()
        .map(|c| c.iter().filter_map(J::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    if let Some(rows) = a["rows"].as_array_mut() {
        for row in rows {
            for (i, name) in columns.iter().enumerate() {
                let is_json = spec
                    .index_of(name)
                    .is_some_and(|at| spec.columns[at].ty == rails_polyfill::ColumnType::Json);
                if let (true, Some(text)) = (is_json, row[i].as_str()) {
                    row[i] = serde_json::from_str(text)
                        .unwrap_or_else(|e| panic!("{table}.{name}: {e}"));
                }
            }
        }
    }
    a
}

#[test]
fn the_desk_answers_every_read_as_postgresql_did() {
    let doc: J = serde_json::from_str(include_str!("../seed/demo.json")).expect("demo.json");
    let desk = Desk::from_seed(&doc).expect("desk");
    let path = common::repo_root().join("oracle/desk-reads.pg.json");
    let recorded: J =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("oracle file")).expect("json");
    let recorded = recorded.as_array().expect("list");
    let reads = demo_reads();
    assert_eq!(
        recorded.len(),
        reads.len(),
        "the recording covers every read"
    );
    let mut rows_compared = 0;
    for (read, entry) in reads.iter().zip(recorded) {
        assert_eq!(entry["read"], read.to_json(), "recording order");
        let answer = read
            .on_desk(&desk)
            .unwrap_or_else(|e| panic!("{:?} refused: {e}", read.to_json()))
            .to_json();
        assert_eq!(
            json_cells_parsed(read.table(), &answer),
            json_cells_parsed(read.table(), &entry["answer"]),
            "{}",
            read.to_json()
        );
        rows_compared += entry["answer"]["rows"].as_array().map_or(0, Vec::len);
    }
    assert!(
        rows_compared > 200,
        "the recording is not empty: {rows_compared} rows"
    );
}
