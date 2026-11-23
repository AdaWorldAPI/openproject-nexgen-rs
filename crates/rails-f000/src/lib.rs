// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Rails scopes transposed onto the lance-graph F000 polyfill lens.
//!
//! ```text
//! ruff + OGAR class (ogar_classes)  ->  OgarBasis (identity-carrying ClassView)
//! scope body (Ruby AST)             ->  transpose  (named verdict, ordered Effects)
//! Effects                           ->  Engine     (DT64 BindStore + R2IL native plan)
//! callbacks                         ->  effects    (never a population fold)
//! every scope of an app             ->  matrix     (one auditable row each)
//! ```

pub mod effects;
pub mod exec;
pub mod lanes;
pub mod matrix;
pub mod ogar_basis;
pub mod transpose;

pub use exec::{Compiled, Engine, MethodHandle};
pub use ogar_basis::{FieldMeta, IriMode, OgarBasis};
pub use transpose::{Receiver, RefuseReason, Request, ScopeCatalog, Terminal, Transposed, Verdict, transpose};
