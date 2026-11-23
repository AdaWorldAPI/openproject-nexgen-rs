//! Writes `crates/op-desk/src/generated/` from `openproject.toml` and the
//! OpenProject checkout named by `OPENPROJECT_SRC`:
//!
//! ```text
//! OPENPROJECT_SRC=<openproject checkout> cargo run -p op-desk --example generate
//! ```
//!
//! The example links `op-desk` itself (for `op_desk::structure` and
//! `op_desk::types`). If the committed `src/generated/` no longer compiles,
//! replace `src/generated/mod.rs` with
//! `pub static MODELS: &[&rails_polyfill::ModelSpec] = &[];` and run it again.

#[path = "../tests/gen/mod.rs"]
mod gen;

use std::process::ExitCode;

fn main() -> ExitCode {
    let root = gen::repo_root();
    let files = match gen::generate(&root.join("openproject.toml")) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("generate failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let dir = root.join("crates/op-desk/src/generated");
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("{}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    for (name, text) in &files {
        if let Err(e) = std::fs::write(dir.join(name), text) {
            eprintln!("{name}: {e}");
            return ExitCode::FAILURE;
        }
    }
    eprintln!("wrote {} files to {}", files.len(), dir.display());
    ExitCode::SUCCESS
}
