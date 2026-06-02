# Sprint C6 Report — Additive `RouteBucketTyped<Kind>` in `lance-graph-contract::codegen_spine`

**Date:** 2026-06-02 · **Branch:** `claude/beautiful-gates-dJo0u` (nexgen)
· **Workbench:** `/home/user/lance-graph` @ `main` (commit `ec1f7d2`).

---

## Headline

> Closes the M1 R1 blocker from Sprint C5-combo. A **sibling generic trait
> `RouteBucketTyped<Kind>`** lands next to the existing `RouteBucket` in
> `codegen_spine.rs` — non-Odoo codegen targets (OpenProject, Wikidata,
> future frameworks) can now plug their own handler-kind enum into the
> spine **without editing the spine**. Back-compat is preserved via a
> blanket impl: every `RouteBucket` is automatically a
> `RouteBucketTyped<Kind = OdooMethodKind>`. **599/599 tests green** in
> `lance-graph-contract` (566 pre-existing + 4 new C6 + others).

## What was blocked before

Sprint C5-combo's M1 mapping
(`.claude/sprints/c5-combo/map/M1-codegen-spine.md`) flagged:

> **R1 (HARD): `OdooMethodKind` wired into `RouteBucket::kind()` return type
> (`:345`), concrete not generic/associated.** 16 Odoo-Python variants; OP
> uses `list_for_tenant`/`detail_for_tenant`/`template_get`. No additive way
> to make RouteBucket speak OP kinds without editing the spine (forbidden) or
> a cross-repo breaking change. → For C5: use the projection layer for
> SurrealQL **schema** only; route/handler classification is OUT.

The diagnosis was correct; the resolution was to **add a parallel trait**
rather than modify the existing one — true additive escape from the concrete
return type.

## The change (one file: `crates/lance-graph-contract/src/codegen_spine.rs`)

### Net-new (+187 lines)

```rust
pub trait RouteBucketTyped {
    type Kind: Copy + Eq;
    fn kind(&self) -> Self::Kind;
    fn id(&self) -> &str;
    fn id_owned(&self) -> String { self.id().to_string() }
}

impl<T: RouteBucket> RouteBucketTyped for T {
    type Kind = OdooMethodKind;
    fn kind(&self) -> OdooMethodKind { RouteBucket::kind(self) }
    fn id(&self) -> &str { RouteBucket::id(self) }
    fn id_owned(&self) -> String { RouteBucket::id_owned(self) }
}
```

Plus **4 new tests** in the existing `mod tests` block:

| Test | What it proves |
|---|---|
| `route_bucket_typed_accepts_non_odoo_kind` | An `OpBucket` that impls **only** `RouteBucketTyped` (no `RouteBucket`) with its own `OpKindFixture` enum works — non-Odoo targets plug in additively without touching the legacy trait or its enum. |
| `route_bucket_typed_generic_consumer_accepts_op_kind` | A generic fn `dispatch_one<B: RouteBucketTyped<Kind = OpKindFixture>>(b: &B)` resolves + matches over the OP kind. The whole point of the additive trait. |
| `route_bucket_blanket_impl_preserves_odoo_consumers` | An `OdooBucketCompat` that impls `RouteBucket` (legacy shape) is usable as `&dyn RouteBucketTyped<Kind = OdooMethodKind>` via the blanket impl — no extra consumer code. |
| `route_bucket_typed_generic_consumer_accepts_odoo_via_blanket` | A generic fn parameterised on `OdooMethodKind` accepts an implementor that only knows about `RouteBucket` — the blanket impl is a real back-compat bridge, not folklore. |

### Semantics-preserving: UFCS disambiguation in 2 pre-existing tests

The blanket impl makes the bare method call `bucket.kind()` ambiguous **inside
the test module** because `use super::*` brings both traits into scope and
both expose `kind()`. Disambiguated:

```rust
// 2 sites, before:
assert_eq!(r.kind().id(), "iter_records_aggregate_relation");
Ok(format!("widget for kind={}", bucket.kind()))

// after (UFCS, semantics-preserving):
assert_eq!(RouteBucket::kind(&r).id(), "iter_records_aggregate_relation");
Ok(format!("widget for kind={}", RouteBucket::kind(bucket)))
```

**Downstream consumers are unaffected.** They `use ...::RouteBucket;` and
do NOT see `RouteBucketTyped` in scope unless they explicitly import it. The
ambiguity is a test-module-only artefact of `use super::*`.

## Coherence note (documented in the trait's doc-comment)

