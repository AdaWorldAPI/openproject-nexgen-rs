# Deferred bugs and known gaps

Every known bug or behaviour gap that was found and deliberately not fixed yet,
with where it was found and why it waits. "Rails" means ActiveRecord 8.1.4.1,
the version OpenProject's Gemfile pins; claims marked *checked* were run
against it (`oracle/lock_rollback.rb` and the scenarios cited in the PRs).

When an item is fixed, delete its row in the fixing PR.

## 1. rails-polyfill (nexgen's copy, `crates/rails-polyfill`)

| # | Gap | Rails behaviour | Found in | Why deferred |
|---|-----|-----------------|----------|--------------|
| P1 | `save` on a destroyed record is not refused | `save` returns false for a destroyed record (`persistence.rb:925`) | #165 | Not needed by the work-package journey yet |
| P2 | No destroy re-entrancy guard | `@_destroy_callback_already_called` stops a destroy started from inside its own callbacks (`callbacks.rb:419-427`) | #165 | Same |
| P3 | Readonly records are not modelled | `readonly!` / `readonly?` make `save` and `destroy` raise `ReadOnlyRecord` | #165 | Same |
| P4 | A destroyed record is not frozen | `destroy` freezes the record; assigning to it raises `FrozenError` | #165 | Same |
| P5 | `around_save` / `around_create` / `around_update` / `around_destroy` run in the before slot | An around callback wraps the inner chain and the write (`yield`) | #165 | Pre-existing in the zammad-rs polyfill (`AroundSave`); needs a callback runner that can resume |
| P6 | When an exception from a nested operation reaches the outermost transaction, only the outermost record's state is restored | `rolledback!(force_restore_state:)` runs for every record that joined the transaction (`transactions.rb:415-423`) | #165 | Needs the transaction to keep each participant's restore snapshot |
| P7 | `lock_optimistically = false` per model is not modelled | A model can turn locking off while having a `lock_version` column | #165 | `ModelSpec` has no per-model flag; presence of the column is the switch |
| P8 | On a locked table the stale check runs before the NOT NULL check | Not checked against Rails | #165 | Order only matters when an update is both stale and violates NOT NULL |
| P9 | **Not on main yet:** transaction participants are tracked by memory address, which merges records built one after another in a loop (only one `after_commit` runs), and a nested save that raises after its write loses its `after_commit` | Participants are objects; a written row's `after_commit` runs (*checked*: `loop creates`, `rescued save`) | Codex on #165, after merge | Fix is commit 6ce2673, landed after #165 merged; re-landed as its own PR (`claude/polyfill-participant-tokens`) |
| P10 | `run_commit_callbacks_on_first_saved_instances_in_transaction = false` is not modelled: when one row is saved through several instances, after_commit always runs on the first one saved | With `false`, the last instance saved runs it, and inherits `_new_record_before_last_commit` from an earlier one (`transaction.rb:359-383`; *checked*: oracle `same row twice`) | Codex on #168 | `true` is the class default and OpenProject's effective value (`load_defaults 6.1`; the 7.1 override is commented out); `ModelSpec` has no per-model flag |

## 2. Copied crates: bugs zammad-rs still has

Fixed only in nexgen's copies. zammad-rs keeps them until someone ports the
fixes (or the crates move to OGAR, where the nexgen versions should be the
starting point).

| # | Bug | Fixed in nexgen by | zammad-rs file |
|---|-----|--------------------|----------------|
| Z1 | Optimistic locking, `destroy` / `destroy!` and their callbacks missing | #165 | `rails-polyfill/src/{store,lifecycle}.rs` |
| Z2 | A rolled-back `save` / `destroy` leaves the record's dirty baseline, `new_record?` and `saved_changes` as the failed operation set them (*checked*) | #165 | `rails-polyfill/src/lifecycle.rs` |
| Z3 | A failed nested `save` / `destroy` rolls the store back to its own snapshot; Rails rolls nothing back below the outermost transaction (*checked*) | #165 | same |
| Z4 | `after_commit` is queued per operation, not once per participating record (double callbacks for save-then-destroy, lost callbacks after a rescued raise) (*checked*) | #165, plus the participant-token PR (P9) | same |
| Z5 | Saved time precision: a saved record keeps sub-precision microseconds that the row truncated | zammad-rs #103 (fixed there too) | — |
| Z6 | `rails-f000` transpose: a two-term scope with no room for its scratch mask reuses the accumulator, so the count ignores the first term | #166 (`7d2ba55`) | `rails-f000/src/transpose.rs` |
| Z7 | `rails-f000`: only the first `default_scope` of a class is checked (*checked*) | #166 (`73a7b28`) | same |
| Z8 | `rails-f000`: default scopes from included concerns and superclasses are not checked (*checked*) | #167 | same |
| Z9 | `quack-shadow` `apply_entry`: feed cells are read by position, ignoring the entry's `columns`; a different column order updates nothing and invents a row | #167 | `quack-shadow/src/lib.rs` |

