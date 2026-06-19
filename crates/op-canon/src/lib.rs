//! `op-canon` — the **canonical contract** for the OpenProject → Rust port.
//!
//! Carries the OGAR *canonical extraction snapshot* of the OpenProject
//! source corpus (`AdaWorldAPI/openproject`, walked end-to-end including
//! `modules/*/app/models`) and exposes it as typed Rust. It is the spine
//! of `openproject-nexgen-rs`'s canonical layer: every domain crate
//! (`op-work-packages`, `op-users`, `op-projects`, …) is free to use its
//! curator name on the Rails-facing side, but the **codebook id** is what
//! makes it the same node a Redmine consumer sees.
//!
//! # The symmetric move
//!
//! `op-canon` is the OpenProject-side sibling of
//! [`AdaWorldAPI/redmine-rs`'s `redmine-canon`](https://github.com/AdaWorldAPI/redmine-rs).
//! Both `-rs` ports vendor the **same OGAR codebook ids** minted in
//! [`AdaWorldAPI/OGAR`](https://github.com/AdaWorldAPI/OGAR), so a node
//! typed `project_work_item` (`0x0102`) is the same identity whether it
//! came from OpenProject's `WorkPackage` or Redmine's `Issue`. The
//! fork-lineage convergence (Redmine → ChiliProject → OpenProject) is
//! made operational here: 26 of 26 canonical concepts the Redmine corpus
//! contributes are *also* contributed by OpenProject, with identical ids.
//!
//! > "Rails words die, the invariant lives."
//!
//! # Why a snapshot, not a live extraction
//!
//! The mapping (OpenProject class → canonical concept → `u16` codebook id)
//! is produced *upstream*, in `AdaWorldAPI/OGAR`, by the producer pipeline:
//!
//! ```text
//!   AdaWorldAPI/openproject  (Ruby/Rails, core app/models + modules/*)
//!         │  ruff_ruby_spo::extract_app_with(path, "openproject")
//!         ▼
//!   ruff_spo_triplet::ModelGraph
//!         │  ogar_from_ruff::lift_model_graph   (domain-gated)
//!         ▼
//!   Vec<ogar_vocab::Class>  ──  canonical_concept + canonical_id (CODEBOOK)
//!         │  snapshot dump
//!         ▼
//!   crates/op-canon/data/op.ogar.json   (this crate vendors it)
//! ```
//!
//! Vendoring the snapshot keeps this crate's tests **self-contained** — CI
//! needs no Ruby corpus and no network — while the snapshot stays the
//! single source of truth a regeneration run overwrites.
//!
//! # The codebook is domain-encoded (`0xDDCC`)
//!
//! Each canonical concept owns a stable `u16` id whose **high byte is its
//! domain**. OpenProject is a project-management curator, so every id here
//! lives in the `0x01` (project-mgmt) block. A consumer holding several
//! domains routes on `id >> 8` with no table lookup — see
//! [`Concept::domain_high_byte`]. Ids serialise as 2 little-endian bytes
//! (`class_id_le`), wire-compatible with the OGAR `NodeGuid` layout.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod class_ids;

use serde::Deserialize;

/// The raw vendored snapshot JSON (the single source of truth).
pub const SNAPSHOT_JSON: &str = include_str!("../data/op.ogar.json");

/// Provenance of a [`Snapshot`] — where the extraction came from and how.
#[derive(Debug, Clone, Deserialize)]
pub struct Provenance {
    /// The specific curator product (`"openproject"`).
    pub source_curator: String,
    /// The repository the corpus was harvested from.
    pub source_repo: String,
    /// The coarse domain bucket (`"project"`).
    pub source_domain: String,
    /// The producer pipeline that emitted the snapshot.
    pub extractor: String,
    /// The codebook the ids are minted from.
    pub ogar_codebook: String,
    /// ISO date the snapshot was generated.
    pub generated: String,
}

