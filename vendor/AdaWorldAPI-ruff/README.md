# vendor/AdaWorldAPI-ruff/

**Source-mirror** of the changes that close Sprint C1 Gap 1 (seaorm→sqlx target)
in [`AdaWorldAPI/ruff`](https://github.com/AdaWorldAPI/ruff). Paths under
`crates/ruff_python_dto_check/` match the upstream layout exactly, so the
files here can be reviewed in the nexgen branch and applied to upstream ruff
without any path translation.

The corresponding apply-bundle (`sprint-c1-gap1.patch` +
`sprint-c1-gap1-final.tar.gz`) lives in
`../../calibration/sprint-c1-gap1-ruff/`.

## Why vendored here

`AdaWorldAPI/ruff` is outside this session's MCP push scope. The workbench at
`/home/user/ruff-clone` can read+clone the public repo but cannot locally
commit (the env's commit-signing server only accepts local-proxy origins).
Vendoring the changed files into nexgen makes the actual code reviewable on
the `claude/beautiful-gates-dJo0u` branch; the patch in
`calibration/sprint-c1-gap1-ruff/` is the application form.

## Contents (16 files in the patch; 14 files here + 1 diff)

### Net-new (12 files, copied verbatim)

| Path | Purpose |
|---|---|
| `crates/ruff_python_dto_check/src/codegen/sqlx_emit/mod.rs` | Module aggregate |
| `crates/ruff_python_dto_check/src/codegen/sqlx_emit/list_for_tenant.rs` | `emit_list_for_tenant_sqlx` |
| `crates/ruff_python_dto_check/src/codegen/sqlx_emit/detail_for_tenant.rs` | `emit_detail_for_tenant_sqlx` |
| `crates/ruff_python_dto_check/src/codegen/sqlx_emit/soft_delete.rs` | `emit_soft_delete_sqlx` |
| `crates/ruff_python_dto_check/src/codegen/sqlx_emit/toggle_bool_field.rs` | `emit_toggle_bool_field_sqlx` |
| `crates/ruff_python_dto_check/tests/sqlx_emit_test.rs` | 4 byte-exact golden-roundtrip tests |
| `crates/ruff_python_dto_check/tests/sqlx_target_spec_test.rs` | 4 spec + back-compat tests |
| `crates/ruff_python_dto_check/tests/golden/codegen/sqlx/expected/list_for_tenant.rs` | Golden output (the spec) |
| `crates/ruff_python_dto_check/tests/golden/codegen/sqlx/expected/detail_for_tenant.rs` | Golden output |
| `crates/ruff_python_dto_check/tests/golden/codegen/sqlx/expected/soft_delete.rs` | Golden output |
| `crates/ruff_python_dto_check/tests/golden/codegen/sqlx/expected/toggle_bool_field.rs` | Golden output |
| `crates/ruff_python_dto_check/examples/openproject-axum-sqlx.toml` | Declarative spec example (23 mappings) |
| `crates/ruff_python_dto_check/SQLX-TARGET.md` | Public docs |

### Modified (1 file vendored full + 1 file as diff)

| Path | Form here | Why |
|---|---|---|
| `crates/ruff_python_dto_check/src/codegen/target.rs` | Full modified file (423 lines) | The spec is the source-of-truth artifact for review |
| `codegen-mod-rs.diff` | Hunk only (47 lines) | The base file is 1545 lines, ~98% unchanged — the diff is the meaningful unit |

## The codegen/mod.rs delta (also in `codegen-mod-rs.diff`)

```diff
@@ pub mod columns;
 pub mod dto;
 pub mod jinja;
 pub mod pipeline;
+pub mod sqlx_emit;
 pub mod target;

 use std::fmt::Write as _;

-use crate::codegen::target::{ModelMapping, TargetSpec};
+use crate::codegen::target::{ModelMapping, Orm, TargetSpec};
```

```diff
@@ /// Emit target source for a single contract against a target spec.
 pub fn emit(contract: &RouteContract, spec: &TargetSpec) -> Emitted {
+    // sqlx target: route the four implemented kinds to `sqlx_emit` before
+    // the seaorm dispatch. The `can_emit` gate keeps kinds that aren't in
+    // the spec's `emit_kinds` list on the stub path (honest coverage).
+    // Kinds not implemented by sqlx_emit at all fall through — see the
+    // module docs for that footgun.
+    if spec.orm == Orm::Sqlx && spec.can_emit(contract.handler_kind) {
+        match contract.handler_kind {
+            HandlerKind::ListForTenant => {
+                return sqlx_emit::list_for_tenant::emit_list_for_tenant_sqlx(contract, spec);
+            }
+            HandlerKind::DetailForTenant => {
+                return sqlx_emit::detail_for_tenant::emit_detail_for_tenant_sqlx(contract, spec);
+            }
+            HandlerKind::SoftDelete => {
+                return sqlx_emit::soft_delete::emit_soft_delete_sqlx(contract, spec);
+            }
+            HandlerKind::ToggleBoolField => {
+                return sqlx_emit::toggle_bool_field::emit_toggle_bool_field_sqlx(contract, spec);
+            }
+            _ => {}
+        }
+    }
     let recipe = KindRecipe::for_kind(contract.handler_kind, spec);
     match recipe {
```

That's the entire change to `codegen/mod.rs`. 28 added lines, 0 removed, in a
1545-line file.

## Verification

From the workbench (`/home/user/ruff-clone` at upstream SHA `5179bc00`):

```
cargo check -p ruff_python_dto_check --tests
# Finished `dev` profile in 0.74s

cargo test -p ruff_python_dto_check
# 17 + 6 + 28 + 6 + 1 + 1 + 4 + 4 = 67 tests, 0 failures
# - 17 unit tests (pre-existing)
# - 28 codegen tests (pre-existing — back-compat seaorm)
# - 6 config_parse tests (pre-existing)
# - 6 codegen unit tests (pre-existing)
# - 1 golden test, 1 observations test (pre-existing)
# - 4 sqlx_emit_test  ← NEW (Sprint C1 Gap 1)
# - 4 sqlx_target_spec_test  ← NEW (Sprint C1 Gap 1)
```

## How to apply this to upstream ruff

```bash
git clone https://github.com/AdaWorldAPI/ruff.git
cd ruff
git checkout main  # @ 5179bc00 (or newer; the patch is forward-compatible)
git checkout -b claude/openproject-sqlx-emitter
git apply ../openproject-nexgen-rs/calibration/sprint-c1-gap1-ruff/sprint-c1-gap1.patch
cargo test -p ruff_python_dto_check     # expect 67 passed
git add -A
git commit -m "feat(codegen): add sqlx (axum) target for openproject port"
git push -u origin claude/openproject-sqlx-emitter
# Open PR on AdaWorldAPI/ruff
```

The 16 files in the patch are exactly the 14 files vendored here +
codegen/mod.rs (whose hunk is shown inline above and in `codegen-mod-rs.diff`).
