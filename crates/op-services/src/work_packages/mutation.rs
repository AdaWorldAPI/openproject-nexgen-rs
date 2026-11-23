//! The one authorized boundary for work-package mutations.
//!
//! Every create, update and delete of a work package, from the JSON API (`op-api`) and from the
//! pages (`op-server`), goes through this module. Nothing else may call the repository's
//! `create` / `update` / `delete` for a work package.
//!
//! The boundary does three things, in this order, and the order is the point:
//!
//! 1. **Identity.** A caller that is anonymous (and not explicitly allowed to be, which only the
//!    demo posture of the pages ever is) is refused before the store is touched, so an anonymous
//!    request learns nothing about which records exist.
//! 2. **Authorization**, project-scoped, by [`may`]: the permission must be held through a role of
//!    the caller's membership in the *record's* project (OpenProject's `allowed_in_project?`),
//!    and the project must be active. Create additionally runs the generated admission of
//!    `WorkPackages::CreateContract` through [`CreateWorkPackageService::call`].
//! 3. **The write**, through the store.
//!
//! Update and delete are two-step by type: [`authorize_update`] / [`authorize_delete`] return a
//! ticket ([`AuthorizedUpdate`], [`AuthorizedDelete`]) that is the only way to reach the store's
//! write. The page path needs the gap (CSRF is checked between the authorization and the write);
//! the API path uses [`update`] / [`delete`], which do both.
//!
//! Everything decision-shaped is pure and tested with a fake store; the Postgres store
//! ([`PgMutationStore`]) is the same for both callers.

use std::collections::HashSet;

use op_contracts::base::UserContext;
use op_contracts::work_packages::AdmissionFacts;
use op_core::error::ValidationErrors;
use op_core::traits::Id;
use op_db::repository::{Repository, RepositoryError};
use op_db::work_packages::{CreateWorkPackageDto, UpdateWorkPackageDto, WorkPackageRow};
use op_db::{
    MemberRepository, ProjectRepository, RoleRepository, StatusRepository, TypeRepository,
    UserRepository, WorkPackageRepository,
};
use sqlx::PgPool;

use super::{CreateWorkPackageService, WorkPackageParams};

/// The permission that gates a work-package create.
pub const ADD_WORK_PACKAGES: &str = "add_work_packages";
/// The permission that gates a work-package edit.
pub const EDIT_WORK_PACKAGES: &str = "edit_work_packages";
/// The permission that gates a work-package delete.
pub const DELETE_WORK_PACKAGES: &str = "delete_work_packages";
/// `Setting.journal_aggregation_time_minutes`, at its OpenProject default: `journal_aggregation_time_minutes:
/// { default: 5, allowed: 0..120 }` (`config/constants/settings/definition.rb:770-773`). This
/// schema has no settings table, so the default is the setting; `0` would switch aggregation off
/// (`Journals::CreateService#aggregation_active?`, `create_service.rb:603-605`).
pub const JOURNAL_AGGREGATION_TIME_MINUTES: i64 = 5;
/// `Setting.total_percent_complete_mode`, at its OpenProject default `work_weighted_average`
/// (`total_percent_complete_mode: { default: "work_weighted_average", allowed: %w[work_weighted_average
/// simple_average] }`, `config/constants/settings/definition.rb:279-283`). This schema has no
/// settings table, so the default is the setting. Only `work_weighted_average` is ported; under any
/// other value [`derive_done_ratio`]'s caller derives no done ratio (Ruby returns `nil` when
/// neither `work_weighted_average_mode?` nor `simple_average_mode?` holds,
/// `update_ancestors_service.rb:121-127`).
pub const TOTAL_PERCENT_COMPLETE_MODE: &str = WORK_WEIGHTED_AVERAGE;
/// The one `total_percent_complete_mode` this port computes.
pub const WORK_WEIGHTED_AVERAGE: &str = "work_weighted_average";
/// `Journal::CausedByWorkPackageChildValuesChange#type`
/// (`app/models/journal/caused_by_work_package_child_values_change.rb:31-34`).
pub const CAUSE_CHILD_VALUES_CHANGE: &str = "work_package_children_changed_values";

// ── who is asking ────────────────────────────────────────────────────

/// Who asks for a mutation, before any store lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    /// No signed-in person. `allowed` is the explicit opt-in of the pages' demo posture
    /// (`OP_ALLOW_ANONYMOUS`); the API never sets it, whatever its configuration says.
    Anonymous { allowed: bool },
    /// A verified person, named by the `users.login` the identity layer vouches for (the
    /// provisioning convention is `oidc:<subject>`). This module does no verification itself.
    Actor { login: String },
}

impl Caller {
    /// The caller of a mutation that no configuration may open to anonymous callers.
    pub fn anonymous_refused() -> Self {
        Caller::Anonymous { allowed: false }
    }

    fn refused_before_any_lookup(&self) -> bool {
        matches!(self, Caller::Anonymous { allowed: false })
    }
}

/// Who is asking, as far as authorization is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// No signed-in person. `allowed` is the explicit demo opt-in; without it an anonymous
    /// request may not write.
    Anonymous { allowed: bool },
    /// Signed in, but no `users` row has this actor as its login: no memberships, no rights.
    Unprovisioned,
    /// A `users` row, with the permissions their roles in the *target project* grant.
    User {
        admin: bool,
        active: bool,
        permissions: HashSet<String>,
    },
}

/// May `principal` exercise `permission` in a project whose `active` flag is `project_active`?
///
/// Mirrors OpenProject's `allowed_in_project?`: an archived project allows nothing (not even to an
/// admin), a locked user nothing, an admin everything else, anyone else exactly what their roles
/// in this project grant. A member of project A holds nothing in project B.
pub fn may(principal: &Principal, project_active: bool, permission: &str) -> bool {
    if !project_active {
        return false;
    }
    match principal {
        Principal::Anonymous { allowed } => *allowed,
        Principal::Unprovisioned => false,
        Principal::User {
            admin,
            active,
            permissions,
        } => *active && (*admin || permissions.contains(permission)),
    }
}

// ── errors ───────────────────────────────────────────────────────────

/// A store failure. `Conflict` and `NotFound` are kept apart because callers answer them
/// differently (409, 404); everything else is an opaque server fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreFailure {
    Conflict(String),
    NotFound(String),
    Other(String),
}

impl std::fmt::Display for StoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreFailure::Conflict(m) | StoreFailure::NotFound(m) | StoreFailure::Other(m) => {
                f.write_str(m)
            }
        }
    }
}

impl From<RepositoryError> for StoreFailure {
    fn from(e: RepositoryError) -> Self {
        match e {
            RepositoryError::Conflict(m) => StoreFailure::Conflict(m),
            RepositoryError::NotFound(m) => StoreFailure::NotFound(m),
            other => StoreFailure::Other(other.to_string()),
        }
    }
}

/// Why a mutation did not happen. Each variant has exactly one HTTP answer.
#[derive(Debug, Clone)]
pub enum MutationError {
    /// HTTP 401: the caller is anonymous.
    Unauthenticated,
    /// HTTP 403: the caller lacks the permission in the record's project (or the project is
    /// archived, missing, or the signed-in person has no `users` row).
    Forbidden,
    /// HTTP 404.
    NotFound,
    /// HTTP 422: the contract's error keys and codes (`project: blank`, ...).
    Invalid(ValidationErrors),
    /// HTTP 422: a decision that could not be made from trusted data (an admission fact the
    /// caller cannot supply, or a default that is not configured). Never answered with a guess.
    Unresolved(String),
    /// HTTP 409: the record was modified by someone else (`lock_version`).
    Conflict(String),
    /// HTTP 500.
    Store(String),
}

impl From<StoreFailure> for MutationError {
    fn from(e: StoreFailure) -> Self {
        match e {
            StoreFailure::Conflict(m) => MutationError::Conflict(m),
            StoreFailure::NotFound(_) => MutationError::NotFound,
            StoreFailure::Other(m) => MutationError::Store(m),
        }
    }
}

// ── the store seams ──────────────────────────────────────────────────

/// What authorizing and updating an *existing* record needs. The pages and the API both implement
/// or adapt to it, so the decision logic is tested with a fake.
#[allow(async_fn_in_trait)]
pub trait MutationStore {
    async fn find_wp(&self, id: Id) -> Result<Option<WorkPackageRow>, StoreFailure>;
    /// Whether the project exists and is active (an archived or missing project allows nothing).
    async fn project_active(&self, project_id: Id) -> Result<bool, StoreFailure>;
    /// The caller's principal against `project_id`.
    async fn principal(&self, caller: &Caller, project_id: Id) -> Result<Principal, StoreFailure>;
    /// Run the whole update (the lookup, the authorization reads, the UPDATE and the journal) as
    /// ONE unit of work: committed when it returns `Ok`, rolled back when it returns an error.
    /// OpenProject's `WorkPackages::UpdateService` runs inside
    /// `Shared::ServiceContext#in_mutex_context`: an advisory lock on the work package inside
    /// `ActiveRecord::Base.transaction`, rolled back (`raise ActiveRecord::Rollback`) when the
    /// result is a failure (`app/services/shared/service_context.rb:34-49`).
    ///
    /// The caller is the person acting: the journal records them, and aggregation compares them
    /// with the user of the last journal. A store with no transaction (a test fake) just
    /// updates; [`PgMutationStore`] opens a real transaction.
    async fn update_unit_of_work(
        &self,
        caller: &Caller,
        id: Id,
        dto: UpdateWorkPackageDto,
    ) -> Result<WorkPackageRow, MutationError>;
}

/// What a delete additionally needs.
#[allow(async_fn_in_trait)]
pub trait DeleteStore: MutationStore {
    async fn delete_wp(&self, id: Id) -> Result<(), StoreFailure>;
}

/// What a create additionally needs: the facts Rails reads from the database to default what the
/// caller left out, and the insert.
#[allow(async_fn_in_trait)]
pub trait CreationStore: MutationStore {
    /// `users.id` of the signed-in person, `None` when no `users` row has this login.
    async fn author_id(&self, caller: &Caller) -> Result<Option<Id>, StoreFailure>;
    /// `project.enabled_types.first` (`app/models/projects/enabled_types.rb:34`,
    /// `Type.enabled_in`, `app/models/type.rb:101-103`, ordered by `position`, `type.rb:61`).
    async fn first_enabled_type(&self, project_id: Id) -> Result<Option<Id>, StoreFailure>;
    /// `Status.default` (`app/models/status.rb:70-76`: the first row with `is_default`).
    async fn default_status(&self) -> Result<Option<Id>, StoreFailure>;
    /// `IssuePriority.active.default` (`app/models/issue_priority.rb`, `app/models/enumeration.rb:50,58-67`:
    /// the first active `IssuePriority` with `is_default`, ordered by `position`,
    /// `enumeration.rb:33`).
    async fn default_priority(&self) -> Result<Option<Id>, StoreFailure>;
    /// Whether the status is closed; `None` when no such status exists.
    async fn status_is_closed(&self, status_id: Id) -> Result<Option<bool>, StoreFailure>;
    async fn insert_wp(&self, dto: CreateWorkPackageDto) -> Result<WorkPackageRow, StoreFailure>;
    /// Run the whole create (authorization reads, defaults, insert and anything after the insert)
    /// as ONE unit of work: committed when `create_in` returns `Ok`, rolled back when it returns
    /// an error. OpenProject's `Shared::ServiceContext#in_user_context` wraps
    /// `WorkPackages::CreateService` in `ActiveRecord::Base.transaction` and raises
    /// `ActiveRecord::Rollback` on a failed result (`app/services/shared/service_context.rb`).
    ///
    /// A store with no transaction (a test fake) runs `create_in(self, ..)`; [`PgMutationStore`]
    /// opens a real transaction and hands `create_in` a scope bound to it.
    async fn create_unit_of_work(
        &self,
        caller: &Caller,
        params: WorkPackageParams,
    ) -> Result<WorkPackageRow, MutationError>;
}

// ── authorization ────────────────────────────────────────────────────

/// The one authorization decision: may this caller exercise `permission` in `project_id`?
async fn authorize<S: MutationStore>(
    store: &S,
    caller: &Caller,
    project_id: Id,
    permission: &str,
) -> Result<(), MutationError> {
    let active = store.project_active(project_id).await?;
    let principal = store.principal(caller, project_id).await?;
    if may(&principal, active, permission) {
        return Ok(());
    }
    Err(match principal {
        Principal::Anonymous { allowed: false } => MutationError::Unauthenticated,
        _ => MutationError::Forbidden,
    })
}

