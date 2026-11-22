// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The sign-in flow driven through `Login` over a fake provider: no socket is opened. ID
//! tokens are signed with real ES256 keys and checked by the real verifier.

mod support;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;
use support::*;
use web_login::{CallbackError, Config, Login, SignedIn};

async fn restored(p: &Provider, cookie: &str) -> Option<SignedIn> {
    p.login.restore(cookie).await;
    p.login.signed_in(cookie)
}

fn with_person(c: &mut Value) {
    c["sub"] = json!("290000000000000042");
    c[ORG] = json!("300000000000000001");
    c[ROLES] = json!({
        "role-a": {"300000000000000001": "one.example.test"},
        "role-b": {
            "300000000000000001": "one.example.test",
            "300000000000000002": "two.example.test"
        }
    });
    c["email"] = json!("person@one.example.test");
    c["email_verified"] = json!(true);
}

// ── the happy path ──────────────────────────────────────────────────────────

#[tokio::test]
async fn a_sign_in_yields_the_subject_organization_roles_and_verified_email() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(1), "k1", with_person).await;
    let f = r.expect("signs in");
    let s = &f.signed_in;
    assert_eq!(s.issuer, ISSUER);
    assert_eq!(s.subject, "290000000000000042");
    assert_eq!(s.org.as_deref(), Some("300000000000000001"));
    assert_eq!(
        s.roles,
        vec![
            ("300000000000000001".to_string(), "role-a".to_string()),
            ("300000000000000001".to_string(), "role-b".to_string()),
            ("300000000000000002".to_string(), "role-b".to_string()),
        ]
    );
    assert_eq!(s.email.as_deref(), Some("person@one.example.test"));
    assert!(
        s.expires_at > now() && s.expires_at <= now() + 300,
        "{}",
        s.expires_at
    );
    assert_eq!(f.return_to, "/");
    // The session cookie yields the same person, and the canonical user `establish` made.
    assert_eq!(p.login.signed_in(&f.session_cookie).as_ref(), Some(s));
    let v = p.login.verified(&f.session_cookie).expect("verified login");
    assert_eq!(v.identity().subject().as_str(), "290000000000000042");
    assert_eq!(
        v.authenticated_user().user.subject,
        "oidc:290000000000000042"
    );
}

#[tokio::test]
async fn a_token_with_no_organization_and_no_role_is_still_a_sign_in() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(1), "k1", |_| {}).await;
    let s = r.expect("signs in").signed_in;
    assert_eq!(s.org, None);
    assert!(s.roles.is_empty());
    assert_eq!(s.email, None);
}

#[tokio::test]
async fn an_unverified_or_unmarked_email_is_not_reported() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| {
        c["email"] = json!("a@one.example.test");
        c["email_verified"] = json!(false);
    })
    .await;
    assert_eq!(r.unwrap().signed_in.email, None);
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| {
        c["email"] = json!("a@one.example.test");
    })
    .await;
    assert_eq!(r.unwrap().signed_in.email, None);
}

#[tokio::test]
async fn an_organization_that_could_forge_a_key_is_not_reported() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| c[ORG] = json!("1/2")).await;
    assert_eq!(r.unwrap().signed_in.org, None);
}

#[tokio::test]
async fn the_claim_names_come_from_the_configuration() {
    let cfg = base_config()
        .with_roles_claim("my:roles")
        .with_org_claim("my:org");
    let p = Provider::with_config(cfg);
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| {
        c["my:org"] = json!("77");
        c["my:roles"] = json!({"r": {"77": "x"}});
        c[ORG] = json!("999");
        c[ROLES] = json!({"other": {"999": "x"}});
    })
    .await;
    let s = r.unwrap().signed_in;
    assert_eq!(s.org.as_deref(), Some("77"));
    assert_eq!(s.roles, vec![("77".to_string(), "r".to_string())]);
}

// ── the authorization request and the code exchange ─────────────────────────

