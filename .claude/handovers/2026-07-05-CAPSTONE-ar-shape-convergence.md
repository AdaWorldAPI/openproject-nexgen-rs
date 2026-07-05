# CAPSTONE — the AR-shape convergence substrate (2026-07-05)

> The single top-level plan. It **references** the sub-plans below; it does
> not restate them. New here: the 2026-07-05 rebase delta (recent ruff
> codegen + OGAR updates) and the operator **actions ruling** (actions are
> `part_of` / `is_a`).

## Thesis (one paragraph)

Rails apps (OpenProject, Redmine, and any AR consumer) converge at the
**AR/Rails object-graph shape** — `ogar_vocab::Class` (associations,
validations, callbacks, scopes, concerns, methods) — keyed by the shared
**OGAR codebook classid** (`WorkPackage` ≡ `Issue` ≡ `project_work_item`
≡ `0x0102`). **Structure and behavior both converge on the same canonical
node**; ports differ only by classid prefix + a small rename table. The
ORM/JSON serialization is **refused as identity** — the ORM is used only as
a schema/typing input and a behavior-reconstruction aid.

## The plans this capstone indexes (owns → where)

| Plan / doc | Owns |
|---|---|
| `2026-07-05-redmine-op-ar-shape-convergence-plan.md` | the Redmine⇄OP sequence (C1–C6), the measured name-preservation table, the action-translation table |
| `2026-07-02-wishlist-cross-session.md` | the cross-session items (R1–R8 ruff, L1–L4 lance, O1–O4 OGAR) + the A1–A3 collisions |
| `2026-07-02-ruff-upstream-extraction-contract.md` | the extraction contract + D-AR-3.5 column stratum (the lift-input) |
| `.claude/knowledge/TWO-SHAPES-COMPILED-NOT-PARSED.md` | keep AR shape / refuse ORM-as-identity; the stack topology |
| `.claude/knowledge/RESIDUAL-THREE-BUCKETS.md` | the residual taxonomy + zone registry (validated across Odoo lineage) |
| `.claude/knowledge/RAILS-COVERAGE-KIT.md` | canonical-label doctrine; the recipe families |
| `2026-07-02-classid-canon-high-flip.md` | the addressing (`classid = [hi: canon concept][lo: app prefix]`) |
| `2026-06-30-1200-op-redmine-ogar-convergence-assessment.md` | the original structural assessment (pre-flip ids — see its banner) |

## Delta since those plans — the 2026-07-05 rebase (recent ruff/OGAR updates)

The rebase (`7a3a75d`) moved all three upstreams; the load-bearing changes:

- **OGAR `rust_class.rs`** — *"the compile-time ERB/askama transpiler:
  ClassView × FieldMask → struct, plus the OGAR `ActionDef` DO-arm →
  a struct-of-methods constructor."* **The behavior arm is now real:** the
  render leg emits `struct { present-bit fields } + impl { new() + one fn
  per ActionDef }`. This is the correct masked-classview render (the leg I
  built-and-reverted three times, now upstream and correct).
- **ruff `ruff_ruby_spo`** — gains `extract_tree_with`; `functions.rs`
  +278 (body-fact extraction depth, feeds F17 / R6).
- **ruff `ruff_spo_triplet`** — now **emits `Predicate::InheritsFrom`** —
  the **`is_a`** edge (STI / `subClassOf`) is live in the triple stream.
- **lance-graph-contract** — `network.rs` (the ruff→OGAR harvest **sink**
  onto the V3 SoA), `unicharcompress.rs`; `ActionDef` in `action.rs:79`.
- Vendor: new **D4** deviation (render-askama → lance-graph-contract path
  redirect); the sync tool's report now uses **git truth** (it had lied
  "0 changed" on a 16-file sweep).

## Operator ruling (2026-07-05) — actions are `part_of` / `is_a`

**`actions ⊂ part_of / is_a::part_of::is_a::(input)`** — actions are lifted
into the same ontology rails as concepts, not a side table:

- **action `part_of` class** — an `ActionDef` belongs to its class; in
  `rust_class.rs` it is literally attached to the class's `impl` block
  (the DO-arm). The action lives *on* `0x0102`, not beside it.
- **action `is_a` canonical verb** — an action resolves to a canonical
  action concept in the codebook (the same convergence mechanism as class
  concepts). `Issue#assigned_to=` and `WorkPackage#assigned_to=` are the
  same action because they share `(classid, is_a canonical-verb)`.
- **input `is_a` typed input** — the action's input is typed by an `is_a`
  edge (now emittable — `InheritsFrom` is live). The input's type is an
  ontology node, not a serialized shape.

This **upgrades** the convergence plan's C5 (a flat `redmine ⇄ op` action
table) into an **ontology-rail model**: actions converge exactly like
concepts — same `is_a` canonical node, port-specific skins, drift captured
by the rename table (§3 of the convergence plan: `tracker→type`,
`fixed_version→version`; identity otherwise).

## Sequenced capstone view (what the rebase unblocked)

| # | Step | Status after rebase | Owner |
|---|---|---|---|
| C1 | lift both apps → `Vec<Class>` (`compile_graph_ruby`) | pending (wishlist **O3**) | upstream ogar-from-ruff |
| C2 | type AR fields from schema stratum | ready (**R1** patch; not yet merged upstream) | ruff |
| C3 | structural convergence (26/26, `0x0102`) | **asserted** in op-canon ‖ redmine-canon | here + sibling |
| C4 | name-preservation **rename table** (assoc+attr+method) | measurable now (WorkPackage row done) | **here** |
| C5 | **action ontology** — `(classid, is_a verb) ⇄ {redmine, op}` | **unblocked**: `ActionDef` + `InheritsFrom` + `rust_class` DO-arm now exist | cross (F17/R6 source + C4 table) |
| C6 | behavior runs off the **canonical** ActionDef | render leg exists (`rust_class`); one def per classid | upstream OGAR ActionDef |

## Boundaries / guardrails

- Behavior/actions live on the **canonical** `ActionDef`; ports are skins.
  No per-port behavior code.
- ORM/JSON is **membrane-only** — schema-typing input + translation aid,
  never internal truth.
- **`[H]`**: the action convergence table (C5) must be **measured** by an
  A/B method-diff between the two ports before it is trusted — same
  falsifier discipline as the WorkPackage oracle diff. Do not ship claimed
  coverage that hasn't been measured.
- This repo (op-nexgen) owns **C4** (the rename table) and **consuming**
  `rust_class`; the lift (C1/O3), the codebook, and `ActionDef` are
  upstream; `redmine-canon` is the sibling.
