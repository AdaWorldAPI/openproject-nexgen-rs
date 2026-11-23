//! Work Package API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use op_core::error::ValidationErrors;
use op_core::traits::Id;
use op_db::work_packages::WorkPackageRow;
use op_db::{Repository, WorkPackageRepository};
use op_services::work_packages::mutation::{
    self, Caller, CreationStore, DeleteStore, MutationError, MutationStore, PgMutationStore,
};
use op_services::work_packages::WorkPackageParams;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::extractors::{AppState, AuthenticatedUser, HalResponse, Pagination};

/// GET /api/v3/work_packages
pub async fn list_work_packages(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    pagination: Pagination,
) -> ApiResult<impl IntoResponse> {
    let pool = state.pool()?;
    let repo = WorkPackageRepository::new(pool.clone());

    let rows = repo
        .find_all(pagination.page_size as i64, pagination.offset as i64)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let total = repo
        .count()
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let elements: Vec<WorkPackageResponse> = rows
        .into_iter()
        .map(|row| WorkPackageResponse {
            type_name: "WorkPackage".into(),
            id: row.id,
            subject: row.subject,
            description: row.description,
            project_id: row.project_id,
            type_id: row.type_id,
            status_id: row.status_id,
            priority_id: row.priority_id,
            author_id: row.author_id,
            assigned_to_id: row.assigned_to_id,
            start_date: row.start_date.map(|d| d.to_string()),
            due_date: row.due_date.map(|d| d.to_string()),
            estimated_hours: row.estimated_hours,
            done_ratio: row.done_ratio,
            lock_version: row.lock_version,
            created_at: row.created_at.to_rfc3339(),
            updated_at: row.updated_at.to_rfc3339(),
        })
        .collect();

    let collection = WorkPackageCollection {
        type_name: "Collection".into(),
        total: total as usize,
        count: elements.len(),
        page_size: pagination.page_size,
        offset: pagination.offset,
        elements,
    };
    Ok(HalResponse(collection))
}

/// GET /api/v3/work_packages/:id
pub async fn get_work_package(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(id): Path<Id>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.pool()?;
    let repo = WorkPackageRepository::new(pool.clone());

    let row = repo
        .find_by_id(id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| ApiError::not_found("WorkPackage", id))?;

    Ok(HalResponse(WorkPackageResponse {
        type_name: "WorkPackage".into(),
        id: row.id,
        subject: row.subject,
        description: row.description,
        project_id: row.project_id,
        type_id: row.type_id,
        status_id: row.status_id,
        priority_id: row.priority_id,
        author_id: row.author_id,
        assigned_to_id: row.assigned_to_id,
        start_date: row.start_date.map(|d| d.to_string()),
        due_date: row.due_date.map(|d| d.to_string()),
        estimated_hours: row.estimated_hours,
        done_ratio: row.done_ratio,
        lock_version: row.lock_version,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }))
}

/// POST /api/v3/work_packages
///
/// Every mutation here goes through [`op_services::work_packages::mutation`]: a verified caller,
/// project-scoped authorization, the generated create admission, then the write. Nothing in this
/// file calls a repository to create, update or delete a work package.
pub async fn create_work_package(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(dto): Json<CreateWorkPackageDto>,
) -> ApiResult<impl IntoResponse> {
    let caller = verified_caller(&user)?;
    let store = PgMutationStore {
        pool: state.pool()?.clone(),
    };
    let row = create_work_package_core(&store, &caller, dto).await?;
    Ok((
        StatusCode::CREATED,
        HalResponse(WorkPackageResponse::from(row)),
    ))
}

/// PATCH /api/v3/work_packages/:id
pub async fn update_work_package(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Id>,
    Json(dto): Json<UpdateWorkPackageDto>,
) -> ApiResult<impl IntoResponse> {
    let caller = verified_caller(&user)?;
    let store = PgMutationStore {
        pool: state.pool()?.clone(),
    };
    let row = update_work_package_core(&store, &caller, id, dto).await?;
    Ok(HalResponse(WorkPackageResponse::from(row)))
}

