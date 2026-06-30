# Handover — OP + Redmine → OGAR convergence (assessment + increment plan)

> 2026-06-30. Captures a grounded 6-agent assessment (1.04M tokens) of
> converging OpenProject + Redmine onto the OGAR transpile / ClassView
> (ERB→askama) substrate, **with the operator's load-bearing correction**.
> Companion to: ruff#38 (writes/calls, MERGED), OGAR#143 (recipe-bitmask
> canon, MERGED), PR#60 (this branch — coverage-kit groundwork).

---

## 0. THE CORRECTION — read first (operator, 2026-06-30)

**We do NOT remove ActiveRecord. AR is exactly what we keep.** The earlier
"castrate the hand-rolled PostgreSQL Rails ActiveRecord betrayal" framing
(mine, last turn) was WRONG and inverts the architecture.

- **KEEP** — the **ActiveRecord pattern**: the class *is* a record + behavior
  (associations, validations, callbacks, STI). This is the domain model. It is
  literally what **OGAR = Open Graph Active Record** represents; the name was
  inspired by AR in OpenProject. The OGAR `Class`/`ClassView` IS the active
  record.
- **REMOVE** — the **ORM**: the hand-rolled persistence plumbing that sits
  between AR and the DB. In `openproject-nexgen-rs` that is `op-db`'s
  hand-typed SQL repos + `FromRow` row structs (`op-db` = 9009 LOC) and
  `op-api`'s hand-mapped row→DTO code (8592 LOC). That layer is the "betrayal"
  — an ORM re-implemented by hand.
- **WIRE** — "**pure AR on Rails**": the AR domain model backed **directly by
  the OGAR graph substrate** (the classid-keyed node + ClassView IS the record;
  persistence is the graph via OGAR emit), with **no ORM intermediary**.

**The "open-heart operation":** excise the ORM organ (op-db SQL layer), keep
the AR heart (the domain model in op-models/op-contracts), re-plumb AR onto
OGAR. The ClassView is the AR; the graph is the store; the ORM is gone.

**Redmine-as-root:** OpenProject is a Redmine→ChiliProject→OpenProject fork.
Redmine's cleaner ancestral AR (ERB **fieldview**) defines the canonical
ClassView; OP's AR (which accreted the hand-rolled ORM on top) converges onto
it. **fieldview/erb → classview/askama** = the AR's view layer (Rails ERB
fieldview) becomes the OGAR ClassView rendered via askama — the render *skin*
over the AR *substance* ("ice caking"). Redmine "remembers the roots."

---

## 1. Verdict

Two independent assessors over four grounded maps:

- **convergence-architect → RESONATES.** The 0-friction boundary is real and
  the asymmetry is *more* favorable than expected: OGAR already ships both
  halves of a Rails `CompiledClass` assembler — the Rails-correct lift
  (`lift_model_graph`, `Language::Ruby`, no `project_odoo_fields`) AND the
  frontend-agnostic mint (`mint_graph::<P>`, already tested minting
  `openproject:WorkPackage → 0x0001_0102`). The only missing piece is a
  ~15-LOC `compile_graph_ruby` (the Python one minus the Python field
  projection). OP is the *best-shaped* consumer (71.7% chaining collapse vs
  Redmine 53.8 / Odoo 22.7).
- **dilution-collapse-sentinel → QUALIFIED.** Resonates on **design**, not yet
  on **wiring**. The phrase "OP+Redmine convergence" conflates THREE legs at
  three maturity tiers; bundling them lets the proven one launder the unbuilt
  ones. The convergence stack is a proven-but-**parallel** scaffold —
  *nothing in OP's 22-crate live request path consumes ClassView/classid/DDL
  yet*.

**Synthesis: RESONATES, do it — as a sequence of additive, offline-verifiable
bricks, never as one "convergence" blob.** The design is sound and partly
shipped; the risk is overclaiming the unbuilt legs and front-running an
unproven foundation. Keep the three legs filed separately (below).

---

## 2. The three legs (DO NOT conflate — the core dilution guard)

