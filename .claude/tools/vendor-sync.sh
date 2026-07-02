#!/usr/bin/env bash
# vendor-sync.sh — the "PR merged upstream, rebase" drill as one command.
#
# Sweeps every vendored mirror against its upstream main via
# raw.githubusercontent.com (the only reachable surface in scoped
# sessions: git relay / api.github.com 403 for non-scoped repos; raw
# 404s return a "404: Not Found" BODY, so http codes are checked, never
# file contents). Syncs changed files, then re-applies the recorded
# local deviations. Prints a report; does NOT test, commit, or push —
# run `cargo test --workspace` and commit yourself after reviewing.
#
# P0 accuracy fix (2026-07-03): the report is a snapshot-diff of FINAL
# committed byte content (sha256 before vs. after the whole pipeline —
# fetch + deviation re-apply), not an intermediate fetch-step diff. A
# file that a raw fetch overwrites and a deviation re-apply then
# restores to its prior bytes (patch churn) is correctly reported
# unchanged — the prior version of this script called that "SYNCED",
# true of the intermediate state but false of the thing that matters
# (does `git status` show anything to commit). Observed in the field:
# a #630 sync reported 6 files SYNCED; 4 were pure churn, only 2 were
# real (`git diff --stat` showed 0 lines for the 4, 193 for the 2).
#
# Recorded deviations (re-applied after sync, in this order):
#   1. RETIRED — lance-graph-contract/src/codegen_spine.rs
#      (C6 RouteBucketTyped merged upstream in lance-graph #632; guard
#      only, no patch step — see the check below)
#   2. ogar-class-view/Cargo.toml lance-graph-contract git→path redirect
#   3. ruff D-AR-3.5-column-stratum.diff (pending upstream: wishlist R1)
#
# Usage: .claude/tools/vendor-sync.sh   (from anywhere; resolves ROOT itself)
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

VENDOR_DIRS=(
  vendor/AdaWorldAPI-lance-graph/crates/lance-graph-contract
  vendor/AdaWorldAPI-OGAR/crates/ogar-vocab
  vendor/AdaWorldAPI-OGAR/crates/ogar-render-askama
  vendor/AdaWorldAPI-OGAR/crates/ogar-class-view
  vendor/AdaWorldAPI-ruff/crates/ruff_ruby_spo
  vendor/AdaWorldAPI-ruff/crates/ruff_spo_triplet
)

BEFORE_SNAP="$(mktemp)"
AFTER_SNAP="$(mktemp)"
trap 'rm -f "$BEFORE_SNAP" "$AFTER_SNAP" /tmp/vendor-sync.probe' EXIT

# snapshot <outfile> — `sha256sum  path` (sorted by path) for every *.rs
# under every VENDOR_DIRS entry. Absolute paths, so it's cwd-independent
# and directly `diff`-able between two runs.
snapshot() {
  local out="$1"
  : > "$out"
  for d in "${VENDOR_DIRS[@]}"; do
    find "$ROOT/$d" -name '*.rs' -type f 2>/dev/null
  done | sort | xargs -r sha256sum >> "$out" 2>/dev/null
}

snapshot "$BEFORE_SNAP"

# sweep <vendor-subdir> <github-repo> <repo-prefix> — fetch + overwrite
# where bytes differ from the raw upstream copy; discover new modules
# one level (re-run the tool if a fetched module itself declares
# children). No longer prints per-file "changed" — the final
# snapshot-diff below is the single source of truth for what actually
# changed.
sweep() {
  local vdir="$1" repo="$2" prefix="$3"
  cd "$ROOT/$vdir"
  while IFS= read -r f; do
    local code
    code=$(curl -sS -o /tmp/vendor-sync.probe -w '%{http_code}' \
      "https://raw.githubusercontent.com/$repo/main/$prefix/$f" 2>/dev/null)
    if [ "$code" = "200" ] && ! diff -q "$f" /tmp/vendor-sync.probe >/dev/null 2>&1; then
      cp /tmp/vendor-sync.probe "$f"
    fi
  done < <(find src -name '*.rs' 2>/dev/null | sort)
  for root in src/lib.rs $(find src -name 'mod.rs' 2>/dev/null); do
    [ -f "$root" ] || continue
    local dir; dir=$(dirname "$root")
    for m in $(grep -hoE '^(pub )?mod [a-z_0-9]+;' "$root" | awk '{print $NF}' | tr -d ';'); do
      if [ ! -f "$dir/$m.rs" ] && [ ! -f "$dir/$m/mod.rs" ]; then
        local mcode
        mcode=$(curl -sS -o "$dir/$m.rs" -w '%{http_code}' \
          "https://raw.githubusercontent.com/$repo/main/$prefix/$dir/$m.rs" 2>/dev/null)
        if [ "$mcode" != "200" ]; then
          rm -f "$dir/$m.rs"
          mkdir -p "$dir/$m"
          mcode=$(curl -sS -o "$dir/$m/mod.rs" -w '%{http_code}' \
            "https://raw.githubusercontent.com/$repo/main/$prefix/$dir/$m/mod.rs" 2>/dev/null)
          if [ "$mcode" != "200" ]; then
            rm -rf "$dir/$m"
            echo "!! new module $m declared in $root but not fetchable" >&2
          fi
        fi
      fi
    done
  done
}

