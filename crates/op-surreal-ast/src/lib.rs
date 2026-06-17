//! `op-surreal-ast` — typed SurrealQL DDL AST for the codegen pipeline.
//!
//! # Mirror-layout — what this is and isn't
//!
//! The structs below mirror the **DDL-relevant** slots of surrealdb-core's
//! `catalog::TableDefinition` / `catalog::schema::FieldDefinition` /
//! `catalog::schema::IndexDefinition` (and the `sql::statements::define::*`
//! they lower to), field-for-field where the field carries DDL meaning.
//! Intentionally **omitted** are the runtime/catalog-internal fields
//! (`namespace_id`, `database_id`, `table_id`, `cache_*_ts`) — those exist
//! to track in-DB state, not to describe a schema.
//!
//! Why mirror instead of importing surrealdb-core directly:
//!
//! 1. `surrealdb-core::sql::statements::DefineTableStatement` is `pub(crate)`
//!    — not importable from outside the crate.
//! 2. `catalog::TableDefinition` IS `pub`, but its fields are `pub(crate)`
//!    and its constructor demands runtime IDs (`new(ns_id, db_id, ...)`)
//!    + caches `Uuid`s — wrong shape for static DDL emission.
//! 3. surrealdb-core's full build pulls tokio + async-graphql + rust 1.95
//!    + (optional) lance/rocksdb — disproportional weight for "emit DDL".
//!
//! This crate is the **bridge**: typed slots that match the canonical
//! shape so a future `From<op_surreal_ast::TableDefinition> for
//! catalog::TableDefinition` impl (once the fork adds DDL-friendly
//! setters in C16b) is mechanical.
//!
//! # Output equivalence
//!
//! [`ToSql::to_sql`] on a [`Schema`] returns a string **byte-identical** to
//! the previous `format!`-based emission in `op-codegen-projection`. That's
//! the pin for the C16a refactor: same output, typed mechanics.

use std::fmt::Write;

pub mod from_triples;
pub use from_triples::triples_to_schema;

// ---------------------------------------------------------------------------
// ToSql trait — mirrored from surrealdb_types::sql::ToSql.
//
// Defined locally so this crate has zero external deps. Signature matches
// upstream; swap-out to `surrealdb_types::ToSql` is a one-line change once
// C16b lands the fork.
// ---------------------------------------------------------------------------

/// Convert a typed AST node to a SurrealQL string fragment.
///
/// Mirrors `surrealdb_types::ToSql` (single required method, two default
/// methods). Implementors append to a borrowed `String` so callers can
/// compose a whole `Schema` render in one allocation.
pub trait ToSql {
    /// Render this node as single-line SurrealQL.
    fn to_sql(&self) -> String {
        let mut f = String::new();
        self.fmt_sql(&mut f, SqlFormat::SingleLine);
        f
    }

    /// Render this node as pretty-printed SurrealQL with indentation.
    fn to_sql_pretty(&self) -> String {
        let mut f = String::new();
        self.fmt_sql(&mut f, SqlFormat::Indented(0));
        f
    }

    /// Append this node's SurrealQL form to `f`.
    fn fmt_sql(&self, f: &mut String, fmt: SqlFormat);
}

/// SurrealQL render formatting mode. Mirrors `surrealdb_types::SqlFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlFormat {
    /// Everything on one line (semicolon-terminated statements separated
    /// only by `\n`). This is what the C9–C15 pipeline emits.
    SingleLine,
    /// Indented by N tab levels. Currently unused by the projection but
    /// kept so the trait surface matches upstream byte-for-byte.
    Indented(u8),
}

