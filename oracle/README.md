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
