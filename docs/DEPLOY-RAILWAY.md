# Deploying op-nexgen to Railway (with a reality-check kanban board)

This is the end-to-end path from an empty Railway project to a running
`op-server` serving a populated kanban board over `/api/v3`.

## What's in the box

| Piece | Path | Role |
|---|---|---|
| Container build | `Dockerfile` | multi-stage; builder = `rust:1.95` (matches the workspace `rust-version`), runtime = `debian:bookworm-slim`, non-root, `tini` init. `HEALTHCHECK` hits `/health` — a **liveness** probe (is the process up), deliberately distinct from Railway's readiness probe below |
| Railway config | `railway.toml` | Dockerfile builder, `/health/ready` healthcheck (**readiness** — DB-aware, so a DB-less deploy is not marked healthy), `$PORT`/`HOST`/`RUST_LOG` env; `HYDRATE` + `OP_ALLOW_ANONYMOUS` set per-instance in the dashboard (both off by default) |
| Schema | `crates/op-db/migrations/0001_init.sql` | the 23 tables `op-db`'s queries target, derived from op-db's own SELECT/Row contract + the OpenProject baseline (`db/migrate/tables/*.rb`) |
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
3. **Choose the posture (per-instance, in the dashboard).** A bare deploy is
   a migrated, EMPTY, auth-required instance. For the reality-check/demo
   board set BOTH: `HYDRATE=1` (loads the kanban seed on boot) and
   `OP_ALLOW_ANONYMOUS=1` (serves `/api/v3` without credentials — anonymous
   READS AND WRITES; demo boxes only, never production).
4. **Verify.** `GET https://<service>/health/ready` → ready (this is the
   DB-aware route Railway's healthcheck uses; plain `/health` is a liveness
   string that stays green even with no database);
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
nullability. Column resolution was checked during authoring by running op-db's
real SELECT lists against the created schema (a manual pass — not yet a
committed test); `scripts/reality-check.sh` re-proves the applied schema +
seed on every run, including a **double-apply** pass that asserts idempotency
by execution.

## Schema evolution — APPEND-ONLY (read before touching migrations/)

`sqlx::migrate!` records a checksum per applied file in `_sqlx_migrations`
and verifies it on every boot; boot treats a migration failure as **fatal**.
Therefore, once any real deploy has applied `0001_init.sql`:

- **Never edit an applied migration.** An edit changes the checksum →
  `VersionMismatch` → every existing instance crash-loops on its next boot.
- **All growth goes in new files**: `crates/op-db/migrations/0002_<desc>.sql`,
  `0003_…`, strictly increasing. `CREATE TABLE IF NOT EXISTS` in `0001` does
  not help here — sqlx applies each file exactly once and checksums it.
- The transpile (op-generated, 721 classes) will eventually demand many more
  tables; those land as new numbered migrations, ideally **regenerated from
  the ruff-harvested ModelGraph** (`ruff_ruby_spo::extract_app_with_schema`
  already carries the columns) rather than hand-derived — tracked as owed
  follow-up from the render-bake/transpile arc.

## Evidence status (what is proven vs expected)

- **Proven locally**: the SQL applies + reseeds idempotently
  (`reality-check.sh`, double-apply); the host-built server binary migrates,
  seeds under `HYDRATE=1`, and serves the board over `/api/v3`; the release
  build compiles (`cargo build --release -p op-server`).
- **Expected, unverified until the first real deploy**: the Docker image
  build on Railway's builder, Railway's `DATABASE_URL` auto-injection on
  plugin add, healthcheck/`$PORT` routing. These match Railway's documented
  behavior but were not executable from the authoring environment — confirm
  on first deploy and note it here.
