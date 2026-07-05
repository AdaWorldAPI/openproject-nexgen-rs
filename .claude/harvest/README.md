# `.claude/harvest/` — the ORM→AR back-projection resolver config

> Stood up per §5 step 2 of
> `.claude/handovers/2026-07-05-ogar-v3-consumer-migration-plan.md`
> (the additive, zero-risk first step). This directory is **data**, not
> code: it is the ONE training wheel op-nexgen owns after the OGAR V3
> transpiler correction. Everything else in the codegen stack retires into
> the upstream pipeline (`ruff` + `OGAR` + lance V3).

## 1. What this directory is

op-nexgen's narrowed role is **thin consumer of the OGAR V3 transpiler +
exactly one training wheel** (migration plan §3). The training wheel is the
**ORM→AR back-projection**: mapping **ORM-shaped source** (a Rails migration
DSL / physical DB schema) *before* it is fully AR/Rails/Ruby, so we can
**back-project the DB schema into guessed ActiveRecord behaviour** and recover
the residual the AR extraction cannot see on its own.

ruff is already smart for AR/Rails/Ruby (the class-body / method / validation
strata). The **only** gap is the *column* stratum: the physical schema in
`db/migrate/tables/*.rb`, and the AR associations/validations implied by it.
The WorkPackage oracle diff (migration plan §4; RESIDUAL-THREE-BUCKETS.md §4c)
measured that **~90 % of a hand-written Rust model struct derives from the
column stratum alone** (name + type + nullability), with the remaining ~10 %
coming from validation triples the expander already ships — i.e. the column
stratum + validation triples ≈ 100 % of the model shape. This config is the
data that turns those column facts into guessed AR declarations, closing the
90 → 100 gap the oracle diff left open.

The crude v1 of exactly this lived, buried, in the vendored ruff patch
**D-AR-3.5** (`vendor/AdaWorldAPI-ruff/crates/ruff_ruby_spo/src/schema.rs`).
This directory promotes it from *code buried in a vendor mirror* to
**resolver config (data)** that op-nexgen owns explicitly.

- Input: `orm-ar-backprojection.toml` — one `[[rule]]` per ORM/DB shape →
  guessed AR declaration.

## 2. The data-not-code doctrine

**Config is data.** The back-projection rules are declarative pattern →
declaration mappings, stored as TOML. op-nexgen does **not** detect, address,
or transpile — that intelligence lives in ruff (`soc` detect / `mint` address
/ `propose`). This config is the one place where op-nexgen supplies a small,
enumerable table of ORM-shape heuristics.

**Where the data is insufficient, make *ruff* smarter — spec it, don't fake
it here** (migration plan §6 guardrail). Two categories of rule live at two
different homes:

- **Pure pattern → declaration** (a column type maps to a field; a `null:
  false` maps to a presence guess; an `<x>_id` name maps to a `belongs_to`
  guess) = **data** → lives here as resolver config.
- **Rules that need the *actual* AR code** to be correct (don't guess
  `belongs_to` if the association is already declared; is a DB constraint
  truly a validation or just a storage invariant?) = **ruff gets smarter**.
  Those are specced to a ruff session, never faked in op-nexgen. A rule here
  whose `notes` says "needs ruff to lift X first" is a marker that the
  correct fix is upstream, not a richer heuristic in this file.

## 3. The oracle-validation discipline — measure, don't claim

**Every guess is validated against the AR oracle** (the 90/10 oracle-diff
discipline; migration plan §4, §4c). No coverage ships that was not measured.
The oracle is the three-way diff: DB schema (ground truth) × hand-written Rust
× measured extraction; each guessed declaration must reproduce the AR the
oracle witnesses, or it is corrected/retired.

**Nothing in `orm-ar-backprojection.toml` is oracle-validated yet.** Every
rule carries `validation = "unmeasured"`. That is deliberate and honest: this
step (§5 step 2) stands up the config *additively*, ahead of the AR-oracle
pass. When the oracle pass runs, each rule's `validation` moves from
`"unmeasured"` to a measured verdict (e.g. `"confirmed"` / `"corrected"` /
`"retired"`), and no rule is treated as shipped coverage until it has one.

## 4. Schema of `orm-ar-backprojection.toml`

A `[meta]` table (schema version + doctrine one-liners + the `kind` /
`validation` legends), then an array of `[[rule]]` tables. Fields per rule:

| field | meaning |
|---|---|
| `id` | stable machine handle (snake_case, unique, append-only) |
| `input_name` | human name of the ORM/DB shape the rule keys on |
| `input_pattern` | structural description of the input (which stratum, what shape) |
| `input_regex` | a regex matching the input line or harvested field name, where one applies (`""` when the shape is purely structural) |
| `guessed_output` | the AR declaration this rule proposes |
| `kind` | `"direct"` \| `"guess"` \| `"weak"` (see legend below) |
| `validation` | oracle-validation state; **all `"unmeasured"`** at stand-up |
| `source` | provenance: migration-plan §4 row and/or `schema.rs` line range |
| `notes` | caveats, "make ruff smarter" markers, discrepancy pointers |

