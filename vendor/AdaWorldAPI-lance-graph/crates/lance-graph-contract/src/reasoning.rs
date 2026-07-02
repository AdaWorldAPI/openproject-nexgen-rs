//! Reasoning contract — adapts existing thinking / faculty / plan
//! surfaces for line-of-business callers. Zero-dep.

use core::future::Future;

pub trait Reasoner: Send + Sync {
    type Conclusion;
    type Error: core::fmt::Debug + Send + Sync + 'static;

    /// Derive a conclusion from a scoped context. Implementations
    /// compose the existing `thinking::*` and `faculty::*` surfaces.
    fn reason<'a>(
        &'a self,
        context: ReasoningContext<'a>,
    ) -> impl Future<Output = Result<Self::Conclusion, Self::Error>> + Send + 'a;
}

pub struct ReasoningContext<'a> {
    /// Jurisdiction / tenant scope.
    pub namespace: &'a str,
    /// Question kind; implementations dispatch on this.
    pub kind: ReasoningKind,
    /// Optional reference to evidence batches (Arrow).
    pub evidence: &'a [EvidenceRef<'a>],
    /// Budget hints.
    pub budget: Budget,
}

#[derive(Clone, Copy, Debug)]
pub enum ReasoningKind {
    CustomerCategory,
    PostingAnomaly,
    NextBestAction,
    InvoiceCompleteness,
    MailIntent,
    Other(u32),
}

pub struct EvidenceRef<'a> {
    pub table: &'a str,
    pub schema_fingerprint: u64,
    pub rows: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_tokens: u32,
    pub max_ms: u32,
    pub max_evidence_rows: u32,
}
