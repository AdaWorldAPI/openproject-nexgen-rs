# Copied from zammad-rs

`crates/rails-f000` is a copy of the crate of the same name in
`AdaWorldAPI/zammad-rs`, taken at commit `e8b19a83dadbf2da38449637b6e357fecf58efb7` (main, after #103). nexgen
does not depend on zammad-rs. The copy is meant to move into OGAR later; until
then, a change made here is not carried to zammad-rs or back automatically.

Carried unchanged: `src/` (library, and the `rails-f000-matrix` binary).
Changed: `Cargo.toml` only. Dependency lines use git and path coordinates
(the same ones as zammad-rs's workspace table) instead of `workspace = true`;
`edition = "2024"`, which zammad-rs sets workspace-wide; the `f000-bench`
`[[bin]]` entry and the `zammad-generated` dependency are removed.

Not carried:
- `src/bin/f000-bench.rs` and its `[[bin]]` entry. It is the only user of
  `zammad-generated` in the crate's `src/` (`grep -rn zammad_generated src`
  finds only that file, lines 65, 175, 181), so removing it needed no change to
  any other `src/` file. nexgen must not contain `zammad-generated`.
- `tests/`: all of it (ten test files and `common/mod.rs`), as the task was
  library only. At least `effect_parity.rs` and `negative_oracles.rs` read
  `crates/zammad-generated` sources, which nexgen must not contain; the others
  were not audited one by one. They keep running in zammad-rs.

Note: `src/matrix.rs` and `src/effects.rs` carry zammad-rs's own names and
oracle-file strings in text and match arms; they are carried as is.

## Changes made here since the copy

- `transpose`: a scope with two or more terms is refused ("basis full") when
  the scratch mask for its second term cannot be allocated. zammad-rs reuses
  the accumulator instead, so the second term overwrites the first and the
  count ignores it (Codex, nexgen #166). Test:
  `a_two_term_scope_without_room_for_its_scratch_mask_is_refused`. Not in
  zammad-rs.
- `transpose`: every `default_scope` of a class is checked, not only the
  first; a filtering one in any position refuses the plan, since ActiveRecord
  combines them (checked against ActiveRecord 8.1.4.1: two default scopes
  produce `WHERE active = TRUE AND rank = 7 ORDER BY id`). zammad-rs checks
  only the first (Codex, nexgen #166). Test:
  `a_filtering_second_default_scope_refuses_too`. Not in zammad-rs.
