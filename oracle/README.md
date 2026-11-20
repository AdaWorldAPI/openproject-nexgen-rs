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
