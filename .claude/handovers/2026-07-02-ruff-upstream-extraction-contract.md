# Handoff → ruff session: extraction-layer fixes + two new contracts

> ## STATUS UPDATE (2026-07-02, later the same day — verified against
> ## upstream `main` HEAD via raw-fetch reassembly)
>
> | Item | Status |
> |---|---|
> | §1 walk widening | **ALREADY ON MAIN** — `collect_model_roots` walks `modules/*/app/models` + `engines/*/app/models` (`parse.rs:76`), tested. The engine-walk branch adds nothing over main here. |
> | §1 curated-list fixes (Priority/Activity) | Still open — `ruff_openproject` lives branch-side (404 on main); fixes remain yours. |
> | §2 provenance stamping | Still open. |
> | §3 conservation ledger | **SEEDED** — the D-AR-3.5 patch introduces `SchemaReport` (tables seen/matched/unmatched **named**, files skipped named, `columns_from` marker). Full per-stage ledger still open. |
> | §4 determinism | **ALREADY ON MAIN** — files sorted, `expand()` sorted+deduped by `(s,p,o)`, BTree throughout, doc'd as a contract. |
> | §5 confidence-channel bucketing | Still open (forward design). |
> | §6 column stratum | **IMPLEMENTED — patch ready.** `vendor/AdaWorldAPI-ruff/D-AR-3.5-column-stratum.diff` (642 lines, 5 files) applies to pristine `main`. Details below. |
> | (new) F17 write/call capture | **ALREADY ON MAIN** — `writes_field` / `calls` predicates exist; the §6-of-coverage-kit prerequisite is met. |
>
> ### What the D-AR-3.5 patch contains
>
> - `ruff_spo_triplet`: `Field.not_null: Option<bool>` (serde-stable), new
>   closed-vocab predicate `column_not_null` (Authoritative; count-lock test
>   62→63), emission next to `field_type`.
> - `ruff_ruby_spo`: new `schema.rs` — line-scanner for the
>   `db/migrate/tables/*.rb` baseline DSL (17 typed forms + `t.column` +
>   `references`/`belongs_to` incl. the polymorphic `_id`/`_type` pair +
>   `timestamps` + implicit `id` PK w/ `id: false` opt-out), Rails
>   inflection, merge into the extracted graph via
>   `extract_app_with_schema(root, ns) -> (ModelGraph, SchemaReport)`.
> - Fills the `extract_fields` D-AR-3.5 stub's intent from the migration
>   DSL instead of the planned `db/schema.rb` (OpenProject ships none).
>
> **Measured on the real corpus** (`OPENPROJECT_PATH=/home/user/openproject`):
> 99 baseline tables seen, 65 matched to extracted models, 34 unmatched
> (join tables, named in the report), 0 files skipped; WorkPackage lands
> exactly its 27 baseline columns typed+nullability'd (incl.
> `done_ratio` nullable — the oracle-diff unset≠0% bug, now visible in
> data). All 41 ruby_spo + 66 triplet tests green, clippy clean.
>
> ### One drift you should know about (verified, not caused by the patch)
>
> Vendored branch-era `ruff_openproject`'s locked-shape test
> (`extract_triples_produces_locked_shape`) fails against **pristine**
> main's ruby_spo — the fixture's `raises` body-fact is no longer
> emitted. Main-vs-branch reconciliation is yours; evidence in
> `vendor/AdaWorldAPI-ruff/Cargo.toml` header.

> From the op-nexgen session, 2026-07-02, under operator full authorization.
> Companion canon: `.claude/knowledge/RESIDUAL-THREE-BUCKETS.md` (three-buckets
> doctrine, measured manifest, §4b probe) and
> `.claude/knowledge/RAILS-COVERAGE-KIT.md`. Everything here is
> **upstream-owned** (`AdaWorldAPI/ruff`); op-nexgen deliberately did NOT
> patch its vendor mirror (`vendor/AdaWorldAPI-ruff`) to avoid silent drift
> from your source of truth. File:line refs below are into that mirror's
> snapshot — verify against your branch head before applying.
>
> Ordering is by leverage: items 1–2 are cheap bugfixes; items 3–4 are new
> *contracts* that change what every downstream consumer can assume; item 5
> is forward-looking design.

