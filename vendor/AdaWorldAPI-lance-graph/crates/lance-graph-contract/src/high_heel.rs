//! HighHeelBGZ — 2KB container for family basin nodes in AriGraph.
//!
//! One container = one family basin (TWIG-level centroid) + up to 240 causal edges.
//!
//! ```text
//! HEEL (128 bytes = W0-W15):
//!   W0:      DN address (u64)
//!   W1:      Label hash (u32) + flags (u32)
//!   W2-W14:  SpoBase17 (102 bytes + 2 pad) — the family basin vector
//!   W15:     NARS truth (u8 freq, u8 conf, u8 scent, u8 plasticity, u32 temporal)
//!
//! EDGES (1920 bytes = W16-W255):
//!   W16-W255: up to 240 × CausalEdge64 (8 bytes each)
//! ```
//!
//! HHTL cascade mapping:
//!   HEEL (1 byte scent in W15)   → 95% rejection pre-filter
//!   HIP  (3 bytes palette in W1) → CAKES metric-safe pruning
//!   TWIG (102 bytes SpoBase17)   → family basin L1 distance
//!   LEAF (full 16Kbit planes)    → computed on demand, not stored
//!
//! For streaming: triplets accumulate into basins. Related triplets
//! (Base17 L1 < threshold) merge into the same basin. Edges represent
//! inter-basin causal structure. 240 edges is sufficient for episodic context.
//!
//! For fulltext distillation: chain N × 2KB containers for long documents.
//! Each container is one basin (paragraph cluster), edges link basins across
//! the document. A book = Vec<HighHeelBGZ> where each container is a
//! thematic basin with causal edges to related basins.
//!
//! Zero dependencies. Pure data types.

/// Size of the container in u64 words.
pub const CONTAINER_WORDS: usize = 256;
/// Size of the container in bytes.
pub const CONTAINER_BYTES: usize = CONTAINER_WORDS * 8; // 2048
/// Heel size in words (metadata region).
pub const HEEL_WORDS: usize = 16;
/// Maximum number of CausalEdge64 entries.
pub const MAX_EDGES: usize = CONTAINER_WORDS - HEEL_WORDS; // 240

/// SpoBase17 — three Base17 planes (S, P, O) packed as 102 bytes.
///
/// Each plane is 17 × i16 = 34 bytes. Three planes = 102 bytes.
/// This is the family basin centroid at TWIG level (ρ=0.965 vs full planes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct SpoBase17 {
    /// Subject plane: 17 dimensions.
    pub s: [i16; 17],
    /// Predicate plane: 17 dimensions.
    pub p: [i16; 17],
    /// Object plane: 17 dimensions.
    pub o: [i16; 17],
}

impl SpoBase17 {
    pub const ZERO: Self = Self {
        s: [0i16; 17],
        p: [0i16; 17],
        o: [0i16; 17],
    };

    /// L1 distance between two SpoBase17 vectors (all 3 planes).
    pub fn l1_distance(&self, other: &Self) -> u32 {
        let mut d = 0u32;
        for i in 0..17 {
            d += (self.s[i] as i32 - other.s[i] as i32).unsigned_abs();
            d += (self.p[i] as i32 - other.p[i] as i32).unsigned_abs();
            d += (self.o[i] as i32 - other.o[i] as i32).unsigned_abs();
        }
        d
    }

    /// L1 distance on subject plane only.
    pub fn l1_subject(&self, other: &Self) -> u32 {
        let mut d = 0u32;
        for i in 0..17 {
            d += (self.s[i] as i32 - other.s[i] as i32).unsigned_abs();
        }
        d
    }

    /// Scent byte: 7-bit Boolean lattice of plane proximity.
    /// Bit 0: S close, 1: P close, 2: O close,
    /// 3: SP, 4: SO, 5: PO, 6: SPO.
    pub fn scent(&self, other: &Self, threshold: u32) -> u8 {
        let ds = self.l1_subject(other);
        let dp = {
            let mut d = 0u32;
            for i in 0..17 {
                d += (self.p[i] as i32 - other.p[i] as i32).unsigned_abs();
            }
            d
        };
        let do_ = {
            let mut d = 0u32;
            for i in 0..17 {
                d += (self.o[i] as i32 - other.o[i] as i32).unsigned_abs();
            }
            d
        };
        let s_close = ds < threshold;
        let p_close = dp < threshold;
        let o_close = do_ < threshold;
        let mut b = 0u8;
        if s_close {
            b |= 1;
        }
        if p_close {
            b |= 2;
        }
        if o_close {
            b |= 4;
        }
        if s_close && p_close {
            b |= 8;
        }
        if s_close && o_close {
            b |= 16;
        }
        if p_close && o_close {
            b |= 32;
        }
        if s_close && p_close && o_close {
            b |= 64;
        }
        b
    }
}

/// Heel metadata — the identity + content of a family basin.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Heel {
    /// DN address (W0): identity in the BindSpace.
    pub dn_address: u64,
    /// Label hash (lower 32 bits) + flags (upper 32 bits) (W1).
    /// Flags: bits 0-2 = palette_s, 3-5 = palette_p, 6-8 = palette_o (HIP level).
    pub label_flags: u64,
    /// The family basin vector (W2-W14, 102 bytes + 2 pad).
    pub spo: SpoBase17,
    /// NARS truth + scent + plasticity + temporal (W15).
    /// Byte 0: frequency (u8, 0-255 → 0.0-1.0)
    /// Byte 1: confidence (u8, 0-255 → 0.0-1.0)
    /// Byte 2: scent (u8, 7-bit Boolean lattice)
    /// Byte 3: plasticity (u8, 0=frozen, 1=cooling, 2=warm, 3=hot)
    /// Bytes 4-7: temporal index (u32, basin creation step)
    pub truth_meta: u64,
}

