//! WorldMapDto — rendering-agnostic state snapshot with pluggable labels.
//!
//! Complements `world_model::WorldModelDto`:
//!
//! - `WorldModelDto` is the structured quadrant-snapshot (self / other /
//!   field / context). Heavy, good when the consumer wants the full
//!   situational-awareness payload.
//! - `WorldMapDto` (this module) is a **minimal numeric map** — raw state
//!   vector + anchor + drive mode. No named fields. Consumers render it
//!   by dropping in a [`WorldMapRenderer`] that provides their own axis
//!   and anchor labels.
//!
//! This keeps the contract free of any consumer-specific vocabulary.
//! A health-coach application and an industrial-monitoring application
//! can both consume the same map, each applying its own renderer, and
//! the contract never needs to know which labels either side uses.
//!
//! ```text
//!   WorldMapDto (numbers only)
//!        ├── rendered via DefaultRenderer → clinical labels (canonical)
//!        ├── rendered via AdaFeltRenderer → companion-agent labels
//!        └── rendered via XyzRenderer     → industry-specific labels
//! ```

// `AnchorState` retained for wiring into the upcoming AnchorState→WorldMap
// blending function (TD-WM-1). Currently the renderer reads from the
// scalar StateAnchor variants directly.
#[allow(unused_imports)]
use crate::proprioception::{
    AnchorState, DriveMode, StateAnchor, StateReport, AXIS_LABELS, STATE_DIMS,
};

// ═══════════════════════════════════════════════════════════════════════════
// The DTO — minimum viable state map
// ═══════════════════════════════════════════════════════════════════════════

/// Minimal state-map snapshot.
///
/// All semantic weight lives in the numeric fields. Downstream renderers
/// provide human-readable framing without the contract having to carry
/// any vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldMapDto {
    /// Raw 11D state vector.
    pub state_vector: [f32; STATE_DIMS],
    /// Closest calibration anchor (enum, no name strings).
    pub anchor: StateAnchor,
    /// L2 distance to the anchor.
    pub distance: f32,
    /// Rung of the matched anchor (3-7).
    pub rung: u8,
    /// Drive regime.
    pub drive_mode: DriveMode,
    /// Monotonic cycle counter.
    pub cycle_index: u64,
}

impl WorldMapDto {
    /// Build from a raw state vector by running the default classifier.
    pub fn from_state_vector(v: &[f32; STATE_DIMS], cycle_index: u64) -> Self {
        let (anchor, distance) = crate::proprioception::nearest_anchor(v);
        let anchor_state = crate::proprioception::anchor_state(anchor);
        Self {
            state_vector: *v,
            anchor,
            distance,
            rung: anchor_state.rung,
            drive_mode: anchor_state.drive_mode(),
            cycle_index,
        }
    }

    /// Build from a classifier report + input vector.
    pub fn from_report(v: &[f32; STATE_DIMS], r: &StateReport, cycle_index: u64) -> Self {
        Self {
            state_vector: *v,
            anchor: r.anchor,
            distance: r.distance,
            rung: r.rung,
            drive_mode: r.drive_mode,
            cycle_index,
        }
    }

    /// Apply a renderer to produce a human-readable description.
    pub fn render<R: WorldMapRenderer>(&self, r: &R) -> String {
        r.describe(self)
    }

    /// Is this a recognised state (low distance)?
    pub fn is_recognised(&self) -> bool {
        self.distance < 0.5
    }

