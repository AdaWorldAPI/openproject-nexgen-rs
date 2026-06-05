# Substrate Endgame — nexgen view (openproject-nexgen-rs slice)

> **Purpose.** Tailored view onto the substrate-b endgame architecture
> from `AdaWorldAPI/openproject-nexgen-rs`'s perspective. The master
> doc lives in `AdaWorldAPI/OGAR/docs/SUBSTRATE-ENDGAME.md` (full
> five-rooms map); this doc highlights the OP-graduation slice and
> the op-codegen-* / op-surreal-ast convergence with OGAR.
>
> **Why a tailored view.** nexgen's role in the endgame is specific
> and load-bearing: it's the OP-specific implementation path that
> converges with OGAR's domain-agnostic IR via `From<op_surreal_ast::*>
> for catalog::*` (planned C16c sprint) and via the general
> `From<ogar_vocab::Class> for catalog::TableDefinition` path. This
> doc surfaces that role + the dependency chain from in-flight
> sprints (C9, C15, C16a, C16b, C16c) to Room 3's OP-as-operator-pane
> destination.
>
> Companion: `OGAR/docs/ARCHITECTURAL-DECISIONS-2026-06-04.md` (ADR-
> style backward-looking session capture). **ADR-012** (nexgen as
> special case, not collision) and **ADR-019** (OP-as-operator-pane
> as self-hosting destination) are the nexgen-touching decisions.
>
> Status: **CARVED v0** (2026-06-05). Mirror of `OGAR/docs/SUBSTRATE-
> ENDGAME.md` for nexgen concerns. Master doc is authoritative; this
> view stays consistent on per-quarter review.

## 0. nexgen's role in the substrate endgame — the convergence story

nexgen ships OpenProject-specific Rust crates that converge with OGAR's
domain-agnostic IR at `surrealdb-core::catalog::TableDefinition` —
**special case + general case meet at the catalog type, not at the
schema-source level**. Per ADR-012 (`AdaWorldAPI/OGAR/docs/
ARCHITECTURAL-DECISIONS-2026-06-04.md`):

```
OpenProject Rails AR models (sources)
              │
              │ extraction (one AST parse, two emitters)
              ▼
┌──────────────────────────┐    ┌────────────────────────────┐
│  nexgen op-surreal-ast   │    │  OGAR ogar-vocab::Class    │
│  (OP-specific mirror     │    │  (wide source-side IR with │
│   of catalog::*)         │    │   producer metadata kept)  │
│  Sprint C16a             │    │  ogar-from-ruby (planned)  │
└──────────┬───────────────┘    └──────────┬─────────────────┘
           │                                │
           │ From<op_surreal_ast::*>        │ From<ogar_vocab::Class>
           │ for catalog::*                 │ for catalog::TableDefinition
           │ (Sprint C16c, planned)         │ (Sprint 4.5+ when surrealdb-
           │                                │  parser dep unblocks)
           ▼                                ▼
    ┌──────────────────────────────────────────────┐
    │  surrealdb-core::catalog::TableDefinition    │
    │  (canonical schema-projection target)        │
    │  Sprint C16b new_for_ddl + with_* builders   │
    └──────────────────┬───────────────────────────┘
                       │ ToSql::to_sql()
                       ▼
              SurrealQL DDL string
