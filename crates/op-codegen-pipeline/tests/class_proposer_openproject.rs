//! The zammad-rs class proposer, run on OpenProject's work-package contracts.
//!
//! `extract_contract_guards_with_report` (ruff) -> `crosswalk::from_contract_guards`
//! -> `class_proposer::propose`, with `oracle/work-package-contract-trace.json`
//! as the oracle trace (it already has the shape the proposer reads: `cases`
//! with `contract`, `name`, `writable`).
//!
//! Needs `OPENPROJECT_SRC`. Without it the test prints UNVERIFIED and passes,
//! unless `REQUIRE_SOURCES=1`, which makes a missing source a failure.

use std::path::{Path, PathBuf};

use op_codegen_pipeline::class_proposer::{propose, ProposerConfig};
use op_codegen_pipeline::crosswalk::{from_contract_guards, CrosswalkReport};
use ruff_ruby_spo::extract_contract_guards_with_report;

const TRACE: &str = "oracle/work-package-contract-trace.json";

fn source() -> Option<PathBuf> {
    match std::env::var("OPENPROJECT_SRC") {
        Ok(s) if !s.is_empty() => Some(PathBuf::from(s)),
        _ => {
            assert!(
                std::env::var("REQUIRE_SOURCES").as_deref() != Ok("1"),
                "REQUIRE_SOURCES=1 but OPENPROJECT_SRC is not set"
            );
            eprintln!("UNVERIFIED: class_proposer_openproject needs OPENPROJECT_SRC");
            None
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn trace_contracts() -> Vec<String> {
    let text = std::fs::read_to_string(repo_root().join(TRACE)).expect("committed trace");
    let v: serde_json::Value = serde_json::from_str(&text).expect("trace is JSON");
    let mut names: Vec<String> = v["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|c| c["contract"].as_str().expect("contract").to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn config(trace: bool) -> ProposerConfig {
    ProposerConfig {
        min_support: 2,
        k: 1,
        oracle_trace: trace.then(|| TRACE.to_string()),
    }
}

fn report(src: &Path) -> (CrosswalkReport, usize) {
    let (guards, scan) =
        extract_contract_guards_with_report(&src.join("app/contracts/work_packages"));
    let contracts = trace_contracts();
    let extra: Vec<&str> = contracts.iter().map(String::as_str).collect();
    let n = guards.len();
    (from_contract_guards(&guards, &scan, &extra), n)
}

#[test]
fn proposals_over_the_work_package_contracts() {
    let Some(src) = source() else { return };
    let (report, guards) = report(&src);
    let out = propose(&report, &config(true), &repo_root()).expect("proposer identity holds");
    let c = &out.coverage;

    eprintln!("guards read: {guards}; run_status {}", out.run_status);
    eprintln!(
        "coverage: attributes {} = rows {} + modules {} + redeclared {}; models {}; proposals {} {:?}; unclustered {}",
        c.attributes_classified,
        c.rows,
        c.skipped_in_modules,
        c.redeclared,
        c.models,
        c.proposals,
        c.proposals_by_feature,
        c.unclustered
    );
    if let Some(o) = &c.oracle {
        eprintln!(
            "oracle: trace {} cases {} without_model {} verdicts {:?}",
            o.trace, o.cases, o.cases_without_model, o.verdicts
        );
    }
    for p in &out.proposals {
        eprintln!(
            "{} model={} witness={}={:?} members={:?} oracle={} truth={:?} evidence={:?}",
            p.id,
            p.model,
            p.witness.feature,
            p.witness.value,
            p.members,
            p.oracle.as_ref().map_or("none", |o| o.verdict()),
            p.truth,
            p.evidence
        );
    }
    for m in &out.models {
        eprintln!(
            "model {}: rows {} clustered {} unclustered {:?}",
            m.model, m.rows, m.clustered, m.unclustered
        );
    }
    for (slag, n) in &report.slag {
        eprintln!("slag {slag:?}: {n}");
    }

    assert_eq!(
        c.attributes_classified,
        c.rows + c.skipped_in_modules + c.redeclared,
        "coverage identity"
    );
    assert_eq!(c.attributes_classified, guards);
    assert!(
        c.proposals >= MEASURED_PROPOSALS,
        "{} proposals, measured {MEASURED_PROPOSALS}",
        c.proposals
    );
    assert!(
        out.proposals.iter().any(|p| p.witness.feature == "guard"),
        "no guard-witness proposal"
    );
    // Every guard proposal must co-vary in the oracle where the trace compares it.
    for p in out
        .proposals
        .iter()
        .filter(|p| p.witness.feature == "guard")
    {
        assert!(
            !matches!(
                p.oracle,
                Some(op_codegen_pipeline::class_proposer::Oracle::Diverges { .. })
            ),
            "guard proposal {} diverges: {:?}",
            p.id,
            p.oracle
        );
    }
}

/// Negative control: with a distinct guard per attribute nothing is shared, so
/// the proposer must find no group.
#[test]
fn distinct_guards_yield_no_guard_proposals() {
    let Some(src) = source() else { return };
    let (mut report, _) = report(&src);
    for (i, f) in report.attribute_facts.iter_mut().enumerate() {
        f.guard = Some(op_codegen_pipeline::crosswalk::GuardNames {
            self_calls: Vec::new(),
            receiver_reads: vec![format!("fake_fact_{i}")],
        });
        f.writable_literal = None;
    }
    let out = propose(&report, &config(false), &repo_root()).expect("identity holds");
    let guard_proposals = out
        .proposals
        .iter()
        .filter(|p| p.witness.feature == "guard")
        .count();
    eprintln!(
        "negative control: {} attributes, {} proposals",
        out.coverage.rows, out.coverage.proposals
    );
    assert!(
        out.coverage.rows >= 10,
        "control ran on {} rows",
        out.coverage.rows
    );
    assert_eq!(guard_proposals, 0);
}

/// Measured on the OpenProject checkout: two guard groups (`writable=false` x3,
/// `leaf?`+`schedule_manually?` x3).
const MEASURED_PROPOSALS: usize = 2;
