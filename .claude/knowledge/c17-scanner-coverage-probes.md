# C17 — `ruff_ruby_spo` Scanner Coverage Probes (2026-06-03)

Empirical survey of what the C4-completed dependency-free line/block scanner
extracts vs. what's actually in real OpenProject models. **Read-only probe.**
The scanner is unmodified; this file is the "signal what to update" the user
asked for.

## Method

Five hand-picked probes covering the OpenProject Rails surface, each
representative of a different scanner-stressing pattern. Per probe: actual
constructs in the file, what the scanner extracts per `parse.rs` / `fields.rs`
/ `functions.rs` rules, gap classification.

No code changes. No fixture additions. No scanner runs. The scanner's rules
are deterministic enough to be evaluated by reading both the model file and
the rules side-by-side.

## Repo-wide landscape (`/home/user/openproject/app/models/`)

| Construct | Count | Detection rule |
| --- | --- | --- |
| `*.rb` files (top-level) | 112 | `ls app/models/*.rb` |
| `*.rb` files (recursive, incl. concerns + STI subtrees) | 941 | `find` |
| STI subclasses (`class X < Y`, Y not Record/Base/Model) | 23 | grep `^class … < (not Record/Base)$` |
| `polymorphic: true` belongs_to | 20 | grep |
| `has_many :through` | 17 | grep |
| `include Concern` in `app/models/*.rb` (top-level only) | 65 | grep |
| `scope :name, -> {...}` lambdas | 98 | grep |
| Lifecycle callbacks (`before_*`, `after_*`, `around_*`) | 74 | grep |
| `store_accessor` (JSONB pseudo-field declarations) | 1 (Journal `:cause`, 8 keys) | grep |
| `attribute :foo, type` (typed attribute overrides) | 27 | grep |
| `enum :foo, {...}` (status/category enums) | many (uncounted) | grep |
| `as: :name` reverse-side polymorphic on has_many/has_one | 5 (incl. WorkPackage) | grep |

## Probe 1 — `principal.rb` (286L) — STI base + rich macros

Scanner output:
- Class: `Principal` ✓
- Associations: `preference`, `members`, `memberships`, `work_package_shares`,
  `projects`, `categories`, `user_auth_provider_links`, `auth_providers`,
  `persisted_views`, `persisted_queries` (leading symbols only)

Missed:
- A1 `self.table_name = "#{table_name_prefix}users#{table_name_suffix}"` —
  runtime-built table name. Scanner uses inflected default `principals` for
  the columns join in `parse.rs::parse_schema`. **Actual DB table is `users`.**
  Every column on the Principal model is mis-attributed to the wrong table or
  missed entirely.
- A2 `through: :memberships` on `:projects`, `:user_auth_provider_links` on
  `:auth_providers` — extracted as flat has_many; the join-table indirection
  is lost.
- A3 `class_name: "Member"` / `"UserPreference"` on 4 associations — target
  type not captured.
- A4 `foreign_key: "user_id"` on 5 associations — FK column not captured.
- A5 `inverse_of: :principal` / `:user` on most associations — bidirectional
  symmetry not captured.
- A6 `dependent: :destroy / :nullify / :delete_all` — cascade semantics not
  captured; downstream `traverses_relation` triplets don't know which edges
  imply a delete-cascade.
- A7 `enum :status, { active: 1, registered: 2, locked: 3, invited: 4,
  deleted: 5 }, scopes: false` — 5 status values + scope side-effect lost.
- A8 `default_scope -> { where.not(status: …) }` — invisible global filter.
- A9 `include ::Scopes::Scoped`, `include HasDetailsTable` — 2 concerns'
  worth of model-extending behavior not introspected.
- A10 `scopes :like, :having_entity_membership, :human, …, :status` (10
  registered scope-method names) — declarative scope registration ignored.
- A11 `scope :in_project, ->(project) { … }`, `scope :not_in_project, …` —
  lambda-bodied scopes ignored.
- A12 (Sub-STI) `class Group < Principal`, `class User < Principal`,
  `class AnonymousUser < User`, etc. — Principal is the root of a 5+ class
  hierarchy. Scanner captures each subclass-file independently with no
  awareness that they share a discriminator (`type` column) or that queries
  on `Principal` polymorphically dispatch.

## Probe 2 — `view.rb` (36L) — minimal class, **STI explicitly disabled**

Scanner output:
- Class: `View` ✓
- Association: `query` ✓

Missed:
- B1 `self.inheritance_column = :_type_disabled` — Rails-meta directive that
  says "this `View` is _not_ a STI root". Without this signal, downstream
  reasoning that walks `class X < View` files would falsely assume STI
  polymorphism. Scanner has no concept of inheritance_column.

