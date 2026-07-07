



//! `Attachment` — canonical class generated from `ogar_vocab::project_attachment()`.
//! Fields are the ClassView × FieldMask projection; methods are the OGAR
//! `ActionDef` DO-arm (behaviour is Rust, never SurrealQL DDL).
//! DO NOT EDIT BY HAND. Re-render via `ogar-render-askama`.

/// Canonical concept name as in the OGAR codebook.
pub const CANONICAL_CONCEPT: &str = "project_attachment";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attachment {
    /// attribute `id`.
    pub id: i64,
    /// attribute `content_type`.
    pub content_type: String,
    /// attribute `digest`.
    pub digest: String,
    /// attribute `created_at`.
    pub created_at: String,
    /// attribute `description`.
    pub description: String,
    /// attribute `status`.
    pub status: i64,
    /// belongs_to `container`.
    pub container: Option<u64>,
    /// belongs_to `author`.
    pub author: Option<u64>,
}

impl Attachment {
    /// Canonical codebook id for this class.
    pub const CLASS_ID: u16 = 0x010E;

    /// Struct-of-methods constructor over the ClassView × FieldMask field set.
    pub fn new(id: i64, content_type: String, digest: String, created_at: String, description: String, status: i64, container: Option<u64>, author: Option<u64>) -> Self {
        Self { id, content_type, digest, created_at, description, status, container, author }
    }
}