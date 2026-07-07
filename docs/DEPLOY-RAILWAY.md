# Deploying op-nexgen to Railway (with a reality-check kanban board)

This is the end-to-end path from an empty Railway project to a running
`op-server` serving a populated kanban board over `/api/v3`.

## What's in the box

| Piece | Path | Role |
|---|---|---|
| Container build | `Dockerfile` | multi-stage; builder = `rust:1.95` (matches the workspace `rust-version`), runtime = `debian:bookworm-slim`, non-root, `tini` init, `/health` HEALTHCHECK |
| Railway config | `railway.toml` | Dockerfile builder, `/health` healthcheck, `$PORT`/`HOST`/`RUST_LOG`/`HYDRATE` env |
| Schema | `crates/op-db/migrations/0001_init.sql` | the 22 tables every `op-db` query targets, derived from op-db's own SELECT/Row contract + the OpenProject baseline (`db/migrate/tables/*.rb`) |
| Seed | `crates/op-db/seeds/kanban_seed.sql` | mock kanban board (projects · statuses · types · users · work_packages), `ON CONFLICT DO NOTHING` |
| Boot hydration | `op-db::Database::{run_migrations,seed_kanban}` + `op-server` main | migrate-on-boot always; seed only when `HYDRATE=1` |
| Local proxy | `scripts/reality-check.sh` | spins an ephemeral Postgres, applies schema+seed, prints the board — the same chain Railway runs |

## The boot contract

`op-server` on startup:
1. reads `DATABASE_URL` (or Railway's individual `PG*` vars) via `op-core::config`;
2. connects; **applies schema migrations** (`sqlx::migrate!`, embedded in the
   binary — the build needs no live DB, hence `SQLX_OFFLINE=true`);
3. if `HYDRATE=1`, loads the kanban seed;
4. binds `0.0.0.0:$PORT` and serves `/health`, `/metrics`, `/api/v3/*`.

A migration failure is fatal (no serving against a half-built schema). A seed
failure is logged and tolerated (the server still serves an empty board).

## Deploy steps

1. **Create the project + Postgres.** In Railway: new project → add a
   **Postgres** plugin. Railway injects `DATABASE_URL` into the app service
   automatically; `op-core` picks it up (no manual wiring).
2. **Deploy the app.** Point the service at this repo; `railway.toml` selects
   the Dockerfile builder. First build compiles the workspace (~a few min).
3. **Seed the board.** `HYDRATE=1` is set in `railway.toml`; the first boot
   applies the schema and loads the kanban seed. (Flip to `0` for an empty
   instance.)
4. **Verify.** `GET https://<service>/health` → healthy;
   `GET /api/v3/work_packages` → the seeded cards;
   `GET /api/v3/statuses` → the board columns.

## Local reality-check (no Railway needed)

```bash
./scripts/reality-check.sh
```

Spins a throwaway Postgres under `/tmp`, applies `crates/op-db/migrations` +
the seed, and prints the board (cards per status per project) plus row
totals, then tears the cluster down. `KEEP=1 ./scripts/reality-check.sh`
leaves it running and prints a `DATABASE_URL` you can point a local
`op-server` at:

```bash
KEEP=1 ./scripts/reality-check.sh          # prints DATABASE_URL=...
DATABASE_URL='postgres://…' HYDRATE=0 PORT=8080 \
  cargo run -p op-server --bin openproject-server
curl localhost:8080/api/v3/work_packages
```

## Schema provenance (why it's trustworthy, not hand-guessed)

`0001_init.sql` is anchored to two sources: **op-db's own query surface** (the
`FromRow` structs + every SELECT/INSERT column — the schema must satisfy what
the code actually asks for) and the **OpenProject baseline** migrations in the
transpile corpus (`db/migrate/tables/*.rb`) for authoritative column types and
nullability. Column resolution was verified by running op-db's real SELECT
lists against the created schema.
