//! Guards for the state-changing pages: who may write, and proof the write was meant.
//!
//! Two independent checks sit in front of every page mutation:
//!
//! 1. **Authorization** ([`Principal`] + [`may`]) mirrors OpenProject's
//!    `edit_work_packages` rule (`app/contracts/work_packages/update_contract.rb`,
//!    `update_allowed?` line 35-36 and `user_allowed_to_edit` line 83-89): the user must hold the
//!    permission through a role of their membership *in the work package's project*, admins
//!    included only while the project is active (OpenProject's `allowed_in_project?` is false for
//!    archived projects). A member of project A holds nothing in project B.
//! 2. **CSRF** ([`token_for`], [`token_valid`]): a signed double-submit token. The browser holds a
//!    random `op_csrf` cookie; the form carries `HMAC-SHA256(server secret, cookie, actor)`. A
//!    cross-site form cannot read the cookie, so it cannot compute the token; the actor binding
//!    ties the token to the signed-in person, so a token minted for one person is useless for
//!    another. The comparison is constant-time.
//!
//! Everything decision-shaped here is pure and tested without a database; the DB access sits
//! behind [`WpStore`] (real impl: [`PgWpStore`]).

use std::sync::OnceLock;

use axum::http::{header, HeaderMap};
use hmac::{Hmac, Mac};
use op_db::repository::Repository;
use op_db::{
    PriorityRepository, PriorityRow, StatusRepository, StatusRow, TypeRepository, TypeRow,
    UpdateWorkPackageDto, UserRepository, UserRow,
};
use op_services::work_packages::mutation::{
    self, Caller, MutationError, MutationStore, PgMutationStore, StoreFailure,
};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;
use sqlx::PgPool;

use crate::signin::Who;

/// The permission that gates a project edit.
pub const EDIT_PROJECT: &str = "edit_project";

/// Name of the CSRF cookie.
pub const CSRF_COOKIE: &str = "op_csrf";
/// Name of the hidden form field the render-bake form emits for the token.
pub const CSRF_FIELD: &str = "authenticity_token";

// ── CSRF ─────────────────────────────────────────────────────────────

/// HMAC-SHA256 over `msg` with `key` (RustCrypto `hmac`).
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(msg);
    mac.finalize().into_bytes().into()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Decode lower- or upper-case hex; `None` for anything else.
fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// The server-side CSRF secret: `OP_CSRF_SECRET` (hashed, so any length) when set, else 32 random
/// bytes for this process. A per-process secret means tokens die on restart and are not shared
/// between instances; set `OP_CSRF_SECRET` on a multi-instance deploy.
pub fn secret() -> &'static [u8; 32] {
    static SECRET: OnceLock<[u8; 32]> = OnceLock::new();
    SECRET.get_or_init(|| {
        match std::env::var("OP_CSRF_SECRET")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            Some(s) => Sha256::digest(s.as_bytes()).into(),
            None => {
                let mut b = [0u8; 32];
                getrandom::getrandom(&mut b).expect("OS entropy available");
                b
            }
        }
    })
}

/// A fresh random cookie value: 32 random bytes, hex (64 chars).
pub fn new_cookie_value() -> String {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).expect("OS entropy available");
    hex(&b)
}

/// A cookie value is well-formed when it is exactly what [`new_cookie_value`] writes. Anything
/// else (absent, truncated, junk set by another party) is treated as no cookie.
fn cookie_well_formed(v: &str) -> bool {
    v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit())
}

/// The CSRF cookie of a request, when present and well-formed.
pub fn cookie_from_headers(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    web_login::cookie(Some(raw), CSRF_COOKIE)
        .filter(|v| cookie_well_formed(v))
        .map(str::to_owned)
}

/// `Set-Cookie` value for the CSRF cookie. `HttpOnly` (script never needs it), `SameSite=Strict`
/// (never sent on a cross-site request), `Secure` when the deploy is HTTPS.
pub fn set_cookie_value(value: &str, secure: bool) -> String {
    format!(
        "{CSRF_COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict{}",
        if secure { "; Secure" } else { "" }
    )
}

/// Whether the deploy is served over HTTPS (`PUBLIC_URL=https://…` or Railway's injected domain).
pub fn deploy_is_https() -> bool {
    std::env::var("PUBLIC_URL").is_ok_and(|u| u.starts_with("https://"))
        || std::env::var("RAILWAY_PUBLIC_DOMAIN").is_ok_and(|d| !d.trim().is_empty())
}

