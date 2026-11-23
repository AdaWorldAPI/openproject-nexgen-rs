//! The AR arm of the behaviour differential: the callbacks and validations
//! ActiveRecord actually ran when real OpenProject executed
//! `WorkPackages::CreateService` (`oracle/work-package-service-trace.json`),
//! compared with the declarations ruff harvests from the Rails class bodies
//! (`ruff_ruby_spo::events`, ordered, with `if:` / `unless:` / `on:` options).
//!
//! Declarations come from `ruff_ruby_spo::events::source_ore_from` (classes and
//! concern modules, with `included do` bodies, `includes` and `superclass`).
//! Callbacks ActiveRecord generates from association macros come from
//! `ruff_ruby_spo::rails_generated::derive`; they match like declared ones but
//! are counted apart and carry the association and the Rails rule that
//! registered them. Rules `derive` cannot decide are `Slag::GeneratedUndecided`.
//!
//! The test also rebuilds the order Rails registers each chain in (superclass
//! first, `include` at its statement, a concern's dependencies before its
//! `included` block, generated callbacks at their association) and compares it
//! with the order the trace recorded. Differences are printed, not asserted.
//!
//! Engine patches (`patches %i[WorkPackage]`, `include_module ... into:`) are
//! applied from `oracle/intake.json`: ruff finds the calls (`DslCall`), the rules
//! in the data file derive the patch module and when it runs, and its
//! declarations are spliced after the model's own. A call the rules cannot
//! resolve gets a [`PatchSlag`], never a silent skip.
//!
//! The contract arm (`writable:` guards) is a separate differential; see
//! `oracle/README.md` for how the two relate.
//!
//! Every fired callback is either MATCHED to a declaration by
//! `(chain, kind, filter)` or assigned exactly one [`Slag`] reason; every
//! declared callback that never fired is assigned exactly one [`Silent`]
//! reason. Both enums are closed: there is no catch-all variant, and each
//! variant's doc names the stage that would have to supply the missing fact.
//!
//! Needs `OPENPROJECT_SRC` (a checkout at the trace's revision). Without it the
//! test prints UNVERIFIED and passes, unless `REQUIRE_SOURCES=1`, which makes a
//! missing source a failure.
//!
//! Evidence the harvest cannot give (where a filter is defined, which
//! associations exist anywhere in the tree) comes from a plain line scan of the
//! whole checkout. The scan only chooses between slag reasons; it never turns a
//! fired callback into a match.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use op_codegen_pipeline::callback_harvest::*;
use serde_json::Value;

/// Distinct journal entries matched to a declaration, measured on the committed trace.
const MATCHED_FLOOR: usize = 91;

/// Of those, entries that match only through a generated callback.
const MATCHED_GENERATED_FLOOR: usize = 67;

/// Distinct patch modules the intake applies to `WorkPackage` (include only),
/// measured on the checkout at the trace's revision.
const WORK_PACKAGE_PATCHES_FLOOR: usize = 4;

/// Patch calls whose `into:` target ruff renders as `<expr>`. Falls when ruff
/// renders keyword arguments; a rise means the intake found more calls.
const TARGET_UNREADABLE_CEILING: usize = 38;

// ─────────────────────────────────────────────────────────────────────────
// Source access
// ─────────────────────────────────────────────────────────────────────────

fn source() -> Option<PathBuf> {
    match std::env::var("OPENPROJECT_SRC") {
        Ok(s) if !s.is_empty() => Some(PathBuf::from(s)),
        _ => {
            assert!(
                std::env::var("REQUIRE_SOURCES").as_deref() != Ok("1"),
                "REQUIRE_SOURCES=1 but OPENPROJECT_SRC is not set"
            );
            eprintln!("UNVERIFIED: callback_differential needs OPENPROJECT_SRC");
            None
        }
    }
}

fn trace() -> Value {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/work-package-service-trace.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("committed trace"))
        .expect("trace is JSON")
}