impl Heel {
    /// Extract NARS frequency [0.0, 1.0].
    pub fn frequency(&self) -> f32 {
        (self.truth_meta & 0xFF) as f32 / 255.0
    }

    /// Extract NARS confidence [0.0, 1.0].
    pub fn confidence(&self) -> f32 {
        ((self.truth_meta >> 8) & 0xFF) as f32 / 255.0
    }

    /// Extract scent byte.
    pub fn scent(&self) -> u8 {
        ((self.truth_meta >> 16) & 0xFF) as u8
    }

    /// Extract plasticity state (0=frozen..3=hot).
    pub fn plasticity(&self) -> u8 {
        ((self.truth_meta >> 24) & 0xFF) as u8
    }

    /// Extract temporal index.
    pub fn temporal(&self) -> u32 {
        (self.truth_meta >> 32) as u32
    }

    /// Pack truth_meta from components.
    pub fn pack_truth_meta(freq: f32, conf: f32, scent: u8, plasticity: u8, temporal: u32) -> u64 {
        let f = (freq.clamp(0.0, 1.0) * 255.0) as u64;
        let c = (conf.clamp(0.0, 1.0) * 255.0) as u64;
        f | (c << 8)
            | ((scent as u64) << 16)
            | ((plasticity as u64) << 24)
            | ((temporal as u64) << 32)
    }
}

/// HighHeelBGZ — 2KB container: one family basin + up to 240 causal edges.
///
/// The raw backing store is `[u64; 256]`.
/// W0-W15 = Heel (identity + SpoBase17 basin vector + NARS truth).
/// W16-W255 = CausalEdge64 edges (each is one u64).
///
/// For fulltext distillation, chain containers: `Vec<HighHeelBGZ>`.
/// Each container is a thematic basin, edges link across basins.
#[derive(Clone, Debug)]
pub struct HighHeelBGZ {
    /// The heel: identity, basin vector, truth.
    pub heel: Heel,
    /// Causal edges (up to 240). Each is a packed u64.
    /// Format: CausalEdge64 bit layout (S/P/O palette + NARS + Pearl mask + inference + plasticity + temporal).
    pub edges: Vec<u64>,
}

impl HighHeelBGZ {
    /// Create an empty container with the given DN address and basin vector.
    pub fn new(dn_address: u64, spo: SpoBase17) -> Self {
        Self {
            heel: Heel {
                dn_address,
                label_flags: 0,
                spo,
                truth_meta: Heel::pack_truth_meta(0.5, 0.1, 0, 3, 0), // weak prior, hot plasticity
            },
            edges: Vec::new(),
        }
    }

    /// Add a causal edge. Returns false if at capacity (240).
    pub fn add_edge(&mut self, edge: u64) -> bool {
        if self.edges.len() >= MAX_EDGES {
            return false;
        }
        self.edges.push(edge);
        true
    }

    /// Number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Is this basin frozen (plasticity = 0, truth stable)?
    pub fn is_crystallized(&self) -> bool {
        self.heel.plasticity() == 0 && self.heel.confidence() > 0.8
    }

    /// Update NARS truth via revision (new evidence).
    pub fn revise_truth(&mut self, new_freq: f32, new_conf: f32) {
        let old_f = self.heel.frequency();
        let old_c = self.heel.confidence();
        // NARS revision: weighted average by confidence
        let w1 = old_c;
        let w2 = new_conf;
        let total = w1 + w2;
        if total < 1e-6 {
            return;
        }
        let merged_f = (old_f * w1 + new_freq * w2) / total;
        let merged_c = (total / (total + 1.0)).min(0.99); // confidence approaches but never reaches 1.0
                                                          // Cool plasticity as confidence rises
        let plasticity = if merged_c > 0.8 {
            0
        }
        // frozen
        else if merged_c > 0.6 {
            1
        }
        // cooling
        else if merged_c > 0.3 {
            2
        }
        // warm
        else {
            3
        }; // hot
        self.heel.truth_meta = Heel::pack_truth_meta(
            merged_f,
            merged_c,
            self.heel.scent(),
            plasticity,
            self.heel.temporal(),
        );
    }

    /// Pack to raw `[u64; 256]` wire format.
    pub fn pack(&self) -> [u64; 256] {
        let mut buf = [0u64; 256];
        buf[0] = self.heel.dn_address;
        buf[1] = self.heel.label_flags;
        // Pack SpoBase17 into W2-W14 (102 bytes → 13 words)
        let spo_bytes = spo_to_bytes(&self.heel.spo);
        let mut word_idx = 2;
        let mut byte_idx = 0;
        while byte_idx + 8 <= 104 && word_idx < 15 {
            let mut w = [0u8; 8];
            let end = (byte_idx + 8).min(spo_bytes.len());
            let len = end - byte_idx;
            w[..len].copy_from_slice(&spo_bytes[byte_idx..end]);
            buf[word_idx] = u64::from_le_bytes(w);
            word_idx += 1;
            byte_idx += 8;
        }
        buf[15] = self.heel.truth_meta;
        // Pack edges
        for (i, &edge) in self.edges.iter().enumerate() {
            if i >= MAX_EDGES {
                break;
            }
            buf[16 + i] = edge;
        }
        buf
    }

