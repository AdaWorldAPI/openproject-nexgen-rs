//! `rails-compile <app.toml>` — harvest, compile and emit one Rails
//! application slice, and write the machine-readable harvest report.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use rails_compiler::config::Loaded;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let corpus_only = args.iter().any(|a| a == "--corpus");
    let Some(cfg_path) = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from) else {
        eprintln!("usage: rails-compile [--corpus] <app.toml>");
        return ExitCode::from(2);
    };
    let cfg = match Loaded::read(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let t0 = Instant::now();
    if corpus_only {
        return match rails_compiler::corpus(&cfg) {
            Ok(c) => {
                let out = cfg.base.join(format!("harvest/{}-corpus.json", cfg.config.app.name));
                let _ = std::fs::create_dir_all(cfg.base.join("harvest"));
                let j = serde_json::to_string_pretty(&c).unwrap_or_default();
                if let Err(e) = std::fs::write(&out, j + "\n") {
                    eprintln!("{}: {e}", out.display());
                    return ExitCode::FAILURE;
                }
                eprintln!("corpus measured in {} ms → {}", t0.elapsed().as_millis(), out.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    let compiled = match rails_compiler::compile(&cfg) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("compile failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let compile_ms = t0.elapsed().as_millis();
    // Stores are part of the semantic class; re-expand to read them.
    let stores: BTreeMap<String, Vec<String>> = compiled
        .models
        .iter()
        .map(|m| (m.name.clone(), m.stores.clone()))
        .collect();
    let out = cfg.base.join(&cfg.config.output);
    let t1 = Instant::now();
    let written = match rails_compiler::emit::write(&compiled, &stores, &out) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("emit failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let behaviour_dir = cfg.base.join("harvest/behaviour");
    let behaviour = match rails_compiler::behaviour::write(&compiled, &cfg.config.app.namespace, &behaviour_dir) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("behaviour table failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let emit_ms = t1.elapsed().as_millis();
    let report = cfg.base.join("harvest/report.json");
    if let Some(p) = report.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    match serde_json::to_string_pretty(&compiled) {
        Ok(j) => {
            if let Err(e) = std::fs::write(&report, j + "\n") {
                eprintln!("{}: {e}", report.display());
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("report: {e}");
            return ExitCode::FAILURE;
        }
    }
    for m in &compiled.models {
        let lowered = m.chain.iter().filter(|c| matches!(c.binding, rails_compiler::Binding::Lowered(_))).count();
        let residual = m.chain.iter().filter(|c| matches!(c.binding, rails_compiler::Binding::Residual(_))).count();
        eprintln!(
            "{:<24} chain {:>3}  lowered {:>2}  residual {:>2}  methods {:>2}  host-calls {}",
            m.name,
            m.chain.len(),
            lowered,
            residual,
            m.lowered_methods.len(),
            m.host_calls.len()
        );
    }
    eprintln!("compile {compile_ms} ms, emit {emit_ms} ms; wrote {} files + {}", written.len() + 2 + behaviour.len(), report.display());
    ExitCode::SUCCESS
}
