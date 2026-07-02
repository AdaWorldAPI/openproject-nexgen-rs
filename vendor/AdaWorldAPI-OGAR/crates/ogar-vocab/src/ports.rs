//! Port specifications — `(namespace, bridge_id, public_name → class_id)`
//! triples consumed by `lance_graph_ogar::bridges::UnifiedBridge` to
//! project per-port public name vocabularies onto the shared OGAR codebook.
//!
//! # The goal — one bridge harness, port-specific data
//!
//! Before this module landed, each port shipped a clone of the same
//! NamespaceBridge boilerplate (WoaBridge, MedcareBridge,
//! OpenProjectBridge, RedmineBridge) — same struct, same impl shape,
//! same codebook-aware `entity()` override, with a per-bridge constants
//! table baked into each file. Adding a port meant copy-pasting a
//! NamespaceBridge impl AND duplicating its alias table.
//!
//! [`PortSpec`] flips that: the bridge becomes one generic
//! `lance_graph_ogar::bridges::UnifiedBridge<P: PortSpec>` harness, and
//! the per-port differences (namespace, bridge_id, alias table) live
//! here as data attached to the canonical class schema. Adding a port
//! is now one `impl PortSpec for FooPort {...}` block with three
//! constants and the alias slice — no bridge boilerplate, no risk of
//! two ports' codebook tables drifting on a shared concept.
//!
//! # Apple meets apple — cross-fork convergence by data
//!
//! Both [`OpenProjectPort`] and [`RedminePort`] map their port-public
//! names to the **same** `class_ids::*` constants. So
//! `OpenProjectPort::class_id("WorkPackage") == RedminePort::class_id("Issue")`,
//! and any consumer reading
//! `bridge.entity(name).schema_ptr.entity_type_id()` gets identical
//! ids across the two ports — the cross-fork convergence the codebook
//! was calcified for, now sourced from the OGAR class schema rather
//! than re-declared per bridge.
//!
//! See [`tests`] below for the convergence pins.

use crate::class_ids;

/// Per-port specification consumed by the unified bridge.
///
/// Implementations carry zero state — they're zero-sized types that
/// parameterize the unified bridge at compile time. Three pieces of
/// data per port:
///
/// - [`Self::NAMESPACE`]: the canonical TTL namespace (matches
///   `ogit.<NS>:` prefix in the per-entity TTL files).
/// - [`Self::BRIDGE_ID`]: lowercase bridge_id for
///   `NamespaceBridge::bridge_id()` and registry dispatch.
/// - [`Self::aliases`]: slice of `(public_name, canonical_class_id)`
///   pairs. The default [`Self::class_id`] does a linear scan over
///   the slice; bypass it only when a port has so many aliases that
///   the O(n) lookup matters (none today; 32 concepts max per port).
pub trait PortSpec: 'static + Send + Sync {
    /// Canonical namespace name (e.g. `"OpenProject"`, `"Redmine"`).
    /// Matches the `ogit.<NS>:` TTL prefix and the
    /// `NamespaceRegistry::seed_defaults()` key.
    const NAMESPACE: &'static str;
    /// Lowercase bridge_id for `NamespaceBridge::bridge_id()`.
    const BRIDGE_ID: &'static str;

    /// Reserved APP / render prefix — the high u16 of a full 32-bit
    /// classid (`APP-CLASS-CODEBOOK-LAYOUT.md` §2).
    ///
    /// Composing the full render classid:
    /// ```text
    /// render_classid = (APP_PREFIX as u32) << 16 | concept_low_u16
    /// ```
    ///
    /// `0x0000` is the **shared canonical core** — the cross-app ontology
    /// every consumer reuses. Each app port overrides with its reserved
    /// prefix from the §2 allocation table, waking its own per-app
    /// ClassView / Askama template set for rendering while keeping the
    /// low-u16 concept (RBAC + ontology) shared.
    ///
    /// Reserving this prefix costs nothing (§2: "Reserving a prefix costs
    /// nothing — no codebook is materialised until the app mints its first
    /// private class"). This constant is the allocation table as typed
    /// data — consumers re-export it instead of hardcoding `0x0001` etc.
    const APP_PREFIX: u16 = 0x0000;

    /// All `(port-public-name, canonical-class-id)` aliases for this
    /// port. Order is not significant for resolution but kept stable
    /// for human readability.
    fn aliases() -> &'static [(&'static str, u16)];

    /// Map a port-public name to the canonical OGAR class_id.
    /// Returns `None` for names outside the alias table.
    fn class_id(public_name: &str) -> Option<u16> {
        Self::aliases()
            .iter()
            .find(|(name, _)| *name == public_name)
            .map(|(_, id)| *id)
    }
}

// ── OpenProject port ────────────────────────────────────────────────

/// OpenProject's `PortSpec` — maps OpenProject's Rails model names
/// (`WorkPackage`, `TimeEntry`, …) onto the shared OGAR codebook.
///
/// Sister of [`RedminePort`]. Concept-pair convergence (e.g.
/// `WorkPackage` ↔ `Issue` both → `class_ids::PROJECT_WORK_ITEM`)
/// is pinned by [`tests::openproject_and_redmine_converge_on_shared_concepts`].
pub struct OpenProjectPort;

impl PortSpec for OpenProjectPort {
    const NAMESPACE: &'static str = "OpenProject";
    const BRIDGE_ID: &'static str = "openproject";
    /// `0x0001` — OpenProject render prefix (§2 allocation table).
    const APP_PREFIX: u16 = 0x0001;
    fn aliases() -> &'static [(&'static str, u16)] {
        OPENPROJECT_ALIASES
    }
}