/// The distinct fired callbacks over every recorded case, grouped by the
/// journal entry they came from (one entry may expand to several validator
/// attributes). Returns `(entries, distinct)`: `distinct` keys the entry by
/// `(model, phase, label)`.
/// `(model, phase, journal label)`.
type EntryKey = (String, String, String);

fn fired_entries() -> (usize, BTreeMap<EntryKey, Vec<Fired>>) {
    let t = trace();
    let mut total = 0usize;
    let mut distinct = BTreeMap::new();
    for case in t["cases"].as_array().expect("cases") {
        for e in case["journal"].as_array().expect("journal") {
            total += 1;
            let model = e["model"].as_str().expect("model");
            let phase = e["phase"].as_str().expect("phase");
            let label = e["callback"].as_str().expect("callback");
            let key = (model.to_string(), phase.to_string(), label.to_string());
            if let Some(f) = parse_fired(model, phase, label) {
                distinct.insert(key, f);
            }
        }
    }
    (total, distinct)
}

struct Outcome {
    /// distinct journal entries (model, phase, label) matched / total
    matched: usize,
    /// Of `matched`: entries that needed a generated callback for at least one
    /// of their expansions (no source declaration matches it).
    matched_generated: usize,
    /// Provenance of those: entry, association, Rails rule.
    generated_matches: Vec<String>,
    total: usize,
    slag: BTreeMap<Slag, Vec<String>>,
    /// Silent reasons of source-declared callbacks.
    silent: BTreeMap<Silent, Vec<String>>,
    /// Silent reasons of generated callbacks.
    generated_silent: BTreeMap<Silent, Vec<String>>,
    declared_total: usize,
    generated_total: usize,
    undecided_total: usize,
}

