// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Start-up configuration of the sign-in. Sign-in is optional here: with no
//! `OP_SIGNIN_OIDC_ISSUER` the server behaves as it did before and nothing in this module
//! runs. Once the issuer is set, anything incomplete, ambiguous, unreadable or refused stops
//! the process with a named class instead of serving half a sign-in.
//!
//! | variable | meaning |
//! |---|---|
//! | `OP_SIGNIN_OIDC_ISSUER` | https issuer URL of Zitadel; unset turns sign-in off |
//! | `OP_SIGNIN_OIDC_CLIENT_ID` | the client id registered at the provider (required once the issuer is set) |
//! | `OP_SIGNIN_OIDC_REDIRECT_URL` | `https://<public host>/auth/callback` as registered (required once the issuer is set) |
//! | `OP_SIGNIN_SESSION_KEY` | 32 bytes as base64 or 64 hex digits; without it sessions end at a restart |
//! | `OP_SIGNIN_OIDC_CLIENT_SECRET` | only for a confidential client |
//! | `OP_SIGNIN_OIDC_ROLES_CLAIM`, `OP_SIGNIN_OIDC_ORG_CLAIM`, `OP_SIGNIN_OIDC_EXTRA_SCOPES` | override the Zitadel defaults |
//! | `OP_SIGNIN_MAP` or `OP_SIGNIN_MAP_TEXT` | the sign-in map (path, or the text itself); exactly one is required once the issuer is set |
//! | `OP_API_CLIENTS` | comma-separated Zitadel client ids whose bearer tokens `/api/v3` accepts; unset or blank turns the token path off |
//! | `OP_API_CALLERS` | comma-separated `subject=login` pairs: the OpenProject login a verified token acts as; required with `OP_API_CLIENTS` |
//!
//! `OP_API_CLIENTS` needs the sign-in (the tokens are checked by the same provider setup) and
//! `OP_API_CALLERS`; an unmapped subject is refused at request time.
//!
//! The sign-in map is described in [`crate::signin_map`].

use std::collections::BTreeMap;
use std::sync::Arc;

use web_login::{Login, OidcHttp};

use crate::api_token::LoginTokenVerifier;

use crate::signin_gate::Gate;
use crate::signin_map::SigninMap;

/// Prefix of the sign-in variables `web_login::Config::from_vars` reads. The cookie stem it
/// derives is `op_signin`.
pub const SIGNIN_PREFIX: &str = "OP_SIGNIN_";
/// Path of the sign-in map.
pub const SIGNIN_MAP_VAR: &str = "OP_SIGNIN_MAP";
/// The sign-in map text itself.
pub const SIGNIN_MAP_TEXT_VAR: &str = "OP_SIGNIN_MAP_TEXT";
/// Client ids whose bearer tokens the JSON API accepts.
pub const API_CLIENTS_VAR: &str = "OP_API_CLIENTS";
/// `subject=login` pairs: who a verified token acts as.
pub const API_CALLERS_VAR: &str = "OP_API_CALLERS";

/// Why the sign-in could not be started. A class only: no path, no file content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    /// A sign-in variable is wrong; the text names the variable, never its value.
    SignInInvalid(String),
    /// Neither `OP_SIGNIN_MAP` nor `OP_SIGNIN_MAP_TEXT` is set.
    MapIncomplete,
    MapAmbiguous,
    MapUnreadable,
    /// A row of the sign-in map cannot be read, or the map does not hold together.
    MapRefused(String),
    /// `OP_API_CLIENTS` has an empty item, or an item with whitespace or control characters.
    ApiClientsInvalid,
    /// `OP_API_CALLERS` has an empty item, an item without `=`, whitespace, or a repeated subject.
    ApiCallersInvalid,
    /// `OP_API_CLIENTS` is set but `OP_SIGNIN_OIDC_ISSUER` is not.
    ApiClientsNeedSignIn,
    /// `OP_API_CLIENTS` is set but `OP_API_CALLERS` is not.
    ApiClientsNeedCallers,
}

