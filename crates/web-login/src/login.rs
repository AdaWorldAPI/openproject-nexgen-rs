// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The sign-in state of one process: the flow, the sessions and the sealed cookie.

use crate::config::{Config, ConfigError, LOGIN_TTL, SESSION_TTL};
use crate::http::OidcHttp;
use crate::token::{token_exp, token_org, token_roles};
use crate::util::{now_unix, random_token, urlencode};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ogar_auth::federation::{
    AssertionVerifier, LoginError, TrustedIssuers, VerifiedClaims, VerifiedLogin, establish,
};
use ogar_auth::identity::ExternalIdentity;
use ogar_auth::user::{ProviderId, User, UserId, UserStore};
use ogar_encryption::aead::{NONCE_LEN, open_with_key, seal_with_key};
use ogar_oidc::{Client, Discovery, IdToken, KeySet, OidcError, OidcVerifier};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_PENDING: usize = 4096;
const MAX_SESSIONS: usize = 10_000;
/// Browsers keep cookies up to about 4096 bytes including name and attributes.
const MAX_SESSION_COOKIE: usize = 3800;
/// Version prefix of the sealed session cookie.
const SEALED_V1: &str = "v1.";
/// Provider label recorded on the login; never an authorization input.
const PROVIDER: ProviderId = ProviderId("oidc");
/// Subject prefix of the canonical user a login resolves to.
const USER_SUBJECT_PREFIX: &str = "oidc:";

/// A verified identity and what the provider assigned it. Every field comes from an ID
/// token the verifier accepted (signature, issuer, audience, expiry, nonce). Nothing here
/// says what the person may do: that is the consumer's decision.
#[derive(Clone, PartialEq, Eq)]
pub struct SignedIn {
    pub issuer: String,
    pub subject: String,
    /// The user's organization at the provider, when the token names a plain one.
    pub org: Option<String>,
    /// `(organization id, role key)` pairs from the roles claim, sorted.
    pub roles: Vec<(String, String)>,
    /// The email of the token, only when the provider marked it verified.
    pub email: Option<String>,
    /// End of the session, unix seconds: the session lifetime or the ID token's expiry,
    /// whichever comes first.
    pub expires_at: u64,
}

impl fmt::Debug for SignedIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedIn")
            .field("issuer", &self.issuer)
            .field("subject", &self.subject)
            .field("org", &self.org)
            .field("roles", &self.roles)
            .field("email", &self.email.as_ref().map(|_| "<present>"))
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Why a callback was refused. Only the class is ever shown or logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackError {
    /// No or mismatching login cookie, unknown or expired `state`.
    State,
    /// The provider returned an error instead of a code.
    Provider,
    /// Discovery, JWKS or the token endpoint failed.
    Upstream,
    /// The ID token did not verify.
    Token,
    /// `establish` found no user for a verified identity (never expected: the user store
    /// is built from the verified token).
    Unbound,
    /// The consumer's admission check refused a verified identity.
    NoAccess,
}

impl CallbackError {
    pub fn class(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Provider => "provider",
            Self::Upstream => "upstream",
            Self::Token => "token",
            Self::Unbound => "unbound",
            Self::NoAccess => "admission",
        }
    }
}

/// A finished sign-in: the session cookie value, where to send the browser and who signed in.
pub struct Finished {
    pub session_cookie: String,
    pub return_to: String,
    pub signed_in: SignedIn,
}

impl fmt::Debug for Finished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Finished")
            .field("session_cookie", &"<redacted>")
            .field("return_to", &self.return_to)
            .field("signed_in", &self.signed_in)
            .finish()
    }
}

/// Endpoints read from discovery.
#[derive(Debug, Clone)]
struct Endpoints {
    authorization: String,
    token: String,
    jwks: String,
}

struct Pending {
    verifier: String,
    nonce: String,
    return_to: String,
    created: SystemTime,
}

