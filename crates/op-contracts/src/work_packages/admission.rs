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

/// One attribute the create contract declares (canonical name, no `_id`).
#[derive(Debug, Clone, Copy)]
pub struct AttrRule {
    pub name: &'static str,
    /// Permissions of which the user must hold one; `None` means none required.
    pub permissions: Option<&'static [&'static str]>,
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
    let condition: Result<bool, Unresolved> = match rule.writable {
        WritableRule::Always => Ok(true),
        WritableRule::Never => Ok(false),
        WritableRule::Opaque(m) => Err(Unresolved::OpaqueWritable(m)),
        WritableRule::Unlowered(r) => Err(Unresolved::GuardUnlowered(r)),
        WritableRule::Program(ops) => {
            run_program(ops, user, facts).map_err(Unresolved::FactUnavailable)
        }
    };
    let permitted = rule.permissions.is_none_or(|needed| {
        user.is_admin()
            || needed.iter().any(|p| match project {
                Some(id) => user.allowed_in_project(p, id),
                None => user.allowed_in_any_project(p),
            })
    });
    match (condition, permitted) {
        (Ok(false), _) | (_, false) => Writable::No,
        (Ok(true), true) => Writable::Yes,
        (Err(u), true) => Writable::Unresolved(u),
    }
}

/// `CreateContract#user_allowed_to_add`, ported by hand:
/// `(model.project && !allowed_in_project?(perm, project)) || !allowed_in_any_project?(perm)`.
fn user_allowed_to_add<U: UserContext + ?Sized>(user: &U, project: Option<Id>) -> bool {
    let perm = table::ADD_PERMISSION;
    let in_project = match project {
        Some(id) => user.is_admin() || user.allowed_in_project(perm, id),
        None => true,
    };
    in_project && (user.is_admin() || user.allowed_in_any_project(perm))
}

/// Admission of a create that submits `submitted` attributes (names, as the API
/// receives them): the authorization rule plus the read-only check of each.
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
                _ => None,
            }
        }
    }

    const MEMBER: User = User {
        admin: false,
        held: &["add_work_packages"],
    };

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
    fn an_undeclared_attribute_is_unresolved() {
        let w = is_writable(
            &MEMBER,
            Some(5),
            "no_such_attribute",
            &Facts(true, false, false),
        );
        assert_eq!(w, Writable::Unresolved(Unresolved::UndeclaredAttribute));
    }
}
