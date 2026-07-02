//! `ogar_codebook` — the OGAR concept codebook, wire-compatible mirror (D-OVC-1).
//!
//! OGAR's `ogar-vocab` crate owns the canonical class-identity codebook: a curated
//! `(concept, u16)` table whose ids are **domain-encoded** as `0xDDCC` (`DD` = the
//! domain high byte, `CC` = the concept slot, `CC == 0x00` = the domain root,
//! reserved). Its own doc-comment says the long-term home of these types is
//! `lance-graph-contract`, "alongside `ClassId` and the `NodeGuid` LE layout."
//!
//! This module is that home — but **wire-compatible, not a dependency**. The
//! contract is zero-runtime-dep by design, so it does NOT depend on `ogar-vocab`;
//! instead both crates agree on the **wire**: a concept's id is one `u16`,
//! serialized little-endian, and that id IS the low 16 bits of
//! [`NodeGuid::classid`](crate::NodeGuid). Any encoder/decoder that agrees on
//! `u16` LE is compatible regardless of which crate it links. The parity tests
//! below pin the shared values; if OGAR's `CODEBOOK` ever moves an id, BOTH sides
//! must update together (the drift guard).
//!
//! What this mirror carries: the **codebook-id layer** the contract needs to route
//! a `classid` to its domain ([`canonical_concept_domain`], [`classid_concept_domain`])
//! and to resolve a canonical-concept string to its id ([`canonical_concept_id`],
//! [`LabelDTO::from_canonical`]). It also carries the **APP / render-prefix
//! layer** (the hi u16): [`AppPrefix`] (the §2 allocation table as typed data),
//! [`render_classid`] / [`render_classid_for_concept`] (compose), and
//! [`classid_app_prefix`] / [`classid_concept`] (decompose) — the membrane
//! equivalent of OGAR `render_classid_for::<P>()`, so a zero-dep consumer stamps
//! the prefix from ONE source instead of hardcoding `0x000N`. What it does NOT carry: OGAR's curator-alias
//! normalizer (`canonical_concept` — the large `"Issue"`/`"WorkPackage"` →
//! `"project_work_item"` table). Alias normalization stays in `ogar-vocab`; this
//! module resolves canonical-shaped concept strings only (hence `from_canonical`,
//! not `from_alias` — naming the difference rather than faking parity).
//!
//! Cross-ref: `.claude/plans/ogar-vocab-contract-codebook-migration-v1.md`,
//! OGAR `crates/ogar-vocab/src/lib.rs` (`CODEBOOK` / `ConceptDomain` / `LabelDTO`),
//! [`canonical_node`](crate::canonical_node) (`CLASSID_*`), [`codebook`](crate::codebook)
//! (the FINER per-family scope — this is the coarse concept/classid scope).