/// One promoted canonical concept that the OpenProject corpus exhibits.
#[derive(Debug, Clone, Deserialize)]
pub struct Concept {
    /// Canonical concept name (curator-agnostic, e.g. `project_work_item`).
    pub canonical_concept: String,
    /// Codebook id as a `0xDDCC` hex string (e.g. `"0x0102"`).
    pub class_id: String,
    /// Codebook id as 2 little-endian bytes — the wire form.
    pub class_id_le: [u8; 2],
    /// The OpenProject Rails class name(s) that converge onto this concept.
    pub curator_classes: Vec<String>,
}

impl Concept {
    /// The codebook id as a `u16`, decoded from its little-endian bytes.
    #[must_use]
    pub fn class_id_u16(&self) -> u16 {
        u16::from_le_bytes(self.class_id_le)
    }

    /// The domain high byte (`id >> 8`). `0x01` for every OpenProject concept.
    #[must_use]
    pub fn domain_high_byte(&self) -> u8 {
        self.class_id_le[1]
    }
}

/// The full canonical snapshot for the OpenProject curator.
#[derive(Debug, Clone, Deserialize)]
pub struct Snapshot {
    /// Schema version tag (`"op-canon/1"`).
    pub schema_version: String,
    /// Where the snapshot came from.
    pub provenance: Provenance,
    /// Total classes the producer extracted from the corpus
    /// (core `app/models` + every `modules/*/app/models`).
    pub total_classes_extracted: usize,
    /// How many of those promoted into a codebook concept.
    pub promoted_classes: usize,
    /// The promoted canonical concepts (sorted by concept name).
    pub concepts: Vec<Concept>,
}

impl Snapshot {
    /// Parse the embedded snapshot. Panics only if the vendored JSON is
    /// malformed — which a test in this crate guarantees it is not.
    #[must_use]
    pub fn load() -> Self {
        serde_json::from_str(SNAPSHOT_JSON).expect("embedded op.ogar.json is valid JSON")
    }

    /// Find a concept by its canonical name.
    #[must_use]
    pub fn concept(&self, canonical: &str) -> Option<&Concept> {
        self.concepts
            .iter()
            .find(|c| c.canonical_concept == canonical)
    }

