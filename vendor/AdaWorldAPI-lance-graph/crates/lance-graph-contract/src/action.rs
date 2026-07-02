//! `action` — the **DO arm** of the OGAR IR: `ActionDef` (static declaration) +
//! `ActionInvocation` (dynamic fire). The Perdurant complement of the Endurant
//! field-set: where `class_view`/`codegen_manifest` carry a class's *state* and
//! *method signatures* (THINK), this carries its *functional actions* (DO).
//!
//! Sourced from the SPO harvest's `has_function` rows (the Perdurant methods)
//! and shaped per OGAR `OGAR-AST-CONTRACT.md` §1. An `ActionInvocation` rides
//! the canonical [`crate::orchestration::UnifiedStep`] envelope to a
//! [`crate::kanban::ExecTarget`] (native / jit / **SurrealQL** / elixir) — it is
//! NOT a per-crate endpoint.
//!
//! # OGAR inheritance (classid → ClassView)
//!
//! A class's DO surface is **its own actions plus its parents'**, minus
//! overrides — the same `classid → ClassView` inheritance the field-set uses.
//! [`ActionDef::overrides`] names a parent-class action this one supersedes;
//! [`effective_actions`] composes the inherited set. No adapter carries its own
//! action table; the harvest IS the manifest (mirrors
//! [`crate::codegen_manifest`]).
//!
//! # The commit gate (RBAC + MUL) — why an action does not fire freely
//!
//! A DO action mutates an external domain consumer (odoo-rs / openproject /
//! woa-rs / tesseract-rs), so it is high-stakes: [`ActionInvocation::commit`]
//! advances `Pending → Committed` **only if** (a) the actor is RBAC-authorized
//! for the action's [`ActionDef::required_role`] ([`crate::auth::ActorContext`]),
//! AND (b) the MUL impact assessment ([`crate::mul::GateDecision`]) is `Flow`.
//! A `Hold` keeps it `Pending` (escalate / re-assess), a `Block` `Cancelled`,
//! and an unauthorized actor `Failed`. This IS the "commit to the external
//! consumer after the cycle decides the result sound" egress.

use crate::auth::ActorContext;
use crate::canonical_node::NodeGuid;
use crate::kanban::ExecTarget;
use crate::mul::GateDecision;

/// Lifecycle state of an [`ActionInvocation`] (OGAR `ActionStateKind`).
///
/// `Pending → Committed | Failed | Cancelled`. The terminal states are sticky.
/// Maps onto the Rubicon commit boundary: an action is `Pending` until the
/// cycle decides the result sound (RBAC + MUL pass), then it commits OUT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ActionState {
    /// Declared/fired, not yet adjudicated (or `Hold`-escalated).
    #[default]
    Pending = 0,
    /// RBAC-authorized + MUL `Flow` — dispatched to the external consumer.
    Committed = 1,
    /// RBAC-unauthorized (or runtime failure).
    Failed = 2,
    /// MUL `Block` — impact judged unsound, refused.
    Cancelled = 3,
}

impl ActionState {
    /// Whether this is a terminal (non-`Pending`) state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        !matches!(self, ActionState::Pending)
    }
}

/// A `KausalSpec::StateGuard` on an [`ActionDef`] — the action fires only when
/// `field` holds `value`. `const`-constructible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateGuard {
    /// The field name guarded.
    pub field: &'static str,
    /// The value `field` must hold for the action to be eligible.
    pub value: &'static str,
}

/// DO arm — **static** action declaration (one per handler / transition), the
/// Perdurant sibling of [`crate::codegen_manifest::MethodSig`]. All fields are
/// `&'static`/`Copy` so a generated `const ACTIONS: &[ActionDef] = &[..]`
/// compiles — the action-axis manifest the consumer repos emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionDef {
    /// Event / handler name — the `has_function` method (e.g. `"action_confirm"`).
    pub predicate: &'static str,
    /// The acted-upon class (OGAR classid).
    pub object_class: u32,
    /// Where the action's work runs (native / jit / SurrealQL / elixir).
    pub exec: ExecTarget,
    /// Optional `KausalSpec::StateGuard`: fire only when `field == value`.
    pub guard: Option<StateGuard>,
    /// RBAC role required to invoke this action (`None` = unguarded).
    /// Checked against [`crate::auth::ActorContext`] at commit.
    pub required_role: Option<&'static str>,
    /// Fully-qualified parent-class action this overrides (OGAR `classid →
    /// ClassView` inheritance), if any. `None` = a fresh action.
    pub overrides: Option<&'static str>,
}

