//! OpenProject's `default_scope`s, lowered to what a read applies: the order
//! it sorts by and the filters it keeps. The data is generated from the
//! OpenProject source (`generated/default_scopes.rs`, written by
//! `examples/generate.rs` from rails-f000's scope catalog), never written by
//! hand, so a changed `default_scope` shows up as a changed file.
//!
//! What a scope says that has no effect on which rows a read returns or in
//! which order (`includes`, `preload`, …) is recorded in `no_row_effect`.
//! Anything the lowering cannot read is recorded in `unlowered` with its Ruby
//! text and location, and a read of that table is refused by name rather than
//! answered without it.

/// Ascending or descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Asc,
    Desc,
}

/// One `ORDER BY` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    pub col: &'static str,
    pub dir: Dir,
}

/// A literal a filter compares with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lit {
    Int(i64),
    Str(&'static str),
    Bool(bool),
    Nil,
}

/// `where(col: lit)` is `Eq`; `where.not(col: lit)` is `Ne`. A `Nil` literal
/// is `IS NULL` / `IS NOT NULL`, as in Rails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
}

/// One filter of a default scope, ANDed with the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    pub col: &'static str,
    pub op: Op,
    pub lit: Lit,
}

/// A part of a default scope the lowering could not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unlowered {
    pub ruby: &'static str,
    /// What the lowering could not read.
    pub reason: &'static str,
}

/// Every default scope that applies to a model (its own, then its ancestors'
/// in ActiveRecord's order), lowered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultScope {
    pub model: &'static str,
    pub table: &'static str,
    pub order: &'static [Order],
    pub filters: &'static [Filter],
    /// Ruby text of the parts with no effect on rows or order.
    pub no_row_effect: &'static [&'static str],
    pub unlowered: &'static [Unlowered],
    /// `path:line` of every `default_scope` declaration read.
    pub provenance: &'static [&'static str],
}

/// The lowered default scope of `table`, if its model declares one.
#[must_use]
pub fn of_table(table: &str) -> Option<&'static DefaultScope> {
    crate::generated::default_scopes::DEFAULT_SCOPES
        .iter()
        .find(|s| s.table == table)
}