// ---------------------------------------------------------------------------
// Kind — field type expression.
//
// Mirrors a strict subset of surrealdb-core::expr::kind::Kind:
//   - Any   ↔ Kind::Any
//   - Int   ↔ Kind::Int
//   - Record(Vec<String>) ↔ Kind::Record(Vec<Table>)  (single-target for now)
//   - Option(Box<Kind>)   ↔ Kind::Option(Box<Kind>)
//
// Variants we'll add in later sprints (one variant per Rails-mapping sprint,
// not pre-emptively):
//   - String, Bool, Datetime, Decimal, Uuid, Bytes
//   - Array(Box<Kind>)         — has_many → array<record<…>>
//   - Set(Box<Kind>)
//   - Either(Vec<Kind>)        — polymorphic associations → record<A|B|C>
//   - Literal(Literal)         — STI discriminator value
// ---------------------------------------------------------------------------

/// SurrealQL field-type expression. Strict subset of upstream `Kind`;
/// extended one variant per Rails-mapping sprint.
///
/// **D-AR-5.2** added the 7 Rails-scalar types — `String`, `Bool`,
/// `Float`, `Decimal`, `Datetime`, `Bytes`, `Uuid` — so attributes
/// extracted with an explicit `attribute :name, :type` annotation map
/// to a concrete SurrealQL kind instead of the catch-all `Any`. These
/// match the variants the surrealdb-core `op_bridge` already expects
/// in its `From<ast::Kind> for catalog::Kind` impl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// `any` — untyped slot, the universal supertype.
    Any,
    /// `int` — signed integer.
    Int,
    /// `string` — UTF-8 text (Rails `:string` / `:text`).
    String,
    /// `bool` — boolean (Rails `:boolean`).
    Bool,
    /// `float` — IEEE 754 double (Rails `:float`).
    Float,
    /// `decimal` — fixed-precision decimal (Rails `:decimal`).
    Decimal,
    /// `datetime` — RFC 3339 instant (Rails `:datetime` / `:timestamp`
    /// / `:date` / `:time` all collapse to this — SurrealQL doesn't
    /// distinguish at the schema level).
    Datetime,
    /// `bytes` — binary blob (Rails `:binary`).
    Bytes,
    /// `uuid` — UUID (Rails `:uuid`).
    Uuid,
    /// `record<Target>` — link to a row in another table. The vector
    /// holds the candidate target tables; today always length 1, but the
    /// shape supports `record<A|B|C>` for the C19 polymorphic-association
    /// sprint without changing this variant's layout.
    Record(Vec<String>),
    /// `option<Inner>` — nullable wrapper. Use [`Kind::optional`] to
    /// build these; the `Box` keeps the enum sized.
    Option(Box<Kind>),
}

impl Kind {
    /// Wrap `self` in `option<…>`. Idempotent: wrapping an already-optional
    /// kind returns it unchanged (matches upstream's `Kind::make_optional`
    /// semantics — `option<option<T>>` is normalised to `option<T>`).
    #[must_use]
    pub fn optional(self) -> Self {
        match self {
            Self::Option(_) => self,
            other => Self::Option(Box::new(other)),
        }
    }