## 1. Bugfixes (probe-verified, file:line)

Two of the 18 `CORE_V3_RESOURCES` produce no `Model` today; a third only
matches by accident of pipeline vintage. Full diagnosis with evidence:
RESIDUAL-THREE-BUCKETS.md §4b.

- **Walk gap (hides ~half the domain):** `ruff_ruby_spo::parse::parse_models`
  walks only `<root>/app/models` (`parse.rs:64-68`). OpenProject keeps ~half
  its models in engine dirs — `TimeEntry` lives at
  `modules/costs/app/models/time_entry.rb:31` and is invisible. Fix: also
  walk `modules/*/app/models/**/*.rb` (and `engines/*/app/models` for other
  Rails hosts) — the coverage kit's `extract_app_with` already names this
  surface; make the core walk honor it or make `extract_app_with` the
  documented default for full-app extraction.
- **Curated-list mismatches** (`ruff_openproject/src/lib.rs:56-75`):
  - `"Priority"` → **`"IssuePriority"`** (real class:
    `app/models/issue_priority.rb:31`, STI under `Enumeration`).
    `filter_to_core` exact-matches and silently drops it (`lib.rs:97-101`).
  - `"Activity"` → **no AR class exists at all** (only
    `module Projects::Activity`, an `Activities::Event` Struct, and
    `*ActivityProvider` classes). The entry names an API-v3 aggregate, not a
    model. Drop it or replace with the provider classes — decision is yours;
    op-nexgen only needs the list to stop naming phantoms.

## 2. Provenance stamping (drift is now *proven*, not hypothetical)

The 2026-07-01 measured run emitted `DEFINE TABLE Priority`; the code
snapshot vendored in op-nexgen *cannot* (exact-match filter above). Artifact
and alleged producer already disagree, and it was discovered by accident.

**Contract:** every generated artifact carries a provenance header —

```
-- generated-by: ruff@<git-sha> curated-list@<hash-of-CORE_V3_RESOURCES> <UTC timestamp>
```

Cheap (the emitter knows its own build info), and it converts "which
pipeline produced this file?" from archaeology into a grep. Applies to the
SurrealQL emitter and any future Rust/sqlx emitter equally.

## 3. Conservation-of-mass ledger (the systemic fix)

All three §1 failures had different causes but one shared property:
**silent loss**. `filter_to_core` retains without logging; the walk skips
without logging; the phantom entry matches nothing without warning. For a
pipeline whose thesis is "determine statically," unaccounted mass is the
worst failure mode — every future miss costs an agent-investigation instead
of a grep.

**Contract:** each stage reports `N_in = N_out + Σ N_dropped(reason)`:

- extraction: files seen / parsed / skipped(reason: no-class, parse-error);
- filtering: models in / retained / dropped(no-curated-match: **name them**);
- projection: triples in / projected / unrecognized-predicate(count by kind).

Emit the ledger to stderr AND as a trailer comment block in the artifact:

```
-- dropped: Activity (curated entry matched no model)
-- dropped: 312 models (not in curated list)  [full list: stderr]
```

With this, the coverage number becomes *self-reporting* on every run, and
the curated-list mismatch class of bug (§1) can never silently recur.

## 4. Determinism contract (dissolves most of bucket B1 at the source)

The three-buckets doctrine's B1 ("emits X but the arrangement drifts
run-to-run") conflates two populations:

- **pipeline nondeterminism** — filesystem walk order, hash-map iteration
  order leaking into emission. Fixable once, at the source, for every
  consumer forever.
- **domain order-sensitivity** — order that carries meaning. The real
  signal; must escalate to B3 (PRESERVE + RFC), never be "fixed".

