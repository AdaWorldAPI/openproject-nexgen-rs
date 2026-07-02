//! `rbac` — the classid-keyed authorization trait surface (OGAR keystone §4/§11).
//!
//! The keystone §11 build order places the **`ClassRbac` grant-resolution trait**
//! in this zero-dep contract crate so that *both* the concrete kernel
//! (`lance-graph-rbac`, which holds `authorize()` + `Policy` + the `0x0B` auth
//! membrane) *and* the active-record `ClassView` producer (`lance-graph-ogar`'s
//! `OgarClassView`, which deps contract but **not** rbac) can implement / consume
//! one trait. Before this module the trait lived in `lance-graph-rbac`, so ogar —
//! which does not depend on rbac — could not satisfy the keystone's
//! `impl ClassRbac for OgarClassView` (Q5). This is that placement.
//!
//! Only the **trait + the `Operation` it ranges over** live here (pure types, no
//! runtime — `Operation` reads [`PrefetchDepth`](crate::property::PrefetchDepth),
//! already in this crate). The concrete `authorize()` kernel, `ClassGrants`,
//! `Policy`, `AccessDecision`, and the auth membrane stay in `lance-graph-rbac`;
//! it **re-exports** these so existing `lance_graph_rbac::authorize::ClassRbac` /
//! `lance_graph_rbac::policy::Operation` paths are unchanged (callcenter +
//! the sibling `smb-realtime` / `medcare-realtime` gates keep compiling).
//!
//! # Relationship to the rest of the contract auth surface
//!
//! - [`crate::auth::ActorContext`] is the *resolved actor identity* (actor id +
//!   tenant + roles). `lance-graph-rbac`'s `auth::ResolvedIdentity` (the `0x0B`
//!   membrane output) carries the same triple plus the resolving provider's
//!   classid; converging the two onto `ActorContext` is a tracked follow-on, not
//!   forced here.
//! - [`crate::external_membrane::MembraneGate`] is the *gate* a consumer impls to
//!   admit/deny an external commit; `ClassRbac` is the *grant resolution* a gate
//!   consults. They compose: a gate calls `authorize(rbac, actor, class, op)`.

use crate::class_view::FieldMask;
use crate::property::PrefetchDepth;

/// §3/§4 compiled scope-and-projection token — the `(tenant, predicate_key)` pair
/// that constrains a role's row-visibility on a class. `predicate_key = 0` means
/// tenant-only scope (the common case). Intentionally opaque: NO `evaluate` /
/// interpret methods — it is a compiled address token, not a runtime policy
/// engine; the kernel resolves it against the store, never interprets it inline.
/// `Copy` POD (no heap).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ScopeSpec {
    /// The tenant discriminant. `None` = global (cross-tenant) scope.
    pub tenant: Option<u64>,
    /// Reserved predicate key (0 = tenant-only; non-zero reserved for future
    /// predicate-scoped row filters — do NOT interpret in this PR).
    pub predicate_key: u32,
    /// The **empty** scope — `true` ⇒ no row is visible. This is the sound
    /// representation of an irreconcilable [`intersect`](ScopeSpec::intersect):
    /// when two granting roles bind DIFFERENT tenants, no row can satisfy both,
    /// so the AND-fold is empty — NOT "one tenant arbitrarily wins" (which would
    /// silently *widen* visibility to a tenant the other role never granted).
    /// `deny` is absorbing: `deny ∩ anything = deny`. Default `false` (a fresh
    /// scope denies nothing).
    pub deny: bool,
}

impl ScopeSpec {
    /// The empty scope — denies every row. The absorbing element of
    /// [`intersect`](ScopeSpec::intersect).
    pub const DENY: ScopeSpec = ScopeSpec {
        tenant: None,
        predicate_key: 0,
        deny: true,
    };

    /// Restrictive intersection of two scopes — the AND-fold `authorize_scoped`
    /// uses when a user holds several granting roles (each row must satisfy
    /// EVERY granting role's scope). `None` (global) ∩ x = x (the specific one
    /// restricts). Two **distinct** tenants is a genuine conflict: no row lives
    /// in both, so the result is the empty scope ([`ScopeSpec::DENY`]) — never
    /// "self wins" (that would widen visibility to `self`'s tenant, which the
    /// other granting role never authorized). `deny` is absorbing. `predicate_key`s
    /// are OR-combined (reserved; both keys apply once a consumer interprets them).
    #[inline]
    #[must_use]
    pub const fn intersect(self, other: Self) -> Self {
        if self.deny || other.deny {
            return ScopeSpec::DENY;
        }
        let predicate_key = self.predicate_key | other.predicate_key;
        match (self.tenant, other.tenant) {
            (None, t) | (t, None) => Self {
                tenant: t,
                predicate_key,
                deny: false,
            },
            (Some(a), Some(b)) if a == b => Self {
                tenant: Some(a),
                predicate_key,
                deny: false,
            },
            // Distinct tenants: empty intersection — no row satisfies both.
            (Some(_), Some(_)) => ScopeSpec::DENY,
        }
    }
}

