//! The residual lifecycle on top of [`crate::reconcile`]: every row that ends as typed
//! slag becomes a [`Residual`] carrying an owner stage, a locator, an inert
//! [`Proposal`], the evidence attached to it, and a [`Lifecycle`] status computed from
//! that evidence. Nothing here edits code, intake configuration or a store; the
//! resolver discovers and records, a human applies.
//!
//! Three rules hold everywhere in this module:
//!
//! 1. A structural verdict is never changed. A residual resolved by a data witness is
//!    still `Slag(RepresentedAsData)` structurally; [`Lifecycle::Resolved`] records the
//!    level that was established ([`ResolutionLevel`]) and implies no higher level.
//! 2. Evidence rests on files whose FNV-1a 64 hashes it records. A hash that no
//!    longer matches (or a file that cannot be read) rejects the evidence: the
//!    residual returns to [`Lifecycle::Open`] and the rejected attempt is retained.
//!    Evidence that rests on no file at all cannot go stale and is therefore never
//!    accepted: it is rejected the same way.
//! 3. The runtime axis ([`RuntimeVerdict`]) is derived from
//!    [`crate::reconcile::RuntimeEvidence`] and kept separate from the lifecycle. A
//!    runtime agreement resolves nothing structurally; a cited gate that did not run
//!    here is [`RuntimeVerdict::Unverified`].
//!
//! The module is application-agnostic: claims, proposals and witness bindings are
//! supplied by the caller as data.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use ruff_ruby_spo::GuardExpr;
use serde_json::{json, Value};

use crate::reconcile::{
    fnv1a64, Level, ReconciledRow, Reconciliation, RubyEvidence, RuntimeEvidence, RustProbe,
    SlagReason, Verdict,
};

// ─────────────────────────────────────────────────────────────────────────
// Vocabulary
// ─────────────────────────────────────────────────────────────────────────

/// The stage that owns a missing fact. Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OwnerStage {
    /// The application's intake configuration (which files, engines, roots are read).
    IntakeConfig,
    /// The source-side extractor (ruff's Ruby frontend).
    RubyExtraction,
    /// Generic framework semantics (Rails polyfill / OGAR).
    RailsPolyfill,
    /// The contract arm of the source-side extractor (guards, validators, blocks).
    ContractHarvest,
    /// The compiled-Rust witness (Charon through `ruff_charon`).
    RustExtraction,
    /// A comparison of generated data against its generator input.
    DataWitness,
    /// An executed comparison against a frozen oracle.
    RuntimeWitness,
}

impl OwnerStage {
    /// The default owner of a structural slag reason. A caller may refine it
    /// (`RubySideNotHarvested` on a contract block is `ContractHarvest`).
    pub fn default_for(reason: SlagReason) -> Self {
        match reason {
            SlagReason::RustSideNotExtracted
            | SlagReason::RustSymbolNotInStore
            | SlagReason::RustEvidenceStale
            | SlagReason::RustBodyNotStructured => OwnerStage::RustExtraction,
            SlagReason::RepresentedAsData => OwnerStage::DataWitness,
            SlagReason::RubySideNotHarvested => OwnerStage::RubyExtraction,
        }
    }
}

/// The level a resolution established. Each level is separate: none implies another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResolutionLevel {
    /// Previously unread code is now represented in ORE.
    Extraction,
    /// Generated data equals what its generator input says, item by item.
    DataWitness,
    /// The Rails or Rust semantics (conditions, ordering, generated behaviour) were
    /// reconstructed.
    Semantic,
    /// Comparable Ruby and Charon facts agree at the same semantic address.
    StructuralAgreement,
    /// The Ruby and Rust transitions both executed and their outcomes match.
    ExecutionAgreement,
    /// Persisted rows, callbacks, journals or jobs agree per the operation's oracle.
    EffectsAgreement,
}

