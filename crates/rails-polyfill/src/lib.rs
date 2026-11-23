//! `rails-polyfill` — Rails/ActiveRecord semantics implemented once, natively.
//!
//! Two halves, both application-independent:
//!
//! - **Runtime** ([`value`], [`record`], [`store`], [`callbacks`],
//!   [`validations`], [`lifecycle`], [`ctx`], [`ruby`]): what generated code
//!   calls. Ruby truthiness and `blank?`, dirty tracking, the ActiveRecord 8.1
//!   save pipeline with its callback phases, `on:` / `if:` / `unless:`,
//!   `throw :abort`, timestamps, partial updates, after-commit ordering,
//!   rollback, association navigation.
//! - **Rules** ([`rules`]): the declarative rewrite rules the compiler applies
//!   to a harvested class before emitting it — concern expansion, the
//!   framework-generated callbacks of each association macro, duplicate
//!   re-positioning, and the relation semantics (`is_a` / `part_of` / role /
//!   lifecycle obligation). They are data with stable ids, so every emitted
//!   fact names the rule that produced it.
//!
//! Nothing here names an application. A second Rails application uses this
//! crate unchanged; that is the point of it.

pub mod callbacks;
pub mod ctx;
pub mod lifecycle;
pub mod record;
pub mod ruby;
pub mod rules;
pub mod schema;
pub mod store;
pub mod validations;
pub mod value;

pub use callbacks::{Action, Callback, Cond, On, Phase, Target};
pub use ctx::{Ctx, Entry, Halt, Host, NoHost};
pub use lifecycle::ModelSpec;
pub use record::Record;
pub use schema::{Column, ColumnType, Default, TableSpec};
pub use store::{Commit, Store, TableChange};
pub use value::{Timestamp, Value};
