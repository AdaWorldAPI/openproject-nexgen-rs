# gap-research.md — Extractor/Target gaps blocking the ruff codegen pipeline on OpenProject

Sprint C0 / agent **W1-X3** (analysis). Research input for C0-09
`extraction-gap-proposals.md`. Every claim below is backed by a real
`file:line` citation in `/home/user/ruff-main` (the ruff codegen engine) or
`/home/user/openproject-nexgen-rs` (the target seed).

**Scope reminder.** The ruff pipeline is, end-to-end, a *Python-source →
Rust/axum/SeaORM + askama* transpiler:
`pipeline.rs:39` parses with `parse_module`, `pipeline.rs:47` walks
`Stmt::FunctionDef`, `pipeline.rs:48` calls `detect_route(func)` (Flask/Python
route detection). Its single shipped target is `rust-axum-seaorm`
(`target.rs:6`, `target.rs:84`). OpenProject is a **Ruby/Rails backend +
Angular SPA frontend over a HAL+JSON API**, persisted with **sqlx**, split
across many crates. None of the four pipeline assumptions below hold for that
target.

---

## Gap 1 — SeaORM target vs. sqlx seed

### (a) Evidence

The target's model-path model and **every** DB-touching emitter arm are
hardwired to SeaORM's `Entity`/`Column`/`ActiveModel`/`Set` API:

- `ModelMapping` resolves model types to SeaORM's three-type layout — the
  suffixes `::Model`, `::Entity`, `::Column` are string-concatenated, not
  configurable:
  - `target.rs:33` — `format!("{root}::{}::Model", self.module_path)`
  - `target.rs:37` — `format!("{root}::{}::Entity", self.module_path)`
  - `target.rs:41` — `format!("{root}::{}::Column", self.module_path)`
- The emitters hardcode SeaORM call syntax in their format strings:
  - `mod.rs:213` — `use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};`
  - `mod.rs:237` — `let {collection} = {entity}::find()` then `mod.rs:238` `.filter({column}::{tenant}.eq(...))` (list_for_tenant)
  - `mod.rs:478` — `let mut active: {module}::ActiveModel = row.into();` and `mod.rs:479` `active.aktiv = sea_orm::Set(false);` (soft_delete)
  - `mod.rs:507` / `mod.rs:660` / `mod.rs:806` — `{entity}::find_by_id({primary_param})` (soft_delete / detail_for_tenant / toggle_bool_field)
  - `mod.rs:813`–`mod.rs:814` — `ActiveModel` + `sea_orm::Set(new_value)` (toggle_bool_field)
  - `sea_orm` / `DatabaseConnection` / `ActiveModel` / `EntityTrait` appears **30 times** across `mod.rs` (rg -c on `codegen/mod.rs`).

The OpenProject seed is sqlx end-to-end — there is no `Entity`, no `Column`
enum, no `ActiveModel`:

- `op-db/src/work_packages.rs:8` — `use sqlx::{FromRow, PgPool, Row};`
- `op-db/src/work_packages.rs:13`–`14` — `#[derive(Debug, Clone, FromRow)] pub struct WorkPackageRow`
- `op-db/src/work_packages.rs:94` — `sqlx::query_as::<_, WorkPackageRow>(` over a raw `SELECT … FROM work_packages WHERE …` string (the SeaORM `Entity::find().filter(Column::…)` builder has no analogue here).
- `op-db/src/work_packages.rs:394` — hard delete via `sqlx::query("DELETE FROM work_packages WHERE id = $1")`; OpenProject has **no `aktiv` soft-delete boolean** (the `WorkPackageRow` struct, lines 13–35, has no such column), so `emit_soft_delete`'s `active.aktiv = Set(false)` is meaningless against this schema.
- The domain model `op-models/src/work_package/model.rs:8`–`25` is a plain `serde` struct (`#[serde(rename_all = "camelCase")]`), not a SeaORM `Model`.
- Persistence is sqlx everywhere: `op-db/Cargo.toml:12`, `op-models/Cargo.toml:15`, `op-api/Cargo.toml:16`, `op-server/Cargo.toml:18`, `op-journals/Cargo.toml:14` all declare `sqlx.workspace = true`. The workspace root *declares* `sea-orm` at `Cargo.toml:43`, but **no `op-*` crate depends on it** — it is an unused declaration, confirming the seed deliberately chose sqlx.

