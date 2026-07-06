# OP-rs ⇄ Redmine-rs convergence — consumer-side handover (2026-07-05)

> **What this is.** The consumer-side companion to the OGAR recipe handover
> (`OGAR/.claude/handovers/2026-07-05-recipe-integration-phase-2-handover.md`).
> That doc drives the **upstream** verb-codebook wiring (Phase 2 in
> `ogar-from-ruff`); THIS doc drives how **openproject-nexgen-rs and
> redmine-rs land on the same AR-shape endgame** — the rename table, the
> action A/B measurement, and eventual recipe-codebook adoption. A fresh
> session should be able to execute from here without re-deriving context.
> The copy-paste kickoff prompt is §10.

---

## 0. Mandatory reads (in this order — do NOT skip)

1. **This repo's plans** —
   `.claude/handovers/2026-07-05-redmine-op-ar-shape-convergence-plan.md`
   (the sequenced C1–C6 plan; §5 is the ownership table) and its companion
   `.claude/handovers/2026-06-30-1200-op-redmine-ogar-convergence-assessment.md`
   (structural facts; **pre-flip classid order** — read its banner).
2. **The upstream twin** — OGAR
   `.claude/handovers/2026-07-05-recipe-integration-phase-2-handover.md`
   (Phase 1 shipped; Phase 2 spec; the branch blocker in its §6 = our §7).
3. **OGAR consumer doctrine** — `OGAR/docs/OGAR-CONSUMER-BEST-PRACTICES.md`
   + lance-graph `.claude/knowledge/ogar-consumer-preflight.md` (never
   construct a `*Bridge`, never copy the codebook; pull via port/codebook
   fns). Classid order is **canon HIGH / custom LOW** since 2026-07-02.
4. **The kit** — `.claude/knowledge/RAILS-COVERAGE-KIT.md` §5 (the four
   recipe families + `RecipeConceptId` + no-zoo doctrine that Phase 1
   implements).
5. **How to reach the sibling.** `redmine-rs` = `AdaWorldAPI/redmine-rs`
   (public, default branch `main`). It is **not** in a fresh session's
   repo scope by default — add it via the `add_repo` tool (owner
   `adaworldapi`, repo `redmine-rs`), then read its `redmine-canon` crate
   **before asserting anything about convergence state on the Redmine
   side**. From this repo, redmine-rs is read-only: coordinate via
   handovers, never push there without an explicit ask.

---

## 1. The one-line thesis (operator order, not a proposal)

Redmine → ChiliProject → OpenProject is a **fork lineage**; the reunion is
an operator ORDER (`E-RECIPE-REUNION-ORDER`, OGAR EPIPHANIES): lift both
apps to the same AR shape (`ogar_vocab::Class`), keyed by the same OGAR
codebook classid, so `WorkPackage ≡ Issue ≡ 0x0102` (`project_work_item`)
— one canonical node, two curator skins. The ORM is allowed as a
*schema/typing input* and an *action-table translation aid*, never as
identity or wire format. Do NOT re-litigate this (a mis-framed council
already rejected settled canon once — see §9.1).

## 2. What is already shipped (verify in code, don't trust prose)

- **Noun side (structural):** `op-canon` asserts *"26 of 26 canonical
  concepts the Redmine corpus contributes are also contributed by
  OpenProject, with identical ids"* (`crates/op-canon/src/lib.rs`).
  **Asserted, not measured** — the sibling `redmine-canon` snapshot pins
  the Redmine half; C3's shared test is what makes it a pin.
- **Verb side (behavioral):** `ogar_vocab::recipe` Phase 1 is SHIPPED
  (OGAR branch `claude/openproject-nexgen-ogar-review-mkjtpq`):
  `RecipeConceptId(u16)` newtype, `RecipeFamily`
  (Lifecycle/Guard/Relation/Action; Scope/Concern reserved-unminted,
  mint-on-emit), 27 concepts in `recipe_ids::*`, and
  **`recipe_concept_from_surface(surface, lang)`** — the lift-time
  resolver, with the machine-checked convergence pin
  `belongs_to ≡ Many2one → REL_MANY_TO_ONE`.
