# NEXGEN_SEED.md

**This repository is a seed.** It was initialized by mirroring `AdaWorldAPI/openproject-rs` (the first manual Rust-port attempt) at commit-state of 2026-06-01, and is the **target** for a codegen-driven regeneration run from `AdaWorldAPI/openproject` (the upstream Ruby/Rails source).

## The three repositories in relation

```
  AdaWorldAPI/openproject          (source — Ruby/Rails, OpenProject industry-grade PM stack)
            │
            │  ruff fork pipeline:
            │  ─ ruff_python_dto_check        AST → RouteContract extraction
            │  ─ ruff_spo_triplet             SPO triple core (NARS-calibrated)
            │  ─ ruff_ruby_spo                Ruby/Rails frontend
            │  ─ ruff_python_codegen          TargetSpec emitter
            ▼
  AdaWorldAPI/openproject-rs       (REFERENCE — first manual port, ~6-18% coverage)
            │                       Frozen as calibration baseline.
            │                       NOT to be edited going forward.
            │
            │  [seed copy at 2026-06-01]
            ▼
  AdaWorldAPI/openproject-nexgen-rs (THIS REPO — codegen target)
                                    Seeded from openproject-rs structure.
                                    Crates and Cargo.toml act as TargetSpec
                                    template for the codegen run.
                                    Calibration lints quantify the gap to
                                    100% upstream coverage.
```

## Why a manual seed before codegen

Codegen needs a TargetSpec — a description of *what the target source should look like*. Three options for getting one:

1. **Write it from scratch.** Months of work, high risk of getting Rust idioms wrong.
2. **Auto-derive from the source.** Doesn't work — the source is Ruby/Rails, the target is Rust/axum/SQLx, the idioms are structurally different.
3. **Use the existing manual port as the proof template.** This is the pragmatic answer. `openproject-rs` contains roughly 6-18% of upstream coverage but the *Rust idioms* it shows (crate structure, axum handler shapes, SQLx model patterns, dto module paths) are correct. Codegen reads these patterns from the seed and applies them at scale.

This is exactly what `crates/ruff_python_dto_check/CODEGEN-DESIGN.md` calls *"port the proven translation logic that already exists in the downstream repo's tools/ — do NOT reinvent."*

## What changes vs the seed snapshot

After codegen runs, this repo will diverge from openproject-rs in three predictable ways:

1. **Many more crates / handler files / model files.** Codegen extracts handlers from openproject Rails source that the manual port never touched. Expect 5-10× more handler emitters, more DTO definitions, more query types.

2. **Calibration reports.** Every codegen run emits a `calibration.json` per crate that flags `unmapped-model`, `template-context-mismatch`, `form-field-gap`, `output-kind-mismatch`, `extractor-gap`. These are the *honest* report of what coverage was achieved and where the source code is too irregular for current extraction profiles.

3. **The 6-18% number becomes measurable.** The seed's coverage estimate was informal. After codegen, we measure: of N OpenProject controller actions, M were emitted faithfully, K were stubbed-but-flagged, L were extractor-gap'd. That ratio is the empirical truth.

## What NOT to do in this repo

- **Do not hand-edit generated code.** Calibration lints will silently overwrite hand edits on next regen. If a generated file is wrong, the fix lives in the extraction profile or the target spec, not in the generated file.
- **Do not delete openproject-rs.** It's the calibration baseline. Coverage comparisons only make sense if the manual port stays frozen.
- **Do not commit code that doesn't compile.** Every codegen sprint ends with `cargo check` green. If extraction gaps cause invalid code, the lint must fire BEFORE the file lands, not after.

## What lives in this repo besides generated code

- `SPRINT_CODEGEN.md` — the sprint plan for the next codegen run, with tickets, fairness methodology, and calibration acceptance criteria.
- `.claude/` — agent guardrails ported from the bardioc PoC (same `block-grep-sed-head-tail.sh` hook, same skill structure adapted for codegen work).
- `target-spec/` (future) — TOML/JSON files describing how each Rails idiom maps to Rust idioms. Initially derived from the seed; refined per sprint.

🦋 The seed is the starting line, not the finish.
