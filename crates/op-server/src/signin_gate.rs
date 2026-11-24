// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The sign-in of the rendered pages as one router layer, and the `/auth/*` routes of
//! `web_login` to mount beside them. (zammad-ticket's `server.rs` does the same for its one
//! page; op-server has many pages, so the check is a middleware over a router instead of a
//! call inside each handler.)
//!
//! - An anonymous `GET` is sent to `/auth/login?return_to=<the page asked for>`; any other
//!   anonymous method is `401`.
//! - A person whose role keys map to no OpenProject role, with no default row in the sign-in
//!   map, gets no session at all: the callback answers `403` and sets no session cookie. A
//!   session that is verified but no longer maps (the map changed) is `403` on the page.
//! - A signed-in request carries its [`Who::Caller`] as a request extension, and an HTML
//!   page shows the person in the top bar, next to a sign-out form.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use op_api::extractors::ApiTokenVerifier;
use web_login::{routes_with, safe_return_to, urlencode, Admit, Login};

use crate::provision::Provisioner;
use crate::signin::{op_role_of, LoginSignIn, SignIn, Who};
use crate::signin_map::SigninMap;

/// The largest page the top bar is written into; a bigger body passes through untouched.
const MAX_PAGE_BYTES: usize = 8 * 1024 * 1024;

/// The role keys the token carried, for the refusal log: keys only, never the
/// organization they were granted in or any other claim.
fn role_keys_note(who: &web_login::SignedIn) -> String {
    let keys: std::collections::BTreeSet<&str> =
        who.roles.iter().map(|(_, k)| k.as_str()).collect();
    if keys.is_empty() {
        "the token carries no project roles".to_owned()
    } else {
        format!("none of the role keys {keys:?} is mapped")
    }
}

/// Minimal HTML text/attribute escape for the person shown in the top bar.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// The sign-in of the pages: the session of `login`, read through the sign-in map.
#[derive(Clone)]
pub struct Gate {
    login: Arc<Login>,
    map: Arc<SigninMap>,
    sign_in: Arc<dyn SignIn>,
    api_tokens: Option<Arc<dyn ApiTokenVerifier>>,
}

impl Gate {
    pub fn new(login: Arc<Login>, map: Arc<SigninMap>) -> Gate {
        let sign_in = Arc::new(LoginSignIn::new(login.clone(), map.clone()));
        Gate {
            login,
            map,
            sign_in,
            api_tokens: None,
        }
    }

    /// Provision every person the map admits, in `pool`, when the map has `admin=` or
    /// `projects=` rows; otherwise the gate is returned as it is.
    pub fn with_provisioning(mut self, pool: sqlx::PgPool) -> Gate {
        if let Some(provisioner) = Provisioner::new(pool, self.map.clone()) {
            self.sign_in = Arc::new(
                LoginSignIn::new(self.login.clone(), self.map.clone())
                    .with_provisioner(Arc::new(provisioner)),
            );
        }
        self
    }

    /// The verifier of machine-client bearer tokens for `/api/v3`.
    pub fn with_api_tokens(mut self, verifier: Arc<dyn ApiTokenVerifier>) -> Gate {
        self.api_tokens = Some(verifier);
        self
    }

    /// `None` unless `OP_API_CLIENTS` turned the token path on.
    pub fn api_tokens(&self) -> Option<Arc<dyn ApiTokenVerifier>> {
        self.api_tokens.clone()
    }

    /// `/auth/login`, `/auth/callback`, `/auth/logout`, `/auth/signed-out`, with the
    /// admission check of the sign-in map at the callback.
    pub fn routes(&self) -> Router {
        let admit: Admit = {
            let map = self.map.clone();
            Arc::new(move |who| {
                let role = op_role_of(&map, who);
                if role.is_none() {
                    eprintln!(
                        "openproject-server: admission refused: {}",
                        role_keys_note(who)
                    );
                }
                role.is_some()
            })
        };
        routes_with(self.login.clone(), admit)
    }

    /// `pages` behind the sign-in. Routes merged into the result afterwards (the `/auth/*`
    /// routes, health, metrics, the JSON API) are not covered.
    pub fn protect(&self, pages: Router) -> Router {
        pages.layer(middleware::from_fn_with_state(
            self.clone(),
            require_sign_in,
        ))
    }
}

