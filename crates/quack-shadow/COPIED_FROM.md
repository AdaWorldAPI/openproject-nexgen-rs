# Copied from zammad-rs

`crates/quack-shadow` is a copy of the crate of the same name in
`AdaWorldAPI/zammad-rs`, taken at commit `e8b19a83dadbf2da38449637b6e357fecf58efb7` (main, after #103). nexgen
does not depend on zammad-rs. The copy is meant to move into OGAR later; until
then, a change made here is not carried to zammad-rs or back automatically.

Carried unchanged: `src/` and all 23 files of `tests/` (including
`tests/common/mod.rs`). No test was edited, so every test path
(`CARGO_MANIFEST_DIR/../../oracle/...`) resolves to this repository's
`oracle/`.
Changed: `Cargo.toml` only: `edition = "2024"` (zammad-rs sets it
workspace-wide). Its dependencies already used git and path coordinates, and
`serde_json = { workspace = true }` resolves against nexgen's workspace table.

Oracle files copied (recorded PostgreSQL captures, self-contained, byte-identical
to zammad-rs's `oracle/`; no name collides with an existing nexgen oracle file,
so they keep their names): `shadow-capture.pg.json`, `shadow-joins.pg.json`,
`shadow-rows.pg.json`, `shadow-scenario.pg.ndjson`,
`shadow-scenario-wide.pg.ndjson`, `shadow-scenario-types.pg.ndjson`,
`statement-census.pg.json`, `statement-census-wide.pg.json`.

Tests that pass without doing their work here: the tests that bind a class map
call `tests/common/mod.rs::basis_of`, which prints `SKIP: ZAMMAD_SRC is not set`
and returns when no Zammad checkout is given (67 SKIP lines in a
`cargo test -p quack-shadow -- --nocapture` run), and `vrow_pg.rs` skips when
no PostgreSQL server answers (1 SKIP line). They count as passes; they are
carried because the replay, join, code, lane and cube tests that need only the
recorded files do run. Setting `ZAMMAD_SRC` here does not work: `basis_of`
reads `zammad.toml` from the repository root, and
`register_census.rs` writes `harvest/register-census-zammad.json`; neither
`zammad.toml` nor `harvest/` exists in nexgen. Those paths are unchanged
from zammad-rs.
