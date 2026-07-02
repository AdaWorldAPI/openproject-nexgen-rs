//! CAM-PQ distance contract.
//!
//! Defines the trait for CAM-PQ operations. ndarray provides the codec;
//! lance-graph provides the UDF + storage; consumers call this trait.

/// CAM fingerprint size in bytes.
pub const CAM_SIZE: usize = 6;

/// Number of subspaces.
pub const NUM_SUBSPACES: usize = 6;

/// Number of centroids per subspace.
pub const NUM_CENTROIDS: usize = 256;

/// Minimum element count for a tensor to be worth encoding via CAM-PQ.
/// Below this, codebook storage overhead dominates.
pub const CAM_PQ_MIN_ELEMENTS: u64 = 4096;

/// Routing decision for a single tensor in a model checkpoint.
///
/// Enforces invariant I1 (two regimes): index-regime tensors (embeddings,
/// lm_head) MUST stay Passthrough to preserve identity lookup; argmax-regime
/// tensors (attention Q/K/V/O, MLP gate/up/down) route through CAM-PQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecRoute {
    /// Encode via CAM-PQ: 6-byte fingerprint + per-tensor codebook.
    /// Target: attention projections and MLP feed-forward layers.
    CamPq,
    /// Store as f32 (no compression). Required for index-regime tensors:
    /// embedding table, lm_head output projection, any tensor where row
    /// identity must round-trip exactly.
    Passthrough,
    /// Skip codec entirely — leave as f32 alongside other small tensors.
    /// Target: norms, biases, anything too small to benefit from codec.
    Skip,
}

/// Route a single tensor by name + dimensions.
///
/// Matching rules (applied in order; first match wins):
/// 1. `token_embd`, `embed_tokens`, `lm_head`, `wte`, `wpe` → `Passthrough`.
///    Identity lookup must be exact — no codec can survive Invariant I1.
/// 2. `norm`, `ln_`, `layer_norm` → `Skip`. Small; codec overhead wastes space.
/// 3. Attention `q/k/v/o_proj`, `attn_q/k/v/output`, `self_attn` → `CamPq`.
/// 4. MLP `gate_proj`, `up_proj`, `down_proj`, `ffn_gate/up/down`, `fc1/fc2`,
///    `w1/w2/w3`, generic `mlp`/`ffn` → `CamPq`.
/// 5. 4D tensors (Conv2D kernels) → `Skip` — not our target.
/// 6. Small tensors (< [`CAM_PQ_MIN_ELEMENTS`]) → `Skip`.
/// 7. Ambiguous 2D matrix ≥ `CAM_PQ_MIN_ELEMENTS` → `CamPq` (argmax default).
/// 8. Everything else → `Skip`.
///
/// # Example
///
/// ```
/// use lance_graph_contract::cam::{route_tensor, CodecRoute};
///
/// assert_eq!(route_tensor("model.layers.0.self_attn.q_proj.weight", &[4096, 4096]), CodecRoute::CamPq);
/// assert_eq!(route_tensor("model.embed_tokens.weight", &[151936, 1024]), CodecRoute::Passthrough);
/// assert_eq!(route_tensor("lm_head.weight", &[151936, 1024]), CodecRoute::Passthrough);
/// assert_eq!(route_tensor("model.layers.0.post_attention_layernorm.weight", &[4096]), CodecRoute::Skip);
/// ```
pub fn route_tensor(name: &str, dims: &[u64]) -> CodecRoute {
    // Rule 1: index-regime tensors — must be exact. Check before size/shape
    // rules so lm_head (which is 2D and large) isn't misrouted as CamPq.
    // Use `wte.` / `wpe.` as anchors to avoid matching unrelated 3-letter runs.
    let n_lower = name.to_ascii_lowercase();
    let is_wte_wpe = n_lower == "wte"
        || n_lower == "wpe"
        || n_lower.starts_with("wte.")
        || n_lower.starts_with("wpe.")
        || n_lower.ends_with(".wte")
        || n_lower.ends_with(".wpe")
        || n_lower.contains(".wte.")
        || n_lower.contains(".wpe.");
    if n_lower.contains("token_embd")
        || n_lower.contains("embed_tokens")
        || n_lower.contains("embedding")
        || n_lower.ends_with(".embed.weight")
        || n_lower.contains(".embed.")
        || n_lower.contains("lm_head")
        || is_wte_wpe
    {
        return CodecRoute::Passthrough;
    }

    // Rule 2: norms are small and not worth encoding.
    if n_lower.contains("norm") || n_lower.contains("ln_") || n_lower.contains("layer_norm") {
        return CodecRoute::Skip;
    }

    // Rule 5 (applied early): skip conv kernels.
    if dims.len() == 4 {
        return CodecRoute::Skip;
    }

    // Rule 6: skip anything too small to benefit.
    let total: u64 = dims.iter().product();
    if total < CAM_PQ_MIN_ELEMENTS {
        return CodecRoute::Skip;
    }

    // Rule 3: attention projections.
    if n_lower.contains("q_proj")
        || n_lower.contains("k_proj")
        || n_lower.contains("v_proj")
        || n_lower.contains("o_proj")
        || n_lower.contains("attn_q")
        || n_lower.contains("attn_k")
        || n_lower.contains("attn_v")
        || n_lower.contains("attn_output")
        || n_lower.contains("self_attn")
    {
        return CodecRoute::CamPq;
    }

    // Rule 4: MLP / feed-forward.
    if n_lower.contains("gate_proj")
        || n_lower.contains("up_proj")
        || n_lower.contains("down_proj")
        || n_lower.contains("ffn_gate")
        || n_lower.contains("ffn_up")
        || n_lower.contains("ffn_down")
        || n_lower.contains("mlp")
        || n_lower.contains("ffn")
        || n_lower.contains("fc1")
        || n_lower.contains("fc2")
        || n_lower.contains("w1")
        || n_lower.contains("w2")
        || n_lower.contains("w3")
    {
        return CodecRoute::CamPq;
    }

    // Rule 7: ambiguous 2D matrix that's large enough → CamPq by default.
    if dims.len() == 2 && total >= CAM_PQ_MIN_ELEMENTS {
        return CodecRoute::CamPq;
    }

    CodecRoute::Skip
}