**Contract:** extraction output is a pure function of source *content* —
byte-identical across runs regardless of walk order. Concretely: sort file
lists after collection; use BTreeMap/sorted emission wherever iteration
order reaches output; then pin it with a property test — extract the same
tree twice with shuffled file enumeration; assert byte-identical triples.

Once this holds, any *remaining* arrangement instability is real domain
signal, and the B1 gate (`order_free_eq` in op-nexgen's
`op-codegen-residual`) becomes a sharp instrument instead of a noise filter.
Downstream, ~9 of the 21 measured residual rows are expected to move
B1 → determined without any consumer-side work.

## 5. Forward design: bucket via the confidence channel (don't build a side-car)

The triples already carry NARS truth `f`/`c` end-to-end and the projection
ignores it (strict-roundtrip trick aside). The three buckets are a
confidence gradient in disguise: determined (declared column, c≈1) /
fuzzy-arrangement / anticipated-shape / bespoke (c≈0). If extraction stamps
**how** each field was derived (declared column vs. method-inferred vs.
metaprogrammed) into `f`/`c` (or a derivation predicate), the residual
manifest becomes *computed per run* instead of hand-transcribed —
op-nexgen's `RESIDUAL_MANIFEST` is a snapshot that will drift; the
c-channel version cannot. No schema change needed — the channel is already
plumbed; it just needs semantics assigned at emit time.

## 6. The missing stratum: migration-DSL column extraction (measured 90% yield)

Added after the WorkPackage oracle diff (RESIDUAL-THREE-BUCKETS.md §4c) —
this outranks everything above in yield.

The extractor today reads the **instance-method / AR-DSL stratum** of
`app/models`. The oracle diff measured what a Rust model struct actually
needs: **90% of it is the column stratum** (name + SQL type + nullability),
which lives in the **migration DSL** — OpenProject ships no
`schema.rb`/`structure.sql`; the squashed baseline is
`db/migrate/tables/*.rb` (plain Rails `create_table`/`t.column`/`t.index`
calls — a fixed, enumerable, statically-parsable vocabulary, exactly like
the AR recipe DSL you already parse), plus incremental migrations layered
after (`db/migrate/*.rb`, `modules/*/db/migrate/*.rb`).

And the last 10% is **already in your triples**: the two non-schema-derivable
typings found (`description`'s Formattable convention aside) come from
`validates` calls — `done_ratio`'s `0..100` clamp is a
`validates_constraint`/`validation_param` fact you already emit (a
`GUARD_RANGE` recipe concept). So:

> **column stratum (new) + validation triples (shipped) ≈ 100% of model
> struct shape.**

Suggested contract:

- Parse `db/migrate/tables/*.rb` baseline first (cheap, static; ignore
  incremental replay initially — note it in the conservation ledger as
  `columns-from: baseline-only`).
- Emit `has_column` triples: `(ns:Model, has_column, "name")` +
  `(ns:Model.name, has_sql_type, "bigint")` + nullability/default facts —
  whatever predicate shape fits your existing vocabulary; the projection
  then emits typed `DEFINE FIELD` instead of `option<any>`.
- Table→model naming: `work_packages` → `WorkPackage` (Rails inflection);
  emit unmatched tables in the ledger rather than dropping them.
- Module migrations (`modules/*/db/migrate`) ride the same §1 walk widening.

Yield claim to verify on your side: for WorkPackage this turns a 2-field
`option<any>` emission into ~35 typed fields, and the same should hold
across the curated set (the columns are the *easy* 90% — it's the stratum
hand-written Rust actually consumed).

## What op-nexgen holds on its side (no action needed from you)

- `op-codegen-residual` (standalone crate): typed manifest, B1 blade,
  `order_free_eq` gate, 7-zone `LandingZone::REGISTRY` — consumes your fixes
  as manifest-row deletions (rows only ever leave).
- The doctrine doc tracks which residual rows are expected to become
  determined after your §4 (determinism) and C12-era type inference — we
  re-measure and prune when the OGAR crates are vendored and the pipeline
  builds here again (that vendoring is on us, not you).
