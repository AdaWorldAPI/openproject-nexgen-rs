# Extraction-Gap Proposals — Sprint C0 (C0-09)

The four hard gaps that block running the real `AdaWorldAPI/ruff` codegen pipeline
end-to-end against OpenProject. Each is substantiated with `file:line` evidence
(research detail: `calibration/notes/gap-research.md`). Proposals, not implementations.

Ranked cheapest-to-close × highest coverage payoff.

---

## Gap 1 — SeaORM-vs-sqlx target mismatch  *(close first)*
**Evidence:** the emitter hardcodes SeaORM's ActiveModel/Set/Entity API —
`ruff/crates/ruff_python_dto_check/src/codegen/mod.rs:479` `active.aktiv = sea_orm::Set(false);`
and `codegen/target.rs:33/37/41` append `::Model`/`::Entity`/`::Column`. The
OpenProject seed is raw sqlx — `op-db/src/work_packages.rs:8` `use sqlx::{FromRow, PgPool, Row};`.
So no generated handler/repo compiles against the seed.
**Impact:** gates all 13 `emit_kinds` (`target.rs:181-193`).
**Proposal:** add an `orm = "sqlx" | "seaorm"` switch on `TargetSpec`; provide an
additive sqlx recipe set (`query_as::<_, T>` + `FromRow`, `PgPool` repo shape) for
the DB-touching arms (`emit_list_for_tenant`, `emit_soft_delete`, `emit_detail_for_tenant`,
`emit_toggle_bool_field`). The `emit()` dispatch and contract stay untouched.
Best effort:coverage ratio — this single switch unblocks the whole emitter for OpenProject.

## Gap 2 — HAL/JSON output vs jinja/askama view emitters  *(close second)*
**Evidence:** view emitters are jinja→askama only —
`codegen/jinja.rs:1` `//! jinja → askama translation`, `codegen/columns.rs:1`. The
target has no template engine — it's an Angular SPA over HAL+JSON
(`op-api/src/representers/hal.rs:151` `pub struct HalResource<T>`); `emit_ajax_json`
(`mod.rs:1018`) emits flat `serde_json::Value` with no HAL `_type`/`_links`/`_embedded`.
**Impact:** the dominant OpenProject output kind (`ajax_json`) emits non-HAL JSON;
template arms are dead weight.
**Proposal:** a `views = "none"` spec flag that skips `columns.rs`/`jinja.rs`
entirely, plus a `hal` envelope option on `emit_ajax_json` (wrap fields in
`_type` + `_links{self,...}` + `_embedded`). Cheapest absolute change.

## Gap 3 — single `models_root` vs multi-crate layout  *(close last)*
**Evidence:** `codegen/target.rs:51` `pub models_root: String` — one root, no
`handlers_root`/per-crate map (whole struct `target.rs:47-71`). OpenProject splits
one entity across `op-models` / `op-db` / `op-api` / `op-contracts`.
**Impact:** the emitter can target one crate's module tree, not the seed's 4-layer split.
**Proposal:** generalize `TargetSpec` to a `{ models_root, repo_root, handlers_root,
contracts_root }` map (or a per-layer table). Only matters AFTER Gap 1 makes code compile.

## Gap 4 — Ruby/Rails frontend is an empty scaffold  *(independent track — C0-01)*
**Evidence:** `ruff/crates/ruff_ruby_spo/src/lib.rs:83` `todo!("wire a Ruby parser
(lib-ruby-parser): …")` (also `:102`, `:125`); the IR + a locked target-shape test
(`lib.rs:160`) exist — only the 3 extraction fns are missing. Note the codegen
pipeline is otherwise Python-source-driven (`pipeline.rs:39` `parse_module`), i.e.
expects Flask/Python, orthogonal to OpenProject's Ruby.
**Impact:** there is no automated path from OpenProject Rails → ModelGraph yet; this
is why Wave 1 was agent-emitted rather than pipeline-emitted.
**Proposal:** implement the 3 `ruff_ruby_spo` extraction fns against `lib-ruby-parser`
to emit a ModelGraph for 3 models (the C0-01 deliverable). Well-bounded (parser dep +
3 fn bodies, test-locked) but serves the SPO-triplet pipeline, not the axum emitter —
so it is **independent** of Gaps 1–3 and can proceed in parallel.

---

### Cross-cutting note
The workspace root declares `sea-orm` (`Cargo.toml:43`) but **no `op-*` crate depends
on it** (all use sqlx) — that declaration is dead and should not be mistaken for SeaORM
adoption. The seed is unambiguously axum+sqlx; the target spec is named accordingly.
