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
//!
//! The sign-in map is described in [`crate::signin_map`].

use std::sync::Arc;

use web_login::{Login, OidcHttp};

use crate::signin_gate::Gate;
use crate::signin_map::SigninMap;

/// Prefix of the sign-in variables `web_login::Config::from_vars` reads. The cookie stem it
/// derives is `op_signin`.
pub const SIGNIN_PREFIX: &str = "OP_SIGNIN_";
/// Path of the sign-in map.
pub const SIGNIN_MAP_VAR: &str = "OP_SIGNIN_MAP";
/// The sign-in map text itself.
pub const SIGNIN_MAP_TEXT_VAR: &str = "OP_SIGNIN_MAP_TEXT";

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
}

impl Settings {
    /// `Ok(None)` when `OP_SIGNIN_OIDC_ISSUER` is unset: sign-in is off.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Option<Settings>, StartError> {
        let set = |k: &str| get(k).filter(|v| !v.trim().is_empty());

        let Some(login) = web_login::Config::from_vars(SIGNIN_PREFIX, &get)
            .map_err(|e| StartError::SignInInvalid(e.to_string()))?
        else {
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
        Ok(Some(Settings { login, signin_map }))
    }

    /// Builds the gate. `http` is how the sign-in reaches the identity provider.
    pub fn into_gate(self, http: Box<dyn OidcHttp>) -> Result<Gate, StartError> {
        let login =
            Login::new(self.login, http).map_err(|e| StartError::SignInInvalid(e.to_string()))?;
        Ok(Gate::new(Arc::new(login), Arc::new(self.signin_map)))
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

    #[test]
    fn a_half_configured_provider_is_an_error_not_a_silent_off() {
        let only_issuer =
            |k: &str| (k == "OP_SIGNIN_OIDC_ISSUER").then(|| "https://idp.example.test".to_owned());
        let e = Settings::from_env(only_issuer).unwrap_err();
        assert!(e.to_string().contains("OP_SIGNIN_OIDC_CLIENT_ID"), "{e}");
    }
}