/// Named CAM bytes (stroke positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CamByte {
    Heel = 0,
    Branch = 1,
    TwigA = 2,
    TwigB = 3,
    Leaf = 4,
    Gamma = 5,
}

/// CAM-PQ scan strategy (selected by cost model).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CamStrategy {
    /// Full ADC: 6 table lookups per candidate. Good for < 10M.
    FullAdc,
    /// Stroke cascade: HEEL → BRANCH → full. 99% rejection. Good for > 10M.
    Cascade,
    /// IVF + Cascade: coarse partition probe then cascade. Good for > 100M.
    IvfCascade,
}

/// Precomputed distance tables (6 subspaces × 256 centroids = 6KB).
///
/// Fits in L1 cache. Computed once per query, reused for all candidates.
pub trait DistanceTableProvider: Send + Sync {
    /// Precompute distance tables from query vector + codebook.
    fn precompute(&self, query: &[f32], codebook: &[Vec<Vec<f32>>]) -> [[f32; 256]; 6];

    /// ADC distance to one 6-byte CAM fingerprint using precomputed tables.
    fn distance(&self, tables: &[[f32; 256]; 6], cam: &[u8; 6]) -> f32;

    /// Batch distance for N candidates. Returns distances.
    fn distance_batch(&self, tables: &[[f32; 256]; 6], cams: &[[u8; 6]]) -> Vec<f32>;
}

/// CAM-PQ codec contract.
///
/// ndarray implements this with its AVX-512 kernels.
/// lance-graph-planner uses it via the CamPqScanOp physical operator.
pub trait CamCodecContract: Send + Sync {
    /// Encode a batch of vectors to CAM fingerprints.
    fn encode_batch(&self, vectors: &[Vec<f32>], codebook: &[Vec<Vec<f32>>]) -> Vec<[u8; 6]>;

    /// Decode a CAM fingerprint back to approximate vector.
    fn decode(&self, cam: &[u8; 6], codebook: &[Vec<Vec<f32>>]) -> Vec<f32>;

    /// Train codebook from sample vectors (k-means).
    fn train_codebook(&self, vectors: &[Vec<f32>], dim: usize) -> Vec<Vec<Vec<f32>>>;

