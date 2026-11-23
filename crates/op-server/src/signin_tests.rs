// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The page sign-in end to end over the real `Router`: a person signs in at the fake
//! provider of web-login's tests (reused as it is), the sign-in map gives them an
//! OpenProject role, and the pages answer accordingly. Every subject, role key and
//! organization id here is invented.
//!
//! Each guard says which line to break to see its test fail:
//! - anonymous redirect: `signin_gate.rs`, the `Who::Anonymous` arm of `require_sign_in`
//!   (return `next.run(req).await`) → `an_anonymous_get_is_sent_to_sign_in_...`,
//!   `an_anonymous_post_is_401_...`.
//! - role mapping at the callback: `signin_gate.rs`, `Gate::routes` (`role.is_some()` →
//!   `true`) → `a_person_whose_role_keys_map_to_no_role_gets_no_session_...`.
//! - refused session: `signin_gate.rs`, the `Who::Refused` arm (answer 200) →
//!   `a_session_that_no_longer_maps_is_403_on_the_page`.
//! - top bar: `signin_gate.rs`, `show_in_top_bar` (return `resp`) or `esc` (return `s`) →
//!   `a_signed_in_person_sees_the_page_and_is_named_in_the_top_bar`,
//!   `the_person_in_the_top_bar_is_escaped`.
//! - sign-in off: `main.rs`, `build_router_with` (the `None => page_routes` arm: layer the
//!   pages with anything that answers 401) → `without_sign_in_pages_render_as_before`.

// web-login keeps this file byte-identical to zammad-rs (edition 2024 style);
// op-server is edition 2021, so rustfmt must not reformat it from here.
#[rustfmt::skip]
#[path = "../../web-login/tests/support/mod.rs"]
mod provider;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::health::{AppState, HealthChecker, HealthConfig};
use crate::metrics::Metrics;
use crate::signin_gate::Gate;
use crate::signin_map::SigninMap;
use op_core::config::AppConfig;

const ORIGIN: &str = "https://op.example.test";
const SID: &str = "__Host-op_signin_sid";
const LOGIN: &str = "__Host-op_signin_login";
const ORG_ID: &str = "480000000000000042";

const MEMBER_SUB: &str = "sub-4f9a6c21m01";
const ADMIN_SUB: &str = "sub-91c5e0a7adm";
const UNMAPPED_SUB: &str = "sub-3b7712unmap";

const MEMBER_KEY: &str = "op-member";
const ADMIN_KEY: &str = "op-admin";

const MAP: &str = "\
role=op-member:Member
role=op-admin:Project admin
rank=Project admin,Member
";

#[derive(Debug)]
struct Resp {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Resp {
    fn location(&self) -> Option<String> {
        self.headers
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap().to_owned())
    }

    /// The non-empty value of cookie `name` set by this response.
    fn cookie_value(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok()?.strip_prefix(&format!("{name}=")))
            .filter_map(|rest| rest.split(';').next())
            .find(|v| !v.is_empty())
            .map(str::to_owned)
    }
}

