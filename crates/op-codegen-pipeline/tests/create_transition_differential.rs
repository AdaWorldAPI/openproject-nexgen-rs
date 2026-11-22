//! One executed ON/WHEN/DO transition: `WorkPackages::CreateService` for the
//! `create_member` case, evaluated on the Rust side and compared with what real
//! OpenProject did (`oracle/work-package-service-trace.json`,
//! `oracle/work-package-contract-trace.json`).
//!
//! Three layers, kept apart in the assertions and in the output:
//!
//! 1. WHEN (contract admission). `CreateContract` is evaluated: each submitted
//!    attribute's `writable:` guard (lowered by `ruff_ruby_spo::extract_contract_guards`)
//!    and the permission the contract declares for it, for the role the oracle
//!    setup gave the user, plus `user_allowed_to_add`. The verdict and the
//!    per-attribute `writable` answers are compared with the oracle.
//! 2. Registration versus invocation. The callbacks Rails registers for a
//!    create (the effective chain harvested by `callback_differential`) against
//!    the ones the oracle journal shows invoked. Registration is never counted
//!    as invocation.
//! 3. DO (observable effects). What the Rust create path assigns and persists
//!    against `changed_attributes`, `row_deltas`, `persisted`, `enqueued_jobs`.
//!    Every oracle effect is either predicted (and compared) or a typed slag.
//!
//! The Rust create path that is exercised is `op_services::CreateWorkPackageService`
//! (a stub: it validates through `op_contracts::CreateWorkPackageContract` and
//! assigns an id, nothing else). The admission verdict does NOT come from it: its
//! contract is hand-written, so the verdict is evaluated from the lowered guards
//! and the scanned permission declarations. The stub supplies the entity the
//! effect layer predicts from and a second admission opinion that is reported.
//!
//! Needs `OPENPROJECT_SRC` (a checkout at the trace revision). Without it the
//! test prints UNVERIFIED and passes, unless `REQUIRE_SOURCES=1`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use op_contracts::base::UserContext;
use op_core::traits::Id;
use op_services::work_packages::{CreateWorkPackageService, WorkPackageParams};
use ruff_ruby_spo::{extract_contract_guards, ContractGuard, GuardExpr, GuardOutcome};
use serde_json::Value;

/// The callback harvest and the effective Rails registration order are the ones
/// `callback_differential` measures; its items are `pub` for this reuse. Its own
/// test also runs in this binary (same result, a few seconds).
#[allow(dead_code)]
#[path = "callback_differential.rs"]
mod cb;

const CASE: &str = "create_member";
const DENIED_CASE: &str = "create_no_permission";
const CREATE: &str = "WorkPackages::CreateContract";
const BASE: &str = "WorkPackages::BaseContract";

/// Lowered fact name -> fact key in the contract trace (as in `guard_differential`).
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
    // Not a trace fact: the oracle setup says whether the user is an admin.
    ("user.admin?", "user.admin?"),
];

/// Measured at OpenProject 2725194 on the committed traces.
const FLOOR_CONTRACT_COMPARED: usize = 100;

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
    let at = text.find(&head).unwrap_or_else(|| panic!("no {name} in the oracle script"));
    let rest = &text[at + head.len()..];
    let end = rest.find(']').expect("unterminated %i[");
    rest[..end].split_whitespace().map(str::to_string).collect()
}

/// The text of one case lambda in a `CASES = { "name" => lambda { ... } }` table.
fn case_block<'a>(rb: &'a str, name: &str) -> &'a str {
    let start = rb
        .find(&format!("\"{name}\" => lambda"))
        .unwrap_or_else(|| panic!("no case {name}"));
    let rest = &rb[start + 1..];
    let end = rest
        .find("\" => lambda")
        .map_or_else(|| rest.find(".freeze").expect("end of CASES"), |e| e);
    &rb[start..start + 1 + end]
}

