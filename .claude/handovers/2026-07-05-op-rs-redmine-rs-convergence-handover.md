# OpenProject-rs ⇄ Redmine-rs convergence — handover (2026-07-05)

> **What this is.** The **consumer-side** convergence handover: how
> `openproject-nexgen-rs` (in scope) and `redmine-rs` (sibling — `AdaWorldAPI/redmine-rs`,
> **NOT in this session's default scope**) land on the same AR-shape endgame,
> keyed by the shared OGAR codebook. Pairs with the OGAR-side handover
> `OGAR/.claude/handovers/2026-07-05-recipe-integration-phase-2-handover.md`
> (the verb-codebook wiring). Copy-paste **session kickoff prompt** is §10.

---

## 0. Mandatory reads (in order)

**op-nexgen (this repo):**
1. `.claude/handovers/2026-07-05-redmine-op-ar-shape-convergence-plan.md` — the C1–C6 sequence, the measured rename table, the action-table shape. **This handover extends it; that doc is the spine.**
2. `.claude/handovers/2026-07-05-CAPSTONE-ar-shape-convergence.md` — the thesis + the C5 action-ontology ruling (actions are `part_of`/`is_a`).
3. `.claude/knowledge/RAILS-COVERAGE-KIT.md` (§0 STI collapse, §5 the recipe families, §6 F17 body triage) + `TWO-SHAPES-COMPILED-NOT-PARSED.md` (§2 keep-AR / ORM-as-bridge).
4. `.claude/board/EPIPHANIES.md` — the recipe arc (`E-RECIPE-REUNION-ORDER`, `E-GRAMMAR-IS-THE-RECIPE-SHAPE`, `E-F17-PREREQ-VERIFIED`, the recipe-codebook-P1 mirror).

**OGAR (sibling — path `../OGAR` or the git dep):**
5. `.claude/handovers/2026-07-05-recipe-integration-phase-2-handover.md` — the verb codebook (`ogar_vocab::recipe`) this consumer work adopts.
6. `docs/OGAR-CONSUMER-BEST-PRACTICES.md` — MANDATORY before any consumer call site touching `class_id`/`PortSpec`/`ClassView`. `docs/CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK.md` for the render leg.

**redmine-rs (sibling — must be added):** it is **not** in the default 20-repo scope. A future session working it runs `add_repo("AdaWorldAPI","redmine-rs")` (default branch `main`), or the operator grants access. Read `redmine-canon` (its `class_ids` re-export) before asserting any convergence.

---

## 1. The thesis (one paragraph)

The reunion is an **operator ORDER** (`E-RECIPE-REUNION-ORDER`), not a
conjecture. Redmine → ChiliProject → OpenProject is a fork lineage, so the
two apps are the **same object graph with drift**. Convergence = lift both
to `ogar_vocab::Class` at the **AR/Rails/Ruby shape** (associations =
`part_of`, STI = `is_a`), keyed by the **same OGAR codebook classid**. Then
`WorkPackage` and `Issue` are one canonical node (`project_work_item`,
`0x0102`) with two curator skins. **ORM is used only for schema-typing +
action reconstruction; never as identity.** Nouns already converge
(structural); **verbs are the remaining work** (the C5 action table) — and
the verb codebook they converge on now exists (`ogar_vocab::recipe`,
Phase 1).

---

## 2. What is already SHIPPED (do not re-derive)

- **Noun convergence, pinned in code:** `ogar_vocab::ports` —
  `OpenProjectPort` / `RedminePort` map their public names onto the **same**
  `class_ids::*` (28 aliases each). `openproject_and_redmine_converge_on_shared_concepts`
  machine-checks 26 pairs (`WorkPackage↔Issue`, `Status↔IssueStatus`,
  `Type↔Tracker`, …). **Status: asserted, NOT measured** (capstone C3) — do
  not cite it as a measured precedent.
- **`op-canon`** (this repo) = the OP codebook snapshot; **`redmine-canon`**
  (sibling) is its twin — both re-export `ogar_vocab::class_ids`, so the
  values cannot drift.
- **The verb codebook (`ogar_vocab::recipe`)** — Phase 1: the four recipe
  families as `RecipeConceptId`, + `recipe_concept_from_surface(surface, lang)`
  (the lift resolver). The Rails↔Odoo relation convergence is machine-checked;
  the Ruby surfaces (callbacks/validations/associations/HandlerKinds) are
  seeded. **This is what C5 converges on.**
- **The measured rename table (WorkPackage row)** — §3 of the redmine-op plan:
  7 of 9 `Issue`↔`WorkPackage` associations are **identity**; `tracker→type`
  and `fixed_version→version` are the only drift.

---

## 3. The sequence C1–C6 (owners + status, updated for the recipe codebook)

| # | Step | Owner | Status |
|---|---|---|---|
| C1 | lift both apps → `Vec<Class>` (`compile_graph_ruby`) | upstream `ogar-from-ruff` | ready (wishlist O3) |
| C2 | type AR fields from the schema stratum (D-AR-3.5) | ruff (R1) + `ogar-from-ruff` `project_rails_fields` | **shipped** (Rails-field projection landed) |
| C3 | structural convergence (26/26, `0x0102`) | `op-canon` ‖ `redmine-canon` | **asserted** (pin exists; not oracle-measured) |
| **C4** | **name-preservation rename table** (assoc + attr + **method**) | **op-nexgen (HERE)** | WorkPackage row done; extend to methods → §4 |
| **C5** | **action table** `(classid, RecipeConceptId) ⇄ {redmine, op}` | cross (F17/R6 source + C4 + `ogar_vocab::recipe`) | **unblocked** by the recipe codebook → §5 |
| C6 | behaviour runs off the **canonical** ActionDef | upstream OGAR `ActionDef` | render leg exists (`rust_class`) |

---

## 4. C4 — the rename table (op-nexgen owns; buildable NOW, no upstream gate)

The one squarely-consumer-domain deliverable. Extend §3's association table
to **attributes + methods**, measured from both Rails sources (both are
readable: OpenProject via the corpus snapshot; Redmine via `redmine-rs`
once added). Shape: `redmine_name ⇄ canonical ⇄ op_name`, keyed by
`(classid, name-after-rename)`. Most rows are **identity** (the fork
preserved variable names — that IS the lever); the drift set is small and
enumerable (`tracker→type`, `fixed_version→version`, extended to methods).
Deliverable: a typed table in `op-canon` (or a sibling module) + a test
pinning the identity/drift split. **Measure; do not assume 100%.**

---

## 5. C5 — the action A/B (the measurement gate; now UNBLOCKED)

The verb-side convergence measurement — the twin of the noun-side pin, and
the falsifier `E-RECIPE-REUNION-ORDER` pre-registered. Now doable because
`ogar_vocab::recipe` exists:

1. Harvest **both** ports' controller actions (`ruff_ruby_spo::extract_tree_with`
   over `app/controllers`, public actions only — already live, ruff #42/#43)
   and their AR recipes (callbacks/validations/associations).
2. Resolve each surface to a `RecipeConceptId` via
   `recipe_concept_from_surface(surface, RecipeLang::Ruby)`.
3. **Measure the shared-`RecipeConceptId` collapse rate** between OP and
   Redmine: numerator = actions/recipes whose id also appears in the other
   port's set (after the C4 rename table); denominator = one port's full set.
4. **Pre-register the KILL threshold (collapse-rate %) BEFORE the run.** The
   noun side (26/26) is *asserted*, not measured, so the verb side may not
   borrow it as a measured precedent. This is a DISTINCT measurement from the
   noun convergence and does not stand in for it.

Gate honesty: the F17 body-triage falsifier (RAILS-COVERAGE-KIT §6) measures
the accidentally-imperative vs foreign split of hook *bodies*; C5 measures
the *predicate* convergence. Both are coverage measurements of a canonized
convergence — don't ship claimed coverage unmeasured.

---

## 6. Consumer adoption of the recipe codebook (the "zoo → codebook" payoff)

`op-codegen-bucket::OpHandlerKind` (6 kinds) is the per-consumer enum
`RAILS-COVERAGE-KIT §5` names as the zoo. Adoption = map each `OpHandlerKind`
→ its `recipe_ids::ACTION_*` `RecipeConceptId` (a `From`/`const` table + a
test), so OP and Redmine dispatch on the shared verb id, not a private enum.
Redmine-rs does the mirror (its handler set → the same `ACTION_*`). **This is
GATED on §7** — the recipe codebook must be reachable first.

---

## 7. ⚠ The blocker only the operator resolves — branch consumability

`openproject-nexgen-rs` deps OGAR via **`branch = "claude/odoo-rs-transcode-lf8ya5"`**
(the convergence branch) — check `crates/op-canon/Cargo.toml`,
`crates/op-codegen-*/Cargo.toml`. **All the recipe work is on
`claude/openproject-nexgen-ogar-review-mkjtpq`** (OGAR PR #157). So
`ogar_vocab::recipe` is **NOT reachable** by op-nexgen or redmine-rs until it
lands on the convergence branch **or** the dep is repointed. That is a
branch/merge-strategy decision for the **operator** — do NOT unilaterally
repoint. C4 (§4) needs no dep and is doable today; C5's *resolution step*
(§5.2) and §6 adoption are gated on this.

---

## 8. Redmine-rs sibling coordination (lockstep, shared codebook)

- Redmine-rs is the **structural template's twin** — same `PortSpec` pattern,
  same `ogar_vocab::class_ids` re-export via `redmine-canon`. The two repos
  **must not fork the codebook**: a concept is minted once in OGAR
  (`ogar-vocab`), both ports alias it.
- The C5 A/B needs **both** ports' harvested surfaces — a future session must
  have `redmine-rs` added (§0) to run it; op-side alone can only prepare the
  OP half of the table.
- Do not push to `redmine-rs` from an op-nexgen session; treat it read-mostly,
  coordinate the shared-codebook mints upstream in OGAR.

---

## 9. Hard lessons from this session — do NOT repeat

1. **Read the plans before acting.** A 5+3 council was convened this session
   and *wrongly rejected* the route-dedup↔SoC unification the operator had
   canonized — because it was never pointed at the rulings (`CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK`,
   `RAILS-COVERAGE-KIT §5`). The reunion is an ORDER; execute it, don't
   re-litigate. See `E-ROUTE-KIND-VERB-STRATA` (SUPERSEDED) for the cautionary
   record.
2. **Verify surface strings in code before matching them.** The recipe
   codebook's Odoo relation labels were first written capitalized
   (`Many2one`); ruff emits them **lowercased** (`many2one`). Grep the
   producer (`ruff_python_spo`, `ruff_spo_triplet::ir`) for the exact token
   before writing a resolver key. (Ruby DSL surfaces are verbatim; Odoo
   constructor names are lowercased at harvest.)
3. **Measure before claiming coverage** — C3 is *asserted*, not measured;
   C4/C5 must be measured with a pre-registered threshold.
4. **Board is append-only** (regrade Status in place, prepend corrections);
   scope cargo to `-p <crate>`, never `--all`.

---

## 10. SESSION KICKOFF PROMPT (copy-paste)

```
You are executing the OpenProject-rs ⇄ Redmine-rs AR-shape convergence
(consumer side). The reunion is an OPERATOR ORDER, not a conjecture — execute
it, do NOT run a council to "test" it.

FIRST read (mandatory):
- openproject-nexgen-rs/.claude/handovers/2026-07-05-op-rs-redmine-rs-convergence-handover.md  (THIS plan; §3 sequence, §4 C4, §5 C5)
- .../2026-07-05-redmine-op-ar-shape-convergence-plan.md  (the spine)
- .claude/knowledge/RAILS-COVERAGE-KIT.md (§0/§5/§6) + TWO-SHAPES-COMPILED-NOT-PARSED.md §2
- OGAR docs/OGAR-CONSUMER-BEST-PRACTICES.md + the recipe Phase 2 handover
- .claude/board/EPIPHANIES.md — the recipe arc entries (they are operator canon)

If Redmine work is in scope: add the sibling — add_repo("AdaWorldAPI","redmine-rs")
(default branch main) — and read redmine-canon. Do NOT push to redmine-rs.

DO (no upstream/branch gate):
- C4 (§4): build the measured name-preservation rename table (assoc + attr +
  method) in op-canon, keyed by (classid, name-after-rename); test the
  identity/drift split (tracker→type, fixed_version→version). Measure from both
  Rails sources; do not assume 100%.

DO (once the recipe codebook is reachable — see §7 branch gate):
- C5 (§5): harvest both ports' actions+recipes, resolve via
  ogar_vocab::recipe::recipe_concept_from_surface(..., RecipeLang::Ruby),
  measure the shared-RecipeConceptId collapse rate; PRE-REGISTER the KILL % first.
- §6: map op-codegen-bucket::OpHandlerKind → recipe_ids::ACTION_* (retire the
  per-consumer enum); redmine-rs mirrors.

FLAG, don't resolve: op-nexgen deps OGAR via claude/odoo-rs-transcode-lf8ya5,
so ogar_vocab::recipe isn't reachable until the operator settles the branch
strategy (§7). C4 is doable regardless.

Verify surface tokens in the producer (ruff) before matching them — Odoo
constructor names are lowercased at harvest. Scope cargo to -p <crate>; never
--all. Board append-only. Commit small + push; no PR unless asked.
```

---

## Appendix — the convergence receipts (where the pins live)

- `ogar_vocab::ports::tests::openproject_and_redmine_converge_on_shared_concepts` — the 26-pair noun pin.
- `ogar_vocab::recipe::tests::relation_verbs_converge_across_ruby_and_python` — the verb-side pin (`belongs_to ≡ many2one`).
- redmine-op plan §3 — the measured association rename table (WorkPackage row).
- `op-codegen-pipeline::ogar_consumer::tests::openproject_and_redmine_converge_on_shared_concepts` — the consumer-path convergence pin.
