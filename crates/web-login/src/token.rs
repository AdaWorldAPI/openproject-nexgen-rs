// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Claims read from an ID token the verifier has already accepted.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;

/// Role key a consumer may use to mark organization membership; a provider role of that
/// name is never reported, so it cannot be mistaken for one.
const RESERVED_ROLE_KEY: &str = "*";

fn payload(token: &str) -> Option<Value> {
    let part = token.split('.').nth(1)?;
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).ok()?).ok()
}

/// `exp` of an ID token the verifier has already accepted (so the payload is authentic and
/// `exp` is present: `ogar-oidc` requires it).
pub(crate) fn token_exp(token: &str) -> Option<u64> {
    payload(token)?.get("exp")?.as_u64()
}

/// The `aud` entries of an unverified token, for the disable run only.
#[cfg(feature = "disable-api-client-check")]
pub(crate) fn token_audiences(token: &str) -> Vec<String> {
    match payload(token).and_then(|p| p.get("aud").cloned()) {
        Some(Value::String(s)) => vec![s],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// The `(organization id, role key)` pairs in claim `claim`: `{role_key: {org_id: …}}`
/// gives one pair per organization of each role. Anything else is no role. Sorted, without
/// duplicates.
pub(crate) fn token_roles(token: &str, claim: &str) -> Vec<(String, String)> {
    let Some(claims) = payload(token) else {
        return Vec::new();
    };
    let Some(roles) = claims.get(claim).and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = roles
        .iter()
        .filter(|(key, _)| !key.trim().is_empty() && key.as_str() != RESERVED_ROLE_KEY)
        .filter_map(|(key, orgs)| orgs.as_object().map(|o| (key, o)))
        .flat_map(|(key, orgs)| {
            orgs.keys()
                .filter(|org| !org.trim().is_empty() && !org.contains('/'))
                .map(move |org| (org.clone(), key.clone()))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The organization id in claim `claim`: a non-empty string without `/` (so it can never
/// forge a grant key). Anything else is no organization.
pub(crate) fn token_org(token: &str, claim: &str) -> Option<String> {
    let claims = payload(token)?;
    let org = claims.get(claim)?.as_str()?.trim();
    (!org.is_empty() && !org.contains('/')).then(|| org.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DEFAULT_ORG_CLAIM, DEFAULT_ROLES_CLAIM};
    use serde_json::json;

    /// A token whose payload is `claims` (the signature is not looked at here).
    fn unsigned(claims: Value) -> String {
        format!("h.{}.s", URL_SAFE_NO_PAD.encode(claims.to_string()))
    }

    fn zitadel_id_token(roles: Value) -> Value {
        json!({
            "iss": "https://idp.example.test",
            "sub": "290000000000000042",
            "exp": 1_900_000_000,
            "urn:zitadel:iam:org:project:roles": roles.clone(),
            "urn:zitadel:iam:org:project:290000000000000006:roles": roles,
            "urn:zitadel:iam:user:resourceowner:id": "300000000000000001"
        })
    }

    #[test]
    fn token_org_admits_only_a_plain_id() {
        let org = |v: Value| token_org(&unsigned(json!({"o": v})), "o");
        assert_eq!(org(json!("111")), Some("111".to_string()));
        assert_eq!(org(json!(" 111 ")), Some("111".to_string()));
        for bad in [json!("111/*"), json!("1/some-role"), json!(""), json!(111)] {
            assert_eq!(org(bad.clone()), None, "{bad}");
        }
        assert_eq!(token_org(&unsigned(json!({})), "o"), None);
        assert_eq!(token_org("not-a-token", "o"), None);
    }

    #[test]
    fn the_default_claims_read_a_zitadel_id_token() {
        let t = unsigned(zitadel_id_token(json!({
            "role-a": {"300000000000000001": "one.example.test"},
            "role-b": {
                "300000000000000001": "one.example.test",
                "300000000000000002": "two.example.test"
            }
        })));
        assert_eq!(
            token_roles(&t, DEFAULT_ROLES_CLAIM),
            vec![
                ("300000000000000001".to_string(), "role-a".to_string()),
                ("300000000000000001".to_string(), "role-b".to_string()),
                ("300000000000000002".to_string(), "role-b".to_string()),
            ]
        );
        assert_eq!(
            token_org(&t, DEFAULT_ORG_CLAIM).as_deref(),
            Some("300000000000000001")
        );
        assert_eq!(token_exp(&t), Some(1_900_000_000));
    }

    #[test]
    fn role_keys_are_taken_verbatim() {
        let t = unsigned(zitadel_id_token(json!({
            "Role-A": {"300000000000000001": "one.example.test"}
        })));
        assert_eq!(
            token_roles(&t, DEFAULT_ROLES_CLAIM),
            vec![("300000000000000001".to_string(), "Role-A".to_string())]
        );
    }

    #[test]
    fn a_token_without_the_roles_claim_has_no_role() {
        let mut claims = zitadel_id_token(json!({}));
        let map = claims.as_object_mut().unwrap();
        map.remove("urn:zitadel:iam:org:project:roles");
        let t = unsigned(claims);
        assert!(token_roles(&t, DEFAULT_ROLES_CLAIM).is_empty());
        assert_eq!(
            token_org(&t, DEFAULT_ORG_CLAIM).as_deref(),
            Some("300000000000000001")
        );
    }

    #[test]
    fn malformed_role_entries_and_the_reserved_key_yield_nothing() {
        let t = unsigned(json!({"r": {
            "*": {"1": "x"},
            " ": {"1": "x"},
            "ok": {"1/2": "x", "": "x", "  ": "x", "7": "x"},
            "not-an-object": "x"
        }}));
        assert_eq!(
            token_roles(&t, "r"),
            vec![("7".to_string(), "ok".to_string())]
        );
        assert!(token_roles(&unsigned(json!({"r": ["x"]})), "r").is_empty());
    }
}