/// Who the case runs as, read from the lambda body.
fn user_of_block(block: &str, default_perms: &[String]) -> UserKind {
    if block.contains("create(:admin)") {
        UserKind::Admin
    } else if block.contains("member_user(project, [])") || block.contains("member_user(wp.project, [])") {
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
    let body = &rb[def..];
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
// Contract declarations ruff does not lower: `permission:` and aliases
// ─────────────────────────────────────────────────────────────────────────
//
// `extract_contract_guards` lowers `writable:` only. Which permission an
// attribute needs is declared next to it (`attribute :x, permission: ...`,
// `default_attribute_permission ...`), and `BaseContract#reduce_by_writable_permissions`
// (`app/contracts/base_contract.rb:~245-265`) combines the two. The declarations
// are read here with a line scan of the two contract files; that is a gap in
// ruff's contract arm, named in the oracle README, not a rule written by hand.

#[derive(Debug, Clone, Default)]
struct AttrInfo {
    /// `writable: false` written literally.
    literal_false: bool,
    /// The writable condition comes from a macro ruff cannot see.
    opaque_writable: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ContractModel {
    /// Canonical attribute name (no `_id`) -> declaration.
    declared: BTreeMap<String, AttrInfo>,
    /// Permission list as declared (`status_id`, `target_versions`, ...).
    perms: BTreeMap<String, Vec<String>>,
    default_perm: Option<Vec<String>>,
    /// `attribute_alias :db, :outside` as outside canonical -> db canonical.
    alias: BTreeMap<String, String>,
    /// `validate :method` statements, in order.
    validators: Vec<String>,
}

fn canon(attr: &str) -> String {
    attr.strip_suffix("_id").unwrap_or(attr).to_string()
}

fn ident_after(s: &str, prefix: &str) -> Option<String> {
    let rest = s.trim_start().strip_prefix(prefix)?;
    let id: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

/// `%i[a b]` or `:a` at the start of `v`.
fn symbol_list(v: &str) -> Vec<String> {
    let v = v.trim_start();
    if let Some(rest) = v.strip_prefix("%i[") {
        let end = rest.find(']').unwrap_or(rest.len());
        rest[..end].split_whitespace().map(str::to_string).collect()
    } else if let Some(id) = ident_after(v, ":") {
        vec![id]
    } else {
        Vec::new()
    }
}

/// Fold one contract file into `m` (later files override earlier ones, as a
/// subclass overrides its ancestors in `collect_ancestor_attributes`).
fn scan_contract_file(text: &str, m: &mut ContractModel) {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with("attribute ") && t.contains(':') {
            // A statement continues while the line ends with a comma.
            let mut stmt = t.to_string();
            while stmt.trim_end().ends_with(',') && i + 1 < lines.len() {
                i += 1;
                stmt.push(' ');
                stmt.push_str(lines[i].trim());
            }
            if let Some(name) = ident_after(&stmt, "attribute :") {
                let mut info = AttrInfo::default();
                if let Some(at) = stmt.find("writable:") {
                    info.literal_false = stmt[at + 9..].trim_start().starts_with("false");
                }
                if let Some(at) = stmt.find("permission:") {
                    let list = symbol_list(&stmt[at + 11..]);
                    if !list.is_empty() {
                        m.perms.insert(name.clone(), list);
                    }
                }
                m.declared.insert(canon(&name), info);
            }
        } else if let Some(rest) = t.strip_prefix("attribute_alias ") {
            let ids: Vec<String> = rest
                .split(',')
                .filter_map(|p| ident_after(p, ":"))
                .collect();
            if let [db, outside] = ids.as_slice() {
                m.alias.insert(canon(outside), canon(db));
            }
        } else if let Some(rest) = t.strip_prefix("default_attribute_permission ") {
            m.default_perm = Some(symbol_list(rest));
        } else if let Some(rest) = t.strip_prefix("allow_writable_timestamps ") {
            for name in symbol_list(rest) {
                m.declared.insert(
                    canon(&name),
                    AttrInfo {
                        literal_false: false,
                        opaque_writable: Some(
                            "AdminWritableTimestamps.allow_writable_timestamps".into(),
                        ),
                    },
                );
            }
        } else if let Some(name) = ident_after(t, "validate :") {
            m.validators.push(name);
        }
        i += 1;
    }
}

fn contract_model(src: &Path) -> ContractModel {
    let mut m = ContractModel::default();
    for f in ["base_contract.rb", "create_contract.rb"] {
        let text = std::fs::read_to_string(src.join("app/contracts/work_packages").join(f))
            .unwrap_or_else(|e| panic!("{f}: {e}"));
        scan_contract_file(&text, &mut m);
    }
    m
}

// ─────────────────────────────────────────────────────────────────────────
// Guard evaluation (three-valued: a fact the oracle did not record is unknown)
// ─────────────────────────────────────────────────────────────────────────

type Facts = BTreeMap<String, Value>;

/// Kleene evaluation. `Err(names)` lists the unrecorded facts that decided
/// nothing; `And(false, unknown)` is still `false`.
fn eval(e: &GuardExpr, facts: &Facts) -> Result<bool, Vec<String>> {
    match e {
        GuardExpr::Const(b) => Ok(*b),
        GuardExpr::Fact(name) => {
            let key = FACT_ALIASES.iter().find(|(f, _)| f == name).map(|(_, k)| *k);
            match key.and_then(|k| facts.get(k)) {
                Some(Value::Bool(b)) => Ok(*b),
                Some(Value::Null) => Ok(false),
                _ => Err(vec![name.clone()]),
            }
        }
        GuardExpr::Not(a) => eval(a, facts).map(|b| !b),
        GuardExpr::And(a, b) => match (eval(a, facts), eval(b, facts)) {
            (Ok(false), _) | (_, Ok(false)) => Ok(false),
            (Ok(true), Ok(true)) => Ok(true),
            (x, y) => Err(x.err().into_iter().chain(y.err()).flatten().collect()),
        },
        GuardExpr::Or(a, b) => match (eval(a, facts), eval(b, facts)) {
            (Ok(true), _) | (_, Ok(true)) => Ok(true),
            (Ok(false), Ok(false)) => Ok(false),
            (x, y) => Err(x.err().into_iter().chain(y.err()).flatten().collect()),
        },
    }
}

fn fact_names(e: &GuardExpr, out: &mut BTreeSet<String>) {
    match e {
        GuardExpr::Const(_) => {}
        GuardExpr::Fact(n) => {
            out.insert(n.clone());
        }
        GuardExpr::Not(a) => fact_names(a, out),
        GuardExpr::And(a, b) | GuardExpr::Or(a, b) => {
            fact_names(a, out);
            fact_names(b, out);
        }
    }
}

/// The guard for `attr`: `CreateContract`'s own, else `BaseContract`'s
/// (`writable_conditions` is keyed by the attribute without `_id`).
fn guard_for<'a>(guards: &'a [ContractGuard], canon_attr: &str) -> Option<&'a ContractGuard> {
    let find = |class: &str| {
        guards
            .iter()
            .find(|g| g.class == class && canon(&g.attribute) == canon_attr)
    };
    find(CREATE).or_else(|| find(BASE))
}

/// Why a verdict could not be made. Closed: no catch-all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Unmodeled {
    /// ruff could not lower the `writable:` value.
    GuardUnlowered(String),
    /// A lowered guard reads a fact the oracle did not record.
    FactUnrecorded(String),
    /// The writable condition is produced by a macro ruff does not expand.
    OpaqueWritable(String),
    /// What an admin may do is recorded by neither trace.
    AdminNotModeled,
    /// No `attribute` declaration in the scanned contract files: the oracle
    /// attribute comes from an engine patch or a module contract.
    UndeclaredInScan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Writable {
    Yes,
    No,
    Unmodeled(Unmodeled),
}

impl Writable {
    fn of(b: bool) -> Self {
        if b {
            Writable::Yes
        } else {
            Writable::No
        }
    }
}

struct Rust<'a> {
    model: &'a ContractModel,
    guards: &'a [ContractGuard],
}

