//! ORE/SLAG resolution for WorkPackage create admission, on top of the three-witness
//! reconciliation of the parent test file (this module is declared at the end of
//! `tests/reconcile_create_admission.rs`, so it builds the same claims through the
//! same functions and never restates them).
//!
//! It turns every slag row into a typed [`Residual`] with an owner stage, an inert
//! proposal and the evidence attached to it, computes each residual's lifecycle,
//! writes the machine-readable ledger next to the Markdown drilldown, and runs the
//! negative controls of the resolution lifecycle.
//!
//! Two OpenProject-specific witnesses live here as data bindings:
//!
//! - the data witness for the lowered `writable:` guards: ruff's `GuardExpr` truth
//!   table against production `is_writable` over the generated `ATTRIBUTES` table,
//!   bound to the existing generator drift check (it does not replace it);
//! - the `schedule_manually` block differential: the contract trace's
//!   `cannot_be_automatically_scheduled` error against production admission. It is a
//!   separate case, never added to `create_transition_differential`'s 350 items.
//!
//! `UPDATE_RECONCILE_LEDGER=1` rewrites the checked-in ledger. Without
//! `OPENPROJECT_SRC` the Ruby-dependent parts print UNVERIFIED (and `REQUIRE_SOURCES=1`
//! fails, through the parent's `source()`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use op_codegen_pipeline::admission_gen;
use op_codegen_pipeline::callback_harvest as cb;
use op_codegen_pipeline::reconcile::ReconciledRow;
use op_codegen_pipeline::resolution::{
    compare_truth_table, guard_fact_names, render_ledger, EvidenceKind, EvidenceRef, FileRoot,
    Lifecycle, OwnerStage, Proposal, Residual, ResolutionLevel, Roots, RuntimeVerdict, Scorecard,
};
use op_contracts::work_packages::guard_facts;
use ruff_ruby_spo::GuardExpr;

use super::*;

const LEDGER: &str = "oracle/charon/reconcile-create-admission.ledger.json";
const ADMISSION_RS: &str = "crates/op-contracts/src/work_packages/admission.rs";
const BASE_CONTRACT: &str = "app/contracts/base_contract.rb";
const WP_BASE_CONTRACT: &str = "app/contracts/work_packages/base_contract.rb";
const KEY_WRITABLE: &str =
    "openproject/WorkPackage/create/guard/admit_create/evaluates:writable?-per-submitted-attribute";
const KEY_SCHEDULE: &str = "openproject/WorkPackage/create/guard/CreateContract.attribute(:schedule_manually).block/validates:validate_has_predecessors_or_children";
const KEY_PERSIST: &str =
    "openproject/WorkPackage/create/persistence/mutation::create/inserts:work_packages";
const KEY_API: &str =
    "openproject/WorkPackage/create/admission/POST /api/v3/work_packages/enters:mutation::create";
const DRIFT_GATE: &str = "tests/create_transition_differential.rs::checked_in_admission_table_is_what_the_generator_produces";
const WITNESS_GATE: &str =
    "tests/reconcile_create_admission.rs::ore_slag::ore_slag_ledger_and_lifecycle";
const SCHEDULE_CODE: &str = "cannot_be_automatically_scheduled";

fn roots(src: Option<&std::path::Path>) -> Roots {
    Roots {
        source: src.map(std::path::Path::to_path_buf),
        target: Some(root()),
    }
}

fn op_rev() -> String {
    trace()["source"]["rev"]
        .as_str()
        .unwrap_or("unknown")
        .to_string()
}

// ─────────────────────────────────────────────────────────────────────────
// The data witness for lowered guards
// ─────────────────────────────────────────────────────────────────────────

struct MapFacts<'a>(&'a BTreeMap<String, bool>);

impl AdmissionFacts for MapFacts<'_> {
    fn fact(&self, name: &str) -> Option<bool> {
        self.0.get(name).copied()
    }
}

/// How the witness maps a Ruby guard onto the Rust table. Swappable so the negative
/// controls can break the mapping or the Ruby input.
struct WitnessBinding {
    /// Ruby attribute -> the Rust attribute asked.
    attribute: fn(&[ContractGuard], &ContractGuard) -> String,
    /// The Ruby expression the comparison reads.
    expr: fn(&GuardExpr) -> GuardExpr,
}

const FAITHFUL: WitnessBinding = WitnessBinding {
    attribute: |_, g| g.attribute.clone(),
    expr: GuardExpr::clone,
};

/// The guard a claim row names, when its Ruby side is a ruff fact.
fn guard_of<'a>(row: &ReconciledRow, guards: &'a [ContractGuard]) -> Option<&'a ContractGuard> {
    let RubyEvidence::Fact { extractor, .. } = &row.spec.ruby else {
        return None;
    };
    if extractor != RUFF {
        return None;
    }
    let m = &row.spec.key.method;
    let (class, rest) = m.split_once(".attribute(:")?;
    let attr = rest.strip_suffix(')')?;
    effective_guards(guards)
        .into_iter()
        .find(|g| g.class == class && g.attribute == attr)
}