### (b) Impact on Sprint C0

Every emitter arm that touches the DB produces code that will not compile
against the seed: `list_for_tenant`, `detail_for_tenant`, `soft_delete`,
`toggle_bool_field`, and the `unresolved`/scope-filter paths
(`mod.rs:464`, `mod.rs:1361`). That is **4 of the 13** `emit_kinds`
(`target.rs:181`–`193`) that are *actively wrong* (emit SeaORM); a further set
(`form_get_post`, `ajax_json`, `download_blob`, `pdf_render`, `sa_admin_view`,
`signed_link_action`, `csrf_form_post`, `template_get`) only *import*
`sea_orm::DatabaseConnection` and thread `State<DatabaseConnection>`
(e.g. `mod.rs:1042`, `mod.rs:1053` for ajax_json) but don't issue queries — they
break on the connection type alone, not query semantics. Net: **0 of 13 arms
compile as-is** against an sqlx + `PgPool` seed.

### (c) Minimal ruff extension (proposal, not implementation)

Make the persistence API a property of the `TargetSpec`, not baked into
`mod.rs`:
1. Add an `orm` enum to `TargetSpec` (`seaorm` | `sqlx`) plus a small
   per-kind "DB recipe" template (find-one / list-by / soft-or-hard-delete /
   field-update) selected by `orm`.
2. Add a `sqlx` recipe set whose `find_by_id` emits
   `sqlx::query_as::<_, {Row}>("SELECT … WHERE id = $1").bind(id).fetch_optional(pool)`
   and whose connection type is `PgPool`, mirroring
   `op-db/src/work_packages.rs:250`–`267`.
3. Replace `ModelMapping::{model_type,entity_path,column_path}`
   (`target.rs:32`–`42`) with an ORM-parameterised resolver: for sqlx there is
   only a `Row` type and a table name, no `Entity`/`Column`.
This is data + one recipe module; the dispatch in `emit()` (`mod.rs:104`–`120`)
stays unchanged.

---

## Gap 2 — Python/Flask extractor vs. Ruby/Rails frontend (C0-01 needs Ruby in, ModelGraph out)

### (a) Evidence

C0-01 wants: accept an OpenProject Rails path and emit a `ModelGraph` for 3
models. The crate that is *supposed* to do this is a scaffold whose every
extraction function is `todo!()`:

- `ruff_ruby_spo/src/lib.rs:1` — doc header literally says
  `//! ... **SCAFFOLD** Ruby/Rails frontend` and `:11` "This crate exists to be
  *finished*, not to work yet."
- `ruff_ruby_spo/src/lib.rs:82`–`87` — `parse_models()` is `todo!("wire a Ruby
  parser (lib-ruby-parser): collect class defs + association declarations …")`.
- `lib.rs:101`–`107` — `extract_fields()` is `todo!()`.
- `lib.rs:124`–`130` — `extract_functions()` is `todo!()`.
- The public entry `extract(source_tree: &Path) -> ModelGraph`
  (`lib.rs:57`–`67`) calls `parse_models` first, so calling it **panics
  immediately**.

What *does* exist: only the IR shape and a hand-built fixture. The downstream
`ModelGraph`/`expand` triple contract is real and tested
(`lib.rs:141`–`204` `locked_work_package_graph` + `locked_shape_expands_to_…`),
and the consumers (`lance_graph` SPO loader, `action_emitter`, `link_chain`)
need zero changes (`lib.rs:23`–`25`). So the gap is *exactly and only* the
Ruby front half. The whole `ruff_ruby_spo` crate is a single 215-line file
(`ls` of `crates/ruff_ruby_spo/src/` shows only `lib.rs`) — there is no parser,
no `schema.rb` reader, no `app/models` walker.

