#!/usr/bin/env bash
# nav-crawl.sh — LIVE connectivity crawler ("Mario-World-editor guarantee"):
# boots the real server against an ephemeral Postgres (same bring-up as
# reality-check.sh), then BFS-crawls the rendered HTML from `/`, proving
# every internal lane is reachable and returns 200, and stays connected.
#
#   ./scripts/nav-crawl.sh            # crawl + report + teardown
#   KEEP=1 ./scripts/nav-crawl.sh     # leave PG + server running, print URLs
#
# Requires: a local postgres install (initdb/pg_ctl/psql), a built
# openproject-server binary (this script builds it), and curl.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MIGRATIONS="$REPO/crates/op-db/migrations"
SEED="$REPO/crates/op-db/seeds/kanban_seed.sql"
NAV_RS="$REPO/crates/op-server/src/nav.rs"

PGBIN="$(ls -d /usr/lib/postgresql/*/bin 2>/dev/null | sort -V | tail -1 || true)"
[ -n "$PGBIN" ] || PGBIN="$(dirname "$(command -v initdb)")"
PGDATA="${PGDATA:-/tmp/oprc-nav-pg}"
SOCK="${SOCK:-/tmp/oprc-nav-sock}"
PORT="${PGPORT:-5433}"
DB="openproject_rc"
APP_PORT="${APP_PORT:-8910}"
BASE="http://localhost:$APP_PORT"
MAX_REQUESTS=500

# Run pg tooling as an unprivileged owner when we are root (pg refuses root).
if [ "$(id -u)" = "0" ]; then
  if ! id postgres >/dev/null 2>&1; then
    echo "ERROR: running as root but no 'postgres' system user exists." >&2
    echo "       Run this script as a non-root user, or create one:  useradd -r postgres" >&2
    exit 1
  fi
  AS="runuser -u postgres --"; OWNER=postgres
else
  AS=""; OWNER="$(id -un)"
fi

SERVER_PID=""

cleanup() {
  if [ "${KEEP:-0}" != "1" ]; then
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" >/dev/null 2>&1 || true
    $AS "$PGBIN/pg_ctl" -D "$PGDATA" -m immediate stop >/dev/null 2>&1 || true
    rm -rf "$PGDATA" "$SOCK"
  fi
}
trap cleanup EXIT

echo "== ephemeral postgres =="
rm -rf "$PGDATA" "$SOCK"; mkdir -p "$PGDATA" "$SOCK"
[ "$(id -u)" = "0" ] && chown -R "$OWNER":"$OWNER" "$PGDATA" "$SOCK"
$AS "$PGBIN/initdb" -D "$PGDATA" -U postgres --auth=trust >/tmp/oprc-nav-initdb.log 2>&1
$AS "$PGBIN/pg_ctl" -D "$PGDATA" \
    -o "-k $SOCK -p $PORT -c listen_addresses=''" -l /tmp/oprc-nav-pg.log -w start
$AS "$PGBIN/createdb" -h "$SOCK" -p "$PORT" -U postgres "$DB"
PSQL() { $AS "$PGBIN/psql" -h "$SOCK" -p "$PORT" -U postgres -d "$DB" -v ON_ERROR_STOP=1 "$@"; }