#[tokio::test]
async fn the_authorization_request_carries_pkce_state_nonce_and_scopes() {
    let p = Provider::new();
    let st = begin(&p, None).await;
    assert!(st.url.starts_with(&format!("{AUTHORIZE}?")), "{}", st.url);
    let q = query(&st.url);
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["client_id"], CLIENT_ID);
    assert_eq!(q["redirect_uri"], REDIRECT);
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(
        q["scope"],
        "openid urn:zitadel:iam:user:resourceowner urn:zitadel:iam:org:projects:roles"
    );
    assert_ne!(st.state, st.nonce);
    assert!(st.state.len() >= 43 && st.nonce.len() >= 43);

    let (_, r) = sign_in(&p, None, &key(1), "k1", |_| {}).await;
    r.expect("signs in");
    let forms = p.fake.forms.lock().unwrap();
    let form = &forms[0];
    let get = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(get("grant_type").as_deref(), Some("authorization_code"));
    assert_eq!(get("code").as_deref(), Some("c1"));
    assert_eq!(get("redirect_uri").as_deref(), Some(REDIRECT));
    assert_eq!(get("client_id").as_deref(), Some(CLIENT_ID));
    assert_eq!(get("client_secret"), None);
    drop(forms);
}

#[tokio::test]
async fn the_code_verifier_is_the_preimage_of_the_challenge() {
    let p = Provider::new();
    let st = begin(&p, None).await;
    p.set_token(sign(&key(1), "k1", &claims("user-1", &st.nonce)));
    callback(&p, &st.state, Some(&st.state))
        .await
        .expect("signs in");
    let challenge = query(&st.url)["code_challenge"].clone();
    let forms = p.fake.forms.lock().unwrap();
    let verifier = forms[0]
        .iter()
        .find(|(n, _)| n == "code_verifier")
        .map(|(_, v)| v.clone())
        .expect("verifier sent");
    assert_eq!(b64(&Sha256::digest(verifier.as_bytes())), challenge);
    assert!(
        !st.url.contains(&verifier),
        "the verifier never leaves the server"
    );
}

#[tokio::test]
async fn extra_scopes_follow_the_configuration() {
    let p = Provider::with_config(base_config().with_extra_scopes("profile email"));
    assert_eq!(
        query(&begin(&p, None).await.url)["scope"],
        "openid profile email"
    );
    let p = Provider::with_config(base_config().with_extra_scopes(""));
    assert_eq!(query(&begin(&p, None).await.url)["scope"], "openid");
}

#[tokio::test]
async fn a_client_secret_is_sent_only_when_configured() {
    let p = Provider::with_config(base_config().with_client_secret("test-secret-value"));
    let (_, r) = sign_in(&p, None, &key(1), "k1", |_| {}).await;
    r.expect("signs in");
    let forms = p.fake.forms.lock().unwrap();
    let sent = forms[0]
        .iter()
        .find(|(n, _)| n == "client_secret")
        .map(|(_, v)| v.as_str());
    assert_eq!(sent, Some("test-secret-value"));
}

