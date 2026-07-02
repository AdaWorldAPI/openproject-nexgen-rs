//! The Odoo savant roster — 25 delegated reasoners (lanes L1–L15).
//!
//! Source of truth: `.claude/odoo/SAVANTS.md` (+ the L1–L15 lane drafts), the
//! woa-rs → lance-graph delegation harvest. Each **Savant** is a delegated
//! reasoner: woa-rs keeps the deterministic guard (AXIS-A); the ambiguous,
//! evidence-weighted core (AXIS-B) is delegated here via
//! [`crate::reasoning::Reasoner`] + [`crate::reasoning::ReasoningContext`].
//!
//! This module is the **roster spine**: the 25 savants as data + their dispatch
//! tuple (OGIT family · [`ReasoningKind`] · [`InferenceType`] · [`SemiringChoice`]
//! · [`StyleCluster`]). The tuple fully determines runtime dispatch
//! (`InferenceType::default_strategy()` → `QueryStrategy`). The actual
//! `Reasoner` impls per `ReasoningKind`, the two new OGIT families
//! (`0x63 ProductCatalog`, `0x90 HRFoundation`), and the Layer-2 alignment
//! axioms for the `None` classes are the follow-on deliverables tracked in
//! `.claude/plans/odoo-savant-roster-v1.md`.

use crate::nars::{InferenceType, SemiringChoice};
use crate::reasoning::ReasoningKind;
use crate::thinking::StyleCluster;

/// Stable codes for the `ReasoningKind::Other(u32)` savants (SAVANTS.md).
pub mod other_kind {
    pub const PRICELIST_ASSIGNMENT: u32 = 1;
    pub const CHART_ACCOUNT_MAPPING: u32 = 3;
    pub const CONSOLIDATION_RATE_POLICY: u32 = 4;
    pub const RECONCILE_MATCH: u32 = 5;
    pub const BANK_STATEMENT_MATCH: u32 = 6;
}

/// One delegated Odoo reasoner.
#[derive(Debug, Clone, Copy)]
pub struct Savant {
    /// Roster id (SAVANTS.md numbering; 16 is intentionally absent).
    pub id: u8,
    /// Savant name.
    pub name: &'static str,
    /// OGIT family (8-bit), or `None` until a Layer-2 alignment axiom lands.
    pub family: Option<u8>,
    /// The use-case the reasoner serves.
    pub kind: ReasoningKind,
    /// Inference type — selects the query strategy via `default_strategy()`.
    pub inference: InferenceType,
    /// How evidence fuses across paths.
    pub semiring: SemiringChoice,
    /// Thinking style cluster (inherited from the family).
    pub style: StyleCluster,
    /// Source lane (`.claude/odoo/L*.md`).
    pub lane: &'static str,
    /// The AXIS-B decision it makes (one line).
    pub decides: &'static str,
}

use InferenceType::*;
use ReasoningKind::*;
use SemiringChoice::*;
use StyleCluster::*;