    /// Select scan strategy based on candidate count.
    fn select_strategy(&self, num_candidates: u64) -> CamStrategy;
}

/// IVF (Inverted File) contract for billion-scale search.
pub trait IvfContract: Send + Sync {
    /// Train coarse centroids from sample vectors.
    fn train(&self, vectors: &[Vec<f32>], num_partitions: usize);

    /// Assign a vector to its nearest partition.
    fn assign(&self, vector: &[f32]) -> u32;

    /// Find top-P closest partitions for a query.
    fn probe(&self, query: &[f32], num_probes: usize) -> Vec<(u32, f32)>;
}

// ─────────────────────────────────────────────────────────────────────
// Codec sweep parameters (plan: .claude/plans/codec-sweep-via-lab-infra-v1.md)
//
// CodecParams is the sweep-tunable shape the lab API passes to the JIT
// compiler. Consumers (cognitive-shader-driver) serde this from JSON /
// YAML at ingress; everything after ingress is in-memory Rust objects
// (Rule F — serialisation at the edge only).
//
// Zero-dep: no serde derives here. YAML/JSON shape lives in the consumer.
// ─────────────────────────────────────────────────────────────────────

/// SIMD lane width the codec kernel will run on. Mirrors `ndarray::simd::*`
/// lane types; lab Wire DTOs carry this enum verbatim so the JIT compiles
/// against the width the REST handler decoded for (Rule E —
/// Wire surface IS the SIMD surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LaneWidth {
    /// AVX-512 f32 lane — default codec decode / ADC accumulator.
    #[default]
    F32x16,
    /// AVX-512 u8 lane — palette index reads (`tile_dpbusd` input).
    U8x64,
    /// AVX-512 f64 lane — high-precision calibration.
    F64x8,
    /// AVX-512 bf16 lane — required for OPQ rotation (`tile_dpbf16ps`).
    BF16x32,
}

/// Distance metric variant. Per CODING_PRACTICES gap 5: split u8/i8
/// because sign-handling affects bipolar cancellation (codec-findings-
/// 2026-04-20.md §I1 sign-flip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Distance {
    /// Asymmetric distance computation, unsigned palette indices.
    #[default]
    AdcU8,
    /// Asymmetric distance, signed palette indices (bipolar cancellation).
    AdcI8,
}

/// Pre-rotation applied before PQ encoding. Each variant maps to a
/// specific SIMD tier (Rule C — polyfill hierarchy):
///
/// - `Identity` — no-op.
/// - `Hadamard { dim }` — Sylvester butterfly; stays on Tier-3 F32x16.
/// - `Opq { matrix_blob_id, dim }` — learned rotation matmul; Tier-1
///   AMX (`tile_dpbf16ps`) when `ndarray::simd_amx::amx_available()`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Rotation {
    /// No-op pre-rotation.
    #[default]
    Identity,
    /// Sylvester butterfly Hadamard rotation; stays on Tier-3 F32x16.
    Hadamard {
        /// Dimension of the Hadamard matrix (must be a power of 2).
        dim: u32,
    },
    /// Optimized Product Quantization learned rotation (matmul, AMX path).
    Opq {
        /// Reference into the rotation-matrix blob store.
        matrix_blob_id: u64,
        /// Dimension of the rotation matrix.
        dim: u32,
    },
}

impl Rotation {
    /// True when the rotation is a matmul (OPQ) and therefore
    /// benefits from Tier-1 AMX dispatch. Hadamard is add/sub
    /// butterfly — no matmul, no AMX speedup.
    pub fn is_matmul(&self) -> bool {
        matches!(self, Self::Opq { .. })
    }
}

/// Residual PQ refinement pass. `depth = 0` disables residual;
/// `depth > 0` encodes residuals after first-pass decode through
/// another PQ stage (Rule A — composition via JIT; Rule B — stages
/// themselves are `ndarray::simd::*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResidualSpec {
    pub depth: u8,
    pub centroids: u32,
}

impl Default for ResidualSpec {
    fn default() -> Self {
        Self {
            depth: 0,
            centroids: NUM_CENTROIDS as u32,
        }
    }
}