```

nexgen owns the OP-specific fast in-repo path; OGAR owns the
domain-agnostic generalization. They meet at `catalog::*` —
**no collision, no duplication, no dual-source**. Each path has
different downstream consumers (nexgen → OP-specific tooling;
OGAR → cross-language ecosystem); the catalog target is shared.

## 1. Five-rooms map — what nexgen owns

### 1.1 Room 1 — current floor (in-flight nexgen sprints)

Per `op-codegen-bridge/README.md` in the AdaWorldAPI/surrealdb fork
+ the nexgen sprint logs:

- **C9** — extract-to-projection pipeline (`op-codegen-pipeline`,
  `op-codegen-bucket`). Walks Rails AR models; bucketizes extraction;
  feeds projection.
- **C15** — `op-codegen-projection` codegen CLI (renders OP schema
  elements as DDL via op-surreal-ast).
- **C16a** — `op-surreal-ast` (OP-specific mirror of
  `surrealdb-core::catalog` layout). Status: shipped.
- **C16b** (in the surrealdb fork, not nexgen) — `TableDefinition::
  new_for_ddl(...).with_*(...)` builders. The C16b README explicitly
  names op-codegen-bridge (this initiative) as the first downstream
  consumer; the future `ogar-adapter-surrealql` (OGAR-side) is the
  second consumer. Status: shipped in surrealdb fork; tests verify
  dummy IDs don't leak into DDL output.
- **C16c (planned)** — `From<op_surreal_ast::*> for catalog::*`
  impls. **This is the convergence sprint**: once landed,
  `op-surreal-ast` can either drop the mirror entirely (route via
  the `From` impl) or keep it as a fast in-repo path while the
  general OGAR-`Class` → `catalog::TableDefinition` path coexists.

Plus nexgen's per-domain crates that model OP at the application
layer (independent of substrate-b for now):
- `op-api`, `op-attachments`, `op-auth`, `op-cli`, `op-contracts`,
  `op-core`, `op-db`, `op-journals`, `op-models`, `op-notifications`,
  `op-projects`, `op-queries`, `op-server`, `op-services`,
  `op-users`, `op-work-packages`.

### 1.2 Room 2 — migration scaffold (nexgen's slice)

When the Room 2 substrate (Kanban polyglot dispatcher, HTTP-sidecar
bridge, §14 oracle harness) is built (runtime session work; see
`lance-graph/docs/SUBSTRATE-ENDGAME-RUNTIME-VIEW.md`), **nexgen's
op-codegen-projection path becomes the producer for substrate-b
work-items**. Specifically:

- The HTTP-sidecar variant routes Kanban work-items to OP's Rails
  Puma; OP responds via the existing controller actions
  (op-codegen-projection-rendered or hand-written). **The runtime
  doesn't know it's talking to OP** — just that the work-item form
  is HTTP RPC.
- The reflection-dump variant (the cheapest beauty win per
  `SUBSTRATE-ENDGAME.md §2.3.1`) consumes OP's runtime AR
  reflection model — `Model.reflect_on_all_associations` /
  `_validators` / `_save_callbacks` — and feeds it to OGAR's
  `ogar-from-ruby` producer. nexgen's `op-models` crate can host the
  `rails runner` script that emits the reflection dump.

### 1.3 Room 3 — OP-as-operator-pane (nexgen's destination)

This is the room where nexgen's work fully lands. Per ADR-019 +
`SUBSTRATE-ENDGAME.md §3`:

- **OP graduates per-class** from Rails-AR-on-Puma to native ractor
  handlers on substrate-b. Per-class because the §14 oracle gates
  graduation; `WorkPackage` likely first.
- **`op-codegen-projection`'s DDL output** populates the substrate's
  schema-registry at boot; nexgen's existing CLI is the schema
  authority for OP.
- **OP's UI** (Hotwire / Turbo / ViewComponent kanban, custom
  field editors, admin Workflow editor) becomes the operator pane
  for the substrate. nexgen's `op-server` is the Rails app surface;
  substrate-b is the hosting runtime.
- **The Workflow table** as live Rubicon machines (per ADR-014
  hydrator pattern + `OPENPROJECT-TRANSCODING.md §3` data-driven
  FSM observation): operator-edited rows in nexgen's `op-models`
  Workflow model regenerate the Rubicon machines in-process without
  redeploy. Bridge: nexgen `op-models::Workflow` ↔ OGAR `ActionDef`
  (one ActionDef per Workflow row).

### 1.4 Room 4 — visualization (nexgen's contribution)

OP's existing kanban UI becomes substrate visualization (per
`SUBSTRATE-ENDGAME.md §4.2`):

- **WorkPackage cards** show live substrate state. Dragging a card
  triggers a Kanban work-item; transition + commit + UI update via
  WebSocket (substrate's `version_watcher` → OP's Hotwire stream).
- **Custom Fields panel** displays per-class metadata; can show
  substrate-specific fields (`emitted_at_millis`, `idempotency_key`,
  `trace_id`) alongside OP's domain fields.
- **Notifications + Reminders** wire to substrate observability
  (StateTimeout hits → reminders; Failed transitions → notifications;
  Postpone retries → optional in-card indicators).

The OP UI changes needed are *small adapter layers* between
ViewComponent's render path and the substrate's event stream — most
of the polish is OP's existing UX.

### 1.5 Room 5 — SDK endgame (nexgen's role)

Once OP-on-substrate-b is production-deployed (probably at
OpenProject Edge first, then a willing community instance per
`SUBSTRATE-ENDGAME.md §5.5`):

- **nexgen is the reference graduation pattern** — any Rails app
  following nexgen's structure can graduate similarly. The reflection-
  dump-as-producer-input + HTTP-sidecar-then-native-ractor approach is
  documented per nexgen's experience.
- **nexgen's per-domain crates** become exemplars of the "wide OGAR
  arm" per ADR-011's two-arm naming pattern. Other domains (HIRO/
  Bardioc Elixir, future Go/Swift/etc.) follow this shape.
- **Convergence at `catalog::*`** is the standing demonstration that
  OGAR's general IR + nexgen's special-case path coexist productively
  in the same ecosystem.

## 2. The C16c convergence sprint (load-bearing for SDK Room 5)

Per ADR-012, the C16c sprint is the **convergence point** for the
nexgen + OGAR + surrealdb-fork ecosystem. Concretely:

```
nexgen C16c sprint deliverable:

  impl From<op_surreal_ast::TableMirror> for catalog::TableDefinition {
      fn from(mirror: op_surreal_ast::TableMirror) -> Self {
          catalog::TableDefinition::new_for_ddl(mirror.name)
              .with_schemafull(mirror.schemafull)
              .with_drop(mirror.drop)
              .with_comment(mirror.comment)
              .with_table_type(mirror.table_type.into())
              // ... etc.
      }
  }
  // similar for FieldMirror, IndexMirror, ViewMirror, etc.

