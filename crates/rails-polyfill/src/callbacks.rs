//! Callback chains as data.
//!
//! A model's callbacks are a flat, already-linearized list: the compiler
//! applies the declaration-time rules (concern expansion, duplicate
//! re-positioning, `prepend: true`, framework-generated callbacks) and emits
//! the result in the order Rails would execute it. The runtime only filters by
//! phase and condition, so the order is visible in the generated source and
//! is never re-derived at run time.

use crate::ctx::{Ctx, Halt};
use crate::record::Record;
use crate::value::Value;

/// The ActiveRecord callback chains, in the order a save visits them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    AfterInitialize,
    AfterFind,
    BeforeValidation,
    Validate,
    AfterValidation,
    BeforeSave,
    AroundSave,
    BeforeCreate,
    AroundCreate,
    BeforeUpdate,
    AroundUpdate,
    AfterCreate,
    AfterUpdate,
    AfterSave,
    BeforeDestroy,
    AroundDestroy,
    AfterDestroy,
    AfterCommit,
    AfterRollback,
    AfterTouch,
}

impl Phase {
    /// The macro spelling (`before_create`), used in journals and provenance.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AfterInitialize => "after_initialize",
            Self::AfterFind => "after_find",
            Self::BeforeValidation => "before_validation",
            Self::Validate => "validate",
            Self::AfterValidation => "after_validation",
            Self::BeforeSave => "before_save",
            Self::AroundSave => "around_save",
            Self::BeforeCreate => "before_create",
            Self::AroundCreate => "around_create",
            Self::BeforeUpdate => "before_update",
            Self::AroundUpdate => "around_update",
            Self::AfterCreate => "after_create",
            Self::AfterUpdate => "after_update",
            Self::AfterSave => "after_save",
            Self::BeforeDestroy => "before_destroy",
            Self::AroundDestroy => "around_destroy",
            Self::AfterDestroy => "after_destroy",
            Self::AfterCommit => "after_commit",
            Self::AfterRollback => "after_rollback",
            Self::AfterTouch => "after_touch",
        }
    }
}

/// `on:` — which persistence actions a callback applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct On {
    pub create: bool,
    pub update: bool,
    pub destroy: bool,
}

impl On {
    pub const ANY: Self = Self {
        create: true,
        update: true,
        destroy: true,
    };
    pub const CREATE: Self = Self {
        create: true,
        update: false,
        destroy: false,
    };
    pub const UPDATE: Self = Self {
        create: false,
        update: true,
        destroy: false,
    };
    pub const DESTROY: Self = Self {
        create: false,
        update: false,
        destroy: true,
    };

    #[must_use]
    pub fn applies(self, action: Action) -> bool {
        match action {
            Action::Create => self.create,
            Action::Update => self.update,
            Action::Destroy => self.destroy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    Destroy,
}

/// A lowered method body: the record is `self`.
pub type BodyFn = fn(&mut Ctx, &mut Record) -> Result<Value, Halt>;

/// What runs when a callback fires.
#[derive(Clone, Copy)]
pub enum Target {
    /// A method body the compiler lowered to Rust.
    Lowered(BodyFn),
    /// A boundary declared in the application's slice configuration: the body
    /// is not part of this compilation and the host answers for it. Firing
    /// one is journaled exactly like a lowered callback.
    Host,
    /// A body the compiler could not lower. Firing it fails the save — an
    /// unsupported construct never passes as success.
    Residual(&'static str),
    /// A framework callback that ActiveRecord defines itself
    /// (`autosave_associated_records_for_*`, `around_save_collection_association`).
    /// The runtime holds no association cache, so these have nothing to save;
    /// they are kept in the chain so its order stays checkable.
    Framework,
    /// A validator (`validates :x, presence: true`, `validates_with V`).
    Validator(crate::validations::Validator),
}

/// `if:` / `unless:`.
#[derive(Clone, Copy)]
pub enum Cond {
    If(BodyFn),
    Unless(BodyFn),
    /// A condition the compiler could not lower; evaluating it fails the save.
    Residual(&'static str),
}

#[derive(Clone, Copy)]
pub struct Callback {
    pub phase: Phase,
    /// The callback's journal name: the method symbol, or the framework /
    /// validator / object identity.
    pub name: &'static str,
    pub on: On,
    pub conds: &'static [Cond],
    pub target: Target,
    /// `path:line` of the declaration that produced this entry, or the rule
    /// id for framework-generated entries.
    pub provenance: &'static str,
}