impl ResidualSpec {
    pub fn none() -> Self {
        Self {
            depth: 0,
            centroids: 0,
        }
    }
    pub fn depth(d: u8, centroids: u32) -> Self {
        Self {
            depth: d,
            centroids,
        }
    }
}

/// Full codec parameter shape consumed by the JIT compiler.
///
/// One `CodecParams` per candidate. The `kernel_signature()` method
/// returns a stable hash keyed over the IR-shaping fields; the
/// JIT kernel cache keys on this hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CodecParams {
    pub subspaces: u32,
    pub centroids: u32,
    pub residual: ResidualSpec,
    pub lane_width: LaneWidth,
    pub pre_rotation: Rotation,
    pub distance: Distance,
    pub calibration_rows: u32,
    pub measurement_rows: u32,
    pub seed: u64,
}

/// Errors returned by `CodecParamsBuilder::build()` when validation fails.
/// Precision-ladder rejection fires before any JIT compile (D0.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecParamsError {
    /// `subspaces = 0` or `centroids = 0` — sweep would divide by zero.
    ZeroDimension { field: &'static str },
    /// OPQ requires BF16x32 lane to match `tile_dpbf16ps` tile format
    /// (Rule C Tier 1; D0.7 precision ladder).
    OpqRequiresBf16 { got: LaneWidth },
    /// Hadamard dim must be a power of two (Sylvester construction).
    HadamardDimNotPow2 { dim: u32 },
    /// Overfit guard: pipeline refuses to emit ICC when
    /// `calibration_rows == measurement_rows` (the PR #219 artifact).
    CalibrationEqualsMeasurement { rows: u32 },
}

impl core::fmt::Display for CodecParamsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZeroDimension { field } => write!(f, "codec param `{}` must be non-zero", field),
            Self::OpqRequiresBf16 { got } => write!(
                f,
                "OPQ rotation requires LaneWidth::BF16x32 (tile_dpbf16ps), got {:?}",
                got
            ),
            Self::HadamardDimNotPow2 { dim } => write!(
                f,
                "Hadamard dim must be a power of two (Sylvester), got {}",
                dim
            ),
            Self::CalibrationEqualsMeasurement { rows } => write!(
                f,
                "calibration_rows ({}) must differ from measurement_rows \
                 (would silently reproduce PR #219 overfit)",
                rows
            ),
        }
    }
}

impl core::error::Error for CodecParamsError {}

impl CodecParams {
    /// Stable hash over the IR-shaping fields. JIT kernel cache key.
    ///
    /// Adding an unrelated field (e.g. seed) does NOT invalidate
    /// existing kernel entries — seed is excluded because it does
    /// not shape the emitted IR (only the calibration sample).
    pub fn kernel_signature(&self) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.subspaces.hash(&mut h);
        self.centroids.hash(&mut h);
        self.residual.hash(&mut h);
        self.lane_width.hash(&mut h);
        self.pre_rotation.hash(&mut h);
        self.distance.hash(&mut h);
        // calibration_rows / measurement_rows / seed intentionally excluded.
        h.finish()
    }

    /// True when the kernel will benefit from Tier-1 AMX dispatch
    /// (matmul-heavy: OPQ pre-rotation, or wide codebook > 512).
    pub fn is_matmul_heavy(&self) -> bool {
        self.pre_rotation.is_matmul() || self.centroids > 512
    }
}

/// Fluent builder for `CodecParams`. CODING_PRACTICES gap 3 remediation.
///
/// Programmatic entry point used by sweep driver, tests, and frontier
/// analysis. YAML ingress produces `CodecParams` via serde (in the
/// consumer crate, not here) and does NOT need the builder.
#[derive(Debug, Clone)]
pub struct CodecParamsBuilder {
    subspaces: u32,
    centroids: u32,
    residual: ResidualSpec,
    lane_width: LaneWidth,
    pre_rotation: Rotation,
    distance: Distance,
    calibration_rows: u32,
    measurement_rows: u32,
    seed: u64,
}