| Leg | State | Evidence |
|---|---|---|
| **Identity / classid** | **SHIPPED, machine-checked** | `op-canon/src/app.rs` (`OpenProjectPort` pull, `APP_PREFIX=0x0001`, `render_classid` = `0x0001_DDCC`); `lib.rs:220-249` `fork_lineage_convergence_invariants_hold` pins 26/26 Redmine concepts == OP at identical lo-u16 ids; WorkPackage==Issue==`project_work_item` 0x0102; TimeEntry==`BILLABLE_WORK_ENTRY` 0x0103 (shared with Odoo `account_analytic_line` + WoA/SMB). |
| **Structural transpile** (the AR→graph spine) | **NOT BUILT for Rails** | OP lowers via a field-copy `bridge_triples` (`op-codegen-pipeline/src/lib.rs:59-70`) + bespoke `OpSurrealProjection` — Odoo's literal PRE-Stage-B position. `compile_graph_ruby`/`CompiledClass` for Rails: **zero hits** in OP. Upstream `ogar-from-ruff` has only `compile_graph_python` (`mint.rs:69`). |
| **Render skin** (ERB→askama, the "ice caking") | **KIT exists, SKIN unwired** | `ogar-render-askama` HtmlForm/HtmlDetailView/HtmlListView/RustStruct/SurrealqlTable are real, templated, XSS-tested — **but** every emitter resolves id via `canonical_concept_id(concept)` (bare lo-u16), **never** `(APP_PREFIX<<16)|concept`. The per-app skin (hi-u16) is not threaded; askama has zero knowledge of `CompiledClass`/`Facet`/`APP_PREFIX`. It also renders CURATED codebook attribute sets, not raw Rails-lifted fields. |

---

## 3. What's SHIPPED vs MISSING (grounded, from the sweep)

**SHIPPED (lean on these):**
- Rails lift: `ogar-from-rails::extract`/`extract_app[_with]` walk `app/models`
  **+ engines** (`modules/*`, `engines/*` — OP's TimeEntry lives in
  `modules/costs`) → `Vec<ogar_vocab::Class>` via `lift_model_graph`
  (Language::Ruby). ~30 unit tests.
