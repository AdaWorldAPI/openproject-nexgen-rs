//! Regenerates `crates/op-contracts/src/work_packages/generated_create_admission.rs`
//! from an OpenProject checkout:
//!
//! ```text
//! OPENPROJECT_SRC=/path/to/openproject cargo run -p op-codegen-pipeline --example emit_create_admission
//! ```
//!
//! The test `create_transition_differential` fails when the checked-in file differs
//! from what this generates, so the table cannot drift from the Ruby.

use std::path::{Path, PathBuf};

fn main() {
    let src = PathBuf::from(std::env::var("OPENPROJECT_SRC").expect("OPENPROJECT_SRC"));
    let out = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../op-contracts/src/work_packages/generated_create_admission.rs");
    std::fs::write(&out, op_codegen_pipeline::admission_gen::generate(&src)).expect("write");
    eprintln!("wrote {}", out.display());
}