Note: this is a **separate pipeline** from the SeaORM codegen (Gaps 1/3/4,
which is Python→Rust). C0-01's "ModelGraph for 3 models" rides the
`ruff_spo_triplet` IR, not the `RouteContract`/`emit` path.

### (b) Impact on Sprint C0

C0-01 cannot run at all: `extract()` panics on the first `todo!()`. There is no
path today from an OpenProject `app/models/*.rb` + `db/schema.rb` to a
`ModelGraph`. Everything downstream (SPO triples for 3 models) is blocked on
three unimplemented functions, despite the target shape being fully specified
and test-locked.

### (c) Minimal ruff extension (proposal, not implementation)

Implement the three `todo!()`s against a pure-Rust Ruby parser, scoped to 3
models only:
1. `parse_models` (`lib.rs:82`): add `lib-ruby-parser`, find
   `class X < ApplicationRecord`, capture body source range, collect
   `belongs_to`/`has_many`/`has_one`/`has_and_belongs_to_many` symbols into
   `RubyClass.associations` (`lib.rs:46`–`48`); parse `db/schema.rb` once for
   baseline columns.
2. `extract_fields` (`lib.rs:101`): schema columns + `attribute`/`attr_accessor`/
   `store_accessor`; link a derived attr to its computing method
   (`emitted_by`) and that method's read chains (`depends_on`), per the
   doc-comment at `lib.rs:89`–`100`.
3. `extract_functions` (`lib.rs:109`): `def` bodies → `reads` (self/attr
   reads), `raises` (`raise`/`errors.add`/`validates`→`ActiveRecord::RecordInvalid`),
   `traverses` (calls whose receiver is in `associations`).
Validate against the existing locked test (`lib.rs:160`) — no IR or downstream
changes needed.

---

## Gap 3 — Single `models_root` vs. OpenProject's multi-crate split

### (a) Evidence

`TargetSpec` carries exactly **one** model root and **no** per-crate or
handlers-root knob:

- `target.rs:51` — `pub models_root: String` (singular), defaulted to
  `crate::models` at `target.rs:176` / `target.rs:229`.
- The whole `TargetSpec` struct (`target.rs:47`–`71`) has fields
  `id, models_root, models, tenant_column, templates_root, emit_kinds` — there
  is no `handlers_root`, no per-crate path map, no notion of a workspace with
  multiple member crates. The TOML reader confirms the same closed schema
  (`target.rs:264`–`268`).
- Every model path is built as `models_root` + `module_path` + a fixed suffix
  (`target.rs:33`/`37`/`41`), and emitters hardcode `crate::models::…` use
  paths (`mod.rs:456` `format!("crate::models::{}", mapping.module_path)`,
  `mod.rs:499` `use {model_use};`). So all emitted code assumes one crate whose
  models live under one `crate::models` module.

OpenProject splits the same concern across **four** crates with different
roles:

- `op-models` — domain/serde structs (`op-models/src/work_package/model.rs:10`
  `pub struct WorkPackage`).
- `op-db` — sqlx rows + repositories (`op-db/src/work_packages.rs:14`
  `WorkPackageRow`, `:79` `WorkPackageRepository`).
- `op-api` — axum handlers + HAL representers (`op-api/src/handlers/` and
  `op-api/src/representers/`).
- `op-contracts` — validation contracts (`op-contracts/src/{work_packages,projects,users}`).

The directory listing of `crates/` shows 16 `op-*` crates; a single
`models_root` string cannot address `op_models::work_package::WorkPackage` for
the domain type **and** `op_db::work_packages::WorkPackageRow` for the row
**and** an `op_api` handler module **and** an `op_contracts` validator — they
are different crates with different type names for the same entity.

### (b) Impact on Sprint C0

Even after fixing the ORM (Gap 1), the emitter writes everything under one
`crate::models::…` path and one output `handlers/` dir
(`pipeline.rs:113`–`117`, `pipeline.rs:157`–`160`). It cannot place the row
type in `op-db`, the DTO in `op-models`, the handler in `op-api`, and the
validator in `op-contracts`. The single-root assumption mis-locates every
generated symbol, so generated `use` paths won't resolve in the real
workspace.

