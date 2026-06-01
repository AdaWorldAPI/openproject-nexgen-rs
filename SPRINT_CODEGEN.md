# SPRINT_CODEGEN.md

**One-sprint plan for the first end-to-end codegen run** from `AdaWorldAPI/openproject` (Ruby/Rails source) into this repo (`openproject-nexgen-rs`), using the `AdaWorldAPI/ruff` fork's `ruff_python_dto_check` + `ruff_spo_triplet` + `ruff_ruby_spo` + codegen pipeline.

This is **Sprint C0** (Codegen-Zero) — the first end-to-end pass. The goal is not 100% coverage; the goal is a **working pipeline** with a **measured coverage number** and **calibration reports** that point at exactly what to extend next.

---

## Pipeline diagram (operational)

```
   AdaWorldAPI/openproject          (Ruby/Rails source, ~2.7 GB)
            │
            │  Step 1: extract
            │   ruff_ruby_spo + ruff_spo_triplet
            ▼
   ModelGraph IR (in-memory)
            │
            │  Step 2: lift
            │   ruff_python_dto_check::contract
            ▼
   Vec<RouteContract> + Vec<Triple>
            │
            │  Step 3: emit
            │   ruff_python_codegen + TargetSpec
            │   (TargetSpec seeded from openproject-rs structure)
            ▼
   AdaWorldAPI/openproject-nexgen-rs (this repo)
            │
            │  Step 4: calibrate
            │   ruff_python_dto_check::calibrate
            ▼
   calibration.json per crate + summary report
```

Step 1 reads Rails. Step 2 normalizes into the language-agnostic contract spine. Step 3 writes Rust. Step 4 checks the three-way invariant (AST ↔ contract ↔ codegen ↔ template).

---

## Sprint C0 tickets

Ten file-disjoint tickets. Each maps to one crate in the ruff pipeline OR one slice of the target repo. Agent guardrails in `.claude/skills/sprint-C0-SKILL.md`.

### Phase A — Pipeline activation (3 tickets, parallel)

| # | Ticket | Done = |
|---|---|---|
| **C0-01** | `ruff_ruby_spo` accepts an OpenProject Rails app path and emits `ModelGraph` JSON for at least three models (e.g. `WorkPackage`, `Project`, `User`) | `ruff-py-dto harvest --frontend ruby --root path/to/openproject --models WorkPackage,Project,User --out bundles/` produces three NDJSON bundles |
| **C0-02** | `ruff_spo_triplet::expand` produces valid triples for the three models from C0-01 | Triple count matches expected algebra (n_fields + n_functions + structural triples); NARS truth tiers assigned per provenance |
| **C0-03** | `TargetSpec` for `rust-axum-sqlx` derived from `openproject-rs` Cargo.toml + crate structure (NOT from scratch — read the seed) | `target-spec/rust-axum-sqlx.toml` exists, names every crate in `openproject-rs/crates/`, points each at its `models_root` and `handlers_root` |

**Phase A acceptance:** end-to-end smoke test runs without panics on three models. No emit yet — just extraction + spec.

### Phase B — First emit slice (4 tickets, partially serial)

| # | Ticket | Done = |
|---|---|---|
| **C0-04** | Emit `WorkPackage` model into `crates/op-models/src/work_package.rs` from extracted triples + target spec | File compiles in the seed Cargo workspace; SQLx FromRow + Serialize/Deserialize derived correctly; no `todo!()` |
| **C0-05** | Emit `WorkPackage` handlers into `crates/op-work-packages/src/handlers/` for the three most common `HandlerKind`s (likely `list_for_tenant`, `detail_for_tenant`, `template_get` based on Rails conventions) | Files compile; signatures match the existing `op-work-packages` patterns from the seed |
| **C0-06** | Emit DTOs into `crates/op-contracts/src/work_package/` for the form-handling kinds | Compiles; field types narrowed via calibration lint (not all `Option<String>` defaults) |
| **C0-07** | Emit calibration report for the WorkPackage slice → `calibration/work_package.json` + human-readable `calibration/work_package.md` | Report enumerates: emitted handlers, stubbed handlers, extractor-gap'd handlers, unmapped-model warnings, template-context-mismatch warnings |

**Phase B acceptance:** `cargo check -p op-models -p op-work-packages -p op-contracts` is green on the regenerated WorkPackage slice. The seed code for these three crates can be diffed against the codegen output; the diff is the *delta the pipeline produced*.

### Phase C — Coverage measurement + generalization (3 tickets)

| # | Ticket | Done = |
|---|---|---|
| **C0-08** | Run the pipeline against **all** OpenProject controllers (not just WorkPackage) and emit coverage report | `coverage-report.md` with: total Rails controller actions, emitted faithfully, stubbed-but-flagged, extractor-gap'd. The 6-18% baseline becomes a measured number. |
| **C0-09** | Top-3 extractor gaps identified in C0-08 get extension proposals (not implementations — proposals) in `extraction-gap-proposals.md` | Each proposal: which Rails pattern failed extraction, why, what minimal addition to `ruff_ruby_spo` would catch it |
| **C0-10** | Final sprint report → `docs/SPRINT_C0_REPORT.md` | Coverage numbers; measured time; calibration findings; explicit recommendation on whether to do Sprint C1 or to extend ruff_ruby_spo first |

**Phase C acceptance:** an honest report that says "we generated X% of OpenProject end-to-end in N hours of pipeline time, here are the M top gaps." That's the asset.

---

## What this sprint is NOT trying to do