/// Codebook **domain** — the high byte of a canonical id (`id >> 8`, the `0xDDCC`
/// layout). Lets a consumer route on domain in O(1) from just the `u16`, no table
/// lookup. Reserved high-byte slots have a stable variant even before a concept
/// lands there, so consumers can branch on them today. Mirrors OGAR
/// `ogar_vocab::ConceptDomain` (wire-compatible — same `id >> 8` discriminant).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConceptDomain {
    /// `0x00XX` — reserved (`0x0000` is [`NodeGuid::CLASSID_DEFAULT`]).
    Reserved,
    /// `0x01XX` — project-management (OpenProject ↔ Redmine).
    ProjectMgmt,
    /// `0x02XX` — commerce / billing / ERP (Odoo ↔ OSB).
    Commerce,
    /// `0x07XX` — OSINT (open-source intelligence / Palantir-Gotham).
    Osint,
    /// `0x08XX` — OCR (optical character recognition / document extraction).
    Ocr,
    /// `0x09XX` — Health (clinical / patient / care).
    Health,
    /// `0x0AXX` — Anatomy (FMA reference ontology; bones / skeleton / joints).
    /// Public structural reference, distinct from `Health` PHI — the FMA
    /// anatomy domain (`anatomical_structure` / `skeleton` / `bone` / `joint`).
    Anatomy,
    /// `0x0BXX` — Auth (identity / authz: AuthStore, Zitadel, Zanzibar, Ory Keto).
    Auth,
    /// `0x0CXX` — Automation (the HIRO IT-automation stack: the MARS structural
    /// CMDB — `mars_application` / `mars_resource` / `mars_software` /
    /// `mars_machine` — and the Automation DO-arm actuators — `knowledge_item` /
    /// `mars_node_template` / `action_handler` / `action_applicability` /
    /// `automation_trigger`). Infrastructure config, not PHI. Mirrors OGAR
    /// `ogar_vocab::ConceptDomain::Automation`.
    Automation,
    /// `0x0DXX` — HR (employment / org / contracts; `vcard:Individual` /
    /// `org:OrganizationalUnit` / `org:Role` / `fibo:Contract` alignment).
    /// Public master-data for person + organizational-unit + role +
    /// employment-contract entities; distinct from `Auth` (the IdP→classid
    /// bridge) and from `Health` PHI. Mirrors OGAR
    /// `ogar_vocab::ConceptDomain::HR` (added in OGAR PR #127).
    HR,
    /// `0x0EXX` — Genetics (pharmacogenomics; CPIC gene–drug guidelines, variant
    /// annotations). Public reference knowledge, distinct from `Health` PHI.
    /// Operator-allocated 2026-06-26 (`0x0D` was already HR). Mirror target
    /// `ogar_vocab::ConceptDomain::Genetics` — OGAR catches up under the drift
    /// guard (the parity tests pin `0x0E00 → Genetics`).
    Genetics,
    /// Any high-byte slot not yet assigned a domain (`0x03XX`–`0x06XX`, `0x0FXX`+).
    Unassigned,
}

/// Resolve a canonical id's [`ConceptDomain`] from its high byte. Pure,
/// deterministic, O(1) — no table lookup. The single rule both the contract's
/// `classid → ReadMode` registry and OGAR's promotion gate route on.
#[inline]
#[must_use]
pub fn canonical_concept_domain(id: u16) -> ConceptDomain {
    match id >> 8 {
        0x00 => ConceptDomain::Reserved,
        0x01 => ConceptDomain::ProjectMgmt,
        0x02 => ConceptDomain::Commerce,
        0x07 => ConceptDomain::Osint,
        0x08 => ConceptDomain::Ocr,
        0x09 => ConceptDomain::Health,
        0x0A => ConceptDomain::Anatomy,
        0x0B => ConceptDomain::Auth,
        0x0C => ConceptDomain::Automation,
        0x0D => ConceptDomain::HR,
        0x0E => ConceptDomain::Genetics,
        _ => ConceptDomain::Unassigned,
    }
}

/// Resolve a [`NodeGuid`](crate::NodeGuid) `classid` to its [`ConceptDomain`] (D-OVC-4). The
/// codebook id is the low 16 bits of the classid (`0xDDCC` lives in the low u16);
/// the high u16 is the canon-reserved zero-fallback prefix. So a domain route is
/// `canonical_concept_domain(classid as u16)`. This is the coarse sibling of the
/// per-family scope in [`codebook`](crate::codebook): classid (domain) selects the
/// coarse codebook; `family` selects the sub-codebook (longest-prefix-wins).
#[inline]
#[must_use]
pub fn classid_concept_domain(classid: u32) -> ConceptDomain {
    canonical_concept_domain(classid as u16)
}

/// Map a coarse curator `source_domain` tag (`"project"`, `"erp"`, `"german-erp"`)
/// to the [`ConceptDomain`] its promotions live in. `None` for an unrecognised tag
/// (the producer's source-domain → typed-domain seam). Mirrors OGAR
/// `source_domain_concept`.
#[inline]
#[must_use]
pub fn source_domain_concept(source_domain: &str) -> Option<ConceptDomain> {
    match source_domain {
        "project" => Some(ConceptDomain::ProjectMgmt),
        "erp" | "german-erp" => Some(ConceptDomain::Commerce),
        _ => None,
    }
}

