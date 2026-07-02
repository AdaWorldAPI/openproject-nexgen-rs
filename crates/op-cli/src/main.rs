//! `op-codegen` — thin binary shell around [`op_cli::dispatch_codegen`].
//! Prints the emitted SurrealQL DDL to stdout on success; prints the
//! error (and the usage line on usage errors) to stderr otherwise.

use std::process::ExitCode;

use op_cli::{dispatch_codegen, CliError, EXIT_IO, EXIT_USAGE};

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match dispatch_codegen(&argv) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e @ CliError::Usage(_)) => {
            eprintln!("{e}");
            ExitCode::from(EXIT_USAGE)
        }
        Err(e @ CliError::PathNotFound(_)) => {
            eprintln!("{e}");
            ExitCode::from(EXIT_IO)
        }
    }
}