- **Gap ledger (code-verified):** (a) `writes_field`/`calls` capture —
  CLOSED upstream in `ruff_spo_triplet`; (b) `routes.rb` stratum — OPEN;
  (c) recipe codebook — HALF-CLOSED (codebook shipped, lift wiring =
  OGAR Phase 2).

## 3. The sequence — C1–C6, owners + current status

| # | Step | Owner | Status |
|---|---|---|---|
| C1 | Lift both apps to `Vec<Class>` (`compile_graph_ruby`) | upstream ogar-from-ruff (wishlist O3) | open |
| C2 | Type AR fields from the schema stratum (ORM as schema ONLY) | upstream D-AR-3.5 (wishlist R1) | open |
| C3 | Pin structural convergence with a **shared test** (26/26 today is asserted) | here + redmine-rs | open |
| C4 | **Name-preservation rename table** (assoc + attr + method) | **here — buildable NOW, no gate** | open (§4) |
| C5 | **Action translation table** `(classid, canonical_action) ⇄ {redmine, op}` | cross — F17 source + C4 + the recipe codebook | **unblocked** by Phase 1 (§5) |
| C6 | Behavior reconstruction off the canonical action (ports are skins) | upstream (OGAR ActionDef / DO-arm) | gated on C5 |

## 4. C4 — the rename table (this repo's do-it-now work)

The only step squarely in op-nexgen's consumer domain and **buildable
today with zero upstream/branch gate**: both Rails sources are readable;
the plan's §3 already measured the `Issue`/`WorkPackage` association row
(7 preserved identically; `tracker→type`, `fixed_version→version`
renamed). Extend that measurement to **attributes and methods**, producing
the small enumerable drift set the action table consumes. Deliverable: a
data table (in-crate const or committed data file) + the measurement
script/notes, so the "small and enumerable" claim is reproducible, not
vibes.

## 5. C5 — the action A/B (unblocked by the verb codebook)

The recipe codebook turns C5 from "invent a table" into "measure a
collapse": harvest both ports' action/route surfaces, resolve each through
`recipe_concept_from_surface` (+ the C4 rename table for the drift rows),
and measure the **shared-`RecipeConceptId` collapse rate** across ports.

- **Pre-register the KILL threshold (collapse-rate %) BEFORE running.**
  The noun side's 26/26 is *asserted*, not measured — the verb side may
  not borrow it as precedent. This A/B is a distinct falsifier.
- Requires OGAR Phase 2 (ids stamped at lift) for the clean version;
  a surface-string prototype via the resolver alone is possible earlier.

## 6. Consumer adoption — retiring the per-consumer verb enum

Once reachable (§7), op-nexgen maps `OpHandlerKind → recipe_ids::ACTION_*`
and retires the local enum as identity (it may survive as a label skin).
Same move lands in redmine-rs against its handler kinds. Per consumer
doctrine: **pull** ids from `ogar_vocab::recipe`; never copy the codebook
into a consumer, never wrap it in a bridge.

## 7. ⚠ The branch blocker (operator decision — flag, don't resolve)

All recipe work lives on OGAR branch
`claude/openproject-nexgen-ogar-review-mkjtpq`, but consumers (this repo
included) dep OGAR via **`branch = "claude/odoo-rs-transcode-lf8ya5"`**.
`ogar_vocab::recipe` is therefore unreachable by any consumer until the
work lands on the convergence branch OR the dep is repointed. That is the
**operator's** branch/merge-strategy call — do not unilaterally repoint.
C4 (§4) and the OGAR-side Phase 2 are fully doable regardless; §6 is what
waits.

## 8. redmine-rs coordination

- Lockstep, shared codebook: `redmine-canon` is the sibling of `op-canon`;
  both must key off the SAME `class_ids`/`recipe_ids` (no per-repo copies).