/// The OpenProject port's `(public_name, class_id)` alias slice,
/// exposed for downstream `pub const` re-exports
/// (e.g. `lance_graph_ogar::bridges::OPENPROJECT_CODEBOOK` keeps
/// its pre-migration shape by aliasing this slice). Prefer
/// [`OpenProjectPort::aliases`] in new code — going through the
/// `PortSpec` impl works generically across ports.
pub const OPENPROJECT_ALIASES: &[(&str, u16)] = &[
    ("Project", class_ids::PROJECT),
    ("WorkPackage", class_ids::PROJECT_WORK_ITEM),
    ("TimeEntry", class_ids::BILLABLE_WORK_ENTRY),
    // STI chain → project_actor (codex P2 on PR #87). Both OpenProject
    // and Redmine ship `User`, `Principal`, and `Group` as three classes
    // backed by one table; the codebook intentionally collapses them
    // ("Principal + User + Group STI chain collapsed" — see
    // `class_ids::PROJECT_ACTOR` doc). All three aliases route to the
    // SAME canonical id so dispatching on `entity_type_id()` reaches the
    // same arm regardless of which name the consumer hands in.
    ("User", class_ids::PROJECT_ACTOR),
    ("Principal", class_ids::PROJECT_ACTOR),
    ("Group", class_ids::PROJECT_ACTOR),
    ("Status", class_ids::PROJECT_STATUS),
    ("Type", class_ids::PROJECT_TYPE),
    // Both ports expose the priority class as `IssuePriority` (Rails
    // STI on Enumeration); codex P2 on PR #87 caught the original
    // `Priority` entry was the canonical-concept *name*, not the Rails
    // class name. Match the actual class so `class_id("IssuePriority")`
    // resolves.
    ("IssuePriority", class_ids::PRIORITY),
    // OpenProject's actual Rails class for `project_membership` is `Member`
    // (mirrors Redmine — both forks ship the join row as `Member`). The
    // engine-walking corpus snapshot in op-canon carries `Member`. The
    // earlier `Membership` alias was pre-snapshot prose; keep it as a
    // deprecated synonym so any consumer holding the old name still
    // resolves, but `Member` is the canonical OP surface for the concept.
    // Closes the openproject-nexgen-rs#56 pinned
    // `port_and_snapshot_membership_vocab_mismatch_is_known` test.
    ("Member", class_ids::PROJECT_MEMBERSHIP),
    ("Membership", class_ids::PROJECT_MEMBERSHIP),
    ("Journal", class_ids::PROJECT_JOURNAL),
    ("Repository", class_ids::PROJECT_REPOSITORY),
    ("Version", class_ids::PROJECT_VERSION),
    ("WikiPage", class_ids::PROJECT_WIKI_PAGE),
    ("Query", class_ids::PROJECT_QUERY),
    ("Attachment", class_ids::PROJECT_ATTACHMENT),
    ("CustomField", class_ids::PROJECT_CUSTOM_FIELD),
    ("Relation", class_ids::PROJECT_RELATION),
    ("Changeset", class_ids::PROJECT_CHANGESET),
    ("Watcher", class_ids::PROJECT_WATCHER),
    ("News", class_ids::PROJECT_NEWS),
    ("Message", class_ids::PROJECT_MESSAGE),
    ("Forum", class_ids::PROJECT_FORUM),
    ("Role", class_ids::PROJECT_ROLE),
    ("MemberRole", class_ids::PROJECT_MEMBER_ROLE),
    ("CustomValue", class_ids::PROJECT_CUSTOM_VALUE),
    ("EnabledModule", class_ids::PROJECT_ENABLED_MODULE),
];

// ── Redmine port ────────────────────────────────────────────────────

/// Redmine's `PortSpec` — maps Redmine's Rails model names (`Issue`,
/// `Tracker`, `IssueStatus`, …) onto the shared OGAR codebook.
///
/// Sister of [`OpenProjectPort`]. Both reference the same
/// `class_ids::*` constants for converging concepts, so
/// `OpenProjectPort::class_id("WorkPackage")` and
/// `RedminePort::class_id("Issue")` both resolve to `0x0102
/// project_work_item`.
pub struct RedminePort;

impl PortSpec for RedminePort {
    const NAMESPACE: &'static str = "Redmine";
    const BRIDGE_ID: &'static str = "redmine";
    /// `0x0007` — Redmine render prefix (§2 allocation table).
    const APP_PREFIX: u16 = 0x0007;
    fn aliases() -> &'static [(&'static str, u16)] {
        REDMINE_ALIASES
    }
}

/// The Redmine port's `(public_name, class_id)` alias slice — the
/// `pub` counterpart of [`OPENPROJECT_ALIASES`] for symmetry. See its
/// doc for rationale.
pub const REDMINE_ALIASES: &[(&str, u16)] = &[
    ("Project", class_ids::PROJECT),
    ("Issue", class_ids::PROJECT_WORK_ITEM),
    ("TimeEntry", class_ids::BILLABLE_WORK_ENTRY),
    // STI chain → project_actor (codex P2 on PR #87). Same fold as
    // OpenProject's aliases — see OPENPROJECT_ALIASES doc.
    ("User", class_ids::PROJECT_ACTOR),
    ("Principal", class_ids::PROJECT_ACTOR),
    ("Group", class_ids::PROJECT_ACTOR),
    ("IssueStatus", class_ids::PROJECT_STATUS),
    ("Tracker", class_ids::PROJECT_TYPE),
    // IssuePriority maps to the shared `priority` codebook arm
    // (codex P2 on PR #87 — Redmine's port previously had no
    // priority entry at all).
    ("IssuePriority", class_ids::PRIORITY),
    ("Member", class_ids::PROJECT_MEMBERSHIP),
    ("Journal", class_ids::PROJECT_JOURNAL),
    ("Repository", class_ids::PROJECT_REPOSITORY),
    ("Version", class_ids::PROJECT_VERSION),
    ("WikiPage", class_ids::PROJECT_WIKI_PAGE),
    ("Query", class_ids::PROJECT_QUERY),
    ("Attachment", class_ids::PROJECT_ATTACHMENT),
    ("Comment", class_ids::PROJECT_COMMENT),
    ("CustomField", class_ids::PROJECT_CUSTOM_FIELD),
    ("IssueRelation", class_ids::PROJECT_RELATION),
    ("Changeset", class_ids::PROJECT_CHANGESET),
    ("Watcher", class_ids::PROJECT_WATCHER),
    ("News", class_ids::PROJECT_NEWS),
    ("Message", class_ids::PROJECT_MESSAGE),
    ("Board", class_ids::PROJECT_FORUM),
    ("Role", class_ids::PROJECT_ROLE),
    ("MemberRole", class_ids::PROJECT_MEMBER_ROLE),
    ("CustomValue", class_ids::PROJECT_CUSTOM_VALUE),
    ("EnabledModule", class_ids::PROJECT_ENABLED_MODULE),
];

// ── Healthcare (medcare-rs) port ────────────────────────────────────

/// MedCare-rs's `PortSpec` — maps the `Healthcare` namespace's OGIT
/// entity names (`Patient`, `Diagnosis`, `LabValue`, `Medication`,
/// `Treatment`, `Visit`, `VitalSign`) onto the canonical OGAR Health
/// codebook (`0x09XX`).
///
/// Unlike [`OpenProjectPort`] / [`RedminePort`] — which converge two
/// project-management forks on a shared codebook — Healthcare is a
/// single-tenant namespace today, so there is no cross-port convergence
/// pin yet. The port exists so `lance_graph_ogar`'s `MedcareBridge`
/// collapses to `UnifiedBridge<HealthcarePort>`: the namespace,
/// bridge_id, and alias table are now **inherited from this canonical
/// class schema** instead of being re-declared per bridge in lance-graph.
/// Northstar T9 (Healthcare codebook promotion).
///
/// When a second clinical curator lands (FMA / SNOMED / RadLex import,
/// `lance-graph-rdf-fma-snomed-v1`), its port maps onto these same
/// `class_ids::*` constants — at which point Healthcare gains the same
/// apple-meets-apple convergence the project-management ports have.
pub struct HealthcarePort;

impl PortSpec for HealthcarePort {
    const NAMESPACE: &'static str = "Healthcare";
    const BRIDGE_ID: &'static str = "medcare";
    /// `0x0005` — Medcare / Healthcare render prefix (§2 allocation table).
    const APP_PREFIX: u16 = 0x0005;
    fn aliases() -> &'static [(&'static str, u16)] {
        HEALTHCARE_ALIASES
    }
}

