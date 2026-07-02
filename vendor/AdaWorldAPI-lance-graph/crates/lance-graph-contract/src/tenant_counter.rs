//! `tenant_counter` — per-[`ValueTenant`](crate::canonical_node::ValueTenant)
//! update counters for cheap debug instrumentation of the SoA write cascade.
//!
//! The "easy debug" lever for the capstone NaN-census / wiring measurement: each
//! tenant write bumps an atomic counter, so a probe can read which tenants the
//! update cascade actually touched (and how often) in one cycle — the runtime
//! evidence behind seam-wiring% / run-NaN%.
//!
//! **Compile-time dispatch.** Gated behind the `tenant-counters` feature: when
//! off, [`tenant_update`] is a `#[inline]` no-op (the argument is consumed, the
//! whole call optimizes away — zero cost). When on, it is one relaxed atomic
//! increment into a `LazyLock<[AtomicU64; N]>`. Reads ([`tenant_count`],
//! [`snapshot`]) exist only under the feature.
//!
//! Wire pattern: every tenant setter calls `tenant_update(ValueTenant::X)` (e.g.
//! [`NodeRow::set_kanban`](crate::canonical_node::NodeRow::set_kanban)). As more
//! setters are wired, the cascade becomes self-measuring.

use crate::canonical_node::ValueTenant;

/// Number of distinct [`ValueTenant`] positions the counter array covers —
/// derived from the canonical carve so it can never drift out of sync with the
/// enum (adding a `ValueTenant` grows `VALUE_TENANTS`, which grows this, which
/// grows the counter array — no hand-maintained constant to forget, no
/// out-of-bounds on `tenant as usize`). Equals the highest discriminant + 1
/// because `VALUE_TENANTS` is contiguous discriminant-ordered (canon-asserted).
pub const N_TENANTS: usize = crate::canonical_node::VALUE_TENANTS.len();

#[cfg(feature = "tenant-counters")]
static TENANT_COUNTERS: std::sync::LazyLock<[std::sync::atomic::AtomicU64; N_TENANTS]> =
    std::sync::LazyLock::new(|| std::array::from_fn(|_| std::sync::atomic::AtomicU64::new(0)));

/// Record one update to `tenant`. **No-op unless the `tenant-counters` feature is
/// enabled** (compile-time dispatch — the call optimizes away when off).
#[inline]
pub fn tenant_update(tenant: ValueTenant) {
    #[cfg(feature = "tenant-counters")]
    {
        TENANT_COUNTERS[tenant as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    #[cfg(not(feature = "tenant-counters"))]
    {
        let _ = tenant;
    }
}

/// The current update count for `tenant`. Only available under `tenant-counters`.
#[cfg(feature = "tenant-counters")]
#[must_use]
pub fn tenant_count(tenant: ValueTenant) -> u64 {
    TENANT_COUNTERS[tenant as usize].load(std::sync::atomic::Ordering::Relaxed)
}

/// A snapshot of all per-tenant counters (indexed by `ValueTenant as usize`).
/// Only available under `tenant-counters`.
#[cfg(feature = "tenant-counters")]
#[must_use]
pub fn snapshot() -> [u64; N_TENANTS] {
    std::array::from_fn(|i| TENANT_COUNTERS[i].load(std::sync::atomic::Ordering::Relaxed))
}

#[cfg(all(test, feature = "tenant-counters"))]
mod tests {
    use super::*;

    #[test]
    fn counter_increments_for_its_tenant_only() {
        let before = tenant_count(ValueTenant::Meta);
        tenant_update(ValueTenant::Meta);
        tenant_update(ValueTenant::Meta);
        assert_eq!(tenant_count(ValueTenant::Meta), before + 2);
        // snapshot is consistent with the per-tenant read
        let snap = snapshot();
        assert_eq!(
            snap[ValueTenant::Meta as usize],
            tenant_count(ValueTenant::Meta)
        );
    }
}
