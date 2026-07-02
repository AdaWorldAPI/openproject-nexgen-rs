//! Sandboxed tax-declaration contract. Zero-dep.

pub trait TaxEngine: Send + Sync {
    type Declaration;
    type Error: core::fmt::Debug + Send + Sync + 'static;

    /// Pure function: same inputs (rule_bundle + period + entries) must
    /// produce the same declaration on every call. Implementations that
    /// cannot guarantee this MUST return `Err(TaxError::Nondeterministic)`.
    fn collect(
        &self,
        rule_bundle_version: &str,
        period: TaxPeriod,
        entries: PostingBatchRef<'_>,
    ) -> Result<Self::Declaration, Self::Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct TaxPeriod {
    pub year: u16,
    /// 1..=4 for quarters, 1..=12 for months. `kind` disambiguates.
    pub ordinal: u8,
    pub kind: PeriodKind,
    pub jurisdiction: Jurisdiction,
}

#[derive(Clone, Copy, Debug)]
pub enum PeriodKind {
    Month,
    Quarter,
    Year,
}

#[derive(Clone, Copy, Debug)]
pub enum Jurisdiction {
    De,
    At,
    Ch,
    Other([u8; 3]),
}

/// Opaque reference to a batch of postings, expected to be an Arrow
/// RecordBatch matching the `fibu_entry` schema. The contract is
/// batch-shaped so SIMD ops on (booking_code, amount, tax_rate)
/// columns stay cache-friendly.
pub struct PostingBatchRef<'a> {
    pub schema_fingerprint: u64,
    pub rows: u64,
    pub _marker: core::marker::PhantomData<&'a ()>,
}

pub trait RuleBundle: Send + Sync {
    /// Stable version string; changing this invalidates cached
    /// declarations. Consumers should include it in any cache key.
    fn version(&self) -> &str;

    /// Compliance-checkable checksum of the rule set.
    fn digest(&self) -> [u8; 32];
}