impl ActionDef {
    /// Whether this action overrides a parent-class action (OGAR inheritance).
    /// Mirrors [`crate::codegen_manifest::MethodSig::is_override`].
    #[must_use]
    pub const fn is_override(&self) -> bool {
        self.overrides.is_some()
    }
}

/// One class's action manifest, keyed by classid — the action-axis sibling of
/// [`crate::codegen_manifest::ClassMethods`]. Generated downstream; the Core
/// provides the type + the (inheritance-aware) lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassActions {
    /// The OGAR classid this manifest belongs to.
    pub classid: u32,
    /// The class's own actions (a generated `const` table).
    pub actions: &'static [ActionDef],
}

/// Resolve a classid to its OWN action manifest within a generated `registry`
/// (zero-fallback: an unregistered classid resolves to no actions, never a
/// panic — mirrors [`crate::codegen_manifest::methods_for`]).
#[must_use]
pub fn actions_for(registry: &[ClassActions], classid: u32) -> &'static [ActionDef] {
    registry
        .iter()
        .find(|entry| entry.classid == classid)
        .map_or(&[], |entry| entry.actions)
}

/// Compose a class's **effective** DO surface via OGAR inheritance: the
/// `parent` class's actions, then the `child`'s, with a child action of the
/// same `predicate` **overriding** the parent's. This is the `classid →
/// ClassView` inheritance applied to the action axis — a class inherits its
/// parents' actions and may supersede them, exactly as it inherits fields.
///
/// Returns an owned `Vec` (resolution, not `const`): parent-only actions first
/// (in order), each replaced in place if the child redefines its `predicate`,
/// then the child's net-new actions appended.
#[must_use]
pub fn effective_actions(parent: &[ActionDef], child: &[ActionDef]) -> Vec<ActionDef> {
    // Precondition: predicates are unique WITHIN each class's action table (the
    // harvest is the manifest — a duplicate predicate is a generator bug). The
    // merge below is also defensively dedup'd so a stray duplicate never
    // double-dispatches; the assert surfaces the generator bug in debug.
    debug_assert!(
        !has_duplicate_predicate(parent) && !has_duplicate_predicate(child),
        "effective_actions: each ActionDef slice must have unique predicates per class"
    );
    let mut out: Vec<ActionDef> = Vec::with_capacity(parent.len() + child.len());
    // Parent actions, each overridden by a same-predicate child if present.
    // Emit each predicate at most once (first occurrence within parent wins).
    for p in parent {
        if out.iter().any(|e| e.predicate == p.predicate) {
            continue; // duplicate predicate already emitted
        }
        match child.iter().find(|c| c.predicate == p.predicate) {
            Some(c) => out.push(*c),
            None => out.push(*p),
        }
    }
    // Child net-new actions (predicate not already emitted), deduped.
    for c in child {
        if out.iter().any(|e| e.predicate == c.predicate) {
            continue;
        }
        out.push(*c);
    }
    out
}

/// Whether `defs` contains two actions with the same `predicate` (a per-class
/// uniqueness violation — the harvest should never produce one).
fn has_duplicate_predicate(defs: &[ActionDef]) -> bool {
    defs.iter()
        .enumerate()
        .any(|(i, a)| defs[..i].iter().any(|b| b.predicate == a.predicate))
}

/// DO arm — **dynamic** invocation (one per fire). Carries the lifecycle, the
/// S2.5 SoA cycle stamp, and the dedup/provenance keys.
///
/// **`Clone`, NOT `Copy`** (deliberate): this is a one-shot lifecycle carrier
/// whose `commit` mutates `state`/`emitted_at_millis` in place. `Copy` would let
/// a caller commit a *copy* and silently lose the mutation on the original (and
/// mint duplicate-`idempotency_key` fires) — so duplication must be an explicit
/// `.clone()` at the call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionInvocation {
    /// The acted-upon class (OGAR classid) — with `predicate`, identifies the
    /// [`ActionDef`] this realizes. Equals `object_instance.classid()`.
    pub object_class: u32,
    /// The realized action's `has_function` name.
    pub predicate: &'static str,
    /// The specific instance acted on — the **full canonical [`NodeGuid`]**, not
    /// a bare identity tail. It carries the `classid`, HHTL prefix, `family`
    /// basin, and `identity`, so a consumer reconstructs the exact target with
    /// no truncation or basin ambiguity. (A bare `u32` tail would panic the
    /// 24-bit assert or alias two basins; the external-mutation egress must not
    /// drop the key.)
    pub object_instance: NodeGuid,
    /// Lifecycle state.
    pub state: ActionState,
    /// SoA cycle-ownership stamp (S2.5) — the mailbox `current_cycle` that
    /// precipitated this fire, so a dispatched action is tied to its cycle.
    pub cycle: u32,
    /// Idempotency key — dedups re-fires of the same logical action.
    pub idempotency_key: u64,
    /// Trace id for provenance.
    pub trace_id: u64,
    /// HLC emit stamp (set on commit); `None` while `Pending`.
    pub emitted_at_millis: Option<u64>,
}

