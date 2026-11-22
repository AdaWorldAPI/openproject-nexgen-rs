// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A fake OIDC provider behind the `OidcHttp` seam, and the sign-in steps. Every value is
//! invented; tokens are signed with a real ES256 key and checked by the real verifier. Only
//! discovery, the JWKS document and the token endpoint are served from memory.
#![allow(dead_code)]

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::ecdsa::SigningKey;
use p256::ecdsa::signature::Signer;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use web_login::{CallbackError, Config, Finished, HttpFuture, Login, OidcHttp};

pub const ISSUER: &str = "https://idp.example.test";
pub const CLIENT_ID: &str = "web-login-test";
pub const REDIRECT: &str = "https://app.example.test/auth/callback";
pub const STEM: &str = "demo";
pub const AUTHORIZE: &str = "https://idp.example.test/oauth/v2/authorize";
pub const TOKEN: &str = "https://idp.example.test/oauth/v2/token";
pub const JWKS: &str = "https://idp.example.test/oauth/v2/keys";
pub const ROLES: &str = "urn:zitadel:iam:org:project:roles";
pub const ORG: &str = "urn:zitadel:iam:user:resourceowner:id";
pub const SESSION_KEY: &str = "0101010101010101010101010101010101010101010101010101010101010101";
pub const OTHER_KEY: &str = "0202020202020202020202020202020202020202020202020202020202020202";

pub struct Fake {
    pub jwks: Mutex<String>,
    pub jwks_fetches: AtomicUsize,
    pub discovery_fetches: AtomicUsize,
    pub forms: Mutex<Vec<Vec<(String, String)>>>,
    pub id_token: Mutex<String>,
    pub refuse_token: AtomicBool,
    pub discovery: Mutex<Value>,
}

impl Fake {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            jwks: Mutex::new(jwks_for(&key(1), "k1")),
            jwks_fetches: AtomicUsize::new(0),
            discovery_fetches: AtomicUsize::new(0),
            forms: Mutex::new(Vec::new()),
            id_token: Mutex::new(String::new()),
            refuse_token: AtomicBool::new(false),
            discovery: Mutex::new(json!({
                "issuer": ISSUER,
                "authorization_endpoint": AUTHORIZE,
                "token_endpoint": TOKEN,
                "jwks_uri": JWKS,
            })),
        })
    }

    pub fn jwks_fetches(&self) -> usize {
        self.jwks_fetches.load(Ordering::SeqCst)
    }

    pub fn token_calls(&self) -> usize {
        self.forms.lock().unwrap().len()
    }
}

pub struct Handle(pub Arc<Fake>);

impl OidcHttp for Handle {
    fn get<'a>(&'a self, url: &'a str) -> HttpFuture<'a> {
        Box::pin(async move {
            if url == format!("{ISSUER}/.well-known/openid-configuration") {
                self.0.discovery_fetches.fetch_add(1, Ordering::SeqCst);
                Ok(self.0.discovery.lock().unwrap().to_string().into_bytes())
            } else if url == JWKS {
                self.0.jwks_fetches.fetch_add(1, Ordering::SeqCst);
                Ok(self.0.jwks.lock().unwrap().clone().into_bytes())
            } else {
                Err(format!("unexpected GET {url}"))
            }
        })
    }

    fn post_form<'a>(&'a self, url: &'a str, form: &'a [(&'a str, &'a str)]) -> HttpFuture<'a> {
        Box::pin(async move {
            if url != TOKEN {
                return Err(format!("unexpected POST {url}"));
            }
            self.0.forms.lock().unwrap().push(
                form.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            );
            if self.0.refuse_token.load(Ordering::SeqCst) {
                return Err("status 400".to_string());
            }
            let t = self.0.id_token.lock().unwrap().clone();
            Ok(json!({ "id_token": t }).to_string().into_bytes())
        })
    }
}

