// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Browser sign-in against an OpenID Connect provider: authorization code flow with PKCE
//! on `ogar-oidc`, `ogar_auth::federation::establish`, and a sealed session cookie.
//!
//! The crate answers one question: which verified identity, with which provider-assigned
//! grants, sent this request ([`SignedIn`]). It does not decide what that person may do.
//!
//! 1. [`Login::start`] makes a `state`, a `nonce` and a PKCE verifier, keeps them
//!    server-side and returns the authorization URL (`code_challenge_method=S256`); the
//!    caller sets a short-lived cookie holding `state`.
//! 2. [`Login::finish`] requires the cookie's `state` to equal the query's, consumes the
//!    pending login (single use), exchanges the code with the verifier, verifies the ID
//!    token (signature, issuer, audience, expiry, nonce) and runs `establish`. Only then a
//!    session exists.
//! 3. The session cookie is sealed with XChaCha20-Poly1305 (`ogar-encryption`) and carries
//!    the ID token, its nonce, a random session id and the session expiry. The additional
//!    data binds the cookie name, issuer and client id. A process that sees a cookie it does
//!    not know yet opens it and runs `establish` again ([`Login::restore`]), so a session
//!    survives a restart when the key is configured; without a key every process seals
//!    under its own random one. A session never outlives its ID token. Signing out revokes
//!    the session in this process; a copy of the cookie stays valid in another replica or
//!    after a restart until it expires.
//!
//! A service that is called by another service with a user's access token uses
//! [`Login::verify_bearer`] instead: the same verification for the clients named in
//! [`Login::accept_bearer_clients`], without a nonce and without a session.
//!
//! Discovery and the JWKS are fetched lazily and cached; an unknown `kid` refetches the
//! JWKS once.
//!
//! ID tokens must be signed with ES256 or EdDSA. RS256 is refused: `ogar-oidc` accepts it
//! only behind an opt-in this crate does not take, as in report-rs. A provider that signs
//! with RS256 has to be switched to one of the two.
//!
//! ## Differences from the report-rs sign-in this was lifted from
//!
//! - A token that names no organization and no role is a valid sign-in here
//!   ([`SignedIn::org`] is `None`, [`SignedIn::roles`] empty). report-rs refused it as
//!   `unassigned`; refusing it is now the consumer's decision.
//! - Every verified identity of the trusted issuer becomes its canonical user (the
//!   equivalent of report-rs's granted store). There are no bootstrap bindings, no
//!   membership grant key and no admission policy.
//! - Role pairs are `(organization id, role key)` instead of `org/role` strings; the
//!   same filters apply (empty role key, a role key of `*`, an empty organization id or
//!   one containing `/` yield no pair).
//!
//! Secrets (the session key, the client secret, tokens, codes, verifiers, nonces and
//! cookie values) never appear in `Debug` output, logs or error text.

#[cfg(all(feature = "disable-state-check", not(debug_assertions)))]
compile_error!("disable-state-check switches a sign-in check off and exists for test runs only");

#[cfg(all(feature = "disable-api-client-check", not(debug_assertions)))]
compile_error!(
    "disable-api-client-check switches a bearer check off and exists for test runs only"
);

mod config;
mod http;
mod login;
mod token;
mod util;

#[cfg(feature = "axum")]
mod routes;

pub use config::{
    Config, ConfigError, DEFAULT_EXTRA_SCOPES, DEFAULT_ORG_CLAIM, DEFAULT_ROLES_CLAIM, LOGIN_TTL,
    SESSION_TTL,
};
#[cfg(feature = "reqwest")]
pub use http::ReqwestHttp;
pub use http::{HttpFuture, OidcHttp};
pub use login::{CallbackError, Finished, Login, SignedIn};
pub use util::{cookie, safe_return_to, set_cookie, urlencode};

#[cfg(feature = "axum")]
pub use routes::{Admit, routes, routes_with, signed_in_from_headers};