/// Proof that the caller may edit this record. The only way to reach
/// [`MutationStore::update_unit_of_work`] from this module; it cannot be built outside it.
#[derive(Debug)]
pub struct AuthorizedUpdate {
    existing: WorkPackageRow,
    caller: Caller,
}

impl AuthorizedUpdate {
    /// The record as it was when the authorization was decided.
    pub fn existing(&self) -> &WorkPackageRow {
        &self.existing
    }

    /// Perform the write the authorization was for.
    pub async fn commit<S: MutationStore>(
        self,
        store: &S,
        dto: UpdateWorkPackageDto,
    ) -> Result<WorkPackageRow, MutationError> {
        validate_update(&self.existing, &dto)?;
        store
            .update_unit_of_work(&self.caller, self.existing.id, dto)
            .await
    }
}

/// Proof that the caller may delete this record.
#[derive(Debug)]
pub struct AuthorizedDelete {
    existing: WorkPackageRow,
}

impl AuthorizedDelete {
    pub fn existing(&self) -> &WorkPackageRow {
        &self.existing
    }

    pub async fn commit<S: DeleteStore>(self, store: &S) -> Result<(), MutationError> {
        Ok(store.delete_wp(self.existing.id).await?)
    }
}

/// `edit_work_packages` on record `id`, refusing the anonymous caller before any lookup, demo
/// posture or not: an anonymous edit cannot be journaled here (no `users.type`, so no
/// `AnonymousUser` row), and an edit the history cannot attribute is not applied
/// (docs/decisions/0001-journaled-edits.md §2, a deliberate divergence from OpenProject).
async fn authorize_edit<S: MutationStore>(
    store: &S,
    caller: &Caller,
    id: Id,
) -> Result<WorkPackageRow, MutationError> {
    if matches!(caller, Caller::Anonymous { .. }) {
        return Err(MutationError::Unauthenticated);
    }
    authorize_record(store, caller, id, EDIT_WORK_PACKAGES).await
}

/// The model validation an update can newly fail: `validates :priority, presence: true`
/// (`app/models/work_package/validations.rb:34`, run by `ModelContract#valid?`,
/// `app/contracts/model_contract.rb:85-88`). A work package stored without a priority, edited
/// without setting one, is refused with `priority: blank` before any write
/// (docs/decisions/0001-journaled-edits.md §1).
fn validate_update(existing: &WorkPackageRow, dto: &UpdateWorkPackageDto) -> Result<(), MutationError> {
    if dto.priority_id.or(existing.priority_id).is_none() {
        let mut errors = ValidationErrors::new();
        errors.add("priority", "blank");
        return Err(MutationError::Invalid(errors));
    }
    Ok(())
}

async fn authorize_record<S: MutationStore>(
    store: &S,
    caller: &Caller,
    id: Id,
    permission: &str,
) -> Result<WorkPackageRow, MutationError> {
    if caller.refused_before_any_lookup() {
        return Err(MutationError::Unauthenticated);
    }
    let existing = store.find_wp(id).await?.ok_or(MutationError::NotFound)?;
    authorize(store, caller, existing.project_id, permission).await?;
    Ok(existing)
}

/// Decide whether `caller` may edit work package `id` (`edit_work_packages` in the record's
/// project). Looks the record up, so a missing record is [`MutationError::NotFound`].
pub async fn authorize_update<S: MutationStore>(
    store: &S,
    caller: &Caller,
    id: Id,
) -> Result<AuthorizedUpdate, MutationError> {
    let existing = authorize_edit(store, caller, id).await?;
    Ok(AuthorizedUpdate {
        existing,
        caller: caller.clone(),
    })
}

/// Decide whether `caller` may delete work package `id` (`delete_work_packages` in the record's
/// project).
pub async fn authorize_delete<S: MutationStore>(
    store: &S,
    caller: &Caller,
    id: Id,
) -> Result<AuthorizedDelete, MutationError> {
    let existing = authorize_record(store, caller, id, DELETE_WORK_PACKAGES).await?;
    Ok(AuthorizedDelete { existing })
}

/// Update work package `id`: authorize, then write.
pub async fn update<S: MutationStore>(
    store: &S,
    caller: &Caller,
    id: Id,
    dto: UpdateWorkPackageDto,
) -> Result<WorkPackageRow, MutationError> {
    authorize_update(store, caller, id)
        .await?
        .commit(store, dto)
        .await
}

/// Delete work package `id`: authorize, then delete.
pub async fn delete<S: DeleteStore>(
    store: &S,
    caller: &Caller,
    id: Id,
) -> Result<(), MutationError> {
    authorize_delete(store, caller, id)
        .await?
        .commit(store)
        .await
}

// ── create ───────────────────────────────────────────────────────────

/// The user the create's admission is evaluated for: the caller's rights in the one project the
/// create targets. A grant in another project is not a grant here.
struct ScopedUser {
    id: Id,
    admin: bool,
    project_id: Id,
    permissions: HashSet<String>,
}

impl UserContext for ScopedUser {
    fn id(&self) -> Id {
        self.id
    }
    fn is_admin(&self) -> bool {
        self.admin
    }
    fn is_anonymous(&self) -> bool {
        false
    }
    fn allowed_in_project(&self, permission: &str, project_id: Id) -> bool {
        project_id == self.project_id && (self.admin || self.permissions.contains(permission))
    }
    fn allowed_globally(&self, _permission: &str) -> bool {
        false
    }
}

/// The model facts the admission guards read, from data this module looked up itself. A fact the
/// create does not determine is `None`, which makes the verdict unresolved, never true.
struct CreateFacts {
    status_submitted: bool,
    status_closed: Option<bool>,
    versions_submitted: bool,
}

impl AdmissionFacts for CreateFacts {
    fn fact(&self, name: &str) -> Option<bool> {
        match name {
            // A new record: a submitted status is a change from nothing.
            "model.status_id_change" => Some(self.status_submitted),
            "model.status&.closed?" | "model.status.closed?" => self.status_closed,
            // No version is attached unless one was submitted, and none is ever looked up.
            "model.effective_target_versions.any?(&:closed?)" => {
                (!self.versions_submitted).then_some(false)
            }
            _ => None,
        }
    }
}

fn is_unresolved_refusal(msg: &str) -> Option<String> {
    (msg.starts_with("admission_unresolved") || msg.starts_with("type_default_unresolved"))
        .then(|| msg.to_string())
}

/// Create a work package for `caller` from the attributes the caller submitted.
///
/// * The caller must be a verified person with a `users` row, and may add work packages in the
///   submitted project ([`may`] with [`ADD_WORK_PACKAGES`]): a missing, archived or foreign
///   project all answer [`MutationError::Forbidden`], so project ids cannot be probed.
/// * No project is ever chosen for the caller: without one the answer is `project: blank`
///   (`app/models/work_package/validations.rb:34`) and nothing is stored.
/// * Without a type, Rails assigns `project.enabled_types.first`
///   (`app/services/work_packages/set_attributes_service.rb:458-459`); that is read from the
///   project's enabled types, and a project without any is a typed refusal, not type 1.
/// * Without a status, Rails uses `Status.default` (`set_attributes_service.rb:233-235`); that is
///   read from `statuses.is_default`, and its absence is a typed refusal, not status 1.
/// * The generated admission of `WorkPackages::CreateContract` runs through
///   [`CreateWorkPackageService::call`]; the author is the caller, never a submitted value.
pub async fn create<S: CreationStore>(
    store: &S,
    caller: &Caller,
    params: WorkPackageParams,
) -> Result<WorkPackageRow, MutationError> {
    store.create_unit_of_work(caller, params).await
}

/// The body of [`create`]: identity, authorization, defaults, admission and the insert, against
/// whatever scope the store hands it. Called by `CreationStore::create_unit_of_work`
/// implementations, inside their transaction; not a boundary of its own.
pub async fn create_in<S: CreationStore>(
    store: &S,
    caller: &Caller,
    params: WorkPackageParams,
) -> Result<WorkPackageRow, MutationError> {
    // Identity. An anonymous caller never creates, not even where pages allow anonymous edits:
    // a work package needs an author.
    if !matches!(caller, Caller::Actor { .. }) {
        return Err(MutationError::Unauthenticated);
    }
    let author_id = store
        .author_id(caller)
        .await?
        .ok_or(MutationError::Forbidden)?;

    // No project is ever picked for the caller.
    let Some(project_id) = params.project_id else {
        let mut errors = ValidationErrors::new();
        errors.add("project", "blank");
        return Err(MutationError::Invalid(errors));
    };

    // Authorization, project-scoped.
    let active = store.project_active(project_id).await?;
    let principal = store.principal(caller, project_id).await?;
    if !may(&principal, active, ADD_WORK_PACKAGES) {
        return Err(MutationError::Forbidden);
    }
    let Principal::User {
        admin, permissions, ..
    } = principal
    else {
        return Err(MutationError::Forbidden);
    };
    let scoped = ScopedUser {
        id: author_id,
        admin,
        project_id,
        permissions,
    };

    // Defaults from trusted data. The defaulted type is handed to the service because the service
    // needs one; the defaulted status is not, because a default is not a submitted attribute.
    let mut params = params;
    if params.type_id.is_none() {
        let first = store.first_enabled_type(project_id).await?;
        params.type_id = Some(first.ok_or_else(|| {
            MutationError::Unresolved(
                "type_default_unresolved: the project has no enabled type".into(),
            )
        })?);
    }
    let status_closed = match params.status_id {
        Some(s) => Some(store.status_is_closed(s).await?.ok_or_else(|| {
            MutationError::Unresolved(format!("status_unknown: no status with id {s}"))
        })?),
        None => None,
    };
    // `set_default_priority` (`set_attributes_service.rb:237-239`): `work_package.priority ||=
    // IssuePriority.active.default`. With no default Ruby leaves it nil, and the model validation
    // `validates :priority, ... presence: true` (`app/models/work_package/validations.rb:34`)
    // fails the save with `priority: blank`; that is mirrored below, never a NULL insert.
    let priority_id = match params.priority_id {
        Some(p) => Some(p),
        None => store.default_priority().await?,
    };
    let facts = CreateFacts {
        status_submitted: params.status_id.is_some(),
        status_closed,
        versions_submitted: params.version_id.is_some(),
    };

    let result = CreateWorkPackageService::without_notifications(&scoped)
        .with_facts(&facts)
        .call(params.clone());
    if result.is_failure() {
        let mut errors = result.errors().clone();
        if priority_id.is_none() {
            errors.add("priority", "blank");
        }
        if let Some(msg) = errors
            .base_errors
            .iter()
            .find_map(|m| is_unresolved_refusal(m))
        {
            return Err(MutationError::Unresolved(msg));
        }
        if errors.base_errors.iter().any(|m| m == "error_unauthorized") {
            return Err(MutationError::Forbidden);
        }
        return Err(MutationError::Invalid(errors));
    }
    let entity = result.unwrap();
    let Some(priority_id) = priority_id else {
        let mut errors = ValidationErrors::new();
        errors.add("priority", "blank");
        return Err(MutationError::Invalid(errors));
    };

    let status_id = match params.status_id {
        Some(s) => s,
        None => store.default_status().await?.ok_or_else(|| {
            MutationError::Unresolved(
                "status_default_unresolved: no status is marked default".into(),
            )
        })?,
    };

    // The entity's own status and priority are placeholders of the service's in-memory model;
    // the row takes the resolved status and the resolved priority.
    let dto = CreateWorkPackageDto {
        subject: entity.subject,
        description: entity.description,
        project_id: entity.project_id,
        type_id: entity.type_id,
        status_id,
        priority_id: Some(priority_id),
        author_id: entity.author_id,
        assigned_to_id: entity.assigned_to_id,
        responsible_id: entity.responsible_id,
        start_date: entity.start_date,
        due_date: entity.due_date,
        estimated_hours: entity.estimated_hours,
        remaining_hours: entity.remaining_hours,
        done_ratio: entity.done_ratio,
        parent_id: entity.parent_id,
        version_id: entity.version_id,
        category_id: entity.category_id,
    };
    Ok(store.insert_wp(dto).await?)
}

// ── ancestor totals (pure part) ──────────────────────────────────────

