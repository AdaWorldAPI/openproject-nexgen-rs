//! One transition, executed: `WorkPackages::CreateService` for `create_member`,
//! compared with what real OpenProject did (`oracle/work-package-service-trace.json`,
//! `oracle/work-package-contract-trace.json`).
//!
//! Every compared item carries one of three categories, and only the first counts
//! towards execution parity:
//!
//! - EXECUTED: production Rust (not test code) ran the rule and produced the result.
//!   Here: `op_services::CreateWorkPackageService::call`, which runs
//!   `op_contracts::work_packages::admit_create` over a table generated from the
//!   Ruby contracts by ruff's lowering. Compared: the verdict, the error keys and
//!   codes, and `writable?` per attribute.
//! - PREDICTED: a model derived from the Ruby source (not from the trace) says what
//!   to expect, but no Rust implementation ran it: the callback registrations
//!   (`callback_harvest`) and the effect expectations derived from admission.
//! - SLAG: not modeled; named, never dropped.
//!
//! Inputs are the oracle's own: user, permissions and submitted attribute names are
//! parsed from `work_package_service.rb`; guard facts come from the contract trace.
//! The traces record attribute NAMES for the submitted work package, not values, so
//! no value is compared and none is invented: the service takes names only.
//!
//! The test also fails when the checked-in admission table differs from what the
//! generator produces from the checkout.
//!
//! Needs `OPENPROJECT_SRC` (a checkout at the trace revision). Without it the
//! test prints UNVERIFIED and passes, unless `REQUIRE_SOURCES=1`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use op_codegen_pipeline::admission_gen::{self, ExtractionGap};
use op_codegen_pipeline::callback_harvest as cb;
use op_contracts::base::UserContext;
use op_contracts::work_packages::{
    changed_column, guard_facts, is_writable, not_evaluated_validators, AdmissionFacts,
    CreateAdmission, Unresolved, Verdict, Writable,
};
use op_core::traits::Id;
use op_services::work_packages::{CreateWorkPackageService, WorkPackageParams};
use serde_json::Value;

const CASE: &str = "create_member";
const DENIED_CASE: &str = "create_no_permission";
const CREATE: &str = "WorkPackages::CreateContract";

/// The oracle records the permission set a user holds in the one project its case
/// creates, not a project id. This handle stands for that project: the test user
/// answers for it and for nothing else (asserted), so it is a name, not a value.
const CASE_PROJECT: Id = 7;

/// An id that says "this attribute is submitted" for attributes whose values the
/// oracle does not record. Admission never reads it and no comparison uses it.
const OPAQUE_MARKER: Id = 1;

/// The checked-in table, for the drift check.
const CHECKED_IN_TABLE: &str =
    include_str!("../../op-contracts/src/work_packages/generated_create_admission.rs");

/// Lowered fact name -> fact key in the contract trace.
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

/// Measured at OpenProject 2725194 on the committed traces.
const FLOOR_CONTRACT_COMPARED: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Category {
    Executed,
    Predicted,
    Slag,
}

#[derive(Default, Debug, Clone, Copy)]
struct Score {
    executed: usize,
    predicted: usize,
    slag: usize,
}

impl Score {
    fn add(&mut self, c: Category, n: usize) {
        match c {
            Category::Executed => self.executed += n,
            Category::Predicted => self.predicted += n,
            Category::Slag => self.slag += n,
        }
    }
}

fn source() -> Option<PathBuf> {
    match std::env::var("OPENPROJECT_SRC") {
        Ok(s) if !s.is_empty() => Some(PathBuf::from(s)),
        _ => {
            assert!(
                std::env::var("REQUIRE_SOURCES").as_deref() != Ok("1"),
                "REQUIRE_SOURCES=1 but OPENPROJECT_SRC is not set"
            );
            eprintln!("UNVERIFIED: create_transition_differential needs OPENPROJECT_SRC");
            None
        }
    }
}

fn oracle_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle")
        .join(name)
}

fn read(name: &str) -> String {
    std::fs::read_to_string(oracle_path(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn json(name: &str) -> Value {
    serde_json::from_str(&read(name)).expect("trace is JSON")
}

fn service_case(trace: &Value, name: &str) -> Value {
    trace["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("service trace has no case {name}"))
        .clone()
}

// ─────────────────────────────────────────────────────────────────────────
// What the oracle setup says (read from the Ruby scripts, not retyped)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum UserKind {
    /// A project member holding exactly these permissions.
    Member(Vec<String>),
    /// `create(:admin)`: what an admin may do is not recorded by either trace.
    Admin,
}

impl UserKind {
    fn label(&self) -> &'static str {
        match self {
            UserKind::Member(p) if p.is_empty() => "member_no_permissions",
            UserKind::Member(_) => "member",
            UserKind::Admin => "admin",
        }
    }
}

/// `NAME = %i[ a b c ].freeze` -> `["a", "b", "c"]`.
fn percent_i(text: &str, name: &str) -> Vec<String> {
    let head = format!("{name} = %i[");
    let at = text
        .find(&head)
        .unwrap_or_else(|| panic!("no {name} in the oracle script"));
    let rest = &text[at + head.len()..];
    let end = rest.find(']').expect("unterminated %i[");
    rest[..end].split_whitespace().map(str::to_string).collect()
}

/// The text of one case lambda in a `CASES = { "name" => lambda { ... } }` table.
fn case_block<'a>(rb: &'a str, name: &str) -> &'a str {
    let header = format!("\"{name}\" => lambda");
    let start = rb.find(&header).unwrap_or_else(|| panic!("no case {name}"));
    let body = start + header.len();
    let end = rb[body..]
        .find("\" => lambda")
        .or_else(|| rb[body..].find(".freeze"))
        .expect("end of CASES");
    &rb[start..body + end]
}