impl ActionInvocation {
    /// Construct a fresh `Pending` invocation.
    ///
    /// `object_instance` is the full canonical [`NodeGuid`] of the target; its
    /// `classid()` must equal `object_class` (debug-asserted — the def-match
    /// keys on `object_class`, the address on the GUID, and they must agree).
    #[must_use]
    pub fn pending(
        object_class: u32,
        predicate: &'static str,
        object_instance: NodeGuid,
        cycle: u32,
        idempotency_key: u64,
        trace_id: u64,
    ) -> Self {
        debug_assert_eq!(
            object_instance.classid(),
            object_class,
            "object_instance GUID classid must match object_class"
        );
        Self {
            object_class,
            predicate,
            object_instance,
            state: ActionState::Pending,
            cycle,
            idempotency_key,
            trace_id,
            emitted_at_millis: None,
        }
    }

    /// Adjudicate a `Pending` action against its `ActionDef` (match), RBAC, the
    /// state guard, and the MUL impact gate, in that order. Returns the
    /// resulting [`ActionState`].
    ///
    /// `guard_field_value` is the current value of `def.guard.field` on the
    /// target instance, supplied by the caller (the Core holds no object state —
    /// `I-VSA-IDENTITIES`); `None` when unknown. It is consulted ONLY when
    /// `def.guard` is `Some`.
    ///
    /// Order and outcomes:
    /// - `def` does not identify THIS invocation (`object_class`/`predicate`
    ///   mismatch) → `Failed` — authorization is NEVER applied against an
    ///   unrelated definition's `required_role`.
    /// - unauthorized actor (lacks [`ActionDef::required_role`], not admin) → `Failed`
    /// - state guard present and unsatisfied (`guard_field_value != Some(value)`)
    ///   → `Cancelled` — the action is not eligible in the current state.
    /// - MUL `Flow`  → `Committed` (dispatched; `emitted_at_millis` stamped)
    /// - MUL `Hold`  → stays `Pending` (escalate / re-assess next cycle)
    /// - MUL `Block` → `Cancelled`
    ///
    /// Terminal states are sticky (a committed/failed/cancelled action is not
    /// re-adjudicated). The `def`-match is checked FIRST, before RBAC/guard/MUL.
    pub fn commit(
        &mut self,
        def: &ActionDef,
        actor: &ActorContext,
        impact: &GateDecision,
        guard_field_value: Option<&str>,
        now_millis: u64,
    ) -> ActionState {
        if self.state.is_terminal() {
            return self.state; // sticky
        }
        // The `def` MUST identify THIS invocation — otherwise RBAC/guard/MUL
        // would adjudicate against an unrelated action's policy (P1).
        if def.object_class != self.object_class || def.predicate != self.predicate {
            self.state = ActionState::Failed;
            return self.state;
        }
        // RBAC — authority before eligibility/impact.
        if let Some(role) = def.required_role {
            let authorized = actor.is_admin() || actor.roles.iter().any(|r| r == role);
            if !authorized {
                self.state = ActionState::Failed;
                return self.state;
            }
        }
        // State guard — fire only when `field == value` (P2). An unsatisfied (or
        // unknown) guarded state refuses the fire; a fresh invocation runs when
        // the instance re-enters the eligible state.
        if let Some(g) = def.guard {
            if guard_field_value != Some(g.value) {
                self.state = ActionState::Cancelled;
                return self.state;
            }
        }
        // MUL impact assessment.
        self.state = match impact {
            GateDecision::Flow => {
                self.emitted_at_millis = Some(now_millis);
                ActionState::Committed
            }
            GateDecision::Hold { .. } => ActionState::Pending,
            GateDecision::Block { .. } => ActionState::Cancelled,
        };
        self.state
    }
}

