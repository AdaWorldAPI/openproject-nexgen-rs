//! Graph visual-render contract — Neo4j/Palantir Gotham cockpit surface.
//!
//! Defines the traits and DTOs that any visual renderer (q2 Palantir cockpit,
//! Neo4j Browser-style UI, or debugging dashboards) uses to consume the live
//! knowledge graph without depending on lance-graph core.
//!
//! **Producer:** lance-graph arigraph (TripletGraph, EpisodicMemory, GraphSensorium).
//! **Consumer:** q2 cockpit-server `graph_engine.rs`, future Neo4j-compat UI.
//!
//! Zero dependencies. Pure data types + traits.

/// A rendered graph node for visual display.
#[derive(Debug, Clone)]
pub struct RenderNode {
    /// Unique node identifier (entity name or DN).
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Node type/category for visual styling (e.g., "TechCompany", "Person").
    pub kind: String,
    /// NARS confidence in this entity's existence [0.0, 1.0].
    pub confidence: f32,
    /// Additional display properties.
    pub props: Vec<(String, String)>,
}

/// A rendered graph edge for visual display.
#[derive(Debug, Clone)]
pub struct RenderEdge {
    /// Source node ID.
    pub source: String,
    /// Target node ID.
    pub target: String,
    /// Relationship type label.
    pub label: String,
    /// NARS truth: frequency component [0.0, 1.0].
    pub frequency: f32,
    /// NARS truth: confidence component [0.0, 1.0].
    pub confidence: f32,
    /// Whether this edge was inferred (vs directly observed).
    pub inferred: bool,
}

/// An inferred connection discovered by NARS deduction.
#[derive(Debug, Clone)]
pub struct InferredConnection {
    /// The deduced triplet: subject.
    pub subject: String,
    /// The deduced triplet: relation.
    pub relation: String,
    /// The deduced triplet: object.
    pub object: String,
    /// Deduction truth: frequency.
    pub frequency: f32,
    /// Deduction truth: confidence.
    pub confidence: f32,
    /// Inference chain description (human-readable provenance).
    pub chain: String,
}

/// A detected contradiction between two triplets.
#[derive(Debug, Clone)]
pub struct Contradiction {
    /// Index of first triplet.
    pub triplet_a: usize,
    /// Index of second triplet.
    pub triplet_b: usize,
    /// Human-readable description.
    pub description: String,
}

/// Full graph snapshot for rendering.
#[derive(Debug, Clone)]
pub struct GraphSnapshot {
    /// All visible nodes.
    pub nodes: Vec<RenderNode>,
    /// All visible edges (observed + inferred).
    pub edges: Vec<RenderEdge>,
    /// NARS-inferred connections (subset of edges where inferred=true).
    pub inferences: Vec<InferredConnection>,
    /// Detected contradictions.
    pub contradictions: Vec<Contradiction>,
    /// Snapshot timestamp (logical clock).
    pub timestamp: u64,
}

/// Graph health summary for dashboard display.
#[derive(Debug, Clone, Copy)]
pub struct GraphHealth {
    /// Total node count.
    pub node_count: usize,
    /// Total edge count (observed + inferred).
    pub edge_count: usize,
    /// Number of NARS-inferred edges.
    pub inference_count: usize,
    /// Number of active contradictions.
    pub contradiction_count: usize,
    /// Mean confidence across all edges [0.0, 1.0].
    pub mean_confidence: f32,
    /// Episodic memory utilization [0.0, 1.0].
    pub episodic_saturation: f32,
}

impl Default for GraphHealth {
    fn default() -> Self {
        Self {
            node_count: 0,
            edge_count: 0,
            inference_count: 0,
            contradiction_count: 0,
            mean_confidence: 0.0,
            episodic_saturation: 0.0,
        }
    }
}

/// Result of a Cypher query execution against the graph.
#[derive(Debug, Clone)]
pub struct CypherResult {
    /// Column names (Cypher RETURN clause aliases).
    pub columns: Vec<String>,
    /// Row data — each row is a vec of cell values.
    pub rows: Vec<Vec<CypherValue>>,
    /// Number of nodes created/matched.
    pub nodes_touched: usize,
    /// Number of relationships created/matched.
    pub relationships_touched: usize,
}

/// A single cell value in a Cypher result row.
#[derive(Debug, Clone)]
pub enum CypherValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Node(RenderNode),
    Edge(RenderEdge),
    Path(Vec<String>),
}

/// Episodic trace entry for reasoning provenance display.
#[derive(Debug, Clone)]
pub struct EpisodicTrace {
    /// Step number when this episode was recorded.
    pub step: u64,
    /// The observation text.
    pub observation: String,
    /// Triplet strings extracted from this episode.
    pub triplets: Vec<String>,
    /// Relevance score to the current query [0.0, 1.0].
    pub relevance: f32,
}

/// Trait for graph snapshot providers.
///
/// lance-graph arigraph implements this over TripletGraph + EpisodicMemory.
/// q2 cockpit-server consumes it via `dyn GraphSnapshotProvider`.
pub trait GraphSnapshotProvider: Send + Sync {
    /// Full graph snapshot for visual rendering.
    fn snapshot(&self) -> GraphSnapshot;

    /// Graph health summary for dashboard.
    fn health(&self) -> GraphHealth;

    /// Nodes and edges reachable within `hops` of the seed entities.
    fn subgraph(&self, seeds: &[&str], hops: usize) -> GraphSnapshot;
}

/// Trait for NARS inference over the graph.
///
/// Runs 2-hop deduction, detects contradictions, revises truth values.
pub trait GraphInferenceProvider: Send + Sync {
    /// Run NARS deduction up to `max_hops` and return inferred connections.
    fn infer(&self, max_hops: usize) -> Vec<InferredConnection>;