impl StartError {
    pub fn class(&self) -> String {
        match self {
            StartError::SignInInvalid(why) => format!("sign-in configuration refused: {why}"),
            StartError::MapIncomplete => {
                format!("{SIGNIN_MAP_VAR} or {SIGNIN_MAP_TEXT_VAR} is missing")
            }
            StartError::MapAmbiguous => "the sign-in map is given both as a file and inline".into(),
            StartError::MapUnreadable => "sign-in map file unreadable".into(),
            StartError::MapRefused(why) => format!("sign-in map refused: {why}"),
            StartError::ApiClientsInvalid => {
                format!("{API_CLIENTS_VAR} has an empty item or an item with whitespace")
            }
            StartError::ApiCallersInvalid => format!(
                "{API_CALLERS_VAR} must be comma-separated subject=login pairs, \
                 with no empty item, no blank, and no subject twice"
            ),
            StartError::ApiClientsNeedSignIn => {
                format!("{API_CLIENTS_VAR} is set but the sign-in is not configured")
            }
            StartError::ApiClientsNeedCallers => {
                format!("{API_CLIENTS_VAR} is set but {API_CALLERS_VAR} is not")
            }
        }
    }
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.class())
    }
}

impl std::error::Error for StartError {}

/// The sign-in configuration, read and checked but not yet started.
#[derive(Debug)]
pub struct Settings {
    pub login: web_login::Config,
    pub signin_map: SigninMap,
    /// Client ids whose tokens the JSON API accepts; empty: the token path is off.
    pub api_clients: Vec<String>,
    /// Token subject to the OpenProject login it acts as.
    pub api_callers: BTreeMap<String, String>,
}

/// The client ids of `OP_API_CLIENTS`: none when it is unset or blank.
fn api_clients(value: Option<String>) -> Result<Vec<String>, StartError> {
    let Some(value) = value.filter(|v| !v.trim().is_empty()) else {
        return Ok(Vec::new());
    };
    let mut ids: Vec<String> = Vec::new();
    for item in value.split(',') {
        let id = item.trim();
        if id.is_empty() || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(StartError::ApiClientsInvalid);
        }
        if !ids.iter().any(|known| known == id) {
            ids.push(id.to_owned());
        }
    }
    Ok(ids)
}

/// The `subject=login` pairs of `OP_API_CALLERS`: none when it is unset or blank.
fn api_callers(value: Option<String>) -> Result<BTreeMap<String, String>, StartError> {
    let Some(value) = value.filter(|v| !v.trim().is_empty()) else {
        return Ok(BTreeMap::new());
    };
    let plain = |s: &str| {
        !s.is_empty() && !s.contains('=') && !s.chars().any(|c| c.is_whitespace() || c.is_control())
    };
    let mut callers = BTreeMap::new();
    for item in value.split(',') {
        let (subject, login) = item
            .trim()
            .split_once('=')
            .ok_or(StartError::ApiCallersInvalid)?;
        if !plain(subject) || !plain(login) {
            return Err(StartError::ApiCallersInvalid);
        }
        if callers
            .insert(subject.to_owned(), login.to_owned())
            .is_some()
        {
            return Err(StartError::ApiCallersInvalid);
        }
    }
    Ok(callers)
}

impl Settings {
    /// `Ok(None)` when `OP_SIGNIN_OIDC_ISSUER` is unset: sign-in is off.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Option<Settings>, StartError> {
        let set = |k: &str| get(k).filter(|v| !v.trim().is_empty());

        let api_clients = api_clients(get(API_CLIENTS_VAR))?;
        let api_callers = api_callers(get(API_CALLERS_VAR))?;

