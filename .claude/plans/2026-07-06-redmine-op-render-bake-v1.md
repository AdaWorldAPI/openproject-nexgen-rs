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
