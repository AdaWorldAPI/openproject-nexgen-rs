# Sprint C5 Report — `ruff_openproject` crate (additive)

**Date:** 2026-06-02 · **Branch:** `claude/beautiful-gates-dJo0u` (nexgen)
· **Workbench:** `claude/openproject-sqlx-emitter` on the ruff-clone.

---

## Headline

> A new ruff crate, **`ruff_openproject`**, ties the existing Ruby/Rails
> frontend (`ruff_ruby_spo`, C4) and SPO core (`ruff_spo_triplet`) into a
> thin OpenProject-specific entry point — `extract_graph()` / `extract_triples()`
> + a curated `CORE_V3_RESOURCES` list. **8/8 tests green**. Zero edits to
> upstream-neutral crates; zero new runtime deps. Auto-discovered via the
> ruff workspace's `members = ["crates/*"]` glob — no workspace edit needed.

## Why this shape (not the lance-graph/surreal combo)

The C5-combo plan (lance-graph/surrealdb/ndarray integration) launched a
5-agent read-only mapping sprint earlier this turn (M1 returned with a clean
read of `codegen_spine`; M2-M5 still running in background). Per user pivot
("just create a OP crate in ruff"), this sprint takes the small, concrete,
additive path: ONE new crate, file-disjoint, ZERO touch of any existing
file. The combo integration remains an open path (the M1-M5 maps will
inform a future sprint targeting `lance-graph-contract::codegen_spine` —
M1 identified the additive seam: `impl TripletProjection` in a new crate
without modifying the spine).

## What landed

| File | Purpose |
|---|---|
| `crates/ruff_openproject/Cargo.toml` | name, deps (`ruff_ruby_spo` + `ruff_spo_triplet` via workspace), no extras |
| `crates/ruff_openproject/src/lib.rs` | 4 public fns + 2 constants + 3 unit tests |
| `crates/ruff_openproject/tests/extract_test.rs` | 4 integration tests against the shared C4 fixture (reused via relative path; no duplicate fixture) |

## Public surface

```rust
pub const NAMESPACE: &str = ruff_ruby_spo::NAMESPACE;   // "openproject", re-exported
pub const CORE_V3_RESOURCES: &[&str];                   // 18 curated v3 resources, alphabetised

pub fn extract_graph(rails_root: &Path) -> ModelGraph;
pub fn extract_triples(rails_root: &Path) -> Vec<Triple>;
pub fn filter_to_core(graph: &mut ModelGraph);
pub fn extract_core_triples(rails_root: &Path) -> Vec<Triple>;
```

That's it. A thin orchestrator. The OP-specific knowledge in this crate is:
1. The namespace constant (re-exported from `ruff_ruby_spo`, not redefined).
2. The curated `CORE_V3_RESOURCES` list (matches the `openproject-nexgen-rs`
   seed: 11 fully-covered core + 7 partially-covered).
3. The one-call wrappers that make `extract → expand` a single call from
   the OP entry point.

Why so thin: the OpenProject-specific logic ALREADY lives in `ruff_ruby_spo`
(the IRI namespace, the Rails-specific extraction). The honest role of this
crate is to be the canonical OP entry point — consumers depend on
`ruff_openproject`, not on the lower layers individually.

## Verification

```sh
cargo check -p ruff_openproject --tests    # green, 25.41s
cargo test  -p ruff_openproject            # 3 + 4 + 1 = 8 tests passed
```

- **3 lib unit:** `namespace_matches_ruby_spo`,
  `core_v3_resources_are_alphabetised_and_unique`,
  `filter_to_core_keeps_known_drops_unknown`.
- **4 integration** (against `../ruff_ruby_spo/tests/fixtures/openproject/`):
  `extract_graph_returns_known_models`, `extract_triples_produces_locked_shape`,
  `filter_to_core_keeps_fixture_models`,
  `extract_core_triples_matches_extract_triples_on_pure_core_fixture`.
- **1 doc-test:** the rustdoc example in `lib.rs`.

## Fix-loop

One iteration: `CORE_V3_RESOURCES` was initially grouped "core / partial",
which broke its own `core_v3_resources_are_alphabetised_and_unique` unit
test. Fixed by sorting alphabetically and tagging origin in line comments.
This is the test catching real drift, not theatre.

## What this unblocks

Any downstream consumer (next-gen seed crates, lance-graph SPO loader, a
CLI tool, a notebook) that wants OpenProject SPO triples now has **one
import path** (`ruff_openproject::extract_triples`) instead of two
(`ruff_ruby_spo::extract` + `ruff_spo_triplet::expand`). The seam for
later additive work (`impl TripletProjection` for `lance-graph-contract`)
naturally lives in this crate or a sibling that depends on it.

## Sprint metrics — C0 → C5

| | C0 | C1 | C2 | C3 | C4 | C5 |
|---|---|---|---|---|---|---|
| Agent-runs | 6 | 8 | 3 | 3 | 4 | 0 |
| Fix-loop iterations | 1 | 1 | 0 | 0 | 0 | 1 |
| LoC delta (workbench) | ~46K | ~2200 | ~600 | ~600 | ~1035 | ~200 |
| Git commits to nexgen | many | 4 | 1 | 1 | 1 | 1 |
| Track | codegen | codegen | codegen | codegen | frontend | entry point |

Smallest sprint by LoC. Cleanest deliverable shape — one new crate, no
edits anywhere else, auto-glob workspace membership.

## Recommendation for Sprint C6

Three candidates:

1. **C6-codegen-spine-projection** (M1's seam): new crate `op-surreal-codegen`
   that `impl TripletProjection` from `lance-graph-contract::codegen_spine`
   with `Const = SurrealQL schema IR`, gated by `roundtrip_eq` build-test.
   Cleanest additive plug into the spine. ~2 agents.
2. **C6-combo-mapping-finish**: wait for M2-M5 to settle, persist all maps,
   then design the lance-graph/surrealdb/ndarray integration informed by the
   real seam data. Slower but lower risk.
3. **C6-fixture-grow**: extend the C4 fixture with 3-5 more OP models
   (Project, User, Member) so the OP crate's coverage curve becomes
   measurable; tightens the regression net for future C6+.

My recommendation: **C6-codegen-spine-projection**. Sprint C5 produced
the entry point; C6 plugs it into the contract layer with a build-time
losslessness gate. That converts "we have extraction" into "we have
verified-lossless extraction → SurrealQL schema". M1's map gives the exact
plug; the work is bounded.

## Artifacts

- `docs/SPRINT_C5_REPORT.md` (this file)
- `vendor/AdaWorldAPI-ruff/crates/ruff_openproject/` (Cargo.toml + src + tests)
- `.claude/sprints/c5-op-crate/STATE`
- `.claude/sprints/c5-combo/map/M1-codegen-spine.md` (carries forward —
  the seam data the codegen-spine sprint will need)

🦋