impl Rust<'_> {
    /// `BaseContract#writable?`: the writable condition and the permission, both
    /// must hold; a known `false` wins over an unknown other half.
    fn writable(&self, attr: &str, user: &UserKind, facts: &Facts) -> Writable {
        let c = self.model.alias.get(&canon(attr)).cloned().unwrap_or_else(|| canon(attr));
        let Some(info) = self.model.declared.get(&c) else {
            return Writable::Unmodeled(Unmodeled::UndeclaredInScan);
        };
        let mut facts = facts.clone();
        facts.insert("user.admin?".into(), Value::Bool(*user == UserKind::Admin));

        // Writable condition.
        let cond: Result<bool, Unmodeled> = if info.literal_false {
            Ok(false)
        } else if let Some(m) = &info.opaque_writable {
            Err(Unmodeled::OpaqueWritable(m.clone()))
        } else {
            match guard_for(self.guards, &c).map(|g| &g.outcome) {
                None => Ok(true),
                Some(GuardOutcome::Lowered(e)) => eval(e, &facts)
                    .map_err(|n| Unmodeled::FactUnrecorded(n.join(", "))),
                Some(GuardOutcome::Unlowered { reason }) => {
                    Err(Unmodeled::GuardUnlowered(reason.clone()))
                }
            }
        };

        // Permission.
        let perm: Result<bool, Unmodeled> = match user {
            UserKind::Admin => Err(Unmodeled::AdminNotModeled),
            UserKind::Member(held) => {
                let needed = self
                    .model
                    .perms
                    .get(&c)
                    .or_else(|| self.model.perms.get(&format!("{c}_id")))
                    .or(self.model.default_perm.as_ref());
                Ok(needed.is_none_or(|n| n.iter().any(|p| held.contains(p))))
            }
        };

        match (cond, perm) {
            (Ok(false), _) | (_, Ok(false)) => Writable::No,
            (Ok(true), Ok(true)) => Writable::Yes,
            (Err(u), _) | (_, Err(u)) => Writable::Unmodeled(u),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Layer 1: WHEN (contract admission)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    Admit,
    Deny,
    Unrecorded(Unmodeled),
}

#[derive(Debug, Clone)]
struct Admission {
    verdict: Verdict,
    /// Predicted `errors`, as the service result carries them: attribute (or
    /// `base`) -> error codes.
    errors: BTreeMap<String, Vec<String>>,
    writable: BTreeMap<String, Writable>,
}

/// `CreateContract#valid?` for the rules the Rust side can evaluate: the
/// `user_allowed_to_add` validation and the read-only check of every submitted
/// attribute. Every other rule is listed by [`unevaluated_rules`].
fn admit(
    r: &Rust,
    user: &UserKind,
    cols: &[String],
    project_given: bool,
    facts: &Facts,
) -> Admission {
    let mut errors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut writable = BTreeMap::new();
    let mut unmodeled: Option<Unmodeled> = None;

    // `user_allowed_to_add`: allowed in the model's project, and in any project.
    match user {
        UserKind::Member(held) => {
            // `member_with_permissions { project => perms }`: the same grant
            // answers both the project check and the any-project check.
            let in_project = held.iter().any(|p| p == "add_work_packages");
            let in_any_project = in_project;
            if (project_given && !in_project) || !in_any_project {
                errors
                    .entry("base".into())
                    .or_default()
                    .push("error_unauthorized".into());
            }
        }
        UserKind::Admin => unmodeled = Some(Unmodeled::AdminNotModeled),
    }
    for col in cols {
        let w = r.writable(col, user, facts);
        match &w {
            Writable::No => errors
                .entry(col.clone())
                .or_default()
                .push("error_readonly".into()),
            Writable::Unmodeled(u) => unmodeled = unmodeled.take().or(Some(u.clone())),
            Writable::Yes => {}
        }
        writable.insert(col.clone(), w);
    }
    let verdict = match unmodeled {
        Some(u) => Verdict::Unrecorded(u),
        None if errors.is_empty() => Verdict::Admit,
        None => Verdict::Deny,
    };
    Admission {
        verdict,
        errors,
        writable,
    }
}

/// Contract rules that run in `CreateContract#valid?` and are NOT evaluated here:
/// the `validate :x` statements of the two contract files (minus the one that is),
/// the `attribute ... do` blocks, and the model validations `ModelContract#valid?`
/// runs first (`app/contracts/model_contract.rb:85-88`).
fn unevaluated_rules(m: &ContractModel) -> Vec<String> {
    let mut v: Vec<String> = m
        .validators
        .iter()
        .filter(|n| *n != "user_allowed_to_add")
        .map(|n| format!("ContractRuleNotEvaluated:{n}"))
        .collect();
    v.push("ContractRuleNotEvaluated:attribute-blocks".into());
    v.push("ModelValidationsNotEvaluated:ModelContract#valid?".into());
    v
}

/// The attribute names the service case submits, as the contract sees them: the
/// changed column. An association (`project`) changes its foreign key
/// (`project_id`); the trace's `writable` map tells which names exist.
fn columns(submitted: &[String], oracle_keys: &BTreeSet<String>) -> Vec<String> {
    submitted
        .iter()
        .map(|k| {
            let fk = format!("{k}_id");
            if oracle_keys.contains(&fk) {
                fk
            } else {
                k.clone()
            }
        })
        .collect()
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

/// A `CreateContract` record of the contract trace for a user label. The
/// service trace records no facts; these are the facts of the closest recorded
/// analogue (same contract, same user, a new unsaved work package).
fn facts_for_service_case<'a>(contract: &'a Value, label: &str) -> &'a Value {
    contract["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|r| r["contract"] == CREATE && r["user"] == label)
        .unwrap_or_else(|| panic!("no CreateContract record for user {label}"))
}

fn facts_of(rec: &Value) -> Facts {
    rec["facts"]
        .as_object()
        .expect("facts")
        .clone()
        .into_iter()
        .collect()
}

struct L1 {
    case: String,
    user: UserKind,
    borrowed_from: String,
    submitted: Vec<String>,
    admission: Admission,
    oracle_success: bool,
    oracle_errors: BTreeMap<String, Vec<String>>,
    /// Predicted verdict agrees with the oracle `success` (None: unrecorded).
    verdict_match: Option<bool>,
    /// Predicted `errors` equal the oracle's exactly (None: unrecorded).
    errors_match: Option<bool>,
    /// Submitted attributes compared / matched against the oracle's `writable`.
    sub_compared: usize,
    sub_matched: usize,
    mismatches: Vec<String>,
}

fn layer1(
    r: &Rust,
    svc: &Value,
    contract: &Value,
    service_rb: &str,
    default_perms: &[String],
    case: &str,
    facts_override: Option<&Facts>,
) -> L1 {
    let block = case_block(service_rb, case);
    let user = user_of_block(block, default_perms);
    let sc = service_case(svc, case);
    assert_eq!(
        sc["user"], user.label(),
        "the user read out of the setup script disagrees with the trace label"
    );
    let rec = facts_for_service_case(contract, user.label());
    let facts = facts_override.cloned().unwrap_or_else(|| facts_of(rec));
    let oracle_keys: BTreeSet<String> = rec["writable"]
        .as_object()
        .expect("writable")
        .keys()
        .cloned()
        .collect();
    let submitted = submitted_of(service_rb, block);
    let cols = columns(&submitted, &oracle_keys);
    let admission = admit(r, &user, &cols, submitted.iter().any(|s| s == "project"), &facts);

    let oracle_errors = codes(&sc["errors"]);
    let oracle_success = sc["success"].as_bool().expect("success");
    let recorded = !matches!(admission.verdict, Verdict::Unrecorded(_));
    let verdict_match =
        recorded.then(|| (admission.verdict == Verdict::Admit) == oracle_success);
    let errors_match = recorded.then(|| admission.errors == oracle_errors);

    let (mut compared, mut matched, mut mismatches) = (0, 0, Vec::new());
    for col in &cols {
        let oracle_writable = !oracle_errors
            .get(col)
            .is_some_and(|c| c.iter().any(|e| e == "error_readonly"));
        match admission.writable[col] {
            Writable::Unmodeled(_) => {}
            ref w => {
                compared += 1;
                if (*w == Writable::Yes) == oracle_writable {
                    matched += 1;
                } else {
                    mismatches.push(format!("{case} {col}: rust {w:?}, oracle writable={oracle_writable}"));
                }
            }
        }
    }
    L1 {
        case: case.into(),
        user,
        borrowed_from: format!("contract trace case `{}` ({CREATE}, user {})", rec["name"].as_str().unwrap_or("?"), rec["user"]),
        submitted,
        admission,
        oracle_success,
        oracle_errors,
        verdict_match,
        errors_match,
        sub_compared: compared,
        sub_matched: matched,
        mismatches,
    }
}

/// Every attribute of every `CreateContract` record in the contract trace:
/// guard + permission against the recorded `writable?`.
#[derive(Default)]
struct ContractWide {
    compared: usize,
    matched: usize,
    mismatches: Vec<String>,
    unmodeled: BTreeMap<Unmodeled, usize>,
}

fn contract_wide(r: &Rust, contract: &Value, contract_rb: &str, transform: impl Fn(&Facts) -> Facts) -> ContractWide {
    let default_perms = percent_i(contract_rb, "PERMISSIONS");
    let mut out = ContractWide::default();
    for rec in contract["cases"].as_array().expect("cases") {
        if rec["contract"] != CREATE {
            continue;
        }
        let name = rec["name"].as_str().expect("name");
        let user = user_of_block(case_block(contract_rb, name), &default_perms);
        assert_eq!(rec["user"], user.label(), "contract case {name}: user label");
        let facts = transform(&facts_of(rec));
        for (attr, want) in rec["writable"].as_object().expect("writable") {
            match r.writable(attr, &user, &facts) {
                Writable::Unmodeled(u) => *out.unmodeled.entry(u).or_default() += 1,
                w => {
                    out.compared += 1;
                    if (w == Writable::Yes) == want.as_bool().expect("bool") {
                        out.matched += 1;
                    } else {
                        out.mismatches
                            .push(format!("{name} {} {attr}: rust {w:?}, oracle {want}", rec["user"]));
                    }
                }
            }
        }
    }
    out
}

/// Negative control (b): find a submitted attribute whose lowered guard reads
/// recorded facts, and the smallest change of those facts that flips it.
/// Returns `(column, changed fact keys, perturbed facts)`.
fn perturb(r: &Rust, user: &UserKind, cols: &[String], facts: &Facts) -> Option<(String, Vec<String>, Facts)> {
    let mut with_user = facts.clone();
    with_user.insert("user.admin?".into(), Value::Bool(*user == UserKind::Admin));
    for col in cols {
        let c = canon(col);
        let Some(GuardOutcome::Lowered(e)) = guard_for(r.guards, &c).map(|g| &g.outcome) else {
            continue;
        };
        let Ok(base) = eval(e, &with_user) else { continue };
        let mut names = BTreeSet::new();
        fact_names(e, &mut names);
        let keys: Vec<&str> = names
            .iter()
            .filter_map(|n| FACT_ALIASES.iter().find(|(f, _)| f == n).map(|(_, k)| *k))
            .filter(|k| *k != "user.admin?")
            .collect();
        let mut best: Option<(usize, Vec<String>, Facts)> = None;
        for mask in 0u32..(1 << keys.len()) {
            let mut f = facts.clone();
            let mut changed = Vec::new();
            for (i, k) in keys.iter().enumerate() {
                let was = matches!(facts.get(*k), Some(Value::Bool(true)));
                let now = mask & (1 << i) != 0;
                f.insert((*k).to_string(), Value::Bool(now));
                if now != was {
                    changed.push((*k).to_string());
                }
            }
            let mut f_user = f.clone();
            f_user.insert("user.admin?".into(), Value::Bool(*user == UserKind::Admin));
            if eval(e, &f_user) == Ok(!base)
                && best.as_ref().is_none_or(|(n, ..)| changed.len() < *n)
            {
                best = Some((changed.len(), changed, f));
            }
        }
        if let Some((_, changed, f)) = best {
            return Some((col.clone(), changed, f));
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────
// Layer 2: registration versus invocation
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
    let (ctxs, _unresolved) = cb::contexts(src, &models);
    let ev = cb::scan_tree(src, &models);
    Harvested { ctxs, ev }
}

#[derive(Default)]
struct L2 {
    /// Create-applicable registered callbacks over the models the case touched.
    registered: usize,
    /// Distinct `(model, phase, label)` journal entries.
    invoked: usize,
    /// Invoked entries with a create-applicable registration (every expansion).
    both: usize,
    invoked_not_registered: BTreeMap<InvokedNotRegistered, Vec<String>>,
    registered_not_invoked: BTreeMap<RegisteredNotInvoked, Vec<String>>,
    /// Groups `(model, phase)` with two or more matched entries.
    order_groups: usize,
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
    let mut ranks: BTreeMap<(String, String), Vec<(String, usize)>> = BTreeMap::new();
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
                    ranks
                        .entry((model.clone(), phase.clone()))
                        .or_default()
                        .push((label.clone(), r));
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
        let chains = fired_chains.get(model).cloned().unwrap_or_default();
        for (i, d) in regs.iter().enumerate() {
            if on_create_path(d).is_err() || u.contains(&i) {
                continue;
            }
            let why = if predicted_denied && matches!(d.chain.as_str(), "save" | "create" | "commit") {
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
            out.registered_not_invoked.entry(why).or_default().push(format!(
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

    // Order on the matched subset: recorded order against registration order.
    for ((model, phase), seq) in ranks {
        if seq.len() < 2 {
            continue;
        }
        out.order_groups += 1;
        let inv = seq.windows(2).filter(|w| w[0].1 > w[1].1).count();
        if inv > 0 {
            out.order_differing += 1;
            out.order_inversions += inv;
            let mut expected = seq.clone();
            expected.sort_by_key(|(_, r)| *r);
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
// Layer 3: DO (observable effects)
// ─────────────────────────────────────────────────────────────────────────

/// A user for the existing Rust create path, built from the oracle setup.
struct LegacyUser {
    id: Id,
    held: Vec<String>,
}

impl UserContext for LegacyUser {
    fn id(&self) -> Id {
        self.id
    }
    fn is_admin(&self) -> bool {
        false
    }
    fn is_anonymous(&self) -> bool {
        false
    }
    fn allowed_in_project(&self, permission: &str, _project_id: Id) -> bool {
        self.held.iter().any(|p| p == permission)
    }
    fn allowed_globally(&self, _permission: &str) -> bool {
        false
    }
}

fn default_like<T: Default>(_: &T) -> T {
    T::default()
}

/// What the existing Rust create path did with the submitted attributes.
struct Legacy {
    success: bool,
    error_keys: BTreeSet<String>,
    /// Columns the created entity holds that a blank entity does not.
    assigned: BTreeSet<String>,
    /// The entity got an id, i.e. the Rust path would insert one row.
    inserted: bool,
}

/// Runs `op_services::CreateWorkPackageService` with the attributes the case
/// submits. Values are placeholders (the oracle recorded names, not values).
fn legacy_create(user: &UserKind, submitted: &[String]) -> Option<Legacy> {
    let UserKind::Member(held) = user else {
        return None;
    };
    let mut params = WorkPackageParams::new();
    for k in submitted {
        match k.as_str() {
            "subject" => params.subject = Some("oracle subject".into()),
            "project" => params.project_id = Some(1),
            "type" => params.type_id = Some(1),
            "status" => params.status_id = Some(1),
            "priority" => params.priority_id = Some(1),
            "parent" => params.parent_id = Some(1),
            other => panic!("no Rust parameter for submitted attribute {other}"),
        }
    }
    let u = LegacyUser { id: 2, held: held.clone() };
    let result = CreateWorkPackageService::new(&u).call(params);
    let mut error_keys: BTreeSet<String> = result.errors().errors.keys().cloned().collect();
    if !result.errors().base_errors.is_empty() {
        error_keys.insert("base".into());
    }
    let mut assigned = BTreeSet::new();
    let mut inserted = false;
    if let Some(e) = result.result() {
        let blank = default_like(e);
        macro_rules! differs {
            ($($f:ident),*) => { $( if e.$f != blank.$f { assigned.insert(stringify!($f).to_string()); } )* };
        }
        differs!(
            id, subject, description, project_id, type_id, status_id, priority_id, author_id,
            assigned_to_id, responsible_id, start_date, due_date, estimated_hours, done_ratio,
            parent_id, version_id, category_id
        );
        inserted = e.id.is_some();
    }
    Some(Legacy {
        success: result.is_success(),
        error_keys,
        assigned,
        inserted,
    })
}

/// What Rust expects to observe. `None` where the Rust path cannot say.
#[derive(Debug, Clone)]
struct Predicted {
    persisted: bool,
    changed: BTreeSet<String>,
    deltas: BTreeMap<String, i64>,
}

fn predict(admitted: bool, legacy: Option<&Legacy>) -> Predicted {
    let mut p = Predicted {
        persisted: false,
        changed: BTreeSet::new(),
        deltas: BTreeMap::new(),
    };
    // A denied contract stops the create before any write.
    if !admitted {
        return p;
    }
    if let Some(l) = legacy {
        p.persisted = l.inserted;
        p.changed = l.assigned.clone();
        if l.inserted {
            // The entity is a work package; its table is `work_packages`.
            p.deltas.insert("work_packages".into(), 1);
        }
    }
    p
}

/// An oracle effect the Rust side cannot predict. Closed: no catch-all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum EffectSlag {
    /// A changed attribute the Rust entity does not assign
    /// (timestamps, `position` from acts_as_list).
    AttributeNotModeled(String),
    /// A table row delta the Rust create path does not write.
    EffectNotModeled(String),
    /// A job the Rust create path does not enqueue.
    JobNotModeled(String),
}

#[derive(Debug, Default)]
struct L3 {
    /// Oracle effects: persisted + each changed attribute + each table delta + each job.
    oracle_effects: usize,
    matched: usize,
    /// Oracle effects Rust predicted with another value.
    mismatched: Vec<String>,
    /// Rust predicted an effect the oracle did not observe.
    extra: Vec<String>,
    slag: Vec<EffectSlag>,
}

fn compare_effects(p: &Predicted, sc: &Value) -> L3 {
    let mut out = L3::default();
    let oracle_persisted = sc["persisted"].as_bool().expect("persisted");
    out.oracle_effects += 1;
    if p.persisted == oracle_persisted {
        out.matched += 1;
    } else {
        out.mismatched
            .push(format!("persisted: rust {}, oracle {oracle_persisted}", p.persisted));
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
            out.matched += 1;
        } else {
            out.slag.push(EffectSlag::AttributeNotModeled(a.clone()));
        }
    }
    for a in p.changed.difference(&oracle_changed) {
        out.extra.push(format!("changed_attributes: rust assigns {a}, oracle did not change it"));
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
            Some(m) if m == n => out.matched += 1,
            Some(m) => out
                .mismatched
                .push(format!("row_deltas.{t}: rust {m}, oracle {n}")),
            None => out.slag.push(EffectSlag::EffectNotModeled(t.clone())),
        }
    }
    for (t, n) in &p.deltas {
        if !oracle_deltas.contains_key(t) {
            out.extra.push(format!("row_deltas.{t}: rust writes {n}, oracle wrote none"));
        }
    }

    for j in sc["enqueued_jobs"].as_array().expect("jobs") {
        out.oracle_effects += 1;
        out.slag
            .push(EffectSlag::JobNotModeled(j.as_str().expect("class").to_string()));
    }
    out
}

/// Negative control (c): the predicted names moved one place along a sorted
/// vocabulary, so every predicted effect is attached to the wrong name.
fn shuffled(p: &Predicted, attr_vocab: &BTreeSet<String>, table_vocab: &BTreeSet<String>) -> Predicted {
    fn next(v: &BTreeSet<String>, k: &str) -> String {
        let all: Vec<&String> = v.iter().collect();
        let at = all.iter().position(|x| x.as_str() == k).expect("in vocabulary");
        all[(at + 1) % all.len()].clone()
    }
    Predicted {
        persisted: p.persisted,
        changed: p.changed.iter().map(|a| next(attr_vocab, a)).collect(),
        deltas: p
            .deltas
            .iter()
            .map(|(t, n)| (next(table_vocab, t), *n))
            .collect(),
    }
}

fn tally<K: Ord + Clone>(items: impl Iterator<Item = K>) -> BTreeMap<K, usize> {
    let mut m = BTreeMap::new();
    for i in items {
        *m.entry(i).or_default() += 1;
    }
    m
}
