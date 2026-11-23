//! Generator for `op_contracts::work_packages::generated_create_admission`: the
//! per-attribute admission table of `WorkPackages::CreateContract`.
//!
//! Writable guards come from ruff's lowering (`extract_contract_guards`). Which
//! permission an attribute needs is NOT lowered by ruff yet; it is read with the
//! line scan below. That scan is the one typed extraction gap
//! ([`ExtractionGap::PermissionDeclScan`]): it feeds only this generator, and it is
//! to be deleted when ruff lowers `permission:` / `default_attribute_permission`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use ruff_ruby_spo::{extract_contract_guards, ContractGuard, GuardExpr, GuardOutcome};

const CREATE: &str = "WorkPackages::CreateContract";
const BASE: &str = "WorkPackages::BaseContract";

/// Facts ruff does not extract and this crate reads by line scan instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionGap {
    /// `attribute :x, permission: ...`, `default_attribute_permission ...`,
    /// `attribute_alias`, `allow_writable_timestamps`, the `validate :x` list, and the
    /// permission literal inside `user_allowed_to_add`.
    PermissionDeclScan,
}

impl ExtractionGap {
    pub fn describe(self) -> &'static str {
        match self {
            ExtractionGap::PermissionDeclScan => {
                "ExtractionGap::PermissionDeclScan: permission:/default_attribute_permission/attribute_alias/allow_writable_timestamps/validate/user_allowed_to_add line scan of base_contract.rb and create_contract.rb (feeds the generator only)"
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct AttrInfo {
    literal_false: bool,
    opaque_writable: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct ContractModel {
    declared: BTreeMap<String, AttrInfo>,
    raw: BTreeSet<String>,
    perms: BTreeMap<String, Vec<String>>,
    default_perm: Option<Vec<String>>,
    alias: BTreeMap<String, String>,
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
                    info.literal_false = stmt[at + 9..].trim_start().starts_with("false");
                }
                if let Some(at) = stmt.find("permission:") {
                    let list = symbol_list(&stmt[at + 11..]);
                    if !list.is_empty() {
                        m.perms.insert(name.clone(), list);
                    }
                }
                m.raw.insert(name.clone());
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

/// What the [`ExtractionGap::PermissionDeclScan`] line scan found in
/// `base_contract.rb` and `create_contract.rb`, for consumers other than the
/// generator (the three-witness reconciliation, [`crate::reconcile`]). It is the same
/// scan the generator runs, not a second reading of the Ruby.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractScanFacts {
    /// The permission literal inside `CreateContract#user_allowed_to_add`.
    pub add_permission: Option<String>,
    /// Every `validate :name` in scan order (base contract first).
    pub validators: Vec<String>,
}

/// Run the scan over the checkout at `src`.
pub fn contract_scan_facts(src: &Path) -> ContractScanFacts {
    let m = contract_model(src);
    ContractScanFacts {
        add_permission: m.add_permission,
        validators: m.validators,
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
    let m = contract_model(src);
    let add = m
        .add_permission
        .clone()
        .expect("user_allowed_to_add names a permission");

    let mut o = String::new();
    let _ = writeln!(
        o,
        "//! @generated by `cargo run -p op-codegen-pipeline --example emit_create_admission`\n//! from OpenProject `app/contracts` (ruff's lowered `writable:` guards). DO NOT EDIT.\n//!\n//! {}\n",
        ExtractionGap::PermissionDeclScan.describe()
    );
    o.push_str("use super::admission::{AttrRule, Op, WritableRule};\n\n");
    let _ = writeln!(o, "pub const ADD_PERMISSION: &str = {add:?};\n");
    let raw: Vec<String> = m.raw.iter().cloned().collect();
    let _ = writeln!(
        o,
        "pub static RAW_ATTRIBUTES: &[&str] = &[{}];\n",
        str_list(&raw)
    );
    let alias: Vec<String> = m
        .alias
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
        let perms = m
            .perms
            .get(name)
            .or_else(|| m.perms.get(&format!("{name}_id")))
            .or(m.default_perm.as_ref());
        let perms = perms.map_or("None".to_string(), |p| format!("Some(&[{}])", str_list(p)));
        let writable = if info.literal_false {
            "WritableRule::Never".to_string()
        } else if let Some(mac) = &info.opaque_writable {
            format!("WritableRule::Opaque({mac:?})")
        } else {
            match guard_for(&guards, name).map(|g| &g.outcome) {
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
        };
        let _ = writeln!(
            o,
            "    AttrRule {{ name: {name:?}, permissions: {perms}, writable: {writable} }},"
        );
    }
    o.push_str("];\n");
    o
}