impl Default for CodecParamsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CodecParamsBuilder {
    pub fn new() -> Self {
        Self {
            subspaces: NUM_SUBSPACES as u32,
            centroids: NUM_CENTROIDS as u32,
            residual: ResidualSpec::default(),
            lane_width: LaneWidth::default(),
            pre_rotation: Rotation::default(),
            distance: Distance::default(),
            calibration_rows: 2048,
            measurement_rows: 0, // 0 means "use held-out complement"
            seed: 42,
        }
    }
    pub fn subspaces(mut self, n: u32) -> Self {
        self.subspaces = n;
        self
    }
    pub fn centroids(mut self, n: u32) -> Self {
        self.centroids = n;
        self
    }
    pub fn residual(mut self, spec: ResidualSpec) -> Self {
        self.residual = spec;
        self
    }
    pub fn lane_width(mut self, lw: LaneWidth) -> Self {
        self.lane_width = lw;
        self
    }
    pub fn rotation(mut self, r: Rotation) -> Self {
        self.pre_rotation = r;
        self
    }
    pub fn distance(mut self, d: Distance) -> Self {
        self.distance = d;
        self
    }
    pub fn calibration_rows(mut self, n: u32) -> Self {
        self.calibration_rows = n;
        self
    }
    pub fn measurement_rows(mut self, n: u32) -> Self {
        self.measurement_rows = n;
        self
    }
    pub fn seed(mut self, s: u64) -> Self {
        self.seed = s;
        self
    }

    /// Build with precision-ladder validation (D0.7).
    pub fn build(self) -> Result<CodecParams, CodecParamsError> {
        if self.subspaces == 0 {
            return Err(CodecParamsError::ZeroDimension { field: "subspaces" });
        }
        if self.centroids == 0 {
            return Err(CodecParamsError::ZeroDimension { field: "centroids" });
        }
        // Precision ladder: OPQ routes through tile_dpbf16ps → BF16x32 only.
        if matches!(self.pre_rotation, Rotation::Opq { .. })
            && self.lane_width != LaneWidth::BF16x32
        {
            return Err(CodecParamsError::OpqRequiresBf16 {
                got: self.lane_width,
            });
        }
        // Hadamard Sylvester construction needs dim = 2^k.
        if let Rotation::Hadamard { dim } = &self.pre_rotation {
            if *dim == 0 || !dim.is_power_of_two() {
                return Err(CodecParamsError::HadamardDimNotPow2 { dim: *dim });
            }
        }
        // Overfit guard: reject calibration_rows == measurement_rows (PR #219 pattern).
        if self.measurement_rows != 0 && self.calibration_rows == self.measurement_rows {
            return Err(CodecParamsError::CalibrationEqualsMeasurement {
                rows: self.calibration_rows,
            });
        }
        Ok(CodecParams {
            subspaces: self.subspaces,
            centroids: self.centroids,
            residual: self.residual,
            lane_width: self.lane_width,
            pre_rotation: self.pre_rotation,
            distance: self.distance,
            calibration_rows: self.calibration_rows,
            measurement_rows: self.measurement_rows,
            seed: self.seed,
        })
    }
}

#[cfg(test)]
mod codec_params_tests {
    use super::*;

    #[test]
    fn builder_default_matches_pr220_baseline_shape() {
        let p = CodecParamsBuilder::new().build().unwrap();
        assert_eq!(p.subspaces, 6);
        assert_eq!(p.centroids, 256);
        assert_eq!(p.residual.depth, 0);
        assert_eq!(p.pre_rotation, Rotation::Identity);
        assert_eq!(p.distance, Distance::AdcU8);
        assert_eq!(p.lane_width, LaneWidth::F32x16);
    }

    #[test]
    fn builder_zero_subspaces_rejected() {
        let err = CodecParamsBuilder::new().subspaces(0).build().unwrap_err();
        assert!(matches!(
            err,
            CodecParamsError::ZeroDimension { field: "subspaces" }
        ));
    }

    #[test]
    fn builder_zero_centroids_rejected() {
        let err = CodecParamsBuilder::new().centroids(0).build().unwrap_err();
        assert!(matches!(
            err,
            CodecParamsError::ZeroDimension { field: "centroids" }
        ));
    }