/// `Journal::CausedByWorkPackageChildValuesChange.new(work_package).to_hash`
/// (`caused_by_work_package_child_values_change.rb:31-36`, `CauseOfChange::Base#to_hash`,
/// `lib_static/plugins/acts_as_journalized/lib/cause_of_change/base.rb:38-40`): `{"type" => type}`
/// merged with `{"work_package_id" => id}`. `Journals::CreateService#cause_sql` stores it as JSON
/// (`create_service.rb:583-586`).
pub fn child_values_change_cause(work_package_id: Id) -> serde_json::Value {
    serde_json::json!({
        "type": CAUSE_CHILD_VALUES_CHANGE,
        "work_package_id": work_package_id,
    })
}

/// Ruby's `Array#sum` over floats (`rb_ary_sum`, `array.c`): Kahan-Babuska compensated summation
/// from an integer `0`, so a list of floats sums with the compensation term added at the end. A
/// plain left fold (or Postgres `sum(float8)`) can differ in the last bit. Non-finite values follow
/// Ruby's branches: NaN wins, infinities of opposite sign give NaN.
pub fn ruby_float_sum(values: &[f64]) -> f64 {
    let mut f = 0.0_f64;
    let mut c = 0.0_f64;
    for &x in values {
        if f.is_nan() {
            continue;
        }
        if x.is_nan() {
            f = x;
            continue;
        }
        if x.is_infinite() {
            if f.is_infinite() && x.signum() != f.signum() {
                f = f64::NAN;
            } else {
                f = x;
            }
            continue;
        }
        if f.is_infinite() {
            continue;
        }
        let t = f + x;
        if f.abs() >= x.abs() {
            c += (f - t) + x;
        } else {
            c += (x - t) + f;
        }
        f = t;
    }
    f + c
}

/// One work package as `UpdateAncestors::Loader` sees it for a total: its own `estimated_hours` and
/// `remaining_hours`, and whether its status is `excluded_from_totals`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TotalsInput {
    pub estimated_hours: Option<f64>,
    pub remaining_hours: Option<f64>,
    pub excluded_from_totals: bool,
}

/// The three derived values of one ancestor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerivedTotals {
    pub estimated_hours: Option<f64>,
    pub remaining_hours: Option<f64>,
    pub done_ratio: Option<i32>,
}

/// `derive_total` (`update_ancestors_service.rb:219-228`): `nil` when the ancestor has no
/// descendants (`no_children?` counts every descendant, excluded or not); otherwise the sum of the
/// non-nil values over the ancestor and its descendants whose status is not excluded from totals
/// (`included_in_totals_calculation?`, `app/models/work_package.rb:295-297`, and the loader's
/// `WorkPackageLikeStruct`, `loader.rb:33-37`), `nil` when there is none.
fn derive_total(
    own: &TotalsInput,
    descendants: &[TotalsInput],
    attribute: fn(&TotalsInput) -> Option<f64>,
) -> Option<f64> {
    if descendants.is_empty() {
        return None;
    }
    let values: Vec<f64> = std::iter::once(own)
        .chain(descendants)
        .filter(|w| !w.excluded_from_totals)
        .filter_map(attribute)
        .collect();
    if values.is_empty() {
        return None;
    }
    Some(ruby_float_sum(&values))
}

/// `calculate_work_weighted_average_percent_complete` (`update_ancestors_service.rb:129-138`):
/// `nil` when either total is `nil` or the estimate is zero, otherwise
/// `(((estimated - remaining).to_f / estimated) * 100).round`. Ruby's `Float#round` rounds half
/// away from zero, as Rust's `f64::round`; Postgres' `round(double precision)` rounds half to even,
/// so this is computed here and not in SQL.
pub fn derive_done_ratio(
    derived_estimated: Option<f64>,
    derived_remaining: Option<f64>,
) -> Option<i32> {
    let (estimated, remaining) = (derived_estimated?, derived_remaining?);
    if estimated == 0.0 {
        return None;
    }
    let work_done = estimated - remaining;
    let progress = (work_done / estimated) * 100.0;
    Some(progress.round() as i32)
}

/// `derive_total_estimated_and_remaining_hours` then `derive_done_ratio`
/// (`update_ancestors_service.rb:210-213`, :103-107, `compute_derived_done_ratio` :112-127) for
/// one ancestor, from itself and all its descendants.
pub fn derive_ancestor_totals(own: &TotalsInput, descendants: &[TotalsInput]) -> DerivedTotals {
    let estimated_hours = derive_total(own, descendants, |w| w.estimated_hours);
    let remaining_hours = derive_total(own, descendants, |w| w.remaining_hours);
    let done_ratio = if descendants.is_empty() {
        None
    } else {
        match TOTAL_PERCENT_COMPLETE_MODE {
            WORK_WEIGHTED_AVERAGE => derive_done_ratio(estimated_hours, remaining_hours),
            _ => None,
        }
    };
    DerivedTotals {
        estimated_hours,
        remaining_hours,
        done_ratio,
    }
}

// ── Postgres store ───────────────────────────────────────────────────

/// Resolve the principal of `caller` against `project_id`.
///
/// The actor is matched against `users.login`; the permissions in the project are the union of
/// the permissions of every role of the person's membership there.
pub async fn load_principal(
    pool: &PgPool,
    caller: &Caller,
    project_id: Id,
) -> Result<Principal, StoreFailure> {
    let login = match caller {
        Caller::Anonymous { allowed } => return Ok(Principal::Anonymous { allowed: *allowed }),
        Caller::Actor { login } => login,
    };
    let Some(user) = UserRepository::new(pool.clone())
        .find_by_login(login)
        .await?
    else {
        return Ok(Principal::Unprovisioned);
    };
    let mut permissions = HashSet::new();
    if let Some(member) = MemberRepository::new(pool.clone())
        .find_by_project_and_user(project_id, user.id)
        .await?
    {
        let roles = RoleRepository::new(pool.clone());
        for role_id in member.role_ids {
            permissions.extend(roles.get_permissions(role_id).await?);
        }
    }
    Ok(Principal::User {
        admin: user.admin,
        active: user.is_active(),
        permissions,
    })
}

/// Whether the project exists and is active.
pub async fn load_project_active(pool: &PgPool, project_id: Id) -> Result<bool, StoreFailure> {
    Ok(ProjectRepository::new(pool.clone())
        .find_by_id(project_id)
        .await?
        .is_some_and(|p| p.active))
}

/// The Postgres-backed store, the same for the API and the pages.
pub struct PgMutationStore {
    pub pool: PgPool,
}

impl MutationStore for PgMutationStore {
    async fn find_wp(&self, id: Id) -> Result<Option<WorkPackageRow>, StoreFailure> {
        Ok(WorkPackageRepository::new(self.pool.clone())
            .find_by_id(id)
            .await?)
    }

    async fn project_active(&self, project_id: Id) -> Result<bool, StoreFailure> {
        load_project_active(&self.pool, project_id).await
    }

    async fn principal(&self, caller: &Caller, project_id: Id) -> Result<Principal, StoreFailure> {
        load_principal(&self.pool, caller, project_id).await
    }

    async fn update_unit_of_work(
        &self,
        caller: &Caller,
        id: Id,
        dto: UpdateWorkPackageDto,
    ) -> Result<WorkPackageRow, MutationError> {
        self.update_then(caller, id, dto, None).await
    }
}

impl DeleteStore for PgMutationStore {
    async fn delete_wp(&self, id: Id) -> Result<(), StoreFailure> {
        Ok(WorkPackageRepository::new(self.pool.clone())
            .delete(id)
            .await?)
    }
}

impl CreationStore for PgMutationStore {
    async fn author_id(&self, caller: &Caller) -> Result<Option<Id>, StoreFailure> {
        let Caller::Actor { login } = caller else {
            return Ok(None);
        };
        Ok(UserRepository::new(self.pool.clone())
            .find_by_login(login)
            .await?
            .map(|u| u.id))
    }

    async fn first_enabled_type(&self, project_id: Id) -> Result<Option<Id>, StoreFailure> {
        // `find_by_project` joins `projects_types` and orders by `position`.
        Ok(TypeRepository::new(self.pool.clone())
            .find_by_project(project_id)
            .await?
            .first()
            .map(|t| t.id))
    }

    async fn default_status(&self) -> Result<Option<Id>, StoreFailure> {
        Ok(StatusRepository::new(self.pool.clone())
            .find_default()
            .await?
            .map(|s| s.id))
    }

    async fn default_priority(&self) -> Result<Option<Id>, StoreFailure> {
        // Same statement as `PgCreateScope::default_priority`; keep the two in sync.
        sqlx::query_scalar(
            "SELECT id FROM enumerations WHERE type = 'IssuePriority' AND active = true \
             AND is_default = true ORDER BY position ASC, id ASC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_failure)
    }

    async fn status_is_closed(&self, status_id: Id) -> Result<Option<bool>, StoreFailure> {
        Ok(StatusRepository::new(self.pool.clone())
            .find_by_id(status_id)
            .await?
            .map(|s| s.is_closed))
    }

    async fn insert_wp(&self, dto: CreateWorkPackageDto) -> Result<WorkPackageRow, StoreFailure> {
        Ok(WorkPackageRepository::new(self.pool.clone())
            .create(dto)
            .await?)
    }

    async fn create_unit_of_work(
        &self,
        caller: &Caller,
        params: WorkPackageParams,
    ) -> Result<WorkPackageRow, MutationError> {
        self.create_then(caller, params, None).await
    }
}

impl PgMutationStore {
    /// The production create: one transaction around [`create_in`] and `post_insert`, committed
    /// only if both succeed. `post_insert` is the statement that runs after the INSERT on the same
    /// transaction (OpenProject runs `after_create`: ancestors, rescheduling, watcher, and the
    /// journal in its save callbacks, inside `in_user_context`). The journal is written here
    /// ([`PgCreateScope::write_creation_journal`]), then the ancestors' totals
    /// ([`PgCreateScope::update_ancestors_of_created`]), then the creator as watcher
    /// ([`PgCreateScope::add_creator_as_watcher`]); `apply_patterns` and rescheduling are not
    /// ported. `None` is the production call. `post_insert` exists so the
    /// failure-after-the-journal case can be exercised through the real transaction code.
    pub async fn create_then(
        &self,
        caller: &Caller,
        params: WorkPackageParams,
        post_insert: Option<&str>,
    ) -> Result<WorkPackageRow, MutationError> {
        let scope = PgCreateScope::begin(&self.pool).await?;
        let mut out = create_in(&scope, caller, params).await;
        // The creation journal (`acts_as_journalized`'s `after_save :save_journals`), on the same
        // transaction as the INSERT: a failure here rolls the work package back with it.
        if let Ok(row) = &out {
            if let Err(e) = scope.write_creation_journal(row.id, row.author_id).await {
                out = Err(e.into());
            }
        }
        // `after_create`'s `multi_update_ancestors` (create_service.rb:75-77): the totals of the
        // ancestors, before the watcher (:82) and after the journal. Only a work package with a
        // parent has ancestors.
        if let Ok(row) = &out {
            if row.parent_id.is_some() {
                if let Err(e) = scope
                    .update_ancestors_of_created(row.id, row.parent_id, row.author_id)
                    .await
                {
                    out = Err(e.into());
                }
            }
        }
        // `after_create`'s last step, `set_user_as_watcher` (create_service.rb:82, :118-122). The
        // creator is the author here (`create_in` sets `author_id` from the caller).
        if let Ok(row) = &out {
            if let Err(e) = scope
                .add_creator_as_watcher(row.id, row.project_id, row.author_id)
                .await
            {
                out = Err(e.into());
            }
        }
        if let (Ok(_), Some(sql)) = (&out, post_insert) {
            if let Err(e) = scope.execute(sql).await {
                out = Err(e.into());
            }
        }
        match out {
            Ok(row) => {
                scope.commit().await?;
                Ok(row)
            }
            Err(e) => {
                scope.rollback().await;
                Err(e)
            }
        }
    }
}

impl PgMutationStore {
    /// The production update: one transaction around the advisory lock, the authorization reads,
    /// the UPDATE and the journal, committed only if all succeed (OpenProject:
    /// `WorkPackages::UpdateService < BaseServices::Update`, run by `in_mutex_context`,
    /// `app/services/shared/service_context.rb:34-49`). `post_update` is a statement that runs on
    /// the same transaction after the journal (`None` is the production call); it exists so the
    /// failure-after-the-journal case can be exercised through the real transaction code.
    pub async fn update_then(
        &self,
        caller: &Caller,
        id: Id,
        dto: UpdateWorkPackageDto,
        post_update: Option<&str>,
    ) -> Result<WorkPackageRow, MutationError> {
        let scope = PgCreateScope::begin(&self.pool).await?;
        let mut out = scope
            .update_in(caller, id, dto, JOURNAL_AGGREGATION_TIME_MINUTES)
            .await;
        if let (Ok(_), Some(sql)) = (&out, post_update) {
            if let Err(e) = scope.execute(sql).await {
                out = Err(e.into());
            }
        }
        match out {
            Ok(row) => {
                scope.commit().await?;
                Ok(row)
            }
            Err(e) => {
                scope.rollback().await;
                Err(e)
            }
        }
    }
}

/// What an update wrote to the journals ([`PgCreateScope::write_update_journal`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateJournal {
    /// No journaled column differs from the last journal: nothing was written.
    Unchanged,
    /// The last journal was rewritten in place (same user within the aggregation window).
    Aggregated,
    /// A new journal of this version was added and its predecessor's validity period closed.
    Created { version: i32 },
}

