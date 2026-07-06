



//! `Capability` — canonical class generated from `ogar_vocab::capability()`.
//! Fields are the ClassView × FieldMask projection; methods are the OGAR
//! `ActionDef` DO-arm (behaviour is Rust, never SurrealQL DDL).
//! DO NOT EDIT BY HAND. Re-render via `ogar-render-askama`.

/// Canonical concept name as in the OGAR codebook.
pub const CANONICAL_CONCEPT: &str = "capability";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capability {
    /// attribute `action`.
    pub action: String,
    /// belongs_to `context`.
    pub context: Option<u64>,
}

impl Capability {

    /// Struct-of-methods constructor over the ClassView × FieldMask field set.
    pub fn new(action: String, context: Option<u64>) -> Self {
        Self { action, context }
    }
}