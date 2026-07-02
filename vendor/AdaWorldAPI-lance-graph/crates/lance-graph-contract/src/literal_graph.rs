//! Literal AriGraph — nodes and edges as strings, not vectors.
//!
//! Faithful to the original AriGraph paper: triplets are literal
//! `(subject, relation, object)` with string labels. Embeddings
//! exist alongside for search, never as identity.
//!
//! Compatible with Neo4j Cypher `CREATE (n:Label {props})-[:REL]->(m)`.
//! Compatible with HighHeelBGZ containers (nodes → containers, edges → CausalEdge64).
//!
//! Zero dependencies. Pure data types.

use core::fmt;

/// A literal node in the knowledge graph.
///
/// Maps to Neo4j `(n:Label {id, name, ...})`.
/// The `dn` field is the distinguished name (container address).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LiteralNode {
    /// Unique identifier (e.g., "Palantir", "Lavender", "US").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Node label/type (e.g., "TechCompany", "System", "Nation").
    pub label: String,
    /// Additional properties as key-value pairs.
    pub props: Vec<(String, String)>,
    /// DN address (assigned when stored in HighHeelBGZ container).
    pub dn: u64,
}

/// A literal edge in the knowledge graph.
///
/// Maps to Neo4j `(a)-[:REL_TYPE {props}]->(b)`.
/// Maps to CausalEdge64 when packed into HighHeelBGZ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralEdge {
    /// Source node ID.
    pub source: String,
    /// Target node ID.
    pub target: String,
    /// Relationship type (e.g., "developed", "used in", "part of").
    pub label: String,
    /// Edge weight (if available).
    pub weight: Option<u8>,
    /// Source reference (provenance).
    pub reference: Option<String>,
}

/// A literal knowledge graph — the AriGraph.
///
/// Stores nodes and edges as literal strings. This IS the graph.
/// No sparse vectors, no embeddings pretending to be nodes.
///
/// Nodes are indexed by ID for O(1) lookup.
/// Edges are stored in adjacency lists (source → edges).
pub struct LiteralGraph {
    /// All nodes, keyed by ID.
    nodes: Vec<LiteralNode>,
    /// Node ID → index in nodes vec.
    index: std::collections::HashMap<String, usize>,
    /// All edges.
    edges: Vec<LiteralEdge>,
    /// Source ID → edge indices (adjacency list).
    adj: std::collections::HashMap<String, Vec<usize>>,
    /// Target ID → edge indices (reverse adjacency).
    rev_adj: std::collections::HashMap<String, Vec<usize>>,
    /// Next DN address to assign.
    next_dn: u64,
    /// Label codebook: label string → palette index (u8).
    /// Used when packing to HighHeelBGZ.
    pub label_codebook: Vec<String>,
}

