//! `op-codegen-bucket` — OpenProject HandlerKind enum + `RouteBucketTyped`
//! bridge into [`lance_graph_contract::codegen_spine`].
//!
//! # Why this crate exists
//!
//! Sprint C5 mapping (`.claude/sprints/c5-combo/map/M1-codegen-spine.md`) named
//! R1 as the hard block in the upstream spine: `RouteBucket::kind()` returned
//! `OdooMethodKind` by value — concrete, not generic. Non-Odoo codegen targets
//! (OpenProject, Wikidata, future) had no additive way to bring their own
//! handler-kind taxonomy through the spine.
//!
//! Sprint C6 closed R1 by adding `RouteBucketTyped<Kind>` next to the existing
//! `RouteBucket`, with a back-compat blanket impl
//! (`impl<T: RouteBucket + ?Sized> RouteBucketTyped for T`). The `?Sized` bound
//! was tightened in the PR #8 P2 follow-up so `&dyn RouteBucket` also flows
//! through the bridge.
//!
//! This crate is the **first concrete consumer** of those changes:
//!
//! - [`OpHandlerKind`] enumerates the six sqlx-emittable OpenProject handler
//!   kinds from Sprints C1–C3 (`list_for_tenant`, `detail_for_tenant`,
//!   `soft_delete`, `toggle_bool_field`, `ajax_json`,
//!   `csrf_form_post_engine_call`). It is **not** a subset of
//!   `OdooMethodKind`; it is a parallel taxonomy.
//! - [`OpBucket`] holds a `kind` + `id` and impls
//!   [`RouteBucketTyped<Kind = OpHandlerKind>`][rbt] but **not** the legacy
//!   [`RouteBucket`] trait. Per the C6 coherence rule, a type that wants
//!   `RouteBucketTyped` with a non-Odoo `Kind` must not also impl
//!   `RouteBucket`. OP buckets are OP-only.
//!
//! Tests prove (a) OP buckets work in generic consumers parameterised on
//! `Kind = OpHandlerKind`, AND (b) the C6 blanket impl bridge still carries
//! a sized Odoo `RouteBucket` impl AND an erased `&dyn RouteBucket` into the
//! same generic consumer — both shapes validated end-to-end.
//!
//! [rbt]: lance_graph_contract::codegen_spine::RouteBucketTyped

use lance_graph_contract::codegen_spine::RouteBucketTyped;

/// The six sqlx-emittable OpenProject handler kinds from Sprints C1–C3.
///
/// The naming mirrors `ruff_python_dto_check::contract::HandlerKind` (the
/// existing classification taxonomy used by the seaorm and sqlx emitters in
/// `AdaWorldAPI/ruff`). When a future Sprint introduces a 7th sqlx kind (e.g.
/// `form_get_post`, `signed_link_action`), add a variant here — pure
/// extension; downstream `match` arms that handle the existing 6 keep
/// working (no `#[non_exhaustive]` because consumers are in-repo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpHandlerKind {
    /// `GET /…/<scope>/things` — tenant-scoped list with pagination.
    ListForTenant,
    /// `GET /…/<scope>/things/:id` — scoped find-by-id.
    DetailForTenant,
    /// `POST /…/things/:id/archive` — UPDATE … SET active = false; 204.
    SoftDelete,
    /// `POST /…/things/:id/toggle` — UPDATE … SET active = NOT active;
    /// returns the updated row.
    ToggleBoolField,
    /// JSON-return handler with HAL envelope (with-model or stub branch).
    AjaxJson,
    /// `POST /…/things` — Json<Form> → INSERT … RETURNING; 201.
    CsrfFormPostEngineCall,
}

impl OpHandlerKind {
    /// Stable snake_case identifier — matches the strings used in the
    /// `emit_kinds` lists in `target-spec/openproject-axum-sqlx.toml`.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::ListForTenant => "list_for_tenant",
            Self::DetailForTenant => "detail_for_tenant",
            Self::SoftDelete => "soft_delete",
            Self::ToggleBoolField => "toggle_bool_field",
            Self::AjaxJson => "ajax_json",
            Self::CsrfFormPostEngineCall => "csrf_form_post_engine_call",
        }
    }

    /// All six variants, in declaration order. Useful for exhaustive
    /// dispatcher tables.
    pub const ALL: [OpHandlerKind; 6] = [
        Self::ListForTenant,
        Self::DetailForTenant,
        Self::SoftDelete,
        Self::ToggleBoolField,
        Self::AjaxJson,
        Self::CsrfFormPostEngineCall,
    ];
}

/// One OpenProject route's bucket assignment: which [`OpHandlerKind`] it is +
/// how it identifies itself.
///
/// Impls only [`RouteBucketTyped<Kind = OpHandlerKind>`][rbt]; deliberately
/// does NOT impl the legacy `lance_graph_contract::codegen_spine::RouteBucket`
/// trait (which would force the kind into `OdooMethodKind`).
///
/// [rbt]: lance_graph_contract::codegen_spine::RouteBucketTyped
#[derive(Debug, Clone)]
pub struct OpBucket {
    /// The OpenProject handler-kind this route falls under.
    pub kind: OpHandlerKind,
    /// Stable identity (e.g. `projects.list_work_packages`).
    pub id: String,
}

