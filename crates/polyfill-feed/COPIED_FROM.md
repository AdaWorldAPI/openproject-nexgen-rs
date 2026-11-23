# Copied from zammad-rs

`crates/polyfill-feed` holds the feed writer of `crates/zammad-parity` in
`AdaWorldAPI/zammad-rs`, taken at commit `e8b19a83dadbf2da38449637b6e357fecf58efb7` (main, after #103). nexgen
does not depend on zammad-rs. The copy is meant to move into OGAR later; until
then, a change made here is not carried to zammad-rs or back automatically.

Carried:
- `src/lib.rs` is `crates/zammad-parity/src/feed.rs`, byte-identical (the file
  already compiles as a crate root; it uses only `rails-polyfill` and
  `serde_json`, and its tests use `super::*`). The module `zammad_parity::feed`
  becomes the crate root `polyfill_feed`.
- `tests/fixtures/float_to_s.{rb,tsv}`, byte-identical. The header comment of
  `float_to_s.rb` still names `zammad_parity::feed::float_text` and
  `crates/zammad-parity/tests/fixtures/` paths; it is a generator for the
  fixture and is not run here.
- `tests/float_text_oracle.rs`. One line changed:
  `use zammad_parity::feed::float_text;` became `use polyfill_feed::float_text;`.

Changed: new `Cargo.toml` (`rails-polyfill = { path = "../rails-polyfill" }`,
`serde_json`, `edition = "2024"`). The two features `feed.rs` tests with
`cfg!(feature = ...)` are declared as in zammad-parity's `Cargo.toml`
(`disable-date-column`, `disable-usec-precision` forwarding to
`rails-polyfill/disable-usec-precision`); without them clippy fails on
`unexpected_cfgs`.

Not carried: the rest of `zammad-parity` (`src/lib.rs`, the other eight test
files), which drive the generated Zammad slice (`zammad-generated`).