/// ruff's lowered guard (the resolved Ruby input) against production `is_writable`
/// over the generated table, on every assignment of the guard's facts, plus the
/// fact-name set the table reads. `None`: no ruff fact to compare (the Ruby side
/// was not extracted), so no evidence.
fn guard_witness(
    row: &ReconciledRow,
    guards: &[ContractGuard],
    bind: &WitnessBinding,
    roots: &Roots,
) -> Option<EvidenceRef> {
    let g = guard_of(row, guards)?;
    let GuardOutcome::Lowered(lowered) = &g.outcome else {
        return None;
    };
    let ruby = (bind.expr)(lowered);
    let attr = (bind.attribute)(
        &effective_guards(guards)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>(),
        g,
    );
    let perms = oracle_permissions();
    let table = compare_truth_table(&ruby, |facts| {
        let admin = facts.get("user.admin?").copied().unwrap_or(false);
        let user = OracleUser::of(if admin { "admin" } else { "member" }, &perms);
        match is_writable(&user, Some(CASE_PROJECT), &attr, &MapFacts(facts)) {
            Writable::Yes => Some(true),
            Writable::No => Some(false),
            Writable::Unresolved(_) => None,
        }
    });
    let ruby_names = guard_fact_names(&ruby);
    let rust_names: Vec<String> = guard_facts(&attr).iter().map(|s| s.to_string()).collect();
    let mut mismatches = table.mismatches.clone();
    if ruby_names != rust_names {
        mismatches.push(format!(
            "facts read: ruby {ruby_names:?}, rust {rust_names:?}"
        ));
    }
    let src_file = format!("app/contracts/{}", g.file);
    let rests_on = [
        roots.hash(FileRoot::Source, &src_file),
        roots.hash(FileRoot::Target, GENERATED),
        roots.hash(FileRoot::Target, ADMISSION_RS),
    ]
    .into_iter()
    .collect::<Option<Vec<_>>>()?;
    Some(EvidenceRef {
        kind: EvidenceKind::DataWitness,
        by: format!(
            "{WITNESS_GATE} (ruff GuardExpr truth table vs op_contracts::is_writable({attr}))"
        ),
        revision: format!("ruff {}", op_codegen_pipeline::RUFF_REV),
        rests_on,
        compared: table.assignments + 1,
        mismatches: mismatches.len(),
        detail: if mismatches.is_empty() {
            format!(
                "{} assignments of {:?} agree; the table reads the same facts",
                table.assignments, ruby_names
            )
        } else {
            mismatches.join("; ")
        },
        source_side_parsed: true,
    })
}

/// The generator drift check, as evidence. It is the existing gate's predicate
/// (`admission_gen::generate(src) == checked-in table`), not a second comparison.
fn drift_evidence(src: &std::path::Path, roots: &Roots, parsed: bool) -> Option<EvidenceRef> {
    let checked_in = std::fs::read_to_string(root().join(GENERATED)).ok()?;
    let same = admission_gen::generate(src) == checked_in;
    let rests_on = [
        // The contract files the generator reads for these guards: the guard lowering
        // starts from the work-package base contract, follows it into the
        // application's `BaseContract` (`writable?`, `attribute`), and the permission
        // line scan reads `create_contract.rb`. The generator scans the whole
        // `app/contracts` tree; a change to another file there is caught by this
        // check re-running, not by these hashes.
        roots.hash(FileRoot::Source, BASE_CONTRACT),
        roots.hash(FileRoot::Source, WP_BASE_CONTRACT),
        roots.hash(FileRoot::Source, CREATE_CONTRACT),
        roots.hash(FileRoot::Target, GENERATED),
    ]
    .into_iter()
    .collect::<Option<Vec<_>>>()?;
    Some(EvidenceRef {
        kind: EvidenceKind::DriftCheck,
        by: DRIFT_GATE.into(),
        revision: format!("ruff {}; admission_gen", op_codegen_pipeline::RUFF_REV),
        rests_on,
        compared: 1,
        mismatches: usize::from(!same),
        detail: if same {
            "the checked-in table is byte-identical to the generator's output for this checkout"
                .into()
        } else {
            "the checked-in table differs from the generator's output".into()
        },
        source_side_parsed: parsed,
    })
}