/// What the sealed cookie carries: enough for `establish` to run again, nothing else.
#[derive(Serialize, Deserialize)]
struct Sealed {
    /// Random session id: the key of the per-process cache and of revocation.
    sid: String,
    /// The ID token `establish` accepted at sign-in.
    tok: String,
    /// The nonce that token was issued for.
    nonce: String,
    /// Session expiry, unix seconds.
    exp: u64,
}

struct Session {
    signed_in: SignedIn,
    /// The verified login the session was created from, so a consumer that needs the
    /// canonical user gets the value `establish` produced, never one rebuilt from fields.
    login: VerifiedLogin,
    expires: SystemTime,
}

/// What the verified token says besides the identity.
struct Verified {
    login: VerifiedLogin,
    org: Option<String>,
    roles: Vec<(String, String)>,
    email: Option<String>,
}

/// The one-user store a login resolves against: `user` for exactly `identity`. Every
/// verified identity of the trusted issuer becomes its canonical user.
struct GrantedStore {
    identity: ExternalIdentity,
    user: User,
}

impl UserStore for GrantedStore {
    fn resolve(&self, _binding: &ogar_auth::user::AuthBinding) -> Option<&User> {
        None
    }
    fn user(&self, id: UserId) -> Option<&User> {
        (id == self.user.id).then_some(&self.user)
    }
    fn resolve_external(&self, id: &ExternalIdentity) -> Option<&User> {
        (id == &self.identity).then_some(&self.user)
    }
}

/// A session ends at the session lifetime or at its ID token's expiry, whichever comes
/// first, in every process: a replica that established the session itself must not keep
/// it longer than one that has to restore it.
fn session_exp(start: u64, token: &str) -> Option<u64> {
    Some((start + SESSION_TTL.as_secs()).min(token_exp(token)?))
}

fn https(url: &str) -> Result<String, CallbackError> {
    if url.starts_with("https://") {
        Ok(url.to_string())
    } else {
        Err(CallbackError::Upstream)
    }
}

/// The sign-in state of one process.
pub struct Login {
    cfg: Config,
    http: Box<dyn OidcHttp>,
    trusted: TrustedIssuers,
    endpoints: RwLock<Option<Endpoints>>,
    verifier: RwLock<OidcVerifier>,
    /// Client ids whose bearer tokens [`Login::verify_bearer`] accepts.
    bearer_clients: Vec<String>,
    /// The issuer's keys as the bearer path last saw them, shared by every client id.
    bearer_keys: RwLock<KeySet>,
    pending: Mutex<HashMap<String, Pending>>,
    /// Sessions this process has established or restored, by session id.
    sessions: Mutex<HashMap<String, Session>>,
    /// Session ids signed out in this process, with their expiry.
    revoked: Mutex<HashMap<String, u64>>,
    key: [u8; 32],
    aad: Vec<u8>,
}

impl fmt::Debug for Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Login")
            .field("issuer", &self.cfg.issuer().as_str())
            .field("client_id", &self.cfg.client_id())
            .field("session_cookie", &self.cfg.session_cookie())
            .field("key", &"SessionKey(redacted)")
            .finish_non_exhaustive()
    }
}