/// Who the case runs as, read from the lambda body.
fn user_of_block(block: &str, default_perms: &[String]) -> UserKind {
    if block.contains("create(:admin)") {
        UserKind::Admin
    } else if block.contains("member_user(project, [])")
        || block.contains("member_user(wp.project, [])")
    {
        UserKind::Member(Vec::new())
    } else if block.contains("member_user(project)") || block.contains("member_user(wp.project)") {
        UserKind::Member(default_perms.to_vec())
    } else {
        panic!("cannot read the user out of case block: {block}")
    }
}

/// Split `s` at commas that are not inside brackets, parentheses or braces.
fn top_level_commas(s: &str) -> Vec<&str> {
    let (mut depth, mut from, mut out) = (0i32, 0usize, Vec::new());
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[from..]);
    out
}

/// Keys of a `key: value, key:` list.
fn keys_of(list: &str) -> Vec<String> {
    top_level_commas(list)
        .iter()
        .filter_map(|p| p.split_once(':').map(|(k, _)| k.trim().to_string()))
        .filter(|k| !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect()
}

/// The attributes the service case passes to `CreateService#call`: the hash in
/// `params_for` plus the extras the case lambda adds.
fn submitted_of(rb: &str, block: &str) -> Vec<String> {
    let def = rb.find("def params_for").expect("params_for");
    // Skip the signature line (`extra = {}` holds a brace of its own).
    let body = &rb[def + rb[def..].find('\n').expect("params_for body")..];
    let open = body.find('{').expect("hash literal") + 1;
    let close = body[open..].find("}.merge").expect("}.merge") + open;
    let mut keys = keys_of(&body[open..close]);
    if let Some(at) = block.find("params_for(project,") {
        let args = &block[at + "params_for(project,".len()..];
        let args = &args[..args.find(')').expect("params_for(")];
        keys.extend(keys_of(args));
    }
    keys
}

// ─────────────────────────────────────────────────────────────────────────
// Authorization: EXECUTED by production code
// ─────────────────────────────────────────────────────────────────────────

/// The user of an oracle case, as production code sees a user.
struct OracleUser {
    kind: UserKind,
}

impl UserContext for OracleUser {
    fn id(&self) -> Id {
        // The user's own id: read only as the author the service sets; the oracle
        // does not record it and nothing compares it.
        1
    }
    fn is_admin(&self) -> bool {
        self.kind == UserKind::Admin
    }
    fn is_anonymous(&self) -> bool {
        false
    }
    fn allowed_in_project(&self, permission: &str, project_id: Id) -> bool {
        self.allowed_globally(permission) && project_id == CASE_PROJECT
    }
    fn allowed_globally(&self, permission: &str) -> bool {
        matches!(&self.kind, UserKind::Member(held) if held.iter().any(|p| p == permission))
    }
}

/// Facts of one trace record, as production asks for them (lowered fact names).
#[derive(Clone)]
struct TraceFacts(BTreeMap<String, Value>);

impl AdmissionFacts for TraceFacts {
    fn fact(&self, name: &str) -> Option<bool> {
        let key = FACT_ALIASES.iter().find(|(f, _)| *f == name)?.1;
        match self.0.get(key)? {
            Value::Bool(b) => Some(*b),
            Value::Null => Some(false),
            _ => None,
        }
    }
}

fn facts_of(rec: &Value) -> TraceFacts {
    TraceFacts(
        rec["facts"]
            .as_object()
            .expect("facts")
            .clone()
            .into_iter()
            .collect(),
    )
}

/// The subject literal of `params_for` in the setup script.
fn subject_literal(rb: &str) -> String {
    let at = rb.find("subject: \"").expect("subject literal") + "subject: \"".len();
    rb[at..at + rb[at..].find('"').expect("closing quote")].to_string()
}

fn codes(errors: &Value) -> BTreeMap<String, Vec<String>> {
    errors
        .as_object()
        .expect("errors object")
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_array()
                    .expect("error list")
                    .iter()
                    .map(|d| d["error"].as_str().expect("error code").to_string())
                    .collect(),
            )
        })
        .collect()
}

/// The service trace records no facts. These are the facts of the closest recorded
/// analogue: the contract trace's `CreateContract` record for the same user (a new,
/// unsaved work package).
fn analogue<'a>(contract: &'a Value, label: &str) -> &'a Value {
    contract["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|r| r["contract"] == CREATE && r["user"] == label)
        .unwrap_or_else(|| panic!("no CreateContract record for user {label}"))
}

/// One authorization comparison, every item EXECUTED.
struct Auth {
    case: String,
    user: UserKind,
    borrowed_from: String,
    submitted: Vec<String>,
    admission: CreateAdmission,
    /// The REAL `CreateWorkPackageService::call` result: success and error keys/codes.
    call_success: bool,
    call_errors: BTreeMap<String, Vec<String>>,
    oracle_success: bool,
    oracle_errors: BTreeMap<String, Vec<String>>,
    /// `None`: the production verdict was unresolved.
    verdict_match: Option<bool>,
    errors_match: Option<bool>,
    writable_compared: usize,
    writable_matched: usize,
    mismatches: Vec<String>,
}

