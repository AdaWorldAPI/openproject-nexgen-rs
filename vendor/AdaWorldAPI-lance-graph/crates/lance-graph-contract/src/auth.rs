//! Actor identity context for authentication and authorization.
//!
//! `ActorContext` is the identity envelope extracted from a JWT and
//! carried through the request lifecycle. It is consumed by:
//!
//! - The RLS rewriter (callcenter `rls.rs`) to inject tenant + actor
//!   predicates into every `TableScan`.
//! - The audit log (future) to attribute mutations to an actor.
//! - The SMB session layer to scope queries to a tenant.
//!
//! Lives in the zero-dep contract crate so any consumer can reference
//! the identity shape without pulling in serde, DataFusion, or axum.
//!
//! # Design Decisions (LF-3)
//!
//! - UNKNOWN-3 resolved: RLS via DataFusion LogicalPlan rewriter, NOT
//!   pgwire. Predicates are injected as an optimizer rule.
//! - UNKNOWN-4 resolved: `actor_id: String` (JWT `sub` claim, unchanged).
//!   `CommitFilter.actor_id: Option<u64>` stays as hash for fast filtering
//!   at the commit fan-out layer.

use crate::sla::TenantId;

/// Identity context extracted from a JWT. Carried through the request
/// lifecycle; consumed by the RLS rewriter and audit log.
///
/// # Fields
///
/// - `actor_id` — JWT `sub` claim, unchanged. Canonical actor identity.
///   This is the string the JWT issuer assigned; we do NOT hash or
///   transform it. The `CommitFilter.actor_id: Option<u64>` is a
///   separate hash used for fast commit-level filtering.
///
/// - `tenant_id` — Tenant identifier. Extracted from JWT custom claim
///   (`tenant_id` or `tid`) or derived from the `sub` domain.
///
/// - `roles` — Roles the actor holds. Used by the RLS rewriter to
///   determine which predicates to inject. An actor with the `"admin"`
///   role bypasses the per-actor filter (still gets tenant isolation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorContext {
    /// JWT `sub` claim, unchanged. Canonical actor identity.
    pub actor_id: String,
    /// Tenant identifier. Extracted from JWT custom claim or derived.
    pub tenant_id: TenantId,
    /// Roles the actor holds. Used by the RLS rewriter to determine
    /// which predicates to inject.
    pub roles: Vec<String>,
}

impl ActorContext {
    /// Create a new `ActorContext`.
    pub fn new(actor_id: String, tenant_id: TenantId, roles: Vec<String>) -> Self {
        Self {
            actor_id,
            tenant_id,
            roles,
        }
    }

    /// Returns `true` if the actor holds the `"admin"` role.
    ///
    /// Admin actors bypass the per-actor RLS predicate but still get
    /// tenant-scoped isolation.
    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|r| r == "admin")
    }
}

/// Errors that can occur during JWT extraction / validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthError {
    /// Token is not in the expected `header.payload.signature` format.
    MalformedToken,
    /// Base64 decoding of the payload section failed.
    InvalidBase64,
    /// Payload JSON is not valid or is missing required fields.
    InvalidPayload(String),
    /// The `sub` claim is missing from the JWT payload.
    MissingSub,
    /// Signature verification failed (Phase 2 — not yet implemented).
    InvalidSignature,
}

impl core::fmt::Display for AuthError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MalformedToken => write!(f, "JWT is not in header.payload.signature format"),
            Self::InvalidBase64 => write!(f, "base64 decoding of JWT payload failed"),
            Self::InvalidPayload(msg) => write!(f, "invalid JWT payload: {msg}"),
            Self::MissingSub => write!(f, "JWT payload missing required 'sub' claim"),
            Self::InvalidSignature => write!(f, "JWT signature verification failed"),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_context_new() {
        let ctx = ActorContext::new("user@example.com".into(), 42, vec!["viewer".into()]);
        assert_eq!(ctx.actor_id, "user@example.com");
        assert_eq!(ctx.tenant_id, 42);
        assert_eq!(ctx.roles, vec!["viewer"]);
    }

    #[test]
    fn is_admin_true() {
        let ctx = ActorContext::new("a".into(), 1, vec!["admin".into()]);
        assert!(ctx.is_admin());
    }

    #[test]
    fn is_admin_false_empty_roles() {
        let ctx = ActorContext::new("a".into(), 1, vec![]);
        assert!(!ctx.is_admin());
    }

    #[test]
    fn is_admin_false_other_roles() {
        let ctx = ActorContext::new("a".into(), 1, vec!["viewer".into(), "editor".into()]);
        assert!(!ctx.is_admin());
    }

    #[test]
    fn is_admin_among_many_roles() {
        let ctx = ActorContext::new("a".into(), 1, vec!["viewer".into(), "admin".into()]);
        assert!(ctx.is_admin());
    }

    #[test]
    fn auth_error_display() {
        assert_eq!(
            AuthError::MalformedToken.to_string(),
            "JWT is not in header.payload.signature format"
        );
        assert_eq!(
            AuthError::MissingSub.to_string(),
            "JWT payload missing required 'sub' claim"
        );
        assert_eq!(
            AuthError::InvalidPayload("bad json".into()).to_string(),
            "invalid JWT payload: bad json"
        );
    }
}
