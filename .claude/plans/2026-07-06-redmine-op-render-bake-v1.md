# Redmine/OP render bake v1 — AR/Rails views → ClassView×FieldMask → askama/jinja

> **Type:** plan + PRE-REGISTRATION (thresholds written BEFORE the first
> measurement run — the F17/C5 discipline).
> **Status:** PLANTED 2026-07-06, fleet in flight (ERB extractor · OP recon ·
> render-kit join design).
> **Thesis:** a Rails view is *detected config* (fuzzy-recipe-codebook §8c):
> it bakes to DATA — a `(class, field-set)` mask — never to hand-transcribed
> templates. The dual-target render (askama Rust / jinja Python, falsifier #2,
> OGAR #158) then re-emits both skins from the mask. If Redmine views and OP
> representers of the same concept project to overlapping masks, "routes are
> skins" stops being doctrine and becomes measurement.

## Shape

```
Redmine app/views/**.erb ──┐ ruff_ruby_spo::views (field-set extractor, closed-vocab)
OP api/v3 representers ────┘ (leg 2; grammar per recon)
        │  per-view (model, field-name set)
        ▼
canonical field order (extract_app_with_schema, N3 positions)
        ▼
FieldMask / WideFieldMask  (#651 — work_packages ~109 cols is the born use-case)
        ▼
dual-target emit: askama (Rust) ∥ jinja (Python)   ← falsifier #2 machinery
        ▼
parked bake: .claude/harvest/redmine-view-bake/ (masks.ndjson + README + samples)
```

## PRE-REGISTERED metrics + thresholds (2026-07-06, before any run)

1. **Mask coverage** — fraction of `receiver.ident` references in views that
   resolve to harvested canonical fields.
   - ≥ 60% → the ERB surface is mask-shaped; the bake stands.
   - 30–60% → partial: bake ships with the uncovered-reference census as the
     finding (helpers/computed surface = the render-side jitter codebook).
   - < 30% → KILL: views are not field-projections; the bake claim is regraded.
2. **Dual-target parity** — for every baked (class, mask), the askama field set
   MUST equal the jinja field set. Threshold: **100%** (deterministic
   machinery; any mismatch is a bug in the kit, not tolerance).
3. **Mask-reuse ratio** — distinct masks per class ÷ views referencing that
   class. < 0.5 supports the Scope/route-dedup SoC claim (many views, few
   masks); ≈ 1.0 refutes it (every view its own mask — skins are NOT shared).

Tail discipline: any view whose field-set maps to NO harvested field is
excluded and counted (never silently dropped); the uncovered-reference
histogram is published with the bake.

## Legs

- **Leg 1 (this session): Redmine ERB** — 506 views local/in-scope; models via
  `extract_app_with_schema` (fixed walker). Measured leg.
- **Leg 2: OP representers** — modern OP is Angular+APIv3; the field surface
  lives in `*_representer.rb`/schema declarations, not ERB. Same
  `(class, field-set)` reduction; grammar per the recon agent. May land as a
  follow-up if the declaration surface needs its own extractor.
- **Convergence measurement (the point):** shared-mask overlap between
  Redmine-`Issue` views and OP-`WorkPackage` representers via the C4 rename
  table — the render-side sibling of C5's shared-RecipeConceptId collapse.
  NOT pre-registered yet: needs leg 2's grammar first; register before running.

## Fleet (model policy: grindwork=Sonnet, accumulation=Opus)

- A1 Sonnet — `ruff_ruby_spo::views` extractor (std-only, closed-vocab,
  word-boundary; presence-only per C2).
- A2 Sonnet — OP corpus recon (direct-token clone; ERB count, representer
  grammar, verbatim samples).
- A3 Opus — render-kit join design (mask width gaps, jinja runtime, canonical
  order source, artifact schema) → `bake-design.md`.
- Orchestrator — assembles the probe in `crates/ruff_openproject/tests/`,
  runs leg 1, pins drift fuses, parks the bake, PRs.

## Run log

- **Run 1 (2026-07-06): VOID — harvest-layout gap, not a KILL.** The E1 gate
  fired at median 0.000 because `extract_app_with_schema` reads only the
  OP-layout baseline (`db/migrate/tables/*.rb`); Redmine ships classic
  migrations, so `Model.fields` came back empty and the basis carried no DB
  columns — the measurement was views-vs-a-columnless-basis, i.e. invalid,
  and the KILL assert did exactly its job (loud on a broken join). The
  pre-registered thresholds stand unchanged for run 2. Fix in flight: a
  classic-migration fallback in the ruff schema reader (create_table blocks +
  add_column applied in file order; renames/removals COUNTED in SchemaReport,
  not applied — an honestly-Inferred basis).
- **Run 2 (2026-07-06, redmine @ `bfd3c33a`, classic-fallback via ruff #48
  patched locally): GREEN — the bake STANDS.**
  - **E1 = 0.667 median coverage** over 342 (view,model) rows (506 ERB
    scanned, 240 views with hits) → **≥ 0.60: the ERB surface is
    mask-shaped; the bake stands.** The uncovered-reference census ships
    with the bake anyway (it is the render-side jitter codebook).
    *Transcription note:* the probe's doc-comment had mis-copied the stands
    bar as 0.80 (plan of record says 0.60 — this section, committed before
    any run). Recorded against both bars: stands@0.60, partial@0.80; the
    census ships either way, so the stricter reading's obligation is met.
  - **E2 = 1.00** — 244/244 non-wide rows: askama == bit-walk oracle, jinja
    witnessed OK. One probe-side parser bug found and fixed en route (the
    kit's `type` → `r#type` raw-ident escape; the KIT was correct, the
    probe's `pub <ident>:` reader wasn't stripping `r#`).
  - **E3 aggregate = 161 distinct masks / 333 views ≈ 0.48 < 0.5 → supports
    the Scope/route-dedup SoC claim**, concentrated exactly where it
    matters: Repository 0.22, Group 0.25, WikiContent 0.25, Project 0.29,
    Query 0.33, User 0.35, Wiki 0.38, CustomField 0.40, Tracker 0.44,
    Issue 0.47 all reuse hard; small classes (Board/Comment/Journal/Version
    = 1.00) trivially don't. "Routes are skins" is now a measurement, not
    doctrine, for leg 1.
  - Drift fuses pinned in the probe (content-signature-guarded on 506 ERB +
    ns=redmine): shape (240, 342), E1 band [0.60, 0.75), renderable rows
    244. Artifact parked at `.claude/harvest/redmine-view-bake/`
    (field_order.ndjson 61 models · masks.ndjson 342 rows · 5 samples).
  - Wide classes (>64 fields) recorded + render-skipped until OGAR #163's
    `render_class_with_methods_wide` is wired (OP work_packages leg).
