//! `op-codegen-residual` — the three-buckets doctrine as typed data, plus
//! the B1 canonicalize blade.
//!
//! Machine-readable counterpart of
//! `.claude/knowledge/RESIDUAL-THREE-BUCKETS.md`: the ~28% of the AR surface
//! the ruff extraction cannot determine (emitted today as `TYPE
//! option<any>`) is **not one population** — it splits into three buckets,
//! each with its own handler:
//!
//! - **B1 fuzzy-shaped** — emits X reliably but the arrangement drifts
//!   run-to-run. Handler: the deterministic normalizer in this crate
//!   ([`canonicalize`]); once canonicalized the value is determined. The
//!   arbiter for membership is the round-trip-order-free parity check
//!   ([`order_free_eq`]) — RAILS-COVERAGE-KIT §6's "random orders is the
//!   gate, not noise".
//! - **B2 anticipated-standard DO** — recurring cross-domain objects
//!   (ACL/permission sets, locale, audit/revision chains, document links).
//!   Handler: an ontological **landing zone** — one DTO adapter per
//!   [`LandingZone`], labelled against the OGAR/OGIT ontology per the
//!   canonical-label doctrine (concept id is the truth, surface string is a
//!   `LabelDto` skin). The swiss-knife verbs (`open`/`filter`/`reorder`/
//!   `apply mask`) are already shipped in the stack; adapters only map into
//!   them.
//! - **B3 irreducibly random** — genuinely bespoke logic. Handler: manual
//!   rewrite that **mints a new standard interface** ([`InterfaceMint`]) —
//!   which is how B3 feeds future B2 zones and the residual ratchets down
//!   monotonically across consumers (OpenProject's mints become Redmine's
//!   landing zones).
//!
//! # Why this crate has no dependencies
//!
//! `op-codegen-projection` (the intended consumer) git-deps on
//! `AdaWorldAPI/OGAR`, which some session network scopes cannot reach
//! (RESIDUAL-THREE-BUCKETS.md §4). The doctrine data and the B1 blade are
//! pure; keeping them dependency-free means the manifest stays buildable and
//! testable everywhere, and the projection picks it up with a plain path
//! dep when it next builds.
//!
//! # Provenance of the manifest
//!
//! [`RESIDUAL_MANIFEST`] transcribes the measured 2026-07-01 run of the C9
//! pipeline over the real OpenProject Rails source (`extract_core_triples`,
//! 18 curated `CORE_V3_RESOURCES`): every `TYPE option<any>` field the
//! projection emitted, bucketed. When the pipeline is re-runnable (OGAR
//! vendored) and the C12 type inference has moved rows to determined,
//! re-measure and prune — rows only ever *leave* this manifest; the buckets
//! and blades stay.

/// Which of the three residual buckets a field lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    /// Fuzzy-shaped: right content, unstable arrangement. Normalize with
    /// [`canonicalize`]; membership is gated by [`order_free_eq`] parity.
    B1Fuzzy,
    /// Anticipated-standard domain object: lands on a [`LandingZone`]
    /// adapter written once per zone.
    B2Landing,
    /// Irreducibly random: manual rewrite that mints an [`InterfaceMint`].
    B3Manual,
}