/// A residual's lifecycle. `Resolved` records exactly the level it established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// No candidate source or rule, or every attempt was rejected.
    Open,
    /// A unique candidate source location and an existing supplier are named.
    Candidate,
    /// Evidence is attached and current, but it does not establish a level.
    Evidenced,
    /// A witness established a level against current evidence.
    Verified(ResolutionLevel),
    /// Verified, and a named gate fails if the evidence regresses.
    Resolved(ResolutionLevel),
}

impl fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Lifecycle::Open => f.write_str("OPEN"),
            Lifecycle::Candidate => f.write_str("CANDIDATE"),
            Lifecycle::Evidenced => f.write_str("EVIDENCED"),
            Lifecycle::Verified(l) => write!(f, "VERIFIED({l:?})"),
            Lifecycle::Resolved(l) => write!(f, "RESOLVED({l:?})"),
        }
    }
}

/// The runtime axis, read from [`RuntimeEvidence`]. Never changes a structural verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeVerdict {
    ExecutedAgreement,
    ExecutedDisagreement,
    /// Rust executed; the oracle records nothing comparable.
    ExecutedRustOnly,
    NotExecuted,
    /// Asserted by a gate that did not run here.
    Unverified,
}

impl RuntimeVerdict {
    pub fn of(e: &RuntimeEvidence) -> Self {
        match e {
            RuntimeEvidence::Executed { compared: 0, .. } => RuntimeVerdict::ExecutedRustOnly,
            RuntimeEvidence::Executed { mismatches: 0, .. } => RuntimeVerdict::ExecutedAgreement,
            RuntimeEvidence::Executed { .. } => RuntimeVerdict::ExecutedDisagreement,
            RuntimeEvidence::Cited { .. } => RuntimeVerdict::Unverified,
            RuntimeEvidence::NotExecuted { .. } => RuntimeVerdict::NotExecuted,
        }
    }
}

/// What kind of thing a claim compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Representation {
    Declaration,
    GeneratedData,
    Call,
    Guard,
    Callback,
    Effect,
}

impl Representation {
    pub fn of(row: &ReconciledRow) -> Self {
        match (&row.spec.rust, row.spec.level) {
            (RustProbe::Data { .. }, _) => Representation::GeneratedData,
            (_, Level::Registration) => Representation::Declaration,
            (_, Level::CallbackInvocation) => Representation::Callback,
            (_, Level::Persistence) => Representation::Effect,
            (_, Level::GuardEvaluation) => Representation::Guard,
            (_, Level::Admission) => Representation::Call,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Evidence
// ─────────────────────────────────────────────────────────────────────────

/// Which checkout a hashed file lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileRoot {
    /// The source application (OpenProject checkout).
    Source,
    /// This repository.
    Target,
}

/// A file evidence rests on, with its hash at the time the evidence was produced.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileHash {
    pub root: FileRoot,
    pub path: String,
    pub fnv64: u64,
}

/// Where the two checkouts live. `None`: that root is not available here.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub source: Option<PathBuf>,
    pub target: Option<PathBuf>,
}

/// The result of re-hashing one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCheck {
    Current,
    /// The bytes changed, or the file is gone.
    Stale,
    /// Its checkout is not available: neither current nor stale is known.
    Unverifiable,
}

impl Roots {
    fn dir(&self, root: FileRoot) -> Option<&Path> {
        match root {
            FileRoot::Source => self.source.as_deref(),
            FileRoot::Target => self.target.as_deref(),
        }
    }

    /// Hash `path` under `root` now. `None` when the root is absent or the file
    /// cannot be read.
    pub fn hash(&self, root: FileRoot, path: &str) -> Option<FileHash> {
        let bytes = std::fs::read(self.dir(root)?.join(path)).ok()?;
        Some(FileHash {
            root,
            path: path.to_string(),
            fnv64: fnv1a64(&bytes),
        })
    }

    pub fn check(&self, f: &FileHash) -> FileCheck {
        let Some(dir) = self.dir(f.root) else {
            return FileCheck::Unverifiable;
        };
        match std::fs::read(dir.join(&f.path)) {
            Ok(b) if fnv1a64(&b) == f.fnv64 => FileCheck::Current,
            _ => FileCheck::Stale,
        }
    }
}

