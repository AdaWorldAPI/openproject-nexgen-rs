



//! `Token` — canonical class generated from `ogar_vocab::token()`.
//! Fields are the ClassView × FieldMask projection; methods are the OGAR
//! `ActionDef` DO-arm (behaviour is Rust, never SurrealQL DDL).
//! DO NOT EDIT BY HAND. Re-render via `ogar-render-askama`.

/// Canonical concept name as in the OGAR codebook.
pub const CANONICAL_CONCEPT: &str = "token";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Token {
    /// attribute `value`.
    pub value: String,
}

impl Token {

    /// Struct-of-methods constructor over the ClassView × FieldMask field set.
    pub fn new(value: String) -> Self {
        Self { value }
    }
}