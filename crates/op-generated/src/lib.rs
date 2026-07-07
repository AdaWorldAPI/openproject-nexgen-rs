//! `op-generated` — the OP → op-rs V3 transpile landing zone.
//!
//! This crate holds Rust emitted by the OGAR V3 consumer pipeline
//! (`ruff_ruby_spo::extract_app_with_schema` → `filter_to_core` →
//! `op_codegen_pipeline::ogar_consumer::compile_op::<OpenProjectPort>` →
//! render) under [`generated`]. The `.rs` files there are written by the
//! checked-in generator binary
//! `crates/op-codegen-pipeline/src/bin/emit_generated.rs` —
//! **do not hand-edit anything under `src/generated/`**; the next
//! generator run overwrites it. Review the diff it produces instead, the
//! way you'd review a `.pb.rs` protobuf-codegen diff.
//!
//! # Why a separate crate, not per-crate `generated/` modules?
//!
//! One object model in one place. Hand-written crates (`op-work-packages`,
//! `op-models`, …) opt in by *depending on* `op-generated`; until a crate
//! adds that dependency, nothing about its build changes.
//!
//! # The dependency boundary is the point
//!
//! This crate has **zero dependencies** and must stay that way. Hand-written
//! app crates will eventually depend on `op-generated` (W6), so if this
//! crate ever pulled in OGAR / askama / ruff / lance-graph, every such
//! consumer would transitively inherit the transpiler's dependency graph.
//! The emit side (`emit_generated`, which *does* need OGAR/askama/ruff to
//! harvest and render) lives upstream in `op-codegen-pipeline`, gated
//! behind the `ogar-emit` feature, precisely so this crate never has to.

pub mod generated;