use crate::rbac::{ActorId, ClassRbac, Operation};

impl ActionInvocation {
    /// ClassRbac convergence of [`ActionInvocation::commit`]'s inline RBAC gate.
    ///
    /// Identical lifecycle order to `commit` (sticky-terminal → def-match → RBAC →
    /// state guard → MUL impact), but RBAC is resolved through the [`ClassRbac`]
    /// trait instead of an [`crate::auth::ActorContext`] value.
    ///
    /// **No `is_admin` bypass**: unlike `commit`, this method does NOT grant
    /// unconditional access to admins. An actor claiming administrative privilege
    /// must hold a role whose grant explicitly permits
    /// `Operation::Act { action: def.predicate }` on `def.object_class`. This is
    /// deliberate: the `ClassRbac` surface is the authoritative grant registry;
    /// out-of-band bypass is not modelled here.
    pub fn commit_via<R: ClassRbac>(
        &mut self,
        def: &ActionDef,
        rbac: &R,
        actor_id: ActorId<'_>,
        impact: &GateDecision,
        guard_field_value: Option<&str>,
        now_millis: u64,
    ) -> ActionState {
        // Sticky terminal — already adjudicated, not re-adjudicated.
        if self.state.is_terminal() {
            return self.state;
        }
        // Def-match: the def MUST identify THIS invocation before any policy check.
        // Applying an unrelated def's required_role would authorize via the wrong policy.
        if def.object_class != self.object_class || def.predicate != self.predicate {
            self.state = ActionState::Failed;
            return self.state;
        }
        // RBAC — resolve `ok` to bool BEFORE mutating `self.state` (borrow hygiene).
        if let Some(required_role) = def.required_role {
            let ok = rbac.actor_roles(actor_id).iter().any(|&r| {
                r == required_role
                    && rbac.grant_permits(
                        r,
                        def.object_class,
                        &Operation::Act {
                            action: def.predicate,
                        },
                    )
            });
            if !ok {
                self.state = ActionState::Failed;
                return self.state;
            }
        }
        // State guard — fire only when `field == value` (P2).
        // An unsatisfied or unknown guarded state refuses the fire (Cancelled, not Failed).
        if let Some(g) = def.guard {
            if guard_field_value != Some(g.value) {
                self.state = ActionState::Cancelled;
                return self.state;
            }
        }
        // MUL impact assessment — mirrors `commit` exactly.
        self.state = match impact {
            GateDecision::Flow => {
                self.emitted_at_millis = Some(now_millis);
                ActionState::Committed
            }
            GateDecision::Hold { .. } => ActionState::Pending,
            GateDecision::Block { .. } => ActionState::Cancelled,
        };
        self.state
    }
}

#[cfg(test)]
mod commit_via_tests {
    use super::*;

    // Minimal ClassRbac test double: a fixed grant table.
    struct TestRbac {
        // triples of (role, object_class, action_predicate) that are permitted
        grants: Vec<(&'static str, u32, &'static str)>,
        // pairs of actor_id -> roles
        actor_roles_map: Vec<(&'static str, Vec<&'static str>)>,
    }

    impl ClassRbac for TestRbac {
        fn actor_roles<'a>(&'a self, actor_id: ActorId<'_>) -> &'a [&'static str] {
            for (id, roles) in &self.actor_roles_map {
                if *id == actor_id {
                    return roles.as_slice();
                }
            }
            &[]
        }
        fn grant_permits(&self, role: &'static str, object_class: u32, op: &Operation) -> bool {
            match op {
                Operation::Act { action } => self
                    .grants
                    .iter()
                    .any(|&(r, c, a)| r == role && c == object_class && a == *action),
                _ => false,
            }
        }
    }

    fn inst(identity: u32) -> NodeGuid {
        NodeGuid::new(0x0A1E_0001, 0, 0, 0, 0, identity)
    }

    const DEF_WITH_ROLE: ActionDef = ActionDef {
        predicate: "action_confirm",
        object_class: 0x0A1E_0001,
        exec: ExecTarget::SurrealQl,
        guard: Some(StateGuard {
            field: "state",
            value: "draft",
        }),
        required_role: Some("sales_manager"),
        overrides: None,
    };