/// The identity a token is bound to: the signed-in actor, or `anonymous`.
pub fn actor_key(who: Option<&Who>) -> String {
    match who {
        Some(Who::Caller { actor, .. }) => actor.clone(),
        _ => "anonymous".to_owned(),
    }
}

/// The form token for a browser cookie and an actor.
pub fn token_for(secret: &[u8], cookie: &str, actor: &str) -> String {
    hex(&hmac_sha256(secret, &token_message(cookie, actor)))
}

/// The message a token signs: a version tag, the cookie and the actor, NUL-separated.
fn token_message(cookie: &str, actor: &str) -> Vec<u8> {
    let mut msg = Vec::with_capacity(cookie.len() + actor.len() + 14);
    msg.extend_from_slice(b"op-csrf-v1\0");
    msg.extend_from_slice(cookie.as_bytes());
    msg.push(0);
    msg.extend_from_slice(actor.as_bytes());
    msg
}

/// Whether a submitted `token` proves the write came from a form this server issued to this
/// browser for this actor. A missing cookie or missing token is invalid.
pub fn token_valid(secret: &[u8], cookie: Option<&str>, token: Option<&str>, actor: &str) -> bool {
    match (cookie, token) {
        (Some(c), Some(t)) if cookie_well_formed(c) => {
            // `verify_slice` compares in constant time (via `subtle`).
            let Some(submitted) = unhex(t) else {
                return false;
            };
            let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
            mac.update(&token_message(c, actor));
            mac.verify_slice(&submitted).is_ok()
        }
        _ => false,
    }
}

// ── Authorization ────────────────────────────────────────────────────

#[cfg(test)]
pub use op_services::work_packages::mutation::EDIT_WORK_PACKAGES;
/// The authorization rule is the work-package mutation boundary's, shared with the JSON API
/// (`op_services::work_packages::mutation`): the same [`Principal`], the same [`may`], the same
/// permission name. Pages and API cannot disagree about who may edit a record.
pub use op_services::work_packages::mutation::{may, Principal};

/// The boundary's [`Caller`] for a request's sign-in outcome. Only a verified person is an
/// `Actor`; a refused or absent sign-in is anonymous and never allowed, and `allow_anonymous` (the
/// `OP_ALLOW_ANONYMOUS` demo opt-in) applies only to a request with no sign-in at all.
pub fn caller_of(who: Option<&Who>, allow_anonymous: bool) -> Caller {
    match who {
        None => Caller::Anonymous {
            allowed: allow_anonymous,
        },
        Some(Who::Caller { actor, .. }) => Caller::Actor {
            login: actor.clone(),
        },
        Some(_) => Caller::Anonymous { allowed: false },
    }
}

/// A store failure, kept as text: the pages render it, they never branch on its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn store_err(e: impl std::fmt::Display) -> StoreError {
    StoreError(e.to_string())
}

/// The lookup tables the edit form renders as `<select>` options.
#[derive(Default)]
pub struct EditOptions {
    pub statuses: Vec<StatusRow>,
    pub types: Vec<TypeRow>,
    pub priorities: Vec<PriorityRow>,
    pub users: Vec<UserRow>,
}

/// The database seam of the work-package write path. The handler cores are generic over it so
/// the decision logic (404 vs 500, 403, 422, "no write happened") is tested with a fake.
pub trait WpStore {
    async fn find_wp(
        &self,
        id: i64,
    ) -> Result<Option<op_db::work_packages::WorkPackageRow>, StoreError>;
    /// Whether the project exists and is active (an archived or missing project allows nothing).
    async fn project_active(&self, project_id: i64) -> Result<bool, StoreError>;
    async fn principal(
        &self,
        who: Option<&Who>,
        project_id: i64,
        allow_anonymous: bool,
    ) -> Result<Principal, StoreError>;
    async fn edit_options(&self) -> EditOptions;
    /// The update as one unit of work (lookup, authorization, UPDATE and journal on one
    /// transaction): [`MutationStore::update_unit_of_work`] seen from the pages.
    async fn update_unit_of_work(
        &self,
        caller: &Caller,
        id: i64,
        dto: UpdateWorkPackageDto,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError>;
}

/// The Postgres-backed [`WpStore`].
pub struct PgWpStore {
    pub pool: PgPool,
}

/// Resolve the principal for a request against `project_id`: the boundary's loader
/// (`op_services::work_packages::mutation::load_principal`), the same one the API uses.
///
/// The signed-in actor is `oidc:<subject>`; it is matched against `users.login` (the provisioning
/// convention: a person's `login` is their actor string). Their permissions in the project are
/// the union of the permissions of every role of their membership there. The sign-in map's role
/// name only admits the person to the pages; it names no project, so it grants nothing here.
pub async fn load_principal(
    pool: &PgPool,
    who: Option<&Who>,
    project_id: i64,
    allow_anonymous: bool,
) -> Result<Principal, StoreError> {
    mutation::load_principal(pool, &caller_of(who, allow_anonymous), project_id)
        .await
        .map_err(|e| StoreError(e.to_string()))
}

/// Whether the project exists and is active.
pub async fn load_project_active(pool: &PgPool, project_id: i64) -> Result<bool, StoreError> {
    mutation::load_project_active(pool, project_id)
        .await
        .map_err(|e| StoreError(e.to_string()))
}

impl WpStore for PgWpStore {
    async fn find_wp(
        &self,
        id: i64,
    ) -> Result<Option<op_db::work_packages::WorkPackageRow>, StoreError> {
        op_db::WorkPackageRepository::new(self.pool.clone())
            .find_by_id(id)
            .await
            .map_err(store_err)
    }

