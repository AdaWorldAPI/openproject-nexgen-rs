# Rails coverage kit — config, crates/shape, runbook + the canonical-label doctrine

> For the OpenProject/Redmine (Rails → Rust → SurrealQL/OGAR) session. Saves the
> exact reproducible measurement kit used 2026-06-30 to cross-check the
> recipe-bitmask conjecture on real Rails source, and pins **how the extracted
> shape lands canonically reusable** (content-addressable concept ids + label
> DTOs) instead of an extinct zoo of per-consumer label enums.
>
> Harness lives at `.claude/tools/rails-coverage-harness/`. Companion: the
> handover `.claude/handovers/2026-06-30-0715-odoo-recipe-bitmask-to-openproject-rails.md`
> and OGAR `E-RECIPE-BITMASK` / `E-RECIPE-BITMASK-CHAIN` / `F15` / `F16`.

## 0. Measured baselines (same metric across consumers — regression anchors)

STI/`inherits_from` **chaining collapse** (how much of the materialised method
recipe is inherited-shared = free at the cost of an import):

| Consumer | models | with-ancestors | chaining collapse (methods) |
|---|---:|---:|---:|
| Odoo (Python, `inherits_from`) | 388 harvested | 26% | **22.7%** |
| Redmine (`app/models`) | 111 | 52% | **53.8%** |
| OpenProject (`app/models`, recursive) | 684 | 71% | **71.7%** |

Monotonic with inheritance density — the "best-shaped consumer" result.
OpenProject models-only coverage ledger (declaration count, naive recipe 13 550):
**~11% minted (structural) + ~69% imported (STI) = ~80% covered, ~20% own** —
a lower bound (concerns uncredited, ORM-castration drops more).

## 1. Crates & shape (what to call, what comes back)

Two crates from `AdaWorldAPI/ruff`, both `workspace = true`-inheriting (so they
need the ruff workspace root present — see the isolation trap in §2):

- **`ruff_ruby_spo`** — the Ruby/Rails frontend. Entry points (`src/lib.rs`):
  - `extract(&Path) -> ModelGraph` — walks `<root>/app/models/**/*.rb`, default namespace `"openproject"`.
  - `extract_with(&Path, namespace) -> ModelGraph` — same, caller-tagged namespace (use for Redmine/Spree/…).
  - `extract_app_with(&Path, ns)` — **core + every engine** (`modules/*/app/models`, `engines/*/app/models`). OpenProject keeps ~half its domain in `modules/*`; `extract`/`extract_with` are core-only.
  - `extract_from_source(&str) -> Vec<RubyClass>` — single-file, lower level.
  - Same-named `class X` reopens across files merge into one `Model` (Vec fields concat; `sti` first-non-None-wins).
- **`ruff_spo_triplet`** — the language-agnostic IR + expander (shared with the Python/Odoo frontend). Re-exports at the crate root: `AssocKind`, `ValidationKind`, `Callback`, `ConcernRef`, `StiInfo`, `Model`, `ModelGraph`, `expand`, `Triple`, …

`Model` fields the harness reads (`ir.rs`): `name`, `fields`, `functions`
(`Vec<Function>`), `associations` (`Vec<AssocDecl{kind: AssocKind, name, options}>`),
`validations` (`Vec<Validation{kind: ValidationKind, target, options}>`),
`callbacks` (`Vec<Callback{phase: String, target, options}>`),
`concerns` (`Vec<ConcernRef{kind: ConcernKind, module, body_ref}>`),
`attributes`, `scopes`, `delegations`, `sti: Option<StiInfo{inherits_from: Option<String>, …}>`.

`ruff_spo_triplet::expand(&graph) -> Vec<Triple>` emits the canonical SPO shape
(same predicates as the Python frontend, plus AR-shape): `has_function`,
`has_callback` (`"<phase>:<target>"`), `validates_constraint` + `validation_kind`
+ `validation_param`, `inherits_from` (STI parents/concerns), association
predicates. **This is the bridge to OGAR** — the triples are the IR OGAR lifts.

The **"recipe"** = the AR-DSL declaration types (associations, validations,
callbacks, scopes, concerns) — a *fixed enumerable protocol*, captured as
first-class typed data (unlike Odoo, where it's inferred from method-name
prefixes + `raises`).

## 2. Exact config (and the one trap)

`.claude/tools/rails-coverage-harness/Cargo.toml` — a **standalone workspace**
(empty `[workspace]` table). Deps: `ruff_ruby_spo` + `ruff_spo_triplet`.

