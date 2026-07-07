# Smoothness patches — measured dissolution of the OP transpile frictions

> Operator ask (2026-07-07): "improve ogar + ruff until the OP task dissolves
> to smoothness." Six fixes, built + tested in local clones, **proven** against
> the real OpenProject corpus via a temporary (never-committed) `[patch]` of
> op-nexgen onto the fixed clones.

## The proof (measured 2026-07-07, corpus 46c1fda2)

| metric | pinned upstream (before) | with these 6 patches | 
|---|---|---|
| emitted structs (EMIT_ALL) | 719 | **726** |
| quarantined uncompilable | 7 | **0** |
| `cargo check -p op-generated` w/ empty quarantine | fails (E0592/E0201/parse) | **clean** |
| AR graph from/to edges | 292 / 232 | **299 / 240** |
| unresolved `→ ?` targets | pervasive (project/type/version/…) | **25 — ALL polymorphic** (`journable`, `customized`, `entity`… — by-design runtime-only targets) |
| wide (>64-field) classes | 0 (max 50) | 0 (max 50) — corpus truth, not a gap |

The consumer needs **no quarantine list, no workarounds**: every extractable
class with a shape emits through ruff→OGAR and compiles.

## The patches (apply in order; each tested + clippy-clean)

ruff (`claude/op-smoothness-ruff` off main; 106+66 tests green):
1. `0001-ruff_ruby_spo-post-baseline-migration-replay…` — the W3 wave
2. `0002-…qualify-nested-classes-by-enclosing-cl…` — fixes the `Rate::Methods` /
   `DefaultHourlyRate::Methods` cross-class merge
3. `0003-…strip-trailing-comments…` — fixes the `t.string :type # comment`
   column-name leak (quote-guarded)

OGAR (`claude/op-smoothness-ogar` off main; render+lift suites green):
1. `0001-ogar-render-askama-keep-foo-foo-foo-sanitized-idents…` — `?`→`_q`,
   `!`→`_bang`, `=`→`set_` prefix; un-quarantines 4 classes
2. `0002-ogar-from-ruff-last-def-wins-dedup…` — Ruby redefinition semantics at
   `lift_actions`; un-quarantines `CostQuery::Operator`
3. `0003-ogar-from-ruff-Rails-convention-association-target-d…` — default
   `class_name` by convention (never overrides explicit; skips polymorphic);
   resolves the `→ ?` AR-graph edges

## Ship state

Write access to `AdaWorldAPI/{ruff,OGAR}` is gated in this session (relay
403; MCP scope = the 3 openproject repos; the session token itself reads but
cannot push those repos — verified empirically). Land via either:
- a session whose scope includes ruff/OGAR: clone, `git am` these patches,
  push `claude/op-smoothness-{ruff,ogar}`, open PRs; or
- extend this session's repo scope, and it finishes the job itself.

After both merge upstream: in op-nexgen, `cargo update -p ruff_spo_triplet
-p ogar-vocab`, EMPTY `QUARANTINE_BY_NAME` (emit_generated.rs), re-run
`EMIT_ALL=1 … emit_generated`, commit the regenerated 726 — the numbers above
are what to expect; the drift fuses will confirm.