/// The 25 Odoo savants (SAVANTS.md roster). `family = None` = needs an alignment axiom.
pub const SAVANTS: [Savant; 25] = [
    // ── L8–L15 gap lanes (15) ──
    Savant {
        id: 1,
        name: "FiscalPositionResolver",
        family: Some(0x80),
        kind: CustomerCategory,
        inference: Deduction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L9",
        decides: "which fiscal position (tax mapping) applies to a partner",
    },
    Savant {
        id: 2,
        name: "PartnerTrustAdvisor",
        family: Some(0x80),
        kind: CustomerCategory,
        inference: Revision,
        semiring: NarsTruth,
        style: Empathic,
        lane: "L9",
        decides: "partner trust / dunning-risk from payment history",
    },
    Savant {
        id: 3,
        name: "PricelistAssignmentAgent",
        family: Some(0x64),
        kind: Other(other_kind::PRICELIST_ASSIGNMENT),
        inference: Revision,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L8",
        decides: "partner pricelist when no explicit property (country-group/config fallback)",
    },
    Savant {
        id: 4,
        name: "AnalyticDistributionSuggester",
        family: Some(0x62),
        kind: NextBestAction,
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L10",
        decides: "suggested cost-centre distribution for a move line",
    },
    Savant {
        id: 5,
        name: "AnalyticModelScorer",
        family: None,
        kind: CustomerCategory,
        inference: Deduction,
        semiring: HammingMin,
        style: Analytical,
        lane: "L10",
        decides: "which analytic.distribution.model matches (priority-scored)",
    },
    Savant {
        id: 6,
        name: "SequenceGapAnomalyDetector",
        family: Some(0x62),
        kind: PostingAnomaly,
        inference: Abduction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L11",
        decides: "journal sequence gaps ⇒ deleted posted entries (GoBD)",
    },
    Savant {
        id: 7,
        name: "ExchangeAccountSelector",
        family: Some(0x62),
        kind: Other(other_kind::CHART_ACCOUNT_MAPPING),
        inference: Deduction,
        semiring: Boolean,
        style: Analytical,
        lane: "L12",
        decides: "gain/loss account for FX diff (sign-driven; config-assist)",
    },
    Savant {
        id: 8,
        name: "ReportRateTypeSelector",
        family: Some(0x62),
        kind: Other(other_kind::CONSOLIDATION_RATE_POLICY),
        inference: Deduction,
        semiring: Boolean,
        style: Analytical,
        lane: "L12",
        decides: "current/historical/average rate per report line (IFRS vs HGB)",
    },
    Savant {
        id: 9,
        name: "CurrencySelectionAdvisor",
        family: Some(0x62),
        kind: NextBestAction,
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L12",
        decides: "which currencies to enable (geography signal)",
    },
    Savant {
        id: 10,
        name: "UserCompanyAccessAdvisor",
        family: Some(0x80),
        kind: CustomerCategory,
        inference: Induction,
        semiring: NarsTruth,
        style: Empathic,
        lane: "L12",
        decides: "branch-access subset by user role/context",
    },
    Savant {
        id: 11,
        name: "ProcurementRuleSelector",
        family: None,
        kind: NextBestAction,
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L13",
        decides: "route among equal-sequence rules (lead/availability/reliability)",
    },
    Savant {
        id: 12,
        name: "ReorderTimingAdvisor",
        family: None,
        kind: NextBestAction,
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L13",
        decides: "reorder timing under demand/supplier uncertainty",
    },
    Savant {
        id: 13,
        name: "ReplenishmentReportAdvisor",
        family: None,
        kind: NextBestAction,
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L13",
        decides: "real shortfall vs demand noise in the replenishment report",
    },
    Savant {
        id: 14,
        name: "RouteTiebreaker",
        family: None,
        kind: NextBestAction,
        inference: Abduction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L13",
        decides: "equal-sequence route tiebreak (supplier lead/cost/capacity)",
    },
    Savant {
        id: 15,
        name: "TaxExigibilitySuggestor",
        family: Some(0x62),
        kind: NextBestAction,
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L15",
        decides: "tax exigibility (on-invoice vs on-payment / cash-basis)",
    },
    // ── L1–L7 original lanes (10; id 16 intentionally absent per SAVANTS.md) ──
    Savant {
        id: 17,
        name: "AutopostRecommender",
        family: Some(0x81),
        kind: PostingAnomaly,
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L1",
        decides: "recommend auto-posting bills after 3+ unmodified from a partner",
    },
    Savant {
        id: 18,
        name: "LockDateAdvancer",
        family: Some(0x81),
        kind: PostingAnomaly,
        inference: Abduction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L1",
        decides: "which next open period to advance a move into when date is locked",
    },
    Savant {
        id: 19,
        name: "ReconcileMatchSelector",
        family: None,
        kind: Other(other_kind::RECONCILE_MATCH),
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L2",
        decides: "which open items to propose as reconciliation candidates",
    },
    Savant {
        id: 20,
        name: "BankStatementMatcher",
        family: None,
        kind: Other(other_kind::BANK_STATEMENT_MATCH),
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L5",
        decides: "which reconcile-model rule matches a bank line + write-offs",
    },
    Savant {
        id: 21,
        name: "PaymentToInvoiceMatcher",
        family: None,
        kind: Other(other_kind::RECONCILE_MATCH),
        inference: Induction,
        semiring: NarsTruth,
        style: Analytical,
        lane: "L5",
        decides: "whether a payment fully reconciles open invoices (Mahnwesen gate)",
    },
    Savant {
        id: 22,
        name: "UpsellActivityTrigger",
        family: Some(0x81),
        kind: NextBestAction,
        inference: Induction,
        semiring: NarsTruth,
        style: Exploratory,
        lane: "L6",
        decides: "qty_delivered>ordered ⇒ upsell TODO for salesperson",
    },
    Savant {
        id: 23,
        name: "PricelistRecommender",
        family: Some(0x81),
        kind: NextBestAction,
        inference: Synthesis,
        semiring: NarsTruth,
        style: Exploratory,
        lane: "L6",
        decides: "which pricelist rule when multiple candidates apply",
    },
    Savant {
        id: 24,
        name: "RemovalStrategySelector",
        family: None,
        kind: NextBestAction,
        inference: Induction,
        semiring: XorBundle,
        style: Exploratory,
        lane: "L7",
        decides: "which quants to bind to a reservation (FIFO/FEFO/LIFO/closest)",
    },
    Savant {
        id: 25,
        name: "MoveAssignmentPrioritizer",
        family: None,
        kind: NextBestAction,
        inference: Induction,
        semiring: NarsTruth,
        style: Exploratory,
        lane: "L7",
        decides: "which confirmed moves to satisfy first (priority/deadline/quants)",
    },
    Savant {
        id: 26,
        name: "BackorderJudge",
        family: None,
        kind: NextBestAction,
        inference: Abduction,
        semiring: NarsTruth,
        style: Exploratory,
        lane: "L7",
        decides: "partial fulfilment ⇒ backorder vs cancel remainder",
    },
];