## Probe 3 — `journal.rb` (90L visible, 247L total) — polymorphic + concerns + DSL-heavy

Scanner output:
- Class: `Journal` ✓
- Associations: `user`, `journable`, `data`, `agenda_item_journals`,
  `participant_journals`, `attachable_journals`, `customizable_journals`,
  `custom_comment_journals`, `project_phase_journals`, `storable_journals`,
  `notifications`

Missed:
- C1 `belongs_to :journable, polymorphic: true` and
  `belongs_to :data, polymorphic: true` — **the two most semantically
  important relations on this class** (Journal's whole purpose is to be a
  polymorphic audit-trail attached to any journable thing). Scanner emits them
  as flat belongs_to. Without `polymorphic: true`, downstream cannot know that
  the `journable_type`/`journable_id` column pair encodes the target type.
- C2 `class_name: "Journal::MeetingAgendaItemJournal"`, …, 7× namespaced
  `Journal::*Journal` STI subtype names — target types `::`-namespaced;
  scanner can't represent `::`.
- C3 `acts_as_attachable view_permission: …, add_on_new_permission: …, …` —
  DSL macro that, in OP's runtime, adds a polymorphic `has_many :attachments,
  as: :container`. Scanner has zero DSL awareness.
- C4 23× `register_journal_formatter OpenProject::JournalFormatter::…` —
  extension registry; not edges in the model graph, but the SPO target
  vocabulary may want them as `registers` triples — currently invisible.
- C5 `store_accessor :cause, %i[type feature import_history work_package_id
  changed_days status_name status_id status_changes], prefix: true` — **8
  JSONB-backed pseudo-fields** (`cause_type`, `cause_feature`, …). None
  appear as `Field`s; the scanner emits zero `Field`s for them.
- C6 `self.ignored_columns += ["activity_type"]` — runtime column blacklist;
  scanner emits `activity_type` as a baseline Field from `schema.rb`, but
  the Rails runtime ignores it.
- C7 5 concerns (`::JournalChanges`, `::JournalFormatter`, `::Acts::
  Journalized::FormatHooks`, `Journal::Timestamps`, `Reactable`) — model
  behavior not introspected.

## Probe 4 — `work_package.rb` (728L total, 120L probed) — heavyweight central model

Scanner output:
- Class: `WorkPackage` ✓
- Associations (~20 captured): `project`, `type`, `status`, `author`,
  `assigned_to`, `responsible`, `version`, `project_phase_definition`,
  `priority`, `category`, `time_entries`, `file_links`, `storages`,
  `changesets`, `github_pull_requests`, `meeting_agenda_items`,
  `meeting_outcomes`, `meetings`, …

Missed:
- D1 `has_many :time_entries, …, inverse_of: :entity, as: :entity` and
  `has_many :file_links, …, as: :container` — **reverse-side polymorphic**
  declarations. `time_entries.entity_type` will be `"WorkPackage"` at
  runtime; scanner emits these as plain has_many.
- D2 8× `class_name:` on belongs_to: `Status`, `User`, `Principal`
  (×2 for assigned_to+responsible), `IssuePriority`, `Storages::FileLink`,
  `Category` — none captured. Critical for assigned_to/responsible:
  scanner-naive resolution would resolve `belongs_to :assigned_to` → model
  named `AssignedTo` (which doesn't exist).
- D3 5× `optional: true` (`assigned_to`, `responsible`, `version`,
  `project_phase_definition`, `category`) — nullability of belongs_to FK
  not captured.
- D4 4× `through:` (`storages` through `:project`; `meetings` through
  `:meeting_agenda_items`) — multi-hop relations flattened.
- D5 `has_and_belongs_to_many :changesets, -> {…}` and
  `:github_pull_requests` — scanner handles habtm (it's in
  ASSOCIATION_MACROS), but the join table is named by Rails inflection
  (`changesets_work_packages`); scanner has no mapping.
- D6 `15 concerns`: `SemanticIdentifier`, `Validations`, `SchedulingRules`,
  `StatusTransitions`, `AskBeforeDestruction`, `TimeEntriesCleaner`,
  `Ancestors`, `CustomActioned`, `Hooks`, `DerivedDates`, `SpentTime`,
  `Costs`, `Relations`, `::Scopes::Scoped`, `HasMembers`, `Remindable`,
  `OpenProject::Journal::AttachmentHelper`. Each is a real source file with
  its own associations / validations / callbacks. **The single largest
  source of unseen Rails surface area in this model.**
- D7 10+ scope lambdas (`:recently_updated`, `:visible`, `:in_status`,
  `:for_projects`, `:changed_since`, …) — all bodies invisible.
- D8 Constants `DONE_RATIO_OPTIONS`, `TOTAL_PERCENT_COMPLETE_MODE_OPTIONS`
  — domain enums encoded as constants (not Rails enums), ignored.

## Probe 5 — `project.rb` (315L total, 100L probed) — heavy + namespaced + source: aliasing

Scanner output:
- Class: `Project` ✓
- Associations: `members`, `memberships`, `member_principals`, `users`,
  `principals`, `calculated_value_errors`, `enabled_modules`, `types`,
  `work_packages`, `work_package_changes`, `versions`, `time_entries`,
  `time_entry_activities_projects`, `cost_types_projects`, `cost_types`,
  `queries`, `news`, `categories`, `forums`, `repository`, `changesets`,
  `wiki`, `budgets`, `notification_settings`, `project_storages`, `storages`,
  …

Missed:
- E1 `has_many :users, through: :members, source: :principal` and
  `has_many :principals, through: :member_principals, source: :principal` —
  **`source:` aliasing**: `project.users` means "principals via members" but
  aliased to a different relation name. Scanner sees `:users` only; downstream
  has no way to know `project.users.first` ↔ `project.members.first.principal`.
- E2 `has_many :calculated_value_errors, …, as: :customized` — reverse-side
  polymorphic, same as Probe 4 D1.
- E3 `class_name: "Storages::ProjectStorage"`, `class_name: "Member"` —
  `::`-namespaced and bare class_name overrides.
- E4 `has_many :enabled_modules, dependent: :delete_all, after_remove:
  :module_disabled` — **collection callback** on association (`after_remove:`
  + method symbol). Scanner has no concept; semantics of "removing an
  enabled module triggers module_disabled" lost.
- E5 `enum :workspace_type, { project: "project", program: "program",
  portfolio: "portfolio" }, validate: true` — string-backed enum with
  validation side-effect.
- E6 `ALLOWED_PARENT_WORKSPACE_TYPES = {…}.with_indifferent_access` —
  class-level constant encoding domain rule.
- E7 11 concerns (`Projects::Activity`, `…AncestorsFromRoot`,
  `…CustomFields`, `…Hierarchy`, `…Storage`, `…Types`, `…Versions`,
  `…WorkPackageCustomFields`, `…CreationWizard`, `…Identifier`,
  `…SemanticIdentifier`, `::Scopes::Scoped`).
- E8 Chained `has_many :work_package_changes, through: :work_packages,
  source: :journals` — 3-hop relation (Project→WorkPackage→Journal),
  flattened to 1 hop.

## Universal gap taxonomy

Reorganized across all 5 probes:

| Code | Gap kind | Sites in probes | Repo-wide signal |
| --- | --- | --- | --- |
| **G1** | Macro-option blindness: `class_name`, `foreign_key`, `inverse_of`, `dependent`, `optional`, `source`, `through`, `as`, `polymorphic` | A2-A6, C2, D1-D5, E1-E3 | Affects every association line with options — the **default** in OP |
| **G2** | `polymorphic: true` (belongs_to side) | C1 (Journal) | **20 repo-wide** |
| **G3** | `as: :name` (reverse-side polymorphic on has_many / has_one) | D1, E2 | **5 repo-wide** (+ DSL-induced cases via `acts_as_attachable` not counted) |
| **G4** | `through:` flattened (1- and N-hop) | A2, D4, E1, E8 | **17 repo-wide** |
| **G5** | `source:` aliasing on through | E1, E8 | implicit when through-target differs from association name |
| **G6** | `class_name:` `::`-namespaced | C2, D2, E3 | pervasive in BIM module, in `Storages::*`, `Journal::*Journal` |
| **G7** | STI subtype semantics — hierarchy + discriminator column | A12, B1 (negative case) | **23 STI classes**, plus 1 explicit-disable |
| **G8** | `enum :foo, {…}` field declaration | A7, E5 | pervasive (status, workspace_type, etc.) |
| **G9** | `store_accessor :col, %i[…], prefix: true` JSONB pseudo-fields | C5 | rare in count (1) but 8 hidden fields on the most-edited model |
| **G10** | `attribute :foo, :type` — typed attribute override (Rails 5+) | (not in probes) | **27 repo-wide** |
| **G11** | `self.table_name = …` (runtime-expr or literal) | A1, C0 (literal) | distorts the `parse_schema` table-name join |
| **G12** | `self.inheritance_column = :_type_disabled` | B1 | rare but semantically critical when present |
| **G13** | `self.ignored_columns +=` runtime blacklist | C6 | common in legacy models post-migration |
| **G14** | `include Concern` — extension modules | A9, C7, D6, E7 | **65 top-level + many more in subdirs**; concerns are how OP composes models |
| **G15** | `scope :name, -> {…}` lambda-bodied scopes | A11, D7 | **98 repo-wide**; lambda bodies invisible |
| **G16** | `scopes :a, :b, :c` declarative scope-name list | A10 | uncommon but legit |
| **G17** | `default_scope -> {…}` global filter | A8 | invisible filter applied to every query |
| **G18** | DSL macros: `acts_as_*`, `has_paper_trail`, `register_*`, … | C3, C4 | pervasive in OP (acts_as_attachable, journalized, watchable, etc.) |
| **G19** | Lifecycle callbacks (`before_save`, `after_create`, `around_update`, …) | (counted) | **74 repo-wide**; scanner extracts only `validates`/`validate` |
| **G20** | Association-collection callbacks (`after_add`/`after_remove`) | E4 | rare but semantic |
| **G21** | Class-level constants encoding domain rules | D8, E6 | hard to bound without taste — only if user wants them as triples |

## Priority signal (frequency × semantic weight)

Pri-1 (high freq + high semantic weight):
- **G1** macro options — every association line is a candidate; fixing this
  closes G2–G6 simultaneously (they're all "macro option = X").
- **G14** concerns — 65 sites, each containing more associations + callbacks
  that hide a large fraction of the model surface area.
- **G2 + G3** polymorphic both sides — 25 sites; needed for Journal, Member,
  Reminder, Attachment-DSL surface; downstream graph integrity depends on it.

Pri-2 (medium freq, high weight):
- **G4 + G5** through + source — 17 sites; needed for `Principal.projects`,
  `WorkPackage.storages`, `Project.users`, etc. — high-traffic getter
  semantics.
- **G7** STI hierarchy — 23 classes; without it, the `Group < Principal`
  family is 5 disjoint graphs instead of one.
- **G8 + G9 + G10** enums / store_accessor / attribute — pseudo-fields not
  in schema.rb; many DB columns of type `integer` are actually enum strings
  at the Rails layer.

Pri-3 (high freq, lower per-site weight):
- **G15** scope lambdas — 98 sites; bodies often contain `joins`, `where`,
  arbitrary SQL. Authoritative-tier consumers can ignore; Inferred-tier
  consumers may want them.
- **G19** lifecycle callbacks — 74 sites; relevant for "what triggers what"
  triples.
- **G18** DSL macros — pervasive; hand-listing the OP-relevant ones
  (`acts_as_attachable`, `acts_as_journalized`, `acts_as_watchable`, …) is
  bounded by the gem count and tractable.

Pri-4 (rare or niche):
- G11–G13 table_name / inheritance_column / ignored_columns — handful of
  sites, each a meta-rule that distorts a single class.
- G16 G17 scopes / default_scope — uncommon but present.
- G20 association-collection callbacks — rare.
- G21 class-level constants — taste-dependent.

## What this signals

The C4 line-scanner is **correct on what it captures** (every association
leading symbol is real). It is **incomplete on options**: nearly every
real-OP association line carries options the scanner doesn't read. Two
ways forward, both visible from this evidence:

1. **Option-parser extension** (incremental). Add option-hash parsing on the
   macro lines — `scan::macro_options(line, macro_name) -> HashMap<Sym, Val>`
   alongside the existing `macro_symbols`. Closes G1 + G2 + G3 + G4 + G5 + G6
   in one architectural move. Stays within the C4 "dependency-free" decision.
   ~200 LOC plus tests.

2. **Parser graduation to `lib-ruby-parser`** (larger). Replace `scan::*` and
   per-extractor scaffolding with typed-AST traversal. Closes most gaps
   uniformly (including G14 concerns, G15 scope lambdas, G18 DSL macros via
   AST recognition, G19 callbacks). ~1500 LOC plus a real parser dep.

The frequency tables above give the user the data to choose between
incremental gap-closure and parser graduation; both paths are now
characterized rather than speculated.

## Out of scope of this probe

- Module-level `app/models/**/sub_dir/*.rb` — 941 - 112 = 829 files in
  subdirs (e.g. `app/models/work_package/`, `app/models/projects/`,
  `app/models/queries/`). Probes here are top-level only; the subdirs hold
  most of the concern modules (G14 surface).
- `modules/*/app/models/*.rb` (BIM, OAuth, Storages, etc.) — separate per-
  module model surface, not probed.
- `lib/` extension files — `lib/redmine/acts/*` defines several of the DSL
  macros (G18); reading those would catalog the DSL grammar.
