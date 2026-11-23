//! Three-witness reconciliation of OpenProject `WorkPackage` create admission:
//!
//! 1. the Ruby structural witness: what `WorkPackages::CreateContract` declares, as the
//!    existing extractors produce it (`admission_gen`'s contract declaration scan,
//!    ruff's `extract_contract_guards`), with spans located in the checkout;
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
//!
//! The claims are data: `oracle/charon/ore-slag/openproject.toml`, loaded and checked by
//! `op_codegen_pipeline::ore_slag_config`. What stays here is the app-side binding: the
//! named evidence evaluators ([`Source`]), the `schedule_manually` witness, and the
//! `effective_guards` generator over the harvest.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use op_codegen_pipeline::admission_gen::{contract_scan_facts, ContractScanFacts};
use op_codegen_pipeline::ore_slag_config::{EvidenceSource, GeneratedClaim, OreSlagConfig};
use op_codegen_pipeline::reconcile::{
    crate_of, locate, render_markdown, CharonStore, ClaimSpec, Reconciler, Reconciliation,
    Revisions, RubyEvidence, RuntimeEvidence, RustProbe, SlagReason, SourceSpan, Verdict,
};
use op_codegen_pipeline::RUFF_REV;
use op_contracts::base::UserContext;
use op_contracts::work_packages::{is_writable, AdmissionFacts, Writable};
use op_core::traits::Id;
use op_services::work_packages::CreateWorkPackageService;
use ruff_ruby_spo::{
    extract_attribute_blocks, extract_contract_guards, AttributeBlock, ContractGuard, GuardExpr,
    GuardOutcome,
};
use serde_json::Value;

const CREATE_CONTRACT: &str = "app/contracts/work_packages/create_contract.rb";
const CREATE_SERVICE: &str = "app/services/work_packages/create_service.rb";
const GENERATED: &str = "crates/op-contracts/src/work_packages/generated_create_admission.rs";
const REPORT: &str = "oracle/charon/reconcile-create-admission.md";
/// The reconciliation's data: claims, residual proposals, gates (see the module doc).
const ORE_SLAG_CONFIG: &str = "oracle/charon/ore-slag/openproject.toml";

// Join keys of the rows the assertions name.
const KEY_ENTRY: &str = "openproject/WorkPackage/create/admission/CreateWorkPackageService.call/evaluates:WorkPackages::CreateContract";
const KEY_BOUNDARY_PERMISSION: &str =
    "openproject/WorkPackage/create/admission/mutation::create/checks:add_work_packages-in-project";
const KEY_API: &str =
    "openproject/WorkPackage/create/admission/POST /api/v3/work_packages/enters:mutation::create";
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

// ─────────────────────────────────────────────────────────────────────────
// The Ruby witness
// ─────────────────────────────────────────────────────────────────────────

struct Ruby {
    contract: String,
    service: String,
    scan: ContractScanFacts,
    guards: Vec<ContractGuard>,
    blocks: Vec<AttributeBlock>,
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
            blocks: extract_attribute_blocks(src, &[CREATE_CONTRACT]).0,
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

/// The config: loaded and checked once. A config that does not load fails every test.
fn config() -> &'static OreSlagConfig {
    static CONFIG: OnceLock<OreSlagConfig> = OnceLock::new();
    CONFIG.get_or_init(|| {
        OreSlagConfig::load(&root().join(ORE_SLAG_CONFIG)).unwrap_or_else(|e| panic!("{e}"))
    })
}

/// An extractor label of the config's `[extractors]` table.
fn extractor(name: &str) -> String {
    config().extractor(name).unwrap_or_else(|e| panic!("{e}"))
}

const SCHEDULE_CLASS: &str = "WorkPackages::CreateContract";
const SCHEDULE_ATTRIBUTE: &str = "schedule_manually";
const SCHEDULE_VALIDATOR: &str = "validate_has_predecessors_or_children";
const SCHEDULE_CONDITION: &str = "model.schedule_automatically?";

