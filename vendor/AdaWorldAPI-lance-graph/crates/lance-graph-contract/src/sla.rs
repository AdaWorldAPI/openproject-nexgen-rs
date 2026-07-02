//! Outside-BBB integration types for SLA contracts and multi-tenant
//! isolation (LF-91, LF-92).
//!
//! These types belong to the agnostic surface that external consumers
//! (REST adaptors, gRPC bridges, downstream apps) bind against without
//! knowing anything about the LanceMembrane internals. They are pure
//! data — no method wiring elsewhere in the contract crate.
//!
//! # LF-91 — SLA policy
//!
//! `SlaPolicy` declares the latency / freshness / priority envelope a
//! query or projection commits to honor. Const constructors keep the
//! type usable in `&'static` schemas; `SlaPolicy::STANDARD` and
//! `SlaPolicy::INTERACTIVE` are the two pre-baked tiers.
//!
//! # LF-92 — Multi-tenant isolation
//!
//! `TenantId` is a stable u64 embedded in CommitFilter and AuditEntry
//! signatures so cross-tenant data never leaks through a shared
//! LanceMembrane. `TenantScope` narrows a query to one tenant
//! (`Single`), a federated set (`Multi`), or `All` (admin / cross-
//! tenant analytics, requires policy override at the bridge).

// ═══════════════════════════════════════════════════════════════════════════
// LF-91 — SLA POLICY
// ═══════════════════════════════════════════════════════════════════════════

/// Service-level objective scope. Tells external consumers what
/// guarantees a query / projection commits to honor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SlaPolicy {
    pub max_latency_ms: u32,
    pub min_freshness_ms: u32, // staleness ceiling
    pub priority: SlaPriority,
}

/// Priority tier ordering for SLA scheduling. `Background` is the
/// lowest priority, `Urgent` the highest. `PartialOrd`/`Ord` derived
/// from declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SlaPriority {
    Background,
    Standard,
    Interactive,
    Urgent,
}

impl SlaPolicy {
    /// Standard tier — 1 s latency budget, 1 min staleness ceiling.
    /// Default for batch / dashboard / non-interactive paths.
    pub const STANDARD: SlaPolicy = SlaPolicy {
        max_latency_ms: 1_000,
        min_freshness_ms: 60_000,
        priority: SlaPriority::Standard,
    };

    /// Interactive tier — 100 ms latency budget, 1 s staleness ceiling.
    /// For user-facing chat / search / autocomplete paths.
    pub const INTERACTIVE: SlaPolicy = SlaPolicy {
        max_latency_ms: 100,
        min_freshness_ms: 1_000,
        priority: SlaPriority::Interactive,
    };

    pub const fn new(max_latency_ms: u32, min_freshness_ms: u32, priority: SlaPriority) -> Self {
        Self {
            max_latency_ms,
            min_freshness_ms,
            priority,
        }
    }
}

impl Default for SlaPolicy {
    fn default() -> Self {
        SlaPolicy::STANDARD
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LF-92 — MULTI-TENANT ISOLATION
// ═══════════════════════════════════════════════════════════════════════════

/// Tenant identifier. Stable across queries / projections;
/// embedded in CommitFilter and AuditEntry signatures so
/// cross-tenant data never leaks through a shared LanceMembrane.
pub type TenantId = u64;

/// Scope a query or projection to one or more tenants.
/// Single = strict isolation; Multi = federated read with
/// per-tenant marking applied to each row.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum TenantScope {
    /// Strict single-tenant isolation.
    Single(TenantId),
    /// Federated read across the listed tenants; per-tenant marking applies per row.
    Multi(Vec<TenantId>),
    /// Default — unrestricted; admin / cross-tenant analytics. Requires policy override.
    /// CommitFilter narrows from this.
    #[default]
    All,
}

impl TenantScope {
    pub fn contains(&self, tenant: TenantId) -> bool {
        match self {
            Self::Single(t) => *t == tenant,
            Self::Multi(ts) => ts.contains(&tenant),
            Self::All => true,
        }
    }

    pub fn as_slice(&self) -> &[TenantId] {
        match self {
            Self::Single(t) => std::slice::from_ref(t),
            Self::Multi(ts) => ts.as_slice(),
            Self::All => &[],
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── LF-91 SLA policy ──

    #[test]
    fn sla_policy_standard_constants() {
        let s = SlaPolicy::STANDARD;
        assert_eq!(s.max_latency_ms, 1_000);
        assert_eq!(s.min_freshness_ms, 60_000);
        assert_eq!(s.priority, SlaPriority::Standard);
    }

    #[test]
    fn sla_policy_interactive_constants() {
        let s = SlaPolicy::INTERACTIVE;
        assert_eq!(s.max_latency_ms, 100);
        assert_eq!(s.min_freshness_ms, 1_000);
        assert_eq!(s.priority, SlaPriority::Interactive);
    }

    #[test]
    fn sla_policy_default_is_standard() {
        assert_eq!(SlaPolicy::default(), SlaPolicy::STANDARD);
    }

    #[test]
    fn sla_priority_ordering() {
        // Declaration order: Background < Standard < Interactive < Urgent.
        assert!(SlaPriority::Background < SlaPriority::Standard);
        assert!(SlaPriority::Standard < SlaPriority::Interactive);
        assert!(SlaPriority::Interactive < SlaPriority::Urgent);
        assert!(SlaPriority::Background < SlaPriority::Urgent);
    }

    #[test]
    fn sla_policy_const_new() {
        const CUSTOM: SlaPolicy = SlaPolicy::new(50, 500, SlaPriority::Urgent);
        assert_eq!(CUSTOM.max_latency_ms, 50);
        assert_eq!(CUSTOM.min_freshness_ms, 500);
        assert_eq!(CUSTOM.priority, SlaPriority::Urgent);
    }

    // ── LF-92 multi-tenant isolation ──

    #[test]
    fn tenant_scope_contains_single() {
        let s = TenantScope::Single(42);
        assert!(s.contains(42));
        assert!(!s.contains(7));
    }

    #[test]
    fn tenant_scope_contains_multi() {
        let s = TenantScope::Multi(vec![1, 2, 3]);
        assert!(s.contains(1));
        assert!(s.contains(3));
        assert!(!s.contains(4));
    }

    #[test]
    fn tenant_scope_contains_all() {
        let s = TenantScope::All;
        assert!(s.contains(0));
        assert!(s.contains(u64::MAX));
    }

    #[test]
    fn tenant_scope_default_is_all() {
        assert_eq!(TenantScope::default(), TenantScope::All);
    }

    #[test]
    fn tenant_scope_as_slice() {
        let single = TenantScope::Single(7);
        assert_eq!(single.as_slice(), &[7][..]);

        let multi = TenantScope::Multi(vec![1, 2, 3]);
        assert_eq!(multi.as_slice(), &[1, 2, 3][..]);

        let all = TenantScope::All;
        assert!(all.as_slice().is_empty());
    }
}