- C3's shared test and C5's A/B both need the Redmine half — a session
  doing them adds `AdaWorldAPI/redmine-rs` via `add_repo` (§0.5).
- From this repo, redmine-rs is **read-only**; deliverables that belong
  there go via a handover or an explicitly-requested PR on that repo.

## 9. Hard lessons (do NOT repeat)

1. **Never council settled canon.** A 5+3 council rejected the AR-shape
   reunion / route-dedup↔SoC unification as "mere rhyme" — the operator
   had already canonized it. Read the rulings first; council only
   genuinely-new claims (`E-ROUTE-KIND-VERB-STRATA`, SUPERSEDED).
2. **Verify producer token casing before writing resolver keys.** The
   resolver's keys must match what the harvester actually emits — ruff
   lowercases `fields.Many2one(...)` to `many2one` at harvest
   (`ruff_python_spo` `kind.to_lowercase()`); a `Many2one`-cased key
   silently never matches. Grep the producer's emit site before seeding
   any `recipe_concept_from_surface` table (the Bugbot-class catch on
   Phase 1).
3. **Verify gap-ledger claims in CODE.** A stale "ruff captures reads not
   writes" claim propagated to a board entry; `writes` had shipped months
   earlier.
4. **Scope cargo to `-p <crate>`; never `--all`** from a path-dep
   workspace (drags siblings).
5. **Boards are append-only:** regrade `**Status:**` in place, prepend
   corrections, never edit an entry body.
6. **Classid order flipped 2026-07-02** (canon HIGH / custom LOW) — the
   2026-06-30 assessment predates the flip; translate its ids.

## 10. SESSION KICKOFF PROMPT (copy-paste to start the next session)

```
You are continuing the OP-rs ⇄ Redmine-rs AR-shape convergence arc
(consumer side). Context + work order:
openproject-nexgen-rs/.claude/handovers/2026-07-05-op-rs-redmine-rs-convergence-handover.md
— read it fully first, then its §0 mandatory reads (the C1–C6 plan, the
OGAR recipe Phase 2 handover, OGAR consumer best-practices, and
RAILS-COVERAGE-KIT.md §5).

Those docs carry OPERATOR RULINGS (the reunion order; ORM as
schema/action-input only; the recipe codebook). Execute them — do NOT
re-litigate or council them.

Your ungated work is C4: measure the Redmine↔OP name-preservation rename
table (associations done in the plan §3; extend to attributes + methods),
committed as reproducible data + measurement notes in
openproject-nexgen-rs. If the session also has OGAR Phase 2 landed and
the branch question resolved, proceed to C5: the pre-registered
shared-RecipeConceptId collapse A/B (register the KILL % BEFORE running).

To touch the Redmine half, add the sibling repo via add_repo
(adaworldapi/redmine-rs, public, branch main) and read redmine-canon
before asserting convergence state. redmine-rs is read-only unless
explicitly asked to push there.

FLAG, don't resolve: consumers dep OGAR via
`claude/odoo-rs-transcode-lf8ya5`; the recipe codebook lives on
`claude/openproject-nexgen-ogar-review-mkjtpq`. Consumer adoption (§6)
waits on the operator's branch decision (§7).

Scope cargo to `-p <crate>`. Commit small + push. Do NOT open a PR
unless asked.
```

---

## Appendix — arc commit trail

op-nexgen (5, pre-merge of PR #72): `22c6826` park dto-check → `f942c77`
reunion-order → `379580d` F17-verified → `603610b` grammar mirror →
`83426d7` codebook-P1 mirror; merged to `main` as `8eb533e` (#72).
OGAR (6, on `claude/openproject-nexgen-ogar-review-mkjtpq`): `b792002` →
`8503948` → `5dd2be1` → `e02608b` → `f76698c` recipe codebook Phase 1 →
`34fa06c` Scope/Concern reserved.