/// The Healthcare port's `(public_name, class_id)` alias slice — the OGIT
/// `NTO/Healthcare/entities/` entity names projected onto `class_ids::*`.
/// `pub` for symmetry with [`OPENPROJECT_ALIASES`] / [`REDMINE_ALIASES`].
pub const HEALTHCARE_ALIASES: &[(&str, u16)] = &[
    ("Patient", class_ids::PATIENT),
    ("Diagnosis", class_ids::DIAGNOSIS),
    ("LabValue", class_ids::LAB_VALUE),
    ("Medication", class_ids::MEDICATION),
    ("Treatment", class_ids::TREATMENT),
    ("Visit", class_ids::VISIT),
    ("VitalSign", class_ids::VITAL_SIGN),
];

// ── WoA (work-order management) port ────────────────────────────────

/// WoA's `PortSpec` — maps the `WorkOrder` namespace's public names
/// (Customer / Vorgang / Position / Stundenzettel / …) onto the shared
/// OGAR codebook.
///
/// The convergence pin that matters: WoA's `Stundenzettel` and
/// `TimesheetActivity` resolve to [`class_ids::BILLABLE_WORK_ENTRY`] —
/// the **same** id [`OpenProjectPort::TimeEntry`] and
/// [`RedminePort::TimeEntry`] resolve to. That's the operator's value
/// statement ("planner times align with billable hours") realised as
/// data: a planner consumer's `TimeEntry` and an ERP consumer's
/// `Stundenzettel` carry the same `entity_type_id()` on the
/// `EntityRef`, so the cross-system integration is a codebook lookup,
/// not a translation layer. The pin is asserted in
/// [`tests::time_entry_converges_across_planner_and_erp_ports`].
///
/// Commerce concepts (Customer / Vorgang / Position / TaxRate /
/// Zahlung) route into the canonical `0x02XX` commerce block —
/// `BILLING_PARTY` / `COMMERCIAL_DOCUMENT` / `COMMERCIAL_LINE_ITEM` /
/// `TAX_POLICY` / `PAYMENT_RECORD` — so WoA's `Customer` and SMB's
/// `Kunde` and a future Odoo `res.partner` all resolve to one id.
/// Sister of [`SmbPort`]: both ports map their German + English
/// public names onto the same canonical block, giving German-SMB ERP
/// consumers cross-fork convergence the project-management ports
/// already enjoy.
pub struct WoaPort;

impl PortSpec for WoaPort {
    const NAMESPACE: &'static str = "WorkOrder";
    const BRIDGE_ID: &'static str = "woa";
    /// `0x0003` — WoA render prefix (§2 allocation table).
    const APP_PREFIX: u16 = 0x0003;
    fn aliases() -> &'static [(&'static str, u16)] {
        WOA_ALIASES
    }
}

/// The WoA port's `(public_name, class_id)` alias slice. Exposed `pub`
/// for symmetry with [`OPENPROJECT_ALIASES`] / [`REDMINE_ALIASES`] /
/// [`HEALTHCARE_ALIASES`]; prefer [`WoaPort::aliases`] in new code.
///
/// Includes both German and English synonyms for each canonical
/// concept (Vorgang ≡ WorkOrder, Stundenzettel ≡ TimesheetActivity ≡
/// TimeEntry, Zahlung ≡ Payment, etc.) — they all collapse to the
/// same canonical class_id, so consumers reading either German or
/// English public names route to the same dispatch arm.
pub const WOA_ALIASES: &[(&str, u16)] = &[
    // ── Billing party (BILLING_PARTY 0x0204) ─────────────────────
    ("Customer", class_ids::BILLING_PARTY),
    ("Kunde", class_ids::BILLING_PARTY),
    // ── Commercial document (COMMERCIAL_DOCUMENT 0x0202) ─────────
    // `Vorgang` is WoA's umbrella for the Quote/Order/Invoice/CreditNote
    // family (same SQL row, kind enum discriminates). All English and
    // German specializations collapse to one canonical id.
    ("Vorgang", class_ids::COMMERCIAL_DOCUMENT),
    ("WorkOrder", class_ids::COMMERCIAL_DOCUMENT),
    ("Quote", class_ids::COMMERCIAL_DOCUMENT),
    ("Angebot", class_ids::COMMERCIAL_DOCUMENT),
    ("Order", class_ids::COMMERCIAL_DOCUMENT),
    ("Auftrag", class_ids::COMMERCIAL_DOCUMENT),
    ("Invoice", class_ids::COMMERCIAL_DOCUMENT),
    ("Rechnung", class_ids::COMMERCIAL_DOCUMENT),
    ("CreditNote", class_ids::COMMERCIAL_DOCUMENT),
    ("Gutschrift", class_ids::COMMERCIAL_DOCUMENT),
    ("RecurringInvoice", class_ids::COMMERCIAL_DOCUMENT),
    // ── Commercial line item (COMMERCIAL_LINE_ITEM 0x0201) ───────
    ("Position", class_ids::COMMERCIAL_LINE_ITEM),
    ("LineItem", class_ids::COMMERCIAL_LINE_ITEM),
    // ── Tax (TAX_POLICY 0x0203) ──────────────────────────────────
    ("TaxRate", class_ids::TAX_POLICY),
    ("Steuersatz", class_ids::TAX_POLICY),
    ("Tax", class_ids::TAX_POLICY),
    // ── Payment (PAYMENT_RECORD 0x0205) ──────────────────────────
    ("Payment", class_ids::PAYMENT_RECORD),
    ("Zahlung", class_ids::PAYMENT_RECORD),
    ("PaymentRecord", class_ids::PAYMENT_RECORD),
    // ── BILLABLE WORK ENTRY (0x0103) — the convergence pin ───────
    // Planner-side `TimeEntry` (OpenProject/Redmine) and ERP-side
    // `Stundenzettel` / `TimesheetActivity` (WoA) resolve to the
    // SAME canonical id. This is the operator's value statement
    // realised as data: zero-translation flow between planner hours
    // and billable hours.
    ("Stundenzettel", class_ids::BILLABLE_WORK_ENTRY),
    ("TimesheetActivity", class_ids::BILLABLE_WORK_ENTRY),
    ("TimeEntry", class_ids::BILLABLE_WORK_ENTRY),
    ("Zeiterfassung", class_ids::BILLABLE_WORK_ENTRY),
];

// ── SMB (small-and-medium-business German office ERP) port ──────────

/// SMB-office-rs's `PortSpec` — maps the `SMB` namespace's public
/// names (Kunde / Auftrag / Rechnung / Stundenzettel / …) onto the
/// shared OGAR codebook.
///
/// Sister of [`WoaPort`]: both ports cover overlapping commerce +
/// billable-hours surfaces with German-vendor vocabularies. SMB's
/// `Kunde` and WoA's `Kunde` both resolve to
/// [`class_ids::BILLING_PARTY`]; SMB's `Stundenzettel` and WoA's
/// `Stundenzettel` and OpenProject's `TimeEntry` all resolve to
/// [`class_ids::BILLABLE_WORK_ENTRY`]. The convergence pin lives in
/// [`tests::time_entry_converges_across_planner_and_erp_ports`].
///
/// SMB-specific concepts that don't yet have a canonical class_id
/// (Artikel / Product / SKU, Geschäftspartner / Lieferant, FiBu
/// account chart) are intentionally absent; adding them is an
/// extension of the `0x02XX` commerce block and a paired SmbPort
/// alias entry. Until then `SmbPort::class_id("Artikel")` returns
/// `None` and the consumer's `bridge.entity()` call falls through
/// to the OGIT registry-resolution path (so a TTL-hydrated concept
/// still resolves; the codebook synthesis just doesn't kick in).
pub struct SmbPort;