    pub fn is_liminal(&self) -> bool {
        !self.is_recognised()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Renderer trait — the drop-in hook
// ═══════════════════════════════════════════════════════════════════════════

/// Consumer-supplied rendering for a `WorldMapDto`.
///
/// Implement this trait in your own crate to present the map in
/// whatever vocabulary fits your application. The contract ships with
/// [`DefaultRenderer`] (neutral axis + anchor names); ada-rs,
/// crewai-rust, n8n-rs etc. can each provide their own.
pub trait WorldMapRenderer {
    /// Label for axis `idx` (0..10).
    fn axis_label(&self, idx: usize) -> &str;

    /// Label for the anchor.
    fn anchor_label(&self, anchor: StateAnchor) -> &str;

    /// Label for the drive regime.
    fn drive_label(&self, mode: DriveMode) -> &str {
        match mode {
            DriveMode::Explore => "explore",
            DriveMode::Exploit => "exploit",
            DriveMode::Reflect => "reflect",
        }
    }

    /// Full descriptive rendering of the map. Default composes the
    /// individual labels; override for domain-specific narration.
    fn describe(&self, map: &WorldMapDto) -> String {
        let top = map
            .state_vector
            .iter()
            .enumerate()
            .take(STATE_DIMS)
            .filter(|(_, v)| **v > 0.5)
            .map(|(i, v)| format!("{}={:.2}", self.axis_label(i), v))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "anchor={} drive={} rung={} d={:.2} cycle={} [{}]",
            self.anchor_label(map.anchor),
            self.drive_label(map.drive_mode),
            map.rung,
            map.distance,
            map.cycle_index,
            top,
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DefaultRenderer — ships with the contract
// ═══════════════════════════════════════════════════════════════════════════

/// Neutral renderer using the canonical axis and anchor labels.
/// Always available; consumers can compose or replace as needed.
pub struct DefaultRenderer;

impl WorldMapRenderer for DefaultRenderer {
    fn axis_label(&self, idx: usize) -> &str {
        AXIS_LABELS.get(idx).copied().unwrap_or("")
    }

    fn anchor_label(&self, anchor: StateAnchor) -> &str {
        anchor.name()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_builds_from_anchor_coords() {
        let anchor = crate::proprioception::anchor_state(StateAnchor::Balanced);
        let map = WorldMapDto::from_state_vector(&anchor.coords, 0);
        assert_eq!(map.anchor, StateAnchor::Balanced);
        assert!(map.distance < 1e-5);
        assert_eq!(map.rung, 6);
        assert_eq!(map.drive_mode, DriveMode::Exploit);
    }

    #[test]
    fn default_renderer_produces_neutral_labels() {
        let anchor = crate::proprioception::anchor_state(StateAnchor::Focused);
        let map = WorldMapDto::from_state_vector(&anchor.coords, 42);
        let rendered = map.render(&DefaultRenderer);
        assert!(rendered.contains("anchor=focused"));
        assert!(rendered.contains("cycle=42"));
    }

    #[test]
    fn drop_in_custom_renderer() {
        struct GreekRenderer;
        impl WorldMapRenderer for GreekRenderer {
            fn axis_label(&self, idx: usize) -> &str {
                const G: [&str; 11] = [
                    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota",
                    "kappa", "lambda",
                ];
                G.get(idx).copied().unwrap_or("")
            }
            fn anchor_label(&self, anchor: StateAnchor) -> &str {
                match anchor {
                    StateAnchor::Intake => "Α",
                    StateAnchor::Focused => "Β",
                    StateAnchor::Rest => "Γ",
                    StateAnchor::Flow => "Δ",
                    StateAnchor::Observer => "Ε",
                    StateAnchor::Balanced => "Ζ",
                    StateAnchor::Baseline => "Η",
                }
            }
        }

        let anchor = crate::proprioception::anchor_state(StateAnchor::Flow);
        let map = WorldMapDto::from_state_vector(&anchor.coords, 0);
        let rendered = map.render(&GreekRenderer);
        assert!(rendered.contains("anchor=Δ"), "got: {}", rendered);
    }

    #[test]
    fn recognised_vs_liminal_matches_state_report() {
        let anchor = crate::proprioception::anchor_state(StateAnchor::Rest);
        let map = WorldMapDto::from_state_vector(&anchor.coords, 0);
        assert!(map.is_recognised());
        assert!(!map.is_liminal());
    }
}
