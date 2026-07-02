# Wishlist → sibling sessions (from op-nexgen, 2026-07-02, post-#630)

> Forwardable as-is. Every item is self-contained (paths, numbers, gate
> criteria). Ordered by leverage within each section. Evidence lives in
> this repo: `.claude/knowledge/RESIDUAL-THREE-BUCKETS.md`,
> `TWO-SHAPES-COMPILED-NOT-PARSED.md`, handover
> `2026-07-02-ruff-upstream-extraction-contract.md`.

## To the ruff session

- **R1 — Merge D-AR-3.5 (column stratum).** Patch ready:
  `vendor/AdaWorldAPI-ruff/D-AR-3.5-column-stratum.diff` in
  openproject-nexgen-rs (642 lines, applies to pristine main; unit tests +
  corpus gate included — measured 99 baseline tables seen / 65 matched /
  34 unmatched named / 0 skipped; WorkPackage pins 27 typed columns).
  Downstream already consumes it: nexgen's typed DDL is at **89.5% typed
  fields** on the real corpus with this patch vendor-applied.
- **R2 — Curated-list fixes:** `"Priority"` → `"IssuePriority"`
  (`ruff_openproject/src/lib.rs:67`; exact-match filter silently drops it)
  and remove/rethink `"Activity"` (no AR class exists — probe evidence in
  nexgen `RESIDUAL-THREE-BUCKETS.md` §4b).
- **R3 — Incremental-migration replay.** Baseline-only misses post-squash
  columns (`sequence_number`, `identifier`, `project_phase` rename).
  Parse `add_column`/`rename_column`/`remove_column` in `db/migrate/*.rb`
  + `modules/*/db/migrate`. Retires the `columns-from: baseline-only`
  ledger marker.
- **R4 — Conservation ledger generalization.** `SchemaReport` is the seed;
  extend to parse/filter stages: files skipped (with reasons), dropped
  models NAMED. Contract in nexgen handover §3.
- **R5 — Provenance stamping:** `ruff@<sha> curated-list@<hash>` header in
  every emitted artifact (drift between artifact and producer was PROVEN
  this week — the Priority discrepancy).
- **R6 — F17 body triage is unblocked** (`writes_field`/`calls` are live
  on main): recover `(verb, criteria)` per hook body, order-signature
  gate. This is the only path to the last ~10%.
- **R7 — Reconcile `ruff_openproject` (branch-side) with main:** its
  locked-shape test fails against main's ruby_spo even unpatched (the
  fixture `raises` body-fact is no longer emitted) — verified twice.
- **R8 (small) — `column_default` predicate** next to `column_not_null`:
  the migration DSL carries defaults; `DEFINE FIELD … DEFAULT` is the
  natural consumer. We deliberately skipped it in D-AR-3.5.

## To the lance-graph session

- **L1 — Merge or bless `RouteBucketTyped` (C6).** Still absent upstream
  after #626–#630 (verified per merge). nexgen's `op-codegen-bucket`
  depends on it; we re-apply `codegen_spine.diff` on every sync. Either
  merge the diff (in nexgen: `vendor/AdaWorldAPI-lance-graph/codegen_spine.diff`)
  or provide the sanctioned alternative and we'll migrate.
- **L2 — `emission_scan`: a `classid_scan` sibling for typed-DDL
  adoption.** Zero-dep counting module in the contract:
  `TypedForm { Typed, AnyTyped, RecordLink, Stub }` + fold to counts — so
  every consumer measures schema-coverage identically (nexgen currently
  greps its own DDL for the 89.5% figure). Same design language as #630.
- **L3 — Arrow/Lance columnar triple interchange.** Triples are five
  parallel columns (`s p o f c`); ndjson is the last text format in the
  pipeline's middle. Emitting record batches makes extraction output load
  into the store with no parse and no reassemble — "compiled, not
  parsed" applied to the interchange, and the storage layer already
  speaks the format.
- **L4 — A materialization slot for DAG-backed columns.** Rails
  `derived_*` columns are materializations of compute-DAG nodes
  (`emitted_by`/`depends_on` are already extracted facts). A contract
  flag ("this field is a cache of DAG node X") lets consumers stop
  transcoding computation as data — part of the residual dissolves
  instead of needing coverage.

## To the OGAR session

- **O1 — Accept the Rails front-end for `ogar-from-schema`.** Your own
  crate header defines the structural arm + the schema↔source drift
  detector; the migration-DSL scanner (nexgen `ruff_ruby_spo/src/schema.rs`
  via the D-AR-3.5 diff) is that front-end for Rails, tested on the
  OpenProject corpus. We'll send it shaped to your `Class` lift on request.
- **O2 — `ogar-adapter-surrealql`: direct AST handoff.** The crate already
  builds `surrealdb_ast` values; make `surrealdb-core` consumption the
  primary path and `emit_surrealql_ddl -> String` external-only. Final
  step of compiled-not-parsed for the DDL edge.
- **O3 — `compile_graph_ruby` in `ogar-from-ruff/src/mint.rs`** (~15 LOC,
  mirrors `compile_graph_python`, calls the existing `lift_model_graph`).
  Test expectations in the FLIPPED order: `openproject:WorkPackage` →
  `0x0102_0001`, `redmine:Issue` → `0x0102_0007`.
- **O4 — OGIT zone keys:** mint `Session` / `DocLink` / `Locale` /
  `ScheduledReminder` / `ExternalRef` per the in-repo `JournalEntry.ttl`
  precedent (stock schema shape + 3-hop doctrine); key `GroupMembership`
  on stock `ogit.Auth:isMemberOf` (confirmed); locate/verify the
  `ogit:Subscription` and `ogit:Timeseries` definition files (attested as
  edge targets, definitions unlocated by our inventory).

## Cross-cutting (any session)

- **X1 — Adopt a `COORDINATION.md` board** per repo (the PR-table format
  the operator already uses in chat) so merge events stop needing a
  human relay; nexgen ships `.claude/tools/vendor-sync.sh` as the
  consuming end of that signal.
- **X2 — Probe-preamble convention:** subagent fetch/diff prompts carry
  environment facts (what's scoped, expected 403s, authenticity checks)
  — prevents a verified-real repo from being misread as fabricated (it
  happened) while keeping the flag-concerning-content behavior.
