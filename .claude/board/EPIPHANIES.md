# EPIPHANIES.md — findings log for openproject-nexgen-rs `.claude/`

> **APPEND-ONLY.** Newest at top. Each entry is a dated insight with a
> `**Status:**` line (FINDING / CONJECTURE / SUPERSEDED). Only the Status
> line is mutable — body and date are immutable. Corrections append as
> new dated entries citing the original.
>
> Convention adopted from `AdaWorldAPI/surrealdb`'s `.claude/board/EPIPHANIES.md`.
>
> **Status legend:**
> - **FINDING** — empirically verified (test ran, behaviour observed, source read).
> - **CONJECTURE** — plausible but unverified; a probe is queued.
> - **SUPERSEDED** — invalidated by a later entry; keep the row.

## Entries (newest first)

## 2026-07-10 — Gap (b) CLOSED: the routes.rb harvest arm ships (ruff #73) — helper stem → controller#action is now resolvable
**Status:** FINDING (shipped + gated on the real corpus; ruff PR #73, branch `claude/openproject-transcode-status-c6e8in`)
**Scope:** ruff `ruff_ruby_spo::routes` × `ruff_spo_triplet::Predicate` × the E-CLICKWEG-CHOREOGRAPHY-1 gap ledger (entry below)

The routes.rb stratum — gap (b) from the 2026-07-09 choreography ruling
("HTTP verb / member-collection / return shape, the one missing Action-kind
fact source") — is closed. A new `ruff_ruby_spo::routes` AST-walk arm resolves
a route-helper **stem (+ verb) → `controller#action`** (`RoutesTo` +
`RouteScope`, 2 additive Authoritative predicates, count-lock 71→73), so the
`InvokesAction`/`NavigatesTo` stems that "resolved to nothing" now join. Built
council-first (5 savants → consolidate → 3 reviewers → v3 freeze; R1 the
correctness-adversary caught 6 corpus-anchored P0 Rails-semantics bugs BEFORE
implementation) then gated centrally against the live corpus: 29 files, 1625
declared routes, 1534 emitted, `escaped_other` exactly `["use_doorkeeper"]`.
Central review added four real-corpus fixes (namespace-path controller
fallback so a bare `get action:` in `namespace :admin;:settings` resolves to
`admin/settings#…` instead of leaking the verb into `escaped_other`; measured
`escaped_dynamic=13`; `as:`-verbatim collection name; `except:`-respecting
spot-check). Full account: `.claude/ruff-expansions/2026-07-10-routes-arm-spec.md`
(v3 contract + SHIPPED note).

**Gap ledger now:** (a) writes/calls CLOSED · **(b) routes.rb stratum CLOSED
(this arm)** · (c) recipe codebook Phase 2 unwired · (d) permission-declaration
arm · (e) DB-resident choreography content → hydrator. The remaining open arms
(d)+(e) are the guard/state strata the corpus reality-check found to be
DB-resident or declaration-static; (c) is the OGAR-side codebook wire.

## 2026-07-09 — Clickwege live in the moving joints: traces-not-facts, the five-edge choreography mint, ore/slag refinery (operator ruling + verified gap map)
**Status:** FINDING (operator ruling 2026-07-09; gap map verified in code this session — 5-reader sweep over ruff / OGAR / op-nexgen / upstream corpus / MedCare-rs. OGAR board mirror pending.)
**Scope:** ruff `ruff_ruby_spo` × OGAR `ogar-vocab`/`ogar-from-ruff` × `op-server/{nav,viewfilter,board}.rs` × corpus `config/routes.rb` + `app/models/workflow.rb` × MedCare-rs parity

Operator, in substance: *the Clickwege are not in the schema — they live in
the moving joints. ORM gives nouns (what exists/relates/persists), AR gives
verbs (what can move, what mutates together, what becomes legal after state
changes), Clickwege give choreography — and choreography must be harvested
from motion, not tables. The hard extraction is joint → intent: harvest
Clickweg candidates as TRACES, not facts ("user sees button X because view
renders helper Y → route Z → controller A → mutates model B, if policy C
and state D"), and OGAR mints NavigationEdge / ActionEdge / MutationEdge /
GuardEdge / StateTransitionEdge. The residue is never handwritten Rust —
every failed/uncertain click path becomes another recipe; the refinery gets
cleaner with every project. The 723 generated files are a cache; the real
artifact is the harvested semantic graph.*

Verified against code (receipts abridged):

| proposed edge | fact ore today (ruff) | mint target today (OGAR) | joint gap |
|---|---|---|---|
| NavigationEdge | YES — `NavigatesTo` ×3 shapes (ERB click / `redirect_to` / menu-DSL) + `InvokesAction` (`ruff_ruby_spo/{navigation,menu,actions}.rs`) | ABSENT as edge — `ogar-from-rails::RailAction` is a vertex, not a connection | **routes.rb never parsed**: `InvokesAction`'s object is the helper STEM, never resolved `controller#action` (`actions.rs:102-104`) = ledger gap (b) |
| ActionEdge | YES — controller DO-arm live (`extract_tree_with` → `lift_actions` → `ActionDef`) | `ActionDef` node-attached; `predicate: String`; `RecipeConceptId` Phase 2 unwired (grep `ogar-from-ruff` = 0 hits) | route-kind discriminant (b) + codebook wire (c) |
| MutationEdge | YES — `WritesField`/`WritesIfBlank`/`Calls` (closed 24-verb `AR_MUTATORS`) | `ActionDef.writes` name-level; `EnterEffect{field,to_value}` string-encoded | written VALUES not captured; effects node-attached, not edge-shaped |
| GuardEdge | **ABSENT** — `permit`/`params.require` = 0 matches in ruff; `before_action :authorize` degrades to untyped `HasDslCall` | `Guard 0x02XX` recipe family = VALIDATION guards only; no permission concept; `required_role` lives downstream in lance-graph-ogar | **gap (d)** — permission-DECLARATION arm. Corpus verdict MIXED: declarations are static ore (`config/initializers/permissions.rb` `AccessControl.map` = permission→{controller:[actions]}; contract `attribute …, permission:` DSL; `Accounts::Authorization` concern) — grants (`role_permissions` rows) are DB. Prior art: RESIDUAL-THREE-BUCKETS B2 rows + OGAR CLASSID-RBAC-KEYSTONE-SPEC (doctrine, no arm) |
| StateTransitionEdge | **ABSENT** — no state-machine arm; `enum :x` harvests the state COLUMN, not transitions | **doc-only** — `results_in: Option<StateTransition>` (OGAR-AST-CONTRACT.md:88) has NO crate type (grep = 0); `StateMachineDecl` named-unbuilt | **gap (e)** — transition CONTENT is DB rows (`workflows` table: type×role×old_status→new_status, `db/migrate/tables/workflows.rb:33-44`); code holds only hook locations (`base_contract.rb:168 validate_status_transition`). Landing zone: the existing `ogar-hydrator-postgres` proposal |

Composition state: **no multi-hop trace exists anywhere today.** ruff emits
isolated per-file facts and delegates every cross-strata join to callers
(`navigation.rs:9-13`); OGAR names the split explicitly — "shape vs
choreography", choreography = the runtime invocation log, never a static
type (`docs/ADAPTERS-AND-ACTORS.md:76`); op-nexgen's `navigates_to ⋈
writes_field` join is verified ∅ (nav-only + mask-only layers; `AnonymousRbac`
hardcoded; board affordances hand-written). The one fully-traced corpus
example — watch/unwatch: view helper → `watch_path` → `routes.rb:287-289`
constraint-object route → `watchers_controller` `before_action` guards →
`add_watcher` row insert — is STATIC at every hop except the guard's
*answer*. Joint → intent is harvestable; grants/state content needs the
hydrator. That residue is precisely the operator's "daily migration grind
becomes the fuel."

Consequences:
1. **Traces, not facts.** The `Triple` already carries NARS `{f,c}` — a
   trace is a typed CHAIN of triples with composed confidence. Low-confidence
   chains ARE the "flag uncertain residue → review once → recipe library
   grows" flywheel; the proposer emits trace candidates, nothing hand-codes
   a click path.
2. **Mint constraint (canon).** The five edge kinds would be OGAR's first
   edge-shaped behavioural types. Per V3-TRANSPILER-ADR they land as
   GUID-reference tenants / triplet-mode `[SpoTriple; 4]` facets — never a
   resurrected EdgeBlock.
3. **Gap ledger extended:** (a) writes/calls CLOSED · (b) routes.rb stratum
   OPEN — and measured: 1311 lines of heavily customized DSL (route
   concerns, lambda constraints, constraint objects, catch-alls); the D3
   cross-file trap confirmed (menu is Rails-central) · (c) recipe codebook
   HALF-CLOSED (Phase 2 unwired) · **(d) permission-declaration arm** ·
   **(e) DB-resident choreography content → hydrator**.
4. **MedCare parity CONFIRMED** — same refinery in the C# coat: 56,812
   structural + 97,176 body triples, WinForms `navigates_to`/`selects_view`
   choreography plane, `FORM_TO_NODE` the one hand-authored seam, recipe
   residue 99.6–99.7% recoverable / 5 essential (vs Rails 98.4% / 1 —
   explicit cross-citation in the MedCare handover). The pattern generalizes
   across language coats; only the adapters differ.
5. **723 files = cache, graph = artifact** — verified literally: generated
   ActionDef bodies are `// TODO: port` stubs (3,427 counted); the durable
   artifact is the harvested graph + the transpile LEDGER. The emit is
   re-derivable; the ontology is not.

## 2026-07-05 — Recipe codebook Phase 1 SHIPPED upstream (`ogar-vocab::recipe`); gap (c) half-closes
**Status:** FINDING (mirrors OGAR `E-RECIPE-CODEBOOK-MINTED-P1`)
**Scope:** OGAR `ogar-vocab::recipe` × `.claude/knowledge/RAILS-COVERAGE-KIT.md` §5 gap (c)

The recipe-concept codebook + the lift-time predicate resolver
(`recipe_concept_from_surface`: `Triple.p: String × lang → RecipeConceptId`)
shipped upstream in `ogar-vocab` — the four §5 families as a typed
`RecipeConceptId` newtype (collision-proof vs class `u16`), 27 concepts,
forward/reverse + drift-gate tests. The verb-side convergence pin is
machine-checked: Rails `belongs_to` ≡ Odoo `Many2one` → `REL_MANY_TO_ONE`.
So gap (c) ("recipe-concept codebook unminted") from the entry below is
**HALF-CLOSED**: the codebook + resolver exist; Phase 2 (wire the resolver
into `ogar-from-ruff` lift so `ActionDef`/triples carry the id) is the next
step, zero output-shape change this pass. `OpHandlerKind` remains the
per-consumer enum until Phase 2 lands. Canon: OGAR
`E-RECIPE-CODEBOOK-MINTED-P1`.

## 2026-07-05 — The recipe shape ruff lands on IS the `<port>::<path>(<shape>)` grammar (a canonicalized SPO triple), not the per-consumer zoo
**Status:** FINDING (operator insight; mirrors OGAR `E-GRAMMAR-IS-THE-RECIPE-SHAPE`)
**Scope:** `.claude/knowledge/RAILS-COVERAGE-KIT.md` §5 × OGAR invocation grammar × ruff `expand()`

A recipe = a canonicalized SPO triple, and the grammar's three positions
are the triple's three legs: subject = `part_of::is_a` facet → classid
(shipped, `ruff_spo_address::mint`); predicate = verb → `RecipeConceptId`
(OPEN — this is gap (c) from the entry below); object = `input[type]`
typed by the schema/association stratum (shipped). ruff already emits the
triples via `expand()`, but `Triple.p` is a **String** — the zoo, one
level down. So gap (c) sharpens to: **canonicalize the predicate at lift**
(`Triple.p: String → RecipeConceptId`, string kept as the `LabelDto`
skin); the four §5 families = which verb-codebook the predicate comes
from. No new extractor, no per-consumer enum — a resolver + the codebook.
Canon: OGAR `E-GRAMMAR-IS-THE-RECIPE-SHAPE`; §5 dated pointer added.

## 2026-07-05 — Gap ledger verified in code: F17 writes/calls prerequisite is DONE; remaining gaps = routes.rb stratum + recipe codebook
**Status:** FINDING (mirrors OGAR `E-F17-PREREQ-VERIFIED`; corrects gap item (a) in the entry below, which propagated a stale RAILS-COVERAGE-KIT §6 claim)
**Scope:** ruff `ruff_spo_triplet`/`ruff_ruby_spo` × OGAR `ogar-from-ruff`/`ogar-vocab` × `.claude/knowledge/RAILS-COVERAGE-KIT.md` §6

Verified on the consumed branch: **(a) CLOSED** — `Function::{writes,
calls}` shipped (`ir.rs:264-284`), populated by the Ruby walker
(`functions.rs`), emitted as `writes_field`/`calls` triples with truth
values; the controller DO-arm harvest is live (`extract_tree_with`,
ruff #42/#43 → `lift_actions` → `ActionDef`, facts-only). **(b) OPEN**
— the `routes.rb` stratum (HTTP verb / member-collection / return
shape), the one missing Action-kind fact source. **(c) OPEN** — the
OGAR recipe-concept codebook unminted (`LifecycleTrigger{event:String}`
still surface-string). Dated staleness note added in place at
RAILS-COVERAGE-KIT §6; the fragment README's gap ledger updated.
Consequence: the Action-kind classifier's inputs are mostly harvestable
TODAY; next levers are upstream — ruff routes-stratum + the OGAR §5
codebook mint (serialized-allocation train).

## 2026-07-05 (correction) — The reunion is an ORDER; route/action dedup IS SoC + the recipe codebook (operator canon). Corrects the entry below.
**Status:** FINDING (operator ruling 2026-07-05 — mirrors OGAR `E-RECIPE-REUNION-ORDER`)
**Scope:** `crates/ruff_python_dto_check/` × `.claude/knowledge/{CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK(OGAR), RAILS-COVERAGE-KIT, TWO-SHAPES-COMPILED-NOT-PARSED}` × `2026-07-05-redmine-op-ar-shape-convergence-plan.md`

The entry below (my prior FINDING) recorded a council that REJECTED the
route-dedup ⇄ SoC unification as `[S]` mere-rhyme. That was WRONG — the
unification was already operator-canon before the council ran, and the
council was mis-framed (pointed only at `soc.rs` + `op-codegen-bucket`,
never at the rulings). Operator, verbatim: *"The reunion is an order. We
only use ORM for Schema and actions. We keep AR and rails/ruby. Redmine
teaches us the ancestry. ERB redmine fieldview teaches us to translate
into askama classview fieldmask."* The five clauses each have a canon
home (see OGAR `E-RECIPE-REUNION-ORDER` for the full mapping):

1. reunion = order — Redmine ⇄ OP at the AR shape (`WorkPackage ≡ Issue ≡
   0x0102`); fork lineage. `2026-07-05-redmine-op-ar-shape-convergence-plan.md` §0.
2. ORM only for Schema + actions (D-AR-3.5 stratum + `(verb,criteria)`
   body triage). `TWO-SHAPES` §2, RAILS-COVERAGE-KIT §6.
3. keep AR/Rails/Ruby (the class-body AST = the wings). `TWO-SHAPES` §2.
4. Redmine teaches ancestry — STI collapse IS coverage (Redmine 53.8% /
   OP 71.7%). RAILS-COVERAGE-KIT §0.
5. ERB fieldview → askama classview fieldmask — route dedup IS SoC:
   `FIELD_MASK_CAP = MAX_SIBLINGS_PER_TIER`, one cap (OGAR
   `CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK`, operator 2026-06-29);
   `HandlerKind` is the canon Action-kind recipe family →
   `RecipeConceptId` (RAILS-COVERAGE-KIT §5).

Consequences landed this commit: `crates/ruff_python_dto_check/README.md`
re-framed from "parked, retire" to the ERB-fieldview → askama render +
Action-kind recipe corpus (teaching material seeding `ogar-render-askama`
+ the recipe codebook). What survives from the council: only the factual
gap ledger (ruff lacks writes/calls capture per F17; the recipe-concept
codebook unminted) — queued upstream, never op-side.

## 2026-07-05 [SUPERSEDED] — Route-kind dedup ⇄ SoC synergy: council-rejected rhyme; ruff_python_dto_check parked as the un-upstreamed sqlx delta
**Status:** SUPERSEDED (2026-07-05, same day — by the correction entry above, on operator ruling. The `[S]` rejection contradicted operator canon; the council was mis-framed. Kept append-only as the cautionary record.)
**Scope:** `crates/ruff_python_dto_check/` × `crates/op-codegen-bucket/` × OGAR board × ruff `ruff_spo_address::soc`

The proposed synergy "route deduplication is the DO-arm mirror of ruff's
SoC lint" went through OGAR's 5+3 hardening council (5 research savants +
3 brutally-honest reviewers, all passes recorded) and was **REJECTED at
`[S]` mere-rhyme** — grounds: detect≠curate (ruff harvests no route
discriminant facts; no classifier exists), discard≠retain (soc reclaims
duplicate rows; bucketing retains every skin — DRY templating, not
deduplication), and the vacuity trap ("N siblings → K representatives +
residual" is the workspace's universal quotient primitive; soc's
distinctive content — harvested relation + byte-cap + `law_holds` — does
not transfer). Canonical verdict entry with receipts (16/16 CODED), the
verb ≠ route-recipe carve, the pre-registered OP⇄Redmine kind A/B probe,
and the mint fence: OGAR `.claude/board/EPIPHANIES.md`
**E-ROUTE-KIND-VERB-STRATA** (+ `docs/DISCOVERY-MAP.md` twin
D-ROUTE-KIND-VERB-STRATA).

Local consequences landed with this entry:

- `crates/ruff_python_dto_check/` is **PARKED** via its new README: it is
  the un-upstreamed **sqlx-target delta** against live ruff's
  `ruff_python_dto_check` (upstream has `contract.rs` + seaorm codegen,
  no `sqlx_emit/`); no `Cargo.toml`, deliberately not a workspace member.
  Retirement path: upstream the sqlx arm to ruff (E-VENDOR-DELTA
  pattern), recipes to `ogar-adapter-*`, then the directory retires.
- The route-kind A/B is a **DISTINCT measurement** from the capstone C5
  verb A/B (route-recipe stratum vs verb stratum) — dated notes added to
  the capstone and the migration plan §3.

## 2026-06-03 — Rails-to-SPO triples is structurally the same write shape as Lance + Raft
**Status:** CONJECTURE
**Scope:** `crates/op-codegen-pipeline/` × `vendor/AdaWorldAPI-lance-graph/codegen_spine` × upstream `AdaWorldAPI/lance-graph#452`

Lance-graph PR #452 (`docs/APPEND_ONLY_RAFT_DOVETAIL.md`, merged
2026-06-03) makes explicit that Lance's append-only storage shape
and Raft's append-only commit log are **structurally** — not
coincidentally — the same write. Consensus and storage become the
SAME write, not two separate taxes layered on top of each other.

Our pipeline (Rails source → `ruff_ruby_spo::extract` → `ModelGraph`
→ `expand` → `Vec<Triple>` → SurrealQL DDL via op-codegen-projection)
is **also** append-only at each layer:

- `ModelGraph::models` is a `Vec` — extending it is concatenation.
- `expand()` is a pure fold over the graph, producing a `Vec<Triple>`
  where each triple is an immutable fact (subject, predicate, object,
  truth tier).
- Op-codegen-projection emits SurrealQL `DEFINE TABLE` /
  `DEFINE FIELD` statements that are themselves append-only DDL
  (`OVERWRITE` is opt-in; default is "add").
- The catalog DDL builders C16b landed in surrealdb (`new_for_ddl`
  + chainable setters) compose a `TableDefinition` by additive
  setter chains — no mutation of prior state.

If this conjecture holds, then the same dovetail that makes "peer-Raft
+ Lance-local-on-each-node" cheap deployments structurally cheap will
make "openproject-rs running locally + replicated by Raft" structurally
cheap. The OpenProject upstream (Rails + Postgres + Patroni) cannot
make the same claim because the underlying storage is not append-only;
it requires 2PC-flavored synchronization.

**To verify (probe queue):** trace the codegen-projection output for
a representative OP fixture (WorkPackage + Project + Member) and check
that every emitted SurrealQL statement composes purely additively with
the previous state — no `REMOVE` / `OVERWRITE` flows under default
projection settings. If true, the property holds end-to-end and we
should record it as a FINDING.

**Cross-ref:** `AdaWorldAPI/lance-graph#452`, `crates/op-codegen-pipeline/src/lib.rs`, `vendor/AdaWorldAPI-lance-graph/crates/lance-graph-contract/src/codegen_spine.rs`.

---

## 2026-06-03 — Cluster asymmetry maps to our sprint-by-sprint gap closure discipline
**Status:** FINDING
**Scope:** Sprint sequencing across C4 → C17a → C17b → C17c × upstream `AdaWorldAPI/lance-graph#453`

Lance-graph PR #453 (`docs/CLUSTER_ASYMMETRY.md`, open) frames an
asymmetry: OLD stacks (Cassandra+JG, ElasticSearch, ...) cluster
*because they have to* — data doesn't fit on one node. Lance-graph
consumers cluster *because they choose to* — for availability, geo,
load distribution. Same word, opposite cost structure.

Our C4 → C17a graduation has the same shape applied to dependencies:

- **C4** shipped a dependency-free line scanner *because the IR shape
  had not been pinned yet*. Adding `lib-ruby-parser` then would have
  paid for AST machinery to verify a shape we hadn't yet committed
  to. The scaffold paid for itself precisely by being limited — it
  pinned `RubyClass`, `Model`, `Field`, `Function` via a passing
  test, then got out of the way.
- **C17a** graduated to `lib-ruby-parser` *because the gaps the line
  scanner missed had been measured* (the 5-probe coverage report).
  Adding the parser now had a known payoff: 6 of 21 documented gaps
  closed in one move (G1-G6). The parser cost is real (workspace
  dep, compile time, 1500 LOC bridge) but justified by the closed
  gaps.

The discipline: **don't pay the parser-graduation tax until the
line-scanner gaps are measured**. Same shape as #453: don't pay the
clustering tax until the single-node capacity is exceeded.

C17b + C17c continued the pattern: each sprint closes a specific
cluster of gaps (G7-G14, G15-G20) by reading more AST node types
within the architecture C17a established. No re-design between sprints
— the gap report is the bar.

**Cross-ref:** `AdaWorldAPI/lance-graph#453`, `.claude/knowledge/c17-scanner-coverage-probes.md`, sprint commits `269ef5e` (C17a) / `2927e27` (C17b) / `43ddba7` + `ab4a058` (C17c).

---

## 2026-06-03 — Vendor mirror discipline is the "review fits locally" property
**Status:** FINDING
**Scope:** `vendor/AdaWorldAPI-lance-graph/README.md` × Lance-graph "Wikidata fits locally" framing

The nexgen vendor mirror philosophy is explicit (vendor README, Sprint
C6 introduction):

> Only the modified file + its diff are mirrored here — the full
> `lance-graph-contract` crate is 94 source files / 1.6 MB; mirroring
> everything would obscure the single ~140-line additive change
> without adding review value.

This is the same property Lance-graph claims about its data shape
(115M Wikidata entities in low single-digit GB compressed), applied
to *review* instead of *storage*. **Review fits locally**: a reviewer
sees the change without having to grok the upstream 1.6 MB; storage
fits locally: a query touches one node without consulting N-1
others.

Both are the same architectural decision applied at different layers:
**minimize the surface a consumer must hold in memory to reason
locally**. Cassandra-style spreads the data across N nodes; every
query reads from N. Vendor-everything spreads the change across 94
unchanged files; every review reads 94. Lance-style + vendor-the-delta
both flip this: the consumer reads what is actually relevant, and
nothing else.

The two openproject patches in C16a (op-surreal-ast) + C16b
(surrealdb DDL builders) + the C17a-c ruff_ruby_spo extensions all
maintained this discipline — even when ruff_ruby_spo was edited
heavily (520 + 506 + 418 lines across three sprints, all in
`parse.rs` / `lib.rs`), the vendor mirror still only carries those
two files plus the schema mirror.

**Cross-ref:** `vendor/AdaWorldAPI-lance-graph/README.md`, `vendor/AdaWorldAPI-ruff/README.md`, vendor mirror commits across C6 + C16 + C17 sprints.

---

## 2026-06-03 — Gap-closure work is graph-identity work, not feature parity work
**Status:** FINDING
**Scope:** `vendor/AdaWorldAPI-ruff/crates/ruff_ruby_spo/src/lib.rs` `RubyClass` + `AssociationDecl` extensions across C17a-c

Each gap closure (G1-G20 over three sprints) adds a field to
`RubyClass` or `AssociationDecl` that names a piece of Rails
semantics — `polymorphic`, `through`, `source`, `as_target`,
`class_name`, `concerns`, `enums`, `store_accessors`, `attributes`,
`scope_definitions`, `default_scope_body`, `callbacks`, etc. (17
fields full + 2 partial after C17c).

The naive framing: we're catching up to Rails feature parity.

The structural framing (matching lance-graph's SPO insistence): each
field is a piece of **graph identity**. `class_name: "Principal"`
isn't a Rails-config quirk; it's the fact `belongs_to-target =
openproject:Principal`. `through: :memberships` isn't a query
optimization; it's `path = WorkPackage → Member → Principal`. The
graph-identity question is "do downstream consumers see the SAME
edge structure they would have inferred from running the Rails app
itself?"

This reframing has a load-bearing consequence: **partial gap closure
is partial graph identity**. The pipeline can still emit triples; it
just emits *fewer correct* triples and *some wrong* ones. The
remaining gaps (G18 DSL, G21 constants, plus partial G7 / G11)
matter not because of "more is better" but because closing them
makes the graph correct in the cases they cover.

Implication for sprint C17d-h: the same architecture is correct for
controllers, routes, duplicate-routes, and Rails TargetSpec
extensions. Each emits triples; the graph is the source of truth;
SurrealQL DDL / lance-graph triplets / TinkerPop Gremlin / etc. are
all just renderings.

**Cross-ref:** `c17-evaluation.md`, `c17-scanner-coverage-probes.md`, `vendor/AdaWorldAPI-ruff/crates/ruff_ruby_spo/src/lib.rs`.

---

## 2026-06-03 — AdaWorldAPI ecosystem pin matrix is a single deployment-shape decision
**Status:** FINDING
**Scope:** `rust-toolchain.toml` (nexgen + surrealdb + ndarray fork) × `Cargo.toml` lance / lancedb / datafusion / arrow / ndarray pins

The pin matrix across the AdaWorldAPI ecosystem (`rust 1.95`,
`lance =7.0.0`, `lance-index =7.0.0`, `lancedb =0.30.0`, `datafusion
53` transitive, `arrow 58`, `ndarray` fork with
`std + hpc-extras`) reads as a maintenance ritual but is actually a
single deployment-shape decision.

The constraint chain:
1. `lance =7.0.0` and `lancedb =0.30.0` are exact-pinned because
   Lance + Datafusion are in active development and any minor bump
   reshapes the query path. The append-only-Raft dovetail (PR #452)
   only holds if the Lance storage layer's write shape is stable.
2. `datafusion 53` is transitive-pinned because Lance 7.0.0's
   Cargo.lock resolves it; bumping Lance is the only path.
3. `arrow 58` semver range works because Arrow's stability inside a
   major is real and our consumers don't pin against minor
   differences.
4. `ndarray` fork with `std + hpc-extras` exists because
   `crate::simd::F64x8` is needed for the HNSW vector index distance
   kernels (L2 / L1 / L∞ / Pearson at `src/idx/trees/vector.rs`
   L421/L450/L475/L496) — and the upstream crate doesn't ship those
   SIMD primitives. The fork is rev-pinned (`0129b5c8...`) because
   `cargo update` would otherwise drift it to fork-HEAD between
   sessions, and that's the one place in the matrix where silent
   drift would change behaviour.
5. `rust 1.95` aligns all crates so the toolchain version is not a
   re-test dimension.

Read together: the pin matrix encodes the architectural decision
"lance + lancedb + datafusion form one coherent storage layer at
exactly these versions; rust 1.95 is the build floor; ndarray-fork
SIMD is the floor for performant vector indexes; everything else
(arrow, surrealdb deps) can move within minor". It's not five
independent pinning choices; it's one decision with five
manifestations.

**Cross-ref:** `surrealdb` PR #34 (knowledge doc + ndarray rev pin),
`openproject-nexgen-rs` PR #21 (C16d rust 1.95 alignment),
`.claude/knowledge/adaworldapi-pinning.md` (in surrealdb fork).

---

## 2026-06-03 — Vendor patches survive arbitrary upstream-side merges
**Status:** FINDING
**Scope:** Session-spanning incident across surrealdb PR #33 + #34 (C16b + follow-ups)

Surrealdb fork PR #33's merge picked up the FIRST commit on the
branch (`8ffe408`) — the actual C16b DDL builder code — but the
follow-up commits (knowledge doc, ndarray rev pin) arrived AFTER the
merge was performed and so didn't land in main. Three commits became
"unreachable from main but reachable from the still-existing remote
branch" — the textbook git definition of orphan-adjacent.

Recovery: cherry-pick the useful follow-ups onto a fresh branch from
main, open a focused PR (#34), and delete the orphaned source branch.
Three commits → two useful ones rescued (the third was a redundant
re-author of `8ffe408` which was already in main with its original
authorship).

The pattern is worth naming because vendor mirrors compound this
risk: every PR can lose 1-N follow-up commits if the maintainer
merges before the full sequence is pushed. The mitigation is
boring: **push the whole intended sequence before requesting
review**, or accept that follow-up rescue PRs are normal.

Same shape applies to C16a → C16b → C16c → C16d sprint sequencing
on nexgen, where each was its own PR. Each PR was self-contained;
no orphan risk because the sequence was implicit in the merge
order, not implicit in a single branch's history.

**Cross-ref:** surrealdb fork PR #33 + PR #34, this session's
cleanup of `claude/beautiful-gates-dJo0u` on nexgen.

---

## 2026-06-03 — `lib-ruby-parser` Kwargs vs Hash distinction is Ruby 3.0 keyword-arg separation
**Status:** FINDING
**Scope:** `vendor/AdaWorldAPI-ruff/crates/ruff_ruby_spo/src/parse.rs::parse_association_send` first-version bug

The first C17a implementation matched `Node::Hash` for the trailing
options on `belongs_to :foo, dependent: :destroy` etc. Tests for
plain associations passed; tests for any associations *with options*
all failed with `expected Some("destroy"), got None`.

Root cause: `lib-ruby-parser` 4.0 (which tracks Ruby 3.1) distinguishes
`Hash` (braced literal `{a: 1, b: 2}`) from `Kwargs` (trailing
keyword args `a: 1, b: 2`). Ruby 3.0 made this distinction
load-bearing — `f({a: 1})` and `f(a: 1)` are no longer the same
call. The parser models this faithfully.

Fix: match BOTH `Node::Hash` and `Node::Kwargs` in option-extraction
code. Defense-in-depth: legacy braced form (`belongs_to :x,
{dependent: :destroy}`) still works for any consumer that wrote it
that way; modern trailing form is the default path.

The same distinction came up again in C17b (`enum` options have
both: positional Hash for values dict, trailing Kwargs for options
like `scopes:`) and C17c (Block-wrapped do/end callbacks are at the
*statement* level, not in `send.args`). The shape is: ruby AST
faithfulness sometimes requires reading 2 node types where you'd
naively expect 1.

**Cross-ref:** C17a commit `269ef5e`, debug session in
`/tmp/probe-ast/main.rs`.

---

## 2026-06-03 — Codex review caught a real correctness gap C17c authors missed
**Status:** FINDING
**Scope:** `vendor/AdaWorldAPI-ruff/crates/ruff_ruby_spo/src/parse.rs::parse_default_scope_send` + `CALLBACK_EVENTS`

Codex automated review on PR #24 flagged two correctness issues that
the local test suite did NOT catch:

1. `default_scope { where(...) }` — the canonical Rails form — was
   silently dropped. The C17c handler took `block: Option<&Block>`
   from the dispatcher loop but didn't use it; `parse_default_scope_send`
   only looked at `send.args`. The block-form's body was lost.
2. `around_validation` was missing from `CALLBACK_EVENTS`,
   silently excluding `around_validation :method` from
   `RubyClass.callbacks`.

The bug-class shape: I wrote 11 tests for C17c, focusing on each new
extracted field. None of those tests covered the **canonical** Rails
form for `default_scope` — only the lambda-arg form (`default_scope ->
{ ... }`). And for callbacks I wrote tests for around_destroy +
after_create_commit but not around_validation.

**Generalizable lesson**: when the test author writes fixtures, they
unconsciously bias toward the form they have in their head. The
canonical Rails form is the one Rails docs lead with, which doesn't
necessarily match the form I write tests for. A second pair of eyes
on the fixture surface — even an automated one with no model
knowledge — catches these.

Fix landed in commit `ab4a058`: `parse_default_scope_send` and
`parse_scope_send` now accept the dispatcher's `outer_block` and fall
back to it. 4 new regression tests cover both default_scope forms (lambda,
brace-block, do-end-block), the scope do-end form, and around_validation.

**Cross-ref:** PR #24 Codex review threads, commit `ab4a058`.

---

## 2026-06-03 — The "ruff dto crate routes + duplicate routes" Rails port is graph-emission, not analytics
**Status:** CONJECTURE
**Scope:** Upstream `ruff_python_dto_check/{extractors/routes.rs, preflight/scanner.rs, matcher/, contract.rs}` × proposed Rails port (C17d-h)

The eval doc (`c17-evaluation.md`) surveyed the upstream
`ruff_python_dto_check` modules: extractors/routes,
preflight/scanner, matcher, contract.rs. The natural framing for
porting these to Rails would be "detect routes from
`config/routes.rb`, classify handlers from
`app/controllers/*.rb`, run duplicate-route preflight, emit
TargetSpec edges".

The lance-graph SPO framing reorients this: every one of those steps
is a **triple emitter**, not a processor.

```
openproject:WorkPackagesController.show  http:verb        "GET"
openproject:WorkPackagesController.show  http:path        "/work_packages/:id"
openproject:WorkPackagesController.show  rdf:type         ogit:HandlerKind:DetailForTenant
openproject:WorkPackagesController.show  reads_model      openproject:WorkPackage
openproject:WorkPackagesController.show  renders_format   "application/hal+json"
```

Once route facts are SPO triples, "duplicate routes" is just a SPARQL
query: `?a http:path ?p . ?b http:path ?p . ?a http:verb ?v . ?b http:verb ?v
. FILTER(?a != ?b)`. The graph fits locally (cluster-asymmetry property),
so the query is local. The "preflight scanner" doesn't need to do its own
histograms — it can run as a set of SPARQL aggregates over the same
graph the codegen consumes.

If this conjecture holds, the Rails port of `ruff_python_dto_check`
isn't a new framework — it's a new family of extractor modules in
`ruff_ruby_spo` (controllers, routes, route_duplicates) that all
emit into the same `Vec<Triple>` consumers already read. The
TargetSpec nested-config edge cases become per-emission options on
each triple, not a separate schema.

**To verify (probe queue):** sketch the actual triple shape for a
representative OP route (`WorkPackagesController#show`) and check
that the existing `ruff_spo_triplet::expand` ABI accommodates the
new predicates (`http:verb`, `http:path`, `reads_model`,
`renders_format`) without modification. If yes, the conjecture
holds and we can skip designing a separate routes IR.

**Cross-ref:** `.claude/knowledge/c17-evaluation.md`, upstream `ruff_python_dto_check/src/contract.rs::RouteContract`.