#[tokio::test]
async fn discovery_is_fetched_once_and_cached() {
    let p = Provider::new();
    ok_sign_in(&p).await;
    ok_sign_in(&p).await;
    assert_eq!(
        p.fake
            .discovery_fetches
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

#[tokio::test]
async fn discovery_naming_another_issuer_is_refused() {
    let p = Provider::new();
    p.fake.discovery.lock().unwrap()["issuer"] = json!("https://evil.example.test");
    assert_eq!(
        p.login.start(None).await.unwrap_err(),
        CallbackError::Upstream
    );
}

#[tokio::test]
async fn a_non_https_token_endpoint_is_refused() {
    let p = Provider::new();
    p.fake.discovery.lock().unwrap()["token_endpoint"] = json!("http://idp.example.test/token");
    assert_eq!(
        p.login.start(None).await.unwrap_err(),
        CallbackError::Upstream
    );
}

// ── state, single use ───────────────────────────────────────────────────────

#[tokio::test]
async fn a_state_that_differs_from_the_cookie_is_refused() {
    let p = Provider::new();
    let st = begin(&p, None).await;
    p.set_token(sign(&key(1), "k1", &claims("user-1", &st.nonce)));
    let r = callback(&p, &st.state, Some("some-other-state")).await;
    assert_eq!(r.unwrap_err(), CallbackError::State);
    assert_eq!(
        p.fake.token_calls(),
        0,
        "a state failure must not reach the token endpoint"
    );
    // The pending login survived the refusal, so the right cookie still works once.
    callback(&p, &st.state, Some(&st.state))
        .await
        .expect("signs in");
}

#[tokio::test]
async fn a_callback_without_the_cookie_or_the_query_state_is_refused() {
    let p = Provider::new();
    let st = begin(&p, None).await;
    p.set_token(sign(&key(1), "k1", &claims("user-1", &st.nonce)));
    assert_eq!(
        callback(&p, &st.state, None).await.unwrap_err(),
        CallbackError::State
    );
    assert_eq!(
        p.login
            .finish(Some(&st.state), None, Some("c1"), false)
            .await
            .unwrap_err(),
        CallbackError::State
    );
    assert_eq!(p.fake.token_calls(), 0);
}

#[tokio::test]
async fn an_unknown_state_is_refused() {
    let p = Provider::new();
    let r = callback(&p, "never-issued", Some("never-issued")).await;
    assert_eq!(r.unwrap_err(), CallbackError::State);
    assert_eq!(p.fake.token_calls(), 0);
}

#[tokio::test]
async fn a_state_works_once() {
    let p = Provider::new();
    let st = begin(&p, None).await;
    p.set_token(sign(&key(1), "k1", &claims("user-1", &st.nonce)));
    callback(&p, &st.state, Some(&st.state))
        .await
        .expect("first use");
    let again = callback(&p, &st.state, Some(&st.state)).await;
    assert_eq!(again.unwrap_err(), CallbackError::State);
    assert_eq!(p.fake.token_calls(), 1);
}

#[tokio::test]
async fn a_provider_error_or_a_missing_code_is_refused_and_uses_up_the_state() {
    let p = Provider::new();
    let st = begin(&p, None).await;
    let r = p
        .login
        .finish(Some(&st.state), Some(&st.state), None, true)
        .await;
    assert_eq!(r.unwrap_err(), CallbackError::Provider);
    assert_eq!(
        callback(&p, &st.state, Some(&st.state)).await.unwrap_err(),
        CallbackError::State
    );
    let st = begin(&p, None).await;
    for code in [None, Some("")] {
        let st2 = begin(&p, None).await;
        let r = p
            .login
            .finish(Some(&st2.state), Some(&st2.state), code, false)
            .await;
        assert_eq!(r.unwrap_err(), CallbackError::Provider);
    }
    drop(st);
    assert_eq!(p.fake.token_calls(), 0);
}

#[tokio::test]
async fn pending_logins_are_capped_and_the_oldest_goes_first() {
    let p = Provider::new();
    let first = begin(&p, None).await;
    for _ in 0..4096 {
        begin(&p, None).await;
    }
    let last = begin(&p, None).await;
    assert_eq!(
        callback(&p, &first.state, Some(&first.state))
            .await
            .unwrap_err(),
        CallbackError::State
    );
    p.set_token(sign(&key(1), "k1", &claims("user-1", &last.nonce)));
    callback(&p, &last.state, Some(&last.state))
        .await
        .expect("newest survives");
}

// ── the ID token ────────────────────────────────────────────────────────────

async fn refused_then_control(p: &Provider, r: Result<web_login::Finished, CallbackError>) {
    assert_eq!(r.unwrap_err(), CallbackError::Token);
    assert_eq!(p.fake.token_calls(), 1);
    // The refusal is not sticky: the same process signs a correct token in.
    ok_sign_in(p).await;
}

#[tokio::test]
async fn a_token_with_the_wrong_nonce_is_refused() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| {
        c["nonce"] = json!("not-the-nonce-we-sent");
    })
    .await;
    refused_then_control(&p, r).await;
}

#[tokio::test]
async fn a_token_for_another_audience_is_refused() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| c["aud"] = json!("other")).await;
    refused_then_control(&p, r).await;
}