fn db_failure(e: sqlx::Error) -> StoreFailure {
    StoreFailure::from(RepositoryError::from(e))
}

/// A create's or update's database scope: every read and the write run on ONE open transaction,
/// so the authorization facts and the write are isolated together and a failure after the write
/// leaves nothing behind. It is only ever built by [`PgMutationStore`]'s `create_then` and
/// `update_then`, which commit or roll it back.
///
/// The queries repeat those of the repositories (`UserRepository::find_by_login`,
/// `MemberRepository::find_by_project_and_user` + `RoleRepository::get_permissions`,
/// `TypeRepository::find_by_project`, `StatusRepository::{find_default, find_by_id}`,
/// `WorkPackageRepository::create`) because those are bound to the pool and cannot join a
/// transaction.
pub struct PgCreateScope {
    tx: tokio::sync::Mutex<sqlx::Transaction<'static, sqlx::Postgres>>,
}

impl PgCreateScope {
    pub async fn begin(pool: &PgPool) -> Result<Self, StoreFailure> {
        let tx = pool.begin().await.map_err(db_failure)?;
        Ok(Self {
            tx: tokio::sync::Mutex::new(tx),
        })
    }

    pub async fn commit(self) -> Result<(), StoreFailure> {
        self.tx
            .into_inner()
            .commit()
            .await
            .map_err(db_failure)
    }

    /// Roll back. A failed rollback is not reported: the connection is dropped, which makes the
    /// server discard the transaction anyway.
    pub async fn rollback(self) {
        let _ = self.tx.into_inner().rollback().await;
    }

    /// `WorkPackages::CreateService#after_create`'s `multi_update_ancestors`
    /// (`create_service.rb:75-77`) for the new work package `work_package_id`: Ruby runs
    /// `UpdateAncestorsService` (`update_ancestors_service.rb:40-52`) with every attribute marked
    /// changed (`Shared::UpdateAncestors#update_ancestors`, `shared/update_ancestors.rb:44-50`), so
    /// each ancestor, parent first, derives its totals from itself and all its descendants
    /// ([`derive_ancestor_totals`]); only an ancestor whose derived values changed is saved
    /// (`select { ancestor.changed? }`, `update_ancestors_service.rb:62-69`), and each saved
    /// ancestor is journaled with `Journal::CausedByWorkPackageChildValuesChange`
    /// (`set_journal_cause`, :99-102) as the acting `user_id` (`save_journals` falls back to
    /// `User.current`, which `in_user_context` sets to the creator,
    /// `lib_static/plugins/acts_as_journalized/lib/acts/journalized/save_hooks.rb:61-80`),
    /// through [`PgCreateScope::write_journal`] so the aggregation rules of
    /// `Journals::CreateService` apply. The save is `wp.save(validate: false)` (:78-87): it bumps
    /// `lock_version` and `updated_at` as any save does.
    ///
    /// The new work package is itself one of the loader's candidates (`Loader#select` yields
    /// `[work_package, *ancestors]`), but it has no descendants, so every derived value is `nil`
    /// as before and it is not saved: nothing is done for it here. Returns the ids of the ancestors
    /// that were saved, nearest first.
    ///
    /// Not ported: `switch_to_automatic_mode` (:157-181; it needs scheduling relations, which this
    /// schema does not have), `derive_ignore_non_working_days` (:183-199; no column),
    /// `simple_average` mode and the status-based done-ratio mode ([`TOTAL_PERCENT_COMPLETE_MODE`]),
    /// former ancestors (only a change of parent has any, `Loader#former_ancestors`), the
    /// `JOURNAL_CREATED` notification (the ancestors are saved with notifications off,
    /// :80-84) and the update path: an edit does not recompute ancestors yet. Like Ruby's create
    /// (`in_user_context`, not `in_mutex_context`) it takes no lock on the ancestors.
    pub async fn update_ancestors_of_created(
        &self,
        work_package_id: Id,
        parent_id: Option<Id>,
        user_id: Id,
    ) -> Result<Vec<Id>, StoreFailure> {
        let Some(parent_id) = parent_id else {
            return Ok(Vec::new());
        };
        // The chain parent, grandparent, ... root. `UNION` (not `UNION ALL`) ends a cycle.
        let links: Vec<(Id, Option<Id>)> = {
            let mut tx = self.tx.lock().await;
            sqlx::query_as(
                "WITH RECURSIVE up(id, parent_id) AS ( \
                     SELECT id, parent_id FROM work_packages WHERE id = $1 \
                     UNION \
                     SELECT p.id, p.parent_id FROM up JOIN work_packages p ON p.id = up.parent_id) \
                 SELECT id, parent_id FROM up",
            )
            .bind(parent_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(db_failure)?
        };
        let parent_of: std::collections::HashMap<Id, Option<Id>> = links.into_iter().collect();
        let mut chain = Vec::new();
        let mut next = Some(parent_id);
        while let Some(id) = next {
            if id == work_package_id || chain.contains(&id) || !parent_of.contains_key(&id) {
                break;
            }
            chain.push(id);
            next = parent_of[&id];
        }

        let cause = child_values_change_cause(work_package_id);
        let mut saved = Vec::new();
        for ancestor in chain {
            // The ancestor first, then its descendants (`[work_package] + descendants_of`),
            // which exclude the ancestor itself even in a cycle (`where.not(id:)`).
            let rows: Vec<(Id, Option<f64>, Option<f64>, bool)> = {
                let mut tx = self.tx.lock().await;
                sqlx::query_as(
                    "WITH RECURSIVE down(id) AS ( \
                         SELECT id FROM work_packages WHERE parent_id = $1 \
                         UNION \
                         SELECT w.id FROM work_packages w JOIN down d ON w.parent_id = d.id) \
                     SELECT w.id, w.estimated_hours, w.remaining_hours, \
                            COALESCE(s.excluded_from_totals, false) \
                     FROM work_packages w LEFT JOIN statuses s ON s.id = w.status_id \
                     WHERE w.id = $1 OR w.id IN (SELECT id FROM down WHERE id <> $1) \
                     ORDER BY (w.id = $1) DESC, w.id",
                )
                .bind(ancestor)
                .fetch_all(&mut **tx)
                .await
                .map_err(db_failure)?
            };
            let input = |r: &(Id, Option<f64>, Option<f64>, bool)| TotalsInput {
                estimated_hours: r.1,
                remaining_hours: r.2,
                excluded_from_totals: r.3,
            };
            let Some(own) = rows.iter().find(|r| r.0 == ancestor).map(input) else {
                continue;
            };
            let descendants: Vec<TotalsInput> =
                rows.iter().filter(|r| r.0 != ancestor).map(input).collect();
            let derived = derive_ancestor_totals(&own, &descendants);

            let changed = {
                let mut tx = self.tx.lock().await;
                sqlx::query(
                    "UPDATE work_packages SET derived_estimated_hours = $2, \
                         derived_remaining_hours = $3, derived_done_ratio = $4, \
                         lock_version = lock_version + 1, updated_at = statement_timestamp() \
                     WHERE id = $1 AND (derived_estimated_hours IS DISTINCT FROM $2 \
                         OR derived_remaining_hours IS DISTINCT FROM $3 \
                         OR derived_done_ratio IS DISTINCT FROM $4)",
                )
                .bind(ancestor)
                .bind(derived.estimated_hours)
                .bind(derived.remaining_hours)
                .bind(derived.done_ratio)
                .execute(&mut **tx)
                .await
                .map_err(db_failure)?
                .rows_affected()
                    == 1
            };
            if changed {
                self.write_journal(ancestor, user_id, JOURNAL_AGGREGATION_TIME_MINUTES, &cause)
                    .await?;
                saved.push(ancestor);
            }
        }
        Ok(saved)
    }

    /// The creator becomes a watcher of the new work package: `WorkPackages::CreateService#
    /// set_user_as_watcher` (`app/services/work_packages/create_service.rb:118-122`) runs
    /// `Services::CreateWatcher` (`lib/services/create_watcher.rb:36-49`), which adds the watcher
    /// unless the user already watches it, and only if the `Watcher` is valid
    /// (`app/models/watcher.rb`): the user is not locked (`status = 3`, `validate_active_user`)
    /// and may view the work package (`validate_user_allowed_to_watch` -> `possible_watcher?`,
    /// `view_work_packages` in the project, `lib_static/open_project/acts/watchable.rb:99-103`;
    /// `acts_as_watchable(permission: :view_work_packages)`, work_package.rb:136). An invalid
    /// watcher is skipped without an error ("We don't care if it fails here"); a database error is
    /// not, and rolls the create back. Returns whether a watcher row was written.
    ///
    /// The permission is read the way [`PgCreateScope::principal`] reads it: an admin, or a
    /// project membership whose roles grant the permission. Not ported: the `WATCHER_ADDED`
    /// notification (sent with `send_notifications: false`), non-member access to public
    /// projects, and enabled-module checks, none of which [`PgCreateScope::principal`] models.
    pub async fn add_creator_as_watcher(
        &self,
        work_package_id: Id,
        project_id: Id,
        user_id: Id,
    ) -> Result<bool, StoreFailure> {
        let mut tx = self.tx.lock().await;
        let written = sqlx::query(
            "INSERT INTO watchers (watchable_type, watchable_id, user_id) \
             SELECT 'WorkPackage', $1, u.id FROM users u \
             WHERE u.id = $3 AND u.status <> 3 \
               AND (u.admin OR EXISTS ( \
                   SELECT 1 FROM members m \
                   JOIN member_roles mr ON mr.member_id = m.id \
                   JOIN role_permissions rp ON rp.role_id = mr.role_id \
                   WHERE m.project_id = $2 AND m.user_id = u.id AND m.entity_type IS NULL \
                     AND rp.permission = 'view_work_packages')) \
               AND NOT EXISTS ( \
                   SELECT 1 FROM watchers w \
                   WHERE w.watchable_type = 'WorkPackage' AND w.watchable_id = $1 \
                     AND w.user_id = u.id)",
        )
        .bind(work_package_id)
        .bind(project_id)
        .bind(user_id)
        .execute(&mut **tx)
        .await
        .map_err(db_failure)?;
        Ok(written.rows_affected() == 1)
    }

    /// The creation journal of a work package, written on the open transaction.
    ///
    /// OpenProject: `WorkPackage` is `acts_as_journalized`
    /// (`app/models/work_package/journalized.rb`), whose `after_save :save_journals`
    /// (`lib_static/plugins/acts_as_journalized/lib/acts/journalized/save_hooks.rb:55,61-80`) calls
    /// `Journals::CreateService.new(self, User.current).call(notes: "", internal: false,
    /// cause: NoCause)`. For a work package with no journal yet that service writes
    /// (`app/services/journals/create_service.rb`):
    ///
    /// * one `work_package_journals` row (`insert_data_sql`, :389-404): every journaled column
    ///   copied from the `work_packages` row; text columns pass through
    ///   `REGEXP_REPLACE(COALESCE(col,''), '\r\n', '\n', 'g')` (`helpers.rb`
    ///   `normalize_newlines_sql`), so a NULL `description` is stored as `''`;
    /// * one `journals` row (`insert_journal_sql`, :465-507): `journable_type` `'WorkPackage'`,
    ///   `version` `COALESCE(max version, 0) + 1` (1 on create), `user_id` the acting user,
    ///   `notes` `''`, `restricted` false, `created_at` = `updated_at` = the work package's
    ///   `updated_at` (`fetch_time_sql`, :354-365, with no earlier journal), `data_id` the new
    ///   data row, `data_type` `'Journal::WorkPackageJournal'`, `cause` `{}` (`NoCause#to_hash`)
    ///   and `validity_period` `tstzrange(created_at, NULL)`.
    ///
    /// Not ported: the association rows (`attachable_journals`, `customizable_journals`,
    /// `label_journals`, `work_package_version_journals`: all empty for a work package that has
    /// no attachment, custom value, label or version, which is every work package this create
    /// writes), the `JOURNAL_CREATED` notification, and aggregation (a create has no predecessor).
    pub async fn write_creation_journal(
        &self,
        work_package_id: Id,
        user_id: Id,
    ) -> Result<(), StoreFailure> {
        let mut tx = self.tx.lock().await;
        let data_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO work_package_journals (
                type_id, project_id, subject, description, due_date, category_id, status_id,
                assigned_to_id, priority_id, version_id, author_id, done_ratio, estimated_hours,
                start_date, parent_id, responsible_id, schedule_manually, duration,
                remaining_hours, story_points,
                derived_estimated_hours, derived_remaining_hours, derived_done_ratio
            )
            SELECT
                type_id, project_id, subject,
                REGEXP_REPLACE(COALESCE(description, ''), '\r\n', E'\n', 'g'),
                due_date, category_id, status_id,
                assigned_to_id, priority_id, version_id, author_id, done_ratio, estimated_hours,
                start_date, parent_id, responsible_id, schedule_manually, duration,
                remaining_hours, story_points,
                derived_estimated_hours, derived_remaining_hours, derived_done_ratio
            FROM work_packages WHERE id = $1
            RETURNING id
            "#,
        )
        .bind(work_package_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_failure)?;
        sqlx::query(
            r#"
            INSERT INTO journals (
                journable_id, journable_type, version, user_id, notes, restricted,
                created_at, updated_at, data_id, data_type, cause, validity_period
            )
            SELECT
                wp.id, 'WorkPackage',
                COALESCE((SELECT MAX(version) FROM journals
                          WHERE journable_id = wp.id AND journable_type = 'WorkPackage'), 0) + 1,
                $2, '', false,
                wp.updated_at, wp.updated_at, $3, 'Journal::WorkPackageJournal', '{}'::jsonb,
                tstzrange(wp.updated_at, NULL)
            FROM work_packages wp WHERE wp.id = $1
            "#,
        )
        .bind(work_package_id)
        .bind(user_id)
        .bind(data_id)
        .execute(&mut **tx)
        .await
        .map_err(db_failure)?;
        Ok(())
    }

