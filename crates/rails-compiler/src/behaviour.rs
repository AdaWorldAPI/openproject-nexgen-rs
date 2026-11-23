// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The callback table as grep-native rows.
//!
//! Rows come from the same in-memory chain that [`crate::emit`] writes as
//! Rust, so a person, `rg` or a graph reader can see what the generated
//! crate executes, journals or leaves out without reading table data in
//! source form. The generated Rust is the truth; these files are an index.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::{Binding, ChainEntry, Compilation};

pub const CALLBACKS_HEADER: &str = "#behaviour v1\n#schema\tmodel\tphase\tname\ttarget\ton\tconditions\tprovenance\n";
pub const SUMMARY_HEADER: &str = "#schema\tmodel\ttarget\tentries\tdistinct_names\n";

/// The table's own class for an entry, spelled as the generator spells it.
pub fn target_class(b: &Binding) -> &'static str {
    match b {
        Binding::Lowered(_) => "Lowered",
        Binding::Host => "Host",
        Binding::Framework => "Framework",
        Binding::Validator(_) => "Validator",
        Binding::Residual(_) => "Residual",
    }
}

fn cell(s: &str) -> String {
    let t: String = s.chars().map(|c| if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c }).collect();
    if t.is_empty() { "-".into() } else { t }
}

fn on_cell(on: &Option<Vec<String>>) -> String {
    let Some(v) = on else { return "any".into() };
    let flags: Vec<&str> = ["create", "update", "destroy"].into_iter().filter(|a| v.iter().any(|x| x == a)).collect();
    if flags.is_empty() { "none".into() } else { flags.join(",") }
}

/// Conditions as the table holds them; a condition the compiler could not
/// lower is marked `RESIDUAL:` so it can be searched for.
fn conds_cell(e: &ChainEntry) -> String {
    if e.conds.is_empty() {
        return "-".into();
    }
    let parts: Vec<String> = e
        .conds
        .iter()
        .map(|(kind, code)| {
            if code.starts_with("builtin:") || code.starts_with("fn:") {
                format!("{kind}:{code}")
            } else {
                format!("RESIDUAL:{kind}:{}", code.trim_start_matches("residual:"))
            }
        })
        .collect();
    cell(&parts.join("; "))
}

fn included(b: &Binding) -> bool {
    !(cfg!(feature = "disable-behaviour-host") && matches!(b, Binding::Host))
}

/// One row per chain entry, sorted by value.
pub fn callback_rows(c: &Compilation) -> Vec<[String; 7]> {
    let mut rows: Vec<[String; 7]> = Vec::new();
    for m in &c.models {
        for e in m.chain.iter().filter(|e| included(&e.binding)) {
            rows.push([
                cell(&m.name),
                cell(&e.phase),
                cell(&e.name),
                target_class(&e.binding).to_string(),
                on_cell(&e.on),
                conds_cell(e),
                cell(&format!("{} [{}]", e.provenance, e.rule)),
            ]);
        }
    }
    rows.sort_by(|a, b| (&a[0], &a[1], &a[2], &a[3], &a[6], &a[4], &a[5]).cmp(&(&b[0], &b[1], &b[2], &b[3], &b[6], &b[4], &b[5])));
    rows
}

pub fn callbacks_tsv(c: &Compilation) -> String {
    let mut out = String::from(CALLBACKS_HEADER);
    for r in callback_rows(c) {
        out.push_str(&r.join("\t"));
        out.push('\n');
    }
    out
}

pub fn summary_tsv(c: &Compilation) -> String {
    let mut by: BTreeMap<(String, String), (usize, BTreeSet<String>)> = BTreeMap::new();
    for r in callback_rows(c) {
        let e = by.entry((r[0].clone(), r[3].clone())).or_default();
        e.0 += 1;
        e.1.insert(r[2].clone());
    }
    let mut out = String::from(SUMMARY_HEADER);
    for ((model, target), (entries, names)) in by {
        out.push_str(&format!("{model}\t{target}\t{entries}\t{}\n", names.len()));
    }
    out
}

/// Writes `<namespace>.callbacks.tsv` and `<namespace>.summary.tsv` into `dir`.
pub fn write(c: &Compilation, namespace: &str, dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let files = [
        (dir.join(format!("{namespace}.callbacks.tsv")), callbacks_tsv(c)),
        (dir.join(format!("{namespace}.summary.tsv")), summary_tsv(c)),
    ];
    let mut written = Vec::new();
    for (path, text) in files {
        std::fs::write(&path, text)?;
        written.push(path);
    }
    Ok(written)
}