#[tokio::test]
async fn a_token_from_another_issuer_is_refused() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| {
        c["iss"] = json!("https://evil.example.test");
    })
    .await;
    refused_then_control(&p, r).await;
}

#[tokio::test]
async fn an_expired_token_is_refused() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| {
        c["iat"] = json!(now() - 7200);
        c["exp"] = json!(now() - 3600);
    })
    .await;
    refused_then_control(&p, r).await;
}

#[tokio::test]
async fn a_token_signed_by_an_unknown_key_is_refused() {
    let p = Provider::new();
    // Right kid, wrong key: the signature does not verify.
    let (_, r) = sign_in(&p, None, &key(7), "k1", |_| {}).await;
    refused_then_control(&p, r).await;
}

#[tokio::test]
async fn an_unknown_kid_refetches_once_and_is_then_refused() {
    let p = Provider::new();
    let (_, r) = sign_in(&p, None, &key(3), "k9", |_| {}).await;
    assert_eq!(r.unwrap_err(), CallbackError::Token);
    assert_eq!(p.fake.jwks_fetches(), 1, "one refetch, no loop");
}

#[tokio::test]
async fn a_kid_that_appears_after_a_rotation_is_accepted_after_one_refetch() {
    let p = Provider::new();
    ok_sign_in(&p).await;
    assert_eq!(p.fake.jwks_fetches(), 1);
    // The provider rotates to k2; the cached set only has k1.
    *p.fake.jwks.lock().unwrap() = jwks_for(&key(2), "k2");
    let (_, r) = sign_in(&p, None, &key(2), "k2", |_| {}).await;
    r.expect("accepted after the refetch");
    assert_eq!(p.fake.jwks_fetches(), 2);
    // The new key is cached now: a second token needs no refetch.
    let (_, r) = sign_in(&p, None, &key(2), "k2", |_| {}).await;
    r.expect("cached");
    assert_eq!(p.fake.jwks_fetches(), 2);
}

#[tokio::test]
async fn the_token_endpoint_refusing_the_code_is_refused_without_a_session() {
    let p = Provider::new();
    p.fake
        .refuse_token
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (_, r) = sign_in(&p, None, &key(1), "k1", |_| {}).await;
    assert_eq!(r.unwrap_err(), CallbackError::Upstream);
    assert_eq!(p.fake.jwks_fetches(), 0, "no token, so no key lookup");
    p.fake
        .refuse_token
        .store(false, std::sync::atomic::Ordering::SeqCst);
    ok_sign_in(&p).await;
}

// ── the session cookie ──────────────────────────────────────────────────────

#[tokio::test]
async fn an_altered_session_cookie_is_refused() {
    let before = Provider::keyed(SESSION_KEY);
    let f = ok_sign_in(&before).await;
    let mut b = f.session_cookie.clone().into_bytes();
    let i = b.len() - 5;
    b[i] = if b[i] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(b).unwrap();
    assert_ne!(tampered, f.session_cookie);
    let after = Provider::keyed(SESSION_KEY);
    for p in [&before, &after] {
        assert_eq!(restored(p, &tampered).await, None);
    }
    for junk in ["", "v1.", "v1.AAAA", "v2.abc", "not-a-cookie"] {
        assert_eq!(restored(&after, junk).await, None, "{junk:?}");
    }
    assert!(
        restored(&after, &f.session_cookie).await.is_some(),
        "control"
    );
}

