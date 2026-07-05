# Vendor state — deviation telemetry

> Consumed by `.claude/tools/vendor-sync.sh`, which appends a timestamped
> log line after every sweep (below the table). The table itself is
> hand-maintained — update it when a deviation is created or retires.
> Epiphany this operationalizes: deviations need expiry telemetry so
> escalation pressure is data, not vibes (2026-07-02 epiphany #2).

## Active deviations

| # | File | Reason | Created | Status | Retires when |
|---|---|---|---|---|---|
| D3 | `vendor/AdaWorldAPI-ruff/{ruff_ruby_spo,ruff_spo_triplet}` | D-AR-3.5 column-stratum patch, local-first (`D-AR-3.5-column-stratum.diff`), re-applied every sweep if upstream overwrites its targets | 2026-07-02 | active, ~1 re-apply/day observed | ruff session merges wishlist **R1** (`.claude/handovers/2026-07-02-ruff-upstream-extraction-contract.md` §6) |

> **Note (2026-07-05): the whole "vendored slice, offline-only, redirect git→path"
> premise was falsified for lance-graph.** `git ls-remote`/`git clone` of the
> AdaWorldAPI repos succeed over the proxy. So lance-graph is now a plain Cargo
> **git dep** (below), and D2/D4 — which existed only to make an offline vendored
> slice resolve — are retired. The same treatment is available for OGAR (no local
> source deviations); only ruff must stay vendored until its D3 patch lands
> upstream (R1).

## Retired deviations

| # | File | Reason | Created | Retired | Retired by |
|---|---|---|---|---|---|
| — | **lance-graph vendoring (whole `vendor/AdaWorldAPI-lance-graph` mirror, ~47k LOC)** | Vendored on the false premise that AdaWorldAPI repos were unreachable and only `raw` worked | 2026-07-02 | **2026-07-05** | **Empirically un-vendored.** git clone works over the proxy and `lance-graph-contract` is a trait-only **zero-dep leaf**, so all consumers (op-nexgen's 5 + OGAR's 2) use a Cargo **git dep** `{ git = ".../lance-graph", branch = "main" }`; Cargo lock-pins `eda867bd` as one unified source. No vendored source, no clones, no bootstrap script. |
| D2 | `vendor/AdaWorldAPI-OGAR/crates/ogar-class-view/Cargo.toml` | `lance-graph-contract` git→path redirect (needed only while lance-graph was vendored/offline) | 2026-07-02 | 2026-07-05 | Retired **with** the lance-graph un-vendoring — OGAR carries the upstream **git** dep verbatim; the raw sweep keeps it as-is, no re-apply. |
| D4 | `vendor/AdaWorldAPI-OGAR/crates/ogar-render-askama/Cargo.toml` | same git→path redirect (added 2026-07-05 for `rust_class.rs`'s new contract dep) | 2026-07-05 | 2026-07-05 | Same as D2 — obsolete the moment lance-graph became a reachable git dep. Lived less than a day. |
| D1 | `vendor/AdaWorldAPI-lance-graph/crates/lance-graph-contract/src/codegen_spine.rs` | `RouteBucketTyped` (C6) absent upstream; re-applied `codegen_spine.diff` every sweep (6+ across #626–#631) | 2026-07-02 | 2026-07-02 | lance-graph **#632** merged the symbol upstream; moot now that lance-graph is a git dep (diff archived as `codegen_spine.diff.retired-632`). |

## Sync log

<!-- vendor-sync.sh appends below this line -->

- 2026-07-02T20:55Z — sweep: clean, 0 files changed

- 2026-07-02T20:57Z — sweep: clean, 0 files changed

- 2026-07-05T05:53Z — sweep: clean, 0 files changed
