# ORE, SLAG and behavioural reconciliation

This document explains how nexgen compares what OpenProject's Ruby source declares
with what the compiled Rust does, how it records what neither side can settle, and how
such a record moves towards resolution. It covers WorkPackage create admission, the
first operation reconciled this way.

The code:

- `crates/op-codegen-pipeline/src/reconcile.rs`: claims, the Charon store reader,
  structural verdicts, the Markdown drilldown.
- `crates/op-codegen-pipeline/src/resolution.rs`: residuals, owner stages, proposals,
  evidence with file hashes, the lifecycle, the ledger and the scorecard.
- `crates/op-codegen-pipeline/tests/reconcile_create_admission.rs`: the OpenProject
  claims and runtime attachments.
- `crates/op-codegen-pipeline/tests/ore_slag/resolution.rs`: the OpenProject
  proposals and witnesses, the ledger check and the negative controls.

Generated artifacts, never edited by hand:

- `oracle/charon/reconcile-create-admission.md`: the Markdown drilldown.
- `oracle/charon/reconcile-create-admission.ledger.json`: the machine-readable ledger.

Both come from the same in-memory reconciliation in one test run. Neither is a second
source of truth.

## 1. What ORE represents

ORE is the set of structural facts an extractor produced from source code, each
addressed and located. There are two readings:

- **Ruby ORE.** ruff's Ruby frontend (`ruff_ruby_spo`) reads the OpenProject checkout.
  For create admission it supplies the lowered `writable:` guards
  (`extract_contract_guards`). Two further facts come from weaker readers, and each row
  names its reader: the `admission_gen` permission line scan
  (`ExtractionGap::PermissionDeclScan`) and literals located in the source without a
  parse.
- **Rust ORE.** Charon compiles `op-contracts` and `op-services`; `ruff_charon` imports
  the result into `oracle/charon/<crate>.{rows,residuals,files,effects}.tsv`, with
  `PROVENANCE.txt` recording the revisions. Rows are `DEFINES`, `CALLS_RESOLVED`,
  `MAY_DISPATCH`, `SYNTHETIC` and others, each with a byte range in a file whose
  FNV-1a 64 hash is recorded.

ORE is structure. It says that a call edge exists, or that a guard lowers to
`leaf? || schedule_manually?`. It does not say that anything ran.

## 2. What SLAG means

A claim is SLAG when the witnesses cannot decide it. SLAG is typed: every row names
the missing fact, the stage that would supply it, and the evidence that would settle
it. SLAG is not necessarily a defect. It marks where the current extractors stop.
Examples:

- A lowered guard stored as a row of a generated table is invisible to a call graph.
  Charon finds no edge because there is no call, and the behaviour is still there
  (`RepresentedAsData`).
- `mutation::create` is an `async fn`. Charon recorded `BODY_NOT_STRUCTURED` for it,
  so the absence of an edge out of it means nothing (`RustBodyNotStructured`).

Absence of an extracted row never counts as absence of code or behaviour. `RubyOnly`
and `CharonOnly` are used only when the other side was inspected: crate in the store,
function defined, body structured, file unchanged since extraction, behaviour not held
as data. On the Ruby side, the span was read and the fact is not there.

## 3. Witness boundaries

| Witness | Reads | Establishes | Never establishes |
|---|---|---|---|
| Ruby ORE | OpenProject source at the trace revision | declarations, lowered guards, located literals | that Rails ran them, or in which order |
| Charon ORE | compiled `op-contracts`, `op-services` | defined functions, resolved call edges, data items | runtime values; anything in a crate not in the store |
| Frozen Ruby oracle | `oracle/*-trace.json`, produced by OpenProject itself | what Rails did on the recorded cases | anything outside those cases (it is not a completeness oracle) |
| Executed Rust differential | production Rust run against the oracle | whether Rust's outcome matches on those cases | structure; anything the case does not exercise |
| EXECUTED_DB | `op-server` journey on PostgreSQL | which rows the Rust paths commit | Rails persistence parity |

The structural verdict uses the first two witnesses only. The runtime verdict uses the
executed ones. One never changes the other: the reconciliation test asserts that
attaching runtime evidence leaves every verdict unchanged, and the resolution test
asserts the same for the lifecycle.

## 4. Semantic identity and comparison rules

A claim's identity is `Application/Model/Operation/Phase/Method/Effect`, for example
`openproject/WorkPackage/create/guard/WorkPackages::BaseContract.attribute(:status_id)/guard:lowered`.
A bare method name never joins two witnesses. One Ruby method can hold several claims
(a registration, an invocation, a guard, a write), and each gets its own verdict.

Structural verdicts: `Agreement`, `Disagreement`, `RubyOnly`, `CharonOnly`,
`Slag(reason)`, `NotComparable`. The decision order:

1. Is the Rust side inspectable? If not, the row is `Slag` with a Rust reason.
2. Was the Ruby side harvested? If not, the row is `Slag(RubySideNotHarvested)`.
3. Do the two sides speak about the same level? If not, the row is `NotComparable`.
   Example: a DSL registration (`validate :user_allowed_to_add`) compared with a
   direct call has no registration step on the Rust side.
4. Otherwise compare: present against present is `Agreement`; a conflicting edge is
   `Disagreement`; present against inspected-absent is `RubyOnly` or `CharonOnly`.

Runtime verdicts: `ExecutedAgreement`, `ExecutedDisagreement`, `ExecutedRustOnly`,
`NotExecuted`, `Unverified`. A gate that asserts something but did not run in this
test is `Unverified`, never an agreement.

## 5. Agreement is not behavioural equivalence

Each level below is recorded on its own. None is inferred from another.

| Level | Meaning | What it does not imply |
|---|---|---|
| Extraction | previously unread code is now in ORE | that its semantics were reconstructed |
| DataWitness | generated data equals its generator input, item by item | a call-graph agreement; that the data runs |
| Semantic | conditions, ordering and generated behaviour reconstructed | that it executed |
| StructuralAgreement | comparable facts agree at one address | that both transitions ran the same way |
| ExecutionAgreement | both transitions executed and outcomes match | that persisted effects match |
| EffectsAgreement | rows, callbacks, journals and jobs match per the oracle | anything outside the recorded cases |

