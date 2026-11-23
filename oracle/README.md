# OpenProject behavioural oracle

`work_package_contract.rb` runs OpenProject's own `WorkPackages::CreateContract` and
`WorkPackages::UpdateContract` on records built with OpenProject's own FactoryBot factories, inside the
real OpenProject app (test environment, PostgreSQL), and writes `work-package-contract-trace.json`:
per case, `writable?` for every attribute the contract declares, `valid?` with its error details, and
the inputs the guards read (`leaf?`, `schedule_manually`, closed status and version). No rule is
written by hand; the only stubs (mail, jobs, cache, request store) are listed in the trace.

The trace is the oracle for the DT64 behaviour compiled from the contracts
(`.claude/plans/2026-11-20-openproject-on-when-do-v1.md`): a lowered guard is registered only when it
gives the same `writable` answers on every case.

## Run

From an OpenProject checkout with its bundle installed and a test database migrated:

```bash
RAILS_ENV=test bin/rails runner /path/to/this/repo/oracle/work_package_contract.rb
```

`OP_TRACE_OUT` overrides the output path. Two runs give identical bytes (no ids or timestamps are
recorded).

## Known noise

Every `CreateContract` case that sets an author fails on `author_id`, admin included: the real create
service sets the author itself. The `writable` maps are unaffected; the Create `valid` results carry
that error.

## Service oracle

`work_package_service.rb` runs `WorkPackages::CreateService` itself, not only the
contract, and writes `work-package-service-trace.json`: per case the service result,
the ordered callback journal of the call (setup is not journaled; every body runs;
the label comes from the chain that is running), the transaction depth of each
journaled callback, row-count deltas, enqueued job classes, and whether the
persisted record passes ActiveRecord validation on its own.

Each case commits, so `after_commit` fires. The script truncates every table before
each case and refuses to start on a non-empty `work_packages`: run it only against a
disposable test database.

```bash
RAILS_ENV=test bin/rails runner /path/to/this/repo/oracle/work_package_service.rb
```

Reproduced at OpenProject `2725194`, Ruby 4.0.10, Rails 8.1.4.1, PostgreSQL 16.15:
two consecutive runs give identical bytes. The contract oracle above reproduces
the committed trace byte-for-byte on the same setup.

What the four cases show, and what they do not:

- The create path journals 162 callbacks on WorkPackage, Watcher and
  WorkPackageHierarchy; the one `after_commit` callback (`call_after_create_hook`)
  runs at transaction depth 0, after the commit.
- Without permissions the contract refuses before any save: 65 validation-phase
  callbacks, no row written.
- `ar_valid_after_persist` is true in every successful case, and for this service it
  must be: `ModelContract#valid?` runs `model.valid?` before the contract's own
  validations (`app/contracts/model_contract.rb:85-88`) unless the contract sets
  `validate_model?` to false, and `WorkPackages::CreateContract` / `UpdateContract`
  do not. The model validations have therefore already run when
  `create_service.rb:64` saves with `validate: false`; the skip avoids running them
  twice, it does not drop them. The contract is the superset (model validations +
  writable/permission checks + its own `validate` rules), so a port that copies only
  the model validations loses the contract half. The contracts that genuinely skip
  model validation are `WorkPackages::CreateNoteContract` and
  `WorkPackages::CopyProjectContract` (`validate_model? = false`); a case through one
  of them is the one that separates the two.
- `create_child_of_automatic_parent` journals exactly what a parentless create
  does: with no dates or work on either record the ancestor update saves nothing,
  so the `validate: false` saves in `update_ancestors_service.rb` are not exercised.

## The two arms: AR callbacks and contract guards

OpenProject behaviour comes from two differently shaped sources, and each has its own differential.

