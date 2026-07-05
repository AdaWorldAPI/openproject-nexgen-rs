# Redmine ⇄ OpenProject convergence at the AR/Rails/Ruby shape — integration plan

> Operator ask (2026-07-05): converge Redmine and openproject-nexgen-rs at
> the **AR/Rails/Ruby shape** — NOT the ORM/JSON serialization shape. The
> ORM is allowed as a *schema/typing input* and as a *translation aid for
> behavior reconstruction* (an action table), but never as the identity or
> the internal wire format.
>
> Companion (structural facts, pre-flip classid order — see its banner):
> `2026-06-30-1200-op-redmine-ogar-convergence-assessment.md`.
> This doc is the sequenced plan the assessment lacked.

## 0. The one-line thesis (already half-true in code)

Redmine → ChiliProject → OpenProject is a **fork lineage**, so the two
apps are genetically the same object graph with drift. Convergence = lift
**both** to the same AR shape (`ogar_vocab::Class`), keyed by the **same
OGAR codebook classid** — then `WorkPackage` and `Issue` are one canonical
node (`project_work_item`, `0x0102`) with two curator skins. `op-canon`
already asserts this structurally: *"26 of 26 canonical concepts the
Redmine corpus contributes are also contributed by OpenProject, with
identical ids"* (`crates/op-canon/src/lib.rs`). `redmine-canon` is its
sibling in `AdaWorldAPI/redmine-rs`. The **structural** arm is largely
done; the **behavioral** arm (actions) is the real work.

## 1. What is the "AR shape" we converge on (and what we refuse)

- **KEEP — the identity:** the class-body declarative AST, carried by
  `ogar_vocab::Class`: `associations`, `validations`, `callbacks`,
  `scopes`, `concerns`/`mixins`, `enums`, `store_accessors`, `methods`,
  `computed_fields`, `sti`/`inheritance`. This is the Rails object graph.
- **ALLOWED as input only — the ORM/schema:** the migration-DSL column
  stratum (D-AR-3.5, `ruff_ruby_spo/src/schema.rs`) — `field_type` +
  `column_not_null`. It *types* the AR fields; it is not the identity.
- **REFUSED — internal truth:** ORM row objects, JSON serialization as the
  wire/internal format, DDL-text-as-interface. JSON survives only at the
  client membrane, never as internal truth. (This is the two-shapes
  doctrine; the surrealdb/DDL rendering leg is deprioritized.)

## 2. The pipeline (one frontend, one lift, two namespaces)

```text
AdaWorldAPI/openproject  ──┐  ruff_ruby_spo::extract_app_with(path, "openproject")
AdaWorldAPI/redmine      ──┘  ruff_ruby_spo::extract_app_with(path, "redmine")
        │                              (SAME frontend — Redmine & OP are both Rails;
        ▼                               the fork preserved most names, see §3)
   ruff_spo_triplet::ModelGraph   (one per app, tagged by namespace)
        │  ogar_from_ruff::lift_model_graph          ← UPSTREAM (wishlist O3:
        ▼                                              compile_graph_ruby, ~15 LOC)
   Vec<ogar_vocab::Class>  — canonical_concept + classid (the CODEBOOK convergence)
        │                              WorkPackage → 0x0102 ← → Issue → 0x0102
        ▼
   op-canon (OP snapshot)  ‖  redmine-canon (Redmine snapshot)  — same ids
```

The convergence *happens* at the codebook: both classes resolve to the
same `classid`. Nothing new to invent structurally — it's O3 (the Ruby
lift) + the codebook, both upstream/sibling.

## 3. Name preservation — MEASURED, not assumed (the fork kept the variables)

The operator's premise ("variables kept their names") is the lever for the
action table. Measured `Issue` (Redmine) vs `WorkPackage` (OP) associations:

| Redmine `Issue` | OP `WorkPackage` | status |
|---|---|---|
| `assigned_to`, `author`, `category`, `priority`, `project`, `status`, `time_entries` | identical | **preserved (7)** |
| `tracker` | `type` | **renamed** |
| `fixed_version` | `version` | **renamed** |