    async fn project_active(&self, project_id: i64) -> Result<bool, StoreError> {
        load_project_active(&self.pool, project_id).await
    }

    async fn principal(
        &self,
        who: Option<&Who>,
        project_id: i64,
        allow_anonymous: bool,
    ) -> Result<Principal, StoreError> {
        load_principal(&self.pool, who, project_id, allow_anonymous).await
    }

    async fn edit_options(&self) -> EditOptions {
        EditOptions {
            statuses: StatusRepository::new(self.pool.clone())
                .find_all(200, 0)
                .await
                .unwrap_or_default(),
            types: TypeRepository::new(self.pool.clone())
                .find_all(200, 0)
                .await
                .unwrap_or_default(),
            priorities: PriorityRepository::new(self.pool.clone())
                .find_all(200, 0)
                .await
                .unwrap_or_default(),
            users: UserRepository::new(self.pool.clone())
                .find_all(500, 0)
                .await
                .unwrap_or_default(),
        }
    }

    async fn update_unit_of_work(
        &self,
        caller: &Caller,
        id: i64,
        dto: UpdateWorkPackageDto,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .update_unit_of_work(caller, id, dto)
        .await
    }
}

/// A [`WpStore`] seen through the mutation boundary's store trait. The pages keep their own
/// store seam (it also serves the edit-form lookups); this adapter lets the boundary's
/// authorization and write run over it, so the page and the API share one decision.
pub struct PageStore<'a, S: WpStore>(pub &'a S);

fn failure(e: StoreError) -> StoreFailure {
    StoreFailure::Other(e.0)
}

