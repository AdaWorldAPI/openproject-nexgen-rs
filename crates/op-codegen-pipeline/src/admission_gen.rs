//! Generator for `op_contracts::work_packages::generated_create_admission`: the
//! per-attribute admission table of `WorkPackages::CreateContract`.
//!
//! Writable guards come from ruff's lowering (`extract_contract_guards`). Which
//! permission an attribute needs, the default permission and the attribute aliases
//! come from ruff's `extract_contract_permissions`, resolved the way
//! `BaseContract#reduce_by_writable_permissions` does (`app/contracts/base_contract.rb`
//! lines 251-266). A requirement ruff cannot determine stays typed
//! ([`PermissionFact::Unresolved`]) and becomes a refusal-to-guess at runtime, never
//! "no permission needed".
//!
//! What ruff does not extract is still read with the line scan below, the typed
//! extraction gap [`ExtractionGap::ContractDeclScan`]: the declared attribute names and
//! `writable: false`, `allow_writable_timestamps`, the `validate :x` list and the
//! permission literal inside `user_allowed_to_add`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use ruff_ruby_spo::{
    extract_contract_guards, extract_contract_permissions, ContractGuard, ContractPermissions,
    GuardExpr, GuardOutcome, PermissionState, PermissionValue,
};

const CREATE: &str = "WorkPackages::CreateContract";
const BASE: &str = "WorkPackages::BaseContract";

/// Facts ruff does not extract and this crate reads by line scan instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionGap {
    /// The `attribute :x` names and `writable: false`, `allow_writable_timestamps`,
    /// the `validate :x` list, and the permission literal inside
    /// `user_allowed_to_add`. (`permission:`, `default_attribute_permission` and
    /// `attribute_alias` come from ruff's `extract_contract_permissions`.)
    ContractDeclScan,
}

impl ExtractionGap {
    pub fn describe(self) -> &'static str {
        match self {
            ExtractionGap::ContractDeclScan => {
                "ExtractionGap::ContractDeclScan: attribute/writable: false/allow_writable_timestamps/validate/user_allowed_to_add line scan of base_contract.rb and create_contract.rb; permissions, default permission and aliases come from ruff_ruby_spo::extract_contract_permissions"
            }
        }
    }
}

/// The permission requirement of one attribute, from
/// [`ContractPermissions::lookup`]. Closed: ruff's four outcomes map onto three,
/// and none of them collapses into another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionFact {
    /// `Explicit`, `Inherited` or `Default`: the user must hold one of these
    /// (`permissions.any?`, base_contract.rb:259).
    AnyOf(Vec<String>),
    /// `ProvenAbsent`: nothing in the chain declares a permission, so none is
    /// required (`next unless permissions`, base_contract.rb:257).
    NoneRequired,
    /// `Unresolved`: ruff could not determine the requirement (reason from ruff).
    Unresolved(String),
}

/// What `attribute` requires under `contract`.
pub fn permission_fact(contract: &ContractPermissions, attribute: &str) -> PermissionFact {
    let any_of = |value: &PermissionValue| match value {
        PermissionValue::AnyOf(v) => PermissionFact::AnyOf(v.clone()),
        // `lookup` reports an unlowered value as `Unresolved`; this arm keeps the
        // mapping total without trusting that.
        PermissionValue::Unlowered {
            reason,
            source_text,
        } => PermissionFact::Unresolved(format!("{reason}: {source_text}")),
    };
    match contract.lookup(attribute) {
        PermissionState::Explicit(d) | PermissionState::Inherited(d) => any_of(&d.value),
        PermissionState::Default(d) => any_of(&d.value),
        PermissionState::ProvenAbsent => PermissionFact::NoneRequired,
        PermissionState::Unresolved { reason } => PermissionFact::Unresolved(reason),
    }
}

/// `WorkPackages::CreateContract`'s permission surface. The scan root is `app`, not
/// `app/contracts`: contracts include modules from `app/services`, and a module ruff
/// does not scan is a gap.
fn create_permissions(src: &Path) -> ContractPermissions {
    extract_contract_permissions(&src.join("app"))
        .into_iter()
        .find(|c| c.class == CREATE)
        .unwrap_or_else(|| panic!("ruff found no {CREATE}"))
}

/// The Rust expression for `fact` in the generated table.
fn permission_rule(fact: &PermissionFact) -> String {
    match fact {
        PermissionFact::AnyOf(v) => format!("PermissionRule::AnyOf(&[{}])", str_list(v)),
        PermissionFact::NoneRequired => "PermissionRule::NoneRequired".into(),
        PermissionFact::Unresolved(reason) => format!("PermissionRule::Unresolved({reason:?})"),
    }
}

