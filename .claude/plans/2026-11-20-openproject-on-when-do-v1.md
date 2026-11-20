# OpenProject as ON / WHEN / DO in DT64

Status: PLAN, 2026-11-20. Nothing below is built except where marked.

## Goal

Stay faithful to OpenProject's behaviour, but compile it instead of porting it.
Both halves OpenProject declares its rules in are read from the Rails source and
land as DT64 behaviour:

- the **ActiveRecord half**: models (associations, validations, callbacks);
- the **contract half**: `app/contracts` (`attribute … permission: … writable: ->`,
  `validate`, `validates`). OpenProject's create services save with
  `save(validate: false)`; the contract is the rule table.

The landing form is lance-graph's call shape
(`.claude/knowledge/classid-on-do-call-shape.md` in lance-graph):

| DT64 | OpenProject |
| --- | --- |
| ON: the classid's left side | the class (`WorkPackage` under OpenProject's app id) |
| DO: the right side `0xF###` | one behaviour: an attribute write, a derived attribute, a validation, a callback |
| WHEN: supplied by `register` | the contract's `writable:` guard, lowered to a `SetExpr` over field positions |
| reads / writes | `WideFieldMask` |
| reader scope, joined at execution | `permission:` and the `AccessControl` map; not part of the method |
| relations | read through the class view; declared at intake; no foreign key |

Harvested rules live outside the `BindStore` until `register` promotes them
(D-DT64-7 D2). That is the ore and slag split: harvest rows are the explore set;
a rule is registered only after its answers equal OpenProject's.

## What exists

- Contracts as rows: `codegraph/harvest/class-macros-contracts-openproject.json`
  (rails-codegraph K9; 1026 calls, 951 fully read, 69 guard lambdas, 6 unread
  values; OpenProject dev 27251944). BUILT.
- Models: ruff `extract_app_with_schema` and `op-generated` (723 generated files).
  BUILT, but used from 2 places; the running code uses `op-db`.
- `op-db`: the hand-written storage ORM copied from openproject-rs (319 SQL query
  sites, 23 hand-written tables, 197 uses outside the crate). To retire.
- A registered DT64 method executed over lent lanes: lance-graph
  `foundry-parity-lab/src/method.rs`. BUILT (lab).
- A consumer that registers Rails behaviour in a `BindStore`: zammad-rs
  `rails-codegraph` `plug`. BUILT, Zammad-specific in places.
- An OpenProject behavioural oracle on real ActiveRecord 8.1.3.1 over sqlite:
  zammad-rs `oracle/openproject_f000.rb`. BUILT for one scope.

## Slices

1. **Contract oracle.** A Ruby script on real ActiveRecord and OpenProject's own
   contract classes records, per fixture, `writable?(attr)` and the contract's
   errors for a set of changes. Starts with `WorkPackages::BaseContract`. Gate:
   the script runs from the source, no hand-copied rule.
2. **Behaviour manifest by codegen.** From the K9 rows and the model harvest,
   generate per class: its classid (ON), one `0xF###` per attribute write and per
   `validate` (DO), the field masks, and a `SetExpr` guard where the `writable:`
   lambda lowers. A lambda that does not lower stays in the explore list with its
   reason and is not registered. Gate: every K9 row is either in the manifest or
   in the explore list (conservation), and the manifest names no lane or plane
   number.
3. **Guard lowering.** Lower `writable:` lambdas and the predicates they call
   (`leaf_or_manually_scheduled?`, `closed_version_and_status?`) through ruff's
   body facts into `SetExpr`. Each lowered guard is checked against the oracle
   before it is registered. Gate: guard answers equal the oracle on every fixture.
4. **Execution.** Register the manifest in a `BindStore` and run `WorkPackage`'s
   write checks as `on :: do` over `borrow_all`, the reader scope joined at
   execution. Gate: answers equal the oracle; nanoseconds per row, nothing
   allocated per call.
5. **Retire `op-db`.** One call site at a time, behind a test that its answer
   equals the old repository's; then delete the crate.

Field view (Redmine's two-column rows and labelled form builder) and click-path
parity with OpenProject's ERB views follow on the same manifest: a page is the
class view of the record through the reader's field mask.

## Not done here

- No second registry, no method table, no foreign-key read (lance-graph §7).
- No hand-written rule: a rule that cannot be read stays in the explore list.