#[tokio::test]
async fn a_cookie_sealed_under_another_key_is_refused() {
    let before = Provider::keyed(SESSION_KEY);
    let f = ok_sign_in(&before).await;
    let other = Provider::keyed(OTHER_KEY);
    assert_eq!(restored(&other, &f.session_cookie).await, None);
    assert_eq!(other.fake.jwks_fetches(), 0);
    assert_eq!(
        other
            .fake
            .discovery_fetches
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn a_cookie_sealed_for_another_client_issuer_or_cookie_name_is_refused() {
    let before = Provider::keyed(SESSION_KEY);
    let f = ok_sign_in(&before).await;
    let bound_elsewhere = [
        Config::new(ISSUER, "other-client", REDIRECT, STEM),
        Config::new("https://other.example.test", CLIENT_ID, REDIRECT, STEM),
        Config::new(ISSUER, CLIENT_ID, REDIRECT, "other"),
    ];
    for cfg in bound_elsewhere {
        let p = Provider::with_config(cfg.unwrap().with_session_key(SESSION_KEY).unwrap());
        assert_eq!(restored(&p, &f.session_cookie).await, None);
        assert_eq!(p.fake.jwks_fetches(), 0);
        assert_eq!(
            p.fake
                .discovery_fetches
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }
    let same = Provider::keyed(SESSION_KEY);
    assert!(
        restored(&same, &f.session_cookie).await.is_some(),
        "control"
    );
}

fn foreign_cookie(exp: u64) -> String {
    let tok = sign(&key(1), "k1", &claims("user-1", "n1"));
    let binding = Binding {
        key_hex: SESSION_KEY,
        cookie_name: "__Host-demo_sid",
        issuer: ISSUER,
        client_id: CLIENT_ID,
    };
    seal_cookie(
        &binding,
        &json!({"sid": "sid-1", "tok": tok, "nonce": "n1", "exp": exp}),
    )
}

#[tokio::test]
async fn a_cookie_past_its_expiry_is_refused() {
    let p = Provider::keyed(SESSION_KEY);
    assert_eq!(restored(&p, &foreign_cookie(now() - 10)).await, None);
    assert_eq!(p.fake.jwks_fetches(), 0);
    let live = restored(&p, &foreign_cookie(now() + 100)).await;
    assert_eq!(live.expect("control").subject, "user-1");
}

#[tokio::test]
async fn a_token_too_large_for_a_cookie_gets_no_session() {
    let p = Provider::keyed(SESSION_KEY);
    let (_, r) = sign_in(&p, None, &key(1), "k1", |c| {
        c["pad"] = json!("x".repeat(4000))
    })
    .await;
    assert_eq!(r.unwrap_err(), CallbackError::Upstream);
    ok_sign_in(&p).await;
}

#[tokio::test]
async fn a_session_survives_a_restart_with_the_same_key_and_is_verified_again() {
    let before = Provider::keyed(SESSION_KEY);
    let f = ok_sign_in(&before).await;
    let after = Provider::keyed(SESSION_KEY);
    assert_eq!(
        after.login.signed_in(&f.session_cookie),
        None,
        "not before restore"
    );
    let s = restored(&after, &f.session_cookie).await.expect("restored");
    assert_eq!(s.subject, "user-1");
    assert_eq!(s.expires_at, f.signed_in.expires_at);
    // The new process checked the token against the provider's keys again.
    assert_eq!(after.fake.jwks_fetches(), 1);
    // A second request uses the restored session and fetches nothing more.
    assert!(restored(&after, &f.session_cookie).await.is_some());
    assert_eq!(after.fake.jwks_fetches(), 1);
}

#[tokio::test]
async fn a_restore_fails_when_the_provider_no_longer_publishes_the_signing_key() {
    let before = Provider::keyed(SESSION_KEY);
    let f = ok_sign_in(&before).await;
    let after = Provider::keyed(SESSION_KEY);
    *after.fake.jwks.lock().unwrap() = jwks_for(&key(2), "k2");
    assert_eq!(restored(&after, &f.session_cookie).await, None);
    assert_eq!(after.fake.jwks_fetches(), 1);
}

#[tokio::test]
async fn without_a_configured_key_a_restart_ends_the_session() {
    assert!(!base_config().has_session_key());
    let a = Provider::new();
    let f = ok_sign_in(&a).await;
    let b = Provider::new();
    assert_eq!(restored(&b, &f.session_cookie).await, None);
    assert!(a.login.signed_in(&f.session_cookie).is_some(), "control");
}

#[tokio::test]
async fn a_session_does_not_outlive_its_id_token() {
    // A token that expires long before the 8 h session.
    let short = |c: &mut Value| c["exp"] = json!(now() + 2);
    let signed_in = Provider::keyed(SESSION_KEY);
    let (_, r) = sign_in(&signed_in, None, &key(1), "k1", short).await;
    let f = r.expect("signs in");
    assert!(f.signed_in.expires_at <= now() + 2);
    let other = Provider::keyed(SESSION_KEY);
    assert!(restored(&other, &f.session_cookie).await.is_some());
    assert!(signed_in.login.signed_in(&f.session_cookie).is_some());
    tokio::time::sleep(Duration::from_secs(3)).await;
    // Past the token's expiry neither the signing-in nor the restoring process serves it,
    // and a fresh process does not restore it.
    assert_eq!(restored(&signed_in, &f.session_cookie).await, None);
    assert_eq!(restored(&other, &f.session_cookie).await, None);
    assert_eq!(
        restored(&Provider::keyed(SESSION_KEY), &f.session_cookie).await,
        None
    );
}

#[tokio::test]
async fn logout_ends_the_session_in_this_process_and_restore_does_not_undo_it() {
    let p = Provider::keyed(SESSION_KEY);
    let f = ok_sign_in(&p).await;
    assert!(p.login.signed_in(&f.session_cookie).is_some());
    p.login.logout(&f.session_cookie);
    assert_eq!(p.login.signed_in(&f.session_cookie), None);
    // The cookie is still authentic, so restore would accept it; revocation must win.
    assert_eq!(restored(&p, &f.session_cookie).await, None);
    // A copy of the cookie stays valid in another process until it expires.
    let replica = Provider::keyed(SESSION_KEY);
    assert!(restored(&replica, &f.session_cookie).await.is_some());
}

// ── return_to ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_local_return_path_is_kept() {
    let p = Provider::new();
    for good in ["/view/reports", "/a?b=c", "/"] {
        let (_, r) = sign_in(&p, Some(good), &key(1), "k1", |_| {}).await;
        assert_eq!(r.unwrap().return_to, good);
    }
}

#[tokio::test]
async fn an_unsafe_return_path_falls_back_to_the_default() {
    let p = Provider::with_config(base_config().with_default_return_to("/desk").unwrap());
    for bad in [
        "//evil.example.test",
        "https://evil.example.test",
        "/\\evil",
        "view",
        "",
        "/a\nb",
        "/a\u{7f}b",
    ] {
        let (_, r) = sign_in(&p, Some(bad), &key(1), "k1", |_| {}).await;
        assert_eq!(r.unwrap().return_to, "/desk", "{bad:?}");
    }
}

// ── Debug ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn debug_output_shows_no_key_material_token_or_cookie() {
    let cfg = keyed_config(SESSION_KEY).with_client_secret("test-secret-value");
    let shown_cfg = format!("{cfg:?}");
    let p = Provider::with_config(cfg);
    let (_, r) = sign_in(&p, None, &key(1), "k1", with_person).await;
    let f = r.unwrap();
    let tok = p.fake.id_token.lock().unwrap().clone();
    let shown = [
        shown_cfg,
        format!("{:?}", p.login),
        format!("{f:?}"),
        format!("{:?}", f.signed_in),
        format!("{:?}", CallbackError::Token),
    ];
    for s in &shown {
        assert!(!s.contains("1, 1, 1") && !s.contains("0101"), "{s}");
        assert!(!s.contains("test-secret-value"), "{s}");
        assert!(!s.contains(&f.session_cookie) && !s.contains(&tok), "{s}");
        assert!(!s.contains("person@one.example.test"), "{s}");
    }
    assert!(shown[0].contains("SessionKey(redacted)"), "{}", shown[0]);
    assert!(shown[1].contains("SessionKey(redacted)"), "{}", shown[1]);
}

#[tokio::test]
async fn a_login_never_needs_a_key_to_be_shown() {
    let fake = Fake::new();
    let login = Login::new(base_config(), Box::new(Handle(fake))).unwrap();
    assert!(!format!("{login:?}").contains("0101"));
}
