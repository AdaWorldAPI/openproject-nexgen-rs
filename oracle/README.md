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
With the intake the storages patch is found; since ruff #156 its `def self.included` hook is read, and the
order picture changed (see "After ruff #156" below).

### Engine patches (`oracle/intake.json`)

OpenProject's plugin modules change core models through a DSL, not by file location. An engine
calls `patches %i[WorkPackage]`, which queues `WorkPackage.include(<Plugin>::Patches::WorkPackagePatch)`
for `config.to_prepare` (`lib/open_project/plugins/acts_as_op_engine.rb:105-114`, `to_prepare` at `:107`,
`include` at `:111`). The sibling calls are `patch_with_namespace` (`:116-129`, with a `rescue NameError`
fallback to the un-namespaced patch name at `:122-124`) and `include_module` / `prepend_module`
(`:131-137`, via `add_module_to_classes`, `:361-371`). The test reads `oracle/intake.json` (the roots,
how to find engines, the engine order, and one rule per call) and names no OpenProject module itself.

- The calls come from ruff: `Declaration::DslCall` (the catch-all for class-body calls ruff does not
  promote) carries the call name and its rendered arguments, in statement order.
- A patch runs after the target class body has loaded, so its declarations register after the model's
  own. `include` runs the module's `included do` block then (`concern.rb:129-140`), unless the module is
  already an ancestor (`acts_as_op_engine.rb:111`); `prepend` registers no callback (`concern.rb:142-152`).
- Order across engines is the order the module gems are required (`Gemfile.modules`), because an
  engine's `to_prepare` blocks are queued by its initializer and engine initializers run in railtie
  order. Within an engine, statement order. This is an assumption read from Rails, not something the
  trace contradicts or confirms yet (see Order below).
- Patches found: 34 engines, 62 calls matched by a rule. For `WorkPackage` 4 `patches` includes resolve
  (backlogs, meeting, storages, bim, in that order); for `Watcher` none; no engine, initializer or
  `to_prepare` block in `modules/` patches `Watcher`, and none patches `WorkPackage` by `.include` /
  `.prepend` / `class_eval` outside the DSL (checked by search, each engine read).
- Unresolved calls get a closed `PatchSlag`; `ArgsUnreadable` and `PatchFileNotFound` must be 0.

Counts as first measured (ruff before #156; the current counts are in "After ruff #156" below):

| `PatchSlag` | count | stage |
|---|---:|---|
| `TargetUnreadable` | 38 | ruff extension: `walk::render_node` renders `Hash` but not `Kwargs`, so `include_module "X", into: "WorkPackage"` arrives as `"X", <expr>`. All of costs, repositories, resource_management and budgets use this form. |
| `BodyUnreadable` | 24 | ruff extension: storages and bim patch modules register through `def self.included(base) ... base.class_eval do ... end`; `module_ore` walks only a direct `included do`. |
| `ArgsUnreadable`, `PatchFileNotFound` | 0 | |

Conservation is per (call, target), not per call. A call such as `into: ["A", "B", "C"]` is three jobs; a
call whose targets cannot be read is one job with no target. The test records every job and requires that
each ends in exactly one applied patch or exactly one typed `PatchSlag` (`ArgsUnreadable`,
`TargetUnreadable`, `PatchFileNotFound`); `BodyUnreadable` and `BodyPartiallyRead` are notes on a patch that
was applied. `calls` (62) still counts each call once.

Ledger, attributed per change (157 entries; every column keeps `matched + slag == total`):

| reason | (a) main as merged | (b) after ruff #155 only | (c) after the modules intake |
|---|---:|---:|---:|
| matched to a declaration | 89 | 89 | 91 |
| of which only a generated callback matches | 65 | 65 | 67 |
| slag AssociationFromModule | 24 | 24 | 21 |
| slag AssociationFromPlugin | 5 | 5 | 5 |
| slag AssociationNotInCheckout | 15 | 15 | 15 |
| slag DefinedInModule | 1 | 1 | 1 |
| slag DefinedInPlugin | 7 | 7 | 7 |
| slag FromGem | 8 | 8 | 8 |
| slag ValidatorFromModule | 1 | 1 | 0 |
| slag ChainFileNotHarvested | n/a | n/a | 2 |
| slag ModelWithoutClassFile | 3 | 3 | 3 |
| slag GeneratedUndecided | 4 | 4 | 4 |
| declared (source) callbacks | 37 | 37 | 38 |
| order: groups compared / differing | 19 / 3 | 19 / 3 | 19 / 3 |

(b) moves nothing: ruff #155 makes option-dependent generated rules `Undecided` when an association
carries a keyword splat, and no association on `WorkPackage` or `Watcher` does.

(c) adds the two `belongs_to` of the backlogs patch (`:sprint`, `:backlog_bucket`: +2 matched, both
through generated callbacks) and meeting's `around_destroy` (declared; its chain is not exercised by a
create, so it is `ChainNotExercised`). `ChainFileNotHarvested` is the new closed reason for a name that a
harvested file spells but the harvest produced no declaration for: `:bcf_issue` (bim's patch, the
`BodyUnreadable` shape) and `validates:numericality:story_points` (backlogs, a `validates_numericality_of`
helper, which `walk` sends to the `DslCall` catch-all). Both used to read as module slag
(`AssociationNotInCheckout`, `ValidatorFromModule`); naming them says the file was found and ruff did not
read it. The negative controls are unchanged: 0 matches with every name reversed; reversing only the
generated names drops exactly the generated matches. A run without the intake is part of the test and
must not match more than the run with it.


