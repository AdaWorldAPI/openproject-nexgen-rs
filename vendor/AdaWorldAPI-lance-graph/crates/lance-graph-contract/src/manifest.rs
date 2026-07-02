// AUTO-GENERATED support types for manifest codegen.
// This file is hand-written (~60 LOC); the generated data
// lives in src/generated/ogit_namespace.rs and
// src/generated/manifest_metadata.rs.

/// Escalation mode when a policy evaluation fails or
/// when an action requires elevated review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escalation {
    /// Route to an LLM for automated secondary evaluation.
    Llm,
    /// Route to a human reviewer.
    Human,
    /// Deny the action immediately; no secondary evaluation.
    Deny,
}

/// Per-domain runtime knobs extracted from `stack_profile:`
/// in the manifest YAML. All fields are set to safe defaults
/// when the YAML block is `~` (null / absent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackProfile {
    /// Number of days audit records must be retained
    /// (0 = not specified / not applicable).
    pub audit_days: u32,
    /// If `true`, a `Policy::evaluate` error results in a Deny
    /// rather than a default-allow.
    pub fail_closed: bool,
    /// Where to route actions that require secondary review.
    pub escalation: Escalation,
}

impl Default for StackProfile {
    fn default() -> Self {
        Self {
            audit_days: 0,
            fail_closed: false,
            escalation: Escalation::Deny,
        }
    }
}

/// Per-domain metadata extracted from manifests at compile time.
/// Data-only: holds `&'static str` and primitive types — no
/// consumer crate references, no generic parameters.
///
/// `MANIFEST_METADATA` is a `&'static [ManifestEntry]` sorted
/// ascending by `g_slot`; look up entries via
/// `manifest_metadata(g_slot)` which uses `binary_search_by_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestEntry {
    /// The OGIT G slot number (primary key, used for binary search).
    pub g_slot: u32,
    /// Manifest schema version (`version:` field in YAML).
    pub version: u32,
    /// The `domain_name` field (must equal the directory name).
    pub domain_name: &'static str,
    /// `true` when no actor crate is expected at compile time.
    pub inert: bool,
    /// Name of the RBAC policy, or `None` for inert modules.
    pub rbac_policy: Option<&'static str>,
    /// Runtime stack knobs from `stack_profile:`.
    pub stack: StackProfile,
    /// Cargo crate name of the actor implementation, if any.
    pub actor_crate: Option<&'static str>,
    /// Actor struct name within that crate, if any.
    pub actor_type: Option<&'static str>,
    /// Number of entity types declared in this manifest.
    pub entity_count: u32,
}

// Include generated data produced by build.rs.
// The generated files define:
//   pub mod OGIT { pub const *_V*: (u32, u32) }
//   pub const ALL_G_SLOTS: &[u32]
//   pub static MANIFEST_METADATA: &[ManifestEntry]
include!(concat!(env!("OUT_DIR"), "/ogit_namespace.rs"));
include!(concat!(env!("OUT_DIR"), "/manifest_metadata.rs"));

/// Look up a manifest entry by G slot using binary search.
///
/// Returns `None` when `g_slot` is not registered in any manifest.
/// `O(log N)` where N ≤ 50 for any realistic workspace.
pub fn manifest_metadata(g_slot: u32) -> Option<&'static ManifestEntry> {
    MANIFEST_METADATA
        .binary_search_by_key(&g_slot, |e| e.g_slot)
        .ok()
        .map(|idx| &MANIFEST_METADATA[idx])
}
