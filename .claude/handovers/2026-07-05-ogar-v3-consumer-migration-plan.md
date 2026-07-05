# op-nexgen → OGAR V3 transpiler: consumer migration (integration plan)

> **Supersedes the stale OGAR model in the capstone.** The capstone
> (`2026-07-05-CAPSTONE-ar-shape-convergence.md`) and the convergence plan
> describe OGAR as `vocab + render-askama + class-view`. That is a **1-year-old
> mental model.** OGAR is now the **V3-shaped transpiler sink** — a full
> pipeline (detect → address → lift → propose → transpile). This plan records
> the corrected architecture (read from `AdaWorldAPI/{ruff,OGAR}@main`,
> 2026-07-05) and the migration it forces on op-nexgen. **Written as plan-only
> under a token wall; not executed.**

## 0. The correction in one sentence

op-nexgen has been **reimplementing a transpiler** (SurrealQL AST, DDL
projection, residual bucketing) that now lives, complete, in **ruff + OGAR +
lance V3**. op-nexgen's real job is **thin consumer + exactly one training
wheel** (ORM→AR back-projection). Everything else retires.

## 1. The real pipeline (what actually exists upstream)

| stage | crate | function |
|---|---|---|
| harvest | `ruff_{ruby,python,cpp,csharp}_spo` | source → SPO triples (`Triple.p` is a `String`) |
| **detect** | `ruff_spo_address::soc` | the 256-cap SoC lint. Over-cap sibling set is **Duplication** (→ mask by classid into a `ClassView`) or **Conflation** (data `has_field` + behaviour `has_function` under one parent → **split**). `soc_findings()` / `law_holds()` |
| **address** | `ruff_spo_address::mint` | SPO → `(part_of:is_a)` Facet → lance V3 GUID SoA. `mint_with_classid` takes the OGAR-codebook resolver. **This is `op::part_of::Is_a`.** |
| lift | `ogar-from-ruff::lift_model_graph` | ruff `Model` → `ogar_vocab::Class` (the O3 lift — exists now) |
| **propose** | `ogar-proposal::class_to_drafts` | Class → `lance-graph-ontology::MappingProposal` — the **proposed resolver** |
| config-as-data | `ogar-from-schema` | json_schema / openapi / prisma / ttl → `Class` — the **mini-schema stays data** |
| transpile | `ogar-emitter` + `ogar-adapter-{surrealql,clickhouse-ddl,ttl}` | Class → DDL / V3 triples. `emit_surrealql_ddl` is **wired** |
| behave | `ActionDef + KausalSpec` + `ogar-action-handler` | behaviour — "the MAGIC, in the Core never the address" = `op::part_of::Is_a(input)` |

## 2. The V3 substrate (OGAR is its transpiler sink)

- Node budget is **shape-adaptive, one 12-slot factoring**, GUID `4–12` wide:
  - **AR/Rails/Ruby → 6× `(part_of : is_a)`** (6-tier cascade; mereology × taxonomy)
  - **generic → 4× triplets** (SPO)
  - **Odoo → 3× quadruplets** (SPOG)
  - `6·2 = 4·3 = 3·4 = 12` — same substrate, different factoring per source.
- **lance V3 now carries the SurrealQL AST/DDL shape natively.** There is **no
  external SurrealDB**, and no need for an op-side SurrealQL AST. When the DDL
  shape is needed it is **rendered** off V3 through the ERB fieldview×classview
  kit — **askama (Rust) / jinja (Python)** (`ogar-render-askama`,
  `CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK`). Substrate + render kit = the transpiler.
- classid `= [hi u16 concept ‖ lo u16 render]` resolves to `ClassView` (skin) ‖
  `Class` (shape) ‖ `ActionDef + KausalSpec` (behaviour).

## 3. op-nexgen's narrowed role + crate fate

op-nexgen = **consumer of the OGAR V3 transpiler + one training wheel.**

