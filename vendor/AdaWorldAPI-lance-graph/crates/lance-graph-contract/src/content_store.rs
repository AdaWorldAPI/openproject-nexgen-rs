// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright The Lance Authors

//! `content_store` — content-addressed cold text/blob store contract (zero-dep).
//!
//! The episodic/OSINT **text table**: `ContentId` (the `fnv1a` hash of the bytes)
//! → bytes, resolved **cold, at the membrane** — never in the hot path. This is
//! the typed surface for the rule the OGAR canon + `I-VSA-IDENTITIES` Test 0
//! (register laziness) demand: *the reference is the identity, never a serialized
//! pointer/offset inlined in the SoA*.
//!
//! ## Three invariants this encodes
//!
//! 1. **The join key IS the identity.** Nothing variable-length enters the 512 B
//!    node. The node carries only a fixed-size [`ContentId`] (a value tenant);
//!    the text lives in a columnar table next to it and joins by id. No pointer
//!    field, no budget break.
//! 2. **Content-address, not raw GUID.** OSINT sources are shared (one document
//!    backs many observations). [`ContentId::of`] hashes the bytes, so identical
//!    sources dedup (many episodic edges → one source row). Uses [`crate::hash::fnv1a`]
//!    — **stable across versions/platforms** (unlike `DefaultHasher`, which must
//!    never key a content address; see `TECH_DEBT` re `WitnessEntry::tie_break_hash`).
//! 3. **Hot/cold firewall (ADR-022).** [`ContentStore::resolve`] is the COLD /
//!    membrane surface: bytes are materialized only when genuinely needed (LLM
//!    hydration, rendering, citing). The hot path (SIMD sweep, resonance,
//!    AriGraph edge traversal, family-basin routing) touches only the fixed-size
//!    [`ContentId`] + [`SourceSpan`] — the fingerprint is the hot-path stand-in
//!    for the text; this trait is never called during computation.
//!
//! ## Provenance: `SourceSpan` is the typed `(source_id, start, end)`
//!
//! The merged `template-equivalence` provenance model uses
//! `source_spans: Vec<(String, usize, usize)>` = `(source_id, start, end)`.
//! [`SourceSpan`] is its fixed-size typed form: `source_id` IS a [`ContentId`]
//! (the content-table key), `start`/`end` index into the resolved bytes. The
//! gate "no source span → no claim" is literally [`SourceSpan::is_cited`].

use crate::hash::fnv1a;

/// A content address: the `fnv1a`-64 hash of the stored bytes.
///
/// Identical bytes ⇒ identical id ⇒ natural dedup. `ContentId(0)` is the
/// reserved **empty/sentinel** (no content), mirroring the canon's zero-fallback
/// ladder (a zero tier = "not consulted", never a valid address).
///
/// Note: 64-bit fnv1a is the workspace-canonical hash and is sufficient for
/// OSINT-corpus scale; if a corpus ever approaches birthday-collision range
/// (~2^32 distinct sources), widen to a 128-bit content address — the upgrade
/// is local to this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ContentId(pub u64);

impl ContentId {
    /// Content-address arbitrary bytes.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(fnv1a(bytes))
    }

    /// Content-address a string slice.
    #[must_use]
    pub fn of_str(s: &str) -> Self {
        Self(fnv1a(s.as_bytes()))
    }

    /// The reserved empty/sentinel address (no content).
    #[must_use]
    pub fn is_sentinel(self) -> bool {
        self.0 == 0
    }
}

/// A provenance reference: which content, and the `[start, end)` byte span within
/// it. Fixed-size and `Copy` — it lives on the episodic node (a value tenant);
/// the bytes resolve cold via [`ContentStore`]. The typed form of
/// `template-equivalence`'s `(source_id, start, end)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct SourceSpan {
    /// The content-table key (the source the span cites).
    pub content: ContentId,
    /// Inclusive start byte offset into the resolved content.
    pub start: u32,
    /// Exclusive end byte offset.
    pub end: u32,
}

impl SourceSpan {
    /// New span; `end` is clamped to be `>= start`.
    #[must_use]
    pub fn new(content: ContentId, start: u32, end: u32) -> Self {
        Self {
            content,
            start,
            end: end.max(start),
        }
    }

    /// Span length in bytes. Saturating: a malformed span (`end < start`, only
    /// constructible by bypassing [`new`](Self::new) via the public fields)
    /// reports `0`, consistent with [`is_empty`](Self::is_empty) — never panics
    /// (debug) or wraps to a huge value (release).
    #[must_use]
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Whether the span covers zero bytes.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.end <= self.start
    }

    /// "No source span → no claim": a claim is cited iff it carries a non-empty
    /// span into real (non-sentinel) content. The provenance gate's predicate.
    #[must_use]
    pub fn is_cited(self) -> bool {
        !self.content.is_sentinel() && !self.is_empty()
    }
}

/// Failure resolving content from the cold store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentError {
    /// No content stored under this id.
    NotFound,
    /// The span's `[start, end)` exceeds the resolved content's length.
    SpanOutOfBounds,
}

