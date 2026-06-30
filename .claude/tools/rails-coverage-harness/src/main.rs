//! Rails AR-DSL coverage + STI chaining-collapse harness.
//!
//! Parses a Rails `<root>/app/models/**/*.rb` tree via `ruff_ruby_spo` and
//! reports: (1) the AR-DSL composition (what kind of code), (2) the behavioural
//! recipe surface (callback phases / validation kinds / association kinds — the
//! fixed enumerable protocol), (3) per-model recipe density, (4) the STI
//! chaining-collapse (how much of the materialised recipe is inherited-shared,
//! i.e. free "at the cost of an import").
//!
//! Usage: `cargo run --release -- <source_root> [namespace_label]`
//!   <source_root>     a dir whose `app/models/` subtree holds the .rb models.
//!   namespace_label   IRI namespace tag (e.g. "redmine" / "openproject"),
//!                     default "rails". Purely cosmetic for this harness.
//!
//! Verified baselines (2026-06-30, same metric across consumers):
//!   Odoo (Python, inherits_from)  : 22.7% chaining collapse (methods)
//!   Redmine (app/models)          : 53.8%
//!   OpenProject (app/models, recursive) : 71.7%
//! The gradient tracks inheritance density — the "best-shaped consumer" result.
//!
//! See ../../knowledge/RAILS-COVERAGE-KIT.md for the full runbook + the
//! canonical-label doctrine (labels are content-addressable concept ids, not a
//! per-consumer enum zoo).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ruff_ruby_spo::extract_with;
use ruff_spo_triplet::{AssocKind, ValidationKind};