impl OpBucket {
    /// Construct an `OpBucket` from a kind + a string-like id.
    pub fn new(kind: OpHandlerKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}

impl RouteBucketTyped for OpBucket {
    type Kind = OpHandlerKind;

    fn kind(&self) -> OpHandlerKind {
        self.kind
    }

    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::codegen_spine::{OdooMethodKind, RouteBucket};

    // ----- OpHandlerKind itself -----

    #[test]
    fn op_kinds_have_stable_ids() {
        // Stable ids match the strings in target-spec/openproject-axum-sqlx.toml
        // (the emit_kinds array). Drift here would silently desync the spec.
        let expected = [
            "list_for_tenant",
            "detail_for_tenant",
            "soft_delete",
            "toggle_bool_field",
            "ajax_json",
            "csrf_form_post_engine_call",
        ];
        let actual: Vec<&'static str> = OpHandlerKind::ALL.iter().map(|k| k.id()).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn op_kinds_are_distinct() {
        // Sanity: no two variants share an id.
        let ids: Vec<_> = OpHandlerKind::ALL.iter().map(|k| k.id()).collect();
        let mut dedup = ids.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(ids.len(), dedup.len());
    }

    // ----- OpBucket through RouteBucketTyped -----

    #[test]
    fn op_bucket_speaks_route_bucket_typed_with_op_kind() {
        let b = OpBucket::new(OpHandlerKind::ListForTenant, "projects.list_work_packages");
        assert_eq!(b.kind(), OpHandlerKind::ListForTenant);
        assert_eq!(<OpBucket as RouteBucketTyped>::id(&b), "projects.list_work_packages");
        assert_eq!(b.id_owned(), "projects.list_work_packages");
    }

    /// A generic consumer that takes any `RouteBucketTyped` whose `Kind` is
    /// `OpHandlerKind` — this is the call shape the C6 design unlocks.
    fn op_dispatch<B: RouteBucketTyped<Kind = OpHandlerKind> + ?Sized>(b: &B) -> String {
        format!("{} -> {}", b.id(), b.kind().id())
    }

    #[test]
    fn generic_op_consumer_accepts_op_bucket() {
        let b = OpBucket::new(OpHandlerKind::CsrfFormPostEngineCall, "projects.create_work_package");
        assert_eq!(
            op_dispatch(&b),
            "projects.create_work_package -> csrf_form_post_engine_call"
        );
    }

    #[test]
    fn op_dispatch_accepts_dyn_op_bucket() {
        // ?Sized bound on op_dispatch + the C6/P2 ?Sized blanket on
        // RouteBucketTyped means a trait object also flows through.
        let b = OpBucket::new(OpHandlerKind::ToggleBoolField, "projects.toggle_active");
        let erased: &dyn RouteBucketTyped<Kind = OpHandlerKind> = &b;
        assert_eq!(erased.kind(), OpHandlerKind::ToggleBoolField);
        assert_eq!(erased.id(), "projects.toggle_active");
    }

    // ----- C6 back-compat bridge: legacy Odoo RouteBucket still flows -----

    /// A minimal Odoo bucket that impls only the legacy `RouteBucket`. Proves
    /// the C6 blanket impl carries it into the new typed surface.
    struct LegacyOdooBucket {
        kind: OdooMethodKind,
        id: &'static str,
    }
    impl RouteBucket for LegacyOdooBucket {
        fn kind(&self) -> OdooMethodKind {
            self.kind
        }
        fn id(&self) -> &str {
            self.id
        }
    }

    /// A generic consumer parameterised on `OdooMethodKind` — the
    /// pre-C6 RouteBucket-style consumer shape, now expressed through the
    /// new trait via the blanket impl.
    fn odoo_dispatch<B: RouteBucketTyped<Kind = OdooMethodKind> + ?Sized>(b: &B) -> String {
        format!("{} -> {}", b.id(), b.kind().id())
    }

    #[test]
    fn legacy_odoo_bucket_flows_through_c6_blanket() {
        let b = LegacyOdooBucket {
            kind: OdooMethodKind::IterRecordsComputeFromRelated,
            id: "account.move._compute_amount",
        };
        assert_eq!(
            odoo_dispatch(&b),
            "account.move._compute_amount -> iter_records_compute_from_related"
        );
    }

    #[test]
    fn pr8_p2_fix_erased_legacy_odoo_bucket_flows_through_blanket() {
        // Codex PR #8 P2 regression coverage at the consumer-crate level:
        // `&dyn RouteBucket` (the documented erased shape) must reach the
        // OpenProject side too, not just upstream's own tests.
        let b = LegacyOdooBucket {
            kind: OdooMethodKind::PassOverride,
            id: "account.move._compute_amount",
        };
        let erased: &dyn RouteBucket = &b;
        assert_eq!(
            odoo_dispatch(erased),
            "account.move._compute_amount -> pass_override"
        );
    }
}