/// What produced a piece of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceKind {
    /// The generated file equals the generator's output for the source checkout.
    DriftCheck,
    /// Generated data compared item by item with the extractor's resolved input.
    DataWitness,
    /// Production Rust executed against a frozen oracle in this run.
    RuntimeExecuted,
    /// A gate elsewhere asserts it; not run here.
    RuntimeCited,
}

/// One piece of evidence attached to a residual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    pub kind: EvidenceKind,
    /// The function or test that produced it.
    pub by: String,
    /// Extractor / generator revision it was produced with.
    pub revision: String,
    pub rests_on: Vec<FileHash>,
    pub compared: usize,
    pub mismatches: usize,
    /// What was compared, in words, and any mismatch.
    pub detail: String,
    /// Whether the source side of the comparison is a parsed extractor fact (not a
    /// line scan or a located literal). Only parsed input can verify a data witness.
    pub source_side_parsed: bool,
}

impl EvidenceRef {
    /// The level this evidence would establish if current, or `None`.
    fn establishes(&self) -> Option<ResolutionLevel> {
        let passed = self.compared > 0 && self.mismatches == 0;
        match self.kind {
            EvidenceKind::DataWitness if passed && self.source_side_parsed => {
                Some(ResolutionLevel::DataWitness)
            }
            _ => None,
        }
    }
}

/// An attempt that did not hold, retained with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub evidence: EvidenceRef,
    pub rejected_because: String,
}

// ─────────────────────────────────────────────────────────────────────────
// Proposals and residuals
// ─────────────────────────────────────────────────────────────────────────

/// An inert, evidence-backed proposal for resolving one residual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// The fact that is missing.
    pub missing_fact: String,
    /// The source or rule that could supply it.
    pub supplier: String,
    /// Whether that supplier exists today (a tool that can be run, a rule that is
    /// written). A proposal naming a supplier still to be built keeps the residual
    /// OPEN.
    pub supplier_exists: bool,
    /// Exact source location(s), `file:line` or `file bytes a..b`.
    pub source_location: String,
    /// The revision the location refers to.
    pub source_revision: String,
    /// How the semantic claim would change if the fact were supplied.
    pub claim_change: String,
    /// The independent evidence that would validate the resolution.
    pub validating_evidence: String,
    /// The negative control that would invalidate an incorrect resolution.
    pub negative_control: String,
}

/// One unresolved (or resolved-at-a-level) claim.
#[derive(Debug, Clone)]
pub struct Residual {
    pub key: String,
    pub reason: SlagReason,
    /// A finer cause inside `reason`, from the caller.
    pub sub_reason: String,
    pub owner_stage: OwnerStage,
    /// The Rust and Ruby locations the residual sits at.
    pub locator: String,
    pub proposal: Proposal,
    pub evidence: Vec<EvidenceRef>,
    /// The gate that fails when the resolving evidence regresses. `None`: none.
    pub regression_gate: Option<String>,
    /// Companion evidence kinds that must each be attached, current and passing for a
    /// level to be established. Without them the evidence that carries the level is
    /// supporting only and the residual stays [`Lifecycle::Evidenced`].
    pub required_companions: Vec<EvidenceKind>,
    pub status: Lifecycle,
    pub attempts: Vec<Attempt>,
}

impl Residual {
    pub fn new(key: String, reason: SlagReason, proposal: Proposal) -> Self {
        Residual {
            key,
            reason,
            sub_reason: String::new(),
            owner_stage: OwnerStage::default_for(reason),
            locator: String::new(),
            proposal,
            evidence: Vec::new(),
            regression_gate: None,
            required_companions: Vec::new(),
            status: Lifecycle::Open,
            attempts: Vec::new(),
        }
    }

