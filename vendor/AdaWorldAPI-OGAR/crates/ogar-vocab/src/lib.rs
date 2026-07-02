//! `ogar-vocab` — the canonical Rust types for the OGAR vocabulary.
//!
//! OGAR is the language-independent Active Record pattern as a graph
//! ontology. These types are the **IR** that producers (Ruby AR via
//! `ruff_ruby_spo`, Python Odoo via `ogar-python`, SQL DDL via
//! `ogar-sql-ddl`, …) emit and consumers (lance-graph triple loader,
//! `ogar-to-postgres`, `ogar-to-surrealql`, …) read.
//!
//! See [`Class`] for the entry-point shape. The types deliberately mirror
//! the C17a–c stable shape in `ruff_ruby_spo` so the existing producer can
//! be lifted in-place; the only change is stripping the Ruby-specific
//! framing (`body_source` becomes opaque `source` with a `language`
//! discriminant on the parent class).
//!
//! # Layer position
//!
//! ```text
//!   source AST  ──▶  ogar-vocab::Class  ──▶  ogar-ontology  ──▶  lance-graph triples
//!   (Ruby/Py/  )      (this crate)            (prefix         (Arrow/Lance SoA)
//!    SQL/TS    )                              routing)
//! ```

#![warn(missing_docs)]
#![forbid(unsafe_code)]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Source language hint — discriminates the producer for traceability
/// and for emitter dispatch on Ruby/Python-specific extension shapes
/// (e.g. Odoo `ComputedField`). Not a hard schema discriminator: a class
/// is fully described by the canonical fields below regardless of
/// `language`.
///
/// **Vocabulary versioning:** `#[non_exhaustive]` so adding a new
/// language (e.g. `Elixir`) is non-breaking. Match expressions in
/// consumer code must include a `_ =>` arm. This applies to every
/// `pub enum` / `pub struct` in this module: the OGAR vocabulary is
/// expected to evolve over time, and every base type is forward-
/// compatible-by-construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum Language {
    /// Ruby ActiveRecord (`class Foo < ApplicationRecord`).
    #[default]
    Ruby,
    /// Python — covers Django ORM and Odoo `models.Model`.
    Python,
    /// SQL DDL (`CREATE TABLE …`).
    Sql,
    /// TypeScript — covers Prisma, TypeORM, Drizzle.
    TypeScript,
    /// SurrealQL DDL (`DEFINE TABLE …`).
    SurrealQl,
    /// Elixir — Ecto schemas (`schema "t" do …`), Phoenix contexts, and
    /// OTP behaviours (`GenServer` / `gen_statem`). **First-class for
    /// migration**: the OLD HIRO/Bardioc stack is Elixir, so it is the
    /// source of every byte of migration debt and the bridge to the old
    /// adapters. `gen_statem` lifecycles lower onto the same `Action`
    /// state machine as every other producer (see `docs/ELIXIR-HIRO-PREFETCH.md`).
    Elixir,
    /// Unknown or hand-authored.
    Unknown,
}

/// The canonical OGAR class — a single AR-shaped record-class declaration
/// lifted from its source language into the language-independent vocabulary.
///
/// Fields are grouped by C17 sprint of origin in the `ruff_ruby_spo` lift:
/// - **C17a** core: [`name`](Self::name), [`parent`](Self::parent),
///   [`associations`](Self::associations).
/// - **C17b** schema-extensions: [`enums`](Self::enums),
///   [`store_accessors`](Self::store_accessors),
///   [`attributes`](Self::attributes),
///   [`mixins`](Self::mixins), [`table_name`](Self::table_name),
///   [`inheritance_column_disabled`](Self::inheritance_column_disabled).
/// - **C17c** runtime-shape: [`ignored_columns`](Self::ignored_columns),
///   [`scopes`](Self::scopes),
///   [`scope_predeclarations`](Self::scope_predeclarations),
///   [`default_scope`](Self::default_scope), [`callbacks`](Self::callbacks).
///
/// Per-language extensions (Odoo `compute`, `_inherits` delegation,
/// workflow state machines) are not on this base type — they live in
/// `ogar-extensions/*` crates so the core IR stays canonical.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Class {
    /// Class name as written in the source. For dotted-name ORMs
    /// (Odoo `account.move`) the dots are preserved; the prefix-radix
    /// routing in `ogar-ontology` handles the dotted segments.
    pub name: String,
    /// Superclass name as written, when one is declared. Used by
    /// consumers to assemble single-table-inheritance hierarchies.
    pub parent: Option<String>,
    /// Agnostic inheritance slot — metabolizes the three things Rails
    /// conflates (STI parent / abstract base / STI root) into one typed
    /// value. Mixins / concerns are a SEPARATE axis ([`Self::mixins`]) and
    /// are never folded in here. `parent` / `abstract_model` /
    /// `inheritance_column_disabled` are retained for one migration cycle;
    /// new consumers should read `inheritance`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub inheritance: Inheritance,
    /// Source language of the producer that emitted this class.
    pub language: Language,
    /// `belongs_to` / `has_one` / `has_many` / `has_and_belongs_to_many`
    /// declarations in source order.
    pub associations: Vec<Association>,
    /// `include Mixin` / `_inherit = 'mixin.thread'` mixin paths in
    /// declaration order. Dotted names preserved verbatim.
    pub mixins: Vec<String>,
    /// `enum status: { ... }` / `fields.Selection([...])` enum-backed
    /// columns in declaration order.
    pub enums: Vec<EnumDecl>,
    /// `store_accessor :col, %i[a b c]` JSONB pseudo-field bundles in
    /// declaration order. Rails-only today; Python equivalents (Odoo
    /// `fields.Json` with derived properties) lift here too.
    pub store_accessors: Vec<StoreAccessor>,
    /// `attribute :name, :type` typed-attribute overrides in
    /// declaration order.
    pub attributes: Vec<Attribute>,
    /// `self.table_name = "..."` literal-string override. `None` when
    /// the consumer should infer the table name (the common case).
    pub table_name: Option<String>,
    /// `self.inheritance_column = :_type_disabled` was set. Signals
    /// the class deliberately opts out of STI dispatch even with
    /// subclasses present.
    pub inheritance_column_disabled: bool,
    /// `self.ignored_columns += [...]` runtime blacklist columns in
    /// source order across however many `+=` statements appear.
    pub ignored_columns: Vec<String>,
    /// `scope :name, -> { body }` definitions in source order.
    pub scopes: Vec<Scope>,
    /// `scopes :a, :b, :c` declarative-list scope-name predeclarations
    /// — a DSL form that pre-declares scope class-methods defined in
    /// mixins elsewhere.
    pub scope_predeclarations: Vec<String>,
    /// `default_scope -> { body }` global filter body, when present.
    pub default_scope: Option<String>,
    /// Lifecycle callback declarations in source order.
    pub callbacks: Vec<Callback>,
    /// Validation declarations in source order (`validates :col, ...`,
    /// `@api.constrains('col')`).
    pub validations: Vec<Validation>,

    // ─────────────────────────────────────────────────────────────
    // Odoo-shaped fields (also populated by Rails/Django where
    // sensible). See `docs/ODOO-TRANSCODING.md` §7.
    // ─────────────────────────────────────────────────────────────
    /// `_description = 'Sale Order'` (Odoo) — human-readable name.
    /// Rails has no direct equivalent (class comment usually).
    pub description: Option<String>,
    /// `_order = 'date desc, id'` (Odoo) — default record ordering.
    /// Distinct from `default_scope` (Rails) which is a full where
    /// clause; `record_order` is just the ORDER BY tail.
    pub record_order: Option<String>,
    /// `_rec_name = 'name'` (Odoo) — UI display field. Defaults to
    /// `'name'` if unset (Odoo convention).
    pub rec_name: Option<String>,
    /// `_check_company_auto = True` (Odoo) — auto multi-company
    /// check on FK targets.
    pub check_company_auto: Option<bool>,
    /// `_log_access = False` (Odoo) — skip create_uid / write_uid
    /// audit columns.
    pub log_access: Option<bool>,
    /// `_auto = False` (Odoo) — no auto CREATE TABLE (SQL view
    /// models like `account.invoice.report`).
    pub auto_create_table: Option<bool>,
    /// `_abstract = True` (Odoo) — base class, no table. Methods
    /// inherited but data not stored.
    pub abstract_model: bool,
    /// `_transient = True` (Odoo) — wizard/scratchpad model with
    /// vacuumed rows.
    pub transient: bool,
    /// `_register = False` (Odoo) — skip from registry (rare;
    /// usually only base classes).
    pub register: Option<bool>,
    /// Module name from `__manifest__.py` (`'sale'`, `'account'`).
    /// Required for Odoo classes (every class lives in one module);
    /// optional for Rails (engines / gems are the closest concept).
    /// Emitted as `ogar:declaredIn <module>` triple — see BO2 #3.
    pub declared_in_module: Option<String>,
    /// Source language major version (`"17.0"`, `"7.2"`, ...) for
    /// multi-version source compatibility. Reserved; v1 leaves `None`.
    pub source_version: Option<String>,
    /// Curator **domain** — the kind of system this class was harvested
    /// from: `"project"` (OpenProject / Redmine), `"erp"` (Odoo / SAP), …
    /// A coarse, curator-agnostic tag (NOT the namespace or module) and a
    /// component of the `ClassFingerprint` used to mint a stable `ClassId`.
    /// Set by the frontend from the harvest namespace; `None` when
    /// unrecognized.
    #[cfg_attr(feature = "serde", serde(default))]
    pub source_domain: Option<String>,
    /// Source **curator** — the *specific* product this class was
    /// harvested from (`"openproject"`, `"redmine"`, `"odoo"`,
    /// `"osb"`, …), as opposed to the coarse [`source_domain`](Self::source_domain)
    /// (`"project"` / `"erp"`). Two curators in the same domain (Redmine
    /// and OpenProject are both `project`) are distinguished here. Set by
    /// the frontend from the harvest namespace (`ModelGraph::namespace`);
    /// `None` when the frontend didn't tag one.
    #[cfg_attr(feature = "serde", serde(default))]
    pub source_curator: Option<String>,
    /// The class's canonical **concept** — its normalized identity
    /// ([`canonical_concept`]); the key cross-domain convergence bridges
    /// on. Most names normalize lexically (`User` → `user`); proven
    /// cross-domain invariants resolve to a promoted concept (OpenProject
    /// `TimeEntry` and Odoo `account.analytic.line` both →
    /// `billable_work_entry`, the [`billable_work_entry`] canonical class).
    /// Set by the frontend at lift time.
    #[cfg_attr(feature = "serde", serde(default))]
    pub canonical_concept: Option<String>,
    /// Computed-field declarations (Odoo `compute=...` fields, also
    /// Rails / Django where producers can detect them). Lives in
    /// base vocab — see `docs/ODOO-TRANSCODING.md` §8.
    pub computed_fields: Vec<ComputedField>,
    /// CRUD overrides and other method declarations (Odoo
    /// `def create / write / unlink / copy` overrides, Rails
    /// `def self.method`, etc.). Distinct from `callbacks` which
    /// are declarative hooks.
    pub methods: Vec<MethodDecl>,
}

/// How a class sits in its inheritance lattice — the agnostic
/// metabolization of the three things Rails conflates: STI parent,
/// abstract base, and STI root. Mixins / concerns are a SEPARATE axis
/// ([`Class::mixins`]) and are never folded in here.
///
/// Producer IR carries parent/root as **names** (`String`); the registry
/// mints the `ClassId` later. Cross-curator mapping: Rails `< Parent` /
/// `self.abstract_class` / `self.inheritance_column`; Odoo `_inherit` /
/// `_abstract`; Django abstract base classes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum Inheritance {
    /// No superclass beyond the ORM root (Rails `< ApplicationRecord`).
    #[default]
    Root,
    /// Concrete STI child of `parent` (shares the parent's table).
    Concrete {
        /// Parent class name as written.
        parent: String,
    },
    /// Abstract base — methods / fields inherited, but no table of its own
    /// (Rails `self.abstract_class = true`; Odoo `_abstract = True`).
    Abstract,
    /// Root of an STI hierarchy — defines the discriminator column but is
    /// not itself a child. `root` is this class's own name.
    RootedAt {
        /// The hierarchy root class name (this class).
        root: String,
    },
}

/// A computed-field declaration. Carries the field name, the compute
/// method's symbol, and the dependency list from `@api.depends`.
/// Universal across ORMs: Odoo `compute='_compute_x'` + `@api.depends`,
/// Django `cached_property`, Rails instance-method-derived attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct ComputedField {
    /// The field being computed.
    pub field: String,
    /// Compute method name (`"_compute_total"`).
    pub compute_method: String,
    /// Dependency paths from `@api.depends('partner_id',
    /// 'order_line.price_total')`. Empty if no `@api.depends`.
    pub depends: Vec<String>,
    /// `@api.depends_context('uid', 'company')` — env-context
    /// dependencies (Odoo only).
    pub depends_context: Vec<String>,
    /// `store=True` — store result in DB column. `False` recomputes
    /// on every read.
    pub stored: bool,
    /// `inverse='_inverse_total'` — write-direction helper
    /// (turning back from computed value to raw field assignments).
    pub inverse_method: Option<String>,
    /// `search='_search_total'` — search helper for filtering by
    /// computed value.
    pub search_method: Option<String>,
}

/// A method declaration: CRUD override (`def create`/`def write`/
/// `def unlink`/`def copy`), `@api.model` helper, or plain instance
/// method. Distinct from `Callback` which is a declarative hook.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct MethodDecl {
    /// Method name as written.
    pub name: String,
    /// The kind of method — distinguishes total overrides from
    /// declared hooks.
    pub kind: MethodKind,
    /// Method body verbatim. Consumers re-parse or emit opaque.
    pub body_source: String,
    /// Decorator names as written: `["api.depends", "api.constrains"]`.
    pub decorators: Vec<String>,
    /// Recordset binding semantics — does the method bind to a
    /// single record, a recordset, or class-level?
    pub semantics: RecordSemantics,
}

/// Method kind — distinguishes overrides from helpers from plain
/// methods. The producer determines kind from decorator + name
/// inspection; see `docs/ODOO-TRANSCODING.md` §13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum MethodKind {
    /// `def create(self, vals_list)` — total override of an ORM
    /// CRUD method. Semantically distinct from Rails callbacks.
    CrudOverride,
    /// `@api.model def helper(self, ...)` — class-method-like.
    ApiModel,
    /// `@api.model_create_multi def create(self, vals_list)` —
    /// Odoo's bulk-create override.
    ApiModelCreateMulti,
    /// Plain instance method, no special semantics.
    #[default]
    Instance,
}

/// Recordset semantics — Odoo methods can bind to a record (single),
/// a recordset (the default for most methods), or be class-level
/// (`@api.model`). Captured for cross-language consumers that
/// project to per-record vs per-collection APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum RecordSemantics {
    /// Single-record context.
    Record,
    /// Recordset (Odoo default for most methods).
    #[default]
    Recordset,
    /// Class-level (`@api.model` or no `self`).
    ClassLevel,
}

// ─────────────────────────────────────────────────────────────────────
// Sprint 3 — Action vocabulary with SPO + TeKaMoLo grammar
// (per docs/ADAPTERS-AND-ACTORS.md + brutal-review cycle 3 fixes)
//
// B1 fix: Action split into ActionDef (declaration) + ActionInvocation
// (per-context invocation). One ActionDef may have N invocations.
// B1 fix: KausalSpec is a proper sum type, not free-form opaque.
// B2 fix: Provenance fields (trace_id, parent_action_id, idempotency_key,
// emitted_at) carved into ActionInvocation.
// B2 fix: ActionState lifecycle (Pending / Committed / Failed) carved.
// ─────────────────────────────────────────────────────────────────────

/// An action declaration — the AST-extracted shape of a business
/// operation (a method-decorator combo, a callback declaration, a
/// workflow transition). One per source-level method/callback decl.
/// Invocations of this declaration become `ActionInvocation` triples.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct ActionDef {
    /// Stable identity for the action declaration (e.g.
    /// `ogit-erp/sale.order::action_def::action_confirm`).
    pub identity: String,
    /// Predicate name as written in source — `action_confirm`,
    /// `before_save`, etc.
    pub predicate: String,
    /// Object class — the OGAR-canonical class identity this action
    /// applies to (`ogit-erp/sale.order`).
    pub object_class: String,
    /// Default subject when not specified by an invocation.
    pub default_subject: ActionSubject,
    /// Default temporal annotation.
    pub default_temporal: TemporalSpec,
    /// Default modal annotation.
    pub default_modal: ModalSpec,
    /// Causal precondition — when None, action fires unconditionally
    /// at the right Te point. Sum type: real producers populate one
    /// of the typed variants below.
    pub kausal: Option<KausalSpec>,
    /// Method body verbatim (for projection emission).
    pub body_source: Option<String>,
    /// Decorator names that drove the extraction (Odoo `@api.depends`,
    /// Rails callback macro name).
    pub decorators: Vec<String>,

    // ── Rubicon statem carriers (OGAR-AST-CONTRACT §6) ──
    // The three semantics that don't survive Action-flattening; each lowers
    // onto `ractor_actors::state_machine` with `State = ActionState`.
    /// Entry effect fired on entering `Committed` (the Rubicon crossing) —
    /// typed via [`EnterEffect`] so codegen can apply the transition
    /// structurally (no string-parsing). Emitted as `ogar:onEnter`; lowers
    /// to `StateMachine::on_enter` / the `CommitHook`.
    pub on_enter: Option<EnterEffect>,
    /// Disposition when the Kausal `StateGuard` fails: `Postponable` (stay
    /// `Pending`, replay) vs `Reject` (`Pending → Failed`, the default).
    /// Emitted as `ogar:guardFailurePolicy`.
    pub guard_failure_policy: Option<GuardFailurePolicy>,
    /// Per-state SLA deadline on `Pending`, in milliseconds. Emitted as
    /// `ogar:stateTimeoutMillis`; the gen-stamped timer auto-cancels at the
    /// `Pending → Committed` crossing.
    pub state_timeout_millis: Option<i64>,
}

/// Typed entry effect — the structured representation of the state mutation
/// that fires on entering `Committed` (the Rubicon crossing). Replaces
/// free-form strings on [`ActionDef::on_enter`] so the codegen can apply the
/// transition structurally instead of string-parsing.
///
/// v1 carries the dominant lifecycle-FSM case (`field := to_value`). Complex
/// domain operations (e.g. chess `Move::Castle`) carry their payload on the
/// `ActionInvocation` and use `on_enter` only for the lifecycle-visible
/// transition (e.g. `side_to_move := Black`). Future tightening to typed
/// values (beyond string-encoded `to_value`) is a tracked follow-up.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct EnterEffect {
    /// Field on `object_instance` being set.
    pub field: String,
    /// Value to set the field to (string-encoded; typed values noted as a follow-up).
    pub to_value: String,
}

impl EnterEffect {
    /// Convenience constructor for the common `field := value` case.
    pub fn transition(field: impl Into<String>, to_value: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            to_value: to_value.into(),
        }
    }
}

/// Disposition when a `KausalSpec::StateGuard` is not satisfied — the Modal
/// sub-property for the Rubicon statem lowering (OGAR-AST-CONTRACT §6).
/// `#[non_exhaustive]` per the vocabulary forward-compat convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum GuardFailurePolicy {
    /// Transient failure — stay `Pending` and replay after the next
    /// transition. Lowers to `Transition::Postpone`.
    Postponable,
    /// Hard failure — `Pending → Failed` (the default).
    #[default]
    Reject,
}

/// A runtime invocation of an `ActionDef` — one per (S, P, O, context)
/// tuple. Captures the actual subject (which user / cron / cascade
/// fired this), provenance for tracing, and lifecycle state.
///
/// B2 production-blocker fixes: every invocation carries trace_id,
/// parent_action_id, idempotency_key (for at-least-once dedup), and
/// ActionState (Pending / Committed / Failed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct ActionInvocation {
    /// Unique per-invocation identity (ULID/UUID at runtime; OGAR
    /// canonical form `ogit-erp/sale.order::invocation::<ulid>`).
    pub identity: String,
    /// Reference to the ActionDef this invocation realizes.
    pub action_def: String,
    /// Subject of this specific invocation.
    pub subject: ActionSubject,
    /// Object instance identity (e.g. `ogit-erp/sale.order/42`).
    pub object_instance: String,
    /// Actual temporal context at invocation time (may differ from
    /// ActionDef.default_temporal — e.g. cron-deferred vs immediate).
    pub temporal: TemporalSpec,
    /// Actual modal context.
    pub modal: ModalSpec,
    /// Resolved Lokal (which actor instance / tenant / company).
    pub lokal: LokalSpec,
    /// Lifecycle state. Sprint 3 — start Pending; the callcenter
    /// (Sprint 7) transitions to Committed or Failed.
    pub state: ActionState,
    // ── B2 provenance fields ────────────────────────────────────
    /// OpenTelemetry trace ID for cross-actor correlation.
    pub trace_id: Option<String>,
    /// Parent action invocation that cascaded this one (None for
    /// top-level user-initiated actions).
    pub parent_invocation: Option<String>,
    /// Idempotency key — for Modal=Idempotent actions, the dedup
    /// store keys on this string.
    pub idempotency_key: Option<String>,
    /// UTC timestamp of invocation emit (millis since epoch).
    pub emitted_at_millis: Option<i64>,
    /// Failure detail when state == Failed.
    pub failure_reason: Option<String>,
}

/// Subject of a business action — who/what initiated it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum ActionSubject {
    /// A human user (UI button click, RPC call from authenticated user).
    User,
    /// Internal system trigger (no specific user).
    #[default]
    System,
    /// Scheduled (`ir.cron`, Rails `Whenever`).
    Cron,
    /// Reactive (DB event, `@api.depends` triggered).
    Trigger,
    /// Cascaded from a parent action invocation.
    Cascade,
}

/// Temporal context — when does the action happen relative to its
/// trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum TemporalSpec {
    /// Synchronous, on-call.
    #[default]
    Immediate,
    /// Queued, async background.
    Deferred,
    /// Run at scheduled time / interval (cron-like).
    Scheduled,
    /// After DB transaction commits (Rails `after_commit`,
    /// Odoo `@api.depends`).
    OnCommit,
}

/// Modal context — how is the action performed.
/// Per B3 YAGNI: dropped `Requires` (no v1 consumer); kept Idempotent
/// because it gates the dedup mechanism in `ActionInvocation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum ModalSpec {
    /// Synchronous, blocking.
    #[default]
    Sync,
    /// Fire-and-forget.
    Async,
    /// Safe to retry (uses `idempotency_key`).
    Idempotent,
    /// All-or-nothing transaction.
    Atomic,
}

/// Causal precondition — what triggered this action. **Sum type** per
/// B1 review fix. Producers populate one variant; the runtime guard
/// evaluator dispatches on the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum KausalSpec {
    /// State-field precondition (`self.state in {'draft', 'sent'}`).
    StateGuard {
        /// Field name on the object class.
        guard_field: String,
        /// Allowed values for the field.
        guard_values: Vec<String>,
    },
    /// Lifecycle event trigger (`before_save`, `after_create`).
    LifecycleTrigger {
        /// Event name as written.
        event: String,
    },
    /// `@api.depends` dependency paths (1..N).
    /// Per R3 research: avg 3 paths, p95 8, max 14.
    Depends {
        /// Field paths that trigger this action's recomputation.
        paths: Vec<String>,
    },
    /// `@api.depends_context` env-context keys.
    ContextDepends {
        /// Context keys that trigger recomputation.
        keys: Vec<String>,
    },
    /// External cause (RPC call, HTTP request) — no precondition
    /// to check inside the system.
    External,
}

/// Lokal context — where does the action execute (which actor /
/// tenant / company / db partition).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct LokalSpec {
    /// Actor identity that should handle this invocation (e.g.
    /// `ogit-erp/sale.order::actor`). Routes via NiblePath.
    pub actor: Option<String>,
    /// Tenant scope from the `Identity` tenant prefix.
    pub tenant: Option<String>,
    /// Multi-company id when applicable.
    pub company: Option<String>,
}

/// Lifecycle state of an `ActionInvocation`.
/// Per B2 production-blocker #3: explicit state machine prevents
/// the silent-gap problem (action started, didn't complete, no record).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum ActionState {
    /// Emitted but not yet processed by the callcenter.
    #[default]
    Pending,
    /// Successfully processed; effects committed.
    Committed,
    /// Processing failed; rollback complete (if Atomic) or
    /// partial effects recorded in `failure_reason`.
    Failed,
    /// Cancelled before execution.
    Cancelled,
}

// ─────────────────────────────────────────────────────────────────────
// Sprint 3 constructors (per #[non_exhaustive] convention)
// ─────────────────────────────────────────────────────────────────────

impl ActionDef {
    /// Build an ActionDef with identity, predicate, and object class.
    /// All other fields default.
    #[must_use]
    pub fn new(
        identity: impl Into<String>,
        predicate: impl Into<String>,
        object_class: impl Into<String>,
    ) -> Self {
        Self {
            identity: identity.into(),
            predicate: predicate.into(),
            object_class: object_class.into(),
            ..Default::default()
        }
    }
}

impl ActionInvocation {
    /// Build an ActionInvocation pointing at a defined ActionDef.
    #[must_use]
    pub fn new(
        identity: impl Into<String>,
        action_def: impl Into<String>,
        object_instance: impl Into<String>,
    ) -> Self {
        Self {
            identity: identity.into(),
            action_def: action_def.into(),
            object_instance: object_instance.into(),
            ..Default::default()
        }
    }
}

impl KausalSpec {
    /// Convenience: build a StateGuard.
    #[must_use]
    pub fn state_guard(field: impl Into<String>, values: Vec<String>) -> Self {
        Self::StateGuard {
            guard_field: field.into(),
            guard_values: values,
        }
    }

    /// Convenience: build a LifecycleTrigger.
    #[must_use]
    pub fn lifecycle(event: impl Into<String>) -> Self {
        Self::LifecycleTrigger {
            event: event.into(),
        }
    }

    /// Convenience: build a Depends spec.
    #[must_use]
    pub fn depends(paths: Vec<String>) -> Self {
        Self::Depends { paths }
    }
}

/// The four canonical Active Record relation kinds. Cross-ORM mapping:
/// Rails `belongs_to`/`has_one`/`has_many`/`has_and_belongs_to_many`,
/// Odoo `Many2one`/`One2many`/`Many2many` (Odoo collapses `has_one` into
/// `One2many` constrained to 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum AssociationKind {
    /// Owning side of a 1:N — the FK lives on this class's table.
    #[default]
    BelongsTo,
    /// Non-owning side of a 1:1.
    HasOne,
    /// Non-owning side of a 1:N.
    HasMany,
    /// Both sides of an M:N via join table.
    HasAndBelongsToMany,
}

/// An association declaration with the full Rails / Odoo option set.
/// Options unset by the source class are `None`; the consumer should
/// treat `None` as "infer per ORM defaults".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Association {
    /// The relation kind.
    pub kind: AssociationKind,
    /// Relation name — the leading symbol on the macro call
    /// (`:project`, `:line_items`, …).
    pub name: String,
    /// `class_name: "Foo::Bar"` — explicit target type when it can't be
    /// inferred from the relation name. `::` namespaces preserved.
    pub class_name: Option<String>,
    /// `foreign_key: "user_id"` — the FK column on the owning table.
    pub foreign_key: Option<String>,
    /// `polymorphic: true` — on `BelongsTo`, target is determined at
    /// runtime by a `<name>_type` column.
    pub polymorphic: Option<bool>,
    /// `through: :memberships` — names the intermediate association
    /// for `HasMany`/`HasOne`.
    pub through: Option<String>,
    /// `source: :principal` — aliasing on a through-association.
    pub source: Option<String>,
    /// `as: :container` — reverse-side polymorphism marker.
    pub as_target: Option<String>,
    /// `dependent: :destroy` / `:delete_all` / `:nullify` / `:restrict_*`.
    pub dependent: Option<String>,
    /// `optional: true` — on `BelongsTo`, allows the FK to be null.
    pub optional: Option<bool>,
    /// `inverse_of: :user` — the reciprocal relation on the target.
    pub inverse_of: Option<String>,
    /// `before_add: :method` collection callback.
    pub before_add: Option<String>,
    /// `after_add: :method` collection callback.
    pub after_add: Option<String>,
    /// `before_remove: :method` collection callback.
    pub before_remove: Option<String>,
    /// `after_remove: :method` collection callback.
    pub after_remove: Option<String>,
    /// Scoping lambda body — for Rails `has_many :line_items, -> { where(active: true) }`,
    /// Django `limit_choices_to={'active': True}`, Odoo `domain=[('active','=',True)]`.
    ///
    /// Captured verbatim as source text. Consumers treat as opaque
    /// (emit into the target form directly) or re-parse for their
    /// needs. `None` means the association has no scoping constraint
    /// — the default and most common case.
    pub scope_source: Option<String>,
    /// `ondelete='cascade'/'restrict'/'set null'/'set default'` —
    /// **DB-level FK action**, distinct from Rails `dependent:`
    /// (app-level). Stored separately to prevent cascade-semantics
    /// confusion. See `docs/ODOO-TRANSCODING.md` §5.
    pub ondelete: Option<String>,
    /// `auto_join=True` (Odoo) — auto SQL-join instead of lazy
    /// load on Many2one.
    pub auto_join: Option<bool>,
    /// `context={...}` (Odoo) — UI default context for navigation
    /// through this association. Captured verbatim as source text.
    pub context_source: Option<String>,
    /// `check_company=True` (Odoo) — multi-company tenancy check
    /// on the FK target.
    pub check_company: Option<bool>,
    /// `delegate=True` — legacy Odoo Many2one delegation (rare;
    /// modern Odoo uses `_inherits` on the class instead).
    pub delegate: Option<bool>,
}

