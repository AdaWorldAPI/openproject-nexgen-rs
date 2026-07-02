# 2026-07-02 — classid canon:custom half-order flip (canon HIGH / prefix LOW)

The 2026-06-30 convergence assessment's classid examples use the PRE-FLIP
order and are historical. As of 2026-07-02 (operator trigger, lance-graph
`classid-canon-custom-flip-v1` P1 + OGAR lockstep flip):

- `classid : u32 = [hi u16: CANON concept][lo u16: APP/render prefix]`
- `openproject:WorkPackage` → `0x0102_0001` (was `0x0001_0102`)
- `redmine:Issue` → `0x0102_0007` (was `0x0007_0102`)
- `APP_PREFIX` VALUES unchanged (OP `0x0001`, Redmine `0x0007`) — only the
  position moved to the LOW half.
- `op_canon::app` re-exports flipped in lockstep with `ogar_vocab::app`
  (this repo owns no bit math); literal-pinned tests/doctests updated.
- Pre-flip persisted ids resolve upstream via read-only legacy registry
  aliases (mint-forward; retirement gated on corpus proof).

This branch's op-canon changes are gated on the OGAR flip PR merging to
OGAR main (the git dep tracks `branch = "main"`).