fn plain(status: StatusCode, text: &'static str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        text,
    )
        .into_response()
}

/// The address an anonymous `GET` of `asked` is sent to.
fn sign_in_location(base: &str, asked: &str) -> String {
    if safe_return_to(asked) {
        format!("{base}?return_to={}", urlencode(asked))
    } else {
        base.to_owned()
    }
}

fn redirect(location: &str) -> Response {
    let mut r = (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, location.to_owned())],
    )
        .into_response();
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

async fn require_sign_in(State(gate): State<Gate>, mut req: Request, next: Next) -> Response {
    match gate.sign_in.resolve(req.headers()).await {
        Who::Caller { actor, role } => {
            let bar = who_html(&actor, &role);
            req.extensions_mut().insert(Who::Caller { actor, role });
            show_in_top_bar(next.run(req).await, &bar).await
        }
        Who::Anonymous { sign_in_url } => {
            let readable = matches!(*req.method(), Method::GET | Method::HEAD);
            match sign_in_url {
                Some(base) if readable => {
                    let asked = req
                        .uri()
                        .path_and_query()
                        .map(|p| p.as_str())
                        .unwrap_or("/");
                    redirect(&sign_in_location(&base, asked))
                }
                _ => plain(StatusCode::UNAUTHORIZED, "sign in required"),
            }
        }
        Who::Refused { .. } => plain(StatusCode::FORBIDDEN, "no access"),
    }
}

/// The person and a sign-out form, as the top bar shows them.
fn who_html(actor: &str, role: &str) -> String {
    format!(
        "<span class=\"topnav-who\" style=\"margin-left:auto\">{} · {}</span>\
<form method=\"post\" action=\"/auth/logout\" style=\"margin:0\">\
<button type=\"submit\">Sign out</button></form>",
        esc(actor),
        esc(role)
    )
}

/// Writes `bar` into the page's top bar (`<nav … topnav">…</nav>`), before its closing tag.
/// A response that is not HTML, has no top bar or is too large passes through unchanged.
async fn show_in_top_bar(resp: Response, bar: &str) -> Response {
    let is_html = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    if !is_html {
        return resp;
    }
    let (mut parts, body) = resp.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_PAGE_BYTES).await else {
        return plain(StatusCode::INTERNAL_SERVER_ERROR, "page too large");
    };
    let html = String::from_utf8_lossy(&bytes);
    let out = inject_top_bar(&html, bar).unwrap_or_else(|| html.into_owned());
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(out))
}

/// `html` with `bar` before the `</nav>` that closes the `topnav` bar; `None` without one.
fn inject_top_bar(html: &str, bar: &str) -> Option<String> {
    let start = html.find("topnav\">")?;
    let close = start + html[start..].find("</nav>")?;
    let mut out = String::with_capacity(html.len() + bar.len());
    out.push_str(&html[..close]);
    out.push_str(bar);
    out.push_str(&html[close..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signin::SIGN_IN_PATH;

    #[test]
    fn the_top_bar_gets_the_person_escaped_before_its_closing_tag() {
        let page = "<nav class=\"a topnav\"><a href=\"/\">x</a></nav><main></main><nav>other</nav>";
        let out = inject_top_bar(page, &who_html("oidc:<b>&\"'", "Member")).unwrap();
        assert!(
            out.contains("oidc:&lt;b&gt;&amp;&quot;&#39; · Member"),
            "{out}"
        );
        assert!(!out.contains("<b>"), "{out}");
        let i = out.find("topnav-who").unwrap();
        assert!(
            i < out.find("<main>").unwrap(),
            "before the main region: {out}"
        );
        assert!(
            i > out.find("<a href=\"/\">x</a>").unwrap(),
            "after the links: {out}"
        );
        assert!(inject_top_bar("<p>no bar</p>", "x").is_none());
    }

    #[test]
    fn the_sign_in_address_returns_to_a_local_page_only() {
        assert_eq!(
            sign_in_location(SIGN_IN_PATH, "/projects?x=1"),
            "/auth/login?return_to=%2Fprojects%3Fx%3D1"
        );
        assert_eq!(
            sign_in_location(SIGN_IN_PATH, "//evil.example"),
            "/auth/login"
        );
    }
}