impl PortSpec for SmbPort {
    const NAMESPACE: &'static str = "SMB";
    const BRIDGE_ID: &'static str = "smb";
    /// `0x0004` — SMB-Office render prefix (§2 allocation table).
    const APP_PREFIX: u16 = 0x0004;
    fn aliases() -> &'static [(&'static str, u16)] {
        SMB_ALIASES
    }
}

/// The SMB port's `(public_name, class_id)` alias slice. German and
/// English synonyms collapse to one canonical class_id, matching the
/// WoaPort approach so both German-SMB consumers route through one
/// shared codebook.
pub const SMB_ALIASES: &[(&str, u16)] = &[
    // ── Billing party (BILLING_PARTY 0x0204) ─────────────────────
    ("Kunde", class_ids::BILLING_PARTY),
    ("Customer", class_ids::BILLING_PARTY),
    // ── Commercial document (COMMERCIAL_DOCUMENT 0x0202) ─────────
    ("Auftrag", class_ids::COMMERCIAL_DOCUMENT),
    ("Order", class_ids::COMMERCIAL_DOCUMENT),
    ("Rechnung", class_ids::COMMERCIAL_DOCUMENT),
    ("Invoice", class_ids::COMMERCIAL_DOCUMENT),
    ("Angebot", class_ids::COMMERCIAL_DOCUMENT),
    ("Quote", class_ids::COMMERCIAL_DOCUMENT),
    ("Gutschrift", class_ids::COMMERCIAL_DOCUMENT),
    ("CreditNote", class_ids::COMMERCIAL_DOCUMENT),
    // ── Commercial line item (COMMERCIAL_LINE_ITEM 0x0201) ───────
    ("Position", class_ids::COMMERCIAL_LINE_ITEM),
    ("LineItem", class_ids::COMMERCIAL_LINE_ITEM),
    // ── Tax (TAX_POLICY 0x0203) ──────────────────────────────────
    ("Steuer", class_ids::TAX_POLICY),
    ("Tax", class_ids::TAX_POLICY),
    ("Steuersatz", class_ids::TAX_POLICY),
    // ── Payment (PAYMENT_RECORD 0x0205) ──────────────────────────
    ("Zahlung", class_ids::PAYMENT_RECORD),
    ("Payment", class_ids::PAYMENT_RECORD),
    // ── BILLABLE WORK ENTRY (0x0103) — the convergence pin ───────
    // Same convergence as WoA: SMB `Stundenzettel` ≡ OpenProject
    // `TimeEntry` ≡ Redmine `TimeEntry` ≡ WoA `Stundenzettel`.
    ("Stundenzettel", class_ids::BILLABLE_WORK_ENTRY),
    ("TimeEntry", class_ids::BILLABLE_WORK_ENTRY),
    ("Zeiterfassung", class_ids::BILLABLE_WORK_ENTRY),
];

// ── Odoo (odoo-rs) port ─────────────────────────────────────────────

/// Odoo's `PortSpec` — maps Odoo model names (`account.move`,
/// `account.move.line`, `account.tax`, …) onto the canonical OGAR
/// **commerce** codebook (`0x02XX`), plus the one cross-arm bridge to
/// the project domain (`account.analytic.line` → `billable_work_entry`,
/// `0x0103`).
///
/// This is the commerce-arm sibling of [`OpenProjectPort`] /
/// [`RedminePort`] — Northstar plan §7 T10 ("convergence proof for the
/// commerce arm … mirroring the project-mgmt arm, same shape, different
/// domain"). Odoo's `account.*` ERP models are the first commerce
/// curator on the codebook; a second (OSB) maps onto these same
/// `class_ids::*` to complete the apple-meets-apple pin for commerce.
///
/// **Why this matters (severity).** `odoo-rs` currently lowers Odoo's
/// ontology through a *bespoke* SurrealQL AST + triple pipeline that
/// forks `op-surreal-ast` / `ogar-adapter-surrealql` and never touches
/// `ogar-vocab`. OGAR exists precisely to own the AR-shaped Class / AST
/// / ClassView surface; this port is the beachhead that lets `odoo-rs`
/// converge onto the canonical layer (lower onto `ogar_vocab::Class`,
/// emit via `ogar-adapter-surrealql`) instead of re-deriving it.
pub struct OdooPort;

impl PortSpec for OdooPort {
    const NAMESPACE: &'static str = "Odoo";
    const BRIDGE_ID: &'static str = "odoo";
    /// `0x0002` — Odoo render prefix (§2 allocation table).
    const APP_PREFIX: u16 = 0x0002;
    fn aliases() -> &'static [(&'static str, u16)] {
        ODOO_ALIASES
    }
}