| op-nexgen crate | fate | superseded by |
|---|---|---|
| `op-surreal-ast` | **retire** | lance V3 SurrealQL-AST shape + `ogar-adapter-surrealql` (ERB render) |
| `op-codegen-projection` | **retire** | `ogar-emitter` / `ogar-adapter-surrealql` |
| `op-codegen-residual` | **retire** → data moves to `.claude/harvest` | `ruff_spo_address::soc` + the ORM→AR back-projection config |
| `op-codegen-pipeline` | **thin to a consumer** | `ogar-from-ruff::lift_model_graph` → Class → adapters |
| `op-codegen-bucket` | evaluate (it's the `RouteBucketTyped` spine consumer) | possibly folds into the consumer |
| `op-canon` | **keep** — OP codebook snapshot (26/26 classid convergence) | consumes `ogar-vocab` |
| `op-api / -work-packages / -db / …` (the app) | **untouched** — the actual Rust port | — |

Consumer path: `OpenProject source → ruff_ruby_spo::extract_app_with →
ModelGraph → ogar-from-ruff::lift_model_graph → ogar_vocab::Class →
{ ogar-proposal | ogar-adapter-surrealql::emit_surrealql_ddl | ogar-emitter }`.

> **2026-07-05 note (route-kind stratum):** `crates/ruff_python_dto_check/`
> (not in the table — never a workspace member) is PARKED as the
> un-upstreamed sqlx-target delta against live ruff's
> `ruff_python_dto_check`; see its README + OGAR
> `E-ROUTE-KIND-VERB-STRATA` (council-rejected SoC rhyme; surviving
> carve + kind A/B probe). `op-codegen-bucket`'s "evaluate" fate gains
> one input: its `OpHandlerKind` mirrors the parked taxonomy's sqlx
> subset and is the typed-spine consumer the kind A/B would use.

## 4. The one training wheel — ORM→AR back-projection (`.claude/harvest/`)

ruff is smart for AR/Rails/Ruby. The **only** gap: mapping **ORM-shaped** source
*before* it is AR/Rails/Ruby, so we can **back-project the DB schema into guessed
AR behaviour** and recover the uncovered residual (the 90→100 the oracle diff
left open). The D-AR-3.5 patch was the crude v1 of exactly this, buried in
vendored ruff. It becomes **resolver config (data)** in `.claude/harvest/`:

| ORM/DB shape (input) | guessed AR behaviour (output) | kind |
|---|---|---|
| `t.<type> :c` | `Field{field_type}` | direct |
| `null: false` | `required` + `validates :c, presence` | direct → **guess** |
| `<x>_id` (int) | `belongs_to :x` | **guess** |
| `<x>_id` + `<x>_type` | `belongs_to :x, polymorphic` | **guess** |
| `add_index unique` | `validates :c, uniqueness` | **guess** |
| join table `a_b` (2 FKs, no model) | `has_and_belongs_to_many` both sides | **guess** |
| `<assoc>_count` | `counter_cache: true` | **guess** |
| `lft/rgt` \| `parent_id` | nested-set \| tree | weak |

- **direct** + pattern→declaration **guess** rows = pure **data** → live in
  `.claude/harvest/` resolver config.
- Rows needing the *actual* AR code (don't guess `belongs_to` if already
  declared; is the constraint truly a validation?) = **ruff gets smarter**
  (spec to a ruff session; do **not** fake in op-nexgen).
- **Measure, don't claim:** every guess is validated against the AR oracle (the
  90/10 oracle-diff discipline). No shipped coverage that wasn't measured.

## 5. Sequenced migration (execute after this PR; token-walled here)

1. **[DONE]** un-vendor lance-graph + OGAR → git deps (this PR).
2. **[DONE]** Stand up `.claude/harvest/` — the ORM→AR back-projection resolver
   config (data) + README. *(landed 4102eb0)*
3. **[DONE]** Un-vendor ruff → git dep: retire the D-AR-3.5 patch; the
   back-projection moves to §4; cross-ref rows → ruff spec. `vendor/` retires
   entirely. *(this commit; upstream-first landed as ruff 8d6c31b — schema
   stratum, visibility filtering, tree harvest, ColumnNotNull/inherits; the
   D-AR-3.5 mechanism lives in ruff, the guess-rules live in .claude/harvest)*
4. **[DONE]** Add op-nexgen deps: `ogar-from-ruff`, `ogar-adapter-surrealql`
   (git deps, same as ogar-vocab today; `ogar-adapter-surrealql` pulled with
   default features only — its `surrealdb-parser` feature stays off).
   *(landed in `op-codegen-pipeline`'s `ogar-emit` optional feature; `ogar-emitter`
   deliberately deferred — it targets V3 triples, not the DDL shape this step
   needs. `ogar-adapter-surrealql::emit_surrealql_ddl` is the DDL path used
   instead.)*
5. **[DONE — additive]** Rewire `op-codegen-pipeline` → consumer: `ogar-from-ruff`
   → Class → `ogar-adapter-surrealql::emit_surrealql_ddl`. Port the pipeline tests.
   *(landed as `op_codegen_pipeline::ogar_consumer`, feature-gated behind
   `ogar-emit` and wired alongside — not replacing — the native
   `op-surreal-ast` path; `compile_op` / `emit_surreal_via_ogar` /
   `render_surreal_via_ogar` / `render_classid_of` cover the source → ruff →
   OGAR lift/mint → Class → adapter-emit chain, plus the convergence-pin and
   `tests/ogar_consumer_fixture.rs` fixture tests. Full test-porting off the
   native path — i.e. retiring `op-surreal-ast` — is step 6, not yet done.)*
6. Retire `op-surreal-ast`, `op-codegen-projection`, `op-codegen-residual`
   (fold their oracle/falsifier tests into the consumer + `.claude/harvest`).
7. Keep `op-canon` + the app crates; decide `op-codegen-bucket`.
8. Correct the stale OGAR model in the capstone + convergence plan.

## 6. Guardrails (what NOT to do)

- **No SurrealDB, no op-side SurrealQL AST, no hand-minted DDL.** The DDL shape
  is a render off lance V3 via OGAR's ERB kit.
- **Intelligence lives in ruff** (soc detect / mint address / propose). op-nexgen
  does not detect, address, or transpile — it consumes.
- **Config is data** (`.claude/harvest` resolver config + `ogar-from-schema`).
  Where data is insufficient, make **ruff** smarter — spec it, don't fake it here.
- **Behaviour = `ActionDef + KausalSpec` reached through the classid**, never in
  the address. Overflow methods/controllers become these reusable action nodes.
- op-nexgen owns **the codebook snapshot (`op-canon`) + the ORM→AR training
  wheel + the app port**. Nothing else.