// ── APP / render-prefix layer (the hi u16) — wire-compat mirror of OGAR `ogar_vocab::app` ──

/// The **APP / render prefix** — the high u16 of a full 32-bit `classid`.
///
/// A full render classid is two orthogonal halves:
///
/// ```text
/// classid : u32  =  [ hi u16 : APP / render prefix ]  [ lo u16 : concept ]
///                     0xAAAA (per-app ClassView lens)    0xDDCC (shared RBAC+ontology)
/// ```
///
/// `0x0000` ([`AppPrefix::Core`]) is the shared canonical core — every
/// [`canonical_concept_id`] is `0x0000_DDCC`, additive and invariant. A
/// non-zero prefix selects an app's render lens (its per-app `ClassView` /
/// template set) while the lo-u16 concept — the RBAC + ontology + cross-app
/// identity key — stays shared; concept/domain routing reads only the low half
/// ([`classid_concept_domain`] does `… as u16`), so it is identical under every
/// render prefix. Mirrors OGAR `PortSpec::APP_PREFIX` (the
/// `APP-CLASS-CODEBOOK-LAYOUT.md` §2 allocation table as typed data);
/// wire-compatible, **no `ogar-vocab` dependency**. This is the membrane
/// equivalent of OGAR's `render_classid_for::<P>()` — the contract carries the
/// prefix as an enum value rather than a `PortSpec` generic, so a zero-dep
/// consumer never hand-stamps `0x000N`. Drift is guarded by
/// [`tests::app_prefixes_match_ogar_allocation_table`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AppPrefix {
    /// `0x0000` — shared canonical core (default `ClassView`, no render lens).
    Core,
    /// `0x0001` — OpenProject (project-mgmt render lens).
    OpenProject,
    /// `0x0002` — Odoo (commerce / ERP render lens).
    Odoo,
    /// `0x0003` — WoA (WorkOrder render lens).
    Woa,
    /// `0x0004` — SMB-Office render lens.
    Smb,
    /// `0x0005` — Healthcare / MedCare render lens.
    Healthcare,
    /// `0x0007` — Redmine (project-mgmt render lens; OpenProject twin at the
    /// shared concept level).
    Redmine,
}

impl AppPrefix {
    /// The reserved high-u16 prefix from the §2 allocation table. `const` so it
    /// composes in `const` contexts. MUST match OGAR `PortSpec::APP_PREFIX`
    /// (pinned by [`tests::app_prefixes_match_ogar_allocation_table`]).
    #[inline]
    #[must_use]
    pub const fn prefix(self) -> u16 {
        match self {
            AppPrefix::Core => 0x0000,
            AppPrefix::OpenProject => 0x0001,
            AppPrefix::Odoo => 0x0002,
            AppPrefix::Woa => 0x0003,
            AppPrefix::Smb => 0x0004,
            AppPrefix::Healthcare => 0x0005,
            AppPrefix::Redmine => 0x0007,
        }
    }

    /// Resolve a high-u16 prefix value back to its [`AppPrefix`]. `None` for an
    /// unallocated value (`0x0006`, `0x0008`+ — reserved, costs nothing until
    /// an app mints its first private class).
    #[inline]
    #[must_use]
    pub const fn from_prefix(prefix: u16) -> Option<Self> {
        match prefix {
            0x0000 => Some(AppPrefix::Core),
            0x0001 => Some(AppPrefix::OpenProject),
            0x0002 => Some(AppPrefix::Odoo),
            0x0003 => Some(AppPrefix::Woa),
            0x0004 => Some(AppPrefix::Smb),
            0x0005 => Some(AppPrefix::Healthcare),
            0x0007 => Some(AppPrefix::Redmine),
            _ => None,
        }
    }