    /// Unpack from raw `[u64; 256]` wire format.
    pub fn unpack(buf: &[u64; 256]) -> Self {
        let dn_address = buf[0];
        let label_flags = buf[1];
        // Unpack SpoBase17 from W2-W14
        let mut spo_bytes = [0u8; 104]; // 102 + 2 pad
        for i in 0..13 {
            let w = buf[2 + i].to_le_bytes();
            let start = i * 8;
            let end = (start + 8).min(104);
            spo_bytes[start..end].copy_from_slice(&w[..end - start]);
        }
        let spo = bytes_to_spo(&spo_bytes);
        let truth_meta = buf[15];
        // Unpack edges (non-zero entries in W16-W255)
        let edges: Vec<u64> = buf[16..256].iter().copied().filter(|&v| v != 0).collect();
        Self {
            heel: Heel {
                dn_address,
                label_flags,
                spo,
                truth_meta,
            },
            edges,
        }
    }

    /// Total byte size on wire.
    pub const fn wire_size() -> usize {
        CONTAINER_BYTES
    }
}

/// Convert SpoBase17 to 102 bytes (+ 2 padding = 104).
fn spo_to_bytes(spo: &SpoBase17) -> [u8; 104] {
    let mut out = [0u8; 104];
    for i in 0..17 {
        let b = spo.s[i].to_le_bytes();
        out[i * 2] = b[0];
        out[i * 2 + 1] = b[1];
    }
    for i in 0..17 {
        let b = spo.p[i].to_le_bytes();
        out[34 + i * 2] = b[0];
        out[34 + i * 2 + 1] = b[1];
    }
    for i in 0..17 {
        let b = spo.o[i].to_le_bytes();
        out[68 + i * 2] = b[0];
        out[68 + i * 2 + 1] = b[1];
    }
    out
}

/// Convert 104 bytes back to SpoBase17.
fn bytes_to_spo(bytes: &[u8; 104]) -> SpoBase17 {
    let mut spo = SpoBase17::ZERO;
    for i in 0..17 {
        spo.s[i] = i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
    }
    for i in 0..17 {
        spo.p[i] = i16::from_le_bytes([bytes[34 + i * 2], bytes[34 + i * 2 + 1]]);
    }
    for i in 0..17 {
        spo.o[i] = i16::from_le_bytes([bytes[68 + i * 2], bytes[68 + i * 2 + 1]]);
    }
    spo
}

// ═══════════════════════════════════════════════════════════════════════════
// BASIN ACCUMULATOR — streaming triplets into family basins
// ═══════════════════════════════════════════════════════════════════════════

/// Basin merge threshold: if L1 distance < this, triplets belong to same basin.
pub const DEFAULT_BASIN_THRESHOLD: u32 = 2000;

/// Streaming basin accumulator.
///
/// Receives SpoBase17 triplets and merges them into family basins.
/// Each basin is a HighHeelBGZ container. New triplets either join an
/// existing basin (L1 < threshold) or create a new one.
///
/// For fulltext distillation (e.g., Rumi, Tagore via Gutenberg):
/// stream paragraphs → embed as SpoBase17 → accumulate into basins →
/// Vec<HighHeelBGZ> = the distilled document.
pub struct BasinAccumulator {
    /// Active basins.
    pub basins: Vec<HighHeelBGZ>,
    /// Merge threshold (L1 distance).
    pub threshold: u32,
    /// Next DN address to assign.
    next_dn: u64,
    /// Total triplets ingested.
    pub ingested: u64,
    /// Total basins created.
    pub basins_created: u64,
    /// Total merges (triplet joined existing basin).
    pub merges: u64,
}

impl BasinAccumulator {
    pub fn new(threshold: u32) -> Self {
        Self {
            basins: Vec::new(),
            threshold,
            next_dn: 1,
            ingested: 0,
            basins_created: 0,
            merges: 0,
        }
    }

    /// Auto-calibrate threshold from observed pairwise distances.
    /// Call after ingesting a seed batch (e.g., first 10-20 triplets).
    /// Sets threshold to the given percentile of pairwise distances.
    pub fn calibrate(&mut self, percentile: f32) {
        if self.basins.len() < 2 {
            return;
        }
        let mut dists = Vec::new();
        for i in 0..self.basins.len() {
            for j in (i + 1)..self.basins.len() {
                dists.push(
                    self.basins[i]
                        .heel
                        .spo
                        .l1_distance(&self.basins[j].heel.spo),
                );
            }
        }
        if dists.is_empty() {
            return;
        }
        dists.sort_unstable();
        let idx = ((percentile * dists.len() as f32) as usize).min(dists.len() - 1);
        self.threshold = dists[idx];
    }

