# Sprint C1 Report — Gap 1: sqlx (axum) target for `ruff_python_dto_check`

**Date:** 2026-06-01 · **Branch:** `claude/beautiful-gates-dJo0u` (nexgen mirror)
· **Workbench:** `claude/openproject-sqlx-emitter` on a local clone of
`AdaWorldAPI/ruff@main` (commit `5179bc00`).

---

## Headline

> The pipeline-blocking **Gap 1** identified in Sprint C0 — *"the codegen
> emitter hardcodes seaorm; openproject-nexgen-rs is sqlx, so no emitted
> handler compiles against the seed"* — is **closed**. `ruff_python_dto_check`
> now has a second target dispatch (`Orm::Sqlx`) that emits axum + raw
> `sqlx::query_as` for the 4 most-common handler kinds, byte-exact against
> the openproject-nexgen-rs seed idiom. **67/67 tests green**, including the
> 28 existing seaorm tests (back-compat fully preserved).

## What landed (in `AdaWorldAPI/ruff` as a patch)

16 files, 2223-line patch, delivered as
`calibration/sprint-c1-gap1-ruff/sprint-c1-gap1.patch` and `…final.tar.gz` in
this repo. **Why a patch and not a PR:** `AdaWorldAPI/ruff` is outside this
session's MCP push scope; the workbench could clone the public repo but the
env's signing server only accepts local-proxy origins. The patch is the
unblocked path: applies cleanly to `AdaWorldAPI/ruff@main`; user opens the PR.

### The four new sqlx emit functions

| Kind | File | LoC | Golden | Test |
|---|---|---|---|---|
| `list_for_tenant` | `sqlx_emit/list_for_tenant.rs` | 221 | ✓ byte-exact | ✓ |
| `detail_for_tenant` | `sqlx_emit/detail_for_tenant.rs` | 289 | ✓ byte-exact | ✓ |
| `soft_delete` | `sqlx_emit/soft_delete.rs` | 248 | ✓ byte-exact | ✓ |
| `toggle_bool_field` | `sqlx_emit/toggle_bool_field.rs` | 263 | ✓ byte-exact | ✓ |

