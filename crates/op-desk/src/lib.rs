//! `op-desk`: the polyfill table and model specs of the OpenProject tables the
//! work-package journey touches, generated from OpenProject's own schema.
//!
//! - [`generated`] is written by `rails-compiler`'s emitter from
//!   `openproject.toml` (repository root) and OpenProject's `db/structure.sql`.
//!   Regenerate with
//!   `OPENPROJECT_SRC=<checkout> cargo run -p op-desk --example generate`;
//!   `tests/regenerate.rs` fails when the committed files differ from a fresh run.
//! - [`structure`] reads `CREATE TABLE` column lists from a PostgreSQL schema
//!   dump and from nexgen's SQL migrations.
//! - [`types`] is the column-type mapping, with `date` and `float` kept as
//!   [`rails_polyfill::ColumnType::Date`] / `Float`.
//! - [`snapshot`] writes a polyfill [`rails_polyfill::Store`] down as plain
//!   data and reads it back.
//!
//! [`desk::Desk`] holds the polyfill store and a Quack catalog over the same
//! rows. op-server holds one and reads the read-only pages from it
//! (`crates/op-server/src/desk_pages.rs`).

pub mod desk;
pub mod reads;
pub mod scopes;
#[rustfmt::skip]
pub mod generated;
pub mod snapshot;
pub mod structure;
pub mod types;

/// The demo seed (`seed/demo.json`): OpenProject's own schema, the same ids as
/// op-server's `kanban_seed.sql`. Load it with [`desk::Desk::from_seed`].
pub const DEMO_SEED: &str = include_str!("../seed/demo.json");

/// The tables the work-package journey reads and writes
/// (`crates/op-server/src/board/executed_db.rs`, `oracle/executed-db.pg.ndjson`).
/// `projects_types` is in nexgen's schema only: current OpenProject dropped it
/// (db/migrate/20260807100000_create_type_variants.rb).
pub const JOURNEY_TABLES: &[&str] = &[
    "enumerations",
    "journals",
    "member_roles",
    "members",
    "non_working_days",
    "projects",
    "projects_types",
    "relations",
    "role_permissions",
    "roles",
    "statuses",
    "types",
    "users",
    "watchers",
    "work_package_journals",
    "work_packages",
];

/// Tables the desk serves to reads only (list pages), beyond the journey:
/// `time_entries` backs the BillableWorkEntry pages (OGAR `billable_work_entry`,
/// OpenProject `TimeEntry` in modules/costs).
pub const READ_TABLES: &[&str] = &["time_entries"];

/// Whether `table` is in the generated slice: a journey table or a read table.
#[must_use]
pub fn is_slice_table(table: &str) -> bool {
    JOURNEY_TABLES.contains(&table) || READ_TABLES.contains(&table)
}