What follows from this:

- A reconstructed callback is not an executed callback.
- A Charon call edge is not a runtime observation.
- A matching permission table is not full contract validation. The
  `schedule_manually` block below shows a rule the table does not hold at all.
- A committed PostgreSQL row is not Rails persistence parity.
- A callback held as generated data is not missing because Charon finds no call.

## 6. SLAG reasons, owners and remedies

Structural reasons (`reconcile::SlagReason`), each with its default owner stage
(`resolution::OwnerStage::default_for`):

| Reason | Missing fact | Owner | Remedy |
|---|---|---|---|
| `RustSideNotExtracted` | the crate's Charon artifacts | RustExtraction | run Charon on the crate, add its four files |
| `RustSymbolNotInStore` | the function in an extracted crate | RustExtraction | re-extract (renamed, moved, cfg'd out) |
| `RustEvidenceStale` | current byte offsets | RustExtraction | re-extract at the current revision |
| `RustBodyNotStructured` | a structured body | RustExtraction | Charon async structuring, or a synchronous seam |
| `RepresentedAsData` | nothing behavioural: table data has no edges | DataWitness | compare the data with its generator input |
| `RubySideNotHarvested` | the source fact | RubyExtraction, or ContractHarvest for contract arms | a ruff extraction rule |

The other owner stages are `IntakeConfig` (which roots and engines are read),
`RailsPolyfill` (generic framework semantics, e.g. association-generated callbacks)
and `RuntimeWitness`. A caller may refine the default owner with data. The two
contract rows below are `ContractHarvest`.

The callback ledger has its own closed reasons (`callback_harvest::Slag`, `Silent`).
Each variant's doc comment names its stage: `FromGem` and `ModelWithoutClassFile` are
Rails/gem polyfill; `DefinedInPlugin` is intake configuration plus a ruff `acts_as_*`
expansion; `GeneratedUndecided` is a ruff option evaluation. These counts are listed
in the scorecard (section 10), not folded into the 21 claims.

## 7. The resolution lifecycle

Each SLAG row becomes a `resolution::Residual` with `reason`, `sub_reason`,
`owner_stage`, `locator`, `proposal`, `evidence`, `regression_gate`, `status` and
`attempts`. The status is recomputed from the evidence on every run
(`Residual::evaluate`):

```
OPEN -> CANDIDATE -> EVIDENCED -> VERIFIED(level) -> RESOLVED(level)
```

- **OPEN**: no candidate. Either the supplier does not exist yet, or every attempt
  was rejected.
- **CANDIDATE**: a located source and an existing supplier are named, but nothing
  has been run. Example: a crate whose Charon extraction is a command away.
- **EVIDENCED**: current evidence is attached, but it establishes no level. Example:
  the drift check over a line-scan input.
- **VERIFIED(level)**: a witness established the level against current evidence,
  and its source side is parsed ORE.
- **RESOLVED(level)**: verified, and a named gate fails if the evidence regresses.

Evidence is promoted only if it compared something, found zero mismatches, rests on
at least one file whose hash still matches (evidence resting on no file cannot go
stale, so it is rejected), and, for a data witness, reads parsed ORE (not a line
scan). A residual may also name companion evidence kinds that must be present and
passing before any level is established (the guard rows require the drift check). It is invalidated, moved into `attempts` with the reason, and the residual
falls back to OPEN or CANDIDATE, when:

- a file it rests on changed (`stale: <path> changed since the evidence was produced`);
- the comparison has mismatches;
- it rests on no file;
- its checkout is not available (it cannot verify, so it cannot promote).

A data witness resolves only `RepresentedAsData`. Evidence attached to any other
reason leaves that residual EVIDENCED at most. Runtime evidence is never residual
evidence. It sits on the claim's runtime axis.

Each proposal (`resolution::Proposal`) is inert. It records:

- the missing fact;
- the supplier, and whether that supplier exists today;
- the exact source location and revision;
- how the claim would change;
- the independent evidence that would validate it;
- the negative control that would invalidate a wrong resolution.

The resolver edits no code and no configuration.

## 8. Walkthrough: one claim end to end

`openproject/WorkPackage/create/guard/WorkPackages::BaseContract.attribute(:status_id)/guard:lowered`

1. **Ruby declaration** (`app/contracts/work_packages/base_contract.rb:39`):
   `attribute :status_id, writable: ->(*) { status_id_change || !closed_version_and_status? }`.
   The helper is inlined from the same class.
2. **Ruby ORE.** ruff lowers it to
   `Or(Fact("model.status_id_change"), Not(And(Fact("model.status&.closed?"), Fact("model.effective_target_versions.any?(&:closed?)"))))`.
3. **Rust/Charon ORE.** `admission_gen` turns that expression into a postfix program in
   `crates/op-contracts/src/work_packages/generated_create_admission.rs` (`ATTRIBUTES`,
   lines 16-46). Charon records the table as a `SYNTHETIC` promoted constant at bytes
   1987..5866. The file hash matches the checkout.
4. **SLAG.** No call edge can carry a table row: `Slag(RepresentedAsData)`, owner
   `DataWitness`.
5. **Proposed remedy.** A data witness: ruff's expression against the generated row,
   compared through the production evaluator. Supplier exists. Negative control: swap
   `And`/`Or` in the input, or rotate the attribute mapping.
6. **Verified evidence.** Two pieces:
   - The drift check: `admission_gen::generate(src)` is byte-identical to the
     checked-in table. This is the existing gate's predicate, bound and not
     re-implemented.
   - The truth table: all 8 assignments of the three facts give the same answer in
     Ruby's expression and in production `is_writable("status_id")` for a member
     holding the oracle permissions, and the table reads exactly those three facts.
     9 items compared, 0 mismatches. The truth table rests on
     `app/contracts/work_packages/base_contract.rb`, the generated file and
     `admission.rs`, by hash. The drift check rests on the application's
     `app/contracts/base_contract.rb`, `work_packages/base_contract.rb` and
     `work_packages/create_contract.rb` plus the generated file. The generator scans
     the whole `app/contracts` tree; a change to another file there is caught by the
     drift check re-running, not by these hashes.

   Status `RESOLVED(DataWitness)`, and only together with the drift check: the drift
   check is a required companion of these rows. If it is missing or fails, it is
   moved to `attempts` (when failing) and the truth table alone leaves the row
   `EVIDENCED`. Regression gates: this test and the drift check.
7. **Structural verdict**: still `Slag(RepresentedAsData)`. It is not upgraded to
   `Agreement`, because a data witness is not a call-graph fact.
8. **Runtime verdict**: `ExecutedAgreement`, with 6 records compared and 0 mismatches.
   Production `is_writable` ran against the contract trace's `writable[status_id]`.
   This is a separate axis.

## 9. One success, one deliberately unresolved

**Success: the eight lowered `writable:` guards.** These are `author_id`, the three
`derived_*`, `done_ratio`, `due_date`, `ignore_non_working_days` and `start_date`. All
eight are `RESOLVED(DataWitness)` as in section 8, and all eight stay
`Slag(RepresentedAsData)` structurally. (`status_id` was the ninth. `CreateContract`
redeclares it with no `writable:` (`create_contract.rb:40-44`), which stores `nil` and
shadows the base lambda, so a create has no status guard and the row is gone.) `ADD_PERMISSION` is held as data too, but it
stays `EVIDENCED`. The drift check passes, and production admission authorizes with
exactly the scanned permission and refuses with every other oracle permission (2/2).
Its Ruby input is still the permission line scan, not parsed ORE. Once a ruff
permission lowering feeds the generator, the same witness can verify it.

**Closed: `attribute :schedule_manually do validate_has_predecessors_or_children end` (claim 9).**

- The source is `app/contracts/work_packages/create_contract.rb:46-48`, with the
  validator defined at line 68: `if model.schedule_automatically?`, then `errors.add(:schedule_manually,
  :cannot_be_automatically_scheduled)` unless the parent has scheduling relations or
  the record has children. `BaseContract.attribute` registers the block with
  `validate(attribute, &block)` (`app/contracts/base_contract.rb:89`), so it runs on
  every validation, whether or not `schedule_manually` was submitted.
- Ruby side: harvested. The permission scan still skips the block, but ruff's
  `extract_attribute_blocks` (ruff #160, `180cf4f`) reads it: class
  `WorkPackages::CreateContract`, attribute `schedule_manually`, call
  `validate_has_predecessors_or_children` at `create_contract.rb:47`, condition
  `model.schedule_automatically?` lowered to `Fact`. The claim's Ruby fact exists only
  when all of those hold.
- Rust side, before the validator was ported: `admit_create` had no edge to such a
  validator, `NOT_EVALUATED_VALIDATORS` did not list it, and no crate contained the
  string `cannot_be_automatically_scheduled`: a silent omission. Now `admit_create`
  evaluates it on every create (`validate_has_predecessors_or_children`, from the facts
  `model.schedule_manually?`, `model.children.any?` and
  `Relation.used_for_scheduling_of(model.parent).any?`), so it stays out of
  `NOT_EVALUATED_VALIDATORS`. A fact that blocks the error decides without the others;
  an unknown fact with no blocker makes the verdict unresolved. A create supplies what
  it determines itself: `schedule_manually` is the column default `true`, there are no
  children, and no scheduling predecessors unless a parent is submitted.
- Executable coverage before this change: none. `create_transition_differential`
  compares `writable?` per attribute and the service trace's errors; the service
  cases set no `schedule_manually: false`. The reconciliation compared only `errors[base]`.
- This change adds a separate runtime case. For each of the 6 `CreateContract` records in
  the contract trace, it asks whether Ruby records `schedule_manually:
  cannot_be_automatically_scheduled` and whether production
  `CreateWorkPackageService::admit` produces it. Admit is asked with
  `schedule_manually` both unsubmitted and submitted.
  - Ruby fires on `leaf_automatic` only, with recorded facts `schedule_manually: false`
    and `children.any?: false`, and no parent.
  - Rust fires on `leaf_automatic` only.
  - The 5 records where both are silent are not counted as agreements.
  - Compared: 1 discriminating record, 0 mismatches: **ExecutedAgreement** on the
    error key. It is not added to the 350 contract-rule items.
- Structural verdict: `Agreement` (before: `Slag(RubySideNotHarvested)`). The row has
  no residual. The comparison is of the call: Charon sees `admit_create ->
  validate_has_predecessors_or_children` as `CALLS_RESOLVED`, and the Rust call is
  unconditional, with the condition evaluated inside the validator through
  `FACT_SCHEDULE_MANUALLY`. The structural verdict does not compare the condition;
  the differential above witnesses it.
- Negative controls (`ore_slag_negative_controls`, control 9): a different call name,
  a missing block, a call with its condition dropped and a call under another
  condition each leave the Ruby side unharvested, so the same Rust edge yields
  `Slag(RubySideNotHarvested)`; a renamed Rust callee yields `RubyOnly`.
- The validator is implemented in `admit_create`; its differential case is the one
  above, kept apart from the 350. The ledger and report are regenerated from a Charon
  extraction that includes the changed Rust files: until then the stored ledger still
  shows the earlier numbers.

## 10. Scorecard (this branch; ruff `180cf4f`, OpenProject `2725194`, store from nexgen `8d38cd3`, crates op-contracts, op-services, op-api)

Separate denominators. Before means #138 as it was; after means this change.

| Measure | Before | After |
|---|---|---|
| Claims | 21 | 20 (the create `status_id` guard row is gone: `CreateContract` shadows the base condition) |
| Ruby side inspected (fact or inspected absence) | 18 / 21 | 18 / 20 |
| of those from a parser-backed extractor (ruff) | 9 / 18 | 9 / 18 |
| Rust counterpart inspectable in the store | 18 / 21 | 17 / 20 |
| Structural: Agreement / Disagreement | 4 / 0 | 5 / 0 |
| Structural: RubyOnly / CharonOnly / NotComparable | 0 / 1 / 1 | 0 / 1 / 1 |
| Slag rows | 15 | 13 (claim 9 is now Agreement; the status guard row is gone) |
| Residuals open, by reason | not typed | RubySideNotHarvested 1, RustBodyNotStructured 3, RepresentedAsData 1 (op-api is in the store; claim 12 is now `RustBodyNotStructured`) |
| Residuals open, by owner | not typed | ContractHarvest 1, RustExtraction 3, DataWitness 1 |
| Resolved by extraction / data witness / semantic rule | 0 / 0 / 0 | 0 / 8 / 0 |
| Lifecycle | none | RESOLVED(DataWitness) 8, EVIDENCED 1, OPEN 4 |
| Runtime verdicts over the claims | 10 executed (0 mismatches), 3 cited, 8 none | ExecutedAgreement 10, ExecutedDisagreement 0, Unverified 3, NotExecuted 7 (20 claims) |
| Contract-rule items executed against the oracle (`create_transition_differential`) | 350, 0 mismatches | 350, 0 mismatches (unchanged; the new case is separate) |
| Lowered guards against the oracle (`guard_differential`) | 84 pairs, 0 mismatches | 84, 0 |
| `schedule_manually` block differential | not run | 1 discriminating record, 0 mismatches |
| Callback ledger (`callback_differential`) | 115 / 157 matched, 42 slag (ruff 79ed2ba) | 154 / 157, 3 slag (ruff b3bdfb1, from #140; not this PR) |
| Callback slag by reason | AssociationNotInCheckout 15, FromGem 8, DefinedInPlugin 7, AssociationFromPlugin 5, GeneratedUndecided 4, ModelWithoutClassFile 3 | AssociationNotInCheckout 2, GeneratedUndecided 1 |
| Callback order | 19 groups, 19 definite, 0 differ | 29 groups: 29 definite, 0 UNKNOWN, 0 differ (ruff ce96aae `hidden_complete`; 16 / 13 between the #140 review fix and this change) |
| Callback bodies executed | 0 | 0 |
| Database effects (EXECUTED_DB, PostgreSQL 16) | journey 7/7, 13 rows asserted | journey 14/14, 299 rows asserted; create transactional, creation journal, default priority, creator as watcher, ancestor totals on create and on update (derived hours and done ratio, excluded status, caused journals, former and new chains after a move, rollback, concurrent create caught by lock_version), switch to automatic mode and `ignore_non_working_days` of the ancestors, rescheduling (successors by working days with lag, calendar days, a non-working day, parent dates from children, the former parent after a move, rollback, a stale rescheduled row caught by lock_version), the dates taken before the save (a new work package's default dates from its parent, an edited automatic successor snapping to its predecessor's soonest start within its own UPDATE, a given due date kept, a manual one untouched, an automatic parent taking its children's span, a work package moved under a parent with a predecessor); update transactional with its journal (no-op, aggregated, new version, rollback); cited by the reconciliation, run separately |

The P3b gaps are reported, not hidden:

- Closed: the create is transactional. The authorization reads, defaults and
  INSERT run on one transaction; a failure after the INSERT leaves no row
  (EXECUTED_DB step 7, `transactional create: YES`, with a committing control).
- Closed: the create writes its creation journal on the same transaction (one
  `journals` row at version 1 and one `work_package_journals` row copied from the work
  package; EXECUTED_DB step 2, and step 7 shows a post-insert failure rolls it back).
  The create defaults the priority as Ruby does (`IssuePriority.active.default`,
  `set_attributes_service.rb:237-239`); with no default it fails `priority: blank`
  (`work_package/validations.rb:34`) instead of inserting NULL, so
  `work_package_journals.priority_id` keeps its NOT NULL.
  Not ported of that journal: the association rows (attachments, custom values,
  labels, versions: empty for every work package this create writes), the
  `JOURNAL_CREATED` notification and aggregation.
- Closed: the update is transactional and journals like `Journals::CreateService`.
  `PgMutationStore::update_then` runs the advisory lock, the lookup and authorization,
  the UPDATE and the journal on one transaction (`Shared::ServiceContext`,
  `service_context.rb:34-49`). The journal follows `create_service.rb`: no journaled
  column changed means no journal (`changes` CTE, :509-532); the same user within
  `journal_aggregation_time_minutes` (default 5, `definition.rb:770-773`) with an empty
  cause and no note rewrites the last journal in place (`aggregatable?` :595-601,
  `update_journal_sql` :437-461), so an edit right after the create is NOT a version 2;
  any other case adds the next version and closes the predecessor's validity period
  (`update_predecessor_sql` :410-425). EXECUTED_DB steps 4, 8 and 9 assert each case,
  a rollback after the journal, a stale `lock_version`, and a work package with no
  journal yet. Not ported: the association rows, notifications, notes and causes,
  `skip_aggregation`, journaling of the anonymous demo editor (this schema has no
  `users.type`, so the anonymous caller may not edit at all: a deliberate divergence,
  docs/decisions/0001-journaled-edits.md §2), and Rails' skipped
  UPDATE for a no-op save (a no-op edit still bumps `lock_version` and `updated_at`;
  it writes no journal). The aggregation window is a constant because there is no
  settings table. An edit of a work package stored without a priority, that does not set
  one, is refused with `priority: blank` (`validates :priority, presence: true`,
  work_package/validations.rb:34, via `ModelContract#valid?`), as in OpenProject.
- Closed: `after_create`'s `set_user_as_watcher` (create_service.rb:82, :118-122). The
  creator becomes a watcher on the create's transaction when not locked and allowed
  `view_work_packages` in the project (an admin always); otherwise the create succeeds
  without one, as `Services::CreateWatcher` skips an invalid `Watcher`. EXECUTED_DB step 2
  (watcher written), the add-only member before step 8 (none), and step 7 (rolled back with
  the create) assert each case. Not ported: the `WATCHER_ADDED` notification (sent with
  `send_notifications: false`), non-member access to public projects, module checks.
- Closed: `after_create`'s `multi_update_ancestors` (create_service.rb:75-77) for a work
  package created with a parent. On the create's transaction, between the creation
  journal and the watcher (Ruby's order), each ancestor, parent first, derives
  `derived_estimated_hours` and `derived_remaining_hours` (the sum of the non-null values
  over the ancestor and all its descendants whose status is not `excluded_from_totals`;
  `nil` with no descendants or no value, `update_ancestors_service.rb:219-228`,
  `WorkPackage#included_in_totals_calculation?`, work_package.rb:295-297) and
  `derived_done_ratio` (work-weighted average,
  `round((estimated - remaining) / estimated * 100)`, `nil` when either total is `nil`, the
  estimate is zero or there are no descendants, :129-138; `Float#round` is half away from
  zero, so the value is computed in Rust, not with Postgres' half-to-even `round`; the sum
  is Ruby's compensated `Array#sum`). Only an ancestor whose values changed is saved
  (`lock_version` and `updated_at` bump) and journaled with cause
  `{"type": "work_package_children_changed_values", "work_package_id": <new id>}`, as the
  acting user, through the update journal's rules (create_service.rb:595-627): the journal is
  written even with no other change, the last journal is rewritten in place when it is the
  same user's, within the window, with an empty cause or this very cause (the rewrite takes
  the new cause), and a predecessor with a different non-empty cause is never aggregated
  into (`Base#merge_into` is `nil`). New columns in migration 0003:
  `work_packages.derived_*` and `statuses.excluded_from_totals`; the journal snapshot copies
  and compares the derived columns. The create now also accepts `remaining_hours`. The mode
  is the constant `TOTAL_PERCENT_COMPLETE_MODE` (`work_weighted_average`, the default of
  `total_percent_complete_mode`, definition.rb:279-283) because there is no settings
  table. EXECUTED_DB step 10 asserts, on a three-level chain G > P > C: P and G derived
  values by hand (G: 200 / 199 hours, `round(0.5) = 1`), the grandparent updated too, an
  excluded-status child leaving every row and journal byte-identical, the caused journal
  aggregated for the same user and not aggregated across a different cause, a new version
  for another user, the journal data rows carrying the derived values, and a failure after
  the ancestor update leaving ancestors, their journals and the child untouched (with a
  committing control. Not ported: `simple_average` and status-based modes, former
  ancestors (a create has none), notifications. (`switch_to_automatic_mode` and
  `derive_ignore_non_working_days` are the next-but-one item below.) Like Ruby's create it
  takes no lock on the ancestors.
- Closed: ancestor totals on update (`WorkPackages::UpdateService#update_related_work_packages`,
  update_service.rb:54-81, `update_ancestors`, shared/update_ancestors.rb:42-48). On the
  update's transaction, after the UPDATE and its journal, inside the advisory lock. The
  attributes the edit changed (estimated hours, done ratio, parent, status: the row before
  against after) must intersect `modified_attributes_justify_derivation?`
  (update_ancestors_service.rb:239-248) or nothing is touched; then `derive_attributes`
  (:96-121) runs only the derivations those attributes name: the derived hours for hours,
  status or parent, the derived done ratio for those and the done ratio (so a done-ratio-only
  edit recomputes the ratio from the derived hours as loaded). The candidates are those of
  `Loader#select` (loader.rb:54-60): the edited work package itself, its ancestors and, when
  the parent changed, the former parent and its ancestors (:84-94, :139-146). A status
  excluded from totals changes the sums, and a status change alone starts the derivation.
  The edited work package is saved only if its own derived values changed; its journal
  aggregates into the edit's journal (same user, no cause: one journal whose snapshot holds
  the edit and the derived values), and its `lock_version` takes the second save (the
  response carries the final value). Every other candidate is saved when changed and
  journaled with the child-values cause of the edited work package. Each save checks
  `lock_version` as loaded (a mismatch is a conflict and rolls the update back); the
  edited work package is re-read after its own UPDATE, so it does not conflict with
  itself. Create and update share one function. EXECUTED_DB step 11 asserts, by hand
  (comments give the arithmetic): an estimate edit moving the parent and grandparent
  (`round(11.11) = 11`), a status edit to an excluded status dropping the hours out with new
  journal versions for another user, an edit of a parent with children (own totals
  `70 / 50 / 29`, one aggregated journal, `lock_version` 5, grandparent `round(11.76) = 12`),
  a move between parents recomputing the former chain (to all nil) and the new chain with a
  new journal each, a subject-only edit writing no ancestor row and journal (with a
  hand-corrupted derived value that a recompute would repair), the done-ratio-only edit, and
  a failure after the ancestor writes leaving everything unchanged (with a committing
  control). Not ported: `simple_average` and status-based modes, `update_descendants` on a
  project change, notifications (`reschedule_related` is the bullet below).
- Closed: the two remaining `UpdateAncestorsService` derivations, on create and on update
  (update_ancestors_service.rb:96-121), both selected by a `parent` / `parent_id` change and run
  per candidate in Ruby's order, on the totals' save, journal and `lock_version` path (an
  ancestor whose only change is a scheduling flag is saved and journaled with the child-values
  cause; the journal snapshot copies and compares `ignore_non_working_days`; migration 0004
  adds `work_packages.ignore_non_working_days`, structure.sql:9034).
  - `switch_to_automatic_mode` (:157-181): only the initiator's current direct parent, only if it
    is `schedule_manually`, has exactly one child, and `Relation.used_for_scheduling_of(parent)` is
    empty. The scope (relations/scopes/used_for_scheduling_of.rb:44-52) is ported over `parent_id`
    with a recursive CTE, because this schema has no `work_package_hierarchies`: the parent itself
    (closure_tree's generation-0 row; `WorkPackage::Ancestors::Aggregator` filters
    `generations > 0` to drop it) and its automatically scheduled ancestors, that is the
    ancestors that are not manual and not above a manual one (:55-59). `follows` is
    `relation_type = 'follows'` and `from_id` is the successor (relation.rb:41-44, a `precedes`
    is stored reversed, :210-220). `state.bulk_duplicate_in_progress` is false for a plain
    create or update, and the child is never destroyed here, so neither exits early.
  - `derive_ignore_non_working_days` (:183-199, :250-256): never the initiator, never a manually
    scheduled candidate (read after the switch of the same pass, so a parent just switched takes
    its child's value in that save); otherwise true if any child has it. With no child left the
    candidate keeps its own value: Ruby's comment (:179-182) says the former parent "is reset to
    `false`", but the code (:190-192) assigns the ancestor's own value when
    `ignore_non_working_days_of_descendants` is nil, and the port follows the code. A former
    parent that keeps another child is recomputed from it (reset to false when that child is
    false).
  EXECUTED_DB step 12 asserts, by hand: a first child switches a manual parent (saved once,
  lock 0 to 1, aggregated journal with the child's cause and a switched snapshot, the initiator
  untouched); a second child leaves it manual and writes nothing; a `follows` predecessor of the
  parent keeps it manual, while a parent that is itself the predecessor switches; an automatic
  ancestor's predecessor keeps a manual grandchild-parent manual, while a manual ancestor cuts the
  chain off and the parent switches; a move switches only the new parent (the former parent, manual
  with one child left, is untouched), with a new journal version by the other user; a child with
  the flag moved under a manual lone-child parent makes it automatic and true in one save, under
  an automatic parent with another child makes it true, and under a manual parent with two
  children changes nothing; moving the true child away resets the former parent that keeps a
  false child, and leaves one with no child true. Not ported: `simple_average` and status-based
  modes, notifications (`reschedule_related` is the bullet below).
- Not ported, schema-absent: `apply_patterns` (create_service.rb:74,
  `app/services/types/apply_patterns.rb`) is a no-op over this schema. It resolves
  `model.type_variant&.enabled_patterns` (`WorkPackage#type_variant`, work_package.rb:289-292),
  and the `patterns text` column lives in `type_variants` (structure.sql:8072), a table this schema
  does not have (there is no `types.patterns` either): no type variant means no pattern to apply.
- Closed (core, with a Not-ported list): `reschedule_related`, on create (`after_create`, after the
  ancestors and before the watcher, create_service.rb:79, :90-110) and on update
  (`update_related`, after the ancestors, update_service.rb:62-65, :149-170), over
  `SetScheduleService` (set_schedule_service.rb:30-203). The pure part is
  `work_packages/scheduling.rs`, the loading and saving is `mutation/reschedule.rs`.
  - Which work packages are looked at: `WorkPackage.for_scheduling`
    (work_packages/scopes/for_scheduling.rb:119-262), ported as a walk over the moved ones:
    successors by `follows` relations, children and ancestors (the ancestors also from an origin
    that is itself manual), stopping at a manually scheduled work package unless it is the
    origin, and a parent counts as manual only if all its descendants are. Then the descendants
    and predecessors that influence the dates (`preload_scheduling_data`,
    schedule_dependency.rb:160-190). The hierarchy is read from `parent_id`: this schema has no
    `work_package_hierarchies`.
  - The order and the dates: `DependencyGraph#schedule_order` (dependency_graph.rb:34-51), then
    per work package, with the state the earlier ones just produced: switch to automatic if it is
    forced (create of an automatic work package), then by its descendants (`start` = earliest and
    `due` = latest of all their dates, a parent whose children have no dates loses its own), else by
    its predecessors (soonest start from `due_date || start_date` plus the lag in working days,
    moved to the next working day of the successor; the due date follows the duration and is at
    least the start; no due date and no duration leaves it nil), else, unless forced automatic, it
    becomes manual (set_schedule_service.rb:108-116). Moving a predecessor earlier moves the
    successor earlier: the code assigns the soonest start whichever way, whatever the comment at
    :143-146 says.
  - Working days: `Setting.working_days` is the constant Monday to Friday (definition.rb:1548-1553),
    `non_working_days` is migration 0005 (structure.sql:5299-5305, :15401), `ignore_non_working_days`
    on the work package selects calendar days (`AllDays`, shared/days.rb); the lag is always counted
    in working days (all_days.rb:46-49).
  - Saving: each altered work package once (Ruby consolidates per work package,
    update_service.rb:172-185; here there is one source, so one save), `UPDATE ... WHERE
    lock_version = <as loaded>` (a mismatch is a conflict that rolls the create or update back),
    `lock_version` and `updated_at`, one journal through the same aggregation rules. The cause is
    `Journal::CausedByWorkPackageRelatedChange` with the edited work package
    (`{"type": "work_package_related_changed_times", "work_package_id": ...}`,
    set_schedule_service.rb:184-203; the predecessor and child causes are commented out in Ruby).
    It is assigned only when a date or the duration changed, and a create has no initiator, so a
    create writes no cause.
  - Two existing expectations changed because Ruby now acts: a former parent that is automatic,
    has lost its last child and has no predecessor becomes manual (step 11 D for M1: lock 3 and a
    second journal with an empty cause, which cannot aggregate into the child-values one; step 12 I
    for F2: now manual, lock 2).
  EXECUTED_DB step 13 asserts, by hand (comments give the dates): a successor with lag 2 moved
  from Mon 9 .. Tue 10 to Wed 11 .. Thu 12 by the predecessor's due date moving to Fri 6, with the
  related-change journal (and back, aggregated); a manual successor, a manual parent and the work
  packages behind a manual one untouched; an edited manual parent keeping its own dates;
  `ignore_non_working_days` giving Sat 7 .. Mon 9; a non-working day moving the result to Thu 12 ..
  Fri 13; an automatic parent spanning its children (Mon 16 .. Tue 24, 7 working days); a create
  under an automatic parent (aggregated, no cause); a move recomputing the former parent; rollback
  with a committing control; a second transaction's stale save of a rescheduled row a conflict.
  The duration is derived as Ruby's `set_calculated_attributes` does (set_attributes_service.rb:92-107,
  :158-185) for the dates of a create (the INSERT carries it) and an update (the UPDATE writes it):
  milestone unification and duration 1 (`types.is_milestone`), the shift of the dates to working
  days, then the first of duration, due date and start date that was not given and whose other two
  are present (else exactly one present) is derived, in working or calendar days by
  `ignore_non_working_days`. A date counts as given when set on create and when it differs from the
  stored one on update (the update carries every current value forward). Step 13 asserts a
  Rust-created successor (Mon 9 .. Thu 12, duration 4 from the INSERT) moved to Wed 11 .. Mon 16
  with its four working days, and edits deriving 7 and 11 working days (a Saturday due date moves to
  Monday) and 9 calendar days.
- Closed: the dates an edit and a create take before the save (EXECUTED_DB step 14). On create,
  `set_default_start_date` / `set_default_due_date` (set_attributes_service.rb:241-257, called for
  a new record by :217-223): without a date from the caller, the start is the parent's start when
  it is earlier than the due date (`parent_start_earlier_than_due?`, :542-547; `due` is the
  work package's own or the parent's) and the due date is the parent's when it is later than the
  start (`parent_due_later_than_start?`, :549-554, which reads the start just assigned); a date
  the caller gave is never defaulted (`attributes.has_key?`, :242, :252). The create params carry
  `Option` dates, so a date is "given" exactly when it is `Some`; a key given as `nil` cannot be
  said and counts as absent. `Setting.work_package_startdate_is_adddate` is a constant at its
  default `false` (definition.rb:1545-1547): its "today" branch (:246-247) is ported behind the
  constant and tested with the setting passed in, but never taken by the store. On update,
  `update_dates` runs before the save (:111-118, :337-381), so the UPDATE writes its result:
  without children, a manual work package with a start date is left alone (:484-489), without
  one it takes the parent's soonest start; an automatic one takes the soonest working day of the
  later of the new parent's soonest start (only when the parent changed, :497-504) and its own
  `soonest_start` (the `follows` relations of itself and its automatically scheduled ancestors,
  `scheduling_rules.rb:61-71`), keeps a due date the edit gave (:508-509), else keeps the
  current due date when there is no usable duration (never before the start), else follows the
  duration (:506-524); with children, an automatic work package that is not turning into a
  milestone and whose start, due date or parent changed takes the span of its (rescheduled)
  children through `SetScheduleService` with itself switching to automatic mode (:346-365,
  set_schedule_service.rb:42-44), the others that service moves being discarded in memory.
  The rescheduling after the save then finds nothing left to move, so an edited successor is
  one save and one journal, and the edited due date of an automatic parent is never written.
  One existing expectation changed: step 13's edit of the automatic parent P's own due date was
  an UPDATE plus a rescheduling save (lock 1 -> 3); it is now the one UPDATE (lock 1 -> 2),
  because the span is restored before the save. Ruby issues no UPDATE at all for that edit (no
  attribute changed, lock stays 1): the remaining difference is the documented no-op bump.
  Not ported: the "came from user" state of `ignore_non_working_days` (an edit cannot write it),
  a due date set to the value it already had (Ruby counts it as given, the carried-forward
  update cannot tell), `update_ignore_non_working_days` of the edited parent itself
  (:377-381), `update_descendants` on a project change (update_service.rb:67-82), non-member and
  module checks of `WorkPackage.visible` for the former parent, `parent_id_changed?` of
  schedule_following (:84-90, never true after a save), notifications, `apply_patterns`.
  Kept as in OpenProject (owner decision on #154): on a move, the soonest start still
  includes the predecessors of the former ancestors, because `new_start_date`
  (set_attributes_service.rb:481-494) takes `max(new_start_date_from_parent,
  work_package.soonest_start)` and `soonest_start` (scheduling_rules.rb:68-80) reads the stored
  hierarchy before the save. With a due date given by the same edit, the dates can end up
  inverted (start after due), exactly as in Ruby. Diverging needs an RFC.

A committed row in that journey is evidence about the Rust paths, not Rails
persistence parity.

The callback rows in the right column come from #140 (ruff #158 at `b3bdfb1`),
merged into this branch's base; this PR does not change them. They passed the
unchanged ledger after the rebase. The 13 UNKNOWN order groups are the
WorkPackage chains through `acts_as_customizable`: a module whose include-time
body has a conditional is order-unknown, because ruff does not say whether the
skipped branch was read in full.

## 11. Negative controls

| # | Control | Where | Result |
|---|---|---|---|
| 1 | Remove the Charon artifact (`op_contracts`) | `ore_slag_negative_controls`, also the reconciliation test (d) | affected rows become `Slag(RustSideNotExtracted)`, 0 `Disagreement`, 0 resolutions carried over |
| 2 | Remove the Ruby extractor result | same | no source: 0 `Agreement`, 0 resolved; one guard row without its ruff fact: not resolved (EVIDENCED by the drift check only), structurally still slag |
| 3 | Corrupt an evidence hash, and a store hash | same | evidence: 9/9 resolutions rejected as stale and retained as attempts; store: 10 rows `Slag(RustEvidenceStale)`, 0 `Disagreement` |
| 4 | Break a semantic mapping | same | rotated attribute mapping: 4/9 still resolve (identical guards are indistinguishable by truth table, see section 14); a renamed callee turns an `Agreement` into `RubyOnly` |
| 5 | Disable a data-witness rule | same; live run | `And`/`Or` swapped in the witness input: 5 guards fail, 4 (no connective) resolve; live: `eval_guard` reading `Or` as `And` fails both tests (5 of 9 resolve) |
| 6 | Disable the Rust behaviour | live run only | `WritableRule::Never => Ok(true)` in `admission.rs`: `create_transition_differential` 30 mismatches; the data witness drops to 6/9; the store reports `admission.rs` stale, so Charon rows on it become `RustEvidenceStale`, not disagreements |
| 7 | Add an unsupported generated callback | `ore_slag_negative_controls` | injected into WorkPackage's effective chain (160 to 161), it matches no journal entry and is classified `Silent::Unexplained`. `create_transition_differential` asserts that class is empty, so it is reported, not hidden |
| 8 | Fail or remove the drift check | `ore_slag_negative_controls` | the truth table still passes, but all 8 guard rows stay `EVIDENCED` (a failing drift check is moved to `attempts`); the drift check is a required companion |

Live runs were made on a committed tree and restored with `git checkout`. The stale
check of the lifecycle was disabled once as well: control 3 then fails.

## 12. Adding an application (Zammad)

The generic parts are `reconcile.rs`, `resolution.rs` and `ore_slag_config.rs`. They
name no application. An application supplies:

1. A Charon store for its Rust crates, in the same TSV format, with `PROVENANCE.txt`.
2. A config file, `oracle/charon/ore-slag/<app>.toml` (OpenProject:
   `openproject.toml`), holding the data:
   - `[scope]`, `[extractors]`, `[gates]`;
   - `[[claim]]`: join key, level, Rust probe (`edges` or `data`), comparability,
     repair, and a declarative Ruby evidence spec (`fact` with a named predicate,
     `not_harvested`, `absent_in_body`, or `witness` with a named evaluator);
   - `[[claim_family]]`: claims a named generator produces from the harvest;
   - `[[residual]]` per key and `[residual_default.<SlagReason>]` templates: the
     proposal fields, owner stage, sub-reason, source location and revision specs.
3. App evaluators in its test module: the named predicates, witnesses and
   generators the config refers to. `ore_slag_config::load` rejects unknown keys
   and duplicate claim or residual keys.
4. Runtime attachments from its own oracle traces.
5. A ledger path, checked like this one.

**Learning.** The config's `[[learned]]` block, below its marker line, records each
residual's computed lifecycle status. The ledger test fails when it is stale.
`UPDATE_ORE_SLAG=1` rewrites only that block. It also appends a `[[residual]]` stub
(owner `OwnerStage::default_for(reason)`, every proposal field `"TODO"`) for a slag
row that has neither an entry nor a default. The test then fails until the stub is
filled in, so a new residual cannot pass unowned.

zammad-rs already has a typed SLAG model with stage and remedy
(`crates/rails-codegraph/src/slag.rs`), and its `codegraph-two-readings.md` describes
the same two readings. Its residuals map onto `OwnerStage`: `Ore` maps to
`RubyExtraction`, `GenericRule` to `RailsPolyfill`, `Codegraph` to
`RustExtraction`/`IntakeConfig`. Its `Target::Host` callback table rows are the
`RepresentedAsData` case. No Zammad name enters the generic modules.

## 13. Reproducing from pinned sources

- OpenProject `AdaWorldAPI/openproject` at `2725194454c895dfab6ff2685d9688b0cba53068`
  (the revision the traces record).
- ruff pinned at `RUFF_REV` (`79ed2ba`) in `crates/op-codegen-pipeline/src/lib.rs`.
- The Charon store is committed. Its revisions are in `oracle/charon/PROVENANCE.txt`.
  Re-run Charon only when a hashed file changed; the reconciliation fails on a stale
  file. The store holds op-contracts, op-services and op-api. Extract one crate at a
  time (`charon cargo --dest-file <crate>.llbc` in the crate directory, nightly
  toolchain and the extraction-only sqlx-postgres patch from `PROVENANCE.txt`), then
  `CHARON_TSV=1 ruff_charon import <dir> <crate>.llbc <crate>` per crate and copy the
  four `.tsv` files into `oracle/charon/`. `op_api.rows.tsv` is about 20 MB because
  Charon harvests the crate's monomorphized dependency graph.

```bash
export OPENPROJECT_SRC=/path/to/openproject OP_CORPUS_SRC=$OPENPROJECT_SRC REQUIRE_SOURCES=1
cargo test -p op-codegen-pipeline --test reconcile_create_admission -- --nocapture
# regenerate the generated artifacts after an intended change:
UPDATE_RECONCILE_REPORT=1 UPDATE_RECONCILE_LEDGER=1 cargo test -p op-codegen-pipeline --test reconcile_create_admission
# the database journey (separate gate):
OP_TEST_DATABASE_URL=postgres://... REQUIRE_DB=1 cargo test -p op-server executed_db -- --nocapture
```

## 14. How CI detects stale, missing or inconsistent evidence

- **Stale Rust evidence.** `files.tsv` hashes are compared with the checkout. A
  changed file fails the reconciliation test and turns rows on it into
  `RustEvidenceStale`.
- **Stale resolution evidence.** Every piece of evidence records the hashes of the
  files it rests on, and the ledger stores them. A changed source, generated file or
  evaluator changes the ledger, and the test fails until it is regenerated. The
  lifecycle rejects evidence whose recorded hash no longer matches.
- **Missing sources.** Without `OPENPROJECT_SRC` the tests print UNVERIFIED. Source
  evidence is unverifiable and cannot promote. CI sets `REQUIRE_SOURCES=1`, which
  turns a missing checkout into a failure.
- **Inconsistent artifacts.** The Markdown drilldown and the ledger are regenerated
  in the test and compared with the checked-in copies.
- **Unknown store versions** are refused, never guessed (`StoreError::UnknownVersion`).

Limits:

- A truth table cannot tell two identical guards apart. Six guards share an
  expression with another (the three `derived_*`, and `due_date` /
  `ignore_non_working_days` / `start_date`), so a mapping error between them passes
  the witness. The drift check and the per-attribute runtime comparison cover that
  case.
- The ledger includes the claims' extractor labels. A change to the reconciliation
  claims (for example a different permission extractor) requires regenerating it.
