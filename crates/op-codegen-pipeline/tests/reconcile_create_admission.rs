//! Three-witness reconciliation of OpenProject `WorkPackage` create admission:
//!
//! 1. the Ruby structural witness: what `WorkPackages::CreateContract` declares, as the
//!    existing extractors produce it (`admission_gen`'s permission scan, ruff's
//!    `extract_contract_guards`), with spans located in the checkout;
//! 2. the Charon structural witness: the frozen store in `oracle/charon/`
//!    (op-contracts and op-services only; see `PROVENANCE.txt`);
//! 3. the runtime axis: production Rust executed against the frozen contract trace in
//!    `oracle/`, plus gates elsewhere that are cited, not run.
//!
//! The verdict is per semantic claim. The runtime axis is attached after the verdicts
//! and asserted not to change them.
//!
//! Needs `OPENPROJECT_SRC` for the Ruby witness. Without it the Ruby side is
//! `NotHarvested`, the Charon-only assertions (b), (d) and the silence twin still run,
//! and (a) and the report check print UNVERIFIED; `REQUIRE_SOURCES=1` makes a missing
//! checkout a failure. `UPDATE_RECONCILE_REPORT=1` rewrites the checked-in report.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use op_codegen_pipeline::admission_gen::{contract_scan_facts, ContractScanFacts};
use op_codegen_pipeline::reconcile::{
    crate_of, locate, render_markdown, CharonStore, ClaimSpec, Comparability, JoinKey, Level,
    Reconciler, Reconciliation, Revisions, RubyEvidence, RuntimeEvidence, RustProbe, SlagReason,
    SourceSpan, Verdict,
};
use op_codegen_pipeline::RUFF_REV;
use op_contracts::base::UserContext;
use op_contracts::work_packages::{is_writable, AdmissionFacts, Writable};
use op_core::traits::Id;
use op_services::work_packages::CreateWorkPackageService;
use ruff_ruby_spo::{extract_contract_guards, ContractGuard, GuardOutcome};
use serde_json::Value;

const CREATE_CONTRACT: &str = "app/contracts/work_packages/create_contract.rb";
const CREATE_SERVICE: &str = "app/services/work_packages/create_service.rb";
const GENERATED: &str = "crates/op-contracts/src/work_packages/generated_create_admission.rs";
const REPORT: &str = "oracle/charon/reconcile-create-admission.md";

// Rust symbols, in the normalized form (`reconcile::normalize`).
const SVC: &str = "op_services::work_packages::create::CreateWorkPackageService";
const ADMIT_CREATE: &str = "op_contracts::work_packages::admission::admit_create";
const USER_ALLOWED_TO_ADD: &str = "op_contracts::work_packages::admission::user_allowed_to_add";
const IS_WRITABLE: &str = "op_contracts::work_packages::admission::is_writable";
const MUTATION_CREATE: &str = "op_services::work_packages::mutation::create";
const API_CREATE: &str = "op_api::handlers::work_packages::create_work_package_core";

// Join keys of the rows the assertions name.
const KEY_ENTRY: &str = "openproject/WorkPackage/create/admission/CreateWorkPackageService.call/evaluates:WorkPackages::CreateContract";
const KEY_BOUNDARY_PERMISSION: &str =
    "openproject/WorkPackage/create/admission/mutation::create/checks:add_work_packages-in-project";
const KEY_PERMISSION: &str =
    "openproject/WorkPackage/create/admission/user_allowed_to_add/requires:add_work_packages";

/// The oracle's project handle: the test user answers for it and nothing else.
const CASE_PROJECT: Id = 7;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn store_dir() -> PathBuf {
    root().join("oracle/charon")
}

fn source() -> Option<PathBuf> {
    match std::env::var("OPENPROJECT_SRC") {
        Ok(s) if !s.is_empty() => Some(PathBuf::from(s)),
        _ => {
            assert!(
                std::env::var("REQUIRE_SOURCES").as_deref() != Ok("1"),
                "REQUIRE_SOURCES=1 but OPENPROJECT_SRC is not set"
            );
            None
        }
    }
}