/// An enum-backed column declaration.
///
/// The `source` field captures three Odoo cases (static / computed /
/// additive); for Rails / Django / Ecto only `Static` applies.
/// See `docs/ODOO-TRANSCODING.md` §6.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct EnumDecl {
    /// Column the enum is backed by.
    pub column: String,
    /// Where the enum's variant list comes from.
    pub source: EnumSource,
    /// `scopes: false` (Rails) — disables ORM-generated scope class
    /// methods. `None` when unset or non-bool.
    pub scopes_disabled: Option<bool>,
}

impl Default for EnumDecl {
    fn default() -> Self {
        Self {
            column: String::new(),
            source: EnumSource::Static(Vec::new()),
            scopes_disabled: None,
        }
    }
}

/// Source of an enum's variant list. Three cases capture Odoo's
/// `selection=`, `selection=lambda`, and `selection_add=`
/// surface; Rails / Django / Ecto producers always emit `Static`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum EnumSource {
    /// `selection=[('draft', 'Draft'), ('done', 'Done')]` — fixed
    /// list of `(key, label)` pairs.
    Static(Vec<(String, String)>),
    /// `selection=lambda self: self.env['res.country']...` — computed
    /// at runtime. The lambda body is captured verbatim.
    Computed(String),
    /// `selection_add=[('paid', 'Paid')]` — extends a parent
    /// `_inherit` model's enum without redeclaring it. `parent_selection`
    /// names the parent class.
    Add {
        /// Additional variants to add to the parent's selection.
        items: Vec<(String, String)>,
        /// The parent class whose selection is being extended
        /// (e.g. `"account.move.line"`).
        parent_selection: String,
    },
}

/// A `store_accessor :col, %i[a b c], prefix: true` declaration — N
/// JSONB pseudo-fields backed by one column.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct StoreAccessor {
    /// JSONB column backing the pseudo-fields.
    pub column: String,
    /// Pseudo-field names in source order.
    pub fields: Vec<String>,
    /// `prefix:` option as written.
    pub prefix: Option<bool>,
}

/// An `attribute :name, :type` schemaless / typed-attribute override.
///
/// Carries an `options` struct for all the cross-cutting kwargs Odoo,
/// Django, and Rails attach to field declarations (`required`,
/// `default`, `translate`, `tracking`, `index`, etc.). Producers
/// populate the subset they support; consumers branch on what's
/// `Some`. See `docs/ODOO-TRANSCODING.md` §4 for the full Odoo
/// kwarg → option mapping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Attribute {
    /// Attribute name as written.
    pub name: String,
    /// Type name as written (`"string"`, `"integer"`, `"big_integer"`,
    /// `"Char"`, `"Many2one"`, `"Monetary"`, `"Html"`, `"Image"`, …).
    /// Producer-specific — consumers interpret per source language.
    pub type_name: Option<String>,
    /// Cross-cutting per-attribute options. Populated by Odoo /
    /// Django / Rails producers as applicable.
    pub options: AttributeOptions,
}

/// The structured option-set on `Attribute`. Every Odoo kwarg has a
/// home here; no kwarg-dump bucket. Forward-compat via
/// `#[non_exhaustive]` — new producers add new fields, no breaking
/// change.
///
/// See `docs/ODOO-TRANSCODING.md` §4 for the full mapping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct AttributeOptions {
    /// `default=value` — literal as source text, or callable name.
    /// `None` means no default — column null / Rails default.
    pub default_source: Option<String>,
    /// `required=True` — NOT NULL constraint at the ORM level.
    pub required: Option<bool>,
    /// `readonly=True` — UI / ORM-write blocked.
    pub readonly: Option<bool>,
    /// `index=True` — DB index on the column.
    pub indexed: Option<bool>,
    /// `store=True` — relevant for computed fields; `False` means
    /// the value is recomputed on every read.
    pub stored: Option<bool>,
    /// `translate=True` — i18n column (jsonb in Odoo 17.0).
    pub translate: Option<bool>,
    /// `tracking=True` / `tracking=10` — Odoo audit log priority.
    /// `None` means no tracking; `Some(0)` means tracking with
    /// default priority; higher values are explicit priorities.
    pub tracking: Option<u8>,
    /// `groups='group.xml.id,another.group'` — visibility ACL.
    /// Comma-split into a list.
    pub groups: Vec<String>,
    /// `company_dependent=True` — value varies by `res.company`
    /// (Odoo multi-tenancy).
    pub company_dependent: Option<bool>,
    /// `copy=False` — excluded from `model.copy()`.
    pub copy_on_duplicate: Option<bool>,
    /// `help='...'` — UI tooltip text.
    pub help_text: Option<String>,
    /// `string='Label'` — UI label override (independent of `name`).
    pub label: Option<String>,
    /// `digits=(precision, scale)` — Float/Monetary precision.
    pub digits: Option<(u8, u8)>,
    /// `size=N` — Char/Binary size limit.
    pub size: Option<usize>,
    /// `currency_field='currency_id'` — Monetary field's currency
    /// linkage. Required for `Monetary` type, ignored otherwise.
    pub currency_field: Option<String>,
}

/// A `scope :name, -> { body }` definition. `body_source` is opaque
/// (verbatim source between the lambda brackets) — consumers either
/// accept it as an opaque SQL/DSL snippet or re-parse it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Scope {
    /// Scope name.
    pub name: String,
    /// Body source verbatim between the lambda brackets.
    pub body_source: String,
}

/// A lifecycle callback declaration. Two source forms collapse here:
///
/// - `event :method_name` → `target_method = Some`, `body_source = None`.
/// - `event do ... end` → `target_method = None`, `body_source = Some(text)`.
///
/// The event distinction (`before_*`/`after_*`/`around_*`) is preserved
/// in [`event`](Self::event) so consumers can reason about cascade vs.
/// wrap semantics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Callback {
    /// Event name as written: `before_save`, `after_create`,
    /// `around_destroy`, `after_commit`, …
    pub event: String,
    /// Method name target when the callback names a method.
    pub target_method: Option<String>,
    /// Block body source when the callback is `event do ... end` /
    /// `event { ... }`.
    pub body_source: Option<String>,
}

/// A validation declaration — `validates :col, presence: true` /
/// `@api.constrains('col')`. Placeholder shape; the validation-rule
/// grammar is the next sprint to lift cleanly across ORMs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Validation {
    /// Column or attribute the validation applies to.
    pub target: String,
    /// Validation rule body verbatim. Per-ORM grammar is producer-side.
    pub rule_source: String,
}

// ─────────────────────────────────────────────────────────────────────
// Constructors
//
// Because the public types in this module are `#[non_exhaustive]` (for
// forward compatibility — see the `Language` enum docs), external crates
// cannot construct them with struct-literal syntax. The constructors
// below take the minimal required fields and default the rest, then
// the caller mutates whatever it needs:
//
//     let mut class = Class::new("WorkPackage");
//     class.parent = Some("ApplicationRecord".into());
//
// This is the canonical Rust pattern for `#[non_exhaustive]` types.
// ─────────────────────────────────────────────────────────────────────

impl Class {
    /// Build a new class with only the name set. All other fields are
    /// `Default::default()`. Mutate after construction.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Resolve this class's binary [OGAR codebook] identity — the `u16`
    /// canonical id derived from its stored `canonical_concept`. Returns
    /// `None` when the class has no canonical concept set (a producer
    /// that never set it; rare in practice).
    ///
    /// This is the load-bearing convergence claim: two curator-shaped
    /// classes (Redmine `Issue`, OpenProject `WorkPackage`) lifting to
    /// the same `canonical_concept` produce the same `canonical_id`.
    /// **String labels are decorative; the codebook value is the identity.**
    #[must_use]
    pub fn canonical_id(&self) -> Option<u16> {
        self.canonical_concept
            .as_deref()
            .and_then(canonical_concept_id)
    }

    /// `canonical_id` rendered as **2 little-endian bytes** — the wire
    /// contract for downstream consumers (SurrealAST, lance-graph-planner,
    /// kanban, …). `None` when no `canonical_concept` is set.
    #[must_use]
    pub fn canonical_id_le(&self) -> Option<[u8; 2]> {
        self.canonical_id().map(u16::to_le_bytes)
    }
}

/// **OGAR codebook registry** — the curated `(canonical_concept, id)`
/// table. Per the integration contract (`docs/INTEGRATION-MAP.md`:92-93,
/// "ClassId / entity_type is minted uniquely by the registry and is
/// never a content hash"), codebook ids are **assigned**, not derived
/// from a hash.
///
/// # Domain-encoded id layout — `0xDDCC`
///
/// Codebook ids are **domain-prefixed**: the high byte encodes the
/// concept's domain, the low byte its slot within that domain. A
/// consumer loading both project and commerce concepts routes on domain
/// in O(1) from just the `u16` — see [`canonical_concept_domain`].
///
/// ```text
///   0x00XX  reserved          (0x0000 = NodeGuid::CLASSID_DEFAULT)
///   0x01XX  project-mgmt      (OP ↔ Redmine fork lineage)
///   0x02XX  commerce / ERP    (OSB ↔ Odoo cross-curator)
///   0x03XX  unassigned
///   0x04XX  unassigned
///   0x05XX  unassigned
///   0x06XX  unassigned
///   0x07XX  reserved: OSINT
///   0x08XX  OCR               (container kinds: unicharset/recoder/charset)
///   0x09XX  Health            (clinical / patient / care; 7 OGIT entities)
///   0x0AXX  Anatomy           (FMA reference ontology; bones/skeleton)
///   0x0BXX  Auth              (IAM; the AuthStore class family)
///   0x0CXX  Automation        (HIRO IT-automation: MARS CMDB + actuators)
///   0x0DXX  HR                (employment / org / contracts)
///   0x0EXX  reserved: Genetics (CPIC pharmacogenomics, consumed by q2)
///   0x0FXX+ unassigned
/// ```
///
/// **Anatomy vs Health (the firewall split).** `0x0AXX` Anatomy is the
/// **public reference structure** (the femur exists; it is `part_of` the
/// lower limb) — the FMA atlas frame every imaging modality registers
/// against. It is deliberately NOT in `0x09XX` Health: a clinical *finding
/// about* anatomy (a fracture diagnosis on a named patient) is Health PHI,
/// but the anatomical *structure itself* is public and must not be pulled
/// into medcare-rs's fail-closed Health RBAC coverage set. This is why the
/// earlier "FMA / SNOMED converges into Health" forward-note (below) lands
/// in its own domain instead: reference ≠ PHI.
///
/// Reserved blocks have a placeholder [`ConceptDomain`] variant so a
/// consumer routing on `id >> 8` returns a stable domain tag even before
/// any concept lands in that block.
///
/// Ids are stable forever — once shipped, never re-assigned. Each new
/// promoted concept gets the next free slot inside its domain block.
/// Verified collision-free + non-zero by `codebook_has_no_duplicate_ids_or_zero`.
const CODEBOOK: &[(&str, u16)] = &[
    // ── 0x01XX — project-mgmt domain (OP ↔ Redmine fork lineage) ──
    ("project", 0x0101),
    ("project_work_item", 0x0102),
    ("billable_work_entry", 0x0103),
    ("project_actor", 0x0104),
    ("project_status", 0x0105), // Redmine IssueStatus ↔ OP Status
    ("project_type", 0x0106),   // Redmine Tracker     ↔ OP Type
    ("priority", 0x0107),
    // New project-mgmt promotions from the cross-curator overlap probe
    // (Redmine 111 classes ↔ OpenProject 681 classes ⇒ 38 common names;
    // 9 chosen for promotion based on substantive shape on both sides):
    ("project_membership", 0x0108), // Redmine Member       ↔ OP Member
    ("project_journal", 0x0109),    // Redmine Journal      ↔ OP Journal
    ("project_repository", 0x010A), // Redmine Repository   ↔ OP Repository
    ("project_version", 0x010B),    // Redmine Version      ↔ OP Version
    ("project_wiki_page", 0x010C),  // Redmine WikiPage     ↔ OP WikiPage
    ("project_query", 0x010D),      // Redmine Query        ↔ OP Query
    ("project_attachment", 0x010E), // Redmine Attachment   ↔ OP Attachment
    ("project_comment", 0x010F),    // Redmine Comment      ↔ OP Comment
    ("project_custom_field", 0x0110), // Redmine CustomField  ↔ OP CustomField
    ("project_relation", 0x0111),   // Redmine IssueRelation ↔ OP Relation (name divergence)
    ("project_changeset", 0x0112),  // Redmine Changeset     ↔ OP Changeset
    ("project_watcher", 0x0113),    // Redmine Watcher       ↔ OP Watcher
    ("project_news", 0x0114),       // Redmine News          ↔ OP News
    ("project_message", 0x0115),    // Redmine Message       ↔ OP Message (board/forum divergence)
    ("project_forum", 0x0116), // Redmine Board         ↔ OP Forum (name divergence; parent of project_message)
    ("project_role", 0x0117),  // Redmine Role          ↔ OP Role (RBAC permission set)
    ("project_member_role", 0x0118), // MemberRole — RBAC join (membership ↔ role)
    ("project_custom_value", 0x0119), // CustomValue — value of a custom field on a record
    ("project_enabled_module", 0x011A), // EnabledModule — per-project module enablement
    // ── 0x02XX — commerce / billing / ERP domain (OSB ↔ Odoo) ──
    // Promoted from the parallel session's `lance-graph-ontology::ar_shape`
    // upstream-candidate registry; each backed by ≥2-curator structural
    // evidence on the OSB and Odoo corpora.
    //
    //   | Canonical              | OSB             | Odoo                |
    //   |------------------------|-----------------|---------------------|
    //   | commercial_line_item   | InvoiceLineItem | account_move_line   |
    //   | commercial_document    | Invoice         | account_move        |
    //   | tax_policy             | Tax             | account_tax         |
    //   | billing_party          | Client          | res_partner         |
    //   | payment_record         | Payment         | account_payment     |
    //   | currency_policy        | Currency        | res_currency        |
    //
    // (`tax_policy` is also referenced by `billable_work_entry().classified_by`
    // as an ERP-boundary edge — this lands the canonical class that edge
    // points at.)
    ("commercial_line_item", 0x0201),
    ("commercial_document", 0x0202),
    ("tax_policy", 0x0203),
    ("billing_party", 0x0204),
    ("payment_record", 0x0205),
    ("currency_policy", 0x0206),
    // Phase-3 OGAR-side mints from the cross-axis identity gap surfaced in
    // odoo-rs PR #14 (`alignment_pin::seeded_classes_have_compatible_ogar_identity`):
    // `OdooPort` covered the commerce arm only (9 aliases); the alignment
    // table extends to 6 basins (BillingCore / SMBAccounting /
    // SmbFoundryCustomer / SmbFoundryInvoice / ProductCatalog / HRFoundation).
    // These two mints close the highest-impact gap (4 of 11 missing aliases).
    // The remaining 7 (pricelist*, uom*, hr*) are queued for follow-up — see
    // PR description for the queue.
    ("product", 0x0207),
    ("accounting_account", 0x0208),
    // ProductCatalog cluster — closes 3 more of the 11-gap. All stay in 0x02XX
    // (no new ConceptDomain needed). HR cluster (hr.*) remains queued; needs
    // a new 0x0DXX concept domain (keystone-style §7 review).
    ("pricelist", 0x0209),
    ("pricelist_rule", 0x020A),
    ("unit_of_measure", 0x020B),
    // ── 0x07XX — OSINT domain: ZERO vocabulary rows BY DESIGN (operator
    // ruling 2026-07-02, corrects PR #145's two hallucinated concept mints
    // `osint_system@0x0700` / `osint_person@0x0701`). Within the OSINT domain
    // the low byte is NOT a concept slot — it is allocated domain-wise as an
    // APPID: `0x0700` = the OSINT domain itself (low byte 00 = domain-wide),
    // `0x0701` = OSINT-for-q2 (q2 is appid 0x01, the consumer); V3 stored form
    // `0x0701_1000` (canon HIGH since the same-day half-order flip —
    // human-readable `0x07:01::1000`). Class content (AIRO/VAIR system card,
    // McClelland/Rubicon
    // person lens) lives consumer-side in q2's `osint_classview.rs` — OGAR
    // vocabulary carries no OSINT concept names. Do NOT re-mint rows here.
    // ── 0x08XX — OCR domain (document extraction; the Tesseract-rs arc) ──
    // Class-level container KINDS only: unicharset / recoder / charset are
    // the container types the Core resolves. Their CONTENT (the 112 unichars
    // of a trained set, the code tables) lives in content stores — never as
    // concept slots. Guard precedent: the 0x07XX Osint zero-rows ruling
    // (content ≠ concepts); the difference here is that OCR's container
    // kinds ARE cross-app concepts (Tesseract-rs producer, lance-graph
    // keystone consumer), so the kinds get slots while the content does not.
    ("unicharset", 0x0801),
    ("recoder", 0x0802),
    ("charset", 0x0803),
    // ── 0x09XX — Health domain (clinical / patient / care) ──
    // medcare-rs Healthcare-namespace promotion (Northstar T9). The 7
    // entities the OGIT `NTO/Healthcare/entities/` TTL ships, projected
    // onto canonical Health ids so `ports::HealthcarePort` resolves them
    // through the `UnifiedBridge` codebook path (the same way OpenProject
    // `WorkPackage` and Redmine `Issue` resolve through their ports).
    // Single-tenant today — no cross-curator convergence yet — but the
    // ids are minted into the shared codebook so a future second clinical
    // curator (FMA / SNOMED import) converges here rather than re-mints.
    ("patient", 0x0901),
    ("diagnosis", 0x0902),
    ("lab_value", 0x0903),
    ("medication", 0x0904),
    ("treatment", 0x0905),
    ("visit", 0x0906),
    ("vital_sign", 0x0907),
    // ── 0x0AXX — Anatomy domain (FMA reference ontology) ──
    // The public anatomical reference frame consumed by the splat-native
    // ultrasound arc (`docs/SPLAT-NATIVE-CUSTOMER.md` §6 litmus) and the FMA
    // skeletal spine (`crates/ogar-fma-skeleton`). These are the *kinds*
    // (Bone, Skeleton, …); the ~206 individual bones are NOT concept slots —
    // they live as cascade-path nodes (FMA partonomy → HEEL/HIP/TWIG prefix
    // tree), the same way Wikidata-HHTL lives in the path, not the codebook.
    // `bone` is the clamped convergence-anchor class: bones are the rigid,
    // non-negotiable frame every imaging modality (ViT / X-ray / ultrasound
    // × Doppler) registers against. See `docs/FMA-SKELETON-CONVERGENCE-ANCHOR.md`.
    ("anatomical_structure", 0x0A01),
    ("skeleton", 0x0A02),
    ("bone", 0x0A03),
    ("joint", 0x0A04),
    // ── 0x0BXX — Auth domain (IAM; provider-agnostic — the AuthStore class family) ──
    // Per `docs/CLASSID-RBAC-KEYSTONE-SPEC.md` §7 + `APP-CLASS-CODEBOOK-LAYOUT.md`
    // §2: auth is a CORE domain of its own (`0x0B`), cross-app and
    // provider-agnostic. The IdP→classid bridge IS a registry class (keystone
    // I-K0), preminted as provider profiles. CONFIRMED by the canonical OGIT
    // shape: arago's January-2026 `NTO/Auth/Configuration` entity — keyed by
    // `organizationId` / `accountId` / `applicationId` / `scopeId` +
    // `configurationData`, "registered in hiro knowledge core" — IS `auth_store`,
    // built upstream independently (see `.claude/board/EPIPHANIES.md` 2026-06-23).
    // `auth_store` does the mapping (`sub`→actor `0x0104`, role-key→role `0x0117`,
    // org/tenant→scope); the provider profiles carry each IdP's claim grammar as
    // data; Zitadel maps 1:1. These are RESERVATIONS (reserving costs nothing);
    // the enforcement `authorize()` is gated on `PROBE-OGAR-RBAC-AUTHORIZE`
    // (keystone §10) and is NOT part of this mint.
    ("auth_store", 0x0B01),
    ("auth_zitadel", 0x0B02),
    ("auth_zanzibar", 0x0B03),
    ("auth_ory_keto", 0x0B04),
    // ── 0x0CXX — Automation domain (the HIRO IT-automation stack) ──
    // One domain spanning the MARS structural CMDB (`ogit.MARS:` —
    // Application/Resource/Software/Machine, the A→R→S→M dependsOn backbone)
    // AND the Automation actuators (`ogit.Automation:` — KnowledgeItem /
    // ActionHandler / Trigger, HIRO's behavioral vocabulary). Two OGIT
    // sub-namespaces, ONE concept domain — the same justification the Auth
    // family uses (heterogeneous shapes, one cross-app concern): the domain
    // byte is the hi-u16 shared-concept half; the render prefix
    // (`ogit-mars` / `ogit-automation`) is the lo-u16 skin (canon HIGH /
    // custom LOW since the 2026-07-02 half-order flip). The DO arm (`ActionDef`) and the
    // THINK arm (the MARS `Class`es) meet here. This IS the codebook pass that
    // `docs/MARS-TRANSCODING.md` §1 deferred ("provisional… after the codebook
    // pass"); minted via the 5+3 hardening (theorem-checker / doctrine-keeper /
    // integration-lead / runtime-archaeologist + cargo gates). The set is the
    // load-bearing concepts the structural + DO-arm lifts stand on; further
    // Automation entities (action_capability / intent / automation_issue /
    // variable / mars_node) are RESERVED, minted when a lift/consumer uses them.
    ("mars_application", 0x0C01),
    ("mars_resource", 0x0C02),
    ("mars_software", 0x0C03),
    ("mars_machine", 0x0C04),
    ("knowledge_item", 0x0C05),
    ("mars_node_template", 0x0C06),
    ("action_handler", 0x0C07),
    ("action_applicability", 0x0C08),
    ("automation_trigger", 0x0C09),
    // ── 0x0DXX — HR domain (employment / org / contracts) ──
    // Public HR master-data: person + organizational-unit + role + employment-
    // contract entities. Distinct from Auth (IdP→classid bridge) and Health
    // (PHI). Closes the final 4-of-11 cross-axis identity gap surfaced by
    // odoo-rs PR #14: hr.employee / hr.department / hr.job / hr.contract.
    ("hr_employee", 0x0D01),
    ("hr_department", 0x0D02),
    ("hr_job", 0x0D03),
    ("hr_employment_contract", 0x0D04),
    // ── 0x0EXX — Genetics domain (CPIC pharmacogenomics, consumed by q2):
    // ZERO vocabulary rows today — same posture as the 0x07XX OSINT block
    // above. The domain slot is reserved (`ConceptDomain::Genetics`) so
    // `canonical_concept_domain` returns a stable tag before any concept
    // mints, per the ledger commitment to the V3 marker form
    // `0x0E01_1000` (`docs/DISCOVERY-MAP.md` D-CLASSID-CANON-HIGH-FLIP).
    // Do NOT mint rows here without an operator ruling.
];

/// Codebook **domain** — the high byte of a canonical id (see
/// [`CODEBOOK`] layout). Lets a consumer route on domain in O(1) from
/// just the `u16`, without a table lookup.
///
/// Reserved high-byte slots are listed with their intended domain name
/// even before any concept lands in that block, so consumers can branch
/// on them today and the meaning is stable as concepts arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum ConceptDomain {
    /// `0x00XX` — reserved (`0x0000` is `NodeGuid::CLASSID_DEFAULT`).
    Reserved,
    /// `0x01XX` — project-management (OP ↔ Redmine).
    ProjectMgmt,
    /// `0x02XX` — commerce / billing / ERP (OSB ↔ Odoo).
    Commerce,
    /// `0x07XX` — OSINT (open-source intelligence).
    Osint,
    /// `0x08XX` — OCR (optical character recognition / document
    /// extraction).
    Ocr,
    /// `0x09XX` — Health (clinical / patient / care).
    Health,
    /// `0x0AXX` — Anatomy (FMA reference ontology; the public anatomical
    /// structure frame — bones / skeleton / joints). Distinct from
    /// [`Health`](Self::Health): reference structure is public, a clinical
    /// finding *about* it is PHI. The clamped convergence-anchor frame for
    /// the splat-native imaging arc.
    Anatomy,
    /// `0x0BXX` — Auth (IAM; provider-agnostic — the AuthStore class
    /// family: `auth_store` + per-IdP profiles `auth_zitadel` /
    /// `auth_zanzibar` / `auth_ory_keto`). See
    /// `docs/CLASSID-RBAC-KEYSTONE-SPEC.md` §7.
    Auth,
    /// `0x0CXX` — Automation (the HIRO IT-automation stack). One domain
    /// spanning the MARS structural CMDB (`mars_application` / `mars_resource`
    /// / `mars_software` / `mars_machine` — the A→R→S→M dependsOn backbone)
    /// and the Automation actuators (`knowledge_item` / `mars_node_template` /
    /// `action_handler` / `action_applicability` / `automation_trigger` —
    /// HIRO's DO-arm vocabulary). The DO arm (`ActionDef`) and the THINK arm
    /// (the MARS `Class`es) meet here. Infrastructure config, NOT PHI — same
    /// public-reference posture as [`Anatomy`](Self::Anatomy). See
    /// `docs/MARS-TRANSCODING.md` + `docs/HIRO-DO-ARM-LIFT.md`.
    Automation,
    /// `0x0DXX` — HR (employment / org / contracts). Public master-data for
    /// person + organizational-unit + role + employment-contract entities;
    /// distinct from `Auth` identity (which is the IdP→classid bridge) and
    /// from `Health` PHI. Mirrors arago HIRO HR semantics + Odoo `hr.*` +
    /// `vcard:Individual` / `org:OrganizationalUnit` / `org:Role` /
    /// `fibo:Contract` alignment.
    HR,
    /// `0x0EXX` — Genetics (CPIC pharmacogenomics), consumed by q2. Carries
    /// ZERO vocabulary rows today — same posture as [`Osint`](Self::Osint):
    /// the domain slot is reserved so `canonical_concept_domain` returns a
    /// stable tag before any concept mints, per the 2026-07-02 half-order
    /// flip ledger commitment to the V3 marker form `0x0E01_1000` (CPIC
    /// under q2, appid `0x01`; `docs/DISCOVERY-MAP.md`
    /// D-CLASSID-CANON-HIGH-FLIP). Do NOT re-mint OSINT-style hallucinated
    /// concept rows here — same guard as the OSINT domain note in
    /// [`CODEBOOK`].
    Genetics,
    /// Any high-byte slot not yet assigned a domain (`0x03XX`–`0x06XX`,
    /// `0x0FXX`+).
    Unassigned,
}

/// Resolve a canonical id's [`ConceptDomain`] from its high byte. Pure +
/// deterministic + O(1) — no table lookup needed.
#[must_use]
pub fn canonical_concept_domain(id: u16) -> ConceptDomain {
    match id >> 8 {
        0x00 => ConceptDomain::Reserved,
        0x01 => ConceptDomain::ProjectMgmt,
        0x02 => ConceptDomain::Commerce,
        0x07 => ConceptDomain::Osint,
        0x08 => ConceptDomain::Ocr,
        0x09 => ConceptDomain::Health,
        0x0A => ConceptDomain::Anatomy,
        0x0B => ConceptDomain::Auth,
        0x0C => ConceptDomain::Automation,
        0x0D => ConceptDomain::HR,
        0x0E => ConceptDomain::Genetics,
        _ => ConceptDomain::Unassigned,
    }
}

/// Every promoted `(canonical_concept_name, id)` whose id lives in `domain`,
/// in codebook order. The **reusable enumeration hook** a domain-scoped
/// consumer uses to inherit its full concept set from the canonical layer
/// instead of hand-maintaining a parallel list.
///
/// This is the fail-closed primitive behind RBAC marking-inheritance: a
/// consumer that must access-control every concept in a sensitive domain
/// (e.g. medcare-rs over [`ConceptDomain::Health`]) derives the required
/// coverage set from here, so a concept newly promoted into the codebook
/// upstream becomes a *missing-coverage* signal at the consumer's boot
/// gate — never a silently-uncovered (fail-open) row.
///
/// ```
/// use ogar_vocab::{concepts_in_domain, ConceptDomain};
///
/// let health: Vec<_> = concepts_in_domain(ConceptDomain::Health)
///     .map(|(name, _id)| name)
///     .collect();
/// assert!(health.contains(&"patient"));
/// assert_eq!(health.len(), 7); // the 7 OGIT Healthcare entities
/// ```
pub fn concepts_in_domain(domain: ConceptDomain) -> impl Iterator<Item = (&'static str, u16)> {
    CODEBOOK
        .iter()
        .copied()
        .filter(move |&(_, id)| canonical_concept_domain(id) == domain)
}