    #[test]
    fn opq_with_f32x16_rejected_precision_ladder() {
        // OPQ routes through tile_dpbf16ps — BF16x32 is the only allowed lane.
        let err = CodecParamsBuilder::new()
            .lane_width(LaneWidth::F32x16)
            .rotation(Rotation::Opq {
                matrix_blob_id: 0xDEAD,
                dim: 4096,
            })
            .build()
            .unwrap_err();
        assert!(matches!(
            err,
            CodecParamsError::OpqRequiresBf16 {
                got: LaneWidth::F32x16
            }
        ));
    }

    #[test]
    fn opq_with_bf16x32_accepted() {
        let p = CodecParamsBuilder::new()
            .lane_width(LaneWidth::BF16x32)
            .rotation(Rotation::Opq {
                matrix_blob_id: 0xDEAD,
                dim: 4096,
            })
            .build()
            .unwrap();
        assert!(p.is_matmul_heavy());
    }

    #[test]
    fn hadamard_non_pow2_rejected() {
        let err = CodecParamsBuilder::new()
            .rotation(Rotation::Hadamard { dim: 3000 })
            .build()
            .unwrap_err();
        assert!(matches!(
            err,
            CodecParamsError::HadamardDimNotPow2 { dim: 3000 }
        ));
    }

    #[test]
    fn hadamard_pow2_accepted_stays_on_tier3() {
        let p = CodecParamsBuilder::new()
            .rotation(Rotation::Hadamard { dim: 4096 })
            .build()
            .unwrap();
        // Hadamard is add/sub butterfly — no matmul → no AMX benefit.
        assert!(!p.pre_rotation.is_matmul());
    }

    #[test]
    fn overfit_guard_rejects_calibration_equal_measurement() {
        // Reproduces the PR #219 pattern: trained and tested on same rows.
        // The pipeline must refuse to emit ICC on that configuration.
        let err = CodecParamsBuilder::new()
            .calibration_rows(128)
            .measurement_rows(128)
            .build()
            .unwrap_err();
        assert!(matches!(
            err,
            CodecParamsError::CalibrationEqualsMeasurement { rows: 128 }
        ));
    }

    #[test]
    fn overfit_guard_allows_distinct_row_sets() {
        let p = CodecParamsBuilder::new()
            .calibration_rows(2048)
            .measurement_rows(512)
            .build()
            .unwrap();
        assert_ne!(p.calibration_rows, p.measurement_rows);
    }

    #[test]
    fn kernel_signature_stable_within_process() {
        let a = CodecParamsBuilder::new().centroids(1024).build().unwrap();
        let b = CodecParamsBuilder::new().centroids(1024).build().unwrap();
        assert_eq!(a.kernel_signature(), b.kernel_signature());
    }

    #[test]
    fn kernel_signature_excludes_seed() {
        // Seed changes calibration sample but NOT emitted IR — cache must hit.
        let a = CodecParamsBuilder::new().seed(1).build().unwrap();
        let b = CodecParamsBuilder::new().seed(2).build().unwrap();
        assert_eq!(a.kernel_signature(), b.kernel_signature());
    }

    #[test]
    fn kernel_signature_changes_with_centroids() {
        let a = CodecParamsBuilder::new().centroids(256).build().unwrap();
        let b = CodecParamsBuilder::new().centroids(1024).build().unwrap();
        assert_ne!(a.kernel_signature(), b.kernel_signature());
    }

    #[test]
    fn kernel_signature_changes_with_rotation_kind() {
        let a = CodecParamsBuilder::new()
            .rotation(Rotation::Identity)
            .build()
            .unwrap();
        let b = CodecParamsBuilder::new()
            .rotation(Rotation::Hadamard { dim: 4096 })
            .build()
            .unwrap();
        assert_ne!(a.kernel_signature(), b.kernel_signature());
    }