fn run_auth(
    svc: &Value,
    contract: &Value,
    service_rb: &str,
    default_perms: &[String],
    case: &str,
    facts_override: Option<&TraceFacts>,
) -> Auth {
    let block = case_block(service_rb, case);
    let user = user_of_block(block, default_perms);
    let sc = service_case(svc, case);
    assert_eq!(
        sc["user"],
        user.label(),
        "the user read out of the setup script disagrees with the trace label"
    );
    let rec = analogue(contract, user.label());
    let facts = facts_override.cloned().unwrap_or_else(|| facts_of(rec));
    let submitted = submitted_of(service_rb, block);
    let names: Vec<&str> = submitted.iter().map(String::as_str).collect();
    let project = submitted
        .iter()
        .any(|s| s == "project")
        .then_some(CASE_PROJECT);

    let oracle_user = OracleUser { kind: user.clone() };
    // The production entry point under test.
    let admission = CreateWorkPackageService::new(&oracle_user).admit(project, &names, &facts);

    // The production service path: `call` runs admission and acts on it. The params
    // carry the submitted attributes. Admission reads which are present, never their
    // values; the subject is the script's own literal, and the association attributes
    // carry the case-project handle or an opaque marker id that admission does not
    // read and no comparison uses.
    let mut params = WorkPackageParams::new();
    for k in &submitted {
        match k.as_str() {
            "subject" => params.subject = Some(subject_literal(service_rb)),
            "project" => params.project_id = Some(CASE_PROJECT),
            "type" => params.type_id = Some(OPAQUE_MARKER),
            "status" => params.status_id = Some(OPAQUE_MARKER),
            "priority" => params.priority_id = Some(OPAQUE_MARKER),
            "parent" => params.parent_id = Some(OPAQUE_MARKER),
            other => panic!("no service parameter for submitted attribute {other}"),
        }
    }
    let called = CreateWorkPackageService::new(&oracle_user)
        .with_facts(&facts)
        .call(params);
    let call_success = called.is_success();
    let mut call_errors: BTreeMap<String, Vec<String>> =
        called.errors().errors.clone().into_iter().collect();
    if !called.errors().base_errors.is_empty() {
        call_errors.insert("base".into(), called.errors().base_errors.clone());
    }

    let oracle_errors = codes(&sc["errors"]);
    let oracle_success = sc["success"].as_bool().expect("success");
    assert!(
        !matches!(admission.verdict, Verdict::Unresolved(_)),
        "admission was unresolved for a recorded case"
    );
    // Admission behaviour: what the real `call` returned, against the oracle.
    let verdict_match = Some(call_success == oracle_success);
    let errors_match = Some(call_errors == oracle_errors);

    let (mut compared, mut matched, mut mismatches) = (0, 0, Vec::new());
    for (col, w) in &admission.writable {
        let oracle_writable = !oracle_errors
            .get(col)
            .is_some_and(|c| c.iter().any(|e| e == "error_readonly"));
        if !matches!(w, Writable::Unresolved(_)) {
            compared += 1;
            if (*w == Writable::Yes) == oracle_writable {
                matched += 1;
            } else {
                mismatches.push(format!(
                    "{case} {col}: rust {w:?}, oracle writable={oracle_writable}"
                ));
            }
        }
    }
    Auth {
        case: case.into(),
        borrowed_from: format!(
            "contract trace case `{}` ({CREATE}, user {})",
            rec["name"].as_str().unwrap_or("?"),
            rec["user"]
        ),
        user,
        submitted,
        admission,
        call_success,
        call_errors,
        oracle_success,
        oracle_errors,
        verdict_match,
        errors_match,
        writable_compared: compared,
        writable_matched: matched,
        mismatches,
    }
}

/// Every attribute of every `CreateContract` record in the contract trace, through
/// the production `is_writable`, against the recorded `writable?`.
#[derive(Default)]
struct ContractWide {
    compared: usize,
    matched: usize,
    mismatches: Vec<String>,
    unresolved: BTreeMap<Unresolved, usize>,
}

fn contract_wide(contract: &Value, contract_rb: &str) -> ContractWide {
    let default_perms = percent_i(contract_rb, "PERMISSIONS");
    let mut out = ContractWide::default();
    for rec in contract["cases"].as_array().expect("cases") {
        if rec["contract"] != CREATE {
            continue;
        }
        let name = rec["name"].as_str().expect("name");
        let user = OracleUser {
            kind: user_of_block(case_block(contract_rb, name), &default_perms),
        };
        assert_eq!(
            rec["user"],
            user.kind.label(),
            "contract case {name}: user label"
        );
        let facts = facts_of(rec);
        for (attr, want) in rec["writable"].as_object().expect("writable") {
            match is_writable(&user, Some(CASE_PROJECT), attr, &facts) {
                Writable::Unresolved(u) => *out.unresolved.entry(u).or_default() += 1,
                w => {
                    out.compared += 1;
                    if (w == Writable::Yes) == want.as_bool().expect("bool") {
                        out.matched += 1;
                    } else {
                        out.mismatches.push(format!(
                            "{name} {} {attr}: rust {w:?}, oracle {want}",
                            rec["user"]
                        ));
                    }
                }
            }
        }
    }
    out
}

