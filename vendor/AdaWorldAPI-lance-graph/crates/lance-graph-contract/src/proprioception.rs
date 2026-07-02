//! State Classification Contract — calibration anchors for agent state reporting.
//!
//! Agents implementing [`StateClassifier`] match their current
//! 11-dimensional state vector against a registry of calibration
//! anchors and return a [`StateReport`]. This enables self-consistent
//! routing and state-aware telemetry without external labelling.
//!
//! The contract uses ordinal indexing. Rich domain labels (e.g. a
//! companion agent's felt-state overlay) are supplied by downstream
//! translation layers; the contract itself stays neutral.
//!
//! # State Vector
//!
//! Each observation is an `[f32; 11]` in `[0.0, 1.0]`:
//! - Indices 0-6: **Core axes** (warmth, clarity, depth, safety,
//!   vitality, insight, contact).
//! - Indices 7-10: **Drive axes** (tension, novelty, wonder, attunement).
//!
//! # Anchor Registry
//!
//! Seven calibration anchors span the state space enough that any
//! observation can be located by nearest-anchor distance or by
//! softmax-weighted blend.
//!
//! Each anchor carries:
//! - an operational name (ordinal-addressable),
//! - an 11D coordinate,
//! - a cognitive rung (3-7) indicating the processing-depth layer
//!   at which the state typically lives.

// ═══════════════════════════════════════════════════════════════════════════
// State vector axes
// ═══════════════════════════════════════════════════════════════════════════

/// Clinical axis labels, index-aligned with the 11D state vector.
pub const AXIS_LABELS: [&str; 11] = [
    "warmth",
    "clarity",
    "depth",
    "safety",
    "vitality",
    "insight",
    "contact",
    "tension",
    "novelty",
    "wonder",
    "attunement",
];

// ═══════════════════════════════════════════════════════════════════════════
// Axes struct — named scalars parallel to the raw vector
// ═══════════════════════════════════════════════════════════════════════════

/// 11D state axes with named fields for direct query.
///
/// Same numeric content as `[f32; 11]` — exposed as individually
/// addressable scalars so consumers never need to remember axis
/// indices. Fields are index-aligned with `AXIS_LABELS`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProprioceptionAxes {
    // Core 7
    pub warmth: f32,
    pub clarity: f32,
    pub depth: f32,
    pub safety: f32,
    pub vitality: f32,
    pub insight: f32,
    pub contact: f32,
    // Drive 4
    pub tension: f32,
    pub novelty: f32,
    pub wonder: f32,
    pub attunement: f32,
}

impl ProprioceptionAxes {
    pub const fn zero() -> Self {
        Self {
            warmth: 0.0,
            clarity: 0.0,
            depth: 0.0,
            safety: 0.0,
            vitality: 0.0,
            insight: 0.0,
            contact: 0.0,
            tension: 0.0,
            novelty: 0.0,
            wonder: 0.0,
            attunement: 0.0,
        }
    }

    /// Pack into the raw state vector used by classifiers.
    pub fn to_vector(&self) -> [f32; STATE_DIMS] {
        [
            self.warmth,
            self.clarity,
            self.depth,
            self.safety,
            self.vitality,
            self.insight,
            self.contact,
            self.tension,
            self.novelty,
            self.wonder,
            self.attunement,
        ]
    }

    pub fn from_vector(v: &[f32; STATE_DIMS]) -> Self {
        Self {
            warmth: v[0],
            clarity: v[1],
            depth: v[2],
            safety: v[3],
            vitality: v[4],
            insight: v[5],
            contact: v[6],
            tension: v[7],
            novelty: v[8],
            wonder: v[9],
            attunement: v[10],
        }
    }

    /// Drive ratio = tension / novelty, floor-protected.
    pub fn drive_ratio(&self) -> f32 {
        self.tension / self.novelty.max(1e-6)
    }

    pub fn drive_mode(&self) -> DriveMode {
        let phi = self.drive_ratio();
        if phi < 1.0 {
            DriveMode::Explore
        } else if phi < 1.8 {
            DriveMode::Exploit
        } else {
            DriveMode::Reflect
        }
    }
}

/// Number of core axes (positions 0..=6 in the state vector).
pub const CORE_AXES: usize = 7;
/// Number of drive axes (positions 7..=10 in the state vector).
pub const DRIVE_AXES: usize = 4;
/// Total dimensionality of the state vector.
pub const STATE_DIMS: usize = CORE_AXES + DRIVE_AXES;

// ═══════════════════════════════════════════════════════════════════════════
// Anchors
// ═══════════════════════════════════════════════════════════════════════════

