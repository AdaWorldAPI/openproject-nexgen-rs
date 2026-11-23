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
    async fn update_wp(
        &self,
        id: Id,
        dto: UpdateWorkPackageDto,
    ) -> Result<WorkPackageRow, StoreFailure>;
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

/// Proof that the caller may edit this record. The only way to reach [`MutationStore::update_wp`]
/// from this module; it cannot be built outside it.
#[derive(Debug)]
pub struct AuthorizedUpdate {
    existing: WorkPackageRow,
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
        Ok(store.update_wp(self.existing.id, dto).await?)
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
    let existing = authorize_record(store, caller, id, EDIT_WORK_PACKAGES).await?;
    Ok(AuthorizedUpdate { existing })
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
    let facts = CreateFacts {
        status_submitted: params.status_id.is_some(),
        status_closed,
        versions_submitted: params.version_id.is_some(),
    };

    let result = CreateWorkPackageService::without_notifications(&scoped)
        .with_facts(&facts)
        .call(params.clone());
    if result.is_failure() {
        let errors = result.errors().clone();
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

    let status_id = match params.status_id {
        Some(s) => s,
        None => store.default_status().await?.ok_or_else(|| {
            MutationError::Unresolved(
                "status_default_unresolved: no status is marked default".into(),
            )
        })?,
    };

    // The entity's own status and priority are placeholders of the service's in-memory model;
    // the row takes the resolved status and the submitted priority.
    let dto = CreateWorkPackageDto {
        subject: entity.subject,
        description: entity.description,
        project_id: entity.project_id,
        type_id: entity.type_id,
        status_id,
        priority_id: params.priority_id,
        author_id: entity.author_id,
        assigned_to_id: entity.assigned_to_id,
        responsible_id: entity.responsible_id,
        start_date: entity.start_date,
        due_date: entity.due_date,
        estimated_hours: entity.estimated_hours,
        done_ratio: entity.done_ratio,
        parent_id: entity.parent_id,
        version_id: entity.version_id,
        category_id: entity.category_id,
    };
    Ok(store.insert_wp(dto).await?)
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

    async fn update_wp(
        &self,
        id: Id,
        dto: UpdateWorkPackageDto,
    ) -> Result<WorkPackageRow, StoreFailure> {
        Ok(WorkPackageRepository::new(self.pool.clone())
            .update(id, dto)
            .await?)
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
    /// ([`PgCreateScope::write_creation_journal`]); the ancestors, rescheduling and watcher are
    /// not ported. `None` is the production call. `post_insert` exists so the
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

fn db_failure(e: sqlx::Error) -> StoreFailure {
    StoreFailure::from(RepositoryError::from(e))
}

/// A create's database scope: every read and the insert run on ONE open transaction, so the
/// authorization facts and the write are isolated together and a failure after the insert leaves
/// nothing behind. It is only ever built by [`PgMutationStore`]'s `create_unit_of_work`, which
/// commits or rolls it back.
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
                remaining_hours, story_points
            )
            SELECT
                type_id, project_id, subject,
                REGEXP_REPLACE(COALESCE(description, ''), '\r\n', E'\n', 'g'),
                due_date, category_id, status_id,
                assigned_to_id, priority_id, version_id, author_id, done_ratio, estimated_hours,
                start_date, parent_id, responsible_id, schedule_manually, duration,
                remaining_hours, story_points
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
    async fn find_wp(&self, _id: Id) -> Result<Option<WorkPackageRow>, StoreFailure> {
        Err(StoreFailure::Other(
            "a create scope does not look up work packages".into(),
        ))
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

    async fn update_wp(
        &self,
        _id: Id,
        _dto: UpdateWorkPackageDto,
    ) -> Result<WorkPackageRow, StoreFailure> {
        Err(StoreFailure::Other(
            "a create scope does not update work packages".into(),
        ))
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
                created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, 0, NOW(), NOW()
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
            priority_id: None,
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
        async fn update_wp(
            &self,
            id: Id,
            dto: UpdateWorkPackageDto,
        ) -> Result<WorkPackageRow, StoreFailure> {
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
    async fn the_demo_anonymous_opt_in_edits_but_never_creates() {
        let store = Fake::new();
        let demo = Caller::Anonymous { allowed: true };
        assert!(update(&store, &demo, 100, UpdateWorkPackageDto::default())
            .await
            .is_ok());
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
        assert_eq!(dto.priority_id, None, "no priority is invented");
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
