//! Gate 2, first slice: OpenProject's `writable:` guards, lowered by ruff
//! (`ruff_ruby_spo::extract_contract_guards`) from the Rails source, evaluated
//! over the facts the contract oracle recorded, and compared with the
//! `writable?` answers real OpenProject gave (`oracle/work-package-contract-trace.json`).
//!
//! Nothing is hand-written except the fact alias table: the oracle names a fact
//! by the expression it evaluated (`leaf?`), the lowering by the model call it
//! reads (`model.leaf?`). A guard reading a fact the oracle did not record is
//! counted UNRECORDED and not compared.
//!
//! Needs `OPENPROJECT_SRC` (a checkout at the trace's revision). Without it the
//! test prints UNVERIFIED and passes, unless `REQUIRE_SOURCES=1`, which makes a
//! missing source a failure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ruff_ruby_spo::{extract_contract_guards, ContractGuard, GuardExpr, GuardOutcome};
use serde_json::Value;

/// Lowered fact name -> fact key in the oracle trace.
const FACT_ALIASES: &[(&str, &str)] = &[
    ("model.leaf?", "leaf?"),
    ("model.schedule_manually?", "schedule_manually"),
    ("model.status.closed?", "status.closed?"),
    // ruff keeps safe navigation in fact names (`status&.closed?`) since the
    // CodeRabbit fix on ruff #151; both spellings name the same oracle fact.
    ("model.status&.closed?", "status.closed?"),
    (
        "model.effective_target_versions.any?(&:closed?)",
        "effective_target_versions.any?(&:closed?)",
    ),
    ("model.status_id_change", "status_id_change_present"),
];

const BASE: &str = "WorkPackages::BaseContract";

fn source() -> Option<PathBuf> {
    match std::env::var("OPENPROJECT_SRC") {
        Ok(s) if !s.is_empty() => Some(PathBuf::from(s)),
        _ => {
            assert!(
                std::env::var("REQUIRE_SOURCES").as_deref() != Ok("1"),
                "REQUIRE_SOURCES=1 but OPENPROJECT_SRC is not set"
            );
            eprintln!("UNVERIFIED: OPENPROJECT_SRC unset; guard differential not run");
            None
        }
    }
}

fn trace() -> Value {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/work-package-contract-trace.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("committed trace"))
        .expect("trace is JSON")
}

/// `Some(value)` when every fact the expression reads was recorded.
fn eval(e: &GuardExpr, facts: &BTreeMap<String, Value>) -> Option<bool> {
    Some(match e {
        GuardExpr::Const(b) => *b,
        GuardExpr::Fact(name) => {
            let key = FACT_ALIASES.iter().find(|(f, _)| f == name)?.1;
            match facts.get(key)? {
                Value::Bool(b) => *b,
                Value::Null => false,
                _ => return None,
            }
        }
        GuardExpr::Not(a) => !eval(a, facts)?,
        GuardExpr::And(a, b) => eval(a, facts)? && eval(b, facts)?,
        GuardExpr::Or(a, b) => eval(a, facts)? || eval(b, facts)?,
    })
}

/// The negative control: every top-level `Or` becomes `And` and vice versa.
fn swap_top(e: &GuardExpr) -> GuardExpr {
    match e {
        GuardExpr::Or(a, b) => GuardExpr::And(a.clone(), b.clone()),
        GuardExpr::And(a, b) => GuardExpr::Or(a.clone(), b.clone()),
        GuardExpr::Not(a) => GuardExpr::Not(Box::new(swap_top(a))),
        other => other.clone(),
    }
}

/// The guard that applies to `attr` under `contract`: the contract's own
/// declaration, else the base contract's.
fn guard_for<'a>(
    guards: &'a [ContractGuard],
    contract: &str,
    attr: &str,
) -> Option<&'a ContractGuard> {
    guards
        .iter()
        .find(|g| g.class == contract && g.attribute == attr)
        .or_else(|| {
            guards
                .iter()
                .find(|g| g.class == BASE && g.attribute == attr)
        })
}

struct Tally {
    compared: usize,
    mismatches: Vec<String>,
    unrecorded: usize,
}

fn run(guards: &[ContractGuard], transform: impl Fn(&GuardExpr) -> GuardExpr) -> Tally {
    let mut t = Tally {
        compared: 0,
        mismatches: Vec::new(),
        unrecorded: 0,
    };
    for rec in trace()["cases"].as_array().expect("cases") {
        // A user without permissions gets `false` from the permission check before
        // any guard is consulted; those records test permissions, not guards.
        if rec["user"] == "member_no_permissions" {
            continue;
        }
        let contract = rec["contract"].as_str().expect("contract");
        let facts: BTreeMap<String, Value> = rec["facts"]
            .as_object()
            .expect("facts")
            .clone()
            .into_iter()
            .collect();
        for (attr, want) in rec["writable"].as_object().expect("writable") {
            let Some(g) = guard_for(guards, contract, attr) else {
                continue;
            };
            let GuardOutcome::Lowered(expr) = &g.outcome else {
                continue;
            };
            match eval(&transform(expr), &facts) {
                None => t.unrecorded += 1,
                Some(got) => {
                    t.compared += 1;
                    if Some(got) != want.as_bool() {
                        t.mismatches.push(format!(
                            "{} {} {contract}.{attr}: lowered {got}, oracle {want}",
                            rec["name"], rec["user"]
                        ));
                    }
                }
            }
        }
    }
    t
}

#[test]
fn lowered_writable_guards_equal_the_contract_oracle() {
    let Some(src) = source() else { return };
    let guards = extract_contract_guards(&src.join("app/contracts"));
    let wp: Vec<_> = guards
        .iter()
        .filter(|g| g.class.starts_with("WorkPackages::"))
        .collect();
    let lowered = wp
        .iter()
        .filter(|g| matches!(g.outcome, GuardOutcome::Lowered(_)))
        .count();
    eprintln!(
        "work-package guards: {} declared, {lowered} lowered",
        wp.len()
    );
    for g in &wp {
        if let GuardOutcome::Unlowered { reason } = &g.outcome {
            eprintln!(
                "  unlowered {}.{} ({}:{}): {reason}",
                g.class, g.attribute, g.file, g.line
            );
        }
    }

    let t = run(&guards, Clone::clone);
    eprintln!(
        "compared {} (record, attribute) pairs; {} unrecorded; {} mismatches",
        t.compared,
        t.unrecorded,
        t.mismatches.len()
    );
    for m in &t.mismatches {
        eprintln!("  MISMATCH {m}");
    }
    // Anti-vacuity: the comparison must have covered real pairs.
    assert!(t.compared >= 20, "only {} pairs compared", t.compared);
    assert!(t.mismatches.is_empty(), "{} mismatches", t.mismatches.len());

    // Negative control: swapping Or/And must be caught by the same comparison.
    let control = run(&guards, swap_top);
    eprintln!("negative control: {} mismatches", control.mismatches.len());
    assert!(
        !control.mismatches.is_empty(),
        "the comparison cannot tell Or from And"
    );
}