/// Negative control (b): the smallest change of recorded facts that flips a
/// submitted attribute's guard, found by asking production `is_writable`.
/// Returns `(column, changed fact keys, perturbed facts)`.
fn perturb(
    user: &OracleUser,
    cols: &[String],
    facts: &TraceFacts,
) -> Option<(String, Vec<String>, TraceFacts)> {
    for col in cols {
        let base = is_writable(user, Some(CASE_PROJECT), col, facts);
        if matches!(base, Writable::Unresolved(_)) {
            continue;
        }
        let keys: Vec<&str> = guard_facts(col)
            .iter()
            .filter_map(|n| FACT_ALIASES.iter().find(|(f, _)| f == n).map(|(_, k)| *k))
            .collect();
        let mut best: Option<(Vec<String>, TraceFacts)> = None;
        for mask in 0u32..(1 << keys.len()) {
            let mut f = facts.clone();
            let mut changed = Vec::new();
            for (i, k) in keys.iter().enumerate() {
                let was = matches!(facts.0.get(*k), Some(Value::Bool(true)));
                let now = mask & (1 << i) != 0;
                f.0.insert((*k).to_string(), Value::Bool(now));
                if now != was {
                    changed.push((*k).to_string());
                }
            }
            let flipped = is_writable(user, Some(CASE_PROJECT), col, &f);
            if flipped != base
                && !matches!(flipped, Writable::Unresolved(_))
                && best.as_ref().is_none_or(|(c, _)| changed.len() < c.len())
            {
                best = Some((changed, f));
            }
        }
        if let Some((changed, f)) = best {
            return Some((col.clone(), changed, f));
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────
// Callbacks: registrations RECONSTRUCTED (PREDICTED), bodies never run here
// ─────────────────────────────────────────────────────────────────────────

/// Why a registered declaration does not belong to a create at all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NotOnCreate {
    /// `update`, `destroy`, `touch`, ... chains do not run on a create.
    ChainNotOnCreatePath(String),
    /// `on:` (or the macro name) excludes create.
    OnOptionExcludesCreate(String),
}

/// A callback is registered *for the create transition* when its chain runs on a
/// create and its `on:` (explicit or implied by `after_*_commit`) allows create.
fn on_create_path(d: &cb::Declared) -> Result<(), NotOnCreate> {
    match d.chain.as_str() {
        "validation" | "validate" | "save" | "create" | "commit" => {}
        other => return Err(NotOnCreate::ChainNotOnCreatePath(other.into())),
    }
    let on = d
        .options
        .iter()
        .find(|(k, _)| k == "on")
        .map(|(_, v)| v.clone())
        .or_else(|| d.implied_on.map(|s| format!(":{s}")));
    match on {
        Some(on) if !on.contains("create") && !on.contains("save") => {
            Err(NotOnCreate::OnOptionExcludesCreate(on))
        }
        _ => Ok(()),
    }
}

/// Why an invoked callback has no registration for create.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum InvokedNotRegistered {
    /// No declaration in the harvested ancestors (the closed `Slag` of
    /// `callback_differential`, with the stage that would supply it).
    Slag(cb::Slag),
    /// Declared, but the `on:` option says it does not run on create.
    DeclaredButNotOnCreate(NotOnCreate),
    /// In the walk's declared or generated set, absent from the rebuilt
    /// registration order (an include the order reconstruction did not follow).
    InWalkNotInEffectiveChain,
}

/// Why a registered callback was not invoked.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RegisteredNotInvoked {
    /// Predicted by Rust: the contract denied the create, the record never reached
    /// a save, so the save/create/commit chains do not run.
    ContractDeniedBeforeSave,
    /// The closed reason of `callback_differential` (block filter, chain not
    /// exercised, `on:` not exercised, condition not evaluated, unexplained).
    Silent(cb::Silent),
}

struct Harvested {
    ctxs: BTreeMap<String, cb::ModelCtx>,
    ev: cb::Evidence,
}

