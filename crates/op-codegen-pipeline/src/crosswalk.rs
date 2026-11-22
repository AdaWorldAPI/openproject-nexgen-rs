//! The part of a contract crosswalk the class proposer reads, filled from ruff.
//!
//! zammad-rs's `rails-codegraph` builds a full crosswalk by parsing Ruby. This
//! crate must not parse Ruby: everything here comes from
//! `ruff_ruby_spo::extract_contract_guards_with_report`. The types carry the
//! names and field types the proposer (`class_proposer.rs`, a 1:1 copy) uses,
//! and only those fields:
//!
//! - [`AttributeFact`] and [`GuardNames`]: same fields as zammad-rs.
//! - [`CrosswalkReport`]: `coverage.partial`, `classes` (the proposer's oracle
//!   check maps a trace case's contract to its model through it) and
//!   `attribute_facts`.
//! - [`ClassRow`]: `class` and `model`.
//!
//! # Mapping from ruff's guards
//!
//! One [`ContractGuard`] (an `attribute :x, writable: ...` declaration) is one
//! [`AttributeFact`].
//!
//! - `class`, `name` (= `attribute`), `file`, `line`: copied.
//! - `model`: see [`model_of_class`].
//! - `guard`, for a lowered, non-constant expression: [`GuardNames`] with
//!   `self_calls` empty (ruff inlines receiver-less calls such as
//!   `leaf_or_manually_scheduled?` into the facts they read) and
//!   `receiver_reads` the sorted distinct facts of the expression
//!   ([`GuardExpr::Fact`]), with a leading `model.` removed
//!   (`model.leaf?` -> `leaf?`). Facts rooted elsewhere (`user.admin?`,
//!   `WorkPackage.work_based_mode?`) keep their full text so they cannot be
//!   mistaken for a model read.
//! - `writable_literal`: `Some(b)` for a top-level [`GuardExpr::Const`].
//! - `permissions`, `permission_not_literal`, `block`, `block_reads`: ruff does
//!   not supply them yet. They are left empty and counted as [`Slag`].
//! - An [`GuardOutcome::Unlowered`] guard has no names: `guard` stays `None`
//!   and the gap is counted.

use std::collections::{BTreeMap, BTreeSet};

use ruff_ruby_spo::{ContractGuard, GuardExpr, GuardOutcome, GuardScanReport};
use serde::Serialize;

/// The names an attribute's `writable` lambda calls and reads on the receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardNames {
    pub self_calls: Vec<String>,
    pub receiver_reads: Vec<String>,
}

/// What the crosswalk read of one classified attribute declaration, for the
/// class proposer.
#[derive(Debug, Clone)]
pub struct AttributeFact {
    pub class: String,
    /// The model of the class; `None` for an attribute of a module or of a
    /// class outside any namespace.
    pub model: Option<String>,
    pub name: String,
    pub file: String,
    pub line: u32,
    pub guard: Option<GuardNames>,
    pub permissions: Vec<String>,
    pub permission_not_literal: bool,
    pub writable_literal: Option<bool>,
    pub block: bool,
    pub block_reads: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Coverage {
    pub partial: bool,
    pub why: String,
}

/// A contract class and its model.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ClassRow {
    pub class: String,
    pub model: Option<String>,
}

/// A fact the proposer could use that ruff does not supply yet. Every variant
/// names the ruff extension that would supply it; the counts are the input to
/// the next ruff expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Slag {
    /// A `writable:` value ruff could not lower to facts
    /// ([`GuardOutcome::Unlowered`]). Counted per attribute. Supplied by
    /// lowering more node kinds in `ruff_ruby_spo::guards`.
    UnloweredGuard,
    /// An `attribute :x` call without `writable:`. ruff reports it only as
    /// `GuardScanReport::attribute_calls`, so it has no [`AttributeFact`] and
    /// cannot be a member of any proposal. Supplied by a ruff arm that emits
    /// one record per attribute call, with or without `writable:`.
    AttributeWithoutWritable,
    /// The `permission:` keyword of an attribute call
    /// (`permission: %i[edit_work_packages change_work_package_status]`).
    /// Counted per fact, all of which carry no permission. Supplied by
    /// extending the guard arm to read the keyword (literal symbols, arrays of
    /// them, or a marker for a non-literal value).
    PermissionKwarg,
    /// The `do ... end` block attached to an attribute call
    /// (`attribute :assigned_version, permission: :assign_versions do ... end`)
    /// and the receiver reads inside it. Counted per fact: ruff reports
    /// neither whether a block is attached nor its reads. Supplied by
    /// recording the block and lowering its body like a guard.
    AttachedBlock,
    /// The contract's model comes from [`model_of_class`] (namespace
    /// singularized), not from a `def self.model` the class declares
    /// (`def self.model = Reminder`). Counted per class. Supplied by an arm
    /// that records `def self.model` / `model_class` per contract class.
    ModelInferredFromNamespace,
    /// ruff's `ContractGuard` does not say whether `class` is a class or a
    /// module (modules' attribute calls are not reported as guards at all),
    /// so a module that is named like a class would be taken for a class.
    /// Counted per class. Supplied by adding the declaring kind to
    /// `ContractGuard`.
    ClassKindUnknown,
}