impl core::fmt::Display for ContentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ContentError::NotFound => write!(f, "content-store: id not found"),
            ContentError::SpanOutOfBounds => write!(f, "content-store: span out of bounds"),
        }
    }
}

/// The content-addressed **cold** store (read side).
///
/// Lives in the zero-dep contract so any consumer can declare it without pulling
/// Arrow/Lance. Implemented downstream by a Lance text table (and, in-RAM, by the
/// AriGraph `EpisodicMemory` / `WitnessCorpus` acting as the cold tier).
/// `resolve` returns a borrow into the backing store (mmap'd Lance buffer or
/// in-RAM `Bytes`), so reads are zero-copy at the membrane.
pub trait ContentStore {
    /// Resolve the full content bytes for an id. `None` if absent. COLD path only.
    fn resolve(&self, id: ContentId) -> Option<&[u8]>;

    /// Resolve a span's bytes (cold). Default composes [`resolve`](Self::resolve)
    /// with a bounds check.
    fn resolve_span(&self, span: SourceSpan) -> Result<&[u8], ContentError> {
        let bytes = self.resolve(span.content).ok_or(ContentError::NotFound)?;
        bytes
            .get(span.start as usize..span.end as usize)
            .ok_or(ContentError::SpanOutOfBounds)
    }

    /// Whether an id is present without committing to a borrow shape.
    fn contains(&self, id: ContentId) -> bool {
        self.resolve(id).is_some()
    }
}

/// The content-addressed store (write side, membrane-only).
///
/// Ingest is idempotent by construction: identical bytes ⇒ same [`ContentId`] ⇒
/// dedup (the many-episodes → one-source rule). Writing happens at the cold
/// membrane during ingestion, never on the hot path.
pub trait ContentSink {
    /// Store `bytes`, returning their content address. Idempotent.
    fn put(&mut self, bytes: &[u8]) -> ContentId;

    /// Store a string slice.
    fn put_str(&mut self, s: &str) -> ContentId {
        self.put(s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Reference in-RAM impl (the cold tier mirror) used to exercise the contract.
    #[derive(Default)]
    struct MemStore {
        map: HashMap<ContentId, Vec<u8>>,
    }
    impl ContentStore for MemStore {
        fn resolve(&self, id: ContentId) -> Option<&[u8]> {
            self.map.get(&id).map(Vec::as_slice)
        }
    }
    impl ContentSink for MemStore {
        fn put(&mut self, bytes: &[u8]) -> ContentId {
            let id = ContentId::of(bytes);
            self.map.entry(id).or_insert_with(|| bytes.to_vec());
            id
        }
    }

    #[test]
    fn content_address_is_stable_and_dedups() {
        let a = ContentId::of_str("the same source document");
        let b = ContentId::of_str("the same source document");
        assert_eq!(a, b); // identical bytes ⇒ identical id (dedup key)
        assert_ne!(a, ContentId::of_str("a different document"));
    }

    #[test]
    fn put_is_idempotent_one_row_per_source() {
        let mut s = MemStore::default();
        let id1 = s.put_str("shared OSINT source");
        let id2 = s.put_str("shared OSINT source"); // many episodes → one source
        assert_eq!(id1, id2);
        assert_eq!(s.map.len(), 1);
    }

    #[test]
    fn resolve_span_returns_the_cited_slice() {
        let mut s = MemStore::default();
        let id = s.put_str("Alice met Bob in Paris.");
        let span = SourceSpan::new(id, 10, 13); // "Bob"
        assert_eq!(s.resolve_span(span).unwrap(), b"Bob");
        assert!(span.is_cited());
    }

    #[test]
    fn out_of_bounds_and_missing_fail() {
        let mut s = MemStore::default();
        let id = s.put_str("short");
        assert_eq!(
            s.resolve_span(SourceSpan::new(id, 0, 999)),
            Err(ContentError::SpanOutOfBounds)
        );
        assert_eq!(
            s.resolve_span(SourceSpan::new(ContentId(123), 0, 1)),
            Err(ContentError::NotFound)
        );
    }

    #[test]
    fn uncited_span_is_rejected_by_the_gate() {
        // sentinel content, or empty span ⇒ not a citation
        assert!(!SourceSpan::new(ContentId(0), 0, 5).is_cited());
        assert!(!SourceSpan::new(ContentId(7), 5, 5).is_cited());
        assert!(SourceSpan::new(ContentId(7), 0, 5).is_cited());
    }

    #[test]
    fn malformed_span_len_saturates_not_panics() {
        // Public fields let a consumer build end < start, bypassing new()'s clamp.
        // len() must saturate to 0 (consistent with is_empty), never panic/wrap.
        let bad = SourceSpan {
            content: ContentId(7),
            start: 13,
            end: 0,
        };
        assert_eq!(bad.len(), 0);
        assert!(bad.is_empty());
        assert!(!bad.is_cited());
    }
}
