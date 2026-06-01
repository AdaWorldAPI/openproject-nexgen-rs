#!/usr/bin/env bash
# .claude/hooks/block-grep-sed-head-tail.sh
#
# PreToolUse hook for Claude Code: blocks grep/sed/head/tail in Bash tool calls.
# These tools produce convincing-but-wrong context for agents — partial file
# reads, binary-file garbage, regex silently failing, etc.
#
# The reason strings are deliberately specific so the agent knows what to use
# instead, not just that something was blocked.
#
# This hook works even with --dangerously-skip-permissions.

set -euo pipefail

# Read the PreToolUse JSON payload from stdin
payload="$(cat)"

# Extract the bash command (jq is available in standard Claude Code environment)
cmd="$(echo "$payload" | jq -r '.tool_input.command // empty')"

# If no command (e.g. different tool), allow silently
if [[ -z "$cmd" ]]; then
  echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}'
  exit 0
fi

# Strip leading whitespace
trimmed="$(echo "$cmd" | awk '{$1=$1; print}')"

deny_with_reason() {
  local reason="$1"
  cat <<EOF
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "$reason"
  }
}
EOF
  exit 0
}

# Pattern matches: bare invocation OR pipeline position OR subshell position.
# We use awk word-boundary detection since we cannot use grep here.

# Check for grep (allow rg explicitly)
if echo "$trimmed" | awk 'BEGIN{rc=1} /(^|[|;&` (])grep([ ]|$)/{rc=0} END{exit rc}'; then
  deny_with_reason "grep is forbidden in this repo. Use the view tool to read files (with view_range for specific lines), or rg (ripgrep) for cross-file text search. grep on binary, columnar, or packed-byte files (Lance manifests, BGZ17 blocks, SoA layouts) produces garbage. For Rust symbol search, use cargo check / cargo clippy / rust-analyzer, not text search. See .claude/skills/sprint-S0-SKILL.md sections 'Hard rules' and 'Negative examples'."
fi

# Check for sed
if echo "$trimmed" | awk 'BEGIN{rc=1} /(^|[|;&` (])sed([ ]|$)/{rc=0} END{exit rc}'; then
  deny_with_reason "sed is forbidden in this repo. Use the str_replace tool for file edits (preserves whitespace, fails loudly on bad matches, reports actual diffs). sed silently no-ops on bad regex and loses error context. For config-value extraction, use the language's config library (Application.fetch_env! in Elixir, @Value in Spring, etc.)."
fi

# Check for head
if echo "$trimmed" | awk 'BEGIN{rc=1} /(^|[|;&` (])head([ ]|$)/{rc=0} END{exit rc}'; then
  deny_with_reason "head is forbidden in this repo. Use the view tool with explicit view_range — showing partial content via head leads to false confidence about the rest of the file. If you need only the first N lines, state that explicitly and use view with view_range=[1, N]. If you need to skim a file you have not seen, view the whole file with no range first to see the structure."
fi

# Check for tail
if echo "$trimmed" | awk 'BEGIN{rc=1} /(^|[|;&` (])tail([ ]|$)/{rc=0} END{exit rc}'; then
  deny_with_reason "tail is forbidden in this repo. Use the view tool with explicit view_range — if you need the end of a file, first determine its line count, then view with view_range=[N, -1]. For log following in a running service, use docker logs -f or the Railway log stream, not tail. For tests, write structured assertions, not output-tailing."
fi

# All checks passed — silent allow
echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}'
exit 0