impl<S: WpStore> MutationStore for PageStore<'_, S> {
    async fn find_wp(
        &self,
        id: i64,
    ) -> Result<Option<op_db::work_packages::WorkPackageRow>, StoreFailure> {
        self.0.find_wp(id).await.map_err(failure)
    }

    async fn project_active(&self, project_id: i64) -> Result<bool, StoreFailure> {
        self.0.project_active(project_id).await.map_err(failure)
    }

    async fn principal(&self, caller: &Caller, project_id: i64) -> Result<Principal, StoreFailure> {
        let result = match caller {
            Caller::Anonymous { allowed } => self.0.principal(None, project_id, *allowed).await,
            Caller::Actor { login } => {
                let who = Who::Caller {
                    actor: login.clone(),
                    role: String::new(),
                };
                self.0.principal(Some(&who), project_id, false).await
            }
        };
        result.map_err(failure)
    }

    async fn update_unit_of_work(
        &self,
        caller: &Caller,
        id: i64,
        dto: UpdateWorkPackageDto,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
        self.0.update_unit_of_work(caller, id, dto).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-secret-0123456789abcdef0123";

    fn user(admin: bool, active: bool, perms: &[&str]) -> Principal {
        Principal::User {
            admin,
            active,
            permissions: perms.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    // RFC 4231 test cases 1 and 2: the hand-rolled HMAC is the real one.
    #[test]
    fn hmac_sha256_matches_rfc_4231() {
        assert_eq!(
            hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn a_token_validates_only_for_its_own_cookie_secret_and_actor() {
        let cookie = new_cookie_value();
        let t = token_for(SECRET, &cookie, "oidc:alice");
        assert!(token_valid(SECRET, Some(&cookie), Some(&t), "oidc:alice"));
        // wrong actor, wrong cookie, wrong secret, tampered token
        assert!(!token_valid(SECRET, Some(&cookie), Some(&t), "oidc:bob"));
        let other = new_cookie_value();
        assert!(!token_valid(SECRET, Some(&other), Some(&t), "oidc:alice"));
        assert!(!token_valid(
            b"another-secret-another-secret!!",
            Some(&cookie),
            Some(&t),
            "oidc:alice"
        ));
        let mut bad = t.clone();
        bad.replace_range(..1, if t.starts_with('0') { "1" } else { "0" });
        assert!(!token_valid(
            SECRET,
            Some(&cookie),
            Some(&bad),
            "oidc:alice"
        ));
    }

    #[test]
    fn a_missing_cookie_or_token_or_a_malformed_cookie_never_validates() {
        let cookie = new_cookie_value();
        let t = token_for(SECRET, &cookie, "anonymous");
        assert!(!token_valid(SECRET, None, Some(&t), "anonymous"));
        assert!(!token_valid(SECRET, Some(&cookie), None, "anonymous"));
        assert!(!token_valid(SECRET, None, None, "anonymous"));
        assert!(!token_valid(SECRET, Some(&cookie), Some(""), "anonymous"));
        // A short attacker-chosen cookie is refused even with a token computed over it.
        let t2 = token_for(SECRET, "abc", "anonymous");
        assert!(!token_valid(SECRET, Some("abc"), Some(&t2), "anonymous"));
    }

    #[test]
    fn the_cookie_is_read_from_the_cookie_header_only_when_well_formed() {
        let good = new_cookie_value();
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("a=b; {CSRF_COOKIE}={good}; c=d").parse().unwrap(),
        );
        assert_eq!(cookie_from_headers(&h).as_deref(), Some(good.as_str()));
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("{CSRF_COOKIE}=short").parse().unwrap(),
        );
        assert_eq!(cookie_from_headers(&h), None);
        assert_eq!(cookie_from_headers(&HeaderMap::new()), None);
        assert!(set_cookie_value(&good, true).ends_with("; Secure"));
        assert!(!set_cookie_value(&good, false).contains("Secure"));
        assert!(set_cookie_value(&good, false).contains("HttpOnly; SameSite=Strict"));
    }

    #[test]
    fn authorization_rule() {
        let edit = [EDIT_WORK_PACKAGES];
        // member with the permission in this project
        assert!(may(&user(false, true, &edit), true, EDIT_WORK_PACKAGES));
        // member without it (e.g. a viewer role)
        assert!(!may(
            &user(false, true, &["view_work_packages"]),
            true,
            EDIT_WORK_PACKAGES
        ));
        // not a member of this project: empty permission set
        assert!(!may(&user(false, true, &[]), true, EDIT_WORK_PACKAGES));
        // admin passes without a membership
        assert!(may(&user(true, true, &[]), true, EDIT_WORK_PACKAGES));
        // locked/inactive user, even an admin or a member
        assert!(!may(&user(true, false, &[]), true, EDIT_WORK_PACKAGES));
        assert!(!may(&user(false, false, &edit), true, EDIT_WORK_PACKAGES));
        // archived or missing project: nothing, not even admin
        assert!(!may(&user(true, true, &edit), false, EDIT_WORK_PACKAGES));
        // signed in without a users row
        assert!(!may(&Principal::Unprovisioned, true, EDIT_WORK_PACKAGES));
        // anonymous: only with the explicit opt-in
        assert!(!may(
            &Principal::Anonymous { allowed: false },
            true,
            EDIT_WORK_PACKAGES
        ));
        assert!(may(
            &Principal::Anonymous { allowed: true },
            true,
            EDIT_WORK_PACKAGES
        ));
        // a different permission is not the edit permission
        assert!(!may(
            &user(false, true, &[EDIT_PROJECT]),
            true,
            EDIT_WORK_PACKAGES
        ));
    }

    #[test]
    fn the_token_actor_is_the_signed_in_actor_else_anonymous() {
        let who = Who::Caller {
            actor: "oidc:x".into(),
            role: "Member".into(),
        };
        assert_eq!(actor_key(Some(&who)), "oidc:x");
        assert_eq!(actor_key(None), "anonymous");
        assert_eq!(
            actor_key(Some(&Who::Refused {
                actor: "oidc:y".into()
            })),
            "anonymous"
        );
    }
}
