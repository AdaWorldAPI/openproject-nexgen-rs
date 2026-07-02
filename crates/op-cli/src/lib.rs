//! `op-cli` — entry points for the OpenProject-nexgen command-line tools.
//!
//! Currently exposes one subcommand:
//!
//! - `op-codegen <rails-app-path>` — drive the C9 pipeline
//!   (ruff extraction → spine bridge → projection → SurrealQL emission)
//!   and print the DDL to stdout.
//!
//! The CLI shell (binary) is a thin wrapper around [`run_codegen`]; all
//! testable logic lives here so a smoke test can exercise it without
//! shelling out to a built binary.

use std::path::Path;

use op_codegen_pipeline::render_typed_surreal;

/// Errors that the CLI surface can return. Currently only one shape
/// (missing / non-existent rails app path); future subcommands or
/// stricter validation can extend the enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// User-facing usage error: missing or unrecognised argument.
    /// `String` is the message to print to stderr.
    Usage(String),
    /// I/O-shape error: a provided path does not exist or is not a
    /// directory. `String` is the offending path (for the error
    /// message).
    PathNotFound(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(m) => write!(f, "{m}"),
            Self::PathNotFound(p) => {
                write!(f, "error: path does not exist or is not a directory: {p}")
            }
        }
    }
}

impl std::error::Error for CliError {}

/// Exit code for [`CliError::Usage`] errors.
pub const EXIT_USAGE: u8 = 2;
/// Exit code for I/O-shape errors (e.g. path does not exist).
pub const EXIT_IO: u8 = 1;
/// Concise usage line for the `op-codegen` binary.
pub const USAGE: &str = "usage: op-codegen <rails-app-path>";

/// Run the codegen pipeline on a Rails app at `rails_root` and return
/// the emitted SurrealQL DDL as a `String`. Composes the
/// already-public [`extract_core_triples`] (filesystem walk + filter to
/// `CORE_V3_RESOURCES`) and [`render_surreal_from_ruff`] (spine bridge
/// + projection + emission); the CLI adds nothing semantic — it just
/// exposes the pipeline at a stable entry point.
///
/// Returns `Err(CliError::PathNotFound)` if `rails_root` does not exist
/// or is not a directory; the inner pipeline calls are expected to be
/// infallible on a well-formed Rails-shaped tree.
pub fn run_codegen(rails_root: &Path) -> Result<String, CliError> {
    if !rails_root.is_dir() {
        return Err(CliError::PathNotFound(rails_root.display().to_string()));
    }
    Ok(render_typed_surreal(rails_root))
}

/// Dispatch an `argv`-shaped slice (NOT including the program name) to
/// the `op-codegen` subcommand. Encapsulates the arg-parsing so the
/// binary's `main` stays a one-liner AND so smoke tests can hit the
/// same dispatcher.
///
/// Today the dispatcher accepts exactly one positional argument: the
/// Rails app path. A bare `--help` / `-h` returns the usage as an
/// `Err(CliError::Usage)` so the binary exits with [`EXIT_USAGE`] and
/// prints the message — matches the GNU convention for help-as-stderr.
pub fn dispatch_codegen(args: &[String]) -> Result<String, CliError> {
    match args {
        [] => Err(CliError::Usage(USAGE.to_string())),
        [first] if first == "-h" || first == "--help" => Err(CliError::Usage(USAGE.to_string())),
        [rails_root] => run_codegen(Path::new(rails_root)),
        _ => Err(CliError::Usage(format!(
            "{USAGE}\nerror: too many arguments ({} given)",
            args.len()
        ))),
    }
}