    /// Compose the full render `classid` for this app and a canonical concept
    /// id: `(prefix << 16) | concept`. The membrane equivalent of OGAR
    /// `render_classid_for::<P>(concept)`, reading the prefix from typed data
    /// rather than a `PortSpec` generic.
    #[inline]
    #[must_use]
    pub const fn render(self, concept: u16) -> u32 {
        render_classid(self.prefix(), concept)
    }
}

/// Compose a full render `classid` from an app `prefix` (high u16) and a
/// canonical `concept` id (low u16): `(prefix << 16) | concept`. Wire-compat
/// mirror of OGAR `ogar_vocab::app::render_classid`.
///
/// `render_classid(0x0005, 0x0901)` → `0x0005_0901` (MedCare's `patient`); the
/// core form `render_classid(0x0000, id)` equals `id` widened to `u32`
/// (additive — a bare concept IS a render classid under the core lens).
#[inline]
#[must_use]
pub const fn render_classid(prefix: u16, concept: u16) -> u32 {
    ((prefix as u32) << 16) | (concept as u32)
}

/// Compose a render `classid` from an [`AppPrefix`] and a **canonical-concept
/// string** — looks the concept up in [`CODEBOOK`], then stamps the prefix.
/// `None` if the concept is not promoted. The one-call membrane equivalent of
/// OGAR `render_classid_for::<P>(class_ids::CONCEPT)`: a consumer pulls the id
/// AND the prefix from ONE source instead of hardcoding `0x000N`.
///
/// ```
/// use lance_graph_contract::{render_classid_for_concept, AppPrefix};
/// // MedCare patient under the Healthcare render lens — the canonical example.
/// assert_eq!(render_classid_for_concept(AppPrefix::Healthcare, "patient"), Some(0x0005_0901));
/// assert_eq!(render_classid_for_concept(AppPrefix::Healthcare, "not_a_concept"), None);
/// ```
#[inline]
#[must_use]
pub fn render_classid_for_concept(app: AppPrefix, concept: &str) -> Option<u32> {
    canonical_concept_id(concept).map(|id| app.render(id))
}

/// The APP / render-prefix half of a full `classid` (`classid >> 16`). Mirror
/// of OGAR `ogar_vocab::app::app_of`. Pair with [`AppPrefix::from_prefix`] to
/// recover the typed app.
#[inline]
#[must_use]
pub const fn classid_app_prefix(classid: u32) -> u16 {
    (classid >> 16) as u16
}

/// The canonical concept-id half of a full `classid` (`classid as u16`) — the
/// shared RBAC + ontology + cross-app identity key, identical under every
/// render prefix. Mirror of OGAR `ogar_vocab::app::concept_of`; the sibling of
/// [`classid_concept_domain`], which routes this half to its [`ConceptDomain`].
#[inline]
#[must_use]
pub const fn classid_concept(classid: u32) -> u16 {
    classid as u16
}