    /// The whole update on the open transaction, in OpenProject's order: the advisory lock
    /// (`OpenProject::Mutex.with_advisory_lock_transaction`, `lib/open_project/mutex.rs`'s Ruby
    /// original, `lib/open_project/mutex.rb:58-65`, lock name `mutex_on_WorkPackage_<id>`), then
    /// the lookup and authorization, then the UPDATE (`BaseServices::Write#persist`,
    /// `app/services/base_services/write.rb:35-43`), then the journal.
    ///
    /// The lock is Postgres's `pg_advisory_xact_lock` on a hash of the same lock name; it
    /// serialises writers of this program, it does not interlock with Ruby's `with_advisory_lock`
    /// gem (another hash function).
    pub async fn update_in(
        &self,
        caller: &Caller,
        id: Id,
        dto: UpdateWorkPackageDto,
        aggregation_minutes: i64,
    ) -> Result<WorkPackageRow, MutationError> {
        self.lock_work_package(id).await?;
        let existing = authorize_edit(self, caller, id).await?;
        validate_update(&existing, &dto)?;
        let user_id = self.acting_user_id(caller).await?;
        let mut row = self.update_row(id, dto).await?;
        self.write_update_journal(id, user_id, aggregation_minutes)
            .await?;
        // `touch_journable` may have advanced `updated_at` past the UPDATE's value: answer with
        // the committed timestamp, the one the journal carries.
        let mut tx = self.tx.lock().await;
        row.updated_at = sqlx::query_scalar("SELECT updated_at FROM work_packages WHERE id = $1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .map_err(db_failure)?;
        Ok(row)
    }

    async fn lock_work_package(&self, id: Id) -> Result<(), StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("mutex_on_WorkPackage_{id}"))
            .execute(&mut **tx)
            .await
            .map(|_| ())
            .map_err(db_failure)
    }

    /// `users.id` of the person the update is journaled for: `User.current` in the Ruby service
    /// (`User.execute_as user`, `service_context.rb:66-70`), a verified actor's `users` row. The
    /// anonymous caller never gets here ([`authorize_edit`] refuses it,
    /// docs/decisions/0001-journaled-edits.md §2); if one did, the update is refused rather than
    /// written without a journal.
    async fn acting_user_id(&self, caller: &Caller) -> Result<Id, StoreFailure> {
        let Caller::Actor { login } = caller else {
            return Err(StoreFailure::Other(
                "an anonymous update has no users row to journal it as".into(),
            ));
        };
        let mut tx = self.tx.lock().await;
        let found: Option<Id> = sqlx::query_scalar("SELECT id FROM users WHERE login = $1")
            .bind(login)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_failure)?;
        found.ok_or_else(|| {
            StoreFailure::Other("no users row to attribute the journal of this update to".into())
        })
    }

    /// The UPDATE of `WorkPackageRepository::update`, on the open transaction. A stale
    /// `lock_version` matches no row: `Conflict`, as the repository answers it.
    async fn update_row(
        &self,
        id: Id,
        dto: UpdateWorkPackageDto,
    ) -> Result<WorkPackageRow, StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query_as::<_, WorkPackageRow>(
            r#"
            UPDATE work_packages SET
                subject = COALESCE($1, subject),
                description = COALESCE($2, description),
                type_id = COALESCE($3, type_id),
                status_id = COALESCE($4, status_id),
                priority_id = COALESCE($5, priority_id),
                assigned_to_id = $6,
                responsible_id = $7,
                start_date = $8,
                due_date = $9,
                estimated_hours = $10,
                done_ratio = COALESCE($11, done_ratio),
                parent_id = $12,
                version_id = $13,
                category_id = $14,
                lock_version = lock_version + 1,
                updated_at = NOW()
            WHERE id = $15 AND lock_version = $16
            RETURNING id, subject, description, project_id, type_id, status_id,
                      priority_id, author_id, assigned_to_id, responsible_id,
                      start_date, due_date, estimated_hours, done_ratio,
                      parent_id, version_id, category_id, lock_version,
                      created_at, updated_at
            "#,
        )
        .bind(&dto.subject)
        .bind(&dto.description)
        .bind(dto.type_id)
        .bind(dto.status_id)
        .bind(dto.priority_id)
        .bind(dto.assigned_to_id)
        .bind(dto.responsible_id)
        .bind(dto.start_date)
        .bind(dto.due_date)
        .bind(dto.estimated_hours)
        .bind(dto.done_ratio)
        .bind(dto.parent_id)
        .bind(dto.version_id)
        .bind(dto.category_id)
        .bind(id)
        .bind(dto.lock_version)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_failure)?
        .ok_or_else(|| StoreFailure::Conflict("Work package was modified by another user".into()))
    }

    /// The journal of an update, written on the open transaction after the UPDATE.
    ///
    /// OpenProject: `after_save :save_journals`
    /// (`lib_static/plugins/acts_as_journalized/lib/acts/journalized/save_hooks.rb:55,61-80`) runs
    /// `Journals::CreateService.new(self, User.current).call(notes: "", internal: false,
    /// cause: NoCause, skip_aggregation: nil)`. With a journal already present that service does
    /// (`app/services/journals/create_service.rb`):
    ///
    /// * **Nothing changed.** The `changes` CTE (`select_changed_sql` / `changes_data_sql` /
    ///   `data_changes_condition_sql`, :425-427, :509-532, :534-551) compares every journaled
    ///   column of the work package with the last journal's data row (`IS DISTINCT FROM`; text
    ///   columns, here `description`, compare after newline normalisation and `COALESCE(.., '')`).
    ///   With no difference, and no note or cause (`only_on_changed_or_forced_condition_sql`,
    ///   `create_service/helpers.rb:54-60`), every later CTE is a no-op: no journal, no data row.
    ///   The `lock_version` and `updated_at` of the work package are not journaled columns.
    /// * **Aggregation** (`aggregatable_predecessor` / `aggregatable?`, :82-90, :595-601): the
    ///   last journal is *rewritten* instead of a new version being added when aggregation is
    ///   active (minutes > 0), its `updated_at` is within the window, it is the same user's, its
    ///   cause is empty (`only_one_or_same_cause?`: `NoCause` merges into nothing), it has no
    ///   note (`only_one_note?`, the new notes are `""`) and it has the same `restricted` flag.
    ///   Then (`cleanup_predecessor_data_sql`, `update_journal_sql`, :118-120, :437-461) the
    ///   predecessor's `work_package_journals` row is deleted, a new data row is inserted, and the
    ///   journal keeps its `id`, `version`, `created_at` and `validity_period` and gets the new
    ///   `data_id`, `updated_at = fetch_time` and `cause = {}`.
    /// * **A new version** otherwise (`insert_journal_sql`, :465-507): version
    ///   `COALESCE(max, 0) + 1`, the acting user, `created_at = updated_at = fetch_time`,
    ///   `validity_period = tstzrange(fetch_time, NULL)`, and the predecessor's validity period is
    ///   closed (`update_predecessor_sql`, :410-425): `tstzrange(lower(validity_period),
    ///   fetch_time, '[)')`.
    /// * `fetch_time` (`touch_journable_sql`, `fetch_time_sql`, `after_latest_journal_sql`,
    ///   :331-392): the work package's `updated_at`, forced to be at least one microsecond after
    ///   the last journal's `updated_at`; if the work package's own `updated_at` is not newer than
    ///   that journal it is touched to this value.
    /// * No journal at all yet (a work package that predates journalling) is a version-1 journal
    ///   with nothing to close: the `changes` comparison finds every column different.
    ///
    /// Not ported: the association rows (`attachable_journals`, `customizable_journals`,
    /// `label_journals`, `work_package_version_journals`, file links, project phases), the
    /// `JOURNAL_CREATED` notification, notes, internal/restricted notes and causes (an edit has
    /// none), and `skip_aggregation`.
    pub async fn write_update_journal(
        &self,
        work_package_id: Id,
        user_id: Id,
        aggregation_minutes: i64,
    ) -> Result<UpdateJournal, StoreFailure> {
        self.write_journal(
            work_package_id,
            user_id,
            aggregation_minutes,
            &serde_json::json!({}),
        )
        .await
    }

    /// [`PgCreateScope::write_update_journal`] for a journal with a `cause`
    /// (`Journals::CreateService#call(cause:)`, `create_service.rb:55`); `{}` is `NoCause`.
    ///
    /// What a non-empty cause changes, all from `create_service.rb`:
    ///
    /// * **The journal is written even if no journaled column differs**
    ///   (`only_on_changed_or_forced_condition_sql`, `helpers.rb:54-60`: the `changes` condition
    ///   is only added when `notes.blank? && cause.blank?`, and `CauseOfChange::Base#blank?` is
    ///   `false`, `lib_static/plugins/acts_as_journalized/lib/cause_of_change/base.rb`).
    /// * **Aggregation** also needs `only_one_or_same_cause?` (:619-621): the predecessor's cause
    ///   is empty, or equal to this cause (`Hash#==` hands a non-Hash operand to its own `==`,
    ///   which compares `to_hash`), or `cause.mergeable_into?(predecessor.cause)`. `Base#merge_into`
    ///   is `nil`, and `Journal::CausedByWorkPackageChildValuesChange` does not override it, so a
    ///   predecessor with a different non-empty cause is never aggregated into.
    /// * The rewritten journal takes this cause (`aggregate_cause`, :623-627: `merge_into` is
    ///   `nil`, so the cause itself), and a new journal stores it (`insert_journal_sql`,
    ///   `cause_sql` :583-586).
    async fn write_journal(
        &self,
        work_package_id: Id,
        user_id: Id,
        aggregation_minutes: i64,
        cause: &serde_json::Value,
    ) -> Result<UpdateJournal, StoreFailure> {
        let forced = cause.as_object().is_some_and(|c| !c.is_empty());
        let mut tx = self.tx.lock().await;

        // `max_journals`: the last journal, if any (`journable.last_journal` in Ruby).
        let last: Option<(Id, i32, Id, Id, Option<String>, bool, serde_json::Value, bool)> =
            sqlx::query_as(
                "SELECT id, version, data_id, user_id, notes, restricted, cause, \
                        updated_at >= clock_timestamp() - make_interval(mins => $2::int) \
                 FROM journals WHERE journable_id = $1 AND journable_type = 'WorkPackage' \
                 ORDER BY version DESC LIMIT 1",
            )
            .bind(work_package_id)
            .bind(i32::try_from(aggregation_minutes.max(0)).unwrap_or(i32::MAX))
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_failure)?;
        let last_id = last.as_ref().map_or(0, |l| l.0);
        let last_data_id = last.as_ref().map_or(0, |l| l.2);

        // `changes`.
        let changed: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM work_packages w
                LEFT JOIN work_package_journals d ON d.id = $2
                WHERE w.id = $1 AND (
                    w.type_id IS DISTINCT FROM d.type_id
                    OR w.project_id IS DISTINCT FROM d.project_id
                    OR w.subject IS DISTINCT FROM d.subject
                    OR w.due_date IS DISTINCT FROM d.due_date
                    OR w.category_id IS DISTINCT FROM d.category_id
                    OR w.status_id IS DISTINCT FROM d.status_id
                    OR w.assigned_to_id IS DISTINCT FROM d.assigned_to_id
                    OR w.priority_id IS DISTINCT FROM d.priority_id
                    OR w.version_id IS DISTINCT FROM d.version_id
                    OR w.author_id IS DISTINCT FROM d.author_id
                    OR w.done_ratio IS DISTINCT FROM d.done_ratio
                    OR w.estimated_hours IS DISTINCT FROM d.estimated_hours
                    OR w.start_date IS DISTINCT FROM d.start_date
                    OR w.parent_id IS DISTINCT FROM d.parent_id
                    OR w.responsible_id IS DISTINCT FROM d.responsible_id
                    OR w.schedule_manually IS DISTINCT FROM d.schedule_manually
                    OR w.duration IS DISTINCT FROM d.duration
                    OR w.remaining_hours IS DISTINCT FROM d.remaining_hours
                    OR w.story_points IS DISTINCT FROM d.story_points
                    OR w.derived_estimated_hours IS DISTINCT FROM d.derived_estimated_hours
                    OR w.derived_remaining_hours IS DISTINCT FROM d.derived_remaining_hours
                    OR w.derived_done_ratio IS DISTINCT FROM d.derived_done_ratio
                    OR REGEXP_REPLACE(COALESCE(w.description, ''), '\r\n', E'\n', 'g')
                       != REGEXP_REPLACE(COALESCE(d.description, ''), '\r\n', E'\n', 'g')
                )
            )
            "#,
        )
        .bind(work_package_id)
        .bind(last_data_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_failure)?;
        if !changed && !forced {
            return Ok(UpdateJournal::Unchanged);
        }

        // `aggregatable?` for notes `""`, internal false, `NoCause`.
        let aggregate = last.as_ref().is_some_and(
            |(_, _, _, last_user, notes, restricted, last_cause, within)| {
                aggregation_minutes > 0
                    && *within
                    && *last_user == user_id
                    && (last_cause.as_object().is_none_or(|c| c.is_empty())
                        || last_cause == cause)
                    && notes.as_deref().is_none_or(str::is_empty)
                    && !*restricted
            },
        );

        // `cleanup_predecessor_data`.
        if aggregate {
            sqlx::query("DELETE FROM work_package_journals WHERE id = $1")
                .bind(last_data_id)
                .execute(&mut **tx)
                .await
                .map_err(db_failure)?;
        }

        // `touch_journable` (only when the work package is not already newer than the last
        // journal; with no journal the comparison is NULL and nothing is touched).
        sqlx::query(
            "UPDATE work_packages SET updated_at = GREATEST(statement_timestamp(), \
                 (SELECT updated_at FROM journals WHERE id = $2) + interval '1 microsecond') \
             WHERE id = $1 AND NOT updated_at > (SELECT updated_at FROM journals WHERE id = $2)",
        )
        .bind(work_package_id)
        .bind(last_id)
        .execute(&mut **tx)
        .await
        .map_err(db_failure)?;

        // `fetch_time`.
        let fetch_time: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT GREATEST((SELECT updated_at FROM work_packages WHERE id = $1), \
                 (SELECT updated_at FROM journals WHERE id = $2) + interval '1 microsecond')",
        )
        .bind(work_package_id)
        .bind(last_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_failure)?;

        // `insert_data`.
        let data_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO work_package_journals (
                type_id, project_id, subject, description, due_date, category_id, status_id,
                assigned_to_id, priority_id, version_id, author_id, done_ratio, estimated_hours,
                start_date, parent_id, responsible_id, schedule_manually, duration,
                remaining_hours, story_points,
                derived_estimated_hours, derived_remaining_hours, derived_done_ratio
            )
            SELECT
                type_id, project_id, subject,
                REGEXP_REPLACE(COALESCE(description, ''), '\r\n', E'\n', 'g'),
                due_date, category_id, status_id,
                assigned_to_id, priority_id, version_id, author_id, done_ratio, estimated_hours,
                start_date, parent_id, responsible_id, schedule_manually, duration,
                remaining_hours, story_points,
                derived_estimated_hours, derived_remaining_hours, derived_done_ratio
            FROM work_packages WHERE id = $1
            RETURNING id
            "#,
        )
        .bind(work_package_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_failure)?;

        if aggregate {
            // `update_journal_sql`: the predecessor is rewritten in place.
            sqlx::query(
                "UPDATE journals SET updated_at = $2, data_id = $3, cause = $4 \
                 WHERE id = $1",
            )
            .bind(last_id)
            .bind(fetch_time)
            .bind(data_id)
            .bind(cause)
            .execute(&mut **tx)
            .await
            .map_err(db_failure)?;
            return Ok(UpdateJournal::Aggregated);
        }

        // `update_predecessor`: close the last journal's validity period.
        sqlx::query(
            "UPDATE journals SET validity_period = tstzrange(lower(validity_period), $2, '[)') \
             WHERE id = $1",
        )
        .bind(last_id)
        .bind(fetch_time)
        .execute(&mut **tx)
        .await
        .map_err(db_failure)?;
        // `insert_journal_sql`.
        let next_version = last.as_ref().map_or(0, |l| l.1) + 1;
        sqlx::query(
            r#"
            INSERT INTO journals (
                journable_id, journable_type, version, user_id, notes, restricted,
                created_at, updated_at, data_id, data_type, cause, validity_period
            ) VALUES (
                $1, 'WorkPackage', $2, $3, '', false,
                $4, $4, $5, 'Journal::WorkPackageJournal', $6,
                tstzrange($4, NULL)
            )
            "#,
        )
        .bind(work_package_id)
        .bind(next_version)
        .bind(user_id)
        .bind(fetch_time)
        .bind(data_id)
        .bind(cause)
        .execute(&mut **tx)
        .await
        .map_err(db_failure)?;
        Ok(UpdateJournal::Created {
            version: next_version,
        })
    }

    /// Run a statement on the open transaction (the hook after the insert and the journal).
    pub async fn execute(&self, sql: &str) -> Result<(), StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query(sql)
            .execute(&mut **tx)
            .await
            .map(|_| ())
            .map_err(db_failure)
    }
}

