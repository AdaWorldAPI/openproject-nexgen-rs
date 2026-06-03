//! `ruff_ruby_spo` — Ruby/Rails frontend for the shared SPO triplet core.
//!
//! Sprint C17a: parser graduation. The crate now uses `lib-ruby-parser`
//! (pure-Rust typed AST) to extract class shape + association options from
//! `app/models/*.rb`. Sprint C4's line scanner is retained for `body_source`
//! traversal inside `fields.rs` / `functions.rs` (def-block detection,
//! ivar finding, identifier reads) — graduating those is the next step
//! per `RUBY-FRONTEND.md`.
//!
//! # What the AST adds on top of the line scanner
//!
//! The C4 scanner captured `belongs_to :project, …` as just the leading
//! symbol `"project"`. The AST captures the WHOLE call: macro name + leading
//! symbol + every option-hash entry it carries (`class_name:`, `through:`,
//! `polymorphic:`, `as:`, `source:`, `dependent:`, `optional:`,
//! `inverse_of:`, `foreign_key:`). Plus the class's superclass for STI
//! hierarchy tracking. The probe report at
//! `.claude/knowledge/c17-scanner-coverage-probes.md` (nexgen) lists what
//! this closes (G1, G2, G3, G4, G5, G6 there).
//!
//! # Pipeline shape
//!
//! 1. [`parse_models`] (in `parse`) walks an `app/models/` tree, parses each
//!    `*.rb` file with `lib_ruby_parser::Parser`, finds the top-level class
//!    node, and produces a [`RubyClass`] per ActiveRecord-style class.
//! 2. [`extract_fields`] / [`extract_functions`] scan the captured
//!    [`RubyClass::body_source`] using the C4 line primitives (still
//!    deterministic + dependency-free at that layer).
//! 3. [`extract`] wires both into a [`ModelGraph`].
//!
//! The downstream consumers (`lance_graph` SPO loader, `action_emitter`,
//! `link_chain`) need ZERO changes — they already consume the triple shape
//! this crate targets.

use std::path::Path;

use ruff_spo_triplet::{Model, ModelGraph};

mod fields;
mod functions;
mod parse;
mod scan;

/// The namespace prefix for OpenProject subjects/objects.
pub const NAMESPACE: &str = "openproject";

/// The four ActiveRecord association macros whose leading positional symbol
/// names a relation. Kept here (and re-used by `parse.rs`) so the boundary
/// between "association macro" and "ordinary class-body call" is defined
/// in one place.
pub(crate) const ASSOCIATION_MACROS: &[&str] = &[
    "belongs_to",
    "has_many",
    "has_one",
    "has_and_belongs_to_many",
];

/// A Rails `enum :column, { variant_a: 1, variant_b: 2 }, scopes: false`
/// declaration. The values dict is captured verbatim — variants in source
/// order with their literal value (`"1"`, `"active"`, …) as a string so
/// both int- and string-backed enums fit in one shape. C17b addition,
/// closes gap-probe G8.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnumDecl {
    /// The column the enum is backed by (`status`, `workspace_type`, …).
    pub column: String,
    /// Variant name → literal value, in declaration order. Value is
    /// stringified: `"1"` for int-backed enums, `"active"` for
    /// string-backed enums.
    pub values: Vec<(String, String)>,
    /// `scopes: false` was passed (disables Rails-generated `.active` /
    /// `.not_active` class-method scopes). `None` if `scopes:` was unset
    /// or had a non-bool value.
    pub scopes_disabled: Option<bool>,
}

/// A Rails `store_accessor :col, %i[a b c], prefix: true` declaration:
/// declares N JSONB pseudo-fields backed by the same column. C17b
/// addition, closes gap-probe G9.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreAccessorDecl {
    /// The JSONB column the pseudo-fields are backed by (`cause`,
    /// `metadata`, …).
    pub column: String,
    /// Pseudo-field names, in source order. Each is exposed at runtime
    /// as `<prefix>_<name>` (when `prefix: true`) or `<name>` (default).
    pub fields: Vec<String>,
    /// `prefix:` option as written. `Some(true)` means each field reads
    /// + writes as `<column>_<field>`; `Some(false)` or `None` means bare
    /// `<field>` accessors. (Rails also supports a String prefix; we
    /// collapse that here to "non-bool unset" and the column name in
    /// the field name resolution is handled by the consumer.)
    pub prefix: Option<bool>,
}