    /// Recompute `status` from the attached evidence: stale or failing evidence is
    /// moved into `attempts` (retained) and the residual falls back. A level is only
    /// established for the reason it resolves (a data witness resolves
    /// `RepresentedAsData`, nothing else).
    pub fn evaluate(&mut self, roots: &Roots) {
        let mut kept = Vec::new();
        let mut unverifiable = false;
        for e in std::mem::take(&mut self.evidence) {
            let checks: Vec<(String, FileCheck)> = e
                .rests_on
                .iter()
                .map(|f| (f.path.clone(), roots.check(f)))
                .collect();
            if e.rests_on.is_empty() {
                self.attempts.push(Attempt {
                    evidence: e,
                    rejected_because:
                        "rests on no file: it can never go stale, so it cannot be verified".into(),
                });
                continue;
            }
            if let Some((p, _)) = checks.iter().find(|(_, c)| *c == FileCheck::Stale) {
                self.attempts.push(Attempt {
                    evidence: e,
                    rejected_because: format!("stale: {p} changed since the evidence was produced"),
                });
                continue;
            }
            if e.mismatches > 0 && e.kind != EvidenceKind::RuntimeExecuted {
                self.attempts.push(Attempt {
                    rejected_because: format!(
                        "{} of {} compared items differ: {}",
                        e.mismatches, e.compared, e.detail
                    ),
                    evidence: e,
                });
                continue;
            }
            // Evidence whose source side is not parsed ORE is kept but cannot verify
            // (`EvidenceRef::establishes`).
            if checks.iter().any(|(_, c)| *c == FileCheck::Unverifiable) {
                unverifiable = true;
            }
            kept.push(e);
        }
        self.evidence = kept;
        let companions_hold = self.required_companions.iter().all(|k| {
            self.evidence
                .iter()
                .any(|e| e.kind == *k && e.mismatches == 0)
        });
        let level = (!unverifiable && companions_hold)
            .then(|| {
                self.evidence
                    .iter()
                    .filter_map(EvidenceRef::establishes)
                    .find(|l| {
                        *l == ResolutionLevel::DataWitness
                            && self.reason == SlagReason::RepresentedAsData
                    })
            })
            .flatten();
        self.status = match level {
            Some(l) if self.regression_gate.is_some() => Lifecycle::Resolved(l),
            Some(l) => Lifecycle::Verified(l),
            None if !self.evidence.is_empty() && !unverifiable => Lifecycle::Evidenced,
            None if self.proposal.supplier_exists && !self.proposal.source_location.is_empty() => {
                Lifecycle::Candidate
            }
            None => Lifecycle::Open,
        };
    }
}

// ─────────────────────────────────────────────────────────────────────────
// A generic data witness: a lowered guard against a production evaluator
// ─────────────────────────────────────────────────────────────────────────

/// Every fact name a guard reads, sorted.
pub fn guard_fact_names(e: &GuardExpr) -> Vec<String> {
    fn walk(e: &GuardExpr, out: &mut BTreeSet<String>) {
        match e {
            GuardExpr::Const(_) => {}
            GuardExpr::Fact(n) => {
                out.insert(n.clone());
            }
            GuardExpr::Not(a) => walk(a, out),
            GuardExpr::And(a, b) | GuardExpr::Or(a, b) => {
                walk(a, out);
                walk(b, out);
            }
        }
    }
    let mut s = BTreeSet::new();
    walk(e, &mut s);
    s.into_iter().collect()
}

/// Ruby's value of a lowered guard under a total assignment of its facts.
pub fn eval_guard(e: &GuardExpr, facts: &BTreeMap<String, bool>) -> bool {
    match e {
        GuardExpr::Const(b) => *b,
        GuardExpr::Fact(n) => facts[n],
        GuardExpr::Not(a) => !eval_guard(a, facts),
        GuardExpr::And(a, b) => eval_guard(a, facts) && eval_guard(b, facts),
        GuardExpr::Or(a, b) => eval_guard(a, facts) || eval_guard(b, facts),
    }
}

/// The outcome of comparing one guard's truth table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruthTable {
    pub assignments: usize,
    pub mismatches: Vec<String>,
}

