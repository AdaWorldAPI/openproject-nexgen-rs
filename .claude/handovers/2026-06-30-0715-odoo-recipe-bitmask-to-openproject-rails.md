# Handover — recipe-bitmask conjecture → the Rails/OpenProject clean testbed

> **From:** the odoo-rs transcode session (OGAR `D-RECIPE-BITMASK` arc, 2026-06-30).
> **To:** whichever session is driving the OpenProject/Redmine (Ruby/Rails → Rust →
> SurrealQL) transcode in `openproject-nexgen-rs`.
> **One line:** OGAR is **Open Graph Active Record**, so the canonical "recipe" IS
> the ActiveRecord lifecycle protocol. Odoo (Python) is the *upper-bound* test of
> the recipe-bitmask compression; **Rails is the *clean* test** — and this repo is
> where it should run. Below is the conjecture, the Odoo result, and a concrete,
> grounded probe spec for the Rails leg.

---

## TL;DR — what to run

Add a probe (mirror of `odoo-rs/crates/od-ontology/tests/recipe_redundancy_probe.rs`)
that parses N real Rails models through this repo's existing frontend
(`ruff_openproject` → `ruff_spo_triplet::ModelGraph`, already a path-dep of
`op-codegen-pipeline`), reads each `Model.callbacks` / `Model.validations`
(first-class data — see FINDING below), and measures **how much of the AR
lifecycle is a shared recipe + a per-model override bitmask vs genuine
per-model leftover.** Report the leftover %; that number grades OGAR's
`F15` / `PROBE-OGAR-AR-RECIPE-COLLAPSE` and the `D-RECIPE-BITMASK` `[H]→[G]`
promotion. Prediction: Rails lands **far below** Odoo's 54.3% leftover —
toward the ~7% target — because the Rails callback/validation recipe is a
**fixed, small, enumerable protocol**, not open-ended compute methods.

---

## What I did (this session)

1. **Designed the recipe-bitmask conjecture** and filed it in OGAR canon
   (append-only): `EPIPHANIES.md` → `E-RECIPE-BITMASK` (CONJECTURE `[H]`);
   `DISCOVERY-MAP.md §2.8` → `D-RECIPE-BITMASK` (H / EPIPHANY);
   `INTEGRATION-MAP.md §6` → `F15` (`PROBE-OGAR-AR-RECIPE-COLLAPSE`).
2. **Ran the Odoo (upper-bound) leg** as `odoo-rs/crates/od-ontology/tests/recipe_redundancy_probe.rs`
   (default build, offline). Result below.
3. **Filed two ruff gaps** the probe surfaced in `odoo-rs/crates/od-ontology/specs/UPSTREAM_WISHLIST.md`.

The Rails leg is the *other half* of the same falsifier, and it's your repo's
lane — hence this handover.

---

## FINDING (empirical, verified this session)

### The mechanism is real and already half-shipped (Odoo side)
`od-ontology::ogar_actions::corpus_to_actions` lifts Odoo's behaviour into
exactly **two recipe shapes**: the recompute arm (`KausalSpec::Depends{paths}`)
and the guard arm (`KausalSpec::LifecycleTrigger{before_save}` + `Reject`).
Every guard is byte-identical except its address; every compute shares one
shape and differs only in `Depends.paths`. That IS recipe + per-class delta.

### Odoo upper-bound numbers (slice_2 corpus)
```
behavioral arm : 358  (47 guards + 311 computes; 141 computes reads-captured)
guard arm      : 47 → 1 shared recipe (FULL collapse)
compute arm    : 101 distinct path-sets of 141 resolved (mostly genuine)
headline       : 45.7% recipe-collapsible / 54.3% leftover (188 resolved methods)
```
This **refutes** the strong "Odoo → 7%" reading and **confirms the scoping**:
7% is the best-shaped *Rails-AR* case, not compute-heavy *Odoo-Python*.

