// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The three routes (and the landing page) over a `Router`, driven in-process with the fake
//! provider.
#![cfg(feature = "axum")]

mod support;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use std::sync::Arc;
use support::*;
use tower::ServiceExt;
use web_login::{SESSION_TTL, routes, routes_with, signed_in_from_headers};

const SID: &str = "__Host-demo_sid";
const LOGIN: &str = "__Host-demo_login";

struct Resp {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Resp {
    fn location(&self) -> String {
        self.headers
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    }
    fn set_cookie(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .find(|c| c.starts_with(&format!("{name}=")))
    }
    /// The non-empty value of cookie `name` set by this response.
    fn cookie_value(&self, name: &str) -> Option<String> {
        let c = self.set_cookie(name)?;
        let v = c.strip_prefix(&format!("{name}="))?.split(';').next()?;
        (!v.is_empty()).then(|| v.to_string())
    }
}

async fn call(app: &Router, method: Method, uri: &str, cookie: Option<&str>) -> Resp {
    let mut rq = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        rq = rq.header(header::COOKIE, c);
    }
    let resp = app
        .clone()
        .oneshot(rq.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Resp {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn attrs(cookie: &str) -> Vec<&str> {
    cookie.split("; ").skip(1).collect()
}

fn assert_cookie_attributes(cookie: &str, max_age: u64) {
    let a = attrs(cookie);
    for want in ["Path=/", "Secure", "HttpOnly", "SameSite=Lax"] {
        assert!(a.contains(&want), "{want} missing in {cookie}");
    }
    assert!(
        a.contains(&format!("Max-Age={max_age}").as_str()),
        "{cookie}"
    );
    assert!(cookie.starts_with("__Host-"), "{cookie}");
    assert!(!a.iter().any(|x| x.starts_with("Domain")), "{cookie}");
}

struct Harness {
    app: Router,
    p: Provider,
}

fn harness() -> Harness {
    let p = Provider::keyed(SESSION_KEY);
    Harness {
        app: routes(p.login.clone()),
        p,
    }
}

async fn begin_route(h: &Harness, return_to: Option<&str>) -> (Resp, Started) {
    let uri = match return_to {
        Some(r) => format!("/auth/login?return_to={}", web_login::urlencode(r)),
        None => "/auth/login".to_string(),
    };
    let r = call(&h.app, Method::GET, &uri, None).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let q = query(&r.location());
    let st = Started {
        state: q["state"].clone(),
        nonce: q["nonce"].clone(),
        url: r.location(),
    };
    (r, st)
}

async fn sign_in_route(
    h: &Harness,
    return_to: Option<&str>,
    tweak: impl FnOnce(&mut serde_json::Value),
) -> (Started, Resp) {
    let (_, st) = begin_route(h, return_to).await;
    let mut c = claims("user-1", &st.nonce);
    tweak(&mut c);
    h.p.set_token(sign(&key(1), "k1", &c));
    let r = call(
        &h.app,
        Method::GET,
        &format!("/auth/callback?code=c1&state={}", st.state),
        Some(&format!("{LOGIN}={}", st.state)),
    )
    .await;
    (st, r)
}

fn sid_cookie(r: &Resp) -> String {
    format!("{SID}={}", r.cookie_value(SID).expect("session cookie set"))
}

#[tokio::test]
async fn login_redirects_to_the_provider_and_sets_the_login_cookie() {
    let h = harness();
    let (r, st) = begin_route(&h, None).await;
    assert!(r.location().starts_with(&format!("{AUTHORIZE}?")));
    assert_eq!(query(&r.location())["code_challenge_method"], "S256");
    let c = r.set_cookie(LOGIN).expect("login cookie");
    assert_eq!(
        c.split(';').next().unwrap(),
        format!("{LOGIN}={}", st.state)
    );
    assert_cookie_attributes(&c, 600);
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    assert!(r.set_cookie(SID).is_none());
}

#[tokio::test]
async fn the_callback_sets_the_session_cookie_clears_the_login_cookie_and_redirects() {
    let h = harness();
    let (_, r) = sign_in_route(&h, Some("/desk/tickets?x=1"), |_| {}).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location(), "/desk/tickets?x=1");
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    assert_cookie_attributes(&r.set_cookie(SID).unwrap(), SESSION_TTL.as_secs());
    let cleared = r.set_cookie(LOGIN).expect("login cookie cleared");
    assert_cookie_attributes(&cleared, 0);
    assert!(cleared.starts_with(&format!("{LOGIN}=;")), "{cleared}");
    let sid = sid_cookie(&r);
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, HeaderValue::from_str(&sid).unwrap());
    let who = signed_in_from_headers(&h.p.login, &headers)
        .await
        .expect("signed in");
    assert_eq!(who.subject, "user-1");
    assert_eq!(who.issuer, ISSUER);
}

#[tokio::test]
async fn an_unsafe_return_to_goes_to_the_default() {
    let h = harness();
    for bad in [
        "//evil.example.test",
        "https://evil.example.test",
        "/\\evil",
    ] {
        let (_, r) = sign_in_route(&h, Some(bad), |_| {}).await;
        assert_eq!(r.status, StatusCode::SEE_OTHER, "{bad}");
        assert_eq!(r.location(), "/", "{bad}");
    }
}

#[tokio::test]
async fn a_failed_callback_is_a_plain_401_with_no_session_and_a_cleared_login_cookie() {
    let h = harness();
    let (_, st) = begin_route(&h, None).await;
    h.p.set_token(sign(&key(1), "k1", &claims("user-1", &st.nonce)));
    // The cookie names another state than the query.
    let r = call(
        &h.app,
        Method::GET,
        &format!("/auth/callback?code=c1&state={}", st.state),
        Some(&format!("{LOGIN}=some-other-state")),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body, "sign-in failed");
    assert!(
        r.headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    assert!(r.cookie_value(SID).is_none(), "no session may start");
    assert!(
        r.set_cookie(LOGIN).is_none(),
        "the cookie belongs to another attempt and is left alone"
    );
    assert_eq!(h.p.fake.token_calls(), 0);
    // A bad token is the same answer, and this attempt's own cookie is cleared.
    let (_, r) = sign_in_route(&h, None, |c| c["aud"] = serde_json::json!("other")).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body, "sign-in failed");
    assert!(r.cookie_value(SID).is_none());
    assert_cookie_attributes(&r.set_cookie(LOGIN).unwrap(), 0);
    // And a provider error.
    let (_, st) = begin_route(&h, None).await;
    let r = call(
        &h.app,
        Method::GET,
        &format!("/auth/callback?error=access_denied&state={}", st.state),
        Some(&format!("{LOGIN}={}", st.state)),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(r.cookie_value(SID).is_none());
}

#[tokio::test]
async fn an_unreachable_provider_is_a_502() {
    let h = harness();
    h.p.fake.discovery.lock().unwrap()["issuer"] = serde_json::json!("https://evil.example.test");
    let r = call(&h.app, Method::GET, "/auth/login", None).await;
    assert_eq!(r.status, StatusCode::BAD_GATEWAY);
    assert_eq!(r.body, "sign-in unavailable");
    assert!(r.set_cookie(LOGIN).is_none());
}

#[tokio::test]
async fn logout_clears_the_cookie_ends_the_session_and_lands_on_a_page_that_does_not_sign_in() {
    let h = harness();
    let (_, cb) = sign_in_route(&h, None, |_| {}).await;
    let cookie = sid_cookie(&cb);
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
    assert!(signed_in_from_headers(&h.p.login, &headers).await.is_some());

    let out = call(&h.app, Method::POST, "/auth/logout", Some(&cookie)).await;
    assert_eq!(out.status, StatusCode::SEE_OTHER);
    assert_eq!(out.location(), "/auth/signed-out");
    let cleared = out.set_cookie(SID).expect("session cookie cleared");
    assert!(cleared.starts_with(&format!("{SID}=;")), "{cleared}");
    assert_cookie_attributes(&cleared, 0);
    assert!(signed_in_from_headers(&h.p.login, &headers).await.is_none());

    let landing = call(&h.app, Method::GET, "/auth/signed-out", Some(&cookie)).await;
    assert_eq!(landing.status, StatusCode::OK);
    assert!(landing.set_cookie(LOGIN).is_none() && landing.headers.get(header::LOCATION).is_none());
}

#[tokio::test]
async fn logout_is_post_only() {
    let h = harness();
    let r = call(&h.app, Method::GET, "/auth/logout", None).await;
    assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED);
    let (_, cb) = sign_in_route(&h, None, |_| {}).await;
    let cookie = sid_cookie(&cb);
    let r = call(&h.app, Method::GET, "/auth/logout", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED);
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
    assert!(
        signed_in_from_headers(&h.p.login, &headers).await.is_some(),
        "a GET ends nothing"
    );
}

#[tokio::test]
async fn a_request_after_a_restart_is_restored_from_the_header() {
    let before = harness();
    let (_, cb) = sign_in_route(&before, None, |_| {}).await;
    let cookie = sid_cookie(&cb);
    let after = harness();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("a=1; {cookie}; b=2")).unwrap(),
    );
    let who = signed_in_from_headers(&after.p.login, &headers)
        .await
        .expect("restored");
    assert_eq!(who.subject, "user-1");
    assert_eq!(after.p.fake.jwks_fetches(), 1);
    assert!(
        signed_in_from_headers(&after.p.login, &HeaderMap::new())
            .await
            .is_none()
    );
}