fn akind(k: AssocKind) -> &'static str {
    match k {
        AssocKind::BelongsTo => "belongs_to",
        AssocKind::HasMany => "has_many",
        AssocKind::HasOne => "has_one",
        AssocKind::HasAndBelongsToMany => "habtm",
        AssocKind::AcceptsNestedAttributesFor => "nested_attrs",
    }
}
fn vkind(k: ValidationKind) -> &'static str {
    match k {
        ValidationKind::Validates => "validates",
        ValidationKind::Validate => "validate",
        ValidationKind::Normalizes => "normalizes",
        ValidationKind::ValidatesAssociated => "validates_associated",
        ValidationKind::ValidatesEach => "validates_each",
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("usage: rails_coverage_harness <source_root> [label]");
    let label = args.next().unwrap_or_else(|| "rails".to_string());
    let g = extract_with(Path::new(&dir), &label);
    let n = g.models.len();
    assert!(n > 0, "0 models parsed — does <source_root>/app/models exist?");

    let (mut assoc, mut valid, mut cb, mut meth, mut conc, mut attr, mut scope, mut deleg) =
        (0usize, 0, 0, 0, 0, 0, 0, 0);
    let mut sti = 0usize;
    let (mut m_cb, mut m_val, mut m_conc, mut m_assoc) = (0usize, 0, 0, 0);
    let mut phases: BTreeMap<String, u32> = BTreeMap::new();
    let mut akinds: BTreeMap<&str, u32> = BTreeMap::new();
    let mut vkinds: BTreeMap<&str, u32> = BTreeMap::new();

    for m in &g.models {
        assoc += m.associations.len();
        valid += m.validations.len();
        cb += m.callbacks.len();
        meth += m.functions.len();
        conc += m.concerns.len();
        attr += m.attributes.len();
        scope += m.scopes.len();
        deleg += m.delegations.len();
        sti += usize::from(m.sti.is_some());
        m_cb += usize::from(!m.callbacks.is_empty());
        m_val += usize::from(!m.validations.is_empty());
        m_conc += usize::from(!m.concerns.is_empty());
        m_assoc += usize::from(!m.associations.is_empty());
        for c in &m.callbacks {
            *phases.entry(c.phase.clone()).or_default() += 1;
        }
        for a in &m.associations {
            *akinds.entry(akind(a.kind)).or_default() += 1;
        }
        for v in &m.validations {
            *vkinds.entry(vkind(v.kind)).or_default() += 1;
        }
    }

    let total = assoc + valid + cb + meth + conc + attr + scope + deleg;
    let pct = |x: usize| if total == 0 { 0.0 } else { 100.0 * x as f64 / total as f64 };

    println!("──── {label} AR-DSL coverage (ruff_ruby_spo, {n} models) ────");
    println!("total declarations: {total}");
    for (name, c) in [
        ("associations", assoc), ("validations", valid), ("callbacks", cb),
        ("methods", meth), ("concerns", conc), ("attrs", attr),
        ("scopes", scope), ("delegations", deleg),
    ] {
        println!("  {name:<13} {c:>5}  {:>5.1}%", pct(c));
    }
    println!("── behavioural recipe surface (the fixed enumerable protocol) ──");
    println!("  callback phases used: {} distinct  (total {cb})", phases.len());
    let mut ph: Vec<(&String, &u32)> = phases.iter().collect();
    ph.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (p, c) in ph.iter().take(14) {
        println!("      {p:<26} {c}");
    }
    println!("  validation kinds: {vkinds:?}");
    println!("  association kinds: {akinds:?}");
    println!("── per-model recipe density ──");
    let dpct = |x: usize| 100.0 * x as f64 / n as f64;
    println!("  models w/ associations: {m_assoc} ({:>4.1}%)", dpct(m_assoc));
    println!("  models w/ validations : {m_val} ({:>4.1}%)", dpct(m_val));
    println!("  models w/ callbacks   : {m_cb} ({:>4.1}%)", dpct(m_cb));
    println!("  models w/ concerns    : {m_conc} ({:>4.1}%)", dpct(m_conc));
    println!("  models w/ STI parent  : {sti} ({:>4.1}%)", dpct(sti));

    // ── STI chaining-collapse (mirror of odoo-rs recipe_chaining_collapse) ──
    let names: BTreeSet<String> = g.models.iter().map(|m| m.name.clone()).collect();
    let mut own_full: BTreeMap<String, usize> = BTreeMap::new();
    let mut own_meth: BTreeMap<String, usize> = BTreeMap::new();
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    for m in &g.models {
        let full = m.associations.len() + m.validations.len() + m.callbacks.len()
            + m.functions.len() + m.concerns.len() + m.attributes.len()
            + m.scopes.len() + m.delegations.len();
        own_full.insert(m.name.clone(), full);
        own_meth.insert(m.name.clone(), m.functions.len());
        if let Some(s) = &m.sti {
            if let Some(p) = &s.inherits_from {
                if names.contains(p) {
                    parent.insert(m.name.clone(), p.clone());
                }
            }
        }
    }
    let ancestors = |name: &str| -> Vec<String> {
        let (mut out, mut seen, mut cur) = (vec![], BTreeSet::new(), name.to_string());
        while let Some(p) = parent.get(&cur) {
            if !seen.insert(p.clone()) { break; }
            out.push(p.clone());
            cur = p.clone();
        }
        out
    };
    let chained_full: usize = own_full.values().sum();
    let chained_meth: usize = own_meth.values().sum();
    let (mut naive_full, mut naive_meth, mut with_parent) = (0usize, 0usize, 0usize);
    for m in &g.models {
        let anc = ancestors(&m.name);
        with_parent += usize::from(!anc.is_empty());
        naive_full += own_full[&m.name] + anc.iter().map(|a| own_full[a]).sum::<usize>();
        naive_meth += own_meth[&m.name] + anc.iter().map(|a| own_meth[a]).sum::<usize>();
    }
    let coll = |c: usize, nv: usize| if nv == 0 { 0.0 } else { 100.0 - 100.0 * c as f64 / nv as f64 };
    println!("── STI chaining collapse (in-corpus parents only; concerns are out-of-tree → lower bound) ──");
    println!("  models w/ in-corpus STI parent: {with_parent}");
    println!("  full recipe : chained {chained_full} / naive {naive_full} -> {:.1}% collapse", coll(chained_full, naive_full));
    println!("  methods only: chained {chained_meth} / naive {naive_meth} -> {:.1}% collapse", coll(chained_meth, naive_meth));
}
