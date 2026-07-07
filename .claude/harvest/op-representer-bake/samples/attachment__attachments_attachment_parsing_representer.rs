



//! `Attachment` — canonical class generated from `ogar_vocab::project_attachment()`.
//! Fields are the ClassView × FieldMask projection; methods are the OGAR
//! `ActionDef` DO-arm (behaviour is Rust, never SurrealQL DDL).
//! DO NOT EDIT BY HAND. Re-render via `ogar-render-askama`.

/// Canonical concept name as in the OGAR codebook.
pub const CANONICAL_CONCEPT: &str = "project_attachment";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attachment {
    /// attribute `filename`.
    pub filename: String,
    /// attribute `filesize`.
    pub filesize: i64,
    /// attribute `content_type`.
    pub content_type: String,
    /// attribute `digest`.
    pub digest: String,
    /// attribute `description`.
    pub description: String,
    /// attribute `file`.
    pub file: String,
}

impl Attachment {
    /// Canonical codebook id for this class.
    pub const CLASS_ID: u16 = 0x010E;

    /// Struct-of-methods constructor over the ClassView × FieldMask field set.
    pub fn new(filename: String, filesize: i64, content_type: String, digest: String, description: String, file: String) -> Self {
        Self { filename, filesize, content_type, digest, description, file }
    }
}