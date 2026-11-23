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

/// Distinct journal entries matched to a declaration, measured on the committed
/// trace at OpenProject 2725194, as `(with vendor/bundle, without it)`. The
/// harvest reads the gems the intake names (the Rails framework, acts_as_list,
/// closure_tree) only where `vendor/bundle` holds them; CI unpacks them with
/// `scripts/fetch-oracle-gems.sh`. Class-level macros are read from their own
/// `def` and `has_and_belongs_to_many` middle associations are decided from the
/// host class, so these hold at ruff b3bdfb1. Without `vendor/bundle` only the
/// in-tree plugin macros (`lib_static`) are read.
const MATCHED_FLOOR: (usize, usize) = (154, 132);

/// Of those, entries that match only through a generated callback, as `(with
/// vendor/bundle, without it)`.
const MATCHED_GENERATED_FLOOR: (usize, usize) = (112, 99);

/// Typed slag per reason as measured at ruff b3bdfb1, and no more: a reason may
/// fall, never grow. Only checked where `vendor/bundle` is installed.
///
/// Left: paper_trail's `has_many :paper_trail_audits`, named by a config value
/// (`config/initializers/paper_trail.rb:31-35`, `has_paper_trail_defaults`
/// `versions: { name: :paper_trail_audits }`) the macro merges at runtime, not
/// by source the harvest reads; and the journals validation, whose `validate:`
/// may come from `**has_many_journals_options`, a method's computed hash
/// (`lib_static/plugins/acts_as_journalized/lib/acts_as_journalized.rb:82`).
const SLAG_CEILINGS: &[(Slag, usize)] = &[
    (Slag::AssociationNotInCheckout, 2),
    (Slag::GeneratedUndecided, 1),
];

/// Order groups that must be DEFINITE and agree with the recording, as
/// `(with vendor/bundle, without it)`. A group is UNKNOWN when a callback in it
/// is registered in a scope with any statement that may hide a registration
/// (`ModelCtx::order_unknown_scopes`, even a conditional whose branches ruff
/// names), or when a skipped branch could register one of its entries
/// (`order_hidden`).
///
/// LOWERED from (29, 19) to (16, 6) by the review fix that stopped treating
/// ruff's `Unharvested::hidden` as exhaustive: a nameable conditional used to
/// clear its scope, which let a group hold an entry registered by a statement
/// ruff does not recognise inside the same branch. 13 groups (the WorkPackage
/// chains through `acts_as_customizable`) are now UNKNOWN; none of them would
/// differ from the recording. The floor goes back up when ruff exposes an
/// exhaustiveness flag on `Unharvested` and the scope check can use it.
const ORDER_DEFINITE_FLOOR: (usize, usize) = (16, 6);

/// Order groups that must stay DEFINITE and agree on the harvest WITHOUT
/// class-level macros: the population the order gate was first measured on.
const ORDER_GROUPS_WITHOUT_MACROS_FLOOR: usize = 19;

/// Callbacks the harvest predicts that never fired in the recording and that
/// the harvest without class-level macros does not predict, as `(with
/// vendor/bundle, without it)`. Each is explained (a chain the cases do not
/// run, an `on:` they do not take, a condition that is not evaluated); an
/// unexplained one fails the test on its own. A rise means a rule predicts
/// more than it did when measured.
const EXTRA_PREDICTIONS_CEILING: (usize, usize) = (16, 5);

/// Distinct patch modules the intake applies to `WorkPackage` (include only),
/// measured on the checkout at the trace's revision (4 before #156 rendered
/// keyword arguments: the 5 `include_module` calls now resolve).
const WORK_PACKAGE_PATCHES_FLOOR: usize = 9;

/// Patch calls whose `into:` target ruff renders as `<expr>`. 38 before #156,
/// which renders keyword arguments; a rise means ruff stopped reading one.
const TARGET_UNREADABLE_CEILING: usize = 0;

/// Order groups compared on the unchanged oracle (OpenProject 2725194).
const ORDER_GROUPS_FLOOR: usize = 19;

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
        let chains = exercised_chains(&fired_chains.get(model).cloned().unwrap_or_default());
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
    /// Of those, groups where an entry is registered in a scope ruff left
    /// statements unread in (`ModelCtx::order_unknown_scopes`). Their order is
    /// reported as UNKNOWN: it is neither confirmed nor counted as a difference.
    unknown: usize,
    /// Unknown groups whose reconstruction (ignoring the unread statements)
    /// still differs from the recorded order.
    unknown_differing: usize,
    /// Definite groups whose recorded order differs from the reconstructed one.
    differing: usize,
    /// Adjacent pairs of definite groups recorded in the opposite order.
    inversions: usize,
    examples: Vec<String>,
    unknown_examples: Vec<String>,
}

impl OrderReport {
    fn definite(&self) -> usize {
        self.groups - self.unknown
    }
}

