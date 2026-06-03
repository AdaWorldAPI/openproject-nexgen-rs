# C17 — Evaluation: properties, classes, routes, duplicate routes (`ruff_python_dto_check` + codegen)

Companion to `c17-scanner-coverage-probes.md`. After the C17a parser
graduation (this PR), the scanner can SEE Rails option semantics. This doc
surveys the downstream consumers (`ruff_python_dto_check`, the "ruff dto
crate" + ruff codegen crate) in upstream `AdaWorldAPI/ruff` and lists what
must extend for Rails, plus the nested-JSON/TOML configuration edge cases
those extensions surface.

Source-of-truth read: `/home/user/ruff-clone/crates/ruff_python_dto_check/`
(the upstream that the vendor mirror in nexgen tracks).

## Upstream module inventory (`ruff_python_dto_check/src/`)

| Module | LOC | Role | Rails-port status |
| --- | ---: | --- | --- |
| `bundle.rs` | — | Source + decorator bundle collection | needs port — Rails has `Bundle = controller + actions` not `function + decorator` |
| `calibrate.rs` | — | Calibration over a corpus | reusable shape, language-agnostic |
| `codegen/` | — | Emit Rust handler/repo source | reusable; sqlx_emit + ajax_json + csrf_form_post kinds already mirrored |
| `codegen/target.rs` | 425 | `TargetSpec` + `ModelMapping` (the nested config struct) | needs Rails edge-case fields (STI, polymorphic, concerns, engines) |
| `config.rs` | — | Config loading | reusable |
| `contract.rs` | 451 | `RouteContract` + `HandlerKind` (14-variant emergent taxonomy) | needs Rails-side classifier; HandlerKind taxonomy is emergent so a Rails pass produces its own |
| `emit.rs` | — | Emit dispatcher | reusable |
| `extractors/routes.rs` | 111 | Detect Flask `@bp.route("/path", methods=[…])` decorators | **NEEDS REPLACEMENT** — Rails routes are not decorator-side |
| `extractors/decorators.rs` | — | Python decorator scan | not relevant (Rails has filters not decorators) |
| `extractors/body.rs` | — | Python function-body fact extraction | needs Rails-action equivalent |
| `matcher/function_with_decorator.rs` | 201 | Match handler function ↔ decorator | needs Rails-action ↔ route equivalent |
| `observations.rs` | — | Reporting | reusable |
| `preflight/scanner.rs` | 402 | Single-pass corpus collector (file counts, decorator histograms, URL templates, blueprint graph, candidate misses) | needs Rails-side counterpart (controller class histograms, before_action chains, route DSL keywords) |

## Routes (Flask vs Rails)

**Upstream Flask model** (`extractors/routes.rs`):

```rust
pub struct RouteInfo {
    pub blueprint: String,
    pub function: String,
    pub path: String,
    pub methods: Vec<String>,
    pub line_start: u32,
    pub line_end: u32,
}

pub fn detect_route(func: &StmtFunctionDef) -> Option<RouteInfo>
```

Routes live as `@blueprint.route("/path", methods=[…])` decorators on
Python `FunctionDef` nodes. One source file = one (blueprint, function set).

**Rails model** (what we need):

```ruby
# config/routes.rb — DSL, not class-attached
Rails.application.routes.draw do
  resources :work_packages              # ← expands to 7 routes
  get '/work_packages/:id/download', to: 'work_packages#download'
  namespace :api do
    namespace :v3 do
      resources :work_packages          # ← another 7 routes, prefixed
    end
  end
end
```

Differences with semantic weight:

| Aspect | Flask | Rails |
| --- | --- | --- |
| Source of route definition | decorator on a handler function | central `config/routes.rb` DSL |
| Path → action binding | name of the decorated function | `controller#action` reference (decoupled file) |
| Method enumeration | `methods=["POST", …]` keyword | per-DSL-keyword (`get`, `post`, `put`, `delete`, `patch`, `match`) |
| Path resource expansion | none (paths are literal) | `resources :x` expands to 7 RESTful routes |
| Namespacing | blueprint prefix | `namespace :api do … end` block prefix |
| Engine routes | not applicable | engines mount whole sub-route DSLs |

**Rails-side `RouteInfo` will need**: `verb`, `path`, `controller`, `action`,
`route_helper_name` (Rails generates `work_packages_url` etc.), `namespace`,
`engine` (Option<String>), `from_resources` (Option<String>). The
controller#action target is decoupled from the routes.rb file — the
extractor must follow the `controller#action` pair to `app/controllers/
<controller>_controller.rb` to read the action's body for classification.

## Duplicate routes

**Upstream**: there is **no explicit duplicate-route detector** in
`preflight/scanner.rs` today. The closest is `url_template_segments`
(BTreeMap<String, count>) — a histogram of URL template parts which surfaces
high-frequency segments but doesn't classify dupes. Per-route counts and
collisions would be a follow-up addition.

For Rails the duplicate detection is **necessary** and well-defined:

| Duplicate kind | What it means | Detection rule |
| --- | --- | --- |
| Exact dupe | Same `(verb, path)` declared twice | sort by (verb, path), find equal-key runs |
| Resource overlap | `resources :x` + manual `get '/x/:id'` for the same id | enumerate `resources :x` to its 7 expansions, intersect with manual entries |
| Engine collision | App declares `/work_packages/:id`; mounted engine also declares it | accumulate engine routes under their `mount` path, then compare against app routes |
| Action ambiguity | Two different controllers route to the same path/verb | report `(path, verb) → [controller#action, …]` if len > 1 |
| Shadow route | Earlier `match '/*path'` catch-all swallows a later specific route | linear scan with prefix-shadowing rule |

The duplicate detector belongs alongside the route extractor as a separate
pass that ingests `Vec<RouteInfo>` and emits `Vec<DuplicateFinding>`. No
config schema change needed for the detector itself — it's a derived
analysis over the extracted routes.

## HandlerKind taxonomy (Rails extensions)

Upstream `HandlerKind` (`contract.rs`) is a 14-variant enum derived
empirically from the python codebase (`SignedLinkAction`, `SaAdminView`,
`AjaxJson`, `PdfRender`, `DownloadBlob`, `SoftDelete`, `ToggleBoolField`,
`CsrfFormPostEngineCall`, `GetRedirectShortcut`, `FormGetPost`,
`ListForTenant`, `DetailForTenant`, `TemplateGet`, `Other`).

The taxonomy is **emergent** — running the classifier against a different
codebase produces a different distribution. For Rails (OpenProject in
particular) the equivalent classifier needs cues from Rails controllers:

| Rails action shape | Likely `HandlerKind` | Cue |
| --- | --- | --- |
| `render json: …` + 200 | `ajax_json` | `respond_to :json` block / explicit `render json:` |
| `redirect_to … `, no render | `get_redirect_shortcut` | `redirect_to` is the only output |
| `send_data` / `send_file` | `download_blob` | `send_data` / `send_file` calls |
| `respond_to :pdf` | `pdf_render` | format spec |
| update_attribute(:bool_field, …) | `toggle_bool_field` | single-column boolean toggle in action body |
| update_attribute(:deleted_at, Time.now) | `soft_delete` | (matches the Sprint-C1-gap-1 sqlx_emit kind) |
| `@records = Model.for_project(project).page` + render :index | `list_for_tenant` | tenant filter + paged collection (incl. v3 `_embedded.elements`) |
| `@record = Model.find(params[:id])` + render :show | `detail_for_tenant` | single-record fetch + permission check |
| `before_action :authorize` chain, no body | `csrf_form_post_engine_call` | thin POST handler that delegates to an engine call |
| `render :template_name` only | `template_get` | simple template render |

The classifier is then `classify_action(action_body, &before_actions, &
respond_to_blocks) -> HandlerKind`. The Python classifier's `output ×
inputs` algebra still applies, the cues differ.

## Nested config — `TargetSpec` + `ModelMapping` edge cases for Rails

Upstream shape:

```rust
pub struct TargetSpec {
    pub id: String,
    pub models_root: String,
    pub models: BTreeMap<String, ModelMapping>,
    pub tenant_column: String,           // default "TenantId"
    pub templates_root: Option<String>,
    pub emit_kinds: Vec<String>,
    pub orm: Orm,
}

pub struct ModelMapping {
    pub module_path: String,             // "work_package" or "erp::k6_cash::cash_journal"
}
```

OpenProject seed already overrides `tenant_column = "ProjectId"`. But
Rails models surface configuration needs the current shape **cannot
express**:

| Edge case | Rails source | TargetSpec gap | Proposed nested-config addition |
| --- | --- | --- | --- |
| STI hierarchy | `class Group < Principal` (Group shares Principal's `users` table) | no `sti_parent` field on `ModelMapping` | `sti_parent: Option<String>` + `sti_discriminator_value: Option<String>` (the `type` column value) |
| Polymorphic belongs_to | `belongs_to :journable, polymorphic: true` | no polymorphic table-name enumeration | `polymorphic_targets: Option<Vec<String>>` (e.g. `["WorkPackage", "Project", "Wiki"]`) so codegen can `match j.journable_type { … }` |
| Concerns adding fields | `include WorkPackage::SemanticIdentifier` adds `display_id` virtual | no `concerns` list per model | `concerns: Vec<String>` referencing the concern module path under `app/models/concerns/` |
| Engine namespacing | BIM module owns `IfcModel`, OAuth has `OauthApplication` | `module_path` is flat | `engine: Option<String>` (e.g. `"bim"`) for routes-side `mount BIM::Engine, at: …` linkage |
| has_many :through chain | `has_many :work_package_changes, through: :work_packages, source: :journals` | not modelled | `through_chain: Vec<RelationStep>` for the codegen-side eager-loading or join planning |
| Enum + store_accessor pseudo-fields | `enum :status, { active: 1, … }`, `store_accessor :cause, %i[type feature …], prefix: true` | no field-level metadata | per-`ModelMapping`: `enums: Vec<EnumSpec>` + `store_accessors: Vec<StoreAccessorSpec>` |
| Custom `self.table_name = …` | `Principal.table_name = "users"` | inferred from class name | `table_name_override: Option<String>` |
| Route → action binding | `resources :work_packages` expands to 7 routes hitting WorkPackagesController | `models` only knows model names | new top-level `routes: BTreeMap<String, RouteSpec>` that maps `WorkPackagesController#index` → ModelMapping ref |
| Engine route prefixes | `mount BIM::Engine, at: '/bim'` | not modelled | `engines: BTreeMap<String, EngineSpec>` with `mount_path` |

The Rails extension is **additive** — old python-side TOMLs stay valid
(every new field has a `#[serde(default)]` empty value). The new fields
appear in `openproject-axum-sqlx.toml` only when the project needs them.

## Suggested PR sequence (post-C17a)

PR sequence in priority order — each is reviewable independently and
unblocks the next:

1. **C17b — `RubyClass` extension**: `concerns`, `enums`, `store_accessors`,
   `inheritance_column_disabled`, `table_name_override`. Populated from
   the AST in `parse.rs`. Tests on real OP fixtures.
2. **C17c — `ruff_ruby_spo::controllers`**: new module that parses
   `app/controllers/*.rb` with the same AST machinery (`Parser::do_parse`),
   extracts actions (top-level `def`s on controllers), before_action
   chains, render/redirect/send_data calls. New `RubyController` shape
   parallel to `RubyClass`.
3. **C17d — `ruff_ruby_spo::routes`**: parse `config/routes.rb` for the
   Rails DSL (`resources`, `get`, `post`, `namespace`, `mount`). Expand
   `resources` to 7 RESTful routes. Returns `Vec<RubyRoute>` (verb, path,
   controller, action, engine).
4. **C17e — duplicate-route detector**: `find_duplicates(routes:
   &[RubyRoute]) -> Vec<DuplicateFinding>`. The 5 categories above. Tests
   on a hand-built `routes.rb` covering each kind, plus a real run on OP.
5. **C17f — Rails HandlerKind classifier**: port the upstream
   `classify_kind` to Rails action shape. Probably wires through
   `extractors/body.rs`-equivalent.
6. **C17g — `TargetSpec` Rails extensions**: add the 9 fields above with
   `#[serde(default)]`. Update `openproject-axum-sqlx.toml` example to
   exercise STI + polymorphic + concerns + engines on real OP models.
7. **C17h — Vendor mirror sync**: re-vendor the new upstream modules
   (preflight Rails-side, contract Rails-side) so nexgen reviewers see
   the full implementations alongside the patch-file form.

Each step alone improves coverage; the sequence drives toward full Rails
parity with the python-side handler-emit pipeline.

## What this PR (C17a) closes from the coverage probe

The C17a parser graduation closes the following gap codes from
`c17-scanner-coverage-probes.md`:

| Code | Gap kind | Closed how |
| --- | --- | --- |
| G1 | Macro-option blindness | `AssociationDecl` captures all 9 option fields |
| G2 | `polymorphic: true` belongs_to side | `AssociationDecl::polymorphic` |
| G3 | `as: :name` reverse-side polymorphism | `AssociationDecl::as_target` |
| G4 | `through:` flattened | `AssociationDecl::through` |
| G5 | `source:` aliasing | `AssociationDecl::source` |
| G6 | `class_name:` `::`-namespaced | `AssociationDecl::class_name`, full string verbatim |
| G7 (partial) | STI hierarchy semantics — parent capture | `RubyClass::superclass`. **Hierarchy assembly** still pending C17b. |

Still unclosed (next PRs): G8 (enums), G9 (store_accessor), G10
(attribute), G11–G13 (class-meta), G14 (concerns), G15 (scope lambdas),
G16 (scopes), G17 (default_scope), G18 (DSL macros), G19 (callbacks),
G20 (collection callbacks), G21 (constants).

Coverage delta — **6 of 21 universal gaps closed in C17a**, with the
parser foundation in place to close the remaining ~12 structural gaps
(G8–G14, G19, G20) by reading more AST node types. Behavior gaps
(G15, G18) wait on either deeper AST traversal or a more sophisticated
classifier.
