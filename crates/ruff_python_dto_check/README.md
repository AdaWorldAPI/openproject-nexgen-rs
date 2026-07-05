# ruff_python_dto_check — PARKED (un-upstreamed sqlx-target delta, not a compiled crate)

> **Status: PARKED, 2026-07-05** (council-reviewed — OGAR
> `E-ROUTE-KIND-VERB-STRATA`, 5+3 pass). This directory is a **spec
> fragment**, not a workspace member: it has no `Cargo.toml`, and its
> modules reference `crate::contract::{HandlerKind, RouteContract}` which
> live in **upstream ruff's `crates/ruff_python_dto_check/`** (that crate
> has `contract.rs` + the seaorm codegen arm — but **no `sqlx_emit/`**).
> This fragment is precisely the **un-upstreamed sqlx-target delta**
> against that live crate. It is deliberately NOT wired into this
> workspace and MUST NOT be — see "where this goes" below.

## What this is

Six sqlx emit recipes (`list_for_tenant`, `detail_for_tenant`,
`soft_delete`, `toggle_bool_field`, `ajax_json`,
`csrf_form_post_engine_call`) with eight golden files (ajax_json and
csrf_form_post each carry a with-model + stub branch pair), plus
`SQLX-TARGET.md` — the spec for the axum+sqlx+HAL target matching this
repo's `op-db`/`op-api` idioms. The six are the sqlx-emitted subset of
the 13-kind `HandlerKind` taxonomy (the other 7 are seaorm-only).

Shape knowledge encoded here that must not be lost (it exists nowhere
else as golden-tested **sqlx** recipes): the HAL envelope conventions
(`_type`/`_embedded`/`_links.self.href`), tenant scoping as
`WHERE <tenant_col> = $N`, bind-placeholder contiguity from `$1`, the
two-branch with-model/stub pattern, and the never-`todo!()` guardrail
(PR #102).

## Why parked, not wired

Per `.claude/handovers/2026-07-05-ogar-v3-consumer-migration-plan.md` §6:
**intelligence lives in ruff** (detect / address / propose) and **recipes
live in OGAR adapters**; op-nexgen consumes. Compiling a route-transpiler
here would recreate the parallel-model anti-pattern the migration retires.

## The council verdict (keep the eye honest)

The proposed synergy "route deduplication is the DO-arm mirror of ruff's
SoC lint" was **REJECTED** as mere-rhyme by OGAR's 5+3 hardening council —
see OGAR `.claude/board/EPIPHANIES.md` `E-ROUTE-KIND-VERB-STRATA` (+
DISCOVERY-MAP twin) for the grounds. What survives and what this
directory feeds:

- **The carve:** a `HandlerKind` is **verb × transport ×
  persistence-shape** — a route RECIPE, not a verb. The verb projected
  OUT OF a kind (`soft_delete` → `is_a` update) is the only
  verb-codebook candidate; the recipe itself is adapter-side
  (`ogar-adapter-*` / the render kit), never a vocab row.
- **The probe:** the six kinds are fuel for the **OP⇄Redmine
  route-surface kind A/B** (pre-registered KILL threshold required) —
  an independent convergence probe, DISTINCT from the capstone C5 verb
  A/B. No verb-codebook row is minted until it runs green.

## Where this goes (retirement path)

1. The sqlx arm upstreams to ruff's `ruff_python_dto_check` (the
   E-VENDOR-DELTA pattern: spec the delta upstream, don't fork here).
2. Emit recipes migrate to `ogar-adapter-*` as render-kit lowering
   passes.
3. This directory then retires — conditional on 1–2 landing and the
   goldens migrating with them.

## Do NOT

- Do not add a `Cargo.toml` / wire into `[workspace] members`.
- Do not grow new recipes here — spec them upstream (ruff / OGAR).
- Do not delete without migrating the goldens: they are the only
  golden-tested statement of the **sqlx** recipe shapes (upstream's
  goldens cover seaorm only).