/// Look up a savant by roster id.
#[inline]
pub fn savant(id: u8) -> Option<&'static Savant> {
    SAVANTS.iter().find(|s| s.id == id)
}

/// Look up a savant by name.
#[inline]
pub fn savant_by_name(name: &str) -> Option<&'static Savant> {
    SAVANTS.iter().find(|s| s.name == name)
}

/// All savants still awaiting a Layer-2 alignment axiom (`family = None`).
pub fn unaligned() -> impl Iterator<Item = &'static Savant> {
    SAVANTS.iter().filter(|s| s.family.is_none())
}

impl Savant {
    /// The runtime query strategy this savant dispatches to.
    pub fn query_strategy(&self) -> crate::nars::QueryStrategy {
        self.inference.default_strategy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_is_25_with_unique_ids() {
        assert_eq!(SAVANTS.len(), 25);
        for s in &SAVANTS {
            assert!(!s.name.is_empty() && !s.decides.is_empty());
            assert_eq!(
                savant(s.id).map(|x| x.name),
                Some(s.name),
                "id lookup round-trips"
            );
        }
        assert!(savant(16).is_none(), "id 16 intentionally absent");
    }

    #[test]
    fn lookups_and_dispatch() {
        let fp = savant_by_name("FiscalPositionResolver").unwrap();
        assert_eq!(fp.id, 1);
        assert_eq!(fp.family, Some(0x80));
        // Deduction → CamExact (per InferenceType::default_strategy).
        assert_eq!(fp.query_strategy(), crate::nars::QueryStrategy::CamExact);
    }

    #[test]
    fn unaligned_savants_need_axioms() {
        // The stock.* / analytic.model / reconcile savants carry family=None.
        let names: Vec<&str> = unaligned().map(|s| s.name).collect();
        assert!(names.contains(&"ProcurementRuleSelector"));
        assert!(names.contains(&"ReconcileMatchSelector"));
        assert!(!names.contains(&"FiscalPositionResolver")); // 0x80, aligned
        assert!(unaligned().count() >= 9);
    }
}
