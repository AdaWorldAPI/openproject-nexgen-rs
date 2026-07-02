//! Persona — identity bundle for agent cards participating in the A2A blackboard.
//!
//! A persona is what a consumer (n8n-rs, crewai-rust, openclaw, or an internal
//! YAML card in `.claude/agents/`) looks like from the blackboard's point of
//! view. It bundles:
//!
//! - `ExternalRole`: family at the gate (Rag / CrewaiAgent / N8n / …)
//! - `ExpertEntry`: specific card + capability + trust prior
//! - (consumer-side) AriGraph subgraph: the persona's memory, committed facts,
//!   reversal history — lives in `lance-graph::graph::arigraph`, referenced by
//!   handle, NOT carried in this zero-dep crate.
//!
//! ## Identity lives in metadata; VSA binding is stack-side (BBB invariant)
//!
//! `PersonaCard` fields are METADATA — typed scalars safely crossing the BBB
//! as Arrow columns on `cognitive_event` rows. They are:
//!
//! - SQL-queryable (`WHERE external_role = X AND expert_id = Y`)
//! - Cypher / GQL matchable (`MATCH (e:Event {external_role: ...})`)
//! - NARS-truth-taggable (attach f,c to persona metadata facts)
//! - Qualia-classifiable (map metadata rows to qualia signatures)
//!
//! VSA binding of these identities happens STACK-SIDE only — when a blackboard
//! entry is processed, the stack deterministically maps metadata values
//! (`external_role: u8`, `expert_id: u16`) to `RoleKey` slot addresses in the
//! 10k-dim VSA substrate. The external surface never sees those slots.
//! The metadata → slot mapping is internal and never crosses the gate.
//!
//! ## Why the AriGraph reference is consumer-side
//!
//! The contract crate stays zero-dep — no Arrow, no Lance, no SPO store. The
//! persona's *identity* lives here (role + card + trust); the persona's
//! *memory* lives where the AriGraph lives (lance-graph core). A consumer
//! that wants to resolve a `PersonaCard` to its AriGraph subgraph does so
//! at the lance-graph boundary, e.g.
//!     `arigraph.subgraph_for(persona.entry.id)`.
//!
//! ## Routing: explicit vs implicit
//!
//! An external consumer deposits a seed with an optional `RoutingHint`. The
//! blackboard router reads it as follows:
//!
//! - `RoutingHint { target_role: Some(_), target_card: Some(_) }` — explicit
//!   full address: exactly that card in that family gets activated.
//! - `RoutingHint { target_role: Some(_), target_card: None }` — family route:
//!   the router picks the best card within that family (by AriGraph resonance
//!   against the seed payload).
//! - `RoutingHint { target_role: None, target_card: Some(_) }` — card-only:
//!   regardless of family, that specific card handles it.
//! - `RoutingHint::default()` — pure implicit: the router matches the seed's
//!   context fingerprint against every registered persona's AriGraph subgraph
//!   resonance, activates the top-k.
//!
//! READ BY: every session touching callcenter wiring, agent card registration,
//! consumer routing, or persona integration.
//!
//! Plan: `.claude/plans/callcenter-membrane-v1.md` § 10.6 – § 10.8

use crate::a2a_blackboard::{ExpertEntry, ExpertId};
use crate::external_membrane::ExternalRole;

/// Identity bundle for a persona participating in the A2A blackboard.
///
/// The AriGraph subgraph for this persona lives in lance-graph core
/// (`graph::arigraph`); consumers resolve it by `entry.id` at their boundary.
#[derive(Clone, Debug)]
pub struct PersonaCard {
    /// Family identity at the gate.
    pub role: ExternalRole,
    /// Specific card + capability + trust prior.
    pub entry: ExpertEntry,
}

// NOTE: identity is not packed into a single braid key. Role and card are
// separate typed metadata columns; they are addressable independently via
// SQL/Cypher/GQL/NARS/qualia. VSA binding of these identities happens
// stack-side only, via a deterministic metadata→RoleKey slot mapping that
// never crosses the BBB. See module-level docs.

/// Optional routing hint carried on `ExpertCapability::ExternalSeed` entries.
///
/// See module-level docs for the four routing modes (explicit-full,
/// family-only, card-only, implicit).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoutingHint {
    /// If `Some`, restrict activation to this family.
    pub target_role: Option<ExternalRole>,
    /// If `Some`, activate this specific card.
    pub target_card: Option<ExpertId>,
}

impl RoutingHint {
    /// True when no explicit targeting is present — the router must fall back
    /// to AriGraph-resonance matching against the seed payload.
    #[inline]
    pub fn is_implicit(&self) -> bool {
        self.target_role.is_none() && self.target_card.is_none()
    }
}
