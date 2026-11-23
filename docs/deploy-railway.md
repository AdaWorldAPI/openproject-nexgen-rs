# Deploying to Railway

> **To deploy the current server, use the `sqlx` branch.** It freezes the server with
> its SQLx/PostgreSQL storage as it was at `20e8d95`, with its own `Dockerfile.sqlx` and
> a copy of this guide. In Railway, set the service's **Settings → Source** branch to
> `sqlx`. `main` is moving to the Rails polyfill + Quack in lance-graph, where
> SQLx/PostgreSQL is the oracle only; the steps below describe the SQLx deploy and are
> replaced when that migration lands.

The repository root holds everything Railway needs:

- `Dockerfile`: builds the `openproject-server` binary (crate `op-server`) in a Rust
  1.99.0 image and runs it in a slim Debian image.
- `railway.toml`: tells Railway to use that Dockerfile and to treat `/health/ready` as
  the readiness check.

On start the server connects to PostgreSQL and applies the schema migrations in
`crates/op-db/migrations`. It then serves the board at `/`, the rendered work package
and project pages, and the OpenProject `/api/v3` JSON API.

> **Interim storage.** This guide deploys the server as it is today, to see the
> current status before the migration. Today it persists through SQLx to
> PostgreSQL. That is not the target: SQLx/PostgreSQL is to be the oracle only.
> The target maps the models onto the Rails polyfill and reads them through Quack
> in lance-graph, the same way zammad-rs does (`rails-polyfill`, `quack-shadow`).
> When that lands, this guide changes.

## 1. Create the project

1. In Railway: **New Project → Deploy from GitHub repo → `AdaWorldAPI/openproject-nexgen-rs`**.
   Railway reads `railway.toml` and builds with the Dockerfile. Change no build
   settings.
2. In the same project: **New → Database → PostgreSQL**.
3. Connect the database to the service. Open the service's **Variables** tab and add
   a reference variable:

   ```
   DATABASE_URL = ${{ Postgres.DATABASE_URL }}
   ```

   (`Postgres` is the database service's name; use its actual name if you renamed it.)
   Without `DATABASE_URL`, the server builds a URL from `PGHOST`, `PGPORT`, `PGUSER`,
   `PGPASSWORD` and `PGDATABASE`, if those are set.

The build needs no credentials. The git dependencies (OGAR, a2ui-rs, lance-graph,
ndarray, elliptic-curves, password-hashes) are public, and cargo fetches them
anonymously. Never put a token in the Dockerfile or in build variables.

## 2. Networking

- **Settings → Networking → Generate Domain** gives the service a public
  `*.up.railway.app` address.
- Leave the target port unset. Railway injects `PORT`, and the server binds
  `0.0.0.0:$PORT` (`resolve_bind_addr` in `crates/op-server/src/main.rs`). Setting a
  fixed `PORT` variable yourself would break the routing.
- The API's links (HAL `_links`) use `PUBLIC_URL` if it is set, otherwise
  `https://$RAILWAY_PUBLIC_DOMAIN`, which Railway injects. Set `PUBLIC_URL` only for a
  custom domain, e.g. `PUBLIC_URL=https://openproject.example.com`.

## 3. Variables

Required: only `DATABASE_URL` (step 1.3).

Recommended:

| Variable | Purpose |
|---|---|
| `OP_CSRF_SECRET` | Secret for the edit forms' CSRF tokens. If unset, a random secret is made at each start: forms opened before a restart stop working, and replicas don't share it. Set it to a long random string. |
| `RUST_LOG` | Log filter. `railway.toml` sets `info,op_server=debug`. |
| `TZ` | Time zone, e.g. `Europe/Berlin`. |

Demo switches. Both are off by default. Set them only on a throwaway instance, never
on a database with real data:

| Variable | Effect |
|---|---|
| `HYDRATE=1` | Loads the demo kanban board on start (`crates/op-db/seeds/kanban_seed.sql`: 3 projects, 5 statuses, about 40 work packages; running it twice is harmless). |
| `OP_ALLOW_ANONYMOUS=1` | Serves `/api/v3` reads without authentication. API writes still need an authenticated user. |

Optional sign-in through Zitadel/OIDC. This protects the rendered pages; `/health*`,
`/metrics*` and `/api/v3` are not covered. If `OP_SIGNIN_OIDC_ISSUER` is set, the
following are required, and the server refuses to start if one is missing:

| Variable | Value |
|---|---|
| `OP_SIGNIN_OIDC_ISSUER` | the issuer's https URL |
| `OP_SIGNIN_OIDC_CLIENT_ID` | the client id registered at the provider |
| `OP_SIGNIN_OIDC_REDIRECT_URL` | `https://<your domain>/auth/callback`, exactly as registered |
| `OP_SIGNIN_MAP_TEXT` (or `OP_SIGNIN_MAP` as a file path) | the sign-in map from provider roles to OpenProject roles; see `crates/op-server/src/signin_map.rs` |
| `OP_SIGNIN_OIDC_CLIENT_SECRET` | only for a confidential client |

Store secrets as Railway variables (encrypted, and injected at runtime only), never in
`railway.toml` or the repository.

## 4. Deploy and check

Push to `main`, or click **Deploy**. The first build compiles the whole dependency
tree and takes a while; later builds reuse Railway's layer cache when nothing in the
build context changed.

Railway marks the deploy healthy once `GET /health/ready` returns 200. That route runs
`SELECT 1` against the database. Without a database it returns 503, so a deploy with
a missing or wrong `DATABASE_URL` fails the health check instead of going live.

After the deploy:

```sh
curl https://<domain>/health          # "OK": the process is up
curl https://<domain>/health/ready    # JSON, database "Connected"
curl https://<domain>/api/v3          # API root
```

Then open `https://<domain>/` for the board. A fresh database without `HYDRATE=1` is
migrated but empty.

## 5. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| Health check fails, log says `Failed to connect to database ... Running without database` | `DATABASE_URL` is missing or wrong. Add the reference variable from step 1.3 and redeploy. |
| Log says `Migration failed`, container exits | The database rejected a migration. The server refuses to serve a half-migrated schema; read the error above that line. |
| Public URL returns 502 | A fixed port is set. Remove any `PORT` variable and any target port in Networking. |
| Log says `invalid PORT env var` | `PORT` was set by hand to something that is not a port. Remove it. |
| Start fails naming an `OP_SIGNIN_*` variable | `OP_SIGNIN_OIDC_ISSUER` is set but sign-in is half configured. Set the missing variable, or remove the issuer to turn sign-in off. |
| Build fails fetching a git dependency | One of the AdaWorldAPI dependency repos is not public. Make it public; do not add a token to the build. |

## Local run with the same image

```sh
docker build -t openproject-nexgen .
docker run --rm -p 8080:8080 \
  -e DATABASE_URL=postgres://user:pass@host.docker.internal:5432/openproject \
  openproject-nexgen
```

`docker-compose.yml` starts the server and a PostgreSQL 17 together:
`docker compose up --build`.