pub fn key(seed: u8) -> SigningKey {
    SigningKey::from_slice(&[seed; 32]).expect("valid scalar")
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn jwks_for(sk: &SigningKey, kid: &str) -> String {
    let sec1 = sk.verifying_key().to_sec1_bytes();
    assert_eq!(sec1.len(), 65);
    json!({"keys": [{
        "kty": "EC", "crv": "P-256", "kid": kid, "use": "sig",
        "x": b64(&sec1[1..33]), "y": b64(&sec1[33..]),
    }]})
    .to_string()
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn claims(sub: &str, nonce: &str) -> Value {
    json!({
        "iss": ISSUER, "sub": sub, "aud": CLIENT_ID,
        "iat": now(), "exp": now() + 300, "nonce": nonce,
    })
}

pub fn sign(sk: &SigningKey, kid: &str, claims: &Value) -> String {
    let header = json!({"alg": "ES256", "kid": kid, "typ": "JWT"});
    let input = format!(
        "{}.{}",
        b64(header.to_string().as_bytes()),
        b64(claims.to_string().as_bytes())
    );
    let sig: p256::ecdsa::Signature = sk.sign(input.as_bytes());
    format!("{input}.{}", b64(&sig.to_bytes()))
}

pub fn base_config() -> Config {
    Config::new(ISSUER, CLIENT_ID, REDIRECT, STEM).expect("config")
}

pub fn keyed_config(key: &str) -> Config {
    base_config().with_session_key(key).expect("key")
}

/// One process: its own `Login` and its own fake provider (same signing key), so two
/// providers model a restart or a second replica.
pub struct Provider {
    pub fake: Arc<Fake>,
    pub login: Arc<Login>,
}

impl Provider {
    pub fn with_config(cfg: Config) -> Self {
        let fake = Fake::new();
        let login =
            Arc::new(Login::new(cfg, Box::new(Handle(fake.clone()))).expect("login builds"));
        Self { fake, login }
    }

    /// No configured key: a random one per process.
    pub fn new() -> Self {
        Self::with_config(base_config())
    }

    pub fn keyed(key: &str) -> Self {
        Self::with_config(keyed_config(key))
    }

    pub fn set_token(&self, t: String) {
        *self.fake.id_token.lock().unwrap() = t;
    }
}

pub fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// The decoded query parameters of a redirect `Location`.
pub fn query(location: &str) -> HashMap<String, String> {
    let (_, q) = location.split_once('?').expect("location has a query");
    q.split('&')
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap();
            (pct_decode(k), pct_decode(v))
        })
        .collect()
}

pub struct Started {
    pub url: String,
    pub state: String,
    pub nonce: String,
}

pub async fn begin(p: &Provider, return_to: Option<&str>) -> Started {
    let (url, state) = p.login.start(return_to).await.expect("start works");
    let q = query(&url);
    assert_eq!(q["state"], state);
    Started {
        nonce: q["nonce"].clone(),
        url,
        state,
    }
}

pub async fn callback(
    p: &Provider,
    query_state: &str,
    cookie_state: Option<&str>,
) -> Result<Finished, CallbackError> {
    p.login
        .finish(cookie_state, Some(query_state), Some("c1"), false)
        .await
}

/// A whole sign-in: begin, have the provider sign a token (after `tweak`), call back.
pub async fn sign_in(
    p: &Provider,
    return_to: Option<&str>,
    sk: &SigningKey,
    kid: &str,
    tweak: impl FnOnce(&mut Value),
) -> (Started, Result<Finished, CallbackError>) {
    let st = begin(p, return_to).await;
    let mut c = claims("user-1", &st.nonce);
    tweak(&mut c);
    p.set_token(sign(sk, kid, &c));
    let r = callback(p, &st.state, Some(&st.state)).await;
    (st, r)
}

pub async fn ok_sign_in(p: &Provider) -> Finished {
    sign_in(p, None, &key(1), "k1", |_| {})
        .await
        .1
        .expect("sign-in works")
}

/// What a sealed cookie is bound to: the key and the additional data parts.
pub struct Binding<'a> {
    pub key_hex: &'a str,
    pub cookie_name: &'a str,
    pub issuer: &'a str,
    pub client_id: &'a str,
}

/// Seal a session cookie the way the library does, for cookies the library would never
/// produce itself (an old expiry, another binding). `body` is `{sid, tok, nonce, exp}`.
pub fn seal_cookie(b: &Binding, body: &Value) -> String {
    use ogar_encryption::aead::{NONCE_LEN, seal_with_key};
    let key: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&b.key_hex[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    let key: [u8; 32] = key.try_into().unwrap();
    let aad = format!(
        "web-login session v1|{}|{}|{}",
        b.cookie_name, b.issuer, b.client_id
    )
    .into_bytes();
    let plain = body.to_string().into_bytes();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).unwrap();
    let ct = seal_with_key(&key, &nonce_bytes, &aad, &plain).unwrap();
    let mut raw = nonce_bytes.to_vec();
    raw.extend_from_slice(&ct);
    format!("v1.{}", b64(&raw))
}