### After ruff #156: coverage and completeness are two measures

ruff #156 renders keyword arguments (`into:`), reads `def self.included(base)` hooks (`base.class_eval`,
`base.<macro>`), lists what it could not read in `ClassOre::unharvested`, merges `with_options` options and
promotes `validates_*_of`. Nothing else changed: same trace (157 entries), same OpenProject `2725194`.

**Measure (a): callback coverage.** How many fired callbacks have a declaration (written or generated).

| reason | before (`91` matched) | after | moved by |
|---|---:|---:|---|
| matched to a declaration | 91 | 115 | #156 |
| of which only a generated callback matches | 67 | 89 | #156 |
| slag AssociationFromModule | 21 | 0 | #156: `include_module ... into:` resolves, storages' `def self.included` is read |
| slag DefinedInModule | 1 | 0 | #156 (the one filter with a `def` only under `modules/` is now declared by an applied patch) |
| slag ChainFileNotHarvested | 2 | 0 | #156 (`:bcf_issue` through bim's hook, `story_points` through `validates_*_of`) |
| slag AssociationFromPlugin | 5 | 5 | |
| slag AssociationNotInCheckout | 15 | 15 | |
| slag DefinedInPlugin | 7 | 7 | |
| slag FromGem | 8 | 8 | |
| slag ModelWithoutClassFile | 3 | 3 | |
| slag GeneratedUndecided | 4 | 4 | same 2 `Undecided` rules (`:changesets`, `:github_pull_requests`) |
| slag ValidatorFromModule, ValidatorFromFramework, ValidatesWithUndeclared, AssociationAutosave, DefinedInWalkedRootsNotDeclared | 0 | 0 | |
| typed slag, total | 66 | 42 | `matched + slag == 157` holds in both |
| declared (source) callbacks / generated callbacks (undecided rules) | 38 / n/a | 41 / 206 (2) | |
| declared, never fired: ChainNotExercised / OnOptionNotExercised / ConditionalOptionNotEvaluated / Unexplained | 1 / 1 / 7 / 0 | 2 / 1 / 7 / 0 | `move_cost_entries` (costs, `after_update`) is new |
| generated, never fired: ChainNotExercised / Unexplained | n/a | 39 / 0 | |
| without the intake (matched / via generated) | 89 / 65 | 89 / 65 | unchanged: the intake adds 26 matches |