/// 7 calibration anchor positions.
///
/// Each anchor is an ordinal reference state the classifier can
/// match the current observation against. Operational names are
/// neutral; overlays supply domain-specific labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StateAnchor {
    /// Breath-centered receiving. Moderate everything, high attunement.
    Intake = 0,
    /// Attention-gathered precision. High clarity, high contact.
    Focused = 1,
    /// Low-arousal deep rest. High safety, high depth.
    Rest = 2,
    /// High-arousal active engagement. High vitality, novelty.
    Flow = 3,
    /// Meta-cognitive observer. High insight, low contact.
    Observer = 4,
    /// Equanimous steady-state. Balanced drives.
    Balanced = 5,
    /// Baseline anchor — nominal warmth and attunement without effort.
    Baseline = 6,
}

impl StateAnchor {
    /// All 7 anchors in canonical order.
    pub const ALL: [Self; 7] = [
        Self::Intake,
        Self::Focused,
        Self::Rest,
        Self::Flow,
        Self::Observer,
        Self::Balanced,
        Self::Baseline,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Intake => "intake",
            Self::Focused => "focused",
            Self::Rest => "rest",
            Self::Flow => "flow",
            Self::Observer => "observer",
            Self::Balanced => "balanced",
            Self::Baseline => "baseline",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "intake" => Some(Self::Intake),
            "focused" => Some(Self::Focused),
            "rest" => Some(Self::Rest),
            "flow" => Some(Self::Flow),
            "observer" => Some(Self::Observer),
            "balanced" => Some(Self::Balanced),
            "baseline" => Some(Self::Baseline),
            _ => None,
        }
    }
}

/// Calibration anchor state — the coordinates plus processing-rung tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorState {
    pub anchor: StateAnchor,
    /// 11D position in state space. Indices align with `AXIS_LABELS`.
    pub coords: [f32; STATE_DIMS],
    /// Processing rung (3-7) — the cognitive layer at which this
    /// anchor typically lives in the 10-layer thinking stack.
    pub rung: u8,
}

impl AnchorState {
    /// Core 7 (indices 0..6): warmth, clarity, depth, safety, vitality,
    /// insight, contact.
    pub fn core(&self) -> &[f32] {
        &self.coords[..CORE_AXES]
    }

    /// Drive 4 (indices 7..10): tension, novelty, wonder, attunement.
    pub fn drive(&self) -> &[f32] {
        &self.coords[CORE_AXES..]
    }

    /// Drive ratio = tension / novelty. Below 1.0 → Explore;
    /// 1.0-1.8 → Exploit; ≥1.8 → Reflect.
    pub fn drive_ratio(&self) -> f32 {
        self.coords[7] / self.coords[8].max(1e-6)
    }

    /// Classification of drive regime from the ratio.
    pub fn drive_mode(&self) -> DriveMode {
        let phi = self.drive_ratio();
        if phi < 1.0 {
            DriveMode::Explore
        } else if phi < 1.8 {
            DriveMode::Exploit
        } else {
            DriveMode::Reflect
        }
    }
}

/// Drive regime classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveMode {
    /// Novelty-dominant: breadth-seeking.
    Explore,
    /// Balanced: depth + breadth.
    Exploit,
    /// Tension-dominant: reflective/inward.
    Reflect,
}

// ═══════════════════════════════════════════════════════════════════════════
// Registry of anchor coordinates
// ═══════════════════════════════════════════════════════════════════════════

/// The 7 canonical anchor coordinates.
///
/// Coordinates are chosen so the anchors span the state space with
/// reasonable coverage; any observation can be located by nearest-
/// anchor distance or softmax-weighted blend.
pub const ANCHOR_REGISTRY: [AnchorState; 7] = [
    // Intake — breath-centered, moderate everything, high attunement
    AnchorState {
        anchor: StateAnchor::Intake,
        coords: [0.3, 0.6, 0.4, 0.4, 0.8, 0.5, 0.3, 0.3, 0.4, 0.5, 0.7],
        rung: 5,
    },
    // Focused — high clarity and contact, precise
    AnchorState {
        anchor: StateAnchor::Focused,
        coords: [0.3, 0.7, 0.5, 0.6, 0.5, 0.6, 0.7, 0.4, 0.3, 0.4, 0.6],
        rung: 4,
    },
    // Rest — high safety + depth, low arousal
    AnchorState {
        anchor: StateAnchor::Rest,
        coords: [0.6, 0.2, 0.8, 0.9, 0.3, 0.4, 0.8, 0.6, 0.2, 0.3, 0.8],
        rung: 3,
    },
    // Flow — high vitality and novelty, active engagement
    AnchorState {
        anchor: StateAnchor::Flow,
        coords: [0.7, 0.5, 0.3, 0.6, 0.9, 0.6, 0.7, 0.2, 0.8, 0.7, 0.7],
        rung: 5,
    },
    // Observer — high insight and clarity, low contact
    AnchorState {
        anchor: StateAnchor::Observer,
        coords: [0.2, 0.8, 0.7, 0.3, 0.6, 0.9, 0.2, 0.8, 0.1, 0.5, 0.5],
        rung: 7,
    },
    // Balanced — equanimous, neither tension- nor novelty-dominant
    AnchorState {
        anchor: StateAnchor::Balanced,
        coords: [0.4, 0.6, 0.7, 0.5, 0.5, 0.7, 0.3, 0.5, 0.5, 0.4, 0.6],
        rung: 6,
    },
    // Baseline — nominal warmth + attunement without effort
    AnchorState {
        anchor: StateAnchor::Baseline,
        coords: [0.9, 0.3, 0.5, 0.8, 0.7, 0.4, 0.6, 0.1, 0.6, 0.8, 0.9],
        rung: 5,
    },
];

