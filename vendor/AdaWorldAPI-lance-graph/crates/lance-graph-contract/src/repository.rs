//! Row-oriented entity store contract.
//!
//! Zero-dep. Implementations (`smb-bridge::MongoConnector`,
//! `smb-bridge::LanceConnector`, future in-memory impls) depend on this
//! crate; this crate depends on nothing.

use core::future::Future;

/// Identifier for an entity within a namespace. Opaque bytes; the
/// canonical mapping for SMB entities is the 12-byte BSON ObjectId.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EntityKey<'a>(pub &'a [u8]);

/// Opaque Arrow-compatible batch reference. The contract does not
/// bind to a specific `arrow` version; implementations cast to
/// their chosen `arrow::record_batch::RecordBatch` type.
pub trait Batch: Send + Sync {
    fn num_rows(&self) -> usize;
    fn schema_fingerprint(&self) -> u64;
}

/// Read-side contract. Implementations stream rows in Arrow chunks.
pub trait EntityStore: Send + Sync {
    type Batch: Batch;
    type Error: core::fmt::Debug + Send + Sync + 'static;

    /// List all tables under a namespace (tenant).
    fn list_tables<'a>(
        &'a self,
        namespace: &'a str,
    ) -> impl Future<Output = Result<Vec<&'a str>, Self::Error>> + Send + 'a;

    /// Scan a table; returns a stream of batches. `limit = None` means
    /// "all rows".
    fn scan<'a>(
        &'a self,
        namespace: &'a str,
        table: &'a str,
        limit: Option<usize>,
    ) -> impl Future<Output = Result<Self::Batch, Self::Error>> + Send + 'a;

    /// Point lookup by key.
    fn get<'a>(
        &'a self,
        namespace: &'a str,
        table: &'a str,
        key: EntityKey<'a>,
    ) -> impl Future<Output = Result<Option<Self::Batch>, Self::Error>> + Send + 'a;
}

/// Write-side contract. Append-only; updates are replace-by-key.
pub trait EntityWriter: EntityStore {
    fn upsert<'a>(
        &'a self,
        namespace: &'a str,
        table: &'a str,
        batch: Self::Batch,
    ) -> impl Future<Output = Result<usize, Self::Error>> + Send + 'a;

    fn delete<'a>(
        &'a self,
        namespace: &'a str,
        table: &'a str,
        key: EntityKey<'a>,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send + 'a;
}
