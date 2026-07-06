



//! `Action` — canonical class generated from `ogar_vocab::action()`.
//! Fields are the ClassView × FieldMask projection; methods are the OGAR
//! `ActionDef` DO-arm (behaviour is Rust, never SurrealQL DDL).
//! DO NOT EDIT BY HAND. Re-render via `ogar-render-askama`.

/// Canonical concept name as in the OGAR codebook.
pub const CANONICAL_CONCEPT: &str = "action";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Action {
    /// attribute `id`.
    pub id: String,
}

impl Action {

    /// Struct-of-methods constructor over the ClassView × FieldMask field set.
    pub fn new(id: String) -> Self {
        Self { id }
    }
}