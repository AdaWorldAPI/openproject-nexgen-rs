//! Axum extractors for API handlers

use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts, Query},
    http::request::Parts,
};
use op_auth::permissions::CurrentUser;
use op_db::{UserRepository, UserRow};
use op_services::work_packages::mutation::Caller;
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::ApiError;

/// The answer of an [`ApiTokenVerifier`].
pub type TokenFuture<'a> = Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>>;

/// Turns a bearer token of a machine client into the login the call acts as.
///
/// `Some(login)` only for a token that was verified (signature, issuer, audience, expiry) and
/// whose subject is allowed to act as that login; `None` for every other token. The API reads
/// the login's user row afterwards, so a login without one is refused there.
pub trait ApiTokenVerifier: Send + Sync {
    fn verify<'a>(&'a self, token: &'a str) -> TokenFuture<'a>;
}

/// Application state with database pool
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: Option<PgPool>,
    /// Verifies the bearer tokens of machine clients; `None`: every `Authorization` header is
    /// refused.
    pub api_tokens: Option<Arc<dyn ApiTokenVerifier>>,
}

#[derive(Clone)]
pub struct AppConfig {
    pub api_version: String,
    pub base_url: String,
    pub require_authentication: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_version: "3".into(),
            base_url: "http://localhost:8080".into(),
            require_authentication: true,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: Arc::new(AppConfig::default()),
            db: None,
            api_tokens: None,
        }
    }
}

impl AppState {
    /// Get database pool, returns error if not configured
    pub fn pool(&self) -> Result<&PgPool, ApiError> {
        self.db
            .as_ref()
            .ok_or_else(|| ApiError::internal("Database not configured"))
    }
}

/// Authenticated user extractor
pub struct AuthenticatedUser(pub CurrentUser);

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // SECURITY (5+3 council, S2): an Authorization header used to be
        // accepted UNVALIDATED and mapped onto user id 1 — any caller sending
        // `Bearer anything` became the seeded admin identity and could rewrite
        // that row (email + credential) via the own-row update path. A presented
        // credential is REJECTED unless a configured verifier vouches for it
        // — never trusted, never mapped to a user it did not name.
        if parts.headers.get("authorization").is_some() {
            let Some(verifier) = app_state.api_tokens.clone() else {
                return Err(ApiError::unauthorized(
                    "Token validation is not implemented; do not send an Authorization header",
                ));
            };
            let token = bearer_token(&parts.headers)
                .ok_or_else(|| {
                    ApiError::unauthorized(
                        "Send exactly one `Authorization: Bearer <token>` header",
                    )
                })?
                .to_owned();
            let login = verifier
                .verify(&token)
                .await
                .ok_or_else(|| ApiError::unauthorized("The bearer token is not accepted"))?;
            let row = UserRepository::new(app_state.pool()?.clone())
                .find_by_login(&login)
                .await
                .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
            return Ok(AuthenticatedUser(current_user_of(row)?));
        }

        if !app_state.config.require_authentication {
            return Ok(AuthenticatedUser(CurrentUser::anonymous()));
        }

        Err(ApiError::unauthorized("Authentication required"))
    }
}

/// The token of exactly one `Authorization: Bearer <token>` header.
fn bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(axum::http::header::AUTHORIZATION).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return None;
    };
    let (scheme, token) = value.to_str().ok()?.split_once(' ')?;
    let plain = !token.is_empty() && !token.chars().any(|c| c.is_whitespace() || c.is_control());
    (scheme.eq_ignore_ascii_case("bearer") && plain).then_some(token)
}

/// The user a verified token acts as. A login without a user row, or with a row that is not
/// active, is refused: the token proves who the client is, not that the account may be used.
fn current_user_of(row: Option<UserRow>) -> Result<CurrentUser, ApiError> {
    let row = row
        .filter(UserRow::is_active)
        .ok_or_else(|| ApiError::unauthorized("The token acts as a user that does not exist"))?;
    Ok(if row.admin {
        CurrentUser::admin(row.id, row.login, row.mail)
    } else {
        CurrentUser::new(row.id, row.login, row.mail)
    })
}

impl AuthenticatedUser {
    /// The identity a mutation is authorized as.
    ///
    /// FAIL CLOSED: an anonymous user (what `require_authentication = false` yields) is
    /// [`Caller::Anonymous`] with `allowed: false`, whatever the configuration says. There is no
    /// path from `require_authentication`, `OP_ALLOW_ANONYMOUS` or any other flag to an anonymous
    /// caller that may write through the API. A verified person is named by their login.
    pub fn mutation_caller(&self) -> Caller {
        if self.0.is_anonymous {
            Caller::anonymous_refused()
        } else {
            Caller::Actor {
                login: self.0.login.clone(),
            }
        }
    }
}

impl std::ops::Deref for AuthenticatedUser {
    type Target = CurrentUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Pagination parameters
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationParams {
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_page_size() -> usize {
    20
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page_size: 20,
            offset: 0,
        }
    }
}

pub struct Pagination(pub PaginationParams);

