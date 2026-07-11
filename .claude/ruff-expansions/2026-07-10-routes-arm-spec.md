# SPEC v2 — ruff `routes.rs` harvest arm (gap (b): the routes.rb stratum)

> **Status: v2 CONSOLIDATED (5-savant pass folded in, 2026-07-10) → 3-reviewer
> pass next.** Process per operator directive: spec → 5 savants → consolidate
> → 3 brutal reviewers → fix → freeze v3 → implement. Direction is fixed;
> amendments are folded in place. v1→v2 changelog at the bottom.
>
> **Why this arm:** gap ledger (b) — "the routes.rb stratum (HTTP verb /
> member-collection / return shape), the one missing Action-kind fact source"
> (RAILS-COVERAGE-KIT §6, OGAR E-F17-PREREQ-VERIFIED) — and the
> E-CLICKWEG-CHOREOGRAPHY-1 joint: `InvokesAction`'s object is a route-helper
> STEM that today resolves to nothing. This arm mints the missing hop:
> **helper stem → `controller#action`**, making button → route → controller →
> mutation traces joinable for the first time.
>
> **Implementation base (pinned):** ruff branch
> `claude/openproject-transcode-status-c6e8in` @ `3510f05` = main @ `1fa73b9`
> (#72, 71 predicates) + W3. NOT the pre-#72 tree — the predicate count-lock
> is at 71 there, and #72's nav-plane doc conventions apply.

## 0. Non-goals (closed)

- **NOT return shape** (controller `respond_to`, not routes). Deferred; noted
  in module header. This is the ONLY Action-kind discriminant residual
  (S4: 5 of 6 `OpHandlerKind`s decidable from verb+scope+controller#action;
  only `AjaxJson` needs body facts).
- **NOT redirects** — both forms: `redirect("str")` AND block form
  `redirect { |params, req| … }` (corpus `config/routes.rb:54-56`; backlogs
  invokes a locally-bound lambda). Counted `escaped_redirects`.
- **NOT mounts** — both syntaxes: `mount X => "path"` AND `mount X, at:
  "path"` (corpus `:44` vs `:48`). Counted `escaped_mounts`. Mounts nested
  inside `namespace` blocks do NOT block sibling route parsing (corpus
  `:1274-1288`).
- **NOT proc/lambda endpoints** (`to: proc { … }`). Counted `escaped_procs`.
- **NOT dynamically generated routes** — `.each do |v| … end` loops with
  interpolated/variable names (corpus `:326-331` workspace_type loop,
  `:551-557` revisions loop). Any route declaration whose name/path/`as:`/
  `controller:` is a non-literal AST node (dstr, lvar, send) → counted
  `escaped_dynamic` (NEW counter), never silently dropped and never guessed.
- **NOT constraint semantics.** `constraints(...)` blocks/kwargs transparent;
  routes inside parsed normally (corpus-confirmed `:597`).
- **NOT `direct`/`resolve`/`draw`** — zero corpus occurrences. Unknown DSL
  calls → `escaped_other` (deduped names).
- **NOT APIv3** (Grape/roar — `representers.rs` arm).
- **NOT full Rails inflection.** Reuse `schema.rs::singularize` (make
  `pub(crate)`; proven on corpus) + routes-local `IRREGULAR`. `pluralize` is
  a NEW routes.rs-LOCAL fn (S5: schema.rs stays untouched beyond visibility),
  sharing irregular pairs. A wrong singular yields a differently-named stem,
  never a wrong controller#action; corpus fuse catches stem drift.
- **`defaults:`** — corpus never uses controller/action keys in `defaults:`
  (20+ occurrences, all format:/tab:/etc.). If a `defaults:` hash DOES carry
  `controller:`/`action:` keys → treat route as `escaped_dynamic` (defensive
  honesty) rather than mis-resolving.
- **`param:`/`path_names:`** — URL-only effects, no stem/controller impact
  (corpus: `param:` ~10×, `path_names:` 0×). Deliberately ignored.
- **Doc-debt in triple.rs beyond our edits** (stale "count = 34"/"ALL.len()
  == 62" headers predating us) — explicitly deferred, NOT fixed here.

## 1. Placement + shape

New module `crates/ruff_ruby_spo/src/routes.rs`, registered in `lib.rs`.
**AST walk via `lib-ruby-parser`** (NOT a line scanner): nesting is the
semantics (the D3 lesson). The walker descends generically through plain Ruby
`if`/`unless` bodies (corpus wraps `match`/`mount` in conditionals at
`:87-92`, `:1299-1310`) and treats `do…end` and `{…}` blocks identically
(same lib-ruby-parser Block node; corpus `member { post :toggle }`).

```rust
pub struct RouteEntry {
    pub stem: Option<String>,   // full Rails helper stem; None = no helper derivable
    pub verb: RouteVerb,
    pub controller: String,     // module-qualified path form: "work_packages", "admin/users"
    pub action: String,
    pub scope: RouteScopeKind,  // Member | Collection | Canonical | Standalone
    pub file: String,           // path relative to root, '/'-joined
}

#[derive(Clone, Copy, PartialEq, Eq, ...)]
pub enum RouteVerb { Get, Post, Put, Patch, Delete }
impl RouteVerb { pub fn as_str(self) -> &'static str /* "get" | "post" | ... LOWERCASE (pinned; matches ActionVerb::as_str + HasCallback lowercase discipline) */ }
impl From<ruff_ruby_spo::actions::ActionVerb> for RouteVerb { /* Post|Put|Patch|Delete; ActionVerb has no Get by design */ }

pub enum RouteScopeKind { Member, Collection, Canonical, Standalone }
impl RouteScopeKind { pub fn as_str(self) -> &'static str /* "member"|"collection"|"canonical"|"standalone" */ }

pub struct RouteTable { pub entries: Vec<RouteEntry> }
impl RouteTable {
    /// THE joint: resolve a helper stem (+ verb) to its controller#action.
    /// Pure lookup — composition stays in consumers (navigation.rs:9-13 discipline).
    pub fn resolve(&self, stem: &str, verb: RouteVerb) -> Option<&RouteEntry>;
}

pub struct RouteScanReport {
    pub files_scanned: usize,
    pub declared_routes: usize,
    pub entries_emitted: usize,
    pub entries_without_stem: usize,
    pub escaped_redirects: usize,
    pub escaped_mounts: usize,
    pub escaped_procs: usize,
    pub escaped_via_all: usize,
    pub escaped_dynamic: usize,      // NEW (S1): non-literal name/path/as/controller args
    pub escaped_other: Vec<String>,  // unknown DSL call names, deduped
}

// House pair convention (S4): plain + _with_report
pub fn extract_routes(root: &Path, namespace: &str) -> RouteTable;
pub fn extract_routes_with_report(root: &Path, namespace: &str)
    -> (RouteTable, RouteScanReport);
// scans <root>/config/routes.rb + <root>/modules/*/config/routes.rb (filename-sorted)
```

**Stem convention (join contract, S5):** stems are the LITERAL helper tokens
Rails generates — same convention as `actions.rs::action_target` (`:240-251`:
"an action's member prefix IS the action name", no `new_`/`edit_` stripping) —
explicitly NOT `navigation.rs::resource_stem`'s prefix-stripped form.

## 2. Facts emitted (2 additive `Predicate` variants)

- **`RoutesTo`** — `(ns:<stem>, routes_to, "<verb>:<controller>#<action>")`,
  one triple per (stem, verb) pair; verb LOWERCASE. Two-level object encoding
  (`:` discriminant then Rails' own `#`) — a deliberate new compounding of
  two established idioms (`HasCallback` `<phase>:<target>`; `#` is verified
  unclaimed in every emitted object across all spo crates, S3). Tier:
  **Authoritative**, own standalone match arm + rationale comment in
  `default_provenance` (mirror `ColumnNotNull`'s style, triple.rs:901-903).
- **`RouteScope`** — `(ns:<stem>, route_scope,
  "member"|"collection"|"canonical"|"standalone")`. CHANGED from v1 (S4
  finding 8): emitted for EVERY stemmed entry, closed 4-value vocab —
  explicit beats closed-world absence for the triple-only F17 lift path
  (RAILS-COVERAGE-KIT frames classifier consumption as canonicalized SPO).
  Tier: **Authoritative**.

Both: doc-comment first line `(subject, predicate, object)`; new plane header
`// ───── Rails routes.rb stratum (config-as-data) ─────` distinct from the
UI-navigation plane block (S3); add to `as_str`/`from_str`/`ALL`; bump
`predicate_count_locked_at_71` → 73 with an appended dated comment entry in
that test's house style (S3 — the one hard-coded number that WILL break).

**Subject IRI = bare `<ns>:<stem>`** — byte-identical to `InvokesAction`'s
object (`actions.rs:118-125` `format!("{namespace}:{}", target)`; S4 verified
zero rewriting). The canonical-index stem ↔ `navigates_to` screen-IRI
coincidence is CONDITIONAL on `NavVocab` naming discipline (callers name
screens after plural resource stems — true today, S3), stated as such.

**Drive-by (added to Gate):** fix the stale `InvokesAction` doc-comment
(triple.rs:514 claims object = `resource#member_action`; actual code emits
the bare helper stem — S3 finding 7).

## 3. Helper-stem generation (exact algorithm)

Scope stack; frames:

```
Resources  { plural, singular, shallow: bool }
Resource   { singular }
Namespace  { name }
Scope      { path: Option, as_prefix: Option, module: Option, controller: Option }  // controller NEW (S1 #1)
Controller { name }                                                                  // NEW frame (S1 #2, S2 #10)
Member | Collection | ConcernDef { name }
```

Derived at any point:
- `helper_prefix` = outer→inner join (`_`) of every `Namespace.name` and
  every `Scope.as_prefix` (Some only).
- `controller_prefix` = outer→inner join (`/`) of every `Namespace.name` and
  `Scope.module` (Some only).
- `ambient_controller` = innermost of: `Controller.name` frame, or
  `Scope.controller` (Some), — module-qualified with `controller_prefix`.
- `parent_resource_prefix(style)` = join (`_`) of outer `Resources`/
  `Resource` singulars — for **member-style** stems (show/edit/update/
  destroy/member calls), a frame with `shallow: true` DROPS its own
  contribution (S2 #9: Rails shallow un-nests member routes); collection/
  new-style stems always keep all contributions. Controller is UNAFFECTED by
  shallow (parent resources never prefix controllers; only namespace/module
  do). Corpus nesting reaches 3 levels (projects→meetings→agenda_items→
  outcomes, validated against `project_meeting_agenda_item_outcome_path`
  call sites — S2 #11); the join is recursive by construction.

**Canonical seven** for `resources :foos` (`only:`/`except:` filter; symbol
or array):

| action | stem | verb | scope |
|---|---|---|---|
| index | `{hp}{parent}foos` | get | Canonical |
| create | same as index | post | Canonical |
| new | `new_{hp}{parent}foo` | get | Canonical |
| edit | `edit_{hp}{parent}foo` | get | Canonical |
| show | `{hp}{parent}foo` | get | Canonical |
| update | same as show | patch AND put (2 entries) | Canonical |
| destroy | same as show | delete | Canonical |

controller = `{controller_prefix}foos`; `controller:` kwarg overrides the
last segment. `as: :qux` replaces the SEGMENT: plural position = `qux`
verbatim, singular position = `singularize(qux)` (corpus-verified:
`as: :show_group` → `show_group_path`, S2 #3). `path:` = URL-only, ignored.

**Singular `resource :foo`**: six routes (no index): show/update(patch+put)/
destroy at `{hp}{parent}foo`, create post at same stem, new/edit prefixed.
controller = `pluralize(foo)` with `controller_prefix` (corpus-verified:
`resource :overview` → `overviews/overviews_controller`, S2 #5).

**Member/collection verb calls** (block form or `on:` kwarg):
- First arg Symbol OR String: a bare-identifier String (`[a-z_][a-z0-9_]*`)
  is stem-equivalent to the symbol form (corpus: `get "hover_card" =>`,
  S2 #12); a String containing `/` or other non-identifier chars derives a
  helper ONLY via `as:` (else `entries_without_stem`).
- stem = `{action_seg}_{hp}{parent}{singular|plural}` (singular for Member,
  plural for Collection); `as:` replaces `{action_seg}` only (corpus:
  `as: "new_move"` → `new_move_work_packages_path`, S2 #4).
- action = **`action:` kwarg if present** (S1 #4, corpus `get :parent_page,
  action: "edit_parent_page"`), else `to:`/fat-arrow target's action if
  present, else the symbol/identifier.
- controller = `to:`/fat-arrow override if present, else innermost
  `Resources`/`Resource` controller.
- **Naked verb calls directly inside a `Resources`/`Resource` block** (no
  member/collection wrapper, no `on:`): Resource → Member-style by Rails
  convention; Resources → Collection-style (S1 #5; corpus
  `resource :form_configuration do get :reset_dialog`).

**Standalone verb calls** (not under any Resources/Resource):
- Target resolution, in order: (1) `to: "ctrl#action"`; (2) fat-arrow
  `=> "ctrl#action"`; (3) fat-arrow/`to:` **bare Symbol or no-`#` string** →
  action = that value, controller = `ambient_controller` (S1 #2/#6; corpus
  `get "x" => :x` in `controller … do`); (4) `action:` kwarg + ambient
  controller (S1 #3, ~35 corpus lines in `scope controller:` blocks); (5)
  symbol-form first arg + ambient controller → action = the symbol; (6) no
  resolvable controller → `escaped_other` entry (with the call name).
  All controller values get `controller_prefix` applied unless already
  `/`-qualified in the declaration.
- stem: `as:` value (with `helper_prefix`) if present; else symbol-form /
  bare-identifier-string first arg → that identifier (with `helper_prefix`);
  else string-path → **None** (recorded, no triple, `entries_without_stem`).
- `match …, via: [..]` → one entry per verb; `via: :all` → `escaped_via_all`.
- `root to: "x#y"` → stem = `as:` value else `root`; verb get; Standalone.
- `scope :symbol do` — first positional arg may be a Sym node, coerced like
  the string form (S1 #12).

**Concerns**: `concern :name do … end` stores the block; `concerns(:name)`
call AND `concerns: [...]` kwarg on resources replay it at the invocation's
stack. Same-file only. Unknown name → `escaped_other`. (Corpus uses only the
call form 3×; the kwarg path is spec'd but corpus-unfalsifiable — S1 (e).)

## 4. Provenance + namespace

All emitted triples Authoritative (route declarations name both ends in
machine-readable syntax). Namespace = caller's arg (`"openproject"`).
Sibling fact source, complementary not duplicative: OGAR's
`extract_action_rail` classifies controller actions onto the (part_of:is_a)
rail with zero routing knowledge; this arm supplies the routing layer that
can cross-reference it later (S5 #8).

## 5. Wiring + forward-compat

- `lib.rs`: `pub mod routes;` + re-export types + both extract fns.
- NO change to `extract_app_with_schema`; the nexgen bridge
  (`routes_harvest.rs` twin in op-codegen-pipeline) is a separate follow-up,
  as is any `harvest_op` routes dump (S5 #14).
- Module doc header cites `fuzzy-recipe-codebook.md` §8c ("config.json
  becomes DATA") as the doctrinal grounding: routes.rb → RouteTable is
  data-ingestion, not codegen (S5 #12).
- Forward-compat (S5 #10): `RouteTable::entries` is the future input for the
  §8b duplicate-routes/Scope-split detector — `controller` and `action` stay
  separate fields (never pre-concatenated) so that detector needs no schema
  change.

## 6. Test matrix (unit fixtures in routes.rs)

1. canonical seven incl. patch+put double; `only:`/`except:`
2. member/collection blocks; `on:` kwargs; brace-block form; **naked verb in
   Resource → member, in Resources → collection**
3. `as:` on resources / on member call / on string route
4. target forms: `to:"c#a"` / fat-arrow string / **fat-arrow bare symbol** /
   `action:` kwarg + ambient controller / `controller:`+`action:` kwargs
5. namespace nesting; `scope module:` / `scope "path"` (no prefix effect) /
   `scope :sym` / **`scope controller:`** / **`controller :x do`**
6. nested resources 2-level AND **3-level** (`project_meeting_agenda_item_outcome`)
7. singular `resource` (six routes, pluralized controller)
8. **`shallow: true`**: member-style stems drop the shallow frame's parent
   contribution; collection/new keep it
9. concern define + replay (call form AND kwarg form)
10. `match via:` expansion; `via: :all` escape
11. escapes: redirect (string + block form) / mount (both syntaxes) / proc /
    **`.each` dynamic** (→ `escaped_dynamic`) / **`if`-wrapped route is NOT
    escaped** (parsed through)
12. inflection: statuses→status, activities→activity, news→news,
    queries→query; pluralize: overview→overviews, category→categories
13. `resolve()` joint incl. an `ActionVerb`→`RouteVerb` conversion; a `to:`
    override where target action ≠ symbol (S2 #13)
14. no-stem string route: recorded, no triple, counted
15. triple shape: `RoutesTo` object `"<verb>:<ctrl>#<action>"` lowercase
    verb; `RouteScope` on EVERY stemmed entry (4 values); both Authoritative;
    `Predicate::from_str(as_str)` roundtrip; count-lock 73
16. member/collection String first-arg: bare identifier ↔ symbol
    equivalence; slash-string without `as:` → no stem

## 7. Corpus drift fuse (env-gated `RAILS_CORPUS_SRC`, self-skipping)

Run over the real corpus; assert: (a) `files_scanned == 29`; (b)
`entries_emitted` in a pinned band (recorded at first green run — measure,
don't claim); (c) spot-checks resolve: `work_packages`+get →
`work_packages#index`; `hover_card_work_package`+get →
`work_packages/hover_card#show` (member, String-arg); an `admin_` namespaced
route; root → `home`; `show_group`+get → `groups#show` (as: override);
`project_meeting_agenda_item_outcome`+get (3-level nesting); an
`action:`-kwarg route from the `scope controller: "my"` block; (d)
`escaped_other` matches a pinned allowlist; (e) `escaped_dynamic` ≥ 2 (the
two known `.each` sites) — the honest-denominator proof.

## 8. Gates

`cargo test -p ruff_ruby_spo -p ruff_spo_triplet` green · clippy clean ·
new/touched files fmt-clean · edit allowlist:
`crates/ruff_ruby_spo/src/routes.rs` (new) · `crates/ruff_ruby_spo/src/lib.rs`
(register) · `crates/ruff_ruby_spo/src/schema.rs` (pub(crate) `singularize`
ONLY) · `crates/ruff_spo_triplet/src/triple.rs` (2 variants + count-lock
71→73 + the InvokesAction doc-comment drive-by ONLY).
Base: `3510f05` (main#72 + W3). Nothing else moves.

## v1 → v2 changelog (savant consolidation, 2026-07-10)

- S1: `Scope.controller` field + `Controller` frame (~55 corpus routes);
  `action:` kwarg override (member/collection AND standalone); naked verb
  calls in resource bodies; bare-symbol fat-arrow targets; symbol+`to:` under
  bare namespace; `if`/`unless` descent; `escaped_dynamic` counter +
  `.each`-loop non-goal; both redirect/mount forms; `scope :sym`; brace
  blocks; `defaults:` defensive rule.
- S2: `shallow: bool` semantics (member-style drops prefix); 3-level nesting
  validated + fixture; String first-arg ↔ symbol equivalence rule; `to:`
  differing-action fixture queued.
- S3: plane header comment; standalone provenance arm; count-lock 71→73
  named; InvokesAction doc drive-by; NavVocab-conditional wording; verb
  casing question → resolved lowercase; stale-doc-headers deferred.
- S4: `From<ActionVerb> for RouteVerb`; plain+`_with_report` pair;
  `RouteScope` now explicit 4-value on every stemmed entry.
- S5: literal-stem join contract cited; routes-local `pluralize`; §8b
  forward-compat note; §8c doctrine citation; verb casing pinned lowercase.
- Orchestrator: implementation base pinned to `3510f05` (resolves S3 #11 /
  S5 #13 checkout-skew — #72 IS on main; working tree was pre-#72).

## Council protocol

5 savants DONE (verdicts folded above). Reviewers (3, strong tier) attack
THIS consolidated text: R1 correctness-adversary (construct a corpus line
the §3 algorithm still mishandles — prove it from
/home/user/openproject/config/routes.rb or modules); R2 scope-creep/baton
(boundary + closed-vocab discipline + Gate allowlist consistency); R3
test-sufficiency (which v2 clause has no §6/§7 test). Fixes land as a dated
v3 section; implementation follows frozen v3 only.

---

# v3 — FROZEN implementation contract (3-reviewer pass folded, 2026-07-10)

> This section supersedes v2 §3/§6/§7/§8 where it conflicts. Implementer
> reads the v2 body for structure + this v3 delta for the corrected rules.
> R1 found 6 P0 / 5 P1 / 1 P2 (all corpus-anchored, helper call sites
> grep-confirmed); R2 2 P1 + 5 P2; R3 test-traceability + pre-registered
> census. The path did NOT change — faithful Rails stem generation is
> sharper than v2 assumed. Correctness rule of the arm: **a stem we emit
> MUST be one Rails actually generates, else emit no triple and count it.**

## A. Structural change (absorbs P0 #2 + #3) — token/pair on the frame

`Resources`/`Resource` frames carry a precomputed pair, NOT raw plural/singular:
```
Resources { collection_token, member_token, shallow: bool }
Resource  { collection_token, member_token }   // singleton
```
- `Resources`: `member_token = singularize(name)`;
  `collection_token = (singularize(name) == name) ? format!("{name}_index") : name`
  (Rails `Mapper::Resource#collection_name`: plural==singular → `_index`).
  Corpus: `resources :news` → `news_index` (confirmed `project_news_index_path`);
  `:admin`, `:wiki`, `:activity`, `:gantt` identical.
- `Resource` (singleton): `member_token = name`; `collection_token = name`
  (Rails `SingletonResource` aliases collection_name to singular — P0 #3).
  `controller` still pluralizes (unchanged).
- ALL Collection-style stems (index/create/collection verb-calls) use
  `collection_token`; ALL Member/new/edit stems use `member_token`.

## B. P0 fixes (mandatory — wrong-fact-at-scale)

**B1 (P0 #1) — absolute `controller:` value.** On resources/resource/verb-calls:
if the `controller:` value starts with `/`, strip the slash and use it
VERBATIM, discarding `controller_prefix` and all `module:` frames; else
controller = `{controller_prefix}{value}`, the value replacing the
resource-derived name entirely even if multi-segment (`news/comments`).
51 corpus sites.

**B2 (P0 #4) — `controller:` kwarg on verb calls.** In BOTH member/collection
and standalone target resolution, a per-call `controller:` kwarg ranks WITH
`to:`/fat-arrow (before ambient/innermost-resource controller) and combines
with `action:`. Apply B1's `/`-rule to its value. Corpus: `humanize_schedule,
controller: "recurring_meetings/schedule"` → `recurring_meetings/schedule#humanize_schedule`.

**B3 (P0 #5) — non-resource kwargs on resources/resource wrap a scope.** Any
kwarg outside `{as, controller, path, only, except, param, concerns, shallow}`
is replayed as an enclosing `Scope` frame carrying that kwarg (Rails
`apply_common_behavior_for` slice). At minimum `module:` MUST be honored:
`resources :types, module: "work_package_types"` → controller
`work_package_types/types`, helper unprefixed.

**B4 (P0 #6) — naked verb in a `resources` (plural) block is NESTED, not
collection.** Rails `nested{}`: name = parent-first
`{hp}{parent_incl_this_singular}_{action_seg}`, scope path is member-like →
emit `RouteScope` = `"member"`. (The `Resource` singleton half of v2's naked-verb
rule stays Member-style — correct.) Corpus: `get "/roadmap" => "versions#index"`
inside `resources :projects` → `project_roadmap` (confirmed
`project_roadmap_path`). A bare-`/` string here → collection candidate =
`collection_token` (see B5/C3).

**B5 (P0, folded from #2) — the `_index` double-emission is resolved by A**:
`resources :news` emits index/show on DISTINCT stems (`news_index` get /
`news` get), no collision. Any residual true (stem,verb) duplicate → C4.

## C. P1 fixes

**C1 (P1 #7) — Rails auto-names slash/word string paths.** A String first arg
matching `^/?[-a-z0-9_/]+$` (NO `:` `(` `*` `.`) derives a stem: strip leading
`/`, map `/` and `-` → `_`, compose per scope (standalone `{hp}{derived}`;
collection candidate `[derived, hp, collection_token]`; bare `/` contributes
nothing → collection stem = `collection_token`). ONLY paths with dynamic
(`:x`), glob (`*x`), or format (`.x`) segments fall to `entries_without_stem`.
Corpus: `/api/docs`→`api_docs` (confirmed), `/watch`→`watch`, `put "/" =>
"roles#bulk_update"` in a collection → `roles`.

**C2 (P1 #8) — `as: ""`** (empty) omits the action segment entirely (no
leading `_`); result intentionally collides with the canonical stem → resolved
by C4 order. Corpus: `get "(/:tab)" => "work_packages#show", on: :member,
as: ""` → `work_package` (dup-skips canonical show).

**C3 (P1 #9) — declaration order + `resolve` tie-break.** Walk emits entries in
declaration order EXCEPT a resources/resource's canonical routes emit AFTER its
block body (Rails registers block customs first, canonicals shadow-skip on
name collision). `RouteTable::resolve` returns the FIRST entry for (stem,verb).
A later same-(stem,verb) entry with a DIFFERENT `controller#action` increments
a new `duplicate_stem_conflicts: usize` counter, stays in `entries`, but emits
NO second `RoutesTo` triple (preserves §2 "one triple per (stem,verb)").

**C4 (P1 #10) — multi-name `resources :a, :b`.** N positional Symbol args replay
the whole declaration (kwargs + block) once per name. Non-literal names in the
list → `escaped_dynamic`. Corpus: `resources :activity, :activities`.

**C5 (P1 #11) — non-literal `action:`/`to:` values → `escaped_dynamic`** (extends
v2's counter beyond name/path/as/controller). A `concern` block with a block
param (`do |options|`) replays ONLY if every route-relevant kwarg in its body
is literal; else the whole replay is `escaped_dynamic`. Corpus:
`concern :with_split_view do |options| get "…", action: options.fetch(...)`.

## D. P2 fix

**D1 (P2 #12) — `only:`/`except:` accept Symbol, String, `%i[]`, `%[]`, or array
of either; coerce every element via to_sym.** Corpus: `only: %[show]` (String).

## E. R2 gate/wiring fixes

**E1 (P1) — §8 triple.rs allowlist wording.** Replace with: "2 variants incl.
their `as_str`/`from_str`/`ALL`/`default_provenance` arms + the
`// ───── Rails routes.rb stratum (config-as-data) ─────` plane header + rename
`predicate_count_locked_at_71`→`_at_73` with appended house-style comment entry
+ the `InvokesAction` doc-comment drive-by (triple.rs:514) ONLY."

**E2 (P2) — lib.rs registration is PRIVATE mod + curated re-export** (sibling
convention, lib.rs:37-66): `mod routes;` + `pub use routes::{RouteEntry,
RouteVerb, RouteScopeKind, RouteTable, RouteScanReport, extract_routes,
extract_routes_with_report};`. NOT `pub mod`.

**E3 (P2) — `From<ActionVerb>` path** = `crate::actions::ActionVerb`
(the `actions` mod is private; impl lives in routes.rs, orphan-rule-clean).

**E4 (P2) — `IRREGULAR` + `pluralize` are routes.rs-LOCAL** (schema.rs's
`IRREGULAR` is fn-local; only `singularize` visibility changes). Duplicate the
irregular pairs into routes.rs.

**E5 (P2) — `RouteScope` object = `entry.scope.as_str()`**, never a re-typed
literal (one doc sentence, mirrors `HasVisibility`).

**E6 (P1, honesty note) — verb-ambiguity of triple-only joins.** §2 gains:
"triple-only joins are verb-ambiguous on multi-verb stems (canonical
show/update/destroy share a stem); the verb-carrying join is
`RouteTable::resolve(stem, verb)` frontend-side. An `invokes_with_verb` fact is
explicitly deferred." `concerns:`-kwarg path stays spec'd (standard Rails, ~10
LOC, test-covered).

## F. R3 test/fuse fixes (§6/§7 replace)

**F1 — PRE-REGISTERED census bounds** (measured 2026-07-10; replace v2 §7(b)
"pin at first green run"). The fuse asserts, and these may only NARROW after
first run, never widen:
- `files_scanned == 29`
- `escaped_mounts == 9`
- `escaped_via_all == 6`
- `escaped_dynamic == 2` (the `.each` sites config/routes.rb:326, :551; `==`, not `>=`)
- `declared_routes >= 974` (156 `resources` + 110 `resource` + 708 verb-calls)
- spot-checks resolve: `news_index`+get→`news#index`; `work_packages`+get→
  `work_packages#index`; `hover_card_work_package`+get→`work_packages/hover_card#show`;
  `reassign_work_packages_bulk`+match→bulk target; `menu_project_gantt_index`+get;
  `project_meeting_agenda_item_outcome`+get; `api_docs`+get→`api_docs#index`;
  root: `home`+get→`homescreen#index` AND the 2nd root (account#login) → C3 tie-break
- `escaped_other` matches a pinned allowlist
- NO emitted stem matches `/[#{}]/` and none equals `workspace_types`/`diff_revision`
  (the `.each` would-be-guesses) — false-positive guard for B/C1

**F2 — new unit fixtures** (add to §6): F(a) join test — actions + routes from
one fixture, assert `RoutesTo` subject set ∩ `InvokesAction` object set
byte-equal for a member action (§2's raison d'être, was untested); F(b)
absolute `/`-controller; F(c) `_index` collection (news/admin/gantt); F(d)
singleton collection singular; F(e) `module:` scope-wrap; F(f) naked-verb-nesting
(projects→roadmap); F(g) multi-name replay; F(h) auto-named slash path
(`/api/docs`); F(i) `as: ""`; F(j) (stem,verb) duplicate → `duplicate_stem_conflicts`
++, single triple, `resolve` returns first; F(k) `.each` fixture: `entries_emitted`
delta == 0, no `#{`/lvar residue; F(l) `defaults:{controller:}` → escaped_dynamic;
F(m) unknown call dedup in `escaped_other`; F(n) block-param concern → literal
replays / non-literal escaped_dynamic; F(o) `controller: "/abs"` NOT double-prefixed.

**F3 — `declared_routes` identity** (define the field): `declared_routes ==
entries_emitted-sites + entries_without_stem + all escaped_*`. Assert the
identity in F1 and the corpus fuse.

**F4 — gate is per-file rustfmt** (§8): `rustfmt --edition 2021 --check` over
the exact edit allowlist; doc-comment/plane-header clauses are review-only —
SAY so.

## G. FROZEN edit allowlist + gate (supersedes §8)

Files (nothing else moves; base = ruff `3510f05` = main#72 + W3):
- `crates/ruff_ruby_spo/src/routes.rs` — NEW (the arm + `#[cfg(test)]` incl. the env-gated fuse)
- `crates/ruff_ruby_spo/src/lib.rs` — register (private mod + curated re-export, E2)
- `crates/ruff_ruby_spo/src/schema.rs` — `pub(crate) singularize` ONLY (E4)
- `crates/ruff_spo_triplet/src/triple.rs` — 2 variants per E1 (+arms +plane header +count-lock 71→73 +InvokesAction doc drive-by)

Gate: `cargo test -p ruff_ruby_spo -p ruff_spo_triplet` green · `cargo clippy
-p ruff_ruby_spo -p ruff_spo_triplet` clean · per-file `rustfmt --check` on the
4 files · the corpus fuse (F1) green under `RAILS_CORPUS_SRC=/home/user/openproject`.

**v3 is frozen. Implementation follows this contract exactly; deviations
require a new dated section, not an inline edit.**

---

# 2026-07-10 — SHIPPED (ruff PR #73)

The routes.rb harvest arm landed per this frozen v3 contract. Sonnet grinder
implemented (chunked `tee -a` after two single-write attempts died on the
flaky-connection infra); the Opus orchestrator gated centrally in the shared
`target/` and fixed four issues the subagent's own fixtures didn't exercise —
each verified against the real corpus, none a spec defect:

1. **`escaped_other` verb leakage → namespace-path controller fallback.** A
   bare `get "plugin/:id", action: :show_plugin` inside `namespace :admin;
   namespace :settings` (no controller frame) was dumping the verb name into
   `escaped_other` (the unknown-DSL bucket). Rails resolves it to the module
   scope path — `admin/settings#show_plugin` (verified: the real handler is
   `app/controllers/admin/settings_controller.rb`). `ambient_controller` now
   falls back to `controller_prefix` when only namespace frames exist;
   `escaped_other` is now exactly `["use_doorkeeper"]`.
2. **`escaped_dynamic` measured = 13, not R3's pre-registered 2.** R3's
   grep-estimate counted the two `.each` loop *sites*; the faithful walker
   counts every route *declaration* it conservatively declines (interpolated
   `.each` bodies + module routes carrying a non-literal-shaped `action:`/path
   alongside benign unknown kwargs like `work_package_split_view:` /
   `defaults:` hashes — calendar 4, team_planner 4, boards 1, backlogs 1). All
   counted, none silently dropped (honest denominator). Tightening their
   classification so more emit a fact is tracked below as follow-up polish.
3. **`as:` verbatim collection name.** `resources :foos, as: :bar` → `bar`
   (Rails guide: `as: 'images'` → `images_path`, not re-pluralized), not
   `bars`. Implementation was correct; the subagent's own test asserted the
   wrong `bars` — test corrected.
4. **3-level-nesting spot-check.** The v3 §F1 list named
   `project_meeting_agenda_item_outcome` (show), but the innermost
   `resources :outcomes, except: %i[index show]` removes that helper. The
   walker correctly respects `except:`; the spot-check now uses
   `new_project_meeting_agenda_item_outcome` (which exists + is used in-app).

**Measured corpus fuse (pinned):** 29 files · 1625 declared · 1534 emitted ·
42 stemless · 0 duplicate conflicts · escaped {redirects=19, mounts=9, procs=1,
via_all=6, dynamic=13} · other=`["use_doorkeeper"]`. Tests 149 (ruby) + 133
(triplet), clippy-clean on the 4 touched files, per-file fmt-clean.

**Gap ledger after this arm:** (a) writes/calls CLOSED · **(b) routes.rb
stratum CLOSED** (this arm — stem→controller#action now resolvable) · (c)
recipe codebook Phase 2 unwired · (d) permission-declaration arm · (e)
DB-resident choreography content → hydrator.

**Follow-up polish (tech-debt, not blocking):** the 13 `escaped_dynamic`
includes module routes conservatively declined only because of a benign
unknown kwarg (`work_package_split_view:`) or a non-controller `defaults:`
hash next to a literal `action:`; a tighter literal-check could emit facts for
those instead of declining. Filed as a follow-up, not a fact bug (declining is
safe; emitting a wrong fact is not).

---

# 2026-07-10 — codex review follow-up MERGED (ruff PR #74)

Two codex P2 comments on the merged #73 — both real bugs in the W3
migration-replay (`schema.rs`), both fixed in #74 (branch restarted from main
per the merged-PR protocol):

1. **`def down` rollback bodies were being replayed.** `def up`/`def change`
   describe the schema *after* the migration; `def down` reverses it. The
   replay scanned both halves, so an add-in-`up` + remove-in-`down` netted the
   column away. Fix: a Ruby block-depth tracker skips `def down` bodies.
   **Pinned-fact correction:** the OpenProject `WorkPackage` baseline+replay
   column count is **33, not 31** — `create_work_package_semantic_ids`'s
   `def up` adds `sequence_number`/`identifier` and its `def down` removes
   them; Rails applies only `up`, so they are RETAINED. The old "net to zero"
   (31) was the bug. Corpus schema gate + drift fuse re-pinned 31 → 33.
2. **Parenthesized `add_column(:t, :c, :type)` was silently dropped** —
   `parse_add_column` didn't `strip_call_parens` like its siblings. Fixed.

Both got regression tests. This closes the review loop on the routes-arm PR
arc (ruff #72 nav-plane → #73 routes-arm + W3 → #74 codex fixes, all merged).
The routes.rb harvest arm (gap b) is shipped and hardened; W3's migration-
replay is corrected. Gap ledger unchanged: (b) CLOSED; (c)/(d)/(e) open.

---

# 2026-07-11 — count-lock fuse reframed by the OGAR hot-plug doctrine (ruff #77)

Post-merge, #76 (region-grammar plane) bumped the shared predicate count-lock
73 → 76 and updated `ruff_spo_triplet`'s canonical lock, but the routes arm's
`triple_shape_and_predicate_roundtrip` test still hardcoded `ALL.len() == 73` —
so main went red (two crates asserting one global count). Fixed in ruff **#77**:
the routes test now proves only its own two variants (`RoutesTo`/`RouteScope`)
roundtrip and does NOT re-assert the total.

**Doctrine (read this session):** `OGAR/.claude/knowledge/hotplug-consumer-migration.md`
(PRs #174/#175/#176/#178) — the plug-and-play pattern. A consumer declares a
`HOT_PLUG` const (its hot classids + covered capabilities) and one activation
test; `capability_registry::resolve_hotplug` returns vocab rows + capabilities
by the classid join, with five named drift arms (`UnknownClassid` /
`NoCapabilitiesFor` / `UnexpectedConsumer` / `Uncovered` / `Undeclared`) firing
at the consumer boundary in the consumer's own binary. Motto:
*"wenn's knallt, dann einmal — nicht 200 Pins monitoren."* A cross-crate
re-assertion of a shared global count is exactly the "monitor N pins"
anti-pattern this retires — vocabulary drift is caught by classid/capability
resolution, not a global integer every arm duplicates. The residual `COUNT_FUSE`
didn't vanish; it *consolidated* to the lance-graph **bridge** side
(`rust-test.yml` mirror-parity vs the real OGAR sibling). So #77 moved the ruff
side into the same shape: remove the scattered assertion, keep drift-detection
in its single owned home.

**Forward:** the doc's future-synergy #1 (ActionDef plug-and-play from ruff:
`ruff_*_spo` harvest → fuzzy-recipe codebook → `lift_actions` → domain table
auto-derives) is the path on which the routes arm's `RoutesTo`/`RouteScope`
predicates become an *input* to `resolve_hotplug` — at which point the global
`predicate_count_locked_*` is fully vestigial. That retirement belongs to the
crate's owning session (`ruff_spo_triplet`), not this one; scope of the update
here is my own arm (routes.rs comment, ruff #77 `cd254f8`).