| arm | source | oracle trace | test | what is compared |
|---|---|---|---|---|
| contract guards | ActiveModel contracts in `app/contracts` | `work-package-contract-trace.json` | `tests/guard_differential.rs` | `writable:` guards, lowered by ruff, against the recorded `writable?` answers |
| AR callbacks | ActiveRecord class bodies and concerns | `work-package-service-trace.json` | `tests/callback_differential.rs` | declared callbacks and validations (ruff's ordered `ClassOre` harvest) against the callbacks `WorkPackages::CreateService` actually ran |

`callback_differential` harvests `WorkPackage` (and `Watcher`), every concern it includes or
prepends that Zeitwerk finds under `app/models`, `app/models/concerns` and `lib`, and the
superclass chain. Each fired callback in the journal is matched to a declaration by
`(chain, kind, filter)`; `validates:KIND:attrs` matches when every attribute has a `validates`
declaration naming that validator. A fired callback with no declaration gets exactly one
`Slag` reason; a declared callback that never fired gets exactly one `Silent` reason. Both
enums are closed (no catch-all variant) and the test fails if an entry is neither matched nor
classified. The negative control spells every declared filter backwards and expects
(near) zero matches.

Measured at OpenProject `2725194` on the committed trace (157 distinct `(model, phase, label)`
entries over the four cases). Two ruff changes moved the numbers: ruff #153
(`ruff_ruby_spo::rails_generated`, the callbacks ActiveRecord generates from association macros)
and ruff #154 (`source_ore_from` returns module records, walks `included do` bodies, and exposes
`includes` and `superclass`; it replaced the test's own `lib-ruby-parser` walker, which is no longer
a dev-dependency).

| reason | before | after | moved by |
|---|---:|---:|---|
| matched to a declaration | 24 | 89 | #153 (65 only through a generated callback) |
| slag AssociationAutosave | 63 | 0 | #153 |
| slag AssociationFromModule | 24 | 24 | |
| slag AssociationFromPlugin | 5 | 5 | |
| slag AssociationNotInCheckout | 19 | 15 | #153 (4 are habtm middle models, now GeneratedUndecided) |
| slag DefinedInModule | 1 | 1 | |
| slag DefinedInPlugin | 7 | 7 | |
| slag FromGem | 10 | 8 | #153 (`around_save_collection_association`, `_ensure_no_duplicate_errors`) |
| slag ValidatorFromModule | 1 | 1 | |
| slag ModelWithoutClassFile | 3 | 3 | |
| slag GeneratedUndecided | 0 | 4 | #153 |
| declared (source) / silent | 37 / OnOption 1, Conditional 7 | 37 / OnOption 1, Conditional 7 | #154 reproduces the old walker exactly |
| generated callbacks / silent | n/a | 141 / ChainNotExercised 26 | #153 |

`RailsDefaults` is read from OpenProject: `belongs_to_required_by_default = false`
(`config/application.rb:79`) and `belongs_to_required_validates_foreign_key = true` (not set by
OpenProject; `load_defaults 6.1` predates 7.1, so the framework default holds,
`activerecord-8.1.4.1/lib/active_record/railtie.rb:40`). With `belongs_to` optional by default no
presence validator is generated unless a declaration says `optional: false` / `required: true`.

`Undecided` rules from `derive`: 2, both `has_and_belongs_to_many` (`:changesets`,
`:github_pull_requests`): the middle `has_many` is named by Rails' pluralize of the class name
(`associations.rb:2020`). They explain the 4 `GeneratedUndecided` entries
(`autosave_associated_records_for_changesets_workpackages` and the `github_pull_requests` one, plus
their `validate_associated_records_for_*`). The test only uses the naming guess to pick the slag
reason, never to match.

A generated callback matches by the same `(chain, kind, filter)` as a source one; the test counts
those matches apart (`matched_generated`) and prints the association and the Rails rule for each.
Negative controls: reversing every declared and generated filter name gives 0 matches; reversing only
the generated names drops exactly the 65 generated matches and leaves the 24 source matches.