/// Compare a lowered guard against `rust` on every assignment of its facts. `rust`
/// returns `None` when the production evaluator gives no definite answer, which
/// counts as a mismatch: the table must be decided everywhere.
pub fn compare_truth_table(
    e: &GuardExpr,
    rust: impl Fn(&BTreeMap<String, bool>) -> Option<bool>,
) -> TruthTable {
    let names = guard_fact_names(e);
    let n = names.len();
    assert!(n <= 16, "guard reads {n} facts");
    let mut mismatches = Vec::new();
    for mask in 0u32..(1 << n) {
        let facts: BTreeMap<String, bool> = names
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), mask & (1 << i) != 0))
            .collect();
        let want = eval_guard(e, &facts);
        let got = rust(&facts);
        if got != Some(want) {
            mismatches.push(format!("{facts:?}: ruby {want}, rust {got:?}"));
        }
    }
    TruthTable {
        assignments: 1 << n,
        mismatches,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The ledger and the scorecard
// ─────────────────────────────────────────────────────────────────────────

fn ruby_json(e: &RubyEvidence) -> Value {
    match e {
        RubyEvidence::Fact {
            extractor,
            span,
            fact,
        } => {
            json!({"state": "fact", "extractor": extractor, "span": span.to_string(), "fact": fact})
        }
        RubyEvidence::Absent {
            extractor,
            span,
            looked_for,
        } => {
            json!({"state": "absent", "extractor": extractor, "span": span.to_string(), "looked_for": looked_for})
        }
        RubyEvidence::NotHarvested { why } => json!({"state": "not_harvested", "why": why}),
    }
}

fn evidence_json(e: &EvidenceRef) -> Value {
    json!({
        "kind": format!("{:?}", e.kind),
        "by": e.by,
        "revision": e.revision,
        "compared": e.compared,
        "mismatches": e.mismatches,
        "source_side_parsed": e.source_side_parsed,
        "detail": e.detail,
        "rests_on": e.rests_on.iter().map(|f| json!({
            "root": format!("{:?}", f.root),
            "path": f.path,
            "fnv64": format!("{:016x}", f.fnv64),
        })).collect::<Vec<_>>(),
    })
}

/// The machine-readable residual ledger: generated from the same in-memory
/// reconciliation as the Markdown drilldown, never edited by hand.
pub fn render_ledger(r: &Reconciliation, residuals: &[Residual]) -> String {
    let by_key: BTreeMap<&str, &Residual> = residuals.iter().map(|x| (x.key.as_str(), x)).collect();
    let claims: Vec<Value> = r
        .rows
        .iter()
        .map(|row| {
            let key = row.spec.key.to_string();
            let residual = by_key.get(key.as_str()).map(|x| {
                json!({
                    "reason": format!("{:?}", x.reason),
                    "sub_reason": x.sub_reason,
                    "owner_stage": format!("{:?}", x.owner_stage),
                    "locator": x.locator,
                    "status": x.status.to_string(),
                    "regression_gate": x.regression_gate,
                    "proposed_remedy": {
                        "missing_fact": x.proposal.missing_fact,
                        "supplier": x.proposal.supplier,
                        "supplier_exists": x.proposal.supplier_exists,
                        "source_location": x.proposal.source_location,
                        "source_revision": x.proposal.source_revision,
                        "claim_change": x.proposal.claim_change,
                        "validating_evidence": x.proposal.validating_evidence,
                        "negative_control": x.proposal.negative_control,
                    },
                    "evidence": x.evidence.iter().map(evidence_json).collect::<Vec<_>>(),
                    "attempts": x.attempts.iter().map(|a| json!({
                        "rejected_because": a.rejected_because,
                        "evidence": evidence_json(&a.evidence),
                    })).collect::<Vec<_>>(),
                })
            });
            json!({
                "key": key,
                "identity": {
                    "application": row.spec.key.application,
                    "model": row.spec.key.model,
                    "operation": row.spec.key.operation,
                    "phase": row.spec.key.phase,
                    "method": row.spec.key.method,
                    "effect": row.spec.key.effect,
                },
                "level": format!("{:?}", row.spec.level),
                "representation": format!("{:?}", Representation::of(row)),
                "ruby": ruby_json(&row.spec.ruby),
                "rust": {
                    "span": row.rust_span.as_ref().map(|s| format!("{s}: {}", s.what)),
                },
                "coverage": row.coverage,
                "structural_verdict": row.verdict.to_string(),
                "runtime_verdict": format!("{:?}", RuntimeVerdict::of(&row.runtime)),
                "runtime_evidence": row.runtime.to_string(),
                "residual": residual,
            })
        })
        .collect();
    let doc = json!({
        "generated_by": "crates/op-codegen-pipeline/tests/ore_slag/resolution.rs (via reconcile_create_admission). Do not edit by hand.",
        "title": r.title,
        "revisions": {
            "source_extractor": r.revisions.source_extractor,
            "charon": r.revisions.charon,
            "charon_extractor": r.revisions.charon_extractor,
            "target": r.revisions.target,
            "source_app": r.revisions.source_app,
        },
        "charon_crates": r.extracted,
        "stale_store_files": r.stale_files.iter().collect::<Vec<_>>(),
        "scorecard": Scorecard::of(r, residuals).to_json(),
        "claims": claims,
    });
    let mut s = serde_json::to_string_pretty(&doc).expect("ledger serializes");
    s.push('\n');
    s
}

/// Counts with separate denominators.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scorecard {
    pub claims: usize,
    /// Ruby side: a fact or an inspected absence / of claims.
    pub ruby_inspected: usize,
    /// Of `ruby_inspected`, produced by a parser-backed extractor (not a line scan
    /// or a located literal).
    pub ruby_parsed: usize,
    /// Rust side: the counterpart was inspectable (not a Rust-side slag) / of claims.
    pub rust_inspectable: usize,
    pub structural: BTreeMap<String, usize>,
    pub open_by_reason: BTreeMap<String, usize>,
    pub open_by_owner: BTreeMap<String, usize>,
    pub resolved_by_level: BTreeMap<String, usize>,
    pub lifecycle: BTreeMap<String, usize>,
    pub runtime: BTreeMap<String, usize>,
    /// Sum of `compared` over executed runtime rows (overlapping item sets).
    pub runtime_items_compared: usize,
    pub runtime_items_mismatched: usize,
}

