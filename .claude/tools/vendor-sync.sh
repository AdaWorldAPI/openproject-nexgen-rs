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
# deviation 2 + 4: redirect the lance-graph-contract git dep to our
# vendored path in BOTH crates that carry it — ogar-class-view (D2) and,
# since the 2026-07-05 rebase, ogar-render-askama (D4, needed by the new
# rust_class.rs = ClassView×FieldMask→struct transpiler). A raw fetch
# restores the upstream git dep; redirect it or the offline slice can't
# resolve. `#` in the replacement forbids sed's `s#...#`, so use perl.
for cv in ogar-class-view ogar-render-askama; do
  CV="$ROOT/vendor/AdaWorldAPI-OGAR/crates/$cv/Cargo.toml"
  if grep -q 'lance-graph-contract = { git' "$CV" 2>/dev/null; then
    perl -0pi -e 's{lance-graph-contract = \{ git = "https://github.com/AdaWorldAPI/lance-graph", branch = "main" \}}{lance-graph-contract = { path = "../../../AdaWorldAPI-lance-graph/crates/lance-graph-contract" } # vendored-path deviation; upstream: git AdaWorldAPI/lance-graph\@main}' "$CV"
    echo "$cv lance-graph-contract path deviation re-applied"
  fi
done

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

# ── final truth: GIT is the ground truth for what changed. ──
# The prior hand-rolled sha256 snapshot-diff report LIED: on 2026-07-05 it
# printed "clean / 0 files changed" for a sweep that git showed moving 12
# files across all three mirrors (+ new files + a D-AR-3.5 patch-fuzz
# .orig). An instrument that can silently under-report is worse than none —
# so the report is now `git status`, which cannot misrepresent the working
# tree. (This is the "instruments must not lie" rule, applied to the tool
# that broke it.)
cd "$ROOT"
echo "── vendor-sync report (git working-tree truth) ──"
CHANGED_LINES="$(git status --porcelain -- "${VENDOR_DIRS[@]}" 2>/dev/null)"
TOTAL_CHANGED=$(printf '%s\n' "$CHANGED_LINES" | grep -c .)
if [ "$TOTAL_CHANGED" -eq 0 ]; then
  echo "clean — 0 files changed across all mirrors"
else
  printf '%s\n' "$CHANGED_LINES"
  echo "── $TOTAL_CHANGED path(s) changed (git truth). Watch for .rej/.orig (patch fuzz), then: cargo test --workspace, review, commit."
fi

# ── VENDOR-STATE.md telemetry (P0: deviation expiry tracking) ──
STATE_LOG="$ROOT/.claude/VENDOR-STATE.md"
if [ -f "$STATE_LOG" ]; then
  {
    echo ""
    echo "- $(date -u +%Y-%m-%dT%H:%MZ) — sweep: $TOTAL_CHANGED path(s) changed (git truth)"
  } >> "$STATE_LOG"
fi