/// The curated `(canonical_concept, u16)` codebook — wire-compatible mirror of
/// OGAR `ogar_vocab::CODEBOOK`. Ids are stable forever (once shipped, never
/// re-assigned); domain-encoded `0xDDCC`. Carries the two domains the contract
/// graph surfaces realize today (project-mgmt `0x01XX`, commerce/ERP `0x02XX`);
/// OSINT (`0x07XX`) and Health/anatomy (`0x09XX`) are represented by their
/// [`NodeGuid`](crate::NodeGuid) classid roots, not yet by promoted concept slots here. Drift is
/// guarded by [`tests::codebook_ids_match_ogar_vocab`].
pub const CODEBOOK: &[(&str, u16)] = &[
    // ── 0x01XX — project-mgmt domain (OpenProject ↔ Redmine) ──
    ("project", 0x0101),
    ("project_work_item", 0x0102),
    ("billable_work_entry", 0x0103),
    ("project_actor", 0x0104),
    ("project_status", 0x0105),
    ("project_type", 0x0106),
    ("priority", 0x0107),
    ("project_membership", 0x0108),
    ("project_journal", 0x0109),
    ("project_repository", 0x010A),
    ("project_version", 0x010B),
    ("project_wiki_page", 0x010C),
    ("project_query", 0x010D),
    ("project_attachment", 0x010E),
    ("project_comment", 0x010F),
    ("project_custom_field", 0x0110),
    ("project_relation", 0x0111),
    ("project_changeset", 0x0112),
    ("project_watcher", 0x0113),
    ("project_news", 0x0114),
    ("project_message", 0x0115),
    ("project_forum", 0x0116),
    ("project_role", 0x0117),
    ("project_member_role", 0x0118),
    ("project_custom_value", 0x0119),
    ("project_enabled_module", 0x011A),
    // ── 0x02XX — commerce / billing / ERP domain (Odoo ↔ OSB) ──
    ("commercial_line_item", 0x0201),
    ("commercial_document", 0x0202),
    ("tax_policy", 0x0203),
    ("billing_party", 0x0204),
    ("payment_record", 0x0205),
    ("currency_policy", 0x0206),
    // Phase-3 mints per OGAR PR #111: both product.template / product.product
    // and account.account / account.account.template converge on these two
    // canonical concepts (same convergence pattern as account.move ↔ sale.order
    // → commercial_document). Closes the cross-axis identity gap surfaced by
    // odoo-rs PR #14.
    ("product", 0x0207),
    ("accounting_account", 0x0208),
    // ProductCatalog cluster (OGAR #126): closes 3 more of the 11 cross-axis
    // gaps surfaced by odoo-rs PR #14. All stay in 0x02XX commerce arm.
    ("pricelist", 0x0209),
    ("pricelist_rule", 0x020A),
    ("unit_of_measure", 0x020B),
    // ── 0x09XX — Health domain (MedCare; OGIT NTO/Healthcare promotion) ──
    ("patient", 0x0901),
    ("diagnosis", 0x0902),
    ("lab_value", 0x0903),
    ("medication", 0x0904),
    ("treatment", 0x0905),
    ("visit", 0x0906),
    ("vital_sign", 0x0907),
    // ── 0x0AXX — Anatomy domain (FMA reference ontology; public, not PHI) ──
    // FMA anatomy lives HERE, not in Health 0x09 — reference structure is
    // public, a clinical finding *about* it is PHI. `CLASSID_FMA` retargets to
    // `anatomical_structure` (0x0A01), clearing the prior 0x0901 = `patient`
    // collision. Mirrors OGAR `ogar-vocab` ConceptDomain::Anatomy.
    ("anatomical_structure", 0x0A01),
    ("skeleton", 0x0A02),
    ("bone", 0x0A03),
    ("joint", 0x0A04),
    // ── 0x0BXX — Auth domain (identity / authz; OGAR's 0x0B AuthStore family) ──
    ("auth_store", 0x0B01),
    ("auth_zitadel", 0x0B02),
    ("auth_zanzibar", 0x0B03),
    ("auth_ory_keto", 0x0B04),
    // ── 0x0DXX — HR domain (employment / org / contracts; OGAR PR #127) ──
    // Closes the final 4-of-11 cross-axis identity gap surfaced by odoo-rs
    // PR #14: hr.employee / hr.department / hr.job / hr.contract.
    ("hr_employee", 0x0D01),
    ("hr_department", 0x0D02),
    ("hr_job", 0x0D03),
    ("hr_employment_contract", 0x0D04),
    // ── 0x0CXX — Automation domain (HIRO IT-automation: MARS CMDB + DO-arm
    // actuators; OGAR's 0x0C Automation domain). One domain spanning the MARS
    // structural CMDB and the Automation behavioral vocabulary. ──
    ("mars_application", 0x0C01),
    ("mars_resource", 0x0C02),
    ("mars_software", 0x0C03),
    ("mars_machine", 0x0C04),
    ("knowledge_item", 0x0C05),
    ("mars_node_template", 0x0C06),
    ("action_handler", 0x0C07),
    ("action_applicability", 0x0C08),
    ("automation_trigger", 0x0C09),
];

