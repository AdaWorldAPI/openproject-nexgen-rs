// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The two HTTP calls the flow makes, so tests can run it without a network.

use std::future::Future;
use std::pin::Pin;

/// A boxed response body or a short failure text that never carries a response body.
pub type HttpFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>>;

pub trait OidcHttp: Send + Sync {
    fn get<'a>(&'a self, url: &'a str) -> HttpFuture<'a>;
    fn post_form<'a>(&'a self, url: &'a str, form: &'a [(&'a str, &'a str)]) -> HttpFuture<'a>;
}

/// reqwest over rustls, 10 s timeout, no redirects followed.
#[cfg(feature = "reqwest")]
pub struct ReqwestHttp(reqwest::Client);

#[cfg(feature = "reqwest")]
impl ReqwestHttp {
    pub fn new() -> Self {
        let c = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .expect("reqwest client builds");
        Self(c)
    }
}

#[cfg(feature = "reqwest")]
impl Default for ReqwestHttp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "reqwest")]
async fn body(r: reqwest::Result<reqwest::Response>) -> Result<Vec<u8>, String> {
    let r = r.map_err(|_| "request failed".to_string())?;
    if !r.status().is_success() {
        return Err(format!("status {}", r.status().as_u16()));
    }
    r.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|_| "body read failed".to_string())
}

#[cfg(feature = "reqwest")]
impl OidcHttp for ReqwestHttp {
    fn get<'a>(&'a self, url: &'a str) -> HttpFuture<'a> {
        Box::pin(async move { body(self.0.get(url).send().await).await })
    }
    fn post_form<'a>(&'a self, url: &'a str, form: &'a [(&'a str, &'a str)]) -> HttpFuture<'a> {
        Box::pin(async move { body(self.0.post(url).form(form).send().await).await })
    }
}