/// A Rails `attribute :name, :type, default: value` declaration —
/// schemaless / typed attribute override. C17b addition, closes
/// gap-probe G10.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeDecl {
    /// Attribute name as written (`subject`, `display_id`, …).
    pub name: String,
    /// Type name as a Sym (`:string`, `:integer`, `:big_integer`, …).
    /// `None` if the attribute call has no type positional arg (rare).
    pub type_name: Option<String>,
}

/// A `scope :name, -> { body }` definition (G15). The body is captured
/// verbatim as source text — Inferred-tier in the NARS vocabulary; consumers
/// either accept it as a SQL snippet or re-parse it for their needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeDecl {
    /// Scope name as declared (`recent`, `for_project`, …).
    pub name: String,
    /// Body source between the lambda's `{` and `}` (or `do`/`end`),
    /// trimmed of the bracket bytes themselves. Lambda arity is not
    /// captured separately — the body source still shows the param list
    /// indirectly via `where`/`order` references; explicit arity is a
    /// follow-up.
    pub body_source: String,
}

/// A lifecycle callback declaration (`before_save :method`, `after_create
/// do … end`, `around_destroy :method`, …). Closes G19.
///
/// Two source forms collapse here:
/// - `event :method_name` → `target_method = Some(name)`, `body_source = None`
/// - `event do ... end` → `target_method = None`, `body_source = Some(text)`
///
/// The event distinction (before/after/around) is preserved in [`Self::event`]
/// so consumers can reason about cascade vs. wrap semantics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallbackDecl {
    /// Callback event as written: `before_save`, `after_create`,
    /// `around_destroy`, `after_commit`, `after_create_commit`, …
    pub event: String,
    /// Method name target when the callback names a method
    /// (`before_save :method_name`). `None` for the block form.
    pub target_method: Option<String>,
    /// Block body source when the callback is `event do ... end` or
    /// `event { ... }`. `None` for the method-name form.
    pub body_source: Option<String>,
}

/// Parsed shape of a `belongs_to` / `has_many` / `has_one` /
/// `has_and_belongs_to_many` macro call. Captures both the leading symbol
/// (the relation name) and every option-hash entry the call carries.
///
/// **Why each option is here.** Each option in this struct corresponds to a
/// distinct piece of Rails semantics the C4 line scanner was blind to. See
/// the coverage probe report (`.claude/knowledge/c17-scanner-coverage-probes.md`
/// in nexgen) §"Universal gap taxonomy" for which gap (G1..G6) each closes.
///
/// **Field naming.** `source` / `as` collide with Rust reserved syntax in
/// some positions; we use `source` (safe as a field name) and `as_target`
/// (avoid a `r#as` field, which is uglier at call sites). Booleans
/// (`polymorphic`, `optional`) are `Option<bool>` so "unset" is
/// distinguishable from "explicitly false" — a small but real distinction
/// for the downstream NARS truth tier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssociationDecl {
    /// The macro name as written: `"belongs_to"`, `"has_many"`, `"has_one"`,
    /// `"has_and_belongs_to_many"`.
    pub macro_name: String,
    /// The leading positional symbol — the relation name (`"project"`,
    /// `"time_entries"`, …). Same as [`RubyClass::associations`] entries.
    pub name: String,
    /// `class_name: "Foo"` or `class_name: "Foo::Bar"` — the target type
    /// when it cannot be inferred from the relation name. `::`-namespaced
    /// names are preserved verbatim.
    pub class_name: Option<String>,
    /// `foreign_key: "user_id"` — the FK column on the owning table.
    pub foreign_key: Option<String>,
    /// `polymorphic: true` — on `belongs_to` this means the target is
    /// determined at runtime by a `<name>_type` column.
    pub polymorphic: Option<bool>,
    /// `through: :memberships` — for has_many/has_one, names the
    /// intermediate association the relation goes through.
    pub through: Option<String>,
    /// `source: :principal` — aliasing on a through-association: read the
    /// target via THIS association name on the through-target, instead of
    /// the relation's own leading symbol.
    pub source: Option<String>,
    /// `as: :container` — reverse-side polymorphism: `has_many :x, as: :y`
    /// means the OTHER end's belongs_to was polymorphic with type/id pair
    /// `y_type`/`y_id`.
    pub as_target: Option<String>,
    /// `dependent: :destroy` / `:delete_all` / `:nullify` / `:restrict_*`.
    pub dependent: Option<String>,
    /// `optional: true` — on `belongs_to`, allows the FK to be null.
    pub optional: Option<bool>,
    /// `inverse_of: :user` — the reciprocal relation on the target.
    pub inverse_of: Option<String>,
    /// `before_add: :method_name` — collection callback fired before an
    /// element is added to a has_many. C17c addition, closes G20 (collection
    /// callbacks). For block-form (`before_add: ->{...}`) the value is
    /// not captured here; only the method-symbol form.
    pub before_add: Option<String>,
    /// `after_add: :method_name`.
    pub after_add: Option<String>,
    /// `before_remove: :method_name`.
    pub before_remove: Option<String>,
    /// `after_remove: :method_name`. Project example: `has_many
    /// :enabled_modules, after_remove: :module_disabled`.
    pub after_remove: Option<String>,
}