    /// Detect contradictions in the current graph state.
    fn contradictions(&self) -> Vec<Contradiction>;

    /// Revise truth value of a specific triplet (by index). Returns new (freq, conf).
    fn revise_truth(
        &mut self,
        triplet_idx: usize,
        evidence_freq: f32,
        evidence_conf: f32,
    ) -> (f32, f32);
}

/// Trait for Cypher query execution.
///
/// Parses Cypher → IR → executes against the graph → returns tabular + graph results.
pub trait CypherExecutor: Send + Sync {
    /// Execute a Cypher query string and return results.
    fn execute(&self, cypher: &str) -> Result<CypherResult, CypherError>;

    /// Validate a Cypher query without executing it. Returns column names on success.
    fn validate(&self, cypher: &str) -> Result<Vec<String>, CypherError>;
}

/// Cypher execution errors.
#[derive(Debug, Clone)]
pub enum CypherError {
    /// Query could not be parsed.
    ParseError(String),
    /// Query references unknown labels or types.
    UnknownLabel(String),
    /// Execution failed at the DataFusion layer.
    ExecutionError(String),
}

impl core::fmt::Display for CypherError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ParseError(msg) => write!(f, "parse error: {msg}"),
            Self::UnknownLabel(label) => write!(f, "unknown label: {label}"),
            Self::ExecutionError(msg) => write!(f, "execution error: {msg}"),
        }
    }
}

/// Trait for episodic memory access (reasoning trace for UI).
///
/// q2 renders the reasoning trace as a timeline sidebar. This trait provides
/// the retrieval surface without exposing lance-graph internals.
pub trait EpisodicTraceProvider: Send + Sync {
    /// Retrieve the `k` most relevant episodes for the given query.
    fn retrieve(&self, query: &str, k: usize) -> Vec<EpisodicTrace>;

    /// Retrieve episodes from the last `n` steps.
    fn recent(&self, n: usize) -> Vec<EpisodicTrace>;

    /// Current utilization [0.0, 1.0].
    fn saturation(&self) -> f32;
}

/// Trait for SSE event streaming (shader cycle events for scene player).
///
/// Each cycle emits a `ShaderEvent` that the cockpit scene player renders.
pub trait ShaderEventStream: Send + Sync {
    /// Subscribe to shader cycle events. Returns an iterator of events.
    /// The iterator yields `None` when the stream is closed.
    fn subscribe(&self) -> Box<dyn Iterator<Item = ShaderEvent> + Send>;
}

/// A shader cycle event for the scene player.
#[derive(Debug, Clone)]
pub struct ShaderEvent {
    /// Cycle index.
    pub cycle: u64,
    /// Free energy at end of cycle.
    pub free_energy: f32,
    /// Resolution type ("Commit", "Epiphany", "FailureTicket", "Flow").
    pub resolution: String,
    /// Top style that dispatched.
    pub style: String,
    /// Nodes affected this cycle.
    pub affected_nodes: Vec<String>,
    /// New edges committed this cycle.
    pub new_edges: Vec<RenderEdge>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_health() {
        let h = GraphHealth::default();
        assert_eq!(h.node_count, 0);
        assert_eq!(h.contradiction_count, 0);
    }

    #[test]
    fn cypher_error_display() {
        let e = CypherError::ParseError("unexpected token".into());
        assert!(e.to_string().contains("parse error"));
    }

    #[test]
    fn render_node_roundtrip() {
        let n = RenderNode {
            id: "palantir".into(),
            label: "Palantir Technologies".into(),
            kind: "TechCompany".into(),
            confidence: 0.95,
            props: vec![("country".into(), "US".into())],
        };
        assert_eq!(n.id, "palantir");
        assert_eq!(n.props.len(), 1);
    }

    #[test]
    fn snapshot_with_inferences() {
        let snap = GraphSnapshot {
            nodes: vec![
                RenderNode {
                    id: "a".into(),
                    label: "A".into(),
                    kind: "Entity".into(),
                    confidence: 1.0,
                    props: vec![],
                },
                RenderNode {
                    id: "b".into(),
                    label: "B".into(),
                    kind: "Entity".into(),
                    confidence: 1.0,
                    props: vec![],
                },
            ],
            edges: vec![RenderEdge {
                source: "a".into(),
                target: "b".into(),
                label: "knows".into(),
                frequency: 0.9,
                confidence: 0.8,
                inferred: false,
            }],
            inferences: vec![],
            contradictions: vec![],
            timestamp: 42,
        };
        assert_eq!(snap.nodes.len(), 2);
        assert_eq!(snap.edges.len(), 1);
        assert!(!snap.edges[0].inferred);
    }

    #[test]
    fn cypher_result_structure() {
        let result = CypherResult {
            columns: vec!["n".into(), "r".into()],
            rows: vec![vec![
                CypherValue::Text("Alice".into()),
                CypherValue::Text("knows".into()),
            ]],
            nodes_touched: 1,
            relationships_touched: 1,
        };
        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn episodic_trace_creation() {
        let trace = EpisodicTrace {
            step: 10,
            observation: "discovered new entity".into(),
            triplets: vec!["A - knows - B".into()],
            relevance: 0.85,
        };
        assert_eq!(trace.step, 10);
        assert_eq!(trace.triplets.len(), 1);
    }

    #[test]
    fn shader_event_creation() {
        let event = ShaderEvent {
            cycle: 1,
            free_energy: 0.15,
            resolution: "Commit".into(),
            style: "Analytical".into(),
            affected_nodes: vec!["entity_a".into()],
            new_edges: vec![],
        };
        assert!(event.free_energy < 0.2);
        assert_eq!(event.resolution, "Commit");
    }
}