impl LiteralGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            index: std::collections::HashMap::new(),
            edges: Vec::new(),
            adj: std::collections::HashMap::new(),
            rev_adj: std::collections::HashMap::new(),
            next_dn: 1,
            label_codebook: Vec::new(),
        }
    }

    /// Add a node. Assigns DN address. Deduplicates by ID.
    pub fn add_node(&mut self, mut node: LiteralNode) -> u64 {
        if let Some(&idx) = self.index.get(&node.id) {
            return self.nodes[idx].dn;
        }
        node.dn = self.next_dn;
        self.next_dn += 1;
        // Register label in codebook
        self.ensure_label(&node.label);
        let idx = self.nodes.len();
        self.index.insert(node.id.clone(), idx);
        self.nodes.push(node);
        self.nodes[idx].dn
    }

    /// Add an edge. Source and target must exist.
    pub fn add_edge(&mut self, edge: LiteralEdge) -> bool {
        if !self.index.contains_key(&edge.source) || !self.index.contains_key(&edge.target) {
            return false;
        }
        self.ensure_label(&edge.label);
        let idx = self.edges.len();
        self.adj.entry(edge.source.clone()).or_default().push(idx);
        self.rev_adj
            .entry(edge.target.clone())
            .or_default()
            .push(idx);
        self.edges.push(edge);
        true
    }

    fn ensure_label(&mut self, label: &str) {
        if !self.label_codebook.contains(&label.to_string()) {
            self.label_codebook.push(label.to_string());
        }
    }

    /// Get palette index for a label string.
    pub fn label_to_palette(&self, label: &str) -> Option<u8> {
        self.label_codebook
            .iter()
            .position(|l| l == label)
            .map(|i| i as u8)
    }

    /// Get label string from palette index.
    pub fn palette_to_label(&self, idx: u8) -> Option<&str> {
        self.label_codebook.get(idx as usize).map(|s| s.as_str())
    }

    /// Look up node by ID.
    pub fn node(&self, id: &str) -> Option<&LiteralNode> {
        self.index.get(id).map(|&idx| &self.nodes[idx])
    }

    /// Look up node by DN address.
    pub fn node_by_dn(&self, dn: u64) -> Option<&LiteralNode> {
        self.nodes.iter().find(|n| n.dn == dn)
    }

    /// Get all outgoing edges from a node.
    pub fn edges_from(&self, id: &str) -> Vec<&LiteralEdge> {
        self.adj
            .get(id)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    /// Get all incoming edges to a node.
    pub fn edges_to(&self, id: &str) -> Vec<&LiteralEdge> {
        self.rev_adj
            .get(id)
            .map(|indices| indices.iter().map(|&i| &self.edges[i]).collect())
            .unwrap_or_default()
    }

    /// Get all node IDs.
    pub fn all_node_ids(&self) -> Vec<String> {
        self.nodes.iter().map(|n| n.id.clone()).collect()
    }

    /// BFS from seed entities, N hops. Returns associated triplet strings.
    /// Faithful to original AriGraph `get_associated_triplets(items, steps=2)`.
    pub fn get_associated(&self, seeds: &[&str], steps: usize) -> Vec<String> {
        let mut current: std::collections::HashSet<String> =
            seeds.iter().map(|s| s.to_string()).collect();
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for _ in 0..steps {
            let mut next = std::collections::HashSet::new();
            for item in &current {
                // Outgoing
                for edge in self.edges_from(item) {
                    let triplet = format!("{}, {}, {}", edge.source, edge.label, edge.target);
                    if seen.insert(triplet.clone()) {
                        result.push(triplet);
                        next.insert(edge.target.clone());
                    }
                }
                // Incoming
                for edge in self.edges_to(item) {
                    let triplet = format!("{}, {}, {}", edge.source, edge.label, edge.target);
                    if seen.insert(triplet.clone()) {
                        result.push(triplet);
                        next.insert(edge.source.clone());
                    }
                }
            }
            current = next;
        }
        result
    }

    /// Total node count.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    /// Total edge count.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
    /// Codebook size (distinct labels).
    pub fn codebook_size(&self) -> usize {
        self.label_codebook.len()
    }

    /// Print graph stats.
    pub fn stats(&self) -> GraphStats {
        let mut label_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for node in &self.nodes {
            *label_counts.entry(node.label.clone()).or_default() += 1;
        }
        let mut edge_label_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for edge in &self.edges {
            *edge_label_counts.entry(edge.label.clone()).or_default() += 1;
        }
        GraphStats {
            node_count: self.nodes.len(),
            edge_count: self.edges.len(),
            codebook_size: self.label_codebook.len(),
            node_labels: label_counts,
            edge_labels: edge_label_counts,
        }
    }
}

impl Default for LiteralGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Graph statistics snapshot.
#[derive(Debug, Clone)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub codebook_size: usize,
    pub node_labels: std::collections::HashMap<String, usize>,
    pub edge_labels: std::collections::HashMap<String, usize>,
}