`kind` legend:

- **`direct`** — the ORM shape *determines* the output; no inference. (A typed
  column is a field; a `t.references` DSL call is a declared association.)
- **`guess`** — a name/shape heuristic infers an AR declaration that is
  *usually* right but is not stated by the source (a bare `<x>_id` column
  probably means `belongs_to :x`). Must clear the oracle before it is trusted.
- **`weak`** — a low-confidence structural hint (`lft`/`rgt` ⇒ nested-set;
  `parent_id` ⇒ tree). Never shipped without an oracle confirmation and,
  usually, corroborating AR code from ruff.

## 5. Provenance of `op-codegen-residual` (a DIFFERENT residual — later step)

Migration plan §5 step 6 retires `op-codegen-residual` and says "its data
moves to `.claude/harvest`". **That is a later step, and its data is a
different concern from this file** — it is NOT folded in here, for two
reasons:

1. **Different axis.** `op-codegen-residual`'s `RESIDUAL_MANIFEST`
   (`crates/op-codegen-residual/src/lib.rs`; doctrine in
   `.claude/knowledge/RESIDUAL-THREE-BUCKETS.md`) is the **output-side**
   three-buckets doctrine: which *emitted* fields the extractor cannot
   determine (`TYPE option<any>`) and which bucket (B1 fuzzy / B2 landing-zone
   / B3 manual) each lands in. This file is the **input-side** back-projection:
   ORM/DB shape → guessed AR declaration. Its rows (`model, field, bucket,
   zone, mint`) are not ORM→AR back-projection rules and do not belong in
   `orm-ar-backprojection.toml`.
2. **Additive-only mandate.** This step touches nothing outside
   `.claude/harvest/`. `op-codegen-residual` is left exactly as it is; its
   crate and tests are untouched.

So the residual manifest is row-shaped data, but it lives — for now — in the
crate and the knowledge doc named above. Its migration into `.claude/harvest/`
(as its own file, under its own concern) is a later step of the plan, not part
of standing up the ORM→AR back-projection config.

## 6. §4-table vs `schema.rs` discrepancies (recorded for the oracle pass)

Cross-checking the migration-plan §4 table against what
`vendor/AdaWorldAPI-ruff/crates/ruff_ruby_spo/src/schema.rs` (D-AR-3.5)
actually implements surfaced these gaps. Every one is reflected in the TOML
(extra rules and/or `notes`):

1. **`schema.rs` implements DIRECT association extraction the §4 table omits.**
   `t.references :x` / `t.belongs_to :x` (and `polymorphic: true`) are
   *declared* associations in the migration DSL (`schema.rs:215-223`), a
   higher-confidence `direct` signal than the §4 name-pattern `guess`
   (`<x>_id` ⇒ `belongs_to`). Both are kept: the direct DSL rule fires when the
   migration uses `t.references`; the name-pattern guess is the fallback for a
   bare `<x>_id` column with no covering DSL call.
2. **`schema.rs` implements structural defaults §4 omits** — the implicit
   primary key (`create_table` without `id: false` ⇒ `id` bigint NOT NULL,
   `schema.rs:184-187`) and the `t.timestamps` pair
   (`created_at`/`updated_at`, `schema.rs:207-211`). Added as `direct` rules.
3. **`schema.rs` deliberately SKIPS index/constraint lines.**
   `t.index` / `t.foreign_key` / `t.check_constraint` /
   `t.exclusion_constraint` are treated as "constraint/index facts, not
   columns" (`schema.rs:204-205`, docstring lines 32-34). So the §4
   `add_index unique ⇒ uniqueness` guess has **no backing harvest yet** — the
   unique-index facts are not lifted. That rule's `notes` marks it as a "make
   ruff smarter" item (ruff must lift index facts / replay incrementals first).
4. **`schema.rs` is baseline-only** (`columns_from = "baseline-only"`;
   incremental `add_column`/`rename_column`/`add_index` after the squash are
   not replayed, `schema.rs:27-29`). The unique-index guess and any
   post-squash column additions depend on ruff replaying incremental
   migrations — an upstream item, noted on the affected rules.
5. **§4 collapses `lft/rgt | parent_id` into one row**; the TOML splits it into
   two distinct `weak` rules (`nested_set` from `lft`+`rgt`; `adjacency_tree`
   from a self-referential `parent_id`) because they are separate shapes with
   separate AR back-projections. `schema.rs` harvests neither specially — they
   arrive as plain integer columns.
6. **`schema.rs` harvests columns/tables but does not emit the AR behaviour.**
   `t.references` yields the `_id`/`_type` *columns* (not the `belongs_to`
   *declaration*); join tables land in `unmatched_tables`
   (`schema.rs:31,127-129`) but no HABTM is emitted; an `<assoc>_count` column
   is just an integer field. The association / HABTM / counter_cache
   back-projections are exactly what this config adds on top of the harvest.