# ── lance-graph-contract ──
sweep "vendor/AdaWorldAPI-lance-graph/crates/lance-graph-contract" \
      "AdaWorldAPI/lance-graph" "crates/lance-graph-contract"
# deviation 1 RETIRED (lance-graph #632 merged RouteBucketTyped upstream;
# diff archived as codegen_spine.diff.retired-632). Guard remains: if the
# symbol ever vanishes upstream again, fail loudly instead of silently
# breaking op-codegen-bucket.
if ! grep -q RouteBucketTyped "$ROOT/vendor/AdaWorldAPI-lance-graph/crates/lance-graph-contract/src/codegen_spine.rs"; then
  echo "!! RouteBucketTyped GONE from upstream spine — op-codegen-bucket will break; see codegen_spine.diff.retired-632" >&2
fi

# ── OGAR slice (the three vendored crates) ──
for c in ogar-vocab ogar-render-askama ogar-class-view; do
  sweep "vendor/AdaWorldAPI-OGAR/crates/$c" "AdaWorldAPI/OGAR" "crates/$c"
done
# deviation 2: class-view path redirect (sync may restore the git dep)
CV="$ROOT/vendor/AdaWorldAPI-OGAR/crates/ogar-class-view/Cargo.toml"
if grep -q 'lance-graph-contract = { git' "$CV"; then
  sed -i 's#lance-graph-contract = { git = "https://github.com/AdaWorldAPI/lance-graph", branch = "main" }#lance-graph-contract = { path = "../../../AdaWorldAPI-lance-graph/crates/lance-graph-contract" } # vendored-path deviation; upstream: git AdaWorldAPI/lance-graph@main#' "$CV"
  echo "class-view path deviation re-applied"
fi

# ── ruff slice ──
for c in ruff_ruby_spo ruff_spo_triplet; do
  sweep "vendor/AdaWorldAPI-ruff/crates/$c" "AdaWorldAPI/ruff" "crates/$c"
done
# NOTE: the ruff mirror carries D-AR-3.5 (column stratum) as local-first
# work — if upstream syncs OVERWRITE schema.rs/ir.rs/expand.rs/triple.rs
# before the patch merges upstream, re-apply D-AR-3.5-column-stratum.diff:
cd "$ROOT/vendor/AdaWorldAPI-ruff"
if [ ! -f crates/ruff_ruby_spo/src/schema.rs ] || ! grep -q column_not_null crates/ruff_spo_triplet/src/triple.rs; then
  # The diff creates schema.rs as a new-file hunk; if a stale copy exists
  # (upstream sync overwrote the OTHER targets only), remove it first or
  # patch appends a SECOND copy (E0753 doubled-file corruption).
  rm -f crates/ruff_ruby_spo/src/schema.rs
  if patch -p1 --forward < D-AR-3.5-column-stratum.diff >/dev/null 2>&1; then
    echo "D-AR-3.5 column-stratum diff re-applied"
  else
    echo "!! D-AR-3.5 DIFF CONFLICT — upstream may have merged/moved it; reconcile manually" >&2
  fi
fi

# ── final truth: snapshot-diff report (post fetch + deviation re-apply) ──
cd "$ROOT"
snapshot "$AFTER_SNAP"
echo "── vendor-sync report (final committed-byte diff) ──"
TOTAL_CHANGED=0
LOG_SUMMARY=""
for d in "${VENDOR_DIRS[@]}"; do
  mirror_changed=""
  for f in $(grep -oE "$ROOT/$d/[^ ]+\.rs" "$BEFORE_SNAP" "$AFTER_SNAP" | sort -u); do
    before=$(grep -F "  $f" "$BEFORE_SNAP" | awk '{print $1}')
    after=$(grep -F "  $f" "$AFTER_SNAP" | awk '{print $1}')
    if [ "$before" != "$after" ]; then
      rel="${f#$ROOT/$d/}"
      [ -z "$before" ] && rel="$rel(new)"
      mirror_changed="$mirror_changed $rel"
      TOTAL_CHANGED=$((TOTAL_CHANGED + 1))
    fi
  done
  if [ -n "$mirror_changed" ]; then
    echo "CHANGED $d:$mirror_changed"
    LOG_SUMMARY="$LOG_SUMMARY $d:$mirror_changed"
  else
    echo "clean   $d"
  fi
done
echo "── done: $TOTAL_CHANGED file(s) actually changed (post-deviation-reapply truth). Now: cargo test --workspace, review, commit."

# ── VENDOR-STATE.md telemetry (P0: deviation expiry tracking) ──
STATE_LOG="$ROOT/.claude/VENDOR-STATE.md"
if [ -f "$STATE_LOG" ]; then
  {
    echo ""
    if [ "$TOTAL_CHANGED" -eq 0 ]; then
      echo "- $(date -u +%Y-%m-%dT%H:%MZ) — sweep: clean, 0 files changed"
    else
      echo "- $(date -u +%Y-%m-%dT%H:%MZ) — sweep: $TOTAL_CHANGED file(s) changed —$LOG_SUMMARY"
    fi
  } >> "$STATE_LOG"
fi
