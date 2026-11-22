// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Sign-in configuration, passed in by the consumer. Secrets never show in `Debug`.

use crate::util::safe_return_to;
use base64::Engine as _;
use ogar_auth::identity::Issuer;
use std::fmt;
use std::time::Duration;

/// How long a started login may take.
pub const LOGIN_TTL: Duration = Duration::from_secs(600);
/// Absolute session lifetime.
pub const SESSION_TTL: Duration = Duration::from_secs(8 * 3600);
/// The claim ZITADEL puts the project roles of the requested project in.
pub const DEFAULT_ROLES_CLAIM: &str = "urn:zitadel:iam:org:project:roles";
/// The claim ZITADEL puts the user's organization id in (scope
/// `urn:zitadel:iam:user:resourceowner`).
pub const DEFAULT_ORG_CLAIM: &str = "urn:zitadel:iam:user:resourceowner:id";
/// Scopes requested besides `openid`: the user's organization and the project roles.
pub const DEFAULT_EXTRA_SCOPES: &str =
    "urn:zitadel:iam:user:resourceowner urn:zitadel:iam:org:projects:roles";

/// A bad configuration value. The text names the setting, never its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);

impl ConfigError {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

/// The session sealing key. Its `Debug` never shows the bytes.
#[derive(Clone)]
pub(crate) struct SessionKey(pub(crate) [u8; 32]);

impl fmt::Debug for SessionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionKey(redacted)")
    }
}

/// A confidential client's secret. Its `Debug` never shows the text.
#[derive(Clone)]
pub(crate) struct ClientSecret(pub(crate) String);

impl fmt::Debug for ClientSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientSecret(redacted)")
    }
}

/// Everything the sign-in needs to know. Build it with [`Config::new`] and the `with_*`
/// setters, or read it from the environment with [`Config::from_vars`].
#[derive(Debug, Clone)]
pub struct Config {
    issuer: Issuer,
    client_id: String,
    redirect_url: String,
    client_secret: Option<ClientSecret>,
    session_key: Option<SessionKey>,
    roles_claim: String,
    org_claim: String,
    extra_scopes: String,
    cookie_stem: String,
    default_return_to: String,
}

fn valid_stem(stem: &str) -> bool {
    !stem.is_empty()
        && stem.len() <= 32
        && stem
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// A 32-byte session key as base64 (URL-safe or standard, with or without padding) or
/// 64 hex digits. Anything else is an error, never a shorter key.
fn parse_session_key(k: &str) -> Option<[u8; 32]> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    let bytes = if k.len() == 64 && k.bytes().all(|b| b.is_ascii_hexdigit()) {
        (0..32)
            .map(|i| u8::from_str_radix(&k[2 * i..2 * i + 2], 16))
            .collect::<Result<Vec<u8>, _>>()
            .ok()
    } else {
        [&URL_SAFE_NO_PAD, &URL_SAFE, &STANDARD, &STANDARD_NO_PAD]
            .iter()
            .find_map(|e| e.decode(k).ok())
    };
    bytes.and_then(|b| <[u8; 32]>::try_from(b).ok())
}