### Why Odoo is only an UPPER bound (and Rails isn't)
On the Odoo side three capture gaps cap the measurable collapse — all of which
**Rails does not have**:
- **Odoo:** the live-source `ruff_python_spo` crate path drops `_inherit` in
  `build_graph`; the corpus carries `inherits_from` but the slice's base mixins
  are out-of-slice; method bodies aren't captured.
- **Rails (this repo):** `ruff_ruby_spo` captures the AR recipe **as
  first-class `Model` data** — verified in `/home/user/ruff/crates/ruff_ruby_spo/`:
  - `Model.callbacks: Vec<Callback{phase, target, options}>` — `walk.rs`'s
    `is_callback_phase` allow-lists **20 Rails phases** (`before_save`,
    `after_create`, `around_destroy`, `after_*_commit`, …).
  - `Model.validations`, `Model.associations`, `Model.sti: Option<StiInfo>`.
  - Both Ruby and Python frontends emit the **same** `ruff_spo_triplet::ModelGraph`
    and expand via the **same** `expand()`.

That asymmetry is the whole point: **the AR recipe survives into the IR for
Rails exactly where Odoo's lifecycle is dropped.** The recipe is *literally
captured*, so the per-model override bitmask is directly measurable — no upper
bound.

---

## CONJECTURE (the thing your probe tests)

`D-RECIPE-BITMASK` (OGAR): a best-shaped (AR-canonical) consumer stores the AR
lifecycle recipe **once** and carries only a per-class override **bitmask**
(set = override, clear = inherited default, fall-through per the zero-fallback
ladder) + the genuine deltas — thinning the "impossible 15%" behavioural
leftover toward **~7%**.

Why Rails should hit the target where Odoo didn't: the Rails recipe is a
**fixed enumerable protocol** — 20 callback phases + a handful of validation
kinds (presence/uniqueness/length/format). The bitmask is literally a presence
vector over a known ~20-slot recipe. Odoo's `_compute_*` methods are
open-ended (each reacts to a different field set), so its compute arm is mostly
genuine; Rails' callbacks are not.

Two guards keep it lossless (carry these into the probe):
1. **slot positions RESERVE-DON'T-RECLAIM** — the recipe's slot order is fixed
   (`I-LEGACY-API-FEATURE-GATED`). The 20 callback phases are a stable, ordered
   set; append, never reorder.
