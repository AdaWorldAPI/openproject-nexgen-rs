//! Declarative Rails rewrite rules, applied by the compiler at generation
//! time.
//!
//! Each rule has a stable id and a one-line statement in a MeTTa-like
//! notation. The compiler records the id on every fact it derives, so a
//! generated callback or relation names the rule that put it there. The rules
//! describe ActiveRecord / ActiveSupport 8.1 behaviour and are checked against
//! that version by the behavioural oracle; none names an application.

use crate::callbacks::{Action, Phase};

/// The framework flags `config.load_defaults <version>` turns on that change
/// observable model behaviour (R-LOAD-DEFAULTS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameworkDefaults {
    pub belongs_to_required_by_default: bool,
    pub belongs_to_required_validates_foreign_key: bool,
    pub run_after_transaction_callbacks_in_order_defined: bool,
}

impl FrameworkDefaults {
    /// `load_defaults "major.minor"`; `None` is a Rails app without
    /// `load_defaults` (the framework's own defaults).
    #[must_use]
    pub fn for_version(v: Option<&str>) -> Self {
        let (maj, min) = v
            .and_then(|v| v.split_once('.'))
            .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))
            .unwrap_or((0, 0));
        let at_least = |a: u32, b: u32| (maj, min) >= (a, b);
        Self {
            belongs_to_required_by_default: at_least(5, 0),
            belongs_to_required_validates_foreign_key: !at_least(7, 1),
            run_after_transaction_callbacks_in_order_defined: at_least(7, 1),
        }
    }
}