Order. The test rebuilds the order Rails registers each chain in and compares it with the recorded
order of the matched entries (printed, not asserted): superclass first; `include M` runs M's
`included` block at the statement (`activesupport-8.1.4.1/lib/active_support/concern.rb:129-140`),
after the concern's own dependencies (`:131`, `:135`), and not again if the module is already an
ancestor (`:134`); `prepend` runs only the prepended block (`:142-152`); generated callbacks sit at
their association; a re-registered symbol filter moves to the end (`callbacks.rb:645-663`); `after_*`
run in registration order (`activemodel callbacks.rb:146`, `callbacks.rb:535-539`, `:619-621`).
Result: 19 `(case, model, phase)` groups with two or more matched entries, 3 differ (the same
`WorkPackage after_create` chain in the three successful cases), 3 adjacent inversions. Rebuilt
order is not file order: taking the walk's file order with generated callbacks last differs in
10 groups with 17 inversions. The 3 differences are the `has_many :file_links` / `:storages`
declarations (`app/models/work_package.rb:69-70`): the trace has them after `meetings`, because the
`modules/storages` patch declares them again and Rails moves a re-registered filter to the end, and
the walk does not enter `modules/`. That is the `AssociationFromModule` gap, not an ordering error.

The slag reasons name the next extension, cheapest first:

1. `AssociationFromModule`, `DefinedInModule`, `ValidatorFromModule` (26): codegraph config. Walk
   `modules/*/app` and `modules/*/lib` patch files and follow the engines' `include` of them into `WorkPackage`.
2. `AssociationFromPlugin`, `DefinedInPlugin` (12): codegraph config for `lib_static/plugins`, plus a
   ruff extension that expands the `acts_as_*` macro to the callbacks and associations it registers.
3. `ConditionalOptionNotEvaluated` (7): ruff extension. `if:` lambdas arrive as `<expr>`; lowering
   them to guard expressions (as the contract arm does for `writable:`) lets the case facts decide.
4. `FromGem`, `AssociationNotInCheckout`, `ModelWithoutClassFile`, `GeneratedUndecided` (30): a
   Rails/gem polyfill table (closure_tree, paper_trail, acts_as_list) and the habtm middle name.
   Nothing in the checkout can supply these.

`**opts` association options are invisible to the harvest today; a ruff follow-up makes them
`Undecided`, so some generated callbacks may later move to `GeneratedUndecided`.

## P2: executed transition (`create_member`)

`tests/create_transition_differential.rs` compares one transition, `WorkPackages::CreateService`
for `create_member`, with the traces. Every compared item has one category, and only the
first counts towards execution parity:

- EXECUTED: production Rust ran the rule. `op_services::CreateWorkPackageService::call` runs
  `op_contracts::work_packages::admit_create` over `generated_create_admission.rs`, a table
  generated from the Ruby contracts by ruff's lowered `writable:` guards
  (`cargo run -p op-codegen-pipeline --example emit_create_admission`). The test fails if the
  checked-in table differs from what the generator produces from the checkout. Hand-ported
  production rule: `user_allowed_to_add`. Compared: verdict, error keys and codes, `writable?`.
- PREDICTED: derived from the Ruby source, not from the trace, and not run: the callback
  registrations (`callback_harvest`) and the effect expectations derived from admission.
- SLAG: not modeled, named.

Inputs are the oracle's own: user, permissions and submitted attribute names are parsed from
`work_package_service.rb`; guard facts come from the contract trace (the service trace records
none, so the `CreateContract` record for the same user is borrowed). The traces record attribute
names for the submitted work package, not values, so no value is compared and the service method
takes names only. The permission set is held in "the case's project", an opaque handle: the test
user answers for it and for no other id.

`ExtractionGap::PermissionDeclScan` (typed, temporary): `permission:`, `default_attribute_permission`,
`attribute_alias`, `allow_writable_timestamps`, the `validate :x` list and the permission literal in
`user_allowed_to_add` are read by a line scan of `base_contract.rb` / `create_contract.rb`
(`op-codegen-pipeline/src/admission_gen.rs`). Its only consumer is the generator. It goes when ruff
lowers `permission:`.

Acceptance report (OpenProject `2725194`):