    #[test]
    fn matmul_heavy_detects_opq_and_wide_codebook() {
        let opq = CodecParamsBuilder::new()
            .lane_width(LaneWidth::BF16x32)
            .rotation(Rotation::Opq {
                matrix_blob_id: 1,
                dim: 4096,
            })
            .build()
            .unwrap();
        assert!(opq.is_matmul_heavy(), "OPQ is matmul-heavy");

        let wide = CodecParamsBuilder::new().centroids(1024).build().unwrap();
        assert!(wide.is_matmul_heavy(), "centroids=1024 is matmul-heavy");

        let narrow = CodecParamsBuilder::new().centroids(256).build().unwrap();
        assert!(
            !narrow.is_matmul_heavy(),
            "narrow codebook + identity is not matmul-heavy"
        );
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;

    #[test]
    fn attention_projections_route_campq() {
        assert_eq!(
            route_tensor("model.layers.0.self_attn.q_proj.weight", &[4096, 4096]),
            CodecRoute::CamPq,
        );
        assert_eq!(
            route_tensor("model.layers.12.self_attn.k_proj.weight", &[4096, 1024]),
            CodecRoute::CamPq,
        );
        assert_eq!(
            route_tensor("model.layers.5.self_attn.v_proj.weight", &[4096, 1024]),
            CodecRoute::CamPq,
        );
        assert_eq!(
            route_tensor("model.layers.0.self_attn.o_proj.weight", &[4096, 4096]),
            CodecRoute::CamPq,
        );
    }

    #[test]
    fn mlp_projections_route_campq() {
        assert_eq!(
            route_tensor("model.layers.0.mlp.gate_proj.weight", &[4096, 11008]),
            CodecRoute::CamPq,
        );
        assert_eq!(
            route_tensor("model.layers.0.mlp.up_proj.weight", &[4096, 11008]),
            CodecRoute::CamPq,
        );
        assert_eq!(
            route_tensor("model.layers.0.mlp.down_proj.weight", &[11008, 4096]),
            CodecRoute::CamPq,
        );
    }

    #[test]
    fn embeddings_stay_passthrough() {
        assert_eq!(
            route_tensor("model.embed_tokens.weight", &[151936, 1024]),
            CodecRoute::Passthrough,
        );
        assert_eq!(
            route_tensor("lm_head.weight", &[151936, 1024]),
            CodecRoute::Passthrough,
        );
        // GGUF naming
        assert_eq!(
            route_tensor("token_embd.weight", &[151936, 1024]),
            CodecRoute::Passthrough,
        );
        // GPT-2 naming
        assert_eq!(
            route_tensor("wte.weight", &[50257, 768]),
            CodecRoute::Passthrough,
        );
        // Generic embedding tables (e.g. Qwen3-TTS codec_embedding)
        assert_eq!(
            route_tensor(
                "talker.code_predictor.model.codec_embedding.0.weight",
                &[2048, 1024]
            ),
            CodecRoute::Passthrough,
        );
        assert_eq!(
            route_tensor("speaker.embedding.weight", &[1000, 256]),
            CodecRoute::Passthrough,
        );
    }

    #[test]
    fn norms_skipped() {
        assert_eq!(
            route_tensor("model.layers.0.input_layernorm.weight", &[4096]),
            CodecRoute::Skip,
        );
        assert_eq!(route_tensor("model.norm.weight", &[4096]), CodecRoute::Skip,);
        assert_eq!(route_tensor("ln_1.weight", &[768]), CodecRoute::Skip,);
    }

    #[test]
    fn small_tensors_skipped() {
        // Under 4096 elements — biases, small projections.
        assert_eq!(
            route_tensor("model.layers.0.self_attn.q_proj.bias", &[256]),
            CodecRoute::Skip,
        );
    }

    #[test]
    fn conv2d_skipped() {
        // 4D tensor — conv kernel, not our target.
        assert_eq!(
            route_tensor("vision.patch_embed.proj.weight", &[768, 3, 16, 16]),
            CodecRoute::Skip,
        );
    }

    #[test]
    fn lm_head_not_misrouted_as_campq() {
        // lm_head is 2D, large, would match the ambiguous-2D fallback.
        // Must be caught by rule 1 first.
        let r = route_tensor("lm_head.weight", &[151936, 4096]);
        assert_eq!(
            r,
            CodecRoute::Passthrough,
            "lm_head must NOT route to CamPq"
        );
    }

    #[test]
    fn ambiguous_large_2d_routes_campq() {
        // Generic 2D weight matrix, no clear role name → argmax default.
        assert_eq!(
            route_tensor("encoder.linear_fallback.weight", &[1024, 1024]),
            CodecRoute::CamPq,
        );
    }
}