fn run(
    ctxs: &BTreeMap<String, ModelCtx>,
    entries: &BTreeMap<EntryKey, Vec<Fired>>,
    ev: &Evidence,
    transform: impl Fn(&Declared) -> Declared,
) -> Outcome {
    // Source declarations first, generated ones after; `n_source` splits them.
    let declared: BTreeMap<&String, (usize, Vec<Declared>)> = ctxs
        .iter()
        .map(|(m, c)| {
            let all: Vec<Declared> = c
                .declared
                .iter()
                .chain(c.generated.iter())
                .map(&transform)
                .collect();
            (m, (c.declared.len(), all))
        })
        .collect();
    let mut used: BTreeMap<&String, BTreeSet<usize>> = BTreeMap::new();
    let mut fired_chains: BTreeMap<&String, BTreeSet<String>> = BTreeMap::new();
    let mut out = Outcome {
        matched: 0,
        matched_generated: 0,
        generated_matches: Vec::new(),
        total: entries.len(),
        slag: BTreeMap::new(),
        silent: BTreeMap::new(),
        generated_silent: BTreeMap::new(),
        declared_total: declared.values().map(|(n, _)| *n).sum(),
        generated_total: declared.values().map(|(n, v)| v.len() - n).sum(),
        undecided_total: ctxs.values().map(|c| c.undecided.len()).sum(),
    };
    for ((model, phase, label), expanded) in entries {
        let ctx = &ctxs[model];
        let (n_source, decls) = &declared[model];
        fired_chains
            .entry(model)
            .or_default()
            .extend(expanded.iter().map(|f| f.chain.clone()));
        let mut all = true;
        let mut via_generated: Vec<String> = Vec::new();
        for f in expanded {
            let hits: Vec<usize> = decls
                .iter()
                .enumerate()
                .filter(|(_, d)| declared_matches(d, f))
                .map(|(i, _)| i)
                .collect();
            if hits.is_empty() {
                all = false;
                let (slag, why) = classify(f, ctx, ev);
                out.slag
                    .entry(slag)
                    .or_default()
                    .push(format!("{model} {phase} {label}  [{why}]"));
                break;
            }
            if hits.iter().all(|i| i >= n_source) {
                if let Some(g) = &decls[hits[0]].generated {
                    via_generated.push(format!(
                        "{model} {phase} {label}  <- :{} ({})",
                        g.association, g.rule
                    ));
                }
            }
            used.entry(model).or_default().extend(hits);
        }
        if all {
            out.matched += 1;
            if let Some(first) = via_generated.into_iter().next() {
                out.matched_generated += 1;
                out.generated_matches.push(first);
            }
        }
    }
    for (model, (n_source, decls)) in &declared {
        let empty = BTreeSet::new();
        let u = used.get(model).unwrap_or(&empty);
        let chains = fired_chains.get(model).cloned().unwrap_or_default();
        for (i, d) in decls.iter().enumerate() {
            if !u.contains(&i) {
                let row = format!(
                    "{model} {}_{} {:?} (from {}{}) options={:?}",
                    d.kind,
                    d.chain,
                    d.filter,
                    d.origin,
                    d.generated.as_ref().map_or(String::new(), |g| format!(
                        ", generated for :{} by {}",
                        g.association, g.rule
                    )),
                    d.options
                );
                let map = if i < *n_source {
                    &mut out.silent
                } else {
                    &mut out.generated_silent
                };
                map.entry(silent_reason(d, &chains)).or_default().push(row);
            }
        }
    }
    out
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "fired (distinct entries): {}", self.total)?;
        writeln!(f, "  matched to a declaration: {}", self.matched)?;
        writeln!(
            f,
            "    of which only a generated callback matches: {}",
            self.matched_generated
        )?;
        for s in Slag::all() {
            writeln!(
                f,
                "  slag {:?}: {}",
                s,
                self.slag.get(s).map_or(0, Vec::len)
            )?;
        }
        writeln!(f, "declared callbacks: {}", self.declared_total)?;
        for s in Silent::all() {
            writeln!(
                f,
                "  declared, never fired, {:?}: {}",
                s,
                self.silent.get(s).map_or(0, Vec::len)
            )?;
        }
        writeln!(
            f,
            "generated callbacks: {} (undecided rules: {})",
            self.generated_total, self.undecided_total
        )?;
        for s in Silent::all() {
            writeln!(
                f,
                "  generated, never fired, {:?}: {}",
                s,
                self.generated_silent.get(s).map_or(0, Vec::len)
            )?;
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The test
// ─────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────
// Order comparison (reported, not asserted)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct OrderReport {
    /// (case, model, phase) groups with at least two matched entries.
    groups: usize,
    /// Groups whose recorded order differs from the reconstructed one.
    differing: usize,
    /// Adjacent pairs recorded in the opposite order to the reconstruction.
    inversions: usize,
    examples: Vec<String>,
}