/// Map a coarse [`Class::source_domain`] tag — as produced by the curator
/// namespace classifier (`"project"`, `"erp"`, `"german-erp"`, …) — to the
/// [`ConceptDomain`] its promotions live in. Returns `None` for an
/// unrecognised or absent tag; [`canonical_concept_in_domain`] treats that
/// as "curator domain unknown" and withholds codebook promotion.
///
/// This is the seam a producer crosses from its coarse source-domain string
/// to the typed codebook domain — keeping the mapping in `ogar-vocab` (not
/// hardcoded in each producer) so it stays consistent with the [`CODEBOOK`]
/// layout.
#[must_use]
pub fn source_domain_concept(source_domain: &str) -> Option<ConceptDomain> {
    match source_domain {
        "project" => Some(ConceptDomain::ProjectMgmt),
        "erp" | "german-erp" => Some(ConceptDomain::Commerce),
        _ => None,
    }
}

/// **OGAR codebook lookup** — resolve a canonical-concept string to its
/// stable `u16` codebook id via the curated [`CODEBOOK`] registry.
/// Returns `None` for unpromoted concepts — they are not in the codebook.
///
/// `u16` width per `OD-CLASSID-WIDTH` (lance-graph-contract `ClassId`).
///
/// # Wire contract — 2 little-endian bytes
///
/// Downstream consumers (SurrealDB AST, lance-graph-planner, kanban, …)
/// serialise the id as 2 little-endian bytes via `u16::to_le_bytes`. Byte
/// order matches the `NodeGuid` layout (`lance-graph-contract`:
/// `canonical_node.rs` — LE throughout) so codebook ids and the
/// `NodeGuid.classid` u16 low half are wire-compatible.
///
/// The contract type ([`LabelDTO`]) lives in `ogar-vocab` today; long-term
/// it belongs in `lance-graph-contract` alongside `ClassId` and the
/// `NodeGuid` LE layout. Wire is the source of truth: any encoder/decoder
/// agreeing on `u16` LE is compatible regardless of which crate exports
/// the DTO.
#[must_use]
pub fn canonical_concept_id(concept: &str) -> Option<u16> {
    CODEBOOK
        .iter()
        .find_map(|(name, id)| if *name == concept { Some(*id) } else { None })
}

/// Inverse of [`canonical_concept_id`]: the canonical concept name for a
/// codebook `id`, or `None` if the id is not a promoted concept.
///
/// Ids are unique in [`CODEBOOK`] (a concept arrives once and never moves —
/// asserted by [`tests::canonical_concept_name_round_trips`]), so the mapping
/// is 1:1 and round-trips both ways:
/// `canonical_concept_id(canonical_concept_name(id)?) == Some(id)`.
///
/// This is the reverse lookup a consumer needs to turn a *resolved* classid
/// back into its human-readable concept without re-deriving (or copying) the
/// codebook locally — e.g. to stamp `COMMENT 'commercial_document
/// (classid:0x00020202)'` into emitted DDL, or to populate a `Class`'s
/// `canonical_concept` from an id. The forward `name -> id` step
/// ([`canonical_concept_id`] via a port alias) followed by this `id -> name`
/// step resolves the alias asymmetry that lexical canonicalisation cannot
/// (e.g. both `sale.order` and `account.move` resolve to `0x0202` ->
/// `commercial_document`, where neither name lexically *is* the concept).
#[must_use]
pub fn canonical_concept_name(id: u16) -> Option<&'static str> {
    CODEBOOK
        .iter()
        .find_map(|(name, cid)| if *cid == id { Some(*name) } else { None })
}

/// **Compile-time class-id constants** — every promoted concept's id
/// exposed as a named `pub const u16` so downstream consumers can dispatch
/// on canonical identity at compile time without a [`canonical_concept_id`]
/// lookup.
///
/// This is the **canonical home** of the constants. The two `-rs` consumer
/// crates (`redmine-canon::class_ids` and `op-canon::class_ids`) re-export
/// from here so the values cannot drift across ports: both Rust ports of
/// the project-management curators (Redmine, OpenProject) speak this one
/// list.
///
/// ```
/// use ogar_vocab::class_ids;
///
/// fn dispatch(incoming_id: u16) {
///     match incoming_id {
///         class_ids::PROJECT_WORK_ITEM => handle_work_item(),
///         class_ids::BILLABLE_WORK_ENTRY => handle_time_entry(),
///         _ => {}
///     }
/// }
/// # fn handle_work_item() {}
/// # fn handle_time_entry() {}
/// ```
///
/// Drift between the constants and [`CODEBOOK`] is impossible without a
/// failing test: [`class_ids::tests::constants_match_codebook`] (the
/// forward gate) and [`class_ids::tests::every_codebook_entry_has_a_const`]
/// (the reverse gate) walk both directions.
///
/// Ids are stable forever (per the codebook contract). They only arrive —
/// never move, never get re-assigned.
pub mod class_ids {
    // ── 0x01XX — project-mgmt domain ──

    /// `project` (`0x0101`) — root project container.
    pub const PROJECT: u16 = 0x0101;
    /// `project_work_item` (`0x0102`) — project-scoped work item. Redmine
    /// `Issue` / OpenProject `WorkPackage`.
    pub const PROJECT_WORK_ITEM: u16 = 0x0102;
    /// `billable_work_entry` (`0x0103`) — booked work / time / cost. The
    /// **first cross-domain bridge**: OpenProject `TimeEntry`, Redmine
    /// `TimeEntry`, Odoo `account.analytic.line` all converge here.
    pub const BILLABLE_WORK_ENTRY: u16 = 0x0103;
    /// `project_actor` (`0x0104`) — actor identity (Principal + User +
    /// Group STI chain collapsed).
    pub const PROJECT_ACTOR: u16 = 0x0104;
    /// `project_status` (`0x0105`) — workflow status. Redmine `IssueStatus`,
    /// OpenProject `Status`.
    pub const PROJECT_STATUS: u16 = 0x0105;
    /// `project_type` (`0x0106`) — work-item type. Redmine `Tracker`,
    /// OpenProject `Type`.
    pub const PROJECT_TYPE: u16 = 0x0106;
    /// `priority` (`0x0107`) — priority enumeration. Both ship `IssuePriority`.
    pub const PRIORITY: u16 = 0x0107;
    /// `project_membership` (`0x0108`) — actor↔project join. Both ship `Member`.
    pub const PROJECT_MEMBERSHIP: u16 = 0x0108;
    /// `project_journal` (`0x0109`) — change journal entry.
    pub const PROJECT_JOURNAL: u16 = 0x0109;
    /// `project_repository` (`0x010A`) — VCS repository.
    pub const PROJECT_REPOSITORY: u16 = 0x010A;
    /// `project_version` (`0x010B`) — release / milestone.
    pub const PROJECT_VERSION: u16 = 0x010B;
    /// `project_wiki_page` (`0x010C`).
    pub const PROJECT_WIKI_PAGE: u16 = 0x010C;
    /// `project_query` (`0x010D`) — saved query.
    pub const PROJECT_QUERY: u16 = 0x010D;
    /// `project_attachment` (`0x010E`).
    pub const PROJECT_ATTACHMENT: u16 = 0x010E;
    /// `project_comment` (`0x010F`).
    pub const PROJECT_COMMENT: u16 = 0x010F;
    /// `project_custom_field` (`0x0110`).
    pub const PROJECT_CUSTOM_FIELD: u16 = 0x0110;
    /// `project_relation` (`0x0111`) — work-item↔work-item link. Redmine
    /// `IssueRelation`, OpenProject `Relation`.
    pub const PROJECT_RELATION: u16 = 0x0111;
    /// `project_changeset` (`0x0112`) — VCS commit metadata.
    pub const PROJECT_CHANGESET: u16 = 0x0112;
    /// `project_watcher` (`0x0113`).
    pub const PROJECT_WATCHER: u16 = 0x0113;
    /// `project_news` (`0x0114`) — project news / blog post.
    pub const PROJECT_NEWS: u16 = 0x0114;
    /// `project_message` (`0x0115`) — forum / board message.
    pub const PROJECT_MESSAGE: u16 = 0x0115;
    /// `project_forum` (`0x0116`) — message container. Redmine `Board`,
    /// OpenProject `Forum`.
    pub const PROJECT_FORUM: u16 = 0x0116;
    /// `project_role` (`0x0117`) — RBAC permission-set bundle.
    pub const PROJECT_ROLE: u16 = 0x0117;
    /// `project_member_role` (`0x0118`) — RBAC join (membership ↔ role).
    pub const PROJECT_MEMBER_ROLE: u16 = 0x0118;
    /// `project_custom_value` (`0x0119`) — value of a custom field on a record.
    pub const PROJECT_CUSTOM_VALUE: u16 = 0x0119;
    /// `project_enabled_module` (`0x011A`) — per-project module enablement.
    pub const PROJECT_ENABLED_MODULE: u16 = 0x011A;

    // ── 0x02XX — commerce / billing / ERP domain ──

    /// `commercial_line_item` (`0x0201`) — line on a commercial document.
    /// OSB `InvoiceLineItem`, Odoo `account.move.line`.
    pub const COMMERCIAL_LINE_ITEM: u16 = 0x0201;
    /// `commercial_document` (`0x0202`) — invoice / posting head.
    /// OSB `Invoice`, Odoo `account.move`.
    pub const COMMERCIAL_DOCUMENT: u16 = 0x0202;
    /// `tax_policy` (`0x0203`) — tax rate / classification. OSB `Tax`,
    /// Odoo `account.tax`. Also `billable_work_entry.classified_by` target.
    pub const TAX_POLICY: u16 = 0x0203;
    /// `billing_party` (`0x0204`) — counterparty (customer/vendor/partner).
    /// OSB `Client`, Odoo `res.partner`.
    pub const BILLING_PARTY: u16 = 0x0204;
    /// `payment_record` (`0x0205`) — payment event. OSB `Payment`,
    /// Odoo `account.payment`.
    pub const PAYMENT_RECORD: u16 = 0x0205;
    /// `currency_policy` (`0x0206`) — currency lookup. OSB `Currency`,
    /// Odoo `res.currency`.
    pub const CURRENCY_POLICY: u16 = 0x0206;
    /// `product` (`0x0207`) — saleable / billable item (catalogue master).
    /// OSB `Product`, Odoo `product.template` + `product.product` (both
    /// converge here; the variant relation lives outside the codebook,
    /// `commercial_document.line_items.references` target).
    ///
    /// Promoted Phase-3 from the cross-axis identity gap surfaced in odoo-rs
    /// PR #14: the alignment table seeds `product.template → schema:Product`
    /// + BillingCore (0x61); this id is the OGAR-side identity that closes
    ///   the same axis. `OdooPort` carries `product.template` and
    ///   `product.product` as aliases of `PRODUCT`.
    pub const PRODUCT: u16 = 0x0207;
    /// `accounting_account` (`0x0208`) — general-ledger account (SKR-aligned
    /// chart concept). OSB `Account`, Odoo `account.account` +
    /// `account.account.template` (the SKR03/04 chart-of-accounts template;
    /// both converge on this id).
    ///
    /// Promoted Phase-3 from the cross-axis identity gap surfaced in odoo-rs
    /// PR #14: the alignment table seeds `account.account → fibo:Account` +
    /// SMBAccounting (0x62); this id is the OGAR-side identity that closes
    /// the same axis.
    pub const ACCOUNTING_ACCOUNT: u16 = 0x0208;
    /// `pricelist` (`0x0209`) — price-specification base. OSB `Pricelist`,
    /// Odoo `product.pricelist` (`schema:PriceSpecification`). Phase-3
    /// ProductCatalog cluster.
    pub const PRICELIST: u16 = 0x0209;
    /// `pricelist_rule` (`0x020A`) — per-tier unit-price rule. OSB
    /// `PricelistTier`, Odoo `product.pricelist.item`.
    pub const PRICELIST_RULE: u16 = 0x020A;
    /// `unit_of_measure` (`0x020B`) — measurement unit. OSB `UoM`, Odoo
    /// `uom.uom` (`qudt:Unit`).
    pub const UNIT_OF_MEASURE: u16 = 0x020B;

    // ── 0x08XX — OCR domain (document extraction; the Tesseract-rs arc) ──
    // Class-level container KINDS only: the concept slots name the container
    // types the Core resolves — never their content. The 112 unichars of a
    // trained unicharset are content-store rows, NOT concept slots (the
    // 0x07XX Osint zero-rows ruling is the guard precedent; unlike Osint,
    // OCR's container kinds ARE cross-app concepts and do get slots).

    /// `unicharset` (`0x0801`) — a character-set container: the trained
    /// unichar inventory a recognizer resolves against (Tesseract
    /// `UNICHARSET`). The unichars themselves are content rows.
    pub const UNICHARSET: u16 = 0x0801;
    /// `recoder` (`0x0802`) — a code-compression mapping between unichar ids
    /// and recognizer output codes (Tesseract `UnicharCompress`).
    pub const RECODER: u16 = 0x0802;
    /// `charset` (`0x0803`) — an encoding/character-repertoire declaration a
    /// document or model asserts (distinct from the trained `unicharset`
    /// inventory).
    pub const CHARSET: u16 = 0x0803;

    // ── 0x09XX — health domain (medcare-rs Healthcare namespace) ──

    /// `patient` (`0x0901`) — the person receiving care. OGIT
    /// `Healthcare:Patient`.
    pub const PATIENT: u16 = 0x0901;
    /// `diagnosis` (`0x0902`) — a clinical finding / condition. OGIT
    /// `Healthcare:Diagnosis`.
    pub const DIAGNOSIS: u16 = 0x0902;
    /// `lab_value` (`0x0903`) — a laboratory measurement. OGIT
    /// `Healthcare:LabValue`.
    pub const LAB_VALUE: u16 = 0x0903;
    /// `medication` (`0x0904`) — a prescribed / administered drug. OGIT
    /// `Healthcare:Medication`.
    pub const MEDICATION: u16 = 0x0904;
    /// `treatment` (`0x0905`) — a therapeutic intervention. OGIT
    /// `Healthcare:Treatment`.
    pub const TREATMENT: u16 = 0x0905;
    /// `visit` (`0x0906`) — a clinical encounter / episode. OGIT
    /// `Healthcare:Visit`.
    pub const VISIT: u16 = 0x0906;
    /// `vital_sign` (`0x0907`) — a measured vital. OGIT
    /// `Healthcare:VitalSign`.
    pub const VITAL_SIGN: u16 = 0x0907;

    // ── 0x0AXX — Anatomy domain (FMA reference ontology) ──

    /// `anatomical_structure` (`0x0A01`) — FMA's universal root kind
    /// (everything in the atlas `is-a` this). The abstract anchor of the
    /// anatomy partonomy.
    pub const ANATOMICAL_STRUCTURE: u16 = 0x0A01;
    /// `skeleton` (`0x0A02`) — the whole-body skeletal system; the root of
    /// the bone partonomy (`crates/ogar-fma-skeleton`).
    pub const SKELETON: u16 = 0x0A02;
    /// `bone` (`0x0A03`) — a skeletal element. **The clamped convergence
    /// anchor**: bones are the rigid, non-negotiable reference frame the
    /// splat-fit registers against (`docs/FMA-SKELETON-CONVERGENCE-ANCHOR.md`).
    /// The ~206 individual bones are cascade-path nodes under this concept,
    /// not separate codebook slots.
    pub const BONE: u16 = 0x0A03;
    /// `joint` (`0x0A04`) — an articulation between bones (the skeletal
    /// graph's edges, when materialized).
    pub const JOINT: u16 = 0x0A04;

    // ── 0x0BXX — Auth domain (IAM; the AuthStore class family) ──

    /// `auth_store` (`0x0B01`) — the IdP→classid mapping base class. Does
    /// the mapping: `sub` → actor (`0x0104`), role-key → role (`0x0117`),
    /// org/tenant → scope. The canonical OGIT shape confirms it: arago's
    /// `NTO/Auth/Configuration` entity (keyed by organization/account/
    /// application/scope IDs) is this class, built upstream. See
    /// `docs/CLASSID-RBAC-KEYSTONE-SPEC.md` §7.
    pub const AUTH_STORE: u16 = 0x0B01;
    /// `auth_zitadel` (`0x0B02`) — Zitadel provider profile (`is-a`
    /// `auth_store`). Maps 1:1: Project→class scope, Project-Role→role,
    /// Authorization/Grant→membership tuple, Organization→scope, User→`sub`.
    pub const AUTH_ZITADEL: u16 = 0x0B02;
    /// `auth_zanzibar` (`0x0B03`) — Google Zanzibar / OpenFGA provider
    /// profile (`object#relation@subject` tuple grammar).
    pub const AUTH_ZANZIBAR: u16 = 0x0B03;
    /// `auth_ory_keto` (`0x0B04`) — Ory Keto provider profile.
    pub const AUTH_ORY_KETO: u16 = 0x0B04;

    // ── 0x0DXX — HR domain (employment / org / contracts) ──

    /// `hr_employee` (`0x0D01`) — person record. OSB `Employee`, Odoo
    /// `hr.employee` (`vcard:Individual`).
    pub const HR_EMPLOYEE: u16 = 0x0D01;
    /// `hr_department` (`0x0D02`) — organizational unit (sub-tree of an
    /// organization). OSB `Department`, Odoo `hr.department`
    /// (`org:OrganizationalUnit`).
    pub const HR_DEPARTMENT: u16 = 0x0D02;
    /// `hr_job` (`0x0D03`) — role / position. OSB `Job`, Odoo `hr.job`
    /// (`org:Role`).
    pub const HR_JOB: u16 = 0x0D03;
    /// `hr_employment_contract` (`0x0D04`) — base employment contract.
    /// OSB `Contract`, Odoo `hr.contract` (`fibo:Contract`). Payroll
    /// computation stays outside the codebook (Odoo Enterprise / OSB
    /// add-on territory).
    pub const HR_EMPLOYMENT_CONTRACT: u16 = 0x0D04;

    // ── 0x0CXX — Automation domain (HIRO IT-automation stack) ──

    /// `mars_application` (`0x0C01`) — a MARS Application CMDB entity; head of
    /// the A→R→S→M `dependsOn` backbone (`ogit.MARS:Application`).
    pub const MARS_APPLICATION: u16 = 0x0C01;
    /// `mars_resource` (`0x0C02`) — a MARS Resource (`ogit.MARS:Resource`).
    pub const MARS_RESOURCE: u16 = 0x0C02;
    /// `mars_software` (`0x0C03`) — a MARS Software component
    /// (`ogit.MARS:Software`).
    pub const MARS_SOFTWARE: u16 = 0x0C03;
    /// `mars_machine` (`0x0C04`) — a MARS Machine; tail of the A→R→S→M chain
    /// (`ogit.MARS:Machine`).
    pub const MARS_MACHINE: u16 = 0x0C04;
    /// `knowledge_item` (`0x0C05`) — the Automation KnowledgeItem; the DO-arm
    /// `ActionDef` carrier (`ogit.Automation:KnowledgeItem`). Its opaque body
    /// rides in `knowledgeItemFormalRepresentation` — pointed-to, never inlined
    /// (lossless-DO; `docs/HIRO-DO-ARM-LIFT.md` §1).
    pub const KNOWLEDGE_ITEM: u16 = 0x0C05;
    /// `mars_node_template` (`0x0C06`) — the template a KnowledgeItem
    /// `relates` to; the DO-arm `ActionDef.object_class`
    /// (`ogit.Automation:MARSNodeTemplate`).
    pub const MARS_NODE_TEMPLATE: u16 = 0x0C06;
    /// `action_handler` (`0x0C07`) — the ActionHandler adapter/membrane that
    /// `provides` Applicability + Capability (`ogit.Automation:ActionHandler`).
    /// Where the DO arm and the auth/RBAC arm meet (`HIRO-DO-ARM-LIFT.md` §3).
    pub const ACTION_HANDLER: u16 = 0x0C07;
    /// `action_applicability` (`0x0C08`) — the ActionApplicability; its
    /// `environmentFilter` is the DO-arm `KausalSpec::StateGuard`
    /// (`ogit.Automation:ActionApplicability`).
    pub const ACTION_APPLICABILITY: u16 = 0x0C08;
    /// `automation_trigger` (`0x0C09`) — the Trigger a KnowledgeItem
    /// `contains`; the DO-arm `KausalSpec::LifecycleTrigger`
    /// (`ogit.Automation:Trigger`).
    pub const AUTOMATION_TRIGGER: u16 = 0x0C09;

    // ── 0x07XX — OSINT domain: no concept constants (low byte = APPID,
    // domain-wise; q2 = 0x01 → `0x0701` is OSINT-for-q2, not a concept —
    // operator ruling 2026-07-02; see the CODEBOOK section note). ──

    /// Every `(canonical_concept_name, id)` pair the constants vouch for.
    /// Drift-guarded against [`super::CODEBOOK`] by tests in this module.
    pub const ALL: &[(&str, u16)] = &[
        // 0x01XX — project-mgmt
        ("project", PROJECT),
        ("project_work_item", PROJECT_WORK_ITEM),
        ("billable_work_entry", BILLABLE_WORK_ENTRY),
        ("project_actor", PROJECT_ACTOR),
        ("project_status", PROJECT_STATUS),
        ("project_type", PROJECT_TYPE),
        ("priority", PRIORITY),
        ("project_membership", PROJECT_MEMBERSHIP),
        ("project_journal", PROJECT_JOURNAL),
        ("project_repository", PROJECT_REPOSITORY),
        ("project_version", PROJECT_VERSION),
        ("project_wiki_page", PROJECT_WIKI_PAGE),
        ("project_query", PROJECT_QUERY),
        ("project_attachment", PROJECT_ATTACHMENT),
        ("project_comment", PROJECT_COMMENT),
        ("project_custom_field", PROJECT_CUSTOM_FIELD),
        ("project_relation", PROJECT_RELATION),
        ("project_changeset", PROJECT_CHANGESET),
        ("project_watcher", PROJECT_WATCHER),
        ("project_news", PROJECT_NEWS),
        ("project_message", PROJECT_MESSAGE),
        ("project_forum", PROJECT_FORUM),
        ("project_role", PROJECT_ROLE),
        ("project_member_role", PROJECT_MEMBER_ROLE),
        ("project_custom_value", PROJECT_CUSTOM_VALUE),
        ("project_enabled_module", PROJECT_ENABLED_MODULE),
        // 0x02XX — commerce
        ("commercial_line_item", COMMERCIAL_LINE_ITEM),
        ("commercial_document", COMMERCIAL_DOCUMENT),
        ("tax_policy", TAX_POLICY),
        ("billing_party", BILLING_PARTY),
        ("payment_record", PAYMENT_RECORD),
        ("currency_policy", CURRENCY_POLICY),
        ("product", PRODUCT),
        ("accounting_account", ACCOUNTING_ACCOUNT),
        ("pricelist", PRICELIST),
        ("pricelist_rule", PRICELIST_RULE),
        ("unit_of_measure", UNIT_OF_MEASURE),
        // 0x07XX — OSINT: ZERO vocabulary rows BY DESIGN (operator ruling
        // 2026-07-02; see the CODEBOOK 0x07XX section note). No entries
        // follow — OGAR vocabulary carries no OSINT concept names.
        // 0x08XX — OCR (container kinds only; unichar content stays out)
        ("unicharset", UNICHARSET),
        ("recoder", RECODER),
        ("charset", CHARSET),
        // 0x09XX — health
        ("patient", PATIENT),
        ("diagnosis", DIAGNOSIS),
        ("lab_value", LAB_VALUE),
        ("medication", MEDICATION),
        ("treatment", TREATMENT),
        ("visit", VISIT),
        ("vital_sign", VITAL_SIGN),
        // 0x0AXX — anatomy (FMA reference ontology)
        ("anatomical_structure", ANATOMICAL_STRUCTURE),
        ("skeleton", SKELETON),
        ("bone", BONE),
        ("joint", JOINT),
        // 0x0BXX — auth (AuthStore class family)
        ("auth_store", AUTH_STORE),
        ("auth_zitadel", AUTH_ZITADEL),
        ("auth_zanzibar", AUTH_ZANZIBAR),
        ("auth_ory_keto", AUTH_ORY_KETO),
        // 0x0DXX — HR (employment / org / contracts; closes the final
        // 4-of-11 cross-axis gap from odoo-rs PR #14)
        ("hr_employee", HR_EMPLOYEE),
        ("hr_department", HR_DEPARTMENT),
        ("hr_job", HR_JOB),
        ("hr_employment_contract", HR_EMPLOYMENT_CONTRACT),
        // 0x0CXX — automation (HIRO IT-automation: MARS CMDB + actuators)
        ("mars_application", MARS_APPLICATION),
        ("mars_resource", MARS_RESOURCE),
        ("mars_software", MARS_SOFTWARE),
        ("mars_machine", MARS_MACHINE),
        ("knowledge_item", KNOWLEDGE_ITEM),
        ("mars_node_template", MARS_NODE_TEMPLATE),
        ("action_handler", ACTION_HANDLER),
        ("action_applicability", ACTION_APPLICABILITY),
        ("automation_trigger", AUTOMATION_TRIGGER),
    ];

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{CODEBOOK, canonical_concept_id};

        #[test]
        fn constants_match_codebook() {
            // Forward gate: every (name, const) in ALL agrees with the
            // CODEBOOK lookup for the same canonical_concept name. A new
            // CODEBOOK promotion that changes an id (which the codebook
            // contract forbids — ids never re-move) fails THIS test before
            // any consumer breaks.
            for (name, id) in ALL {
                assert_eq!(
                    canonical_concept_id(name),
                    Some(*id),
                    "{name}: constant 0x{id:04X} disagrees with CODEBOOK lookup",
                );
            }
        }

        #[test]
        fn every_codebook_entry_has_a_const() {
            // Reverse gate: if the CODEBOOK promotes a new concept, ALL
            // (and the const block above) must learn it. Catches the
            // "promotion landed without exposing a const" case.
            let known: std::collections::HashSet<&str> = ALL.iter().map(|(n, _)| *n).collect();
            for (name, _id) in CODEBOOK {
                assert!(
                    known.contains(name),
                    "{name} promoted in CODEBOOK but missing from class_ids::ALL",
                );
            }
        }

        #[test]
        fn constants_are_unique_and_non_zero() {
            use std::collections::HashSet;
            let mut seen = HashSet::new();
            for (name, id) in ALL {
                assert_ne!(*id, 0, "{name}: id must be non-zero (0x0000 reserved)");
                assert!(seen.insert(*id), "duplicate id 0x{id:04X} (saw at {name})");
            }
        }

        #[test]
        fn count_fuse_matches_lance_graph_ogar_mirror() {
            // OGAR-side half of the two-sided drift fuse. The lance-graph
            // side half is `lance_graph_ogar::parity::COUNT_FUSE`
            // (lance-graph `crates/lance-graph-ogar/src/lib.rs:119`), a
            // compile-time assert that
            // `lance_graph_contract::ogar_codebook::CODEBOOK.len() ==
            // ogar_vocab::class_ids::ALL.len()`. That fuse only fires if
            // OGAR's count changes without the contract mirror following —
            // it cannot detect the mirror itself drifting, because it has
            // no independent number to compare against on the OGAR side.
            // Pin the number here too, so a change to this count is visible
            // in THIS repo's CI the moment it happens, not only when the
            // lance-graph mirror is rebuilt against it.
            assert_eq!(
                ALL.len(),
                68,
                "class_ids::ALL count changed — update this pin AND the \
                 lance-graph mirror COUNT_FUSE (crates/lance-graph-ogar/src/lib.rs) \
                 in the same PR",
            );
        }

        #[test]
        fn divergent_curator_names_share_one_constant() {
            // The whole point of the codebook, in code: a Redmine Issue
            // and an OpenProject WorkPackage both route on the SAME arm.
            assert_eq!(PROJECT_WORK_ITEM, 0x0102);
            assert_eq!(PROJECT_STATUS, 0x0105);
            assert_eq!(PROJECT_TYPE, 0x0106);
            assert_eq!(PROJECT_FORUM, 0x0116);
            assert_eq!(BILLABLE_WORK_ENTRY, 0x0103);
        }
    }
}

// Per-port specifications consumed by `lance_graph_ontology::UnifiedBridge`.
// One module per port-vs-port concern stays here at the canonical-layer level
// so the bridge harness in lance-graph stays generic.
pub mod ports;

// APP‖class composition — the high-u16 render-prefix machinery
// (APP-CLASS-CODEBOOK-LAYOUT.md §0/§4). Builds on `PortSpec::APP_PREFIX`.
pub mod app;

/// **Cross-domain bridge concepts** — promoted concepts whose convergence
/// is intentionally *across* domains, so they must be exempt from the
/// domain gate in [`canonical_concept_in_domain`].
///
/// A bridge concept owns one home domain via its codebook id (high byte),
/// but curators in *other* domains legitimately map onto it — that shared
/// node identity is the whole point of the convergence. `billable_work_entry`
/// (`0x0103`, project-mgmt home) is the canonical example: OpenProject
/// `TimeEntry` (project), Odoo `account.analytic.line` (erp), and WoA
/// `Arbeitszeit` (german-erp) all converge here. Gating it by home domain
/// would sever the erp/german-erp witnesses and destroy the bridge.
///
/// Everything else (`project_role`, `commercial_document`, …) is
/// domain-specific: a collision from a foreign domain is an accident, not a
/// bridge, and is withheld.
const CROSS_DOMAIN_CONCEPTS: &[&str] = &["billable_work_entry"];