The "before" column is the committed ledger (c) above. Negative controls are unchanged: 0 of 157 with every
name reversed, 0 generated matches with only the generated names reversed. No new undecidable case: the
`Undecided` rules from `derive` are the same 2.

Extending `oracle/intake.json` moved no callback number by itself: with the old roots ruff #156 already gave
115 matched, because every `WorkPackage` patch module sits under a root already listed (the new roots hold
patch modules for contracts, services and components, which no traced model includes). They matter for the
patch ledger below.

**Measure (b): extraction completeness.** A matching callback name does not mean the module that registers it
was read in full. `ClassOre::unharvested` lists the include-time statements (`included do`,
`def self.included`, its `base.class_eval`) that produced no declaration, with a typed reason. The test
classifies every module in a traced model's chain (concern or engine patch):

| completeness | meaning | `WorkPackage` chain (31 modules) |
|---|---|---:|
| Full | declarations came out, nothing listed unharvested | 21 |
| Partial | declarations came out, statements listed unharvested | 3: `WorkPackage::Ancestors` (METHOD_DEFINITION 1), `WorkPackages::Costs` (METHOD_DEFINITION 5), `OpenProject::Bim::Patches::WorkPackagePatch` (METHOD_DEFINITION 1) |
| Unread | no declaration, statements listed unharvested | 2: `WorkPackage::Automatable` (METHOD_DEFINITION 2), `Scopes::Scoped` (METHOD_DEFINITION 1) |
| NoIncludeTimeBody | nothing to read | 5 |

`Watcher` and `WorkPackageHierarchy` have no module in their chain. Unharvested statements per reason across the
chain: METHOD_DEFINITION 10, every other reason 0. Class bodies are not covered: ruff leaves `unharvested`
empty for a class, so a statement skipped in `app/models/work_package.rb` itself is not listed.

Patch slag, per (call, target) job: 62 calls, 79 jobs, 79 applied, 0 typed before apply. `ArgsUnreadable` 0,
`TargetUnreadable` 0 (38 before), `PatchFileNotFound` 0 (20 appeared when `into:` became readable, see the
roots below), `BodyUnreadable` 1 (24 before), `BodyPartiallyRead` 7 (new). `BodyPartiallyRead` marks a patch module
whose include-time bodies gave a declaration and left statements unread; it is applied anyway. A module read
in full is not slag, and the test re-derives every note from the model chains and requires both to agree
(7 patch modules full, 2 partial, 0 unread among those applied to a traced model). The one `BodyUnreadable` is
bim's `ColorThemesPatch` (a `class << base` block, reason OTHER), which targets no traced model.

Roots from data. The 20 `PatchFileNotFound` were `include_module` / `prepend_module` calls whose module lives
in an engine's `app/components`, `app/contracts`, `app/controllers` (and `app/controllers/concerns`),
`app/formatters` or `app/services`. A Rails engine autoloads every `app/*` subdirectory
(`railties-8.1.4.1/lib/rails/engine/configuration.rb:77-78`), so those are Zeitwerk roots and now sit in
`oracle/intake.json` (`modules/*/app/{components,contracts,controllers,controllers/concerns,formatters,services}`);
no module name is in the Rust. The 21st (`OpenProject::JournalFormatter::SquashTMCauseRendering`) is in
`app/formatters` but the file is `squashtm_cause_rendering.rb`: the engine inflects `squashtm` as `SquashTM`
(`modules/squashtm_integration/lib/open_project/squashtm_integration/engine.rb:42,49`), so the roots entry
`roots.inflections` carries that pair and cites those lines. Removing the six roots brings 20 back, removing
the inflection brings 1 back.