echo "== migrations =="
for f in "$MIGRATIONS"/*.sql; do echo "  apply $(basename "$f")"; PSQL -q -f "$f"; done

echo "== seed =="
if [ -f "$SEED" ]; then echo "  apply $(basename "$SEED")"; PSQL -q -f "$SEED"; else echo "  (no seed file)"; fi

echo "== build =="
cargo build -p op-server 2>&1 | tail -20

echo "== boot server =="
BIN="$REPO/target/debug/openproject-server"
[ -x "$BIN" ] || { echo "ERROR: $BIN not found after build" >&2; exit 1; }

DATABASE_URL="postgres://postgres@localhost:$PORT/$DB?host=$SOCK" \
HYDRATE=1 OP_ALLOW_ANONYMOUS=1 PORT="$APP_PORT" RUST_LOG=warn \
  "$BIN" >/tmp/oprc-nav-server.log 2>&1 &
SERVER_PID=$!

echo "  waiting for /health/ready ..."
READY=0
for _ in $(seq 1 60); do
  code="$(curl -s -o /dev/null -w '%{http_code}' "$BASE/health/ready" || true)"
  if [ "$code" = "200" ]; then READY=1; break; fi
  sleep 0.5
done
if [ "$READY" != "1" ]; then
  echo "ERROR: server did not become ready" >&2
  tail -50 /tmp/oprc-nav-server.log >&2 || true
  exit 1
fi
echo "  ready (pid=$SERVER_PID)"

# ---------------------------------------------------------------------------
# BFS crawl
# ---------------------------------------------------------------------------

shape_of() {
  # Collapse numeric ids to :id for reporting/dedup-by-shape purposes.
  echo "$1" | sed -E 's#/([0-9]+)(/|$)#/:id\2#g'
}

is_internal_followable() {
  # $1 = href path (already stripped to path-only, starts with /)
  case "$1" in
    /api/v3/*) return 1 ;;
    /health*)  return 1 ;;
    /metrics*) return 1 ;;
    *) return 0 ;;
  esac
}

declare -A SEEN=()
declare -A STATUS_OF=()
declare -A LINK_SOURCE=()
QUEUE=("/")
SEEN["/"]=1
BROKEN=()
REQUESTS=0

while [ "${#QUEUE[@]}" -gt 0 ]; do
  path="${QUEUE[0]}"
  QUEUE=("${QUEUE[@]:1}")

  REQUESTS=$((REQUESTS + 1))
  if [ "$REQUESTS" -gt "$MAX_REQUESTS" ]; then
    echo "  (request cap $MAX_REQUESTS reached, stopping crawl)" >&2
    break
  fi

  body_file="$(mktemp)"
  code="$(curl -s -o "$body_file" -w '%{http_code}' "$BASE$path" || echo "000")"
  STATUS_OF["$path"]="$code"

  if [ "${code:0:1}" != "2" ] && [ "${code:0:1}" != "3" ]; then
    BROKEN+=("/ -> $path -> $code")
  fi

  if [ "${code:0:1}" = "2" ]; then
    while IFS= read -r href; do
      [ -n "$href" ] || continue
      # strip href="..." to the path only
      p="${href#href=\"}"; p="${p%\"}"
      case "$p" in
        http*|//*|mailto:*|javascript:*|\#*) continue ;;
      esac
      p="${p%%#*}"
      [ -n "$p" ] || continue
      is_internal_followable "$p" || continue
      if [ -z "${SEEN[$p]:-}" ]; then
        SEEN["$p"]=1
        QUEUE+=("$p")
        # track the linking source for broken-link reporting later
        LINK_SOURCE["$p"]="$path"
      fi
    done < <(grep -oE 'href="/[^"]*"' "$body_file" || true)
  fi
  rm -f "$body_file"
done

# Re-check broken link sources with proper attribution (source -> target -> status)
BROKEN_ATTR=()
for path in "${!STATUS_OF[@]}"; do
  code="${STATUS_OF[$path]}"
  if [ "${code:0:1}" != "2" ] && [ "${code:0:1}" != "3" ]; then
    src="${LINK_SOURCE[$path]:-/}"
    BROKEN_ATTR+=("$src -> $path -> $code")
  fi
done

echo ""
echo "== reachable =="
declare -A SHAPE_COUNT=()
declare -A SHAPE_STATUS=()
for path in "${!STATUS_OF[@]}"; do
  s="$(shape_of "$path")"
  SHAPE_COUNT["$s"]=$(( ${SHAPE_COUNT[$s]:-0} + 1 ))
  SHAPE_STATUS["$s"]="${STATUS_OF[$path]}"
done
for s in $(printf '%s\n' "${!SHAPE_COUNT[@]}" | sort); do
  echo "  $s  (n=${SHAPE_COUNT[$s]}, last_status=${SHAPE_STATUS[$s]})"
done

echo ""
echo "== broken links =="
if [ "${#BROKEN_ATTR[@]}" -eq 0 ]; then
  echo "  (none)"
else
  printf '  %s\n' "${BROKEN_ATTR[@]}"
fi

echo ""
echo "== dead lanes (declared) =="
grep -A20 'NOT_YET_NAVIGABLE: &\[&str\] = &\[' "$NAV_RS" | sed -n '2,/\];/p' | grep -oE '"[A-Za-z]+"' | tr -d '"' | while read -r t; do
  echo "  $t (association target, no page yet — declared debt)"
done

EXPECTED_SHAPES=("/" "/projects" "/projects/:id" "/projects/:id/edit" "/work_packages/:id" "/work_packages/:id/edit")
MISSING=()
for e in "${EXPECTED_SHAPES[@]}"; do
  if [ -z "${SHAPE_COUNT[$e]:-}" ]; then
    MISSING+=("$e")
  fi
done

echo ""
echo "== verdict =="
if [ "${#BROKEN_ATTR[@]}" -eq 0 ] && [ "${#MISSING[@]}" -eq 0 ]; then
  echo "  CONNECTED — every internal link 200/3xx, all expected route shapes reached"
else
  echo "  BROKEN"
  [ "${#BROKEN_ATTR[@]}" -gt 0 ] && { echo "  broken links:"; printf '    %s\n' "${BROKEN_ATTR[@]}"; }
  [ "${#MISSING[@]}" -gt 0 ] && { echo "  missing expected route shapes:"; printf '    %s\n' "${MISSING[@]}"; }
fi

if [ "${KEEP:-0}" = "1" ]; then
  echo ""
  echo "== kept running =="
  echo "  DATABASE_URL=postgres://postgres@localhost:$PORT/$DB?host=$SOCK"
  echo "  APP_URL=$BASE"
  echo "  server pid=$SERVER_PID"
fi