/// A rule's identity and statement.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub id: &'static str,
    pub statement: &'static str,
    /// Where in Rails the behaviour lives, for review against the source.
    pub source: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        id: "R-CONCERN-EXPAND",
        statement: "(include M) ∧ (concern M (included B)) ∧ ¬(already-included M) ⇒ (splice B at include site)",
        source: "activesupport/lib/active_support/concern.rb append_features",
    },
    Rule {
        id: "R-INHERIT-CHAIN",
        statement: "(class C < P) ⇒ (chain C k) = (chain P k) ++ (own-decls C k)",
        source: "activesupport/lib/active_support/callbacks.rb __callbacks class_attribute",
    },
    Rule {
        id: "R-CHAIN-DEDUP",
        statement: "(push chain d) ∧ (∃ e ∈ chain: same-slot e d ∧ same-symbol-filter e d) ⇒ (remove e) before (push d)",
        source: "activesupport/lib/active_support/callbacks.rb CallbackChain#remove_duplicates",
    },
    Rule {
        id: "R-CHAIN-PREPEND",
        statement: "(prepend d) ⇒ (unshift chain d) else (push chain d)",
        source: "activesupport/lib/active_support/callbacks.rb append_one/prepend_one",
    },
    Rule {
        id: "R-AFTER-MODEL-PREPEND",
        statement: "(after_<event> d) ⇒ (prepend d); execution of :after slots = reverse chain order ⇒ declaration order",
        source: "activemodel/lib/active_model/callbacks.rb _define_after_model_callback",
    },
    Rule {
        id: "R-AFTER-COMMIT-ORDER",
        statement: "(after_commit d) ∧ run_after_transaction_callbacks_in_order_defined ⇒ (prepend d) ≡ ¬(user-prepend d)",
        source: "activerecord/lib/active_record/transactions.rb enforced_prepend_option",
    },
    Rule {
        id: "R-COMMIT-SHORTHAND",
        statement: "(after_<a>_commit d) ⇒ (after_commit d (on <a>)); after_save_commit ⇒ (on create update)",
        source: "activerecord/lib/active_record/transactions.rb after_create_commit et al.",
    },
    Rule {
        id: "R-PHASE-ORDER",
        statement: "save ⇒ before_validation → validate → after_validation → before_save∥around_save → before_<a>∥around_<a> → timestamps → write → after_<a> → after_save ⇒ commit → after_commit",
        source: "activerecord/lib/active_record/{transactions,validations,callbacks,timestamp}.rb",
    },
    Rule {
        id: "R-AUTOSAVE-BELONGS-TO",
        statement: "(belongs_to C n) ⇒ (before_save autosave_associated_records_for_n)",
        source: "activerecord/lib/active_record/autosave_association.rb add_autosave_association_callbacks",
    },
    Rule {
        id: "R-AUTOSAVE-COLLECTION",
        statement: "(has_many|habtm C n) ⇒ (around_save around_save_collection_association) ∧ (after_create autosave_n) ∧ (after_update autosave_n) ∧ (validate validate_associated_records_for_n)",
        source: "activerecord/lib/active_record/autosave_association.rb add_autosave_association_callbacks",
    },
    Rule {
        id: "R-AUTOSAVE-HAS-ONE",
        statement: "(has_one C n) ⇒ (after_create autosave_n) ∧ (after_update autosave_n) ∧ ((autosave n) ⇒ (validate validate_associated_records_for_n))",
        source: "activerecord/lib/active_record/autosave_association.rb add_autosave_association_callbacks",
    },
    Rule {
        id: "R-BELONGS-TO-REQUIRED",
        statement: "(belongs_to C n) ∧ ¬(optional n) ∧ belongs_to_required_by_default ⇒ (validates n presence message: required (if ¬validates_fk ⇒ fk nil ∨ fk changed))",
        source: "activerecord/lib/active_record/associations/builder/belongs_to.rb define_validations",
    },
    Rule {
        id: "R-LOAD-DEFAULTS",
        statement: "(config.load_defaults v) ⇒ framework flags of every version ≤ v (belongs_to_required_by_default ≥5.0, belongs_to_required_validates_foreign_key=false ≥7.1, run_after_transaction_callbacks_in_order_defined ≥7.1)",
        source: "railties/lib/rails/application/configuration.rb load_defaults",
    },
    Rule {
        id: "R-BASE-CHAIN",
        statement: "(class C < ActiveRecord::Base) ⇒ (before_validation normalize_changed_in_place_attributes) ∧ (after_validation _ensure_no_duplicate_errors)",
        source: "activemodel/lib/active_model/{attributes/normalization,validations}.rb",
    },
    Rule {
        id: "R-ON-FILTER",
        statement: "(d (on A)) ⇒ (fires d) only for actions ∈ A; on: is honoured on validation and transaction chains",
        source: "activemodel/lib/active_model/validations/callbacks.rb, activerecord transactions.rb",
    },
    Rule {
        id: "R-REL-IS-A-FRAMEWORK",
        statement: "(class C < P) ∧ (abstract P ∨ framework-root P) ⇒ (C is_a P) [framework], ¬(domain-taxonomy C P)",
        source: "Ruby class inheritance; ActiveRecord abstract_class",
    },
    Rule {
        id: "R-REL-IS-A-STI",
        statement: "(class C < P) ∧ (concrete-model P) ∧ (column (table P) type) ⇒ (C is_a P) [domain, single-table]",
        source: "activerecord/lib/active_record/inheritance.rb",
    },
    Rule {
        id: "R-REL-PART-OF",
        statement: "(P has_many|has_one n → C (fk f) (dependent destroy|delete_all)) ∧ (C belongs_to m → P (fk f)) ∧ (m = underscore P) ∧ (required f) ⇒ (C part_of P)",
        source: "containment = existence dependence on the whole (cascade) + mandatory whole (NOT NULL or presence)",
    },
    Rule {
        id: "R-REL-LIFECYCLE",
        statement: "(P has_many|has_one n → C (dependent d)) ⇒ (lifecycle-obligation P d C); by itself ¬(C part_of P)",
        source: "dependent: is a deletion obligation, not a structural claim",
    },
    Rule {
        id: "R-REL-ROLE",
        statement: "(belongs_to n (class_name C)) ∧ (n ≠ underscore C) ⇒ (role n C), even under a cascading inverse (then: lifecycle obligation, ¬part_of)",
        source: "the association name names the role the target plays",
    },
    Rule {
        id: "R-REL-REFERENCE",
        statement: "(belongs_to n C) ∧ (n = underscore C) ∧ ¬(part_of) ⇒ (references C)",
        source: "a plain foreign key carries no containment",
    },
    Rule {
        id: "R-REL-NAMESPACE",
        statement: "(constant P::C) ⇒ (namespaced_under C P), ¬(part_of C P)",
        source: "Ruby constant nesting is naming, not ownership",
    },
    Rule {
        id: "R-REL-MIXIN",
        statement: "(include M) ⇒ (has_behaviour C M), ¬(C is_a M)",
        source: "a concern contributes behaviour, not taxonomy",
    },
    Rule {
        id: "R-FOLD-SCHEMA",
        statement: "(self.class.column_names) ∧ (has_attribute? :c) ⇒ compile-time constant from the table",
        source: "activerecord/lib/active_record/attribute_methods.rb",
    },
    Rule {
        id: "R-TIMESTAMPS",
        statement: "create ⇒ created_at,updated_at := now unless set; update ∧ changes ∧ ¬changed(updated_at) ⇒ updated_at := now",
        source: "activerecord/lib/active_record/timestamp.rb",
    },
];