Order. 19 `(case, model, phase)` groups compared, as before. Before #156: 3 differ (`WorkPackage after_create`,
in the three successful cases), 3 adjacent inversions. After: 4 differ, 4 inversions, and they are a different
group: `WorkPackage before_validate` in all four cases (49 entries, first difference at #30: recorded
`validate_associated_records_for_file_links`, `..._storages`, then `..._changesets`; reconstructed puts
`changesets` first). The `after_create` difference is resolved: storages' hook is read, `file_links` /
`storages` are re-declared, and Rails moves a re-registered `after_create` callback to the end. The
`before_validate` one is new and is visible only because the re-declaration is now read. Cause, from
`activerecord-8.1.4.1/lib/active_record/autosave_association.rb:213-214`: the validation callback is added
only when `!method_defined?(validation_method)`, so a re-declared association does not re-register its
`validate_associated_records_for_*`, and the reconstruction first moved it. That was a reconstruction rule, not an
ordering fact about OpenProject, and it is now fixed: `EffState::push` keeps a generated
`validate_associated_records_for_*` at its first registration (`autosave_association.rb:220`), while save
callbacks, which have no guard (`:197-198`, `:210-211`, `:214`), still move to the end. Result: 19 groups,
**0 differ**, 0 inversions, and the test now asserts it (`ORDER_GROUPS_FLOOR` 19, reconstructed differences 0).
The walk-file-order comparison is kept as the negative control and must differ: 10 groups, 25 inversions
(was 10 / 17).

Unknown order. A group is reported UNKNOWN, not definite, when a matched callback is registered in a module
whose include-time bodies leave a `ControlFlow`, `DynamicSend` or `UnreadBlockBody` statement unread (any of
those may register a callback or re-register one). Measured now: 0 groups, because the only statements left
unread in the traced chains are `def`s. The logic is exercised anyway: with every origin marked unread, all
19 groups turn UNKNOWN and none counts as a definite difference. `UnrecognizedCall`, `CallOnOtherReceiver`,
`MethodDefinition` and `Other` are counted and printed, but do not make an order unknown.

The Rails engine `to_prepare` order (engines in `Gemfile.modules` order, statement order inside one engine)
is still UNVERIFIED: nothing in the trace separates it from other orders, because the groups it would decide
are the ones above and the `after_create` match does not depend on the order between engines.

The slag reasons name the next extension, cheapest first (written before #156; item 1 is done, see above):

1. `AssociationFromModule`, `DefinedInModule` (22) and the 62 patch calls: the intake above walks
   `modules/*` and follows `patches`. What is left needs two ruff changes, in this order: render keyword
   arguments in `walk::render_node` (unlocks the 38 `include_module` calls, hence costs, repositories,
   resource_management and budgets), and walk `def self.included(base)` + `base.class_eval` bodies and the
   `validates_*_of` helpers (unlocks storages' `has_many :file_links` / `:storages` redeclaration, which
   is the 3 ordering differences, bim's `:bcf_issue`, and backlogs' `story_points` validator).
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
| callback registrations RECONSTRUCTED | 127 declarations for create (120 invoked, 7 `ConditionalOptionNotEvaluated`); 157 journal entries: 115 registered, 42 typed slag (after ruff #156: 103 -> 127, 91 -> 115, 66 -> 42; after #131 engine patches: 101 -> 103, 89 -> 91, 68 -> 66) |
| callback bodies EXECUTED | 0 |
| DB effects PERFORMED | 0 (the stub writes no row) |
| effects PREDICTED | attribute state 5 (`subject`, `project_id`, `type_id`, `status_id`, `priority_id`), persistence 2 (`persisted`, one `work_packages` row) |
| effects UNRESOLVED (slag, 11) | `AttributeNotModeled` `author_id`, `id`, `created_at`, `updated_at`, `position`; `EffectNotModeled` `journals`, `work_package_journals`, `watchers`; `JobNotModeled` x3 |

Sections are separate: authorization (executed), callbacks, attribute state, persistence, jobs.
Ordering is compared only inside one `(model, kind_chain)` callback group, where ActiveSupport runs
a chain in registration order; `changed_attributes`, `row_deltas` and `enqueued_jobs` are sorted by
the oracle script and compared as sets or maps, claiming no order. The `file_links` / `storages`
re-declaration in `modules/` is now read (ruff #156), and with the validation-callback guard applied (see
"After ruff #156") `create_transition_differential` reports 6 groups, 0 UNKNOWN, 0 differ.

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