/// The Ruby side of the `schedule_manually` block claim: the call `validator` inside
/// `attribute :schedule_manually do ... end` of `CreateContract`, under the lowered
/// condition `model.schedule_automatically?`. Anything else (block missing, other call
/// name, no condition, a different or unlowered condition) is not the claimed fact.
fn schedule_block_evidence(blocks: &[AttributeBlock], validator: &str) -> RubyEvidence {
    let Some(block) = blocks
        .iter()
        .find(|b| b.class == SCHEDULE_CLASS && b.attribute == SCHEDULE_ATTRIBUTE)
    else {
        return not_harvested(
            "no `attribute :schedule_manually do` block harvested in CreateContract",
        );
    };
    let Some(call) = block.calls.iter().find(|c| c.name == validator) else {
        return not_harvested(&format!(
            "the harvested schedule_manually block does not call {validator}"
        ));
    };
    let Some(cond) = &call.condition else {
        return not_harvested(&format!(
            "the harvested call to {validator} is unconditional, not under {SCHEDULE_CONDITION}"
        ));
    };
    if cond.outcome != GuardOutcome::Lowered(GuardExpr::Fact(SCHEDULE_CONDITION.into())) {
        return not_harvested(&format!(
            "the harvested condition {:?} is not the lowered fact {SCHEDULE_CONDITION}",
            cond.source_text
        ));
    }
    RubyEvidence::Fact {
        extractor: extractor("ruff_blocks"),
        span: SourceSpan {
            file: block.file.clone(),
            line_start: call.line as usize,
            line_end: call.line as usize,
        },
        fact: format!(
            "inside `attribute :schedule_manually do` (line {}), CreateContract calls {} if {} (lowered Fact({:?}))",
            block.line, call.name, cond.source_text, SCHEDULE_CONDITION
        ),
    }
}

/// The ruff guards that decide create admission: `CreateContract` overrides
/// `BaseContract`, per attribute (the generator's `guard_for`). An attribute the
/// subclass declares without `writable:` (`shadowed`, from the generator's scan) has no
/// effective guard: its `nil` condition wins in `collect_ancestor_attributes`
/// (base_contract.rb:182-190) and `reduce_by_writable_conditions` only removes on
/// `== false` (base_contract.rb:238-245).
fn effective_guards<'a>(
    guards: &'a [ContractGuard],
    shadowed: &BTreeSet<String>,
) -> Vec<&'a ContractGuard> {
    let canon = |a: &str| a.strip_suffix("_id").unwrap_or(a).to_string();
    let mut by: BTreeMap<String, &ContractGuard> = BTreeMap::new();
    for class in ["WorkPackages::BaseContract", "WorkPackages::CreateContract"] {
        for g in guards.iter().filter(|g| g.class == class) {
            by.insert(canon(&g.attribute), g);
        }
    }
    by.retain(|name, g| g.class == "WorkPackages::CreateContract" || !shadowed.contains(name));
    by.into_values().collect()
}

// ─────────────────────────────────────────────────────────────────────────
// The claims: data in the config, evaluated through this binding
// ─────────────────────────────────────────────────────────────────────────

/// The named evidence evaluators of the config's `ruby` specs, over the Ruby witness
/// (`None`: no `OPENPROJECT_SRC`).
struct Source<'a>(Option<&'a Ruby>);

impl Source<'_> {
    fn ruby(&self) -> Result<&Ruby, String> {
        self.0.ok_or_else(|| "OPENPROJECT_SRC not set".to_string())
    }
}

