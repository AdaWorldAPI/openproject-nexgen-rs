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
# Recorded deviations (re-applied after sync, in this order):
#   1. lance-graph-contract/src/codegen_spine.rs ← codegen_spine.diff
#      (C6 RouteBucketTyped — absent upstream; op-codegen-bucket needs it)
#   2. ogar-class-view/Cargo.toml lance-graph-contract git→path redirect
#
# Usage: .claude/tools/vendor-sync.sh   (from the repo root)
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
CHANGED_TOTAL=0

sweep() { # sweep <vendor-subdir> <github-repo> <repo-prefix>
  local vdir="$1" repo="$2" prefix="$3" changed=""
  cd "$ROOT/$vdir"
  while IFS= read -r f; do
    local code
    code=$(curl -sS -o /tmp/vendor-sync.probe -w '%{http_code}' \
      "https://raw.githubusercontent.com/$repo/main/$prefix/$f" 2>/dev/null)
    if [ "$code" = "200" ] && ! diff -q "$f" /tmp/vendor-sync.probe >/dev/null 2>&1; then
      cp /tmp/vendor-sync.probe "$f"
      changed="$changed $f"
      CHANGED_TOTAL=$((CHANGED_TOTAL + 1))
    fi
  done < <(find src -name '*.rs' 2>/dev/null | sort)
  # New-module discovery: a synced lib.rs (or mod.rs) may declare modules
  # that don't exist locally yet — fetch them (one level; re-run the tool
  # if a fetched module itself declares children).
  for root in src/lib.rs $(find src -name 'mod.rs' 2>/dev/null); do
    [ -f "$root" ] || continue
    local dir; dir=$(dirname "$root")
    for m in $(grep -hoE '^(pub )?mod [a-z_0-9]+;' "$root" | awk '{print $NF}' | tr -d ';'); do
      if [ ! -f "$dir/$m.rs" ] && [ ! -f "$dir/$m/mod.rs" ]; then
        local mcode
        mcode=$(curl -sS -o "$dir/$m.rs" -w '%{http_code}'           "https://raw.githubusercontent.com/$repo/main/$prefix/$dir/$m.rs" 2>/dev/null)
        if [ "$mcode" = "200" ]; then
          changed="$changed $dir/$m.rs(new)"; CHANGED_TOTAL=$((CHANGED_TOTAL + 1))
        else
          rm -f "$dir/$m.rs"
          mkdir -p "$dir/$m"
          mcode=$(curl -sS -o "$dir/$m/mod.rs" -w '%{http_code}'             "https://raw.githubusercontent.com/$repo/main/$prefix/$dir/$m/mod.rs" 2>/dev/null)
          if [ "$mcode" = "200" ]; then
            changed="$changed $dir/$m/mod.rs(new)"; CHANGED_TOTAL=$((CHANGED_TOTAL + 1))
          else
            rm -rf "$dir/$m"; echo "!! new module $m declared in $root but not fetchable" >&2
          fi
        fi
      fi
    done
  done
  [ -n "$changed" ] && echo "SYNCED $vdir:$changed" || echo "clean  $vdir"
}

# ── lance-graph-contract ──
sweep "vendor/AdaWorldAPI-lance-graph/crates/lance-graph-contract" \
      "AdaWorldAPI/lance-graph" "crates/lance-graph-contract"
# deviation 1: C6 spine diff (idempotent: skip when already applied)
cd "$ROOT/vendor/AdaWorldAPI-lance-graph"
if ! grep -q RouteBucketTyped crates/lance-graph-contract/src/codegen_spine.rs; then
  if patch -p1 --forward < codegen_spine.diff >/dev/null 2>&1; then
    echo "C6 codegen_spine.diff re-applied"
  else
    echo "!! C6 DIFF CONFLICT — rebase codegen_spine.diff manually" >&2
  fi
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

echo "── done: $CHANGED_TOTAL file(s) synced. Now: cargo test --workspace, review, commit."