#[derive(Debug, Clone, Default)]
pub struct CrosswalkReport {
    pub coverage: Coverage,
    pub classes: Vec<ClassRow>,
    pub attribute_facts: Vec<AttributeFact>,
    /// Gaps counted per [`Slag`] variant, in variant order.
    pub slag: BTreeMap<Slag, usize>,
}

/// The model of a contract class: the part of its name before the last `::`
/// with its last segment singularized (`WorkPackages::BaseContract` ->
/// `WorkPackage`, `Queries::CreateContract` -> `Query`).
///
/// This is OpenProject's convention for contracts that do not declare
/// `def self.model`, and the only source available without parsing Ruby. It is
/// wrong where a contract does declare another model (`WorkPackageTypes::...`
/// declares `Type`); see [`Slag::ModelInferredFromNamespace`]. A class outside
/// any namespace has no model.
#[must_use]
pub fn model_of_class(class: &str) -> Option<String> {
    let (namespace, _) = class.rsplit_once("::")?;
    let (outer, last) = match namespace.rsplit_once("::") {
        Some((outer, last)) => (Some(outer), last),
        None => (None, namespace),
    };
    let singular = singularize(last);
    Some(match outer {
        Some(o) => format!("{o}::{singular}"),
        None => singular,
    })
}

fn singularize(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies") {
        format!("{stem}y")
    } else if ["uses", "sses", "xes", "ches", "shes"]
        .iter()
        .any(|e| word.ends_with(e))
    {
        word[..word.len() - 2].to_string()
    } else if word.ends_with("ss") {
        word.to_string()
    } else if let Some(stem) = word.strip_suffix('s') {
        stem.to_string()
    } else {
        word.to_string()
    }
}

fn collect_facts<'a>(e: &'a GuardExpr, out: &mut BTreeSet<&'a str>) {
    match e {
        GuardExpr::Const(_) => {}
        GuardExpr::Fact(name) => {
            out.insert(name.as_str());
        }
        GuardExpr::Not(a) => collect_facts(a, out),
        GuardExpr::And(a, b) | GuardExpr::Or(a, b) => {
            collect_facts(a, out);
            collect_facts(b, out);
        }
    }
}

/// The names a lowered expression reads; `None` when it reads none.
fn guard_names(e: &GuardExpr) -> Option<GuardNames> {
    let mut facts = BTreeSet::new();
    collect_facts(e, &mut facts);
    if facts.is_empty() {
        return None;
    }
    let mut reads: Vec<String> = facts
        .into_iter()
        .map(|f| f.strip_prefix("model.").unwrap_or(f).to_string())
        .collect();
    reads.sort();
    reads.dedup();
    Some(GuardNames {
        self_calls: Vec::new(),
        receiver_reads: reads,
    })
}

