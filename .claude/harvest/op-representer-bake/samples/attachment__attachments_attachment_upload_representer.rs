



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
    /// attribute `created_at`.
    pub created_at: String,
    /// attribute `description`.
    pub description: String,
    /// belongs_to `container`.
    pub container: Option<u64>,
    /// belongs_to `author`.
    pub author: Option<u64>,
}

impl Attachment {
    /// Canonical codebook id for this class.
    pub const CLASS_ID: u16 = 0x010E;

    /// Struct-of-methods constructor over the ClassView × FieldMask field set.
    pub fn new(id: i64, created_at: String, description: String, container: Option<u64>, author: Option<u64>) -> Self {
        Self { id, created_at, description, container, author }
    }
}