/// The codebook class identity an authorization targets — the
/// [`NodeGuid`](crate::NodeGuid) `classid` (its canon half is the codebook id;
/// compose via [`render_classid`](crate::ogar_codebook::render_classid)).
/// Opaque to the kernel: it is compared and looked up, never decoded (the kernel
/// "never touches a token" — only resolved keys go inward).
pub type ClassId = u32;

/// An actor identity. In the full keystone this is the OIDC `sub` resolved to a
/// membership-set ([`crate::auth::ActorContext`]); here it is the opaque key a
/// [`ClassRbac`] impl maps to roles.
pub type ActorId<'a> = &'a str;

/// A role identity (a minted role classid in the full keystone; a role *name*
/// where reconciling against a string-keyed policy).
pub type RoleId = &'static str;

/// What a caller wants to do on a class — the op the [`ClassRbac`] grant gate
/// ranges over. Read is depth-graded ([`PrefetchDepth`]); Write names a
/// predicate; Act names an action. (Promoted from `lance-graph-rbac`'s
/// `policy::Operation`, keystone §11; that path re-exports this type.)
#[derive(Clone, Debug)]
pub enum Operation<'a> {
    /// Read up to a prefetch depth.
    Read {
        /// The requested read depth (`Identity` < … < `Full`).
        depth: PrefetchDepth,
    },
    /// Write a specific predicate.
    Write {
        /// The predicate being written.
        predicate: &'a str,
    },
    /// Trigger a named action.
    Act {
        /// The action name.
        action: &'a str,
    },
}

