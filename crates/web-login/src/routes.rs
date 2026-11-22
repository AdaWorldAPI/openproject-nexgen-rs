// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The sign-in routes over axum. Failure pages are plain text; only the failure class is
//! logged, never a value.

use crate::login::{CallbackError, Login, SignedIn};
use crate::util::{cookie, set_cookie};

/// The value of cookie `name`. A request may carry several `Cookie` fields (HTTP/2 splits
/// them), so every field is read.
fn cookie_in<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .find_map(|v| cookie(v.to_str().ok(), name))
}
use crate::{LOGIN_TTL, SESSION_TTL};
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

/// Decides at the callback whether a verified person may have a session at all. `false`
/// ends the session before it is handed out and answers 403. Without one every verified
/// identity is let in.
pub type Admit = Arc<dyn Fn(&SignedIn) -> bool + Send + Sync>;

#[derive(Clone)]
struct St {
    login: Arc<Login>,
    admit: Option<Admit>,
}

/// `GET /auth/login`, `GET /auth/callback`, `POST /auth/logout` and the landing page
/// `GET /auth/signed-out`.
pub fn routes(login: Arc<Login>) -> Router {
    build(St { login, admit: None })
}

/// [`routes`] with an admission check at the callback.
pub fn routes_with(login: Arc<Login>, admit: Admit) -> Router {
    build(St {
        login,
        admit: Some(admit),
    })
}

fn build(st: St) -> Router {
    Router::new()
        .route("/auth/login", get(auth_login))
        .route("/auth/callback", get(auth_callback))
        .route("/auth/logout", post(auth_logout))
        .route("/auth/signed-out", get(auth_signed_out))
        .with_state(st)
}

/// Who a request is: reads the session cookie, makes a cookie from before a restart usable
/// ([`Login::restore`]) and returns the live session's identity.
pub async fn signed_in_from_headers(login: &Login, headers: &HeaderMap) -> Option<SignedIn> {
    let name = login.config().session_cookie();
    let value = cookie_in(headers, &name)?;
    login.restore(value).await;
    login.signed_in(value)
}

fn text(status: StatusCode, body: &'static str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

fn redirect(location: &str, cookies: &[String]) -> Response {
    let mut r = (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, location.to_string())],
    )
        .into_response();
    for c in cookies {
        if let Ok(v) = HeaderValue::from_str(c) {
            r.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

#[derive(Deserialize)]
struct LoginQuery {
    return_to: Option<String>,
}

/// Start a sign-in: redirect to the provider with state, nonce and a PKCE challenge.
async fn auth_login(State(st): State<St>, Query(q): Query<LoginQuery>) -> Response {
    match st.login.start(q.return_to.as_deref()).await {
        Ok((url, state)) => redirect(
            &url,
            &[set_cookie(
                &st.login.config().login_cookie(),
                &state,
                LOGIN_TTL,
            )],
        ),
        Err(e) => {
            eprintln!("web-login: login start failed: {}", e.class());
            text(StatusCode::BAD_GATEWAY, "sign-in unavailable")
        }
    }
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

enum Refused {
    Failed(CallbackError),
    NoAccess,
}

/// Finish a sign-in. A token, state or provider failure is the same 401 body; a verified
/// person the admission check refuses gets a 403 and no session.
async fn auth_callback(
    State(st): State<St>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let cfg = st.login.config();
    let cookie_state = cookie_in(&headers, &cfg.login_cookie());
    // A cookie that names another state belongs to another attempt and is left alone.
    let own_attempt = cookie_state.is_some() && cookie_state == q.state.as_deref();
    let clear = set_cookie(&cfg.login_cookie(), "", Duration::ZERO);
    let admit = |who: &SignedIn| st.admit.as_ref().is_none_or(|admit| admit(who));
    let finished = st
        .login
        .finish_admitting(
            cookie_state,
            q.state.as_deref(),
            q.code.as_deref(),
            q.error.is_some(),
            &admit,
        )
        .await
        .map_err(|e| match e {
            CallbackError::NoAccess => Refused::NoAccess,
            e => Refused::Failed(e),
        });
    match finished {
        Ok(f) => redirect(
            &f.return_to,
            &[
                clear,
                set_cookie(&cfg.session_cookie(), &f.session_cookie, SESSION_TTL),
            ],
        ),
        Err(refused) => {
            let mut r = match refused {
                Refused::Failed(e) => {
                    eprintln!("web-login: callback refused: {}", e.class());
                    text(StatusCode::UNAUTHORIZED, "sign-in failed")
                }
                Refused::NoAccess => {
                    eprintln!("web-login: callback refused: admission");
                    text(StatusCode::FORBIDDEN, "no access")
                }
            };
            if own_attempt && let Ok(v) = HeaderValue::from_str(&clear) {
                r.headers_mut().append(header::SET_COOKIE, v);
            }
            r
        }
    }
}

/// End the session and clear its cookie. `POST` only; the session cookie is `SameSite=Lax`,
/// so a cross-site request carries no cookie. Such a request ends nothing and clears
/// nothing: the cookie is only cleared for a request that carried it.
async fn auth_logout(State(st): State<St>, headers: HeaderMap) -> Response {
    let name = st.login.config().session_cookie();
    match cookie_in(&headers, &name) {
        Some(sid) => {
            st.login.logout(sid);
            redirect("/auth/signed-out", &[set_cookie(&name, "", Duration::ZERO)])
        }
        None => redirect("/auth/signed-out", &[]),
    }
}

/// Landing page after sign-out. It never starts a sign-in by itself: with the provider's
/// own session still active that would sign the user straight back in.
async fn auth_signed_out() -> Response {
    text(StatusCode::OK, "signed out")
}