/// Compare the order the trace recorded, per case and `(model, phase)`, with
/// the rank of each matched entry in `chain_of(model)`. Membership is not in
/// question here: an entry with an expansion that has no declaration is left
/// out of the comparison. A group with an entry whose declaration comes from a
/// module in `unknown_scopes(model)`, or that a skipped branch could register
/// (`hidden(model)`, see `ModelCtx::order_hidden`), is UNKNOWN, not definite.
fn order_report<'a>(
    chain_of: impl Fn(&str) -> &'a [Declared],
    unknown_scopes: impl Fn(&str) -> &'a BTreeSet<String>,
    hidden: impl Fn(&str) -> &'a [Declared],
    show_examples: usize,
) -> OrderReport {
    let t = trace();
    let mut rep = OrderReport::default();
    for case in t["cases"].as_array().expect("cases") {
        let case_name = case["name"].as_str().unwrap_or("?");
        // (label, rank, registered-in-an-unread-scope: the scope's name)
        type Row = (String, usize, Option<String>);
        let mut groups: BTreeMap<(String, String), Vec<Row>> = BTreeMap::new();
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
            if let Some(ranks) = ranks {
                let scopes = unknown_scopes(model);
                let unread_scope = ranks
                    .iter()
                    .map(|r| &chain[*r].origin)
                    .find(|o| scopes.contains(*o))
                    .cloned()
                    .or_else(|| {
                        hidden(model)
                            .iter()
                            .find(|d| expanded.iter().any(|f| declared_matches(d, f)))
                            .map(|d| format!("a skipped branch in {}", d.origin))
                    });
                if let Some(r) = ranks.into_iter().min() {
                    groups
                        .entry((model.into(), phase.into()))
                        .or_default()
                        .push((label.into(), r, unread_scope));
                }
            }
        }
        for ((model, phase), seq) in groups {
            if seq.len() < 2 {
                continue;
            }
            rep.groups += 1;
            let inv = seq.windows(2).filter(|w| w[0].1 > w[1].1).count();
            let scopes: BTreeSet<&str> = seq.iter().filter_map(|r| r.2.as_deref()).collect();
            if !scopes.is_empty() {
                rep.unknown += 1;
                if inv > 0 {
                    rep.unknown_differing += 1;
                }
                if rep.unknown_examples.len() < show_examples {
                    rep.unknown_examples.push(format!(
                        "{case_name} {model} {phase} ({} entries): UNKNOWN order, unread statements in {:?}; reconstruction ignoring them {}",
                        seq.len(),
                        scopes,
                        if inv > 0 { "differs from the recording" } else { "agrees with the recording" }
                    ));
                }
                continue;
            }
            if inv > 0 {
                rep.differing += 1;
                rep.inversions += inv;
                if rep.examples.len() < show_examples {
                    let mut expected = seq.clone();
                    expected.sort_by_key(|(_, r, _)| *r);
                    let rec: Vec<&str> = seq.iter().map(|(l, ..)| l.as_str()).collect();
                    let exp: Vec<&str> = expected.iter().map(|(l, ..)| l.as_str()).collect();
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
        "order [{label}]: {} groups compared: {} definite ({} differ, {} adjacent inversions), {} UNKNOWN ({} of them would differ)",
        r.groups,
        r.definite(),
        r.differing,
        r.inversions,
        r.unknown,
        r.unknown_differing
    );
    for e in &r.examples {
        eprintln!("     {e}");
    }
    for e in &r.unknown_examples {
        eprintln!("     {e}");
    }
}

#[test]
fn fired_callbacks_are_matched_or_named_slag() {
    let Some(src) = source() else { return };
    let (entries_total, entries) = fired_entries();
    let models: BTreeSet<String> = entries.keys().map(|(m, _, _)| m.clone()).collect();
    let cfg = intake_cfg();
    // The framework roots, the gem macros and the gem constants live in
    // `vendor/bundle`. Without it they are unread, and the floors are the
    // ones from before they existed.
    let vendor = src.join("vendor/bundle").is_dir();
    if vendor {
        let bad = verify_macro_cites(&src, &cfg);
        assert!(
            bad.is_empty(),
            "macro rules the checkout does not bear out: {bad:#?}"
        );
    } else {
        eprintln!("UNVERIFIED: no vendor/bundle in the checkout; gem roots, gem macros and their cites are not read");
    }
    let pick = |(with, without): (usize, usize)| if vendor { with } else { without };
    let matched_floor = pick(MATCHED_FLOOR);
    let matched_generated_floor = pick(MATCHED_GENERATED_FLOOR);
    let mut h = harvest_for(&src, &cfg);
    let patches = discover_patches(&src, &cfg, &mut h);
    let (ctxs, unresolved) = contexts(&mut h, &models, &patches);
    let ev = scan_tree(&src, &models);
    let mut logged = BTreeSet::new();
    for l in &h.macro_log {
        if logged.insert(l.clone()) {
            eprintln!("  macro: {l}");
        }
    }

    eprintln!(
        "intake: {} engines, {} patch calls matched by a rule, {} (call, target) jobs, {} applied, {} typed or noted",
        patches.engines,
        patches.calls,
        patches.jobs.len(),
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
            let un = if u.unharvested.is_empty() {
                String::new()
            } else {
                format!("  unharvested {:?}", u.unharvested)
            };
            eprintln!("     {} {}  [{}]{un}", u.engine, u.call, u.detail);
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
    // Every `include` / `prepend` in the harvested chains names a unit the roots
    // find (a unit nested in its parent's file, a Rails gem, a plugin). One that
    // does not is a registration the harvest cannot follow.
    assert!(
        !vendor || unresolved.is_empty(),
        "includes the harvest cannot resolve: {unresolved:?}"
    );

    let o = run(&ctxs, &entries, &ev, Clone::clone);
    eprintln!("{o}");
    for (s, rows) in &o.slag {
        eprintln!("-- slag {s:?} ({})", rows.len());
        for r in rows.iter().take(60) {
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
        o.matched >= matched_floor,
        "matched {} of {}, floor {matched_floor}",
        o.matched,
        o.total
    );
    assert!(
        o.matched_generated >= matched_generated_floor,
        "matched through generated callbacks: {}, floor {matched_generated_floor}",
        o.matched_generated
    );
    if vendor {
        for s in Slag::all() {
            let ceiling = SLAG_CEILINGS
                .iter()
                .find(|(r, _)| r == s)
                .map_or(0, |(_, n)| *n);
            let n = o.slag.get(s).map_or(0, Vec::len);
            assert!(n <= ceiling, "slag {s:?}: {n}, ceiling {ceiling}");
        }
    }
    // Every macro rule runs on the traced models, and no rule or hand-over
    // names a file the checkout does not have.
    if vendor {
        for rule in &cfg.macros.rules {
            assert!(
                h.macro_log
                    .iter()
                    .any(|l| l.contains(&format!("#{} (", rule.def_name))),
                "macro rule `{}` was never expanded on a traced model",
                rule.dsl
            );
        }
        assert!(
            h.macro_log
                .iter()
                .all(|l| !l.contains("is not in the checkout")),
            "a macro names a file the checkout does not have: {:?}",
            h.macro_log
        );
    }
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
    let bare_order = order_report(
        |m| bare_ctxs[m].effective.as_slice(),
        |m| &bare_ctxs[m].order_unknown_scopes,
        |m| bare_ctxs[m].order_hidden.as_slice(),
        0,
    );
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
    // Attribution: the same run without class-level macros. Macros may only add
    // matches, and the order claim made before they existed must still hold on
    // that population: its groups stay definite and agree.
    h.macros_enabled = false;
    let (nomacro_ctxs, _) = contexts(&mut h, &models, &patches);
    h.macros_enabled = true;
    let nomacro = run(&nomacro_ctxs, &entries, &ev, Clone::clone);
    let nomacro_order = order_report(
        |m| nomacro_ctxs[m].effective.as_slice(),
        |m| &nomacro_ctxs[m].order_unknown_scopes,
        |m| nomacro_ctxs[m].order_hidden.as_slice(),
        3,
    );
    eprintln!(
        "without class-level macros: matched {} of {}, via generated {}",
        nomacro.matched, nomacro.total, nomacro.matched_generated
    );
    for s in Slag::all() {
        eprintln!(
            "  without macros, slag {s:?}: {}",
            nomacro.slag.get(s).map_or(0, Vec::len)
        );
    }
    print_order("without class-level macros", &nomacro_order);
    assert!(
        o.matched >= nomacro.matched,
        "macros lost matches: {} with, {} without",
        o.matched,
        nomacro.matched
    );
    // Extra predictions: callbacks that never fired and that only the macros
    // predict. Each carries its silent reason; `Unexplained` is already a
    // failure above, so what is left here is counted and capped.
    let never_fired = |o: &Outcome| -> BTreeMap<String, Silent> {
        o.silent
            .iter()
            .chain(o.generated_silent.iter())
            .flat_map(|(s, rows)| rows.iter().map(move |r| (r.clone(), *s)))
            .collect()
    };
    let without = never_fired(&nomacro);
    let extra: Vec<(String, Silent)> = never_fired(&o)
        .into_iter()
        .filter(|(r, _)| !without.contains_key(r))
        .collect();
    let mut extra_by: BTreeMap<Silent, usize> = BTreeMap::new();
    for (_, s) in &extra {
        *extra_by.entry(*s).or_default() += 1;
    }
    eprintln!(
        "extra predictions (never fired, predicted only with class-level macros): {} {extra_by:?}",
        extra.len()
    );
    for (r, s) in &extra {
        eprintln!("     {s:?}: {r}");
    }
    let extra_ceiling = pick(EXTRA_PREDICTIONS_CEILING);
    assert!(
        extra.len() <= extra_ceiling,
        "{} extra predictions, ceiling {extra_ceiling}",
        extra.len()
    );
    assert!(
        nomacro_order.definite() >= ORDER_GROUPS_WITHOUT_MACROS_FLOOR,
        "without macros: {} definite order groups, floor {ORDER_GROUPS_WITHOUT_MACROS_FLOOR}",
        nomacro_order.definite()
    );
    assert_eq!(
        nomacro_order.differing, 0,
        "without macros the reconstructed order differs from the recording"
    );
    // Conservation per (call, target). A call can name several targets, some
    // applied and some typed, so `calls` (one per call) cannot be compared with
    // applied + typed. Every JOB is accounted for exactly once: applied, or
    // exactly one typed PatchSlag that is not a note on an applied patch. A
    // call that failed before its targets were known is one job with no target.
    let job_set: BTreeSet<&PatchJob> = patches.jobs.iter().collect();
    assert_eq!(
        job_set.len(),
        patches.jobs.len(),
        "a patch job was recorded twice"
    );
    assert_eq!(
        patches
            .jobs
            .iter()
            .map(|j| (j.engine.as_str(), j.call.as_str(), j.seq))
            .collect::<BTreeSet<_>>()
            .len(),
        patches.calls,
        "a rule-matched call has no job, or a job has no call"
    );
    let mut applied_n: BTreeMap<PatchJob, usize> = BTreeMap::new();
    for p in &patches.applied {
        *applied_n
            .entry(PatchJob {
                engine: p.engine.clone(),
                call: p.call.clone(),
                seq: p.seq,
                target: Some(p.target.clone()),
            })
            .or_default() += 1;
    }
    let mut typed_n: BTreeMap<(String, u32, Option<String>), usize> = BTreeMap::new();
    for u in patches
        .unresolved
        .iter()
        .filter(|u| !u.reason.is_applied_with_note())
    {
        *typed_n
            .entry((u.engine.clone(), u.seq, u.target.clone()))
            .or_default() += 1;
    }
    for j in &patches.jobs {
        let a = applied_n.get(j).copied().unwrap_or(0);
        let t = typed_n
            .get(&(j.engine.clone(), j.seq, j.target.clone()))
            .copied()
            .unwrap_or(0);
        assert_eq!(a + t, 1, "job {j:?}: applied {a} times, typed {t} times");
    }
    assert_eq!(
        applied_n.len() + typed_n.len(),
        patches.jobs.len(),
        "an applied patch or a typed entry belongs to no job"
    );
    assert_eq!(applied_n.values().sum::<usize>(), patches.applied.len());
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
    // The ceiling is 0, and clippy rejects `<= 0` on an unsigned count, so equality.
    assert_eq!(
        count(PatchSlag::TargetUnreadable),
        TARGET_UNREADABLE_CEILING,
        "unreadable patch targets: ruff stopped rendering a keyword argument"
    );
    let wp_patches = patches
        .for_model("WorkPackage")
        .filter(|p| p.how == How::Include)
        .count();
    assert!(
        wp_patches >= WORK_PACKAGE_PATCHES_FLOOR,
        "WorkPackage patches: {wp_patches}, floor {WORK_PACKAGE_PATCHES_FLOOR}"
    );

    // The patch notes (`BodyUnreadable`, `BodyPartiallyRead`) are derived in the
    // intake from the patch module's own ore. Re-derive them from the model
    // context (`ModelCtx::modules`, built from the linearized ancestors) and
    // require the same answer both ways: a module with unharvested statements and
    // some declarations is Partial and has a `BodyPartiallyRead` note; one with
    // none read is Unread and has a `BodyUnreadable` note; one read in full has
    // no note. Nothing else may carry either note.
    let noted = |engine: &str, seq: u32, target: &str, r: PatchSlag| {
        patches.unresolved.iter().any(|u| {
            u.reason == r
                && u.engine == engine
                && u.seq == seq
                && u.target.as_deref() == Some(target)
        })
    };
    let (mut chk_full, mut chk_partial, mut chk_unread) = (0usize, 0usize, 0usize);
    for p in patches.applied.iter().filter(|p| p.how == How::Include) {
        let Some(ctx) = ctxs.get(&p.target) else {
            continue;
        };
        let mr = ctx
            .modules
            .iter()
            .find(|m| m.name == p.patch_name)
            .expect("an applied patch is in its model's chain");
        let partial = noted(&p.engine, p.seq, &p.target, PatchSlag::BodyPartiallyRead);
        let unread = noted(&p.engine, p.seq, &p.target, PatchSlag::BodyUnreadable);
        match mr.completeness {
            Completeness::Full => {
                chk_full += 1;
                assert!(
                    !partial && !unread,
                    "{} is read in full but noted",
                    p.patch_name
                );
            }
            Completeness::Partial => {
                chk_partial += 1;
                assert!(
                    partial && !unread,
                    "{} has unharvested statements {:?} and some declarations but no BodyPartiallyRead",
                    p.patch_name,
                    mr.unharvested
                );
            }
            Completeness::Unread => {
                chk_unread += 1;
                assert!(
                    unread && !partial,
                    "{} has unharvested statements {:?} and no declaration but no BodyUnreadable",
                    p.patch_name,
                    mr.unharvested
                );
            }
            Completeness::NoIncludeTimeBody => {
                assert!(
                    !partial,
                    "{} has nothing unread but is partial",
                    p.patch_name
                );
            }
        }
    }
    assert!(
        chk_full + chk_partial + chk_unread > 0,
        "the patch-note cross-check compared nothing"
    );
    eprintln!(
        "patch notes cross-checked against the model chains: {chk_full} full, {chk_partial} partial, {chk_unread} unread"
    );

    // MEASURE (a): callback coverage. How many of the fired callbacks have a
    // declaration (written or generated) and why the rest do not.
    eprintln!(
        "MEASURE (a) callback coverage: {} of {} fired entries matched ({} from source declarations, {} only through a generated callback); {} typed slag",
        o.matched,
        o.total,
        o.matched - o.matched_generated,
        o.matched_generated,
        slag_total
    );
    for s in Slag::all() {
        let n = o.slag.get(s).map_or(0, Vec::len);
        if n > 0 {
            eprintln!("    slag {s:?}: {n}");
        }
    }

    // MEASURE (b): extraction completeness. Independent of (a): an entry can
    // match a name through a module whose include-time bodies were only partly
    // read. Counted per model over every module (concern or patch) in the chain.
    // Class bodies are not covered: `ClassOre::unharvested` is empty for a class.
    let mut by_completeness: BTreeMap<Completeness, usize> = BTreeMap::new();
    let mut by_reason: BTreeMap<String, usize> = BTreeMap::new();
    for (m, c) in &ctxs {
        eprintln!(
            "MEASURE (b) extraction completeness, {m}: {} modules in the chain",
            c.modules.len()
        );
        for mr in &c.modules {
            *by_completeness.entry(mr.completeness).or_default() += 1;
            for (r, n) in &mr.unharvested {
                *by_reason.entry(r.clone()).or_default() += n;
            }
            if matches!(
                mr.completeness,
                Completeness::Partial | Completeness::Unread
            ) {
                eprintln!(
                    "    {:?} {} [{}] {} include-time declarations, unharvested {:?}  ({})",
                    mr.completeness,
                    mr.name,
                    if mr.is_patch { "patch" } else { "concern" },
                    mr.included_decls,
                    mr.unharvested,
                    mr.file
                );
            }
        }
        for d in &c.order_hidden {
            eprintln!(
                "    a skipped branch in {} could register {} {} {:?}",
                d.origin, d.kind, d.chain, d.filter
            );
        }
        if !c.order_unknown_scopes.is_empty() {
            eprintln!(
                "    order-unknown scopes (unread ControlFlow / DynamicSend / UnreadBlockBody): {:?}",
                c.order_unknown_scopes
            );
        }
    }
    for k in Completeness::all() {
        eprintln!(
            "MEASURE (b) total {k:?}: {}",
            by_completeness.get(k).copied().unwrap_or(0)
        );
    }
    eprintln!("MEASURE (b) unharvested statements per reason: {by_reason:?}");

    // The UNKNOWN order logic must be able to fire and able to stay silent.
    // Can fire: with every origin of the chain declared an unread scope, every
    // group is unknown, none is called definite, and the differences move to
    // `unknown_differing` unchanged. Stays silent: when no scope hides a
    // registration, no group is unknown.
    let all_origins: BTreeSet<String> = ctxs
        .values()
        .flat_map(|c| c.effective.iter().map(|d| d.origin.clone()))
        .collect();
    let forced = order_report(
        |m| ctxs[m].effective.as_slice(),
        |_| &all_origins,
        |_| &[],
        0,
    );
    let plain = order_report(
        |m| ctxs[m].effective.as_slice(),
        |m| &ctxs[m].order_unknown_scopes,
        |m| ctxs[m].order_hidden.as_slice(),
        0,
    );
    assert_eq!(
        forced.unknown, forced.groups,
        "an unread scope did not make its groups unknown"
    );
    assert_eq!(
        forced.differing, 0,
        "an unknown group was counted as a definite difference"
    );
    assert_eq!(
        forced.unknown_differing,
        plain.differing + plain.unknown_differing,
        "marking scopes unknown changed what the reconstruction says"
    );
    // The same for the named skipped branches: a branch that could register
    // every callback of the chain makes every group unknown.
    let no_scope = BTreeSet::new();
    let forced_hidden = order_report(
        |m| ctxs[m].effective.as_slice(),
        |_| &no_scope,
        |m| ctxs[m].effective.as_slice(),
        0,
    );
    assert_eq!(
        forced_hidden.unknown, forced_hidden.groups,
        "a skipped branch that names an entry did not make its group unknown"
    );
    if ctxs
        .values()
        .all(|c| c.order_unknown_scopes.is_empty() && c.order_hidden.is_empty())
    {
        assert_eq!(
            plain.unknown, 0,
            "a group is unknown though no scope hides a registration"
        );
    }

    // Order. A group with a callback registered in a scope that has unread
    // statements is UNKNOWN. Definite groups must agree with the recording;
    // the naive walk order is the negative control showing the comparison can
    // see a difference at all.
    let effective = order_report(
        |m| ctxs[m].effective.as_slice(),
        |m| &ctxs[m].order_unknown_scopes,
        |m| ctxs[m].order_hidden.as_slice(),
        8,
    );
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
    let no_unknown = BTreeSet::new();
    let naive_rep = order_report(|m| naive[m].as_slice(), |_| &no_unknown, |_| &[], 3);
    print_order(
        "file order of the walk, generated last (not runtime order)",
        &naive_rep,
    );
    assert!(
        effective.groups >= ORDER_GROUPS_FLOOR,
        "fewer order groups compared than measured"
    );
    let order_definite_floor = pick(ORDER_DEFINITE_FLOOR);
    assert!(
        effective.definite() >= order_definite_floor,
        "{} definite order groups, floor {order_definite_floor}",
        effective.definite()
    );
    assert_eq!(
        effective.differing, 0,
        "the reconstructed registration order differs from the recording"
    );
    // A group made UNKNOWN by an unread conditional is not a difference, but it
    // must not be one either: its reconstruction, ignoring what was unread,
    // still has to agree with the recording.
    assert_eq!(
        effective.unknown_differing, 0,
        "an UNKNOWN order group would differ from the recording"
    );
    assert!(
        naive_rep.differing > 0,
        "the naive order agrees everywhere, so the order comparison proves nothing"
    );
}

/// The cite check is a check: a wrong line, a wrong needle and a missing file
/// are each reported, and an untouched rule is not.
#[test]
fn macro_cites_are_checked_against_the_checkout() {
    let Some(src) = source() else { return };
    let ok = intake_cfg();
    // The in-tree plugin rules need no vendored gem.
    let in_tree: Vec<_> = ok
        .macros
        .rules
        .iter()
        .filter(|r| r.file.starts_with("lib_static/"))
        .map(|r| r.dsl.clone())
        .collect();
    assert!(!in_tree.is_empty());
    let bad = verify_macro_cites(&src, &ok);
    assert!(
        bad.iter()
            .all(|b| !in_tree.iter().any(|d| b.starts_with(d.as_str()))),
        "an untouched in-tree rule was reported: {bad:?}"
    );
    for (what, edit) in [("needle", 0usize), ("line", 1), ("file", 2), ("def", 3)] {
        let mut cfg = intake_cfg();
        let rule = cfg
            .macros
            .rules
            .iter_mut()
            .find(|r| r.dsl == "acts_as_attachable")
            .expect("rule");
        match edit {
            0 => rule.verify[0].contains = "not in that line".into(),
            1 => rule.verify[0].line += 1,
            2 => rule.verify[0].file = "lib_static/plugins/nope/init.rb".into(),
            _ => rule.def_name = "acts_as_nothing".into(),
        }
        let bad = verify_macro_cites(&src, &cfg);
        assert!(
            bad.iter().any(|b| b.starts_with("acts_as_attachable")),
            "a wrong {what} was not reported: {bad:?}"
        );
    }
}

/// `validate` counts as run only when both `validation` and `save` fired:
/// either alone does not show that `valid?` reached `run_validations!`.
#[test]
fn validate_is_exercised_only_when_validation_and_save_fired() {
    let set = |c: &[&str]| -> BTreeSet<String> { c.iter().map(|s| (*s).to_string()).collect() };
    assert!(exercised_chains(&set(&["validation", "save"])).contains("validate"));
    assert!(!exercised_chains(&set(&["validation"])).contains("validate"));
    assert!(!exercised_chains(&set(&["save", "create"])).contains("validate"));
    assert_eq!(
        exercised_chains(&set(&["save", "create"])),
        set(&["save", "create"])
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
    // What ruff rendered before #156: an unreadable keyword argument.
    assert!(parse_module_into("\"WorkPackages::Costs\", <expr>").is_err());
    // What ruff renders now (#156): a braced hash.
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

// ─────────────────────────────────────────────────────────────────────────
// Macro option splats
// ─────────────────────────────────────────────────────────────────────────

mod splats {
    use op_codegen_pipeline::callback_harvest::{resolve_splats, OP_RAILS_DEFAULTS};
    use ruff_ruby_spo::events::{DeclOrigin, RailsEvent, RailsKind};
    use ruff_ruby_spo::rails_generated::{derive, Filter, Generated};
    use ruff_ruby_spo::Declaration;
    use ruff_spo_triplet::{AssocDecl, AssocKind, OptionSplat};

    fn assoc(options: &[(&str, &str)], splats: &[(&str, usize)]) -> AssocDecl {
        AssocDecl {
            kind: AssocKind::HasMany,
            name: "attachments".into(),
            options: options
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            option_splats: splats
                .iter()
                .map(|(s, b)| OptionSplat {
                    source: (*s).into(),
                    before: *b,
                })
                .collect(),
        }
    }

    fn call(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    fn keys(a: &AssocDecl) -> Vec<&str> {
        a.options.iter().map(|(k, _)| k.as_str()).collect()
    }

    fn last<'a>(a: &'a AssocDecl, key: &str) -> Option<&'a str> {
        a.options
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn the_options_parameter_stands_for_the_calls_options() {
        let mut a = assoc(&[], &[("**options", 0)]);
        resolve_splats(
            &mut a,
            "options",
            Some(&call(&[("order", "\"f\""), ("extract_tsv", "true")])),
        );
        assert_eq!(keys(&a), ["order", "extract_tsv"]);
        assert!(a.option_splats.is_empty());
    }

    /// `reverse_merge!` keeps the call's value where both give a key, `merge`
    /// takes the argument's: the later pair wins in the result.
    #[test]
    fn merge_and_reverse_merge_decide_which_value_wins() {
        let c = call(&[("as", ":mine"), ("order", "1")]);
        let mut rm = assoc(
            &[],
            &[(
                "**options.reverse_merge!(as: :container, dependent: :destroy)",
                0,
            )],
        );
        resolve_splats(&mut rm, "options", Some(&c));
        assert_eq!(last(&rm, "as"), Some(":mine"));
        assert_eq!(last(&rm, "dependent"), Some(":destroy"));
        assert_eq!(last(&rm, "order"), Some("1"));

        let mut m = assoc(&[], &[("**options.merge(as: :container)", 0)]);
        resolve_splats(&mut m, "options", Some(&c));
        assert_eq!(last(&m, "as"), Some(":container"));
    }

    /// Explicit options keep their place around the resolved ones, and a splat
    /// that stays keeps its position among them.
    #[test]
    fn explicit_options_keep_their_place_around_a_resolved_splat() {
        let mut a = assoc(
            &[("a", "1"), ("b", "2"), ("c", "3")],
            &[("**options", 1), ("**unknown", 2)],
        );
        resolve_splats(&mut a, "options", Some(&call(&[("x", "9")])));
        assert_eq!(keys(&a), ["a", "x", "b", "c"]);
        assert_eq!(a.option_splats.len(), 1);
        assert_eq!(a.option_splats[0].source, "**unknown");
        assert_eq!(a.option_splats[0].before, 3);
    }

    /// The silence twin: every splat the rule cannot read is left alone, so the
    /// association's rules still report it undecided.
    #[test]
    fn a_splat_it_cannot_read_stays() {
        let c = call(&[("order", "1")]);
        for source in [
            "**other",
            "**options_more",
            "**options.merge(defaults)",
            "**options.merge(:as => :container)",
            "**options.slice(:order)",
            "**options.merge(as: :container).merge(x: 1)",
            "**<expr>",
        ] {
            let mut a = assoc(&[("a", "1")], &[(source, 1)]);
            resolve_splats(&mut a, "options", Some(&c));
            assert_eq!(keys(&a), ["a"], "{source}");
            assert_eq!(a.option_splats.len(), 1, "{source}");
        }
        // Without the call's options nothing is known about the splat.
        let mut a = assoc(&[], &[("**options", 0)]);
        resolve_splats(&mut a, "options", None);
        assert_eq!(a.option_splats.len(), 1);
    }

    fn collection_validation(call_options: &[(&str, &str)]) -> (bool, bool) {
        let mut a = assoc(
            &[],
            &[(
                "**options.reverse_merge!(as: :container, dependent: :destroy)",
                0,
            )],
        );
        resolve_splats(&mut a, "options", Some(&call(call_options)));
        let events = [RailsEvent {
            seq: 0,
            kind: RailsKind::Association,
            decl: Declaration::Association(a),
            anchor: 0,
            origin: DeclOrigin::Body,
        }];
        let got = derive(&events, OP_RAILS_DEFAULTS);
        let validates = got.iter().any(|g| {
            matches!(g, Generated::Callback(c)
                if matches!(&c.filter, Filter::Method(m) if m.starts_with("validate_associated_records_for_")))
        });
        let undecided = got.iter().any(|g| matches!(g, Generated::Undecided(_)));
        (validates, undecided)
    }

    /// Resolved, the association is decided: a call that gives no `validate:`
    /// leaves the collection's validation callback, one that gives
    /// `validate: false` removes it. Unresolved, it is undecided either way.
    #[test]
    fn a_resolved_splat_decides_the_validation_callback_both_ways() {
        assert_eq!(collection_validation(&[("order", "1")]), (true, false));
        assert_eq!(
            collection_validation(&[("validate", "false")]),
            (false, false)
        );
        assert_eq!(
            collection_validation(&[("validate", "true")]),
            (true, false)
        );
        // A value the rule cannot read as a literal is undecided, not guessed.
        assert_eq!(
            collection_validation(&[("validate", "flag?")]),
            (false, true)
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Review fixes: unsound DEFINITE order, dropped trailing unread statements,
// undecided option splats, cite parsing. Each test runs on a throwaway tree.
// ─────────────────────────────────────────────────────────────────────────

mod review_fixes {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use op_codegen_pipeline::callback_harvest::{
        call_arg_text, contexts, Ancestor, Harvest, IntakeCfg, MacroRule, PatchIntake,
    };
    use ruff_ruby_spo::events::UnharvestedReason;
    use ruff_ruby_spo::Declaration;

    /// A throwaway checkout: `files` are `(relative path, text)`.
    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str, files: &[(&str, &str)]) -> Tree {
            // Unique per tree: the tests of this module run in parallel.
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("nx-p1-{name}-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for (rel, text) in files {
                let path = root.join(rel);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            Tree(root)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn harvest(tree: &Tree) -> Harvest {
        let mut h = Harvest::new(tree.path(), vec!["app".to_string()]);
        h.macros.push(MacroRule {
            dsl: "acts_as_demo".into(),
            file: "lib/mac.rb".into(),
            def_name: "acts_as_demo".into(),
            cite: "test".into(),
            verify: Vec::new(),
        });
        h
    }

    // ── finding 1: a hiding scope is never definite ─────────────────────

    const CONCERN: &str = "module Concern\n  extend ActiveSupport::Concern\n  included do\n    if cond\n      after_save :a\n      send(:before_save, :b)\n    end\n  end\nend\n";

    #[test]
    fn a_scope_with_a_nameable_conditional_stays_order_unknown() {
        let tree = Tree::new(
            "unknown",
            &[
                ("app/host.rb", "class Host\n  include Concern\nend\n"),
                ("app/concern.rb", CONCERN),
            ],
        );
        let mut h = harvest(&tree);
        let models: BTreeSet<String> = ["Host".to_string()].into();
        let (ctxs, _) = contexts(&mut h, &models, &PatchIntake::default());
        let ctx = &ctxs["Host"];
        // Anti-vacuity: ruff does name `after_save :a`, so the OLD rule (every
        // hiding statement nameable => scope definite) would have cleared it.
        let anc: Vec<Ancestor> = h.linearize("Host");
        let concern = anc.iter().find(|a| a.name == "Concern").expect("Concern");
        let u = concern
            .ore
            .unharvested
            .iter()
            .find(|u| u.reason == UnharvestedReason::ControlFlow)
            .expect("the conditional is listed unharvested");
        assert!(
            !u.hidden.is_empty()
                && u.hidden
                    .iter()
                    .all(|e| matches!(e.decl, Declaration::Callback(_))),
            "fixture: ruff names the callback inside the branch"
        );
        assert!(
            ctx.order_unknown_scopes.contains("Concern"),
            "a scope that hides a registration must not be definite, though ruff named {:?}",
            u.hidden.len()
        );
    }

    #[test]
    fn a_scope_without_hiding_statements_stays_definite() {
        let tree = Tree::new(
            "definite",
            &[
                ("app/host.rb", "class Host\n  include Concern\nend\n"),
                (
                    "app/concern.rb",
                    "module Concern\n  extend ActiveSupport::Concern\n  included do\n    after_save :a\n  end\nend\n",
                ),
            ],
        );
        let mut h = harvest(&tree);
        let models: BTreeSet<String> = ["Host".to_string()].into();
        let (ctxs, _) = contexts(&mut h, &models, &PatchIntake::default());
        assert!(
            ctxs["Host"].order_unknown_scopes.is_empty(),
            "{:?}",
            ctxs["Host"].order_unknown_scopes
        );
    }

    // ── finding 2: trailing unread statements survive the final flush ───

    const MAC_TRAILING: &str = "module Mac\n  def acts_as_demo(opts = {})\n    Definer.call(self)\n    if opts[:x]\n      after_save :y\n    end\n  end\nend\n";

    #[test]
    fn unread_statements_after_a_hand_over_are_kept() {
        let tree = Tree::new(
            "trailing",
            &[
                ("app/host.rb", "class Host\n  acts_as_demo\nend\n"),
                ("lib/mac.rb", MAC_TRAILING),
                (
                    "app/definer.rb",
                    "module Definer\n  def self.call(base)\n    base.before_save :z\n  end\nend\n",
                ),
            ],
        );
        let mut h = harvest(&tree);
        let anc = h.linearize("Host");
        let kept: usize = anc
            .iter()
            .filter(|a| a.name.starts_with("Mac#"))
            .flat_map(|a| a.ore.unharvested.iter())
            .filter(|u| u.reason == UnharvestedReason::ControlFlow)
            .count();
        assert_eq!(
            kept, 1,
            "the `if opts[:x]` after the hand-over must reach a run's unharvested; log: {:?}",
            h.macro_log
        );
    }

    // ── finding 3: option splats of a macro call ────────────────────────

    const MAC_SPLAT: &str = "module Mac\n  def acts_as_demo(options = {})\n    has_many :things, **options\n  end\nend\n";

    /// Whether the macro's `**options` splat was resolved from the call.
    fn splat_resolved(call: &str) -> bool {
        let tree = Tree::new(
            "splat",
            &[
                ("app/host.rb", &format!("class Host\n  {call}\nend\n")),
                ("lib/mac.rb", MAC_SPLAT),
            ],
        );
        let mut h = harvest(&tree);
        let anc = h.linearize("Host");
        let mut seen = 0;
        let mut resolved = true;
        for a in anc.iter().filter(|a| a.name.starts_with("Mac#")) {
            for e in &a.ore.declarations {
                if let Declaration::Association(assoc) = &e.decl {
                    seen += 1;
                    resolved &= assoc.option_splats.is_empty();
                }
            }
        }
        assert_eq!(seen, 1, "fixture: one expanded association for {call:?}");
        resolved
    }

    #[test]
    fn a_call_with_only_literal_pairs_decides_the_splat() {
        assert!(splat_resolved("acts_as_demo"));
        assert!(splat_resolved("acts_as_demo order: :position"));
        assert!(splat_resolved("acts_as_demo(order: :position, a: 1)"));
        assert!(splat_resolved("acts_as_demo a: 1,\n    b: 2"));
    }

    #[test]
    fn a_call_ruff_cannot_read_in_full_leaves_the_splat_undecided() {
        assert!(!splat_resolved("acts_as_demo **opts"), "splat argument");
        assert!(
            !splat_resolved("acts_as_demo(some_hash)"),
            "non-hash argument"
        );
        assert!(
            !splat_resolved("acts_as_demo a: 1, **more"),
            "pair then splat"
        );
        assert!(!splat_resolved("acts_as_demo :a => 1"), "hash rocket");
    }

    #[test]
    fn call_arguments_are_read_from_the_source() {
        let args = |t: &str| call_arg_text(t, 0);
        assert_eq!(args("foo").as_deref(), Some(""));
        assert_eq!(args("foo a: 1\nbar").as_deref(), Some("a: 1"));
        assert_eq!(
            args("foo(a: (1), b: 2) # x").as_deref(),
            Some("a: (1), b: 2")
        );
        assert_eq!(args("foo a: 1 # c\nbar").as_deref(), Some("a: 1"));
        // A comment after a trailing comma does not end the call, and does not
        // leak into the arguments.
        let joined = args("foo a: 1, # c\n b: 2\nbar").unwrap();
        assert_eq!(
            joined.split_whitespace().collect::<Vec<_>>(),
            ["a:", "1,", "b:", "2"]
        );
        assert_eq!(args("foo a: 1,\n b: 2\nbar").unwrap().lines().count(), 2);
        assert_eq!(args("foo(a: 1"), None, "unclosed");
        assert_eq!(call_arg_text("foo", 99), None, "anchor outside the text");
    }

    // ── finding 4: cites ────────────────────────────────────────────────

    fn cfg_with(file: &str, def: &str, line: u32) -> IntakeCfg {
        let json = format!(
            r#"{{"roots":{{"list":[]}},"engines":{{"search":[],"file_name":"x","superclass":[]}},
            "engine_order":{{"file":"x","line_marker":"a","line_marker_end":"b"}},
            "patch_rules":[],"phase":{{"applies":"after_class_body"}},
            "macros":{{"rules":[{{"dsl":"d","file":"{file}","def":"{def}","cite":"c",
            "verify":[{{"file":"{file}","line":{line},"contains":"x"}}]}}]}}}}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn a_cite_on_line_zero_is_reported_not_a_panic() {
        let tree = Tree::new("cite0", &[("lib/m.rb", "def foo\nend\n")]);
        let bad = op_codegen_pipeline::callback_harvest::verify_macro_cites(
            tree.path(),
            &cfg_with("lib/m.rb", "foo", 0),
        );
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("does not contain"), "{bad:?}");
    }

    #[test]
    fn a_def_cite_matches_the_whole_method_name() {
        let only_longer = Tree::new("cite-long", &[("lib/m.rb", "def acts_as_list_item\nend\n")]);
        let bad = op_codegen_pipeline::callback_harvest::verify_macro_cites(
            only_longer.path(),
            &cfg_with("lib/m.rb", "acts_as_list", 1),
        );
        assert!(
            bad.iter().any(|b| b.contains("def acts_as_list")),
            "`def acts_as_list_item` must not satisfy `def acts_as_list`: {bad:?}"
        );
        let exact = Tree::new(
            "cite-exact",
            &[(
                "lib/m.rb",
                "def acts_as_list(opts)\nend\ndef self.other\nend\n",
            )],
        );
        let bad = op_codegen_pipeline::callback_harvest::verify_macro_cites(
            exact.path(),
            &cfg_with("lib/m.rb", "acts_as_list", 1),
        );
        assert!(
            !bad.iter().any(|b| b.contains("def acts_as_list")),
            "the exact name is found: {bad:?}"
        );
    }
}