/// The B2 ontological landing zones — one DTO adapter each. Names are
/// ontology *families*; the content-addressable concept ids are minted
/// OGAR-side (RESERVE-DON'T-RECLAIM), and this enum carries only the
/// repo-local handle.
///
/// This enum is the repo-local **zone registry**. The first four zones
/// carry rows in [`RESIDUAL_MANIFEST`] (core-18 residual fields); the last
/// three were established by the 2026-07-01 full-surface survey
/// (RESIDUAL-THREE-BUCKETS.md §4) — their members are whole *models*
/// (`app/models` + `modules/*/app/models`), which enter the manifest when
/// the pipeline widens to `extract_app_with`. Registering them now pins the
/// zone names before adapters exist (append-only, like concept ids).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LandingZone {
    /// Authorization / permission sets (OGIT-auth family):
    /// `allowed_actions`, `allowed_permissions`, and normalized value sets.
    /// Consumed by the `apply mask` verb. Survey: 12 members
    /// (`Role`+STI subtree, `RolePermission`, `Member`/`MemberRole`,
    /// `Capability`, …).
    Acl,
    /// Locale / timezone / user-preference surface. Survey: 3 members —
    /// `UserPreference`'s serialized settings hash is schema-driven
    /// (`UserPreferences::Schema`), i.e. the DTO adapter has a ready spec.
    Locale,
    /// Audit / revision chain (temporal linked list):
    /// `Journal.predecessor` / `Journal.successor`. Survey: 17 members
    /// (the `Journal::*Journal` per-entity family, `Changeset`/`Change`,
    /// PaperTrail audits).
    AuditChain,
    /// Cross-domain document reference: `Version.wiki_page`. Survey: 10
    /// members (`Attachment`, wiki family, `Storages::FileLink`).
    DocLink,
    /// Session / token / auth-credential family — the operator's original
    /// B2 example ("OGIT auth emits the typical auth, session etc").
    /// Survey: 17 members; `Token::Base`/`HashedToken`/`ExpirableToken` is
    /// already a clean STI framework, effectively a ready-made DTO spec.
    /// No manifest rows yet (models live outside the core-18 field set).
    Session,
    /// Subscription / watch family: polymorphic subject + subscribing
    /// principal + reason/channel. Survey: 7 members (`Watcher`,
    /// `Notification`, `NotificationSetting`, `Favorite`, …). No manifest
    /// rows yet.
    Subscription,
    /// External-directory group sync record ("external group ↔ local
    /// user"), distinct from [`LandingZone::Acl`]'s `Member`/`MemberRole`
    /// (no role/permission payload). Survey: 3 members (LDAP groups/
    /// departments, OIDC group membership). No manifest rows yet.
    ///
    /// Odoo probe 2026-07-02: did NOT recur as a standalone shape (Odoo
    /// LDAP is config + template-clone provisioning). Likely a
    /// *specialization* of [`LandingZone::ExternalRef`]; kept per
    /// RESERVE-DON'T-RECLAIM — new adapters should target `ExternalRef`.
    GroupMembership,
    /// Polymorphic to-do: subject + assignee + deadline + state machine.
    /// Established by the Odoo probe (`mail.activity`, mixin-layered onto
    /// most business objects) with OP-side members `Reminder` /
    /// `ReminderNotification` — two independent lineages. No manifest rows
    /// yet.
    ScheduledReminder,
    /// External-identifier ↔ local-record sync registry. Odoo:
    /// `ir.model.data` (`(module, name)` key → polymorphic target,
    /// `noupdate`). OP: `RemoteIdentity`, `Storages::FileLink.origin_*`,
    /// and the directory-membership sync records (see
    /// [`LandingZone::GroupMembership`]). No manifest rows yet.
    ExternalRef,
}

impl LandingZone {
    /// Every registered zone, in registration order (append-only — new
    /// zones go at the end, existing positions never change, mirroring
    /// RESERVE-DON'T-RECLAIM for the OGAR-side concept ids these map to).
    ///
    /// Adapter note (Odoo probe meta-find): five-plus of these zones sit on
    /// one shared substrate — the polymorphic reference
    /// (`{target_class, target_id}`; Rails `belongs_to polymorphic:`, Odoo
    /// `Many2oneReference`). Build the PolyRef DTO once; zone adapters
    /// compose it with zone-specific payload.
    pub const REGISTRY: [LandingZone; 9] = [
        LandingZone::Acl,
        LandingZone::Locale,
        LandingZone::AuditChain,
        LandingZone::DocLink,
        LandingZone::Session,
        LandingZone::Subscription,
        LandingZone::GroupMembership,
        LandingZone::ScheduledReminder,
        LandingZone::ExternalRef,
    ];
}

/// The B3 interface mints the core residual requires. Each is a *candidate*
/// future landing zone: once proven on a second consumer (Redmine), it
/// graduates to B2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InterfaceMint {
    /// OpenProject's bespoke progress derivation
    /// (`WorkPackage.derived_progress_hints`, `Version.issues_progress`).
    /// Redmine's `done_ratio` is the convergence target.
    ProgressDerivation,
    /// Version-level rollups over descendant work packages / time entries
    /// (`estimated_hours`, `estimated_average`, `spent_hours`). Pairs with
    /// the `BILLABLE_WORK_ENTRY` (0x0103) class convergence.
    VersionRollup,
}

