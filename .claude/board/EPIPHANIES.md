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

## 2026-07-05 — Route-kind dedup ⇄ SoC synergy: council-rejected rhyme; ruff_python_dto_check parked as the un-upstreamed sqlx delta
**Status:** FINDING
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