## 3. Copied crates: test coverage that does not run here

| # | Gap | Why |
|---|-----|-----|
| T1 | 67 `quack-shadow` tests print `SKIP: ZAMMAD_SRC is not set` and pass | They bind Zammad's class map (`zammad.toml`, a Zammad checkout); nexgen has neither. Replacing them needs an OpenProject class map and recorded OpenProject captures |
| T2 | `quack-shadow` `vrow_pg` skips | Needs a PostgreSQL server; CI's `executed-db` job has one but does not run this test |
| T3 | `rails-f000` tests not carried | Two read `zammad-generated` sources; the rest were not audited one by one (`crates/rails-f000/COPIED_FROM.md`) |
| T4 | Three `rails-compiler` tests not carried | They read zammad-rs's `oracle/` traces and a Zammad checkout (`crates/rails-compiler/COPIED_FROM.md`) |

## 4. rails-compiler (nexgen's copy) — needed by op-desk

| # | Gap | Workaround today |
|---|-----|------------------|
| C1 | Emitter maps `date` to DateTime and `float` to Decimal | `op_desk::types::polyfill_type` rewrites them after emission |
| C2 | No reader for `db/structure.sql` or for OpenProject's `Tables::X.table(migration)` migration form (both read as 0 tables) | `op_desk::structure` parses `oracle/openproject-structure.sql` |
| C3 | Keyword module names emitted unescaped (`pub mod type;`) | op-desk post-processing writes `r#type` |
| C4 | Regenerate header hardcodes Zammad (`ZAMMAD_SRC`, `zammad.toml`) | op-desk post-processing rewrites it |
| C5 | `Unit::superclass` is stored as a string, so `class X < ::Base` loses its leading `::`; rails-f000 then resolves it lexically and can pick `Ns::Base` (includes are handled from the AST since #167) | None. OpenProject has ~10 such classes under `app/models` (e.g. `class Status < ::Exports::Formatters::Default`); all found so far are export formatters, not ActiveRecord models, so none carries a default_scope today |

## 5. Migration (`docs/plans/polyfill-quack-migration.md`)

| # | Gap | Status |
|---|-----|--------|
| M1 | nexgen's SQL schema differs from OpenProject's in 111 places (`projects_types` exists only in nexgen; `done_ratio` NOT NULL in nexgen, nullable in OpenProject; …) | Pinned in `oracle/op-desk-schema-divergence.txt`; the owner chose OpenProject's schema as canonical; each difference is resolved as the runtime moves to op-desk |
| M2 | `timestamptz` columns: Quack refuses rendering `timestamp with time zone` (`Refusal::Unrenderable`) | Plan §3 gap 2; decision (feed as `timestamp(6)` or wait for Quack) still open |
| M3 | `journals.validity_period` (`tstzrange`) and `journals.cause` (`jsonb`) have no polyfill type | op-desk maps them to Text / Json; plan §3 gap 4 decision open |
| M4 | Quack refuses `ORDER BY` on text (`TextOrder`) and filters on unlaned columns (date, float, jsonb) | API sorts by subject/name must sort in Rust after `rows`, or stay refused (plan §3 gaps 7–8) |

## 6. WorkPackage::Create reconciliation (`oracle/charon/reconcile-create-admission.ledger.json`)

Scorecard: 20 claims; 8 resolved, 1 evidenced, 4 open (plus structural slag).

| # | Claim key | Owner | Blocker |
|---|-----------|-------|---------|
| R1 | `…/guard/admit_create/evaluates:writable?-per-submitted-attribute` | ContractHarvest | `BaseContract#writable?` is not lowered by ruff; scope (base contract vs effective create contract) undecided |
| R2 | `…/admission/mutation::create/checks:add_work_packages-in-project` | RustExtraction | Charon cannot structure the async body of `op_services::work_packages::mutation::create` |
| R3 | `…/persistence/mutation::create/inserts:work_packages` | RustExtraction | same |
| R4 | `…/admission/POST /api/v3/work_packages/enters:mutation::create` | RustExtraction | same, and the API v3 endpoint is not harvested on the Ruby side |
| R5 | `…/admission/user_allowed_to_add/requires:add_work_packages` | DataWitness | Evidenced by a drift check only (EVIDENCED, not RESOLVED) |

## 7. Tooling and dependencies

| # | Gap | Status |
|---|-----|--------|
| D1 | `lib-ruby-parser` comes from crates.io although `AdaWorldAPI/lib-ruby-parser` exists | The fork's repository lacks the generated parser sources the crates.io package ships, so it does not build from git. Using it needs the sources generated (`development` features + bison) and committed to a fork branch; awaiting the owner's go-ahead |
| D2 | The copied crates (`rails-polyfill`, `rails-compiler`, `quack-shadow`, `rails-f000`, `polyfill-feed`) are not rustfmt-clean | They keep zammad-rs's formatting and are left out of CI's fmt step |
