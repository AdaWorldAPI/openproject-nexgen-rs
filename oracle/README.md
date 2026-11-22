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
entries over the four cases):

```
matched to a declaration            24
slag AssociationAutosave            63   Rails writes autosave/validate_associated_* for each declared association
slag AssociationFromModule          24   association declared in modules/* (engine patch, not walked)
slag AssociationFromPlugin           5   association declared by lib_static/ plugin macro (acts_as_*)
slag AssociationNotInCheckout       19   hidden habtm joins, gem-added (closure_tree, paper_trail), journals
slag DefinedInModule                 1   validate_budget, modules/budgets
slag DefinedInPlugin                 7   acts_as_customizable / attachable / journalized hooks
slag FromGem                        10   ActiveRecord, closure_tree, acts_as_list internals
slag ValidatorFromModule             1   story_points, modules/backlogs patch
slag ModelWithoutClassFile           3   WorkPackageHierarchy (generated by closure_tree)
declared, never fired: OnOptionNotExercised 1, ConditionalOptionNotEvaluated 7, Unexplained 0
```

The slag reasons name the next extension, cheapest first:

1. `AssociationAutosave` (63): a Rails polyfill, association declaration to its autosave and
   validate-associated callback pair. No new fact is needed; the declarations are already harvested.
2. `AssociationFromModule`, `DefinedInModule`, `ValidatorFromModule` (26): codegraph config. Walk
   `modules/*/app` and `modules/*/lib` patch files and follow the engines' `include` of them into `WorkPackage`.
3. `AssociationFromPlugin`, `DefinedInPlugin` (12): codegraph config for `lib_static/plugins`, plus a
   ruff extension that expands the `acts_as_*` macro to the callbacks and associations it registers.
4. `ConditionalOptionNotEvaluated` (7): ruff extension. `if:` lambdas arrive as `<expr>`; lowering
   them to guard expressions (as the contract arm does for `writable:`) lets the case facts decide.
5. `FromGem`, `AssociationNotInCheckout`, `ModelWithoutClassFile` (32): a Rails/gem polyfill table.
   Nothing in the checkout can supply these.

Two ruff limits show up in the harvest. `ruff_ruby_spo::events::source_ore` turns only `class`
bodies into `ClassOre`, and `walk_class_body` records an `included do ... end` block as a marker
with a byte range without walking it, so the declarations of every concern module are invisible to
it. The test parses modules itself and passes each `included` body to `class_ore`. Moving that into
ruff would retire the test-local walker.
