# SQLX target: axum + sqlx + HAL/JSON

The crate's first target, `rust-axum-seaorm`, emits server-rendered
jinja+askama plus a SeaORM `Entity`/`ActiveModel`/`Column` DSL. That shape
maps cleanly onto the WoA Flask app, where the route's `output` is a
template and the data layer is SeaORM. OpenProject is the opposite point in
the design space: an API-first backend over `sqlx::PgPool`, with HAL-JSON
envelopes returned to an Angular SPA — no server-side template engine, no
`Entity` DSL.

The seed already in tree at `crates/op-db/src/work_packages.rs:8`
(`use sqlx::{FromRow, PgPool, Row};`) and
`crates/op-api/src/handlers/work_packages.rs` makes the gap concrete: nothing
the existing emitter produces would compile against that crate layout. This
is **Gap 1** in `openproject-nexgen-rs/extraction-gap-proposals.md` — the
cheapest, highest-payoff close: a single ORM switch unblocks the whole
emitter for OpenProject.

The seaorm recipe set stays unchanged. The sqlx target is sibling, additive,
and selected at spec-load time.

## How to select it

A new `orm` field on `TargetSpec` carries the recipe family. The dispatch in
`codegen::emit` reads it once and routes each `HandlerKind` to either the
existing seaorm arm or the new sqlx arm.

```toml
# target-spec/openproject-axum-sqlx.toml
id           = "openproject-axum-sqlx"
orm          = "sqlx"
models_root  = "crate::models"
tenant_column = "project_id"

emit_kinds = [
    "list_for_tenant",
    "detail_for_tenant",
    "soft_delete",
    "toggle_bool_field",
]

[models.WorkPackage]
module_path = "work_package"
```

`orm` defaults to `"seaorm"` so every existing target spec keeps emitting the
same handlers it did before.

## What each HandlerKind emits (sqlx variant)

| kind                  | shape                                                                            | golden                                                                                       |
| --------------------- | -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `list_for_tenant`     | `sqlx::query_as::<_, T>` paged SELECT + `query_scalar::<_, i64>` COUNT, HAL Collection envelope | [`tests/golden/codegen/sqlx/expected/list_for_tenant.rs`](tests/golden/codegen/sqlx/expected/list_for_tenant.rs)     |
| `detail_for_tenant`   | `sqlx::query_as` SELECT with WHERE id = $1 AND tenant_col = $2, `HalResponse` wrap | [`tests/golden/codegen/sqlx/expected/detail_for_tenant.rs`](tests/golden/codegen/sqlx/expected/detail_for_tenant.rs) |
| `soft_delete`         | `sqlx::query` UPDATE SET active=false guarded by id + tenant_col + active=true, 204 | [`tests/golden/codegen/sqlx/expected/soft_delete.rs`](tests/golden/codegen/sqlx/expected/soft_delete.rs)             |
| `toggle_bool_field`   | `sqlx::query_as` UPDATE … SET active = NOT active … RETURNING *, `HalResponse` wrap | [`tests/golden/codegen/sqlx/expected/toggle_bool_field.rs`](tests/golden/codegen/sqlx/expected/toggle_bool_field.rs) |

The seed for the implementation lives in
[`src/codegen/sqlx_emit/list_for_tenant.rs`](src/codegen/sqlx_emit/list_for_tenant.rs);
the other three kinds follow the same pattern and are introduced as sibling
modules.

## What is NOT yet implemented

- The other 9 kinds in the seaorm coverage table (`template_get`,
  `get_redirect_shortcut`, `csrf_form_post_engine_call`, `form_get_post`,
  `ajax_json`, `download_blob`, `pdf_render`, `sa_admin_view`,
  `signed_link_action`) still route to the seaorm arm regardless of `orm =
  "sqlx"`. They will need their own sqlx recipes, or — once Gap 2 lands — a
  HAL envelope variant for `ajax_json` (planned).
- View templates (`codegen/jinja.rs`, `codegen/columns.rs`) are a no-op for
  sqlx targets. The pipeline still walks them, but `views/` is empty.
- Form DTOs (`codegen/dto.rs`) reuse the existing emitter unchanged — the DTO
  shape is framework-neutral; the choice of `sqlx` vs `seaorm` only changes
  the handler that consumes the DTO.
- Multi-crate output (`op-models` / `op-db` / `op-api` split) is Gap 3 — the
  current emitter writes one tree under a single `models_root`. The
  goldens above assume the consumer flattens that tree into their crate.