OGAR's general path follows the same pattern:

  impl From<ogar_vocab::Class> for catalog::TableDefinition {
      fn from(class: ogar_vocab::Class) -> Self {
          catalog::TableDefinition::new_for_ddl(class.name)
              .with_schemafull(true)  // OGAR's default
              .with_comment(class.description)
              // ... etc.
      }
  }
```

Both `From` impls live alongside each other. nexgen consumers don't
care which one fires (the result is `TableDefinition`); OGAR
consumers go via `Class`. The convergence is at `catalog::*` —
**neither has to know about the other**.

When C16c lands, the architecture diagram in §0 is no longer
aspirational; it's the implementation reality.

## 3. Cross-references

### 3.1 Master doc (OGAR)

- `AdaWorldAPI/OGAR/docs/SUBSTRATE-ENDGAME.md` — the comprehensive
  five-rooms architecture; this view is the nexgen slice.
- `AdaWorldAPI/OGAR/docs/ARCHITECTURAL-DECISIONS-2026-06-04.md` —
  ADR-style backward-looking session capture; **ADR-012** (nexgen
  convergence) and **ADR-019** (OP-as-operator-pane) are the
  nexgen-touching decisions.
- `AdaWorldAPI/OGAR/docs/OPENPROJECT-TRANSCODING.md` — the OP-side
  transcoding spec; §10.2 names the nexgen convergence explicitly;
  §10.4 sequences the producer queue.
- `AdaWorldAPI/OGAR/docs/SURREAL-AST-AS-ADAPTER.md` — the structural-
  vs-behavioral decision (ADR-016); §2 names the
  `From<Class> for catalog::TableDefinition` projection that nexgen's
  C16c sprint anchors.

### 3.2 Companion references

- `AdaWorldAPI/lance-graph/docs/SUBSTRATE-ENDGAME-RUNTIME-VIEW.md` —
  the runtime-side view; pairs with this nexgen view.
- `AdaWorldAPI/surrealdb/.claude/op-codegen-bridge/README.md` —
  Sprint C16b new_for_ddl + with_* builders; the canonical
  external-codegen DDL path nexgen's op-codegen-projection consumes.
- `opf/openproject` upstream — the production Rails app modeled by
  nexgen + graduated in Room 3.

## 4. Open items nexgen owns

In addition to the in-flight items in OGAR's master doc:

- **C16c sprint completion** — `From<op_surreal_ast::*> for catalog::*`
  impls. Convergence point for the SDK Room 5 architecture story.
- **Rails-AR-reflection-dump producer integration** — a `rails runner`
  script in nexgen (or alongside `op-models`) that emits
  `Model.reflect_*` to JSON/Arrow for OGAR's `ogar-from-ruby` to
  consume. Per `SUBSTRATE-ENDGAME.md §2.3.1` — the cheapest beauty
  win.
- **OP Hotwire ↔ substrate `version_watcher` adapter** — wire OP's
  WebSocket / Turbo Stream to the substrate event stream for live UI
  updates. Small adapter layer; depends on Room 2 work-item-form
  trait + first per-class graduation.
- **OP-graduation per-class strategy** — sequence OP's models for
  graduation. `WorkPackage` and `Project` are likely highest-priority;
  smaller / less-coupled models can graduate in parallel batches.

## 5. Doc lifecycle

- **Author:** OGAR session 2026-06-04 (placed cross-repo 2026-06-05).
- **Status:** Mirror view; master in OGAR.
- **Update cadence:** when the master doc updates, mirror relevant
  changes here. When nexgen-side items graduate (e.g. C16c ships,
  first OP class graduation lands), add one-line updates.
- **Authority:** master in OGAR. This view is for navigation;
  decisions cite OGAR docs.

## 6. Compact map — nexgen-side dependencies in priority order

For a nexgen session to pick up where this view leaves off:

1. **Complete C16c sprint** — `From<op_surreal_ast::*> for catalog::*`
   impls. Foundation for all subsequent OGAR convergence.
2. **Rails-AR-reflection-dump script** — sharpen OGAR's
   `ogar-from-ruby` producer extraction without embedding.
3. **First OP per-class graduation** (in coordination with runtime
   session) — `WorkPackage#save` through Room 2's HTTP-sidecar path,
   then native ractor handler via Rubicon-from-OGAR.
4. **Hotwire ↔ version_watcher adapter** — OP UI live-updates from
   substrate state.
5. **Workflow → live Rubicon machine** — operator-edited Workflow
   rows regenerate Rubicon dynamically (requires Rubicon Phase 2
   from runtime session).
6. **Per-class graduation parallelism** — `Project`, `User`, `Role`,
   `Status` after `WorkPackage` proves the pattern.
7. **First production deployment** — OpenProject Edge or community
   instance.

Each unlocks the next; pairs with runtime-side dependencies in the
lance-graph view.