#[async_trait]
impl<S> FromRequestParts<S> for Pagination
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(params) = Query::<PaginationParams>::from_request_parts(parts, state)
            .await
            .unwrap_or_else(|_| Query(PaginationParams::default()));
        Ok(Pagination(params))
    }
}

impl std::ops::Deref for Pagination {
    type Target = PaginationParams;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// HAL+JSON response wrapper
pub struct HalResponse<T: serde::Serialize>(pub T);

impl<T: serde::Serialize> axum::response::IntoResponse for HalResponse<T> {
    fn into_response(self) -> axum::response::Response {
        let json = serde_json::to_string(&self.0).unwrap_or_default();
        axum::response::Response::builder()
            .status(200)
            .header("content-type", "application/hal+json; charset=utf-8")
            .body(axum::body::Body::from(json))
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Method, Request};
    use tower::ServiceExt;

    /// Accepts exactly the token `good` and names `robot` as the login.
    struct Fake;

    impl ApiTokenVerifier for Fake {
        fn verify<'a>(&'a self, token: &'a str) -> TokenFuture<'a> {
            Box::pin(async move { (token == "good").then(|| "robot".to_owned()) })
        }
    }

    fn app(verifier: Option<Arc<dyn ApiTokenVerifier>>) -> axum::Router {
        crate::routes::router().with_state(AppState {
            config: Arc::new(AppConfig::default()),
            db: None,
            api_tokens: verifier,
        })
    }

    async fn status_of(app: &axum::Router, auth: &[&str]) -> u16 {
        let mut req = Request::builder()
            .method(Method::GET)
            .uri("/api/v3/work_packages/1");
        for value in auth {
            req = req.header(header::AUTHORIZATION, *value);
        }
        app.clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    fn user(login: &str, admin: bool, status: i32) -> UserRow {
        UserRow {
            id: 9,
            login: login.into(),
            firstname: "R".into(),
            lastname: "Obot".into(),
            mail: "robot@example.test".into(),
            admin,
            status,
            language: None,
            hashed_password: None,
            salt: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_login_on: None,
        }
    }

    /// FAILS IF: a token the verifier did not accept, a second Authorization header or a
    /// scheme other than Bearer reaches a handler instead of answering 401, or a verified
    /// token is refused before the user is looked up (500 here: the test has no database).
    #[tokio::test]
    async fn only_one_verified_bearer_token_gets_past_the_extractor() {
        let with = app(Some(Arc::new(Fake)));
        assert_eq!(status_of(&with, &["Bearer good"]).await, 500);
        assert_eq!(status_of(&with, &["bearer good"]).await, 500);
        assert_eq!(status_of(&with, &["Bearer bad"]).await, 401);
        assert_eq!(status_of(&with, &["Bearer good", "Bearer good"]).await, 401);
        assert_eq!(status_of(&with, &["Basic Z29vZDpnb29k"]).await, 401);
        assert_eq!(status_of(&with, &["good"]).await, 401);
        assert_eq!(status_of(&with, &["Bearer"]).await, 401);
        assert_eq!(status_of(&with, &["Bearer good extra"]).await, 401);
    }

    /// FAILS IF: an Authorization header is accepted while no verifier is configured.
    #[tokio::test]
    async fn without_a_verifier_every_authorization_header_is_401() {
        let without = app(None);
        assert_eq!(status_of(&without, &["Bearer good"]).await, 401);
    }

    /// FAILS IF: no header stops behaving as before: an anonymous read when authentication is
    /// optional (it reaches the missing database, 500), 401 when it is required.
    #[tokio::test]
    async fn no_header_keeps_the_behaviour_it_had() {
        let open = crate::routes::router().with_state(AppState {
            config: Arc::new(AppConfig {
                require_authentication: false,
                ..AppConfig::default()
            }),
            db: None,
            api_tokens: Some(Arc::new(Fake)),
        });
        assert_eq!(status_of(&open, &[]).await, 500);
        assert_eq!(status_of(&app(Some(Arc::new(Fake))), &[]).await, 401);
    }

    /// FAILS IF: a verified login without a user row, or with an inactive one, becomes a user,
    /// or a verified login with an active row does not act as that row's login.
    #[test]
    fn a_verified_login_acts_as_its_user_row_and_only_as_an_active_one() {
        let robot = current_user_of(Some(user("robot", false, 1))).unwrap();
        assert_eq!((robot.id, robot.login.as_str()), (9, "robot"));
        assert!(!robot.is_anonymous && !robot.is_admin);
        let caller = AuthenticatedUser(robot).mutation_caller();
        assert_eq!(
            caller,
            Caller::Actor {
                login: "robot".into()
            }
        );

        assert!(
            current_user_of(user("root", true, 1).into())
                .unwrap()
                .is_admin
        );
        for refused in [
            None,
            Some(user("robot", false, 3)),
            Some(user("robot", false, 2)),
        ] {
            let e = current_user_of(refused).unwrap_err();
            assert_eq!(e.status_code().as_u16(), 401);
        }
    }
}
