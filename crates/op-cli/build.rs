//! Bakes the building repo's git sha into the binary via
//! `NEXGEN_GIT_SHA`, consumed by [`op_codegen_pipeline::render_typed_surreal`]
//! for the provenance trailer (P0 instrument hygiene, 2026-07-02
//! epiphany #4: "instrument lies" — the emitted DDL should be able to
//! answer "which build produced this artifact" without archaeology).
//!
//! Best-effort: falls back to `"unknown"` if git isn't on PATH or the
//! tree isn't a git checkout (e.g. a published tarball) — a missing
//! provenance stamp is a shrug, not a build failure.

use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=NEXGEN_GIT_SHA={sha}");
    // Best-effort rebuild trigger on the common case (a new commit on
    // the current branch). Doesn't catch every ref-mutating operation
    // (rebase, detached-HEAD checkout to another sha via other means) —
    // acceptable for a provenance hint, not a correctness contract.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
