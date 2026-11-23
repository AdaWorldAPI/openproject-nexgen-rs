// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! `rails-f000-matrix <app.toml> [--out <file>]` — walk every scope of a Rails
//! application, transpose it onto the F000 lens, and write one JSON row each.

use std::path::PathBuf;
use std::process::ExitCode;

use rails_compiler::config::Loaded;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cfg_path) = args.iter().find(|a| !a.starts_with("--") && a.ends_with(".toml")).map(PathBuf::from) else {
        eprintln!("usage: rails-f000-matrix <app.toml> [--out <file>]");
        return ExitCode::from(2);
    };
    let out_arg = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let cfg = match Loaded::read(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let matrix = match rails_f000::matrix::build(&cfg) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("matrix failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let out = out_arg.unwrap_or_else(|| cfg.base.join(format!("harvest/f000-matrix-{}.json", cfg.config.app.name)));
    if let Some(dir) = out.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(&matrix).unwrap_or_default();
    if let Err(e) = std::fs::write(&out, json + "\n") {
        eprintln!("{}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    let m = &matrix.metrics;
    println!("app                    {} (activerecord {}, load_defaults {})", matrix.app, matrix.activerecord, matrix.load_defaults.as_deref().unwrap_or("UNKNOWN"));
    println!("constructs harvested   {}  (scope declarations {}, default scopes {}, call sites {})", m.constructs_harvested, m.scope_declarations, m.default_scopes, m.call_sites);
    println!("recognized             {}", m.recognized);
    println!("Alias                  {}", m.alias);
    println!("Compose                {}", m.compose);
    println!("Learned                {}", m.learned);
    println!("Residual               {}", m.residual);
    println!("Keep Rust (runtime)    {}", m.keep_rust);
    println!("Refuse/Unresolved      {}", m.refuse_unresolved);
    println!("prepared natively      {}", m.prepared);
    println!("behaviourally verified {}", m.behaviourally_verified);
    println!("default scopes order-only {} of {}", m.default_scopes_order_only, m.default_scopes);
    println!("call sites with unknown receiver class {}", m.call_sites_class_unknown);
    println!("ruff schema gap filled for {} class(es)", matrix.ruff_schema_gap_filled.len());
    println!("catalog vs ruff scope disagreements in {} class(es)", matrix.catalog_vs_ruff.len());
    println!("wrote {}", out.display());
    ExitCode::SUCCESS
}