impl Login {
    pub fn new(cfg: Config, http: Box<dyn OidcHttp>) -> Result<Self, ConfigError> {
        let mut trusted = TrustedIssuers::new();
        trusted
            .insert(cfg.issuer().clone(), PROVIDER)
            .map_err(|_| ConfigError::new("issuer registered twice"))?;
        let key = match cfg.session_key() {
            Some(k) => k.0,
            None => {
                eprintln!("web-login: no session key configured: sessions end at a restart");
                let mut k = [0u8; 32];
                getrandom::getrandom(&mut k).expect("OS entropy available");
                k
            }
        };
        let aad = format!(
            "web-login session v1|{}|{}|{}",
            cfg.session_cookie(),
            cfg.issuer().as_str(),
            cfg.client_id()
        )
        .into_bytes();
        let mut verifier = OidcVerifier::new();
        verifier.add_client(Client::new(
            cfg.issuer().clone(),
            cfg.client_id().to_string(),
            KeySet::default(),
        ));
        Ok(Self {
            cfg,
            http,
            trusted,
            endpoints: RwLock::new(None),
            verifier: RwLock::new(verifier),
            bearer_clients: Vec::new(),
            bearer_keys: RwLock::new(KeySet::default()),
            pending: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            revoked: Mutex::new(HashMap::new()),
            key,
            aad,
        })
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// The client ids whose tokens [`Login::verify_bearer`] accepts. With none, every
    /// bearer token is refused.
    pub fn accept_bearer_clients(&mut self, client_ids: &[String]) {
        self.bearer_clients = client_ids.to_vec();
    }

    fn seal(&self, s: &Sealed) -> Option<String> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce).ok()?;
        let plain = serde_json::to_vec(s).ok()?;
        let ct = seal_with_key(&self.key, &nonce, &self.aad, &plain).ok()?;
        let mut raw = nonce.to_vec();
        raw.extend_from_slice(&ct);
        Some(format!("{SEALED_V1}{}", URL_SAFE_NO_PAD.encode(raw)))
    }

    /// Open a session cookie: authentic, unexpired and not signed out here.
    fn open(&self, cookie: &str) -> Option<Sealed> {
        let raw = URL_SAFE_NO_PAD
            .decode(cookie.strip_prefix(SEALED_V1)?)
            .ok()?;
        if raw.len() <= NONCE_LEN {
            return None;
        }
        let (nonce, ct) = raw.split_at(NONCE_LEN);
        let nonce: &[u8; NONCE_LEN] = nonce.try_into().ok()?;
        let plain = open_with_key(&self.key, nonce, &self.aad, ct).ok()?;
        let s: Sealed = serde_json::from_slice(&plain).ok()?;
        if s.exp <= now_unix() || self.revoked.lock().expect("lock").contains_key(&s.sid) {
            return None;
        }
        Some(s)
    }

    fn remember(&self, sid: String, session: Session) {
        let mut s = self.sessions.lock().expect("lock");
        let now = SystemTime::now();
        s.retain(|_, v| v.expires > now);
        if s.len() >= MAX_SESSIONS
            && let Some(oldest) = s
                .iter()
                .min_by_key(|(_, v)| v.expires)
                .map(|(k, _)| k.clone())
        {
            s.remove(&oldest);
        }
        s.insert(sid, session);
    }

    /// Verify an ID token, refetching the JWKS once on an unknown key, and resolve its
    /// `(iss, sub)` to the canonical user through `establish`.
    async fn verify_token(
        &self,
        token: &str,
        nonce: &str,
        jwks: &str,
    ) -> Result<Verified, CallbackError> {
        let input = IdToken {
            token: token.to_string(),
            now_unix: now_unix(),
            nonce: Some(nonce.to_string()),
        };
        let verify = |input: &IdToken| {
            let v = self.verifier.read().expect("lock");
            AssertionVerifier::verify(&*v, &self.trusted, input)
        };
        let claims = match verify(&input) {
            Err(OidcError::UnknownKey) => {
                self.refresh_keys(jwks).await?;
                verify(&input)
            }
            other => other,
        }
        .map_err(|_| CallbackError::Token)?;
        let v = self.verifier.read().expect("lock");
        self.resolve(&v, &input, claims)
    }

    /// The claims `verifier` accepted, read through the token and resolved by `establish`.
    fn resolve(
        &self,
        verifier: &OidcVerifier,
        input: &IdToken,
        claims: VerifiedClaims,
    ) -> Result<Verified, CallbackError> {
        let token = input.token.as_str();
        let org = token_org(token, self.cfg.org_claim());
        let roles = token_roles(token, self.cfg.roles_claim());
        let store = GrantedStore {
            identity: claims.identity.clone(),
            user: User {
                id: UserId(0),
                subject: format!(
                    "{USER_SUBJECT_PREFIX}{}",
                    claims.identity.subject().as_str()
                ),
                tenant: org.as_deref().and_then(|o| o.parse().ok()).unwrap_or(0),
                roles: roles.iter().map(|(o, r)| format!("{o}/{r}")).collect(),
                memberships: Vec::new(),
                bindings: Vec::new(),
                key_refs: Vec::new(),
            },
        };
        match establish(verifier, &self.trusted, input, &store) {
            Ok(login) => Ok(Verified {
                login,
                org,
                roles,
                email: claims.email,
            }),
            Err(LoginError::Unbound) => Err(CallbackError::Unbound),
            Err(_) => Err(CallbackError::Token),
        }
    }

    /// Whether any client id was given to [`Login::accept_bearer_clients`].
    pub fn accepts_bearer(&self) -> bool {
        !self.bearer_clients.is_empty()
    }

    /// Verify a token a client presents as a bearer: the same checks as an ID token
    /// (signature, issuer, audience, expiry), without a nonce and without a session. It is
    /// accepted when it is addressed to one of the clients of
    /// [`Login::accept_bearer_clients`]. The JWKS is refetched once on an unknown key.
    pub async fn verify_bearer(&self, token: &str) -> Result<SignedIn, CallbackError> {
        if self.bearer_clients.is_empty() {
            return Err(CallbackError::Token);
        }
        let input = IdToken {
            token: token.to_string(),
            now_unix: now_unix(),
            nonce: None,
        };
        let verified = match self.verify_as_client(&input) {
            Err(OidcError::UnknownKey) => {
                let ep = self.endpoints().await?;
                self.refresh_keys(&ep.jwks).await?;
                self.verify_as_client(&input)
            }
            other => other,
        }
        .map_err(|_| CallbackError::Token)?;
        let exp = token_exp(token).ok_or(CallbackError::Token)?;
        Ok(Self::session_of(verified, exp).0)
    }

    /// Try the token against one verifier per accepted client id, all over the shared keys.
    fn verify_as_client(&self, input: &IdToken) -> Result<Verified, OidcError> {
        let keys = self.bearer_keys.read().expect("lock").clone();
        #[cfg(feature = "disable-api-client-check")]
        let audiences = crate::token::token_audiences(&input.token);
        #[cfg(not(feature = "disable-api-client-check"))]
        let audiences: Vec<String> = Vec::new();
        let mut unknown_key = false;
        for client_id in self.bearer_clients.iter().chain(&audiences) {
            let mut verifier = OidcVerifier::new();
            verifier.add_client(Client::new(
                self.cfg.issuer().clone(),
                client_id.clone(),
                keys.clone(),
            ));
            match AssertionVerifier::verify(&verifier, &self.trusted, input) {
                Ok(claims) => {
                    return self
                        .resolve(&verifier, input, claims)
                        .map_err(|_| OidcError::MissingClaim);
                }
                Err(OidcError::UnknownKey) => unknown_key = true,
                Err(_) => {}
            }
        }
        Err(if unknown_key {
            OidcError::UnknownKey
        } else {
            OidcError::AudienceMismatch
        })
    }

    fn session_of(v: Verified, expires_at: u64) -> (SignedIn, VerifiedLogin) {
        let signed_in = SignedIn {
            issuer: v.login.identity().issuer().as_str().to_string(),
            subject: v.login.identity().subject().as_str().to_string(),
            org: v.org,
            roles: v.roles,
            email: v.email,
            expires_at,
        };
        (signed_in, v.login)
    }

    /// Make a session cookie this process has not seen usable here: open it and run
    /// `establish` again on the token it carries. A no-op for a known or unusable cookie.
    pub async fn restore(&self, cookie: &str) {
        let Some(s) = self.open(cookie) else {
            return;
        };
        if self.sessions.lock().expect("lock").contains_key(&s.sid) {
            return;
        }
        let Ok(ep) = self.endpoints().await else {
            return;
        };
        let Ok(verified) = self.verify_token(&s.tok, &s.nonce, &ep.jwks).await else {
            return;
        };
        // Already capped at sign-in; capped again so a cookie can never outlive the token
        // it carries.
        let exp = s.exp.min(token_exp(&s.tok).unwrap_or(0));
        let (signed_in, login) = Self::session_of(verified, exp);
        self.remember(
            s.sid,
            Session {
                signed_in,
                login,
                expires: UNIX_EPOCH + Duration::from_secs(exp),
            },
        );
    }

    fn live<T>(&self, cookie: &str, pick: impl FnOnce(&Session) -> T) -> Option<T> {
        let sealed = self.open(cookie)?;
        let s = self.sessions.lock().expect("lock");
        s.get(&sealed.sid)
            .filter(|v| v.expires > SystemTime::now())
            .map(pick)
    }

    async fn endpoints(&self) -> Result<Endpoints, CallbackError> {
        if let Some(e) = self.endpoints.read().expect("lock").clone() {
            return Ok(e);
        }
        let url = format!(
            "{}/.well-known/openid-configuration",
            self.cfg.issuer().as_str().trim_end_matches('/')
        );
        let doc = self
            .http
            .get(&url)
            .await
            .map_err(|_| CallbackError::Upstream)?;
        let d =
            Discovery::from_json(self.cfg.issuer(), &doc).map_err(|_| CallbackError::Upstream)?;
        let v: serde_json::Value =
            serde_json::from_slice(&doc).map_err(|_| CallbackError::Upstream)?;
        let field = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_str())
                .ok_or(CallbackError::Upstream)
                .and_then(https)
        };
        let e = Endpoints {
            authorization: field("authorization_endpoint")?,
            token: field("token_endpoint")?,
            jwks: d.jwks_uri,
        };
        *self.endpoints.write().expect("lock") = Some(e.clone());
        Ok(e)
    }

    async fn refresh_keys(&self, jwks_uri: &str) -> Result<(), CallbackError> {
        let doc = self
            .http
            .get(jwks_uri)
            .await
            .map_err(|_| CallbackError::Upstream)?;
        let keys = KeySet::from_jwks_json(&doc).map_err(|_| CallbackError::Upstream)?;
        *self.bearer_keys.write().expect("lock") = keys.clone();
        let mut v = self.verifier.write().expect("lock");
        if let Some(c) = v.client_mut(self.cfg.issuer()) {
            c.set_keys(keys);
        }
        Ok(())
    }

    /// Start a login. Returns the authorization URL and the `state` for the login cookie.
    pub async fn start(&self, return_to: Option<&str>) -> Result<(String, String), CallbackError> {
        let ep = self.endpoints().await?;
        let state = random_token();
        let nonce = random_token();
        let verifier = random_token();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let return_to = return_to
            .filter(|p| crate::util::safe_return_to(p))
            .unwrap_or(self.cfg.default_return_to())
            .to_string();
        {
            let mut p = self.pending.lock().expect("lock");
            p.retain(|_, v| v.created.elapsed().map(|e| e < LOGIN_TTL).unwrap_or(false));
            if p.len() >= MAX_PENDING
                && let Some(oldest) = p
                    .iter()
                    .min_by_key(|(_, v)| v.created)
                    .map(|(k, _)| k.clone())
            {
                p.remove(&oldest);
            }
            p.insert(
                state.clone(),
                Pending {
                    verifier,
                    nonce: nonce.clone(),
                    return_to,
                    created: SystemTime::now(),
                },
            );
        }
        let scope = format!("openid {}", self.cfg.extra_scopes());
        let q = [
            ("response_type", "code"),
            ("client_id", self.cfg.client_id()),
            ("redirect_uri", self.cfg.redirect_url()),
            ("scope", scope.trim()),
            ("state", state.as_str()),
            ("nonce", nonce.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ];
        let query: String = q
            .iter()
            .map(|(k, v)| format!("{k}={}", urlencode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let sep = if ep.authorization.contains('?') {
            '&'
        } else {
            '?'
        };
        Ok((format!("{}{sep}{query}", ep.authorization), state))
    }

    /// Finish a login. `cookie_state` is the login cookie, `query_state`/`code` the
    /// callback query.
    pub async fn finish(
        &self,
        cookie_state: Option<&str>,
        query_state: Option<&str>,
        code: Option<&str>,
        provider_error: bool,
    ) -> Result<Finished, CallbackError> {
        self.finish_admitting(cookie_state, query_state, code, provider_error, &|_| true)
            .await
    }

    /// [`Login::finish`] with the consumer's admission check. It runs on the verified
    /// identity before any session exists; `false` is [`CallbackError::NoAccess`].
    pub async fn finish_admitting(
        &self,
        cookie_state: Option<&str>,
        query_state: Option<&str>,
        code: Option<&str>,
        provider_error: bool,
        admit: &(dyn Fn(&SignedIn) -> bool + Sync),
    ) -> Result<Finished, CallbackError> {
        let (Some(cs), Some(qs)) = (cookie_state, query_state) else {
            return Err(CallbackError::State);
        };
        if !cfg!(feature = "disable-state-check") && cs != qs {
            return Err(CallbackError::State);
        }
        let pending = self
            .pending
            .lock()
            .expect("lock")
            .remove(qs)
            .ok_or(CallbackError::State)?;
        if !pending
            .created
            .elapsed()
            .map(|e| e < LOGIN_TTL)
            .unwrap_or(false)
        {
            return Err(CallbackError::State);
        }
        if provider_error {
            return Err(CallbackError::Provider);
        }
        let code = code
            .filter(|c| !c.is_empty())
            .ok_or(CallbackError::Provider)?;
        let ep = self.endpoints().await?;
        let mut form: Vec<(&str, &str)> = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.cfg.redirect_url()),
            ("client_id", self.cfg.client_id()),
            ("code_verifier", pending.verifier.as_str()),
        ];
        if let Some(s) = self.cfg.client_secret() {
            form.push(("client_secret", s));
        }
        let resp = self
            .http
            .post_form(&ep.token, &form)
            .await
            .map_err(|_| CallbackError::Upstream)?;
        let v: serde_json::Value =
            serde_json::from_slice(&resp).map_err(|_| CallbackError::Upstream)?;
        let token = v
            .get("id_token")
            .and_then(|t| t.as_str())
            .ok_or(CallbackError::Upstream)?;
        let verified = self.verify_token(token, &pending.nonce, &ep.jwks).await?;
        let sid = random_token();
        let now = now_unix();
        let exp = session_exp(now, token)
            .filter(|exp| *exp > now)
            .ok_or(CallbackError::Token)?;
        let (signed_in, login) = Self::session_of(verified, exp);
        if !admit(&signed_in) {
            return Err(CallbackError::NoAccess);
        }
        let cookie = self
            .seal(&Sealed {
                sid: sid.clone(),
                tok: token.to_string(),
                nonce: pending.nonce.clone(),
                exp,
            })
            .ok_or(CallbackError::Upstream)?;
        if cookie.len() > MAX_SESSION_COOKIE {
            eprintln!("web-login: the ID token is too large for a session cookie");
            return Err(CallbackError::Upstream);
        }
        self.remember(
            sid,
            Session {
                signed_in: signed_in.clone(),
                login,
                expires: UNIX_EPOCH + Duration::from_secs(exp),
            },
        );
        Ok(Finished {
            session_cookie: cookie,
            return_to: pending.return_to,
            signed_in,
        })
    }

    /// Who a live session cookie belongs to. A cookie from before a restart answers only
    /// after [`Login::restore`] ran for it.
    pub fn signed_in(&self, cookie: &str) -> Option<SignedIn> {
        self.live(cookie, |s| s.signed_in.clone())
    }

    /// The verified login of a live session cookie.
    pub fn verified(&self, cookie: &str) -> Option<VerifiedLogin> {
        self.live(cookie, |s| s.login.clone())
    }

    /// Sign a session out in this process.
    pub fn logout(&self, cookie: &str) {
        if let Some(s) = self.open(cookie) {
            self.sessions.lock().expect("lock").remove(&s.sid);
            let now = now_unix();
            let mut r = self.revoked.lock().expect("lock");
            r.retain(|_, exp| *exp > now);
            r.insert(s.sid, s.exp);
        }
    }
}
