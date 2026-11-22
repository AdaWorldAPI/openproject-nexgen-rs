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
- `ar_valid_after_persist` is true in every successful case, so these fixtures do
  not yet separate the contract's validation from ActiveRecord's: the
  `save(validate: false)` boundary is observable but not exercised. A case whose
  record passes the contract and fails model validation is still needed.
- `create_child_of_automatic_parent` journals exactly what a parentless create
  does: with no dates or work on either record the ancestor update saves nothing,
  so the `validate: false` saves in `update_ancestors_service.rb` are not exercised.