/// Whether a canonical concept is a [`CROSS_DOMAIN_CONCEPTS`] bridge —
/// intentionally shared across domains and therefore never domain-gated.
#[must_use]
pub fn is_cross_domain_concept(concept: &str) -> bool {
    CROSS_DOMAIN_CONCEPTS.contains(&concept)
}

/// **Consumer-facing label DTO** — `(label, id, canonical)` triple. The
/// three fields cover the three roles a class identity plays:
///
/// - `label` — **consumer-local** name (curator surface like `"Issue"` /
///   `"account.analytic.line"`, or a domain-specific tag). Not normalised
///   by OGAR.
/// - `id` — **binary codebook identity** ([`ogar_codebook`] of `label`).
///   The actual identity used for set-equality, lookup, dispatch. Two
///   consumers with different labels for the same concept produce DTOs
///   with different `label`s and equal `id`s.
/// - `canonical` — **canonical-AST label** ([`canonical_concept`] of
///   `label`). The portable symbol used by AST consumers (SurrealDB AST,
///   lance-graph-planner, kanban, …) when they need a stable
///   curator-agnostic name. AST emission picks this; identity comparison
///   picks `id`; presentation picks `label`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct LabelDTO {
    /// Consumer-local label. Not normalised by OGAR.
    pub label: String,
    /// OGAR codebook binary identity.
    pub id: u16,
    /// Canonical-AST label — the portable symbol AST / planner / kanban
    /// consumers emit when they need a stable curator-agnostic name.
    pub canonical: String,
}

impl LabelDTO {
    /// Build a `LabelDTO` from a consumer-shaped alias. The OGAR codebook
    /// resolves the alias to its canonical `u16` id without normalising
    /// the `label` itself — `"account.analytic.line"` stays
    /// `"account.analytic.line"`, but its `id` is the same as the id for
    /// `"TimeEntry"` and for `"billable_work_entry"`, and its `canonical`
    /// is `"billable_work_entry"` ready for AST emission.
    ///
    /// Returns `None` when `label` does not resolve to a promoted
    /// canonical concept in the [`CODEBOOK`] — unknown labels have no
    /// codebook identity (they are not in the registry).
    #[must_use]
    pub fn from_alias(label: impl Into<String>) -> Option<Self> {
        let label = label.into();
        let canonical = canonical_concept(&label);
        let id = canonical_concept_id(&canonical)?;
        Some(Self {
            label,
            id,
            canonical,
        })
    }

    /// `id` rendered as **2 little-endian bytes** — the wire contract for
    /// downstream consumers. Roundtrip via `u16::from_le_bytes`.
    #[must_use]
    pub fn id_le(&self) -> [u8; 2] {
        self.id.to_le_bytes()
    }
}

/// **OGAR codebook lookup** — map any alias (curator-shaped *or*
/// canonical-shaped) to its canonical binary id. The curator name does
/// not need to be normalised by the producer; passing the raw Rails or
/// Odoo class name yields the same `u16` as the canonical-concept string.
///
/// ```text
///   ogar_codebook("Issue")                     == codebook("project_work_item")
///   ogar_codebook("WorkPackage")               == codebook("project_work_item")
///   ogar_codebook("TimeEntry")                 == codebook("billable_work_entry")
///   ogar_codebook("account.analytic.line")     == codebook("billable_work_entry")
///   ogar_codebook("Project")                   == codebook("project")
/// ```
///
/// Implementation: resolves the alias through [`canonical_concept`]
/// (which carries the promoted-invariant table) and looks the result up
/// in the [`CODEBOOK`] registry via [`canonical_concept_id`]. Returns
/// `None` when the resolved concept is not in the codebook — unknown
/// aliases have no codebook identity.
#[must_use]
pub fn ogar_codebook(alias: &str) -> Option<u16> {
    canonical_concept_id(&canonical_concept(alias))
}

impl Association {
    /// Build a new association with kind and name set.
    #[must_use]
    pub fn new(kind: AssociationKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            ..Default::default()
        }
    }
}

impl EnumDecl {
    /// Build a new enum declaration with the column set.
    #[must_use]
    pub fn new(column: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            ..Default::default()
        }
    }
}

impl StoreAccessor {
    /// Build a new store-accessor bundle with the JSONB column set.
    #[must_use]
    pub fn new(column: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            ..Default::default()
        }
    }
}

impl Attribute {
    /// Build a new attribute override with the name set.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
}

impl ComputedField {
    /// Build a new computed-field declaration with the field name and its
    /// compute method set. Remaining metadata (`depends`, `stored`,
    /// `inverse_method`, …) is filled in by the caller.
    #[must_use]
    pub fn new(field: impl Into<String>, compute_method: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            compute_method: compute_method.into(),
            ..Default::default()
        }
    }
}

impl Scope {
    /// Build a new scope with name and body source.
    #[must_use]
    pub fn new(name: impl Into<String>, body_source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            body_source: body_source.into(),
        }
    }
}

impl Callback {
    /// Build a new method-form callback: `before_save :method_name`.
    #[must_use]
    pub fn method(event: impl Into<String>, target_method: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            target_method: Some(target_method.into()),
            body_source: None,
        }
    }

    /// Build a new block-form callback: `after_create do ... end`.
    #[must_use]
    pub fn block(event: impl Into<String>, body_source: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            target_method: None,
            body_source: Some(body_source.into()),
        }
    }
}