/// DELETE /api/v3/work_packages/:id
pub async fn delete_work_package(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Id>,
) -> ApiResult<impl IntoResponse> {
    let caller = verified_caller(&user)?;
    let store = PgMutationStore {
        pool: state.pool()?.clone(),
    };
    delete_work_package_core(&store, &caller, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The caller of a mutation, or 401 for an anonymous one.
///
/// FAIL CLOSED: this does not read `require_authentication` or any other configuration. An
/// anonymous or unverified caller never mutates, whatever the deployment's anonymous-read posture
/// is. It runs before the database pool is looked up so a deployment without a database answers
/// 401, not 500, to an anonymous write.
fn verified_caller(user: &AuthenticatedUser) -> ApiResult<Caller> {
    match user.mutation_caller() {
        Caller::Anonymous { .. } => Err(MutationError::Unauthenticated.into_api(0)),
        actor => Ok(actor),
    }
}

/// POST core, generic over the store so every guard is testable without a database.
pub async fn create_work_package_core<S: CreationStore>(
    store: &S,
    caller: &Caller,
    dto: CreateWorkPackageDto,
) -> ApiResult<WorkPackageRow> {
    // No `unwrap_or(1)`: a missing project, type or status is decided by the boundary
    // (`project: blank`; the project's first enabled type; `Status.default`), never by a literal.
    let params = WorkPackageParams {
        subject: Some(dto.subject),
        description: dto.description,
        project_id: dto.project_id,
        type_id: dto.type_id,
        status_id: dto.status_id,
        priority_id: dto.priority_id,
        assigned_to_id: dto.assigned_to_id,
        parent_id: dto.parent_id,
        estimated_hours: dto.estimated_hours,
        ..WorkPackageParams::default()
    };
    mutation::create(store, caller, params)
        .await
        .map_err(|e| e.into_api(0))
}

/// PATCH core.
pub async fn update_work_package_core<S: MutationStore>(
    store: &S,
    caller: &Caller,
    id: Id,
    dto: UpdateWorkPackageDto,
) -> ApiResult<WorkPackageRow> {
    let ticket = mutation::authorize_update(store, caller, id)
        .await
        .map_err(|e| e.into_api(id))?;
    // `WorkPackageRepository::update` writes every non-`COALESCE`d column as given, so a field
    // this body does not carry must be carried forward from the record, or it is nulled out.
    let e = ticket.existing().clone();
    let update_dto = op_db::UpdateWorkPackageDto {
        subject: dto.subject,
        description: dto.description,
        type_id: dto.type_id,
        status_id: dto.status_id,
        priority_id: dto.priority_id,
        assigned_to_id: dto.assigned_to_id.or(e.assigned_to_id),
        responsible_id: e.responsible_id,
        start_date: e.start_date,
        due_date: e.due_date,
        estimated_hours: dto.estimated_hours.or(e.estimated_hours),
        done_ratio: dto.done_ratio,
        parent_id: e.parent_id,
        version_id: e.version_id,
        category_id: e.category_id,
        lock_version: dto.lock_version,
    };
    ticket
        .commit(store, update_dto)
        .await
        .map_err(|e| e.into_api(id))
}

/// DELETE core.
pub async fn delete_work_package_core<S: DeleteStore>(
    store: &S,
    caller: &Caller,
    id: Id,
) -> ApiResult<()> {
    mutation::delete(store, caller, id)
        .await
        .map_err(|e| e.into_api(id))
}

/// How a refused or failed mutation is answered over HTTP: 401 unauthenticated, 403 forbidden,
/// 404 not found, 422 validation, 409 conflict, 500 store fault.
trait IntoApi {
    fn into_api(self, id: Id) -> ApiError;
}

impl IntoApi for MutationError {
    fn into_api(self, id: Id) -> ApiError {
        match self {
            MutationError::Unauthenticated => ApiError::unauthorized("Authentication required"),
            MutationError::Forbidden => {
                ApiError::forbidden("You are not authorized to perform this action")
            }
            MutationError::NotFound => ApiError::not_found("WorkPackage", id),
            MutationError::Invalid(errors) => ApiError::Validation(errors),
            MutationError::Unresolved(msg) => {
                let mut errors = ValidationErrors::new();
                errors.add_base(msg);
                ApiError::Validation(errors)
            }
            MutationError::Conflict(msg) => ApiError::conflict(&msg),
            MutationError::Store(msg) => {
                tracing::error!("work package mutation: store error: {msg}");
                ApiError::internal("Database error")
            }
        }
    }
}

impl From<WorkPackageRow> for WorkPackageResponse {
    fn from(row: WorkPackageRow) -> Self {
        WorkPackageResponse {
            type_name: "WorkPackage".into(),
            id: row.id,
            subject: row.subject,
            description: row.description,
            project_id: row.project_id,
            type_id: row.type_id,
            status_id: row.status_id,
            priority_id: row.priority_id,
            author_id: row.author_id,
            assigned_to_id: row.assigned_to_id,
            start_date: row.start_date.map(|d| d.to_string()),
            due_date: row.due_date.map(|d| d.to_string()),
            estimated_hours: row.estimated_hours,
            done_ratio: row.done_ratio,
            lock_version: row.lock_version,
            created_at: row.created_at.to_rfc3339(),
            updated_at: row.updated_at.to_rfc3339(),
        }
    }
}

// DTOs
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkPackageCollection {
    #[serde(rename = "_type")]
    type_name: String,
    total: usize,
    count: usize,
    page_size: usize,
    offset: usize,
    #[serde(rename = "_embedded")]
    elements: Vec<WorkPackageResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkPackageResponse {
    #[serde(rename = "_type")]
    type_name: String,
    id: Id,
    subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    project_id: Id,
    type_id: Id,
    status_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority_id: Option<Id>,
    author_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    assigned_to_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    estimated_hours: Option<f64>,
    done_ratio: i32,
    lock_version: i32,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkPackageDto {
    pub subject: String,
    #[serde(default)]
    pub description: Option<String>,
    pub project_id: Option<Id>,
    pub type_id: Option<Id>,
    pub status_id: Option<Id>,
    pub priority_id: Option<Id>,
    pub assigned_to_id: Option<Id>,
    pub parent_id: Option<Id>,
    pub estimated_hours: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkPackageDto {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub type_id: Option<Id>,
    pub status_id: Option<Id>,
    pub priority_id: Option<Id>,
    pub assigned_to_id: Option<Id>,
    pub estimated_hours: Option<f64>,
    pub done_ratio: Option<i32>,
    #[serde(default)]
    pub lock_version: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::AppConfig;
    use axum::body::Body;
    use axum::http::{header, Method, Request};
    use chrono::Utc;
    use op_db::work_packages::CreateWorkPackageDto as DbCreate;
    use op_services::work_packages::mutation::{Principal, StoreFailure};
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;
    use tower::ServiceExt;

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
            assigned_to_id: Some(9),
            responsible_id: Some(8),
            start_date: None,
            due_date: None,
            estimated_hours: Some(3.5),
            done_ratio: 0,
            parent_id: Some(2),
            version_id: None,
            category_id: None,
            lock_version: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    struct Fake {
        permissions: HashMap<Id, HashSet<String>>,
        inserts: Mutex<Vec<DbCreate>>,
        updates: Mutex<Vec<(Id, op_db::UpdateWorkPackageDto)>>,
        deletes: Mutex<Vec<Id>>,
    }

    impl Fake {
        fn member(project: Id, perms: &[&str]) -> Self {
            Fake {
                permissions: HashMap::from([(
                    project,
                    perms.iter().map(|p| (*p).to_string()).collect(),
                )]),
                inserts: Mutex::new(vec![]),
                updates: Mutex::new(vec![]),
                deletes: Mutex::new(vec![]),
            }
        }
        fn writes(&self) -> usize {
            self.inserts.lock().unwrap().len()
                + self.updates.lock().unwrap().len()
                + self.deletes.lock().unwrap().len()
        }
    }

    impl MutationStore for Fake {
        async fn find_wp(&self, id: Id) -> Result<Option<WorkPackageRow>, StoreFailure> {
            Ok(match id {
                100 => Some(row(100, MEMBER_PROJECT)),
                200 => Some(row(200, OTHER_PROJECT)),
                _ => None,
            })
        }
        async fn project_active(&self, _project_id: Id) -> Result<bool, StoreFailure> {
            Ok(true)
        }
        async fn principal(
            &self,
            caller: &Caller,
            project_id: Id,
        ) -> Result<Principal, StoreFailure> {
            Ok(match caller {
                Caller::Anonymous { allowed } => Principal::Anonymous { allowed: *allowed },
                Caller::Actor { .. } => Principal::User {
                    admin: false,
                    active: true,
                    permissions: self
                        .permissions
                        .get(&project_id)
                        .cloned()
                        .unwrap_or_default(),
                },
            })
        }
        async fn update_wp(
            &self,
            id: Id,
            dto: op_db::UpdateWorkPackageDto,
        ) -> Result<WorkPackageRow, StoreFailure> {
            self.updates.lock().unwrap().push((id, dto));
            Ok(row(id, MEMBER_PROJECT))
        }
    }

    impl DeleteStore for Fake {
        async fn delete_wp(&self, id: Id) -> Result<(), StoreFailure> {
            self.deletes.lock().unwrap().push(id);
            Ok(())
        }
    }

    impl CreationStore for Fake {
        async fn author_id(&self, _caller: &Caller) -> Result<Option<Id>, StoreFailure> {
            Ok(Some(77))
        }
        async fn first_enabled_type(&self, _project_id: Id) -> Result<Option<Id>, StoreFailure> {
            Ok(Some(42))
        }
        async fn default_status(&self) -> Result<Option<Id>, StoreFailure> {
            Ok(Some(99))
        }
        async fn status_is_closed(&self, _status_id: Id) -> Result<Option<bool>, StoreFailure> {
            Ok(Some(false))
        }
        async fn insert_wp(&self, dto: DbCreate) -> Result<WorkPackageRow, StoreFailure> {
            let mut r = row(500, dto.project_id);
            r.type_id = dto.type_id;
            r.status_id = dto.status_id;
            self.inserts.lock().unwrap().push(dto);
            Ok(r)
        }
    }

    fn alice() -> Caller {
        Caller::Actor {
            login: "oidc:alice".into(),
        }
    }

    fn create_dto(project: Option<Id>) -> CreateWorkPackageDto {
        CreateWorkPackageDto {
            subject: "A task".into(),
            description: None,
            project_id: project,
            type_id: None,
            status_id: None,
            priority_id: None,
            assigned_to_id: None,
            parent_id: None,
            estimated_hours: None,
        }
    }

    fn patch_dto() -> UpdateWorkPackageDto {
        UpdateWorkPackageDto {
            subject: Some("renamed".into()),
            description: None,
            type_id: None,
            status_id: None,
            priority_id: None,
            assigned_to_id: None,
            estimated_hours: None,
            done_ratio: None,
            lock_version: 0,
        }
    }

    fn status(e: &ApiError) -> u16 {
        e.status_code().as_u16()
    }

    // ── anonymous, at the HTTP edge, with the flag that used to open it ─

    fn app(require_authentication: bool) -> axum::Router {
        crate::routes::router().with_state(AppState {
            config: std::sync::Arc::new(AppConfig {
                require_authentication,
                ..AppConfig::default()
            }),
            db: None,
        })
    }

    async fn send(app: &axum::Router, method: Method, uri: &str, auth: bool) -> u16 {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        if auth {
            req = req.header(header::AUTHORIZATION, "Bearer anything");
        }
        let body = r#"{"subject":"x","projectId":5}"#;
        app.clone()
            .oneshot(req.body(Body::from(body)).unwrap())
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    #[tokio::test]
    async fn anonymous_api_mutations_are_refused_even_with_require_authentication_false() {
        let open = app(false);
        // Control: the flag really admits an anonymous caller to a READ (it gets as far as the
        // missing database, a 500). So the 401s below are not the flag doing its job.
        assert_eq!(
            send(&open, Method::GET, "/api/v3/work_packages/1", false).await,
            500
        );
        for (method, uri) in [
            (Method::POST, "/api/v3/work_packages"),
            (Method::PATCH, "/api/v3/work_packages/1"),
            (Method::DELETE, "/api/v3/work_packages/1"),
        ] {
            assert_eq!(
                send(&open, method.clone(), uri, false).await,
                401,
                "{method} {uri}"
            );
            // An unvalidated token is no better (token validation is not implemented).
            assert_eq!(
                send(&open, method.clone(), uri, true).await,
                401,
                "{method} {uri} + token"
            );
        }
        // And with the flag on, as before.
        assert_eq!(
            send(&app(true), Method::POST, "/api/v3/work_packages", false).await,
            401
        );
    }

    #[test]
    fn the_extractors_caller_is_anonymous_refused_for_an_anonymous_user_whatever_the_flag() {
        let anon = AuthenticatedUser(op_auth::permissions::CurrentUser::anonymous());
        assert_eq!(anon.mutation_caller(), Caller::Anonymous { allowed: false });
        assert!(verified_caller(&anon).is_err());
        let person = AuthenticatedUser(op_auth::permissions::CurrentUser::new(
            7,
            "oidc:alice",
            "a@x",
        ));
        assert_eq!(
            person.mutation_caller(),
            Caller::Actor {
                login: "oidc:alice".into()
            }
        );
    }

    // ── create ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_member_creates_in_project_5_and_the_stored_row_has_project_5() {
        let store = Fake::member(MEMBER_PROJECT, &["add_work_packages"]);
        let row = create_work_package_core(&store, &alice(), create_dto(Some(MEMBER_PROJECT)))
            .await
            .unwrap();
        assert_eq!(row.project_id, MEMBER_PROJECT);
        let inserts = store.inserts.lock().unwrap();
        assert_eq!(inserts[0].project_id, MEMBER_PROJECT);
        // No literal 1 anywhere: the type and status come from the (fake) database.
        assert_eq!((inserts[0].type_id, inserts[0].status_id), (42, 99));
        assert_eq!(inserts[0].author_id, 77);
    }

    #[tokio::test]
    async fn a_member_without_permission_in_project_1_gets_403_and_nothing_is_stored() {
        let store = Fake::member(MEMBER_PROJECT, &["add_work_packages"]);
        let e = create_work_package_core(&store, &alice(), create_dto(Some(OTHER_PROJECT)))
            .await
            .unwrap_err();
        assert_eq!(status(&e), 403);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn no_project_is_422_project_blank_and_nothing_is_stored() {
        let store = Fake::member(MEMBER_PROJECT, &["add_work_packages"]);
        let e = create_work_package_core(&store, &alice(), create_dto(None))
            .await
            .unwrap_err();
        assert_eq!(status(&e), 422);
        match e {
            ApiError::Validation(v) => assert!(v.has_error("project"), "{v:?}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn the_anonymous_caller_is_401_at_the_core_too() {
        let store = Fake::member(
            MEMBER_PROJECT,
            &[
                "add_work_packages",
                "edit_work_packages",
                "delete_work_packages",
            ],
        );
        let anon = Caller::anonymous_refused();
        assert_eq!(
            status(
                &create_work_package_core(&store, &anon, create_dto(Some(5)))
                    .await
                    .unwrap_err()
            ),
            401
        );
        assert_eq!(
            status(
                &update_work_package_core(&store, &anon, 100, patch_dto())
                    .await
                    .unwrap_err()
            ),
            401
        );
        assert_eq!(
            status(
                &delete_work_package_core(&store, &anon, 100)
                    .await
                    .unwrap_err()
            ),
            401
        );
        assert_eq!(store.writes(), 0);
    }

    // ── update / delete ──────────────────────────────────────────────

    #[tokio::test]
    async fn patch_by_a_user_without_edit_permission_on_the_records_project_is_403_and_not_written()
    {
        // Edits in project 5; record 200 is in project 1.
        let store = Fake::member(MEMBER_PROJECT, &["edit_work_packages"]);
        let e = update_work_package_core(&store, &alice(), 200, patch_dto())
            .await
            .unwrap_err();
        assert_eq!(status(&e), 403);
        assert!(store.updates.lock().unwrap().is_empty());
        // Unknown record: 404.
        let e = update_work_package_core(&store, &alice(), 99999, patch_dto())
            .await
            .unwrap_err();
        assert_eq!(status(&e), 404);
    }

    #[tokio::test]
    async fn an_authorized_patch_carries_forward_the_columns_the_body_does_not_name() {
        let store = Fake::member(MEMBER_PROJECT, &["edit_work_packages"]);
        update_work_package_core(&store, &alice(), 100, patch_dto())
            .await
            .unwrap();
        let updates = store.updates.lock().unwrap();
        let (id, dto) = &updates[0];
        assert_eq!(*id, 100);
        assert_eq!(dto.subject.as_deref(), Some("renamed"));
        // `UPDATE` does not COALESCE these; a bare `None` would null them out.
        assert_eq!(dto.assigned_to_id, Some(9));
        assert_eq!(dto.responsible_id, Some(8));
        assert_eq!(dto.parent_id, Some(2));
        assert_eq!(dto.estimated_hours, Some(3.5));
    }

    #[tokio::test]
    async fn delete_without_delete_permission_on_the_records_project_is_403_and_not_written() {
        let store = Fake::member(MEMBER_PROJECT, &["delete_work_packages"]);
        let e = delete_work_package_core(&store, &alice(), 200)
            .await
            .unwrap_err();
        assert_eq!(status(&e), 403);
        assert!(store.deletes.lock().unwrap().is_empty());
        delete_work_package_core(&store, &alice(), 100)
            .await
            .unwrap();
        assert_eq!(*store.deletes.lock().unwrap(), vec![100]);
    }

    #[test]
    fn every_mutation_error_has_its_http_status() {
        let s = |e: MutationError| status(&e.into_api(1));
        assert_eq!(s(MutationError::Unauthenticated), 401);
        assert_eq!(s(MutationError::Forbidden), 403);
        assert_eq!(s(MutationError::NotFound), 404);
        assert_eq!(s(MutationError::Invalid(ValidationErrors::new())), 422);
        assert_eq!(s(MutationError::Unresolved("x".into())), 422);
        assert_eq!(s(MutationError::Conflict("x".into())), 409);
        assert_eq!(s(MutationError::Store("x".into())), 500);
    }
}