### (c) Minimal ruff extension (proposal, not implementation)

Generalise the spec's path model from one string to a small role→path map:
1. Replace `models_root: String` (`target.rs:51`) with a
   `roots: BTreeMap<Role, String>` where `Role ∈ {domain, row, handler,
   contract}` (e.g. `op_models`, `op_db::…`, `op_api::handlers`,
   `op_contracts`).
2. Add an optional per-`ModelMapping` override so a class can name its
   `row_path` / `domain_path` independently (OpenProject's `WorkPackage`
   vs. `WorkPackageRow`).
3. Have the emitters and `pipeline.rs` write each artifact under the matching
   role's output dir. The `emit_kinds` dispatch is untouched; this is purely
   path resolution.
(Lower coverage payoff than Gaps 1/4 — it only matters once code already
compiles, and is a refactor of `ModelMapping` + `TargetSpec` rather than added
recipes.)

---

## Gap 4 — askama/jinja template + column emitters vs. ERB + Angular SPA over HAL JSON

### (a) Evidence

The view side of the engine is jinja-specific in and out:

- `jinja.rs:1` — `//! jinja → askama translation, ported from
  woa-rs/tools/render_routes.py`. The whole module rewrites jinja `{{ }}`/`{% %}`
  into askama (`translate_cell_expr` `jinja.rs:51`, emitting
  `{% if let Some(v) = … %}` askama syntax at `jinja.rs:78`).
- `columns.rs:1`–`9` — `//! jinja table-column extraction …`; `extract_table_shape`
  (`columns.rs:48`) scans `<table>…{% for %}…</table>` jinja blocks for `<th>`/`<td>`
  cells. It keys entirely off jinja for-loops (`columns.rs:80` `block.contains("{% for")`).
- `templates_root` in the spec (`target.rs:64`–`65`) points at "source jinja
  templates"; the list/detail/sa_admin emitters call
  `emit_table_view`/`emit_skeleton_view` (`mod.rs:314`–`315`, `:688`–`689`,
  `:1255`–`1256`) to produce askama `.html` views.

OpenProject has **no jinja, no askama, and (in this Rust port) no server-rendered
templates at all**:

- There is **no template engine** in the workspace: `rg askama|tera|handlebars|minijinja|liquid Cargo.toml` → "NO template engine in workspace Cargo.toml"; `op-api`/`op-server` Cargo.tomls pull none.
- There are **no `.erb` files** in the seed (`rg --files -g '*.erb'` → none) — the real OpenProject ERB views live in the Rails app, not this Rust target.
- The frontend contract is **HAL+JSON for an Angular SPA**, not HTML:
  `op-api/src/lib.rs` — "This crate implements the [HAL]+JSON API matching
  OpenProject's API v3"; `op-api/src/representers/hal.rs:1` HAL representers;
  `hal.rs:151` `pub struct HalResource<T>` with `#[serde(rename = "_type")]`
  (`hal.rs:152`), `_links` (`hal.rs:156`), `_embedded` (`hal.rs:158`).
- Real handlers return JSON/HAL, never a template:
  `op-api/src/handlers/work_packages.rs:17`–`21` `list_work_packages(... State<AppState>) -> ApiResult<impl IntoResponse>`;
  `:70` `get_work_package`; create/update take `Json<…Dto>` (`:109`, `:167`).
  So the dominant emitter for this target is **`ajax_json`**
  (`mod.rs:1018`), not the table/column path.

### (b) Impact on Sprint C0

The entire view pipeline — `columns.rs` (table extraction),
`jinja.rs` (cell translation), `emit_table_view`/`emit_skeleton_view`, and the
`templates_root` config — is **dead weight** for OpenProject: there are no
jinja templates to read and no askama views to emit. The `view_html` half of
`Emitted` (`pipeline.rs:162`–`165`) never applies. Worse, the JSON arm that
*does* apply, `emit_ajax_json` (`mod.rs:1018`), emits a flat
`#[derive(Serialize)]` response struct with `serde_json::Value` fields
(`mod.rs:1032`–`1035`) and a `sea_orm::DatabaseConnection`
(`mod.rs:1042`/`1053`) — it has **no concept of HAL** `_type`/`_links`/`_embedded`
wrapping (`hal.rs:152`–`158`), so even the relevant arm doesn't match the
target's response envelope.