/// One residual field: where it came from, which bucket, and its handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidualEntry {
    /// AR model name as extracted (`ruff_ruby_spo` surface).
    pub model: &'static str,
    /// Field name as emitted by the projection.
    pub field: &'static str,
    /// Primary bucket (the first gate that fires; see [`bucket gates`](self)).
    pub bucket: Bucket,
    /// B2 landing zone. Also set on B1 rows that *chain* into a zone after
    /// normalization (normalize first, land second — gate 1 before gate 2).
    pub zone: Option<LandingZone>,
    /// B3 interface mint. Set only on `B3Manual` rows.
    pub mint: Option<InterfaceMint>,
}

/// The measured CORE_V3_RESOURCES residual, bucketed. Transcribed from the
/// 2026-07-01 pipeline run (see crate docs for provenance and re-measure
/// policy). Ordering is (model, field) — keep it sorted; the tests pin it.
pub const RESIDUAL_MANIFEST: &[ResidualEntry] = &[
    ResidualEntry {
        model: "Journal",
        field: "predecessor",
        bucket: Bucket::B2Landing,
        zone: Some(LandingZone::AuditChain),
        mint: None,
    },
    ResidualEntry {
        model: "Journal",
        field: "successor",
        bucket: Bucket::B2Landing,
        zone: Some(LandingZone::AuditChain),
        mint: None,
    },
    ResidualEntry {
        model: "Project",
        field: "allowed_actions",
        bucket: Bucket::B2Landing,
        zone: Some(LandingZone::Acl),
        mint: None,
    },
    ResidualEntry {
        model: "Project",
        field: "allowed_permissions",
        bucket: Bucket::B2Landing,
        zone: Some(LandingZone::Acl),
        mint: None,
    },
    ResidualEntry {
        model: "Query",
        field: "available_columns",
        bucket: Bucket::B1Fuzzy,
        zone: None,
        mint: None,
    },
    ResidualEntry {
        model: "Query",
        field: "available_columns_project",
        bucket: Bucket::B1Fuzzy,
        zone: None,
        mint: None,
    },
    ResidualEntry {
        model: "Query",
        field: "for_all",
        bucket: Bucket::B1Fuzzy,
        zone: None,
        mint: None,
    },
    ResidualEntry {
        model: "Role",
        field: "allowed_actions",
        bucket: Bucket::B2Landing,
        zone: Some(LandingZone::Acl),
        mint: None,
    },
    ResidualEntry {
        model: "Type",
        field: "pdf_export_templates",
        bucket: Bucket::B1Fuzzy,
        zone: None,
        mint: None,
    },
    ResidualEntry {
        model: "User",
        field: "allowed_values",
        bucket: Bucket::B1Fuzzy,
        zone: Some(LandingZone::Acl),
        mint: None,
    },
    ResidualEntry {
        model: "User",
        field: "time_zone",
        bucket: Bucket::B2Landing,
        zone: Some(LandingZone::Locale),
        mint: None,
    },
    ResidualEntry {
        model: "Version",
        field: "closed_issues_count",
        bucket: Bucket::B1Fuzzy,
        zone: None,
        mint: None,
    },
    ResidualEntry {
        model: "Version",
        field: "estimated_average",
        bucket: Bucket::B3Manual,
        zone: None,
        mint: Some(InterfaceMint::VersionRollup),
    },
    ResidualEntry {
        model: "Version",
        field: "estimated_hours",
        bucket: Bucket::B3Manual,
        zone: None,
        mint: Some(InterfaceMint::VersionRollup),
    },
    ResidualEntry {
        model: "Version",
        field: "issue_count",
        bucket: Bucket::B1Fuzzy,
        zone: None,
        mint: None,
    },
    ResidualEntry {
        model: "Version",
        field: "issues_progress",
        bucket: Bucket::B3Manual,
        zone: None,
        mint: Some(InterfaceMint::ProgressDerivation),
    },
    ResidualEntry {
        model: "Version",
        field: "open_issues_count",
        bucket: Bucket::B1Fuzzy,
        zone: None,
        mint: None,
    },
    ResidualEntry {
        model: "Version",
        field: "spent_hours",
        bucket: Bucket::B3Manual,
        zone: None,
        mint: Some(InterfaceMint::VersionRollup),
    },
    ResidualEntry {
        model: "Version",
        field: "wiki_page",
        bucket: Bucket::B2Landing,
        zone: Some(LandingZone::DocLink),
        mint: None,
    },
    ResidualEntry {
        model: "WorkPackage",
        field: "assignable_versions",
        bucket: Bucket::B1Fuzzy,
        zone: None,
        mint: None,
    },
    ResidualEntry {
        model: "WorkPackage",
        field: "derived_progress_hints",
        bucket: Bucket::B3Manual,
        zone: None,
        mint: Some(InterfaceMint::ProgressDerivation),
    },
];