#[derive(Debug, Clone, Default)]
struct AttrInfo {
    literal_false: bool,
    opaque_writable: Option<String>,
    /// The (last) declaration carries a `writable:` keyword. `add_writable` stores
    /// `options[:writable]` for every `attribute` call (base_contract.rb:104-113), so a
    /// declaration without the keyword stores `nil`, and `collect_ancestor_attributes`
    /// (`reverse_merge!`, subclass first, base_contract.rb:182-190) lets that `nil` win
    /// over an ancestor's condition.
    writable_keyword: bool,
}

#[derive(Debug, Clone, Default)]
struct ContractModel {
    declared: BTreeMap<String, AttrInfo>,
    raw: BTreeSet<String>,
    validators: Vec<String>,
    add_permission: Option<String>,
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
    let mut in_add = false;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with("attribute ") && t.contains(':') {
            let mut stmt = t.to_string();
            while stmt.trim_end().ends_with(',') && i + 1 < lines.len() {
                i += 1;
                // A comment line inside the statement (`# Overriding permission ...`)
                // neither ends it nor belongs to it.
                if lines[i].trim_start().starts_with('#') {
                    continue;
                }
                stmt.push(' ');
                stmt.push_str(lines[i].trim());
            }
            if let Some(name) = ident_after(&stmt, "attribute :") {
                let mut info = AttrInfo::default();
                if let Some(at) = stmt.find("writable:") {
                    info.writable_keyword = true;
                    info.literal_false = stmt[at + 9..].trim_start().starts_with("false");
                }
                m.raw.insert(name.clone());
                m.declared.insert(canon(&name), info);
            }
        } else if let Some(rest) = t.strip_prefix("allow_writable_timestamps ") {
            for name in symbol_list(rest) {
                m.declared.insert(
                    canon(&name),
                    AttrInfo {
                        literal_false: false,
                        opaque_writable: Some(
                            "AdminWritableTimestamps.allow_writable_timestamps".into(),
                        ),
                        writable_keyword: true,
                    },
                );
            }
        } else if let Some(name) = ident_after(t, "validate :") {
            m.validators.push(name);
        } else if t.starts_with("def user_allowed_to_add") {
            in_add = true;
        } else if in_add && t.contains("allowed_in_project?(:") && m.add_permission.is_none() {
            let at =
                t.find("allowed_in_project?(:").expect("checked") + "allowed_in_project?(".len();
            m.add_permission = ident_after(&t[at..], ":");
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

/// What the [`ExtractionGap::ContractDeclScan`] line scan found in
/// `base_contract.rb` and `create_contract.rb`, for consumers other than the
/// generator (the three-witness reconciliation, [`crate::reconcile`]). It is the same
/// scan the generator runs, not a second reading of the Ruby.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractScanFacts {
    /// The permission literal inside `CreateContract#user_allowed_to_add`.
    pub add_permission: Option<String>,
    /// Every `validate :name` in scan order (base contract first).
    pub validators: Vec<String>,
    /// Canonical names of attributes whose last declaration in the chain has no
    /// `writable:` keyword. Such a declaration shadows any ancestor `writable:` condition
    /// (`reduce_by_writable_conditions` removes only on `== false`, base_contract.rb:238-245).
    pub writable_shadowed: BTreeSet<String>,
}

/// Run the scan over the checkout at `src`.
pub fn contract_scan_facts(src: &Path) -> ContractScanFacts {
    let m = contract_model(src);
    ContractScanFacts {
        add_permission: m.add_permission,
        validators: m.validators,
        writable_shadowed: m
            .declared
            .iter()
            .filter(|(_, i)| !i.writable_keyword)
            .map(|(n, _)| n.clone())
            .collect(),
    }
}

fn guard_for<'a>(guards: &'a [ContractGuard], canon_attr: &str) -> Option<&'a ContractGuard> {
    let find = |class: &str| {
        guards
            .iter()
            .find(|g| g.class == class && canon(&g.attribute) == canon_attr)
    };
    find(CREATE).or_else(|| find(BASE))
}

/// The rule from ruff's guards. A declaration without `writable:` shadows the ancestors'
/// condition, so it needs no guard lookup at all (see [`AttrInfo::writable_keyword`]).
fn guarded_rule(guards: &[ContractGuard], canon_attr: &str, info: &AttrInfo) -> String {
    if !info.writable_keyword {
        return "WritableRule::Always".into();
    }
    match guard_for(guards, canon_attr).map(|g| &g.outcome) {
        None => "WritableRule::Always".into(),
        Some(GuardOutcome::Lowered(e)) => {
            let mut ops = Vec::new();
            postfix(e, &mut ops);
            format!("WritableRule::Program(&[{}])", ops.join(", "))
        }
        Some(GuardOutcome::Unlowered { reason }) => {
            format!("WritableRule::Unlowered({reason:?})")
        }
    }
}

