# Sprint C4 Report — Gap 4: Ruby/Rails frontend for `ruff_ruby_spo`

**Date:** 2026-06-01 · **Branch:** `claude/beautiful-gates-dJo0u` (nexgen)
· **Workbench:** `claude/openproject-sqlx-emitter` on the ruff-clone.

---

## Headline

> **Gap 4 closed.** `ruff_ruby_spo` went from a 215-line scaffold with 3
> `todo!()` extractors to a working line-based Ruby/Rails extractor (1,035 LoC
> across 5 modules, **zero external parser deps**). `extract()` over a fixture
> Rails app/models tree + db/schema.rb now produces the locked SPO triple shape
> end-to-end. **21/21 tests green** (18 unit + 3 end-to-end integration).
>
> This is the **independent track** the C3 report flagged as the next real
> unlock: the axum codegen (C1–C3) was the *emit* side; this is the *extract*
> side. The downstream consumers (`lance_graph` SPO loader, `action_emitter`,
> `link_chain`) need ZERO changes — they already consume this triple shape.

## What landed

| File | Status | Purpose |
|---|---|---|
| `src/lib.rs` | modified | `extract()` flow + `RubyClass` (new `columns` field) + module wiring |
| `src/scan.rs` | new (251 LoC) | Shared scanning primitives: `strip_comment`, `macro_symbols`, `def_blocks` (Ruby keyword-depth block matcher), `ivar_assignments`. Keystone written by orchestrator. 5 unit tests. |
| `src/parse.rs` | new (267 LoC) | `parse_models`: walks app/models/*.rb + db/schema.rb → `RubyClass` records |
| `src/fields.rs` | new (213 LoC) | `extract_fields`: schema columns + memoized `@ivar` derived attrs with `emitted_by`/`depends_on` |
| `src/functions.rs` | new (304 LoC) | `extract_functions`: def bodies (`reads`/`raises`/`traverses`) + synthetic `_validate` guard for declarative validations |
| `tests/ruby_extract_test.rs` | new (3 tests) | End-to-end spec — drives `extract()` over the fixture, asserts the locked SPO triples |
| `tests/fixtures/openproject/` | new | Tiny Rails fixture (work_package.rb + time_entry.rb + db/schema.rb) |
| `RUBY-FRONTEND.md` | new (161 LoC) | Public-facing docs with the honest scope/limits gap list |

## Architectural decision: line scanner, not `lib-ruby-parser`

The scaffold's Cargo.toml explicitly states "zero external parser deps so the
target shape can be locked first." I honored that:

- A focused line scanner for the regular subset of ActiveRecord models is
  enough to lift the IR (class defs, association macros, `def…end` blocks,
  ivar assignments, `validates`).
- Avoids registry/offline risk for an external parser dep.
- The honest gap list (heredocs, `define_method`, multi-line chains across
  lines, concerns/mixins, `scope` lambdas) is documented in
  `RUBY-FRONTEND.md` — not silently misextracted.

## Fanout discipline

**Phase 0** (orchestrator, heavy): wrote `scan.rs` (251 LoC + 5 unit tests),
fixtures, the end-to-end integration test (the spec), and the `lib.rs`
restructure (split single-file into modules, added `columns` to `RubyClass`).
Verified Phase 0 compiled + scan tests passed + integration test compiled
(panicking via `todo!()` until filled — the right failure mode).

**Phase 1** (4 parallel agents, file-disjoint):
- **D-A**: `parse.rs` (267 LoC, walks app/models + schema.rb)
- **D-B**: `fields.rs` (213 LoC, baseline columns + memoized ivars + assoc-chain finder with identifier-boundary correctness)
- **D-C**: `functions.rs` (304 LoC, def-block scan + synthetic `_validate`)
- **D-D**: `RUBY-FRONTEND.md` (161 LoC docs)

**Phase 2** (verify): `cargo test -p ruff_ruby_spo` → **21 passed, 0 failed**
(18 lib unit + 3 end-to-end). **Zero fix-loop iterations** — the integration
test passed first try once the 3 extractors were filled. scan.rs being
heavily front-loaded as the shared contract paid off.

## The locked Rails → IR mapping (as implemented)

| IR target | Rails source | Tier |
|---|---|---|
| `Model::name` | `class WorkPackage < ApplicationRecord` (or `::Base` / STI parent) | Authoritative |
| `Field::name` (baseline) | `db/schema.rb` `t.<type> "<col>"` lines | Authoritative |
| `Field::name` (derived) | memoized `@x ||=` / `@x =` in a `def` body | Authoritative |
| `Field::emitted_by` | the `def` block that memoizes the ivar | Authoritative |
| `Field::depends_on` | `<assoc>.<member>` chains in that def body (`<assoc>` ∈ declared associations) | Inferred |
| `Function::name` | top-level `def NAME` in the class body | Authoritative |
| `Function::reads` | identifiers in the body that match a known column | Inferred |
| `Function::raises` | `raise <Exc>` token verbatim (incl. `::`) + `errors.add(…)` → `ActiveRecord::RecordInvalid` | Authoritative |
| `Function::traverses` | identifiers in the body that match a declared association | Inferred |
| synthetic `_validate` function | any `validates :a, :b, …` or `validate :method` line | Authoritative |

## Sprint metrics — C0 → C4

| | C0 | C1 | C2 | C3 | C4 |
|---|---|---|---|---|---|
| Agent-runs | 6 | 8 | 3 | 3 | 4 |
| Fix-loop iterations | 1 | 1 | 0 | 0 | 0 |
| Tests added | 10 | 8 | 2 | 2 | **21** (lib+integration) |
| LoC delta (workbench) | ~46K seed mirror | ~2200 | ~600 | ~600 | ~1035 |
| Git commits to nexgen | many (learned) | 4 (learned) | 1 | 1 | 1 |
| Track | codegen | codegen | codegen | codegen | **frontend** |

Phase 0 was heavier than usual (orchestrator wrote scan.rs + fixtures + test
+ lib.rs restructure) because the 3 extractors needed to share a tested
scanning vocabulary. That investment paid off: 0 fix-loop iterations across 4
parallel agents on a from-scratch crate.

## Recommendation for Sprint C5

1. **C5-pipeline-run**: with both the emitter (C1–C3) and the extractor (C4)
   now functional, the *end-to-end pipeline* against a real OpenProject
   checkout becomes runnable. Wire `extract()` → `expand()` → NDJSON →
   downstream consumers; produce a coverage report against actual OpenProject
   models. This is the real "does the pipeline work at scale" answer.
2. **C5-scanner-hardening**: close the gap list in `RUBY-FRONTEND.md`
   (heredocs, multi-line method chains, `define_method`, concerns). Each gap
   is a discrete agent.
3. **C5-emitter-rest**: the remaining sqlx kinds (`form_get_post`,
   `signed_link_action`, explicit PATCH/DELETE) — ~2-3 agents, finishes the
   codegen surface.

My recommendation: **C5-pipeline-run**. The interesting question now is
empirical: how much of OpenProject actually extracts cleanly? That data
should drive C6+ decisions (scanner-hardening vs more emitters vs other gaps).

## Artifacts

- `docs/SPRINT_C4_REPORT.md` (this file)
- `vendor/AdaWorldAPI-ruff/crates/ruff_ruby_spo/` (full crate mirror)
- `.claude/sprints/c4-ruby-frontend/STATE`

🦋
