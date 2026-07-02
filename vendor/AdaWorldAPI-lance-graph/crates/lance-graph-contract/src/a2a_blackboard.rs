//! A2A Blackboard — shared workspace for multi-expert coordination.
//!
//! Any consumer that does agent-to-agent communication uses this contract.
//! The blackboard is the shared workspace where experts post results,
//! vote on decisions, and route to the next expert.
//!
//! ```text
//! Expert A posts result → Blackboard
//! Expert B reads, adds → Blackboard
//! Router reads all     → selects top-K experts for next step
//! Consensus merges     → final result
//! ```
//!
//! Zero dependencies. Pure data types + traits.

// ═══════════════════════════════════════════════════════════════════════════
// EXPERT IDENTITY
// ═══════════════════════════════════════════════════════════════════════════

/// Expert identifier. Opaque to the blackboard.
///
/// **Convention:** for agent cards (crewai-rust agents, `.claude/agents/`
/// specialists), `ExpertId = stable_hash_u16(card_yaml)`. This collapses
/// internal A2A experts, external agents, and YAML-defined cards into one
/// identity space. `ExternalRole` carries the family (Rag / CrewaiAgent /
/// N8n / …) at the gate; `ExpertId` carries the specific card on the entry.
///
/// Identity lives in metadata columns (`external_role: UInt8`, `expert_id:
/// UInt16`), not in a packed braid key. Queries over these columns ARE
/// dispatch. VSA binding happens stack-side only — a deterministic metadata →
/// RoleKey slot mapping that never crosses the BBB. See `persona.rs` module
/// docs and plan § 10.6 erratum.
pub type ExpertId = u16;

/// What an expert can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ExpertCapability {
    /// Semantic similarity (embedding distance).
    SemanticSimilarity = 0,
    /// Relevance scoring (cross-encoder reranking).
    RelevanceScoring = 1,
    /// Reasoning topology (LLM weight-derived).
    ReasoningTopology = 2,
    /// Document structure (HTML/markdown parsing).
    DocumentStructure = 3,
    /// Distributional co-occurrence (corpus statistics).
    DistributionalStats = 4,
    /// Knowledge graph (SPO triple lookup).
    KnowledgeGraph = 5,
    /// Style modulation (thinking style selection).
    StyleModulation = 6,
    /// Qualia classification (semantic family assignment).
    QualiaClassification = 7,
    /// External inbound seed — consumer event that triggers a blackboard reasoning cycle.
    /// The entry's `result` field carries an opaque seed handle; the DrainTask resolves it.
    ExternalSeed = 8,
    /// External inbound context — passive consumer event XOR'd into the trajectory bundle
    /// without activating a new reasoning cycle. Same Markov ±5 braiding as grammar tokens.
    ExternalContext = 9,
    /// SMB entity validation (schema + business rules).
    SmbEntityValidation = 10,
    /// SMB lineage tracking (provenance chain).
    SmbLineageTracking = 11,
    /// SMB compliance check (GDPR + cross-border).
    SmbComplianceCheck = 12,
}

/// Expert registration entry.
#[derive(Clone, Debug)]
pub struct ExpertEntry {
    pub id: ExpertId,
    pub capability: ExpertCapability,
    /// Expert's self-reported confidence in this capability (0.0–1.0).
    pub base_confidence: f32,
    /// Weight in mixture (set by router, updated by feedback).
    pub weight: f32,
    /// Number of times this expert has been activated.
    pub activation_count: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// BLACKBOARD ENTRIES
// ═══════════════════════════════════════════════════════════════════════════

/// A single entry posted to the blackboard by an expert.
#[derive(Clone, Debug)]
pub struct BlackboardEntry {
    /// Which expert posted this.
    pub expert_id: ExpertId,
    /// Capability used.
    pub capability: ExpertCapability,
    /// Result: dominant atom/centroid index.
    pub result: u16,
    /// Confidence in this result (0.0–1.0).
    pub confidence: f32,
    /// Top-K supporting atoms.
    pub support: [u16; 4],
    /// Dissonance detected during processing (0.0–1.0).
    pub dissonance: f32,
    /// Processing cost in microseconds.
    pub cost_us: u32,
}

/// The blackboard: shared workspace for all experts.
#[derive(Clone, Debug, Default)]
pub struct Blackboard {
    /// All entries posted this round.
    pub entries: Vec<BlackboardEntry>,
    /// Round counter.
    pub round: u32,
}

impl Blackboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Post a result from an expert.
    pub fn post(&mut self, entry: BlackboardEntry) {
        self.entries.push(entry);
    }