impl MutationStore for PgCreateScope {
    async fn find_wp(&self, id: Id) -> Result<Option<WorkPackageRow>, StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query_as::<_, WorkPackageRow>(
            r#"
            SELECT id, subject, description, project_id, type_id, status_id,
                   priority_id, author_id, assigned_to_id, responsible_id,
                   start_date, due_date, estimated_hours, done_ratio,
                   parent_id, version_id, category_id, lock_version,
                   created_at, updated_at
            FROM work_packages
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_failure)
    }

    async fn project_active(&self, project_id: Id) -> Result<bool, StoreFailure> {
        let mut tx = self.tx.lock().await;
        let active: Option<bool> = sqlx::query_scalar("SELECT active FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_failure)?;
        Ok(active.unwrap_or(false))
    }

    async fn principal(&self, caller: &Caller, project_id: Id) -> Result<Principal, StoreFailure> {
        let login = match caller {
            Caller::Anonymous { allowed } => return Ok(Principal::Anonymous { allowed: *allowed }),
            Caller::Actor { login } => login,
        };
        let mut tx = self.tx.lock().await;
        let user: Option<(i64, bool, i32)> =
            sqlx::query_as("SELECT id, admin, status FROM users WHERE login = $1")
                .bind(login)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db_failure)?;
        let Some((user_id, admin, status)) = user else {
            return Ok(Principal::Unprovisioned);
        };
        let permissions: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT rp.permission FROM members m \
             JOIN member_roles mr ON mr.member_id = m.id \
             JOIN role_permissions rp ON rp.role_id = mr.role_id \
             WHERE m.project_id = $1 AND m.user_id = $2 AND m.entity_type IS NULL",
        )
        .bind(project_id)
        .bind(user_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(db_failure)?;
        Ok(Principal::User {
            admin,
            active: status == 1,
            permissions: permissions.into_iter().collect(),
        })
    }

    /// Already inside the unit of work.
    async fn update_unit_of_work(
        &self,
        caller: &Caller,
        id: Id,
        dto: UpdateWorkPackageDto,
    ) -> Result<WorkPackageRow, MutationError> {
        self.update_in(caller, id, dto, JOURNAL_AGGREGATION_TIME_MINUTES)
            .await
    }
}