**THE TRAP (do not skip):** do **not** `cargo run -p ruff_ruby_spo` from inside
the ruff workspace. Resolving the full ruff workspace pulls `ruff_server`'s
`lsp-types` git dep, which the agent proxy **403s** — the build dies on an
unrelated member. Building from a *standalone* project that path/git-deps only
the two crates resolves just their subgraph (no `ruff_server`, no `lsp-types`).

- **Form A (portable, recommended):** git deps to `AdaWorldAPI/ruff` branch `main`.
- **Form B (offline, what was run):** `path = "/home/user/ruff/crates/ruff_ruby_spo"` + `…/ruff_spo_triplet` (a local checkout that contains the ruff workspace root).

Crate-root re-export gotcha: `ir` is a private module — import the types from the
crate root (`ruff_spo_triplet::{AssocKind, ValidationKind}`), **not** `::ir::`.

## 3. Source acquisition (GH_TOKEN + pygithub)

> **Scope caveat (record honestly):** upstream `redmine/redmine` and
> `opf/openproject` are **outside** the session's default 16-repo GitHub scope.
> They were read under explicit operator authorization of `GH_TOKEN` (public
> repos are world-readable; the agent proxy permitted the read). Only aggregate
> stats are derived; **no upstream (GPL) source is committed anywhere.** A future
> session must re-confirm authorization before pulling upstream again.

Token handling (per medcare-rs note — strip stray quotes):
```python
tok = (os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN") or "").strip().strip('"').strip("'")
from github import Github, Auth
gh = Github(auth=Auth.Token(tok))
```