    /// Map a Rails type literal (`"integer"`, `"string"`, `"decimal"`,
    /// `"datetime"`, …) to the corresponding SurrealQL kind.
    /// Returns `None` for unknown Rails types so the caller can fall
    /// back to `Kind::Any`.
    ///
    /// D-AR-5.2: `op_surreal_ast::from_triples` calls this on the
    /// object of `field_type` triples emitted by `ruff_spo_triplet`.
    ///
    /// Covers both Rails ActiveModel::Type symbols (verbatim from the
    /// parser — `:integer`, `:big_integer`, `:immutable_string`, etc.)
    /// AND the PostgreSQL column-type aliases that Rails surfaces
    /// (`bigint`).
    ///
    /// **Width note**: PostgreSQL `bigint` is an 8-byte signed integer
    /// that maps cleanly to SurrealQL `int` (also i64). Rails'
    /// `:big_integer` is a different beast — `ActiveModel::Type::BigInteger`
    /// wraps Ruby's arbitrary-precision `Integer`, so values outside
    /// the i64 range are valid. Mapping it to `Kind::Int` would make
    /// the generated schema reject values the Rails app happily
    /// stores (codex P2 on #37), so `:big_integer` lowers to
    /// `Kind::Decimal` — SurrealDB's `decimal` is arbitrary-precision.
    #[must_use]
    pub fn from_rails_type(rails_type: &str) -> Option<Self> {
        Some(match rails_type {
            "integer" | "bigint" => Self::Int,
            // Rails ActiveModel::Type::BigInteger is arbitrary-precision
            // (Ruby Integer); narrow to Decimal not Int.
            "big_integer" => Self::Decimal,
            "string" | "text" | "immutable_string" => Self::String,
            "boolean" => Self::Bool,
            "float" => Self::Float,
            "decimal" | "numeric" => Self::Decimal,
            // Rails maps several time-typed columns to instants; SurrealQL
            // bundles them under `datetime` (Date/Time/Datetime/Timestamp).
            "datetime" | "timestamp" | "date" | "time" => Self::Datetime,
            "binary" => Self::Bytes,
            "uuid" => Self::Uuid,
            _ => return None,
        })
    }
}

