//! Faculty — cognitive function identity, the internal counterpart to `ExternalRole`.
//!
//! Where `ExternalRole` answers *"who sent this at the gate"* (external
//! identity), `FacultyRole` answers *"which cognitive function processed this
//! inside the substrate"* (internal identity). Both are `RoleKey`-bindable
//! into the Markov ±5 trajectory; both flow through the same A2A blackboard;
//! unbinding at either key recovers the respective coordinate.
//!
//! ## Asymmetry — inbound vs outbound thinking styles
//!
//! Each faculty has two thinking styles, one per direction:
//!
//! - `inbound_style`: how this faculty INTERPRETS received bundles
//!   (unbinding strategy + ScanParams shape).
//! - `outbound_style`: how this faculty EMITS its contribution to the next
//!   blackboard round (bind strategy + FieldModulation shape).
//!
//! Reading comprehension may receive `Analytical` and emit `Concise`; empathy
//! may receive `Gentle` and emit `Warm`; reasoning may receive `Systematic`
//! and emit `Precise`. The asymmetry is the point — a faculty is a shaped
//! transducer, not a bidirectional pipe.
//!
//! ## Tools
//!
//! Each faculty declares `ToolAbility` ids — concrete operations it can
//! invoke during its processing (chunk_text, tts, nars_revise, qualia_match,
//! …). The tool registry is consumer-side; the contract only carries the
//! opaque id type so that faculty descriptors can be serialized and compared.
//!
//! ## Composition with other roles
//!
//! The full provenance of a blackboard entry is a 3-coordinate identity:
//!
//! ```text
//! (ExternalRole family, ExpertId card, FacultyRole function)
//! ```
//!
//! - External: "a CrewaiAgent deposited this seed"
//! - Card: "the family-codec-smith card handled it"
//! - Faculty: "the Reasoning faculty processed it"
//!
//! All three are RoleKey-bindable; unbinding at any coordinate recovers that
//! slice of the provenance. The shader can observe its own faculties as
//! first-class features — `QualiaClassification` can fire on "Reasoning
//! overwhelmed, Empathy idle" and the router rebalances.
//!
//! Plan: `.claude/plans/callcenter-membrane-v1.md` § 10.10

use crate::thinking::ThinkingStyle;

/// Opaque tool-ability identifier.
///
/// The registry mapping ids to implementations is consumer-side (in
/// lance-graph core or the calling crate). The contract carries the id type
/// only so `FacultyDescriptor` can declare tool lists without pulling in
/// implementation dependencies.
pub type ToolAbility = u16;

/// Cognitive faculty identity — internal role coordinate.
///
/// Extend by adding variants. The `#[repr(u8)]` layout makes faculties cheap
/// to pack into the braid key alongside `ExternalRole` and `ExpertId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FacultyRole {
    /// Text understanding: chunking, entity extraction, coreference.
    ReadingComprehension = 0,
    /// Prosody-aware speech generation / analysis.
    Voice = 1,
    /// Logical composition, NARS revision, semiring derivation.
    Reasoning = 2,
    /// Qualia matching, resonance scoring, other-as-self modelling.
    Empathy = 3,
    // Extensible: Memory, Attention, Prosody, Imagination, Planning, …
}

/// Descriptor: a faculty's identity + asymmetric styles + invocable tools.
///
/// Typically constructed once at registration and held by the faculty
/// dispatcher. The `tools` slice is `'static` because the tool registry is
/// built at startup, not mutated per request.
#[derive(Clone, Debug)]
pub struct FacultyDescriptor {
    pub role: FacultyRole,
    /// How this faculty INTERPRETS incoming bundles.
    pub inbound_style: ThinkingStyle,
    /// How this faculty EMITS its contribution.
    pub outbound_style: ThinkingStyle,
    /// Concrete tool-ability ids this faculty can invoke.
    pub tools: &'static [ToolAbility],
}

impl FacultyDescriptor {
    /// True when inbound and outbound styles differ — the expected case for
    /// a genuine transducer. A faculty with symmetric styles is either
    /// misconfigured or a pure pass-through.
    #[inline]
    pub fn is_asymmetric(&self) -> bool {
        self.inbound_style != self.outbound_style
    }
}
