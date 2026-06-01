# ruff_ruby_spo — finished Ruby/Rails frontend

> Sprint C4. The OpenProject (Ruby/Rails) source-language frontend onto the
> shared SPO triplet core. See `crates/ruff_spo_triplet/SPO_TRIPLET_EXTRACTION.md`
> §5–§6 for the methodology this implements; this doc records what the CODE does.

## What it does

`extract(source_tree)` walks a Rails tree — `app/models/*.rb` for the class
definitions and `db/schema.rb` for the per-table column baseline — and fills a
language-agnostic `ModelGraph` (`ruff_spo_triplet::ir`). That graph `expand()`s
to the exact same closed SPO triple vocabulary as the Python/Odoo frontend, so
every downstream consumer (`lance_graph` SPO loader, `action_emitter`,
`link_chain`) works on the OpenProject graph with zero changes. The reuse seam
is the IR; everything below it is shared.

There is **no external Ruby parser**. The scaffold's original intent (lib.rs
header, `Cargo.toml` "zero external parser deps by design") was to pin the
target triple shape with a passing test before wiring a parser. C4 keeps that
decision: instead of pulling in `lib-ruby-parser` or `tree-sitter-ruby`, it
ships a focused, dependency-free **line/block scanner** (`scan`) for the regular
subset of ActiveRecord model files, and builds the three extractors on top of
it. `Cargo.toml` still carries only the `ruff_spo_triplet` dependency.

The three extractors are file-disjoint and share the `scan` primitives so they
agree on how a class body is tokenised rather than each re-inventing string
slicing. `extract()` joins them: parse classes, then per class fill
`model.fields` and `model.functions`, push to the graph.

## Module layout

| file           | role |
| ---            | --- |
| `src/lib.rs`       | `extract()` entry, `RubyClass` raw record (note the C4 `columns` field), `NAMESPACE = "openproject"`, module wiring. |
| `src/scan.rs`      | Shared dependency-free primitives: `strip_comment`, `macro_symbols`, `def_blocks` (keyword-depth method splitter), `ivar_assignments`. Unit-tested. |
| `src/parse.rs`     | `parse_models`: walk `app/models`, build a `RubyClass` per `class X < …Record`, seed `associations` + join `db/schema.rb` `columns`. |
| `src/fields.rs`    | `extract_fields`: schema columns → baseline `Field`s; memoized `@ivar` assignments → derived `Field`s (`emitted_by` + association-chain `depends_on`). |
| `src/functions.rs` | `extract_functions`: `def` bodies → `Function{reads,raises,traverses}` + a synthetic validate fn for declarative `validates`. (impl landing in this same sprint.) |

## The Rails -> IR mapping (as implemented)

Mirrors SPO_TRIPLET_EXTRACTION.md §5 step-2, stated as what the scanner does.
WorkPackage (fixture) is the worked example.

| IR target             | What the code reads |
| ---                   | --- |
| `Model::name`         | `parse::class_name` — `class WorkPackage < <Super>` → `WorkPackage`. Any superclass accepted; no dot normalisation (Rails names have none). |
| `Field` (baseline)    | `parse::parse_schema` columns for the class's table (`WorkPackage`→`work_packages` via `to_snake`+`pluralize`), one bare `Field` each. |
| `Field` (derived)     | `fields::extract_fields` — `scan::ivar_assignments` inside a `def` (`@total_hours ||= …`) → a `Field` `emitted_by` that method. |
| `Field::depends_on`   | `fields::association_chains` — `<assoc>.<member>` chains the method reads, at identifier boundaries (`time_entries.hours`). Verbatim, source-faithful. |
| associations          | `parse` collects `belongs_to`/`has_many`/`has_one`/`has_and_belongs_to_many` leading symbols → the valid relation-name set (`scan::macro_symbols`). |
| `Function::name`      | `scan::def_blocks` top-level `def` names (strips `self.`, arg lists). |
| `Function::reads`     | bare attribute reads in the body (e.g. `status`). |
| `Function::raises`    | `raise <Type>` / `errors.add` → `exc:<Type>`; `validates`/`validate` → synthetic guard raising `exc:ActiveRecord::RecordInvalid`. |
| `Function::traverses` | body calls to a declared association name → that relation (`time_entries`). |

## Worked example

Fixture `app/models/work_package.rb`:

```ruby
class WorkPackage < ApplicationRecord
  belongs_to :project
  has_many :time_entries

  validates :subject, presence: true

  def compute_total_hours
    raise ActiveRecord::RecordInvalid unless status
    @total_hours ||= time_entries.hours
  end
end
```