    /// Ingest a triplet. Merges into nearest basin or creates new one.
    /// Returns the basin index.
    pub fn ingest(&mut self, spo: SpoBase17, edge: u64) -> usize {
        self.ingested += 1;
        // Find nearest basin
        let mut best_idx = None;
        let mut best_dist = u32::MAX;
        for (i, basin) in self.basins.iter().enumerate() {
            let d = basin.heel.spo.l1_distance(&spo);
            if d < best_dist {
                best_dist = d;
                best_idx = Some(i);
            }
        }
        if best_dist < self.threshold {
            let idx = best_idx.unwrap();
            self.basins[idx].add_edge(edge);
            // Move centroid toward new triplet (exponential moving average)
            let n = self.basins[idx].edge_count() as i32;
            let weight = 1.max(n); // weight of existing centroid
            for d in 0..17 {
                self.basins[idx].heel.spo.s[d] = ((self.basins[idx].heel.spo.s[d] as i32 * weight
                    + spo.s[d] as i32)
                    / (weight + 1)) as i16;
                self.basins[idx].heel.spo.p[d] = ((self.basins[idx].heel.spo.p[d] as i32 * weight
                    + spo.p[d] as i32)
                    / (weight + 1)) as i16;
                self.basins[idx].heel.spo.o[d] = ((self.basins[idx].heel.spo.o[d] as i32 * weight
                    + spo.o[d] as i32)
                    / (weight + 1)) as i16;
            }
            // Revise truth: more evidence → higher confidence
            let freq = self.basins[idx].heel.frequency();
            self.basins[idx].revise_truth(freq, 0.3);
            self.merges += 1;
            idx
        } else {
            // New basin
            let dn = self.next_dn;
            self.next_dn += 1;
            let mut basin = HighHeelBGZ::new(dn, spo);
            basin.add_edge(edge);
            self.basins.push(basin);
            self.basins_created += 1;
            self.basins.len() - 1
        }
    }

    /// Get monitoring snapshot.
    pub fn stats(&self) -> BasinStats {
        let crystallized = self.basins.iter().filter(|b| b.is_crystallized()).count();
        let total_edges: usize = self.basins.iter().map(|b| b.edge_count()).sum();
        let avg_edges = if self.basins.is_empty() {
            0.0
        } else {
            total_edges as f32 / self.basins.len() as f32
        };
        BasinStats {
            basin_count: self.basins.len(),
            total_ingested: self.ingested,
            total_merges: self.merges,
            total_edges,
            avg_edges_per_basin: avg_edges,
            crystallized_count: crystallized,
            merge_ratio: if self.ingested == 0 {
                0.0
            } else {
                self.merges as f32 / self.ingested as f32
            },
        }
    }
}

/// Monitoring snapshot for cognitive debugging.
#[derive(Debug, Clone)]
pub struct BasinStats {
    /// Number of active basins.
    pub basin_count: usize,
    /// Total triplets ingested.
    pub total_ingested: u64,
    /// Total merges (triplet → existing basin).
    pub total_merges: u64,
    /// Total edges across all basins.
    pub total_edges: usize,
    /// Average edges per basin.
    pub avg_edges_per_basin: f32,
    /// Basins that have crystallized (frozen, high confidence).
    pub crystallized_count: usize,
    /// Merge ratio: merges / ingested (higher = more consolidation).
    pub merge_ratio: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_spo(s0: i16, p0: i16, o0: i16) -> SpoBase17 {
        let mut spo = SpoBase17::ZERO;
        spo.s[0] = s0;
        spo.p[0] = p0;
        spo.o[0] = o0;
        spo
    }

    #[test]
    fn test_spo_l1_distance() {
        let a = make_spo(100, 200, 300);
        let b = make_spo(110, 200, 300);
        assert_eq!(a.l1_distance(&b), 10); // only s[0] differs by 10
    }

    #[test]
    fn test_spo_self_distance_zero() {
        let a = make_spo(100, 200, 300);
        assert_eq!(a.l1_distance(&a), 0);
    }

    #[test]
    fn test_scent_all_close() {
        let a = make_spo(100, 200, 300);
        let b = make_spo(105, 205, 305);
        let s = a.scent(&b, 1000);
        assert_eq!(s & 0x7F, 0x7F); // all 7 bits set
    }

    #[test]
    fn test_scent_none_close() {
        let a = make_spo(0, 0, 0);
        let b = make_spo(10000, 10000, 10000);
        let s = a.scent(&b, 100);
        assert_eq!(s, 0);
    }

    #[test]
    fn test_heel_truth_pack_unpack() {
        let meta = Heel::pack_truth_meta(0.9, 0.7, 0x3F, 2, 42);
        let heel = Heel {
            dn_address: 1,
            label_flags: 0,
            spo: SpoBase17::ZERO,
            truth_meta: meta,
        };
        assert!((heel.frequency() - 0.9).abs() < 0.01);
        assert!((heel.confidence() - 0.7).abs() < 0.01);
        assert_eq!(heel.scent(), 0x3F);
        assert_eq!(heel.plasticity(), 2);
        assert_eq!(heel.temporal(), 42);
    }