| section | result |
|---|---|
| contract rules EXECUTED | 350 items, 0 mismatches: `create_member` verdict 1, error keys 0, `writable?` 5 of 5; `create_no_permission` verdict 1, error keys 6, `writable?` 5; every attribute of every `CreateContract` record 332 compared and 332 matched |
| contract attributes unresolved (58) | `UndeclaredAttribute` 36 (engine contracts), `FactUnavailable` 12 (`WorkPackage.work_based_mode?` x10, `Setting.apiv3_write_readonly_attributes?` x2), `OpaqueWritable` 10 (`allow_writable_timestamps`) |
| contract rules not evaluated | the `validate :x` rules listed by `not_evaluated_validators()`, the `attribute ... do` blocks, `ModelContract#valid?` model validations |
| callback registrations RECONSTRUCTED | 101 declarations for create (94 invoked, 7 `ConditionalOptionNotEvaluated`); 157 journal entries: 89 registered, 68 typed slag |
| callback bodies EXECUTED | 0 |
| DB effects PERFORMED | 0 (the stub writes no row) |
| effects PREDICTED | attribute state 5 (`subject`, `project_id`, `type_id`, `status_id`, `priority_id`), persistence 2 (`persisted`, one `work_packages` row) |
| effects UNRESOLVED (slag, 11) | `AttributeNotModeled` `author_id`, `id`, `created_at`, `updated_at`, `position`; `EffectNotModeled` `journals`, `work_package_journals`, `watchers`; `JobNotModeled` x3 |

Sections are separate: authorization (executed), callbacks, attribute state, persistence, jobs.
Ordering is compared only inside one `(model, kind_chain)` callback group, where ActiveSupport runs
a chain in registration order; `changed_attributes`, `row_deltas` and `enqueued_jobs` are sorted by
the oracle script and compared as sets or maps, claiming no order. The one order difference is the
`file_links` / `storages` re-declaration in `modules/`.

Negative controls: (a) `create_no_permission`: production denies with the oracle's exact
`base: error_unauthorized` and `error_readonly` on the five submitted attributes, and the 46 registered
save/create/commit callbacks that did not run are explained by the denial; (b) the smallest change of
recorded facts that flips `status_id` (unchanged status, closed, in a closed version: 3 facts) turns
the production verdict into a denial, the oracle disagrees, and the predicted effects follow. The
callback matcher's reversed-names control is kept. Disabling the production permission check, the
guard negation, or `user_allowed_to_add`, or editing the table, each fails the test.

`CreateWorkPackageService::call` runs the generated admission first and acts on it: a denial ends the call
with the contract's error keys and codes; an admitted create continues into the hand-written path (labelled
in the code: only the validators the generated table has no rule for, plus the model validations, and the stub
flow). Model facts (`leaf?`, status closed, ...) are explicit input through `with_facts`; with none supplied,
a submitted attribute whose guard reads a fact is unresolved and `call` refuses with
`admission_unresolved: <typed reason>`, never an admission. Existing `op-services` tests are unchanged and
green (72 pass: 70 existing plus two new for the unresolved/supplied-fact paths); none relied on the old
`is_writable` list. The test drives the real `call`; the submitted attributes carry the script's own subject
literal, the case-project handle, and an opaque marker id for attributes whose values the oracle does not
record (admission reads presence, not values; no comparison uses a marker).

What each step is worth (the operator's evidence table):

| step | parity |
|---|---|
| ruff extracts a writable guard | none |
| production `op_contracts` evaluates the guard from compiled-from-ruff data | guard behaviour: 332 attribute pairs |
| the real `CreateWorkPackageService::call` uses that decision | admission behaviour: verdict and error keys/codes of `create_member` and `create_no_permission` |
| the stub assigns an id | none (no persistence parity) |
| Rust independently writes and reads the expected state | none yet (0 DB effects performed) |
| a callback name matches Ruby | none (no callback execution parity) |

Disable runs proving the connection: making `call` ignore the admission verdict, replacing the evaluator
inside the service with one that always admits, always admitting inside `call`, skipping admission in `call`, or ignoring `with_facts`, each fails the admission
comparison (error keys differ from the oracle; verdict differs; the 3-fact perturbation no longer flips).