    /// Get all entries from a specific expert.
    pub fn from_expert(&self, id: ExpertId) -> Vec<&BlackboardEntry> {
        self.entries.iter().filter(|e| e.expert_id == id).collect()
    }

    /// Get all entries for a specific capability.
    pub fn by_capability(&self, cap: ExpertCapability) -> Vec<&BlackboardEntry> {
        self.entries
            .iter()
            .filter(|e| e.capability == cap)
            .collect()
    }

    /// Clear for next round.
    pub fn next_round(&mut self) {
        self.entries.clear();
        self.round += 1;
    }

    /// Number of entries this round.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ROUTING
// ═══════════════════════════════════════════════════════════════════════════

/// How to select which experts to activate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingStrategy {
    /// Activate all registered experts.
    All,
    /// Activate top-K by weight.
    TopK(u8),
    /// Activate experts matching specific capabilities.
    ByCapability,
    /// Activate based on previous round's results (feedback loop).
    Adaptive,
}

/// Routing decision: which experts to activate this round.
#[derive(Clone, Debug)]
pub struct RoutingDecision {
    pub strategy: RoutingStrategy,
    pub selected_experts: Vec<ExpertId>,
    pub reason: &'static str,
}

// ═══════════════════════════════════════════════════════════════════════════
// CONSENSUS
// ═══════════════════════════════════════════════════════════════════════════

/// How to merge results from multiple experts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsensusStrategy {
    /// Weighted vote: result with highest weighted confidence wins.
    WeightedVote,
    /// Unanimous: all experts must agree (or flag disagreement).
    Unanimous,
    /// Majority: >50% of experts agree.
    Majority,
    /// Highest confidence: single expert with highest confidence wins.
    HighestConfidence,
}

/// Consensus result from merging expert entries.
#[derive(Clone, Debug)]
pub struct ConsensusResult {
    pub strategy: ConsensusStrategy,
    /// Winning result.
    pub result: u16,
    /// Merged confidence.
    pub confidence: f32,
    /// Agreement level (0.0 = all disagree, 1.0 = unanimous).
    pub agreement: f32,
    /// Number of experts that participated.
    pub expert_count: u8,
    /// Number of experts that agreed with the winner.
    pub agreeing_count: u8,
}