With `db/schema.rb` `work_packages` columns (`subject`, `description`,
`status_id`, `status`, `created_at`, `updated_at`) this extracts to:

- **Model** `WorkPackage`
- **Fields**: the 6 schema columns (bare) + derived `total_hours`
  `{ emitted_by: compute_total_hours, depends_on: [time_entries.hours] }`.
- **Functions**: `compute_total_hours { reads: [status], raises:
  [ActiveRecord::RecordInvalid], traverses: [time_entries] }`, plus a synthetic
  validate fn raising `ActiveRecord::RecordInvalid` for `validates :subject`.

`expand()` projects that to SPO triples (exact shapes from
`tests/ruby_extract_test.rs`):

```
openproject:WorkPackage                  rdf:type      ogit:ObjectType
openproject:WorkPackage.total_hours      emitted_by    openproject:WorkPackage.compute_total_hours
openproject:WorkPackage.total_hours      depends_on    openproject:WorkPackage.time_entries.hours
openproject:WorkPackage.compute_total_hours  raises    exc:ActiveRecord::RecordInvalid
openproject:WorkPackage.compute_total_hours  traverses_relation  openproject:WorkPackage.time_entries
```

A second class `TimeEntry` (columns only) gives
`openproject:TimeEntry rdf:type ogit:ObjectType`. The `time_entries.hours`
dependency path is emitted verbatim under the model IRI; the downstream
`link_chain` splitter does the per-hop decomposition, not this crate.

## Scope + limits (honest)

This is a line scanner for the **regular** ActiveRecord subset, not a Ruby
parser. Do not read it as one.

Handles well: standard `class X < ApplicationRecord` (any superclass);
single-symbol association macros with trailing options hashes
(`has_many :time_entries, dependent: :destroy`); `def`/`end` blocks with nested
`if`/`unless`/`case`/`do` (keyword-depth counting in `scan::def_blocks`);
memoized `@x ||=` / `@x =` (but not `==`); comment- and string-literal-aware
stripping; declarative `validates`/`validate` guards.

Does **not** yet handle (EXTRACTOR-GAPs — under-extraction, not silently wrong
output): metaprogramming (`define_method`, dynamic `send`); STI subtype nuance
(the subclass is captured, hierarchy semantics are not); method chains broken
across multiple physical lines; mixed-in concerns/modules (only the file's own
class body is scanned); heredocs; `scope :x, ->(…) { … }` lambda bodies;
namespaced/`::`-qualified association targets.

Truth tiers (NARS, per SPO_TRIPLET_EXTRACTION.md §2): declared associations and
`validates`/`raise` guards land as **Authoritative**; inferred body reads and
relation traversals land as **Inferred**, so a strict downstream query can drop
the heuristic edges and keep only declared facts. The gaps above cause missing
edges, never a wrong tier.

## How to run / verify

```sh
cargo test -p ruff_ruby_spo
```

Covers three layers:

- `src/scan.rs` unit tests — `strip_comment` string-awareness, `macro_symbols`
  leading-symbol-only, `def_blocks` nested-block/modifier handling,
  `ivar_assignments` memoization.
- `src/lib.rs` `locked_shape_expands_to_expected_triples` — the hand-built
  target `ModelGraph` (what "done" looks like), provenance-independent.
- `tests/ruby_extract_test.rs` — end-to-end: real `extract()` over
  `tests/fixtures/openproject/`, asserting the expanded triples, the synthetic
  validation raise, and full `openproject:`/`exc:` namespacing.

Point it at a real checkout:

```rust
use std::path::Path;
let graph = ruff_ruby_spo::extract(Path::new("/path/to/openproject"));
```

## Next (Sprint C5+)

- **Wire output to ndjson + the loader.** `expand(&graph)` then
  `ruff_spo_triplet::to_ndjson(&triples)` → `openproject.spo.ndjson`, loaded by
  the downstream `lance_graph` SPO loader per SPO_TRIPLET_EXTRACTION.md §4–§5.
  Downstream needs zero changes; the format is identical to Odoo's.
- **Harden the scanner** against the gap list (heredocs, multi-line chains,
  concerns, `scope` lambdas, `define_method`) — or graduate to
  `lib-ruby-parser` if the gaps stop being acceptable as under-extraction.
- **Add fixtures**: callbacks (`before_save`/`after_create`), `attribute`/
  `store_accessor` derived fields, STI hierarchies — and a downstream
  `ActionSpec`-shape parity test to close the loop (§7).