impl ToSql for Kind {
    fn fmt_sql(&self, f: &mut String, fmt: SqlFormat) {
        match self {
            Self::Any => f.push_str("any"),
            Self::Int => f.push_str("int"),
            Self::String => f.push_str("string"),
            Self::Bool => f.push_str("bool"),
            Self::Float => f.push_str("float"),
            Self::Decimal => f.push_str("decimal"),
            Self::Datetime => f.push_str("datetime"),
            Self::Bytes => f.push_str("bytes"),
            Self::Uuid => f.push_str("uuid"),
            Self::Record(targets) => {
                f.push_str("record<");
                for (i, t) in targets.iter().enumerate() {
                    if i != 0 {
                        f.push('|');
                    }
                    f.push_str(t);
                }
                f.push('>');
            }
            Self::Option(inner) => {
                f.push_str("option<");
                inner.fmt_sql(f, fmt);
                f.push('>');
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TableDefinition — DDL-relevant slots of surrealdb's catalog::TableDefinition.
//
// Slots present today (DDL-meaningful):
//   - name, drop, schemafull, comment, table_type
//
// Slots reserved (upstream has them; we don't fill them yet, will in later
// sprints):
//   - view: Option<ViewDefinition>     — C24 (scopes → DEFINE TABLE AS SELECT)
//   - permissions: Permissions          — C(later) auth sprint
//   - changefeed: Option<ChangeFeed>    — out of scope for codegen
//
// Children (rendered after the DEFINE TABLE line, in deterministic order):
//   - fields: Vec<FieldDefinition>
//   - indices: Vec<IndexDefinition>
// ---------------------------------------------------------------------------

/// One `DEFINE TABLE …` plus its `DEFINE FIELD` / `DEFINE INDEX` children.
///
/// Mirrors `surrealdb_core::catalog::TableDefinition` for the slots that
/// influence DDL output. Runtime/catalog fields (namespace/database/table
/// IDs, `cache_*_ts`) are intentionally absent — see the module docstring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDefinition {
    pub name: String,
    pub drop: bool,
    pub schemafull: bool,
    pub comment: Option<String>,
    pub table_type: TableType,
    pub fields: Vec<FieldDefinition>,
    pub indices: Vec<IndexDefinition>,
}

impl TableDefinition {
    /// New schemafull NORMAL table with no comment, no children. Builder
    /// pattern for additions: `.with_field(…).with_index(…)`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            drop: false,
            schemafull: true,
            comment: None,
            table_type: TableType::Normal,
            fields: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Push a field definition. Returns `self` for chaining.
    #[must_use]
    pub fn with_field(mut self, field: FieldDefinition) -> Self {
        self.fields.push(field);
        self
    }

    /// Push an index definition. Returns `self` for chaining.
    #[must_use]
    pub fn with_index(mut self, index: IndexDefinition) -> Self {
        self.indices.push(index);
        self
    }

    /// Attach a table-level `COMMENT '<text>'` clause. Returns `self`
    /// for chaining. `None` clears any prior comment.
    #[must_use]
    pub fn with_comment(mut self, comment: Option<String>) -> Self {
        self.comment = comment;
        self
    }
}

/// Mirrors `surrealdb_core::catalog::TableType`. Variants for future
/// sprints:
///   - `Relation { from: Vec<String>, to: Vec<String> }` — C18 has_many
///     :through → `DEFINE TABLE … TYPE RELATION IN A OUT B`
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TableType {
    /// `TYPE NORMAL` — the implicit default for record tables. Today we
    /// don't emit the `TYPE NORMAL` clause (matches the C9 baseline);
    /// rendering stays silent on this variant.
    #[default]
    Normal,
}

impl ToSql for TableDefinition {
    fn fmt_sql(&self, f: &mut String, fmt: SqlFormat) {
        f.push_str("DEFINE TABLE ");
        f.push_str(&self.name);
        if self.schemafull {
            f.push_str(" SCHEMAFULL");
        }
        if let Some(c) = &self.comment {
            f.push_str(" COMMENT '");
            // Escape single quotes — SurrealQL's COMMENT uses single-
            // quoted strings, so any embedded `'` doubles up.
            for ch in c.chars() {
                if ch == '\'' {
                    f.push_str("''");
                } else {
                    f.push(ch);
                }
            }
            f.push('\'');
        }
        f.push_str(";\n");
        for field in &self.fields {
            field.fmt_sql(f, fmt);
            for index in self
                .indices
                .iter()
                .filter(|i| i.follows_field(&field.name))
            {
                index.fmt_sql(f, fmt);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FieldDefinition — DDL slots of surrealdb's catalog::schema::FieldDefinition.
//
// Slots reserved (upstream has them; later sprints fill them):
//   - flexible, readonly: bool          — C(later) schema flexibility
//   - value: Option<Expr>               — computed default
//   - assert: Option<Expr>              — C23 validations → ASSERT
//   - computed: Option<Expr>            — virtual fields
//   - default: DefineDefault            — Rails default: option
//   - select/create/update_permission   — auth sprint
//   - reference: Option<Reference>      — graph reference metadata
// ---------------------------------------------------------------------------

/// One `DEFINE FIELD …` statement.
///
/// Mirrors `surrealdb_core::catalog::schema::FieldDefinition` DDL slots.
///
/// `assert` carries the field's `ASSERT <expr>` clause when non-`None`
/// — D-AR-5.1 wires Rails `validates_constraint` triples into this
/// slot. The expression is rendered verbatim after the `TYPE` clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDefinition {
    pub name: String,
    pub table: String,
    pub kind: Kind,
    pub assert: Option<String>,
}

impl FieldDefinition {
    #[must_use]
    pub fn new(name: impl Into<String>, table: impl Into<String>, kind: Kind) -> Self {
        Self {
            name: name.into(),
            table: table.into(),
            kind,
            assert: None,
        }
    }

    /// Set the `ASSERT <expr>` clause. Returns `self` for chaining.
    /// `None` clears the assertion.
    #[must_use]
    pub fn with_assert(mut self, expr: Option<String>) -> Self {
        self.assert = expr;
        self
    }
}

impl ToSql for FieldDefinition {
    fn fmt_sql(&self, f: &mut String, fmt: SqlFormat) {
        f.push_str("DEFINE FIELD ");
        f.push_str(&self.name);
        f.push_str(" ON TABLE ");
        f.push_str(&self.table);
        f.push_str(" TYPE ");
        self.kind.fmt_sql(f, fmt);
        if let Some(expr) = &self.assert {
            f.push_str(" ASSERT ");
            f.push_str(expr);
        }
        f.push_str(";\n");
    }
}

// ---------------------------------------------------------------------------
// IndexDefinition — DDL slots of surrealdb's catalog::schema::IndexDefinition.
//
// Slots reserved (later sprints):
//   - unique: bool                      — C(later) has_one → UNIQUE
//   - search: Option<Search>            — full-text search
//   - mtree/hnsw: vector index variants — out of OP-codegen scope
// ---------------------------------------------------------------------------

/// One `DEFINE INDEX …` statement. Follows the field it indexes in the
/// emission stream — [`TableDefinition::fmt_sql`] places each index right
/// after its target field via [`Self::follows_field`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDefinition {
    pub name: String,
    pub table: String,
    pub fields: Vec<String>,
}

impl IndexDefinition {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        table: impl Into<String>,
        fields: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            table: table.into(),
            fields,
        }
    }

    /// True if this index covers a single field with the given name.
    /// Used by [`TableDefinition::fmt_sql`] to interleave each index
    /// right after its target `DEFINE FIELD` line — matching the C14
    /// emission order pinned by tests.
    #[must_use]
    pub(crate) fn follows_field(&self, field_name: &str) -> bool {
        self.fields.len() == 1 && self.fields[0] == field_name
    }
}

impl ToSql for IndexDefinition {
    fn fmt_sql(&self, f: &mut String, _fmt: SqlFormat) {
        f.push_str("DEFINE INDEX ");
        f.push_str(&self.name);
        f.push_str(" ON TABLE ");
        f.push_str(&self.table);
        f.push_str(" FIELDS ");
        for (i, field) in self.fields.iter().enumerate() {
            if i != 0 {
                f.push_str(", ");
            }
            f.push_str(field);
        }
        f.push_str(";\n");
    }
}

// ---------------------------------------------------------------------------
// Schema — top-level container holding the ordered tables of a render.
//
// Surrealdb itself has no single "Schema" struct; this is our convenience
// wrapper so callers can `schema.to_sql()` once instead of looping. The
// rendering visits tables in their stored order (the caller is responsible
// for sorting — op-codegen-projection already sorts alphabetically).
// ---------------------------------------------------------------------------

/// Ordered set of [`TableDefinition`]s — the codegen pipeline's emission
/// unit. Rendering is `tables[0].to_sql() ++ tables[1].to_sql() ++ …`,
/// preserving caller-supplied order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    pub tables: Vec<TableDefinition>,
}

impl Schema {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_table(mut self, table: TableDefinition) -> Self {
        self.tables.push(table);
        self
    }
}

impl ToSql for Schema {
    fn fmt_sql(&self, f: &mut String, fmt: SqlFormat) {
        for table in &self.tables {
            table.fmt_sql(f, fmt);
        }
    }
}

// Silence the `Write` import — kept around in case future ToSql impls
// use `write!` for formatted output. Not used by the current renderers
// (all `push_str`-only) but matches the upstream pattern.
#[allow(dead_code)]
fn _suppress_write_warning(f: &mut String) -> std::fmt::Result {
    write!(f, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_any_renders_as_any() {
        assert_eq!(Kind::Any.to_sql(), "any");
    }

    #[test]
    fn kind_int_renders_as_int() {
        assert_eq!(Kind::Int.to_sql(), "int");
    }

    /// **D-AR-5.2** — the 7 Rails scalar kinds render to their
    /// SurrealQL keywords byte-identically. Lock each one.
    #[test]
    fn kind_scalars_render_to_surrealql_keywords() {
        assert_eq!(Kind::String.to_sql(), "string");
        assert_eq!(Kind::Bool.to_sql(), "bool");
        assert_eq!(Kind::Float.to_sql(), "float");
        assert_eq!(Kind::Decimal.to_sql(), "decimal");
        assert_eq!(Kind::Datetime.to_sql(), "datetime");
        assert_eq!(Kind::Bytes.to_sql(), "bytes");
        assert_eq!(Kind::Uuid.to_sql(), "uuid");
    }

    /// **D-AR-5.2** — `option<scalar>` round-trips through the
    /// optional wrapper. Picks the most common typed nullable
    /// shape: `option<string>` (Rails nullable `:text` column).
    #[test]
    fn kind_option_of_scalar_renders_with_option_wrapper() {
        assert_eq!(Kind::String.optional().to_sql(), "option<string>");
        assert_eq!(Kind::Datetime.optional().to_sql(), "option<datetime>");
        assert_eq!(Kind::Bool.optional().to_sql(), "option<bool>");
    }

    #[test]
    fn kind_record_single_target_renders_as_record_of_target() {
        let k = Kind::Record(vec!["WorkPackage".to_string()]);
        assert_eq!(k.to_sql(), "record<WorkPackage>");
    }

    #[test]
    fn kind_record_multi_target_renders_with_pipe_separator() {
        let k = Kind::Record(vec!["User".to_string(), "Group".to_string()]);
        assert_eq!(k.to_sql(), "record<User|Group>");
    }

    #[test]
    fn kind_option_wraps_inner() {
        let k = Kind::Int.optional();
        assert_eq!(k.to_sql(), "option<int>");
    }

    #[test]
    fn kind_option_normalises_double_optional() {
        let k = Kind::Int.optional().optional();
        assert_eq!(k.to_sql(), "option<int>");
    }

    #[test]
    fn kind_option_of_record_nests_correctly() {
        let k = Kind::Record(vec!["WP".to_string()]).optional();
        assert_eq!(k.to_sql(), "option<record<WP>>");
    }

    #[test]
    fn field_definition_renders_with_table_and_kind() {
        let f =
            FieldDefinition::new("hours", "TimeEntry", Kind::Any);
        assert_eq!(
            f.to_sql(),
            "DEFINE FIELD hours ON TABLE TimeEntry TYPE any;\n"
        );
    }

    #[test]
    fn field_definition_renders_optional_record() {
        let f = FieldDefinition::new(
            "work_package_id",
            "TimeEntry",
            Kind::Record(vec!["WorkPackage".to_string()]).optional(),
        );
        assert_eq!(
            f.to_sql(),
            "DEFINE FIELD work_package_id ON TABLE TimeEntry TYPE option<record<WorkPackage>>;\n"
        );
    }

    #[test]
    fn index_definition_single_field_renders() {
        let i = IndexDefinition::new(
            "idx_TimeEntry_work_package_id",
            "TimeEntry",
            vec!["work_package_id".to_string()],
        );
        assert_eq!(
            i.to_sql(),
            "DEFINE INDEX idx_TimeEntry_work_package_id ON TABLE TimeEntry FIELDS work_package_id;\n"
        );
    }

    #[test]
    fn index_definition_multi_field_renders_with_comma() {
        let i = IndexDefinition::new(
            "idx_a_b",
            "Foo",
            vec!["a".to_string(), "b".to_string()],
        );
        assert_eq!(
            i.to_sql(),
            "DEFINE INDEX idx_a_b ON TABLE Foo FIELDS a, b;\n"
        );
    }

    #[test]
    fn table_definition_schemafull_renders_define_table_line() {
        let t = TableDefinition::new("WorkPackage");
        assert_eq!(t.to_sql(), "DEFINE TABLE WorkPackage SCHEMAFULL;\n");
    }

    #[test]
    fn table_definition_renders_fields_in_insertion_order() {
        let t = TableDefinition::new("WP")
            .with_field(FieldDefinition::new("a", "WP", Kind::Any))
            .with_field(FieldDefinition::new("b", "WP", Kind::Int));
        let expected = "\
DEFINE TABLE WP SCHEMAFULL;
DEFINE FIELD a ON TABLE WP TYPE any;
DEFINE FIELD b ON TABLE WP TYPE int;
";
        assert_eq!(t.to_sql(), expected);
    }

    #[test]
    fn table_definition_interleaves_indices_after_their_field() {
        let t = TableDefinition::new("TimeEntry")
            .with_field(FieldDefinition::new("hours", "TimeEntry", Kind::Any))
            .with_field(FieldDefinition::new(
                "work_package_id",
                "TimeEntry",
                Kind::Record(vec!["WorkPackage".to_string()]).optional(),
            ))
            .with_index(IndexDefinition::new(
                "idx_TimeEntry_work_package_id",
                "TimeEntry",
                vec!["work_package_id".to_string()],
            ));
        let expected = "\
DEFINE TABLE TimeEntry SCHEMAFULL;
DEFINE FIELD hours ON TABLE TimeEntry TYPE any;
DEFINE FIELD work_package_id ON TABLE TimeEntry TYPE option<record<WorkPackage>>;
DEFINE INDEX idx_TimeEntry_work_package_id ON TABLE TimeEntry FIELDS work_package_id;
";
        assert_eq!(t.to_sql(), expected);
    }

    #[test]
    fn schema_renders_tables_in_order() {
        let s = Schema::new()
            .with_table(TableDefinition::new("A"))
            .with_table(TableDefinition::new("B"));
        let expected = "\
DEFINE TABLE A SCHEMAFULL;
DEFINE TABLE B SCHEMAFULL;
";
        assert_eq!(s.to_sql(), expected);
    }

    #[test]
    fn schema_empty_renders_empty_string() {
        let s = Schema::new();
        assert_eq!(s.to_sql(), "");
    }

    #[test]
    fn rails_mini_e2e_byte_for_byte_with_legacy_emission() {
        // This is the C9-C15 reference output (see PR #19, hands-on
        // verification block). Building it via the typed AST must produce
        // a byte-identical string — that's the C16a invariant.
        let schema = Schema::new()
            .with_table(
                TableDefinition::new("TimeEntry")
                    .with_field(FieldDefinition::new(
                        "hours",
                        "TimeEntry",
                        Kind::Any,
                    ))
                    .with_field(FieldDefinition::new(
                        "work_package_id",
                        "TimeEntry",
                        Kind::Record(vec!["WorkPackage".to_string()]).optional(),
                    ))
                    .with_index(IndexDefinition::new(
                        "idx_TimeEntry_work_package_id",
                        "TimeEntry",
                        vec!["work_package_id".to_string()],
                    )),
            )
            .with_table(
                TableDefinition::new("WorkPackage")
                    .with_field(FieldDefinition::new(
                        "status_id",
                        "WorkPackage",
                        Kind::Int.optional(),
                    ))
                    .with_index(IndexDefinition::new(
                        "idx_WorkPackage_status_id",
                        "WorkPackage",
                        vec!["status_id".to_string()],
                    ))
                    .with_field(FieldDefinition::new(
                        "subject",
                        "WorkPackage",
                        Kind::Any,
                    )),
            );
        let expected = "\
DEFINE TABLE TimeEntry SCHEMAFULL;
DEFINE FIELD hours ON TABLE TimeEntry TYPE any;
DEFINE FIELD work_package_id ON TABLE TimeEntry TYPE option<record<WorkPackage>>;
DEFINE INDEX idx_TimeEntry_work_package_id ON TABLE TimeEntry FIELDS work_package_id;
DEFINE TABLE WorkPackage SCHEMAFULL;
DEFINE FIELD status_id ON TABLE WorkPackage TYPE option<int>;
DEFINE INDEX idx_WorkPackage_status_id ON TABLE WorkPackage FIELDS status_id;
DEFINE FIELD subject ON TABLE WorkPackage TYPE any;
";
        assert_eq!(schema.to_sql(), expected);
    }
}