### (c) Minimal ruff extension (proposal, not implementation)

Treat ERB/SPA-HAL as a target whose view layer is "none, JSON-only", and teach
the JSON arm about HAL:
1. Add a spec flag `views: none|askama` (default `askama`); when `none`, skip
   `emit_*_view` and stop emitting `templates_root` plumbing — `columns.rs`/
   `jinja.rs` simply aren't invoked (no code change to them, just not called).
2. Extend `emit_ajax_json` (`mod.rs:1018`) with a `response_envelope:
   plain|hal` option; the `hal` recipe wraps the response in
   `HalResource<T>` with `_type`/`_links`/`_embedded`, mirroring
   `op-api/src/representers/hal.rs:151`–`158`.
3. ERB column extraction is **not needed** for the Rust target (no server-side
   HTML); if OpenProject's Rails ERB ever needs reading, that is a *separate*
   ERB extractor analogous to `columns.rs`, out of scope for compiling the
   axum/HAL target.

---

## Summary — one sentence + single most damning citation per gap

1. **SeaORM-vs-sqlx**: every DB emitter hardcodes SeaORM's `ActiveModel`/`Set`
   API while the seed is raw sqlx, so generated handlers can't compile — most
   damning: `mod.rs:479` `active.aktiv = sea_orm::Set(false);` vs.
   `op-db/src/work_packages.rs:8` `use sqlx::{FromRow, PgPool, Row};`.
2. **Ruby/Rails extractor**: the crate that must turn a Rails path into a
   `ModelGraph` is an all-`todo!()` scaffold that panics on first call — most
   damning: `ruff_ruby_spo/src/lib.rs:83` `todo!("wire a Ruby parser …")`.
3. **single models_root vs multi-crate**: the target schema has one
   `models_root` string and no per-crate/handlers root, but OpenProject splits
   the same entity across `op-models`/`op-db`/`op-api`/`op-contracts` — most
   damning: `target.rs:51` `pub models_root: String`.
4. **ERB-vs-jinja**: the view emitters are jinja→askama only, while the target
   is an Angular SPA over HAL+JSON with no template engine — most damning:
   `jinja.rs:1` `//! jinja → askama translation` vs.
   `op-api/src/representers/hal.rs:151` `pub struct HalResource<T>`.

## Ranking — cheapest-to-close × highest coverage payoff

1. **Gap 1 (SeaORM→sqlx) — DO FIRST.** Highest payoff: it gates **all 13**
   `emit_kinds` (`target.rs:181`–`193`); nothing the engine emits compiles
   against the seed until persistence is sqlx. Cost is moderate and *additive*
   — a new sqlx recipe set + an `orm` switch, with the `emit()` dispatch
   (`mod.rs:104`) untouched. Best effort:coverage ratio.
2. **Gap 4 (HAL/JSON, drop templates) — DO SECOND.** Cheapest absolute change:
   a `views: none` flag stops invoking `columns.rs`/`jinja.rs` (delete nothing),
   plus a `hal` envelope option on the single `emit_ajax_json`
   (`mod.rs:1018`). High payoff because `ajax_json` is the dominant — arguably
   only — relevant arm for an SPA+HAL target.
3. **Gap 2 (Ruby frontend) — DO THIRD, INDEPENDENT.** High payoff but it serves
   the *separate* SPO-triplet pipeline (C0-01), not the axum codegen. Cost is a
   new parser dependency + 3 function bodies (`lib.rs:83`/`102`/`125`); the IR
   and downstream are already done and test-locked, so it's well-bounded but
   not shared with Gaps 1/3/4.
4. **Gap 3 (multi-crate roots) — DO LAST.** Lowest marginal payoff: it only
   matters *after* code compiles (post-Gap-1) and is a refactor of
   `ModelMapping`/`TargetSpec` (`target.rs:23`–`51`) rather than added
   coverage. Defer until artifacts need to land in the right crates.
