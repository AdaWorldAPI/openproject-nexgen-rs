//! `WorkPackages::CreateContract` admission: which submitted attributes a user may
//! write, and whether the create is authorized.
//!
//! The per-attribute rules (writable guards and the permission each attribute
//! needs) are DATA generated from the OpenProject contracts by
//! `cargo run -p op-codegen-pipeline --example emit_create_admission`
//! (see [`super::generated_create_admission`]); this file is the evaluator and the
//! one rule that is ported by hand, `CreateContract#user_allowed_to_add`
//! (`app/contracts/work_packages/create_contract.rb`).
//!
//! Semantics mirror `BaseContract#writable?` (`app/contracts/base_contract.rb`): an
//! attribute is writable when its writable condition holds AND the user holds one
//! of its permissions. A known `false` wins over an unknown other half; a fact the
//! caller cannot supply makes the answer [`Writable::Unresolved`], never a guess.

use std::collections::BTreeMap;

use op_core::traits::Id;

use super::generated_create_admission as table;
use crate::base::UserContext;

/// One postfix instruction of a lowered writable guard.
#[derive(Debug, Clone, Copy)]
pub enum Op {
    Const(bool),
    /// A named fact, in the lowering's canonical text (`model.leaf?`).
    Fact(&'static str),
    Not,
    And,
    Or,
}

/// How an attribute's writable condition is decided.
#[derive(Debug, Clone, Copy)]
pub enum WritableRule {
    /// No `writable:` option.
    Always,
    /// `writable: false`.
    Never,
    /// A lowered guard.
    Program(&'static [Op]),
    /// The guard could not be lowered (reason from the extractor).
    Unlowered(&'static str),
    /// Produced by a macro the extractor does not expand.
    Opaque(&'static str),
}

/// Which permission an attribute needs, as ruff's `extract_contract_permissions`
/// resolved it (`BaseContract#reduce_by_writable_permissions`,
/// `app/contracts/base_contract.rb` lines 251-266).
#[derive(Debug, Clone, Copy)]
pub enum PermissionRule {
    /// Nothing in the contract chain declares a permission: none is required
    /// (`next unless permissions`).
    NoneRequired,
    /// The user must hold one of these (`permissions.any?`).
    AnyOf(&'static [&'static str]),
    /// The requirement could not be determined (reason from the extractor). Never
    /// read as "none required": the verdict is unresolved.
    Unresolved(&'static str),
}

/// One attribute the create contract declares (canonical name, no `_id`).
#[derive(Debug, Clone, Copy)]
pub struct AttrRule {
    pub name: &'static str,
    pub permissions: PermissionRule,
    pub writable: WritableRule,
}

/// Facts about the model the guards read, supplied by the caller.
pub trait AdmissionFacts {
    /// `None` when the caller does not know the fact.
    fn fact(&self, name: &str) -> Option<bool>;
}

/// Why a verdict could not be made. Closed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unresolved {
    GuardUnlowered(&'static str),
    FactUnavailable(Vec<&'static str>),
    OpaqueWritable(&'static str),
    /// The permission the attribute needs could not be extracted.
    PermissionUnresolved(&'static str),
    /// The create contract declares no such attribute.
    UndeclaredAttribute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Writable {
    Yes,
    No,
    Unresolved(Unresolved),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Admitted,
    Denied,
    Unresolved(Unresolved),
}

#[derive(Debug, Clone)]
pub struct CreateAdmission {
    pub verdict: Verdict,
    /// Error codes as the service result carries them: attribute (or `base`) -> codes.
    pub errors: BTreeMap<String, Vec<&'static str>>,
    /// Per submitted column.
    pub writable: BTreeMap<String, Writable>,
}

fn canon(attr: &str) -> &str {
    attr.strip_suffix("_id").unwrap_or(attr)
}

fn rule_for(attr: &str) -> Option<&'static AttrRule> {
    let c = canon(attr);
    let c = table::ALIASES
        .iter()
        .find(|(outside, _)| *outside == c)
        .map_or(c, |(_, db)| *db);
    table::ATTRIBUTES.iter().find(|r| r.name == c)
}

/// The column a submitted attribute changes: an association (`project`) changes its
/// foreign key, which the contract declares as written (`attribute :project_id`).
pub fn changed_column(attr: &str) -> String {
    let fk = format!("{attr}_id");
    if table::RAW_ATTRIBUTES.contains(&fk.as_str()) && !table::RAW_ATTRIBUTES.contains(&attr) {
        fk
    } else {
        attr.to_string()
    }
}

/// The facts the writable guard of `attr` reads (empty when it has none).
pub fn guard_facts(attr: &str) -> Vec<&'static str> {
    let Some(AttrRule {
        writable: WritableRule::Program(ops),
        ..
    }) = rule_for(attr)
    else {
        return Vec::new();
    };
    let mut v: Vec<&'static str> = ops
        .iter()
        .filter_map(|o| if let Op::Fact(n) = o { Some(*n) } else { None })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The create contract's rules that are NOT evaluated by [`admit_create`].
pub fn not_evaluated_validators() -> &'static [&'static str] {
    table::NOT_EVALUATED_VALIDATORS
}

type Tri = Result<bool, Vec<&'static str>>;

fn run_program<U: UserContext + ?Sized>(ops: &[Op], user: &U, facts: &dyn AdmissionFacts) -> Tri {
    let mut stack: Vec<Tri> = Vec::new();
    for op in ops {
        match op {
            Op::Const(b) => stack.push(Ok(*b)),
            Op::Fact("user.admin?") => stack.push(Ok(user.is_admin())),
            Op::Fact(name) => stack.push(facts.fact(name).ok_or_else(|| vec![*name])),
            Op::Not => {
                let a = stack.pop().expect("well-formed program");
                stack.push(a.map(|b| !b));
            }
            Op::And | Op::Or => {
                let b = stack.pop().expect("well-formed program");
                let a = stack.pop().expect("well-formed program");
                let decisive = matches!(op, Op::Or);
                stack.push(match (a, b) {
                    (Ok(x), _) | (_, Ok(x)) if x == decisive => Ok(decisive),
                    (Ok(_), Ok(_)) => Ok(!decisive),
                    (x, y) => Err(x.err().into_iter().chain(y.err()).flatten().collect()),
                });
            }
        }
    }
    stack.pop().expect("non-empty program")
}

/// `BaseContract#writable?` for the create contract.
pub fn is_writable<U: UserContext + ?Sized>(
    user: &U,
    project: Option<Id>,
    attribute: &str,
    facts: &dyn AdmissionFacts,
) -> Writable {
    let Some(rule) = rule_for(attribute) else {
        return Writable::Unresolved(Unresolved::UndeclaredAttribute);
    };
    writable_by_rule(rule, user, project, facts)
}

/// `writable?` for one rule: its condition AND its permission. A known `false` on
/// either side wins over an unknown other side.
fn writable_by_rule<U: UserContext + ?Sized>(
    rule: &AttrRule,
    user: &U,
    project: Option<Id>,
    facts: &dyn AdmissionFacts,
) -> Writable {
    let condition: Result<bool, Unresolved> = match rule.writable {
        WritableRule::Always => Ok(true),
        WritableRule::Never => Ok(false),
        WritableRule::Opaque(m) => Err(Unresolved::OpaqueWritable(m)),
        WritableRule::Unlowered(r) => Err(Unresolved::GuardUnlowered(r)),
        WritableRule::Program(ops) => {
            run_program(ops, user, facts).map_err(Unresolved::FactUnavailable)
        }
    };
    let permitted: Result<bool, Unresolved> = match rule.permissions {
        PermissionRule::NoneRequired => Ok(true),
        PermissionRule::AnyOf(needed) => Ok(user.is_admin()
            || needed.iter().any(|p| match project {
                Some(id) => user.allowed_in_project(p, id),
                None => user.allowed_in_any_project(p),
            })),
        PermissionRule::Unresolved(r) => Err(Unresolved::PermissionUnresolved(r)),
    };
    match (condition, permitted) {
        (Ok(false), _) | (_, Ok(false)) => Writable::No,
        (Ok(true), Ok(true)) => Writable::Yes,
        (Err(u), _) | (_, Err(u)) => Writable::Unresolved(u),
    }
}

/// `CreateContract#user_allowed_to_add`, ported by hand:
/// `(model.project && !allowed_in_project?(perm, project)) || !allowed_in_any_project?(perm)`.
///
/// A grant in the model's project is a grant in some project, so it satisfies
/// `allowed_in_any_project?` without asking: the second query only decides the
/// case without a project.
fn user_allowed_to_add<U: UserContext + ?Sized>(user: &U, project: Option<Id>) -> bool {
    let perm = table::ADD_PERMISSION;
    if user.is_admin() {
        return true;
    }
    match project {
        Some(id) => user.allowed_in_project(perm, id),
        None => user.allowed_in_any_project(perm),
    }
}

/// The facts `validate_has_predecessors_or_children` reads.
pub const FACT_SCHEDULE_MANUALLY: &str = "model.schedule_manually?";
pub const FACT_HAS_CHILDREN: &str = "model.children.any?";
pub const FACT_HAS_SCHEDULING_PREDECESSORS: &str =
    "Relation.used_for_scheduling_of(model.parent).any?";

/// The error code of that validator.
pub const CANNOT_BE_AUTOMATICALLY_SCHEDULED: &str = "cannot_be_automatically_scheduled";

/// `CreateContract` (`app/contracts/work_packages/create_contract.rb:46-48, 68-72`):
///
/// ```ruby
/// attribute :schedule_manually do
///   validate_has_predecessors_or_children if model.schedule_automatically?
/// end
/// def validate_has_predecessors_or_children
///   if Relation.used_for_scheduling_of(model.parent).empty? && model.children.empty?
///     errors.add(:schedule_manually, :cannot_be_automatically_scheduled)
///   end
/// end
/// ```
///
/// The block is registered with `validate(attribute, &block)`
/// (`base_contract.rb:89`), so it runs on every validation, whether or not
/// `schedule_manually` was submitted. It reads the PARENT's scheduling predecessors
/// (`used_for_scheduling_of(model.parent)`, `[]` for no parent), not the work
/// package's own. The error fires only when all three terms hold
/// (`schedule_automatically?`, no predecessors, no children); one term known to
/// block it decides the answer, otherwise an unknown term is unresolved.
fn validate_has_predecessors_or_children(
    facts: &dyn AdmissionFacts,
) -> Result<bool, Vec<&'static str>> {
    let manual = facts.fact(FACT_SCHEDULE_MANUALLY);
    let preds = facts.fact(FACT_HAS_SCHEDULING_PREDECESSORS);
    let children = facts.fact(FACT_HAS_CHILDREN);
    if [manual, preds, children].contains(&Some(true)) {
        return Ok(false);
    }
    let unknown: Vec<&'static str> = [
        (manual, FACT_SCHEDULE_MANUALLY),
        (preds, FACT_HAS_SCHEDULING_PREDECESSORS),
        (children, FACT_HAS_CHILDREN),
    ]
    .iter()
    .filter(|(v, _)| v.is_none())
    .map(|(_, n)| *n)
    .collect();
    if unknown.is_empty() {
        Ok(true)
    } else {
        Err(unknown)
    }
}

/// Admission of a create that submits `submitted` attributes (names, as the API
/// receives them): the authorization rule plus the read-only check of each.
/// Also evaluated: `validate_has_predecessors_or_children`, on every create.
/// Other contract rules are listed by [`not_evaluated_validators`].
pub fn admit_create<U: UserContext + ?Sized>(
    user: &U,
    project: Option<Id>,
    submitted: &[&str],
    facts: &dyn AdmissionFacts,
) -> CreateAdmission {
    let mut errors: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    let mut writable = BTreeMap::new();
    let mut unresolved: Option<Unresolved> = None;
    if !user_allowed_to_add(user, project) {
        errors
            .entry("base".into())
            .or_default()
            .push("error_unauthorized");
    }
    match validate_has_predecessors_or_children(facts) {
        Ok(true) => errors
            .entry("schedule_manually".into())
            .or_default()
            .push(CANNOT_BE_AUTOMATICALLY_SCHEDULED),
        Ok(false) => {}
        Err(names) => unresolved = Some(Unresolved::FactUnavailable(names)),
    }
    for attr in submitted {
        let col = changed_column(attr);
        let w = is_writable(user, project, &col, facts);
        match &w {
            Writable::No => errors
                .entry(col.clone())
                .or_default()
                .push("error_readonly"),
            Writable::Unresolved(u) => unresolved = unresolved.take().or(Some(u.clone())),
            Writable::Yes => {}
        }
        writable.insert(col, w);
    }
    let verdict = match unresolved {
        Some(u) => Verdict::Unresolved(u),
        None if errors.is_empty() => Verdict::Admitted,
        None => Verdict::Denied,
    };
    CreateAdmission {
        verdict,
        errors,
        writable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct User {
        admin: bool,
        held: &'static [&'static str],
    }

    impl UserContext for User {
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
            project_id == 5 && self.allowed_globally(permission)
        }
        fn allowed_globally(&self, permission: &str) -> bool {
            self.held.contains(&permission)
        }
    }

    struct Facts(bool, bool, bool);

    impl AdmissionFacts for Facts {
        fn fact(&self, name: &str) -> Option<bool> {
            match name {
                "model.status_id_change" => Some(self.0),
                "model.status.closed?" | "model.status&.closed?" => Some(self.1),
                "model.effective_target_versions.any?(&:closed?)" => Some(self.2),
                FACT_SCHEDULE_MANUALLY => Some(true),
                FACT_HAS_CHILDREN | FACT_HAS_SCHEDULING_PREDECESSORS => Some(false),
                _ => None,
            }
        }
    }

    const MEMBER: User = User {
        admin: false,
        held: &["add_work_packages"],
    };

    /// Holds `add_work_packages` in project 5 only: no global grant, and an
    /// `allowed_in_any_project` that falls back to the global answer.
    struct ProjectOnly;

    impl UserContext for ProjectOnly {
        fn id(&self) -> Id {
            2
        }
        fn is_admin(&self) -> bool {
            false
        }
        fn is_anonymous(&self) -> bool {
            false
        }
        fn allowed_in_project(&self, permission: &str, project_id: Id) -> bool {
            project_id == 5 && permission == "add_work_packages"
        }
        fn allowed_globally(&self, _permission: &str) -> bool {
            false
        }
    }

    #[test]
    fn a_grant_in_the_model_project_admits_without_a_global_grant() {
        assert!(!ProjectOnly.allowed_in_any_project("add_work_packages"));
        assert!(user_allowed_to_add(&ProjectOnly, Some(5)));
        // Another project still refuses, and no project falls back to the
        // any-project query, which this user does not answer yes to.
        assert!(!user_allowed_to_add(&ProjectOnly, Some(6)));
        assert!(!user_allowed_to_add(&ProjectOnly, None));
    }

    #[test]
    fn a_member_with_the_permission_is_admitted() {
        let a = admit_create(
            &MEMBER,
            Some(5),
            &["subject", "project", "status"],
            &Facts(true, false, false),
        );
        assert_eq!(a.verdict, Verdict::Admitted);
        assert!(a.errors.is_empty());
        assert!(a.writable.contains_key("project_id"));
    }

    #[test]
    fn a_user_without_the_permission_is_denied_with_base_and_readonly() {
        let nobody = User {
            admin: false,
            held: &[],
        };
        let a = admit_create(
            &nobody,
            Some(5),
            &["subject", "project"],
            &Facts(true, false, false),
        );
        assert_eq!(a.verdict, Verdict::Denied);
        assert_eq!(a.errors["base"], ["error_unauthorized"]);
        assert_eq!(a.errors["subject"], ["error_readonly"]);
        assert_eq!(a.errors["project_id"], ["error_readonly"]);
    }

    #[test]
    fn the_permission_is_held_for_another_project_only() {
        let a = admit_create(&MEMBER, Some(6), &["subject"], &Facts(true, false, false));
        assert_eq!(a.verdict, Verdict::Denied);
    }

    #[test]
    fn an_unchanged_closed_status_in_a_closed_version_is_read_only() {
        let open = is_writable(&MEMBER, Some(5), "status_id", &Facts(false, false, false));
        let closed = is_writable(&MEMBER, Some(5), "status_id", &Facts(false, true, true));
        assert_eq!((open, closed), (Writable::Yes, Writable::No));
    }

    #[test]
    fn an_unavailable_fact_is_unresolved_not_guessed() {
        struct None_;
        impl AdmissionFacts for None_ {
            fn fact(&self, _: &str) -> Option<bool> {
                None
            }
        }
        let w = is_writable(&MEMBER, Some(5), "status_id", &None_);
        assert!(matches!(
            w,
            Writable::Unresolved(Unresolved::FactUnavailable(_))
        ));
        // A known `false` still wins over an unknown other half.
        let nobody = User {
            admin: false,
            held: &[],
        };
        assert_eq!(
            is_writable(&nobody, Some(5), "status_id", &None_),
            Writable::No
        );
    }

    #[test]
    fn an_unresolved_permission_is_unresolved_not_allowed() {
        let rule = AttrRule {
            name: "synthetic",
            permissions: PermissionRule::Unresolved("const reference: SOME_PERMISSIONS"),
            writable: WritableRule::Always,
        };
        let facts = Facts(true, false, false);
        // Neither a member nor an admin passes a requirement nobody could read.
        for user in [
            MEMBER,
            User {
                admin: true,
                held: &[],
            },
        ] {
            assert_eq!(
                writable_by_rule(&rule, &user, Some(5), &facts),
                Writable::Unresolved(Unresolved::PermissionUnresolved(
                    "const reference: SOME_PERMISSIONS"
                ))
            );
        }
        // A known-false condition still wins.
        let never = AttrRule {
            writable: WritableRule::Never,
            ..rule
        };
        assert_eq!(
            writable_by_rule(&never, &MEMBER, Some(5), &facts),
            Writable::No
        );
        // The same rule with no requirement is writable: the two are distinct.
        let free = AttrRule {
            permissions: PermissionRule::NoneRequired,
            ..rule
        };
        assert_eq!(
            writable_by_rule(&free, &MEMBER, Some(5), &facts),
            Writable::Yes
        );
    }

    #[test]
    fn an_undeclared_attribute_is_unresolved() {
        let w = is_writable(
            &MEMBER,
            Some(5),
            "no_such_attribute",
            &Facts(true, false, false),
        );
        assert_eq!(w, Writable::Unresolved(Unresolved::UndeclaredAttribute));
    }

    /// `schedule_manually`, children, scheduling predecessors of the parent.
    struct Sched(Option<bool>, Option<bool>, Option<bool>);

    impl AdmissionFacts for Sched {
        fn fact(&self, name: &str) -> Option<bool> {
            match name {
                FACT_SCHEDULE_MANUALLY => self.0,
                FACT_HAS_CHILDREN => self.1,
                FACT_HAS_SCHEDULING_PREDECESSORS => self.2,
                _ => None,
            }
        }
    }

    fn schedule_errors(f: &Sched) -> (Verdict, Option<Vec<&'static str>>) {
        let a = admit_create(&MEMBER, Some(5), &[], f);
        let e = a.errors.get("schedule_manually").cloned();
        (a.verdict, e)
    }

    #[test]
    fn automatic_without_predecessors_or_children_is_refused() {
        // create_contract.rb:46-48, 68-72 on the oracle's `leaf_automatic`.
        let (v, e) = schedule_errors(&Sched(Some(false), Some(false), Some(false)));
        assert_eq!(v, Verdict::Denied);
        assert_eq!(e.unwrap(), [CANNOT_BE_AUTOMATICALLY_SCHEDULED]);
    }

    #[test]
    fn manual_scheduling_is_never_refused() {
        // `if model.schedule_automatically?` is false: the validator is not called.
        let (v, e) = schedule_errors(&Sched(Some(true), Some(false), Some(false)));
        assert_eq!((v, e), (Verdict::Admitted, None));
    }

    #[test]
    fn children_or_parent_predecessors_silence_the_validator() {
        for f in [
            Sched(Some(false), Some(true), Some(false)),
            Sched(Some(false), Some(false), Some(true)),
            Sched(Some(false), Some(true), Some(true)),
        ] {
            assert_eq!(schedule_errors(&f), (Verdict::Admitted, None));
        }
    }

    #[test]
    fn a_blocking_term_decides_without_the_unknown_ones() {
        // Ruby never evaluates the rest once one term blocks.
        assert_eq!(
            schedule_errors(&Sched(Some(true), None, None)),
            (Verdict::Admitted, None)
        );
        assert_eq!(
            schedule_errors(&Sched(None, Some(true), None)),
            (Verdict::Admitted, None)
        );
    }

    #[test]
    fn an_unknown_term_with_no_blocker_is_unresolved_not_guessed() {
        let (v, e) = schedule_errors(&Sched(Some(false), Some(false), None));
        assert_eq!(
            v,
            Verdict::Unresolved(Unresolved::FactUnavailable(vec![
                FACT_HAS_SCHEDULING_PREDECESSORS
            ]))
        );
        assert_eq!(e, None);
    }

    #[test]
    fn it_runs_whether_or_not_schedule_manually_is_submitted() {
        let f = Sched(Some(false), Some(false), Some(false));
        for submitted in [&[][..], &["schedule_manually"][..]] {
            let a = admit_create(&MEMBER, Some(5), submitted, &f);
            assert!(a.errors.contains_key("schedule_manually"), "{submitted:?}");
        }
    }
}