impl fmt::Display for GraphStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Nodes: {}, Edges: {}, Codebook: {} labels",
            self.node_count, self.edge_count, self.codebook_size
        )?;
        writeln!(f, "Node labels:")?;
        let mut labels: Vec<_> = self.node_labels.iter().collect();
        labels.sort_by(|a, b| b.1.cmp(a.1));
        for (label, count) in labels.iter().take(10) {
            writeln!(f, "  {}: {}", label, count)?;
        }
        writeln!(f, "Edge labels:")?;
        let mut elabels: Vec<_> = self.edge_labels.iter().collect();
        elabels.sort_by(|a, b| b.1.cmp(a.1));
        for (label, count) in elabels.iter().take(10) {
            writeln!(f, "  {}: {}", label, count)?;
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// JSON INGEST — load aiwar_graph.json into LiteralGraph
// ═══════════════════════════════════════════════════════════════════════════

/// Ingest aiwar_graph.json format into LiteralGraph.
///
/// Expected structure:
/// ```json
/// {
///   "N_Systems": [{"id": "...", "name": "...", "type": "...", ...}],
///   "N_Stakeholders": [{"id": "...", "name": "...", "type": "...", ...}],
///   "E_connection": [{"source": "...", "target": "...", "label": "...", ...}],
///   ...
/// }
/// ```
pub fn ingest_aiwar_json(json_str: &str) -> Result<LiteralGraph, String> {
    // Minimal JSON parsing without serde (zero deps)
    let mut graph = LiteralGraph::new();

    // Parse node arrays
    for node_key in &[
        "N_Systems",
        "N_Civic",
        "N_Historical",
        "N_Stakeholders",
        "N_People",
    ] {
        let category = match *node_key {
            "N_Systems" => "System",
            "N_Civic" => "CivicSystem",
            "N_Historical" => "HistoricalSystem",
            "N_Stakeholders" => "Stakeholder",
            "N_People" => "Person",
            _ => "Unknown",
        };
        for item in extract_array_items(json_str, node_key) {
            let id = extract_field(&item, "id").unwrap_or_default();
            let name = extract_field(&item, "name").unwrap_or_else(|| id.clone());
            let typ = extract_field(&item, "type").unwrap_or_else(|| category.to_string());
            if id.is_empty() {
                continue;
            }
            let mut props = Vec::new();
            if let Some(year) = extract_field(&item, "year") {
                props.push(("year".into(), year));
            }
            if let Some(status) = extract_field(&item, "currentStatus") {
                props.push(("status".into(), status));
            }
            if let Some(mu) = extract_field(&item, "militaryUse") {
                props.push(("military_use".into(), mu));
            }
            if let Some(cu) = extract_field(&item, "civicUse") {
                props.push(("civic_use".into(), cu));
            }
            graph.add_node(LiteralNode {
                id,
                name,
                label: typ,
                props,
                dn: 0,
            });
        }
    }

    // Parse edge arrays
    for edge_key in &[
        "E_connection",
        "E_isDevelopedBy",
        "E_isDeployedBy",
        "E_place",
        "E_people",
    ] {
        for item in extract_array_items(json_str, edge_key) {
            let source = extract_field(&item, "source").unwrap_or_default();
            let target = extract_field(&item, "target").unwrap_or_default();
            let label = extract_field(&item, "label").unwrap_or_else(|| edge_key.replace("E_", ""));
            if source.is_empty() || target.is_empty() {
                continue;
            }
            graph.add_edge(LiteralEdge {
                source,
                target,
                label,
                weight: None,
                reference: None,
            });
        }
    }

    Ok(graph)
}

/// Extract array items from JSON string by key name (minimal parser, no serde).
fn extract_array_items(json: &str, key: &str) -> Vec<String> {
    let pattern = format!("\"{}\"", key);
    let start = match json.find(&pattern) {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    // Find the opening bracket
    let after_key = &json[start + pattern.len()..];
    let bracket_pos = match after_key.find('[') {
        Some(pos) => start + pattern.len() + pos,
        None => return Vec::new(),
    };
    // Find matching closing bracket
    let mut depth = 0;
    let mut end = bracket_pos;
    for (i, c) in json[bracket_pos..].char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = bracket_pos + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let array_str = &json[bracket_pos + 1..end];
    // Split into objects
    let mut items = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in array_str.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    items.push(array_str[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    items
}

/// Extract a string field from a JSON object string (minimal parser).
fn extract_field(obj: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let pos = obj.find(&pattern)?;
    let after = &obj[pos + pattern.len()..];
    // Skip whitespace and colon
    let after = after.trim_start();
    let after = after.strip_prefix(':')?;
    let after = after.trim_start();
    // Check for null/NaN
    if after.starts_with("null") || after.starts_with("NaN") || after.starts_with("nan") {
        return None;
    }
    // String value
    if let Some(rest) = after.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }
    // Number value
    let end = after.find([',', '}', '\n'])?;
    let val = after[..end].trim();
    if val == "NaN" || val == "null" {
        return None;
    }
    Some(val.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_graph_basics() {
        let mut g = LiteralGraph::new();
        let dn1 = g.add_node(LiteralNode {
            id: "Palantir".into(),
            name: "Palantir".into(),
            label: "TechCompany".into(),
            props: vec![],
            dn: 0,
        });
        let dn2 = g.add_node(LiteralNode {
            id: "Gotham".into(),
            name: "Palantir Gotham".into(),
            label: "System".into(),
            props: vec![],
            dn: 0,
        });
        assert_ne!(dn1, dn2);
        assert_eq!(g.node_count(), 2);

        g.add_edge(LiteralEdge {
            source: "Palantir".into(),
            target: "Gotham".into(),
            label: "developed".into(),
            weight: None,
            reference: None,
        });
        assert_eq!(g.edge_count(), 1);

        let assoc = g.get_associated(&["Palantir"], 1);
        assert_eq!(assoc.len(), 1);
        assert!(assoc[0].contains("developed"));
    }

    #[test]
    fn test_dedup_nodes() {
        let mut g = LiteralGraph::new();
        let dn1 = g.add_node(LiteralNode {
            id: "US".into(),
            name: "United States".into(),
            label: "Nation".into(),
            props: vec![],
            dn: 0,
        });
        let dn2 = g.add_node(LiteralNode {
            id: "US".into(),
            name: "USA".into(),
            label: "Nation".into(),
            props: vec![],
            dn: 0,
        });
        assert_eq!(dn1, dn2); // same DN, deduplicated
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn test_bfs_two_hops() {
        let mut g = LiteralGraph::new();
        for id in &["A", "B", "C", "D"] {
            g.add_node(LiteralNode {
                id: id.to_string(),
                name: id.to_string(),
                label: "Node".into(),
                props: vec![],
                dn: 0,
            });
        }
        g.add_edge(LiteralEdge {
            source: "A".into(),
            target: "B".into(),
            label: "knows".into(),
            weight: None,
            reference: None,
        });
        g.add_edge(LiteralEdge {
            source: "B".into(),
            target: "C".into(),
            label: "knows".into(),
            weight: None,
            reference: None,
        });
        g.add_edge(LiteralEdge {
            source: "C".into(),
            target: "D".into(),
            label: "knows".into(),
            weight: None,
            reference: None,
        });

        let hop1 = g.get_associated(&["A"], 1);
        assert_eq!(hop1.len(), 1); // A→B only

        let hop2 = g.get_associated(&["A"], 2);
        assert_eq!(hop2.len(), 2); // A→B, B→C
    }

    #[test]
    fn test_codebook() {
        let mut g = LiteralGraph::new();
        g.add_node(LiteralNode {
            id: "X".into(),
            name: "X".into(),
            label: "System".into(),
            props: vec![],
            dn: 0,
        });
        g.add_node(LiteralNode {
            id: "Y".into(),
            name: "Y".into(),
            label: "Person".into(),
            props: vec![],
            dn: 0,
        });
        g.add_edge(LiteralEdge {
            source: "X".into(),
            target: "Y".into(),
            label: "created_by".into(),
            weight: None,
            reference: None,
        });
        assert_eq!(g.codebook_size(), 3); // System, Person, created_by
        assert_eq!(g.label_to_palette("System"), Some(0));
        assert_eq!(g.palette_to_label(1), Some("Person"));
    }

    #[test]
    fn test_aiwar_json_ingest() {
        let json = r#"{
            "Schema": [],
            "N_Systems": [
                {"id": "Lavender", "name": "Lavender", "type": "PredictiveAnalytics", "year": 2023, "currentStatus": "Operation", "militaryUse": "Intelligence"},
                {"id": "Gospel", "name": "Gospel", "type": "PredictiveAnalytics", "year": 2023}
            ],
            "N_Civic": [],
            "N_Historical": [],
            "N_Stakeholders": [
                {"id": "Israel", "name": "Israel", "type": "Nation"},
                {"id": "UnitNSO", "name": "NSO Group", "type": "TechCompany"}
            ],
            "N_People": [],
            "E_connection": [
                {"source": "UnitNSO", "target": "Israel", "label": "based in"}
            ],
            "E_isDevelopedBy": [
                {"source": "Israel", "target": "Lavender", "label": "developed"},
                {"source": "Israel", "target": "Gospel", "label": "developed"}
            ],
            "E_isDeployedBy": [],
            "E_place": [
                {"source": "Lavender", "target": "Palestine", "label": "used in"}
            ],
            "E_people": [],
            "E_hierarchical": []
        }"#;

        let g = ingest_aiwar_json(json).unwrap();
        assert_eq!(g.node_count(), 4); // Lavender, Gospel, Israel, UnitNSO
        assert_eq!(g.edge_count(), 3); // based in, 2x developed (Palestine not a node so "used in" fails)

        // BFS from Israel
        let assoc = g.get_associated(&["Israel"], 1);
        assert!(
            assoc.len() >= 2,
            "Israel should have >=2 associations, got {}",
            assoc.len()
        );

        let stats = g.stats();
        eprintln!("{}", stats);
    }

    #[test]
    // Miri can't enter `std::fs` under its default isolation (sensible —
    // host filesystem access would let Miri-checked code escape the
    // sandbox). This test reads a real on-disk graph fixture, so it's
    // not a Miri target. Stable / nightly without Miri still run it.
    #[cfg_attr(miri, ignore)]
    fn test_real_aiwar_graph() {
        let json_path = "/root/data/aiwar_graph.json";
        let json = match std::fs::read_to_string(json_path) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("SKIP: {} not found", json_path);
                return;
            }
        };
        let g = ingest_aiwar_json(&json).unwrap();
        let stats = g.stats();
        eprintln!("\n{}", stats);

        eprintln!("=== BFS from Palantir (2 hops) ===");
        for t in g.get_associated(&["Palantir"], 2) {
            eprintln!("  {}", t);
        }

        eprintln!("\n=== BFS from Anthropic (2 hops) ===");
        for t in g.get_associated(&["Anthropic"], 2) {
            eprintln!("  {}", t);
        }

        eprintln!("\n=== BFS from Lavender (2 hops) ===");
        for t in g.get_associated(&["Lavender"], 2).iter().take(15) {
            eprintln!("  {}", t);
        }

        assert!(
            g.node_count() > 200,
            "expected >200 nodes, got {}",
            g.node_count()
        );
        assert!(
            g.edge_count() > 300,
            "expected >300 edges, got {}",
            g.edge_count()
        );
        eprintln!(
            "\nREALITY CHECK: {} nodes, {} edges, {} codebook labels",
            g.node_count(),
            g.edge_count(),
            g.codebook_size()
        );
    }
}