- PDF / blob downloads are out of scope for this target; they remain
  documented stubs (never `todo!()` — PR #102 guardrail).

## How the sqlx emitter differs from seaorm — concretely

- **Model paths.** seaorm uses `ModelMapping::model_type(root)` which appends
  `::Model` (e.g. `crate::models::work_package::Model`). The sqlx emitter
  builds the path inline as `<models_root>::<module_path>::<Class>` — no
  `::Model` suffix, no `::Entity`, no `::Column`. The Python class name
  reaches the import directly (`crate::models::work_package::WorkPackage`),
  matching the `FromRow` struct convention in the seed.
- **SQL.** Raw string literals via `sqlx::query_as::<_, T>` (returning rows),
  `sqlx::query_scalar::<_, i64>` (returning a count), or `sqlx::query`
  (executing a write). No `Entity::find()`, no `ActiveModel`, no
  `Column::Foo.eq(...)`.
- **Tenant scoping.** The contract's tenant predicate becomes a SQL
  `WHERE <tenant_col> = $N` clause, with the column name taken from
  `TargetSpec::tenant_column` interpreted as a `snake_case` SQL identifier
  (e.g. `project_id`). The seaorm arm reads the same field as a Rust
  `Column::TenantColumn` enum variant — the data field is shared; only the
  rendering changes.
- **Output.** Where seaorm wraps the result in an askama `Template` impl,
  the sqlx arm wraps in `HalResponse(...)` (from the openproject-nexgen-rs
  idiom: `crates/op-api/src/extractors.rs`) — either a `HalResponse(item)`
  for detail, or a `HalResponse(serde_json::json!({ "_type": "Collection",
  ... }))` for list. Soft-delete returns `StatusCode::NO_CONTENT`.

## Calibration

The five existing calibration lints in `calibrate.rs` apply with two
adjustments:

- `template-context-mismatch` is N/A for sqlx targets: there is no template,
  so `output.context_keys` has no consumer to validate against. The lint
  short-circuits when `orm = "sqlx"`.
- `output-kind-mismatch` widens: instead of checking
  template-path-vs-Template return type, it checks **HAL envelope shape vs
  `ApiResult<impl IntoResponse>`** — flag if the emitted return type and the
  envelope (single-item vs Collection) disagree with the contract's `output`
  classification.

The other three (`unmapped-model`, `dropped-fact`, `extractor-gap`) are
shape-agnostic and run unchanged.

## Example: emit one route end-to-end

Source — a Flask-style route classified as `list_for_tenant`:

```python
@bp.route("/projects/<int:project_id>/work_packages")
@login_required
def list_work_packages(project_id):
    items = WorkPackage.query.filter_by(project_id=project_id).order_by(WorkPackage.id.desc()).paginate()
    return jsonify(items)
```

Spec — `target-spec/openproject-axum-sqlx.toml` (see "How to select it"
above) with the `WorkPackage` mapping registered.

Run — `ruff-py-dto codegen --config extract.json --target
target-spec/openproject-axum-sqlx.toml --root op/ --out generated/`.

Output — the file shape matches
[`tests/golden/codegen/sqlx/expected/list_for_tenant.rs`](tests/golden/codegen/sqlx/expected/list_for_tenant.rs)
verbatim: paged `query_as`, scalar `COUNT`, HAL `Collection` envelope, no
askama, no `Entity::find()`.

Verify — the golden test under `tests/golden/codegen/sqlx/` compares the
emitter's output against the four expected files; any drift (extra import,
changed bind order, missing tenant clause) fails the test.

## Calibration acceptance criteria (specific to this target)

- Every `.bind(...)` call has exactly one matching `$N` placeholder in the
  preceding SQL string, and N is contiguous from 1.
- For tenant-scoped kinds (`list_for_tenant`, `detail_for_tenant`,
  `soft_delete`, `toggle_bool_field`), the SQL string contains the exact
  token `<tenant_column> = $` (post-`TargetSpec::tenant_column`
  substitution, snake_case).
- No emitted import contains `::Model`, `::Entity`, or `::Column` —
  trailing-segment guard against accidental seaorm-path leak. The
  `unmapped-model` lint already covers the unresolved case; this is the
  inverse guard for resolved-but-wrong-shape (an `EXTRACTOR-GAP` comment
  marks any seed that cannot yet produce the right shape).
- `HalResponse` appears in every non-204 handler; soft-delete returns
  `StatusCode::NO_CONTENT` and does NOT import `HalResponse`.
- `views/` is empty after the pipeline run; `contracts/` and `handlers/`
  are populated as for the seaorm target.