        let Some(login) = web_login::Config::from_vars(SIGNIN_PREFIX, &get)
            .map_err(|e| StartError::SignInInvalid(e.to_string()))?
        else {
            if !api_clients.is_empty() {
                return Err(StartError::ApiClientsNeedSignIn);
            }
            return Ok(None);
        };
        let map = match (set(SIGNIN_MAP_VAR), set(SIGNIN_MAP_TEXT_VAR)) {
            (Some(_), Some(_)) => return Err(StartError::MapAmbiguous),
            (Some(path), None) => {
                std::fs::read_to_string(path.trim()).map_err(|_| StartError::MapUnreadable)?
            }
            (None, Some(text)) => text,
            (None, None) => return Err(StartError::MapIncomplete),
        };
        let signin_map =
            SigninMap::parse(&map).map_err(|e| StartError::MapRefused(e.to_string()))?;
        signin_map
            .check()
            .map_err(|e| StartError::MapRefused(e.to_string()))?;
        if !api_clients.is_empty() && api_callers.is_empty() {
            return Err(StartError::ApiClientsNeedCallers);
        }
        Ok(Some(Settings {
            login,
            signin_map,
            api_clients,
            api_callers,
        }))
    }

    /// Builds the gate. `http` is how the sign-in reaches the identity provider.
    pub fn into_gate(self, http: Box<dyn OidcHttp>) -> Result<Gate, StartError> {
        let mut login =
            Login::new(self.login, http).map_err(|e| StartError::SignInInvalid(e.to_string()))?;
        login.accept_bearer_clients(&self.api_clients);
        let login = Arc::new(login);
        let gate = Gate::new(login.clone(), Arc::new(self.signin_map));
        if self.api_clients.is_empty() {
            return Ok(gate);
        }
        Ok(gate.with_api_tokens(Arc::new(LoginTokenVerifier::new(login, self.api_callers))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(extra: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k: &str| {
            let base = [
                ("OP_SIGNIN_OIDC_ISSUER", "https://idp.example.test"),
                ("OP_SIGNIN_OIDC_CLIENT_ID", "client-1"),
                (
                    "OP_SIGNIN_OIDC_REDIRECT_URL",
                    "https://op.example.test/auth/callback",
                ),
            ];
            base.iter()
                .chain(extra.iter())
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn no_issuer_means_sign_in_is_off() {
        assert!(Settings::from_env(|_| None).unwrap().is_none());
        // Map variables alone do not switch anything on.
        let only_map = |k: &str| (k == SIGNIN_MAP_TEXT_VAR).then(|| "rank=x".to_owned());
        assert!(Settings::from_env(only_map).unwrap().is_none());
    }

    #[test]
    fn an_issuer_without_a_map_refuses_to_start() {
        assert_eq!(
            Settings::from_env(vars(&[])).unwrap_err(),
            StartError::MapIncomplete
        );
        let both = vars(&[("OP_SIGNIN_MAP", "/x"), ("OP_SIGNIN_MAP_TEXT", "rank=x")]);
        assert_eq!(
            Settings::from_env(both).unwrap_err(),
            StartError::MapAmbiguous
        );
        let gone = vars(&[("OP_SIGNIN_MAP", "/no/such/signin.conf")]);
        assert_eq!(
            Settings::from_env(gone).unwrap_err(),
            StartError::MapUnreadable
        );
        let bad = vars(&[("OP_SIGNIN_MAP_TEXT", "role=a:x\n")]);
        assert!(matches!(
            Settings::from_env(bad).unwrap_err(),
            StartError::MapRefused(_)
        ));
    }

    #[test]
    fn a_complete_configuration_reads_and_uses_the_op_signin_cookie() {
        let s = Settings::from_env(vars(&[(
            "OP_SIGNIN_MAP_TEXT",
            "role=op-admin:Project admin\nrank=Project admin\n",
        )]))
        .unwrap()
        .expect("sign-in on");
        assert_eq!(s.login.session_cookie(), "__Host-op_signin_sid");
        assert_eq!(s.signin_map.op_role(["op-admin"]), Some("Project admin"));
    }

    /// Like `vars`, with owned values so a test can build them at run time.
    fn owned(extra: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let extra: Vec<(String, String)> = extra
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k: &str| {
            let base = [
                ("OP_SIGNIN_OIDC_ISSUER", "https://idp.example.test"),
                ("OP_SIGNIN_OIDC_CLIENT_ID", "client-1"),
                (
                    "OP_SIGNIN_OIDC_REDIRECT_URL",
                    "https://op.example.test/auth/callback",
                ),
            ];
            extra
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
                .or_else(|| {
                    base.iter()
                        .find(|(n, _)| *n == k)
                        .map(|(_, v)| (*v).to_owned())
                })
        }
    }

    const MAP: (&str, &str) = (
        "OP_SIGNIN_MAP_TEXT",
        "role=op-admin:Project admin\nrank=Project admin\n",
    );

    /// FAILS IF: a blank or unset `OP_API_CLIENTS` turns anything on, or a good list is
    /// misread (duplicates must collapse, spaces around items are trimmed).
    #[test]
    fn api_clients_read_good_values_and_blank_is_off() {
        let off = Settings::from_env(owned(&[MAP])).unwrap().unwrap();
        assert!(off.api_clients.is_empty() && off.api_callers.is_empty());
        let blank = Settings::from_env(owned(&[MAP, ("OP_API_CLIENTS", "  ")]))
            .unwrap()
            .unwrap();
        assert!(blank.api_clients.is_empty());

        let on = Settings::from_env(owned(&[
            MAP,
            ("OP_API_CLIENTS", " a1 , b2,a1"),
            ("OP_API_CALLERS", "123=report-rs, 456=other"),
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(on.api_clients, ["a1", "b2"]);
        assert_eq!(on.api_callers.len(), 2);
        assert_eq!(on.api_callers["123"], "report-rs");
        assert_eq!(on.api_callers["456"], "other");
    }

    /// FAILS IF: a malformed `OP_API_CLIENTS` or `OP_API_CALLERS` is read leniently.
    #[test]
    fn each_bad_api_value_refuses_to_start() {
        let clients = |v: &str| {
            Settings::from_env(owned(&[
                MAP,
                ("OP_API_CLIENTS", v),
                ("OP_API_CALLERS", "1=x"),
            ]))
            .unwrap_err()
        };
        for bad in ["a,,b", "a,", ",a", "a b", "a\tb"] {
            assert_eq!(clients(bad), StartError::ApiClientsInvalid, "{bad:?}");
        }
        assert!(clients("a,,b").to_string().contains("OP_API_CLIENTS"));

        let callers = |v: &str| {
            Settings::from_env(owned(&[
                MAP,
                ("OP_API_CLIENTS", "a"),
                ("OP_API_CALLERS", v),
            ]))
            .unwrap_err()
        };
        for bad in [
            "1=x,,2=y", "1=x,", "1", "=x", "1=", "1=x,1=y", "1 =x", "1=x y", "1=x=y",
        ] {
            assert_eq!(callers(bad), StartError::ApiCallersInvalid, "{bad:?}");
        }
        assert!(callers("1").to_string().contains("OP_API_CALLERS"));
    }

    /// FAILS IF: the token path starts without sign-in or without a caller map.
    #[test]
    fn api_clients_need_the_sign_in_and_the_callers() {
        let no_signin = |k: &str| match k {
            "OP_API_CLIENTS" => Some("a".to_owned()),
            "OP_API_CALLERS" => Some("1=x".to_owned()),
            _ => None,
        };
        let e = Settings::from_env(no_signin).unwrap_err();
        assert_eq!(e, StartError::ApiClientsNeedSignIn);
        assert!(e.to_string().contains("OP_API_CLIENTS"));

        let e = Settings::from_env(owned(&[MAP, ("OP_API_CLIENTS", "a")])).unwrap_err();
        assert_eq!(e, StartError::ApiClientsNeedCallers);
        assert!(e.to_string().contains("OP_API_CALLERS"));

        // Callers alone are harmless: the token path stays off.
        let s = Settings::from_env(owned(&[MAP, ("OP_API_CALLERS", "1=x")]))
            .unwrap()
            .unwrap();
        assert!(s.api_clients.is_empty());
        // And with sign-in off, a bad callers value is still caught.
        let bad = |k: &str| (k == "OP_API_CALLERS").then(|| "nonsense".to_owned());
        assert_eq!(
            Settings::from_env(bad).unwrap_err(),
            StartError::ApiCallersInvalid
        );
    }

    #[test]
    fn a_half_configured_provider_is_an_error_not_a_silent_off() {
        let only_issuer =
            |k: &str| (k == "OP_SIGNIN_OIDC_ISSUER").then(|| "https://idp.example.test".to_owned());
        let e = Settings::from_env(only_issuer).unwrap_err();
        assert!(e.to_string().contains("OP_SIGNIN_OIDC_CLIENT_ID"), "{e}");
    }
}
