//! Exit-code regression — locks the `process::exit(2)` contract from PR #31's
//! degenerate-input/output guards against drift.
//!
//! Mirrors `lance-graph#512`'s `#[should_panic(expected = "…")]` discipline,
//! translated for a `process::exit`-based CLI: each test spawns the actual
//! binary, feeds it a degenerate input, and asserts BOTH
//!
//! 1. `status.code() == Some(2)` — the contract documented in the binary's
//!    module doc lines 31-40, citing `lance-graph#512`'s exit-0/1/2 convention.
//! 2. A literal stderr substring grepped from the real guard messages
//!    (`crates/op-surreal-ast/src/bin/op_codegen_ndjson.rs:99` and `:117` at
//!    PR #31's HEAD `2e85636`), so a future refactor that *renames the
//!    message* fails this test instead of silently passing — the equivalent
//!    of `#[should_panic(expected = …)]`'s message lock.
//!
//! Uses `env!("CARGO_BIN_EXE_op-codegen-ndjson")` — Cargo's built-in for
//! binary integration tests — so no `assert_cmd` dep is added. Linux-only by
//! the `/dev/null` empty-stream stand-in (the portability concern was
//! deliberately dropped per the PR review thread).
//!
//! Feature-gated to `cli`: the binary itself is opt-in (`#[cfg(feature = "cli")]`
//! in the bin source); this regression must follow the same gate or
//! `CARGO_BIN_EXE_*` would not resolve.

#![cfg(feature = "cli")]

use std::io::Write;
use std::process::{Command, Stdio};

/// **Degenerate-INPUT guard** — empty triple stream → exit 2.
///
/// Locks the guard at `op_codegen_ndjson.rs:97-103`: when the parser returns
/// zero triples, the CLI must NOT proceed to emit an empty SurrealQL file
/// (which would "make downstream pipelines fail far from the cause" — the
/// guard's own WHY comment). It must instead exit 2 with the literal
/// `input contains zero triples` substring naming the upstream cause.
#[test]
fn empty_input_exits_2_with_zero_triples_message() {
    let out = Command::new(env!("CARGO_BIN_EXE_op-codegen-ndjson"))
        .arg("/dev/null")
        .output()
        .expect("CARGO_BIN_EXE_op-codegen-ndjson resolves under --features cli");

    assert_eq!(
        out.status.code(),
        Some(2),
        "empty input must exit code 2 (degenerate-input guard); got {:?}\n\
         stdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("input contains zero triples"),
        "expected literal substring `input contains zero triples` from the \
         degenerate-input guard at op_codegen_ndjson.rs:99; got:\n{stderr}",
    );
}

/// **Degenerate-OUTPUT guard** — triples without any `(*, rdf:type,
/// ogit:ObjectType)` declaration → exit 2.
///
/// Locks the guard at `op_codegen_ndjson.rs:114-121`: a body-walk-only triple
/// stream (e.g. `has_attribute` rows on subjects no `rdf:type` row ever
/// declares) projects to a `Schema` with zero `tables`. Rendering and writing
/// an empty `.surql` file would silently ship a no-op schema. The guard exits
/// 2 with the literal `none declared a class` substring so the cause is
/// readable from CI logs.
#[test]
fn body_walk_without_rdf_type_object_exits_2_with_no_class_declared_message() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_op-codegen-ndjson"))
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn op-codegen-ndjson");

    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(
            br#"{"s":"openproject:Ghost","p":"has_attribute","o":"subject","f":0.95,"c":0.88}"#,
        )
        .expect("write triple to child stdin");

    let out = child.wait_with_output().expect("wait for op-codegen-ndjson");

    assert_eq!(
        out.status.code(),
        Some(2),
        "no-rdf:type-ObjectType input must exit code 2 (degenerate-output guard); \
         got {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("none declared a class"),
        "expected literal substring `none declared a class` from the \
         degenerate-output guard at op_codegen_ndjson.rs:117; got:\n{stderr}",
    );
}
