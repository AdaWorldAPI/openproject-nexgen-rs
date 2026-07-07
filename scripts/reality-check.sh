#!/usr/bin/env bash
# reality-check.sh — spin an ephemeral Postgres, apply the op-db schema
# migrations + the kanban reality-check seed, and print the resulting board.
#
# This is the local proxy for the Railway deploy: same schema (crates/op-db/
# migrations), same seed (crates/op-db/seeds/kanban_seed.sql), same queries
# op-db issues at runtime — but against a throwaway cluster under /tmp, torn
# down on exit. No network, no persistent state.
#
#   ./scripts/reality-check.sh            # schema + seed + board summary
#   KEEP=1 ./scripts/reality-check.sh     # leave the cluster running, print DATABASE_URL
#
# Requires: a local postgres install (initdb/pg_ctl/psql) and a non-root
# owner (the script uses the `postgres` system user via runuser when root).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MIGRATIONS="$REPO/crates/op-db/migrations"
SEED="$REPO/crates/op-db/seeds/kanban_seed.sql"

PGBIN="$(ls -d /usr/lib/postgresql/*/bin 2>/dev/null | sort -V | tail -1 || true)"
[ -n "$PGBIN" ] || PGBIN="$(dirname "$(command -v initdb)")"
PGDATA="${PGDATA:-/tmp/oprc-pg}"
SOCK="${SOCK:-/tmp/oprc-sock}"
PORT="${PGPORT:-5433}"
DB="openproject_rc"

# Run pg tooling as an unprivileged owner when we are root (pg refuses root).
if [ "$(id -u)" = "0" ]; then AS="runuser -u postgres --"; OWNER=postgres; else AS=""; OWNER="$(id -un)"; fi

cleanup() {
  if [ "${KEEP:-0}" != "1" ]; then
    $AS "$PGBIN/pg_ctl" -D "$PGDATA" -m immediate stop >/dev/null 2>&1 || true
    rm -rf "$PGDATA" "$SOCK"
  fi
}
trap cleanup EXIT

echo "== ephemeral postgres =="
rm -rf "$PGDATA" "$SOCK"; mkdir -p "$PGDATA" "$SOCK"
[ "$(id -u)" = "0" ] && chown -R "$OWNER":"$OWNER" "$PGDATA" "$SOCK"
$AS "$PGBIN/initdb" -D "$PGDATA" -U postgres --auth=trust >/tmp/oprc-initdb.log 2>&1
$AS "$PGBIN/pg_ctl" -D "$PGDATA" \
    -o "-k $SOCK -p $PORT -c listen_addresses=''" -l /tmp/oprc-pg.log -w start
$AS "$PGBIN/createdb" -h "$SOCK" -p "$PORT" -U postgres "$DB"
PSQL() { $AS "$PGBIN/psql" -h "$SOCK" -p "$PORT" -U postgres -d "$DB" -v ON_ERROR_STOP=1 "$@"; }

echo "== migrations =="
for f in "$MIGRATIONS"/*.sql; do echo "  apply $(basename "$f")"; PSQL -q -f "$f"; done

echo "== seed =="
if [ -f "$SEED" ]; then echo "  apply $(basename "$SEED")"; PSQL -q -f "$SEED"; else echo "  (no seed file)"; fi

echo "== reality check: the kanban board =="
PSQL -P pager=off -c "
  SELECT p.name AS project, s.name AS status, count(w.id) AS cards
  FROM work_packages w
  JOIN projects p ON p.id = w.project_id
  JOIN statuses s ON s.id = w.status_id
  GROUP BY p.name, s.position, s.name
  ORDER BY p.name, s.position;"

echo "== totals =="
PSQL -t -P pager=off -c "
  SELECT 'projects=' || (SELECT count(*) FROM projects)
      || ' statuses=' || (SELECT count(*) FROM statuses)
      || ' types=' || (SELECT count(*) FROM types)
      || ' users=' || (SELECT count(*) FROM users)
      || ' work_packages=' || (SELECT count(*) FROM work_packages);"

if [ "${KEEP:-0}" = "1" ]; then
  echo "== cluster kept =="
  echo "DATABASE_URL=postgres://postgres@localhost:$PORT/$DB?host=$SOCK"
fi