/// A minimally-parsed Ruby class — what the parser frontend produces before
/// the IR mapping. Fields fall into two groups:
///
/// 1. **C4 line-scanner-compatible** (`name`, `body_source`, `associations`,
///    `columns`): preserved verbatim from the dependency-free scaffold so
///    `fields.rs` / `functions.rs` continue to work unchanged.
///
/// 2. **C17a parser-driven additions** (`superclass`, `association_options`):
///    require the AST. Populated by [`parse`] from `lib_ruby_parser` output.
#[derive(Debug, Clone, Default)]
pub struct RubyClass {
    /// Class name as written (`WorkPackage`). No dots in Ruby class names,
    /// so no normalisation needed (unlike Odoo's `account.move`).
    pub name: String,
    /// Raw source of the class body — `fields` / `functions` extractors
    /// scan this with [`scan`] primitives.
    pub body_source: String,
    /// Association names declared on the class (`belongs_to`, `has_many`,
    /// `has_one`, `has_and_belongs_to_many`). Seeds the set of valid
    /// relation names so a body call can be told apart from an ordinary
    /// method call. Same set as `association_options.iter().map(|a| &a.name)`,
    /// kept as a `Vec<String>` for the existing extractor consumers.
    pub associations: Vec<String>,
    /// Baseline DB columns for this class's table, seeded from `db/schema.rb`
    /// by [`parse`].
    pub columns: Vec<String>,
    /// Superclass name as written (`"ApplicationRecord"`, `"Principal"`,
    /// `"ActiveRecord::Base"`, …). C17a addition: enables STI hierarchy
    /// detection in downstream consumers — when this is a non-Record/Base
    /// model name, the class is an STI subtype of that model.
    pub superclass: Option<String>,
    /// Full per-association option set, in source order. Same length and
    /// order as [`Self::associations`]; the i-th [`AssociationDecl`]'s
    /// `.name` equals the i-th `associations` entry. C17a addition.
    pub association_options: Vec<AssociationDecl>,
    /// `include Foo` / `include Foo::Bar` mixin paths in declaration
    /// order. `::`-namespaced names preserved verbatim. C17b addition,
    /// closes gap-probe G14.
    pub concerns: Vec<String>,
    /// `enum :col, {...}, scopes: …` declarations, in source order.
    /// C17b addition, closes G8.
    pub enums: Vec<EnumDecl>,
    /// `store_accessor :col, %i[…], prefix: …` declarations, in source
    /// order. C17b addition, closes G9.
    pub store_accessors: Vec<StoreAccessorDecl>,
    /// `attribute :name, :type` declarations, in source order. C17b
    /// addition, closes G10.
    pub attributes: Vec<AttributeDecl>,
    /// `self.table_name = "..."` literal-string override. `None` if the
    /// class lets Rails infer the table name (the common case) or if
    /// the rhs is a dynamic expression (interpolated string, method
    /// call) — those leave the C17b extractor blind, which is the right
    /// answer for an under-extracting tier (the consumer should fall
    /// back to inflection or raise on the model). Closes G11 (partial).
    pub table_name_override: Option<String>,
    /// `self.inheritance_column = :_type_disabled` was set. Signals that
    /// the class deliberately opts OUT of STI dispatch even if its
    /// subclasses exist in the tree. C17b addition, closes G12.
    pub inheritance_column_disabled: bool,
    /// Columns listed in `self.ignored_columns += [...]` runtime blacklists.
    /// Captured in source order across however many `+=` statements appear.
    /// C17c addition, closes G13.
    pub ignored_columns: Vec<String>,
    /// `scope :name, -> { body }` definitions in source order. C17c
    /// addition, closes G15.
    pub scope_definitions: Vec<ScopeDecl>,
    /// `scopes :a, :b, :c` declarative-list scope names (a Scopes::Scoped
    /// DSL form that pre-declares scope class methods defined in concerns
    /// elsewhere). C17c addition, closes G16.
    pub scope_predeclarations: Vec<String>,
    /// `default_scope -> { body }` global filter body, when present.
    /// C17c addition, closes G17.
    pub default_scope_body: Option<String>,
    /// Lifecycle callback declarations (`before_save`, `after_create`,
    /// `around_destroy`, etc.) in source order. C17c addition, closes G19.
    pub callbacks: Vec<CallbackDecl>,
}