fn harvest(src: &Path, svc: &Value) -> Harvested {
    let models: BTreeSet<String> = svc["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .flat_map(|c| c["journal"].as_array().expect("journal").iter())
        .map(|e| e["model"].as_str().expect("model").to_string())
        .collect();
    // The same intake `callback_differential` uses: engine patches included into the
    // model, in the order the engines load.
    let cfg = cb::intake_cfg();
    let mut h = cb::harvest_for(src, &cfg);
    let patches = cb::discover_patches(src, &cfg, &mut h);
    let (ctxs, _unresolved) = cb::contexts(&mut h, &models, &patches);
    let ev = cb::scan_tree(src, &models);
    Harvested { ctxs, ev }
}

#[derive(Default)]
struct L2 {
    /// Create-applicable registered callbacks over the models the case touched.
    registered: usize,
    /// Of those, the ones some journal entry matched.
    registered_invoked: usize,
    /// Distinct `(model, phase, label)` journal entries.
    invoked: usize,
    /// Invoked entries with a create-applicable registration (every expansion).
    both: usize,
    invoked_not_registered: BTreeMap<InvokedNotRegistered, Vec<String>>,
    registered_not_invoked: BTreeMap<RegisteredNotInvoked, Vec<String>>,
    /// Groups `(model, phase)` with two or more matched entries.
    order_groups: usize,
    /// Groups whose order is UNKNOWN (counted in `order_groups`, never in `order_differing`).
    order_unknown: usize,
    order_differing: usize,
    order_inversions: usize,
    order_examples: Vec<String>,
    /// Phases the journal shows (for the denied-case prediction).
    invoked_phases: BTreeSet<String>,
}

fn layer2(
    h: &Harvested,
    svc: &Value,
    case: &str,
    predicted_denied: bool,
    transform: impl Fn(&cb::Declared) -> cb::Declared,
) -> L2 {
    let sc = service_case(svc, case);
    // Distinct entries in first-seen order.
    let mut entries: Vec<(String, String, String)> = Vec::new();
    for e in sc["journal"].as_array().expect("journal") {
        let k = (
            e["model"].as_str().expect("model").to_string(),
            e["phase"].as_str().expect("phase").to_string(),
            e["callback"].as_str().expect("callback").to_string(),
        );
        if !entries.contains(&k) {
            entries.push(k);
        }
    }
    let mut out = L2 {
        invoked: entries.len(),
        ..L2::default()
    };
    let models: BTreeSet<&String> = entries.iter().map(|(m, ..)| m).collect();

    // Registration: the effective chain, restricted to what a create runs.
    let registered: BTreeMap<&String, Vec<cb::Declared>> = models
        .iter()
        .map(|m| {
            let all: Vec<cb::Declared> = h.ctxs[*m].effective.iter().map(&transform).collect();
            (*m, all)
        })
        .collect();
    out.registered = registered
        .values()
        .map(|v| v.iter().filter(|d| on_create_path(d).is_ok()).count())
        .sum();

    let mut used: BTreeMap<&String, BTreeSet<usize>> = BTreeMap::new();
    let mut fired_chains: BTreeMap<&String, BTreeSet<String>> = BTreeMap::new();
    // (label, rank, true when the callback is registered in a module whose
    // include-time body ruff left statements unread in that may hide a registration)
    type RankRow = (String, usize, bool);
    let mut ranks: BTreeMap<(String, String), Vec<RankRow>> = BTreeMap::new();
    for (model, phase, label) in &entries {
        out.invoked_phases.insert(phase.clone());
        let ctx = &h.ctxs[model];
        let regs = &registered[model];
        let Some(expanded) = cb::parse_fired(model, phase, label) else {
            continue;
        };
        fired_chains
            .entry(model)
            .or_default()
            .extend(expanded.iter().map(|f| f.chain.clone()));
        let mut entry_rank = Vec::new();
        let mut miss: Option<(InvokedNotRegistered, &cb::Fired)> = None;
        for f in &expanded {
            let hits: Vec<usize> = regs
                .iter()
                .enumerate()
                .filter(|(_, d)| on_create_path(d).is_ok() && cb::declared_matches(d, f))
                .map(|(i, _)| i)
                .collect();
            if hits.is_empty() {
                let off = regs
                    .iter()
                    .find(|d| cb::declared_matches(d, f))
                    .map(|d| on_create_path(d).expect_err("create-applicable would have hit"));
                let in_walk = ctx
                    .declared
                    .iter()
                    .chain(ctx.generated.iter())
                    .any(|d| cb::declared_matches(&transform(d), f));
                let why = match (off, in_walk) {
                    (Some(n), _) => InvokedNotRegistered::DeclaredButNotOnCreate(n),
                    (None, true) => InvokedNotRegistered::InWalkNotInEffectiveChain,
                    (None, false) => InvokedNotRegistered::Slag(cb::classify(f, ctx, &h.ev).0),
                };
                miss = Some((why, f));
                break;
            }
            entry_rank.push(*hits.iter().min().expect("non-empty"));
            used.entry(model).or_default().extend(hits);
        }
        match miss {
            None => {
                out.both += 1;
                if let Some(r) = entry_rank.into_iter().min() {
                    let unread = ctx.order_unknown_scopes.contains(&regs[r].origin)
                        || expanded.iter().any(|f| cb::hidden_could_register(ctx, f));
                    ranks
                        .entry((model.clone(), phase.clone()))
                        .or_default()
                        .push((label.clone(), r, unread));
                }
            }
            Some((why, f)) => out
                .invoked_not_registered
                .entry(why)
                .or_default()
                .push(format!("{model} {phase} {}", f.label)),
        }
    }

    for (model, regs) in &registered {
        let empty = BTreeSet::new();
        let u = used.get(model).unwrap_or(&empty);
        let chains = cb::exercised_chains(&fired_chains.get(model).cloned().unwrap_or_default());
        for (i, d) in regs.iter().enumerate() {
            if on_create_path(d).is_err() || u.contains(&i) {
                continue;
            }
            let why =
                if predicted_denied && matches!(d.chain.as_str(), "save" | "create" | "commit") {
                    RegisteredNotInvoked::ContractDeniedBeforeSave
                } else {
                    RegisteredNotInvoked::Silent(cb::silent_reason(d, &chains))
                };
            let conditions: Vec<String> = d
                .options
                .iter()
                .filter(|(k, _)| matches!(k.as_str(), "if" | "unless" | "on"))
                .map(|(k, v)| format!("{k}: {v}"))
                .collect();
            out.registered_not_invoked
                .entry(why)
                .or_default()
                .push(format!(
                    "{model} {}_{} {:?} (from {}){}",
                    d.kind,
                    d.chain,
                    d.filter,
                    d.origin,
                    if conditions.is_empty() {
                        String::new()
                    } else {
                        format!(" when {}", conditions.join(", "))
                    }
                ));
        }
    }

    out.registered_invoked = used.values().map(BTreeSet::len).sum();

    // Order on the matched subset: recorded order against registration order.
    for ((model, phase), seq) in ranks {
        if seq.len() < 2 {
            continue;
        }
        out.order_groups += 1;
        let inv = seq.windows(2).filter(|w| w[0].1 > w[1].1).count();
        // An entry registered in a scope with unread ControlFlow / DynamicSend /
        // UnreadBlockBody statements has no definite position: UNKNOWN order.
        if seq.iter().any(|r| r.2) {
            out.order_unknown += 1;
            continue;
        }
        if inv > 0 {
            out.order_differing += 1;
            out.order_inversions += inv;
            let mut expected = seq.clone();
            expected.sort_by_key(|(_, r, _)| *r);
            let at = seq
                .iter()
                .zip(&expected)
                .position(|(a, b)| a.0 != b.0)
                .unwrap_or(0);
            out.order_examples.push(format!(
                "{model} {phase}: first difference at #{at}: recorded `{}` where registration puts `{}`",
                seq[at].0, expected[at].0
            ));
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Effects: PREDICTED from admission, or SLAG. Nothing here is executed.
// ─────────────────────────────────────────────────────────────────────────
//
// No Rust code performs a create here (the service stub assigns an id and writes no
// row, and it needs attribute values the oracle does not record), so there is no
// executed effect. The expectation below is derived from the admission rules and the
// shape of the transition (an admitted create saves one work package carrying the
// submitted attributes), not from the trace.

#[derive(Debug, Clone)]
struct Predicted {
    persisted: bool,
    changed: BTreeSet<String>,
    deltas: BTreeMap<String, i64>,
}

fn predict(a: &CreateAdmission) -> Predicted {
    let mut p = Predicted {
        persisted: false,
        changed: BTreeSet::new(),
        deltas: BTreeMap::new(),
    };
    if a.verdict == Verdict::Admitted {
        p.persisted = true;
        p.changed = a.writable.keys().cloned().collect();
        p.deltas.insert("work_packages".into(), 1);
    }
    p
}

/// An oracle effect nothing predicts. Closed: no catch-all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[expect(
    clippy::enum_variant_names,
    reason = "the shared postfix is the point: each names what is not modeled"
)]
enum EffectSlag {
    /// A changed attribute no rule predicts (the author the service sets, the
    /// generated id, timestamps, `position`).
    AttributeNotModeled(String),
    /// A table row delta nothing predicts.
    EffectNotModeled(String),
    /// A job nothing predicts.
    JobNotModeled(String),
}

#[derive(Debug, Default)]
struct Effects {
    /// attribute state: `changed_attributes` (the oracle sorts it: compared as a set)
    attributes: Score,
    /// persistence: `persisted` and `row_deltas` (compared as a map)
    persistence: Score,
    jobs: Score,
    mismatched: Vec<String>,
    /// Predicted, absent from the oracle.
    extra: Vec<String>,
    slag: Vec<EffectSlag>,
    oracle_effects: usize,
}

fn compare_effects(p: &Predicted, sc: &Value) -> Effects {
    let mut out = Effects::default();
    let oracle_persisted = sc["persisted"].as_bool().expect("persisted");
    out.oracle_effects += 1;
    if p.persisted == oracle_persisted {
        out.persistence.add(Category::Predicted, 1);
    } else {
        out.mismatched.push(format!(
            "persisted: rust {}, oracle {oracle_persisted}",
            p.persisted
        ));
    }
    let oracle_changed: BTreeSet<String> = sc["changed_attributes"]
        .as_array()
        .expect("changed")
        .iter()
        .map(|v| v.as_str().expect("name").to_string())
        .collect();
    for a in &oracle_changed {
        out.oracle_effects += 1;
        if p.changed.contains(a) {
            out.attributes.add(Category::Predicted, 1);
        } else {
            out.attributes.add(Category::Slag, 1);
            out.slag.push(EffectSlag::AttributeNotModeled(a.clone()));
        }
    }
    for a in p.changed.difference(&oracle_changed) {
        out.extra.push(format!(
            "changed_attributes: predicts {a}, oracle did not change it"
        ));
    }
    let oracle_deltas: BTreeMap<String, i64> = sc["row_deltas"]
        .as_object()
        .expect("row_deltas")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_i64().expect("count")))
        .collect();
    for (t, n) in &oracle_deltas {
        out.oracle_effects += 1;
        match p.deltas.get(t) {
            Some(m) if m == n => out.persistence.add(Category::Predicted, 1),
            Some(m) => out
                .mismatched
                .push(format!("row_deltas.{t}: rust {m}, oracle {n}")),
            None => {
                out.persistence.add(Category::Slag, 1);
                out.slag.push(EffectSlag::EffectNotModeled(t.clone()));
            }
        }
    }
    for (t, n) in &p.deltas {
        if !oracle_deltas.contains_key(t) {
            out.extra
                .push(format!("row_deltas.{t}: predicts {n}, oracle wrote none"));
        }
    }
    for j in sc["enqueued_jobs"].as_array().expect("jobs") {
        out.oracle_effects += 1;
        out.jobs.add(Category::Slag, 1);
        out.slag.push(EffectSlag::JobNotModeled(
            j.as_str().expect("class").to_string(),
        ));
    }
    out
}