- `ruff_ruby_spo` Rails frontend (curator-generic, namespace-tagged so OP and
  Redmine share ONE frontend): 5 association macros + verbatim options +
  `class_name`/`association_kind` FK predicates; 5 validation macros +
  `validation_kind`/`validation_param`; 20 callback phases; include/extend/
  prepend + concern markers; 13 attribute macros + `field_type`; 3 scope forms;
  STI via cross-frontend `InheritsFrom`; module-namespace qual + cross-engine
  reopen-merge; **NEW writes/calls (ruff#38)**; rdf:type ObjectType/Property/
  Function spine + has_field/has_function edges (the ClassView skeleton);
  closed 62-predicate NARS-graded vocab.
- Mint: `mint_graph::<P>` is frontend-agnostic, **tested on OpenProjectPort**
  (`mint.rs:197-214`). Ports map 28 Rails class names each (OP 0x0001 / Redmine
  0x0007) onto shared `class_ids::*`.
- Emit: `emit_surrealql_ddl` is offline (surrealdb gated behind an unused
  parse-back feature). `emit_rust/csharp/python` exist (tested on Odoo).
- OP convergence seam present-but-parallel: `op-canon` re-exports
  `ogar_vocab::class_ids` + `OgarClassView`; `op-codegen-projection` /
  `op-surreal-ast` (typed DDL, byte-identical ToSql baseline).

**MISSING / DOC-ONLY (the work):**
- `compile_graph_ruby` (OGAR) — the keystone. ~15 LOC; reuse `lift_model_graph`.
- OP `ogar-emit` path: no dep wiring on ogar-vocab/ogar-from-ruff/
  ogar-adapter-surrealql; needs the OGAR-rev↔ruff-rev pin discipline.
- Render-classid threading into askama (the hi-u16 skin).
- Behavior-arm lift (Rails callbacks/validations → `ActionDef`/`KausalSpec`) —
  no OP analog of od-ontology `corpus_to_actions`. `RecipeConceptId` codebook
  UNMINTED (`KausalSpec::LifecycleTrigger{event:String}` + `Callback{phase}`
  still raw strings = the "zoo").
- `db/schema.rb` field harvest — `ruff_ruby_spo::extract_fields` returns
  `Vec::new()`. Real DB columns absent; ClassView attr set is declaration-
  derived only. **Largest faithfulness ceiling.**
- Concern-body transitive resolution (Acts::Customizable, Journalized push
  heavy behavior through concerns; the include edge is recorded but the
  concern's own assoc/validations/callbacks are not pulled in).
- Real-corpus OP/Redmine convergence tests are all `#[ignore]` (need
  `/home/user/openproject` + `REDMINE_SRC`); proven on synthetic Models only —
  the 71.7%/26-of-26 figures are corpus claims, unrun in CI ([S]/[H], not [G]).

---

## 4. The minimal increment — next session's first bricks (both assessors converged)

**Additive. Offline-verifiable. The ORM path is NOT touched. Do in order:**

1. **OGAR (upstream keystone, ~15 LOC + test):** add `compile_graph_ruby<P: PortSpec>(graph) -> Vec<CompiledClass>` to `crates/ogar-from-ruff/src/mint.rs` — identical to `compile_graph_python` (`mint.rs:69-84`) but calling `lift_model_graph(graph)` (`lib.rs:84`, the existing Ruby lift) instead of `lift_model_graph_python`. `mint_graph::<P>` is unchanged. Test: mint synthetic `openproject:WorkPackage` → facet classid `0x0001_0102`; `redmine:Issue` → `0x0007_0102`. Fully offline. (Optionally name it generic `compile_graph` and have `_python`/`_ruby` delegate, mirroring `lift_model_graph`'s own pair.) **This is pure operator-reuse — do NOT re-derive a Rails lift (AGENTS.md "missing-mechanism" anti-pattern); `project_odoo_fields` must NOT run for Rails (it double-counts — its own doc-comment says so).**
2. **OGAR (10-line test):** run `emit_rust/csharp/python` on a Rails `CompiledClass` in `#[cfg(test)]` (as `emit.rs` already does for OdooPort) — converts "emit_* unproven on Rails" into a green probe, zero OP changes.
3. **OP Stage-B path (additive):** add an `ogar-emit` feature to `op-codegen-pipeline/Cargo.toml` gating new path-deps `ogar-vocab` + `ogar-from-ruff` + `ogar-adapter-surrealql`, pinned to the SAME OGAR rev that `vendor/AdaWorldAPI-ruff` pins (the ModelGraph source-alignment discipline — od-ontology `Cargo.toml:30-38`). Default build unchanged (zero OGAR cost). Add under `#[cfg(feature="ogar-emit")]`:
   ```rust
   pub fn emit_surreal_via_ogar(graph: &ruff_spo_triplet::ModelGraph) -> String {
       let classes: Vec<ogar_vocab::Class> =
           ogar_from_ruff::mint::compile_graph_ruby::<OpenProjectPort>(graph)
               .into_iter().map(|cc| cc.class).collect();
       ogar_adapter_surrealql::emit_surrealql_ddl(&classes)
   }
   ```
   The byte-for-byte mirror of od-ontology `emit_source_via_ogar`. The
   `bridge_triples` path is NOT removed (coexists, like od-ontology keeps
   `emit_via_ogar` alongside `Schema::to_sql()`).
4. **OP convergence pin test** (`op-codegen-pipeline/tests/`, no new dep — it
   already path-deps ruff_openproject + ruff_spo_triplet): build the synthetic
   WorkPackage+TimeEntry graph, run `emit_surreal_via_ogar`, assert both tables
   present; then the symbol-bound pin: `compile_graph_ruby::<OpenProjectPort>`
   TimeEntry facet and `::<RedminePort>` TimeEntry facet share lo-u16
   `0x0103` and differ only in hi-u16 (`0x0001` vs `0x0007`) — the offline
   proof that the two forks collapse to one concept set with two render skins.

**The connection to the operator's correction:** this Stage-B brick proves the
OGAR substrate can produce OP's schema from the lifted AR — the structural
precondition for the open-heart op. The actual ORM excision (route op-db reads/
writes through the OGAR graph instead of hand-rolled SQL; AR domain stays) is
the LATER, gated step — never start it before the shared substrate provably
covers OP's full surface (FK `record<>`, ASSERT, UNIQUE, STI).

---

## 5. Gated / deferred (do NOT bundle into the first increment)

- **Behavior arm** (Rails callbacks/validations → ActionDef/KausalSpec): gated
  on the Rails leg of `PROBE-OGAR-AR-RECIPE-COLLAPSE` (F15) + minting the
  shared `RecipeConceptId` codebook. Blockers: confirm the **vendored**
  `ruff_openproject` populates `Model.callbacks/validations/sti` (verified in
  `/home/user/ruff`, NOT in the vendored copy); source N≥20 real Rails models
  (fixtures are smoke-sized `rails_mini`). Per handover, Rails should hit ~7%
  leftover vs Odoo's 54.3% because the AR lifecycle is first-class enumerable
  data — the highest-payoff, higher-risk half.
- **RBAC keystone** (Rails policy/permission → `lance_graph_rbac::authorize`):
  upstream CONJECTURE for both ports; gated on `PROBE-OGAR-RBAC-AUTHORIZE`. Not
  OP's lane.
- **render-classid threading + askama-for-Rails skin**: separate change against
  the askama crate; do NOT do it under cover of the DDL increment.
- **db/schema.rb harvest, concern-body transitive resolution**: additive
  ruff/ogar-from-rails work; faithfulness, not blockers for Stage-B DDL.
- **ORM excision (the open-heart swap itself)**: only after the shared emitter
  covers OP's full surface. AR domain model is KEPT throughout.

---

## 6. Dilution guards (operator's "don't dilute nor collapse")

1. **Keep AR.** The operation removes the ORM, not ActiveRecord. (The §0
   correction — the single most important line in this doc.)
2. **Three legs stay separate** — identity (SHIPPED) ≠ structural (UNBUILT) ≠
   render skin (UNWIRED). Don't let the proven identity win launder the others.
3. **Kit ≠ skin.** The askama emitters exist (mechanical 85%); the per-app
   render skin (hi-u16 threading + real Rails fields + concern flatten) is the
   missing 15%. "ERB→askama magic" overclaims if the kit is credited for the
   binding.
4. **Operator-reuse, not reimplementation.** `compile_graph_ruby` reuses
   `lift_model_graph`; never a parallel Rails lift; never run
   `project_odoo_fields` on Rails.
5. **Rev-pin discipline.** OGAR-rev ↔ ruff-rev must match (shared
   `ruff_spo_triplet::ModelGraph`), or silent type-skew (Codex-P1 class break).
6. **Additive then (eventually) subtractive.** Keep `bridge_triples` + the
   native op-surreal-ast ToSql baseline; Stage-C ORM removal is a separate,
   probe-gated decision. Substituting a "cleaner" path for a specified one is a
   P0 (q2 architectural-compliance rule).
7. **Corpus claims are [S]/[H], not [G].** The 71.7% / 26-of-26 figures are
   behind `#[ignore]` gates — proven on synthetic Models. Don't report them as
   CI-green.

---

## 7. Pointers

- Full sweep output (maps + both assessments, file:line grounded):
  `/tmp/claude-0/.../tasks/wnt1u7k2c.output` (ephemeral — this doc is the durable digest).
- Prior handover: `.claude/handovers/2026-06-30-0715-odoo-recipe-bitmask-to-openproject-rails.md`
- Coverage kit + baselines + canonical-label doctrine:
  `.claude/knowledge/RAILS-COVERAGE-KIT.md` (§5 RecipeConceptId, §6 body-pass).
- Odoo template (mirror this): `/home/user/odoo-rs/crates/od-ontology/src/ogar_bridge.rs`
  (`compile_source`/`emit_source_via_ogar`); `docs/ODOO-OGAR-MIGRATION-SPRINT.md`.
- OGAR substrate: `docs/OGAR-TRANSPILE-SUBSTRATE.md` (§1.5 compiled ClassView =
  OGAR's own #1 unbuilt "Next"; §1.6 three SDKs); `crates/ogar-from-ruff`
  (`lib.rs:84` lift, `mint.rs:69` python-only assembler); `crates/ogar-from-rails`.
- Merged this arc: ruff#38 (writes/calls), OGAR#143 (recipe-bitmask canon).
  This branch: PR#60 (coverage-kit groundwork + this handover).