→ The rename set is **small and enumerable**. Most of the AR surface maps
by **identity** (free); a short explicit table covers the drift. This is
what makes the behavior arm tractable: the same variable names in Redmine's
ERB (`issue.assigned_to`, `issue.status`) are recognizable in OP.

## 4. The behavior arm — an action translation table (the real deliverable)

Actions (controller verbs, callbacks, state transitions) are the part the
structural convergence does NOT give for free. Plan:

- **Source of actions:** F17 body triage (wishlist R6 — `writes_field` /
  `calls` are live upstream) recovers `(verb, criteria)` per hook body;
  the Redmine ERB harvest (the render kit already lifted Redmine's
  `Query`/`column_value` model — `ogar-render-askama`, and now
  `rust_class.rs` = ClassView×FieldMask→struct) recovers the view/action
  surface.
- **The table shape** — `redmine_action ⇄ canonical_action ⇄ op_action`,
  keyed by the classid + the (mostly-preserved) method/variable name:
  ```
  (0x0102, "assign")   redmine: Issue#assigned_to=  ≡  op: WorkPackage#assigned_to=   [identity]
  (0x0102, "set_type") redmine: Issue#tracker=       ≡  op: WorkPackage#type=          [rename tracker→type]
  (0x0102, "log_time") redmine: Issue#time_entries   ≡  op: WorkPackage#time_entries   [identity]
  ```
  Most rows are identity (recognizable because the names survived §3); the
  renamed rows are the enumerable exceptions from the §3 table, extended to
  methods/actions.
- **Recognition heuristic `[H]`:** an action is "the same" across ports
  when it shares (classid, canonical method-name) after applying the rename
  table. Presumed high-hit because of §3; MUST be measured (an A/B diff of
  the two ports' extracted method sets, the same falsifier shape as the
  WorkPackage oracle diff) before the table is trusted. Do not assume 100%.

## 5. Sequenced steps (who owns what)

| # | Step | Owner | Gate |
|---|---|---|---|
| C1 | Lift both apps to `Vec<Class>` (`compile_graph_ruby`) | **upstream** ogar-from-ruff (wishlist O3) | OP `WorkPackage`→`0x0102_0001`, Redmine `Issue`→`0x0102_0007` (flipped order) |
| C2 | Type the AR fields from the schema stratum | D-AR-3.5 (wishlist R1) | column facts merged into the Class; ORM used for *schema only* |
| C3 | Confirm structural convergence | op-canon ‖ redmine-canon (here + sibling) | already asserted 26/26; pin with a shared test |
| C4 | Build the **name-preservation rename table** (assoc + attr + method) | **here** (consumer domain knowledge) — measurable now from both Rails sources | the §3 table, extended to methods; the small enumerable drift set |
| C5 | Build the **action translation table** `(classid, canonical_action) ⇄ {redmine, op}` | cross: F17 (R6) source + the rename table (C4) | A/B method-diff falsifier ≥ threshold before trust |
| C6 | Behavior reconstruction runs off the canonical action (not per-port) | upstream (OGAR ActionDef / DO-arm) | one action def per classid; ports are skins |

## 6. What is genuinely THIS repo's (op-nexgen) to do

Only **C4** — the rename/name-preservation table — is squarely consumer
domain knowledge and buildable here today (both Rails sources are readable;
§3 already has the WorkPackage row). Everything else is upstream
(ogar-from-ruff lift, OGAR codebook/ActionDef) or the sibling
(redmine-canon). So this plan is a **cross-session plan**, not a
this-repo build — same as the wishlist. The one local, verifiable
contribution is the measured rename table (C4), which the action table (C5)
consumes.

## 7. Non-goals / guardrails

- No ORM row objects or JSON as internal truth (membrane-only).
- No per-port behavior code — behavior lives on the **canonical** action,
  ports differ by classid prefix + the rename table.
- The action-table completeness is `[H]` until the A/B method-diff measures
  it; do not ship a table that claims coverage it hasn't proven (the
  conservation-ledger discipline).