/// `ADD_PERMISSION` through production: a user holding only the scanned permission
/// is authorized and a user holding the other oracle permissions is not. The Ruby
/// side is the line scan (`ExtractionGap::ContractDeclScan`), not parsed ORE.
fn add_permission_evidence(ruby: &Ruby, roots: &Roots) -> Option<EvidenceRef> {
    let perm = ruby.scan.add_permission.clone()?;
    struct Holds(Vec<String>);
    impl UserContext for Holds {
        fn id(&self) -> Id {
            1
        }
        fn is_admin(&self) -> bool {
            false
        }
        fn is_anonymous(&self) -> bool {
            false
        }
        fn allowed_in_project(&self, p: &str, id: Id) -> bool {
            id == CASE_PROJECT && self.allowed_globally(p)
        }
        fn allowed_globally(&self, p: &str) -> bool {
            self.0.iter().any(|h| h == p)
        }
    }
    let base = |u: &Holds| {
        CreateWorkPackageService::new(u)
            .admit(Some(CASE_PROJECT), &[], &NoFacts)
            .errors
            .contains_key("base")
    };
    let others: Vec<String> = oracle_permissions()
        .into_iter()
        .filter(|p| *p != perm)
        .collect();
    let only = !base(&Holds(vec![perm.clone()]));
    let rest = base(&Holds(others));
    let rests_on = [
        roots.hash(FileRoot::Source, CREATE_CONTRACT),
        roots.hash(FileRoot::Target, GENERATED),
        roots.hash(FileRoot::Target, ADMISSION_RS),
    ]
    .into_iter()
    .collect::<Option<Vec<_>>>()?;
    Some(EvidenceRef {
        kind: EvidenceKind::DataWitness,
        by: "ore_slag::add_permission_evidence (production admit with exactly the scanned permission)".into(),
        revision: format!("{SCAN}; ruff {}", op_codegen_pipeline::RUFF_REV),
        rests_on,
        compared: 2,
        mismatches: usize::from(!only) + usize::from(!rest),
        detail: format!("holding only {perm:?} authorizes: {only}; holding every other oracle permission refuses: {rest}"),
        source_side_parsed: false,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// The schedule_manually block: a separate runtime case
// ─────────────────────────────────────────────────────────────────────────

struct ScheduleDiff {
    records: usize,
    ruby_fires: Vec<String>,
    rust_fires: Vec<String>,
    disagreements: Vec<String>,
}

/// Per `CreateContract` record: does Ruby record `schedule_manually:
/// cannot_be_automatically_scheduled`, and does production Rust admission produce it?
///
/// OpenProject registers the block with `validate(attribute, &block)`
/// (`app/contracts/base_contract.rb:89`), so it runs on every validation, whether or
/// not `schedule_manually` was submitted. Rust is asked both ways (not submitted,
/// submitted); either producing the code counts as Rust firing.
fn schedule_manually_differential() -> ScheduleDiff {
    let t = trace();
    let perms = oracle_permissions();
    let mut out = ScheduleDiff {
        records: 0,
        ruby_fires: Vec::new(),
        rust_fires: Vec::new(),
        disagreements: Vec::new(),
    };
    for c in create_cases(&t) {
        out.records += 1;
        let name = c["name"].as_str().expect("name").to_string();
        let ruby = c["errors"]["schedule_manually"]
            .as_array()
            .is_some_and(|v| v.iter().any(|d| d["error"] == SCHEDULE_CODE));
        let user = OracleUser::of(c["user"].as_str().expect("user"), &perms);
        let facts = TraceFacts(&c["facts"]);
        let rust = [&[][..], &["schedule_manually"][..]].iter().any(|sub| {
            CreateWorkPackageService::new(&user)
                .admit(Some(CASE_PROJECT), sub, &facts)
                .errors
                .get("schedule_manually")
                .is_some_and(|codes| codes.contains(&SCHEDULE_CODE))
        });
        if ruby {
            // The premise of the comparison: the validator's inputs as recorded.
            assert_eq!(c["facts"]["schedule_manually"], false, "{name}");
            assert_eq!(c["facts"]["children.any?"], false, "{name}");
            out.ruby_fires.push(name.clone());
        }
        if rust {
            out.rust_fires.push(name.clone());
        }
        if ruby != rust {
            out.disagreements
                .push(format!("{name}: ruby {ruby}, rust {rust}"));
        }
    }
    out
}

fn schedule_runtime(d: &ScheduleDiff) -> RuntimeEvidence {
    // Only discriminating records are compared (those where either side fires):
    // agreement on a record where both are silent would be vacuous.
    let compared = d.ruby_fires.len()
        + d.rust_fires
            .iter()
            .filter(|n| !d.ruby_fires.contains(n))
            .count();
    RuntimeEvidence::Executed {
        by: format!(
            "ore_slag schedule_manually differential: op_services CreateWorkPackageService::admit vs contract trace errors[schedule_manually] = {SCHEDULE_CODE}; {} CreateContract records, Ruby fires on {:?}, Rust on {:?}; separate from the 350",
            d.records, d.ruby_fires, d.rust_fires
        ),
        compared,
        mismatches: d.disagreements.len(),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Residuals (OpenProject-specific proposals as data)
// ─────────────────────────────────────────────────────────────────────────

fn rust_loc(row: &ReconciledRow) -> String {
    row.rust_span.as_ref().map_or("-".into(), |s| s.to_string())
}

fn ruby_loc(row: &ReconciledRow) -> String {
    match &row.spec.ruby {
        RubyEvidence::Fact { span, .. } | RubyEvidence::Absent { span, .. } => span.to_string(),
        RubyEvidence::NotHarvested { .. } => "-".into(),
    }
}

fn nexgen_rev(store: &CharonStore) -> String {
    format!(
        "nexgen {} (store files hash-checked against the checkout)",
        store
            .provenance
            .get("nexgen")
            .map_or("unknown", String::as_str)
    )
}

/// The proposal, sub-reason, owner and locator of one slag row.
fn residual_for(
    row: &ReconciledRow,
    reason: SlagReason,
    store: &CharonStore,
    src: Option<&std::path::Path>,
) -> Residual {
    let key = row.spec.key.to_string();
    let op = format!("openproject {}", op_rev());
    let locate_in = |file: &str, needle: &str| -> String {
        src.and_then(|s| std::fs::read_to_string(s.join(file)).ok())
            .and_then(|t| locate(&t, needle))
            .map_or(format!("{file} (OPENPROJECT_SRC not set)"), |l| {
                format!("{file}:{l}")
            })
    };
    let p = |missing: &str,
             supplier: &str,
             exists: bool,
             loc: String,
             rev: String,
             change: &str,
             validate: &str,
             neg: &str| Proposal {
        missing_fact: missing.into(),
        supplier: supplier.into(),
        supplier_exists: exists,
        source_location: loc,
        source_revision: rev,
        claim_change: change.into(),
        validating_evidence: validate.into(),
        negative_control: neg.into(),
    };
    let (proposal, sub, owner) = if key == KEY_PERMISSION {
        (
            p(
                "a parsed source fact for the create permission (the generator reads it with a line scan)",
                "ruff lowering of the permission literal in CreateContract#user_allowed_to_add, fed to admission_gen",
                false,
                ruby_loc(row),
                op.clone(),
                "lifecycle EVIDENCED -> VERIFIED(DataWitness); structural verdict stays Slag(RepresentedAsData)",
                "ADD_PERMISSION equals the parsed permission; the drift check; runtime authorization (6 records, 0 mismatches)",
                "change the literal in a copy of create_contract.rb: the drift check and this witness must fail",
            ),
            "generated constant; its source side is ExtractionGap::ContractDeclScan, not parsed ORE".to_string(),
            OwnerStage::DataWitness,
        )
    } else if key == KEY_WRITABLE {
        (
            p(
                "BaseContract#writable? as a lowered fact (writable condition AND attribute permission)",
                "ruff contract arm: lower BaseContract#writable?",
                false,
                locate_in(BASE_CONTRACT, "def writable?"),
                op.clone(),
                "Slag(RubySideNotHarvested) -> Agreement if the lowered body is 'condition AND permission' and admit_create -> is_writable stays CALLS_RESOLVED",
                "the lowered fact against the CALLS_RESOLVED edge; runtime axis: create_transition_differential contract-wide writable? (332 pairs)",
                "a lowering that yields 'condition OR permission' must not agree with is_writable's truth table",
            ),
            "framework-level method of the application's base contract; no extractor reads it".to_string(),
            OwnerStage::ContractHarvest,
        )
    } else if key == KEY_API {
        (
            p(
                "a structured body for op_api::handlers::work_packages::create_work_package_core",
                "Charon structuring of async bodies (op-api is in the store; the function is defined, listed in residuals.tsv as BODY_NOT_STRUCTURED), or a synchronous seam the call graph can see",
                false,
                format!("crates/op-api/src/handlers/work_packages.rs:{}", locate(&std::fs::read_to_string(root().join("crates/op-api/src/handlers/work_packages.rs")).unwrap_or_default(), "pub async fn create_work_package_core").unwrap_or(0)),
                nexgen_rev(store),
                "Slag(RustBodyNotStructured) -> RubySideNotHarvested once the edge create_work_package_core -> mutation::create is structured: the API v3 create endpoint is not harvested either, so Rust structuring alone does not decide the row",
                "the CALLS_RESOLVED edge create_work_package_core -> mutation::create, plus a harvested endpoint",
                "remove the call from a structured body: the row must not stay agreeing; drop op-api again: the row returns to RustSideNotExtracted, never Disagreement",
            ),
            "Charon residual BODY_NOT_STRUCTURED (async fn); op-api extracted, no edge visible".to_string(),
            OwnerStage::RustExtraction,
        )
    } else if reason == SlagReason::RustBodyNotStructured {
        let charon = row
            .rust_span
            .as_ref()
            .and_then(|_| store.residual(MUTATION_CREATE))
            .unwrap_or("?")
            .to_string();
        (
            p(
                "a structured body for op_services::work_packages::mutation::create",
                "Charon structuring of async bodies, or a synchronous seam the call graph can see",
                false,
                rust_loc(row),
                nexgen_rev(store),
                "Slag(RustBodyNotStructured) -> Agreement or RubyOnly, decided by the structured edge",
                "the CALLS_RESOLVED edge; the runtime axis stays the EXECUTED_DB journey (cited here; needs PostgreSQL)",
                "remove the call from a structured body: the row must become RubyOnly, never stay Agreement",
            ),
            if key == KEY_PERSIST {
                format!("Charon residual {charon}; the create is transactional and writes its creation journal (EXECUTED_DB steps 2, 7)")
            } else {
                format!("Charon residual {charon}")
            },
            OwnerStage::RustExtraction,
        )
    } else {
        // A lowered `writable:` guard held as generated data.
        (
            p(
                "none for the behaviour: table data is invisible to a call graph",
                "data witness: ruff's GuardExpr truth table against production is_writable over the generated ATTRIBUTES",
                true,
                format!("{} ; {}", ruby_loc(row), rust_loc(row)),
                format!("{op}; {}", nexgen_rev(store)),
                "none structurally (stays Slag(RepresentedAsData)); lifecycle -> RESOLVED(DataWitness)",
                "this witness, bound to the generator drift check",
                "swap And/Or in the ruff expression, or rotate the attribute mapping: the witness must fail",
            ),
            "generated table row (ATTRIBUTES)".to_string(),
            OwnerStage::DataWitness,
        )
    };
    let mut r = Residual::new(key, reason, proposal);
    r.sub_reason = sub;
    r.owner_stage = owner;
    r.locator = format!("ruby {}; rust {}", ruby_loc(row), rust_loc(row));
    r
}

/// Residuals for every slag row, with the evidence this run can attach, evaluated.
fn residuals(
    rec: &Reconciliation,
    store: &CharonStore,
    ruby: Option<&Ruby>,
    src: Option<&std::path::Path>,
    bind: &WitnessBinding,
    roots: &Roots,
) -> Vec<Residual> {
    let drift = src.and_then(|s| drift_evidence(s, roots, false));
    let mut out = Vec::new();
    for row in &rec.rows {
        let Verdict::Slag(reason) = row.verdict else {
            continue;
        };
        let mut r = residual_for(row, reason, store, src);
        if reason == SlagReason::RepresentedAsData {
            if let Some(d) = &drift {
                r.evidence.push(d.clone());
            }
            if row.spec.key.to_string() == KEY_PERMISSION {
                if let Some(e) = ruby.and_then(|rb| add_permission_evidence(rb, roots)) {
                    r.evidence.push(e);
                }
            } else if let Some(e) = ruby.and_then(|rb| guard_witness(row, &rb.guards, bind, roots))
            {
                r.evidence.push(e);
                r.regression_gate = Some(format!("{WITNESS_GATE}; {DRIFT_GATE}"));
                // The witness resolves only together with a passing drift check: the
                // table it compared must be the one the generator produces.
                r.required_companions = vec![EvidenceKind::DriftCheck];
            }
        }
        r.evaluate(roots);
        out.push(r);
    }
    out
}

fn status_of<'a>(rs: &'a [Residual], key: &str) -> &'a Residual {
    rs.iter()
        .find(|r| r.key == key)
        .unwrap_or_else(|| panic!("no residual {key}"))
}

fn full_reconciliation(
    store: &CharonStore,
    ruby: Option<&Ruby>,
) -> (Reconciliation, Option<ScheduleDiff>) {
    let mut rec = reconcile(store, ruby);
    attach_runtime(&mut rec);
    let diff = schedule_manually_differential();
    assert!(rec.attach_runtime(KEY_SCHEDULE, schedule_runtime(&diff)));
    (rec, Some(diff))
}

// ─────────────────────────────────────────────────────────────────────────
// The tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn ore_slag_ledger_and_lifecycle() {
    let store = CharonStore::load(&store_dir()).expect("Charon fixture loads");
    let src = source();
    let ruby = src.as_deref().map(Ruby::load);
    let roots = roots(src.as_deref());
    let (rec, diff) = full_reconciliation(&store, ruby.as_ref());
    let diff = diff.expect("diff");
    let verdicts = |rec: &Reconciliation| -> Vec<(String, String)> {
        rec.rows
            .iter()
            .map(|r| (r.spec.key.to_string(), r.verdict.to_string()))
            .collect()
    };
    let verdicts_before = verdicts(&rec);

    let rs = residuals(
        &rec,
        &store,
        ruby.as_ref(),
        src.as_deref(),
        &FAITHFUL,
        &roots,
    );

    // Lifecycle and runtime never move a structural verdict: the verdicts of the
    // reconciliation that the residuals were built and evaluated from, and the
    // verdicts the ledger reports for them, equal the verdicts before any residual
    // existed.
    assert_eq!(verdicts(&rec), verdicts_before);
    let ledger = render_ledger(&rec, &rs);
    let doc: Value = serde_json::from_str(&ledger).expect("ledger is JSON");
    let ledger_verdicts: Vec<(String, String)> = doc["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .map(|c| {
            (
                c["key"].as_str().expect("key").to_string(),
                c["structural_verdict"]
                    .as_str()
                    .expect("verdict")
                    .to_string(),
            )
        })
        .collect();
    assert_eq!(ledger_verdicts, verdicts_before);
    assert!(
        rs.iter().any(|r| r.status != Lifecycle::Open),
        "vacuous: no residual moved off OPEN, so no lifecycle could have moved a verdict"
    );
    let slag = rec
        .rows
        .iter()
        .filter(|r| matches!(r.verdict, Verdict::Slag(_)))
        .count();
    assert_eq!(rs.len(), slag, "every slag row has exactly one residual");

    // The schedule_manually block: Ruby fires on exactly one record and Rust fires on
    // the same one (only discriminating records are compared). The structural row is
    // an Agreement on the call: ruff's `extract_attribute_blocks` harvests the block,
    // so the row is no longer slag and has no residual. The condition is not part of
    // the structural comparison (see the claim's repair text); this differential is its
    // witness.
    eprintln!(
        "schedule_manually block: {} CreateContract records; Ruby emits {SCHEDULE_CODE} on {:?}; Rust on {:?}; disagreements {:?}",
        diff.records, diff.ruby_fires, diff.rust_fires, diff.disagreements
    );
    assert_eq!(
        diff.ruby_fires,
        ["leaf_automatic"],
        "the oracle's discriminating record moved"
    );
    assert_eq!(
        diff.rust_fires, diff.ruby_fires,
        "Rust must emit {SCHEDULE_CODE} on exactly the record Ruby does"
    );
    assert!(diff.disagreements.is_empty(), "{:?}", diff.disagreements);
    let sched = rec.row(KEY_SCHEDULE).expect("row");
    assert_eq!(sched.verdict, Verdict::Agreement);
    assert!(
        rs.iter().all(|r| r.key != KEY_SCHEDULE),
        "an Agreement row has no residual"
    );
    assert_eq!(
        RuntimeVerdict::of(&sched.runtime),
        RuntimeVerdict::ExecutedAgreement
    );
    assert!(
        !op_contracts::work_packages::not_evaluated_validators()
            .contains(&"validate_has_predecessors_or_children"),
        "the validator is evaluated by admit_create, so it is not listed as not evaluated"
    );

    // Rust-side residuals that no witness here can supply stay open or candidate.
    assert_eq!(
        status_of(&rs, KEY_BOUNDARY_PERMISSION).status,
        Lifecycle::Open
    );
    assert_eq!(status_of(&rs, KEY_PERSIST).status, Lifecycle::Open);
    assert_eq!(status_of(&rs, KEY_API).status, Lifecycle::Open);
    assert_eq!(status_of(&rs, KEY_WRITABLE).status, Lifecycle::Open);

    // Nothing resolves except a RepresentedAsData row, and only at DataWitness level.
    for r in &rs {
        if let Lifecycle::Resolved(l) | Lifecycle::Verified(l) = r.status {
            assert_eq!(r.reason, SlagReason::RepresentedAsData, "{}", r.key);
            assert_eq!(l, ResolutionLevel::DataWitness, "{}", r.key);
            let row = rec.row(&r.key).expect("row");
            assert_eq!(
                row.verdict,
                Verdict::Slag(SlagReason::RepresentedAsData),
                "upgraded: {}",
                r.key
            );
        }
    }

    match &ruby {
        Some(_) => {
            assert_eq!(rec.rows.len(), 21);
            let counts = rec.counts();
            assert_eq!(
                (counts.get("Agreement"), counts.get("Slag"), counts.get("Disagreement")),
                (Some(&5), Some(&14), None)
            );
            let resolved: Vec<&Residual> = rs
                .iter()
                .filter(|r| r.status == Lifecycle::Resolved(ResolutionLevel::DataWitness))
                .collect();
            for r in &resolved {
                assert_eq!(
                    RuntimeVerdict::of(&rec.row(&r.key).expect("row").runtime),
                    RuntimeVerdict::ExecutedAgreement,
                    "{}",
                    r.key
                );
            }
            assert_eq!(resolved.len(), 9, "the nine lowered guard rows resolve as data");
            assert_eq!(status_of(&rs, KEY_PERMISSION).status, Lifecycle::Evidenced);
        }
        None => eprintln!("UNVERIFIED: the data witness, the ledger and the Ruby-side counts need OPENPROJECT_SRC"),
    }

    let card = Scorecard::of(&rec, &rs);
    eprintln!("== ORE/SLAG SCORECARD (WorkPackage create admission)\n{card}");
    eprintln!("   schedule_manually differential: 1 discriminating record compared, 0 disagreements (separate from the 350 contract-rule items)");
    eprintln!("   database effects performed here: 0 (EXECUTED_DB is cited: needs OP_TEST_DATABASE_URL); callback bodies executed: 0");
    eprintln!("   P3b: create is transactional (step 7: a post-INSERT failure leaves no row); it writes its creation journal on that transaction (step 2) and still ports no watcher, ancestor update or reschedule");
    for r in &rs {
        eprintln!(
            "   {} [{:?} / {:?}] {}",
            r.status, r.reason, r.owner_stage, r.key
        );
        for a in &r.attempts {
            eprintln!("       rejected: {}", a.rejected_because);
        }
    }

    if ruby.is_some() {
        let path = root().join(LEDGER);
        if std::env::var("UPDATE_RECONCILE_LEDGER").as_deref() == Ok("1") {
            std::fs::write(&path, &ledger).expect("write ledger");
        }
        assert!(
            std::fs::read_to_string(&path).unwrap_or_default() == ledger,
            "{LEDGER} is stale: rerun with UPDATE_RECONCILE_LEDGER=1"
        );
    }
}

/// A copy of the store with one `files.tsv` hash corrupted.
fn store_with_corrupt_hash(path: &str) -> CharonStore {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("charon-corrupt-hash");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let mut hit = 0;
    for entry in std::fs::read_dir(store_dir()).expect("store dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut text = std::fs::read_to_string(entry.path()).unwrap_or_default();
        if name.ends_with(".files.tsv") {
            text = text
                .lines()
                .map(|l| match l.split_once('\t') {
                    Some((p, _)) if p == path => {
                        hit += 1;
                        format!("{p}\t0000000000000000")
                    }
                    _ => l.to_string(),
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
        }
        std::fs::write(dir.join(&name), text).expect("write");
    }
    assert_eq!(hit, 1, "the corrupted path is in exactly one files.tsv");
    CharonStore::load(&dir).expect("corrupt store loads")
}

#[test]
fn ore_slag_negative_controls() {
    let store = CharonStore::load(&store_dir()).expect("Charon fixture loads");
    let Some(src) = source() else {
        eprintln!("UNVERIFIED: the resolution negative controls need OPENPROJECT_SRC");
        return;
    };
    let ruby = Ruby::load(&src);
    let roots = roots(Some(&src));
    let (rec, _) = full_reconciliation(&store, Some(&ruby));
    let real = residuals(&rec, &store, Some(&ruby), Some(&src), &FAITHFUL, &roots);
    let resolved = |rs: &[Residual]| {
        rs.iter()
            .filter(|r| matches!(r.status, Lifecycle::Resolved(_) | Lifecycle::Verified(_)))
            .count()
    };
    assert_eq!(resolved(&real), 9);

    // (1) Remove a Charon artifact: rows become slag, never a fabricated
    // disagreement, and no data-witness resolution carries over to the new reason.
    let without = store_without(&["op_contracts"]);
    let rec1 = reconcile(&without, Some(&ruby));
    let rs1 = residuals(&rec1, &without, Some(&ruby), Some(&src), &FAITHFUL, &roots);
    assert_eq!(rec1.counts().get("Disagreement"), None);
    assert_eq!(
        resolved(&rs1),
        0,
        "a resolution survived the loss of the Charon artifact"
    );
    eprintln!(
        "control 1 (no op_contracts artifacts): {} residuals, 0 resolved, 0 Disagreement",
        rs1.len()
    );

    // (2) Remove the Ruby extractor result. (a) No source at all: nothing agrees or
    // resolves. (b) One guard row's ruff fact removed: the data witness has no input,
    // so that residual does not resolve (its structural verdict stays slag).
    let rec2 = reconcile(&store, None);
    let rs2 = residuals(
        &rec2,
        &store,
        None,
        None,
        &FAITHFUL,
        &Roots {
            source: None,
            target: Some(root()),
        },
    );
    assert_eq!(rec2.counts().get("Agreement"), None);
    assert_eq!(rec2.counts().get("Disagreement"), None);
    assert_eq!(resolved(&rs2), 0);
    let mut rec2b = rec.clone();
    let k = real
        .iter()
        .find(|r| matches!(r.status, Lifecycle::Resolved(_)))
        .expect("a resolved row")
        .key
        .clone();
    let row = rec2b
        .rows
        .iter_mut()
        .find(|r| r.spec.key.to_string() == k)
        .expect("row");
    row.spec.ruby = RubyEvidence::NotHarvested {
        why: "control: ruff result removed".into(),
    };
    let rs2b = residuals(&rec2b, &store, Some(&ruby), Some(&src), &FAITHFUL, &roots);
    assert!(!matches!(
        status_of(&rs2b, &k).status,
        Lifecycle::Resolved(_) | Lifecycle::Verified(_)
    ));
    assert!(matches!(
        rec2b.row(&k).expect("row").verdict,
        Verdict::Slag(_)
    ));
    eprintln!(
        "control 2: no source -> 0 Agreement, 0 resolved; {k} without its ruff fact -> {}",
        status_of(&rs2b, &k).status
    );

    // (3) Corrupt a revision hash. (a) In the evidence: rejected as stale, retained as
    // an attempt, the residual falls back. (b) In the Charon store: rows on that file
    // become RustEvidenceStale, never a disagreement.
    let mut corrupt = real.clone();
    let mut fell_back = 0;
    for r in corrupt
        .iter_mut()
        .filter(|r| matches!(r.status, Lifecycle::Resolved(_)))
    {
        for e in &mut r.evidence {
            for f in &mut e.rests_on {
                if f.root == FileRoot::Target && f.path == GENERATED {
                    f.fnv64 ^= 1;
                }
            }
        }
        r.evaluate(&roots);
        assert!(
            r.attempts
                .iter()
                .any(|a| a.rejected_because.starts_with("stale")),
            "{}",
            r.key
        );
        fell_back += usize::from(!matches!(
            r.status,
            Lifecycle::Resolved(_) | Lifecycle::Verified(_)
        ));
    }
    assert_eq!(fell_back, 9);
    let stale_store = store_with_corrupt_hash(GENERATED);
    let rec3 = reconcile(&stale_store, Some(&ruby));
    let stale_rows = rec3
        .rows
        .iter()
        .filter(|r| r.verdict == Verdict::Slag(SlagReason::RustEvidenceStale))
        .count();
    assert_eq!(
        stale_rows, 10,
        "ADD_PERMISSION + nine guard rows rest on the generated file"
    );
    assert_eq!(rec3.counts().get("Disagreement"), None);
    eprintln!("control 3: corrupted evidence hash -> 9 of 9 resolutions rejected as stale; corrupted store hash -> {stale_rows} rows RustEvidenceStale, 0 Disagreement");

    // (4) Break a semantic mapping. (a) The witness asks the wrong Rust attribute
    // (rotated): it fails. (b) A join probe names a wrong callee: the agreement
    // becomes RubyOnly, not a silent agreement.
    let rotated = WitnessBinding {
        attribute: |all, g| {
            let i = all.iter().position(|x| x == g).expect("in list");
            all[(i + 1) % all.len()].attribute.clone()
        },
        expr: GuardExpr::clone,
    };
    let rs4 = residuals(&rec, &store, Some(&ruby), Some(&src), &rotated, &roots);
    assert!(
        resolved(&rs4) < 9,
        "a broken attribute mapping still resolved every guard"
    );
    let mut specs = claims(Some(&ruby));
    let entry = specs
        .iter_mut()
        .find(|s| s.key.to_string() == KEY_ENTRY)
        .expect("entry");
    if let RustProbe::Edges { edges, .. } = &mut entry.rust {
        edges[1].1 = format!("{ADMIT_CREATE}_renamed");
    }
    let broken = Reconciler::new(&store, Some(&root())).decide(entry);
    assert_eq!(broken.verdict, Verdict::RubyOnly);
    eprintln!(
        "control 4: rotated attribute mapping -> {} of 9 resolve; renamed callee -> {}",
        resolved(&rs4),
        broken.verdict
    );

    // (5) Disable a data-witness rule: compare against an expression whose `Or` is
    // read as `And`. Every guard with a connective must fail.
    let swapped = WitnessBinding {
        attribute: |_, g| g.attribute.clone(),
        expr: |e| {
            fn swap(e: &GuardExpr) -> GuardExpr {
                match e {
                    GuardExpr::Or(a, b) => GuardExpr::And(Box::new(swap(a)), Box::new(swap(b))),
                    GuardExpr::And(a, b) => GuardExpr::Or(Box::new(swap(a)), Box::new(swap(b))),
                    GuardExpr::Not(a) => GuardExpr::Not(Box::new(swap(a))),
                    x => x.clone(),
                }
            }
            swap(e)
        },
    };
    let rs5 = residuals(&rec, &store, Some(&ruby), Some(&src), &swapped, &roots);
    let with_connective = effective_guards(&ruby.guards)
        .iter()
        .filter(|g| {
            matches!(
                &g.outcome,
                GuardOutcome::Lowered(GuardExpr::Or(..) | GuardExpr::And(..))
            )
        })
        .count();
    assert!(with_connective >= 5);
    assert_eq!(resolved(&rs5), 9 - with_connective);
    eprintln!("control 5: And/Or swapped in the witness input -> {with_connective} guards fail, {} resolve", resolved(&rs5));

    // (8) The drift check is a required companion of the guard rows. A failing or
    // missing drift check keeps the witness from resolving them, even though the
    // witness itself passes.
    let guard_rows: Vec<&Residual> = real
        .iter()
        .filter(|r| matches!(r.status, Lifecycle::Resolved(_)))
        .collect();
    assert_eq!(guard_rows.len(), 9);
    for r in &guard_rows {
        assert_eq!(
            r.required_companions,
            [EvidenceKind::DriftCheck],
            "{}",
            r.key
        );
        let drift_failing = |mutate: fn(&mut Vec<EvidenceRef>)| {
            let mut x = (*r).clone();
            mutate(&mut x.evidence);
            x.evaluate(&roots);
            x
        };
        let failing = drift_failing(|ev| {
            for e in ev.iter_mut().filter(|e| e.kind == EvidenceKind::DriftCheck) {
                e.mismatches = 1;
            }
        });
        assert_eq!(failing.status, Lifecycle::Evidenced, "{}", r.key);
        assert_eq!(failing.attempts.len(), 1, "{}", r.key);
        let missing = drift_failing(|ev| ev.retain(|e| e.kind != EvidenceKind::DriftCheck));
        assert_eq!(missing.status, Lifecycle::Evidenced, "{}", r.key);
    }
    eprintln!("control 8: a failing or missing drift check keeps all 9 guard rows at EVIDENCED");

    // (9) The schedule_manually block claim agrees only on the harvested fact. A
    // different call name, a missing block, a dropped condition or another condition
    // each leave the Ruby side unharvested, so the same Rust edge no longer agrees.
    let decide = |blocks: &[AttributeBlock], validator: &str| {
        let mut specs = claims(Some(&ruby));
        let spec = specs
            .iter_mut()
            .find(|s| s.key.to_string() == KEY_SCHEDULE)
            .expect("schedule row");
        spec.ruby = schedule_block_evidence(blocks, validator);
        Reconciler::new(&store, Some(&root())).decide(spec).verdict
    };
    let real_blocks = &ruby.blocks;
    assert_eq!(decide(real_blocks, SCHEDULE_VALIDATOR), Verdict::Agreement);
    assert_eq!(
        decide(real_blocks, "validate_something_else"),
        Verdict::Slag(SlagReason::RubySideNotHarvested),
        "a different call name must not agree"
    );
    assert_eq!(
        decide(&[], SCHEDULE_VALIDATOR),
        Verdict::Slag(SlagReason::RubySideNotHarvested),
        "a missing block must not agree"
    );
    let mut dropped = real_blocks.clone();
    for b in &mut dropped {
        for c in &mut b.calls {
            c.condition = None;
        }
    }
    assert_eq!(
        decide(&dropped, SCHEDULE_VALIDATOR),
        Verdict::Slag(SlagReason::RubySideNotHarvested),
        "an unconditional call is not the claimed fact"
    );
    let mut other = real_blocks.clone();
    for b in &mut other {
        for c in &mut b.calls {
            if let Some(cond) = &mut c.condition {
                cond.outcome = GuardOutcome::Lowered(GuardExpr::Fact("model.leaf?".into()));
            }
        }
    }
    assert_eq!(
        decide(&other, SCHEDULE_VALIDATOR),
        Verdict::Slag(SlagReason::RubySideNotHarvested),
        "another condition is not the claimed fact"
    );
    // The Rust side still decides: a renamed callee turns the harvested fact into RubyOnly.
    let mut specs = claims(Some(&ruby));
    let spec = specs
        .iter_mut()
        .find(|s| s.key.to_string() == KEY_SCHEDULE)
        .expect("schedule row");
    if let RustProbe::Edges { edges, .. } = &mut spec.rust {
        edges[0].1 = format!("{}_renamed", edges[0].1);
    }
    assert_eq!(
        Reconciler::new(&store, Some(&root())).decide(spec).verdict,
        Verdict::RubyOnly
    );
    eprintln!("control 9: schedule_manually block claim: wrong call / no block / no condition / other condition -> Slag(RubySideNotHarvested); renamed Rust callee -> RubyOnly");

    // (6) Disabling the Rust behaviour is a live run (edit admission.rs, observe,
    // git checkout): see docs/ore-slag-reconciliation.md. Not reproducible in-test
    // without editing production code.

    // (7) An unsupported generated callback: injected into WorkPackage's effective
    // chain, it matches no journal entry and is classified Unexplained, which the
    // create differential reports (it asserts that class is empty).
    let svc = json_file("work-package-service-trace.json");
    let models: BTreeSet<String> = svc["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .flat_map(|c| c["journal"].as_array().expect("journal").iter())
        .map(|e| e["model"].as_str().expect("model").to_string())
        .collect();
    let cfg = cb::intake_cfg();
    let mut h = cb::harvest_for(&src, &cfg);
    let patches = cb::discover_patches(&src, &cfg, &mut h);
    let (ctxs, _) = cb::contexts(&mut h, &models, &patches);
    let injected = cb::Declared {
        chain: "save".into(),
        kind: "before".into(),
        filter: cb::Filter::Method("nexgen_control_unsupported_generated".into()),
        origin: "WorkPackage".into(),
        options: Vec::new(),
        implied_on: None,
        generated: Some(cb::GeneratedFrom {
            association: "nexgen_control".into(),
            rule: "control: no Rails rule".into(),
        }),
    };
    let mut effective = ctxs["WorkPackage"].effective.clone();
    let before = effective.len();
    effective.push(injected.clone());
    let case = svc["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["name"] == "create_member")
        .expect("case");
    let mut chains = BTreeSet::new();
    let mut matched = false;
    for e in case["journal"].as_array().expect("journal") {
        if e["model"] != "WorkPackage" {
            continue;
        }
        if let Some(fired) = cb::parse_fired(
            "WorkPackage",
            e["phase"].as_str().expect("p"),
            e["callback"].as_str().expect("c"),
        ) {
            for f in fired {
                chains.insert(f.chain.clone());
                matched |= cb::declared_matches(&injected, &f);
            }
        }
    }
    assert!(!matched, "the injected callback matched a journal entry");
    assert_eq!(
        cb::silent_reason(&injected, &chains),
        cb::Silent::Unexplained
    );
    eprintln!("control 7: injected generated callback ({before} -> {} declarations) matches nothing and is reported as Silent::Unexplained", effective.len());
}

fn json_file(name: &str) -> Value {
    let text = std::fs::read_to_string(root().join("oracle").join(name)).expect("oracle file");
    serde_json::from_str(&text).expect("JSON")
}
