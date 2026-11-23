# Copied from zammad-rs

`crates/rails-polyfill` and `crates/rails-compiler` are copies of the crates of
the same names in `AdaWorldAPI/zammad-rs`, taken at commit
`e8b19a83dadbf2da38449637b6e357fecf58efb7` (main, after #103). nexgen does not
depend on zammad-rs. The copies are meant to move into OGAR later; until then,
a change made here is not carried to zammad-rs or back automatically.

Carried unchanged: both crates' `src/`, and `rails-compiler/tests/absolute_constants.rs`.
Changed: `Cargo.toml` only: dependency lines (path and git coordinates instead
of zammad-rs's workspace table) and `edition = "2024"`, which zammad-rs sets workspace-wide.

Not carried: `rails-compiler/tests/{baseline_nullability,inflections,parser_recovery}.rs`.
They read zammad-rs's `oracle/` traces and a Zammad checkout (`ZAMMAD_SRC`),
which do not exist in this repository. They keep running in zammad-rs.
