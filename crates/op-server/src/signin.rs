// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Who sent a request. The pages ask a [`SignIn`]; [`LoginSignIn`] answers from the
//! `web_login` session and the sign-in map.
//!
//! The actor of a signed-in person is `oidc:<subject>`. Their role is the OpenProject role
//! name the sign-in map gives their provider role keys.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::http::HeaderMap;
use web_login::{signed_in_from_headers, Login, SignedIn};

use crate::signin_map::SigninMap;

/// Where an anonymous request is sent; the gate appends `?return_to=<the page asked for>`.
pub const SIGN_IN_PATH: &str = "/auth/login";

/// Who sent a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Who {
    Caller {
        actor: String,
        /// The OpenProject role name the sign-in map gave this person.
        role: String,
    },
    /// Not signed in; the address to send the browser to, when there is one.
    Anonymous { sign_in_url: Option<String> },
    /// Verified, but no role key maps to an OpenProject role.
    Refused { actor: String },
}

pub type WhoFuture<'a> = Pin<Box<dyn Future<Output = Who> + Send + 'a>>;

pub trait SignIn: Send + Sync {
    fn resolve<'a>(&'a self, headers: &'a HeaderMap) -> WhoFuture<'a>;
}

/// The sign-in of this service: the session of [`Login`], read through the sign-in map.
pub struct LoginSignIn {
    login: Arc<Login>,
    map: Arc<SigninMap>,
}

impl LoginSignIn {
    pub fn new(login: Arc<Login>, map: Arc<SigninMap>) -> LoginSignIn {
        LoginSignIn { login, map }
    }
}

/// The OpenProject role a verified person holds, if any.
pub fn op_role_of<'a>(map: &'a SigninMap, who: &SignedIn) -> Option<&'a str> {
    map.op_role(who.roles.iter().map(|(_, key)| key.as_str()))
}

impl SignIn for LoginSignIn {
    fn resolve<'a>(&'a self, headers: &'a HeaderMap) -> WhoFuture<'a> {
        Box::pin(async move {
            let Some(who) = signed_in_from_headers(&self.login, headers).await else {
                return Who::Anonymous {
                    sign_in_url: Some(SIGN_IN_PATH.to_owned()),
                };
            };
            match op_role_of(&self.map, &who) {
                Some(role) => Who::Caller {
                    actor: format!("oidc:{}", who.subject),
                    role: role.to_owned(),
                },
                None => Who::Refused {
                    actor: format!("oidc:{}", who.subject),
                },
            }
        })
    }
}
