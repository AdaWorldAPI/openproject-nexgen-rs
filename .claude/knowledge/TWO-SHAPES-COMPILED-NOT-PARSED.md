# Two shapes, one identity — and compiled, not parsed

> Operator decisions (2026-07-02, this session), pinned as canon. Companions:
> `RESIDUAL-THREE-BUCKETS.md` (buckets/zones), `RAILS-COVERAGE-KIT.md`
> (recipe doctrine), handover `2026-07-02-ruff-upstream-extraction-contract.md`.

## 1. The stack (who serves whom)

```
consumers (openproject-nexgen, redmine-rs, odoo-rs, …)
    │  each depends ONLY on lance-graph-contract     ("a domain at the
    ▼                                                  cost of an import")
lance-graph-contract ── lance-graph-ogar (plug-and-play bridge, imports OGAR)
    ▼
lance-graph  — storage + thinking: SoA/columnar, planner, semirings, compute DAG
    ▲
OGAR — the transpiler SINK: consumer domains land here as AST substrate and
    feed lance-graph. Also the OGIT-like ontology cache as a DTO layer —
    what makes the Odoo vocabulary an *agnostic ERP* vocabulary. API: Rust
    first; Python later IFF the surface is tailored agnostically enough.
    ▲
ndarray — SIMD hardware-acceleration substrate.
```

**Migration direction:** the *static compile surface* (nexgen's
`op-codegen-*` emission layer) is transitional and migrates OGAR-side. The
landing pads already exist upstream: `ogar-from-ruff`, `ogar-from-rails`,
`ogar-from-schema` (the eventual home of the D-AR-3.5 column stratum).

## 2. The two-shapes doctrine

**We keep ActiveRecord — but the Ruby shape is the identity; the ORM shape
is only a bridge.**

- **Ruby/AR shape = the wings.** The class-body declarative AST —
  associations, validations, callbacks, scopes, concerns, acts_as, STI —
  is the canonical identity. It lands in OGAR as the rails-shaped compile
  substrate AST, and in lance-graph as SoA for AR-shaped data. Flattening
  a model to its columns cuts the wings off the AR AST-DLL shape.
- **ORM/column shape = the bridge.** Physical columns (type + nullability,
  the migration-DSL stratum) exist to resolve the **90+10%** — they *type*
  the AR shape, they never *are* the model. If 100% transcoding needs the
  ORM shape as a bridge, that's an accepted necessity; wherever
  rails-shape + the ERB pattern carry it early, skip the bridge and skip
  the technical debt.

**In code (2026-07-02, commit `2b4c42c`):** the typed pipeline realizes the
split — `belongs_to` owns the field's *kind* (`record<Target>`), the column
stratum supplies *nullability* (bare vs `option<…>`), and the whole AR
recipe surface rides `DEFINE TABLE … COMMENT` annotations, not columns.
`from_triples.rs` has the AR-wins upgrade path (a schema `option<int>` FK
promotes to `record<Target>` when the association names it) — the doctrine
is executable, not aspirational. Measured: 209 fields, 85% typed, wings
intact.

## 3. Compiled, not parsed

**The surrealdb AST adapter is replaced by OGAR so output is *compiled*,
not *parsed*.**

- Today (parse-debt): IR → `op-surreal-ast` (hand-mirrored AST) → SurrealQL
  **text** → surrealdb re-parses its own dialect at load time.
- Target: OGAR compiles the ontology/DTO layer **directly into
  `surrealdb-ast` values** (the AdaWorldAPI/surrealdb fork's real AST
  types — already OGAR workspace deps alongside `surrealdb-core`) and
  hands them over with no text round-trip and no parser in the loop.
- Consequence: `op-surreal-ast` is **transitional by direction** (kept for
  its round-trip contract and as today's typed emitter), and every new
  investment in text emission is scrutinized — the compiled path is where
  effort compounds.

## 4. The global render pattern

`ogar-class-view` + `ogar-render-askama` (artifact kinds + templates) + the
recipe **bitmask** apply globally to every consumer — the knowledge
transfer from the **Redmine ERB row/view/filter pattern**. One render/filter
surface, per-consumer skins via LabelDtos, never per-consumer re-renderers.
(Memo tracked as its own task; this section pins the decision.)

## 5. What this changes in practice

1. Column-stratum work stays subordinate: emitted as facts about fields
   (`field_type`, `column_not_null`), consumed to type the AR shape —
   never to replace `declares_association`/recipe predicates.
2. New emission targets prefer AST-value construction (compiled) over
   string rendering (parsed) — starting with the OGAR `surrealdb-ast`
   path when the compile surface migrates.
3. nexgen's `op-codegen-*` crates carry a sunset direction: keep them
   green, keep their contracts, but grow new capability OGAR-side where
   possible (`ogar-from-schema` for columns, `ogar-from-rails` for the AR
   recipe, `lance-graph-ogar` for the import bridge).
4. The Python surface question is an API-design constraint TODAY: every
   OGAR-facing interface we shape should avoid Rust-only idioms in its
   contract (plain data in/out, ids + label DTOs) so the later Python
   binding is a binding, not a rewrite.
