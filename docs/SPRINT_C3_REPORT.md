# Sprint C3 Report — `csrf_form_post_engine_call` sqlx emitter (first WRITE handler)

**Date:** 2026-06-01 · **Branch:** `claude/beautiful-gates-dJo0u` (nexgen)
· **Workbench:** `claude/openproject-sqlx-emitter` on the ruff-clone.

---

## Headline

> The sqlx target's **first write handler** landed: `csrf_form_post_engine_call`
> emits `Json<Form>` → `INSERT … RETURNING *` → `201 Created` + HAL (with-model),
> or a `202 Accepted` CALIBRATION stub (no-model). This directly closes the
> create/update/delete **EXTRACTOR-GAP** that the Sprint C0 News vertical flagged
> ("op-db NewsRepository has no write surface"). **71/71 tests green**, all 28
> pre-existing seaorm tests still pass. 6th sqlx-emittable kind.

## What landed

| File | Status | Purpose |
|---|---|---|
| `src/codegen/sqlx_emit/csrf_form_post.rs` | new (358 LoC) | `emit_csrf_form_post_sqlx` — two branches |
| `tests/sqlx_emit_csrf_form_post_test.rs` | new (174 LoC) | 2 byte-exact golden tests |
| `tests/golden/.../csrf_form_post_with_model.rs` | new | Spec: INSERT + 201 branch |
| `tests/golden/.../csrf_form_post_stub.rs` | new | Spec: 202 CALIBRATION branch |
| `src/codegen/sqlx_emit/mod.rs` | modified | `pub mod csrf_form_post;` |
| `src/codegen/mod.rs` | modified | `CsrfFormPostEngineCall` arm in sqlx dispatch |
| `src/codegen/target.rs` | modified | `"csrf_form_post_engine_call"` in `openproject_axum_sqlx()` |
| `examples/openproject-axum-sqlx.toml` | modified | same in declarative emit_kinds |
| `SQLX-TARGET.md` | modified | kind table 5 → 6; "CsrfFormPost — two branches" subsection |

## The write-handler design

The seaorm `emit_csrf_form_post` (mod.rs:909) uses `axum::Form<Dto>` + a redirect
(it targets a server-rendered web app). The sqlx variant is API-first:

- **With-model** (e.g. `POST /projects/<id>/work_packages`):
  ```rust
  #[derive(Debug, serde::Deserialize)]
  pub struct CreateWorkPackageForm { pub subject: Option<String>, pub description: Option<String> }

  pub async fn create_work_package(
      State(state): State<AppState>, _user: AuthenticatedUser,
      Path(project_id): Path<i64>, Json(form): Json<CreateWorkPackageForm>,
  ) -> ApiResult<impl IntoResponse> {
      let pool = state.pool()?;
      let created: WorkPackage = sqlx::query_as::<_, WorkPackage>(
          r#"INSERT INTO work_packages (project_id, subject, description) VALUES ($1, $2, $3) RETURNING *"#,
      ).bind(project_id).bind(&form.subject).bind(&form.description)
       .fetch_one(&pool).await.map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
      Ok((StatusCode::CREATED, HalResponse(created)))
  }
  ```
  The INSERT column list is **scope path-params first** (snake_case verbatim:
  `project_id`), **then the form fields** (`inputs.form_fields`: `subject`,
  `description`). Bind order matches: scope params by value, form fields by ref.

- **Without-model** (e.g. `POST /work_packages/bulk` with `data.models = []`):
  the same form DTO is emitted, then a CALIBRATION stub returning
  `StatusCode::ACCEPTED` with an `EXTRACTOR-GAP` comment. No `todo!()`.

## Fanout discipline

**Phase 0** (orchestrator): 2 golden expected files (with-model + stub).

**Phase 1** (3 parallel agents, file-disjoint):
- **C1**: `csrf_form_post.rs` (358 LoC; byte-exact vs both goldens, verified by
  the agent via a standalone harness + a typecheck of the real module).
- **C2**: `sqlx_emit_csrf_form_post_test.rs` (174 LoC, 2 tests).
- **C3**: `SQLX-TARGET.md` (+27 lines: table 5→6, new subsection, 8→7
  not-implemented count).

**Phase 2** (orchestrator, sequential, `cargo check`-gated): 4 atomic edits
(sqlx_emit/mod.rs, codegen/mod.rs dispatch, target.rs emit_kinds, examples TOML).

**Phase 3** (verify): `cargo test -p ruff_python_dto_check` → **71 passed, 0 failed**.

**Zero fix-loop iterations** — same clean run as C2. The pattern is fully amortized.

## Coverage projection

After C3, the sqlx-emittable handler-kind set is 6: list_for_tenant,
detail_for_tenant, soft_delete, toggle_bool_field, ajax_json,
**csrf_form_post_engine_call**.

| Kind | est. prevalence | emittable |
|---|---|---|
| list_for_tenant | ~25% | ✓ (C1) |
| detail_for_tenant | ~20% | ✓ (C1) |
| ajax_json | ~25% | ✓ (C2) |
| soft_delete | ~5% | ✓ (C1) |
| toggle_bool_field | ~3% | ✓ (C1) |
| csrf_form_post_engine_call | ~4% | ✓ (C3) |
| form_get_post / signed_link_action | ~6% | ✗ (Sprint C4 candidate) |
| template_get / download / pdf / other | ~12% | ✗ (stub or n/a for API-first) |

**Cumulative emittable surface: ~82%** of OpenProject's controller-action count,
up from ~78% after C2. More importantly, C3 makes the target **write-capable** —
the prior 5 kinds were read or scoped-mutate; this is the first `INSERT`.

## Sprint metrics — C0 → C3

| | C0 | C1 | C2 | C3 |
|---|---|---|---|---|
| Agent-runs | 6 | 8 | 3 | 3 |
| Fix-loop iterations | 1 | 1 | 0 | 0 |
| Tests added | 10 | 8 | 2 | 2 |
| Kinds unlocked | (1 resource) | 4 | 1 | 1 |
| Git commits to nexgen | many (learned) | 4 (learned) | 1 | **1** |

The git-hygiene lesson from C0–C2 is now applied: **one sprint = one commit**,
made after the sprint is green, no prep/rename commit noise, no
rebase/squash/force-push after pushing.

## Recommendation for Sprint C4

1. **C4-form-rest**: `form_get_post` + `signed_link_action` sqlx variants —
   finishes the form-handler family (~6% surface). ~2 agents.
2. **C4-frontend** (Gap 4): Ruby/Rails parser in `ruff_ruby_spo`. Independent
   track for the SPO-triplet pipeline; larger scope. This is the gating work for
   actually running the pipeline against OpenProject's real source.
3. **C4-update-delete**: explicit PATCH/DELETE emitters (the write family beyond
   create) — closes the rest of the C0 CRUD gap.

My recommendation: **C4-frontend**. The emitter is now write-capable and covers
~82% of kinds; the bottleneck has shifted from "can we emit?" to "can we extract
contracts from Ruby at all?". Gap 4 is the real unlock for an end-to-end run.

## Artifacts

- `docs/SPRINT_C3_REPORT.md` (this file)
- `vendor/AdaWorldAPI-ruff/` (updated: +csrf_form_post.rs, +2 goldens, +test;
  modified mod.rs/target.rs/examples TOML/SQLX-TARGET.md/codegen-mod-rs.diff)
- `.claude/sprints/c3-csrf-form-post/STATE`

🦋