fn postfix(e: &GuardExpr, out: &mut Vec<String>) {
    match e {
        GuardExpr::Const(b) => out.push(format!("Op::Const({b})")),
        GuardExpr::Fact(n) => out.push(format!("Op::Fact({n:?})")),
        GuardExpr::Not(a) => {
            postfix(a, out);
            out.push("Op::Not".into());
        }
        GuardExpr::And(a, b) => {
            postfix(a, out);
            postfix(b, out);
            out.push("Op::And".into());
        }
        GuardExpr::Or(a, b) => {
            postfix(a, out);
            postfix(b, out);
            out.push("Op::Or".into());
        }
    }
}

fn str_list(v: &[String]) -> String {
    v.iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The generated Rust source for the checkout at `src`.
pub fn generate(src: &Path) -> String {
    let guards = extract_contract_guards(&src.join("app/contracts"));
    let permissions = create_permissions(src);
    let m = contract_model(src);
    let add = m
        .add_permission
        .clone()
        .expect("user_allowed_to_add names a permission");

    let mut o = String::new();
    let _ = writeln!(
        o,
        "//! @generated by `cargo run -p op-codegen-pipeline --example emit_create_admission`\n//! from OpenProject `app` (ruff's lowered `writable:` guards and contract permissions). DO NOT EDIT.\n//!\n//! {}\n",
        ExtractionGap::ContractDeclScan.describe()
    );
    o.push_str("use super::admission::{AttrRule, Op, PermissionRule, WritableRule};\n\n");
    let _ = writeln!(o, "pub const ADD_PERMISSION: &str = {add:?};\n");
    let raw: Vec<String> = m.raw.iter().cloned().collect();
    let _ = writeln!(
        o,
        "pub static RAW_ATTRIBUTES: &[&str] = &[{}];\n",
        str_list(&raw)
    );
    // `attribute_alias db, outside` (base_contract.rb:65-69), keyed outside -> db by
    // canonical name for the runtime's `rule_for`.
    let alias: BTreeMap<String, String> = permissions
        .aliases
        .values()
        .map(|a| (canon(&a.outside), canon(&a.db)))
        .collect();
    let alias: Vec<String> = alias
        .iter()
        .map(|(a, b)| format!("({a:?}, {b:?})"))
        .collect();
    let _ = writeln!(
        o,
        "pub static ALIASES: &[(&str, &str)] = &[{}];\n",
        alias.join(", ")
    );
    let validators: Vec<String> = m
        .validators
        .iter()
        .filter(|v| **v != "user_allowed_to_add")
        .cloned()
        .collect();
    let _ = writeln!(
        o,
        "pub static NOT_EVALUATED_VALIDATORS: &[&str] = &[{}];\n",
        str_list(&validators)
    );
    o.push_str("pub static ATTRIBUTES: &[AttrRule] = &[\n");
    for (name, info) in &m.declared {
        let perms = permission_rule(&permission_fact(&permissions, name));
        let writable = if info.literal_false {
            "WritableRule::Never".to_string()
        } else if let Some(mac) = &info.opaque_writable {
            format!("WritableRule::Opaque({mac:?})")
        } else {
            guarded_rule(&guards, name, info)
        };
        let _ = writeln!(
            o,
            "    AttrRule {{ name: {name:?}, permissions: {perms}, writable: {writable} }},"
        );
    }
    o.push_str("];\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic contract chain: `C` declares one readable and one unreadable
    /// permission, `D < C` adds a default.
    ///
    /// `test` names the directory: tests run in parallel threads of one process,
    /// so a shared directory would be deleted under another test's extraction.
    fn synthetic(test: &str) -> Vec<ContractPermissions> {
        let root = std::env::temp_dir().join(format!(
            "op_admission_gen_perms_{}_{test}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir");
        std::fs::write(
            root.join("c.rb"),
            "class C < BaseContract\n  attribute :a, permission: SOME_PERMISSIONS\n  attribute :b, permission: %i[p r]\nend\n\nclass D < C\n  default_attribute_permission :q\nend\n",
        )
        .expect("write");
        let all = extract_contract_permissions(&root);
        let _ = std::fs::remove_dir_all(&root);
        all
    }

    fn class<'a>(all: &'a [ContractPermissions], name: &str) -> &'a ContractPermissions {
        all.iter().find(|c| c.class == name).expect(name)
    }

    fn guard(class: &str, attribute: &str, fact: &str) -> ContractGuard {
        ContractGuard {
            class: class.into(),
            attribute: attribute.into(),
            file: "x.rb".into(),
            line: 1,
            outcome: GuardOutcome::Lowered(GuardExpr::Fact(fact.into())),
        }
    }

    /// Scan `base` then `sub` (the generator's order) and render attribute `a`.
    fn rendered(base: &str, sub: &str, guards: &[ContractGuard]) -> String {
        let mut m = ContractModel::default();
        scan_contract_file(base, &mut m);
        scan_contract_file(sub, &mut m);
        guarded_rule(guards, "a", &m.declared["a"])
    }

    const BASE_RB: &str = "attribute :a, writable: ->(*) { x? }\n";

    #[test]
    fn a_subclass_declaration_without_writable_shadows_the_base_condition() {
        let guards = [guard(BASE, "a", "model.x?")];
        // Anti-vacuity: without the subclass redeclaration the base condition applies.
        assert_eq!(
            rendered(BASE_RB, "", &guards),
            "WritableRule::Program(&[Op::Fact(\"model.x?\")])"
        );
        assert_eq!(
            rendered(BASE_RB, "attribute :a, permission: :p\n", &guards),
            "WritableRule::Always"
        );
        // `_id` spelling and the block form shadow as well.
        let mut m = ContractModel::default();
        scan_contract_file("attribute :a_id, writable: -> { x? }\n", &mut m);
        scan_contract_file("attribute :a_id do\n  validate_y\nend\n", &mut m);
        assert_eq!(
            guarded_rule(&guards, "a", &m.declared["a"]),
            "WritableRule::Always"
        );
    }

    #[test]
    fn a_subclass_with_its_own_writable_uses_its_own_condition() {
        let guards = [guard(BASE, "a", "model.x?"), guard(CREATE, "a", "model.y?")];
        assert_eq!(
            rendered(BASE_RB, "attribute :a, writable: -> { y? }\n", &guards),
            "WritableRule::Program(&[Op::Fact(\"model.y?\")])"
        );
    }

    #[test]
    fn a_subclass_that_does_not_mention_the_attribute_inherits_the_base_condition() {
        let guards = [guard(BASE, "a", "model.x?")];
        assert_eq!(
            rendered(BASE_RB, "attribute :b, permission: :p\n", &guards),
            "WritableRule::Program(&[Op::Fact(\"model.x?\")])"
        );
    }

    #[test]
    fn scan_facts_list_the_shadowed_attributes() {
        let mut m = ContractModel::default();
        scan_contract_file(BASE_RB, &mut m);
        scan_contract_file(
            "attribute :a, permission: :p\nattribute :c, writable: -> { z? }\n",
            &mut m,
        );
        let shadowed: Vec<_> = m
            .declared
            .iter()
            .filter(|(_, i)| !i.writable_keyword)
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(shadowed, ["a"]);
    }

    #[test]
    fn each_lookup_outcome_maps_to_its_own_fact() {
        let all = synthetic("each_lookup");
        let (c, d) = (class(&all, "C"), class(&all, "D"));
        let pr = vec!["p".to_string(), "r".to_string()];
        // Explicit.
        assert!(matches!(c.lookup("b"), PermissionState::Explicit(_)));
        assert_eq!(permission_fact(c, "b"), PermissionFact::AnyOf(pr.clone()));
        // Inherited, and an `_id` spelling finds the same entry.
        assert!(matches!(d.lookup("b"), PermissionState::Inherited(_)));
        assert_eq!(permission_fact(d, "b_id"), PermissionFact::AnyOf(pr));
        // Default.
        assert_eq!(
            permission_fact(d, "z"),
            PermissionFact::AnyOf(vec!["q".into()])
        );
        // Proven absent.
        assert_eq!(permission_fact(c, "z"), PermissionFact::NoneRequired);
    }

    #[test]
    fn an_unreadable_permission_is_unresolved_never_no_restriction() {
        let all = synthetic("unreadable");
        let fact = permission_fact(class(&all, "C"), "a");
        let PermissionFact::Unresolved(reason) = &fact else {
            panic!("expected Unresolved, got {fact:?}");
        };
        assert!(reason.contains("SOME_PERMISSIONS"), "{reason}");
        let rule = permission_rule(&fact);
        assert!(rule.starts_with("PermissionRule::Unresolved("), "{rule}");
        assert_ne!(rule, "PermissionRule::NoneRequired");
    }
}