    #[test]
    fn test_container_pack_roundtrip() {
        let spo = make_spo(1234, -5678, 9012);
        let mut c = HighHeelBGZ::new(42, spo);
        c.add_edge(0xDEAD_BEEF_CAFE_BABE);
        c.add_edge(0x1234_5678_9ABC_DEF0);
        let buf = c.pack();
        let c2 = HighHeelBGZ::unpack(&buf);
        assert_eq!(c2.heel.dn_address, 42);
        assert_eq!(c2.heel.spo, spo);
        assert_eq!(c2.edges.len(), 2);
        assert_eq!(c2.edges[0], 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(c2.edges[1], 0x1234_5678_9ABC_DEF0);
    }

    #[test]
    fn test_max_edges() {
        let mut c = HighHeelBGZ::new(1, SpoBase17::ZERO);
        for i in 0..MAX_EDGES {
            assert!(c.add_edge(i as u64 + 1));
        }
        assert!(!c.add_edge(999)); // 241st fails
        assert_eq!(c.edge_count(), MAX_EDGES);
    }

    #[test]
    fn test_revise_truth_increases_confidence() {
        let mut c = HighHeelBGZ::new(1, SpoBase17::ZERO);
        let c0 = c.heel.confidence();
        c.revise_truth(0.8, 0.5);
        let c1 = c.heel.confidence();
        assert!(c1 > c0, "confidence should increase with evidence");
    }

    #[test]
    fn test_crystallization() {
        let mut c = HighHeelBGZ::new(1, SpoBase17::ZERO);
        assert!(!c.is_crystallized());
        // Force high confidence + frozen plasticity
        c.heel.truth_meta = Heel::pack_truth_meta(0.9, 0.95, 0, 0, 0);
        assert!(c.is_crystallized());
    }

    #[test]
    fn test_basin_accumulator_merge() {
        let mut acc = BasinAccumulator::new(500);
        // Two similar triplets → same basin
        let spo1 = make_spo(100, 200, 300);
        let spo2 = make_spo(105, 205, 305); // L1 = 15, well under 500
        acc.ingest(spo1, 0x1111);
        acc.ingest(spo2, 0x2222);
        assert_eq!(acc.basins.len(), 1); // merged
        assert_eq!(acc.basins[0].edge_count(), 2);
        let stats = acc.stats();
        assert_eq!(stats.total_merges, 1);
        assert_eq!(stats.total_ingested, 2);
    }

    #[test]
    fn test_basin_accumulator_split() {
        let mut acc = BasinAccumulator::new(500);
        // Two distant triplets → different basins
        let spo1 = make_spo(100, 200, 300);
        let spo2 = make_spo(10000, 20000, 30000); // very far
        acc.ingest(spo1, 0x1111);
        acc.ingest(spo2, 0x2222);
        assert_eq!(acc.basins.len(), 2); // split
    }

    #[test]
    fn test_basin_stats_monitoring() {
        let mut acc = BasinAccumulator::new(1000);
        for i in 0..10 {
            let spo = make_spo(i * 5, i * 5, i * 5); // close together
            acc.ingest(spo, i as u64);
        }
        let stats = acc.stats();
        assert!(
            stats.merge_ratio > 0.5,
            "most should merge at threshold=1000"
        );
        assert!(
            stats.basin_count < 10,
            "should consolidate into fewer basins"
        );
    }

    #[test]
    fn test_wire_size() {
        assert_eq!(HighHeelBGZ::wire_size(), 2048);
    }

    // ═════════════════════════════════════════════════════════════════════
    // STREAMING EXPERIMENT — increasing complexity, cognitive monitoring
    // ═════════════════════════════════════════════════════════════════════

    /// Simulate text_to_base17: hash text into SpoBase17 with SPO plane separation.
    fn text_to_spo(text: &str) -> SpoBase17 {
        let words: Vec<&str> = text.split_whitespace().collect();
        let n = words.len().max(1);
        let mut dims = [0i64; 51]; // 17×3
        let third = n / 3;
        for (i, word) in words.iter().enumerate() {
            let plane = if i < third {
                0
            } else if i < third * 2 {
                17
            } else {
                34
            };
            for (j, byte) in word.bytes().enumerate() {
                let dim = plane + ((j * 11) % 17);
                dims[dim] += byte as i64 * 31;
            }
        }
        let max_abs = dims.iter().map(|d| d.abs()).max().unwrap_or(1).max(1);
        let scale = 10000.0 / max_abs as f64;
        let mut spo = SpoBase17::ZERO;
        let pack = |v: i64| (v as f64 * scale).round().clamp(-32768.0, 32767.0) as i16;
        for (d, slot) in spo.s.iter_mut().enumerate() {
            *slot = pack(dims[d]);
        }
        for (d, slot) in spo.p.iter_mut().enumerate() {
            *slot = pack(dims[17 + d]);
        }
        for (d, slot) in spo.o.iter_mut().enumerate() {
            *slot = pack(dims[34 + d]);
        }
        spo
    }

    /// Make a fake CausalEdge64 from S/P/O palette indices + NARS truth.
    fn make_edge(s: u8, p: u8, o: u8, freq: u8, conf: u8) -> u64 {
        (s as u64)
            | ((p as u64) << 8)
            | ((o as u64) << 16)
            | ((freq as u64) << 24)
            | ((conf as u64) << 32)
    }

    #[test]
    fn experiment_streaming_increasing_complexity() {
        // ═══ LEVEL 1: Simple facts (low entropy, should consolidate fast) ═══
        let simple_facts = [
            "The cat sits on the mat in the warm kitchen",
            "The cat sleeps on the mat near the warm fire",
            "The cat purrs on the mat beside the warm stove",
            "The dog runs in the park chasing the ball",
            "The dog plays in the park fetching the stick",
            "The dog barks in the park at the squirrel",
        ];

        // Phase 1: Seed with zero threshold (every triplet becomes its own basin)
        let mut acc = BasinAccumulator::new(0); // no merging during seed phase
        for (i, text) in simple_facts.iter().enumerate() {
            let spo = text_to_spo(text);
            let edge = make_edge(i as u8, 1, 2, 200, 180);
            acc.ingest(spo, edge);
        }
        // Auto-calibrate: set threshold to p40 of pairwise distances (merge similar)
        acc.calibrate(0.40);
        eprintln!(
            "  [calibrate] threshold set to {} (p40 of {} basins, {} pairs)",
            acc.threshold,
            acc.basins.len(),
            acc.basins.len() * (acc.basins.len() - 1) / 2
        );
        let stats1 = acc.stats();
        // After seed phase: 6 basins (each fact its own), 0 merges
        // Calibration set threshold — merging starts with next batch

        // ═══ LEVEL 2: Conceptual statements (moderate entropy) ═══
        let concepts = [
            "Love is patient and love is kind it does not envy",
            "Love bears all things believes all things hopes all things",
            "Knowledge speaks but wisdom listens to the silence within",
            "Knowledge is power but wisdom is the light that guides power",
            "Time heals all wounds but memory preserves the scars forever",
            "Time flows like a river carrying all things to the sea",
        ];

        for (i, text) in concepts.iter().enumerate() {
            let spo = text_to_spo(text);
            let edge = make_edge(i as u8 + 10, 5, 8, 150, 120);
            acc.ingest(spo, edge);
        }
        let stats2 = acc.stats();

        // ═══ LEVEL 3: Rumi-style poetry (high entropy, metaphor-dense) ═══
        let rumi = [
            "Out beyond ideas of wrongdoing and rightdoing there is a field",
            "I will meet you there when the soul lies down in that grass",
            "The wound is the place where the light enters you my friend",
            "What you seek is seeking you in the silence of the heart",
            "Let yourself be silently drawn by the strange pull of what you love",
            "Do not be satisfied with the stories that come before you unfold yours",
            "The garden of the world has no limits except in your mind",
            "Yesterday I was clever so I wanted to change the world",
            "Today I am wise so I am changing myself completely",
            "Sell your cleverness and buy bewilderment instead",
        ];

        for (i, text) in rumi.iter().enumerate() {
            let spo = text_to_spo(text);
            let edge = make_edge(i as u8 + 20, 12, 15, 100, 80);
            acc.ingest(spo, edge);
        }
        let stats3 = acc.stats();

        // ═══ LEVEL 4: Tagore-style (different metaphorical space) ═══
        let tagore = [
            "Let my love like sunlight surround you and yet give you illumined freedom",
            "The butterfly counts not months but moments and has time enough",
            "Faith is the bird that feels the light and sings when dawn is dark",
            "You cannot cross the sea by merely standing and staring at water",
            "Clouds come floating into my life to add color to my sunset",
        ];

        for (i, text) in tagore.iter().enumerate() {
            let spo = text_to_spo(text);
            let edge = make_edge(i as u8 + 30, 18, 20, 120, 90);
            acc.ingest(spo, edge);
        }
        let stats4 = acc.stats();

        // ═══ COGNITIVE DEBUG OUTPUT ═══
        eprintln!("\n══════════════════════════════════════════════════════════");
        eprintln!("  HighHeelBGZ Streaming Experiment — Reality Check");
        eprintln!("══════════════════════════════════════════════════════════");
        eprintln!(
            "\nL1 Simple facts:    basins={:2}  merges={:2}  merge_ratio={:.2}  edges={}",
            stats1.basin_count, stats1.total_merges, stats1.merge_ratio, stats1.total_edges
        );
        eprintln!(
            "L2 +Concepts:       basins={:2}  merges={:2}  merge_ratio={:.2}  edges={}",
            stats2.basin_count, stats2.total_merges, stats2.merge_ratio, stats2.total_edges
        );
        eprintln!(
            "L3 +Rumi poetry:    basins={:2}  merges={:2}  merge_ratio={:.2}  edges={}",
            stats3.basin_count, stats3.total_merges, stats3.merge_ratio, stats3.total_edges
        );
        eprintln!(
            "L4 +Tagore poetry:  basins={:2}  merges={:2}  merge_ratio={:.2}  edges={}",
            stats4.basin_count, stats4.total_merges, stats4.merge_ratio, stats4.total_edges
        );

        // Show basin sizes
        eprintln!("\nBasin distribution (edges per basin):");
        let mut sizes: Vec<usize> = acc.basins.iter().map(|b| b.edge_count()).collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        for (i, &size) in sizes.iter().enumerate().take(10) {
            let basin = &acc.basins[i];
            let state = match basin.heel.plasticity() {
                0 => "FROZEN",
                1 => "cooling",
                2 => "warm",
                3 => "HOT",
                _ => "?",
            };
            eprintln!(
                "  basin {:2}: {:2} edges  conf={:.2}  plasticity={}",
                i,
                size,
                basin.heel.confidence(),
                state
            );
        }

        // Check L1 distances between basins
        eprintln!("\nInter-basin L1 distances (first 5×5):");
        let n = acc.basins.len().min(5);
        eprint!("       ");
        for j in 0..n {
            eprint!("  B{:<4}", j);
        }
        eprintln!();
        for i in 0..n {
            eprint!("  B{}: ", i);
            for j in 0..n {
                let d = acc.basins[i].heel.spo.l1_distance(&acc.basins[j].heel.spo);
                eprint!("{:6}", d);
            }
            eprintln!();
        }

        // Crystallization check
        let crystallized = acc.basins.iter().filter(|b| b.is_crystallized()).count();
        eprintln!("\nCrystallized: {}/{}", crystallized, acc.basins.len());

        // ═══ REALITY CHECK ASSERTIONS ═══
        // With 27 inputs (6 seed + 21 post-calibration), expect some consolidation
        assert!(
            stats4.total_edges == 27,
            "FAIL: edge count should match input count, got {}",
            stats4.total_edges
        );

        // The key insight metric: are similar texts actually merging?
        eprintln!("\n══════════════════════════════════════════════════════════");
        eprintln!(
            "  VERDICT: {} basins from 27 inputs (compression: {:.1}x)",
            stats4.basin_count,
            27.0 / stats4.basin_count as f64
        );
        if stats4.merge_ratio < 0.2 {
            eprintln!(
                "  WARNING: Low merge ratio ({:.2}) — threshold may be too tight",
                stats4.merge_ratio
            );
            eprintln!(
                "  SUGGESTION: Increase basin threshold or improve text_to_spo discrimination"
            );
        }
        if stats4.basin_count > 20 {
            eprintln!("  WARNING: Too many basins — texts not clustering meaningfully");
            eprintln!("  SUGGESTION: Check SPO plane separation quality");
        }
        eprintln!("══════════════════════════════════════════════════════════\n");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LENS ICC PROFILE — characterize encoding distortion vs ground truth
// ═══════════════════════════════════════════════════════════════════════════

/// Encoding path that produced a distance table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncodingPath {
    /// burn+GGUF f32 cosine (ground truth, expensive).
    RawF32,
    /// HDR CDF u8 (unsigned, loses sign, gains distribution).
    HdrCdfU8,
    /// Signed i8 (preserves sign, linear quantization).
    SignedI8,
    /// Gamma+phi redistributed (nonlinear, role-aware).
    GammaPhiU8,
    /// Gamma+phi signed (best of both).
    GammaPhiI8,
}

/// Lens ICC Profile: characterizes the distortion of one encoding path
/// relative to ground truth (burn+GGUF f32 cosine).
///
/// Like a camera lens profile in Lightroom: measures the transfer function
/// between "what the weights actually say" and "what the table encodes."
/// The γ offset partially corrects it. The ICC captures the residual.
///
/// Size: ~2KB per lens per role. Total for 6 models × 6 roles = ~72KB.
#[derive(Debug, Clone)]
pub struct LensProfile {
    /// Which model this profile describes.
    pub model_name: String,
    /// Which role (Q, K, V, Gate, Up, Down).
    pub role: String,
    /// Which encoding path.
    pub encoding: EncodingPath,
    /// Transfer function: 256 sample points from cos=-1.0 to cos=+1.0.
    /// Maps ground_truth_cos → encoded_value.
    pub transfer_curve: Vec<f32>,
    /// Inverse: encoded_value → estimated_cos.
    pub inverse_curve: Vec<f32>,
    /// Per-centroid bias: systematic over/under-estimation per row.
    pub centroid_bias: Vec<f32>,
    /// Noise floor: below this absolute cosine, the encoding can't distinguish.
    pub noise_floor: f32,
    /// Effective dynamic range in bits (higher = more discrimination).
    pub effective_bits: f32,
    /// Signed ratio: fraction of negative entries in the raw cosine matrix.
    /// ~0.5 = symmetric (reranker), ~0.1 = positive-skewed (Jina v3).
    pub signed_ratio: f32,
}

impl LensProfile {
    /// Build a profile by comparing encoded table against ground truth cosines.
    ///
    /// `ground_truth`: f32 cosine matrix (n×n, from burn+GGUF or rten)
    /// `encoded`: u8 or i8 distance table (n×n, from our encoding pipeline)
    /// `n`: number of centroids
    pub fn build(
        model_name: &str,
        role: &str,
        encoding: EncodingPath,
        ground_truth: &[f32],
        encoded: &[u8],
        n: usize,
    ) -> Self {
        // Build transfer curve: sample 256 points from cos range
        let mut transfer_curve = vec![0.0f32; 256];
        let mut inverse_curve = vec![0.0f32; 256];
        let mut centroid_bias = vec![0.0f32; n];

        // Collect (cos, encoded) pairs
        let mut pairs: Vec<(f32, u8)> = Vec::new();
        let mut negative_count = 0usize;
        let mut total_count = 0usize;

        for i in 0..n {
            let mut row_error = 0.0f32;
            let mut row_count = 0;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let cos = ground_truth[i * n + j];
                let enc = encoded[i * n + j];
                pairs.push((cos, enc));
                if cos < 0.0 {
                    negative_count += 1;
                }
                total_count += 1;
                // Bias: expected encoded vs actual
                let expected = ((cos + 1.0) / 2.0 * 255.0) as u8; // linear mapping
                row_error += (enc as f32 - expected as f32).abs();
                row_count += 1;
            }
            if row_count > 0 {
                centroid_bias[i] = row_error / row_count as f32;
            }
        }

        // Sort pairs by cosine value
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // Sample transfer curve at 256 equidistant cosine points
        let n_pairs = pairs.len();
        for (k, slot) in transfer_curve.iter_mut().enumerate().take(256) {
            let target_cos = -1.0 + k as f32 * 2.0 / 255.0;
            // Find nearest pair
            let idx = pairs.partition_point(|p| p.0 < target_cos).min(n_pairs - 1);
            *slot = pairs[idx].1 as f32;
            inverse_curve[pairs[idx].1 as usize] = target_cos;
        }

        // Noise floor: smallest cosine difference that produces different encoded values
        let mut noise_floor = 2.0f32;
        for w in pairs.windows(2) {
            if w[0].1 != w[1].1 {
                let delta = (w[1].0 - w[0].0).abs();
                if delta < noise_floor {
                    noise_floor = delta;
                }
            }
        }

        // Effective bits: log2 of distinct encoded values
        let mut seen = [false; 256];
        for &(_, e) in &pairs {
            seen[e as usize] = true;
        }
        let distinct = seen.iter().filter(|&&v| v).count();
        let effective_bits = (distinct as f32).log2();

        let signed_ratio = if total_count > 0 {
            negative_count as f32 / total_count as f32
        } else {
            0.0
        };

        Self {
            model_name: model_name.to_string(),
            role: role.to_string(),
            encoding,
            transfer_curve,
            inverse_curve,
            centroid_bias,
            noise_floor,
            effective_bits,
            signed_ratio,
        }
    }
}

/// Standardized lens configuration for the 6-lane pipeline.
#[derive(Debug, Clone)]
pub struct LensConfig {
    /// Model name (e.g., "jina-v3", "reranker-v3", "qwopus-27b").
    pub name: &'static str,
    /// Model family.
    pub family: LensFamily,
    /// Vocabulary size.
    pub vocab_size: usize,
    /// Number of centroids in the baked table.
    pub n_centroids: usize,
    /// Tokenizer family (determines which tokenizer.json to load).
    pub tokenizer: TokenizerFamily,
    /// Raw cosine range observed in the weight matrix.
    pub cos_range: (f32, f32),
    /// Gamma offset for HDR re-encoding (higher = more resolution near zero).
    pub gamma_offset: f32,
    /// Whether this lens uses signed i8 tables.
    pub is_signed: bool,
    /// Whether this is a truth anchor for cross-model evaluation.
    pub is_truth_anchor: bool,
}

/// Model family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LensFamily {
    /// Embedding model (symmetric similarity).
    Embedding,
    /// Reranker (asymmetric relevance scoring).
    Reranker,
    /// Reader model (HTML → text).
    Reader,
    /// Language model (token generation).
    LanguageModel,
    /// Mixture of Experts language model.
    MoE,
}