impl EvidenceSource for Source<'_> {
    fn unavailable(&self) -> Option<String> {
        self.0
            .is_none()
            .then(|| "OPENPROJECT_SRC not set".to_string())
    }

    /// `has_validator`, `add_perm`, `add_perm_and_in_body:<needle>`, `in_body:<needle>`,
    /// `always`.
    fn predicate(&self, name: &str, argument: Option<&str>) -> Result<bool, String> {
        let r = self.ruby()?;
        let in_body = |needle: &str| {
            r.add_body()
                .is_some_and(|(_, _, body)| body.contains(needle))
        };
        let add_perm = r.scan.add_permission.as_deref() == Some("add_work_packages");
        let arg = || argument.ok_or_else(|| format!("predicate {name} needs an argument"));
        match name {
            "has_validator" => Ok(r.scan.validators.iter().any(|v| v == "user_allowed_to_add")),
            "add_perm" => Ok(add_perm),
            "add_perm_and_in_body" => Ok(add_perm && in_body(arg()?)),
            "in_body" => Ok(in_body(arg()?)),
            "always" => Ok(true),
            other => Err(format!("unknown predicate {other}")),
        }
    }

    fn text(&self, file: &str) -> Option<&str> {
        let r = self.0?;
        Some(if file == CREATE_SERVICE {
            &r.service
        } else {
            &r.contract
        })
    }

    fn body(&self, name: &str) -> Result<Option<(usize, usize, String)>, String> {
        match name {
            "add_body" => Ok(self.ruby()?.add_body()),
            other => Err(format!("unknown body {other}")),
        }
    }

    fn witness(&self, name: &str) -> Result<RubyEvidence, String> {
        match name {
            "schedule_block" => Ok(schedule_block_evidence(
                &self.ruby()?.blocks,
                SCHEDULE_VALIDATOR,
            )),
            other => Err(format!("unknown witness {other}")),
        }
    }
}

/// The instances of a generated claim family. `effective_guards`: one per effective
/// ruff guard, so none without the Ruby checkout.
fn generate(ruby: Option<&Ruby>, generator: &str) -> Option<Vec<GeneratedClaim>> {
    match generator {
        "effective_guards" => Some(ruby.map_or_else(Vec::new, |r| {
            effective_guards(&r.guards, &r.scan.writable_shadowed)
                .into_iter()
                .map(|g| {
                    let (effect, fact) = match &g.outcome {
                        GuardOutcome::Lowered(e) => ("guard:lowered", format!("writable: {e:?}")),
                        GuardOutcome::Unlowered { reason } => {
                            ("guard:unlowered", format!("writable: unlowered ({reason})"))
                        }
                    };
                    GeneratedClaim {
                        method: format!("{}.attribute(:{})", g.class, g.attribute),
                        effect: effect.into(),
                        span: SourceSpan {
                            file: format!("app/contracts/{}", g.file),
                            line_start: g.line,
                            line_end: g.line,
                        },
                        fact,
                    }
                })
                .collect()
        })),
        _ => None,
    }
}

fn claims(ruby: Option<&Ruby>) -> Vec<ClaimSpec> {
    config()
        .claim_specs(&Source(ruby), &|g| generate(ruby, g))
        .unwrap_or_else(|e| panic!("{e}"))
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
    ("model.children.any?", "children.any?"),
];

/// `Relation.used_for_scheduling_of(model.parent).any?`: the oracle builds every
/// CreateContract record with `WorkPackage.new` and no parent
/// (`oracle/work_package_contract.rb` `new_from`), and the scope is `[]` for no parent
/// (`app/models/relations/scopes/used_for_scheduling_of.rb:45`).
const NO_PARENT_PREDECESSORS_FACT: &str = "Relation.used_for_scheduling_of(model.parent).any?";

struct TraceFacts<'a>(&'a Value);

impl AdmissionFacts for TraceFacts<'_> {
    fn fact(&self, name: &str) -> Option<bool> {
        if name == NO_PARENT_PREDECESSORS_FACT {
            return Some(false);
        }
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
            asserts: "mutation::create over PgMutationStore commits the row (step 2); a failure after the INSERT rolls it back (step 7)".into(),
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
        // The store is its files; `ore-slag/` holds the reconciliation config.
        if !entry.file_type().expect("file type").is_file() {
            continue;
        }
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
        ["op_api", "op_contracts", "op_services"]
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
        0,
        "every probed crate is in the store"
    );
    // (b') The API handler is an async body Charon did not structure either: the edge
    // `create_work_package_core -> mutation::create` cannot be inspected.
    assert_eq!(
        rec.row(KEY_API).expect("row").verdict,
        Verdict::Slag(SlagReason::RustBodyNotStructured)
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
        &["op_api"][..],
        &["op_services", "op_contracts", "op_api"][..],
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

// ORE/SLAG resolution over this reconciliation (same claims, same functions).
#[path = "ore_slag/resolution.rs"]
mod ore_slag;
