# Residual three-buckets doctrine — what the extractor can't determine, and where it lands

> Operator decision (2026-07-01, this session): the ruff-AR extraction
> determines **~72% of OpenProject's AR surface**. The remaining ~28% — the
> residual — is **not one population**. It splits into **three buckets**, each
> with its own handler. This doc pins the decision, maps it onto existing canon
> (`RAILS-COVERAGE-KIT.md` §5–§6), and carries the **measured manifest** of the
> current residual over the curated `CORE_V3_RESOURCES` set.
>
> Companion: `.claude/knowledge/RAILS-COVERAGE-KIT.md` (the function-side
> triage this is the *field-shape* counterpart of) and the C9–C15 codegen
> pipeline (`op-codegen-pipeline`, `op-codegen-projection`, `op-cli`).

## 0. The decision

The ~28% the extractor punts (emitted today as `TYPE option<any>`) is split:

| Bucket | Name | Shape | Handler |
|---|---|---|---|
| **B1** | **Fuzzy-shaped** | Emits X reliably, but arrangement/order drifts run-to-run ("something that emits X but changes the order every time differently") | **Deterministic normalizer** — the `reorder`/canonicalize blade. No new modeling; stable-sort + canonical arrangement, then the value is B0 (determined). |
| **B2** | **Anticipated-standard DO** | The recurring cross-domain objects every domain has — auth, session, account, ACL/permission, locale, subscription/watch, audit/revision chain | **Ontological landing zone** — write the DTO adapter **once**, label it against the OGIT/OGAR ontology so the AST *lands* in a known zone; from then on the codebook swiss-knife (`open`, `filter`, `reorder`, `apply mask`, …) operates on it generically. Amortizes to ~zero marginal cost per new domain. |
| **B3** | **Irreducibly random** | Genuinely bespoke logic — no anticipated shape, no stable arrangement | **Manual rewrite**, partially **inventing new standard interfaces** in the process. |

**The ratchet (why this converges):** B3 is a *feeder* for B2. Every bespoke
hand-write that turns out to recur graduates into a labeled landing zone — the
knife gains a blade, the ontology gains a concept id, and the residual shrinks
monotonically. Doing OpenProject *and* Redmine matters for exactly this
reason: the B2 zones proven on OpenProject (auth/ACL/session) are reused
wholesale on Redmine, so the second consumer's residual is far smaller than
its first-run 28%-equivalent.

## 1. Mapping onto existing canon (no new machinery)

This decision does **not** introduce a new triage — it names the field-shape
face of what `RAILS-COVERAGE-KIT.md` already pins for function bodies:

| This doc | RAILS-COVERAGE-KIT | Note |
|---|---|---|
| B1 fuzzy-shaped | §6 **coarse tier** — `(target, verb-class, order-signature)`; "random orders is the gate, not noise" | Incidental order (ops commute) → normalize → RECOVER. Significant order → it was never B1; it's B3 (PRESERVE + RFC). The arbiter is the same **round-trip-order-free parity check**. |
| B2 landing zone | §5 **canonical-label doctrine** — content-addressable concept ids, label DTOs as skins; §6 landing zone = `lance-graph-contract::action` + `ogar-render-askama` artifact kinds + `ogar-vocab` | A B2 adapter is a `LabelDto` mapping onto an existing (or newly minted, RESERVE-DON'T-RECLAIM) concept id. The swiss-knife verbs are **already shipped, deterministic** (§6): `filter` = query engine, `project` = compute DAG, `map` = deterministic lookup, `reduce` = semirings, `apply mask` = the RBAC/visibility projection. Do NOT re-implement them per zone. |
| B3 manual | §6 **essentially-foreign** — full escape, point-to-body | The "invent new standard interfaces" clause = §5 rule 2: a genuinely-new concept **mints** a new id (extends the ontology, never forks it). That mint is how B3 feeds B2. |

**Bucket assignment is a gate sequence, not a judgment call:**

```
option<any> field
  │
  ├─ 1. round-trip-order-free parity PASSES? ──────────► B1 (normalize)
  │
  ├─ 2. lands on an existing/anticipatable ontology
  │     concept (auth/session/acl/locale/audit/…)? ────► B2 (DTO adapter, once)
  │
  └─ 3. neither ────────────────────────────────────────► B3 (rewrite; mint iface
                                                              → future B2)
```

Gate 1 before gate 2: a fuzzy-ordered *standard* object (e.g. a permission
set with unstable ordering) is B1-then-B2 — normalize first, land second.

## 2. Measured manifest — CORE_V3_RESOURCES residual, bucketed

Source: `op-codegen /home/user/openproject` (C9 pipeline, extraction =
`extract_core_triples`, i.e. `app/models` filtered to the 18 curated
`CORE_V3_RESOURCES`), measured 2026-07-01. Every `TYPE option<any>` field the
projection emitted is a residual entry; the emitted DDL also carried 15 of the
18 tables (`Activity`, `TimeEntry` and one more resource produced no DDL —
worth a follow-up probe). <!-- MANIFEST:BEGIN -->

*(regenerate with: `cargo run -p op-cli -- <openproject-checkout>` — but see
§4: the build needs the OGAR git deps reachable, which this session's network
scope 403s)*

| Model.field | Bucket | Disposition |
|---|---|---|
| `Project.allowed_actions` | **B2** | Authorization/ACL landing zone (OGIT-auth family). One adapter: permission-set DTO → concept id; `apply mask` consumes it. |
| `Project.allowed_permissions` | **B2** | Same ACL zone/adapter as `allowed_actions` — one landing, two labels. |
| `Role.allowed_actions` | **B2** | Same ACL zone. Role is the canonical carrier; Project's copy is the projected mask. |
| `User.allowed_values` | **B1→B2** | Value-set with unstable arrangement: normalize (B1), then land on the ACL/preference zone. |
| `User.time_zone` | **B2** | Locale/timezone landing zone — standard DO in every domain; adapter once. |
| `Journal.predecessor` | **B2** | Audit/revision-chain zone — the temporal linked list is an anticipated standard (OGIT audit family). |
| `Journal.successor` | **B2** | Same chain zone, forward pointer. |
| `Query.available_columns` | **B1** | Set derivable, order/context isn't → stable-sort normalizer; passes order-free parity. |
| `Query.available_columns_project` | **B1** | Same normalizer, project-scoped variant. |
| `Query.for_all` | **B1** | Boolean-ish derived flag; normalizes to a determined projection. |
| `WorkPackage.assignable_versions` | **B1** | Derived candidate set, unstable order → normalizer. |
| `Type.pdf_export_templates` | **B1** | Template registry list, order-incidental → normalizer. |
| `WorkPackage.derived_progress_hints` | **B3** | OpenProject-bespoke progress derivation. Hand-write; **mint `ProgressDerivation`** as the new standard interface (candidate future B2 zone — Redmine has `done_ratio`). |
| `Version.estimated_hours` | **B3** | Computed rollup with OP semantics (sums over descendant work packages with derivation rules). |
| `Version.estimated_average` | **B3** | Same rollup family. |
| `Version.spent_hours` | **B3** | Rollup over TimeEntry — pairs with the `BILLABLE_WORK_ENTRY` (`0x0103`) convergence; may graduate to B2 once the rollup iface is minted. |
| `Version.issues_progress` | **B3** | Progress aggregation — same `ProgressDerivation` candidate as WorkPackage's hints. |
| `Version.issue_count` | **B1** | Trivial count — order-free parity passes; a `reduce` verb over a determined relation. |
| `Version.open_issues_count` | **B1** | Same trivial-count normalizer. |
| `Version.closed_issues_count` | **B1** | Same. |
| `Version.wiki_page` | **B2** | Wiki/document-link landing zone (standard cross-domain document reference). |

<!-- MANIFEST:END -->

**Composition:** ~9× B1, ~8× B2 (behind ~4 adapters: ACL, locale, audit-chain,
doc-link), ~5× B3 (behind ~2 interfaces: `ProgressDerivation`, version
rollups). The residual is **B1/B2-heavy** — most of the 28% is *anticipated*,
not random. The true B3 tail for OpenProject core is two interface mints.

## 3. What each bucket costs (build order)

1. **B1 normalizer** (cheapest, unblocks the most): one deterministic
   canonicalize pass in the projection layer — stable sort + canonical
   arrangement before emission. Turns ~9 `option<any>` fields into determined
   emissions. Arbiter: round-trip-order-free parity (`F17` shape).
2. **B2 adapters** (once each, amortize forever): ACL/permission-set,
   locale/timezone, audit-chain (Journal pred/succ), document-link. Each is a
   `LabelDto` mapping onto an ogar-vocab concept id (mint per
   RESERVE-DON'T-RECLAIM if absent) + a DTO struct the swiss-knife verbs
   already know how to open/filter/reorder/mask.
3. **B3 rewrites** (bounded, feed the ontology): `ProgressDerivation` iface
   (WorkPackage.derived_progress_hints + Version.issues_progress), version
   rollup iface (estimated/spent hours). Each mints its interface as a
   *candidate* B2 zone for the Redmine pass.

## 4. Caveats (record honestly)

- The manifest covers the **curated 18-resource core** (`CORE_V3_RESOURCES`,
  `app/models` only). OpenProject keeps ~half its domain in `modules/*` —
  `extract_app_with` widens the surface and WILL add residual rows; the
  bucketing gates (§1) apply unchanged.
- The ~72%/28% split is the operator-reported figure from the emitter-session
  extractor; the vendored pipeline here reproduces the *shape* of the residual
  (which fields punt to `option<any>`), which is what the manifest keys on.
- **Regeneration is blocked in the current session:** `op-codegen-projection`
  (and `op-canon`, `op-surreal-ast`) git-dep on `AdaWorldAPI/OGAR` +
  `AdaWorldAPI/lance-graph`, and the session network scope 403s both, so
  `cargo build -p op-cli` dies fetching `ogar-vocab`. `ruff_*` and
  `lance-graph-contract` are already **path-vendored** (`vendor/AdaWorldAPI-*`);
  the same fix pattern applies — vendor the three OGAR crates (`ogar-vocab`,
  `ogar-render-askama`, `ogar-class-view`) when a session with OGAR read
  access next touches this repo. The measurement above predates the recycle
  and is preserved verbatim; the C12 type-inference sprint may since have
  moved some B1 rows to determined — re-run when unblocked and re-bucket.
- Bucket labels for B2 name ontology *families* (OGIT-auth, locale, audit).
  The exact `ogar-vocab` concept ids are minted OGAR-side; this repo's
  adapters carry the `LabelDto` surface only. Until the mint lands, B2
  adapters are staged per-consumer (the §5 "zoo" caveat applies — temporary
  by design).