The blanket impl pins `Kind = OdooMethodKind` for every `RouteBucket`
implementor. A type that *also* needs `RouteBucketTyped` with a **different**
kind must NOT impl `RouteBucket` (the blanket impl would conflict). Non-Odoo
targets simply skip the legacy trait and impl `RouteBucketTyped` directly —
same pattern as Sprint C1's sqlx/seaorm target coexistence (where the sqlx
target lives alongside the seaorm one without modifying it).

## Verification

```
$ cargo test -p lance-graph-contract
running 566 tests; test result: ok. 566 passed   # pre-existing
running 7 tests;   test result: ok. 7 passed     # codegen_spine integration
running 8 tests;   test result: ok. 8 passed     # codegen_spine unit (incl. 4 new C6)
... (other test modules) ...
Total: 599 tests, 0 failures
```

## Fanout discipline

This sprint was **orchestrator-authored** (no fanout): the deliverable is
~190 lines of additive code in **one file**, with the 4 tests proving the
behaviour. A previous mapping wave (Sprint C5-combo) had already produced
the design via M1; C6 just executes it.

**One fix-loop iteration:** the blanket impl made `bucket.kind()` ambiguous
in 2 pre-existing test sites — `cargo check` caught it immediately; fixed
with UFCS (`RouteBucket::kind(&bucket)`), semantics-preserving.

## What this unlocks

With `RouteBucketTyped` in place, an OpenProject codegen layer can:

1. Define its own `OpHandlerKind` enum (`list_for_tenant`, `detail_for_tenant`,
   `template_get`, `csrf_form_post_engine_call`, `ajax_json`, `soft_delete`,
   `toggle_bool_field`) — matching the kinds already supported by Sprint C1–C3
   in the ruff `sqlx_emit` family.
2. Impl `RouteBucketTyped<Kind = OpHandlerKind>` for OP route descriptors.
3. Feed those into a SurrealQL schema/handler emitter (the next sprint) that
   takes `<B: RouteBucketTyped<Kind = OpHandlerKind>>` and lowers via the
   `codegen_spine::TripletProjection` path that was already additive.

The C5 (ruff_openproject crate, OP entry-point) + C6 (codegen_spine
generalisation) combination is the prerequisite for a Sprint C7 that ties
OP triples through the contract layer to an actual SurrealQL emitter,
without the upfront M1 R1 blocker.

## Sprint metrics — C0 → C6

| | C0 | C1 | C2 | C3 | C4 | C5 | C6 |
|---|---|---|---|---|---|---|---|
| Agent-runs | 6 | 8 | 3 | 3 | 4 | 0 | 0 |
| Fix-loop iterations | 1 | 1 | 0 | 0 | 0 | 1 | 1 |
| Tests added | 10 | 8 | 2 | 2 | 21 | 8 | 4 |
| Files touched in upstream | 178 (seed) | 16 | 9 | 9 | 11 | 3 (new crate) | **1** |
| Track | codegen | codegen | codegen | codegen | frontend | integrator | **contract** |

## Recommendation for Sprint C7

**C7 = OP→SurrealQL schema emitter, driven by `TripletProjection` + the C6
generic bucket trait.** Concretely:

- New crate `op_surreal_codegen` in nexgen or ruff that depends on
  `ruff_openproject` (C5) for the triples and `lance-graph-contract` (post-C6)
  for the contract surfaces.
- An `OpHandlerKind` enum (mirroring the 6 sqlx_emit kinds from C1–C3).
- An `impl RouteBucketTyped<Kind = OpHandlerKind>` for OP route descriptors.
- An `impl TripletProjection` whose `Const` is `Vec<SurrealStmt>` (DDL +
  executable, per the M3 surreal-ast mapping in `.claude/sprints/c5-combo/map/`).
- `roundtrip_eq` as the build-time gate (per the codegen_spine contract).

This is the actual next-step where the surrealdb/lance-graph/ndarray combo
becomes runnable end-to-end — but constrained to a single additive crate at
each layer, no global rewrites.

## Artifacts

- `docs/SPRINT_C6_REPORT.md` (this file)
- `vendor/AdaWorldAPI-lance-graph/crates/lance-graph-contract/src/codegen_spine.rs` (modified file)
- `vendor/AdaWorldAPI-lance-graph/codegen_spine.diff` (apply-able unified diff)
- `vendor/AdaWorldAPI-lance-graph/README.md` (review guide + apply instructions)
- `.claude/sprints/c6-codegen-spine-extend/STATE`

🦋