/// The §4 grant-resolution surface, **classid-keyed**. The single trait both the
/// membrane gate and the cognitive loop resolve access through; the impl owns the
/// membership→role folding and the `(role, class)` grant table. `lance-graph-rbac`
/// supplies the reference impl (`ClassGrants`) + the `authorize()` kernel that
/// consumes it; `lance-graph-ogar`'s `OgarClassView` is the keystone's intended
/// active-record impl (Q5).
pub trait ClassRbac {
    /// Roles the actor holds, already folded through
    /// membership → member_role → role (the §4 `actor_roles`). Empty ⇒ the actor
    /// is unknown to the policy.
    fn actor_roles(&self, actor: ActorId<'_>) -> &[RoleId];

    /// Does `role` carry a grant on `class` that permits `op`? The positive
    /// `R⁺` op-mask gate (§5 stage 1). No grant, or a grant that does not permit
    /// the op, ⇒ `false` (restrictive default-deny).
    fn grant_permits(&self, role: RoleId, class: ClassId, op: &Operation<'_>) -> bool;

    /// Axis-2 role-hierarchy fold hook — the roles that *reach* `class` through a
    /// hierarchy above the direct actor roles. Default empty: the kernel consults
    /// only [`actor_roles`](ClassRbac::actor_roles) when this returns `&[]`.
    /// **CONJECTURE (not implemented this PR):** a non-empty return will be folded
    /// in by a future keystone phase; today's `authorize()` / `authorize_scoped()`
    /// do NOT call this method (they use `actor_roles ∧ grant_permits`).
    fn roles_reaching(&self, _class: ClassId) -> &[RoleId] {
        &[]
    }

    /// Axis-3 compiled scope — the row-visibility constraint for `role` on
    /// `class`. `None` (global, see all rows) by default. A non-`None` value is a
    /// pre-compiled [`ScopeSpec`] token; the kernel resolves it against the store
    /// as an address, never interprets it inline.
    fn row_scope(&self, _role: RoleId, _class: ClassId) -> Option<ScopeSpec> {
        None
    }

    /// Axis-4 field projection — the column mask permitted for `role` on `class`.
    /// [`FieldMask::FULL`] (all fields) by default; a narrower mask gives
    /// column-level RBAC. The kernel intersects this with the query's own
    /// projection before emitting rows.
    fn field_mask(&self, _role: RoleId, _class: ClassId) -> FieldMask {
        FieldMask::FULL
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// §6 — the typed `granted` value-tenant (first-class replacement for
// `project_role.permissions: text`).
// ─────────────────────────────────────────────────────────────────────────────

/// The verb bitmask of a class-grant — the §3 axis-1 "verb × class" gate, one
/// `u8`, palette-native (#511 `SoaMemberSpec`: a role's grants are low-tens, one
/// column). Shaped after Odoo `ir.model.access`'s `perm_{read,write,create,unlink}`.
///
/// This is the **coarse verb gate** (§5 stage 1). It answers "may this role
/// *read / write / act on* this class at all", not the finer "at what depth /
/// which predicate / which action name" — those are the field-projection (axis 4)
/// and row-scope (axis 3) refinements that layer *above* a passed verb gate. So
/// [`OpMask::permits`] maps [`Operation::Read`] → the `READ` bit regardless of
/// depth; a depth/predicate/action-name check is a separate, finer stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub struct OpMask(pub u8);

impl OpMask {
    /// May read the class (any depth).
    pub const READ: OpMask = OpMask(1 << 0);
    /// May write a predicate on the class.
    pub const WRITE: OpMask = OpMask(1 << 1);
    /// May create an instance (Odoo `perm_create`).
    pub const CREATE: OpMask = OpMask(1 << 2);
    /// May delete an instance (Odoo `perm_unlink`).
    pub const DELETE: OpMask = OpMask(1 << 3);
    /// May trigger a named action (the DO arm — `ActionDef` fire).
    pub const ACT: OpMask = OpMask(1 << 4);

    /// The empty mask — grants nothing (restrictive default-deny).
    pub const NONE: OpMask = OpMask(0);

    /// Union of two masks (grant composition; e.g. role-hierarchy fold).
    #[inline]
    #[must_use]
    pub const fn union(self, other: OpMask) -> OpMask {
        OpMask(self.0 | other.0)
    }

    /// Whether `self` carries every bit of `bits`.
    #[inline]
    #[must_use]
    pub const fn contains(self, bits: OpMask) -> bool {
        self.0 & bits.0 == bits.0
    }

    /// Whether this mask permits `op` — the verb gate. `Read` → `READ`,
    /// `Write` → `WRITE`, `Act` → `ACT` (depth / predicate / action-name are
    /// finer stages, not decided here).
    #[inline]
    #[must_use]
    pub fn permits(self, op: &Operation<'_>) -> bool {
        let bit = match op {
            Operation::Read { .. } => OpMask::READ,
            Operation::Write { .. } => OpMask::WRITE,
            Operation::Act { .. } => OpMask::ACT,
        };
        self.contains(bit)
    }
}

/// One typed class-grant tuple — `(target_classid: u16, op_mask: u8)`. The
/// first-class, palette-native replacement for the `project_role.permissions:
/// text` blob (keystone §6 / I-K0 registry axiom: "decisions key on `classid`,
/// not on text"). A role's `granted` value-tenant is a `&[ClassGrant]`.
///
/// `target_classid` is the **CANON `u16` codebook id** (the shared-concept half
/// of a [`NodeGuid`](crate::NodeGuid)'s `classid` — the HIGH u16 since the
/// 2026-07-02 half-order flip) — the RBAC + ontology identity,
/// app-render-skin-independent (the custom half chooses render, never grants).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub struct ClassGrant {
    /// The class this grant targets (canon-`u16` codebook id).
    pub target_classid: u16,
    /// The verbs this grant permits on that class.
    pub op_mask: OpMask,
}

impl ClassGrant {
    /// Construct a grant.
    #[inline]
    #[must_use]
    pub const fn new(target_classid: u16, op_mask: OpMask) -> Self {
        Self {
            target_classid,
            op_mask,
        }
    }

    /// Whether this grant permits `op` on `class`. Matches on the **CANON
    /// half** of `class` (the codebook id) via the mint-forward compat reader
    /// [`classid_canon_compat`](crate::ogar_codebook::classid_canon_compat) —
    /// a grant authored against the shared concept applies regardless of
    /// which app's render-skin (custom half) the `ClassId` carries, AND
    /// regardless of whether the id is a post-flip (canon HIGH) or persisted
    /// pre-flip (canon LOW) stored form. Never `class as u16` — post-flip
    /// that reads the custom half and collapses every class (codex P2 #627).
    #[inline]
    #[must_use]
    pub fn permits(&self, class: ClassId, op: &Operation<'_>) -> bool {
        self.target_classid == crate::ogar_codebook::classid_canon_compat(class)
            && self.op_mask.permits(op)
    }
}

/// Does any grant in a role's `granted` set permit `op` on `class`? The slice
/// form of the §5 stage-1 positive op-gate — the body a typed [`ClassRbac`] impl
/// uses for `grant_permits` (restrictive default-deny: empty ⇒ `false`).
#[must_use]
pub fn grants_permit(granted: &[ClassGrant], class: ClassId, op: &Operation<'_>) -> bool {
    granted.iter().any(|g| g.permits(class, op))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_reads_prefetch_depth() {
        // Operation ranges over the contract's own PrefetchDepth — no rbac dep.
        let op = Operation::Read {
            depth: PrefetchDepth::Full,
        };
        assert!(matches!(op, Operation::Read { .. }));
    }

    // A trivial in-contract ClassRbac impl proves the trait is satisfiable with
    // contract-only types (the property ogar relies on: deps contract, not rbac).
    struct OneRole;
    impl ClassRbac for OneRole {
        fn actor_roles(&self, _actor: ActorId<'_>) -> &[RoleId] {
            const R: &[RoleId] = &["reader"];
            R
        }
        fn grant_permits(&self, role: RoleId, class: ClassId, op: &Operation<'_>) -> bool {
            role == "reader" && class == 0x0901 && matches!(op, Operation::Read { .. })
        }
    }

    #[test]
    fn trait_is_satisfiable_with_contract_only_types() {
        let rbac = OneRole;
        assert_eq!(rbac.actor_roles("anyone"), &["reader"]);
        assert!(rbac.grant_permits(
            "reader",
            0x0901,
            &Operation::Read {
                depth: PrefetchDepth::Identity
            }
        ));
        assert!(!rbac.grant_permits("reader", 0x0901, &Operation::Act { action: "x" }));
    }

    // ── §6 typed `granted` value-tenant ──

    const PATIENT: ClassId = 0x0000_0901;

    #[test]
    fn opmask_permits_the_matching_verb_only() {
        let rw = OpMask::READ.union(OpMask::WRITE);
        assert!(rw.permits(&Operation::Read {
            depth: PrefetchDepth::Full
        }));
        assert!(rw.permits(&Operation::Write { predicate: "x" }));
        assert!(!rw.permits(&Operation::Act { action: "approve" }));
        // contains is bit-subset
        assert!(rw.contains(OpMask::READ));
        assert!(!rw.contains(OpMask::ACT));
        assert_eq!(OpMask::NONE, OpMask::default());
    }

    #[test]
    fn class_grant_matches_on_canon_codebook_id() {
        let grant = ClassGrant::new(0x0901, OpMask::READ.union(OpMask::ACT));
        let read = Operation::Read {
            depth: PrefetchDepth::Identity,
        };
        // Same concept, different app render-skin (custom half) → still
        // permitted: the grant keys on the shared-concept CANON half, never
        // the render half. Post-flip forms: concept HIGH, any prefix LOW.
        assert!(grant.permits(0x0901_0000, &read)); // core lens
        assert!(grant.permits(0x0901_0005, &read)); // Healthcare lens
        assert!(grant.permits(0x0901_AB12, &read)); // arbitrary custom half
                                                    // Mint-forward: persisted PRE-flip forms (canon LOW, §2-allocated
                                                    // prefixes < 0x0100 or the 0x1000 V3 marker) still match through the
                                                    // compat reader — a re-bake is never required for authorization.
        assert!(grant.permits(0x0000_0901, &read)); // legacy core
        assert!(grant.permits(0x0005_0901, &read)); // legacy Healthcare render
                                                    // Wrong concept → denied even with the verb (both forms).
        assert!(!grant.permits(0x0902_0000, &read));
        assert!(!grant.permits(0x0000_0902, &read));
        // Right concept, ungranted verb → denied.
        assert!(!grant.permits(0x0901_0000, &Operation::Write { predicate: "due" }));
    }

    /// A typed [`ClassRbac`] impl whose `grant_permits` body IS [`grants_permit`]
    /// over a role's `granted` value-tenant — the §6 shape end-to-end, proving the
    /// typed tenant replaces `permissions: text` with contract-only types.
    struct TypedRoleGrants {
        // physician → {READ+ACT on PATIENT}; cashier → {READ on PATIENT}
        physician: [ClassGrant; 1],
        cashier: [ClassGrant; 1],
    }
    impl ClassRbac for TypedRoleGrants {
        fn actor_roles(&self, actor: ActorId<'_>) -> &[RoleId] {
            match actor {
                "dr-house" => &["physician"],
                "betty" => &["cashier"],
                _ => &[],
            }
        }
        fn grant_permits(&self, role: RoleId, class: ClassId, op: &Operation<'_>) -> bool {
            let granted: &[ClassGrant] = match role {
                "physician" => &self.physician,
                "cashier" => &self.cashier,
                _ => &[],
            };
            grants_permit(granted, class, op)
        }
    }

    #[test]
    fn typed_granted_drives_grant_permits() {
        let rbac = TypedRoleGrants {
            physician: [ClassGrant::new(0x0901, OpMask::READ.union(OpMask::ACT))],
            cashier: [ClassGrant::new(0x0901, OpMask::READ)],
        };
        let act = Operation::Act { action: "approve" };
        // physician may act; cashier may not — restrictive default-deny.
        assert!(rbac.grant_permits("physician", PATIENT, &act));
        assert!(!rbac.grant_permits("cashier", PATIENT, &act));
        // both may read
        let read = Operation::Read {
            depth: PrefetchDepth::Identity,
        };
        assert!(rbac.grant_permits("physician", PATIENT, &read));
        assert!(rbac.grant_permits("cashier", PATIENT, &read));
    }

    // ── F1: axis-2/3/4 default-body + override tests ──

    /// A minimal 2-method impl still compiles after adding default methods —
    /// E0046 cannot fire on default bodies — and the defaults are positive-preserving.
    #[test]
    fn defaults_preserve_two_method_impls() {
        let rbac = OneRole;
        assert_eq!(rbac.roles_reaching(0x0901), &[] as &[RoleId]); // axis-2 empty
        assert!(rbac.row_scope("reader", 0x0901).is_none()); // axis-3 global
        assert_eq!(rbac.field_mask("reader", 0x0901), FieldMask::FULL); // axis-4 all fields
    }

    /// A 5-method impl overriding all three new axis methods — proves the hooks
    /// are individually overridable.
    struct FullRbac;
    impl ClassRbac for FullRbac {
        fn actor_roles(&self, _actor: ActorId<'_>) -> &[RoleId] {
            const R: &[RoleId] = &["admin"];
            R
        }
        fn grant_permits(&self, _role: RoleId, _class: ClassId, _op: &Operation<'_>) -> bool {
            true
        }
        fn roles_reaching(&self, _class: ClassId) -> &[RoleId] {
            const R: &[RoleId] = &["super-admin"];
            R
        }
        fn row_scope(&self, _role: RoleId, _class: ClassId) -> Option<ScopeSpec> {
            Some(ScopeSpec {
                tenant: Some(42),
                predicate_key: 0,
                deny: false,
            })
        }
        fn field_mask(&self, _role: RoleId, _class: ClassId) -> FieldMask {
            FieldMask::EMPTY
        }
    }

    #[test]
    fn override_all_three_axis_methods() {
        let rbac = FullRbac;
        assert_eq!(rbac.roles_reaching(0x0901), &["super-admin"]);
        let scope = rbac.row_scope("admin", 0x0901).expect("should be Some");
        assert_eq!(scope.tenant, Some(42));
        assert_eq!(scope.predicate_key, 0);
        assert_eq!(rbac.field_mask("admin", 0x0901), FieldMask::EMPTY);
    }

    #[test]
    fn scope_intersect_is_restrictive() {
        let global = ScopeSpec::default(); // tenant None, denies nothing
        let t1 = ScopeSpec {
            tenant: Some(1),
            predicate_key: 0,
            deny: false,
        };
        let t2 = ScopeSpec {
            tenant: Some(2),
            predicate_key: 0,
            deny: false,
        };
        // global ∩ specific = specific (the specific one restricts)
        assert_eq!(global.intersect(t1).tenant, Some(1));
        assert!(!global.intersect(t1).deny);
        assert_eq!(t1.intersect(global).tenant, Some(1));
        // same tenant ∩ itself = itself (still visible)
        assert_eq!(t1.intersect(t1).tenant, Some(1));
        assert!(!t1.intersect(t1).deny);
        // distinct tenants: EMPTY intersection (deny), never "self wins" —
        // self-wins would widen visibility to tenant 1 that t2 never granted.
        assert!(t1.intersect(t2).deny);
        assert_eq!(t1.intersect(t2), ScopeSpec::DENY);
        // deny is absorbing
        assert!(ScopeSpec::DENY.intersect(t1).deny);
        assert!(t1.intersect(ScopeSpec::DENY).deny);
    }
}