- **Not aiming for 100% coverage.** Extraction profiles are incomplete by construction on first pass. Coverage growth is sprint-over-sprint.
- **Not benchmarking performance.** This sprint measures pipeline *output*, not pipeline *speed*. Performance work comes after correctness.
- **Not deploying anything.** The deliverable is a Rust source tree with calibration reports, not a running service. Deployment is a separate concern.
- **Not migrating data.** OpenProject has a Postgres schema; we are regenerating *application code*, not data. Schema migration is a separate concern.

---

## Calibration acceptance criteria (the methodology spine)

For each emitted file, four invariants must hold (these are the calibrate.rs lints):

1. **`unmapped-model`**: every model referenced in the extracted contract appears in the emitted handler. If a Rails controller references `Project`, the generated Rust handler must reference `crate::models::project::Project`. No silent drops.

2. **`template-context-mismatch`**: every template `context_key` the Rails view uses is provided by the emitted handler, and vice versa. If `app/views/work_packages/index.html.erb` references `@work_packages`, the Rust handler must put that in the template context.

3. **`form-field-gap`**: every `request.params[:x]` read in the Rails controller has a corresponding DTO field in the Rust form struct.

4. **`output-kind-mismatch`**: the HandlerKind classification matches the Rust return type. A `redirect_to` in Rails → axum `Redirect` return type. A `render :json` → `Json<T>` return.

5. **`extractor-gap`**: facts the Rails extractor could not classify get reported at the SOURCE-CODE level (which Rails file, which line), pointing at *extending ruff_ruby_spo*, not at editing the generated Rust.

A successful Sprint C0 has lint reports for every emitted crate. Files that fail lints are stubbed with explicit `// EXTRACTOR-GAP: <reason>` comments, never silently shipped.

---

## Token budget (32 agents, ~2.5h window)

Phase A (3 tickets) → 1 agent each, ~30 min each. **Total: 3 agents × 0.5h = 1.5 agent-hours.**

Phase B (4 tickets) → 3-5 agents each (the WorkPackage emit is the iterative heart), ~90 min each. **Total: 16 agents × 1.5h = 24 agent-hours.**

Phase C (3 tickets) → 2-3 agents each, ~60 min each. **Total: 8 agents × 1h = 8 agent-hours.**

Grand total: ~27 agents working in parallel. The 32-agent capacity fits with margin. Within a 2.5h wall-clock window, agents that finish Phase A early can join Phase B mid-stream.

---

## Decision points before launching agents

| # | Question | Default | Override if |
|---|---|---|---|
| 1 | Three reference models for C0-01? | `WorkPackage`, `Project`, `User` | OpenProject's actual highest-coverage controllers differ from these — check `app/controllers/` first |
| 2 | Target spec format? | TOML | JSON if Cargo workspace tooling prefers it |
| 3 | Coverage measurement unit? | Controller action count | Endpoint count; LoC ratio; both |
| 4 | Sprint duration cap? | 2.5h (one token-window) | Shorter if early Phase A signal is bad |
| 5 | Failure threshold? | Abort sprint if Phase A doesn't smoke-test green in 45 min | Adjust per actual ruff_ruby_spo readiness |

---

## What happens after C0

Three possible Sprint C1 directions, decided based on the C0 calibration report:

- **C1-coverage:** if C0 shows ~30-50% coverage with most gaps being "extractor missed a known pattern," extend `ruff_ruby_spo` to close those gaps. Repeat the run. Coverage climbs.
- **C1-quality:** if C0 shows ~20% coverage but the *quality* of generated code is high, the next sprint focuses on hand-narrowing the Rust idioms in the target spec — better DTO types, better axum extractors, better error handling.
- **C1-elixir-frontend:** if C0 demonstrates the pipeline works at scale, the next sprint creates `ruff_elixir_spo` and runs the same pipeline against a representative Elixir source (and only then becomes relevant for Bardioc-PoC integration).

The decision is data-driven from C0's report, not pre-committed.

---

## Anti-pattern guardrails (specific to codegen work)

These are additions to the existing `.claude/skills/` and `.claude/hooks/` discipline:

**1. No hand-editing generated code.** Every file in `crates/*/src/` after a codegen run has a header comment `// GENERATED by ruff_python_codegen — DO NOT EDIT. Modify target-spec/<file>.toml instead.` A pre-commit hook rejects PRs that edit generated files without updating the spec.

**2. Stubs must be explicit.** When extraction fails, the emitter writes a stub function body with:
```rust
// EXTRACTOR-GAP: ruff_ruby_spo could not classify this Rails pattern.
// Source: app/controllers/foo_controller.rb:42
// Pattern: respond_to with conditional render
// To resolve: extend ExtractionProfile::respond_to_handler in ruff_ruby_spo
unimplemented!("extractor-gap: see calibration/foo.json")
```
Never `todo!()` (panics at runtime). Never silent. Always pointing at the source-level fix.

**3. The target spec is the canonical artifact.** Not the generated code. If two agents disagree on how something should look, they fight in the `target-spec/*.toml`, not in `crates/*/src/`. The Rust source is the *output*, not the source of truth.

🦋

---

## Quick-start for an agent landing on C0-N

1. Read `NEXGEN_SEED.md` (5 min) — orientation about why this repo exists
2. Read this file (`SPRINT_CODEGEN.md`) (10 min) — what we're trying to do
3. Read `.claude/skills/sprint-C0-SKILL.md` (5 min) — hard rules, ticket scope
4. Verify your `AdaWorldAPI` PAT and the ruff repo are accessible (`gh repo view AdaWorldAPI/ruff` — token comes from your environment, never committed)
5. Read your specific ticket above
6. For Phase B tickets: read `crates/<your-crate>/` in the seed first to understand the target idiom
7. Open a draft PR before writing significant code, so the orchestrator can flag direction problems early
