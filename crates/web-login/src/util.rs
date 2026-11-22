// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Small pure helpers: return-path safety, query encoding, cookie text.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `true` for a same-origin relative path: starts with one `/`, no `//`, no backslash and
/// no control characters. Anything else would make `/auth/login` an open redirect.
pub fn safe_return_to(p: &str) -> bool {
    p.starts_with('/')
        && !p.starts_with("//")
        && !p.contains('\\')
        && !p.chars().any(|c| c.is_control())
}

/// Percent-encode a query value (RFC 3986 unreserved characters stay).
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The value of cookie `name` in a `Cookie` header.
pub fn cookie<'a>(header: Option<&'a str>, name: &str) -> Option<&'a str> {
    header?
        .split(';')
        .map(str::trim)
        .find_map(|kv| kv.strip_prefix(name).and_then(|r| r.strip_prefix('=')))
        .filter(|v| !v.is_empty())
}

/// `Set-Cookie` for a `__Host-` cookie; `max_age` 0 clears it.
pub fn set_cookie(name: &str, value: &str, max_age: Duration) -> String {
    format!(
        "{name}={value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={}",
        max_age.as_secs()
    )
}

pub(crate) fn random_token() -> String {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).expect("OS entropy available");
    URL_SAFE_NO_PAD.encode(b)
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_to_accepts_only_same_origin_paths() {
        assert!(safe_return_to("/view"));
        assert!(safe_return_to("/view/customers?c=K01"));
        for bad in [
            "//evil.example",
            "https://evil.example",
            "/\\evil",
            "view",
            "",
            "/a\nb",
        ] {
            assert!(!safe_return_to(bad), "{bad:?}");
        }
    }

    #[test]
    fn cookie_reads_the_named_value_only() {
        let h = Some("a=1; __Host-x_sid=abc; __Host-x_sidx=zzz");
        assert_eq!(cookie(h, "__Host-x_sid"), Some("abc"));
        assert_eq!(cookie(Some("__Host-x_sidx=zzz"), "__Host-x_sid"), None);
        assert_eq!(cookie(None, "__Host-x_sid"), None);
        assert_eq!(cookie(Some("__Host-x_sid="), "__Host-x_sid"), None);
    }

    #[test]
    fn urlencode_keeps_unreserved_only() {
        assert_eq!(urlencode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(urlencode("https://x/y?z=1"), "https%3A%2F%2Fx%2Fy%3Fz%3D1");
    }

    #[test]
    fn set_cookie_carries_every_attribute() {
        let c = set_cookie("__Host-x_sid", "v", Duration::from_secs(5));
        assert_eq!(
            c,
            "__Host-x_sid=v; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=5"
        );
    }
}