fn key(phase: &str, method: &str, effect: &str) -> JoinKey {
    JoinKey {
        application: "openproject".into(),
        model: "WorkPackage".into(),
        operation: "create".into(),
        phase: phase.into(),
        method: method.into(),
        effect: effect.into(),
    }
}

fn edges(e: &[(&str, &str)]) -> RustProbe {
    RustProbe::Edges {
        edges: e.iter().map(|(a, b)| ((*a).into(), (*b).into())).collect(),
        conflicts: Vec::new(),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The Ruby witness
// ─────────────────────────────────────────────────────────────────────────

struct Ruby {
    contract: String,
    service: String,
    scan: ContractScanFacts,
    guards: Vec<ContractGuard>,
}

impl Ruby {
    fn load(src: &Path) -> Self {
        let read =
            |p: &str| std::fs::read_to_string(src.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
        Ruby {
            contract: read(CREATE_CONTRACT),
            service: read(CREATE_SERVICE),
            scan: contract_scan_facts(src),
            guards: extract_contract_guards(&src.join("app/contracts")),
        }
    }

    /// A fact the extractor produced (`produced`), located at the first `needle` in
    /// `file`. Not produced, or not locatable: not harvested.
    fn fact(
        &self,
        produced: bool,
        extractor: &str,
        file: &str,
        needle: &str,
        fact: &str,
    ) -> RubyEvidence {
        let text = if file == CREATE_SERVICE {
            &self.service
        } else {
            &self.contract
        };
        match (produced, locate(text, needle)) {
            (true, Some(line)) => RubyEvidence::Fact {
                extractor: extractor.into(),
                span: SourceSpan {
                    file: file.into(),
                    line_start: line,
                    line_end: line,
                },
                fact: fact.into(),
            },
            (false, _) => RubyEvidence::NotHarvested {
                why: format!("{extractor} did not produce: {fact}"),
            },
            (true, None) => RubyEvidence::NotHarvested {
                why: format!("{needle:?} not found in {file}"),
            },
        }
    }

    /// The body of `def user_allowed_to_add` (to its `end` at the same indent).
    fn add_body(&self) -> Option<(usize, usize, String)> {
        let lines: Vec<&str> = self.contract.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.trim_start().starts_with("def user_allowed_to_add"))?;
        let indent = lines[start].len() - lines[start].trim_start().len();
        let end = (start + 1..lines.len()).find(|&i| {
            lines[i].trim() == "end" && lines[i].len() - lines[i].trim_start().len() == indent
        })?;
        Some((start + 1, end + 1, lines[start..=end].join("\n")))
    }
}

fn not_harvested(why: &str) -> RubyEvidence {
    RubyEvidence::NotHarvested { why: why.into() }
}

const SCAN: &str = "admission_gen::contract_scan_facts (ExtractionGap::PermissionDeclScan)";
const LOCATED: &str = "literal located in the source (no parse)";
const RUFF: &str = "ruff_ruby_spo::extract_contract_guards";

/// The ruff guards that decide create admission: `CreateContract` overrides
/// `BaseContract`, per attribute (the generator's `guard_for`).
fn effective_guards(guards: &[ContractGuard]) -> Vec<&ContractGuard> {
    let canon = |a: &str| a.strip_suffix("_id").unwrap_or(a).to_string();
    let mut by: BTreeMap<String, &ContractGuard> = BTreeMap::new();
    for class in ["WorkPackages::BaseContract", "WorkPackages::CreateContract"] {
        for g in guards.iter().filter(|g| g.class == class) {
            by.insert(canon(&g.attribute), g);
        }
    }
    by.into_values().collect()
}

// ─────────────────────────────────────────────────────────────────────────
// The claims (OpenProject-specific data)
// ─────────────────────────────────────────────────────────────────────────

fn claims(ruby: Option<&Ruby>) -> Vec<ClaimSpec> {
    let none = || not_harvested("OPENPROJECT_SRC not set");
    let has_validator = |r: &Ruby| r.scan.validators.iter().any(|v| v == "user_allowed_to_add");
    let add_perm = |r: &Ruby| r.scan.add_permission.as_deref() == Some("add_work_packages");
    let add_body = ruby.and_then(Ruby::add_body);
    let in_body = |needle: &str| {
        add_body
            .as_ref()
            .is_some_and(|(_, _, b)| b.contains(needle))
    };

    let mut v = vec![
        ClaimSpec {
            key: key("registration", "CreateContract.validate(:user_allowed_to_add)", "declares-validator"),
            level: Level::Registration,
            ruby: ruby.map_or_else(none, |r| {
                r.fact(has_validator(r), SCAN, CREATE_CONTRACT, "validate :user_allowed_to_add", "CreateContract registers user_allowed_to_add as a validator")
            }),
            rust: edges(&[(ADMIT_CREATE, USER_ALLOWED_TO_ADD)]),
            comparability: Comparability::Different {
                ruby: "DSL registration of a validator".into(),
                rust: "no registration step; admit_create calls the function".into(),
            },
            repair: "none: different levels; the invocation is its own row".into(),
        },
        ClaimSpec {
            key: key("callback", "CreateContract#validate", "invokes:user_allowed_to_add"),
            level: Level::CallbackInvocation,
            ruby: ruby.map_or_else(none, |r| {
                r.fact(has_validator(r), SCAN, CREATE_CONTRACT, "validate :user_allowed_to_add", "validating the create contract runs user_allowed_to_add")
            }),
            rust: edges(&[(ADMIT_CREATE, USER_ALLOWED_TO_ADD)]),
            comparability: Comparability::Same,
            repair: "-".into(),
        },
        ClaimSpec {
            key: key("admission", "CreateWorkPackageService.call", "evaluates:WorkPackages::CreateContract"),
            level: Level::Admission,
            ruby: ruby.map_or_else(none, |r| {
                r.fact(has_validator(r), &format!("{LOCATED} + {SCAN}"), CREATE_SERVICE, "contract_class: WorkPackages::CreateContract", "CreateService admits a create through WorkPackages::CreateContract")
            }),
            rust: edges(&[(&format!("{SVC}::call"), &format!("{SVC}::admit")), (&format!("{SVC}::admit"), ADMIT_CREATE)]),
            comparability: Comparability::Same,
            repair: "-".into(),
        },
        ClaimSpec {
            key: key("admission", "user_allowed_to_add", "requires:add_work_packages"),
            level: Level::Admission,
            ruby: ruby.map_or_else(none, |r| {
                r.fact(add_perm(r), SCAN, CREATE_CONTRACT, "allowed_in_project?(:add_work_packages", "the create is authorized by add_work_packages")
            }),
            rust: RustProbe::Data {
                krate: "op_contracts".into(),
                file: GENERATED.into(),
                item: "ADD_PERMISSION".into(),
            },
            comparability: Comparability::Same,
            repair: "data witness: the generator drift check (create_transition_differential)".into(),
        },
        ClaimSpec {
            key: key("guard", "user_allowed_to_add", "consults:allowed_in_project?"),
            level: Level::GuardEvaluation,
            ruby: ruby.map_or_else(none, |r| {
                r.fact(add_perm(r) && in_body("allowed_in_project?("), SCAN, CREATE_CONTRACT, "allowed_in_project?(:add_work_packages", "user_allowed_to_add asks allowed_in_project?")
            }),
            rust: edges(&[(USER_ALLOWED_TO_ADD, "op_contracts::base::UserContext::allowed_in_project")]),
            comparability: Comparability::Same,
            repair: "-".into(),
        },
        ClaimSpec {
            key: key("guard", "user_allowed_to_add", "consults:allowed_in_any_project?"),
            level: Level::GuardEvaluation,
            ruby: ruby.map_or_else(none, |r| {
                r.fact(in_body("allowed_in_any_project?("), LOCATED, CREATE_CONTRACT, "allowed_in_any_project?(:add_work_packages", "user_allowed_to_add asks allowed_in_any_project?")
            }),
            rust: edges(&[(USER_ALLOWED_TO_ADD, "op_contracts::base::UserContext::allowed_in_any_project")]),
            comparability: Comparability::Same,
            repair: "-".into(),
        },
        ClaimSpec {
            key: key("guard", "user_allowed_to_add", "consults:admin?"),
            level: Level::GuardEvaluation,
            ruby: match &add_body {
                Some((a, b, body)) if !body.contains("admin") => RubyEvidence::Absent {
                    extractor: LOCATED.into(),
                    span: SourceSpan { file: CREATE_CONTRACT.into(), line_start: *a, line_end: *b },
                    looked_for: "an admin check in the body (Ruby's admin bypass is inside User#allowed_in_project?, not harvested)".into(),
                },
                Some(_) => not_harvested("the body mentions admin"),
                None => none(),
            },
            rust: edges(&[(USER_ALLOWED_TO_ADD, "op_contracts::base::UserContext::is_admin")]),
            comparability: Comparability::Same,
            repair: "harvest User#allowed_in_project? to see whether the admin bypass matches".into(),
        },
        ClaimSpec {
            key: key("guard", "admit_create", "evaluates:writable?-per-submitted-attribute"),
            level: Level::GuardEvaluation,
            ruby: not_harvested("BaseContract#writable? (app/contracts/base_contract.rb) is not harvested by any extractor here"),
            rust: edges(&[(ADMIT_CREATE, IS_WRITABLE)]),
            comparability: Comparability::Same,
            repair: "ruff: lower BaseContract#writable? (condition AND permission)".into(),
        },
        ClaimSpec {
            key: key("guard", "CreateContract.attribute(:schedule_manually).block", "validates:validate_has_predecessors_or_children"),
            level: Level::GuardEvaluation,
            ruby: not_harvested("the scan reads `attribute :schedule_manually do` as a declaration and skips its block; ruff lowers `writable:` only"),
            rust: edges(&[(ADMIT_CREATE, "op_contracts::work_packages::admission::validate_has_predecessors_or_children")]),
            comparability: Comparability::Same,
            repair: "ruff: harvest validators inside `attribute ... do` blocks".into(),
        },
        ClaimSpec {
            key: key("admission", "mutation::create", "checks:add_work_packages-in-project"),
            level: Level::Admission,
            ruby: ruby.map_or_else(none, |r| {
                r.fact(add_perm(r), SCAN, CREATE_CONTRACT, "allowed_in_project?(:add_work_packages", "a create needs add_work_packages in the model's project")
            }),
            rust: edges(&[(MUTATION_CREATE, "op_services::work_packages::mutation::may")]),
            comparability: Comparability::Same,
            repair: "Charon: structure async bodies (mutation::create)".into(),
        },
        ClaimSpec {
            key: key("persistence", "mutation::create", "inserts:work_packages"),
            level: Level::Persistence,
            ruby: ruby.map_or_else(none, |r| {
                r.fact(true, LOCATED, CREATE_SERVICE, "work_package.save(validate: false)", "an admitted create saves the work package")
            }),
            rust: edges(&[(MUTATION_CREATE, "op_services::work_packages::mutation::CreationStore::insert_wp")]),
            comparability: Comparability::Same,
            repair: "Charon: structure async bodies (mutation::create, PgMutationStore::insert_wp)".into(),
        },
        ClaimSpec {
            key: key("admission", "POST /api/v3/work_packages", "enters:mutation::create"),
            level: Level::Admission,
            ruby: not_harvested("the API v3 create endpoint is not harvested"),
            rust: edges(&[(API_CREATE, MUTATION_CREATE)]),
            comparability: Comparability::Same,
            repair: "Charon: run on op-api (structural only: API mutations refuse until bearer auth exists)".into(),
        },
    ];
    if let Some(r) = ruby {
        for g in effective_guards(&r.guards) {
            let (effect, fact) = match &g.outcome {
                GuardOutcome::Lowered(e) => ("guard:lowered", format!("writable: {e:?}")),
                GuardOutcome::Unlowered { reason } => {
                    ("guard:unlowered", format!("writable: unlowered ({reason})"))
                }
            };
            v.push(ClaimSpec {
                key: key(
                    "guard",
                    &format!("{}.attribute(:{})", g.class, g.attribute),
                    effect,
                ),
                level: Level::GuardEvaluation,
                ruby: RubyEvidence::Fact {
                    extractor: RUFF.into(),
                    span: SourceSpan {
                        file: format!("app/contracts/{}", g.file),
                        line_start: g.line,
                        line_end: g.line,
                    },
                    fact,
                },
                rust: RustProbe::Data {
                    krate: "op_contracts".into(),
                    file: GENERATED.into(),
                    item: "ATTRIBUTES".into(),
                },
                comparability: Comparability::Same,
                repair: "data witness: the generator drift check (create_transition_differential)"
                    .into(),
            });
        }
    }
    v
}

fn revisions(store: &CharonStore) -> Revisions {
    let p = |k: &str| {
        store
            .provenance
            .get(k)
            .cloned()
            .unwrap_or_else(|| "unknown".into())
    };
    let trace = trace();
    Revisions {
        source_extractor: format!("ruff {RUFF_REV}"),
        charon: p("charon"),
        charon_extractor: p("ruff_charon"),
        target: format!("nexgen {}", p("nexgen")),
        source_app: format!(
            "openproject {}",
            trace["source"]["rev"].as_str().unwrap_or("unknown")
        ),
    }
}

fn reconcile(store: &CharonStore, ruby: Option<&Ruby>) -> Reconciliation {
    Reconciler::new(store, Some(&root())).reconcile(
        "WorkPackage create admission: three-witness reconciliation",
        revisions(store),
        &claims(ruby),
    )
}

// ─────────────────────────────────────────────────────────────────────────
// The runtime axis: production Rust against the frozen contract trace
// ─────────────────────────────────────────────────────────────────────────

fn trace() -> Value {
    let text = std::fs::read_to_string(root().join("oracle/work-package-contract-trace.json"))
        .expect("contract trace");
    serde_json::from_str(&text).expect("trace is JSON")
}

/// `PERMISSIONS = %i[ ... ]` of the oracle script.
fn oracle_permissions() -> Vec<String> {
    let rb = std::fs::read_to_string(root().join("oracle/work_package_contract.rb"))
        .expect("oracle script");
    let at = rb.find("PERMISSIONS = %i[").expect("PERMISSIONS") + "PERMISSIONS = %i[".len();
    rb[at..at + rb[at..].find(']').expect("]")]
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

struct OracleUser {
    admin: bool,
    held: Vec<String>,
}

impl OracleUser {
    fn of(label: &str, perms: &[String]) -> Self {
        match label {
            "admin" => OracleUser {
                admin: true,
                held: Vec::new(),
            },
            "member" => OracleUser {
                admin: false,
                held: perms.to_vec(),
            },
            "member_no_permissions" => OracleUser {
                admin: false,
                held: Vec::new(),
            },
            other => panic!("trace user {other} is not modeled"),
        }
    }
}

impl UserContext for OracleUser {
    fn id(&self) -> Id {
        1
    }
    fn is_admin(&self) -> bool {
        self.admin
    }
    fn is_anonymous(&self) -> bool {
        false
    }
    fn allowed_in_project(&self, permission: &str, project_id: Id) -> bool {
        project_id == CASE_PROJECT && self.allowed_globally(permission)
    }
    fn allowed_globally(&self, permission: &str) -> bool {
        self.held.iter().any(|p| p == permission)
    }
}

const FACT_ALIASES: &[(&str, &str)] = &[
    ("model.leaf?", "leaf?"),
    ("model.schedule_manually?", "schedule_manually"),
    ("model.status.closed?", "status.closed?"),
    ("model.status&.closed?", "status.closed?"),
    (
        "model.effective_target_versions.any?(&:closed?)",
        "effective_target_versions.any?(&:closed?)",
    ),
    ("model.status_id_change", "status_id_change_present"),
];

struct TraceFacts<'a>(&'a Value);

impl AdmissionFacts for TraceFacts<'_> {
    fn fact(&self, name: &str) -> Option<bool> {
        let k = FACT_ALIASES.iter().find(|(f, _)| *f == name)?.1;
        match self.0.get(k)? {
            Value::Bool(b) => Some(*b),
            Value::Null => Some(false),
            _ => None,
        }
    }
}

struct NoFacts;

impl AdmissionFacts for NoFacts {
    fn fact(&self, _: &str) -> Option<bool> {
        None
    }
}

fn create_cases(trace: &Value) -> Vec<&Value> {
    trace["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .filter(|c| c["contract"] == "WorkPackages::CreateContract")
        .collect()
}

/// `user_allowed_to_add` executed through the production service entry
/// (`CreateWorkPackageService::admit`), against the trace's `base` error per case.
fn runtime_authorization(trace: &Value) -> RuntimeEvidence {
    let perms = oracle_permissions();
    let (mut compared, mut mismatches) = (0, 0);
    for c in create_cases(trace) {
        let user = OracleUser::of(c["user"].as_str().expect("user"), &perms);
        let rust = CreateWorkPackageService::new(&user)
            .admit(Some(CASE_PROJECT), &[], &NoFacts)
            .errors
            .contains_key("base");
        let ruby = c["errors"]
            .as_object()
            .expect("errors")
            .contains_key("base");
        compared += 1;
        mismatches += usize::from(rust != ruby);
    }
    RuntimeEvidence::Executed {
        by: "op_services CreateWorkPackageService::admit vs trace errors[base], per CreateContract case".into(),
        compared,
        mismatches,
    }
}

/// `is_writable` (production) for one attribute against the trace's `writable` answer.
fn runtime_writable(trace: &Value, attribute: &str) -> RuntimeEvidence {
    let perms = oracle_permissions();
    let (mut compared, mut mismatches, mut unresolved) = (0, 0, 0);
    for c in create_cases(trace) {
        let Some(want) = c["writable"].get(attribute).and_then(Value::as_bool) else {
            continue;
        };
        let user = OracleUser::of(c["user"].as_str().expect("user"), &perms);
        match is_writable(
            &user,
            Some(CASE_PROJECT),
            attribute,
            &TraceFacts(&c["facts"]),
        ) {
            Writable::Unresolved(_) => unresolved += 1,
            w => {
                compared += 1;
                mismatches += usize::from((w == Writable::Yes) != want);
            }
        }
    }
    if compared == 0 {
        return RuntimeEvidence::NotExecuted {
            why: format!("no comparable trace answer ({unresolved} unresolved)"),
        };
    }
    RuntimeEvidence::Executed {
        by: format!("op_contracts is_writable vs trace writable[{attribute}] ({unresolved} unresolved, not compared)"),
        compared,
        mismatches,
    }
}

fn attach_runtime(r: &mut Reconciliation) {
    let trace = trace();
    assert!(r.attach_runtime(KEY_PERMISSION, runtime_authorization(&trace)));
    assert!(r.attach_runtime(
        KEY_ENTRY,
        RuntimeEvidence::Cited {
            gate: "tests/create_transition_differential.rs".into(),
            asserts: "CreateWorkPackageService::call verdicts, error codes and writable? against the frozen traces with zero mismatches".into(),
        }
    ));
    assert!(r.attach_runtime(
        KEY_BOUNDARY_PERMISSION,
        RuntimeEvidence::Cited {
            gate: "op-server board::executed_db (needs OP_TEST_DATABASE_URL)".into(),
            asserts: "mutation::create in a project without rights returns Forbidden and inserts no row (step 6)".into(),
        }
    ));
    assert!(r.attach_runtime(
        "openproject/WorkPackage/create/persistence/mutation::create/inserts:work_packages",
        RuntimeEvidence::Cited {
            gate: "op-server board::executed_db (needs OP_TEST_DATABASE_URL)".into(),
            asserts: "mutation::create over PgMutationStore commits the row (step 2); not transactional (step 7)".into(),
        }
    ));
    let guard_rows: Vec<(String, String)> = r
        .rows
        .iter()
        .filter(|row| {
            row.spec.key.phase == "guard"
                && row.spec.key.method.contains(".attribute(:")
                && !row.spec.key.method.ends_with(".block")
        })
        .map(|row| {
            let m = &row.spec.key.method;
            let attr = m[m.find("(:").expect("(:") + 2..m.len() - 1].to_string();
            (row.spec.key.to_string(), attr)
        })
        .collect();
    for (k, attr) in guard_rows {
        assert!(r.attach_runtime(&k, runtime_writable(&trace, &attr)));
    }
}

fn verdicts(r: &Reconciliation) -> Vec<(String, Verdict)> {
    r.rows
        .iter()
        .map(|row| (row.spec.key.to_string(), row.verdict.clone()))
        .collect()
}

fn probe_crates(spec: &ClaimSpec) -> Vec<String> {
    match &spec.rust {
        RustProbe::Edges { edges, conflicts } => edges
            .iter()
            .chain(conflicts)
            .map(|(c, _)| crate_of(c).to_string())
            .collect(),
        RustProbe::Data { krate, .. } => vec![krate.clone()],
    }
}

/// A copy of the store without the artifacts of `drop` (the negative control's input).
fn store_without(drop: &[&str]) -> CharonStore {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("charon-without-{}", drop.join("-")));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    for entry in std::fs::read_dir(store_dir()).expect("store dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if drop.iter().any(|d| name.starts_with(&format!("{d}."))) {
            continue;
        }
        std::fs::copy(entry.path(), dir.join(&name)).expect("copy");
    }
    CharonStore::load(&dir).expect("store without")
}

