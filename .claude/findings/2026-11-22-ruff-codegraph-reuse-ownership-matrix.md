# Ruff / OGAR / DT64 / codegraph reuse — ownership matrix and first OpenProject slice

Status: inventory, not a design ratification. Evidence grades: **verified** (read or
run by the author in this session), **mapper** (reported by a read-only mapping agent,
not re-read), **uninspected**.

## 1. Sources and revisions

| Source | Revision | Grade |
|---|---|---|
| OpenProject (`AdaWorldAPI/openproject`, shallow clone) | `2725194454c895dfab6ff2685d9688b0cba53068` (2026-11-20), the revision `oracle/work-package-contract-trace.json` records | verified |
| zammad-rs | `d81ac8a` (PR #99) | verified |
| ruff | `7364e3de` main + branch `claude/ruby-menu-dsl-multiline` (`0ee64a64`) | verified |
| lance-graph | `1388069` (PR #2157) | verified |
| openproject-nexgen-rs | `7bc0caf` + this branch | verified |

## 2. The architecture this matrix serves

Charon is the Rust compiler-semantic witness. It enters through ruff beside the Ruby
frontend; it is not the comparison target and not a lance-graph ingestion path of its
own. Two independently harvested implementations (Rails; generated Rust seen through
Charon) are compared at shared semantic addresses. Structural comparison and
behavioural parity are separate gates; a resolved call edge or a generated method
shell is not parity.

| Layer | Owns |
|---|---|
| ruff | extraction: Ruby declarations and bodies, concern composition, call edges, callback order, AR DSL facts, Rust/Charon import |
| OGAR / generic Rails polyfill | normalisation: class identity, associations, scopes, attributes, Rails semantic rules |
| DT64 / R2IL | typed method addressing, bindings, IR |
| lance-graph-codegraph | persistent facts, provenance, reachability, cross-language comparison |
| application repo (zammad-rs, nexgen) | config, scenario fixtures, source paths, boundary adapters, mapping overrides |

OpenProject has two behaviour sources, ActiveRecord models and contracts. Both go
through the same intake with their execution phases kept distinct.

## 3. First falsifier (verified in source)

`app/services/work_packages/create_service.rb:64`:
`return save_failure(result, work_package) unless work_package.save(validate: false)`,
after `set_attributes` runs the contract (`create_service.rb:57-60`, comment lines
60-61: "Set attributes service passed, meaning the contract is fulfilled. Avoid running
validations again"). Copying only the model's AR validations therefore loses the
validation the contract enforces, and `update_service.rb:85,154` and
`update_ancestors_service.rb:88,91` also save with `validate: false`. A slice that
reproduces both the AR path and the contract path against the existing oracle
(`WorkPackages::CreateContract` / `UpdateContract`, 8 cases) tests whether ON/WHEN/DO
preserves contract-governed ORM behaviour.

## 4. Real-corpus harvest evidence (verified; `examples/real_corpus_harvest.rs`)

The ruff nav/menu/action/widget arms had only been run on synthetic fixtures.
Against the real tree:

| Arm | Result | Reading |
|---|---|---|
| nav (`extract_nav_edges`) | 14 edges, 407 raw route-helper refs, 1911 ERB + 383 controllers | vocabulary is 2 screens, so most refs are counted, not edged |
| menu (`extract_menu_edges`), before fix | **0 edges, 142 pushes** | first version matched single-line fixtures only; OpenProject spreads a push over lines and writes `controller: "/projects"` |
| menu, after fix (ruff branch above) | 2 edges, 142 pushes | the 2-screen vocabulary; 140 pushes target screens outside it |
| actions (`app/views`, `app/components`) | 0 edges, 0 and 2 raw refs | OpenProject renders actions through Primer components (607 view/component files mention `render(Primer::…` or similar; `button_to`/`method: :post` text hits: 141) — the closed vocabulary does not see them |
| widgets, `app/views` | 46 edges, 81 raw refs | arm works on real ERB |
| widgets, `app/components` | 0 edges, 14 raw refs | field symbol not parsed in component templates |

`menu` is the only result that exposed a defect in code shipped earlier; it is fixed
in ruff branch `claude/ruby-menu-dsl-multiline` (test verified red with the join
disabled). The action and widget zeroes are evidence of idiom gaps, not yet diagnosed
per file.

UI note: OpenProject's own `CLAUDE.md` says the new UI is server-rendered
ViewComponent/Hotwire with Angular components being migrated, so an Angular arm
should wait for concrete missing semantics; 526 component and 503 view ERB files and
510 feature specs are the available sources.

## 5. Ownership and migration matrix

Source for rows 1-6: `zammad-rs/docs/research/rails-codegraph-chain.md` §2 and §7
(read in full; a code check of each cited line is **uninspected**).

| # | Fact / rule | Copies today | Proposed owner | Verification | Grade |
|---|---|---|---|---|---|
| 1 | concern/module ancestor expansion | `Assembler::expand` (classes only); `Index::module_chain` (modules) — differ | ruff ore (concern includes) | `tests/duplication.rs` in zammad-rs must keep passing | doc |
| 2 | association target resolution | `assoc_target`, `OgarBasis::association_target`, `sites::gather` — differ for concern associations | OGAR lift (resolved target on `Association`) | `association_target_resolution_differs_between_the_compiler_and_the_intake` | doc |
| 3 | inflection | rails-compiler (now shared), `semantic.rs`, `ogar_basis.rs` | rails-polyfill | md5 of artifacts across runs | doc |
| 4 | scope call sites | rails-f000 regex vs codegraph AST pass | rails-f000 consuming codegraph evidence | existing gate | doc |
| 5 | `.rb` file walk | `rb_files`, ruff's own walk, `matrix.rs` | ruff | byte-identical regeneration | doc |
| 6 | singleton defs, call edges, receiver typing | only in rails-codegraph | ruff ore extension (rows 3, 8, 9 of that doc's §2) | `chain_gate` | doc |
| 7 | menu / nav / action / widget idioms | ruff arms, ERB closed vocabulary | ruff, plus a Primer/ViewComponent arm if justified | real-corpus ledger (§4) | verified |
| 8 | OpenProject source path | four env names: `RAILS_CORPUS_SRC`, `OP_CORPUS_SRC`, `OPENPROJECT_SRC`, `OPENPROJECT_PATH` (documented, never read) | nexgen config: one canonical name with aliases | none yet | mapper |
| 9 | boundary spellings in the host | `ScenarioHost::call` hard-codes five Zammad strings (`zammad-parity/src/lib.rs:54-74`) | declarative scenario config in the application repo | OpenProject scenario passes through the same host | doc |
| 10 | `grade` hard-coded `Compiler` on every finding; `unknown` empty for non-Rust rows; `UNRESOLVED_CALL` object ignored | lance-graph-codegraph (`diag.rs:55,228-262`, `query.rs:136,273`, `graph.rs:310`) | lance-graph, separate PR on a clean checkout | needs a Rails-store test in lance-graph | doc |

Ownership consequences already visible: application-specific names stay in
`zammad.toml` / `openproject.toml`; nothing Zammad-named moves into lance-graph.
lance-graph-codegraph mapper facts (not re-read): it reads `*.rows.bin`/`*.rows.tsv`
and residuals and never writes artifacts; its non-test source uses no dt64 lane or
BindStore (only `graph.rs:9` imports dt64 axis/map types); r2il is not a dependency.

## 6. What this does not establish

- No behavioural result: nothing here executes an OpenProject workflow in Rust and
  compares it with the Ruby oracle. Harvest coverage, executable behaviour, oracle
  parity, click-path parity, authentication and deployment readiness are separate and
  only harvest coverage was measured.
- The existing oracle records contract validation outcomes only; it has no callback
  journal.
- The `rails-codegraph` source was mapped by an agent that had not finished when this
  was written; rows 1-6 rest on the zammad-rs document, not on code I read.
- Authentication was not compared; do not merge the two applications' auth
  implementations on this evidence.

## 7. Next slice (proposed, not started)

1. Land the ruff menu fix (branch pushed; PR is the maintainer's to open).
2. One config-driven `openproject.toml` that selects `WorkPackage` and
   `WorkPackages::Create/UpdateContract`; run the existing generic harvest over it.
3. Extend `oracle/work_package_contract.rb` with a callback journal for the
   `save(validate: false)` path; compare with the lowered Rust; add a negative
   control per lowered rule.
4. Anything needing a lance-graph change (row 10) goes to a separate PR from a clean
   checkout.