#[must_use]
pub fn rule(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

/// The ActiveSupport callback chains of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Chain {
    Initialize,
    Find,
    Validation,
    Validate,
    Save,
    Create,
    Update,
    Destroy,
    Commit,
    Rollback,
    Touch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    Before,
    Around,
    After,
}

/// What a callback calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// A method symbol. The only filter kind that can be a duplicate.
    Method(String),
    /// A callback object or class (`after_create TransactionDispatcher`).
    Object(String),
    /// A block or lambda; `id` is unique per declaration site.
    Lambda { id: String, src: String },
    /// A validator object installed by `validates` / `validates_with`.
    Validator(ValidatorSpec),
}

impl Filter {
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Method(m) | Self::Object(m) => m.clone(),
            Self::Lambda { id, .. } => id.clone(),
            Self::Validator(v) => v.name(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorSpec {
    Presence(String),
    Length { attr: String, min: Option<usize>, max: Option<usize> },
    /// R-BELONGS-TO-REQUIRED: presence of the associated record.
    BelongsToRequired { assoc: String, only_if_fk_missing_or_changed: bool },
    /// `validates_with K` or a validator kind the slice does not lower.
    Other { kind: String, attr: String },
}

impl ValidatorSpec {
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Presence(a) => format!("validates:presence:{a}"),
            Self::Length { attr, .. } => format!("validates:length:{attr}"),
            Self::BelongsToRequired { assoc, .. } => format!("validates:presence:{assoc}"),
            Self::Other { kind, attr } if attr.is_empty() => format!("validates_with:{kind}"),
            Self::Other { kind, attr } => format!("validates:{kind}:{attr}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CondSpec {
    If(Filter),
    Unless(Filter),
}

/// One callback declaration after macro resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    pub chain: Chain,
    pub slot: Slot,
    pub filter: Filter,
    /// `on:` actions; `None` means every action.
    pub on: Option<Vec<Action>>,
    pub conds: Vec<CondSpec>,
    pub prepend: bool,
    pub provenance: String,
    /// The rule that produced this declaration (`R-AUTOSAVE-…`), or
    /// `"source"` for a callback macro written in the application.
    pub rule: &'static str,
}

/// Resolve a callback macro name to its chain and slot, and the `on:` and
/// prepend options Rails enforces for it. `None` for anything that is not a
/// callback macro.
#[must_use]
pub fn resolve_macro(name: &str) -> Option<(Chain, Slot, Option<Vec<Action>>)> {
    let (chain, slot) = match name {
        "after_initialize" => (Chain::Initialize, Slot::After),
        "after_find" => (Chain::Find, Slot::After),
        "before_validation" => (Chain::Validation, Slot::Before),
        "after_validation" => (Chain::Validation, Slot::After),
        "validate" => (Chain::Validate, Slot::Before),
        "before_save" => (Chain::Save, Slot::Before),
        "around_save" => (Chain::Save, Slot::Around),
        "after_save" => (Chain::Save, Slot::After),
        "before_create" => (Chain::Create, Slot::Before),
        "around_create" => (Chain::Create, Slot::Around),
        "after_create" => (Chain::Create, Slot::After),
        "before_update" => (Chain::Update, Slot::Before),
        "around_update" => (Chain::Update, Slot::Around),
        "after_update" => (Chain::Update, Slot::After),
        "before_destroy" => (Chain::Destroy, Slot::Before),
        "around_destroy" => (Chain::Destroy, Slot::Around),
        "after_destroy" => (Chain::Destroy, Slot::After),
        "after_touch" => (Chain::Touch, Slot::After),
        "after_commit" => (Chain::Commit, Slot::After),
        "after_rollback" => (Chain::Rollback, Slot::After),
        "after_create_commit" => return Some((Chain::Commit, Slot::After, Some(vec![Action::Create]))),
        "after_update_commit" => return Some((Chain::Commit, Slot::After, Some(vec![Action::Update]))),
        "after_destroy_commit" => return Some((Chain::Commit, Slot::After, Some(vec![Action::Destroy]))),
        "after_save_commit" => {
            return Some((Chain::Commit, Slot::After, Some(vec![Action::Create, Action::Update])));
        }
        _ => return None,
    };
    Some((chain, slot, None))
}

/// The prepend flag Rails actually applies (R-AFTER-MODEL-PREPEND,
/// R-AFTER-COMMIT-ORDER).
#[must_use]
pub fn effective_prepend(chain: Chain, slot: Slot, user_prepend: bool) -> bool {
    effective_prepend_with(FrameworkDefaults::for_version(Some("8.1")), chain, slot, user_prepend)
}

#[must_use]
pub fn effective_prepend_with(fw: FrameworkDefaults, chain: Chain, slot: Slot, user_prepend: bool) -> bool {
    match (chain, slot) {
        (Chain::Commit | Chain::Rollback, Slot::After) if fw.run_after_transaction_callbacks_in_order_defined => !user_prepend,
        (Chain::Commit | Chain::Rollback, Slot::After) => true,
        (_, Slot::After) => true,
        _ => user_prepend,
    }
}

/// The association macros' framework callbacks (R-AUTOSAVE-*,
/// R-BELONGS-TO-REQUIRED), in the order Rails registers them.
#[must_use]
pub fn association_decls(
    fw: FrameworkDefaults,
    kind: &str,
    name: &str,
    optional: Option<bool>,
    autosave: bool,
    validate_opt: Option<bool>,
    provenance: &str,
) -> Vec<Decl> {
    let save_method = format!("autosave_associated_records_for_{name}");
    let validate_method = format!("validate_associated_records_for_{name}");
    let decl = |chain, slot, filter: Filter, rule: &'static str| Decl {
        chain,
        slot,
        filter,
        on: None,
        conds: Vec::new(),
        prepend: effective_prepend(chain, slot, false),
        provenance: provenance.to_string(),
        rule,
    };
    let mut out = Vec::new();
    match kind {
        "has_many" | "has_and_belongs_to_many" => {
            out.push(decl(
                Chain::Save,
                Slot::Around,
                Filter::Method("around_save_collection_association".into()),
                "R-AUTOSAVE-COLLECTION",
            ));
            out.push(decl(Chain::Create, Slot::After, Filter::Method(save_method.clone()), "R-AUTOSAVE-COLLECTION"));
            out.push(decl(Chain::Update, Slot::After, Filter::Method(save_method), "R-AUTOSAVE-COLLECTION"));
            if validate_opt.unwrap_or(true) {
                out.push(decl(Chain::Validate, Slot::Before, Filter::Method(validate_method), "R-AUTOSAVE-COLLECTION"));
            }
        }
        "has_one" => {
            out.push(decl(Chain::Create, Slot::After, Filter::Method(save_method.clone()), "R-AUTOSAVE-HAS-ONE"));
            out.push(decl(Chain::Update, Slot::After, Filter::Method(save_method), "R-AUTOSAVE-HAS-ONE"));
            if validate_opt.unwrap_or(autosave) {
                out.push(decl(Chain::Validate, Slot::Before, Filter::Method(validate_method), "R-AUTOSAVE-HAS-ONE"));
            }
        }
        "belongs_to" => {
            out.push(decl(Chain::Save, Slot::Before, Filter::Method(save_method), "R-AUTOSAVE-BELONGS-TO"));
            if validate_opt.unwrap_or(autosave) {
                out.push(decl(Chain::Validate, Slot::Before, Filter::Method(validate_method), "R-AUTOSAVE-BELONGS-TO"));
            }
            let required = optional.map_or(fw.belongs_to_required_by_default, |o| !o);
            if required {
                out.push(decl(
                    Chain::Validate,
                    Slot::Before,
                    Filter::Validator(ValidatorSpec::BelongsToRequired {
                        assoc: name.to_string(),
                        only_if_fk_missing_or_changed: !fw.belongs_to_required_validates_foreign_key,
                    }),
                    "R-BELONGS-TO-REQUIRED",
                ));
            }
        }
        _ => {}
    }
    out
}

/// The callbacks every ActiveRecord::Base subclass starts with (R-BASE-CHAIN).
#[must_use]
pub fn base_decls() -> Vec<Decl> {
    let d = |chain, slot, m: &str| Decl {
        chain,
        slot,
        filter: Filter::Method(m.to_string()),
        on: None,
        conds: Vec::new(),
        prepend: effective_prepend(chain, slot, false),
        provenance: "activerecord-8.1.4.1".into(),
        rule: "R-BASE-CHAIN",
    };
    vec![
        d(Chain::Validation, Slot::After, "_ensure_no_duplicate_errors"),
        d(Chain::Validation, Slot::Before, "normalize_changed_in_place_attributes"),
    ]
}

/// A model's callback chains during linearization.
#[derive(Debug, Clone, Default)]
pub struct Chains {
    chains: std::collections::BTreeMap<Chain, Vec<Decl>>,
}

impl Chains {
    /// Register one declaration exactly as ActiveSupport's `set_callback`
    /// does (R-CHAIN-DEDUP, R-CHAIN-PREPEND).
    pub fn push(&mut self, d: Decl) {
        let chain = self.chains.entry(d.chain).or_default();
        if let Filter::Method(m) = &d.filter {
            chain.retain(|e| !(e.slot == d.slot && matches!(&e.filter, Filter::Method(x) if x == m)));
        }
        if d.prepend {
            chain.insert(0, d);
        } else {
            chain.push(d);
        }
    }