/// Compare the order the trace recorded, per case and `(model, phase)`, with
/// the rank of each matched entry in `chain_of(model)`. Membership is not in
/// question here: an entry with an expansion that has no declaration is left
/// out of the comparison.
fn order_report<'a>(
    chain_of: impl Fn(&str) -> &'a [Declared],
    show_examples: usize,
) -> OrderReport {
    let t = trace();
    let mut rep = OrderReport::default();
    for case in t["cases"].as_array().expect("cases") {
        let case_name = case["name"].as_str().unwrap_or("?");
        let mut groups: BTreeMap<(String, String), Vec<(String, usize)>> = BTreeMap::new();
        let mut seen: BTreeSet<EntryKey> = BTreeSet::new();
        for e in case["journal"].as_array().expect("journal") {
            let model = e["model"].as_str().expect("model");
            let phase = e["phase"].as_str().expect("phase");
            let label = e["callback"].as_str().expect("callback");
            if !seen.insert((model.into(), phase.into(), label.into())) {
                continue;
            }
            let Some(expanded) = parse_fired(model, phase, label) else {
                continue;
            };
            let chain = chain_of(model);
            let ranks: Option<Vec<usize>> = expanded
                .iter()
                .map(|f| chain.iter().position(|d| declared_matches(d, f)))
                .collect();
            if let Some(r) = ranks.and_then(|r| r.into_iter().min()) {
                groups
                    .entry((model.into(), phase.into()))
                    .or_default()
                    .push((label.into(), r));
            }
        }
        for ((model, phase), seq) in groups {
            if seq.len() < 2 {
                continue;
            }
            rep.groups += 1;
            let inv = seq.windows(2).filter(|w| w[0].1 > w[1].1).count();
            if inv > 0 {
                rep.differing += 1;
                rep.inversions += inv;
                if rep.examples.len() < show_examples {
                    let mut expected = seq.clone();
                    expected.sort_by_key(|(_, r)| *r);
                    let rec: Vec<&str> = seq.iter().map(|(l, _)| l.as_str()).collect();
                    let exp: Vec<&str> = expected.iter().map(|(l, _)| l.as_str()).collect();
                    let at = rec.iter().zip(&exp).position(|(a, b)| a != b).unwrap_or(0);
                    let window = |v: &[&str]| v[at..(at + 3).min(v.len())].join(" | ");
                    rep.examples.push(format!(
                        "{case_name} {model} {phase} ({} entries), first difference at #{at}: recorded [{}] vs reconstructed [{}]",
                        rec.len(),
                        window(&rec),
                        window(&exp)
                    ));
                }
            }
        }
    }
    rep
}

fn print_order(label: &str, r: &OrderReport) {
    eprintln!(
        "order [{label}]: {} groups compared, {} differ, {} adjacent inversions",
        r.groups, r.differing, r.inversions
    );
    for e in &r.examples {
        eprintln!("     {e}");
    }
}