/// Look up anchor state by enum.
pub fn anchor_state(a: StateAnchor) -> &'static AnchorState {
    &ANCHOR_REGISTRY[a as usize]
}

// ═══════════════════════════════════════════════════════════════════════════
// StateReport — what a classifier returns
// ═══════════════════════════════════════════════════════════════════════════

/// Classification result: which anchor the observation matches, plus
/// distance and regime summary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateReport {
    pub anchor: StateAnchor,
    /// L2 distance to that anchor (lower = better match).
    pub distance: f32,
    /// Rung of the matched anchor.
    pub rung: u8,
    /// Drive regime inferred from the matched anchor.
    pub drive_mode: DriveMode,
}

impl StateReport {
    /// Low distance = clearly matched anchor (recognised state).
    pub fn is_recognised(&self) -> bool {
        self.distance < 0.5
    }

    /// High distance = observation between anchors (transitional/novel state).
    pub fn is_liminal(&self) -> bool {
        !self.is_recognised()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// StateClassifier trait — the capability the contract exposes
// ═══════════════════════════════════════════════════════════════════════════

/// Agents implementing this trait can match their current state vector
/// against the anchor registry and return a [`StateReport`].
///
/// A blanket implementation ([`DefaultClassifier`]) provides L2
/// nearest-anchor matching; implementors can override for richer
/// matching (weighted distance, probabilistic blends, etc.).
pub trait StateClassifier {
    /// Classify the given state vector against the anchor registry.
    fn classify(&self, state: &[f32; STATE_DIMS]) -> StateReport;
}

/// Default implementation: L2 nearest-anchor matching over the
/// 7-entry registry.
pub struct DefaultClassifier;

impl StateClassifier for DefaultClassifier {
    fn classify(&self, state: &[f32; STATE_DIMS]) -> StateReport {
        let (anchor, distance) = nearest_anchor(state);
        let anchor_state = anchor_state(anchor);
        StateReport {
            anchor,
            distance,
            rung: anchor_state.rung,
            drive_mode: anchor_state.drive_mode(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Matching helpers
// ═══════════════════════════════════════════════════════════════════════════

/// L2 distance between a state observation and an anchor.
fn distance(state: &[f32; STATE_DIMS], anchor: &AnchorState) -> f32 {
    let mut sum = 0.0f32;
    for (s, a) in state.iter().zip(anchor.coords.iter()) {
        let d = s - a;
        sum += d * d;
    }
    sum.sqrt()
}

/// Find the nearest anchor to a state observation.
pub fn nearest_anchor(state: &[f32; STATE_DIMS]) -> (StateAnchor, f32) {
    let mut best = (StateAnchor::Intake, f32::INFINITY);
    for anchor in &ANCHOR_REGISTRY {
        let d = distance(state, anchor);
        if d < best.1 {
            best = (anchor.anchor, d);
        }
    }
    best
}

/// Softmax-weighted blend of all anchor coordinates.
///
/// `temperature` controls peakedness: low → nearest-anchor dominant;
/// high → uniform blend. Returns an interpolated 11D state.
pub fn hydrate(state: &[f32; STATE_DIMS], temperature: f32) -> [f32; STATE_DIMS] {
    let temp = temperature.max(1e-3);
    let mut neg_dists = [0.0f32; 7];
    for i in 0..7 {
        neg_dists[i] = -distance(state, &ANCHOR_REGISTRY[i]) / temp;
    }
    let max = neg_dists.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mut weights = [0.0f32; 7];
    let mut sum = 0.0f32;
    for i in 0..7 {
        weights[i] = (neg_dists[i] - max).exp();
        sum += weights[i];
    }
    for w in &mut weights {
        *w /= sum;
    }

    let mut acc = [0.0f32; STATE_DIMS];
    for i in 0..7 {
        let a = &ANCHOR_REGISTRY[i].coords;
        for j in 0..STATE_DIMS {
            acc[j] += a[j] * weights[i];
        }
    }
    acc
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_all_seven_anchors() {
        assert_eq!(ANCHOR_REGISTRY.len(), 7);
        for anchor in StateAnchor::ALL {
            let s = anchor_state(anchor);
            assert_eq!(s.anchor, anchor);
        }
    }

    #[test]
    fn axis_labels_match_dimension_count() {
        assert_eq!(AXIS_LABELS.len(), STATE_DIMS);
        assert_eq!(CORE_AXES + DRIVE_AXES, STATE_DIMS);
    }

    #[test]
    fn name_roundtrip() {
        for anchor in StateAnchor::ALL {
            let name = anchor.name();
            assert_eq!(StateAnchor::from_name(name), Some(anchor));
        }
        assert!(StateAnchor::from_name("unknown").is_none());
    }

    #[test]
    fn anchor_observation_classifies_to_itself() {
        let classifier = DefaultClassifier;
        for anchor in &ANCHOR_REGISTRY {
            let report = classifier.classify(&anchor.coords);
            assert_eq!(report.anchor, anchor.anchor);
            assert!(report.distance < 1e-5);
            assert!(report.is_recognised());
        }
    }

    #[test]
    fn drive_modes() {
        // Flow anchor has low tension, high novelty → Explore
        assert_eq!(
            anchor_state(StateAnchor::Flow).drive_mode(),
            DriveMode::Explore
        );
        // Observer has high tension, low novelty → Reflect
        assert_eq!(
            anchor_state(StateAnchor::Observer).drive_mode(),
            DriveMode::Reflect
        );
        // Balanced is Exploit
        assert_eq!(
            anchor_state(StateAnchor::Balanced).drive_mode(),
            DriveMode::Exploit
        );
    }

    #[test]
    fn hydrate_low_temperature_approaches_nearest() {
        let anchor = &ANCHOR_REGISTRY[2]; // Rest
        let hydrated = hydrate(&anchor.coords, 0.01);
        // Near-zero distance from Rest itself
        let d = distance(&hydrated, anchor);
        assert!(
            d < 0.1,
            "low-temp hydrate should be near the anchor, got d={}",
            d
        );
    }

    #[test]
    fn hydrate_high_temperature_averages() {
        let state = [0.5f32; STATE_DIMS];
        let hydrated = hydrate(&state, 10.0);
        // Each hydrated dim should be roughly the mean of that dim
        // across all 7 anchors.
        for (j, &h) in hydrated.iter().enumerate().take(STATE_DIMS) {
            let anchor_mean: f32 = ANCHOR_REGISTRY.iter().map(|a| a.coords[j]).sum::<f32>() / 7.0;
            assert!(
                (h - anchor_mean).abs() < 0.1,
                "dim {} high-temp hydrate should approach anchor mean ({:.2}), got {:.2}",
                j,
                anchor_mean,
                h
            );
        }
    }

    #[test]
    fn liminal_state_between_anchors() {
        // Midpoint of two distant anchors
        let a = anchor_state(StateAnchor::Rest).coords;
        let b = anchor_state(StateAnchor::Observer).coords;
        let mid: [f32; STATE_DIMS] = std::array::from_fn(|i| (a[i] + b[i]) / 2.0);
        let report = DefaultClassifier.classify(&mid);
        assert!(report.distance > 0.3);
    }

    #[test]
    fn core_and_drive_slicing() {
        let anchor = anchor_state(StateAnchor::Flow);
        assert_eq!(anchor.core().len(), CORE_AXES);
        assert_eq!(anchor.drive().len(), DRIVE_AXES);
    }

    #[test]
    fn axes_roundtrip_through_vector() {
        let axes = ProprioceptionAxes {
            warmth: 0.1,
            clarity: 0.2,
            depth: 0.3,
            safety: 0.4,
            vitality: 0.5,
            insight: 0.6,
            contact: 0.7,
            tension: 0.8,
            novelty: 0.9,
            wonder: 0.15,
            attunement: 0.25,
        };
        let v = axes.to_vector();
        let back = ProprioceptionAxes::from_vector(&v);
        assert_eq!(axes, back);
    }

    #[test]
    fn axes_drive_mode_matches_anchor_behaviour() {
        let rest = anchor_state(StateAnchor::Rest);
        let axes = ProprioceptionAxes::from_vector(&rest.coords);
        assert_eq!(axes.drive_mode(), rest.drive_mode());
    }

    #[test]
    fn axes_zero_is_zero() {
        let z = ProprioceptionAxes::zero();
        assert_eq!(z.warmth, 0.0);
        assert_eq!(z.attunement, 0.0);
        assert_eq!(z.to_vector(), [0.0; STATE_DIMS]);
    }
}