    const DEF_NO_ROLE: ActionDef = ActionDef {
        predicate: "action_cancel",
        object_class: 0x0A1E_0001,
        exec: ExecTarget::SurrealQl,
        guard: None,
        required_role: None,
        overrides: None,
    };

    fn rbac_granting(role: &'static str) -> TestRbac {
        TestRbac {
            grants: vec![(role, 0x0A1E_0001, "action_confirm")],
            actor_roles_map: vec![("u1", vec![role])],
        }
    }

    fn rbac_denying() -> TestRbac {
        TestRbac {
            grants: vec![("sales_manager", 0x0A1E_0001, "action_confirm")],
            actor_roles_map: vec![("u1", vec!["viewer"])],
        }
    }

    fn actor(id: &'static str) -> ActorId<'static> {
        id
    }

    /// Authorized actor (holds a role whose grant permits Act) + guard satisfied + Flow
    /// → Committed, emitted_at_millis stamped. Terminal sticky on re-call.
    #[test]
    fn commit_via_authorized_flow_commits() {
        let rbac = rbac_granting("sales_manager");
        let mut inv = ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(1), 5, 1, 1);
        let state = inv.commit_via(
            &DEF_WITH_ROLE,
            &rbac,
            actor("u1"),
            &GateDecision::Flow,
            Some("draft"),
            2000,
        );
        assert_eq!(state, ActionState::Committed);
        assert_eq!(inv.emitted_at_millis, Some(2000));
        // sticky: re-adjudication is a no-op even with Block
        let state2 = inv.commit_via(
            &DEF_WITH_ROLE,
            &rbac,
            actor("u1"),
            &GateDecision::Block {
                reason: "x".to_string(),
            },
            Some("draft"),
            3000,
        );
        assert_eq!(state2, ActionState::Committed);
        assert_eq!(
            inv.emitted_at_millis,
            Some(2000),
            "stamp must not be overwritten on sticky re-call"
        );
    }