/// Look up a residual entry by `(model, field)`. `None` means the field is
/// not residual — i.e. the extraction determines it (the ~72%).
#[must_use]
pub fn lookup(model: &str, field: &str) -> Option<&'static ResidualEntry> {
    RESIDUAL_MANIFEST
        .iter()
        .find(|e| e.model == model && e.field == field)
}

/// The B1 blade: canonicalize a fuzzy-arranged value set in place —
/// deterministic stable order + dedup. After this, two extractions that
/// emitted the same *content* in different arrangements compare equal, and
/// the value is determined (B0).
///
/// Sort is lexicographic on the surface string; that is deliberate — the
/// canonical arrangement only has to be *stable across runs*, not
/// semantically meaningful. Consumers that need a domain order apply it as
/// a render-side `reorder`, never at the canonical layer.
pub fn canonicalize(values: &mut Vec<String>) {
    values.sort_unstable();
    values.dedup();
}

/// The B1 membership gate: round-trip-order-free parity. Two emissions are
/// order-free-equal iff their canonical forms coincide. `true` → the
/// arrangement drift was incidental (ops commute) → the field is B1 and
/// [`canonicalize`] recovers it. `false` → the order carries meaning → the
/// field was never B1; it escalates to B3 (PRESERVE + RFC, never silently
/// "fix" — RAILS-COVERAGE-KIT §6).
#[must_use]
pub fn order_free_eq(a: &[String], b: &[String]) -> bool {
    let mut ca = a.to_vec();
    let mut cb = b.to_vec();
    canonicalize(&mut ca);
    canonicalize(&mut cb);
    ca == cb
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Composition pinned to the doctrine doc §2: ~9 B1-touched, 8 B2-landed
    /// (7 primary + 1 chained), 5 B3 — 21 rows total.
    #[test]
    fn manifest_composition_matches_doctrine() {
        assert_eq!(RESIDUAL_MANIFEST.len(), 21);
        let b1 = RESIDUAL_MANIFEST
            .iter()
            .filter(|e| e.bucket == Bucket::B1Fuzzy)
            .count();
        let b2 = RESIDUAL_MANIFEST
            .iter()
            .filter(|e| e.bucket == Bucket::B2Landing)
            .count();
        let b3 = RESIDUAL_MANIFEST
            .iter()
            .filter(|e| e.bucket == Bucket::B3Manual)
            .count();
        assert_eq!((b1, b2, b3), (9, 7, 5));
        // Landed rows (primary B2 + B1→B2 chained) = 8.
        let landed = RESIDUAL_MANIFEST
            .iter()
            .filter(|e| e.zone.is_some())
            .count();
        assert_eq!(landed, 8);
    }

    /// The whole B2 surface sits behind exactly four adapters, and the B3
    /// surface behind exactly two mints — the amortization claim.
    #[test]
    fn four_adapters_two_mints() {
        let mut zones: Vec<LandingZone> = RESIDUAL_MANIFEST.iter().filter_map(|e| e.zone).collect();
        zones.sort_by_key(|z| format!("{z:?}"));
        zones.dedup();
        assert_eq!(zones.len(), 4);

        let mut mints: Vec<InterfaceMint> =
            RESIDUAL_MANIFEST.iter().filter_map(|e| e.mint).collect();
        mints.sort_by_key(|m| format!("{m:?}"));
        mints.dedup();
        assert_eq!(mints.len(), 2);
    }

    /// Every zone a manifest row lands on is registered, and the registry
    /// carries the three survey-established zones ahead of their manifest
    /// rows (they arrive with the `extract_app_with` widening).
    #[test]
    fn manifest_zones_are_registered() {
        assert_eq!(LandingZone::REGISTRY.len(), 9);
        for e in RESIDUAL_MANIFEST {
            if let Some(z) = e.zone {
                assert!(
                    LandingZone::REGISTRY.contains(&z),
                    "{}.{} lands on unregistered zone {z:?}",
                    e.model,
                    e.field
                );
            }
        }
        for z in [
            LandingZone::Session,
            LandingZone::Subscription,
            LandingZone::GroupMembership,
            LandingZone::ScheduledReminder,
            LandingZone::ExternalRef,
        ] {
            assert!(
                !RESIDUAL_MANIFEST.iter().any(|e| e.zone == Some(z)),
                "{z:?} unexpectedly gained manifest rows — update this test and the doc"
            );
        }
    }

    /// Structural invariants: mints only on B3 rows; every B3 row mints;
    /// primary-B2 rows always land; no duplicate (model, field) keys; and
    /// the manifest stays (model, field)-sorted so diffs are stable.
    #[test]
    fn manifest_invariants() {
        for e in RESIDUAL_MANIFEST {
            match e.bucket {
                Bucket::B3Manual => {
                    assert!(
                        e.mint.is_some(),
                        "{}.{} B3 without a mint",
                        e.model,
                        e.field
                    );
                    assert!(
                        e.zone.is_none(),
                        "{}.{} B3 rows do not land",
                        e.model,
                        e.field
                    );
                }
                Bucket::B2Landing => {
                    assert!(
                        e.zone.is_some(),
                        "{}.{} B2 without a zone",
                        e.model,
                        e.field
                    );
                    assert!(
                        e.mint.is_none(),
                        "{}.{} B2 rows do not mint",
                        e.model,
                        e.field
                    );
                }
                Bucket::B1Fuzzy => {
                    assert!(
                        e.mint.is_none(),
                        "{}.{} B1 rows do not mint",
                        e.model,
                        e.field
                    );
                }
            }
        }
        let keys: Vec<(&str, &str)> = RESIDUAL_MANIFEST
            .iter()
            .map(|e| (e.model, e.field))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            keys, sorted,
            "manifest must be (model, field)-sorted, no dups"
        );
    }

    #[test]
    fn lookup_hits_and_misses() {
        let e = lookup("WorkPackage", "derived_progress_hints").expect("residual row");
        assert_eq!(e.bucket, Bucket::B3Manual);
        assert_eq!(e.mint, Some(InterfaceMint::ProgressDerivation));
        // A determined field (the ~72%) is not in the manifest.
        assert!(lookup("WorkPackage", "subject").is_none());
        // The B1→B2 chain: normalize first, land second.
        let av = lookup("User", "allowed_values").expect("residual row");
        assert_eq!(av.bucket, Bucket::B1Fuzzy);
        assert_eq!(av.zone, Some(LandingZone::Acl));
    }

    /// The blade is idempotent and permutation-invariant — the property that
    /// makes a B1 value determined after one pass.
    #[test]
    fn canonicalize_is_deterministic_across_arrangements() {
        let mut a = vec!["view".to_string(), "edit".to_string(), "view".to_string()];
        let mut b = vec!["edit".to_string(), "view".to_string()];
        canonicalize(&mut a);
        canonicalize(&mut b);
        assert_eq!(a, b);
        let once = a.clone();
        canonicalize(&mut a);
        assert_eq!(a, once, "idempotent");
    }

    /// The gate: incidental order passes (→ B1), meaningful difference fails
    /// (→ B3 escalation).
    #[test]
    fn order_free_parity_gates_b1_membership() {
        let run1 = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let run2 = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        assert!(order_free_eq(&run1, &run2), "commuting arrangement → B1");
        let run3 = vec!["a".to_string(), "b".to_string()];
        assert!(
            !order_free_eq(&run1, &run3),
            "content differs → not B1 drift"
        );
    }
}