Bulk download `app/models` (recursive, **preserve structure** so concern subdirs
are walked — the c17 probe only saw the 112 top-level files):
```python
repo = gh.get_repo("opf/openproject")               # or "redmine/redmine"
sha  = repo.get_branch(repo.default_branch).commit.sha
tree = repo.get_git_tree(sha, recursive=True)       # 1 call, no truncation for these repos
paths = [e.path for e in tree.tree
         if e.type == "blob" and e.path.startswith("app/models/") and e.path.endswith(".rb")]
# fetch each via raw (no API rate limit), threaded; write to <root>/app/models/<path>
#   url = f"https://raw.githubusercontent.com/opf/openproject/{sha}/{p}"   (requests honours HTTPS_PROXY)
```
Redmine: 91 files. OpenProject: 945 files (`app/models` recursive). `modules/*/app/models`
is a *separate* surface (OpenProject's other ~half) — pull it too + use
`extract_app_with` if you want full-app, not just `models only`.

## 4. Runbook

```bash
# 1. download source -> /tmp/.../<consumer>_src/app/models/**   (see §3)
# 2. build + run the harness (standalone workspace; Form A needs network)
cd .claude/tools/rails-coverage-harness
cargo run --release -- /tmp/.../openproject_src openproject
# 3. read the report: composition, recipe surface, density, STI chaining collapse.
#    Compare collapse vs the §0 baselines (regression: should track inheritance density).
```
If offline, switch the Cargo.toml deps to Form B (local ruff path) first.

## 5. Canonical-label doctrine — content-addressable indices, not a label zoo

**The risk (the operator's exact concern).** Each consumer frontend invents its
own label set: Odoo has `MethodKind{Compute, Check, …}`; Rails has callback-phase
strings (`"before_save"`, `"after_create_commit"`), `ValidationKind`,
`AssocKind`, and the emergent `HandlerKind{AjaxJson, ListForTenant, SoftDelete, …}`.
Left as per-consumer enums/strings, these become **an extinct zoo**: a class's
recipe in Rails can't be compared, shared, or co-resolved with the same concept
in Odoo. The recipe-bitmask only pays off if the bitmask slots are **the same
across consumers**.

**The fix (the operator's instinct, made canon).** A recipe label is **not** a
per-consumer enum variant — it is a **content-addressable concept id** in a
shared **generic recipe ontology**, and the per-language surface string is a
thin **label DTO** pointing at that id. This is exactly OGAR's existing
class-concept doctrine (`classid` is the address; the lo-u16 is the **shared
concept**; the hi-u16 / surface name is the **render skin**) — now applied to
the *recipe vocabulary*, not just the class vocabulary.

```
            generic recipe ontology (canonical, minted ONCE in OGAR)
                 RecipeConceptId  (content-addressable index, RESERVE-DON'T-RECLAIM)
                          ▲                         ▲
          LabelDto{ id, lang:"ruby",  surface }    LabelDto{ id, lang:"python", surface }
              "before_save" ───────────┐     ┌─────── "@api.constrains" (raises)
                                        ▼     ▼
                               LIFECYCLE_BEFORE_PERSIST   ← one slot, both consumers
```

**The shared recipe families (mint as canonical concept ids; surface strings are DTOs):**

| Family | Canonical concept id (illustrative) | Rails surface | Odoo surface |
|---|---|---|---|
| Lifecycle hook | `LIFECYCLE_BEFORE_PERSIST` / `_AFTER_CREATE` / `_AFTER_COMMIT` / `_BEFORE_VALIDATION` … | `before_save`, `after_create`, `after_create_commit`, `before_validation` | `@api.constrains`(raises)→before-persist; `_compute_*`@depends→on-commit |
| Guard kind | `GUARD_PRESENCE` / `_UNIQUENESS` / `_RANGE` / `_FORMAT` / `_ASSOCIATED` | `validates presence:` / `uniqueness:` / `validates_associated` | `_check_*` body pattern (validation_kind) |
| Relation kind | `REL_MANY_TO_ONE` / `_ONE_TO_MANY` / `_MANY_TO_MANY` / `_ONE_TO_ONE` / `_THROUGH` | `belongs_to` / `has_many` / `habtm` / `has_one` / `through:` | `Many2one` / `One2many` / `Many2many` |
| Action kind | `ACTION_LIST_FOR_TENANT` / `_DETAIL` / `_SOFT_DELETE` / `_TOGGLE_BOOL` / `_AJAX_JSON` … | controller `HandlerKind` | (Odoo `action_*`, when the producer fact lands) |

**Three rules that keep it reusable (not a zoo):**

1. **Bitmask slot = concept id, never a surface string.** The recipe-bitmask
   (per-class override vector, `E-RECIPE-BITMASK`) indexes the canonical
   `RecipeConceptId` set. A Rails `before_save` and an Odoo before-persist guard
   land on the **same slot** → the bitmask is cross-consumer comparable and the
   recipe is shared. Slots are **RESERVE-DON'T-RECLAIM** (`I-LEGACY-API-FEATURE-GATED`):
   append new concept ids; never reorder, never repurpose a slot.

2. **One ontology, N label DTOs.** A new consumer with a new surface label for an
   *existing* concept emits a `LabelDto{ existing_id, lang, surface }` — it
   **reuses the slot**, mints nothing. Only a genuinely-new concept mints a new
   id (extends the ontology; never forks it). The frontend's job is the DTO
   mapping (`"before_save" → LIFECYCLE_BEFORE_PERSIST`), not a private enum.

3. **The id is the truth; the label is the skin.** Resolution, RBAC, and the
   recipe-bitmask key on the content-addressable id (stable forever); the surface
   string is render-only and per-language. This is `canonical_concept_id`
   (forward) / `canonical_concept_name` (reverse) in `ogar-vocab`, extended from
   class concepts to *recipe* concepts. PII/leaf-rename guarantees ride the DTO
   skin, never the id.

**Concrete "mint accordingly" for ruff/OGAR (the next step):**
- Today `ogar_vocab::KausalSpec::LifecycleTrigger{ event: String }` and the
  ruff `Callback{ phase: String }` carry the surface string. **Canonicalize:**
  add a `RecipeConceptId` resolved from that string at lift time, keeping the
  string as the `LabelDto` surface. The recipe-bitmask + cross-consumer
  convergence then key on the id.
- Mint the four families above as a **recipe-concept codebook in OGAR** (sibling
  to the class-concept codebook), with the same forward/reverse + per-lang DTO
  table. The ruff frontends (`ruff_ruby_spo`, `ruff_python_spo`) emit the surface
  label; the OGAR lift resolves it to the canonical id.
- Until that lands, the bitmask is per-consumer (the zoo). The probes in this kit
  measure the *shape*; the canonicalization is what makes the shape *converge*.

**Why this is the whole point.** The OGAR thesis — "planner times align with
billable hours" — is exactly cross-consumer concept convergence: OpenProject's
`TimeEntry`, Odoo's `account.analytic.line`, and WoA's `Stundenzettel` already
converge on one class concept (`BILLABLE_WORK_ENTRY`, `0x0103`). The recipe
vocabulary must converge the same way, or the behavioural arm fragments back
into the zoo the structural arm escaped.

## 6. Function catalog = CRITERIA over shipped verbs + the body-pass triage

> Canon: OGAR `E-FUNCTION-CATALOG` + `E-ACCIDENTAL-IMPERATIVE` + `F17`. This
> section is the bounded build spec the next session executes — it is NOT
> "decompile Ruby," and it is NOT "build filter/map/project."

**The verbs are already shipped, deterministic — do not re-implement them.**
`filter` = the query/predicate engine · `project` = the compute/recompute DAG
(`KausalSpec::Depends`) · `map` = a deterministic table/lookup · `reduce` = the
semirings. A consumer function is `(verb, criteria)`; the deliverable is the
**criteria** (a selection-condition + params), EXTRACTED from source, keyed by a
canonical concept id, grounded on a domain ontology. The landing zone is the
existing `lance-graph-contract::action` (`actions_for`/`OgarResolver`) +
`ogar-render-askama` artifact_kinds + `ogar-vocab`.

**Two corrections to avoid (both were mistakes this session):**
- **`map` ≠ CAM-PQ.** Kontenerkennung (account determination) is a deterministic
  relational rule resolution — product/category default account → fiscal-position
  remap → precedent (`reduce(filter(prior_bookings, partner×product), most-recent
  account)`) with a fallback order. Natural keys ⇒ register (relational lookup +
  history query), not ANN (`I-VSA-IDENTITIES` Test 0). CAM-PQ is a *separate
  opt-in* layer for fuzzy suggestion on unseen combinations only.
- **CRUD is generic, not hand-rolled.** create/update = the generic AR lifecycle
  (defaults → validate → hooks → persist → journal); per-class criteria = the
  recipe + the permission (RBAC) + the writable-field set; the action is a
  HandlerKind (`create/update-for-tenant`). Only a non-standard hook *body*
  escapes.

**The residue split.** What's left ("hand-rolled hook bodies") is two
populations, not one:
- **accidentally-imperative** — AR verbs on AR targets, written imperatively only
  because the source had no declarative form (Odoo `@api.depends` vs Rails
  `before_save { total = lines.sum }` — same logic, source-expressiveness, not
  complexity). Recoverable to `(verb, criteria)`.
- **essentially-foreign** — real algorithms / external / ledger math. The only
  true escape (point-to-body, lossless-DO §1).

**The body pass TRIAGES — it does not decompile.** The bounded thing to build:
per hook body, recover `(target classid, verb-class, order-signature)` —
"something that calls an update on X, in some order" — grouped by target. Three
landing tiers (a body drops only as far as its deformity forces):

| Tier | Recovered | Lands as |
|---|---|---|
| clean | `(verb, criteria)` | declarative emit |
| coarse | `(target classid, verb-class)`, order unknown | catalog at coarse key + point-to-body; **order-signature gates recover vs preserve** |
| foreign | no clean target/verb | full escape |

**"random orders" is the gate, not noise.** Incidental order (ops commute) →
the order-free `(verb,criteria)` round-trips → RECOVER. Significant order →
PRESERVE (sequenced/foreign body) + RFC if behaviour diverges ("runs" can hide
ordering/side-effect quirks the app now depends on — behaviour-preserving, never
silently "fix"). Arbiter = the **round-trip-order-free parity check**.

**Falsifier (`F17` / `PROBE-OGAR-BODY-TRIAGE`).** Body pass → `(target,
verb-class, order-sig)` per hook → round-trip-order-free each coarse group.
PASS-rate = the *real* "how many were accidentally-imperative" number;
FAIL-rate = the order-dependent / foreign tail. Control = Odoo `_compute_*`
(already declarative); test = Rails `before_*`/`after_*`.

**The one prerequisite (gated build).** F17 needs a ruff extension: capture
**writes/calls** per function (today `ruff_ruby_spo` captures `reads`/`raises`/
`traverses` — NOT writes), so "calls `update` on X" is extractable. That is the
single, bounded thing to add — a classifier + a parity gate, not a Ruby
decompiler. (A name-only proxy — classify by method-name shape — is possible
sooner but is weak; the real triage needs the write/call capture.)

> **2026-07-05 verification note (the prerequisite is DONE — the paragraph
> above is stale, kept for the record):** verified in code on the consumed
> branch — `Function::{writes, calls}` exist
> (`ruff_spo_triplet/src/ir.rs:264-284`), the Ruby walker populates them
> (`ruff_ruby_spo/src/functions.rs`, op-assign/memoization excluded, tested),
> and `expand()` emits `writes_field` / `calls` triples with truth values
> (`expand.rs:271/:282`). F17 is unblocked on the fact side. The remaining
> Action-kind discriminant gap is the **`routes.rb` stratum** (HTTP verb,
> member/collection, return shape); the controller DO-arm harvest itself is
> live (`extract_tree_with`, ruff #42/#43 → `lift_actions` → `ActionDef`).
> Cross-ref: OGAR `E-F17-PREREQ-VERIFIED`.