/// Fills the proposer's input from ruff's lowered contract guards.
///
/// `extra_classes` are contract classes known from elsewhere (for example the
/// contracts named by an oracle trace) that declare no guard of their own and
/// so would not otherwise get a [`ClassRow`]; each gets its model from
/// [`model_of_class`].
#[must_use]
pub fn from_contract_guards(
    guards: &[ContractGuard],
    scan: &GuardScanReport,
    extra_classes: &[&str],
) -> CrosswalkReport {
    let mut facts = Vec::with_capacity(guards.len());
    let mut classes: BTreeSet<&str> = extra_classes.iter().copied().collect();
    let mut slag: BTreeMap<Slag, usize> = BTreeMap::new();
    for g in guards {
        classes.insert(g.class.as_str());
        let (guard, writable_literal) = match &g.outcome {
            GuardOutcome::Lowered(GuardExpr::Const(b)) => (None, Some(*b)),
            GuardOutcome::Lowered(e) => (guard_names(e), None),
            GuardOutcome::Unlowered { .. } => {
                *slag.entry(Slag::UnloweredGuard).or_default() += 1;
                (None, None)
            }
        };
        facts.push(AttributeFact {
            class: g.class.clone(),
            model: model_of_class(&g.class),
            name: g.attribute.clone(),
            file: g.file.clone(),
            line: u32::try_from(g.line).unwrap_or(u32::MAX),
            guard,
            permissions: Vec::new(),
            permission_not_literal: false,
            writable_literal,
            block: false,
            block_reads: Vec::new(),
        });
    }
    let without_writable = scan.attribute_calls.saturating_sub(scan.writable_guards);
    if without_writable > 0 {
        slag.insert(Slag::AttributeWithoutWritable, without_writable);
    }
    if !facts.is_empty() {
        slag.insert(Slag::PermissionKwarg, facts.len());
        slag.insert(Slag::AttachedBlock, facts.len());
    }
    if !classes.is_empty() {
        slag.insert(Slag::ModelInferredFromNamespace, classes.len());
        slag.insert(Slag::ClassKindUnknown, classes.len());
    }
    let classes: Vec<ClassRow> = classes
        .into_iter()
        .map(|c| ClassRow {
            class: c.to_string(),
            model: model_of_class(c),
        })
        .collect();
    CrosswalkReport {
        coverage: Coverage {
            partial: true,
            why: "built from ruff's contract-guard lowering only: attributes without `writable:`, \
                  `permission:` keywords and attached blocks are not supplied (see Slag)"
                .to_string(),
        },
        classes,
        attribute_facts: facts,
        slag,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowered(class: &str, attr: &str, e: GuardExpr) -> ContractGuard {
        ContractGuard {
            class: class.into(),
            attribute: attr.into(),
            file: "f.rb".into(),
            line: 3,
            outcome: GuardOutcome::Lowered(e),
        }
    }

    #[test]
    fn model_is_the_singularized_namespace() {
        assert_eq!(
            model_of_class("WorkPackages::BaseContract").as_deref(),
            Some("WorkPackage")
        );
        assert_eq!(
            model_of_class("Queries::CreateContract").as_deref(),
            Some("Query")
        );
        assert_eq!(
            model_of_class("Admin::Statuses::BaseContract").as_deref(),
            Some("Admin::Status")
        );
        assert_eq!(model_of_class("ModelContract"), None);
    }

    #[test]
    fn facts_become_receiver_reads_and_consts_become_literals() {
        let f = |s: &str| Box::new(GuardExpr::Fact(s.into()));
        let guards = vec![
            lowered(
                "WorkPackages::BaseContract",
                "start_date",
                GuardExpr::Or(f("model.leaf?"), f("model.schedule_manually?")),
            ),
            lowered(
                "WorkPackages::BaseContract",
                "due_date",
                GuardExpr::Or(f("model.schedule_manually?"), f("model.leaf?")),
            ),
            lowered(
                "WorkPackages::BaseContract",
                "subject",
                GuardExpr::Const(true),
            ),
            lowered(
                "WorkPackages::BaseContract",
                "x",
                GuardExpr::Fact("user.admin?".into()),
            ),
            ContractGuard {
                class: "WorkPackages::BaseContract".into(),
                attribute: "y".into(),
                file: "f.rb".into(),
                line: 9,
                outcome: GuardOutcome::Unlowered { reason: "r".into() },
            },
        ];
        let scan = GuardScanReport {
            attribute_calls: 9,
            writable_guards: 5,
            ..GuardScanReport::default()
        };
        let r = from_contract_guards(&guards, &scan, &[]);
        let names = |i: usize| r.attribute_facts[i].guard.clone().map(|g| g.receiver_reads);
        assert_eq!(
            names(0),
            Some(vec!["leaf?".into(), "schedule_manually?".into()])
        );
        assert_eq!(
            names(0),
            names(1),
            "operand order must not change the witness"
        );
        assert_eq!(r.attribute_facts[2].writable_literal, Some(true));
        assert!(r.attribute_facts[2].guard.is_none());
        assert_eq!(names(3), Some(vec!["user.admin?".into()]));
        assert!(r.attribute_facts[4].guard.is_none());
        assert_eq!(r.slag[&Slag::UnloweredGuard], 1);
        assert_eq!(r.slag[&Slag::AttributeWithoutWritable], 4);
        assert!(r.coverage.partial);
    }
}
