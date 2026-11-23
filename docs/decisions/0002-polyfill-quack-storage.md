# 0002: Storage moves to the Rails polyfill + Quack in lance-graph; SQLx is the oracle only

Status: accepted (owner, 2026-11-23)

## Decision

The server persists through SQLx to PostgreSQL today. It moves to the layout
zammad-rs uses:

- Writes go through the Rails polyfill (`rails-polyfill`, an in-memory ActiveRecord
  with transactions and the save pipeline).
- Reads go through Quack in lance-graph (`quack-shadow` over `lance-graph-quack`).
- State is kept in a state directory: a snapshot plus one record per committed write.

SQLx/PostgreSQL stays only as the oracle. Its recorded answers are what the polyfill
path is checked against. It is never runtime storage again.

The SQLx server as it was before this decision is kept on the `sqlx` branch, with its
own `Dockerfile.sqlx`, so it can be deployed to see the current status. `main`
continues with the migration.

The plan, with sources, is in `docs/plans/polyfill-quack-migration.md`. Its PR
sequence is §5.

## Choices made by the owner

| Question | Choice |
|---|---|
| Which schema the polyfill tables follow | **OpenProject's own** (`db/schema.rb`), generated through `rails-compiler` the way zammad-rs uses `zammad.toml`. Not nexgen's hand-written migrations. Where the nexgen SQL port differs from OpenProject, the difference shows up and gets fixed. |
| Types the polyfill and Quack lack (Date, Float, timestamptz, `lock_version` optimistic locking, a destroy pipeline) | **Added upstream**, in zammad-rs's `rails-polyfill` and in lance-graph, each in its own PR with a disable run, then consumed from here. No nexgen-only encoding. |
| `quack-shadow`'s dependency weight (it pulls `zammad-generated`, `rails-compiler`, ruff and OGAR into the binary) | **Accepted as it is.** Depend on `quack-shadow` without splitting it first. |
| Timestamp precision | **Microseconds, added upstream**, so journal validity ranges (`upper + 1 microsecond`) match OpenProject exactly. |

## What this changes in the plan

- **PR 2** (`op-desk`) generates its table and model specs from OpenProject's schema
  with an `openproject.toml` for `rails-compiler`. This replaces the hand-declared
  specs copied from `op-db/migrations`. The plan's "Later" note is now the main path.
  The column-list falsifier compares against OpenProject's schema, and lists every
  table where the nexgen SQL schema differs.
- **PR 1** (upstream) covers Date, Float, microsecond DateTime, `lock_version`
  optimistic locking and destroy. Each is its own zammad-rs PR with a disable run,
  and must keep zammad-rs's own parity tests green.
- Because the oracle is checked against OpenProject's schema, the EXECUTED_DB capture
  (PR 3) records the SQL port's answers. The OpenProject trace
  (`oracle/work_package_service.rb`) stays the behaviour oracle wherever the two
  disagree.

## Still open

Items 4, 5 and 7 to 12 of the plan's §6:
- representing `validity_period` and `cause`;
- where `feed_json` lives;
- reads on the desk thread;
- pinning zammad-rs and lance-graph revisions;
- Quack refusals for text sorts and date filters;
- migrating any live Railway data;
- the JSON seed;
- delete semantics.

Each is decided when the PR that needs it comes up.
