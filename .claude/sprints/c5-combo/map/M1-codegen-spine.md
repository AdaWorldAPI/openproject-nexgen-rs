# M1 — Codegen Spine Contract Surface (lance-graph-contract)

Map of the four codegen contracts an OpenProject→SurrealQL codegen must conform
to. Source: `/home/user/lance-graph/crates/lance-graph-contract/src/codegen_spine.rs`
(READ-ONLY). Signatures quoted verbatim; cite `file:line`.
Persisted by orchestrator (subagent repo-writes were permission-blocked).

---

## Exists

- `codegen_spine.rs` — four contracts (`codegen_spine.rs:1-632`), zero-dep, std-only.
  Layering: `TRIPLETS →① TripletProjection→ STATIC CODEGEN →② RouteBucket/OdooMethodKind→ ASKAMA ROUTE SoC →③ WidgetRender→ GUI →④ Genericity`.
- IR neighbours: `ontology.rs` (`ExpandedTriple`+`SchemaExpander` :347-371), `property.rs`
  (`PropertySpec`/`Schema`/`LinkSpec`/`ActionSpec`/`Cardinality`), `class_view.rs`
  (`ClassView` :152-207), `nars.rs` (`InferenceType`/`SemiringChoice`).
- Contract crate is genuinely zero runtime deps. Trait-only; impls live downstream.
- Board: `ExecTarget::SurrealQl` already a contract variant (kanban.rs); SurrealDB
  framed as a *view over leading LanceDB*. NO TripletProjection impl / SurrealQL
  emitter exists yet.

## Public API (verbatim)

**① Triple** (`codegen_spine.rs:74-88`) — String-keyed, NOT fingerprint:
```rust
pub struct Triple { pub s: String, pub p: String, pub o: String, pub f: f32, pub c: f32 }
fn key(&self) -> (String,String,String)   // (s,p,o) identity; f,c = NARS truth
```
**① TripletProjection** (`:107-131`) — the static-codegen gate:
```rust
pub trait TripletProjection {
    type Const: Clone;
    fn name() -> &'static str { std::any::type_name::<Self>() }
    fn truth_tolerance() -> f32 { 0.0 }            // 0.0 = exact; ALWAYS checked
    fn project(triples: &[Triple]) -> Self::Const;
    fn decompile(c: &Self::Const) -> Vec<Triple>;  // must be (s,p,o)-equal to input
}
pub fn roundtrip_eq<P: TripletProjection>(input: &[Triple]) -> Result<(), RoundTripFailure>  // :138-183
```
roundtrip_eq compares BTreeSet of (s,p,o) both ways, THEN per-key (f,c) within tol.
Default tol 0.0 = exact truth match (`:546-560`).

**② OdooMethodKind** (`:243-278`) — 16 variants, Copy+Ord+Hash, priority-ordered
(first-match-wins): PassOverride, SuperDelegationPure, SuperExtend,
StateTransitionWithGuard, OnchangeClearDependentCascade, IterFilteredRaiseOnViolation,
IterFilteredMutate, IterRecordsRaiseOnViolation, IterRecordsAggregateRelation,
IterRecordsComputeFromRelated, SudoEscalationLookup, WithContextQueryShift,
ValidatorOther, ComputeScalarOther, OnchangeOther, Other. `id()`/`ALL`/`from_id()`/`Display`.
Classifier (body→kind) is downstream, not in crate.

**② RouteBucket** (`:343-356`):
```rust
pub trait RouteBucket {
    fn kind(&self) -> OdooMethodKind;   // <-- HARDCODED return type
    fn id(&self) -> &str;
    fn id_owned(&self) -> String { self.id().to_string() }
}
```
**③ WidgetRender** (`:368-372`) — askama GUI:
```rust
pub trait WidgetRender<B: RouteBucket> { fn render(bucket: &B) -> Result<String, WidgetRenderError>; }
```
**④ Genericity** (`:399-408`): `enum { Agnostic /*runtime triple-read*/, Domain /*codegen const*/ }` (marker, not enforced).

## Additive seam for OP→Surreal (nothing in spine touched)

1. **New crate** `openproject-nexgen-rs/crates/op-surreal-codegen` depending on
   `lance-graph-contract`. NB: OP does NOT yet depend on the contract crate
   (grep=0) — adding the dep is step zero. Contract is zero-dep → cheap.
2. **`impl TripletProjection`** for an OP projection whose `Const` is the SurrealQL
   schema IR (e.g. `Vec<SurrealStmt>` = DefineTable+DefineField). `project` folds OP
   Triples → IR; `decompile` regenerates exact (s,p,o,f,c). Gate with `#[test]`
   calling `roundtrip_eq::<OpSurrealProjection>(&fixture)`.
3. **SurrealQL emitter** = plain fn over that Const IR (`fn emit_surql(c:&Const)->String`).
   Spine does not constrain emitter syntax.
4. **HandlerKind**: do NOT touch RouteBucket/OdooMethodKind (see R1). Only Triple +
   TripletProjection + roundtrip_eq are needed for a SurrealQL **schema** emitter.
5. **Genericity**: tag OP schema shape = `Domain`; generic SPO traversal = `Agnostic`.

Minimum conformant surface = (2)+(3)+roundtrip test.

## Risks

- **R1 (HARD): `OdooMethodKind` wired into `RouteBucket::kind()` return type
  (`:345`), concrete not generic/associated.** 16 Odoo-Python variants; OP uses
  `list_for_tenant`/`detail_for_tenant`/`template_get`. No additive way to make
  RouteBucket speak OP kinds without editing the spine (forbidden) or a cross-repo
  breaking change. → For C5: use the projection layer for SurrealQL **schema** only;
  route/handler classification is OUT (or a separate escalated spine PR).
- **R2: `WidgetRender` assumes askama GUI; OP is API-first HAL+JSON.** Don't
  implement ③ — no JSON analogue in the spine. ①+② reusable; ③ not.
- **R3: `Triple` is String-IRI, distinct from runtime `ExpandedTriple` (ontology.rs:347,
  u8/255 NARS) and fingerprint `SpoStore`.** Normalise truth (f32) before projecting
  or truth_tolerance fails spuriously.
- **R4: roundtrip_eq strict (tol 0.0).** SurrealQL `DEFINE` has no truth slot → emitter
  that drops/rounds NARS truth fails CI unless `truth_tolerance()` overridden or truth
  side-channelled. Decide truth policy up front.
- **R5: OP↔contract dep not established (grep=0).** New cross-repo coupling (path vs
  published); cheap (zero-dep) but unverified the build resolves it.
- **Gap:** the SurrealQL emitter target, the OP Triple producer, and the Odoo-vs-OP
  classifier live outside this read (incl. `AdaWorldAPI/ruff` fork, not inspected).
