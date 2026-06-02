# vendor/AdaWorldAPI-lance-graph/

**Partial source-mirror** of Sprint C6 additive changes to
[`AdaWorldAPI/lance-graph`](https://github.com/AdaWorldAPI/lance-graph). Only the
modified file + its diff are mirrored here — the full `lance-graph-contract`
crate is 94 source files / 1.6 MB; mirroring everything would obscure the
single ~140-line additive change without adding review value.

## Why vendored here

`AdaWorldAPI/lance-graph` is outside this session's MCP push scope (the active
scope is the three openproject repos). The workbench at `/home/user/lance-graph`
can read+clone the public repo but cannot locally commit there (the env's
signing server only accepts local-proxy origins). Vendoring the changed file
into nexgen makes the actual code reviewable on the
`claude/beautiful-gates-dJo0u` branch.

For upstream application, the unified diff at `codegen_spine.diff` applies
cleanly with `git apply` from the upstream `lance-graph` repo root.

## Contents (Sprint C6)

| Path | Form here | Why |
|---|---|---|
| `crates/lance-graph-contract/src/codegen_spine.rs` | Full modified file (768 lines) | Review the new `RouteBucketTyped` trait + blanket impl + 4 new tests + 2 UFCS disambiguations in context |
| `codegen_spine.diff` | Unified diff vs upstream | Apply with `git apply` from the lance-graph repo root |

## The additive change

**One file touched: `crates/lance-graph-contract/src/codegen_spine.rs`.**

### Net-new (additive)

A sibling trait `RouteBucketTyped` next to the existing `RouteBucket`,
parameterised on `type Kind: Copy + Eq` so non-Odoo codegen targets (OpenProject,
Wikidata, future frameworks) can bring their own handler-kind enum instead of
being forced into `OdooMethodKind`:

```rust
pub trait RouteBucketTyped {
    type Kind: Copy + Eq;
    fn kind(&self) -> Self::Kind;
    fn id(&self) -> &str;
    fn id_owned(&self) -> String { self.id().to_string() }
}

// Back-compat bridge: every RouteBucket is automatically a
// RouteBucketTyped<Kind = OdooMethodKind>, so existing code keeps working
// and generic consumers can accept both shapes.
impl<T: RouteBucket> RouteBucketTyped for T {
    type Kind = OdooMethodKind;
    fn kind(&self) -> OdooMethodKind { RouteBucket::kind(self) }
    fn id(&self) -> &str { RouteBucket::id(self) }
    fn id_owned(&self) -> String { RouteBucket::id_owned(self) }
}
```

Four new tests in the existing `#[cfg(test)] mod tests` block:
- `route_bucket_typed_accepts_non_odoo_kind` — an `OpBucket` that impls **only**
  `RouteBucketTyped` (no `RouteBucket`) with its own `OpKindFixture` enum;
  proves non-Odoo targets plug in without touching the legacy trait or enum.
- `route_bucket_typed_generic_consumer_accepts_op_kind` — a generic function
  `fn dispatch_one<B: RouteBucketTyped<Kind = OpKindFixture>>(b: &B)` resolves
  + matches over the OP kind.
- `route_bucket_blanket_impl_preserves_odoo_consumers` — an `OdooBucketCompat`
  that impls `RouteBucket` (legacy shape) is usable as
  `&dyn RouteBucketTyped<Kind = OdooMethodKind>` via the blanket impl;
  no extra code on the consumer side.
- `route_bucket_typed_generic_consumer_accepts_odoo_via_blanket` — proves the
  blanket impl is the real back-compat bridge: a generic function parameterised
  on `OdooMethodKind` accepts an implementor that only knows about
  `RouteBucket`.

### Non-additive: UFCS disambiguation in 2 pre-existing tests

The blanket impl makes the bare method call `bucket.kind()` ambiguous **inside
the test module** (because `use super::*` brings both traits into scope; both
have a `kind()` method, with the same return type for a `RouteBucket`
implementor). Disambiguated with UFCS:

```rust
// before:
assert_eq!(r.kind().id(), "iter_records_aggregate_relation");
// after:
assert_eq!(RouteBucket::kind(&r).id(), "iter_records_aggregate_relation");
```

This change is **semantics-preserving** and is required only inside the test
module. Downstream consumers of `lance_graph_contract` are unaffected: they
import `use lance_graph_contract::codegen_spine::RouteBucket;` and do NOT see
`RouteBucketTyped` unless they explicitly import it. The blanket impl is in
scope through trait-object coercion only when the trait is named.

## How to apply this to upstream lance-graph

```bash
git clone https://github.com/AdaWorldAPI/lance-graph.git
cd lance-graph
git checkout main
git checkout -b claude/codegen-spine-typed-bucket
git apply ../openproject-nexgen-rs/vendor/AdaWorldAPI-lance-graph/codegen_spine.diff
cargo test -p lance-graph-contract     # expect green (~599 tests pass)
git add -A
git commit -m "feat(codegen_spine): add RouteBucketTyped<Kind> sibling for non-Odoo targets"
git push -u origin claude/codegen-spine-typed-bucket
# Open PR on AdaWorldAPI/lance-graph
```

## Verification (workbench)

```
$ cargo test -p lance-graph-contract
running 566 tests; test result: ok. 566 passed
running 7 tests;   test result: ok. 7 passed       # codegen_spine integration
running 8 tests;   test result: ok. 8 passed       # codegen_spine unit (incl. 4 new)
... (other modules) ...
Total: 599 tests, 0 failures
```

The 4 new tests for `RouteBucketTyped` pass alongside the 4 pre-existing
tests for `RouteBucket` / `TripletProjection` / `WidgetRender` / `Genericity`;
the blanket impl provably preserves the back-compat path.