/// Resolve a **canonical-concept** string to its stable `u16` codebook id via
/// [`CODEBOOK`]. `None` for an unpromoted concept (not in the codebook).
///
/// This resolves canonical-shaped names only (e.g. `"project_work_item"`). For
/// curator-shaped aliases (`"Issue"`, `"WorkPackage"`), normalize through OGAR
/// `ogar_vocab::canonical_concept` first — that alias table stays in `ogar-vocab`,
/// out of the zero-dep contract.
#[inline]
#[must_use]
pub fn canonical_concept_id(concept: &str) -> Option<u16> {
    CODEBOOK
        .iter()
        .find_map(|&(name, id)| (name == concept).then_some(id))
}

/// A curator-agnostic label binding: a consumer-local `label`, its OGAR codebook
/// `id` (binary identity), and the portable `canonical` symbol. Mirrors OGAR
/// `ogar_vocab::LabelDTO` (wire-compatible). Identity comparison uses `id`;
/// AST/planner emission uses `canonical`; presentation uses `label`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LabelDTO {
    /// Consumer-local label. Not normalized by the contract.
    pub label: String,
    /// OGAR codebook binary identity (the classid low u16).
    pub id: u16,
    /// Canonical-AST label — the portable curator-agnostic symbol.
    pub canonical: String,
}

impl LabelDTO {
    /// Build a `LabelDTO` from a **canonical-shaped** concept string. `None` if the
    /// concept is not in [`CODEBOOK`]. (Contract counterpart of OGAR's
    /// `from_alias`, minus curator-alias normalization — see the module docs:
    /// pass a canonical concept, or normalize via `ogar-vocab` first.)
    #[must_use]
    pub fn from_canonical(concept: impl Into<String>) -> Option<Self> {
        let canonical = concept.into();
        let id = canonical_concept_id(&canonical)?;
        Some(Self {
            label: canonical.clone(),
            id,
            canonical,
        })
    }