#[tokio::test]
async fn an_admission_check_can_refuse_a_verified_person_before_a_session_exists() {
    let p = Provider::keyed(SESSION_KEY);
    let admit: web_login::Admit = Arc::new(|s| s.org.as_deref() == Some("77"));
    let app = routes_with(p.login.clone(), admit);
    let h = Harness { app, p };
    let (_, r) = sign_in_route(&h, None, |c| c[ORG] = serde_json::json!("88")).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body, "no access");
    assert!(r.cookie_value(SID).is_none());
    assert_cookie_attributes(&r.set_cookie(LOGIN).unwrap(), 0);
    let (_, r) = sign_in_route(&h, None, |c| c[ORG] = serde_json::json!("77")).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert!(r.cookie_value(SID).is_some());
}

async fn call_split(app: &Router, method: Method, uri: &str, cookies: &[&str]) -> Resp {
    let mut rq = Request::builder().method(method).uri(uri);
    for c in cookies {
        rq = rq.header(header::COOKIE, *c);
    }
    let resp = app
        .clone()
        .oneshot(rq.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Resp {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

#[tokio::test]
async fn a_cookie_in_a_second_cookie_field_is_found() {
    let h = harness();
    let (_, st) = begin_route(&h, None).await;
    h.p.set_token(sign(&key(1), "k1", &claims("user-1", &st.nonce)));
    let login = format!("{LOGIN}={}", st.state);
    let cb = call_split(
        &h.app,
        Method::GET,
        &format!("/auth/callback?code=c1&state={}", st.state),
        &["theme=dark", &login],
    )
    .await;
    assert_eq!(cb.status, StatusCode::SEE_OTHER, "{}", cb.body);
    let sid = sid_cookie(&cb);

    let mut split = HeaderMap::new();
    split.append(header::COOKIE, HeaderValue::from_static("theme=dark"));
    split.append(header::COOKIE, HeaderValue::from_str(&sid).unwrap());
    assert!(signed_in_from_headers(&h.p.login, &split).await.is_some());
    let mut combined = HeaderMap::new();
    combined.insert(
        header::COOKIE,
        HeaderValue::from_str(&format!("theme=dark; {sid}")).unwrap(),
    );
    assert!(
        signed_in_from_headers(&h.p.login, &combined)
            .await
            .is_some()
    );

    let out = call_split(&h.app, Method::POST, "/auth/logout", &["theme=dark", &sid]).await;
    assert_eq!(out.status, StatusCode::SEE_OTHER);
    assert!(
        signed_in_from_headers(&h.p.login, &split).await.is_none(),
        "the session is ended, not only its cookie"
    );
}

#[tokio::test]
async fn a_logout_that_carries_no_session_changes_nothing_in_the_browser() {
    let h = harness();
    let (_, cb) = sign_in_route(&h, None, |_| {}).await;
    let cookie = sid_cookie(&cb);
    let out = call(&h.app, Method::POST, "/auth/logout", None).await;
    assert_eq!(out.status, StatusCode::SEE_OTHER);
    assert!(
        out.set_cookie(SID).is_none(),
        "a request without the cookie must not clear it"
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
    assert!(signed_in_from_headers(&h.p.login, &headers).await.is_some());
}
