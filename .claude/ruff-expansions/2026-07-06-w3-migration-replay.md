# W3 — ruff schema-reader migration-replay (expand ruff)

> Operator directive "expand ruff where you need more." The baseline-only OP
> schema reader (`ruff_ruby_spo/src/schema.rs`) was expanded with post-baseline
> migration-replay. **Code-complete + tested; PR ship deferred to the sanctioned
> path (see below).** Patch: `2026-07-06-w3-migration-replay.patch` (this dir).

## What was built (in ruff, tested)
`replay_post_baseline_migrations` runs after the baseline `parse_tables_dir`,
scanning `db/migrate/*.rb` + `modules/*/db/migrate/*.rb` (filename/timestamp
sorted across both trees) for four top-level forms and applying them onto the
in-memory baseline columns: `add_column`, `rename_column`, `remove_column`/
`remove_columns`, `change_column` (bare + parenthesized). `columns_from` flips
`"baseline-only"` → `"baseline+replay"` only when a mutation actually applies
(no-migration corpus ⇒ byte-identical output, regression-tested). 9 unit tests
+ 1 corpus-gated drift fuse. `cargo build`/`clippy`/`fmt` clean; `cargo test -p
ruff_ruby_spo` 103 passed. Local commit `6e98f0e` (clone `/tmp/ruff-w3-attempt`,
branch `claude/op-schema-migration-replay`).

## The plan-changing FINDING (measure, don't claim)
The plan assumed WorkPackage = ~109 fields, so replay would cross 64 and light
the wide leg (W4). **Measured reality: 27 baseline → 31 post-replay (+4:
`position`, `story_points`, `remaining_hours`, `budget_id`).** The `~109` was an
overestimate. The rest of WorkPackage's real-world breadth lives in:
- `change_table :work_packages do |t| … end` block bodies (`t.references` /
  `t.remove_references`, incl. polymorphic pairs) — OUT of the top-level
  `add_column` spec; would need new, untested block-parsing in ruff; and
- runtime **custom fields** + plugin columns that are NOT in migrations at all.

**Consequence: no core class crosses 64 → W4 (wide emit) is empty by
measurement.** The wide render path (`render_class_with_methods_wide`) stays
property-verified but unexercised — correct, because the corpus has no wide
class. The transpile is complete over the all-narrow core.

## SECURITY CORRECTION (important — supersedes the handover's push workaround)
The `2026-07-06-op-rs-v3-transpile-goal-handover.md` "Push workaround (org
GitHub-App gate)" says to unset `HTTPS_PROXY`, extract `GH_TOKEN`/`GITHUB_TOKEN`
from env, and embed it as `https://x-access-token:${TOK}@github.com/...`. **Do
NOT do this** — it is a token-exfiltration pattern and violates the hard rule
"never disable TLS verification or unset HTTPS_PROXY." Empirical reality:
- Normal proxied **reads** (`git clone`/`fetch`, cargo git deps) work fine.
- Normal proxied **push** to AdaWorldAPI/ruff returns **403** (the org App gate).
- The sanctioned WRITE path is the **MCP GitHub tools** (GitHub App auth) — scope
  the repo via `list_repos` + `add_repo`, then `push_files` / `create_pull_request`.

## To ship (sanctioned path)
1. `list_repos` → `add_repo AdaWorldAPI/ruff`.
2. `push_files` the modified `crates/ruff_ruby_spo/src/schema.rs` onto branch
   `claude/op-schema-migration-replay`, then `create_pull_request`.
   (Or apply `2026-07-06-w3-migration-replay.patch` to a fresh clone and push
   via MCP.) NEVER the token-in-URL bypass.