    /// Execution order: `before`/`around` in chain order, `after` in reverse
    /// chain order (ActiveSupport `CallbackSequence`), each mapped to the
    /// runtime phase.
    #[must_use]
    pub fn execution_order(&self) -> Vec<(Phase, Decl)> {
        let mut out = Vec::new();
        for (chain, decls) in &self.chains {
            let befores = decls.iter().filter(|d| d.slot != Slot::After);
            for d in befores {
                if let Some(p) = phase_of(*chain, d.slot) {
                    out.push((p, d.clone()));
                }
            }
            for d in decls.iter().rev().filter(|d| d.slot == Slot::After) {
                if let Some(p) = phase_of(*chain, d.slot) {
                    out.push((p, d.clone()));
                }
            }
        }
        out
    }
}

#[must_use]
pub fn phase_of(chain: Chain, slot: Slot) -> Option<Phase> {
    Some(match (chain, slot) {
        (Chain::Initialize, Slot::After) => Phase::AfterInitialize,
        (Chain::Find, Slot::After) => Phase::AfterFind,
        (Chain::Validation, Slot::Before) => Phase::BeforeValidation,
        (Chain::Validation, Slot::After) => Phase::AfterValidation,
        (Chain::Validate, _) => Phase::Validate,
        (Chain::Save, Slot::Before) => Phase::BeforeSave,
        (Chain::Save, Slot::Around) => Phase::AroundSave,
        (Chain::Save, Slot::After) => Phase::AfterSave,
        (Chain::Create, Slot::Before) => Phase::BeforeCreate,
        (Chain::Create, Slot::Around) => Phase::AroundCreate,
        (Chain::Create, Slot::After) => Phase::AfterCreate,
        (Chain::Update, Slot::Before) => Phase::BeforeUpdate,
        (Chain::Update, Slot::Around) => Phase::AroundUpdate,
        (Chain::Update, Slot::After) => Phase::AfterUpdate,
        (Chain::Destroy, Slot::Before) => Phase::BeforeDestroy,
        (Chain::Destroy, Slot::After) => Phase::AfterDestroy,
        (Chain::Commit, Slot::After) => Phase::AfterCommit,
        (Chain::Rollback, Slot::After) => Phase::AfterRollback,
        (Chain::Touch, Slot::After) => Phase::AfterTouch,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cb(chain: Chain, slot: Slot, m: &str, user_prepend: bool) -> Decl {
        Decl {
            chain,
            slot,
            filter: Filter::Method(m.into()),
            on: None,
            conds: vec![],
            prepend: effective_prepend(chain, slot, user_prepend),
            provenance: String::new(),
            rule: "source",
        }
    }

    fn names(c: &Chains) -> Vec<String> {
        c.execution_order().into_iter().map(|(_, d)| d.filter.name()).collect()
    }

    #[test]
    fn after_callbacks_run_in_declaration_order() {
        let mut c = Chains::default();
        c.push(cb(Chain::Save, Slot::After, "a", false));
        c.push(cb(Chain::Save, Slot::After, "b", false));
        assert_eq!(names(&c), ["a", "b"]);
    }

    #[test]
    fn redeclaring_a_symbol_moves_it_to_the_end() {
        let mut c = Chains::default();
        c.push(cb(Chain::Create, Slot::Before, "a", false));
        c.push(cb(Chain::Create, Slot::Before, "b", false));
        c.push(cb(Chain::Create, Slot::Before, "a", false));
        assert_eq!(names(&c), ["b", "a"]);
    }

    #[test]
    fn before_prepend_runs_first() {
        let mut c = Chains::default();
        c.push(cb(Chain::Destroy, Slot::Before, "a", false));
        c.push(cb(Chain::Destroy, Slot::Before, "b", true));
        assert_eq!(names(&c), ["b", "a"]);
    }

    #[test]
    fn after_commit_prepend_true_runs_first_under_in_order_defined() {
        let mut c = Chains::default();
        c.push(cb(Chain::Commit, Slot::After, "a", false));
        c.push(cb(Chain::Commit, Slot::After, "b", true));
        assert_eq!(names(&c), ["b", "a"]);
    }

    #[test]
    fn around_save_collection_association_is_deduplicated_to_the_last_collection() {
        let mut c = Chains::default();
        let fw = FrameworkDefaults::for_version(Some("8.1"));
        for d in association_decls(fw, "has_many", "x", Some(true), false, None, "") {
            c.push(d);
        }
        c.push(cb(Chain::Save, Slot::Before, "mine", false));
        for d in association_decls(fw, "has_many", "y", Some(true), false, None, "") {
            c.push(d);
        }
        let save: Vec<_> = c
            .execution_order()
            .into_iter()
            .filter(|(p, _)| matches!(p, Phase::BeforeSave | Phase::AroundSave))
            .map(|(_, d)| d.filter.name())
            .collect();
        assert_eq!(save, ["mine", "around_save_collection_association"]);
    }
}
