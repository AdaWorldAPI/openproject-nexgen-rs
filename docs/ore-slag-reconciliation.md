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
  has been run. Example: running Charon on `op-api`.
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

**Success: the nine lowered `writable:` guards.** These are `author_id`, the three
`derived_*`, `done_ratio`, `due_date`, `ignore_non_working_days`, `start_date` and
`status_id`. All nine are `RESOLVED(DataWitness)` as in section 8, and all nine stay
`Slag(RepresentedAsData)` structurally. `ADD_PERMISSION` is held as data too, but it
stays `EVIDENCED`. The drift check passes, and production admission authorizes with
exactly the scanned permission and refuses with every other oracle permission (2/2).
Its Ruby input is still the permission line scan, not parsed ORE. Once a ruff
permission lowering feeds the generator, the same witness can verify it.

**Retained unknown: `attribute :schedule_manually do validate_has_predecessors_or_children end`.**

- The source is `app/contracts/work_packages/create_contract.rb:46-48`, with the
  validator defined at line 68: `if model.schedule_automatically?`, then `errors.add(:schedule_manually,
  :cannot_be_automatically_scheduled)` unless the parent has scheduling relations or
  the record has children. `BaseContract.attribute` registers the block with
  `validate(attribute, &block)` (`app/contracts/base_contract.rb:89`), so it runs on
  every validation, whether or not `schedule_manually` was submitted.
- Ruby side: not harvested. The permission scan reads the `attribute` line and skips
  the block; ruff lowers only `writable:`.
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
- Structural verdict: `Slag(RubySideNotHarvested)`. Status: **OPEN**, owner
  `ContractHarvest`.
- Proposal: a ruff contract-arm rule that harvests validator calls inside attribute
  blocks, keeping their condition. If supplied, the claim becomes `RubyOnly`. The
  negative control: a harvest that drops the condition would predict the error on
  `leaf_manual`, where the oracle records none.
- The validator is implemented in `admit_create`; its differential case is the one
  above, kept apart from the 350. The ledger and report are regenerated from a Charon
  extraction that includes the changed Rust files: until then the stored ledger still
  shows the earlier numbers.

## 10. Scorecard (this branch; ruff `79ed2ba`, OpenProject `2725194`, store from nexgen `8f2680e`)

Separate denominators. Before means #138 as it was; after means this change.

| Measure | Before | After |
|---|---|---|
| Claims | 21 | 21 |
| Ruby side inspected (fact or inspected absence) | 18 / 21 | 18 / 21 |
| of those from a parser-backed extractor (ruff) | 9 / 18 | 9 / 18 |
| Rust counterpart inspectable in the store | 18 / 21 | 18 / 21 |
| Structural: Agreement / Disagreement | 4 / 0 | 4 / 0 |
| Structural: RubyOnly / CharonOnly / NotComparable | 0 / 1 / 1 | 0 / 1 / 1 |
| Slag rows | 15 | 15 (verdicts unchanged) |
| Residuals open, by reason | not typed | RubySideNotHarvested 2, RustBodyNotStructured 2, RustSideNotExtracted 1, RepresentedAsData 1 |
| Residuals open, by owner | not typed | ContractHarvest 2, RustExtraction 3, DataWitness 1 |
| Resolved by extraction / data witness / semantic rule | 0 / 0 / 0 | 0 / 9 / 0 |
| Lifecycle | none | RESOLVED(DataWitness) 9, EVIDENCED 1, CANDIDATE 1, OPEN 4 |
| Runtime verdicts over the 21 claims | 10 executed (0 mismatches), 3 cited, 8 none | ExecutedAgreement 11, ExecutedDisagreement 0, Unverified 3, NotExecuted 7 |
| Contract-rule items executed against the oracle (`create_transition_differential`) | 350, 0 mismatches | 350, 0 mismatches (unchanged; the new case is separate) |
| Lowered guards against the oracle (`guard_differential`) | 84 pairs, 0 mismatches | 84, 0 |
| `schedule_manually` block differential | not run | 1 discriminating record, 0 mismatches |
| Callback ledger (`callback_differential`) | 115 / 157 matched, 42 slag (ruff 79ed2ba) | 154 / 157, 3 slag (ruff b3bdfb1, from #140; not this PR) |
| Callback slag by reason | AssociationNotInCheckout 15, FromGem 8, DefinedInPlugin 7, AssociationFromPlugin 5, GeneratedUndecided 4, ModelWithoutClassFile 3 | AssociationNotInCheckout 2, GeneratedUndecided 1 |
| Callback order | 19 groups, 19 definite, 0 differ | 29 groups: 29 definite, 0 UNKNOWN, 0 differ (ruff ce96aae `hidden_complete`; 16 / 13 between the #140 review fix and this change) |
| Callback bodies executed | 0 | 0 |
| Database effects (EXECUTED_DB, PostgreSQL 16) | journey 7/7, 13 rows asserted | same; cited by the reconciliation, run separately |

The P3b gaps are reported, not hidden:

- The create is not transactional. A failure after the INSERT leaves the row
  committed: EXECUTED_DB step 7 reports `transactional create: NO`.
- The create writes no journal.
- Authorization and write are not isolated.

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
| 8 | Fail or remove the drift check | `ore_slag_negative_controls` | the truth table still passes, but all 9 guard rows stay `EVIDENCED` (a failing drift check is moved to `attempts`); the drift check is a required companion |

Live runs were made on a committed tree and restored with `git checkout`. The stale
check of the lifecycle was disabled once as well: control 3 then fails.

## 12. Adding an application (Zammad)

The generic parts are `reconcile.rs` and `resolution.rs`. They name no application.
An application supplies data:

1. A Charon store for its Rust crates, in the same TSV format, with `PROVENANCE.txt`.
2. Claims (`ClaimSpec`): join keys, Ruby evidence from its extractors, Rust probes
   (`Edges` or `Data`), comparability.
3. Runtime attachments from its own oracle traces.
4. Proposals per residual. Owner refinements and witness bindings go in its test
   module, as `tests/ore_slag/resolution.rs` does for OpenProject.
5. A ledger path, checked like this one.

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
  file.

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