2. **redundant = content-hash-identical-to-default** — a clear bit means the
   model's body for that hook resolves to the same content-addressed `ActionDef`
   as the inherited default (lossless-DO §1). (Bodies aren't captured yet — see
   Blockers — so the first-cut probe measures *presence + target dedup*, an
   upper bound on collapse, the mirror of the Odoo probe's honesty.)

**Foreign-call escape is permanent.** Non-AR behaviour (a bespoke service
object, a hand-rolled state machine) stays a foreign-call escape; don't force
it into the recipe. The asymptote is not zero.

---

## The concrete probe (PROBE-OGAR-AR-RECIPE-COLLAPSE, Rails leg)

**Where it lands:** `crates/op-codegen-pipeline/tests/` — it already path-deps
`ruff_openproject` + `ruff_spo_triplet` (per `op-codegen-pipeline/Cargo.toml`),
so no new dep. Mirror the structure of the Odoo probe (linked below).

**What it measures (default build, offline, reproducible):**
1. Parse N real Rails models → `Vec<Model>` via the frontend `op-codegen-pipeline`
   already uses.
2. Build the **recipe** = the fixed callback-phase set (the 20 `is_callback_phase`
   phases) + the validation-kind set. This is the shared AR protocol, stored once.
3. Per model, compute the **override bitmask** = which phases/validations it
   declares (its `Model.callbacks` / `Model.validations`).
4. Partition: **recipe-collapsible** = a declared hook whose `target`+`options`
   match the shared/default shape (or recur across models) ; **genuine leftover**
   = a hook with a unique target/options shape.
5. Report `leftover% = genuine / resolved` and `collapse% = 1 - leftover%`,
   plus the per-phase histogram (how concentrated callback usage is).

**Headline to report (grades F15):** the leftover %. Compare against Odoo's
54.3% and the ~7% target. Assert only true structural invariants (corpus
present, N models ≥ threshold, ratios sum to 100) — **do NOT assert a predicted
ratio** (the Odoo probe's first cut made that mistake; a probe reports the
number, it doesn't force a pass).

**Honesty knobs to copy from the Odoo probe:**
- exclude any model whose hooks weren't captured (data gap ≠ redundancy);
- state the upper-bound caveat (bodies not captured → presence+target dedup,
  not content-hash dedup);
- if you call a real lift function, add a feature-gated consistency assertion
  pinning the probe's mirror to it (the Odoo probe pins to `corpus_action_rows`).

**Reference implementation to mirror:**
`odoo-rs/crates/od-ontology/tests/recipe_redundancy_probe.rs` (commit on
`claude/odoo-rs-transcode-lf8ya5`). Same shape; swap `has_function`-classification
for `Model.callbacks`/`Model.validations` (you have the cleaner data).

---

## The view tier is the icing, not the cake (framing, not probe scope)

Per the operator: **ClassView + bitmask + ERB→askama is the rendering tier ON
TOP of the AR core.** This repo already has the ClassView seam
(`op_canon::class_view::OgarClassView`, re-exporting OGAR's `ogar-class-view`).
The ERB→askama view port (the way `woa-rs` already renders) is the presentation
layer that sits above the AR-recipe substrate. The probe above is about the
**AR core** (behaviour collapse); the ERB→askama work is the separate view-tier
port — keep them distinct so neither dilutes the other.

---

## Update — the inheritance axis is now measured (constructor chaining)

Since this handover was first written, the odoo-rs side added the **second**
collapse axis and measured it (OGAR `E-RECIPE-BITMASK-CHAIN` / `F16` /
`odoo-rs tests/recipe_chaining_collapse.rs`):

- **Mechanism:** a derived class's ClassView is built by *chaining its base
  `LazyLock<ClassView>` constants + its own delta* — inherited recipe parts are
  stored once at the base and shared by every subclass (not per-class leftover).
  This dissolves "out-of-slice" (the base is a registry constant, not a slice
  dependency) and makes "redundant = content-identical-to-default" **structural**
  (referential identity — the inherited part IS the shared cached constant, not a
  hash comparison). Chain = MRO; lance-graph #533 `resolve_overrides` is the order.
- **Odoo measured (lower bound):** the full Odoo inheritance manifest (388
  classes, 166 `inherits_from`, 3328 methods) → naive flatten 4215 vs chained
  3328 = **21.0% collapse / 22.7% behavioural**. Lower bound, because the Python
  corpus's mixin harvest is shallow (real `mail.thread` ~100 methods, a handful
  captured).

**Why this matters for your leg, doubly.** Rails captures concerns/STI as
first-class `Model` data, so (a) your `ruff_ruby_spo` parse sees the *real*
mixin/concern recipes the Python corpus under-harvested, and (b) you can resolve
concern/STI bases through the *same* constructor chain. So the OpenProject probe
should measure **both** axes:
  1. within-class callback/validation override density (the F15 leg above);
  2. **the inheritance axis** — chain concern/STI bases via `inherits_from`/STI
     and measure naive-flatten vs chained over the Rails inheritance DAG (mirror
     `odoo-rs tests/recipe_chaining_collapse.rs`).
Because Rails concerns carry genuine callback recipes (not a shallow harvest),
the inheritance-collapse on the Rails side could **far exceed** Odoo's 22.7% —
that's the cleanest path to the 7%-leftover target: both axes stacking on a
consumer whose recipe is actually captured.

Chain-order correctness is the existing `F1` falsifier (the diamond test); the
chain must be acyclic (a `LazyLock` cycle deadlocks — resolve in topological
order).

## Blockers

1. **Confirm the frontend captures callbacks on the path you use.** The scout
   verified `ruff_ruby_spo` (`/home/user/ruff/`) populates `Model.callbacks`.
   This repo vendors `ruff_openproject` (`vendor/AdaWorldAPI-ruff/crates/`) and
   `op-codegen-pipeline` deps *that*. **Verify `ruff_openproject` populates
   `Model.callbacks`/`validations`/`sti`** (it should — callbacks are core Rails —
   but confirm before building the probe on it).
2. **Need N real Rails models.** The committed fixtures are smoke-sized:
   `crates/op-codegen-pipeline/tests/fixtures/rails_mini/app/models/`
   (`work_package.rb`, `time_entry.rb`, `adhoc_thing.rb`) and
   `vendor/.../ruff_ruby_spo/tests/fixtures/openproject/app/models/` (2 models).
   For a real recurrence measurement you need ~20+ models. Either (a) confirm
   the real OpenProject `app/models/` is on disk and generate a committed
   fixture corpus (the way odoo-rs slices its corpus), or (b) expand the
   `rails_mini` fixture set. Flag what you do — no silent small-N.
3. **Method/callback-target bodies aren't captured.** Same content-hash caveat
   as Odoo: the first-cut probe measures presence + target/options dedup, an
   upper bound on collapse. A `target`-body hash would tighten it (optional,
   file upstream if wanted).

---

## Open questions

1. **Where does the Rails "default" come from?** A clear bit = inherited
   default. For Rails that's `ApplicationRecord` + included **concerns**
   (mixins) + STI parents. Does `ruff_ruby_spo` capture concern *inclusion*
   (so a callback declared in a concern is attributable to the shared recipe,
   not the model)? `Model.sti` + `Model.associations` are captured; confirm the
   concern/`include` edge is too — it's the Rails analogue of the Odoo
   `inherits_from` that determines inherited-vs-override. (If concerns are
   captured, Rails can measure *true* inherited-vs-override, which Odoo could
   not — making it strictly the cleaner test.)
2. **Define "leftover" identically to the Odoo probe** so F15's two legs are
   comparable: leftover = genuine-unique-payload hooks / resolved hooks. Keep
   the definitions aligned or the 7% comparison is apples-to-oranges.
3. **Validation kinds:** `ruff#21` added `validation_kind` (presence/uniqueness/
   length/format). Treat each kind as a recipe slot; the bitmask over kinds is
   another collapse axis. Worth folding into the same probe.

---

## Cross-refs

- **OGAR canon (just landed, branch `claude/odoo-rs-transcode-lf8ya5`):**
  `EPIPHANIES.md` `E-RECIPE-BITMASK`; `DISCOVERY-MAP.md §2.8` `D-RECIPE-BITMASK`;
  `INTEGRATION-MAP.md §6` `F15` / `PROBE-OGAR-AR-RECIPE-COLLAPSE`.
- **Odoo leg (the template):** `odoo-rs/crates/od-ontology/tests/recipe_redundancy_probe.rs`
  + `docs/ODOO-OGAR-MIGRATION-SPRINT.md` (§ "Recipe-bitmask probe") +
  `specs/UPSTREAM_WISHLIST.md` (the two ruff gaps).
- **This repo's seams:** `op_canon::class_view::OgarClassView` (ClassView);
  `op-codegen-pipeline` (`ruff_openproject` + `ruff_spo_triplet` path-deps);
  `crates/op-codegen-pipeline/tests/fixtures/rails_mini/`.
- **Frontend (the captured recipe):** `/home/user/ruff/crates/ruff_ruby_spo/src/walk.rs`
  (`is_callback_phase`, `emit_callback`) → `ruff_spo_triplet::ir::{Model, Callback}`.

— end handover —