    /// Ungranted actor (role present but grant does not permit Act) → Failed before guard/MUL.
    #[test]
    fn commit_via_ungranted_actor_fails() {
        let rbac = rbac_denying();
        let mut inv = ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(2), 5, 2, 2);
        let state = inv.commit_via(
            &DEF_WITH_ROLE,
            &rbac,
            actor("u1"),
            &GateDecision::Flow,
            Some("draft"),
            2000,
        );
        assert_eq!(state, ActionState::Failed);
        assert_eq!(inv.emitted_at_millis, None, "failed action must not emit");
    }

    /// required_role: None → proceeds regardless of rbac content (parity with commit).
    /// No role check means even an actor with no roles can proceed to MUL/guard.
    #[test]
    fn commit_via_no_required_role_proceeds_regardless_of_rbac() {
        let rbac = rbac_denying(); // rbac grants nothing useful for this actor
        let mut inv = ActionInvocation::pending(0x0A1E_0001, "action_cancel", inst(3), 5, 3, 3);
        // DEF_NO_ROLE has no guard and no required_role → reaches MUL directly
        let state = inv.commit_via(
            &DEF_NO_ROLE,
            &rbac,
            actor("u1"),
            &GateDecision::Flow,
            None,
            1000,
        );
        assert_eq!(
            state,
            ActionState::Committed,
            "no required_role means rbac is not consulted"
        );
    }

    /// Admin-as-role: an actor explicitly granted "admin" role whose grant permits Act passes.
    /// Bare `is_admin` does NOT bypass — there is no `ActorContext` here, no backdoor.
    #[test]
    fn commit_via_admin_must_be_granted_role_not_bypass() {
        // Actor "admin_user" is explicitly granted "admin" role with the required permission.
        let rbac = TestRbac {
            grants: vec![("admin", 0x0A1E_0001, "action_confirm")],
            actor_roles_map: vec![("admin_user", vec!["admin"])],
        };
        let admin_def = ActionDef {
            required_role: Some("admin"),
            ..DEF_WITH_ROLE
        };
        let mut inv = ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(4), 5, 4, 4);
        let state = inv.commit_via(
            &admin_def,
            &rbac,
            actor("admin_user"),
            &GateDecision::Flow,
            Some("draft"),
            1000,
        );
        assert_eq!(state, ActionState::Committed, "admin-as-granted-role works");

        // Actor whose id is literally "admin" but holds a role with no grant → fails (no bypass).
        let rbac_no_grants = TestRbac {
            grants: vec![],
            actor_roles_map: vec![("admin", vec!["admin"])],
        };
        let mut inv2 = ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(5), 5, 5, 5);
        let state2 = inv2.commit_via(
            &DEF_WITH_ROLE,
            &rbac_no_grants,
            actor("admin"),
            &GateDecision::Flow,
            Some("draft"),
            1000,
        );
        assert_eq!(
            state2,
            ActionState::Failed,
            "no grant → fails; bare is_admin does not bypass"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `const`-constructibility — the exact shape a generated
    /// `const ACTIONS: &[ActionDef]` emits (mirrors the `MethodSig` guard).
    const SALE_ORDER_ACTIONS: &[ActionDef] = &[
        ActionDef {
            predicate: "action_confirm",
            object_class: 0x0A1E_0001,
            exec: ExecTarget::SurrealQl,
            guard: Some(StateGuard {
                field: "state",
                value: "draft",
            }),
            required_role: Some("sales_manager"),
            overrides: None,
        },
        ActionDef {
            predicate: "action_cancel",
            object_class: 0x0A1E_0001,
            exec: ExecTarget::SurrealQl,
            guard: None,
            required_role: None,
            overrides: None,
        },
    ];

    const REGISTRY: &[ClassActions] = &[ClassActions {
        classid: 0x0A1E_0001,
        actions: SALE_ORDER_ACTIONS,
    }];

    #[test]
    fn const_action_manifest_constructs_and_resolves() {
        assert_eq!(actions_for(REGISTRY, 0x0A1E_0001).len(), 2);
        assert!(
            actions_for(REGISTRY, 0xDEAD).is_empty(),
            "unregistered classid → no actions (zero-fallback)"
        );
        assert_eq!(SALE_ORDER_ACTIONS[0].exec, ExecTarget::SurrealQl);
    }

    #[test]
    fn ogar_inheritance_child_overrides_parent_by_predicate() {
        let parent = &[
            ActionDef {
                predicate: "action_confirm",
                object_class: 0x01,
                exec: ExecTarget::Native,
                guard: None,
                required_role: None,
                overrides: None,
            },
            ActionDef {
                predicate: "message_post",
                object_class: 0x01,
                exec: ExecTarget::Native,
                guard: None,
                required_role: None,
                overrides: None,
            },
        ];
        let child = &[
            // overrides the parent's action_confirm
            ActionDef {
                predicate: "action_confirm",
                object_class: 0x02,
                exec: ExecTarget::SurrealQl,
                guard: None,
                required_role: Some("sales_manager"),
                overrides: Some("parent::action_confirm"),
            },
            // net-new
            ActionDef {
                predicate: "action_done",
                object_class: 0x02,
                exec: ExecTarget::SurrealQl,
                guard: None,
                required_role: None,
                overrides: None,
            },
        ];
        let eff = effective_actions(parent, child);
        assert_eq!(eff.len(), 3, "confirm(overridden) + message_post + done");
        let confirm = eff
            .iter()
            .find(|a| a.predicate == "action_confirm")
            .unwrap();
        assert_eq!(
            confirm.object_class, 0x02,
            "child's action_confirm overrides the parent's"
        );
        assert_eq!(confirm.required_role, Some("sales_manager"));
        // inherited unchanged
        assert!(eff
            .iter()
            .any(|a| a.predicate == "message_post" && a.object_class == 0x01));
        // net-new appended
        assert!(eff.iter().any(|a| a.predicate == "action_done"));
    }

    fn actor_with(roles: &[&str]) -> ActorContext {
        // TenantId = u64 (crate::sla); 0 = default tenant.
        ActorContext::new(
            "u1".to_string(),
            0,
            roles.iter().map(|s| s.to_string()).collect(),
        )
    }

    /// A target instance GUID whose classid matches the sale_order actions
    /// (0x0A1E_0001), default basin, `identity` discriminating.
    fn inst(identity: u32) -> NodeGuid {
        NodeGuid::new(0x0A1E_0001, 0, 0, 0, 0, identity)
    }

    #[test]
    fn commit_requires_rbac_then_mul_flow() {
        let def = &SALE_ORDER_ACTIONS[0]; // required_role = "sales_manager"

        // authorized + guard satisfied (state==draft) + Flow → Committed, stamped.
        let mut inv = ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(7), 3, 1, 1);
        assert_eq!(
            inv.commit(
                def,
                &actor_with(&["sales_manager"]),
                &GateDecision::Flow,
                Some("draft"),
                1000
            ),
            ActionState::Committed
        );
        assert_eq!(inv.emitted_at_millis, Some(1000));
        assert_eq!(inv.cycle, 3, "cycle stamp preserved");

        // committed is sticky — re-adjudication is a no-op.
        assert_eq!(
            inv.commit(
                def,
                &actor_with(&["sales_manager"]),
                &GateDecision::Block {
                    reason: "x".to_string()
                },
                Some("draft"),
                2000
            ),
            ActionState::Committed
        );
    }

    #[test]
    fn commit_unauthorized_fails_before_impact() {
        let def = &SALE_ORDER_ACTIONS[0];
        let mut inv = ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(7), 3, 1, 1);
        // wrong role, even with a Flow gate + satisfied guard → Failed (RBAC first).
        assert_eq!(
            inv.commit(
                def,
                &actor_with(&["viewer"]),
                &GateDecision::Flow,
                Some("draft"),
                1000
            ),
            ActionState::Failed
        );
        assert_eq!(inv.emitted_at_millis, None, "failed action never emitted");
    }

    #[test]
    fn mul_hold_keeps_pending_block_cancels() {
        let def = &SALE_ORDER_ACTIONS[1]; // no required_role
        let any = actor_with(&[]);

        // action_cancel has no guard → guard_field_value is ignored (None).
        let mut held = ActionInvocation::pending(0x0A1E_0001, "action_cancel", inst(7), 3, 1, 1);
        assert_eq!(
            held.commit(
                def,
                &any,
                &GateDecision::Hold {
                    reason: "low confidence".to_string()
                },
                None,
                1000
            ),
            ActionState::Pending,
            "Hold escalates — stays Pending for re-assessment"
        );

        let mut blocked = ActionInvocation::pending(0x0A1E_0001, "action_cancel", inst(7), 3, 1, 1);
        assert_eq!(
            blocked.commit(
                def,
                &any,
                &GateDecision::Block {
                    reason: "unsound impact".to_string()
                },
                None,
                1000
            ),
            ActionState::Cancelled
        );
    }

    /// P1 (codex #538): `commit` must reject a `def` that does not identify this
    /// invocation BEFORE applying RBAC — else passing the unguarded, no-role
    /// `action_cancel` def to an `action_confirm` invocation would reach
    /// `Committed` under `Flow` without the confirm role.
    #[test]
    fn commit_rejects_mismatched_def() {
        let confirm_inv_with_cancel_def = || {
            let mut inv =
                ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(7), 3, 1, 1);
            // SALE_ORDER_ACTIONS[1] is action_cancel (no required_role, no guard).
            let state = inv.commit(
                &SALE_ORDER_ACTIONS[1],
                &actor_with(&[]),
                &GateDecision::Flow,
                None,
                1000,
            );
            (inv, state)
        };
        let (inv, state) = confirm_inv_with_cancel_def();
        assert_eq!(
            state,
            ActionState::Failed,
            "a def whose predicate/object_class mismatch the invocation must NOT authorize"
        );
        assert_eq!(inv.emitted_at_millis, None, "mismatched def never emits");
    }

    /// P2 (codex #538): a guarded action (`state == draft`) must NOT commit when
    /// the target instance is in a non-eligible state, even with role + `Flow`.
    #[test]
    fn guarded_action_refused_in_wrong_state() {
        let def = &SALE_ORDER_ACTIONS[0]; // guard: state == "draft"
        let mut inv = ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(7), 3, 1, 1);
        let state = inv.commit(
            def,
            &actor_with(&["sales_manager"]),
            &GateDecision::Flow,
            Some("sent"), // wrong state
            1000,
        );
        assert_eq!(
            state,
            ActionState::Cancelled,
            "guarded action in a non-eligible state is refused, not committed"
        );
        assert_eq!(inv.emitted_at_millis, None, "refused action never emits");

        // unknown state (None) with a guard present is also refused.
        let mut inv2 = ActionInvocation::pending(0x0A1E_0001, "action_confirm", inst(7), 3, 1, 1);
        assert_eq!(
            inv2.commit(
                def,
                &actor_with(&["sales_manager"]),
                &GateDecision::Flow,
                None,
                1000
            ),
            ActionState::Cancelled,
            "unknown guarded state is refused"
        );
    }
}