    /// `id` rendered as **2 little-endian bytes** — the wire contract. Roundtrips
    /// via `u16::from_le_bytes`. Byte order matches the [`NodeGuid`](crate::NodeGuid) LE layout, so
    /// this is exactly the classid low half on the wire.
    #[inline]
    #[must_use]
    pub fn id_le(&self) -> [u8; 2] {
        self.id.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeGuid;

    #[test]
    fn domain_routes_on_high_byte() {
        assert_eq!(canonical_concept_domain(0x0000), ConceptDomain::Reserved);
        assert_eq!(canonical_concept_domain(0x0101), ConceptDomain::ProjectMgmt);
        assert_eq!(canonical_concept_domain(0x0206), ConceptDomain::Commerce);
        assert_eq!(canonical_concept_domain(0x0700), ConceptDomain::Osint);
        assert_eq!(canonical_concept_domain(0x0801), ConceptDomain::Ocr);
        assert_eq!(canonical_concept_domain(0x0901), ConceptDomain::Health);
        assert_eq!(canonical_concept_domain(0x0A01), ConceptDomain::Anatomy);
        assert_eq!(canonical_concept_domain(0x0B01), ConceptDomain::Auth);
        assert_eq!(canonical_concept_domain(0x0C01), ConceptDomain::Automation);
        assert_eq!(canonical_concept_domain(0x0C09), ConceptDomain::Automation);
        assert_eq!(canonical_concept_domain(0x0D01), ConceptDomain::HR);
        assert_eq!(canonical_concept_domain(0x0D04), ConceptDomain::HR);
        assert_eq!(canonical_concept_domain(0x0500), ConceptDomain::Unassigned);
        // Genetics (0x0E) operator-allocated 2026-06-26 for CPIC-V3 (was Unassigned).
        assert_eq!(canonical_concept_domain(0x0E00), ConceptDomain::Genetics);
        assert_eq!(canonical_concept_domain(0x0F00), ConceptDomain::Unassigned);
    }

    #[test]
    fn classid_routes_through_low_u16() {
        // The contract classids resolve to the domain their `0xDDCC` low half
        // encodes — the contract↔OGAR alignment (ISS-CLASSID-OGAR-DRIFT).
        assert_eq!(
            classid_concept_domain(NodeGuid::CLASSID_PROJECT),
            ConceptDomain::ProjectMgmt
        );
        assert_eq!(
            classid_concept_domain(NodeGuid::CLASSID_ERP),
            ConceptDomain::Commerce
        );
        assert_eq!(
            classid_concept_domain(NodeGuid::CLASSID_OSINT),
            ConceptDomain::Osint
        );
        assert_eq!(
            classid_concept_domain(NodeGuid::CLASSID_FMA),
            ConceptDomain::Anatomy,
            "FMA anatomy lives in the Anatomy domain (0x0AXX), not Health — \
             cleared the 0x0901 = `patient` collision"
        );
        assert_eq!(
            classid_concept_domain(NodeGuid::CLASSID_DEFAULT),
            ConceptDomain::Reserved
        );
    }

    #[test]
    fn source_domain_maps_to_concept_domain() {
        assert_eq!(
            source_domain_concept("project"),
            Some(ConceptDomain::ProjectMgmt)
        );
        assert_eq!(source_domain_concept("erp"), Some(ConceptDomain::Commerce));
        assert_eq!(
            source_domain_concept("german-erp"),
            Some(ConceptDomain::Commerce)
        );
        assert_eq!(source_domain_concept("nope"), None);
    }

    #[test]
    fn codebook_ids_match_ogar_vocab() {
        // Drift guard: these MUST match OGAR `ogar_vocab::CODEBOOK` exactly (the
        // wire is the contract). If OGAR moves an id, update BOTH sides together.
        assert_eq!(canonical_concept_id("project"), Some(0x0101));
        assert_eq!(canonical_concept_id("project_work_item"), Some(0x0102));
        assert_eq!(canonical_concept_id("project_enabled_module"), Some(0x011A));
        assert_eq!(canonical_concept_id("commercial_line_item"), Some(0x0201));
        assert_eq!(canonical_concept_id("commercial_document"), Some(0x0202));
        assert_eq!(canonical_concept_id("currency_policy"), Some(0x0206));
        // 0x09XX Health + 0x0BXX Auth (OGAR #110 minted the AuthStore family).
        assert_eq!(canonical_concept_id("patient"), Some(0x0901));
        assert_eq!(canonical_concept_id("vital_sign"), Some(0x0907));
        assert_eq!(canonical_concept_id("auth_store"), Some(0x0B01));
        assert_eq!(canonical_concept_id("auth_ory_keto"), Some(0x0B04));
        // 0x0CXX Automation (the MARS/Automation codebook pass minted these in OGAR).
        assert_eq!(canonical_concept_id("mars_application"), Some(0x0C01));
        assert_eq!(canonical_concept_id("knowledge_item"), Some(0x0C05));
        assert_eq!(canonical_concept_id("mars_node_template"), Some(0x0C06));
        assert_eq!(canonical_concept_id("automation_trigger"), Some(0x0C09));
        assert_eq!(canonical_concept_id("not_a_concept"), None);
    }

    #[test]
    fn codebook_has_no_duplicate_ids_or_zero_concept_slot() {
        // Every id non-zero in its concept slot (CC != 0x00 — root is reserved),
        // every id unique, and each id's domain matches its position.
        let mut seen = std::collections::HashSet::new();
        for &(name, id) in CODEBOOK {
            assert_ne!(
                id & 0x00FF,
                0x00,
                "{name}: concept slot CC must be non-zero"
            );
            assert!(seen.insert(id), "{name}: duplicate id {id:#06x}");
        }
    }

    #[test]
    fn label_dto_roundtrips_canonical_and_wire() {
        let dto = LabelDTO::from_canonical("project_enabled_module").unwrap();
        assert_eq!(dto.id, 0x011A);
        assert_eq!(dto.canonical, "project_enabled_module");
        assert_eq!(dto.id_le(), [0x1A, 0x01]); // LE: low byte (0x1A) first, high (0x01)
        assert_eq!(u16::from_le_bytes(dto.id_le()), 0x011A);
        // domain reachable from the DTO id
        assert_eq!(canonical_concept_domain(dto.id), ConceptDomain::ProjectMgmt);
        assert_eq!(
            LabelDTO::from_canonical("Issue"),
            None,
            "curator alias unresolved in contract (normalize via ogar-vocab first)"
        );
    }

    #[test]
    fn app_prefixes_match_ogar_allocation_table() {
        // §2 allocation table — MUST match OGAR `PortSpec::APP_PREFIX` (the
        // wire). If OGAR re-allocates a prefix, update BOTH sides together.
        assert_eq!(AppPrefix::Core.prefix(), 0x0000);
        assert_eq!(AppPrefix::OpenProject.prefix(), 0x0001);
        assert_eq!(AppPrefix::Odoo.prefix(), 0x0002);
        assert_eq!(AppPrefix::Woa.prefix(), 0x0003);
        assert_eq!(AppPrefix::Smb.prefix(), 0x0004);
        assert_eq!(AppPrefix::Healthcare.prefix(), 0x0005);
        assert_eq!(AppPrefix::Redmine.prefix(), 0x0007);
        // round-trips; unallocated slots are None (reserved, cost nothing).
        for app in [
            AppPrefix::Core,
            AppPrefix::OpenProject,
            AppPrefix::Odoo,
            AppPrefix::Woa,
            AppPrefix::Smb,
            AppPrefix::Healthcare,
            AppPrefix::Redmine,
        ] {
            assert_eq!(AppPrefix::from_prefix(app.prefix()), Some(app));
        }
        assert_eq!(AppPrefix::from_prefix(0x0006), None);
        assert_eq!(AppPrefix::from_prefix(0x0008), None);
    }

    #[test]
    fn render_classid_composes_decomposes_and_preserves_the_concept_half() {
        // Worked examples mirrored from OGAR `ogar_vocab::app` tests.
        assert_eq!(render_classid(0x0001, 0x0102), 0x0001_0102);
        assert_eq!(render_classid(0x0007, 0x0102), 0x0007_0102); // Redmine twin

        // MedCare patient — the canonical worked example: 0x0005_0901.
        let pat = render_classid_for_concept(AppPrefix::Healthcare, "patient").unwrap();
        assert_eq!(pat, 0x0005_0901);
        assert_eq!(classid_app_prefix(pat), 0x0005);
        assert_eq!(classid_concept(pat), 0x0901);
        assert_eq!(
            AppPrefix::from_prefix(classid_app_prefix(pat)),
            Some(AppPrefix::Healthcare)
        );
        // the concept half still routes to its domain under the render prefix.
        assert_eq!(
            canonical_concept_domain(classid_concept(pat)),
            ConceptDomain::Health
        );

        // Core (hi=0x0000) is additive: a bare concept IS a render classid.
        let core = render_classid(0x0000, 0x0102);
        assert_eq!(core, u32::from(0x0102u16));
        assert_eq!(classid_concept(core), 0x0102);

        // The render lens never perturbs the lo-u16 concept RBAC keys on.
        let op = AppPrefix::OpenProject.render(0x0103);
        let rm = AppPrefix::Redmine.render(0x0103);
        assert_ne!(
            classid_app_prefix(op),
            classid_app_prefix(rm),
            "render lenses differ"
        );
        assert_eq!(
            classid_concept(op),
            classid_concept(rm),
            "concept is shared"
        );

        // Unpromoted concept → no classid (don't invent one).
        assert_eq!(
            render_classid_for_concept(AppPrefix::Healthcare, "nope"),
            None
        );
    }
}
