



//! `Query` — canonical class generated from `ogar_vocab::project_query()`.
//! Fields are the ClassView × FieldMask projection; methods are the OGAR
//! `ActionDef` DO-arm (behaviour is Rust, never SurrealQL DDL).
//! DO NOT EDIT BY HAND. Re-render via `ogar-render-askama`.

/// Canonical concept name as in the OGAR codebook.
pub const CANONICAL_CONCEPT: &str = "project_query";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    /// attribute `name`.
    pub name: String,
    /// attribute `description`.
    pub description: String,
}

impl Query {
    /// Canonical codebook id for this class.
    pub const CLASS_ID: u16 = 0x010D;

    /// Struct-of-methods constructor over the ClassView × FieldMask field set.
    pub fn new(name: String, description: String) -> Self {
        Self { name, description }
    }
}