/// Compute consensus from blackboard entries.
pub fn compute_consensus(
    entries: &[BlackboardEntry],
    experts: &[ExpertEntry],
    strategy: ConsensusStrategy,
) -> Option<ConsensusResult> {
    if entries.is_empty() {
        return None;
    }

    match strategy {
        ConsensusStrategy::WeightedVote => {
            // Accumulate weighted votes per result
            let mut votes: std::collections::HashMap<u16, f32> = std::collections::HashMap::new();
            for entry in entries {
                let weight = experts
                    .iter()
                    .find(|e| e.id == entry.expert_id)
                    .map(|e| e.weight)
                    .unwrap_or(1.0);
                *votes.entry(entry.result).or_insert(0.0) += entry.confidence * weight;
            }
            let (&winner, &score) = votes.iter().max_by(|a, b| a.1.partial_cmp(b.1).unwrap())?;
            let total_weight: f32 = votes.values().sum();
            let agreeing = entries.iter().filter(|e| e.result == winner).count();

            Some(ConsensusResult {
                strategy,
                result: winner,
                confidence: score / total_weight.max(0.01),
                agreement: agreeing as f32 / entries.len() as f32,
                expert_count: entries.len() as u8,
                agreeing_count: agreeing as u8,
            })
        }
        ConsensusStrategy::HighestConfidence => {
            let best = entries
                .iter()
                .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())?;
            let agreeing = entries.iter().filter(|e| e.result == best.result).count();
            Some(ConsensusResult {
                strategy,
                result: best.result,
                confidence: best.confidence,
                agreement: agreeing as f32 / entries.len() as f32,
                expert_count: entries.len() as u8,
                agreeing_count: agreeing as u8,
            })
        }
        ConsensusStrategy::Majority | ConsensusStrategy::Unanimous => {
            let mut counts: std::collections::HashMap<u16, usize> =
                std::collections::HashMap::new();
            for e in entries {
                *counts.entry(e.result).or_insert(0) += 1;
            }
            let (&winner, &count) = counts.iter().max_by_key(|e| e.1)?;
            let threshold = match strategy {
                ConsensusStrategy::Unanimous => entries.len(),
                _ => entries.len() / 2 + 1,
            };
            let confidence = if count >= threshold {
                entries
                    .iter()
                    .filter(|e| e.result == winner)
                    .map(|e| e.confidence)
                    .sum::<f32>()
                    / count as f32
            } else {
                0.0
            };

            Some(ConsensusResult {
                strategy,
                result: winner,
                confidence,
                agreement: count as f32 / entries.len() as f32,
                expert_count: entries.len() as u8,
                agreeing_count: count as u8,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blackboard_post_and_read() {
        let mut bb = Blackboard::new();
        bb.post(BlackboardEntry {
            expert_id: 1,
            capability: ExpertCapability::SemanticSimilarity,
            result: 42,
            confidence: 0.9,
            support: [42, 85, 29, 0],
            dissonance: 0.1,
            cost_us: 100,
        });
        assert_eq!(bb.len(), 1);
        assert_eq!(bb.from_expert(1).len(), 1);
        assert_eq!(bb.from_expert(2).len(), 0);
    }

    #[test]
    fn weighted_vote_consensus() {
        let entries = vec![
            BlackboardEntry {
                expert_id: 1,
                capability: ExpertCapability::SemanticSimilarity,
                result: 42,
                confidence: 0.9,
                support: [0; 4],
                dissonance: 0.0,
                cost_us: 0,
            },
            BlackboardEntry {
                expert_id: 2,
                capability: ExpertCapability::RelevanceScoring,
                result: 42,
                confidence: 0.8,
                support: [0; 4],
                dissonance: 0.0,
                cost_us: 0,
            },
            BlackboardEntry {
                expert_id: 3,
                capability: ExpertCapability::ReasoningTopology,
                result: 99,
                confidence: 0.7,
                support: [0; 4],
                dissonance: 0.0,
                cost_us: 0,
            },
        ];
        let experts = vec![
            ExpertEntry {
                id: 1,
                capability: ExpertCapability::SemanticSimilarity,
                base_confidence: 0.9,
                weight: 1.0,
                activation_count: 0,
            },
            ExpertEntry {
                id: 2,
                capability: ExpertCapability::RelevanceScoring,
                base_confidence: 0.8,
                weight: 1.0,
                activation_count: 0,
            },
            ExpertEntry {
                id: 3,
                capability: ExpertCapability::ReasoningTopology,
                base_confidence: 0.7,
                weight: 1.0,
                activation_count: 0,
            },
        ];
        let result =
            compute_consensus(&entries, &experts, ConsensusStrategy::WeightedVote).unwrap();
        assert_eq!(result.result, 42); // 2 experts agree on 42
        assert_eq!(result.agreeing_count, 2);
    }

    #[test]
    fn unanimous_fails_on_disagreement() {
        let entries = vec![
            BlackboardEntry {
                expert_id: 1,
                capability: ExpertCapability::SemanticSimilarity,
                result: 42,
                confidence: 0.9,
                support: [0; 4],
                dissonance: 0.0,
                cost_us: 0,
            },
            BlackboardEntry {
                expert_id: 2,
                capability: ExpertCapability::RelevanceScoring,
                result: 99,
                confidence: 0.8,
                support: [0; 4],
                dissonance: 0.0,
                cost_us: 0,
            },
        ];
        let experts = vec![
            ExpertEntry {
                id: 1,
                capability: ExpertCapability::SemanticSimilarity,
                base_confidence: 0.9,
                weight: 1.0,
                activation_count: 0,
            },
            ExpertEntry {
                id: 2,
                capability: ExpertCapability::RelevanceScoring,
                base_confidence: 0.8,
                weight: 1.0,
                activation_count: 0,
            },
        ];
        let result = compute_consensus(&entries, &experts, ConsensusStrategy::Unanimous).unwrap();
        assert_eq!(result.confidence, 0.0); // not unanimous → zero confidence
    }

    #[test]
    fn next_round_clears() {
        let mut bb = Blackboard::new();
        bb.post(BlackboardEntry {
            expert_id: 1,
            capability: ExpertCapability::SemanticSimilarity,
            result: 42,
            confidence: 0.9,
            support: [0; 4],
            dissonance: 0.0,
            cost_us: 0,
        });
        bb.next_round();
        assert!(bb.is_empty());
        assert_eq!(bb.round, 1);
    }
}