async fn send(app: &Router, req: Request<Body>) -> Resp {
    let resp = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (resp.status(), resp.headers().clone());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Resp {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn get_req(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::get(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::empty()).unwrap()
}

fn login_config() -> web_login::Config {
    web_login::Config::new(
        provider::ISSUER,
        provider::CLIENT_ID,
        &format!("{ORIGIN}/auth/callback"),
        "op_signin",
    )
    .unwrap()
}

fn state() -> Arc<AppState> {
    Arc::new(AppState {
        health: Arc::new(HealthChecker::new(HealthConfig::default())),
        config: AppConfig::default(),
        db: None,
        desk: crate::desk_pages::shared(crate::desk_pages::load_desk(|_| None).expect("demo desk")),
    })
}

struct Site {
    app: Router,
    p: provider::Provider,
}

fn site_with(map: &str) -> Site {
    let p = provider::Provider::with_config(login_config());
    let map = SigninMap::parse(map).expect("map parses");
    map.check().expect("map holds together");
    let gate = Gate::new(p.login.clone(), Arc::new(map));
    let app = crate::build_router_with(state(), Arc::new(Metrics::new()), Some(gate));
    Site { app, p }
}

fn site() -> Site {
    site_with(MAP)
}

/// A whole sign-in through the routes: the session cookie, or the callback's refusal.
#[allow(clippy::result_large_err)]
async fn sign_in_to(
    site: &Site,
    subject: &str,
    role_keys: &[&str],
    return_to: &str,
) -> Result<String, Resp> {
    let started = send(
        &site.app,
        get_req(&format!("/auth/login?return_to={return_to}"), None),
    )
    .await;
    assert_eq!(started.status, StatusCode::SEE_OTHER);
    let q = provider::query(&started.location().unwrap());
    let mut claims = provider::claims(subject, &q["nonce"]);
    claims[provider::ORG] = json!(ORG_ID);
    claims[provider::ROLES] = Value::Object(
        role_keys
            .iter()
            .map(|k| ((*k).to_owned(), json!({ ORG_ID: "op.example.test" })))
            .collect(),
    );
    site.p
        .set_token(provider::sign(&provider::key(1), "k1", &claims));
    let state = &q["state"];
    let done = send(
        &site.app,
        get_req(
            &format!("/auth/callback?code=c1&state={state}"),
            Some(&format!("{LOGIN}={state}")),
        ),
    )
    .await;
    match done.cookie_value(SID) {
        Some(v) if done.status == StatusCode::SEE_OTHER => {
            assert_eq!(done.location().as_deref(), Some(return_to));
            Ok(format!("{SID}={v}"))
        }
        _ => Err(done),
    }
}

#[allow(clippy::result_large_err)]
async fn sign_in(site: &Site, subject: &str, role_keys: &[&str]) -> Result<String, Resp> {
    sign_in_to(site, subject, role_keys, "/").await
}

#[tokio::test]
async fn without_sign_in_pages_render_as_before() {
    // `build_router` is the router of every pre-existing test: no sign-in.
    let app = crate::build_router(state(), Arc::new(Metrics::new()));
    for uri in ["/", "/projects", "/work_packages/1"] {
        let r = send(&app, get_req(uri, None)).await;
        assert_eq!(r.status, StatusCode::OK, "{uri}");
        assert!(r.body.contains("topnav"), "{uri}: the page has its top bar");
        assert!(
            !r.body.contains("topnav-who") && !r.body.contains("/auth/logout"),
            "{uri}: nothing of the sign-in shows: {}",
            r.body
        );
    }
    // The sign-in routes are not mounted either.
    assert_eq!(
        send(&app, get_req("/auth/login", None)).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn an_anonymous_get_is_sent_to_sign_in_and_returns_to_the_page() {
    let site = site();
    let r = send(&site.app, get_req("/", None)).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location().as_deref(), Some("/auth/login?return_to=%2F"));
    let r = send(&site.app, get_req("/projects", None)).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(
        r.location().as_deref(),
        Some("/auth/login?return_to=%2Fprojects")
    );
    assert!(r.body.is_empty() || !r.body.contains("OpenProject RS"));
    let started = send(&site.app, get_req(&r.location().unwrap(), None)).await;
    assert_eq!(started.status, StatusCode::SEE_OTHER);
    assert!(started.location().unwrap().starts_with(provider::AUTHORIZE));
    // The sign-in returns to the page that was asked for.
    let cookie = sign_in_to(&site, MEMBER_SUB, &[MEMBER_KEY], "/projects")
        .await
        .unwrap();
    assert!(cookie.starts_with(SID));
}

#[tokio::test]
async fn an_anonymous_post_is_401_and_changes_nothing() {
    let site = site();
    for uri in ["/projects/1/edit", "/work_packages/1/edit"] {
        let r = send(
            &site.app,
            Request::post(uri)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("name=x"))
                .unwrap(),
        )
        .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{uri}");
        assert!(r.location().is_none(), "{uri}");
    }
}

#[tokio::test]
async fn health_metrics_and_the_api_are_not_covered_by_the_page_sign_in() {
    let site = site();
    for uri in ["/health", "/health/live", "/metrics", "/api/v3"] {
        let r = send(&site.app, get_req(uri, None)).await;
        assert_eq!(r.status, StatusCode::OK, "{uri}");
    }
}

#[tokio::test]
async fn a_signed_in_person_sees_the_page_and_is_named_in_the_top_bar() {
    let site = site();
    let cookie = sign_in(&site, MEMBER_SUB, &[MEMBER_KEY]).await.unwrap();
    for uri in ["/", "/projects"] {
        let r = send(&site.app, get_req(uri, Some(&cookie))).await;
        assert_eq!(r.status, StatusCode::OK, "{uri}");
        assert!(
            r.body
                .contains(&format!("{} · Member", format_args!("oidc:{MEMBER_SUB}"))),
            "{uri}: {}",
            r.body
        );
        assert!(
            r.body.contains("action=\"/auth/logout\""),
            "a sign-out form"
        );
        // The person is written into the top bar, not somewhere below it.
        let who = r.body.find("topnav-who").unwrap();
        assert!(who < r.body.find("<main>").unwrap(), "{uri}");
        assert!(
            r.body.contains("OpenProject RS"),
            "{uri}: the page itself is intact"
        );
    }
    // Two role keys: the better ranked role is the person's role.
    let both = sign_in(&site, ADMIN_SUB, &[MEMBER_KEY, ADMIN_KEY])
        .await
        .unwrap();
    let r = send(&site.app, get_req("/", Some(&both))).await;
    assert!(
        r.body
            .contains(&format!("oidc:{ADMIN_SUB} · Project admin")),
        "{}",
        r.body
    );
}

#[tokio::test]
async fn the_person_in_the_top_bar_is_escaped() {
    let site = site();
    let hostile = "x<script>alert(1)</script>\"&";
    let cookie = sign_in(&site, hostile, &[MEMBER_KEY]).await.unwrap();
    let r = send(&site.app, get_req("/", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(!r.body.contains("alert(1)</script>"), "{}", r.body);
    assert!(
        r.body
            .contains("oidc:x&lt;script&gt;alert(1)&lt;/script&gt;&quot;&amp; · Member"),
        "{}",
        r.body
    );
}

#[tokio::test]
async fn a_person_whose_role_keys_map_to_no_role_gets_no_session_without_a_default() {
    let site = site();
    let refused = sign_in(&site, UNMAPPED_SUB, &["some-other-key"])
        .await
        .expect_err("refused");
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(refused.cookie_value(SID).is_none());
    assert!(refused
        .headers
        .get_all(header::SET_COOKIE)
        .iter()
        .all(|v| !v.to_str().unwrap().starts_with(SID)));
    let none = sign_in(&site, UNMAPPED_SUB, &[])
        .await
        .expect_err("no roles");
    assert_eq!(none.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_default_row_serves_a_person_whose_role_keys_map_to_nothing() {
    let site = site_with(&format!("{MAP}role.default=Member\n"));
    let cookie = sign_in(&site, UNMAPPED_SUB, &["some-other-key"])
        .await
        .unwrap();
    let r = send(&site.app, get_req("/", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(
        r.body.contains(&format!("oidc:{UNMAPPED_SUB} · Member")),
        "{}",
        r.body
    );
}

#[tokio::test]
async fn a_session_that_no_longer_maps_is_403_on_the_page() {
    let site = site();
    let cookie = sign_in(&site, MEMBER_SUB, &[MEMBER_KEY]).await.unwrap();
    // The same sessions, read through a map that no longer names the person's role key.
    let map = SigninMap::parse("role=other-key:Member\nrank=Member\n").unwrap();
    let gate = Gate::new(site.p.login.clone(), Arc::new(map));
    let app = crate::build_router_with(state(), Arc::new(Metrics::new()), Some(gate));
    let r = send(&app, get_req("/", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(!r.body.contains("OpenProject RS"), "{}", r.body);
}

#[tokio::test]
async fn signing_out_ends_the_session() {
    let site = site();
    let cookie = sign_in(&site, MEMBER_SUB, &[MEMBER_KEY]).await.unwrap();
    assert_eq!(
        send(&site.app, get_req("/", Some(&cookie))).await.status,
        StatusCode::OK
    );
    let out = send(
        &site.app,
        Request::post("/auth/logout")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(out.status, StatusCode::SEE_OTHER);
    assert_eq!(out.location().as_deref(), Some("/auth/signed-out"));
    let r = send(&site.app, get_req("/", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location().as_deref(), Some("/auth/login?return_to=%2F"));
}
