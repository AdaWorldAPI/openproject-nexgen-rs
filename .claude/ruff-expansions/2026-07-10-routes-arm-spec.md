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
