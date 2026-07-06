# ruff_python_dto_check — the un-upstreamed ERB-fieldview → askama render + Action-kind recipe corpus

> **Status: teaching corpus, upstream-ward migration** (operator ruling
> 2026-07-05; OGAR `E-RECIPE-REUNION-ORDER`). This directory is a **spec
> fragment**, not a workspace member: no `Cargo.toml`, and its modules
> reference `crate::contract::{HandlerKind, RouteContract}` which live in
> **upstream ruff's `crates/ruff_python_dto_check/`** (that crate has
> `contract.rs` + the seaorm codegen arm — but **no `sqlx_emit/`**). This
> fragment is the **un-upstreamed sqlx-target delta** against that live
> crate. It stays a non-member — but its CONTENT is doctrine input, not
> dead weight. (An earlier README framed it as a "parked parallel-model
> to retire"; that framing was corrected — see below.)

## What this is — and why it matters

Two things the reunion (Redmine ⇄ OpenProject at the AR/Rails shape)
needs, captured on real source:

1. **ERB-fieldview → askama render recipes.** Six sqlx emit recipes with
   eight golden files (ajax_json + csrf_form_post each carry a with-model
   + stub branch). These are the compiled, JSON-free port of Redmine's
   ERB field partials — the render leg the operator named: *"ERB redmine
   fieldview teaches us to translate into askama classview fieldmask."*
   They seed the `ogar-render-askama` classview × fieldmask kit
   (OGAR `docs/CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK.md`).
2. **The Action-kind `HandlerKind` corpus.** The 13-kind taxonomy
   (`list_for_tenant`, `detail_for_tenant`, `soft_delete`,
   `toggle_bool_field`, `ajax_json`, `csrf_form_post_engine_call`, … —
   6 sqlx-emitted, 7 seaorm-only) is the concrete **Action-kind recipe
   family** — one of the four families (Lifecycle / Guard / Relation /
   **Action**) that RAILS-COVERAGE-KIT §5 mints as content-addressable
   `RecipeConceptId`s. It converges cross-consumer exactly like class
   concepts: canonical id + per-language `LabelDto` skin.

Shape knowledge encoded here (golden-tested; exists nowhere else as
**sqlx** recipes): HAL envelope conventions
(`_type`/`_embedded`/`_links.self.href`), tenant scoping as
`WHERE <tenant_col> = $N`, bind-placeholder contiguity from `$1`, the
two-branch with-model/stub pattern, and the never-`todo!()` guardrail
(PR #102).

## Route dedup IS SoC (not a rhyme — operator canon)

The route dedup this corpus embodies is an INSTANCE of the SoC doctrine,
not an analogy to it:

- *"N routes that are the same record, different visible fields are ONE
  templated ClassView render with N masks — route proliferation is
  usually an un-applied mask"* (CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK,
  operator 2026-06-29).
- `< 256` fields → maskable by one ClassView; `≥ 256` → the god-object
  split — **"the same SoC the `ruff_spo_address::soc` lint flags"**. The
  field-view mask cap and the soc sibling cap are the SAME constant:
  `FIELD_MASK_CAP = MAX_SIBLINGS_PER_TIER`.

## Why not wired into the workspace

Per `.claude/handovers/2026-07-05-ogar-v3-consumer-migration-plan.md` §6:
**intelligence lives in ruff** (detect / address / propose) and **recipes
live in OGAR** (`ogar-render-askama` + the recipe-concept codebook);
op-nexgen consumes. Compiling a route-transpiler op-side would recreate
the parallel-model anti-pattern. So the crate stays a non-member — the
migration is upstream-ward, not into this workspace.

## The queued implementation gap (honest ledger — code-verified 2026-07-05, OGAR `E-F17-PREREQ-VERIFIED`)

The convergence is an operator order (`[G]`); its coverage is unmeasured
(`[H]`). Verified gap status, upstream:

- **CLOSED** — writes/calls capture (the F17 fact prerequisite):
  `Function::{writes, calls}` live in `ruff_spo_triplet` (ir.rs:264-284),
  populated by the Ruby walker, emitted as `writes_field`/`calls`
  triples. The controller DO-arm harvest is also live
  (`extract_tree_with` → public actions → `lift_actions` → `ActionDef`).
- **OPEN** — the `routes.rb` stratum: HTTP verb, member/collection
  routes, return-shape (collection|item) — the one remaining fact source
  for Action-kind classification.
- **OPEN** — the OGAR **recipe-concept codebook** (the four families as
  `RecipeConceptId`s) isn't minted; `KausalSpec::LifecycleTrigger` still
  carries the raw surface string — *"until that lands, the bitmask is
  per-consumer (the zoo)"* (RAILS-COVERAGE-KIT §5). `OpHandlerKind` is
  the per-consumer enum awaiting that codebook. Mint rides the
  serialized-allocation train.
- Coverage gate: the OP⇄Redmine action A/B (redmine-op plan C5) + the
  F17 body-triage falsifier measure the coverage %; don't ship claimed
  coverage unmeasured. (This measures a *canonized* convergence's
  coverage — it is not a test of whether the convergence is real.)

## Retirement path

1. Upstream the sqlx arm into ruff's `ruff_python_dto_check` (the
   E-VENDOR-DELTA pattern: spec the delta upstream, don't fork here).
2. Render recipes migrate to `ogar-render-askama`; the Action-kind
   taxonomy mints as `RecipeConceptId`s in the OGAR recipe codebook.
3. This directory then retires — conditional on 1–2 landing and the
   goldens migrating with them.

## Do NOT

- Do not add a `Cargo.toml` / wire into `[workspace] members`.
- Do not grow new recipes here — spec them upstream (ruff / OGAR).
- Do not delete without migrating the goldens: they are the only
  golden-tested statement of the **sqlx** recipe shapes (upstream's
  goldens cover seaorm only).