/// Tokenizer family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenizerFamily {
    XlmRoberta,
    Qwen2,
    Llama,
    SentencePiece,
}

/// The 6-lane lens registry.
pub static LENS_REGISTRY: &[LensConfig] = &[
    LensConfig {
        name: "jina-v3",
        family: LensFamily::Embedding,
        vocab_size: 250_002,
        n_centroids: 256,
        tokenizer: TokenizerFamily::XlmRoberta,
        cos_range: (-0.067, 0.234),
        gamma_offset: 0.37,
        is_signed: false,
        is_truth_anchor: true,
    },
    LensConfig {
        name: "bge-m3",
        family: LensFamily::Embedding,
        vocab_size: 250_002,
        n_centroids: 256,
        tokenizer: TokenizerFamily::XlmRoberta,
        cos_range: (-0.07, 0.23),
        gamma_offset: 0.40,
        is_signed: false,
        is_truth_anchor: false,
    },
    LensConfig {
        name: "reranker-v3",
        family: LensFamily::Reranker,
        vocab_size: 151_936,
        n_centroids: 256,
        tokenizer: TokenizerFamily::Qwen2,
        cos_range: (-0.886, 0.826),
        gamma_offset: 1.50,
        is_signed: false, // best candidate FOR signed
        is_truth_anchor: false,
    },
    LensConfig {
        name: "reader-lm-1.5b",
        family: LensFamily::Reader,
        vocab_size: 151_936,
        n_centroids: 256,
        tokenizer: TokenizerFamily::Qwen2,
        cos_range: (-0.095, 0.336),
        gamma_offset: 0.12,
        is_signed: false,
        is_truth_anchor: false,
    },
    LensConfig {
        name: "qwopus-27b",
        family: LensFamily::LanguageModel,
        vocab_size: 248_320,
        n_centroids: 4096,
        tokenizer: TokenizerFamily::Qwen2,
        cos_range: (-0.23, 0.18),
        gamma_offset: 1.50,
        is_signed: false,
        is_truth_anchor: false,
    },
    LensConfig {
        name: "maverick-128e",
        family: LensFamily::MoE,
        vocab_size: 202_048,
        n_centroids: 256, // TBD: scale to 4096
        tokenizer: TokenizerFamily::Llama,
        cos_range: (0.0, 0.0), // TBD: stream and measure
        gamma_offset: 0.0,     // TBD: calibrate
        is_signed: false,
        is_truth_anchor: false,
    },
];