impl Scorecard {
    pub fn of(r: &Reconciliation, residuals: &[Residual]) -> Self {
        let mut s = Scorecard {
            claims: r.rows.len(),
            ..Scorecard::default()
        };
        let parsed = |x: &str| x.starts_with("ruff_");
        for row in &r.rows {
            match &row.spec.ruby {
                RubyEvidence::Fact { extractor, .. } | RubyEvidence::Absent { extractor, .. } => {
                    s.ruby_inspected += 1;
                    s.ruby_parsed += usize::from(parsed(extractor));
                }
                RubyEvidence::NotHarvested { .. } => {}
            }
            let rust_slag = matches!(
                row.verdict,
                Verdict::Slag(
                    SlagReason::RustSideNotExtracted
                        | SlagReason::RustSymbolNotInStore
                        | SlagReason::RustEvidenceStale
                        | SlagReason::RustBodyNotStructured
                )
            );
            s.rust_inspectable += usize::from(!rust_slag);
            *s.structural
                .entry(row.verdict.state().to_string())
                .or_default() += 1;
            *s.runtime
                .entry(format!("{:?}", RuntimeVerdict::of(&row.runtime)))
                .or_default() += 1;
            if let RuntimeEvidence::Executed {
                compared,
                mismatches,
                ..
            } = row.runtime
            {
                s.runtime_items_compared += compared;
                s.runtime_items_mismatched += mismatches;
            }
        }
        for x in residuals {
            *s.lifecycle.entry(x.status.to_string()).or_default() += 1;
            match x.status {
                Lifecycle::Resolved(l) | Lifecycle::Verified(l) => {
                    *s.resolved_by_level.entry(format!("{l:?}")).or_default() += 1;
                }
                _ => {
                    *s.open_by_reason
                        .entry(format!("{:?}", x.reason))
                        .or_default() += 1;
                    *s.open_by_owner
                        .entry(format!("{:?}", x.owner_stage))
                        .or_default() += 1;
                }
            }
        }
        s
    }