/// Odoo model name → canonical class_id. Commerce-arm core
/// (`account.*` + `res.*`) plus the `account.analytic.line` cross-arm
/// bridge. `pub` for symmetry with the other `*_ALIASES`.
///
/// `account.move` is Odoo's journal-entry / invoice model — the posted
/// commercial document (`od-ontology`'s "Slice 1" target). `sale.order`
/// is the upstream quote/order shape; it also converges on
/// `commercial_document` (many curator models → one canonical concept,
/// same as OpenProject `Status` / Redmine `IssueStatus` → `project_status`).
pub const ODOO_ALIASES: &[(&str, u16)] = &[
    // Commerce arm (0x02XX).
    ("account.move", class_ids::COMMERCIAL_DOCUMENT),
    ("sale.order", class_ids::COMMERCIAL_DOCUMENT),
    ("account.move.line", class_ids::COMMERCIAL_LINE_ITEM),
    ("sale.order.line", class_ids::COMMERCIAL_LINE_ITEM),
    ("account.tax", class_ids::TAX_POLICY),
    ("res.partner", class_ids::BILLING_PARTY),
    ("account.payment", class_ids::PAYMENT_RECORD),
    ("res.currency", class_ids::CURRENCY_POLICY),
    // Product master record — both `product.template` (master) and
    // `product.product` (variant) converge on the same `product` id.
    // Same convergence pattern as `account.move ↔ sale.order →
    // commercial_document`. Phase-3 mint per odoo-rs PR #14 + #16.
    ("product.template", class_ids::PRODUCT),
    ("product.product", class_ids::PRODUCT),
    // General-ledger account — `account.account` (live row) and
    // `account.account.template` (SKR03/04 chart concept) converge on the
    // same `accounting_account` id. Phase-3 mint per odoo-rs PR #14 + #16.
    ("account.account", class_ids::ACCOUNTING_ACCOUNT),
    ("account.account.template", class_ids::ACCOUNTING_ACCOUNT),
    // ProductCatalog cluster — pricing structure + UoM, Phase-3 per
    // odoo-rs PR #14.
    ("product.pricelist", class_ids::PRICELIST),
    ("product.pricelist.item", class_ids::PRICELIST_RULE),
    ("uom.uom", class_ids::UNIT_OF_MEASURE),
    // HR cluster — closes the final 4-of-11 cross-axis identity gap surfaced
    // by odoo-rs PR #14. New 0x0DXX concept domain (HR).
    ("hr.employee", class_ids::HR_EMPLOYEE),
    ("hr.department", class_ids::HR_DEPARTMENT),
    ("hr.job", class_ids::HR_JOB),
    ("hr.contract", class_ids::HR_EMPLOYMENT_CONTRACT),
    // Cross-arm bridge: the timesheet / cost line converges on the
    // project-arm `billable_work_entry` (0x0103) — the SAME id
    // OpenProject `TimeEntry` and Redmine `TimeEntry` resolve to.
    // See `billable_work_entry`'s doc: "OpenProject TimeEntry, Redmine
    // TimeEntry, Odoo account.analytic.line all converge here."
    ("account.analytic.line", class_ids::BILLABLE_WORK_ENTRY),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openproject_namespace_and_bridge_id_match_canonical_strings() {
        assert_eq!(OpenProjectPort::NAMESPACE, "OpenProject");
        assert_eq!(OpenProjectPort::BRIDGE_ID, "openproject");
    }

    #[test]
    fn redmine_namespace_and_bridge_id_match_canonical_strings() {
        assert_eq!(RedminePort::NAMESPACE, "Redmine");
        assert_eq!(RedminePort::BRIDGE_ID, "redmine");
    }

    #[test]
    fn healthcare_namespace_and_bridge_id_match_canonical_strings() {
        assert_eq!(HealthcarePort::NAMESPACE, "Healthcare");
        assert_eq!(HealthcarePort::BRIDGE_ID, "medcare");
    }

    #[test]
    fn healthcare_entities_resolve_into_the_health_domain() {
        use crate::{ConceptDomain, canonical_concept_domain};
        for &(name, _) in HealthcarePort::aliases() {
            let id =
                HealthcarePort::class_id(name).unwrap_or_else(|| panic!("`{name}` must resolve"));
            assert_eq!(
                canonical_concept_domain(id),
                ConceptDomain::Health,
                "`{name}` -> 0x{id:04X} must live in the Health (0x09XX) domain",
            );
        }
        assert_eq!(
            HealthcarePort::class_id("Patient"),
            Some(class_ids::PATIENT)
        );
        assert_eq!(HealthcarePort::class_id("Patient"), Some(0x0901));
    }

    #[test]
    fn healthcare_alias_count_matches_ogit_entities() {
        // Patient / Diagnosis / LabValue / Medication / Treatment / Visit
        // / VitalSign — the 7 OGIT `NTO/Healthcare/entities/` classes.
        assert_eq!(
            HealthcarePort::aliases().len(),
            7,
            "Healthcare alias count drift — re-count against OGIT entities"
        );
    }

    #[test]
    fn healthcare_unknown_public_names_resolve_to_none() {
        assert_eq!(HealthcarePort::class_id("WorkPackage"), None);
        assert_eq!(HealthcarePort::class_id(""), None);
    }

    #[test]
    fn openproject_workpackage_maps_to_project_work_item() {
        assert_eq!(
            OpenProjectPort::class_id("WorkPackage"),
            Some(class_ids::PROJECT_WORK_ITEM)
        );
        assert_eq!(OpenProjectPort::class_id("WorkPackage"), Some(0x0102));
    }

    #[test]
    fn redmine_issue_maps_to_project_work_item() {
        assert_eq!(
            RedminePort::class_id("Issue"),
            Some(class_ids::PROJECT_WORK_ITEM)
        );
        assert_eq!(RedminePort::class_id("Issue"), Some(0x0102));
    }

    /// Headline cross-fork convergence pin: every concept pair below
    /// resolves to the SAME `class_ids::*` constant via both ports'
    /// `class_id()` resolvers. Drift here would re-introduce the codex
    /// P1 bug on PR #559 (distinct entity_type_ids for converging
    /// canonical concepts).
    #[test]
    fn openproject_and_redmine_converge_on_shared_concepts() {
        let pairs: &[(&str, &str, u16)] = &[
            ("Project", "Project", class_ids::PROJECT),
            ("WorkPackage", "Issue", class_ids::PROJECT_WORK_ITEM),
            ("TimeEntry", "TimeEntry", class_ids::BILLABLE_WORK_ENTRY),
            // STI fold (PROJECT_ACTOR): User / Principal / Group all
            // share the same id in both ports.
            ("User", "User", class_ids::PROJECT_ACTOR),
            ("Principal", "Principal", class_ids::PROJECT_ACTOR),
            ("Group", "Group", class_ids::PROJECT_ACTOR),
            ("Status", "IssueStatus", class_ids::PROJECT_STATUS),
            ("Type", "Tracker", class_ids::PROJECT_TYPE),
            ("IssuePriority", "IssuePriority", class_ids::PRIORITY),
            // Both forks ship the membership join as `Member` (engine-walking
            // corpus snapshot). The OpenProject port still carries the legacy
            // `Membership` synonym; the canonical pair is now Member ↔ Member.
            ("Member", "Member", class_ids::PROJECT_MEMBERSHIP),
            ("Journal", "Journal", class_ids::PROJECT_JOURNAL),
            ("Repository", "Repository", class_ids::PROJECT_REPOSITORY),
            ("Version", "Version", class_ids::PROJECT_VERSION),
            ("WikiPage", "WikiPage", class_ids::PROJECT_WIKI_PAGE),
            ("Query", "Query", class_ids::PROJECT_QUERY),
            ("Attachment", "Attachment", class_ids::PROJECT_ATTACHMENT),
            (
                "CustomField",
                "CustomField",
                class_ids::PROJECT_CUSTOM_FIELD,
            ),
            ("Relation", "IssueRelation", class_ids::PROJECT_RELATION),
            ("Changeset", "Changeset", class_ids::PROJECT_CHANGESET),
            ("Watcher", "Watcher", class_ids::PROJECT_WATCHER),
            ("News", "News", class_ids::PROJECT_NEWS),
            ("Message", "Message", class_ids::PROJECT_MESSAGE),
            ("Forum", "Board", class_ids::PROJECT_FORUM),
            ("Role", "Role", class_ids::PROJECT_ROLE),
            ("MemberRole", "MemberRole", class_ids::PROJECT_MEMBER_ROLE),
            (
                "CustomValue",
                "CustomValue",
                class_ids::PROJECT_CUSTOM_VALUE,
            ),
            (
                "EnabledModule",
                "EnabledModule",
                class_ids::PROJECT_ENABLED_MODULE,
            ),
        ];
        for &(op_name, rm_name, expected) in pairs {
            let op = OpenProjectPort::class_id(op_name);
            let rm = RedminePort::class_id(rm_name);
            assert_eq!(
                op,
                Some(expected),
                "OpenProjectPort `{op_name}` should map to 0x{expected:04X}",
            );
            assert_eq!(
                rm,
                Some(expected),
                "RedminePort `{rm_name}` should map to 0x{expected:04X}",
            );
            assert_eq!(
                op, rm,
                "convergence broken: OpenProject `{op_name}` ↔ Redmine `{rm_name}`",
            );
        }
    }

    /// OpenProject ships the membership join as `Member` (mirrors Redmine —
    /// both engine-walking corpus snapshots carry that name). The earlier
    /// `Membership` surface stays as a deprecated synonym so any consumer
    /// holding the old name still resolves; this test pins both routes to
    /// the same canonical id so the additive contract can't drift.
    ///
    /// Closes the openproject-nexgen-rs#56 pinned
    /// `port_and_snapshot_membership_vocab_mismatch_is_known` test — once
    /// this lands and op-canon bumps its `ogar-vocab` git pin,
    /// `OpenProjectPort::class_id("Member")` flips from `None` to
    /// `Some(PROJECT_MEMBERSHIP)`, that pin self-fails, and the consumer
    /// drops it.
    #[test]
    fn openproject_member_and_membership_both_resolve_to_project_membership() {
        let target = Some(class_ids::PROJECT_MEMBERSHIP);
        // Canonical surface (matches the OpenProject corpus + Redmine):
        assert_eq!(OpenProjectPort::class_id("Member"), target);
        // Deprecated synonym kept for backward compatibility:
        assert_eq!(OpenProjectPort::class_id("Membership"), target);
        // Both ports converge under the same canonical surface name now:
        assert_eq!(RedminePort::class_id("Member"), target);
        assert_eq!(
            OpenProjectPort::class_id("Member"),
            RedminePort::class_id("Member"),
            "OP `Member` and RM `Member` must converge on the same id",
        );
    }

    #[test]
    fn unknown_public_names_resolve_to_none() {
        assert_eq!(OpenProjectPort::class_id("NotAConcept"), None);
        assert_eq!(RedminePort::class_id("NotAConcept"), None);
        assert_eq!(OpenProjectPort::class_id(""), None);
        assert_eq!(RedminePort::class_id(""), None);
    }

    #[test]
    fn each_alias_class_id_is_in_the_codebook() {
        // Every class_id in the alias tables must be a real codebook
        // entry — drift between the OpenProject/Redmine port aliases
        // and `class_ids::ALL` is a P1.
        let codebook_ids: Vec<u16> = class_ids::ALL.iter().map(|(_, id)| *id).collect();
        for &(name, id) in OpenProjectPort::aliases() {
            assert!(
                codebook_ids.contains(&id),
                "OpenProjectPort alias `{name}` -> 0x{id:04X} not in class_ids::ALL"
            );
        }
        for &(name, id) in RedminePort::aliases() {
            assert!(
                codebook_ids.contains(&id),
                "RedminePort alias `{name}` -> 0x{id:04X} not in class_ids::ALL"
            );
        }
        for &(name, id) in HealthcarePort::aliases() {
            assert!(
                codebook_ids.contains(&id),
                "HealthcarePort alias `{name}` -> 0x{id:04X} not in class_ids::ALL"
            );
        }
        for &(name, id) in WoaPort::aliases() {
            assert!(
                codebook_ids.contains(&id),
                "WoaPort alias `{name}` -> 0x{id:04X} not in class_ids::ALL"
            );
        }
        for &(name, id) in SmbPort::aliases() {
            assert!(
                codebook_ids.contains(&id),
                "SmbPort alias `{name}` -> 0x{id:04X} not in class_ids::ALL"
            );
        }
        for &(name, id) in OdooPort::aliases() {
            assert!(
                codebook_ids.contains(&id),
                "OdooPort alias `{name}` -> 0x{id:04X} not in class_ids::ALL"
            );
        }
    }

    #[test]
    fn woa_namespace_and_bridge_id_match_canonical_strings() {
        assert_eq!(WoaPort::NAMESPACE, "WorkOrder");
        assert_eq!(WoaPort::BRIDGE_ID, "woa");
    }

    #[test]
    fn smb_namespace_and_bridge_id_match_canonical_strings() {
        assert_eq!(SmbPort::NAMESPACE, "SMB");
        assert_eq!(SmbPort::BRIDGE_ID, "smb");
    }

    /// The operator's value statement realised as a test (2026-06-21):
    /// "in the end planning (openproject) and ERP (odoo, YOU) should
    /// become reusable ontologies so that the planner times can align
    /// with billable hours." Every port's time-tracking public name(s)
    /// — `TimeEntry` (planner), `Stundenzettel` (ERP, both languages),
    /// `TimesheetActivity`, `Zeiterfassung` — must resolve to ONE
    /// canonical `class_ids::BILLABLE_WORK_ENTRY = 0x0103`. Drift here
    /// reintroduces the manual translation layer the codebook exists
    /// to eliminate.
    #[test]
    fn time_entry_converges_across_planner_and_erp_ports() {
        let target = class_ids::BILLABLE_WORK_ENTRY;
        // Planner side (OpenProject + Redmine).
        assert_eq!(OpenProjectPort::class_id("TimeEntry"), Some(target));
        assert_eq!(RedminePort::class_id("TimeEntry"), Some(target));
        // ERP side (WoA + SMB) — both German and English public names
        // collapse to the same id, so the planner→ERP integration is
        // a codebook lookup, not a translation layer.
        for name in [
            "Stundenzettel",
            "TimesheetActivity",
            "TimeEntry",
            "Zeiterfassung",
        ] {
            assert_eq!(
                WoaPort::class_id(name),
                Some(target),
                "WoaPort `{name}` must resolve to BILLABLE_WORK_ENTRY (planner-ERP convergence)",
            );
        }
        for name in ["Stundenzettel", "TimeEntry", "Zeiterfassung"] {
            assert_eq!(
                SmbPort::class_id(name),
                Some(target),
                "SmbPort `{name}` must resolve to BILLABLE_WORK_ENTRY (planner-ERP convergence)",
            );
        }
    }

    /// Commerce-block convergence: WoA's `Customer` / `Kunde` and SMB's
    /// `Kunde` / `Customer` resolve to the SAME canonical
    /// `class_ids::BILLING_PARTY = 0x0204`. Same shape for `Position`,
    /// `Invoice` / `Rechnung`, `Tax` / `Steuer`, `Payment` / `Zahlung`.
    /// Two German-SMB ERP forks converging on one codebook is exactly
    /// the OpenProject ↔ Redmine apple-meets-apple pattern.
    #[test]
    fn woa_and_smb_converge_on_commerce_block() {
        let pairs: &[(&str, &str, u16)] = &[
            ("Customer", "Customer", class_ids::BILLING_PARTY),
            ("Kunde", "Kunde", class_ids::BILLING_PARTY),
            ("Invoice", "Invoice", class_ids::COMMERCIAL_DOCUMENT),
            ("Rechnung", "Rechnung", class_ids::COMMERCIAL_DOCUMENT),
            ("Order", "Order", class_ids::COMMERCIAL_DOCUMENT),
            ("Auftrag", "Auftrag", class_ids::COMMERCIAL_DOCUMENT),
            ("Quote", "Quote", class_ids::COMMERCIAL_DOCUMENT),
            ("Angebot", "Angebot", class_ids::COMMERCIAL_DOCUMENT),
            ("CreditNote", "CreditNote", class_ids::COMMERCIAL_DOCUMENT),
            ("Gutschrift", "Gutschrift", class_ids::COMMERCIAL_DOCUMENT),
            ("Position", "Position", class_ids::COMMERCIAL_LINE_ITEM),
            ("LineItem", "LineItem", class_ids::COMMERCIAL_LINE_ITEM),
            ("Tax", "Tax", class_ids::TAX_POLICY),
            ("Steuersatz", "Steuersatz", class_ids::TAX_POLICY),
            ("Payment", "Payment", class_ids::PAYMENT_RECORD),
            ("Zahlung", "Zahlung", class_ids::PAYMENT_RECORD),
        ];
        for &(woa_name, smb_name, expected) in pairs {
            let woa = WoaPort::class_id(woa_name);
            let smb = SmbPort::class_id(smb_name);
            assert_eq!(
                woa,
                Some(expected),
                "WoaPort `{woa_name}` must map to 0x{expected:04X}",
            );
            assert_eq!(
                smb,
                Some(expected),
                "SmbPort `{smb_name}` must map to 0x{expected:04X}",
            );
            assert_eq!(
                woa, smb,
                "convergence broken: WoA `{woa_name}` ↔ SMB `{smb_name}`",
            );
        }
    }

    #[test]
    fn woa_and_smb_unknown_public_names_resolve_to_none() {
        assert_eq!(WoaPort::class_id("NotAConcept"), None);
        assert_eq!(SmbPort::class_id("NotAConcept"), None);
        assert_eq!(WoaPort::class_id(""), None);
        assert_eq!(SmbPort::class_id(""), None);
        // Artikel / Product / SKU is intentionally absent — needs a
        // codebook extension; until then falls through.
        assert_eq!(SmbPort::class_id("Artikel"), None);
        assert_eq!(SmbPort::class_id("Product"), None);
    }

    #[test]
    fn each_port_has_expected_alias_count() {
        // Asymmetric by design — each port carries exactly the Rails
        // classes its corpus ships, no phantom aliases for concepts
        // the port doesn't expose as a top-level model.
        //
        // OpenProject (28): 25 distinct concept entries + 2 STI-fold
        //   rows (Principal, Group fold into PROJECT_ACTOR alongside
        //   User) + 1 deprecated synonym row (Membership → Member; both
        //   resolve to PROJECT_MEMBERSHIP, the canonical surface is
        //   Member per the engine-walking corpus snapshot). No `Comment`
        //   entry — OpenProject's Journal carries the comment-equivalent
        //   state, no standalone Comment model.
        // Redmine (28): 26 distinct concept entries + 2 STI-fold rows.
        //   Has a standalone `Comment` model on top of `Journal` (the
        //   one extra row vs OpenProject's canonical concepts).
        //
        // Both gained the same +2 STI-fold rows and +0/+1 IssuePriority
        // entry under codex P2 on PR #87 (Redmine previously had no
        // priority entry; OpenProject's was misnamed `Priority`).
        assert_eq!(
            OpenProjectPort::aliases().len(),
            28,
            "OpenProject alias count drift — re-count the table"
        );
        assert_eq!(
            RedminePort::aliases().len(),
            28,
            "Redmine alias count drift — re-count the table"
        );
        // WoA (26): 2 BillingParty (Customer/Kunde) + 11 CommercialDocument
        //   (Vorgang umbrella + Quote/Angebot, Order/Auftrag, Invoice/
        //   Rechnung, CreditNote/Gutschrift, RecurringInvoice) + 2
        //   LineItem (Position, LineItem) + 3 TaxPolicy (TaxRate /
        //   Steuersatz / Tax) + 3 PaymentRecord (Payment/Zahlung/
        //   PaymentRecord) + 4 BillableWorkEntry (Stundenzettel /
        //   TimesheetActivity / TimeEntry / Zeiterfassung). Drift here
        //   means a public-name was added/removed from the alias table.
        assert_eq!(
            WoaPort::aliases().len(),
            25,
            "WoA alias count drift — re-count the table"
        );
        // SMB (20): 2 BillingParty (Kunde/Customer) + 8 CommercialDocument
        //   (Auftrag/Order, Rechnung/Invoice, Angebot/Quote, Gutschrift/
        //   CreditNote) + 2 LineItem (Position, LineItem) + 3 TaxPolicy
        //   (Steuer / Tax / Steuersatz) + 2 PaymentRecord (Zahlung /
        //   Payment) + 3 BillableWorkEntry (Stundenzettel / TimeEntry /
        //   Zeiterfassung). SMB omits some WoA-only synonyms (Vorgang
        //   umbrella, RecurringInvoice, TimesheetActivity) — those are
        //   WoA-specific surface concepts the SMB consumer doesn't need.
        assert_eq!(
            SmbPort::aliases().len(),
            20,
            "SMB alias count drift — re-count the table"
        );
    }

    #[test]
    fn sti_actor_fold_routes_user_principal_group_to_project_actor() {
        // Codex P2 on PR #87: User/Principal/Group all map to the same
        // codebook id in both ports.
        for name in ["User", "Principal", "Group"] {
            assert_eq!(
                OpenProjectPort::class_id(name),
                Some(class_ids::PROJECT_ACTOR),
                "OpenProjectPort `{name}` should fold into PROJECT_ACTOR",
            );
            assert_eq!(
                RedminePort::class_id(name),
                Some(class_ids::PROJECT_ACTOR),
                "RedminePort `{name}` should fold into PROJECT_ACTOR",
            );
        }
    }

    #[test]
    fn issue_priority_resolves_via_actual_rails_class_name() {
        // Codex P2 on PR #87: both ports expose IssuePriority (not
        // the canonical-concept name `Priority`) as the Rails class.
        assert_eq!(
            OpenProjectPort::class_id("IssuePriority"),
            Some(class_ids::PRIORITY)
        );
        assert_eq!(
            RedminePort::class_id("IssuePriority"),
            Some(class_ids::PRIORITY)
        );
    }

    // ── Odoo (commerce arm + the planning↔ERP bridge) ───────────────

    #[test]
    fn odoo_namespace_and_bridge_id_match_canonical_strings() {
        assert_eq!(OdooPort::NAMESPACE, "Odoo");
        assert_eq!(OdooPort::BRIDGE_ID, "odoo");
    }

    #[test]
    fn odoo_account_move_maps_to_commercial_document() {
        assert_eq!(
            OdooPort::class_id("account.move"),
            Some(class_ids::COMMERCIAL_DOCUMENT)
        );
        assert_eq!(OdooPort::class_id("account.move"), Some(0x0202));
    }

    #[test]
    fn odoo_commerce_models_resolve_into_the_commerce_domain() {
        use crate::{ConceptDomain, canonical_concept_domain};
        // Every commerce-arm alias lands in the Commerce (0x02XX) domain.
        // Two deliberate exceptions: `account.analytic.line` is the cross-arm
        // bridge into the project domain, and the `hr.*` cluster lives in
        // the HR domain (0x0DXX) — both are asserted in their own tests
        // (`account_analytic_line_resolves_into_the_project_domain` /
        // `odoo_hr_models_resolve_into_the_hr_domain`).
        for &(name, _) in OdooPort::aliases() {
            if name == "account.analytic.line" || name.starts_with("hr.") {
                continue;
            }
            let id = OdooPort::class_id(name).unwrap_or_else(|| panic!("`{name}` must resolve"));
            assert_eq!(
                canonical_concept_domain(id),
                ConceptDomain::Commerce,
                "`{name}` -> 0x{id:04X} must live in the Commerce (0x02XX) domain",
            );
        }
    }

    #[test]
    fn odoo_hr_models_resolve_into_the_hr_domain() {
        use crate::{ConceptDomain, canonical_concept_domain};
        // The hr.* cluster (HR_EMPLOYEE / HR_DEPARTMENT / HR_JOB /
        // HR_EMPLOYMENT_CONTRACT) lands in the HR (0x0DXX) domain — closes
        // the final 4-of-11 cross-axis identity gap from odoo-rs PR #14.
        for name in ["hr.employee", "hr.department", "hr.job", "hr.contract"] {
            let id = OdooPort::class_id(name).unwrap_or_else(|| panic!("`{name}` must resolve"));
            assert_eq!(
                canonical_concept_domain(id),
                ConceptDomain::HR,
                "`{name}` -> 0x{id:04X} must live in the HR (0x0DXX) domain",
            );
        }
    }

    /// **The planning ↔ ERP convergence pin.** A logged unit of work is
    /// one canonical concept — `billable_work_entry` (0x0103) — whether
    /// it arrives as an OpenProject `TimeEntry`, a Redmine `TimeEntry`,
    /// or an Odoo `account.analytic.line` (the ERP timesheet/cost line).
    /// This is the codebook's named "first cross-domain bridge": the
    /// planning arm (work performed) and the commerce arm (work billed)
    /// meet here. Drift on any side splits a hour-logged-in-planning from
    /// the-same-hour-billed-in-ERP.
    #[test]
    fn planning_and_erp_converge_on_billable_work_entry() {
        let op = OpenProjectPort::class_id("TimeEntry");
        let rm = RedminePort::class_id("TimeEntry");
        let odoo = OdooPort::class_id("account.analytic.line");
        assert_eq!(op, Some(class_ids::BILLABLE_WORK_ENTRY));
        assert_eq!(rm, Some(class_ids::BILLABLE_WORK_ENTRY));
        assert_eq!(odoo, Some(class_ids::BILLABLE_WORK_ENTRY));
        assert_eq!(
            op, odoo,
            "OpenProject TimeEntry ↔ Odoo analytic line must converge"
        );
        assert_eq!(
            rm, odoo,
            "Redmine TimeEntry ↔ Odoo analytic line must converge"
        );
        assert_eq!(odoo, Some(0x0103));
    }

    #[test]
    fn odoo_alias_count_is_stable() {
        // 9 Odoo model aliases = 8 commerce-arm (account.move,
        // sale.order, account.move.line, sale.order.line, account.tax,
        // res.partner, account.payment, res.currency) + 4 product/accounting
        // master-record aliases + 3 ProductCatalog cluster aliases + 4 HR cluster aliases (product.template, product.product,
        // account.account, account.account.template — Phase-3 mints per
        // odoo-rs PR #14 + #16) + 1 cross-arm bridge
        // (account.analytic.line → billable_work_entry). Re-count on drift.
        assert_eq!(
            OdooPort::aliases().len(),
            20,
            "Odoo alias count drift — re-count the ODOO_ALIASES table",
        );
    }

    #[test]
    fn odoo_unknown_model_names_resolve_to_none() {
        assert_eq!(OdooPort::class_id("ir.cron"), None);
        assert_eq!(OdooPort::class_id("WorkPackage"), None);
        assert_eq!(OdooPort::class_id(""), None);
    }

    /// Pins all six APP_PREFIX overrides against the §2 allocation table
    /// in `APP-CLASS-CODEBOOK-LAYOUT.md`. The default in the trait is
    /// `0x0000` (shared canonical core); each app port overrides with its
    /// reserved high-u16 so consumers can re-export the typed constant
    /// instead of hardcoding hex literals.
    ///
    /// Reserving a prefix costs nothing (§2): no codebook is materialised
    /// until the app mints its first private class. This test is asserting
    /// the allocation table as typed data, not minting class_ids.
    #[test]
    fn app_prefixes_match_the_allocation_table() {
        // § 2 table rows that have a PortSpec impl:
        assert_eq!(
            OpenProjectPort::APP_PREFIX,
            0x0001,
            "OpenProject prefix must be 0x0001"
        );
        assert_eq!(OdooPort::APP_PREFIX, 0x0002, "Odoo prefix must be 0x0002");
        assert_eq!(WoaPort::APP_PREFIX, 0x0003, "WoA prefix must be 0x0003");
        assert_eq!(
            SmbPort::APP_PREFIX,
            0x0004,
            "SMB-Office prefix must be 0x0004"
        );
        assert_eq!(
            HealthcarePort::APP_PREFIX,
            0x0005,
            "Healthcare/Medcare prefix must be 0x0005"
        );
        assert_eq!(
            RedminePort::APP_PREFIX,
            0x0007,
            "Redmine prefix must be 0x0007"
        );
        // The trait default (0x0000 = shared core) is expressed directly
        // in the trait definition; ports that do not override it resolve
        // to the core codebook namespace, which is the bootstrap/core
        // prefix per §2 ("hi = 0x0000 is the bootstrap/core prefix").
    }

    /// **The five-way `billable_work_entry` convergence pin.** Ratifies
    /// the `APP-CODEBOOK-MIGRATION-PLAN.md` W0 + W1 + W2 + W3 worked
    /// tables: five port-bearing apps all resolve their *own* timesheet
    /// surface name to one canonical concept ([`class_ids::BILLABLE_WORK_ENTRY`]
    /// = `0x0103`). Five renderers, one concept — the planning ⟷ ERP
    /// convergence as a single codebook entry, machine-checked:
    ///
    /// | app | surface name | port |
    /// |---|---|---|
    /// | OpenProject (`0x0001`) | `TimeEntry` | [`OpenProjectPort`] |
    /// | Odoo (`0x0002`) | `account.analytic.line` | [`OdooPort`] |
    /// | WoA (`0x0003`) | `Stundenzettel` | [`WoaPort`] |
    /// | SMB (`0x0004`) | `Stundenzettel` | [`SmbPort`] |
    /// | Redmine (`0x0007`) | `TimeEntry` | [`RedminePort`] |
    ///
    /// Earlier tests pinned subsets (the four-way planner+ERP test, the
    /// three-way OpenProject ↔ Redmine ↔ Odoo test); this is the full
    /// fan-out the migration plan calls "planner times == billable
    /// hours", asserted in one place so drift on ANY of the five ports
    /// fails CI here.
    #[test]
    fn billable_work_entry_converges_across_all_five_ports() {
        let target = class_ids::BILLABLE_WORK_ENTRY;
        assert_eq!(
            target, 0x0103,
            "codebook id for billable_work_entry must be 0x0103"
        );
        let resolutions: &[(&str, &str, Option<u16>)] = &[
            (
                "OpenProject",
                "TimeEntry",
                OpenProjectPort::class_id("TimeEntry"),
            ),
            (
                "Odoo",
                "account.analytic.line",
                OdooPort::class_id("account.analytic.line"),
            ),
            ("WoA", "Stundenzettel", WoaPort::class_id("Stundenzettel")),
            ("SMB", "Stundenzettel", SmbPort::class_id("Stundenzettel")),
            ("Redmine", "TimeEntry", RedminePort::class_id("TimeEntry")),
        ];
        for &(app, name, got) in resolutions {
            assert_eq!(
                got,
                Some(target),
                "{app}Port::class_id(\"{name}\") must resolve to BILLABLE_WORK_ENTRY \
                 (0x{target:04X}) — the planner⟷ERP convergence pin from \
                 APP-CODEBOOK-MIGRATION-PLAN.md W0+W1+W2+W3",
            );
        }
    }
}
