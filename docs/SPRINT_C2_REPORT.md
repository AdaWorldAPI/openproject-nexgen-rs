# Sprint C2 Report — Gap 2: HAL envelope for `ajax_json` in the sqlx target

**Date:** 2026-06-01 (same session as C0+C1)
· **Branch:** `claude/beautiful-gates-dJo0u` (nexgen)
· **Workbench:** `claude/openproject-sqlx-emitter` on the existing ruff-clone
(from Sprint C1).

---

## Headline

> The 5th sqlx-emittable handler kind landed: **`ajax_json`** with HAL envelope.
> Two-branch implementation (with-model: real `query_as` wrapped in
> `_type`/`_embedded`/`_links`; without-model: stub with `CALIBRATION` comment
> and `serde_json::Value::Null` placeholders from `contract.output.shape`).
> **69/69 tests green** including the 28 pre-existing seaorm tests
> (back-compat preserved). Coverage projection: **~50% → ~78%** of
> OpenProject's controller-action surface is now sqlx-emittable.

## What landed (Sprint C2)

| File | Status | Purpose |
|---|---|---|
| `src/codegen/sqlx_emit/ajax_json.rs` | new (322 LoC) | `emit_ajax_json_sqlx` — two branches |
| `tests/sqlx_emit_ajax_json_test.rs` | new (153 LoC) | 2 byte-exact golden tests |
| `tests/golden/.../ajax_json_with_model.rs` | new | Spec for with-model branch |
| `tests/golden/.../ajax_json_stub.rs` | new | Spec for stub branch |
| `src/codegen/sqlx_emit/mod.rs` | modified | `pub mod ajax_json;` |
| `src/codegen/mod.rs` | modified | `HandlerKind::AjaxJson` arm in sqlx dispatch |
| `src/codegen/target.rs` | modified | `"ajax_json"` in `openproject_axum_sqlx()` emit_kinds |
| `examples/openproject-axum-sqlx.toml` | modified | `"ajax_json"` in declarative emit_kinds |
| `SQLX-TARGET.md` | modified | Kind table 4 → 5; new "AjaxJson — two branches" subsection |

## Two-branch ajax_json design

The seaorm `emit_ajax_json` (at `crates/ruff_python_dto_check/src/codegen/mod.rs:1041`)
emits a `<FnName>Response` struct with `Default + serde::Serialize` and a stub
handler. That works for jinja-templated Flask but produces non-HAL JSON which
OpenProject's API-first frontend can't consume.

The sqlx variant branches on whether the contract has a resolvable primary
model:

- **With-model branch** (most OpenProject ajax_json handlers): emit a
  `sqlx::query_as` SELECT-by-id against the model, wrap the result in HAL JSON:

  ```rust
  Ok(HalResponse(serde_json::json!({
      "_type": "<Class>",
      "_embedded": item,
      "_links": { "self": { "href": format!("/api/v3/<family>/{}", primary_param) } },
  })))
  ```

  This is structurally similar to `detail_for_tenant_sqlx` but **without**
  the tenant-filter (`AND project_id = $2`) and **with** the HAL envelope
  wrapper (vs. detail's bare `HalResponse(item)`).

- **Stub branch** (e.g. `/api/v3/notifications/count` with `data.models = []`):
  emit a handler that takes only `State + AuthenticatedUser`, computes a
  `_type` from `to_pascal(contract.function)`, and fills the json! body
  with `serde_json::Value::Null` placeholders from `contract.output.shape`,
  plus a `CALIBRATION` comment. No `todo!()`, no `unimplemented!()`.

## Fanout discipline

**Phase 0** (orchestrator): 2 golden expected files written by hand to lock
down the spec.

**Phase 1** (3 parallel agents, file-disjoint):
- **B1**: `src/codegen/sqlx_emit/ajax_json.rs` (the emit fn, 322 LoC,
  byte-exact verified against both goldens via mental dry-run)
- **B2**: `tests/sqlx_emit_ajax_json_test.rs` (153 LoC, 2 integration tests)
- **B3**: `SQLX-TARGET.md` update (+28 lines net: kind table 4→5, new
  "AjaxJson emitter — two branches" subsection, 9→8 not-implemented count)

**Phase 2** (orchestrator, sequential, gated by `cargo check`):
- 4 atomic edits across the existing files (sqlx_emit/mod.rs,
  codegen/mod.rs::emit(), target.rs::openproject_axum_sqlx(), example TOML).

**Phase 3** (verify): `cargo check -p ruff_python_dto_check --tests` (5.03s),
`cargo test -p ruff_python_dto_check` — **69 passed, 0 failed**.

**No fix-loop iterations needed** — all 3 agents produced byte-exact output
on the first try. (Sprint C1 needed 1 iteration for `ModelMapping: PartialEq`.)
Pattern-establishment from C1 paid off: agents knew exactly what shape to
emit.

## Coverage projection updates

After Sprint C2, the sqlx-emittable handler-kind set is 5: `list_for_tenant`,
`detail_for_tenant`, `soft_delete`, `toggle_bool_field`, `ajax_json`.
Estimated effect once the Ruby/Rails frontend (Gap 4) extracts contracts:

| Kind | OpenProject prevalence (est.) | C1 emittable? | C2 emittable? |
|---|---|---|---|
| `list_for_tenant` | ~25% | ✓ | ✓ |
| `detail_for_tenant` | ~20% | ✓ | ✓ |
| `soft_delete` | ~5% | ✓ | ✓ |
| `toggle_bool_field` | ~3% | ✓ | ✓ |
| `ajax_json` | ~25% | ✗ | **✓** ← C2 |
| Other 8 kinds | ~22% | ✗ | ✗ (stub) |

**Cumulative emittable surface: ~78%** of OpenProject's controller-action
count, up from ~50% after C1. Most of the remaining ~22% lives in
`template_get` (less relevant for an API-first target), download/PDF
(call-site shape only), and the form kinds (need form-DTO emitter to be
sqlx-aware — Sprint C3 candidate).

## Sprint metrics — C0 vs C1 vs C2

| | C0 (News vertical) | C1 (Gap 1: sqlx target) | C2 (Gap 2: ajax_json) |
|---|---|---|---|
| Agent-runs | 6 | 8 | 3 |
| Fix-loop iterations | 1 | 1 | 0 |
| Tests added | 10 | 8 | 2 |
| LoC delta | ~46K (seed mirror + agent emit) | ~2200 (ruff patch) | ~600 (ruff delta) |
| Coverage delta | 9 → 10 resources (1) | unlocks ~50% surface | unlocks +~28% surface |
| Per-resource cost | high (per-resource agent work) | n/a | n/a |
| Per-kind cost | n/a | ~2 agents/kind | ~1 agent/kind (pattern established) |

The pattern is clear: **closing gaps in ruff is more leveraged than
agent-emitting resources one-by-one**. C2 took 3 agent-runs to unlock ~28% of
the surface; the same 3 agent-runs spent on resource verticals would emit
roughly 0.5 resources (News took 6 agents for 1 resource).

## Recommendation for Sprint C3

Three candidates (decided by sprint-driver):

1. **C3-form-kinds**: add sqlx variants for the 3 form kinds
   (`csrf_form_post_engine_call`, `form_get_post`, `signed_link_action`).
   Each ~1-2 agents (the form-DTO emitter is already framework-neutral and
   reusable). Would push surface to ~85-90%.
2. **C3-frontend**: implement Gap 4 (Ruby/Rails parser in `ruff_ruby_spo`).
   Independent track; serves the SPO-triplet pipeline. Larger scope.
3. **C3-template-get**: emit `template_get` as the OpenProject Angular-SPA
   skeleton route (just route registration, no template). Tiny scope, completes
   the API-first coverage.

My recommendation: **C3-form-kinds**. Highest leverage per agent-run after C2
(diminishing returns from per-kind work, but form kinds are the last
high-volume non-template kinds). C3-frontend is bigger but independent —
can run in parallel with another sprint.

## Artifacts in this commit

- `docs/SPRINT_C2_REPORT.md` (this file)
- `vendor/AdaWorldAPI-ruff/` (updated: +ajax_json.rs, +2 goldens, +ajax_json test, modified mod.rs/target.rs/examples TOML/SQLX-TARGET.md)
- `.claude/sprints/c2-gap2-ruff/STATE` (closed)

Per current sprint policy ("keine Patches, PR"), no patch / tarball artefacts
are committed in `calibration/`. The cumulative state lives in
`vendor/AdaWorldAPI-ruff/` and is reviewed via this PR; upstream `ruff`
application is a separate workflow (rebuild patches from the vendor mirror or
the workbench as needed).

🦋
