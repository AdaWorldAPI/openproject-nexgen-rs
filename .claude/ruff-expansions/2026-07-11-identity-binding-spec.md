# SPEC — bind the menu-quad IDENTITY axis (`surfaces_concept` on menu nodes)

> The keystone lane: set `MenuQuad::identity_concept` (dormant `None` today) so a
> menu node carries a concept token the consumer resolves to a classid — the
> quad's identity axis + the gate that re-opens the staged Lane B (OGAR facet
> wiring). Grounded in the identity scout. Base = ruff main `498ff55`.
>
> **Status: SHIPPED (ruff PR #87, base main `498ff55`).** Built to §v2 below.
> Gate green: 404 tests, count-lock 79 (no mint), clippy+fmt clean. Real OP
> corpus ledger: 137 items → 23 without_concept + 41 derived_matched + 73
> derived_unmatched (conservation holds). Satisfies Lane B's sole gate. Board:
> `.claude/board/EPIPHANIES.md` 2026-07-12 entry.
>
> **[was] READY v2 (council-consolidated: convergence-architect OPPORTUNITY-NOW
> + truth-architect).** Decisive answer to the crux: **ONE predicate
> `surfaces_concept`, NO mint, tier-carried honesty.** The corrections below
> SUPERSEDE §2's tier-unification and §5.1's "harvester can't know" claim.

## v2 — COUNCIL CONSOLIDATION (build to THIS)

**Both councils converge; the reconciling fact (convergence-architect): the vocab
already has `Provenance::OpenProjectExtracted (0.95, 0.88)`** — minted for exactly
"deterministic OpenProject-Rails fact with a small unresolvable residual," which
IS the `controller→singularize→model` case. No `derives_concept` mint (DROP — it
forks the source-agnostic consumer resolver for zero gain).

1. **`MenuQuad` gains `identity_tier: Provenance`** (mirroring the existing
   `part_of_tier`), and `to_triples` emits `SurfacesConcept` at `self.identity_tier`
   — **fixing the hardcoded `Provenance::Authoritative` at `quad.rs:163`** (the
   structural over-claim blocker). `identity_concept: Option<String>` stays.
2. **Per-arm tiers (NOT one tier — §2's unification was the error):**
   - C# `roomAliases` config → **Authoritative** (unchanged).
   - Odoo `res_model` → **Authoritative** — it's a DECLARED literal read verbatim
     from `<field name="res_model">account.move</field>`, no inflection.
   - Rails `controller→singularize→model`, **roster-verified** → **`OpenProjectExtracted (0.95, 0.88)`**.
3. **The roster cross-check makes the derived tier honest (truth-architect
   finding 3 — my §5.1 "harvester can't know" was FALSE).** The schema arm already
   harvests the real model roster (`schema::model_name_for_table` over the
   migration tables). The identity binding MUST cross-check each derived model
   token against that roster and emit `surfaces_concept` ONLY for a **match**.
   This is what licenses `OpenProjectExtracted`: emitted Rails tokens are
   verified against the actual model set, not unverified guesses.
4. **4-bucket conservation ledger** (replaces §3's 2-bucket):
   - `without_concept` — no `controller:`/`res_model` (identity dormant BY DESIGN).
   - `with_concept_declared` — Odoo `res_model` / C# config (Authoritative).
   - `with_concept_derived_matched` — Rails token ∈ roster (OpenProjectExtracted, EMITTED).
   - `with_concept_derived_unmatched` — Rails token ∉ roster (NOT emitted; the
     visible failure-rate bucket — irregular plurals, namespaced controllers).
5. **Fix the inflection edge cases (truth-architect finding 4):** a namespaced
   controller `admin/settings` today splits on `_` not `/` → garbage token
   `Admin/setting`. Split on `/` (take the last path segment) BEFORE singularize.
   The roster cross-check is the safety net for remaining irregular plurals
   (∉ roster → `derived_unmatched`, not emitted) — but fix the `/` case since
   `admin/*` controllers are common in OP.
6. **Doc-comment honesty update (both councils):** amend `SurfacesConcept`'s doc
   at `triple.rs:560-565` from "the config IS the claim" to: *object token sourced
   from corpus-owner config (Authoritative) OR a declared `res_model`
   (Authoritative) OR a roster-verified derived model name (OpenProjectExtracted);
   the provenance tier records which.* Leaving it config-only is doctrine drift.

**Convergence with the existing path (not a bypass):** the Rails token shape
(`WorkPackage`) is byte-identical to the `PortSpec::class_id` key; the harvester
emits the token, the consumer resolves-or-refuses (source-agnostic). No classid
minted on the harvest side.

**This unblocks Lane B:** the promoted CRUD spine (`work_packages`/`projects`/
`time_entries`) is the load-bearing nav core; once these carry `surfaces_concept`,
Lane B's `mint_menu_facets` resolves a non-zero classid → no classid-0 collision.

**Build order:** (1) `quad.rs` `identity_tier` field + emit fix; (2)
`odoo_quad.rs` res_model read → Authoritative; (3) `menu_regions.rs` controller
capture + `/`-aware singularize + roster cross-check + 4-bucket ledger +
OpenProjectExtracted emit; (4) `triple.rs` doc-comment (NO mint). Gate centrally.

---


## 0. What the scout established (with citations)

- **`surfaces_concept` is Authoritative CONFIG today, one producer:** C#'s
  `roomAliases` table (`ruff_csharp_spo/harvester/Program.cs:222-228`), keyed by
  source dir. Its doc frames it as "the config IS the claim"
  (`triple.rs:560-565`). Rails + Odoo pass `None` (`menu_regions.rs:1026`,
  `odoo_quad.rs:141`).
- **The consumer resolver keys on the MODEL name.** `PortSpec::class_id("WorkPackage")`
  (`ogar-vocab/src/ports.rs:120-125`, `OPENPROJECT_ALIASES`) →
  `op-codegen-pipeline::ogar_consumer::render_classid_of` (`ogar_consumer.rs:86-88`)
  → `0x0102_0001`. **Not** the screen name (`work_packages`), **not** the
  canonical concept (`project_work_item`). Three shapes, one idea
  (`ogar-vocab/src/lib.rs:1517-1518`).
- **The harvester emits a TOKEN, the consumer resolves it** (concept-as-join-key,
  playbook §250-259). No `controller→model` fact exists today; the inflection to
  bridge it exists but is unwired (`schema::model_name_for_table` /
  `routes::singularize_local`, `schema.rs:978-1007`).
- **Doctrine:** a concept is mintable only on METHOD ∧ STORAGE ∧ STRUCTURE; a
  menu screen is STRUCTURE only. **Fabricating a concept, keying the wrong
  shape, or minting a two-axis non-concept are anti-patterns — the correct move
  for a targetless/model-less screen is REFUSAL** (`playbook:444-470`).

## 1. The honest minimal binding (harvester emits a derived token; consumer resolves)

**Emit `surfaces_concept = <model-name token>` on a menu node ONLY when the item
resolves to a backing model; else emit nothing (refusal).** The harvester never
resolves a classid (that's the consumer's `PortSpec`); it never fabricates (the
token is derived from harvested facts + deterministic inflection).

- **Rails** (`menu_regions`): the item's target `controller:` (already partially
  captured — the harvest reads `action:`/`has_controller`; it must also capture
  the controller **value**). `controller stem → model name` via the existing
  `singularize` inflection (`work_packages → WorkPackage`). Set
  `identity_concept = Some("WorkPackage")`. Items with no `controller:` (URL
  targets, pure-UI/settings menus) → `None` (refusal — honest; the consumer has
  no model to resolve).
- **Odoo** (`odoo_quad`): EASIER — the menuitem's `act_window.res_model`
  (`account.move`) IS the model token directly (no inflection). The Odoo quad
  harvest already resolves the action; extend it to also read `res_model` and set
  `identity_concept = Some(res_model)`. No `res_model` (server action, no model)
  → `None`.
- **C#** already binds via `roomAliases` — untouched; it is the config-sourced
  precedent this derived arm complements (config where available, derived where a
  clean model target exists, refusal otherwise).

## 2. THE tier decision (council rules)

C#'s `surfaces_concept` is **Authoritative** ("config IS the claim"). A **derived**
model-name token (routes + inflection) is deterministic but NOT curated config —
emitting it Authoritative would over-claim. Proposal: **`OpExtracted`** (or
`Inferred`) for the derived Rails/Odoo binding — honest that it's a
machine-derived candidate the consumer's alias table validates, distinct from
the corpus-owner-curated C# config. The predicate's DEFAULT stays whatever C#
uses; the derived arm passes an explicit lower tier at emit. (Council: confirm
the tier, and whether mixing config-Authoritative + derived-Inferred under one
predicate is honest or needs a separate predicate.)

## 3. Refusal is the correctness property, not a gap

Most menu items are NOT resource-CRUD screens (settings, admin, help, external
URLs). Binding a concept to them would pollute the codebook with two-axis
non-concepts (the doctrine's forbidden move). **The harvest MUST emit no
`surfaces_concept` for a model-less target** and count it (`without_concept` /
`with_concept` conservation ledger). A low `with_concept` fraction is CORRECT,
not a failure — the quad's identity axis is dormant *by design* for non-model
screens.

## 4. What this unblocks (Lane B)

Once menu nodes carry `surfaces_concept`, the staged Lane B facet wiring builds
Slice 1+2 together (the way both Lane-B reviewers said it earns its keep): the
`mint_menu_facets` resolves each node's concept object → classid via
`PortSpec::class_id`, and the facet carries a real (non-zero) classid — no
classid-0 default-bucket collision. This spec is Lane B's prerequisite.

## 5. Honest deltas

1. **`controller → model` is derived, not a harvested fact** — the singularize
   inflection is deterministic but has edge cases (irregular plurals, namespaced
   controllers `admin/settings`). The consumer's `PortSpec` alias table is the
   validator: a wrong-shaped token simply resolves to `None` (no classid), never
   a wrong classid. Fenced by the refusal semantics.
2. **Only promoted models resolve** — `PortSpec::class_id` returns `Some` only
   for models already in `OPENPROJECT_ALIASES` (three-axis-witnessed). A derived
   token for an un-promoted model resolves to `None` — correct (the concept isn't
   minted yet); this arm does NOT mint concepts, it references them.
3. **No config table fabricated** — the doctrine's sanctioned config-row source
   (corpus-owner screen→concept map, like `roomAliases`) is NOT invented here;
   the derived arm covers the mechanical CRUD-resource case, and a future
   config-row arm (operator-supplied) covers the rest. This arm never guesses a
   concept the facts don't support.

## 6. Council questions

1. **convergence-architect** — is the derived `controller→model→token` binding
   honest per the doctrine (config-IS-the-claim vs a deterministic derivation),
   or does `surfaces_concept` REQUIRE a curated config row (operator input) such
   that the derived arm should be a DIFFERENT predicate (`derives_concept`) that
   the consumer treats as a candidate? Does emitting a model-name token the
   consumer resolves converge with the existing PortSpec path, or bypass it?
2. **truth-architect / baton** — the tier (Authoritative-config vs
   Inferred-derived under one predicate); the refusal semantics (no-emit for
   model-less, counted); the name-shape bridge (screen→model) correctness + edge
   cases; no classid re-mint on the harvester side (token only).

**Do not build until both rule.** If the council says a derived token pollutes
the Authoritative `surfaces_concept` semantics, either (a) a separate
`derives_concept` predicate (needs a mint), or (b) STAGE for an operator config
row. Honest either way.

## 7. Gate + allowlist (if greenlit)

Central: `cargo test -p ruff_spo_triplet -p ruff_ruby_spo -p ruff_python_spo`
green · clippy + fmt · corpus probe (`with_concept` measured + pinned). No mint
if the derived arm reuses `surfaces_concept` (tier-only change); a mint (80) if
the council rules a separate `derives_concept`. Allowlist:
`ruff_ruby_spo/src/menu_regions.rs` (controller capture + model derive + emit),
`ruff_python_spo/src/odoo_quad.rs` (res_model read + emit), the shared
`singularize` lift if needed.