    pub fn to_json(&self) -> Value {
        json!({
            "claims": self.claims,
            "ruby_facts_inspected": self.ruby_inspected,
            "ruby_facts_from_parser": self.ruby_parsed,
            "rust_counterparts_inspectable": self.rust_inspectable,
            "structural": self.structural,
            "open_slag_by_reason": self.open_by_reason,
            "open_slag_by_owner": self.open_by_owner,
            "resolved_by_level": self.resolved_by_level,
            "lifecycle": self.lifecycle,
            "runtime": self.runtime,
            "runtime_items_compared_overlapping": self.runtime_items_compared,
            "runtime_items_mismatched": self.runtime_items_mismatched,
        })
    }
}

impl fmt::Display for Scorecard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "claims: {}", self.claims)?;
        writeln!(
            f,
            "ruby side: {} of {} inspected (fact or inspected absence); {} of those from a parser-backed extractor",
            self.ruby_inspected, self.claims, self.ruby_parsed
        )?;
        writeln!(
            f,
            "rust side: {} of {} counterparts inspectable in the Charon store",
            self.rust_inspectable, self.claims
        )?;
        writeln!(f, "structural verdicts: {:?}", self.structural)?;
        writeln!(f, "residual lifecycle: {:?}", self.lifecycle)?;
        writeln!(f, "open slag by reason: {:?}", self.open_by_reason)?;
        writeln!(f, "open slag by owner: {:?}", self.open_by_owner)?;
        writeln!(f, "resolved by level: {:?}", self.resolved_by_level)?;
        writeln!(f, "runtime verdicts: {:?}", self.runtime)?;
        write!(
            f,
            "runtime items compared (overlapping sets, not additive with other gates): {}, mismatched {}",
            self.runtime_items_compared, self.runtime_items_mismatched
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(n: &str) -> Box<GuardExpr> {
        Box::new(GuardExpr::Fact(n.into()))
    }

    #[test]
    fn truth_table_detects_a_swapped_connective() {
        let ruby = GuardExpr::Or(fact("a"), fact("b"));
        let same = compare_truth_table(&ruby, |f| Some(f["a"] || f["b"]));
        assert_eq!((same.assignments, same.mismatches.len()), (4, 0));
        let swapped = compare_truth_table(&ruby, |f| Some(f["a"] && f["b"]));
        assert_eq!(swapped.mismatches.len(), 2);
        // An undecided answer is never an agreement.
        assert_eq!(compare_truth_table(&ruby, |_| None).mismatches.len(), 4);
    }

    fn proposal(exists: bool) -> Proposal {
        Proposal {
            missing_fact: "f".into(),
            supplier: "s".into(),
            supplier_exists: exists,
            source_location: "a.rb:1".into(),
            source_revision: "r".into(),
            claim_change: "c".into(),
            validating_evidence: "v".into(),
            negative_control: "n".into(),
        }
    }

    fn witness(dir: &Path, mismatches: usize, parsed: bool) -> EvidenceRef {
        std::fs::write(dir.join("g.rs"), "x").unwrap();
        EvidenceRef {
            kind: EvidenceKind::DataWitness,
            by: "t".into(),
            revision: "r".into(),
            rests_on: vec![FileHash {
                root: FileRoot::Target,
                path: "g.rs".into(),
                fnv64: fnv1a64(b"x"),
            }],
            compared: 4,
            mismatches,
            detail: String::new(),
            source_side_parsed: parsed,
        }
    }

    #[test]
    fn lifecycle_promotes_only_on_current_passing_parsed_evidence() {
        let dir = std::env::temp_dir().join(format!("resolution-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let roots = Roots {
            source: None,
            target: Some(dir.clone()),
        };
        let mut r = Residual::new("k".into(), SlagReason::RepresentedAsData, proposal(true));
        r.evaluate(&roots);
        assert_eq!(r.status, Lifecycle::Candidate);
        r.evidence.push(witness(&dir, 0, true));
        r.evaluate(&roots);
        assert_eq!(r.status, Lifecycle::Verified(ResolutionLevel::DataWitness));
        r.regression_gate = Some("gate".into());
        r.evaluate(&roots);
        assert_eq!(r.status, Lifecycle::Resolved(ResolutionLevel::DataWitness));
        // The file changes: the evidence is rejected, retained, and the residual falls back.
        std::fs::write(dir.join("g.rs"), "y").unwrap();
        r.evaluate(&roots);
        assert_eq!(r.status, Lifecycle::Candidate);
        assert_eq!(r.attempts.len(), 1);
        assert!(r.attempts[0].rejected_because.starts_with("stale"));
        // A failing witness is rejected; an unparsed source side only evidences.
        let mut f = Residual::new("k".into(), SlagReason::RepresentedAsData, proposal(false));
        f.evidence.push(witness(&dir, 1, true));
        f.evaluate(&roots);
        assert_eq!((f.status, f.attempts.len()), (Lifecycle::Open, 1));
        let mut u = Residual::new("k".into(), SlagReason::RepresentedAsData, proposal(false));
        u.evidence.push(witness(&dir, 0, false));
        u.evaluate(&roots);
        assert_eq!(u.status, Lifecycle::Evidenced);
        // A data witness never resolves another reason.
        let mut o = Residual::new(
            "k".into(),
            SlagReason::RubySideNotHarvested,
            proposal(false),
        );
        o.evidence.push(witness(&dir, 0, true));
        o.evaluate(&roots);
        assert_eq!(o.status, Lifecycle::Evidenced);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn evidence_resting_on_no_file_is_rejected() {
        let dir = std::env::temp_dir().join(format!("resolution-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let roots = Roots {
            source: None,
            target: Some(dir.clone()),
        };
        // The same passing, parsed witness, once with a file and once without.
        let mut with = Residual::new("k".into(), SlagReason::RepresentedAsData, proposal(true));
        with.evidence.push(witness(&dir, 0, true));
        with.evaluate(&roots);
        assert_eq!(
            with.status,
            Lifecycle::Verified(ResolutionLevel::DataWitness)
        );
        let mut without = Residual::new("k".into(), SlagReason::RepresentedAsData, proposal(true));
        let mut e = witness(&dir, 0, true);
        e.rests_on.clear();
        without.evidence.push(e);
        without.regression_gate = Some("gate".into());
        without.evaluate(&roots);
        assert_eq!(without.status, Lifecycle::Candidate);
        assert!(without.evidence.is_empty());
        assert_eq!(without.attempts.len(), 1);
        assert!(without.attempts[0]
            .rejected_because
            .starts_with("rests on no file"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_level_needs_its_required_companions() {
        let dir = std::env::temp_dir().join(format!("resolution-comp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let roots = Roots {
            source: None,
            target: Some(dir.clone()),
        };
        let drift = |mismatches: usize| EvidenceRef {
            kind: EvidenceKind::DriftCheck,
            compared: 1,
            mismatches,
            ..witness(&dir, 0, true)
        };
        let build = |companion: Option<EvidenceRef>| {
            let mut r = Residual::new("k".into(), SlagReason::RepresentedAsData, proposal(true));
            r.required_companions = vec![EvidenceKind::DriftCheck];
            r.regression_gate = Some("gate".into());
            r.evidence.push(witness(&dir, 0, true));
            r.evidence.extend(companion);
            r.evaluate(&roots);
            r
        };
        // Passing companion: resolved.
        assert_eq!(
            build(Some(drift(0))).status,
            Lifecycle::Resolved(ResolutionLevel::DataWitness)
        );
        // Missing companion: the witness alone only evidences.
        assert_eq!(build(None).status, Lifecycle::Evidenced);
        // Failing companion: moved to attempts, the witness alone only evidences.
        let failing = build(Some(drift(1)));
        assert_eq!(failing.status, Lifecycle::Evidenced);
        assert_eq!(failing.attempts.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