// ─────────────────────────────────────────────────────────────────────────
// The test
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn create_admission_three_witness_reconciliation() {
    let store = CharonStore::load(&store_dir()).expect("Charon fixture loads");
    assert_eq!(
        store.crates.keys().map(String::as_str).collect::<Vec<_>>(),
        ["op_contracts", "op_services"]
    );
    let src = source();
    let ruby = src.as_deref().map(Ruby::load);
    let mut rec = reconcile(&store, ruby.as_ref());

    // The fixture's byte offsets refer to the extracted revision. A changed file makes
    // every row on it RustEvidenceStale; re-extract instead of reading stale offsets.
    assert!(
        rec.stale_files.is_empty(),
        "stale Charon fixture files: {:?}",
        rec.stale_files
    );

    // (b) The project/permission check inside `mutation::create` is in an async body
    // Charon did not structure: slag, never a disagreement or a Ruby-only fact.
    assert_eq!(
        rec.row(KEY_BOUNDARY_PERMISSION).expect("row").verdict,
        Verdict::Slag(SlagReason::RustBodyNotStructured)
    );

    // (a) service call -> admit -> admit_create agrees with the Ruby admission.
    if ruby.is_some() {
        assert_eq!(rec.row(KEY_ENTRY).expect("row").verdict, Verdict::Agreement);
    } else {
        eprintln!("UNVERIFIED: (a) and the report need OPENPROJECT_SRC");
    }

    // Silence twin: on the real store, RustSideNotExtracted only where the crate is
    // genuinely absent from the store.
    for row in &rec.rows {
        if row.verdict == Verdict::Slag(SlagReason::RustSideNotExtracted) {
            for k in probe_crates(&row.spec) {
                assert!(
                    !store.crates.contains_key(&k),
                    "{}: {k} is in the store",
                    row.spec.key
                );
            }
        }
    }
    assert_eq!(
        rec.rows
            .iter()
            .filter(|r| r.verdict == Verdict::Slag(SlagReason::RustSideNotExtracted))
            .count(),
        1,
        "only the op-api row lacks an extracted crate"
    );
    assert_eq!(rec.counts().get("Disagreement"), None);

    // (c) The runtime axis is attached after the verdicts and changes none of them.
    let before = verdicts(&rec);
    attach_runtime(&mut rec);
    assert_eq!(
        verdicts(&rec),
        before,
        "runtime evidence changed a structural verdict"
    );
    let mut executed_slag = 0;
    for row in &rec.rows {
        if let RuntimeEvidence::Executed {
            mismatches,
            compared,
            ..
        } = &row.runtime
        {
            assert_eq!(*mismatches, 0, "{}: runtime mismatch", row.spec.key);
            assert!(*compared > 0);
            if matches!(row.verdict, Verdict::Slag(_)) {
                executed_slag += 1;
            }
        }
    }
    // Executed agreement sits next to a structural slag and leaves it slag.
    assert!(matches!(
        rec.row(KEY_PERMISSION).expect("row").runtime,
        RuntimeEvidence::Executed { mismatches: 0, .. }
    ));
    assert_eq!(
        rec.row(KEY_PERMISSION).expect("row").verdict,
        Verdict::Slag(SlagReason::RepresentedAsData)
    );
    assert!(executed_slag >= 1);

    // (d) Negative control: the same claims against a store without a crate's
    // artifacts. Every row that probes the removed crate becomes RustSideNotExtracted;
    // no row becomes a disagreement; rows on other crates are unchanged.
    for drop in [
        &["op_services"][..],
        &["op_contracts"][..],
        &["op_services", "op_contracts"][..],
    ] {
        let without = store_without(drop);
        let neg = reconcile(&without, ruby.as_ref());
        let mut affected = 0;
        for (row, real) in neg.rows.iter().zip(&rec.rows) {
            let crates = probe_crates(&row.spec);
            if crates.iter().any(|k| drop.contains(&k.as_str())) {
                affected += 1;
                assert_eq!(
                    row.verdict,
                    Verdict::Slag(SlagReason::RustSideNotExtracted),
                    "without {drop:?}: {}",
                    row.spec.key
                );
            } else {
                assert_eq!(
                    row.verdict, real.verdict,
                    "without {drop:?}: {}",
                    row.spec.key
                );
            }
        }
        assert!(
            affected > 0,
            "negative control without {drop:?} touched no row"
        );
        assert_eq!(neg.counts().get("Disagreement"), None, "without {drop:?}");
        eprintln!("negative control without {drop:?}: {affected} rows -> Slag(RustSideNotExtracted), 0 Disagreement");
    }

    eprintln!("counts: {:?}", rec.counts());
    for row in &rec.rows {
        eprintln!(
            "  {} -> {} | runtime: {}",
            row.spec.key, row.verdict, row.runtime
        );
    }

    // The drilldown report is generated from the reconciliation; the checked-in copy
    // must be current.
    if ruby.is_some() {
        let md = render_markdown(&rec);
        let path = root().join(REPORT);
        if std::env::var("UPDATE_RECONCILE_REPORT").as_deref() == Ok("1") {
            std::fs::write(&path, &md).expect("write report");
        }
        let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            checked_in == md,
            "{REPORT} is stale: rerun with UPDATE_RECONCILE_REPORT=1"
        );
    }
}
