# Vendor state — deviation telemetry

> Consumed by `.claude/tools/vendor-sync.sh`, which appends a timestamped
> log line after every sweep (below the table). The table itself is
> hand-maintained — update it when a deviation is created or retires.
> Epiphany this operationalizes: deviations need expiry telemetry so
> escalation pressure is data, not vibes (2026-07-02 epiphany #2).

## Active deviations

| # | File | Reason | Created | Status | Retires when |
|---|---|---|---|---|---|
| D2 | `vendor/AdaWorldAPI-OGAR/crates/ogar-class-view/Cargo.toml` | `lance-graph-contract` git dep redirected to the sibling vendor path (offline resolution) | 2026-07-02 | **permanent by design** | never — this is how the vendor slice stays self-contained |
| D3 | `vendor/AdaWorldAPI-ruff/{ruff_ruby_spo,ruff_spo_triplet}` | D-AR-3.5 column-stratum patch, local-first (`D-AR-3.5-column-stratum.diff`), re-applied every sweep if upstream overwrites its targets | 2026-07-02 | active, ~1 re-apply/day observed | ruff session merges wishlist **R1** (`.claude/handovers/2026-07-02-ruff-upstream-extraction-contract.md` §6) |
| D4 | `vendor/AdaWorldAPI-OGAR/crates/ogar-render-askama/Cargo.toml` | `lance-graph-contract` git dep redirected to the sibling vendor path — NEW at the 2026-07-05 rebase: upstream `ogar-render-askama` gained this dep for `rust_class.rs` (the ClassView×FieldMask→struct transpiler). Same shape as D2. | 2026-07-05 | **permanent by design** | never — offline slice self-containment |

## Retired deviations

| # | File | Reason | Created | Retired | Retired by |
|---|---|---|---|---|---|
| D1 | `vendor/AdaWorldAPI-lance-graph/crates/lance-graph-contract/src/codegen_spine.rs` | `RouteBucketTyped` (C6) absent upstream; re-applied `codegen_spine.diff` every sweep (6+ re-applications observed across #626–#631) | 2026-07-02 (this session's vendoring) | 2026-07-02 | lance-graph **#632** merged the symbol upstream same-day as the wishlist ask (L1). Diff archived as `codegen_spine.diff.retired-632`; `vendor-sync.sh` now carries a loud regression guard in its place. |

## Sync log

<!-- vendor-sync.sh appends below this line -->

- 2026-07-02T20:55Z — sweep: clean, 0 files changed

- 2026-07-02T20:57Z — sweep: clean, 0 files changed

- 2026-07-05T05:53Z — sweep: clean, 0 files changed