    /// Reverse lookup: which canonical concept an OpenProject Rails class
    /// maps to (`"WorkPackage"` → `project_work_item`).
    #[must_use]
    pub fn concept_of_class(&self, curator_class: &str) -> Option<&Concept> {
        self.concepts
            .iter()
            .find(|c| c.curator_classes.iter().any(|n| n == curator_class))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_loads_as_openproject_project_domain() {
        let s = Snapshot::load();
        assert_eq!(s.schema_version, "op-canon/1");
        assert_eq!(s.provenance.source_curator, "openproject");
        assert_eq!(s.provenance.source_domain, "project");
        // engine-walking harvest (core + modules/*/app/models). Loose
        // bounds — the band moves with corpus and codebook growth.
        assert!(
            s.total_classes_extracted >= 800,
            "engine-walking harvest should be ~900+, got {}",
            s.total_classes_extracted,
        );
        assert!(!s.concepts.is_empty());
        // promoted_classes counts curator classes, not concepts (some
        // concepts collapse 2+ OpenProject classes — Principal+User+Group
        // for project_actor, Role+ProjectRole for project_role).
        let curator_class_count: usize = s.concepts.iter().map(|c| c.curator_classes.len()).sum();
        assert_eq!(s.promoted_classes, curator_class_count);
    }

    #[test]
    fn every_promoted_id_is_in_the_project_mgmt_domain() {
        // The payoff of domain-encoded ids: a project-domain curator only
        // yields project-mgmt (0x01) codebook ids — no cross-domain leak.
        for c in &Snapshot::load().concepts {
            assert_eq!(
                c.domain_high_byte(),
                0x01,
                "{} ({}) is not in the project-mgmt domain",
                c.canonical_concept,
                c.class_id
            );
            // The hex string and the LE bytes agree.
            assert_eq!(format!("0x{:04X}", c.class_id_u16()), c.class_id);
        }
    }

    #[test]
    fn codebook_ids_are_unique() {
        use std::collections::HashSet;
        let s = Snapshot::load();
        let mut seen = HashSet::new();
        for c in &s.concepts {
            assert!(seen.insert(c.class_id_u16()), "duplicate id {}", c.class_id);
            assert_ne!(
                c.class_id_u16(),
                0,
                "id must be non-zero (0x0000 is reserved)"
            );
        }
    }

    #[test]
    fn fork_lineage_convergence_invariants_hold() {
        // OpenProject's class names map onto the canonical concepts Redmine
        // also lands on. These are the invariants the OGAR real-corpus
        // convergence tests prove; the snapshot pins them here for the OP
        // side. Mirror of `redmine-canon`'s identically-named test.
        let s = Snapshot::load();
        for (curator_class, concept, id) in [
            // Divergent across the fork — same id, different word:
            ("WorkPackage", "project_work_item", 0x0102u16),
            ("Status", "project_status", 0x0105),
            ("Type", "project_type", 0x0106),
            ("Forum", "project_forum", 0x0116),
            ("Relation", "project_relation", 0x0111),
            // Same name on both sides:
            ("Project", "project", 0x0101),
            ("TimeEntry", "billable_work_entry", 0x0103),
            ("User", "project_actor", 0x0104),
            ("IssuePriority", "priority", 0x0107),
            ("Role", "project_role", 0x0117),
            ("MemberRole", "project_member_role", 0x0118),
            ("CustomValue", "project_custom_value", 0x0119),
            ("EnabledModule", "project_enabled_module", 0x011A),
        ] {
            let c = s
                .concept_of_class(curator_class)
                .unwrap_or_else(|| panic!("{curator_class} not mapped in the snapshot"));
            assert_eq!(c.canonical_concept, concept, "{curator_class} concept");
            assert_eq!(c.class_id_u16(), id, "{curator_class} id");
        }
    }

    #[test]
    fn project_actor_collapses_the_sti_chain() {
        // Principal (STI root) + User + Group all converge onto one actor
        // identity — three OpenProject classes, one concept/id.
        let s = Snapshot::load();
        let actor = s.concept("project_actor").unwrap();
        for name in ["User", "Principal", "Group"] {
            assert!(
                actor.curator_classes.contains(&name.to_string()),
                "{name} should collapse into project_actor (saw {:?})",
                actor.curator_classes,
            );
        }
        assert_eq!(actor.class_id_u16(), 0x0104);
    }

    #[test]
    fn project_role_carries_both_role_and_projectrole_subclass() {
        // OpenProject ships BOTH a base `Role` model and a specialized
        // `ProjectRole` subclass on top — both collapse onto project_role.
        // (Redmine ships only `Role`; this is an OP-specific shape detail
        // that the canonical layer abstracts over.)
        let s = Snapshot::load();
        let role = s.concept("project_role").unwrap();
        for name in ["Role", "ProjectRole"] {
            assert!(
                role.curator_classes.contains(&name.to_string()),
                "{name} should collapse into project_role (saw {:?})",
                role.curator_classes,
            );
        }
        assert_eq!(role.class_id_u16(), 0x0117);
    }

    #[test]
    fn billable_work_entry_carries_op_time_entry_from_modules_costs() {
        // OpenProject's TimeEntry lives in modules/costs/app/models — only
        // visible to the engine-walking extractor. Pinning it here closes
        // the cross-domain bridge from the OP side (the concept Odoo's
        // account.analytic.line also lands on in the commerce arm).
        let s = Snapshot::load();
        let bridge = s.concept("billable_work_entry").unwrap();
        assert!(bridge.curator_classes.contains(&"TimeEntry".to_string()));
        assert_eq!(bridge.class_id_u16(), 0x0103);
    }
}
