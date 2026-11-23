//! The desk answers every recorded read exactly as PostgreSQL did
//! (`oracle/desk-reads.pg.json`, recorded by `tests/pg_oracle.rs`). No
//! database is needed; nothing is refused.

use op_desk::desk::Desk;
use op_desk::reads::demo_reads;
use serde_json::Value as J;

mod common;

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
        assert_eq!(answer, entry["answer"], "{}", read.to_json());
        rows_compared += entry["answer"]["rows"].as_array().map_or(0, Vec::len);
    }
    assert!(
        rows_compared > 200,
        "the recording is not empty: {rows_compared} rows"
    );
}