#[test]
fn fired_callbacks_are_matched_or_named_slag() {
    let Some(src) = source() else { return };
    let (entries_total, entries) = fired_entries();
    let models: BTreeSet<String> = entries.keys().map(|(m, _, _)| m.clone()).collect();
    let cfg = intake_cfg();
    let mut h = harvest_for(&src, &cfg);
    let patches = discover_patches(&src, &cfg, &mut h);
    let (ctxs, unresolved) = contexts(&mut h, &models, &patches);
    let ev = scan_tree(&src, &models);

    eprintln!(
        "intake: {} engines, {} patch calls matched by a rule, {} resolved, {} unresolved",
        patches.engines,
        patches.calls,
        patches.applied.len(),
        patches.unresolved.len()
    );
    for model in &models {
        for p in patches.for_model(model) {
            eprintln!(
                "  {model} <- {:?} {} [{}: {} seq {}] {}  ({})",
                p.how, p.patch_name, p.module_dir, p.call, p.seq, p.patch_file, p.cite
            );
        }
    }
    for r in PatchSlag::all() {
        let rows: Vec<&PatchUnresolved> = patches
            .unresolved
            .iter()
            .filter(|u| u.reason == *r)
            .collect();
        eprintln!("  patch slag {r:?}: {}", rows.len());
        for u in rows.iter().take(40) {
            eprintln!("     {} {}  [{}]", u.engine, u.call, u.detail);
        }
    }

    eprintln!(
        "journal entries: {entries_total}; distinct (model, phase, label): {}",
        entries.len()
    );
    for (m, c) in &ctxs {
        eprintln!(
            "  {m}: {} ancestors files, {} declared callbacks, {} generated callbacks, {} undecided rules, {} associations",
            c.ancestor_files.len(),
            c.declared.len(),
            c.generated.len(),
            c.undecided.len(),
            c.assocs.len()
        );
        for u in &c.undecided {
            eprintln!(
                "     undecided :{} (from {}) {} [{}]",
                u.association, u.origin, u.reason, u.rule
            );
        }
    }
    for (from, m) in &unresolved {
        eprintln!("  unresolved include: {from} -> {m}");
    }

    let o = run(&ctxs, &entries, &ev, Clone::clone);
    eprintln!("{o}");
    for (s, rows) in &o.slag {
        eprintln!("-- slag {s:?} ({})", rows.len());
        for r in rows.iter().take(6) {
            eprintln!("     {r}");
        }
    }
    for (s, rows) in &o.silent {
        eprintln!("-- declared, never fired, {s:?} ({})", rows.len());
        for r in rows.iter().take(6) {
            eprintln!("     {r}");
        }
    }
    eprintln!(
        "-- matched only through a generated callback ({})",
        o.matched_generated
    );
    for r in o.generated_matches.iter().take(8) {
        eprintln!("     {r}");
    }
    for (s, rows) in &o.generated_silent {
        eprintln!("-- generated, never fired, {s:?} ({})", rows.len());
        for r in rows.iter().take(6) {
            eprintln!("     {r}");
        }
    }

    // Closed classification: every fired entry is matched or in exactly one slag bucket.
    let slag_total: usize = o.slag.values().map(Vec::len).sum();
    assert_eq!(
        o.matched + slag_total,
        o.total,
        "an entry is neither matched nor classified"
    );
    let silent_total: usize = o.silent.values().map(Vec::len).sum();
    assert!(silent_total <= o.declared_total);
    let gen_silent_total: usize = o.generated_silent.values().map(Vec::len).sum();
    assert!(gen_silent_total <= o.generated_total);

    // Measured at OpenProject 2725194 (trace revision). A drop means the harvest
    // lost declarations; a rise means it gained some: re-measure and move the floor.
    assert!(
        o.matched >= MATCHED_FLOOR,
        "matched {} of {}, floor {MATCHED_FLOOR}",
        o.matched,
        o.total
    );
    assert!(
        o.matched_generated >= MATCHED_GENERATED_FLOOR,
        "matched through generated callbacks: {}, floor {MATCHED_GENERATED_FLOOR}",
        o.matched_generated
    );
    // A declared, unconditioned callback on an exercised chain that never fired
    // would be a real discrepancy, not slag.
    assert!(
        o.generated_silent
            .get(&Silent::Unexplained)
            .is_none_or(Vec::is_empty),
        "generated callbacks that never fired and have no explanation: {:?}",
        o.generated_silent.get(&Silent::Unexplained)
    );
    assert!(
        o.silent.get(&Silent::Unexplained).is_none_or(Vec::is_empty),
        "declared callbacks that never fired and have no explanation: {:?}",
        o.silent.get(&Silent::Unexplained)
    );

    // Negative control: every declared filter, source and generated, spelled
    // backwards: nothing may match.
    let control = run(&ctxs, &entries, &ev, reversed);
    eprintln!(
        "negative control (all reversed): matched {} of {}",
        control.matched, control.total
    );
    assert!(
        control.matched * 50 <= o.matched,
        "the matcher does not discriminate: {} matched with reversed names",
        control.matched
    );
    // Generated callbacks only: the matches that came from source must stay,
    // the ones that needed a generated callback must go.
    let control_gen = run(&ctxs, &entries, &ev, |d| {
        if d.generated.is_some() {
            reversed(d)
        } else {
            d.clone()
        }
    });
    eprintln!(
        "negative control (generated reversed): matched {} of {}, via generated {}",
        control_gen.matched, control_gen.total, control_gen.matched_generated
    );
    assert_eq!(
        control_gen.matched_generated, 0,
        "a generated callback matched with its name reversed"
    );
    assert_eq!(
        control_gen.matched + o.matched_generated,
        o.matched,
        "reversing generated names must drop exactly the generated matches"
    );

    // Attribution: the same run without the engine patches is what ruff alone
    // gives. Patches may only add matches.
    let (bare_ctxs, _) = contexts(&mut h, &models, &PatchIntake::default());
    let bare = run(&bare_ctxs, &entries, &ev, Clone::clone);
    let bare_order = order_report(|m| bare_ctxs[m].effective.as_slice(), 0);
    eprintln!(
        "without the intake: matched {} of {}, via generated {}; order {} groups, {} differ",
        bare.matched, bare.total, bare.matched_generated, bare_order.groups, bare_order.differing
    );
    for s in Slag::all() {
        eprintln!(
            "  without the intake, slag {s:?}: {}",
            bare.slag.get(s).map_or(0, Vec::len)
        );
    }
    assert!(
        o.matched >= bare.matched,
        "the intake lost matches: {} with, {} without",
        o.matched,
        bare.matched
    );
    // Every call a rule matched is applied or typed; none is dropped.
    let typed_before_apply = patches
        .unresolved
        .iter()
        .filter(|u| u.reason != PatchSlag::BodyUnreadable)
        .count();
    assert_eq!(
        patches.calls,
        patches
            .applied
            .iter()
            .map(|p| (p.engine.as_str(), p.call.as_str(), p.seq))
            .collect::<BTreeSet<_>>()
            .len()
            + typed_before_apply,
        "a patch call was neither applied nor given a PatchSlag"
    );
    let count = |r: PatchSlag| patches.unresolved.iter().filter(|u| u.reason == r).count();
    assert_eq!(
        count(PatchSlag::ArgsUnreadable),
        0,
        "a rule did not parse a call"
    );
    assert_eq!(
        count(PatchSlag::PatchFileNotFound),
        0,
        "a patch module the rule derives has no file in the roots"
    );
    assert!(
        count(PatchSlag::TargetUnreadable) <= TARGET_UNREADABLE_CEILING,
        "more unreadable patch targets than measured"
    );
    let wp_patches = patches
        .for_model("WorkPackage")
        .filter(|p| p.how == How::Include)
        .count();
    assert!(
        wp_patches >= WORK_PACKAGE_PATCHES_FLOOR,
        "WorkPackage patches: {wp_patches}, floor {WORK_PACKAGE_PATCHES_FLOOR}"
    );

    // Order: reported, not asserted (yet). Only that the comparison ran.
    let effective = order_report(|m| ctxs[m].effective.as_slice(), 6);
    print_order("reconstructed Rails registration order", &effective);
    let naive: BTreeMap<String, Vec<Declared>> = ctxs
        .iter()
        .map(|(m, c)| {
            (
                m.clone(),
                c.declared
                    .iter()
                    .chain(c.generated.iter())
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    let naive_rep = order_report(|m| naive[m].as_slice(), 3);
    print_order(
        "file order of the walk, generated last (not runtime order)",
        &naive_rep,
    );
    assert!(
        effective.groups > 0,
        "the order comparison compared nothing"
    );
}

#[test]
fn intake_argument_forms() {
    assert_eq!(
        parse_symbol_array("[:PermittedParams,:WorkPackage]"),
        Some(vec!["PermittedParams".into(), "WorkPackage".into()])
    );
    assert_eq!(parse_symbol_array(":A"), None);
    assert_eq!(
        parse_symbol_path(":API, :V3, :Schema"),
        Some(vec!["API".into(), "V3".into(), "Schema".into()])
    );
    assert_eq!(parse_symbol_path(":A, <expr>"), None);
    // What ruff renders today.
    assert!(parse_module_into("\"WorkPackages::Costs\", <expr>").is_err());
    // What a rendered keyword hash would look like.
    assert_eq!(
        parse_module_into("\"WorkPackages::Costs\", {:into: \"WorkPackage\"}"),
        Ok(("WorkPackages::Costs".into(), vec!["WorkPackage".into()]))
    );
    assert_eq!(
        parse_module_into("\"M\", {:into: [\"A\",\"B\"]}").map(|(_, t)| t),
        Ok(vec!["A".into(), "B".into()])
    );
    assert!(parse_module_into("\"M\"").is_err());
    assert_eq!(
        fill(
            "{plugin_module}::Patches::{class}Patch",
            &[("plugin_module", "X::Y"), ("class", "Z")]
        ),
        "X::Y::Patches::ZPatch"
    );
}