impl CreationStore for PgCreateScope {
    async fn author_id(&self, caller: &Caller) -> Result<Option<Id>, StoreFailure> {
        let Caller::Actor { login } = caller else {
            return Ok(None);
        };
        let mut tx = self.tx.lock().await;
        sqlx::query_scalar("SELECT id FROM users WHERE login = $1")
            .bind(login)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_failure)
    }

    async fn first_enabled_type(&self, project_id: Id) -> Result<Option<Id>, StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query_scalar(
            "SELECT t.id FROM types t INNER JOIN projects_types pt ON pt.type_id = t.id \
             WHERE pt.project_id = $1 ORDER BY t.position ASC LIMIT 1",
        )
        .bind(project_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_failure)
    }

    async fn default_status(&self) -> Result<Option<Id>, StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query_scalar("SELECT id FROM statuses WHERE is_default = true LIMIT 1")
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_failure)
    }

    async fn default_priority(&self) -> Result<Option<Id>, StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query_scalar(
            "SELECT id FROM enumerations WHERE type = 'IssuePriority' AND active = true \
             AND is_default = true ORDER BY position ASC, id ASC LIMIT 1",
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_failure)
    }

    async fn status_is_closed(&self, status_id: Id) -> Result<Option<bool>, StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query_scalar("SELECT is_closed FROM statuses WHERE id = $1")
            .bind(status_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_failure)
    }

    async fn insert_wp(&self, dto: CreateWorkPackageDto) -> Result<WorkPackageRow, StoreFailure> {
        let mut tx = self.tx.lock().await;
        sqlx::query_as::<_, WorkPackageRow>(
            r#"
            INSERT INTO work_packages (
                subject, description, project_id, type_id, status_id,
                priority_id, author_id, assigned_to_id, responsible_id,
                start_date, due_date, estimated_hours, done_ratio,
                parent_id, version_id, category_id, lock_version,
                created_at, updated_at, remaining_hours
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, 0, NOW(), NOW(), $17
            )
            RETURNING id, subject, description, project_id, type_id, status_id,
                      priority_id, author_id, assigned_to_id, responsible_id,
                      start_date, due_date, estimated_hours, done_ratio,
                      parent_id, version_id, category_id, lock_version,
                      created_at, updated_at
            "#,
        )
        .bind(&dto.subject)
        .bind(&dto.description)
        .bind(dto.project_id)
        .bind(dto.type_id)
        .bind(dto.status_id)
        .bind(dto.priority_id)
        .bind(dto.author_id)
        .bind(dto.assigned_to_id)
        .bind(dto.responsible_id)
        .bind(dto.start_date)
        .bind(dto.due_date)
        .bind(dto.estimated_hours)
        .bind(dto.done_ratio)
        .bind(dto.parent_id)
        .bind(dto.version_id)
        .bind(dto.category_id)
        .bind(dto.remaining_hours)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_failure)
    }

    /// Already inside the unit of work.
    async fn create_unit_of_work(
        &self,
        caller: &Caller,
        params: WorkPackageParams,
    ) -> Result<WorkPackageRow, MutationError> {
        create_in(self, caller, params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    const USER_ID: Id = 77;
    const MEMBER_PROJECT: Id = 5;
    const OTHER_PROJECT: Id = 1;

    fn row(id: Id, project_id: Id) -> WorkPackageRow {
        WorkPackageRow {
            id,
            subject: "s".into(),
            description: None,
            project_id,
            type_id: 1,
            status_id: 1,
            priority_id: Some(1),
            author_id: 1,
            assigned_to_id: None,
            responsible_id: None,
            start_date: None,
            due_date: None,
            estimated_hours: None,
            done_ratio: 0,
            parent_id: None,
            version_id: None,
            category_id: None,
            lock_version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// A store that answers from plain data and records every write and every lookup.
    struct Fake {
        wps: HashMap<Id, WorkPackageRow>,
        /// project id -> active.
        projects: HashMap<Id, bool>,
        /// The signed-in person's permissions per project.
        memberships: HashMap<Id, HashSet<String>>,
        admin: bool,
        provisioned: bool,
        enabled_types: HashMap<Id, Vec<Id>>,
        default_status: Option<Id>,
        default_priority: Option<Id>,
        closed: HashMap<Id, bool>,
        inserts: Mutex<Vec<CreateWorkPackageDto>>,
        updates: Mutex<Vec<(Id, UpdateWorkPackageDto)>>,
        deletes: Mutex<Vec<Id>>,
        find_calls: AtomicUsize,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                wps: HashMap::from([
                    (100, row(100, MEMBER_PROJECT)),
                    (200, row(200, OTHER_PROJECT)),
                ]),
                projects: HashMap::from([(MEMBER_PROJECT, true), (OTHER_PROJECT, true)]),
                memberships: HashMap::new(),
                admin: false,
                provisioned: true,
                enabled_types: HashMap::from([
                    (MEMBER_PROJECT, vec![7, 9]),
                    (OTHER_PROJECT, vec![8]),
                ]),
                default_status: Some(3),
                default_priority: Some(5),
                closed: HashMap::from([(2, false), (4, true)]),
                inserts: Mutex::new(vec![]),
                updates: Mutex::new(vec![]),
                deletes: Mutex::new(vec![]),
                find_calls: AtomicUsize::new(0),
            }
        }
        fn member(mut self, project: Id, perms: &[&str]) -> Self {
            self.memberships
                .insert(project, perms.iter().map(|p| (*p).to_string()).collect());
            self
        }
        fn writes(&self) -> usize {
            self.inserts.lock().unwrap().len()
                + self.updates.lock().unwrap().len()
                + self.deletes.lock().unwrap().len()
        }
    }

    impl MutationStore for Fake {
        async fn find_wp(&self, id: Id) -> Result<Option<WorkPackageRow>, StoreFailure> {
            self.find_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.wps.get(&id).cloned())
        }
        async fn project_active(&self, project_id: Id) -> Result<bool, StoreFailure> {
            Ok(self.projects.get(&project_id).copied().unwrap_or(false))
        }
        async fn principal(
            &self,
            caller: &Caller,
            project_id: Id,
        ) -> Result<Principal, StoreFailure> {
            Ok(match caller {
                Caller::Anonymous { allowed } => Principal::Anonymous { allowed: *allowed },
                Caller::Actor { .. } if !self.provisioned => Principal::Unprovisioned,
                Caller::Actor { .. } => Principal::User {
                    admin: self.admin,
                    active: true,
                    permissions: self
                        .memberships
                        .get(&project_id)
                        .cloned()
                        .unwrap_or_default(),
                },
            })
        }
        async fn update_unit_of_work(
            &self,
            _caller: &Caller,
            id: Id,
            dto: UpdateWorkPackageDto,
        ) -> Result<WorkPackageRow, MutationError> {
            self.updates.lock().unwrap().push((id, dto));
            Ok(self.wps[&id].clone())
        }
    }

    impl DeleteStore for Fake {
        async fn delete_wp(&self, id: Id) -> Result<(), StoreFailure> {
            self.deletes.lock().unwrap().push(id);
            Ok(())
        }
    }

    impl CreationStore for Fake {
        async fn author_id(&self, caller: &Caller) -> Result<Option<Id>, StoreFailure> {
            Ok(match caller {
                Caller::Actor { .. } if self.provisioned => Some(USER_ID),
                _ => None,
            })
        }
        async fn first_enabled_type(&self, project_id: Id) -> Result<Option<Id>, StoreFailure> {
            Ok(self
                .enabled_types
                .get(&project_id)
                .and_then(|t| t.first().copied()))
        }
        async fn default_status(&self) -> Result<Option<Id>, StoreFailure> {
            Ok(self.default_status)
        }
        async fn default_priority(&self) -> Result<Option<Id>, StoreFailure> {
            Ok(self.default_priority)
        }
        async fn status_is_closed(&self, status_id: Id) -> Result<Option<bool>, StoreFailure> {
            Ok(self.closed.get(&status_id).copied())
        }
        async fn insert_wp(
            &self,
            dto: CreateWorkPackageDto,
        ) -> Result<WorkPackageRow, StoreFailure> {
            let mut r = row(500, dto.project_id);
            r.subject = dto.subject.clone();
            r.type_id = dto.type_id;
            r.status_id = dto.status_id;
            r.author_id = dto.author_id;
            self.inserts.lock().unwrap().push(dto);
            Ok(r)
        }
        async fn create_unit_of_work(
            &self,
            caller: &Caller,
            params: WorkPackageParams,
        ) -> Result<WorkPackageRow, MutationError> {
            create_in(self, caller, params).await
        }
    }

    fn alice() -> Caller {
        Caller::Actor {
            login: "oidc:alice".into(),
        }
    }

    fn new_wp(project: Option<Id>) -> WorkPackageParams {
        let mut p = WorkPackageParams::new().with_subject("A task");
        p.project_id = project;
        p
    }

    // ── anonymous: refused before the store is touched ───────────────

    #[tokio::test]
    async fn an_anonymous_caller_is_refused_for_create_update_and_delete_without_a_lookup() {
        // The store would allow everything: the refusal is the boundary's own.
        let mut store = Fake::new().member(
            MEMBER_PROJECT,
            &[ADD_WORK_PACKAGES, EDIT_WORK_PACKAGES, DELETE_WORK_PACKAGES],
        );
        store.admin = true;
        let anon = Caller::anonymous_refused();
        assert!(matches!(
            create(&store, &anon, new_wp(Some(MEMBER_PROJECT))).await,
            Err(MutationError::Unauthenticated)
        ));
        assert!(matches!(
            update(&store, &anon, 100, UpdateWorkPackageDto::default()).await,
            Err(MutationError::Unauthenticated)
        ));
        assert!(matches!(
            delete(&store, &anon, 100).await,
            Err(MutationError::Unauthenticated)
        ));
        // Not even for a record that does not exist: no existence leak to anonymous callers.
        assert!(matches!(
            update(&store, &anon, 99999, UpdateWorkPackageDto::default()).await,
            Err(MutationError::Unauthenticated)
        ));
        assert_eq!(
            store.find_calls.load(Ordering::SeqCst),
            0,
            "no lookup for an anonymous caller"
        );
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn the_demo_anonymous_opt_in_neither_edits_nor_creates() {
        // docs/decisions/0001-journaled-edits.md §2: no anonymous edit, demo posture or not.
        let store = Fake::new();
        let demo = Caller::Anonymous { allowed: true };
        assert!(matches!(
            update(&store, &demo, 100, UpdateWorkPackageDto::default()).await,
            Err(MutationError::Unauthenticated)
        ));
        assert_eq!(
            store.find_calls.load(Ordering::SeqCst),
            0,
            "refused before any lookup"
        );
        assert!(matches!(
            create(&store, &demo, new_wp(Some(MEMBER_PROJECT))).await,
            Err(MutationError::Unauthenticated)
        ));
        assert!(store.inserts.lock().unwrap().is_empty());
    }

    // ── create ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn an_authorized_member_creates_in_the_submitted_project_with_trusted_defaults() {
        let store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        let created = create(&store, &alice(), new_wp(Some(MEMBER_PROJECT)))
            .await
            .unwrap();
        assert_eq!(created.project_id, MEMBER_PROJECT);
        let inserts = store.inserts.lock().unwrap();
        assert_eq!(inserts.len(), 1);
        let dto = &inserts[0];
        assert_eq!(
            dto.project_id, MEMBER_PROJECT,
            "the project admitted is the project stored"
        );
        assert_eq!(
            dto.type_id, 7,
            "the project's first enabled type (by position), not 1"
        );
        assert_eq!(
            dto.status_id, 3,
            "Status.default read from the database, not 1"
        );
        assert_eq!(dto.author_id, USER_ID, "the author is the caller");
        assert_eq!(
            dto.priority_id,
            Some(5),
            "IssuePriority.active.default read from the database"
        );
    }

    #[tokio::test]
    async fn a_submitted_type_and_status_are_kept_and_the_defaults_are_not_consulted() {
        let store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        let mut p = new_wp(Some(MEMBER_PROJECT));
        p.type_id = Some(9);
        p.status_id = Some(2);
        create(&store, &alice(), p).await.unwrap();
        let inserts = store.inserts.lock().unwrap();
        assert_eq!((inserts[0].type_id, inserts[0].status_id), (9, 2));
    }

    #[tokio::test]
    async fn a_member_without_permission_in_project_1_cannot_create_there() {
        // Holds the permission in project 5 only.
        let store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        let r = create(&store, &alice(), new_wp(Some(OTHER_PROJECT))).await;
        assert!(matches!(r, Err(MutationError::Forbidden)), "{r:?}");
        assert_eq!(store.writes(), 0);
        // The same person still creates in project 5.
        assert!(create(&store, &alice(), new_wp(Some(MEMBER_PROJECT)))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn a_missing_or_archived_project_is_forbidden_exactly_like_a_foreign_one() {
        let mut store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        store.projects.insert(MEMBER_PROJECT, false); // archived
        store.admin = true; // not even an admin may create in an archived project
        for project in [MEMBER_PROJECT, 424242] {
            let r = create(&store, &alice(), new_wp(Some(project))).await;
            assert!(
                matches!(r, Err(MutationError::Forbidden)),
                "project {project}: {r:?}"
            );
        }
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn no_project_is_project_blank_and_nothing_is_stored() {
        let store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        match create(&store, &alice(), new_wp(None)).await {
            Err(MutationError::Invalid(e)) => {
                assert_eq!(
                    e.get("project").map(Vec::as_slice),
                    Some(&["blank".to_string()][..])
                );
            }
            other => panic!("expected project: blank, got {other:?}"),
        }
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn a_signed_in_person_without_a_users_row_cannot_create_or_edit() {
        let mut store =
            Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES, EDIT_WORK_PACKAGES]);
        store.provisioned = false;
        assert!(matches!(
            create(&store, &alice(), new_wp(Some(MEMBER_PROJECT))).await,
            Err(MutationError::Forbidden)
        ));
        assert!(matches!(
            update(&store, &alice(), 100, UpdateWorkPackageDto::default()).await,
            Err(MutationError::Forbidden)
        ));
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn a_project_without_an_enabled_type_is_a_typed_refusal_not_type_1() {
        let mut store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        store.enabled_types.clear();
        let r = create(&store, &alice(), new_wp(Some(MEMBER_PROJECT))).await;
        assert!(
            matches!(&r, Err(MutationError::Unresolved(m)) if m.starts_with("type_default_unresolved")),
            "{r:?}"
        );
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn no_default_status_is_a_typed_refusal_not_status_1() {
        let mut store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        store.default_status = None;
        let r = create(&store, &alice(), new_wp(Some(MEMBER_PROJECT))).await;
        assert!(
            matches!(&r, Err(MutationError::Unresolved(m)) if m.starts_with("status_default_unresolved")),
            "{r:?}"
        );
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn no_default_priority_is_a_blank_validation_error_not_a_null_insert() {
        let mut store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        store.default_priority = None;
        let r = create(&store, &alice(), new_wp(Some(MEMBER_PROJECT))).await;
        assert!(matches!(&r, Err(MutationError::Invalid(_))), "{r:?}");
        assert_eq!(store.writes(), 0);
        let mut submitted = new_wp(Some(MEMBER_PROJECT));
        submitted.priority_id = Some(8);
        let ok = create(&store, &alice(), submitted).await;
        assert!(ok.is_ok(), "a submitted priority needs no default: {ok:?}");
        assert_eq!(store.inserts.lock().unwrap()[0].priority_id, Some(8));
    }

    #[tokio::test]
    async fn a_submitted_status_is_judged_with_facts_looked_up_here_and_an_unknown_one_is_refused()
    {
        let store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        let mut open = new_wp(Some(MEMBER_PROJECT));
        open.status_id = Some(2);
        assert!(
            create(&store, &alice(), open).await.is_ok(),
            "an open status resolves the guard"
        );
        let mut unknown = new_wp(Some(MEMBER_PROJECT));
        unknown.status_id = Some(31337);
        let r = create(&store, &alice(), unknown).await;
        assert!(
            matches!(&r, Err(MutationError::Unresolved(m)) if m.starts_with("status_unknown")),
            "{r:?}"
        );
        assert_eq!(
            store.inserts.lock().unwrap().len(),
            1,
            "only the open-status create was stored"
        );
    }

    #[tokio::test]
    async fn an_attribute_the_member_may_not_write_is_a_422_not_a_silent_drop() {
        // `target_versions` needs `assign_versions`, which this member lacks: the contract's
        // read-only error comes back as a 422, and nothing is stored.
        let store = Fake::new().member(MEMBER_PROJECT, &[ADD_WORK_PACKAGES]);
        let mut p = new_wp(Some(MEMBER_PROJECT));
        p.version_id = Some(3);
        match create(&store, &alice(), p).await {
            Err(MutationError::Invalid(e)) => {
                assert_eq!(
                    e.get("target_versions").map(Vec::as_slice),
                    Some(&["error_readonly".to_string()][..])
                );
            }
            other => panic!("expected target_versions: error_readonly, got {other:?}"),
        }
        assert_eq!(store.writes(), 0);
    }

    // ── update ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn an_editor_of_the_records_project_updates() {
        let store = Fake::new().member(MEMBER_PROJECT, &[EDIT_WORK_PACKAGES]);
        let dto = UpdateWorkPackageDto {
            subject: Some("new".into()),
            ..Default::default()
        };
        assert!(update(&store, &alice(), 100, dto).await.is_ok());
        let updates = store.updates.lock().unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, 100);
    }

    #[tokio::test]
    async fn an_edit_of_a_work_package_without_a_priority_needs_one() {
        // `validates :priority, presence: true` (work_package/validations.rb:34) via
        // ModelContract#valid?: refused with `priority: blank`, nothing written.
        let mut store = Fake::new().member(MEMBER_PROJECT, &[EDIT_WORK_PACKAGES]);
        store.wps.get_mut(&100).unwrap().priority_id = None;
        let subject_only = UpdateWorkPackageDto {
            subject: Some("new".into()),
            ..Default::default()
        };
        match update(&store, &alice(), 100, subject_only).await {
            Err(MutationError::Invalid(errors)) => {
                assert_eq!(errors.get("priority"), Some(&vec!["blank".to_string()]))
            }
            r => panic!("expected priority: blank, got {r:?}"),
        }
        assert_eq!(store.writes(), 0, "nothing written");
        // The same edit that sets a priority is accepted.
        let with_priority = UpdateWorkPackageDto {
            subject: Some("new".into()),
            priority_id: Some(8),
            ..Default::default()
        };
        assert!(update(&store, &alice(), 100, with_priority).await.is_ok());
        assert_eq!(store.writes(), 1);
    }

    #[tokio::test]
    async fn an_update_without_edit_permission_on_the_records_project_is_refused_and_not_written() {
        // Edits in project 5, but record 200 lives in project 1.
        let store = Fake::new().member(MEMBER_PROJECT, &[EDIT_WORK_PACKAGES]);
        let r = update(&store, &alice(), 200, UpdateWorkPackageDto::default()).await;
        assert!(matches!(r, Err(MutationError::Forbidden)), "{r:?}");
        // A viewer role in the right project is no better.
        let viewer = Fake::new().member(MEMBER_PROJECT, &["view_work_packages"]);
        let r = update(&viewer, &alice(), 100, UpdateWorkPackageDto::default()).await;
        assert!(matches!(r, Err(MutationError::Forbidden)), "{r:?}");
        assert_eq!(
            store.updates.lock().unwrap().len() + viewer.updates.lock().unwrap().len(),
            0
        );
    }

    #[tokio::test]
    async fn an_unknown_record_is_not_found_and_a_conflict_is_a_conflict() {
        let store = Fake::new().member(MEMBER_PROJECT, &[EDIT_WORK_PACKAGES]);
        assert!(matches!(
            update(&store, &alice(), 99999, UpdateWorkPackageDto::default()).await,
            Err(MutationError::NotFound)
        ));
        let c: MutationError = StoreFailure::Conflict("stale".into()).into();
        assert!(matches!(c, MutationError::Conflict(_)));
        let n: MutationError = StoreFailure::NotFound("gone".into()).into();
        assert!(matches!(n, MutationError::NotFound));
        let o: MutationError = StoreFailure::Other("db".into()).into();
        assert!(matches!(o, MutationError::Store(_)));
    }

    #[tokio::test]
    async fn the_edit_permission_does_not_authorize_a_delete_and_vice_versa() {
        let store = Fake::new().member(MEMBER_PROJECT, &[EDIT_WORK_PACKAGES]);
        assert!(matches!(
            delete(&store, &alice(), 100).await,
            Err(MutationError::Forbidden)
        ));
        let deleter = Fake::new().member(MEMBER_PROJECT, &[DELETE_WORK_PACKAGES]);
        assert!(matches!(
            update(&deleter, &alice(), 100, UpdateWorkPackageDto::default()).await,
            Err(MutationError::Forbidden)
        ));
        assert_eq!(store.writes() + deleter.writes(), 0);
    }

    // ── delete ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_delete_without_delete_permission_on_the_records_project_is_refused_and_not_written()
    {
        let store = Fake::new().member(MEMBER_PROJECT, &[DELETE_WORK_PACKAGES]);
        // Record 200 is in project 1, where this person holds nothing.
        assert!(matches!(
            delete(&store, &alice(), 200).await,
            Err(MutationError::Forbidden)
        ));
        assert!(store.deletes.lock().unwrap().is_empty());
        // In their own project it works.
        assert!(delete(&store, &alice(), 100).await.is_ok());
        assert_eq!(*store.deletes.lock().unwrap(), vec![100]);
        assert!(matches!(
            delete(&store, &alice(), 99999).await,
            Err(MutationError::NotFound)
        ));
    }

    #[tokio::test]
    async fn an_admin_edits_and_deletes_in_an_active_project_only() {
        let mut store = Fake::new();
        store.admin = true;
        assert!(
            update(&store, &alice(), 100, UpdateWorkPackageDto::default())
                .await
                .is_ok()
        );
        assert!(delete(&store, &alice(), 100).await.is_ok());
        store.projects.insert(MEMBER_PROJECT, false);
        assert!(matches!(
            update(&store, &alice(), 100, UpdateWorkPackageDto::default()).await,
            Err(MutationError::Forbidden)
        ));
        assert!(matches!(
            delete(&store, &alice(), 100).await,
            Err(MutationError::Forbidden)
        ));
    }

    // ── ancestor totals (pure part) ──────────────────────────────────

    fn input(est: Option<f64>, rem: Option<f64>, excluded: bool) -> TotalsInput {
        TotalsInput {
            estimated_hours: est,
            remaining_hours: rem,
            excluded_from_totals: excluded,
        }
    }

    #[test]
    fn a_work_package_without_descendants_derives_nothing() {
        let d = derive_ancestor_totals(&input(Some(8.0), Some(4.0), false), &[]);
        assert_eq!(
            d,
            DerivedTotals {
                estimated_hours: None,
                remaining_hours: None,
                done_ratio: None
            }
        );
    }

    #[test]
    fn totals_sum_the_ancestor_and_its_included_descendants() {
        // 10 + 5 (+ 99 excluded) = 15 estimated; 4 + 1 = 5 remaining; (15 - 5) / 15 * 100 = 66.67.
        let d = derive_ancestor_totals(
            &input(Some(10.0), Some(4.0), false),
            &[
                input(Some(5.0), Some(1.0), false),
                input(Some(99.0), Some(99.0), true),
            ],
        );
        assert_eq!(d.estimated_hours, Some(15.0));
        assert_eq!(d.remaining_hours, Some(5.0));
        assert_eq!(d.done_ratio, Some(67));
    }

    #[test]
    fn an_excluded_ancestor_does_not_count_itself_and_only_nulls_give_null() {
        let d = derive_ancestor_totals(
            &input(Some(10.0), Some(10.0), true),
            &[input(Some(2.0), None, false)],
        );
        assert_eq!(
            d.estimated_hours,
            Some(2.0),
            "the excluded ancestor is left out"
        );
        assert_eq!(d.remaining_hours, None, "no non-null remaining value: nil");
        assert_eq!(d.done_ratio, None, "a nil total gives no done ratio");
        // An excluded-only subtree: descendants exist (`no_children?` is false) but no value counts.
        let d = derive_ancestor_totals(
            &input(None, None, false),
            &[input(Some(3.0), Some(1.0), true)],
        );
        assert_eq!(
            (d.estimated_hours, d.remaining_hours, d.done_ratio),
            (None, None, None)
        );
    }

    #[test]
    fn the_done_ratio_rounds_half_away_from_zero_like_ruby() {
        // 0.5 -> 1 (round-half-even would give 0), 1.5 -> 2, 2.5 -> 3, and -0.5 -> -1.
        assert_eq!(derive_done_ratio(Some(200.0), Some(199.0)), Some(1));
        assert_eq!(derive_done_ratio(Some(200.0), Some(197.0)), Some(2));
        assert_eq!(derive_done_ratio(Some(200.0), Some(195.0)), Some(3));
        assert_eq!(derive_done_ratio(Some(200.0), Some(201.0)), Some(-1));
        assert_eq!(
            derive_done_ratio(Some(0.0), Some(0.0)),
            None,
            "zero estimate: nil"
        );
        assert_eq!(derive_done_ratio(None, Some(1.0)), None);
        assert_eq!(derive_done_ratio(Some(1.0), None), None);
    }

    #[test]
    fn the_float_sum_is_compensated_like_rubys_array_sum() {
        // `[0.1, 0.2, 0.3].sum` is 0.6 in Ruby (Kahan-Babuska); a plain fold gives 0.6000000000000001.
        assert_eq!(ruby_float_sum(&[0.1, 0.2, 0.3]), 0.6);
        assert_ne!([0.1_f64, 0.2, 0.3].iter().sum::<f64>(), 0.6);
        assert_eq!(ruby_float_sum(&[]), 0.0);
    }

    #[test]
    fn the_cause_is_the_children_changed_values_hash() {
        assert_eq!(
            child_values_change_cause(42),
            serde_json::json!({"type": "work_package_children_changed_values", "work_package_id": 42})
        );
    }

    #[test]
    fn the_authorization_rule() {
        let user = |admin: bool, active: bool, perms: &[&str]| Principal::User {
            admin,
            active,
            permissions: perms.iter().map(|s| (*s).to_owned()).collect(),
        };
        assert!(may(
            &user(false, true, &[EDIT_WORK_PACKAGES]),
            true,
            EDIT_WORK_PACKAGES
        ));
        assert!(!may(
            &user(false, true, &["view_work_packages"]),
            true,
            EDIT_WORK_PACKAGES
        ));
        assert!(!may(&user(false, true, &[]), true, EDIT_WORK_PACKAGES));
        assert!(may(&user(true, true, &[]), true, EDIT_WORK_PACKAGES));
        assert!(!may(&user(true, false, &[]), true, EDIT_WORK_PACKAGES));
        assert!(!may(
            &user(true, true, &[EDIT_WORK_PACKAGES]),
            false,
            EDIT_WORK_PACKAGES
        ));
        assert!(!may(&Principal::Unprovisioned, true, EDIT_WORK_PACKAGES));
        assert!(!may(
            &Principal::Anonymous { allowed: false },
            true,
            EDIT_WORK_PACKAGES
        ));
        assert!(may(
            &Principal::Anonymous { allowed: true },
            true,
            EDIT_WORK_PACKAGES
        ));
    }
}