impl Validation {
    /// Build a new validation rule with target column and rule body.
    #[must_use]
    pub fn new(target: impl Into<String>, rule_source: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            rule_source: rule_source.into(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Cross-domain synergies
// ─────────────────────────────────────────────────────────────────────

/// A cross-domain **synergy**: one canonical concept that surfaces in two
/// or more curator [domains](Class::source_domain) — e.g. `user` in both
/// the `project` domain (OpenProject `User`) and the `erp` domain (Odoo
/// `res.users`). Wiring synergies is what makes the agnostic vocab more
/// than the sum of its curators: shared concepts unify across domains.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Synergy {
    /// Canonical concept (normalized class name) the members share.
    pub concept: String,
    /// The classes that realize this concept — one entry per domain that
    /// has it, ordered by domain.
    pub members: Vec<SynergyMember>,
}

/// One domain's realization of a [`Synergy`] concept.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct SynergyMember {
    /// The curator domain (`"project"`, `"erp"`, …) — see
    /// [`Class::source_domain`].
    pub domain: String,
    /// The class name as written in that domain.
    pub class_name: String,
}

/// Wire cross-domain synergies across a set of lifted [`Class`]es.
///
/// Groups classes by [`canonical_concept`] and keeps only concepts that
/// appear in **2+ distinct** [`source_domain`](Class::source_domain)s —
/// those bridges are the synergies. Classes with no `source_domain` are
/// skipped (a synergy needs domains to bridge); the first class seen per
/// (concept, domain) wins. Output is deterministic (ordered by concept,
/// then domain).
#[must_use]
pub fn wire_synergies(classes: &[Class]) -> Vec<Synergy> {
    use std::collections::BTreeMap;
    let mut by_concept: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for c in classes {
        let Some(domain) = c.source_domain.as_ref() else {
            continue;
        };
        // Prefer the concept the producer stored; else compute it
        // deterministically from the name — so any consumer session
        // rediscovers the same bridge from ontology surfaces alone.
        let concept = c
            .canonical_concept
            .clone()
            .unwrap_or_else(|| canonical_concept(&c.name));
        by_concept
            .entry(concept)
            .or_default()
            .entry(domain.clone())
            .or_insert_with(|| c.name.clone());
    }
    by_concept
        .into_iter()
        .filter(|(_, domains)| domains.len() >= 2)
        .map(|(concept, domains)| Synergy {
            concept,
            members: domains
                .into_iter()
                .map(|(domain, class_name)| SynergyMember { domain, class_name })
                .collect(),
        })
        .collect()
}

/// Resolve a class name to its canonical OGAR **concept**.
///
/// Two layers, in order:
/// 1. **Promoted cross-domain invariants** — concepts a Claude Code
///    convergence pass has proven across 2+ domains and promoted into
///    OGAR. OGAR stores only the stable result; the proof is the test, the
///    "finding" was the PR that added the arm. (No `SynergyKind` /
///    `SynergyFinding` taxonomy — convergence is an operation, not a
///    stored object.)
/// 2. **Lexical fallback** — lowercase, last dotted segment (Odoo
///    `res.users` → `users`), drop a single trailing plural `s` except
///    after `ss`. Coarse by design, not a thesaurus.
#[must_use]
pub fn canonical_concept(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    // ── Layer 1: promoted invariants ──
    // BillableWorkEntry — booked work / time / cost against a project or
    // order (see [`billable_work_entry`]). OpenProject `TimeEntry` ↔ Odoo
    // `account.analytic.line` ↔ WoA `Arbeitszeit`. ruff normalizes Odoo
    // dots to underscores, so match both forms.
    if matches!(
        lower.as_str(),
        "timeentry"
            | "time_entry"
            | "timeentries"
            | "time_entries"
            | "account.analytic.line"
            | "account_analytic_line"
            | "leistungsposition"
            | "arbeitszeit"
            // canonical class-name spellings (codex P2 on PR #60):
            // `billable_work_entry().name` is "BillableWorkEntry" so the
            // canonical class must round-trip to its own codebook id.
            | "billable_work_entry"
            | "billableworkentry"
    ) {
        return "billable_work_entry".to_string();
    }
    // ProjectWorkItem — project-scoped work items with status, assignment,
    // author, type/tracker, journals, relations, time tracking. The
    // Redmine `Issue` and OpenProject `WorkPackage` overlap (the fork
    // lineage Redmine → ChiliProject → OpenProject preserves the
    // invariant) — both lift here regardless of OpenProject's later
    // modular enrichment. See [`project_work_item`].
    if matches!(
        lower.as_str(),
        "issue"
            | "workpackage"
            | "work_package"
            // canonical class-name spellings (codex P2 on PR #60).
            | "project_work_item"
            | "projectworkitem"
    ) {
        return "project_work_item".to_string();
    }
    // Project — the root container of project-domain work. Both Redmine
    // and OpenProject use `Project`; explicit promotion (rather than
    // relying on lexical fallback) so the canonical class name round-trips
    // (codex P2 on PR #60).
    if matches!(lower.as_str(), "project" | "projects") {
        return "project".to_string();
    }
    // ProjectActor — the actor canonical concept on the project domain.
    // Both Redmine and OpenProject use `User < Principal < ApplicationRecord`;
    // the STI chain yields multiple classes (Principal at the root, User
    // and Group as STI children) that ALL project to the same actor
    // identity — a Group is a Principal subtype, assignable / member-able
    // exactly where a User is, so it collapses here too (its `has_many
    // :users` aggregation is curator-local structure the canonical layer
    // abstracts over). Plus the canonical class-name spellings for round-trip.
    if matches!(
        lower.as_str(),
        "user"
            | "users"
            | "principal"
            | "principals"
            | "group"
            | "groups"
            | "project_actor"
            | "projectactor"
    ) {
        return "project_actor".to_string();
    }
    // ProjectStatus — the workflow-state lookup. Redmine `IssueStatus`,
    // OpenProject `Status`. Cross-curator name divergence; same concept.
    if matches!(
        lower.as_str(),
        "issuestatus"
            | "issue_status"
            | "issuestatuses"
            | "issue_statuses"
            | "status"
            | "statuses"
            | "project_status"
            | "projectstatus"
    ) {
        return "project_status".to_string();
    }
    // ProjectType — the work-item categorisation lookup. Redmine `Tracker`,
    // OpenProject `Type`. Cross-curator name divergence; same concept.
    if matches!(
        lower.as_str(),
        "tracker" | "trackers" | "type" | "types" | "project_type" | "projecttype"
    ) {
        return "project_type".to_string();
    }
    // ── Project-mgmt promotions from the cross-curator overlap probe ──
    if matches!(
        lower.as_str(),
        "member"
            | "members"
            | "membership"
            | "memberships"
            | "project_membership"
            | "projectmembership"
    ) {
        return "project_membership".to_string();
    }
    if matches!(
        lower.as_str(),
        "journal" | "journals" | "project_journal" | "projectjournal"
    ) {
        return "project_journal".to_string();
    }
    if matches!(
        lower.as_str(),
        "repository" | "repositories" | "project_repository" | "projectrepository"
    ) {
        return "project_repository".to_string();
    }
    if matches!(
        lower.as_str(),
        "version" | "versions" | "project_version" | "projectversion"
    ) {
        return "project_version".to_string();
    }
    if matches!(
        lower.as_str(),
        "wikipage"
            | "wiki_page"
            | "wikipages"
            | "wiki_pages"
            | "project_wiki_page"
            | "projectwikipage"
    ) {
        return "project_wiki_page".to_string();
    }
    if matches!(
        lower.as_str(),
        "query" | "queries" | "project_query" | "projectquery"
    ) {
        return "project_query".to_string();
    }
    if matches!(
        lower.as_str(),
        "attachment" | "attachments" | "project_attachment" | "projectattachment"
    ) {
        return "project_attachment".to_string();
    }
    if matches!(
        lower.as_str(),
        "comment" | "comments" | "project_comment" | "projectcomment"
    ) {
        return "project_comment".to_string();
    }
    if matches!(
        lower.as_str(),
        "customfield"
            | "custom_field"
            | "customfields"
            | "custom_fields"
            | "project_custom_field"
            | "projectcustomfield"
    ) {
        return "project_custom_field".to_string();
    }
    // ProjectRelation — work-item ↔ work-item edge. Cross-curator name
    // divergence: Redmine `IssueRelation`, OpenProject `Relation`. Same
    // concept (precedes/blocks/relates_to between two work items).
    if matches!(
        lower.as_str(),
        "issuerelation"
            | "issue_relation"
            | "issuerelations"
            | "issue_relations"
            | "relation"
            | "relations"
            | "project_relation"
            | "projectrelation"
    ) {
        return "project_relation".to_string();
    }
    // ProjectChangeset — VCS commit on a [`project_repository`]. Both
    // curators ship `Changeset` with the same shape (belongs_to repository
    // + user, revision/comments/commit_date).
    if matches!(
        lower.as_str(),
        "changeset" | "changesets" | "project_changeset" | "projectchangeset"
    ) {
        return "project_changeset".to_string();
    }
    // ProjectWatcher — the per-user follow-relationship on a polymorphic
    // watchable. Both curators ship `Watcher` with `belongs_to :user` +
    // `belongs_to :watchable, polymorphic: true`.
    if matches!(
        lower.as_str(),
        "watcher" | "watchers" | "project_watcher" | "projectwatcher"
    ) {
        return "project_watcher".to_string();
    }
    // ProjectNews — project news/blog post. Both curators ship `News`
    // with belongs_to :project + :author + has_many :comments. Same
    // shape across both.
    if matches!(lower.as_str(), "news" | "project_news" | "projectnews") {
        return "project_news".to_string();
    }
    // ProjectMessage — forum/board message (threaded discussion). Both
    // curators ship `Message`, but the parent container diverges in
    // name (Redmine `Board`, OpenProject `Forum`).
    if matches!(
        lower.as_str(),
        "message" | "messages" | "project_message" | "projectmessage"
    ) {
        return "project_message".to_string();
    }
    // ProjectForum — the parent container for [`project_message`].
    // Cross-curator name divergence: Redmine `Board`, OpenProject `Forum`
    // (same shape — project + last_message + name/description, with
    // has_many :messages).
    if matches!(
        lower.as_str(),
        "board" | "boards" | "forum" | "forums" | "project_forum" | "projectforum"
    ) {
        return "project_forum".to_string();
    }
    // ProjectRole — the RBAC permission-set bundle assigned to actors via
    // memberships. Both curators ship `Role` (has_many :member_roles +
    // :members through, a name, and a permission set). NOTE: distinct from
    // `ogar_from_ruff::project_role`, which maps a curator association
    // NAME to a ProjectWorkItem family-edge role — this is the
    // authorization concept (the `Role` model), not a graph-edge role.
    if matches!(
        lower.as_str(),
        "role" | "roles" | "project_role" | "projectrole"
    ) {
        return "project_role".to_string();
    }
    // ProjectMemberRole — the RBAC join (membership ↔ role). Both
    // curators ship `MemberRole` (belongs_to :member + :role).
    if matches!(
        lower.as_str(),
        "memberrole"
            | "member_role"
            | "memberroles"
            | "member_roles"
            | "project_member_role"
            | "projectmemberrole"
    ) {
        return "project_member_role".to_string();
    }
    // ProjectCustomValue — the value of a [`project_custom_field`] on a
    // record. Both curators ship `CustomValue` (belongs_to :custom_field
    // + polymorphic :customized).
    if matches!(
        lower.as_str(),
        "customvalue"
            | "custom_value"
            | "customvalues"
            | "custom_values"
            | "project_custom_value"
            | "projectcustomvalue"
    ) {
        return "project_custom_value".to_string();
    }
    // ProjectEnabledModule — per-project module enablement. Both curators
    // ship `EnabledModule` (belongs_to :project + a name).
    if matches!(
        lower.as_str(),
        "enabledmodule"
            | "enabled_module"
            | "enabledmodules"
            | "enabled_modules"
            | "project_enabled_module"
            | "projectenabledmodule"
    ) {
        return "project_enabled_module".to_string();
    }
    // ── Commerce / billing / ERP domain (OSB ↔ Odoo) ──
    // CommercialLineItem — line on a commercial document. OSB
    // `InvoiceLineItem`, Odoo `account_move_line` (ruff normalises
    // Odoo dots to underscores; match both forms).
    if matches!(
        lower.as_str(),
        "invoicelineitem"
            | "invoice_line_item"
            | "account.move.line"
            | "account_move_line"
            | "commercial_line_item"
            | "commerciallineitem"
    ) {
        return "commercial_line_item".to_string();
    }
    // CommercialDocument — invoice / posting head. OSB `Invoice`,
    // Odoo `account_move` (covers invoices + credit notes + journal
    // entries; the head shape is promoted, not the variant).
    if matches!(
        lower.as_str(),
        "invoice"
            | "invoices"
            | "account.move"
            | "account_move"
            | "commercial_document"
            | "commercialdocument"
    ) {
        return "commercial_document".to_string();
    }
    // TaxPolicy — tax rate / classification. OSB `Tax`, Odoo
    // `account_tax`. Also the `BillableWorkEntry.classified_by` family
    // edge target.
    if matches!(
        lower.as_str(),
        "tax" | "taxes" | "account.tax" | "account_tax" | "tax_policy" | "taxpolicy"
    ) {
        return "tax_policy".to_string();
    }
    // BillingParty — counterparty (customer / vendor / partner). OSB
    // `Client`, Odoo `res_partner`.
    if matches!(
        lower.as_str(),
        "client" | "clients" | "res.partner" | "res_partner" | "billing_party" | "billingparty"
    ) {
        return "billing_party".to_string();
    }
    // PaymentRecord — payment event. OSB `Payment`, Odoo `account_payment`.
    if matches!(
        lower.as_str(),
        "payment"
            | "payments"
            | "account.payment"
            | "account_payment"
            | "payment_record"
            | "paymentrecord"
    ) {
        return "payment_record".to_string();
    }
    // CurrencyPolicy — currency lookup. OSB `Currency`, Odoo `res_currency`.
    if matches!(
        lower.as_str(),
        "currency"
            | "currencies"
            | "res.currency"
            | "res_currency"
            | "currency_policy"
            | "currencypolicy"
    ) {
        return "currency_policy".to_string();
    }
    // Priority — referenced by project_work_item().priority. Both Redmine
    // and OpenProject use `IssuePriority < Enumeration`; lexical fallback
    // would shuttle bare `Priority`/`Priorities` to `priority` already,
    // but `IssuePriority` lexically lands on `issuepriority` which is NOT
    // in the codebook — must promote explicitly so it collapses.
    if matches!(
        lower.as_str(),
        "issuepriority"
            | "issue_priority"
            | "issuepriorities"
            | "issue_priorities"
            | "priority"
            | "priorities"
    ) {
        return "priority".to_string();
    }
    // ── Layer 2: lexical fallback ──
    lexical_concept(name)
}

/// The **lexical fallback** half of [`canonical_concept`]: lowercase, take
/// the last dotted segment (Odoo `res.users` → `users`), then drop a single
/// trailing plural `s` (except after `ss`). Coarse by design — **no
/// codebook promotion happens here**, so the result is not guaranteed to be
/// in [`CODEBOOK`]. Exposed so [`canonical_concept_in_domain`] can fall
/// back to it when a cross-domain promotion is withheld.
#[must_use]
pub fn lexical_concept(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let last = lower.rsplit('.').next().unwrap_or(lower.as_str());
    if last.len() > 3 && last.ends_with('s') && !last.ends_with("ss") {
        last[..last.len() - 1].to_string()
    } else {
        last.to_string()
    }
}

/// Domain-gated [`canonical_concept`]. Resolves `name`, but **withholds a
/// codebook promotion whose [`ConceptDomain`] does not match the curator's
/// `domain`**, falling back to [`lexical_concept`] instead. This is the
/// resolver a producer should use once it knows the curator's domain (via
/// [`source_domain_concept`]).
///
/// Without the gate, any name colliding with a promoted alias — the codex
/// P2 example is a generic `Role` — inherits a project-mgmt codebook id and
/// routes as [`ConceptDomain::ProjectMgmt`] even when harvested from an
/// unrelated app (PR #72).
///
/// Resolution, in order:
/// - resolved concept is a **cross-domain bridge**
///   ([`is_cross_domain_concept`], e.g. `billable_work_entry`) → keep it
///   regardless of domain; these promotions are intentionally shared.
/// - `domain == Some(d)` and the promotion's codebook domain is `d` → keep.
/// - `domain == Some(other)` (cross-domain collision) → withhold → lexical.
/// - `domain == None` (curator domain unknown) → withhold → lexical: a
///   promotion we cannot vouch for is worse than a coarse lexical concept.
/// - resolved concept is already lexical (not in [`CODEBOOK`]) → unchanged.
#[must_use]
pub fn canonical_concept_in_domain(name: &str, domain: Option<ConceptDomain>) -> String {
    let concept = canonical_concept(name);
    match canonical_concept_id(&concept) {
        Some(_) if is_cross_domain_concept(&concept) => concept,
        Some(id) if domain == Some(canonical_concept_domain(id)) => concept,
        Some(_) => lexical_concept(name),
        None => concept,
    }
}

/// Every promoted canonical class, materialised once and returned in
/// [`class_ids::ALL`] order — the single enumerator a consumer drives to
/// touch all 32 promoted concepts at once.
///
/// # Why this exists
///
/// Until now the codebook exposed 32 separate constructor fns
/// (`project()`, `project_work_item()`, `billable_work_entry()`, …) but
/// no enumerator. A consumer that wanted to drive **every** promoted
/// concept (e.g. emit a SurrealQL schema covering all of them via
/// [`ogar_adapter_surrealql::emit_surrealql_ddl`](../../ogar-adapter-surrealql/src/lib.rs))
/// had to hand-list the 32 calls — drift-prone, breaks silently when a
/// new concept gets promoted.
///
/// `all_promoted_classes()` is the canonical answer:
///
/// ```ignore
/// use ogar_vocab::all_promoted_classes;
/// use ogar_adapter_surrealql::emit_surrealql_ddl;
///
/// let ddl = emit_surrealql_ddl(&all_promoted_classes());
/// // → one SurrealQL string for the full 32-concept schema, in
/// //   codebook (class_ids::ALL) order.
/// ```
///
/// # Order
///
/// Matches [`class_ids::ALL`] exactly. The test
/// [`tests::all_promoted_classes_matches_class_ids_all_order`] pins
/// this so a new codebook promotion that adds to `ALL` but forgets to
/// list a class fn here fails CI, and vice versa.
///
/// # Cost
///
/// Each call constructs 32 fresh `Class` values. Cheap (each is a
/// `Class::new(name)` + a few `Vec::push`) but not free; callers
/// caching the result for repeated reads is fine.
#[must_use]
pub fn all_promoted_classes() -> Vec<Class> {
    vec![
        // 0x01XX — project-mgmt arm (26 concepts).
        project(),
        project_work_item(),
        billable_work_entry(),
        project_actor(),
        project_status(),
        project_type(),
        priority(),
        project_membership(),
        project_journal(),
        project_repository(),
        project_version(),
        project_wiki_page(),
        project_query(),
        project_attachment(),
        project_comment(),
        project_custom_field(),
        project_relation(),
        project_changeset(),
        project_watcher(),
        project_news(),
        project_message(),
        project_forum(),
        project_role(),
        project_member_role(),
        project_custom_value(),
        project_enabled_module(),
        // 0x02XX — commerce arm (8 concepts: 6 OSB-promoted + 2
        //          Phase-3 mints per odoo-rs PR #14 + #16).
        commercial_line_item(),
        commercial_document(),
        tax_policy(),
        billing_party(),
        payment_record(),
        currency_policy(),
        product(),
        accounting_account(),
        pricelist(),
        pricelist_rule(),
        unit_of_measure(),
        // 0x07XX — OSINT arm: ZERO vocabulary rows BY DESIGN (operator
        // ruling 2026-07-02, corrects PR #145's hallucinated
        // `osint_system` / `osint_person` mints); no calls follow — OGAR
        // vocabulary carries no OSINT concept names, see the CODEBOOK
        // 0x07XX section note.
        // 0x08XX — OCR arm (3 container kinds), in class_ids::ALL order.
        unicharset(),
        recoder(),
        charset(),
        // 0x09XX — health arm (7 OGIT Healthcare concepts), in
        // class_ids::ALL order.
        patient(),
        diagnosis(),
        lab_value(),
        medication(),
        treatment(),
        visit(),
        vital_sign(),
        // 0x0AXX — anatomy arm (FMA reference kinds), in class_ids::ALL order.
        anatomical_structure(),
        skeleton(),
        bone(),
        joint(),
        // 0x0BXX — auth arm (the AuthStore class family, keystone §7),
        // in class_ids::ALL order.
        auth_store(),
        auth_zitadel(),
        auth_zanzibar(),
        auth_ory_keto(),
        // 0x0DXX — HR arm
        hr_employee(),
        hr_department(),
        hr_job(),
        hr_employment_contract(),
        // 0x0CXX — automation arm (HIRO MARS CMDB + DO-arm actuators),
        // in class_ids::ALL order.
        mars_application(),
        mars_resource(),
        mars_software(),
        mars_machine(),
        knowledge_item(),
        mars_node_template(),
        action_handler(),
        action_applicability(),
        automation_trigger(),
    ]
}

/// The promoted canonical class for the **first convergence invariant**:
/// booked work / time / cost against a project or order. The shared shape
/// under OpenProject `TimeEntry` (project domain), Odoo
/// `account.analytic.line` (erp domain), and WoA `Leistungsposition` /
/// `Arbeitszeit` (german-erp witness). Curators map in via
/// [`canonical_concept`] (`"billable_work_entry"`).
///
/// This is OGAR storing the *stable result* of a convergence pass — not a
/// synergy taxonomy.
///
/// # The 12 family edges (internal) + the adapter edge (external)
///
/// BillableWorkEntry carries **12 family edges** — relations to other
/// canonical concepts, internal to the ontology. The link from a curator
/// surface (OpenProject `TimeEntry`, Odoo `account.analytic.line`) is the
/// **adapter edge**, living *out of family* on the curator class (its
/// `source_domain` + `canonical_concept`), never among these edges.
///
/// **Tax is a boundary policy.** Three family edges — `classified_by →
/// TaxPolicy`, `materializes_as → InvoiceLineCandidate`, `posted_by →
/// PostingAction` — are populated only past the **ERP / posting
/// boundary**. The project domain records work evidence (`duration`,
/// `about → WorkPackage`, `performed_by → Worker`) and never applies tax.
///
/// `materializes_as` is `BelongsTo`, so many BillableWorkEntries aggregate
/// into one `InvoiceLineCandidate` (one invoice line, many work entries).
#[must_use]
pub fn billable_work_entry() -> Class {
    let mut c = Class::new("BillableWorkEntry");
    // Synthetic canonical class — NOT a Ruby-harvested model. Mark the
    // language neutral so the triple-emitter writes `ogar:sourceLanguage`
    // = `Unknown` and consumers do not route this through Ruby-specific
    // handling (codex P2 on OGAR#57).
    c.language = Language::Unknown;
    c.canonical_concept = Some("billable_work_entry".to_string());
    // The 12 family edges — internal ontology meaning. Every target is a
    // canonical concept (PascalCase), never a curator/adapter surface.
    c.associations = vec![
        family_edge("project", "Project"),
        // `about` targets the canonical project-work-item concept — NOT
        // `WorkPackage` (OP curator surface), so Redmine `Issue` and OP
        // `WorkPackage` converge here through their shared
        // `ProjectWorkItem` projection (codex P2 on OGAR#58).
        family_edge("about", "ProjectWorkItem"),
        family_edge("performed_by", "Worker"),
        family_edge("duration", "Duration"),
        family_edge("priced_by", "RatePolicy"),
        family_edge("cost_center", "CostCenter"),
        family_edge("classified_by", "TaxPolicy"), // ERP boundary
        family_edge("materializes_as", "InvoiceLineCandidate"), // ERP boundary
        family_edge("approval_state", "ApprovalState"),
        family_edge("tenant", "Tenant"),
        family_edge("audit_trail", "AuditTrail"),
        family_edge("posted_by", "PostingAction"), // ERP boundary
    ];
    // The defining flag — typed as boolean so DDL adapters that default
    // untyped fields to string-like columns generate the right schema
    // shape (codex P2 on OGAR#57).
    let mut billable = Attribute::new("billable");
    billable.type_name = Some("boolean".to_string());
    c.attributes = vec![billable];
    c
}

/// Build one BillableWorkEntry **family edge** — a `BelongsTo` relation to
/// a canonical ontology concept (the edge's `class_name`). Family edges
/// are internal; curator / adapter links never appear here.
fn family_edge(role: &str, target_concept: &str) -> Association {
    let mut a = Association::new(AssociationKind::BelongsTo, role);
    a.class_name = Some(target_concept.to_string());
    a
}

/// Build one **has-many** family edge — for canonical concepts that
/// aggregate (a [`project_work_item`] has-many `ProjectJournal`s, etc.).
fn family_has_many(role: &str, target_concept: &str) -> Association {
    let mut a = Association::new(AssociationKind::HasMany, role);
    a.class_name = Some(target_concept.to_string());
    a
}

/// The promoted canonical class for the **project-domain work-item
/// invariant**: project-scoped work with status, assignment, type/tracker,
/// priority, author, journals, relations, and time tracking.
///
/// The Redmine → ChiliProject → OpenProject lineage preserves this
/// invariant: Redmine `Issue` and OpenProject `WorkPackage` both map here
/// via [`canonical_concept`] (`"project_work_item"`). Curator labels
/// (`Tracker`, `Type`, `assigned_to`, `responsible`) are leaf details on
/// the curator class; only the canonical roles survive here.
///
/// The 9 family edges sit fully **inside the project domain** — no
/// ERP-boundary slots. The cross-domain bridge to billable work lives on
/// the `time_entries → BillableWorkEntry` has-many edge (project work
/// produces billable work; tax/posting happens past
/// [`billable_work_entry`]'s ERP-boundary edges).
#[must_use]
pub fn project_work_item() -> Class {
    let mut c = Class::new("ProjectWorkItem");
    // Synthetic canonical class — neutral language so the triple-emitter
    // does not route this through Ruby-specific handling. Same fix as
    // [`billable_work_entry`] (codex P2 on OGAR#57).
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_work_item".to_string());
    c.associations = vec![
        family_edge("project", "Project"),
        family_edge("status", "ProjectStatus"),
        family_edge("type", "ProjectType"), // Redmine Tracker / OP Type
        family_edge("priority", "Priority"),
        family_edge("author", "ProjectActor"),
        family_edge("assignee", "ProjectActor"), // Redmine assigned_to / OP assignee
        family_has_many("journals", "ProjectJournal"),
        family_has_many("relations", "ProjectRelation"),
        family_has_many("time_entries", "BillableWorkEntry"),
    ];
    c
}

/// The promoted canonical class for **project** — the root container of
/// project-domain work. Referenced by [`project_work_item`]'s `project`
/// family edge and [`billable_work_entry`]'s `project` edge; this is the
/// canonical class those edges resolve to.
///
/// Redmine `Project` and OpenProject `Project` are universal and share
/// the AR shape: nested-set `parent` (a project may belong to a parent
/// project), `members` (people on the project), the work items
/// themselves, and the time entries booked against the project. Both
/// curators carry `name` + `identifier` as the identity attributes.
///
/// The `work_items` family edge targets the canonical
/// [`project_work_item`] (not the curator surfaces Redmine `Issue` or OP
/// `WorkPackage`); `time_entries` targets [`billable_work_entry`]; the
/// `members` edge points forward at the still-to-come canonical
/// `ProjectActor`.
#[must_use]
pub fn project() -> Class {
    let mut c = Class::new("Project");
    // Synthetic canonical class — neutral language (codex P2 doctrine).
    c.language = Language::Unknown;
    c.canonical_concept = Some("project".to_string());
    c.associations = vec![
        family_has_many("work_items", "ProjectWorkItem"),
        family_has_many("time_entries", "BillableWorkEntry"),
        family_has_many("members", "ProjectActor"),
        // Nested-project parent is a real cross-curator concept but is
        // surfaced via MIXINS in both: Redmine threads it through the
        // `awesome_nested_set` gem (no direct `belongs_to`), OP through
        // the `Projects::Hierarchy` concern. The producer
        // (`ogar_ruby_spo`) does not yet decode either mixin into a
        // canonical parent edge — when it does, a follow-up PR adds
        // `family_edge("parent", "Project")` here and the matching
        // mixin-derived arm to `ogar_from_ruff::project_role_from_mixin`.
    ];
    // Identity attributes — both curators carry these as the canonical
    // human + URL identity.
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut identifier = Attribute::new("identifier");
    identifier.type_name = Some("string".to_string());
    c.attributes = vec![name, identifier];
    c
}

/// The promoted canonical class for **project actor** — the people /
/// groups who participate in project-domain work. Referenced by
/// [`project_work_item`]'s `author` / `assignee` edges and
/// [`project`]'s `members` edge; this is the canonical class those
/// edges resolve to.
///
/// Both Redmine and OpenProject model actors with the STI chain
/// `User < Principal < ApplicationRecord`, where Principal carries the
/// project-attachment relations (`has_many :members`, `:memberships`,
/// `:projects`-through-memberships) and User adds person-specific
/// preferences/tokens. Both `User` and `Principal` lift to this same
/// canonical concept via the resolver's promoted arm.
///
/// The single direct family edge is `projects` → `Project` (the
/// universal through-memberships relation both curators carry). Memberships
/// themselves wait on a future `Member` / `Membership` canonical class.
#[must_use]
pub fn project_actor() -> Class {
    let mut c = Class::new("ProjectActor");
    // Synthetic canonical class — neutral language (codex P2 doctrine).
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_actor".to_string());
    c.associations = vec![
        // Both Redmine and OP Principal carry
        // `has_many :projects, through: :memberships`.
        family_has_many("projects", "Project"),
    ];
    // Universal identity attributes. `type` is the STI discriminator
    // (`User`, `Group`, `AnonymousUser`, …); both curators expose it.
    let mut login = Attribute::new("login");
    login.type_name = Some("string".to_string());
    let mut sti_type = Attribute::new("type");
    sti_type.type_name = Some("string".to_string());
    c.attributes = vec![login, sti_type];
    c
}

/// The promoted canonical class for **project status** — the workflow
/// state lookup applied to project work items. Referenced by
/// [`project_work_item`]'s `status` family edge.
///
/// Cross-curator name divergence: Redmine `IssueStatus`, OpenProject
/// `Status`. Same concept, same canonical id (`0x0105`). Both curators
/// carry the universal `name` / `position` / `is_closed` triple (closed
/// = workflow terminal state) plus a `has_many :workflows` collection
/// of allowed transitions.
#[must_use]
pub fn project_status() -> Class {
    let mut c = Class::new("ProjectStatus");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_status".to_string());
    c.associations = vec![family_has_many("workflows", "WorkflowTransition")];
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut position = Attribute::new("position");
    position.type_name = Some("integer".to_string());
    let mut is_closed = Attribute::new("is_closed");
    is_closed.type_name = Some("boolean".to_string());
    c.attributes = vec![name, position, is_closed];
    c
}

/// The promoted canonical class for **project type** — the work-item
/// categorisation lookup. Referenced by [`project_work_item`]'s `type`
/// family edge.
///
/// Cross-curator name divergence: Redmine `Tracker`, OpenProject `Type`.
/// Same concept, same canonical id (`0x0106`). Both carry `name` /
/// `position` / `is_default` plus the back-reference `has_many
/// :work_items` to [`project_work_item`].
#[must_use]
pub fn project_type() -> Class {
    let mut c = Class::new("ProjectType");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_type".to_string());
    c.associations = vec![family_has_many("work_items", "ProjectWorkItem")];
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut position = Attribute::new("position");
    position.type_name = Some("integer".to_string());
    let mut is_default = Attribute::new("is_default");
    is_default.type_name = Some("boolean".to_string());
    c.attributes = vec![name, position, is_default];
    c
}

// ─────────────────────────────────────────────────────────────────────
// Project-mgmt batch promotions from the cross-curator overlap probe
// (Redmine ↔ OpenProject). Minimal-by-design canonical shapes; both
// curators' AR forms surface the same universal facets.
// ─────────────────────────────────────────────────────────────────────

/// `Member` — links a [`project_actor`] to a [`project`] with a role.
/// Both curators ship `Member` as a join class.
#[must_use]
pub fn project_membership() -> Class {
    let mut c = Class::new("ProjectMembership");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_membership".to_string());
    c.associations = vec![
        family_edge("project", "Project"),
        family_edge("actor", "ProjectActor"),
    ];
    let mut created_on = Attribute::new("created_on");
    created_on.type_name = Some("datetime".to_string());
    c.attributes = vec![created_on];
    c
}

/// `Journal` — audit-trail record on a [`project_work_item`]. The
/// canonical target of `project_work_item().journals`. Both Redmine and
/// OpenProject ship `Journal`; OP uses `acts_as_journalized` to attach.
#[must_use]
pub fn project_journal() -> Class {
    let mut c = Class::new("ProjectJournal");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_journal".to_string());
    c.associations = vec![
        family_edge("journable", "ProjectWorkItem"),
        family_edge("user", "ProjectActor"),
    ];
    let mut created_at = Attribute::new("created_at");
    created_at.type_name = Some("datetime".to_string());
    let mut notes = Attribute::new("notes");
    notes.type_name = Some("text".to_string());
    c.attributes = vec![created_at, notes];
    c
}

/// `Repository` — VCS source root attached to a [`project`].
/// Both curators ship `Repository` as the project's code surface.
#[must_use]
pub fn project_repository() -> Class {
    let mut c = Class::new("ProjectRepository");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_repository".to_string());
    c.associations = vec![family_edge("project", "Project")];
    let mut url = Attribute::new("url");
    url.type_name = Some("string".to_string());
    let mut scm_type = Attribute::new("scm_type");
    scm_type.type_name = Some("string".to_string());
    c.attributes = vec![url, scm_type];
    c
}

/// `Version` — release milestone on a [`project`]. Both curators ship
/// `Version` as the release-grouping concept for work items.
#[must_use]
pub fn project_version() -> Class {
    let mut c = Class::new("ProjectVersion");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_version".to_string());
    c.associations = vec![family_edge("project", "Project")];
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut effective_date = Attribute::new("effective_date");
    effective_date.type_name = Some("date".to_string());
    let mut status = Attribute::new("status");
    status.type_name = Some("string".to_string());
    c.attributes = vec![name, effective_date, status];
    c
}

/// `WikiPage` — page in a project's wiki. Both curators ship `WikiPage`
/// as the documentation surface attached to a [`project`].
#[must_use]
pub fn project_wiki_page() -> Class {
    let mut c = Class::new("ProjectWikiPage");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_wiki_page".to_string());
    c.associations = vec![family_edge("project", "Project")];
    let mut title = Attribute::new("title");
    title.type_name = Some("string".to_string());
    c.attributes = vec![title];
    c
}

/// `Query` — saved filter / view definition. Both curators ship `Query`
/// as a per-user/per-project saved-search surface.
#[must_use]
pub fn project_query() -> Class {
    let mut c = Class::new("ProjectQuery");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_query".to_string());
    c.associations = vec![
        family_edge("project", "Project"),
        family_edge("user", "ProjectActor"),
    ];
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    c.attributes = vec![name];
    c
}

/// `Attachment` — file attached to a project entity (work item, wiki
/// page, journal, …). Both curators ship `Attachment` polymorphically.
#[must_use]
pub fn project_attachment() -> Class {
    let mut c = Class::new("ProjectAttachment");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_attachment".to_string());
    c.associations = Vec::new();
    let mut filename = Attribute::new("filename");
    filename.type_name = Some("string".to_string());
    let mut filesize = Attribute::new("filesize");
    filesize.type_name = Some("integer".to_string());
    let mut content_type = Attribute::new("content_type");
    content_type.type_name = Some("string".to_string());
    c.attributes = vec![filename, filesize, content_type];
    c
}

/// `Comment` — free-form remark attached polymorphically to a project
/// entity. Both curators ship `Comment` as the lightweight discussion
/// surface separate from full journals.
#[must_use]
pub fn project_comment() -> Class {
    let mut c = Class::new("ProjectComment");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_comment".to_string());
    c.associations = vec![family_edge("author", "ProjectActor")];
    let mut comments = Attribute::new("comments");
    comments.type_name = Some("text".to_string());
    c.attributes = vec![comments];
    c
}

/// `CustomField` — per-tenant schema extension definition. Both Redmine
/// and OpenProject ship `CustomField` to add user-defined attributes to
/// project entities at runtime.
#[must_use]
pub fn project_custom_field() -> Class {
    let mut c = Class::new("ProjectCustomField");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_custom_field".to_string());
    c.associations = Vec::new();
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut field_format = Attribute::new("field_format");
    field_format.type_name = Some("string".to_string());
    let mut is_required = Attribute::new("is_required");
    is_required.type_name = Some("boolean".to_string());
    c.attributes = vec![name, field_format, is_required];
    c
}

/// `IssueRelation`/`Relation` — directed work-item edge. The canonical
/// target of [`project_work_item`]'s `relations` family edge. Both
/// curators have it under divergent names (Redmine `IssueRelation` →
/// `issue_from`/`issue_to`; OP `Relation` → `from`/`to`); the canonical
/// shape uses universal `from`/`to` family edges to [`project_work_item`].
#[must_use]
pub fn project_relation() -> Class {
    let mut c = Class::new("ProjectRelation");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_relation".to_string());
    c.associations = vec![
        family_edge("from", "ProjectWorkItem"),
        family_edge("to", "ProjectWorkItem"),
    ];
    let mut relation_type = Attribute::new("relation_type");
    relation_type.type_name = Some("string".to_string());
    // Redmine names it `delay`, OP names it `lag` — same semantic
    // (offset in days between predecessor and successor). Canonical:
    // `lag` (the more common project-scheduling term).
    let mut lag = Attribute::new("lag");
    lag.type_name = Some("integer".to_string());
    c.attributes = vec![relation_type, lag];
    c
}

/// `Changeset` — VCS commit on a [`project_repository`]. Both curators
/// ship `Changeset` with identical shape: `belongs_to :repository`,
/// `belongs_to :user` (committer mapped to a [`project_actor`]), with
/// `revision` / `commit_date` / `comments` scalars.
#[must_use]
pub fn project_changeset() -> Class {
    let mut c = Class::new("ProjectChangeset");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_changeset".to_string());
    c.associations = vec![
        family_edge("repository", "ProjectRepository"),
        family_edge("user", "ProjectActor"),
    ];
    let mut revision = Attribute::new("revision");
    revision.type_name = Some("string".to_string());
    let mut commit_date = Attribute::new("commit_date");
    commit_date.type_name = Some("date".to_string());
    let mut comments = Attribute::new("comments");
    comments.type_name = Some("text".to_string());
    c.attributes = vec![revision, commit_date, comments];
    c
}

/// `Watcher` — a follow-relationship: a [`project_actor`] watches a
/// polymorphic watchable (work item, project, wiki page, …). Both
/// curators ship `Watcher` with `belongs_to :user` + `belongs_to
/// :watchable, polymorphic: true`. The polymorphic target stays opaque
/// at the canonical layer (the `watchable_type` discriminator is the
/// curator-local cell).
#[must_use]
pub fn project_watcher() -> Class {
    let mut c = Class::new("ProjectWatcher");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_watcher".to_string());
    c.associations = vec![family_edge("user", "ProjectActor")];
    let mut watchable_type = Attribute::new("watchable_type");
    watchable_type.type_name = Some("string".to_string());
    c.attributes = vec![watchable_type];
    c
}

/// `News` — a project news post / announcement. Both curators ship
/// `News` with `belongs_to :project` + `belongs_to :author` (a
/// [`project_actor`]) + `has_many :comments` (to [`project_comment`]).
/// Universal `title` / `summary` / `description` attributes.
#[must_use]
pub fn project_news() -> Class {
    let mut c = Class::new("ProjectNews");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_news".to_string());
    c.associations = vec![
        family_edge("project", "Project"),
        family_edge("author", "ProjectActor"),
        family_has_many("comments", "ProjectComment"),
    ];
    let mut title = Attribute::new("title");
    title.type_name = Some("string".to_string());
    let mut summary = Attribute::new("summary");
    summary.type_name = Some("string".to_string());
    let mut description = Attribute::new("description");
    description.type_name = Some("text".to_string());
    c.attributes = vec![title, summary, description];
    c
}

/// `Message` — threaded forum/board discussion post. Both curators ship
/// `Message` (curator divergence on the parent container: Redmine
/// `Board`, OpenProject `Forum`). Universal: `belongs_to :author`, a
/// self-reference `last_reply` for thread chaining, `acts_as_tree` for
/// nested replies, and `subject` / `content` / `replies_count` scalars.
#[must_use]
pub fn project_message() -> Class {
    let mut c = Class::new("ProjectMessage");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_message".to_string());
    c.associations = vec![
        family_edge("author", "ProjectActor"),
        // Tree self-reference: both curators use `acts_as_tree` with
        // `belongs_to :last_reply, class_name: "Message"` linking the
        // thread chain.
        family_edge("last_reply", "ProjectMessage"),
    ];
    let mut subject = Attribute::new("subject");
    subject.type_name = Some("string".to_string());
    let mut content = Attribute::new("content");
    content.type_name = Some("text".to_string());
    let mut replies_count = Attribute::new("replies_count");
    replies_count.type_name = Some("integer".to_string());
    c.attributes = vec![subject, content, replies_count];
    c
}

/// `Board`/`Forum` — the parent container for [`project_message`].
/// Cross-curator name divergence: Redmine `Board`, OpenProject `Forum`.
/// Universal shape: `belongs_to :project` + `has_many :messages` +
/// `belongs_to :last_message` + `name` / `description` / `position`.
#[must_use]
pub fn project_forum() -> Class {
    let mut c = Class::new("ProjectForum");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_forum".to_string());
    c.associations = vec![
        family_edge("project", "Project"),
        family_has_many("messages", "ProjectMessage"),
        family_edge("last_message", "ProjectMessage"),
    ];
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut description = Attribute::new("description");
    description.type_name = Some("text".to_string());
    let mut position = Attribute::new("position");
    position.type_name = Some("integer".to_string());
    c.attributes = vec![name, description, position];
    c
}

/// `Role` — the RBAC permission-set bundle. Both curators ship `Role`
/// with `has_many :member_roles` + `has_many :members, through:
/// :member_roles` (the actors holding the role) and a serialized /
/// joined permission set. The `memberships` family edge points at the
/// existing [`project_membership`] join.
///
/// NOTE: this is the authorization `Role` model — distinct from
/// `ogar_from_ruff::project_role`, which is a helper that maps a curator
/// association *name* onto a [`project_work_item`] family-edge role.
#[must_use]
pub fn project_role() -> Class {
    let mut c = Class::new("ProjectRole");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_role".to_string());
    c.associations = vec![family_has_many("memberships", "ProjectMembership")];
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut position = Attribute::new("position");
    position.type_name = Some("integer".to_string());
    // The defining RBAC payload — a permission set. Redmine serializes it
    // inline (`serialize :permissions`); OpenProject normalizes it into a
    // `has_many :role_permissions` table. Both forms collapse to the
    // canonical concept: a Role carries permissions. Represented coarsely
    // as a text slot at the canonical layer; the curator-local storage
    // shape is a leaf detail.
    let mut permissions = Attribute::new("permissions");
    permissions.type_name = Some("text".to_string());
    c.attributes = vec![name, position, permissions];
    c
}

/// `MemberRole` — the RBAC join completing the membership↔role triangle:
/// a [`project_membership`] holds one or more [`project_role`]s through
/// this join. Both curators ship `MemberRole` (`belongs_to :member` +
/// `belongs_to :role`).
#[must_use]
pub fn project_member_role() -> Class {
    let mut c = Class::new("ProjectMemberRole");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_member_role".to_string());
    c.associations = vec![
        family_edge("membership", "ProjectMembership"),
        family_edge("role", "ProjectRole"),
    ];
    // Both curators track group-inherited role assignments: when a Group
    // membership confers a role on its member users, the derived
    // MemberRole records the parent it was inherited from.
    let mut inherited_from = Attribute::new("inherited_from");
    inherited_from.type_name = Some("integer".to_string());
    c.attributes = vec![inherited_from];
    c
}

/// `CustomValue` — the stored value of a [`project_custom_field`] on a
/// record. Both curators ship `CustomValue` with `belongs_to
/// :custom_field` + a polymorphic `:customized` (the record the value is
/// attached to — kept opaque at the canonical layer).
#[must_use]
pub fn project_custom_value() -> Class {
    let mut c = Class::new("ProjectCustomValue");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_custom_value".to_string());
    c.associations = vec![family_edge("custom_field", "ProjectCustomField")];
    let mut value = Attribute::new("value");
    value.type_name = Some("text".to_string());
    let mut customized_type = Attribute::new("customized_type");
    customized_type.type_name = Some("string".to_string());
    c.attributes = vec![value, customized_type];
    c
}

/// `EnabledModule` — per-project module enablement (which feature modules
/// a [`project`] has turned on). Both curators ship `EnabledModule` with
/// `belongs_to :project` + a `name` (module key), unique per project.
#[must_use]
pub fn project_enabled_module() -> Class {
    let mut c = Class::new("ProjectEnabledModule");
    c.language = Language::Unknown;
    c.canonical_concept = Some("project_enabled_module".to_string());
    c.associations = vec![family_edge("project", "Project")];
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    c.attributes = vec![name];
    c
}

// ─────────────────────────────────────────────────────────────────────
// Commerce / billing / ERP domain canonical classes (OSB ↔ Odoo).
//
// Promoted from the parallel session's `lance-graph-ontology::ar_shape`
// upstream-candidate registry — each backed by ≥2-curator structural
// evidence on the OSB and Odoo corpora. Shapes are minimal-by-design;
// follow-up refinements (additional family edges, typed relations beyond
// the obvious ones) land additively as the other session's harvest
// surfaces more invariants.
// ─────────────────────────────────────────────────────────────────────

/// Line on a commercial document — OSB `InvoiceLineItem`, Odoo
/// `account_move_line`. Belongs to a [`commercial_document`] (the
/// invoice/posting head) and to a [`tax_policy`] (the classifier
/// applied at posting).
#[must_use]
pub fn commercial_line_item() -> Class {
    let mut c = Class::new("CommercialLineItem");
    c.language = Language::Unknown;
    c.canonical_concept = Some("commercial_line_item".to_string());
    c.associations = vec![
        family_edge("document", "CommercialDocument"),
        family_edge("tax", "TaxPolicy"),
    ];
    let mut quantity = Attribute::new("quantity");
    quantity.type_name = Some("decimal".to_string());
    let mut unit_price = Attribute::new("unit_price");
    unit_price.type_name = Some("decimal".to_string());
    let mut subtotal = Attribute::new("subtotal");
    subtotal.type_name = Some("decimal".to_string());
    c.attributes = vec![quantity, unit_price, subtotal];
    c
}

/// Commercial document head — OSB `Invoice`, Odoo `account_move`.
/// Aggregates many [`commercial_line_item`]s, belongs to a
/// [`billing_party`], denominated in a [`currency_policy`].
#[must_use]
pub fn commercial_document() -> Class {
    let mut c = Class::new("CommercialDocument");
    c.language = Language::Unknown;
    c.canonical_concept = Some("commercial_document".to_string());
    c.associations = vec![
        family_has_many("line_items", "CommercialLineItem"),
        family_edge("party", "BillingParty"),
        family_edge("currency", "CurrencyPolicy"),
    ];
    let mut document_date = Attribute::new("document_date");
    document_date.type_name = Some("date".to_string());
    let mut total = Attribute::new("total");
    total.type_name = Some("decimal".to_string());
    let mut state = Attribute::new("state");
    state.type_name = Some("string".to_string());
    c.attributes = vec![document_date, total, state];
    c
}

/// Tax classification policy — OSB `Tax`, Odoo `account_tax`. The
/// canonical target of [`billable_work_entry`]'s `classified_by`
/// family edge and of [`commercial_line_item`]'s `tax` edge.
#[must_use]
pub fn tax_policy() -> Class {
    let mut c = Class::new("TaxPolicy");
    c.language = Language::Unknown;
    c.canonical_concept = Some("tax_policy".to_string());
    c.associations = Vec::new();
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut rate = Attribute::new("rate");
    rate.type_name = Some("decimal".to_string());
    let mut tax_type = Attribute::new("tax_type");
    tax_type.type_name = Some("string".to_string());
    c.attributes = vec![name, rate, tax_type];
    c
}

/// Counterparty in a commercial transaction — OSB `Client`, Odoo
/// `res_partner`. The target of [`commercial_document`]'s `party` edge
/// and [`payment_record`]'s `party` edge.
#[must_use]
pub fn billing_party() -> Class {
    let mut c = Class::new("BillingParty");
    c.language = Language::Unknown;
    c.canonical_concept = Some("billing_party".to_string());
    c.associations = Vec::new();
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut email = Attribute::new("email");
    email.type_name = Some("string".to_string());
    let mut is_company = Attribute::new("is_company");
    is_company.type_name = Some("boolean".to_string());
    c.attributes = vec![name, email, is_company];
    c
}

/// Payment event — OSB `Payment`, Odoo `account_payment`. Belongs to a
/// [`billing_party`] and (optionally) to a [`commercial_document`]
/// (the invoice being settled).
#[must_use]
pub fn payment_record() -> Class {
    let mut c = Class::new("PaymentRecord");
    c.language = Language::Unknown;
    c.canonical_concept = Some("payment_record".to_string());
    c.associations = vec![
        family_edge("party", "BillingParty"),
        family_edge("document", "CommercialDocument"),
    ];
    let mut amount = Attribute::new("amount");
    amount.type_name = Some("decimal".to_string());
    let mut payment_date = Attribute::new("payment_date");
    payment_date.type_name = Some("date".to_string());
    let mut method = Attribute::new("method");
    method.type_name = Some("string".to_string());
    c.attributes = vec![amount, payment_date, method];
    c
}

/// Priority — the urgency lookup applied to work items. The canonical
/// target of [`project_work_item`]'s `priority` family edge.
///
/// Both Redmine and OpenProject use `IssuePriority < Enumeration` with
/// the universal `name` / `position` / `is_default` triple and the
/// back-reference `has_many :issues` / `:work_packages` to
/// [`project_work_item`].
#[must_use]
pub fn priority() -> Class {
    let mut c = Class::new("Priority");
    c.language = Language::Unknown;
    c.canonical_concept = Some("priority".to_string());
    c.associations = vec![family_has_many("work_items", "ProjectWorkItem")];
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut position = Attribute::new("position");
    position.type_name = Some("integer".to_string());
    let mut is_default = Attribute::new("is_default");
    is_default.type_name = Some("boolean".to_string());
    c.attributes = vec![name, position, is_default];
    c
}

/// Currency lookup — OSB `Currency`, Odoo `res_currency`. The
/// canonical target of [`commercial_document`]'s `currency` edge.
#[must_use]
pub fn currency_policy() -> Class {
    let mut c = Class::new("CurrencyPolicy");
    c.language = Language::Unknown;
    c.canonical_concept = Some("currency_policy".to_string());
    c.associations = Vec::new();
    let mut code = Attribute::new("code");
    code.type_name = Some("string".to_string());
    let mut symbol = Attribute::new("symbol");
    symbol.type_name = Some("string".to_string());
    let mut rate = Attribute::new("rate");
    rate.type_name = Some("decimal".to_string());
    c.attributes = vec![code, symbol, rate];
    c
}

/// `product` (`0x0207`) — saleable / billable item (catalogue master).
/// OSB `Product`, Odoo `product.template` + `product.product` (both
/// converge here; the variant relation is outside the codebook).
///
/// Promoted Phase-3 from the cross-axis identity gap surfaced in odoo-rs
/// PR #14 (`alignment_pin::seeded_classes_have_compatible_ogar_identity`).
/// Attributes mirror the minimal `schema:Product`-aligned shape: `sku`
/// (stock-keeping unit / canonical identifier), `name`, `price` (decimal
/// money), `description` (free-text).
pub fn product() -> Class {
    let mut c = Class::new("Product");
    c.language = Language::Unknown;
    c.canonical_concept = Some("product".to_string());
    c.associations = Vec::new();
    let mut sku = Attribute::new("sku");
    sku.type_name = Some("string".to_string());
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut price = Attribute::new("price");
    price.type_name = Some("decimal".to_string());
    let mut description = Attribute::new("description");
    description.type_name = Some("string".to_string());
    c.attributes = vec![sku, name, price, description];
    c
}

/// `accounting_account` (`0x0208`) — general-ledger account (SKR-aligned
/// chart concept). OSB `Account`, Odoo `account.account` (live row) +
/// `account.account.template` (SKR03/04 chart-of-accounts template; both
/// converge here).
///
/// Promoted Phase-3 from the cross-axis identity gap surfaced in odoo-rs
/// PR #14. Attributes mirror the minimal `fibo:Account`-aligned shape:
/// `code` (chart code, e.g. SKR `1200`), `name`, `account_type` (asset /
/// liability / equity / revenue / expense), `currency` (ISO 4217).
pub fn accounting_account() -> Class {
    let mut c = Class::new("AccountingAccount");
    c.language = Language::Unknown;
    c.canonical_concept = Some("accounting_account".to_string());
    c.associations = Vec::new();
    let mut code = Attribute::new("code");
    code.type_name = Some("string".to_string());
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut account_type = Attribute::new("account_type");
    account_type.type_name = Some("string".to_string());
    let mut currency = Attribute::new("currency");
    currency.type_name = Some("string".to_string());
    c.attributes = vec![code, name, account_type, currency];
    c
}

/// `pricelist` (`0x0209`) — price-specification base.
pub fn pricelist() -> Class {
    let mut c = Class::new("Pricelist");
    c.language = Language::Unknown;
    c.canonical_concept = Some("pricelist".to_string());
    c.associations = Vec::new();
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut currency = Attribute::new("currency");
    currency.type_name = Some("string".to_string());
    let mut active = Attribute::new("active");
    active.type_name = Some("bool".to_string());
    c.attributes = vec![name, currency, active];
    c
}

/// `pricelist_rule` (`0x020A`) — per-tier unit-price rule.
pub fn pricelist_rule() -> Class {
    let mut c = Class::new("PricelistRule");
    c.language = Language::Unknown;
    c.canonical_concept = Some("pricelist_rule".to_string());
    c.associations = Vec::new();
    let mut price = Attribute::new("price");
    price.type_name = Some("decimal".to_string());
    let mut min_quantity = Attribute::new("min_quantity");
    min_quantity.type_name = Some("decimal".to_string());
    let mut max_quantity = Attribute::new("max_quantity");
    max_quantity.type_name = Some("decimal".to_string());
    let mut pricelist_ref = Attribute::new("pricelist_ref");
    pricelist_ref.type_name = Some("string".to_string());
    c.attributes = vec![price, min_quantity, max_quantity, pricelist_ref];
    c
}

/// `unit_of_measure` (`0x020B`) — measurement unit.
pub fn unit_of_measure() -> Class {
    let mut c = Class::new("UnitOfMeasure");
    c.language = Language::Unknown;
    c.canonical_concept = Some("unit_of_measure".to_string());
    c.associations = Vec::new();
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut symbol = Attribute::new("symbol");
    symbol.type_name = Some("string".to_string());
    let mut factor = Attribute::new("factor");
    factor.type_name = Some("decimal".to_string());
    let mut uom_type = Attribute::new("uom_type");
    uom_type.type_name = Some("string".to_string());
    c.attributes = vec![name, symbol, factor, uom_type];
    c
}

// ─────────────────────────────────────────────────────────────────────
// 0x08XX — OCR domain (document extraction; the Tesseract-rs arc).
// Class-level container KINDS only — the concept slots name the container
// types the Core resolves; their content (the 112 unichars of a trained
// set, the code tables) lives in content stores, never as concept slots
// (Osint zero-rows ruling is the guard precedent).
// ─────────────────────────────────────────────────────────────────────

/// Unicharset (`0x0801`) — a trained character-set container: the unichar
/// inventory a recognizer resolves against (Tesseract `UNICHARSET`). The
/// unichars themselves are content-store rows under this concept, the same
/// way the ~206 bones are cascade-path nodes under `bone`, not slots.
#[must_use]
pub fn unicharset() -> Class {
    let mut c = Class::new("Unicharset");
    c.language = Language::Unknown;
    c.canonical_concept = Some("unicharset".to_string());
    let mut size = Attribute::new("size"); // number of unichars in the set
    size.type_name = Some("integer".to_string());
    c.attributes = vec![size];
    c
}

/// Recoder (`0x0802`) — the code-compression mapping between unichar ids
/// and recognizer output codes (Tesseract `UnicharCompress`). Compresses a
/// [`unicharset`]'s inventory; the code tables are content, not slots.
#[must_use]
pub fn recoder() -> Class {
    let mut c = Class::new("Recoder");
    c.language = Language::Unknown;
    c.canonical_concept = Some("recoder".to_string());
    c.associations = vec![family_edge("compresses", "Unicharset")];
    c
}

/// Charset (`0x0803`) — an encoding / character-repertoire declaration a
/// document or model asserts (distinct from the trained [`unicharset`]
/// inventory it may be realized by).
#[must_use]
pub fn charset() -> Class {
    let mut c = Class::new("Charset");
    c.language = Language::Unknown;
    c.canonical_concept = Some("charset".to_string());
    let mut encoding = Attribute::new("encoding");
    encoding.type_name = Some("string".to_string());
    c.attributes = vec![encoding];
    c
}

// ─────────────────────────────────────────────────────────────────────
// 0x09XX — Health domain (OGIT Healthcare). The reusable Active-Record
// shape for the clinical concepts. `diagnosis` (0x0902) is the worked
// example carried to full fidelity; the six siblings are competent
// schemas in the same idiom. Field NAMES are English schema labels —
// never German PII labels, never PHI values (OGAR Non-negotiable: PII).
// ─────────────────────────────────────────────────────────────────────

/// Patient — the clinical subject (OGIT `Patient`, `0x0901`). The root
/// of every Health family edge; diagnoses / visits / labs / medications
/// all `belongs_to` a patient.
#[must_use]
pub fn patient() -> Class {
    let mut c = Class::new("Patient");
    c.language = Language::Unknown;
    c.canonical_concept = Some("patient".to_string());
    c.associations = vec![
        family_has_many("diagnoses", "Diagnosis"),
        family_has_many("visits", "Visit"),
    ];
    let mut mrn = Attribute::new("mrn"); // medical record number (identity)
    mrn.type_name = Some("string".to_string());
    let mut given_name = Attribute::new("given_name");
    given_name.type_name = Some("string".to_string());
    let mut family_name = Attribute::new("family_name");
    family_name.type_name = Some("string".to_string());
    let mut birth_date = Attribute::new("birth_date");
    birth_date.type_name = Some("date".to_string());
    let mut sex = Attribute::new("sex");
    sex.type_name = Some("string".to_string());
    c.attributes = vec![mrn, given_name, family_name, birth_date, sex];
    c
}

/// Diagnosis — a clinical finding / condition (OGIT `Diagnosis`,
/// `0x0902`). **The worked example for the Health domain's reusable
/// stack:** a full typed-attribute schema (ICD coding, FHIR-shaped
/// clinical/verification status, onset/resolution dates, primary flag)
/// plus two family edges (`patient`, the subject; `encounter`, the
/// [`visit`] it was recorded in). A consumer (medcare-rs) maps this one
/// canonical shape onto its own SoA columns; the class_id (`0x0902`) is
/// the identity, the attribute set is the bit-basis, and the RBAC
/// sensitivity is inherited from the Health domain (see
/// `ogar_class_view::OgarClassView::access_marking`).
#[must_use]
pub fn diagnosis() -> Class {
    let mut c = Class::new("Diagnosis");
    c.language = Language::Unknown;
    c.canonical_concept = Some("diagnosis".to_string());
    c.description = Some("A clinical finding or condition attributed to a patient".to_string());
    c.associations = vec![
        family_edge("patient", "Patient"),
        family_edge("encounter", "Visit"),
    ];
    let mut icd_code = Attribute::new("icd_code"); // ICD-10/11 coded identity
    icd_code.type_name = Some("string".to_string());
    let mut description = Attribute::new("description");
    description.type_name = Some("string".to_string());
    let mut category = Attribute::new("category");
    category.type_name = Some("string".to_string());
    let mut clinical_status = Attribute::new("clinical_status"); // active|recurrence|resolved
    clinical_status.type_name = Some("string".to_string());
    let mut verification_status = Attribute::new("verification_status"); // provisional|confirmed
    verification_status.type_name = Some("string".to_string());
    let mut severity = Attribute::new("severity");
    severity.type_name = Some("string".to_string());
    let mut onset_date = Attribute::new("onset_date");
    onset_date.type_name = Some("date".to_string());
    let mut resolved_date = Attribute::new("resolved_date");
    resolved_date.type_name = Some("date".to_string());
    let mut is_primary = Attribute::new("is_primary");
    is_primary.type_name = Some("boolean".to_string());
    let mut note = Attribute::new("note");
    note.type_name = Some("text".to_string());
    c.attributes = vec![
        icd_code,
        description,
        category,
        clinical_status,
        verification_status,
        severity,
        onset_date,
        resolved_date,
        is_primary,
        note,
    ];
    c
}

/// LabValue — a laboratory measurement (OGIT `LabValue`, `0x0903`).
/// LOINC-coded, with value/unit/reference-range and an abnormal flag.
#[must_use]
pub fn lab_value() -> Class {
    let mut c = Class::new("LabValue");
    c.language = Language::Unknown;
    c.canonical_concept = Some("lab_value".to_string());
    c.associations = vec![
        family_edge("patient", "Patient"),
        family_edge("encounter", "Visit"),
    ];
    let mut loinc_code = Attribute::new("loinc_code");
    loinc_code.type_name = Some("string".to_string());
    let mut value = Attribute::new("value");
    value.type_name = Some("decimal".to_string());
    let mut unit = Attribute::new("unit");
    unit.type_name = Some("string".to_string());
    let mut reference_range = Attribute::new("reference_range");
    reference_range.type_name = Some("string".to_string());
    let mut abnormal_flag = Attribute::new("abnormal_flag");
    abnormal_flag.type_name = Some("string".to_string());
    let mut collected_at = Attribute::new("collected_at");
    collected_at.type_name = Some("datetime".to_string());
    c.attributes = vec![
        loinc_code,
        value,
        unit,
        reference_range,
        abnormal_flag,
        collected_at,
    ];
    c
}

/// Medication — a prescribed / administered drug (OGIT `Medication`,
/// `0x0904`). ATC-coded, with dose / route / frequency and a date range.
#[must_use]
pub fn medication() -> Class {
    let mut c = Class::new("Medication");
    c.language = Language::Unknown;
    c.canonical_concept = Some("medication".to_string());
    c.associations = vec![family_edge("patient", "Patient")];
    let mut atc_code = Attribute::new("atc_code");
    atc_code.type_name = Some("string".to_string());
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut dose = Attribute::new("dose");
    dose.type_name = Some("string".to_string());
    let mut route = Attribute::new("route");
    route.type_name = Some("string".to_string());
    let mut frequency = Attribute::new("frequency");
    frequency.type_name = Some("string".to_string());
    let mut start_date = Attribute::new("start_date");
    start_date.type_name = Some("date".to_string());
    let mut end_date = Attribute::new("end_date");
    end_date.type_name = Some("date".to_string());
    c.attributes = vec![atc_code, name, dose, route, frequency, start_date, end_date];
    c
}

/// Treatment — a procedure / intervention performed (OGIT `Treatment`,
/// `0x0905`). Coded, with a performed-at timestamp and an outcome.
#[must_use]
pub fn treatment() -> Class {
    let mut c = Class::new("Treatment");
    c.language = Language::Unknown;
    c.canonical_concept = Some("treatment".to_string());
    c.associations = vec![
        family_edge("patient", "Patient"),
        family_edge("encounter", "Visit"),
    ];
    let mut code = Attribute::new("code");
    code.type_name = Some("string".to_string());
    let mut description = Attribute::new("description");
    description.type_name = Some("string".to_string());
    let mut performed_at = Attribute::new("performed_at");
    performed_at.type_name = Some("datetime".to_string());
    let mut outcome = Attribute::new("outcome");
    outcome.type_name = Some("string".to_string());
    c.attributes = vec![code, description, performed_at, outcome];
    c
}

/// Visit — a clinical encounter (OGIT `Visit`, `0x0906`). The temporal
/// container diagnoses / labs / treatments / vitals are recorded within.
#[must_use]
pub fn visit() -> Class {
    let mut c = Class::new("Visit");
    c.language = Language::Unknown;
    c.canonical_concept = Some("visit".to_string());
    c.associations = vec![family_edge("patient", "Patient")];
    let mut visit_number = Attribute::new("visit_number");
    visit_number.type_name = Some("string".to_string());
    let mut visit_type = Attribute::new("visit_type"); // inpatient|outpatient|emergency
    visit_type.type_name = Some("string".to_string());
    let mut department = Attribute::new("department");
    department.type_name = Some("string".to_string());
    let mut admitted_at = Attribute::new("admitted_at");
    admitted_at.type_name = Some("datetime".to_string());
    let mut discharged_at = Attribute::new("discharged_at");
    discharged_at.type_name = Some("datetime".to_string());
    c.attributes = vec![
        visit_number,
        visit_type,
        department,
        admitted_at,
        discharged_at,
    ];
    c
}

/// VitalSign — a point-in-time physiological measurement (OGIT
/// `VitalSign`, `0x0907`). Coded value/unit with a measured-at timestamp.
#[must_use]
pub fn vital_sign() -> Class {
    let mut c = Class::new("VitalSign");
    c.language = Language::Unknown;
    c.canonical_concept = Some("vital_sign".to_string());
    c.associations = vec![
        family_edge("patient", "Patient"),
        family_edge("encounter", "Visit"),
    ];
    let mut code = Attribute::new("code"); // e.g. LOINC 8867-4 (heart rate)
    code.type_name = Some("string".to_string());
    let mut value = Attribute::new("value");
    value.type_name = Some("decimal".to_string());
    let mut unit = Attribute::new("unit");
    unit.type_name = Some("string".to_string());
    let mut measured_at = Attribute::new("measured_at");
    measured_at.type_name = Some("datetime".to_string());
    c.attributes = vec![code, value, unit, measured_at];
    c
}

// ── 0x0BXX — Auth domain builders (the AuthStore class family, keystone §7) ──

/// The `auth_store` (`0x0B01`) base class of the AuthStore family — the
/// IdP→classid mapping class (`docs/CLASSID-RBAC-KEYSTONE-SPEC.md` §7).
/// Carries the three claim-name slots; maps `sub` → actor (`0x0104`),
/// role-key → role (`0x0117`), org/tenant → scope. The canonical OGIT
/// shape confirms it: arago's `NTO/Auth/Configuration` entity (keyed by
/// organization/account/application/scope IDs) is this class, built
/// upstream independently. A reservation — the enforcement `authorize()`
/// is gated on `PROBE-OGAR-RBAC-AUTHORIZE` (keystone §10).
#[must_use]
pub fn auth_store() -> Class {
    let mut c = Class::new("AuthStore");
    c.language = Language::Unknown;
    c.canonical_concept = Some("auth_store".to_string());
    c.associations = vec![
        family_edge("maps_actor", "ProjectActor"),
        family_edge("maps_role", "ProjectRole"),
    ];
    let mut sub_claim = Attribute::new("sub_claim");
    sub_claim.type_name = Some("string".to_string());
    let mut role_claim = Attribute::new("role_claim");
    role_claim.type_name = Some("string".to_string());
    let mut org_claim = Attribute::new("org_claim");
    org_claim.type_name = Some("string".to_string());
    c.attributes = vec![sub_claim, role_claim, org_claim];
    c
}

/// A preminted provider profile — `is-a` [`auth_store`] carrying that
/// IdP's claim grammar as data (keystone §7).
fn auth_provider(name: &str, concept: &str) -> Class {
    let mut c = Class::new(name);
    c.language = Language::Unknown;
    c.canonical_concept = Some(concept.to_string());
    c.parent = Some("AuthStore".to_string());
    let mut grammar = Attribute::new("claim_grammar");
    grammar.type_name = Some("string".to_string());
    c.attributes = vec![grammar];
    c
}

/// The `auth_zitadel` (`0x0B02`) provider profile. Maps 1:1: Project →
/// class scope, Project-Role → role, Authorization/Grant → membership
/// tuple, Organization → scope, User → `sub`.
#[must_use]
pub fn auth_zitadel() -> Class {
    auth_provider("AuthZitadel", "auth_zitadel")
}

/// The `auth_zanzibar` (`0x0B03`) provider profile — Google Zanzibar /
/// OpenFGA `object#relation@subject` tuple grammar.
#[must_use]
pub fn auth_zanzibar() -> Class {
    auth_provider("AuthZanzibar", "auth_zanzibar")
}

/// The `auth_ory_keto` (`0x0B04`) provider profile — Ory Keto.
#[must_use]
pub fn auth_ory_keto() -> Class {
    auth_provider("AuthOryKeto", "auth_ory_keto")
}

// ── 0x0AXX — Anatomy domain builders (FMA reference kinds) ──
//
// The public anatomical reference frame consumed by the splat-native arc
// (`docs/SPLAT-NATIVE-CUSTOMER.md`) and the FMA skeletal spine
// (`crates/ogar-fma-skeleton`). These are the *kinds* (the FMA universal
// root, the skeletal system, the bone, the joint) — the ~206 individual
// bones are NOT concept slots; they are cascade-path nodes whose 16×8-bit
// Morton-tile address places them in the partonomy + body volume. See
// `docs/FMA-SKELETON-CONVERGENCE-ANCHOR.md`.

/// The `anatomical_structure` (`0x0A01`) — FMA's universal root kind
/// (everything in the atlas `is-a` this). The abstract anchor of the
/// anatomy partonomy.
#[must_use]
pub fn anatomical_structure() -> Class {
    let mut c = Class::new("AnatomicalStructure");
    c.language = Language::Unknown;
    c.canonical_concept = Some("anatomical_structure".to_string());
    let mut fma_id = Attribute::new("fma_id");
    fma_id.type_name = Some("string".to_string());
    let mut name_la = Attribute::new("name_la"); // Terminologia Anatomica
    name_la.type_name = Some("string".to_string());
    c.attributes = vec![fma_id, name_la];
    c
}

// ─────────────────────────────────────────────────────────────────────
// 0x0DXX — HR domain (employment / org / contracts). The reusable
// Active-Record shape for HR master-data per arago HIRO + Odoo `hr.*` +
// vcard/org/fibo alignment. Field names are English schema labels.

/// `hr_employee` (`0x0D01`) — person record (vcard:Individual).
pub fn hr_employee() -> Class {
    let mut c = Class::new("HrEmployee");
    c.language = Language::Unknown;
    c.canonical_concept = Some("hr_employee".to_string());
    c.associations = Vec::new();
    let mut full_name = Attribute::new("full_name");
    full_name.type_name = Some("string".to_string());
    let mut email = Attribute::new("email");
    email.type_name = Some("string".to_string());
    let mut phone = Attribute::new("phone");
    phone.type_name = Some("string".to_string());
    let mut employee_id = Attribute::new("employee_id");
    employee_id.type_name = Some("string".to_string());
    c.attributes = vec![full_name, email, phone, employee_id];
    c
}

/// `hr_department` (`0x0D02`) — organizational unit (org:OrganizationalUnit).
pub fn hr_department() -> Class {
    let mut c = Class::new("HrDepartment");
    c.language = Language::Unknown;
    c.canonical_concept = Some("hr_department".to_string());
    c.associations = Vec::new();
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    let mut manager_ref = Attribute::new("manager_ref");
    manager_ref.type_name = Some("string".to_string());
    let mut parent_ref = Attribute::new("parent_ref");
    parent_ref.type_name = Some("string".to_string());
    c.attributes = vec![name, manager_ref, parent_ref];
    c
}

/// `hr_job` (`0x0D03`) — role / position (org:Role).
pub fn hr_job() -> Class {
    let mut c = Class::new("HrJob");
    c.language = Language::Unknown;
    c.canonical_concept = Some("hr_job".to_string());
    c.associations = Vec::new();
    let mut title = Attribute::new("title");
    title.type_name = Some("string".to_string());
    let mut description = Attribute::new("description");
    description.type_name = Some("string".to_string());
    let mut department_ref = Attribute::new("department_ref");
    department_ref.type_name = Some("string".to_string());
    c.attributes = vec![title, description, department_ref];
    c
}

/// `hr_employment_contract` (`0x0D04`) — base employment contract
/// (fibo:Contract). Payroll computation stays out of the codebook.
pub fn hr_employment_contract() -> Class {
    let mut c = Class::new("HrEmploymentContract");
    c.language = Language::Unknown;
    c.canonical_concept = Some("hr_employment_contract".to_string());
    c.associations = Vec::new();
    let mut start_date = Attribute::new("start_date");
    start_date.type_name = Some("date".to_string());
    let mut end_date = Attribute::new("end_date");
    end_date.type_name = Some("date".to_string());
    let mut contract_type = Attribute::new("contract_type");
    contract_type.type_name = Some("string".to_string());
    let mut salary = Attribute::new("salary");
    salary.type_name = Some("decimal".to_string());
    c.attributes = vec![start_date, end_date, contract_type, salary];
    c
}

/// The `skeleton` (`0x0A02`) — the whole-body skeletal system; the root of
/// the bone partonomy (`crates/ogar-fma-skeleton`).
#[must_use]
pub fn skeleton() -> Class {
    let mut c = Class::new("Skeleton");
    c.language = Language::Unknown;
    c.canonical_concept = Some("skeleton".to_string());
    c.parent = Some("AnatomicalStructure".to_string());
    c.associations = vec![family_edge("bones", "Bone")];
    c
}

/// The `bone` (`0x0A03`) — a skeletal element. **The clamped convergence
/// anchor**: the rigid, non-negotiable frame the splat-fit registers
/// against. The ~206 individual bones are cascade-path nodes under this
/// concept (FMA partonomy → 16×8-bit Morton-tile address), not codebook
/// slots. See `docs/FMA-SKELETON-CONVERGENCE-ANCHOR.md`.
#[must_use]
pub fn bone() -> Class {
    let mut c = Class::new("Bone");
    c.language = Language::Unknown;
    c.canonical_concept = Some("bone".to_string());
    c.parent = Some("AnatomicalStructure".to_string());
    c.associations = vec![
        family_edge("part_of", "Skeleton"),
        family_edge("articulates", "Joint"),
    ];
    let mut rest_pose = Attribute::new("rest_pose"); // rigid transform (T-pose)
    rest_pose.type_name = Some("string".to_string());
    let mut clamped = Attribute::new("clamped"); // bones are always anchors
    clamped.type_name = Some("boolean".to_string());
    c.attributes = vec![rest_pose, clamped];
    c
}

/// The `joint` (`0x0A04`) — an articulation between bones (the skeletal
/// graph's edges, when materialized).
#[must_use]
pub fn joint() -> Class {
    let mut c = Class::new("Joint");
    c.language = Language::Unknown;
    c.canonical_concept = Some("joint".to_string());
    c.parent = Some("AnatomicalStructure".to_string());
    c.associations = vec![family_edge("connects", "Bone")];
    c
}

// ── 0x0CXX — Automation domain builders (HIRO IT-automation stack) ──
// The MARS structural CMDB (A→R→S→M `dependsOn` backbone) + the Automation
// DO-arm actuators. Shapes grounded in the vendored OGIT TTL attributes
// (`vocab/imports/ogit/NTO/{MARS,Automation}/`). See `docs/MARS-TRANSCODING.md`
// + `docs/HIRO-DO-ARM-LIFT.md`.

/// The `mars_application` (`0x0C01`) — head of the MARS A→R→S→M `dependsOn`
/// backbone (`ogit.MARS:Application`).
#[must_use]
pub fn mars_application() -> Class {
    let mut c = Class::new("MarsApplication");
    c.language = Language::Unknown;
    c.canonical_concept = Some("mars_application".to_string());
    let mut class = Attribute::new("class");
    class.type_name = Some("string".to_string());
    c.attributes = vec![class];
    c.associations = vec![family_edge("depends_on", "MarsResource")];
    c
}

/// The `mars_resource` (`0x0C02`) — `ogit.MARS:Resource`.
#[must_use]
pub fn mars_resource() -> Class {
    let mut c = Class::new("MarsResource");
    c.language = Language::Unknown;
    c.canonical_concept = Some("mars_resource".to_string());
    let mut class = Attribute::new("class");
    class.type_name = Some("string".to_string());
    c.attributes = vec![class];
    c.associations = vec![family_edge("depends_on", "MarsSoftware")];
    c
}

/// The `mars_software` (`0x0C03`) — `ogit.MARS:Software`.
#[must_use]
pub fn mars_software() -> Class {
    let mut c = Class::new("MarsSoftware");
    c.language = Language::Unknown;
    c.canonical_concept = Some("mars_software".to_string());
    let mut service_name = Attribute::new("service_name");
    service_name.type_name = Some("string".to_string());
    c.attributes = vec![service_name];
    c.associations = vec![family_edge("depends_on", "MarsMachine")];
    c
}

/// The `mars_machine` (`0x0C04`) — tail of the A→R→S→M chain
/// (`ogit.MARS:Machine`).
#[must_use]
pub fn mars_machine() -> Class {
    let mut c = Class::new("MarsMachine");
    c.language = Language::Unknown;
    c.canonical_concept = Some("mars_machine".to_string());
    let mut cpu_arch = Attribute::new("cpu_arch");
    cpu_arch.type_name = Some("string".to_string());
    let mut cpu_cores = Attribute::new("cpu_cores");
    cpu_cores.type_name = Some("integer".to_string());
    c.attributes = vec![cpu_arch, cpu_cores];
    c
}

/// The `knowledge_item` (`0x0C05`) — the Automation KnowledgeItem; the DO-arm
/// `ActionDef` carrier (`ogit.Automation:KnowledgeItem`). The opaque body
/// (`knowledge_item_formal_representation`) is pointed-to, never inlined.
#[must_use]
pub fn knowledge_item() -> Class {
    let mut c = Class::new("KnowledgeItem");
    c.language = Language::Unknown;
    c.canonical_concept = Some("knowledge_item".to_string());
    // The opaque body slot — the lossless-DO pointer (the attribute exists;
    // the bytes are never inlined into the IR).
    let mut body = Attribute::new("knowledge_item_formal_representation");
    body.type_name = Some("string".to_string());
    c.attributes = vec![body];
    c.associations = vec![
        family_edge("relates", "MarsNodeTemplate"),
        family_edge("contains", "AutomationTrigger"),
    ];
    c
}

/// The `mars_node_template` (`0x0C06`) — the template a KnowledgeItem
/// `relates` to; the DO-arm `ActionDef.object_class`
/// (`ogit.Automation:MARSNodeTemplate`).
#[must_use]
pub fn mars_node_template() -> Class {
    let mut c = Class::new("MarsNodeTemplate");
    c.language = Language::Unknown;
    c.canonical_concept = Some("mars_node_template".to_string());
    let mut repr = Attribute::new("mars_node_formal_representation");
    repr.type_name = Some("string".to_string());
    c.attributes = vec![repr];
    c
}

/// The `action_handler` (`0x0C07`) — the ActionHandler adapter/membrane
/// (`ogit.Automation:ActionHandler`); where the DO arm meets the auth/RBAC arm.
#[must_use]
pub fn action_handler() -> Class {
    let mut c = Class::new("ActionHandler");
    c.language = Language::Unknown;
    c.canonical_concept = Some("action_handler".to_string());
    let mut name = Attribute::new("name");
    name.type_name = Some("string".to_string());
    c.attributes = vec![name];
    c.associations = vec![family_edge("provides", "ActionApplicability")];
    c
}

/// The `action_applicability` (`0x0C08`) — its `environment_filter` is the
/// DO-arm `KausalSpec::StateGuard` (`ogit.Automation:ActionApplicability`).
#[must_use]
pub fn action_applicability() -> Class {
    let mut c = Class::new("ActionApplicability");
    c.language = Language::Unknown;
    c.canonical_concept = Some("action_applicability".to_string());
    let mut env = Attribute::new("environment_filter");
    env.type_name = Some("string".to_string());
    c.attributes = vec![env];
    c
}

/// The `automation_trigger` (`0x0C09`) — the Trigger a KnowledgeItem
/// `contains`; the DO-arm `KausalSpec::LifecycleTrigger`
/// (`ogit.Automation:Trigger`).
#[must_use]
pub fn automation_trigger() -> Class {
    let mut c = Class::new("AutomationTrigger");
    c.language = Language::Unknown;
    c.canonical_concept = Some("automation_trigger".to_string());
    let mut description = Attribute::new("description");
    description.type_name = Some("string".to_string());
    c.attributes = vec![description];
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_default_is_empty() {
        let c = Class::default();
        assert!(c.name.is_empty());
        assert!(c.associations.is_empty());
        assert!(matches!(c.language, Language::Ruby));
    }

    #[test]
    fn class_new_sets_only_name() {
        let c = Class::new("WorkPackage");
        assert_eq!(c.name, "WorkPackage");
        assert!(c.parent.is_none());
        assert!(c.associations.is_empty());
    }

    #[test]
    fn wire_synergies_links_a_concept_across_domains() {
        // Use a genuinely un-promoted lexical pair to demonstrate the
        // synergy mechanism. (Many cross-curator-common names have since
        // been promoted into the codebook with distinct domain blocks —
        // Comment/comments now resolve to `project_comment` for instance.
        // `Setting`/`settings` remain lexical-only.)
        let mut op_setting = Class::new("Setting");
        op_setting.source_domain = Some("project".to_string());
        let mut odoo_setting = Class::new("settings");
        odoo_setting.source_domain = Some("erp".to_string());
        let mut op_wp = Class::new("WorkPackage");
        op_wp.source_domain = Some("project".to_string());

        let syn = wire_synergies(&[op_setting, odoo_setting, op_wp]);
        assert_eq!(syn.len(), 1, "only `setting` bridges both domains");
        assert_eq!(syn[0].concept, "setting");
        assert_eq!(syn[0].members.len(), 2);
        // ordered by domain: erp before project
        assert_eq!(syn[0].members[0].domain, "erp");
        assert_eq!(syn[0].members[0].class_name, "settings");
        assert_eq!(syn[0].members[1].domain, "project");
        assert_eq!(syn[0].members[1].class_name, "Setting");
    }

    #[test]
    fn wire_synergies_needs_two_distinct_domains() {
        // same concept, same domain → not a synergy
        let mut a = Class::new("User");
        a.source_domain = Some("project".to_string());
        let mut b = Class::new("Users");
        b.source_domain = Some("project".to_string());
        // an undomained class is ignored entirely
        let c = Class::new("res.users");
        assert!(wire_synergies(&[a, b, c]).is_empty());
    }

    #[test]
    fn canonical_concept_promotes_billable_work_entry_deterministically() {
        // Promoted cross-domain invariant — OpenProject `TimeEntry` and
        // Odoo `account.analytic.line` converge to one canonical concept
        // (both the dotted and ruff's underscored form). Pure +
        // deterministic: same input → same output, every session.
        for name in [
            "TimeEntry",
            "time_entry",
            "account.analytic.line",
            "account_analytic_line",
            "Leistungsposition",
            "Arbeitszeit",
        ] {
            assert_eq!(canonical_concept(name), "billable_work_entry");
            assert_eq!(canonical_concept(name), canonical_concept(name));
        }
        // Un-promoted names still normalize lexically. (User/Principal
        // promoted into `project_actor` by a later PR; use a name that
        // is genuinely un-promoted today.)
        assert_eq!(canonical_concept("Country"), "country");
        assert_eq!(canonical_concept("res.partners"), "partner");
    }

    #[test]
    fn billable_work_entry_has_twelve_family_edges() {
        let c = billable_work_entry();
        assert_eq!(c.name, "BillableWorkEntry");
        assert_eq!(c.canonical_concept.as_deref(), Some("billable_work_entry"));
        // Synthetic canonical class — neutral language (codex P2 on #57).
        assert_eq!(c.language, Language::Unknown);
        // Defining `billable` flag is typed as a boolean so DDL adapters
        // do not default it to string (codex P2 on #57).
        let billable = c
            .attributes
            .iter()
            .find(|a| a.name == "billable")
            .expect("billable attribute");
        assert_eq!(billable.type_name.as_deref(), Some("boolean"));
        // Exactly the 12 internal family edges, to canonical concepts —
        // `about` points at `ProjectWorkItem` (not the OP curator surface
        // `WorkPackage`) so Redmine `Issue` and OP `WorkPackage` converge
        // here through their shared canonical concept (codex P2 on #58).
        assert_eq!(c.associations.len(), 12);
        for target in [
            "Project",
            "ProjectWorkItem",
            "Worker",
            "Duration",
            "RatePolicy",
            "CostCenter",
            "TaxPolicy",
            "InvoiceLineCandidate",
            "ApprovalState",
            "Tenant",
            "AuditTrail",
            "PostingAction",
        ] {
            assert!(
                c.associations
                    .iter()
                    .any(|e| e.class_name.as_deref() == Some(target)),
                "missing family edge → {target}",
            );
        }
    }

    #[test]
    fn convergence_project_and_erp_materialize_to_billable_work_entry() {
        let canonical = billable_work_entry();
        // Two curators, only domain tag + name — a consumer session
        // rediscovers the bridge deterministically from these surfaces.
        let mut op = Class::new("TimeEntry");
        op.source_domain = Some("project".to_string());
        op.canonical_concept = Some(canonical_concept("TimeEntry"));
        let mut odoo = Class::new("account_analytic_line");
        odoo.source_domain = Some("erp".to_string());
        odoo.canonical_concept = Some(canonical_concept("account_analytic_line"));

        // Both materialize to the SAME canonical concept as the class.
        assert_eq!(op.canonical_concept, canonical.canonical_concept);
        assert_eq!(odoo.canonical_concept, canonical.canonical_concept);

        // wire_synergies rediscovers exactly one cross-domain bridge,
        // and is idempotent (deterministic).
        let syn = wire_synergies(&[op.clone(), odoo.clone()]);
        assert_eq!(syn, wire_synergies(&[op, odoo]));
        assert_eq!(syn.len(), 1);
        assert_eq!(syn[0].concept, "billable_work_entry");
        assert_eq!(syn[0].members.len(), 2);
    }

    #[test]
    fn tax_policy_is_an_erp_boundary_edge_not_in_project_evidence() {
        // TaxPolicy is a family edge on the canonical shape ...
        let bwe = billable_work_entry();
        assert!(
            bwe.associations
                .iter()
                .any(|e| e.class_name.as_deref() == Some("TaxPolicy"))
        );
        // ... but the project curator records work evidence with no tax.
        let mut op = Class::new("TimeEntry");
        op.source_domain = Some("project".to_string());
        op.canonical_concept = Some(canonical_concept("TimeEntry"));
        assert!(op.associations.is_empty());
        assert!(!op.attributes.iter().any(|a| a.name.contains("tax")));
    }

    #[test]
    fn one_invoice_line_aggregates_many_billable_work_entries() {
        let bwe = billable_work_entry();
        let mat = bwe
            .associations
            .iter()
            .find(|e| e.name == "materializes_as")
            .expect("materializes_as edge");
        // BelongsTo: many BillableWorkEntries → one InvoiceLineCandidate
        // (one invoice line aggregates many work entries).
        assert_eq!(mat.kind, AssociationKind::BelongsTo);
        assert_eq!(mat.class_name.as_deref(), Some("InvoiceLineCandidate"));
    }

    #[test]
    fn canonical_concept_promotes_project_work_item_deterministically() {
        // Promoted project-domain invariant — Redmine `Issue` and
        // OpenProject `WorkPackage` (both spellings) resolve to one
        // canonical concept. Pure + deterministic.
        for name in [
            "Issue",
            "issue",
            "WorkPackage",
            "work_package",
            "workpackage",
        ] {
            assert_eq!(canonical_concept(name), "project_work_item");
            assert_eq!(canonical_concept(name), canonical_concept(name));
        }
    }

    #[test]
    fn project_work_item_has_required_family_edges() {
        let c = project_work_item();
        assert_eq!(c.name, "ProjectWorkItem");
        assert_eq!(c.canonical_concept.as_deref(), Some("project_work_item"));
        // Synthetic canonical class — neutral language (codex P2 on #57).
        assert_eq!(c.language, Language::Unknown);
        // The 9 family edges named in the smoke spec.
        for (role, target) in [
            ("project", "Project"),
            ("status", "ProjectStatus"),
            ("type", "ProjectType"),
            ("priority", "Priority"),
            ("author", "ProjectActor"),
            ("assignee", "ProjectActor"),
            ("journals", "ProjectJournal"),
            ("relations", "ProjectRelation"),
            ("time_entries", "BillableWorkEntry"),
        ] {
            let e = c
                .associations
                .iter()
                .find(|a| a.name == role)
                .unwrap_or_else(|| panic!("missing family edge: {role}"));
            assert_eq!(e.class_name.as_deref(), Some(target));
        }
        // has-many vs belongs-to cardinality is correct: journals /
        // relations / time_entries aggregate; the rest are single refs.
        for role in ["journals", "relations", "time_entries"] {
            let e = c.associations.iter().find(|a| a.name == role).unwrap();
            assert_eq!(e.kind, AssociationKind::HasMany);
        }
    }

    #[test]
    fn same_project_domain_curators_do_not_create_duplicate_canonical_concepts() {
        // Redmine `Issue` and OpenProject `WorkPackage` are project-domain
        // work-item curators; they MUST converge to one canonical concept,
        // never two — that's exactly what makes the agnostic vocab worth
        // more than its curators.
        assert_eq!(canonical_concept("Issue"), canonical_concept("WorkPackage"));
        assert_eq!(canonical_concept("Issue"), "project_work_item");
        // The lexical layer remains deterministic for unpromoted names.
        assert_eq!(canonical_concept("User"), canonical_concept("Users"));
    }

    #[test]
    fn codebook_has_no_duplicate_ids_or_zero() {
        // Per `NodeGuid::CLASSID_DEFAULT`, id 0 is canon-reserved; the
        // codebook entries must all be non-zero and unique. This
        // collision-check pins the registry contract (codex P1 on PR #60:
        // unique mint, never a content hash).
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for (name, id) in CODEBOOK {
            assert_ne!(
                *id, 0,
                "id 0 is reserved (CLASSID_DEFAULT); offender: {name}"
            );
            assert!(ids.insert(*id), "duplicate codebook id at `{name}`");
            assert!(names.insert(*name), "duplicate canonical name `{name}`");
        }
    }

    #[test]
    fn canonical_concept_id_returns_some_for_promoted_none_for_unknown() {
        // Promoted concepts are in the curated registry — assigned ids.
        for s in ["project", "project_work_item", "billable_work_entry"] {
            assert!(
                canonical_concept_id(s).is_some(),
                "promoted `{s}` must be in codebook"
            );
        }
        // Unknown concepts have NO codebook identity — they are not in
        // the registry. Returning None instead of a synthesised hash is
        // the no-silent-collision contract.
        assert_eq!(canonical_concept_id("outcome"), None);
        assert_eq!(canonical_concept_id("handle_out"), None);
        assert_eq!(canonical_concept_id(""), None);
        assert_eq!(canonical_concept_id("user"), None);
    }

    #[test]
    fn canonical_concept_name_round_trips() {
        // Every codebook id reverses to its name, and that name maps forward
        // to the same id — proving the reverse lookup is total over the
        // codebook AND that ids are unique (a collision would break one
        // direction). This is the gate `PROBE-OGAR-ID-TO-CONCEPT-NAME` (odoo-rs
        // UPSTREAM_WISHLIST) asks for before the consumer fusion can land.
        for &(name, id) in CODEBOOK {
            assert_eq!(
                canonical_concept_name(id),
                Some(name),
                "id 0x{id:04X} must reverse to `{name}`",
            );
            assert_eq!(canonical_concept_id(name), Some(id));
        }
    }

    #[test]
    fn canonical_concept_name_known_ids_and_none_for_unknown() {
        assert_eq!(canonical_concept_name(0x0202), Some("commercial_document"));
        assert_eq!(canonical_concept_name(0x0103), Some("billable_work_entry"));
        assert_eq!(canonical_concept_name(0x0204), Some("billing_party"));
        // Ids outside the codebook (the 0x0000 default sentinel, the 0xFFFF
        // max) have no canonical concept — None, never a synthesised name.
        assert_eq!(canonical_concept_name(0x0000), None);
        assert_eq!(canonical_concept_name(0xFFFF), None);
    }

    #[test]
    fn ogar_codebook_maps_curator_labels_to_canonical_id() {
        // The load-bearing insight: leave the curator name shape intact;
        // the codebook is what maps to the canonical target.
        let pwi = canonical_concept_id("project_work_item");
        assert!(pwi.is_some());
        assert_eq!(ogar_codebook("Issue"), pwi);
        assert_eq!(ogar_codebook("WorkPackage"), pwi);
        assert_eq!(ogar_codebook("work_package"), pwi);
        // PascalCase canonical class-name spelling resolves to the same
        // id as snake_case canonical (codex P2 fix).
        assert_eq!(ogar_codebook("ProjectWorkItem"), pwi);

        let bwe = canonical_concept_id("billable_work_entry");
        assert!(bwe.is_some());
        assert_eq!(ogar_codebook("TimeEntry"), bwe);
        assert_eq!(ogar_codebook("BillableWorkEntry"), bwe);
        // Odoo-shaped name maps to the same binary id without producer-
        // side normalisation. (Lift implementation lives in the
        // python-side producer the other session owns; the codebook
        // mapping itself stands here.)
        assert_eq!(ogar_codebook("account.analytic.line"), bwe);
        assert_eq!(ogar_codebook("account_analytic_line"), bwe);

        assert_eq!(ogar_codebook("Project"), canonical_concept_id("project"));

        // Unknown alias -> None (no silent hash collision).
        assert_eq!(ogar_codebook("outcome"), None);
        assert_eq!(ogar_codebook("handle_out"), None);
    }

    #[test]
    fn label_dto_carries_local_label_and_shared_codebook_id() {
        // Two consumers with totally different labels for the same
        // concept produce LabelDTOs with different labels and EQUAL ids,
        // and the SAME canonical-AST label (for SurrealAST / planner /
        // kanban consumers that emit a portable symbol).
        let a = LabelDTO::from_alias("Issue").unwrap();
        let b = LabelDTO::from_alias("WorkPackage").unwrap();
        let canonical = LabelDTO::from_alias("project_work_item").unwrap();
        let odoo_shaped = LabelDTO::from_alias("account.analytic.line").unwrap();
        let bwe = LabelDTO::from_alias("billable_work_entry").unwrap();
        // PascalCase canonical class name also resolves (codex P2 fix).
        let pwi_pascal = LabelDTO::from_alias("ProjectWorkItem").unwrap();
        // Labels stay local — not normalised.
        assert_ne!(a.label, b.label, "labels are local");
        assert_eq!(a.label, "Issue");
        assert_eq!(odoo_shaped.label, "account.analytic.line");
        assert_eq!(pwi_pascal.label, "ProjectWorkItem");
        // Ids converge — the address is the identity.
        assert_eq!(a.id, b.id, "address is the identity");
        assert_eq!(a.id, canonical.id, "curator and OGAR labels share the id");
        assert_eq!(
            a.id, pwi_pascal.id,
            "PascalCase canonical name shares the id"
        );
        assert_eq!(
            odoo_shaped.id, bwe.id,
            "cross-domain label converges on the id"
        );
        assert_ne!(a.id, bwe.id, "distinct concepts have distinct ids");
        // Canonical-AST labels converge — what AST consumers emit.
        assert_eq!(a.canonical, "project_work_item");
        assert_eq!(b.canonical, "project_work_item");
        assert_eq!(canonical.canonical, "project_work_item");
        assert_eq!(pwi_pascal.canonical, "project_work_item");
        assert_eq!(odoo_shaped.canonical, "billable_work_entry");
        assert_eq!(bwe.canonical, "billable_work_entry");

        // Unknown labels: None — they are not in the codebook.
        // (Use a genuinely un-promoted name; `user` is now in
        // `project_actor` via the PR adding ProjectActor.)
        assert!(LabelDTO::from_alias("outcome").is_none());
        assert!(LabelDTO::from_alias("nonexistent_widget").is_none());
    }

    #[test]
    fn le_wire_contract_round_trips() {
        // The wire contract: u16 little-endian, roundtrip-stable across
        // Class.canonical_id_le() and LabelDTO.id_le(). What downstream
        // consumers (SurrealAST, planner, kanban) read off the wire.
        let issue = LabelDTO::from_alias("Issue").unwrap();
        let wp = LabelDTO::from_alias("WorkPackage").unwrap();
        // Same wire bytes for the same concept.
        assert_eq!(issue.id_le(), wp.id_le());
        // Roundtrip via u16::from_le_bytes recovers the id.
        assert_eq!(u16::from_le_bytes(issue.id_le()), issue.id);
        // Class.canonical_id_le agrees with LabelDTO.id_le for the same
        // canonical concept.
        let pwi = project_work_item();
        assert_eq!(
            pwi.canonical_id_le().unwrap(),
            LabelDTO::from_alias("project_work_item").unwrap().id_le(),
        );
        // No canonical -> None on the wire.
        assert_eq!(Class::new("Bare").canonical_id_le(), None);
    }

    #[test]
    fn class_canonical_id_round_trips_through_codebook() {
        // A Class with a canonical_concept set produces the matching
        // codebook id; without one, returns None.
        let c = project_work_item();
        assert_eq!(c.canonical_id(), canonical_concept_id("project_work_item"));
        // Curator-shaped class with canonical_concept populated by the
        // lift: same binary id as a hand-built canonical class.
        let mut redmine_issue = Class::new("Issue");
        redmine_issue.canonical_concept = Some(canonical_concept("Issue"));
        assert_eq!(
            redmine_issue.canonical_id(),
            project_work_item().canonical_id()
        );
        // Without a canonical_concept, no id.
        assert_eq!(Class::new("Whatever").canonical_id(), None);
        // Also: canonical_concept that's not promoted -> no codebook id
        // (no silent hash). Set a non-promoted concept directly and
        // confirm None.
        let mut bare = Class::new("Bare");
        bare.canonical_concept = Some("totally_unknown".to_string());
        assert_eq!(bare.canonical_id(), None);
    }

    #[test]
    fn project_is_the_promoted_canonical_class() {
        let c = project();
        assert_eq!(c.name, "Project");
        assert_eq!(c.canonical_concept.as_deref(), Some("project"));
        // Synthetic canonical class — neutral language (codex P2 doctrine).
        assert_eq!(c.language, Language::Unknown);
        // The three direct family edges — all to canonical concepts.
        // (The `parent` edge waits on a producer-side mixin decode for
        // `awesome_nested_set` / `Projects::Hierarchy` — see project()
        // doc.)
        assert_eq!(c.associations.len(), 3);
        for (role, target, kind) in [
            ("work_items", "ProjectWorkItem", AssociationKind::HasMany),
            (
                "time_entries",
                "BillableWorkEntry",
                AssociationKind::HasMany,
            ),
            ("members", "ProjectActor", AssociationKind::HasMany),
        ] {
            let e = c
                .associations
                .iter()
                .find(|a| a.name == role)
                .unwrap_or_else(|| panic!("missing family edge: {role}"));
            assert_eq!(e.class_name.as_deref(), Some(target));
            assert_eq!(e.kind, kind);
        }
        // Identity attributes carry types so DDL adapters generate the
        // right column shape (codex P2 doctrine on typed scalars).
        for attr in ["name", "identifier"] {
            let a = c.attributes.iter().find(|x| x.name == attr).unwrap();
            assert_eq!(a.type_name.as_deref(), Some("string"));
        }
    }

    #[test]
    fn project_actor_is_the_promoted_canonical_class() {
        let c = project_actor();
        assert_eq!(c.name, "ProjectActor");
        assert_eq!(c.canonical_concept.as_deref(), Some("project_actor"));
        // Synthetic canonical class — neutral language.
        assert_eq!(c.language, Language::Unknown);
        // Single direct family edge: projects (through memberships, both
        // curators).
        assert_eq!(c.associations.len(), 1);
        let projects = &c.associations[0];
        assert_eq!(projects.name, "projects");
        assert_eq!(projects.kind, AssociationKind::HasMany);
        assert_eq!(projects.class_name.as_deref(), Some("Project"));
        // Identity attributes carry types (codex P2 doctrine on typed scalars).
        for attr in ["login", "type"] {
            let a = c.attributes.iter().find(|x| x.name == attr).unwrap();
            assert_eq!(a.type_name.as_deref(), Some("string"));
        }
        // In the codebook with a unique id.
        assert!(c.canonical_id().is_some());
        assert_eq!(c.canonical_id(), canonical_concept_id("project_actor"));
    }

    #[test]
    fn project_actor_resolver_collapses_user_principal_sti_chain() {
        // Both Redmine and OP have `User < Principal < ApplicationRecord`
        // AND `Group < Principal`. The promoted arm collapses ALL Principal
        // STI subtypes onto a single canonical concept — they ARE the same
        // actor identity in the ontology (a Group is assignable / member-able
        // exactly where a User is).
        for src in [
            "User",
            "user",
            "Users",
            "Principal",
            "principal",
            "Principals",
            "Group",
            "group",
            "Groups",
        ] {
            assert_eq!(
                canonical_concept(src),
                "project_actor",
                "{src} -> project_actor"
            );
        }
        // PascalCase canonical class name round-trips (codex P2 doctrine).
        assert_eq!(canonical_concept("ProjectActor"), "project_actor");
        assert_eq!(canonical_concept("project_actor"), "project_actor");
        // All resolve to the same codebook id.
        let id = canonical_concept_id("project_actor");
        assert!(id.is_some());
        for src in ["User", "Principal", "Group", "Groups", "ProjectActor"] {
            assert_eq!(ogar_codebook(src), id, "{src} -> codebook id");
        }
    }

    #[test]
    fn codebook_ids_are_domain_prefixed_and_consistent() {
        // The high byte of every codebook id encodes its domain block.
        // Existing concepts must live in their correct domain block.
        for project_concept in [
            "project",
            "project_work_item",
            "billable_work_entry",
            "project_actor",
            "project_status",
            "project_type",
            "priority",
            "project_membership",
            "project_journal",
            "project_repository",
            "project_version",
            "project_wiki_page",
            "project_query",
            "project_attachment",
            "project_comment",
            "project_custom_field",
            "project_relation",
            "project_changeset",
            "project_watcher",
            "project_news",
            "project_message",
            "project_forum",
            "project_role",
            "project_member_role",
            "project_custom_value",
            "project_enabled_module",
        ] {
            let id = canonical_concept_id(project_concept)
                .unwrap_or_else(|| panic!("{project_concept} missing from codebook"));
            assert_eq!(
                id >> 8,
                0x01,
                "{project_concept} id {id:#06x} not in 0x01XX block"
            );
            assert_eq!(canonical_concept_domain(id), ConceptDomain::ProjectMgmt);
        }
        for commerce_concept in [
            "commercial_line_item",
            "commercial_document",
            "tax_policy",
            "billing_party",
            "payment_record",
            "currency_policy",
            "product",
            "accounting_account",
            "pricelist",
            "pricelist_rule",
            "unit_of_measure",
        ] {
            let id = canonical_concept_id(commerce_concept)
                .unwrap_or_else(|| panic!("{commerce_concept} missing from codebook"));
            assert_eq!(
                id >> 8,
                0x02,
                "{commerce_concept} id {id:#06x} not in 0x02XX block"
            );
            assert_eq!(canonical_concept_domain(id), ConceptDomain::Commerce);
        }
        for health_concept in [
            "patient",
            "diagnosis",
            "lab_value",
            "medication",
            "treatment",
            "visit",
            "vital_sign",
        ] {
            let id = canonical_concept_id(health_concept)
                .unwrap_or_else(|| panic!("{health_concept} missing from codebook"));
            assert_eq!(
                id >> 8,
                0x09,
                "{health_concept} id {id:#06x} not in 0x09XX block"
            );
            assert_eq!(canonical_concept_domain(id), ConceptDomain::Health);
        }
        // Reserved + named future-domain blocks.
        assert_eq!(canonical_concept_domain(0x0000), ConceptDomain::Reserved);
        assert_eq!(canonical_concept_domain(0x00FF), ConceptDomain::Reserved);
        assert_eq!(canonical_concept_domain(0x0700), ConceptDomain::Osint);
        assert_eq!(canonical_concept_domain(0x07AB), ConceptDomain::Osint);
        assert_eq!(canonical_concept_domain(0x0800), ConceptDomain::Ocr);
        assert_eq!(canonical_concept_domain(0x0900), ConceptDomain::Health);
        assert_eq!(canonical_concept_domain(0x0B00), ConceptDomain::Auth);
        assert_eq!(canonical_concept_domain(0x0B04), ConceptDomain::Auth);
        // Anatomy block (0x0A) — FMA reference kinds.
        assert_eq!(canonical_concept_domain(0x0A00), ConceptDomain::Anatomy);
        assert_eq!(canonical_concept_domain(0x0A03), ConceptDomain::Anatomy);
        // Automation block (0x0C) — HIRO IT-automation stack.
        assert_eq!(canonical_concept_domain(0x0C00), ConceptDomain::Automation);
        assert_eq!(canonical_concept_domain(0x0C09), ConceptDomain::Automation);
        // Unassigned blocks (3-6).
        assert_eq!(canonical_concept_domain(0x0300), ConceptDomain::Unassigned);
        assert_eq!(canonical_concept_domain(0x0600), ConceptDomain::Unassigned);
        // HR block (0x0D).
        assert_eq!(canonical_concept_domain(0x0D00), ConceptDomain::HR);
        // Genetics block (0x0E) — reserved, zero concept rows today (item-3
        // mint, `docs/DISCOVERY-MAP.md` D-CLASSID-CANON-HIGH-FLIP).
        assert_eq!(canonical_concept_domain(0x0E01), ConceptDomain::Genetics);
        assert_eq!(canonical_concept_domain(0x0EFF), ConceptDomain::Genetics);
        // Trailing unassigned tail (0x0F+).
        assert_eq!(canonical_concept_domain(0xFFFF), ConceptDomain::Unassigned);
    }

    #[test]
    fn auth_domain_concepts_resolve_and_route() {
        // The AuthStore class family (keystone §7) resolves through the
        // codebook and routes to ConceptDomain::Auth. These are
        // reservations — the enforcement authorize() is gated on
        // PROBE-OGAR-RBAC-AUTHORIZE (keystone §10) and is not part of
        // this mint.
        for (concept, id) in [
            ("auth_store", 0x0B01u16),
            ("auth_zitadel", 0x0B02),
            ("auth_zanzibar", 0x0B03),
            ("auth_ory_keto", 0x0B04),
        ] {
            assert_eq!(
                canonical_concept_id(concept),
                Some(id),
                "{concept} missing/wrong in codebook"
            );
            assert_eq!(canonical_concept_domain(id), ConceptDomain::Auth);
        }
        // The four preminted profiles are the whole Auth block today.
        assert_eq!(concepts_in_domain(ConceptDomain::Auth).count(), 4);
    }

    #[test]
    fn health_classes_carry_their_canonical_codebook_identity() {
        // Each Health AR builder resolves to its codebook id (the class_id
        // is the identity; the PascalCase name is decorative). Mirrors the
        // project/commerce gates.
        for (builder, concept, id) in [
            (patient as fn() -> Class, "patient", 0x0901u16),
            (diagnosis, "diagnosis", 0x0902),
            (lab_value, "lab_value", 0x0903),
            (medication, "medication", 0x0904),
            (treatment, "treatment", 0x0905),
            (visit, "visit", 0x0906),
            (vital_sign, "vital_sign", 0x0907),
        ] {
            let c = builder();
            assert_eq!(c.canonical_concept.as_deref(), Some(concept));
            assert_eq!(c.canonical_id(), Some(id), "{concept} -> {id:#06x}");
            assert_eq!(canonical_concept_domain(id), ConceptDomain::Health);
        }
    }

    #[test]
    fn diagnosis_is_the_rich_worked_example() {
        // The 0x0902 worked example: full typed-attribute schema + two
        // family edges. Pins the bit-basis so a downstream FieldMask
        // producer notices a reorder.
        let d = diagnosis();
        assert_eq!(d.name, "Diagnosis");
        // ICD code is the first attribute (the coded identity slot).
        assert_eq!(d.attributes[0].name, "icd_code");
        assert_eq!(d.attributes[0].type_name.as_deref(), Some("string"));
        // Two family edges: the subject and the encounter.
        let edges: Vec<&str> = d.associations.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(edges, ["patient", "encounter"]);
        // Every attribute carries a type (DDL adapters need the shape).
        for a in &d.attributes {
            assert!(a.type_name.is_some(), "{} has no type", a.name);
        }
        // Comfortably under the FieldMask u64 ceiling.
        assert!(d.attributes.len() + d.associations.len() <= 64);
    }

    #[test]
    fn concepts_in_domain_enumerates_exactly_each_domains_codebook_set() {
        // The reusable fail-closed coverage hook: a domain-scoped consumer
        // (e.g. medcare-rs over Health) inherits its required concept set
        // from here. Drift = a concept promoted upstream without the
        // consumer noticing => exactly what the boot-time coverage gate
        // must catch.
        let health: Vec<&str> = concepts_in_domain(ConceptDomain::Health)
            .map(|(name, _)| name)
            .collect();
        assert_eq!(
            health,
            [
                "patient",
                "diagnosis",
                "lab_value",
                "medication",
                "treatment",
                "visit",
                "vital_sign",
            ],
            "Health domain set drift — re-sync the consumer coverage gate",
        );
        // Every yielded id really is in-domain.
        for (_, id) in concepts_in_domain(ConceptDomain::Health) {
            assert_eq!(canonical_concept_domain(id), ConceptDomain::Health);
        }
        // Counts line up with the codebook blocks.
        assert_eq!(concepts_in_domain(ConceptDomain::Health).count(), 7);
        // 0x08XX OCR: the three container KINDS (unicharset/recoder/charset).
        // Content (the 112 unichars, code tables) never becomes concepts —
        // see the CODEBOOK 0x08XX section note (Osint zero-rows precedent).
        assert_eq!(concepts_in_domain(ConceptDomain::Ocr).count(), 3);
        let ocr: Vec<&str> = concepts_in_domain(ConceptDomain::Ocr)
            .map(|(name, _)| name)
            .collect();
        assert_eq!(
            ocr,
            ["unicharset", "recoder", "charset"],
            "OCR domain set drift — re-sync the consumer coverage gate",
        );
        assert_eq!(concepts_in_domain(ConceptDomain::HR).count(), 4);
        assert_eq!(concepts_in_domain(ConceptDomain::Commerce).count(), 11);
        assert_eq!(concepts_in_domain(ConceptDomain::ProjectMgmt).count(), 26);
        assert_eq!(concepts_in_domain(ConceptDomain::Anatomy).count(), 4);
        assert_eq!(concepts_in_domain(ConceptDomain::Auth).count(), 4);
        assert_eq!(concepts_in_domain(ConceptDomain::Automation).count(), 9);
        // Every yielded Automation id really is in-domain (0x0CXX).
        let automation: Vec<&str> = concepts_in_domain(ConceptDomain::Automation)
            .map(|(name, _)| name)
            .collect();
        assert_eq!(
            automation,
            [
                "mars_application",
                "mars_resource",
                "mars_software",
                "mars_machine",
                "knowledge_item",
                "mars_node_template",
                "action_handler",
                "action_applicability",
                "automation_trigger",
            ],
            "Automation domain set drift — re-sync the consumer coverage gate",
        );
        // The OSINT domain carries ZERO vocabulary rows BY DESIGN (operator
        // ruling 2026-07-02): its low byte is appid space (q2 = 0x01), not a
        // concept slot — see the CODEBOOK 0x07XX section note.
        assert_eq!(concepts_in_domain(ConceptDomain::Osint).count(), 0);
        // Same posture for the Genetics domain (0x0E, CPIC pharmacogenomics
        // under q2) — reserved, zero concept rows until an operator ruling
        // mints one — see the CODEBOOK 0x0EXX section note.
        assert_eq!(concepts_in_domain(ConceptDomain::Genetics).count(), 0);
    }

    #[test]
    fn project_mgmt_batch_promotions_each_have_a_codebook_id_and_shape() {
        // The 9 new project-mgmt concepts from the cross-curator overlap
        // probe. Each has a canonical class with `Language::Unknown`, a
        // populated canonical_concept, an id in the 0x01XX block, and
        // typed attributes.
        for (canonical, name, id_hex) in [
            (project_membership(), "ProjectMembership", 0x0108u16),
            (project_journal(), "ProjectJournal", 0x0109),
            (project_repository(), "ProjectRepository", 0x010A),
            (project_version(), "ProjectVersion", 0x010B),
            (project_wiki_page(), "ProjectWikiPage", 0x010C),
            (project_query(), "ProjectQuery", 0x010D),
            (project_attachment(), "ProjectAttachment", 0x010E),
            (project_comment(), "ProjectComment", 0x010F),
            (project_custom_field(), "ProjectCustomField", 0x0110),
            (project_relation(), "ProjectRelation", 0x0111),
            (project_changeset(), "ProjectChangeset", 0x0112),
            (project_watcher(), "ProjectWatcher", 0x0113),
            (project_news(), "ProjectNews", 0x0114),
            (project_message(), "ProjectMessage", 0x0115),
            (project_forum(), "ProjectForum", 0x0116),
            (project_role(), "ProjectRole", 0x0117),
            (project_member_role(), "ProjectMemberRole", 0x0118),
            (project_custom_value(), "ProjectCustomValue", 0x0119),
            (project_enabled_module(), "ProjectEnabledModule", 0x011A),
        ] {
            assert_eq!(canonical.name, name);
            assert_eq!(canonical.language, Language::Unknown);
            assert_eq!(canonical.canonical_id(), Some(id_hex));
            assert_eq!(canonical_concept_domain(id_hex), ConceptDomain::ProjectMgmt);
            assert!(!canonical.attributes.is_empty(), "{name} has no attrs");
            for a in &canonical.attributes {
                assert!(a.type_name.is_some(), "{name}.{} untyped", a.name);
            }
        }
    }

    /// Regression for the codex P2 on PR #66: a Rails-tableized plural
    /// label (e.g. `wiki_pages`, `custom_fields`) must resolve to the
    /// same codebook id as the singular form. The promoted-invariant
    /// arms must accept the plural snake_case spelling — not punt to
    /// lexical fallback, where it would return `wiki_page` /
    /// `custom_field` without re-entering the codebook lookup.
    #[test]
    fn plural_table_aliases_resolve_to_promoted_codebook_id() {
        for (singular, plural) in [
            // From PR #67 — original codex P2 fix.
            ("project_wiki_page", "wiki_pages"),
            ("project_wiki_page", "wikipages"),
            ("project_wiki_page", "WikiPages"),
            ("project_custom_field", "custom_fields"),
            ("project_custom_field", "customfields"),
            ("project_custom_field", "CustomFields"),
            // From PR #68 — codex P2 on `issue_relations`.
            ("project_relation", "issue_relations"),
            ("project_relation", "issuerelations"),
            ("project_relation", "IssueRelations"),
            // Proactive coverage for the same defect class on already-merged
            // promoted concepts where the Rails-table plural is irregular
            // (Rails: ies→y for repository/query, s→ses for status/priority,
            // -ies/-ses pluralization in general). Single-`s` lexical
            // fallback drops to non-canonical strings; explicit promotion
            // ensures the codebook lookup hits.
            ("billable_work_entry", "time_entries"),
            ("billable_work_entry", "timeentries"),
            ("billable_work_entry", "TimeEntries"),
            ("project_status", "issue_statuses"),
            ("project_status", "issuestatuses"),
            ("project_status", "IssueStatuses"),
            ("priority", "issue_priorities"),
            ("priority", "issuepriorities"),
            ("priority", "IssuePriorities"),
            ("project_membership", "memberships"),
            ("project_membership", "Memberships"),
        ] {
            let canonical_id = canonical_concept_id(singular);
            assert!(canonical_id.is_some(), "{singular} must be in codebook");
            assert_eq!(
                canonical_concept(plural),
                singular,
                "{plural} -> {singular}"
            );
            assert_eq!(
                ogar_codebook(plural),
                canonical_id,
                "tableized `{plural}` must share the {singular} codebook id",
            );
        }
    }

    #[test]
    fn project_mgmt_resolver_arms_collapse_curator_names() {
        // Every alias for each new concept resolves to the right id.
        let cases: &[(&str, &[&str])] = &[
            (
                "project_membership",
                &["Member", "members", "ProjectMembership"],
            ),
            (
                "project_journal",
                &["Journal", "journals", "ProjectJournal"],
            ),
            (
                "project_repository",
                &["Repository", "repositories", "ProjectRepository"],
            ),
            (
                "project_version",
                &["Version", "versions", "ProjectVersion"],
            ),
            (
                "project_wiki_page",
                &["WikiPage", "wiki_page", "ProjectWikiPage"],
            ),
            ("project_query", &["Query", "queries", "ProjectQuery"]),
            (
                "project_attachment",
                &["Attachment", "attachments", "ProjectAttachment"],
            ),
            (
                "project_comment",
                &["Comment", "comments", "ProjectComment"],
            ),
            (
                "project_custom_field",
                &["CustomField", "custom_field", "ProjectCustomField"],
            ),
            // Cross-curator name divergence: Redmine `IssueRelation` ↔ OP `Relation`.
            (
                "project_relation",
                &[
                    "IssueRelation",
                    "issue_relation",
                    "Relation",
                    "relations",
                    "ProjectRelation",
                ],
            ),
            (
                "project_changeset",
                &["Changeset", "changesets", "ProjectChangeset"],
            ),
            (
                "project_watcher",
                &["Watcher", "watchers", "ProjectWatcher"],
            ),
            ("project_news", &["News", "news", "ProjectNews"]),
            (
                "project_message",
                &["Message", "messages", "ProjectMessage"],
            ),
            // Cross-curator name divergence: Redmine `Board` ↔ OP `Forum`.
            (
                "project_forum",
                &["Board", "boards", "Forum", "forums", "ProjectForum"],
            ),
            ("project_role", &["Role", "roles", "ProjectRole"]),
            (
                "project_member_role",
                &["MemberRole", "member_roles", "ProjectMemberRole"],
            ),
            (
                "project_custom_value",
                &["CustomValue", "custom_values", "ProjectCustomValue"],
            ),
            (
                "project_enabled_module",
                &["EnabledModule", "enabled_modules", "ProjectEnabledModule"],
            ),
        ];
        for (concept, aliases) in cases {
            let id = canonical_concept_id(concept).unwrap();
            for alias in *aliases {
                assert_eq!(canonical_concept(alias), *concept, "{alias} -> {concept}");
                assert_eq!(ogar_codebook(alias), Some(id), "{alias} -> id");
            }
        }
    }

    #[test]
    fn commerce_canonical_classes_are_promoted_into_the_codebook() {
        // Each commerce concept has an assigned codebook id in the
        // `0x02XX` commerce-domain block and a populated canonical class.
        for (canonical, name, id_hex) in [
            (commercial_line_item(), "CommercialLineItem", 0x0201u16),
            (commercial_document(), "CommercialDocument", 0x0202),
            (tax_policy(), "TaxPolicy", 0x0203),
            (billing_party(), "BillingParty", 0x0204),
            (payment_record(), "PaymentRecord", 0x0205),
            (currency_policy(), "CurrencyPolicy", 0x0206),
        ] {
            assert_eq!(canonical.name, name);
            assert_eq!(canonical.language, Language::Unknown);
            let concept = canonical.canonical_concept.as_deref().unwrap();
            assert_eq!(canonical.canonical_id(), Some(id_hex));
            assert_eq!(canonical_concept_id(concept), Some(id_hex));
            // Every commerce class has at least one typed attribute.
            assert!(!canonical.attributes.is_empty(), "{name} has no attributes");
            for a in &canonical.attributes {
                assert!(a.type_name.is_some(), "{name}.{} untyped", a.name);
            }
        }
    }

    #[test]
    fn commerce_resolver_collapses_osb_and_odoo_curator_divergence() {
        // For each commerce concept, every OSB-side, Odoo-side, and
        // canonical-class-name spelling resolves to the same codebook id.
        let cases: &[(&str, &[&str])] = &[
            (
                "commercial_line_item",
                &[
                    "InvoiceLineItem",
                    "invoice_line_item",
                    "account.move.line",
                    "account_move_line",
                    "CommercialLineItem",
                    "commercial_line_item",
                ],
            ),
            (
                "commercial_document",
                &[
                    "Invoice",
                    "invoices",
                    "account.move",
                    "account_move",
                    "CommercialDocument",
                    "commercial_document",
                ],
            ),
            (
                "tax_policy",
                &[
                    "Tax",
                    "taxes",
                    "account.tax",
                    "account_tax",
                    "TaxPolicy",
                    "tax_policy",
                ],
            ),
            (
                "billing_party",
                &[
                    "Client",
                    "clients",
                    "res.partner",
                    "res_partner",
                    "BillingParty",
                    "billing_party",
                ],
            ),
            (
                "payment_record",
                &[
                    "Payment",
                    "payments",
                    "account.payment",
                    "account_payment",
                    "PaymentRecord",
                    "payment_record",
                ],
            ),
            (
                "currency_policy",
                &[
                    "Currency",
                    "currencies",
                    "res.currency",
                    "res_currency",
                    "CurrencyPolicy",
                    "currency_policy",
                ],
            ),
        ];
        let mut ids = std::collections::HashSet::new();
        for (concept, aliases) in cases {
            let id = canonical_concept_id(concept);
            assert!(id.is_some(), "{concept} must be in the codebook");
            assert!(ids.insert(id), "duplicate codebook id for `{concept}`");
            for alias in *aliases {
                assert_eq!(canonical_concept(alias), *concept, "{alias} -> {concept}");
                assert_eq!(ogar_codebook(alias), id, "{alias} -> codebook id");
            }
        }
    }

    #[test]
    fn source_domain_concept_maps_coarse_tags_to_codebook_domains() {
        assert_eq!(
            source_domain_concept("project"),
            Some(ConceptDomain::ProjectMgmt)
        );
        assert_eq!(source_domain_concept("erp"), Some(ConceptDomain::Commerce));
        assert_eq!(
            source_domain_concept("german-erp"),
            Some(ConceptDomain::Commerce)
        );
        // Unknown / unclassified curator → no domain → promotion withheld.
        assert_eq!(source_domain_concept("health"), None);
        assert_eq!(source_domain_concept(""), None);
    }

    #[test]
    fn canonical_concept_in_domain_gates_generic_role_by_domain() {
        use ConceptDomain::{Commerce, Health, ProjectMgmt};
        // codex P2 on PR #72: a bare `Role` only becomes `project_role`
        // when the curator is actually in the project-mgmt domain.
        assert_eq!(
            canonical_concept_in_domain("Role", Some(ProjectMgmt)),
            "project_role"
        );
        assert_eq!(
            canonical_concept_in_domain("roles", Some(ProjectMgmt)),
            "project_role"
        );
        // Foreign domain → the promotion is a collision, not a bridge → lexical.
        assert_eq!(canonical_concept_in_domain("Role", Some(Commerce)), "role");
        assert_eq!(canonical_concept_in_domain("Role", Some(Health)), "role");
        // Unknown curator domain → withhold → lexical (cannot vouch for it).
        assert_eq!(canonical_concept_in_domain("Role", None), "role");
        // The canonical spelling behaves identically — no special case.
        assert_eq!(
            canonical_concept_in_domain("ProjectRole", Some(Health)),
            "projectrole"
        );
    }

    #[test]
    fn canonical_concept_in_domain_keeps_each_domains_own_promotions() {
        use ConceptDomain::{Commerce, ProjectMgmt};
        // Commerce concept lands only for a commerce curator.
        assert_eq!(
            canonical_concept_in_domain("Invoice", Some(Commerce)),
            "commercial_document"
        );
        assert_eq!(
            canonical_concept_in_domain("Invoice", Some(ProjectMgmt)),
            "invoice"
        );
        // Project concept lands only for a project curator. For a foreign
        // domain it falls through to the coarse lexical fallback (which
        // drops a trailing plural `s`: "Status" -> "statu") — the point is
        // simply that it is NOT the promoted `project_status`.
        assert_eq!(
            canonical_concept_in_domain("WorkPackage", Some(ProjectMgmt)),
            "project_work_item"
        );
        assert_eq!(
            canonical_concept_in_domain("Status", Some(Commerce)),
            "statu"
        );
        assert_ne!(
            canonical_concept_in_domain("Status", Some(Commerce)),
            "project_status"
        );
        // Already-lexical names are unchanged in any domain.
        assert_eq!(
            canonical_concept_in_domain("Setting", Some(ProjectMgmt)),
            "setting"
        );
        assert_eq!(canonical_concept_in_domain("Setting", None), "setting");
    }

    #[test]
    fn cross_domain_bridge_survives_the_domain_gate() {
        use ConceptDomain::{Commerce, ProjectMgmt};
        // `billable_work_entry` is a deliberate cross-domain bridge: it has
        // a project-mgmt home id (0x0103) but erp/german-erp curators must
        // still converge onto it — the gate must NOT sever that.
        assert!(is_cross_domain_concept("billable_work_entry"));
        assert!(!is_cross_domain_concept("project_role"));
        let id = canonical_concept_id("billable_work_entry").unwrap();
        assert_eq!(canonical_concept_domain(id), ProjectMgmt); // home domain
        // Project curator (home domain) — kept.
        assert_eq!(
            canonical_concept_in_domain("TimeEntry", Some(ProjectMgmt)),
            "billable_work_entry"
        );
        // Odoo / erp curator (foreign domain) — STILL kept (bridge exempt).
        assert_eq!(
            canonical_concept_in_domain("account_analytic_line", Some(Commerce)),
            "billable_work_entry"
        );
        // Even an unknown-domain curator keeps the bridge.
        assert_eq!(
            canonical_concept_in_domain("TimeEntry", None),
            "billable_work_entry"
        );
    }

    #[test]
    fn lexical_concept_matches_canonical_fallback_for_unpromoted_names() {
        // `lexical_concept` is exactly the Layer-2 fallback of
        // `canonical_concept` — for an unpromoted name they agree.
        for name in ["Setting", "settings", "res.users", "Address", "WidgetThing"] {
            assert_eq!(lexical_concept(name), canonical_concept(name), "{name}");
        }
        // It does NOT promote: a promoted name still reduces lexically here.
        assert_eq!(lexical_concept("Role"), "role");
        assert_eq!(lexical_concept("WorkPackage"), "workpackage");
    }

    #[test]
    fn commerce_class_family_edges_target_canonical_concepts() {
        // Internal cross-references in the commerce sub-graph are
        // canonical-to-canonical — no curator surface leaks in:
        //   CommercialLineItem -> CommercialDocument, TaxPolicy
        //   CommercialDocument -> CommercialLineItem (HM), BillingParty, CurrencyPolicy
        //   PaymentRecord     -> BillingParty, CommercialDocument
        let line = commercial_line_item();
        assert!(
            line.associations
                .iter()
                .any(|a| a.name == "document"
                    && a.class_name.as_deref() == Some("CommercialDocument"))
        );
        assert!(
            line.associations
                .iter()
                .any(|a| a.name == "tax" && a.class_name.as_deref() == Some("TaxPolicy"))
        );

        let doc = commercial_document();
        let line_items = doc
            .associations
            .iter()
            .find(|a| a.name == "line_items")
            .unwrap();
        assert_eq!(line_items.kind, AssociationKind::HasMany);
        assert_eq!(line_items.class_name.as_deref(), Some("CommercialLineItem"));
        assert!(
            doc.associations
                .iter()
                .any(|a| a.name == "party" && a.class_name.as_deref() == Some("BillingParty"))
        );
        assert!(
            doc.associations
                .iter()
                .any(|a| a.name == "currency" && a.class_name.as_deref() == Some("CurrencyPolicy"))
        );

        let pay = payment_record();
        assert!(
            pay.associations
                .iter()
                .any(|a| a.name == "party" && a.class_name.as_deref() == Some("BillingParty"))
        );
        assert!(
            pay.associations
                .iter()
                .any(|a| a.name == "document"
                    && a.class_name.as_deref() == Some("CommercialDocument"))
        );
    }

    #[test]
    fn priority_is_the_promoted_canonical_class() {
        let c = priority();
        assert_eq!(c.name, "Priority");
        assert_eq!(c.canonical_concept.as_deref(), Some("priority"));
        assert_eq!(c.language, Language::Unknown);
        assert_eq!(c.canonical_id(), Some(0x0107));
        // Universal typed attributes mirroring Status/Type pattern.
        let attr_kind = |n: &str, t: &str| {
            assert_eq!(
                c.attributes
                    .iter()
                    .find(|a| a.name == n)
                    .and_then(|a| a.type_name.as_deref()),
                Some(t),
                "attr {n} typed as {t}",
            );
        };
        attr_kind("name", "string");
        attr_kind("position", "integer");
        attr_kind("is_default", "boolean");
        // Back-reference to ProjectWorkItem.
        let work_items = c
            .associations
            .iter()
            .find(|a| a.name == "work_items")
            .expect("work_items edge");
        assert_eq!(work_items.kind, AssociationKind::HasMany);
        assert_eq!(work_items.class_name.as_deref(), Some("ProjectWorkItem"));
    }

    #[test]
    fn priority_resolver_collapses_redmine_and_op_issuepriority() {
        // Both curators use `IssuePriority < Enumeration`. The lexical
        // form `issuepriority` is distinct from bare `priority`; the
        // promoted arm collapses both.
        let id = canonical_concept_id("priority");
        assert!(id.is_some());
        for src in [
            "IssuePriority",
            "issuepriority",
            "issue_priority",
            "Priority",
            "priority",
            "Priorities",
            "priorities",
        ] {
            assert_eq!(canonical_concept(src), "priority", "{src} -> priority");
            assert_eq!(ogar_codebook(src), id, "{src} -> codebook id");
        }
    }

    #[test]
    fn project_status_and_project_type_are_promoted_canonical_classes() {
        // ProjectStatus — Redmine IssueStatus / OP Status converge here.
        let s = project_status();
        assert_eq!(s.name, "ProjectStatus");
        assert_eq!(s.canonical_concept.as_deref(), Some("project_status"));
        assert_eq!(s.language, Language::Unknown);
        assert_eq!(s.canonical_id(), Some(0x0105));
        let s_attr = |n: &str, t: &str| {
            assert_eq!(
                s.attributes
                    .iter()
                    .find(|a| a.name == n)
                    .and_then(|a| a.type_name.as_deref()),
                Some(t),
            );
        };
        s_attr("name", "string");
        s_attr("position", "integer");
        s_attr("is_closed", "boolean");

        // ProjectType — Redmine Tracker / OP Type converge here.
        let t = project_type();
        assert_eq!(t.name, "ProjectType");
        assert_eq!(t.canonical_concept.as_deref(), Some("project_type"));
        assert_eq!(t.language, Language::Unknown);
        assert_eq!(t.canonical_id(), Some(0x0106));
        let t_attr = |n: &str, ty: &str| {
            assert_eq!(
                t.attributes
                    .iter()
                    .find(|a| a.name == n)
                    .and_then(|a| a.type_name.as_deref()),
                Some(ty),
            );
        };
        t_attr("name", "string");
        t_attr("position", "integer");
        t_attr("is_default", "boolean");
        // ProjectType back-references ProjectWorkItem.
        let work_items = t
            .associations
            .iter()
            .find(|a| a.name == "work_items")
            .expect("work_items edge");
        assert_eq!(work_items.kind, AssociationKind::HasMany);
        assert_eq!(work_items.class_name.as_deref(), Some("ProjectWorkItem"));
    }

    #[test]
    fn project_status_and_type_resolver_collapses_curator_name_divergence() {
        // ProjectStatus: Redmine IssueStatus / OP Status — same id.
        let status_id = canonical_concept_id("project_status");
        assert!(status_id.is_some());
        for src in [
            "IssueStatus",
            "issuestatus",
            "issue_status",
            "Status",
            "statuses",
            "ProjectStatus",
            "projectstatus",
        ] {
            assert_eq!(canonical_concept(src), "project_status");
            assert_eq!(ogar_codebook(src), status_id, "{src} -> project_status id");
        }
        // ProjectType: Redmine Tracker / OP Type — same id.
        let type_id = canonical_concept_id("project_type");
        assert!(type_id.is_some());
        for src in [
            "Tracker",
            "trackers",
            "Type",
            "types",
            "ProjectType",
            "projecttype",
        ] {
            assert_eq!(canonical_concept(src), "project_type");
            assert_eq!(ogar_codebook(src), type_id, "{src} -> project_type id");
        }
        // The two canonical concepts have distinct ids.
        assert_ne!(status_id, type_id);
    }

    #[test]
    fn openproject_enrichment_does_not_break_redmine_ar_overlap() {
        // OpenProject's WorkPackage is the richer organism (extra includes
        // like `WorkPackages::SpentTime`, `WorkPackages::Costs`,
        // `WorkPackages::Relations`); Redmine's Issue is the cleaner AR
        // fossil. The agnostic vocab survives the evolution: both lift to
        // the same canonical concept.
        let mut redmine_issue = Class::new("Issue");
        redmine_issue.source_domain = Some("project".to_string());
        redmine_issue.canonical_concept = Some(canonical_concept("Issue"));
        redmine_issue.mixins = vec!["Redmine::Acts::Mentionable".to_string()];

        let mut op_wp = Class::new("WorkPackage");
        op_wp.source_domain = Some("project".to_string());
        op_wp.canonical_concept = Some(canonical_concept("WorkPackage"));
        op_wp.mixins = vec![
            "WorkPackages::SpentTime".to_string(),
            "WorkPackages::Costs".to_string(),
            "WorkPackages::Relations".to_string(),
            "WorkPackages::Scheduling".to_string(),
            "OpenProject::Journal::AttachmentHelper".to_string(),
        ];

        // OP is strictly richer than Redmine at the mixin axis ...
        assert!(op_wp.mixins.len() > redmine_issue.mixins.len());
        // ... yet the canonical concept is identical: enrichment did not
        // break the overlap.
        assert_eq!(redmine_issue.canonical_concept, op_wp.canonical_concept);
        assert_eq!(
            redmine_issue.canonical_concept.as_deref(),
            Some("project_work_item"),
        );
    }

    #[test]
    fn family_edges_internal_adapter_edges_external() {
        let bwe = billable_work_entry();
        // All 12 family-edge targets are ONTOLOGY concepts (PascalCase),
        // never curator / adapter surfaces — internal by construction.
        assert_eq!(bwe.associations.len(), 12);
        for e in &bwe.associations {
            let target = e.class_name.as_deref().unwrap_or_default();
            assert!(
                target.starts_with(|ch: char| ch.is_ascii_uppercase()),
                "family edge target must be an ontology concept: {target:?}",
            );
            for curator in [
                "TimeEntry",
                "account.",
                "account_",
                "OpenProject",
                "Odoo",
                "res.",
            ] {
                assert!(
                    !target.contains(curator),
                    "curator surface leaked into a family edge: {target:?}",
                );
            }
        }
        // The adapter edge lives OUT of family — on the curator class
        // (source_domain + canonical_concept), not among these edges.
        let mut op = Class::new("TimeEntry");
        op.source_domain = Some("project".to_string());
        op.canonical_concept = Some(canonical_concept("TimeEntry"));
        assert_eq!(op.canonical_concept.as_deref(), Some("billable_work_entry"));
        assert!(bwe.associations.iter().all(|e| e.name != "TimeEntry"));
    }

    #[test]
    fn elixir_language_is_a_distinct_first_class_variant() {
        // The OLD HIRO/Bardioc stack is Elixir; it is a first-class source
        // language (the migration-roundtrip bridge), not Unknown.
        let mut c = Class::new("Account");
        c.language = Language::Elixir;
        assert_eq!(c.language, Language::Elixir);
        assert_ne!(c.language, Language::Unknown);
        assert_ne!(c.language, Language::Ruby);
    }

    #[test]
    fn association_kind_belongs_to_default() {
        let a = Association::default();
        assert!(matches!(a.kind, AssociationKind::BelongsTo));
    }

    #[test]
    fn association_new_sets_kind_and_name() {
        let a = Association::new(AssociationKind::HasMany, "line_items");
        assert!(matches!(a.kind, AssociationKind::HasMany));
        assert_eq!(a.name, "line_items");
        assert!(a.scope_source.is_none());
    }

    #[test]
    fn association_scope_source_field_present() {
        let mut a = Association::new(AssociationKind::HasMany, "line_items");
        a.scope_source = Some("where(active: true)".into());
        assert_eq!(a.scope_source.as_deref(), Some("where(active: true)"));
    }

    #[test]
    fn callback_two_forms() {
        let method_form = Callback::method("before_save", "touch_parent");
        let block_form = Callback::block("after_create", "notify_subscribers");
        assert_ne!(method_form, block_form);
        assert!(method_form.target_method.is_some());
        assert!(method_form.body_source.is_none());
        assert!(block_form.body_source.is_some());
        assert!(block_form.target_method.is_none());
    }

    #[test]
    fn enter_effect_is_typed_and_constructible() {
        // EnterEffect replaces the free-form string carrier on ActionDef.on_enter
        // (per OGAR-AST-CONTRACT §6 follow-up); codegen applies the transition
        // structurally instead of string-parsing.
        let e = EnterEffect::transition("state", "sale");
        assert_eq!(e.field, "state");
        assert_eq!(e.to_value, "sale");
        assert_eq!(
            e,
            EnterEffect {
                field: "state".into(),
                to_value: "sale".into()
            }
        );
        assert_ne!(e, EnterEffect::default());
    }

    #[test]
    fn action_def_on_enter_is_typed_enter_effect() {
        // ActionDef.on_enter is now Option<EnterEffect>, not Option<String>.
        let mut a = ActionDef::default();
        assert!(a.on_enter.is_none());
        a.on_enter = Some(EnterEffect::transition("state", "sale"));
        assert_eq!(a.on_enter.as_ref().unwrap().field, "state");
        assert_eq!(a.on_enter.as_ref().unwrap().to_value, "sale");
    }

    // ── all_promoted_classes() — enumerator pinned to class_ids::ALL ──

    #[test]
    fn all_promoted_classes_matches_class_ids_all_in_length() {
        // Forward gate: the enumerator returns exactly the codebook's
        // count of promoted concepts. A new `ALL` entry that doesn't
        // get a constructor call here fails THIS test before any
        // consumer hits a drift.
        let classes = all_promoted_classes();
        assert_eq!(
            classes.len(),
            class_ids::ALL.len(),
            "all_promoted_classes() count ({}) must match class_ids::ALL ({})",
            classes.len(),
            class_ids::ALL.len(),
        );
    }

    #[test]
    fn all_promoted_classes_matches_class_ids_all_order() {
        // Tighter gate: each position in the enumerator produces a
        // class whose canonical_concept matches the same position in
        // class_ids::ALL. Order is part of the contract (drives
        // deterministic SurrealQL emission via emit_surrealql_ddl).
        let classes = all_promoted_classes();
        for (i, (expected_name, expected_id)) in class_ids::ALL.iter().enumerate() {
            let got = &classes[i];
            assert_eq!(
                got.canonical_concept.as_deref(),
                Some(*expected_name),
                "position {i}: expected canonical_concept `{expected_name}`",
            );
            assert_eq!(
                got.canonical_id(),
                Some(*expected_id),
                "position {i}: expected canonical_id 0x{expected_id:04X}",
            );
        }
    }

    #[test]
    fn all_promoted_classes_has_no_duplicates() {
        // Defensive: a sloppy copy-paste in the enumerator (two calls
        // to the same constructor) shows up here, even if both class
        // ids happen to be in the codebook.
        use std::collections::HashSet;
        let classes = all_promoted_classes();
        let mut seen: HashSet<&str> = HashSet::new();
        for c in &classes {
            let name = c
                .canonical_concept
                .as_deref()
                .expect("every promoted class must carry a canonical_concept");
            assert!(
                seen.insert(name),
                "duplicate `{name}` in all_promoted_classes()",
            );
        }
    }

    #[test]
    fn all_promoted_classes_every_class_has_canonical_id() {
        // Every entry must carry a `canonical_id()` — the class's bridge
        // out to the codebook id. A class that was constructed but
        // forgot to set its canonical_concept slips through `Class::new`
        // but fails here.
        for c in all_promoted_classes() {
            assert!(
                c.canonical_id().is_some(),
                "class `{}` is in all_promoted_classes() but has no canonical_id",
                c.name,
            );
        }
    }
}