impl Config {
    /// `redirect_url` is the absolute https URL of `/auth/callback` as registered at the
    /// provider. `cookie_stem` (lowercase letters, digits, `_`, `-`) names the cookies:
    /// `__Host-<stem>_sid` and `__Host-<stem>_login`.
    pub fn new(
        issuer: &str,
        client_id: &str,
        redirect_url: &str,
        cookie_stem: &str,
    ) -> Result<Self, ConfigError> {
        let issuer = Issuer::new(issuer.trim())
            .map_err(|_| ConfigError::new("the issuer is not an https issuer"))?;
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(ConfigError::new("the client id is empty"));
        }
        let redirect_url = redirect_url.trim();
        if !redirect_url.starts_with("https://") || !redirect_url.ends_with("/auth/callback") {
            return Err(ConfigError::new(
                "the redirect URL must be an https URL ending in /auth/callback",
            ));
        }
        if !valid_stem(cookie_stem) {
            return Err(ConfigError::new(
                "the cookie stem must be 1 to 32 characters of a-z, 0-9, _ or -",
            ));
        }
        Ok(Self {
            issuer,
            client_id: client_id.to_string(),
            redirect_url: redirect_url.to_string(),
            client_secret: None,
            session_key: None,
            roles_claim: DEFAULT_ROLES_CLAIM.to_string(),
            org_claim: DEFAULT_ORG_CLAIM.to_string(),
            extra_scopes: DEFAULT_EXTRA_SCOPES.to_string(),
            cookie_stem: cookie_stem.to_string(),
            default_return_to: "/".to_string(),
        })
    }

    /// Only for a confidential client; sent as `client_secret` at the token endpoint.
    #[must_use]
    pub fn with_client_secret(mut self, secret: &str) -> Self {
        self.client_secret = Some(ClientSecret(secret.to_string()));
        self
    }

    /// 32 bytes as base64 or 64 hex digits. Without a key every process seals under its
    /// own random one, so sessions end at a restart.
    pub fn with_session_key(mut self, key: &str) -> Result<Self, ConfigError> {
        let bytes = parse_session_key(key).ok_or_else(|| {
            ConfigError::new("the session key must be 32 bytes (base64 or 64 hex digits)")
        })?;
        self.session_key = Some(SessionKey(bytes));
        Ok(self)
    }

    #[must_use]
    pub fn with_roles_claim(mut self, claim: &str) -> Self {
        self.roles_claim = claim.to_string();
        self
    }

    #[must_use]
    pub fn with_org_claim(mut self, claim: &str) -> Self {
        self.org_claim = claim.to_string();
        self
    }

    /// Scopes requested besides `openid`, space separated.
    #[must_use]
    pub fn with_extra_scopes(mut self, scopes: &str) -> Self {
        self.extra_scopes = scopes.to_string();
        self
    }

    /// Where the browser goes after sign-in when no safe `return_to` was given.
    pub fn with_default_return_to(mut self, path: &str) -> Result<Self, ConfigError> {
        if !safe_return_to(path) {
            return Err(ConfigError::new(
                "the default return path is not a local path",
            ));
        }
        self.default_return_to = path.to_string();
        Ok(self)
    }

    /// Read the configuration from `<prefix>OIDC_ISSUER`, `<prefix>OIDC_CLIENT_ID`,
    /// `<prefix>OIDC_REDIRECT_URL`, `<prefix>SESSION_KEY` and the optional
    /// `<prefix>OIDC_CLIENT_SECRET`, `<prefix>OIDC_ROLES_CLAIM`, `<prefix>OIDC_ORG_CLAIM`
    /// and `<prefix>OIDC_EXTRA_SCOPES`. The cookie stem is the prefix without trailing `_`,
    /// lowercased (`REPORT_` gives `report`).
    ///
    /// `Ok(None)` when the issuer variable is unset. Otherwise the client id and the
    /// redirect URL are required; a bad value is an error, never a silent fallback.
    pub fn from_vars(
        prefix: &str,
        get: impl Fn(&str) -> Option<String>,
    ) -> Result<Option<Self>, ConfigError> {
        let present = |name: &str| {
            get(&format!("{prefix}{name}"))
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let Some(issuer) = present("OIDC_ISSUER") else {
            return Ok(None);
        };
        let client_id = present("OIDC_CLIENT_ID")
            .ok_or_else(|| ConfigError::new(format!("{prefix}OIDC_CLIENT_ID is required")))?;
        let redirect_url = present("OIDC_REDIRECT_URL")
            .ok_or_else(|| ConfigError::new(format!("{prefix}OIDC_REDIRECT_URL is required")))?;
        let stem = prefix.trim_end_matches('_').to_ascii_lowercase();
        let mut cfg = Self::new(&issuer, &client_id, &redirect_url, &stem)
            .map_err(|e| ConfigError::new(format!("{prefix}OIDC_*: {e}")))?;
        if let Some(k) = present("SESSION_KEY") {
            cfg = cfg.with_session_key(&k).map_err(|_| {
                ConfigError::new(format!(
                    "{prefix}SESSION_KEY must be 32 bytes (base64 or 64 hex digits)"
                ))
            })?;
        }
        if let Some(s) = present("OIDC_CLIENT_SECRET") {
            cfg = cfg.with_client_secret(&s);
        }
        if let Some(c) = present("OIDC_ROLES_CLAIM") {
            cfg.roles_claim = c;
        }
        if let Some(c) = present("OIDC_ORG_CLAIM") {
            cfg.org_claim = c;
        }
        if let Some(s) = present("OIDC_EXTRA_SCOPES") {
            cfg.extra_scopes = s;
        }
        Ok(Some(cfg))
    }

    /// `true` when sessions are sealed under a configured key (they survive a restart).
    pub fn has_session_key(&self) -> bool {
        self.session_key.is_some()
    }

    pub fn issuer(&self) -> &Issuer {
        &self.issuer
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn redirect_url(&self) -> &str {
        &self.redirect_url
    }

    pub fn roles_claim(&self) -> &str {
        &self.roles_claim
    }

    pub fn org_claim(&self) -> &str {
        &self.org_claim
    }

    pub fn extra_scopes(&self) -> &str {
        &self.extra_scopes
    }

    pub fn default_return_to(&self) -> &str {
        &self.default_return_to
    }

    /// The session cookie (the `__Host-` prefix pins it to this host, `Secure` and `Path=/`).
    pub fn session_cookie(&self) -> String {
        format!("__Host-{}_sid", self.cookie_stem)
    }

    /// The login-in-progress cookie holding `state`.
    pub fn login_cookie(&self) -> String {
        format!("__Host-{}_login", self.cookie_stem)
    }

    pub(crate) fn client_secret(&self) -> Option<&str> {
        self.client_secret.as_ref().map(|s| s.0.as_str())
    }

    pub(crate) fn session_key(&self) -> Option<&SessionKey> {
        self.session_key.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0101010101010101010101010101010101010101010101010101010101010101";

    fn base() -> Config {
        Config::new(
            "https://idp.example.test",
            "client-1",
            "https://app.example.test/auth/callback",
            "demo",
        )
        .unwrap()
    }

    #[test]
    fn the_session_key_must_be_32_bytes() {
        use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
        assert!(base().with_session_key(KEY).unwrap().has_session_key());
        let url = URL_SAFE_NO_PAD.encode([7u8; 32]);
        assert!(base().with_session_key(&url).unwrap().has_session_key());
        let std = STANDARD.encode([7u8; 32]);
        assert!(base().with_session_key(&std).unwrap().has_session_key());
        for bad in ["short", "0101", &KEY[..62], ""] {
            assert!(base().with_session_key(bad).is_err(), "{bad:?}");
        }
        assert!(!base().has_session_key());
    }

    #[test]
    fn new_refuses_what_login_rs_refuses() {
        let ok = (
            "https://idp.example.test",
            "c",
            "https://a.example.test/auth/callback",
            "x",
        );
        assert!(Config::new(ok.0, ok.1, ok.2, ok.3).is_ok());
        assert!(Config::new("http://idp.example.test", ok.1, ok.2, ok.3).is_err());
        assert!(Config::new(ok.0, " ", ok.2, ok.3).is_err());
        assert!(Config::new(ok.0, ok.1, "http://a.example.test/auth/callback", ok.3).is_err());
        assert!(Config::new(ok.0, ok.1, "https://a.example.test/other", ok.3).is_err());
        for stem in ["", "A", "a b", "a;b", "a=b", &"x".repeat(33)] {
            assert!(Config::new(ok.0, ok.1, ok.2, stem).is_err(), "{stem:?}");
        }
    }

    #[test]
    fn from_vars_reads_the_prefixed_names_and_derives_the_stem() {
        let vars = |k: &str| match k {
            "TICKET_OIDC_ISSUER" => Some("https://idp.example.test".to_string()),
            "TICKET_OIDC_CLIENT_ID" => Some("client-1".to_string()),
            "TICKET_OIDC_REDIRECT_URL" => Some("https://t.example.test/auth/callback".to_string()),
            "TICKET_SESSION_KEY" => Some(KEY.to_string()),
            "TICKET_OIDC_ROLES_CLAIM" => Some("my:roles".to_string()),
            "TICKET_OIDC_ORG_CLAIM" => Some("my:org".to_string()),
            "TICKET_OIDC_EXTRA_SCOPES" => Some("a b".to_string()),
            _ => None,
        };
        let c = Config::from_vars("TICKET_", vars).unwrap().unwrap();
        assert_eq!(c.session_cookie(), "__Host-ticket_sid");
        assert_eq!(c.login_cookie(), "__Host-ticket_login");
        assert!(c.has_session_key());
        assert_eq!(c.roles_claim(), "my:roles");
        assert_eq!(c.org_claim(), "my:org");
        assert_eq!(c.extra_scopes(), "a b");
        assert!(Config::from_vars("OTHER_", vars).unwrap().is_none());
    }

    #[test]
    fn from_vars_requires_the_rest_once_the_issuer_is_set() {
        let only_issuer =
            |k: &str| (k == "X_OIDC_ISSUER").then(|| "https://idp.example.test".to_string());
        let e = Config::from_vars("X_", only_issuer).unwrap_err();
        assert!(e.to_string().contains("X_OIDC_CLIENT_ID"), "{e}");
        let bad_key = |k: &str| match k {
            "X_OIDC_ISSUER" => Some("https://idp.example.test".to_string()),
            "X_OIDC_CLIENT_ID" => Some("c".to_string()),
            "X_OIDC_REDIRECT_URL" => Some("https://a.example.test/auth/callback".to_string()),
            "X_SESSION_KEY" => Some("not-a-key-0123".to_string()),
            _ => None,
        };
        let e = Config::from_vars("X_", bad_key).unwrap_err();
        assert!(!e.to_string().contains("not-a-key"), "{e}");
    }

    #[test]
    fn debug_shows_no_key_material_or_secret() {
        let c = base()
            .with_session_key(KEY)
            .unwrap()
            .with_client_secret("s3cret-value");
        let shown = format!("{c:?}");
        assert!(shown.contains("SessionKey(redacted)"), "{shown}");
        assert!(shown.contains("ClientSecret(redacted)"), "{shown}");
        assert!(!shown.contains("s3cret"), "{shown}");
        assert!(
            !shown.contains("1, 1, 1") && !shown.contains("0101"),
            "{shown}"
        );
    }

    #[test]
    fn default_return_to_must_be_local() {
        assert!(base().with_default_return_to("/view").is_ok());
        for bad in ["//evil.example", "https://evil.example", "x", "/a\\b"] {
            assert!(base().with_default_return_to(bad).is_err(), "{bad:?}");
        }
    }
}