/// Top-level entry: walk a Rails `app/models/` tree and produce the IR.
///
/// The extraction work is split across three file-disjoint modules ([`parse`],
/// [`fields`], [`functions`]) sharing the [`scan`] primitives — see each
/// module for the Rails→IR mapping it implements.
#[must_use]
pub fn extract(source_tree: &Path) -> ModelGraph {
    let classes = parse::parse_models(source_tree);
    let mut graph = ModelGraph::new(NAMESPACE);
    for class in &classes {
        let mut model = Model::new(&class.name);
        model.fields = fields::extract_fields(class);
        model.functions = functions::extract_functions(class);
        graph.models.push(model);
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_spo_triplet::{expand, Field, Function};

    /// Locked target shape: a hand-built `ModelGraph` matching what a
    /// finished `extract()` MUST produce for a tiny OpenProject-like model.
    /// This test passes today (it does not call the `todo!()` extractors);
    /// it tells the frontend author what "done" looks like.
    fn locked_work_package_graph() -> ModelGraph {
        let mut graph = ModelGraph::new(NAMESPACE);
        graph.models.push(Model {
            name: "WorkPackage".to_string(),
            fields: vec![Field {
                name: "total_hours".to_string(),
                depends_on: vec!["time_entries.hours".to_string()],
                emitted_by: Some("compute_total_hours".to_string()),
            }],
            functions: vec![Function {
                name: "compute_total_hours".to_string(),
                reads: vec!["status".to_string()],
                raises: vec!["ActiveRecord::RecordInvalid".to_string()],
                traverses: vec!["time_entries".to_string()],
            }],
        });
        graph
    }

    #[test]
    fn locked_shape_expands_to_expected_triples() {
        let triples = expand(&locked_work_package_graph());
        let has =
            |s: &str, p: &str, o: &str| triples.iter().any(|t| t.s == s && t.p == p && t.o == o);

        // ObjectType / Property / Function classification.
        assert!(has(
            "openproject:WorkPackage",
            "rdf:type",
            "ogit:ObjectType"
        ));
        assert!(has(
            "openproject:WorkPackage.total_hours",
            "rdf:type",
            "ogit:Property"
        ));
        assert!(has(
            "openproject:WorkPackage.compute_total_hours",
            "rdf:type",
            "ogit:Function"
        ));
        // Compute graph edges.
        assert!(has(
            "openproject:WorkPackage.total_hours",
            "emitted_by",
            "openproject:WorkPackage.compute_total_hours"
        ));
        assert!(has(
            "openproject:WorkPackage.total_hours",
            "depends_on",
            "openproject:WorkPackage.time_entries.hours"
        ));
        // Guard + traversal.
        assert!(has(
            "openproject:WorkPackage.compute_total_hours",
            "raises",
            "exc:ActiveRecord::RecordInvalid"
        ));
        assert!(has(
            "openproject:WorkPackage.compute_total_hours",
            "traverses_relation",
            "openproject:WorkPackage.time_entries"
        ));
    }

    #[test]
    fn namespace_is_openproject() {
        let triples = expand(&locked_work_package_graph());
        assert!(
            triples
                .iter()
                .all(|t| { t.s.starts_with("openproject:") || t.s.starts_with("exc:") })
        );
    }
}
