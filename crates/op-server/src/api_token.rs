// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Bearer tokens of machine clients for `/api/v3`: a token verified by [`Login::verify_bearer`]
//! acts as the OpenProject login `OP_API_CALLERS` gives its subject, and as nobody otherwise.

use std::collections::BTreeMap;
use std::sync::Arc;

use op_api::extractors::{ApiTokenVerifier, TokenFuture};
use web_login::Login;

pub struct LoginTokenVerifier {
    login: Arc<Login>,
    callers: BTreeMap<String, String>,
}

impl LoginTokenVerifier {
    pub fn new(login: Arc<Login>, callers: BTreeMap<String, String>) -> LoginTokenVerifier {
        LoginTokenVerifier { login, callers }
    }
}

impl ApiTokenVerifier for LoginTokenVerifier {
    fn verify<'a>(&'a self, token: &'a str) -> TokenFuture<'a> {
        Box::pin(async move {
            let who = self.login.verify_bearer(token).await.ok()?;
            self.callers.get(&who.subject).cloned()
        })
    }
}
