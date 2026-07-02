//! Container — the BindSpace record unit at 16K width.
//!
//! A Container is a `[u64; 256]` = 16,384 bits = 2 KB, 64-byte aligned.
//! It's the universal address unit — every program, every agent, every
//! shader emits and consumes Containers in the same BindSpace.
//!
//! The Container type is intentionally a type alias for `[u64; 256]`,
//! not a newtype. This keeps it zero-cost and compatible with
//! `ndarray::simd::Fingerprint<256>` (same backing store).
//!
//! CogRecord = metadata Container + content Container = 4 KB.
//! Read-only after construction. Mutations go through CollapseGate.

/// Container = 256 × u64 = 16,384 bits = 2 KB.
/// Same backing as `ndarray::hpc::fingerprint::Fingerprint<256>`.
pub type Container = [u64; 256];

/// Container width in u64 words.
pub const CONTAINER_WORDS: usize = 256;

/// Container width in bits.
pub const CONTAINER_BITS: usize = CONTAINER_WORDS * 64;

/// Container width in bytes.
pub const CONTAINER_BYTES: usize = CONTAINER_WORDS * 8;

/// A cognitive record = metadata + content.
/// 4 KB total. Read-only after construction.
#[derive(Clone, Debug)]
pub struct CogRecord {
    /// Container 0: metadata (identity, NARS, edges, qualia, adjacency).
    pub meta: Container,
    /// Container 1: content (fingerprint, embedding, SPO, whatever geometry says).
    pub content: Container,
}

impl CogRecord {
    /// Create from metadata + content containers.
    pub fn new(meta: Container, content: Container) -> Self {
        Self { meta, content }
    }

    /// Zero record (both containers zeroed).
    pub fn zero() -> Self {
        Self {
            meta: [0u64; 256],
            content: [0u64; 256],
        }
    }

    /// Total byte size.
    pub const BYTE_SIZE: usize = CONTAINER_BYTES * 2; // 4096
}

/// Content geometry: how to interpret Container 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ContentGeometry {
    /// 16K bitpacked fingerprint (standard holographic).
    Bitpacked16K = 0,
    /// Dense f32 embedding (Jina, sentence-transformer). Truncated to fit 2KB.
    DenseF32 = 1,
    /// 3 × Fingerprint (Subject + Predicate + Object decomposition).
    TripleSPO = 2,
    /// Packed edge list (adjacency as content, not metadata).
    EdgePacked = 3,
}