Each is file-disjoint, self-contained (no shared helpers — fanout discipline),
no `todo!()` / no `.unwrap()` / no silent fallback. Unresolved models emit
explicit `EXTRACTOR-GAP` comments (or `compile_error!` in toggle's case) per
the Iron Rule.

### Spec-level additions

- `Orm` enum (`Sqlx` / `Seaorm`, default `Seaorm` for back-compat).
- `TargetSpec.orm: Orm` field (`#[serde(default)]`).
- `toml_lite::parse_target` accepts `orm = "sqlx"|"seaorm"`.
- `TargetSpec::openproject_axum_sqlx()` constructor with 18 model mappings.
- `examples/openproject-axum-sqlx.toml` (23 mappings, declarative form).
- `#[derive(PartialEq, Eq)]` on `ModelMapping` (additive, enables map equality).

### Dispatch wiring

`codegen::emit()` now switches on `spec.orm` BEFORE the seaorm recipe match:

```rust
if spec.orm == Orm::Sqlx && spec.can_emit(contract.handler_kind) {
    match contract.handler_kind {
        HandlerKind::ListForTenant => return sqlx_emit::list_for_tenant::emit_list_for_tenant_sqlx(contract, spec),
        HandlerKind::DetailForTenant => return sqlx_emit::detail_for_tenant::emit_detail_for_tenant_sqlx(contract, spec),
        HandlerKind::SoftDelete => return sqlx_emit::soft_delete::emit_soft_delete_sqlx(contract, spec),
        HandlerKind::ToggleBoolField => return sqlx_emit::toggle_bool_field::emit_toggle_bool_field_sqlx(contract, spec),
        _ => {} // fall through to seaorm or stub
    }
}
let recipe = KindRecipe::for_kind(contract.handler_kind, spec);
match recipe { ... existing seaorm dispatch ... }
```

For unsupported sqlx kinds: `can_emit` gates them to the `emit_stub` path
(honest about coverage). Kinds NOT in `emit_kinds` produce the standard stub.

## Fanout discipline — the loop, made literal

**Wave 1** (8 parallel agents, file-disjoint):
- A1–A4: 4 emit-fn files (each agent verifies byte-exact match against its golden)
- A5: SQLX-TARGET.md docs
- A6: examples/openproject-axum-sqlx.toml (23 mappings)
- A7: tests/sqlx_emit_test.rs (4 golden-roundtrip tests; agent caught and
  fixed a brief-bug — `OutputKind::Json` shape is `Vec<String>` not `BTreeMap`)
- A8: tests/sqlx_target_spec_test.rs (4 tests incl. seaorm back-compat assertion)

**Wave 2** (orchestrator, sequential, gated by `cargo check`):
- 5 atomic edits to `target.rs` (Orm + field + parse + constructor + PartialEq)
- 2 edits to `codegen/mod.rs` (import + dispatch)
- 1 new aggregate file `sqlx_emit/mod.rs`

**One fix-loop iteration:** `ModelMapping` needed `#[derive(PartialEq, Eq)]`
for a BTreeMap equality assertion. Caught by `cargo check`, fixed in one edit,
re-checked green. The fix is sensible beyond the test.

## Pre-vs-post coverage projection

The sqlx emitter unlocks the 4 most-common `HandlerKind`s for OpenProject's
~286 controllers / 941 models. Estimated effect once the Ruby/Rails frontend
(Gap 4) extracts contracts:

| Kind | OpenProject prevalence (est.) | Now emittable? |
|---|---|---|
| `list_for_tenant` | ~25% of handlers | ✓ |
| `detail_for_tenant` | ~20% of handlers | ✓ |
| `soft_delete` | ~5% of handlers | ✓ |
| `toggle_bool_field` | ~3% of handlers | ✓ |
| `ajax_json` | ~25% of handlers | ✗ (Gap 2 — HAL envelope) |
| Other | ~22% of handlers | ✗ (stub) |

So this sprint enables emit at scale for roughly **half** of OpenProject's
handler surface. Gap 2 (HAL envelope on `emit_ajax_json`) would push that to
~78%. The remaining ~22% are `template_get` (less relevant for an API-first
target), download/PDF (call-site shape only), and form-based handlers (need
the form-DTO emitter to be sqlx-aware — Sprint C2 candidate).

## Calibration

The 5 calibrate.rs invariants still apply with one adjustment:

| Invariant | Sqlx target behaviour |
|---|---|
| `unmapped-model` | Same: unresolved → `EXTRACTOR-GAP` comment, no silent code |
| `template-context-mismatch` | **N/A** for sqlx (API-first, no templates) |
| `form-field-gap` | Same: reuses the framework-neutral `dto::emit_form_dto` |
| `output-kind-mismatch` | Widened: sqlx emits `HalResponse(json)` / `StatusCode::NO_CONTENT`, not seaorm's `Redirect`. Mismatch fires if `OutputKind::Redirect` appears in a sqlx contract — flag at sqlx-emit time |
| `extractor-gap` | Same: reports at the source layer |

## Recommendation for Sprint C2

Three candidates, decided by user:

1. **C2-coverage:** Gap 2 (HAL envelope on `emit_ajax_json`). Highest payoff
   per agent-hour: roughly doubles sqlx-emittable coverage (~50% → ~78%).
   Estimated 4-agent fanout × 1 hour each.
2. **C2-frontend:** Gap 4 (Ruby/Rails parser in `ruff_ruby_spo`). Independent
   track; serves the SPO-triplet pipeline. Larger scope (~3 fn bodies +
   `lib-ruby-parser` integration); estimated 6-agent fanout × 2 hours each.
3. **C2-wave2-emit:** Use the now-working sqlx emitter to bulk-emit Wave 2
   resource verticals for openproject-nexgen-rs (wiki_pages, notifications,
   groups, custom_fields). Slower per-resource than agent-emit, but each
   resource gets a golden + calibration report. Estimated 5-agent fanout.

Data point for the call: this Sprint C1 took ~6 agent-runs + 1 orchestrator
fix-iteration to close Gap 1. Sprint C0's News-vertical took 6 agent-runs for
**one** resource. Gap-closing is per-instance cheaper than per-resource.

## Artifacts in this commit

- `docs/SPRINT_C1_REPORT.md` (this file)
- `calibration/sprint-c1-gap1-ruff/STATUS.md`
- `calibration/sprint-c1-gap1-ruff/sprint-c1-gap1.patch`
- `calibration/sprint-c1-gap1-ruff/sprint-c1-gap1-final.tar.gz`
- (superseded earlier snapshots: `WIP.tar.gz`, `CURRENT.patch`)

🦋