fn tally<K: Ord + Clone>(items: impl Iterator<Item = K>) -> BTreeMap<K, usize> {
    let mut m = BTreeMap::new();
    for i in items {
        *m.entry(i).or_default() += 1;
    }
    m
}

fn show<K: std::fmt::Debug>(m: &BTreeMap<K, usize>) -> String {
    if m.is_empty() {
        return "none".into();
    }
    m.iter()
        .map(|(k, n)| format!("{k:?} x{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

// ─────────────────────────────────────────────────────────────────────────
// The tests
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn checked_in_admission_table_is_what_the_generator_produces() {
    let Some(src) = source() else { return };
    assert_eq!(
        admission_gen::generate(&src),
        CHECKED_IN_TABLE,
        "generated_create_admission.rs is stale: run `cargo run -p op-codegen-pipeline --example emit_create_admission`"
    );
    eprintln!("{}", ExtractionGap::PermissionDeclScan.describe());
}

#[test]
fn create_member_transition_is_executed_and_compared() {
    let Some(src) = source() else { return };
    let svc = json("work-package-service-trace.json");
    let contract = json("work-package-contract-trace.json");
    let service_rb = read("work_package_service.rb");
    let contract_rb = read("work_package_contract.rb");
    let default_perms = percent_i(&service_rb, "PERMISSIONS");

    // ── Authorization: EXECUTED ──────────────────────────────────────────
    let member = run_auth(&svc, &contract, &service_rb, &default_perms, CASE, None);
    let denied = run_auth(
        &svc,
        &contract,
        &service_rb,
        &default_perms,
        DENIED_CASE,
        None,
    );
    eprintln!("== AUTHORIZATION (EXECUTED: op_services::CreateWorkPackageService::call -> op_contracts admission)");
    eprintln!(
        "   case {}: user {:?}, submitted {:?}",
        member.case, member.user, member.submitted
    );
    eprintln!(
        "   facts: borrowed from {} (the service trace records none)",
        member.borrowed_from
    );
    eprintln!(
        "   verdict {:?}; oracle success={}; errors {:?} vs oracle {:?}",
        member.admission.verdict,
        member.oracle_success,
        member.admission.errors,
        member.oracle_errors
    );
    for (col, w) in &member.admission.writable {
        eprintln!("   writable {col}: {w:?}");
    }
    eprintln!(
        "   real call(): success={}, errors {:?} (oracle success={}, errors {:?})",
        member.call_success, member.call_errors, member.oracle_success, member.oracle_errors
    );
    eprintln!(
        "   real call() denied case: success={}, errors {:?}",
        denied.call_success, denied.call_errors
    );
    eprintln!("   rules not evaluated: {:?}", not_evaluated_validators());
    eprintln!("   evaluated: user_allowed_to_add (hand-ported), read-only check per submitted attribute, generated writable guards + permissions");
    // The test user answers for the case's project and nothing else.
    let probe = OracleUser {
        kind: member.user.clone(),
    };
    assert!(probe.allowed_in_project("add_work_packages", CASE_PROJECT));
    assert!(!probe.allowed_in_project("add_work_packages", CASE_PROJECT + 1));
    assert_eq!(member.admission.verdict, Verdict::Admitted);
    assert_eq!(
        member.verdict_match,
        Some(true),
        "admission differs from the oracle"
    );
    assert_eq!(
        member.errors_match,
        Some(true),
        "error keys differ from the oracle"
    );
    assert!(
        member.writable_compared >= 5,
        "{} attributes compared",
        member.writable_compared
    );
    assert!(member.mismatches.is_empty(), "{:?}", member.mismatches);

    let wide = contract_wide(&contract, &contract_rb);
    eprintln!(
        "   contract-wide: {} (record, attribute) pairs compared, {} matched; unresolved: {}",
        wide.compared,
        wide.matched,
        show(&wide.unresolved)
    );
    for m in &wide.mismatches {
        eprintln!("   MISMATCH {m}");
    }
    assert!(
        wide.compared >= FLOOR_CONTRACT_COMPARED,
        "only {} pairs compared",
        wide.compared
    );
    assert!(
        wide.mismatches.is_empty(),
        "{} mismatches",
        wide.mismatches.len()
    );

    // ── Callbacks: registrations RECONSTRUCTED, bodies not executed ──────
    let h = harvest(&src, &svc);
    let admitted = member.admission.verdict == Verdict::Admitted;
    let l2 = layer2(&h, &svc, CASE, !admitted, Clone::clone);
    eprintln!("== CALLBACKS (PREDICTED registrations vs the oracle journal; no body is executed)");
    eprintln!(
        "   registered declarations for create {} (invoked {}, not invoked {}); journal entries {}, of which registered {}",
        l2.registered,
        l2.registered_invoked,
        l2.registered_not_invoked.values().map(Vec::len).sum::<usize>(),
        l2.invoked,
        l2.both
    );
    for (why, rows) in &l2.invoked_not_registered {
        eprintln!("   invoked, not registered: {why:?} x{}", rows.len());
    }
    for (why, rows) in &l2.registered_not_invoked {
        eprintln!("   registered, not invoked: {why:?} x{}", rows.len());
        for row in rows.iter().take(3) {
            eprintln!("       {row}");
        }
    }
    eprintln!(
        "   order: compared only inside one (model, kind_chain) group, where ActiveSupport::Callbacks \
         runs a chain's callbacks in registration order (callbacks.rb:645-663 for re-registration); \
         across groups Rails guarantees nothing here. {} groups, {} UNKNOWN (unread include-time statements), {} differ, {} inversions",
        l2.order_groups, l2.order_unknown, l2.order_differing, l2.order_inversions
    );
    for e in &l2.order_examples {
        eprintln!("       {e}");
    }
    let inr: usize = l2.invoked_not_registered.values().map(Vec::len).sum();
    assert_eq!(
        l2.both + inr,
        l2.invoked,
        "an invoked entry is neither matched nor classified"
    );
    assert!(l2.both > 0 && l2.registered > 0, "nothing compared");
    assert_eq!(
        l2.registered,
        l2.registered_invoked
            + l2.registered_not_invoked
                .values()
                .map(Vec::len)
                .sum::<usize>(),
        "a registered callback is neither invoked nor classified"
    );
    for wrong in [
        cb::Silent::ChainNotExercised,
        cb::Silent::OnOptionNotExercised,
    ] {
        assert!(
            l2.registered_not_invoked
                .get(&RegisteredNotInvoked::Silent(wrong))
                .is_none_or(Vec::is_empty),
            "registered for create but {wrong:?}"
        );
    }
    assert!(
        l2.registered_not_invoked
            .get(&RegisteredNotInvoked::Silent(cb::Silent::Unexplained))
            .is_none_or(Vec::is_empty),
        "registered, unconditioned callbacks on an exercised chain that never ran"
    );
    assert!(
        !l2.registered_not_invoked.is_empty(),
        "every registered callback was invoked: registration and invocation were conflated"
    );
    let control_l2 = layer2(&h, &svc, CASE, false, cb::reversed);
    eprintln!(
        "   negative control (names reversed): both {} of {}",
        control_l2.both, control_l2.invoked
    );
    assert!(
        control_l2.both * 50 <= l2.both,
        "the callback matcher does not discriminate"
    );

    // ── Effects: PREDICTED or SLAG ───────────────────────────────────────
    let sc = service_case(&svc, CASE);
    let predicted = predict(&member.admission);
    let fx = compare_effects(&predicted, &sc);
    let slag = tally(fx.slag.iter().cloned());
    eprintln!("== ATTRIBUTE STATE / PERSISTENCE / JOBS (PREDICTED or SLAG; nothing executed)");
    eprintln!(
        "   oracle effects {}: attribute state {:?}, persistence {:?}, jobs {:?}; mismatched {}, predicted-only {}",
        fx.oracle_effects, fx.attributes, fx.persistence, fx.jobs, fx.mismatched.len(), fx.extra.len()
    );
    for s in &slag {
        eprintln!("   SLAG {:?}", s.0);
    }
    assert!(fx.mismatched.is_empty(), "{:?}", fx.mismatched);
    assert!(fx.extra.is_empty(), "{:?}", fx.extra);
    assert_eq!(
        fx.attributes.predicted
            + fx.attributes.slag
            + fx.persistence.predicted
            + fx.persistence.slag
            + fx.jobs.slag,
        fx.oracle_effects,
        "an oracle effect was neither compared nor classified"
    );
    for job in sc["enqueued_jobs"].as_array().expect("jobs") {
        assert!(fx.slag.contains(&EffectSlag::JobNotModeled(
            job.as_str().expect("class").into()
        )));
    }

    // ── Negative control (a): no permission ──────────────────────────────
    eprintln!("== control (a): {DENIED_CASE}");
    eprintln!(
        "   verdict {:?}; errors {:?}; oracle errors {:?}",
        denied.admission.verdict, denied.admission.errors, denied.oracle_errors
    );
    assert_eq!(denied.admission.verdict, Verdict::Denied);
    assert!(!denied.oracle_success);
    assert_eq!(denied.verdict_match, Some(true));
    assert_eq!(
        denied.errors_match,
        Some(true),
        "error keys differ from the oracle"
    );
    assert!(
        denied.writable_compared >= 5 && denied.mismatches.is_empty(),
        "{:?}",
        denied.mismatches
    );
    assert!(denied.admission.errors.contains_key("base") && denied.admission.errors.len() > 1);
    let denied_fx = compare_effects(
        &predict(&denied.admission),
        &service_case(&svc, DENIED_CASE),
    );
    assert!(
        denied_fx.mismatched.is_empty() && denied_fx.extra.is_empty() && denied_fx.slag.is_empty()
    );
    assert_eq!(
        denied_fx.persistence.predicted, 1,
        "no row delta predicted, none observed"
    );
    let denied_l2 = layer2(&h, &svc, DENIED_CASE, true, Clone::clone);
    let write_phases = [
        "before_save",
        "after_save",
        "around_save",
        "before_create",
        "after_create",
        "after_commit",
    ];
    assert!(
        denied_l2
            .invoked_phases
            .iter()
            .all(|p| !write_phases.contains(&p.as_str())),
        "a save/create/commit callback ran although the contract denied"
    );
    let skipped = denied_l2
        .registered_not_invoked
        .get(&RegisteredNotInvoked::ContractDeniedBeforeSave)
        .map_or(0, Vec::len);
    eprintln!("   journal phases {:?}; save/create/commit callbacks not run, explained by the denial: {skipped}", denied_l2.invoked_phases);
    assert!(skipped > 0);
    assert!(l2
        .invoked_phases
        .iter()
        .any(|p| write_phases.contains(&p.as_str())));

    // ── Negative control (b): flip one submitted attribute's guard ───────
    let facts = facts_of(analogue(&contract, member.user.label()));
    let cols: Vec<String> = member.submitted.iter().map(|k| changed_column(k)).collect();
    let user = OracleUser {
        kind: member.user.clone(),
    };
    let (col, changed, perturbed) =
        perturb(&user, &cols, &facts).expect("no submitted attribute has a flippable guard");
    let flipped = run_auth(
        &svc,
        &contract,
        &service_rb,
        &default_perms,
        CASE,
        Some(&perturbed),
    );
    eprintln!("== control (b): perturb {col} by changing facts {changed:?}");
    eprintln!(
        "   verdict {:?} -> {:?}; writable {col}: {:?} -> {:?}; errors {:?}",
        member.admission.verdict,
        flipped.admission.verdict,
        member.admission.writable[&col],
        flipped.admission.writable[&col],
        flipped.admission.errors
    );
    // `status_id_change || !closed_version_and_status?`, helper inlined by ruff: the
    // smallest flip is an unchanged status that is closed, in a closed version.
    assert_eq!(changed.len(), 3, "unexpected minimal flip {changed:?}");
    assert_ne!(member.admission.verdict, flipped.admission.verdict);
    assert_ne!(
        member.admission.writable[&col],
        flipped.admission.writable[&col]
    );
    assert_eq!(
        flipped.admission.errors.get(&col),
        Some(&vec!["error_readonly"])
    );
    assert_eq!(
        flipped.verdict_match,
        Some(false),
        "the oracle saw the unperturbed input"
    );
    let flipped_fx = compare_effects(&predict(&flipped.admission), &sc);
    eprintln!(
        "   predicted effects under the perturbed denial: mismatched {:?}",
        flipped_fx.mismatched
    );
    assert!(
        !flipped_fx.mismatched.is_empty(),
        "effects ignored the admission verdict"
    );

    // ── Acceptance report ────────────────────────────────────────────────
    let exec_member = 1 + member.oracle_errors.len() + member.writable_compared;
    let exec_denied = 1 + denied.oracle_errors.len() + denied.writable_compared;
    let exec_wide = wide.compared;
    let mut parity = Score::default();
    parity.add(Category::Executed, exec_member + exec_denied + exec_wide);
    assert_eq!(member.writable_matched, member.writable_compared);
    assert_eq!(denied.writable_matched, denied.writable_compared);
    eprintln!("== ACCEPTANCE REPORT {CASE} (OpenProject 2725194)");
    eprintln!(
        "   1. contract rules EXECUTED: {} items. {CASE}: verdict 1, error keys {}, writable? {} of {} submitted; {DENIED_CASE}: verdict 1, error keys {}, writable? {}; contract-wide writable? pairs {} ({} matched, {} unresolved). Execution parity: {} items (case + denied + contract-wide), 0 mismatches",
        parity.executed,
        member.oracle_errors.len(),
        member.writable_compared,
        member.submitted.len(),
        denied.oracle_errors.len(),
        denied.writable_compared,
        wide.compared,
        wide.matched,
        wide.unresolved.values().sum::<usize>(),
        parity.executed
    );
    eprintln!(
        "   2. callback registrations RECONSTRUCTED (predicted): {} declarations for create; {} journal entries registered, {} classified slag",
        l2.registered, l2.both, inr
    );
    eprintln!("   3. callback bodies EXECUTED: 0 (the journal is the oracle's; no Rust callback body runs)");
    eprintln!("   4. DB effects PERFORMED: 0 (the service stub writes no row)");
    eprintln!(
        "   5. effects PREDICTED: attribute state {}, persistence {}, jobs {}",
        fx.attributes.predicted, fx.persistence.predicted, fx.jobs.predicted
    );
    eprintln!(
        "   6. effects UNRESOLVED (slag): {} ({})",
        fx.slag.len(),
        show(&tally(fx.slag.iter().map(|k| match k {
            EffectSlag::AttributeNotModeled(a) => format!("AttributeNotModeled:{a}"),
            EffectSlag::EffectNotModeled(t) => format!("EffectNotModeled:{t}"),
            EffectSlag::JobNotModeled(j) => format!("JobNotModeled:{j}"),
        })))
    );
    eprintln!(
        "   typed extraction gap: {}",
        ExtractionGap::PermissionDeclScan.describe()
    );
}
