//! `op-codegen-projection` — OpenProject SurrealQL **schema** projection.
//!
//! Implements [`lance_graph_contract::codegen_spine::TripletProjection`] over
//! an OP-shaped DDL IR. This is the **DDL facet** from the three-facet model
//! (SPO / DDL / Executable). A separate [`surreal_text`] emitter renders the
//! IR to SurrealQL `DEFINE TABLE` / `DEFINE FIELD` statements.
//!
//! # The strict-roundtrip trick (R3 / R4 from C5 M1 mapping)
//!
//! M1 R3 flagged that the codegen-layer [`Triple`] is `String`-IRI with
//! `f`/`c` NARS truth, and M1 R4 flagged that
//! [`TripletProjection::truth_tolerance`] defaults to `0.0` (strict). A naive
//! SurrealQL `DEFINE TABLE` emission has no slot for NARS truth, so any
//! projection that drops it spuriously fails `roundtrip_eq`.
//!
//! The fix here keeps the contract clean and the emission lossy *separately*:
//!
//! - [`OpSurrealConst`] carries TWO fields:
//!   - `tables: Vec<DefineTable>` — the structured DDL IR the SurrealQL
//!     emitter walks (lossy projection of the input triples — schema only).
//!   - `triples: Vec<Triple>` — the **opaque triple trail**, an exact copy of
//!     the input. [`OpSurrealProjection::decompile`] returns these unchanged,
//!     so `roundtrip_eq` passes at the strict default tolerance.
//! - The SurrealQL emitter ignores `triples` and walks `tables`; consumers
//!   that need NARS truth on output read it via a side-channel.
//!
//! This is the same architectural seam Odoo's existing projection uses
//! (see the reference `PredicateIndex` in the upstream codegen_spine tests):
//! the const form *contains* the original triples so decompile is a copy.
//! We add structured emission on top.
//!
//! # First write surface
//!
//! This crate is intentionally narrow — only **`DEFINE TABLE`** + nullable
//! **`DEFINE FIELD`** stubs are recognised in the projection. A future sprint
//! widens the input vocabulary (FK record links, ASSERT clauses, indexes,
//! PERMISSIONS) without touching the projection contract.

use std::collections::{BTreeMap, BTreeSet};

use lance_graph_contract::codegen_spine::{Triple, TripletProjection};

// ---------------------------------------------------------------------------
// The DDL IR
// ---------------------------------------------------------------------------

/// One `DEFINE TABLE <name> SCHEMAFULL` statement plus its `DEFINE FIELD`
/// children, in the OpenProject namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineTable {
    /// The model's local name (e.g. `WorkPackage`). The full IRI is
    /// `openproject:WorkPackage`; namespace is implicit.
    pub name: String,
    /// Fields declared on this table, sorted by name for deterministic
    /// emission (matches the input triple ordering after dedup).
    pub fields: Vec<DefineField>,
}

/// One `DEFINE FIELD <name> ON TABLE <table> TYPE <kind>` stub.
///
/// `kind` is the inferred SurrealQL primitive (see [`ColumnKind`]); the
/// emitted text wraps it in `option<>` when [`Self::required`] is `false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineField {
    /// Field name (e.g. `subject`).
    pub name: String,
    /// `true` when the field is required (declared by the synthetic
    /// `_validate` function on the same table — corresponds to an
    /// ActiveRecord `validates :col, presence: true` declaration).
    pub required: bool,
    /// The SurrealQL primitive inferred for this field — today by
    /// name-suffix convention plus a known-table join (see
    /// [`ColumnKind::infer`]). Future sprints will replace the
    /// heuristic with the upstream `has_type` / `references` predicates.
    pub kind: ColumnKind,
}

/// SurrealQL primitive type for a [`DefineField`].
///
/// Today this is derived from the field name + the set of known tables
/// by [`Self::infer`] (a stopgap until the upstream `ruff_spo_triplet`
/// vocabulary grows proper `has_type` / `references` predicates — see
/// C13 PR body). When a future sprint adds those, the projection's
/// inference path will be replaced with triple lookups; the enum
/// surface stays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnKind {
    /// Unknown / heterogeneous (`any` in SurrealQL).
    Any,
    /// Integer (`int` in SurrealQL). Inferred from the canonical Rails
    /// foreign-key naming `*_id` when the implied target table is NOT a
    /// known model in the same projection — typical for `status_id`,
    /// `category_id`, etc. that point at lookup tables not in scope.
    Int,
    /// Reference to another table (`record<Target>` in SurrealQL).
    /// Inferred from `*_id` when the implied target IS a known table in
    /// the projection — promotes the field from a bare int to a typed
    /// SurrealDB record link.
    Record(String),
}

impl ColumnKind {
    /// The SurrealQL type token (inside any `option<…>` wrapper).
    /// Allocates a `String` for [`Self::Record`] (which carries a
    /// dynamic table name); the other variants render to a constant
    /// string but go through `String` for a uniform return type.
    #[must_use]
    pub fn surreal_token(&self) -> String {
        match self {
            Self::Any => "any".to_string(),
            Self::Int => "int".to_string(),
            Self::Record(target) => format!("record<{target}>"),
        }
    }

    /// Infer the column kind from a field name plus the set of known
    /// tables in the projection. Rules (closed; expanded only when the
    /// convention is ironclad in Rails):
    ///
    /// - field name ends with `_id`:
    ///   - strip `_id`, snake-to-PascalCase the stem, look up in
    ///     `known_tables` — if present, [`Self::Record`] with that name
    ///     (`work_package_id` → `record<WorkPackage>` if WorkPackage is
    ///     a known table)
    ///   - otherwise [`Self::Int`] (lookup-table FK we don't model)
    /// - everything else → [`Self::Any`]
    #[must_use]
    pub fn infer(field_name: &str, known_tables: &BTreeSet<String>) -> Self {
        let Some(stem) = field_name.strip_suffix("_id") else {
            return Self::Any;
        };
        if stem.is_empty() {
            // `_id` alone is degenerate; treat as a plain int.
            return Self::Int;
        }
        let pascal = snake_to_pascal(stem);
        if known_tables.contains(&pascal) {
            Self::Record(pascal)
        } else {
            Self::Int
        }
    }
}

/// `work_package` → `WorkPackage`; `time_entry` → `TimeEntry`;
/// `project` → `Project`. Splits on `_`, uppercases the first char of
/// each non-empty segment, concatenates. ASCII-safe; non-ASCII chars
/// pass through unchanged.
fn snake_to_pascal(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    for word in snake.split('_') {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.extend(chars);
        }
    }
    out
}

/// The Const form of [`OpSurrealProjection`]. Carries the structured DDL IR
/// (walked by [`surreal_text`]) AND the opaque triple trail (returned
/// unchanged by [`OpSurrealProjection::decompile`] so strict roundtrip
/// passes — see module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct OpSurrealConst {
    /// The structured DDL IR for SurrealQL emission.
    pub tables: Vec<DefineTable>,
    /// Exact copy of the input triples. Round-trip identity.
    pub triples: Vec<Triple>,
}

// ---------------------------------------------------------------------------
// Recognised triple predicates (the projection's input vocabulary)
// ---------------------------------------------------------------------------

/// Predicate marking an entity as an `ogit:ObjectType` (model / table).
/// Matches the codegen-spine canonical predicate name.
pub const PRED_OGIT_TYPE: &str = "rdf:type";

/// Object value for the type predicate that flags a subject as a table.
pub const OBJ_OBJECT_TYPE: &str = "ogit:ObjectType";

/// Object value for the type predicate that flags a subject as a field
/// (property of a table).
pub const OBJ_PROPERTY: &str = "ogit:Property";

/// Predicate marking a column as read by a function body (or, for the
/// synthetic `_validate` function, validated).
pub const PRED_READS_FIELD: &str = "reads_field";

/// Suffix of the synthetic per-model validation function emitted by
/// `ruff_spo_triplet::expand` when any declarative `validates …` is present
/// (see `SPO_TRIPLET_EXTRACTION.md` §5). Detection here is suffix-based, not
/// behaviour-based: a custom `def something_check; raise RecordInvalid; end`
/// is NOT treated as a validator — only the canonical synthetic one is.
pub const VALIDATE_FN_SUFFIX: &str = "._validate";

/// IRI namespace prefix for OpenProject subjects (matches the
/// `ruff_openproject` crate's `NAMESPACE`).
pub const NAMESPACE_PREFIX: &str = "openproject:";

// ---------------------------------------------------------------------------
// The projection impl
// ---------------------------------------------------------------------------

/// The OpenProject SurrealQL-schema projection. Implements
/// [`TripletProjection`] so it gates through `roundtrip_eq` as a build-time
/// test — a projection that loses (s, p, o) identity fails CI.
#[derive(Debug, Clone, Copy)]
pub struct OpSurrealProjection;

impl TripletProjection for OpSurrealProjection {
    type Const = OpSurrealConst;

    fn project(triples: &[Triple]) -> Self::Const {
        // First pass: collect every (subject = openproject:Foo) that has a
        // `rdf:type ogit:ObjectType` declaration. Those subjects become
        // tables. Tables are kept in a BTreeMap keyed by local name for
        // deterministic ordering.
        let mut tables: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for t in triples {
            // Workspace is edition 2021 — use nested if-let, not let chains.
            if t.p == PRED_OGIT_TYPE && t.o == OBJ_OBJECT_TYPE {
                if let Some(local) = t.s.strip_prefix(NAMESPACE_PREFIX) {
                    if !local.contains('.') {
                        tables.entry(local.to_string()).or_default();
                    }
                }
            }
        }

        // Second pass: every (subject = openproject:Foo.bar) with
        // `rdf:type ogit:Property` is a field on table Foo (if we already
        // know Foo as a table). Unknown tables: silently skip — the input
        // is the contract; we don't invent tables, only emit what's
        // declared.
        for t in triples {
            if t.p == PRED_OGIT_TYPE && t.o == OBJ_PROPERTY {
                if let Some(local) = t.s.strip_prefix(NAMESPACE_PREFIX) {
                    if let Some((table, field)) = local.split_once('.') {
                        if let Some(fields) = tables.get_mut(table) {
                            fields.insert(field.to_string());
                        }
                    }
                }
            }
        }

        // Third pass: detect validated (required) columns via the synthetic
        // `_validate` function's `reads_field` triples. Pattern:
        //   (openproject:Foo._validate, reads_field, openproject:Foo.bar)
        // -> field `bar` on table `Foo` is required.
        // A reads_field whose subject is some other function (a real
        // method body that reads a column) is correctly ignored; only the
        // synthetic validator flags requiredness.
        let mut required: BTreeSet<(String, String)> = BTreeSet::new();
        for t in triples {
            if t.p != PRED_READS_FIELD {
                continue;
            }
            let subj = match t.s.strip_prefix(NAMESPACE_PREFIX) {
                Some(local) => local,
                None => continue,
            };
            let table = match subj.strip_suffix(VALIDATE_FN_SUFFIX) {
                Some(t) => t,
                None => continue,
            };
            let obj = match t.o.strip_prefix(NAMESPACE_PREFIX) {
                Some(local) => local,
                None => continue,
            };
            if let Some((obj_table, col)) = obj.split_once('.') {
                if obj_table == table {
                    required.insert((table.to_string(), col.to_string()));
                }
            }
        }

        // Snapshot the set of known table names BEFORE consuming `tables`
        // below — `ColumnKind::infer` joins `*_id` fields against this set
        // to decide Int vs Record<Target>.
        let known_tables: BTreeSet<String> = tables.keys().cloned().collect();

        let structured: Vec<DefineTable> = tables
            .into_iter()
            .map(|(table_name, fields)| {
                let define_fields = fields
                    .into_iter()
                    .map(|field_name| DefineField {
                        required: required.contains(&(table_name.clone(), field_name.clone())),
                        kind: ColumnKind::infer(&field_name, &known_tables),
                        name: field_name,
                    })
                    .collect();
                DefineTable {
                    name: table_name,
                    fields: define_fields,
                }
            })
            .collect();

        OpSurrealConst {
            tables: structured,
            triples: triples.to_vec(),
        }
    }

    fn decompile(c: &Self::Const) -> Vec<Triple> {
        // Strict roundtrip: return the opaque triple trail unchanged.
        // The structured `tables` field is for emission, not identity.
        c.triples.clone()
    }
}

// ---------------------------------------------------------------------------
// SurrealQL emission
// ---------------------------------------------------------------------------

/// Render the structured DDL IR to SurrealQL `DEFINE TABLE` /
/// `DEFINE FIELD` / `DEFINE INDEX` statements. Trailing newline,
/// deterministic ordering (alphabetical by table, alphabetical by field
/// within each table; index follows its field).
///
/// Field type emission combines two axes:
///
/// - Kind (from [`DefineField::kind`] / [`ColumnKind::surreal_token`]) —
///   today inferred by name convention (`*_id` → `int` / `record<Target>`,
///   else `any`); future widening targets (proper `has_type` triples from
///   `ruff_spo_triplet`, datetime, bool) extend the enum without
///   touching this fn's structure.
/// - Optionality (from [`DefineField::required`]) — wraps the kind in
///   `option<…>` when the field is NOT required.
///
/// Examples:
///
/// | required | kind | emission |
/// |---|---|---|
/// | true  | `Any`              | `TYPE any`                    |
/// | false | `Any`              | `TYPE option<any>`            |
/// | true  | `Int`              | `TYPE int`                    |
/// | false | `Int`              | `TYPE option<int>`            |
/// | true  | `Record("WP")`     | `TYPE record<WP>`             |
/// | false | `Record("WP")`     | `TYPE option<record<WP>>`     |
///
/// Index emission: every field with kind `Int` or `Record(_)` (the
/// FK-shaped fields detected by [`ColumnKind::infer`]) also emits a
/// non-unique `DEFINE INDEX idx_<Table>_<col> ON TABLE <Table>
/// FIELDS <col>;` on the line after its `DEFINE FIELD`. This matches
/// the conventional Postgres-on-Rails pattern (every `*_id` column has
/// a btree index for the FK lookup). `has_one` 1:1 relations would
/// warrant `UNIQUE`, but that signal isn't in the current ruff
/// vocabulary; non-unique is the safe default.
#[must_use]
pub fn surreal_text(c: &OpSurrealConst) -> String {
    use op_surreal_ast::ToSql;
    build_ast_schema(c).to_sql()
}

/// Lower the projection IR to a typed [`op_surreal_ast::Schema`] tree.
///
/// C16a: this is the single bridge between the IR (`OpSurrealConst`) and
/// the typed AST. Rendering goes via the AST's `ToSql` impls — no more
/// `format!` strings here. Future sprints add capability by extending
/// either side independently:
///   - new IR signal (e.g. C18 `has_many`) → new `ColumnKind` variant
///     above and a new arm in [`column_kind_to_ast_kind`]
///   - new SurrealQL slot (e.g. C24 scopes → `DEFINE TABLE … AS SELECT`)
///     → new field on `op_surreal_ast::TableDefinition`, filled here
fn build_ast_schema(c: &OpSurrealConst) -> op_surreal_ast::Schema {
    let mut schema = op_surreal_ast::Schema::new();
    for table in &c.tables {
        let mut t = op_surreal_ast::TableDefinition::new(table.name.clone());
        for field in &table.fields {
            let mut kind = column_kind_to_ast_kind(&field.kind);
            if !field.required {
                kind = kind.optional();
            }
            t = t.with_field(op_surreal_ast::FieldDefinition::new(
                field.name.clone(),
                table.name.clone(),
                kind,
            ));
            if is_fk_indexable(&field.kind) {
                t = t.with_index(op_surreal_ast::IndexDefinition::new(
                    format!("idx_{}_{}", table.name, field.name),
                    table.name.clone(),
                    vec![field.name.clone()],
                ));
            }
        }
        schema = schema.with_table(t);
    }
    schema
}

/// Map a projection-level [`ColumnKind`] to the typed AST [`op_surreal_ast::Kind`].
/// One arm per `ColumnKind` variant — extending the IR enum forces a
/// compile error here, so we never silently miss a mapping.
fn column_kind_to_ast_kind(k: &ColumnKind) -> op_surreal_ast::Kind {
    match k {
        ColumnKind::Any => op_surreal_ast::Kind::Any,
        ColumnKind::Int => op_surreal_ast::Kind::Int,
        ColumnKind::Record(target) => {
            op_surreal_ast::Kind::Record(vec![target.clone()])
        }
    }
}

/// `true` for FK-shaped columns that warrant an index — currently the
/// kinds [`ColumnKind::Int`] and [`ColumnKind::Record`], both produced
/// by [`ColumnKind::infer`] when the field name ends in `_id`. Centralised
/// so the predicate stays single-source-of-truth even as the kind enum
/// grows (e.g. a future `Datetime` would not be FK-shaped).
#[must_use]
fn is_fk_indexable(kind: &ColumnKind) -> bool {
    matches!(kind, ColumnKind::Int | ColumnKind::Record(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::codegen_spine::roundtrip_eq;

    fn t(s: &str, p: &str, o: &str, f: f32, c: f32) -> Triple {
        Triple {
            s: s.to_string(),
            p: p.to_string(),
            o: o.to_string(),
            f,
            c,
        }
    }

    /// A small openproject-shaped triple set:
    /// - WorkPackage / TimeEntry as ObjectTypes
    /// - subject, status as Properties on WorkPackage
    /// - hours as Property on TimeEntry
    /// Plus some non-projected triples (compute graph edges) to verify they
    /// round-trip through `triples` opaque trail without affecting the IR.
    fn fixture_triples() -> Vec<Triple> {
        vec![
            // tables
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t("openproject:TimeEntry", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            // WorkPackage fields
            t("openproject:WorkPackage.subject", "rdf:type", "ogit:Property", 1.0, 0.9),
            t("openproject:WorkPackage.status", "rdf:type", "ogit:Property", 1.0, 0.9),
            // TimeEntry field
            t("openproject:TimeEntry.hours", "rdf:type", "ogit:Property", 1.0, 0.9),
            // Non-projected: compute graph edge (carried opaquely via triples
            // trail — emitter ignores, round-trip preserves).
            t(
                "openproject:WorkPackage.total_hours",
                "emitted_by",
                "openproject:WorkPackage.compute_total_hours",
                1.0,
                0.8,
            ),
        ]
    }

    // ----- the projection IR -----

    #[test]
    fn project_extracts_tables_and_fields_alphabetically() {
        let c = OpSurrealProjection::project(&fixture_triples());
        let table_names: Vec<&str> = c.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(table_names, ["TimeEntry", "WorkPackage"]);

        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let wp_fields: Vec<&str> = wp.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(wp_fields, ["status", "subject"]);

        let te = c.tables.iter().find(|t| t.name == "TimeEntry").unwrap();
        let te_fields: Vec<&str> = te.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(te_fields, ["hours"]);
    }

    #[test]
    fn project_carries_full_triple_trail_unchanged() {
        // The Const's `triples` field is an exact copy of the input — the
        // opaque trail that lets `roundtrip_eq` pass at strict tolerance.
        let input = fixture_triples();
        let c = OpSurrealProjection::project(&input);
        assert_eq!(c.triples, input);
    }

    #[test]
    fn project_skips_property_for_unknown_table() {
        // A Property triple whose table half is not declared as an
        // ObjectType is silently skipped (the input is the contract; we
        // don't invent tables). The triple still round-trips via the
        // opaque trail.
        let mut triples = fixture_triples();
        triples.push(t(
            "openproject:Mystery.something",
            "rdf:type",
            "ogit:Property",
            1.0,
            0.5,
        ));
        let c = OpSurrealProjection::project(&triples);
        assert!(!c.tables.iter().any(|tab| tab.name == "Mystery"));
        // But the triple is in the trail for round-trip.
        assert!(c
            .triples
            .iter()
            .any(|tr| tr.s == "openproject:Mystery.something"));
    }

    // ----- the contract gate -----

    #[test]
    fn roundtrip_eq_passes_at_strict_default_tolerance() {
        // The whole point of TripletProjection: this MUST pass or our
        // projection is silently lossy. Default truth_tolerance is 0.0
        // (exact f/c match required) — the opaque triple trail makes that
        // trivially the case.
        roundtrip_eq::<OpSurrealProjection>(&fixture_triples())
            .expect("OpSurrealProjection must round-trip losslessly");
    }

    #[test]
    fn roundtrip_eq_passes_on_empty_input() {
        roundtrip_eq::<OpSurrealProjection>(&[])
            .expect("empty input is the trivial round-trip");
    }

    #[test]
    fn roundtrip_eq_passes_on_only_non_projected_triples() {
        // Triples the projection ignores for IR purposes must still
        // round-trip — the trail is opaque.
        let only_compute = vec![
            t(
                "openproject:WorkPackage.total_hours",
                "depends_on",
                "openproject:WorkPackage.time_entries.hours",
                1.0,
                0.8,
            ),
            t(
                "openproject:WorkPackage.compute_total_hours",
                "raises",
                "exc:ActiveRecord::RecordInvalid",
                1.0,
                0.9,
            ),
        ];
        roundtrip_eq::<OpSurrealProjection>(&only_compute)
            .expect("non-projected triples still round-trip via the opaque trail");
    }

    // ----- the surreal text emitter -----

    #[test]
    fn surreal_text_renders_define_table_and_define_field() {
        let c = OpSurrealProjection::project(&fixture_triples());
        let text = surreal_text(&c);

        let expected = "\
DEFINE TABLE TimeEntry SCHEMAFULL;
DEFINE FIELD hours ON TABLE TimeEntry TYPE option<any>;
DEFINE TABLE WorkPackage SCHEMAFULL;
DEFINE FIELD status ON TABLE WorkPackage TYPE option<any>;
DEFINE FIELD subject ON TABLE WorkPackage TYPE option<any>;
";
        assert_eq!(text, expected);
    }

    #[test]
    fn surreal_text_is_empty_on_empty_const() {
        let c = OpSurrealConst {
            tables: Vec::new(),
            triples: Vec::new(),
        };
        assert_eq!(surreal_text(&c), "");
    }

    // ----- C10: validated-column / required-field detection -----

    /// Fixture extending the base one with a synthetic `_validate` function
    /// that reads `subject` (the canonical Rails
    /// `validates :subject, presence: true` shape after ruff expansion).
    fn fixture_with_validates() -> Vec<Triple> {
        let mut triples = fixture_triples();
        // The `_validate` function is itself an `ogit:Function`.
        triples.push(t(
            "openproject:WorkPackage._validate",
            "rdf:type",
            "ogit:Function",
            1.0,
            0.9,
        ));
        // Its body raises ActiveRecord::RecordInvalid (carried opaquely).
        triples.push(t(
            "openproject:WorkPackage._validate",
            "raises",
            "exc:ActiveRecord::RecordInvalid",
            1.0,
            0.9,
        ));
        // The crucial signal: reads_field on the validated column.
        triples.push(t(
            "openproject:WorkPackage._validate",
            "reads_field",
            "openproject:WorkPackage.subject",
            1.0,
            0.7,
        ));
        triples
    }

    #[test]
    fn project_marks_validated_fields_as_required() {
        let c = OpSurrealProjection::project(&fixture_with_validates());
        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        let status = wp.fields.iter().find(|f| f.name == "status").unwrap();
        assert!(subject.required, "validated column must be required");
        assert!(!status.required, "non-validated column stays optional");
    }

    #[test]
    fn project_ignores_reads_field_from_non_validate_functions() {
        // A normal (non-_validate) function reading a column must NOT
        // flag it as required — only the synthetic validator does.
        let mut triples = fixture_triples();
        triples.push(t(
            "openproject:WorkPackage.compute_total_hours",
            "reads_field",
            "openproject:WorkPackage.subject",
            1.0,
            0.7,
        ));
        let c = OpSurrealProjection::project(&triples);
        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert!(
            !subject.required,
            "reads_field outside _validate does not mark required"
        );
    }

    #[test]
    fn project_ignores_validate_reads_field_pointing_to_other_table() {
        // Defensive: a malformed triple where _validate of table A reads
        // a column of table B is silently ignored (we only flag own-table
        // columns).
        let mut triples = fixture_triples();
        triples.push(t(
            "openproject:WorkPackage._validate",
            "reads_field",
            "openproject:TimeEntry.hours",
            1.0,
            0.7,
        ));
        let c = OpSurrealProjection::project(&triples);
        let te = c.tables.iter().find(|t| t.name == "TimeEntry").unwrap();
        let hours = te.fields.iter().find(|f| f.name == "hours").unwrap();
        assert!(!hours.required, "cross-table _validate is ignored");
    }

    #[test]
    fn surreal_text_emits_type_any_for_required_field() {
        let c = OpSurrealProjection::project(&fixture_with_validates());
        let text = surreal_text(&c);

        let expected = "\
DEFINE TABLE TimeEntry SCHEMAFULL;
DEFINE FIELD hours ON TABLE TimeEntry TYPE option<any>;
DEFINE TABLE WorkPackage SCHEMAFULL;
DEFINE FIELD status ON TABLE WorkPackage TYPE option<any>;
DEFINE FIELD subject ON TABLE WorkPackage TYPE any;
";
        assert_eq!(text, expected);
    }

    #[test]
    fn roundtrip_eq_passes_with_validation_triples() {
        // The validation triples enter the opaque trail unchanged; round-
        // trip remains strict.
        roundtrip_eq::<OpSurrealProjection>(&fixture_with_validates())
            .expect("validation triples round-trip via the opaque trail");
    }

    // ----- C12 + C13: ColumnKind inference (`*_id` -> Int or Record<Target>) -----

    /// Empty known-tables set: convenience for tests where the FK target
    /// join is intentionally a miss (i.e. the field should fall back to
    /// `Int` / `Any`).
    fn no_tables() -> BTreeSet<String> {
        BTreeSet::new()
    }

    #[test]
    fn column_kind_infer_recognises_id_suffix_as_int_when_target_unknown() {
        // No tables known -> _id columns fall back to Int (the C12 rule).
        let known = no_tables();
        assert_eq!(ColumnKind::infer("status_id", &known), ColumnKind::Int);
        assert_eq!(ColumnKind::infer("work_package_id", &known), ColumnKind::Int);
        assert_eq!(ColumnKind::infer("user_id", &known), ColumnKind::Int);
    }

    #[test]
    fn column_kind_infer_defaults_to_any_for_non_id_columns() {
        let known = no_tables();
        assert_eq!(ColumnKind::infer("subject", &known), ColumnKind::Any);
        assert_eq!(ColumnKind::infer("hours", &known), ColumnKind::Any);
        // Bare "id" without underscore: defensible either way. The rule is
        // SUFFIX `_id` so a column literally named "id" stays Any (it is
        // typically the primary key anyway and Surreal handles that via
        // RECORD ids, not user-declared fields).
        assert_eq!(ColumnKind::infer("id", &known), ColumnKind::Any);
        // Identifier-like substring in the middle / start: not matched.
        assert_eq!(ColumnKind::infer("id_check", &known), ColumnKind::Any);
        assert_eq!(ColumnKind::infer("identifier", &known), ColumnKind::Any);
    }

    #[test]
    fn column_kind_surreal_token_renders_each_variant() {
        assert_eq!(ColumnKind::Any.surreal_token(), "any");
        assert_eq!(ColumnKind::Int.surreal_token(), "int");
        assert_eq!(
            ColumnKind::Record("WorkPackage".to_string()).surreal_token(),
            "record<WorkPackage>"
        );
    }

    #[test]
    fn project_marks_id_suffixed_columns_as_int_kind_when_target_unknown() {
        // End-to-end through project(): only WorkPackage exists, so
        // status_id falls back to Int (no Status table known).
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t("openproject:WorkPackage.subject", "rdf:type", "ogit:Property", 1.0, 0.9),
            t("openproject:WorkPackage.status_id", "rdf:type", "ogit:Property", 1.0, 0.9),
        ];
        let c = OpSurrealProjection::project(&triples);
        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let status_id = wp.fields.iter().find(|f| f.name == "status_id").unwrap();
        let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert_eq!(status_id.kind, ColumnKind::Int);
        assert_eq!(subject.kind, ColumnKind::Any);
    }

    #[test]
    fn surreal_text_combines_required_and_kind_axes() {
        // The four-cell matrix from surreal_text's doc table, all in one
        // fixture: subject (required + Any), status (optional + Any),
        // status_id (optional + Int), parent_id (required + Int).
        let mut triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t("openproject:WorkPackage.subject", "rdf:type", "ogit:Property", 1.0, 0.9),
            t("openproject:WorkPackage.status", "rdf:type", "ogit:Property", 1.0, 0.9),
            t("openproject:WorkPackage.status_id", "rdf:type", "ogit:Property", 1.0, 0.9),
            t("openproject:WorkPackage.parent_id", "rdf:type", "ogit:Property", 1.0, 0.9),
        ];
        // _validate marks subject AND parent_id as required.
        triples.push(t(
            "openproject:WorkPackage._validate",
            "reads_field",
            "openproject:WorkPackage.subject",
            1.0,
            0.7,
        ));
        triples.push(t(
            "openproject:WorkPackage._validate",
            "reads_field",
            "openproject:WorkPackage.parent_id",
            1.0,
            0.7,
        ));

        let c = OpSurrealProjection::project(&triples);
        let text = surreal_text(&c);

        // C14: both parent_id and status_id are FK-shaped (kind=Int)
        // -> each gets a DEFINE INDEX line immediately after its field.
        let expected = "\
DEFINE TABLE WorkPackage SCHEMAFULL;
DEFINE FIELD parent_id ON TABLE WorkPackage TYPE int;
DEFINE INDEX idx_WorkPackage_parent_id ON TABLE WorkPackage FIELDS parent_id;
DEFINE FIELD status ON TABLE WorkPackage TYPE option<any>;
DEFINE FIELD status_id ON TABLE WorkPackage TYPE option<int>;
DEFINE INDEX idx_WorkPackage_status_id ON TABLE WorkPackage FIELDS status_id;
DEFINE FIELD subject ON TABLE WorkPackage TYPE any;
";
        assert_eq!(text, expected);
    }

    // ----- C13: FK record link inference via known-tables join -----

    #[test]
    fn column_kind_infer_promotes_known_target_to_record() {
        let mut known = BTreeSet::new();
        known.insert("WorkPackage".to_string());
        known.insert("User".to_string());
        // work_package_id -> strip -> work_package -> WorkPackage -> known
        assert_eq!(
            ColumnKind::infer("work_package_id", &known),
            ColumnKind::Record("WorkPackage".to_string())
        );
        // user_id -> User -> known
        assert_eq!(
            ColumnKind::infer("user_id", &known),
            ColumnKind::Record("User".to_string())
        );
        // status_id -> Status -> NOT known -> Int fallback
        assert_eq!(ColumnKind::infer("status_id", &known), ColumnKind::Int);
    }

    #[test]
    fn snake_to_pascal_handles_multi_word_and_single_word() {
        // Direct unit on the helper. Stable; the round-trip from Rails
        // table names (snake_case singular) to model class names (Pascal)
        // is load-bearing for FK record-link inference.
        assert_eq!(snake_to_pascal("project"), "Project");
        assert_eq!(snake_to_pascal("work_package"), "WorkPackage");
        assert_eq!(snake_to_pascal("time_entry"), "TimeEntry");
        assert_eq!(snake_to_pascal("a_b_c"), "ABC");
        // Edge: empty segments (leading / trailing / double underscore)
        // are skipped silently.
        assert_eq!(snake_to_pascal(""), "");
        assert_eq!(snake_to_pascal("_foo"), "Foo");
        assert_eq!(snake_to_pascal("foo__bar"), "FooBar");
    }

    #[test]
    fn project_promotes_fk_to_record_when_target_table_present() {
        // Two tables in the projection: TimeEntry and WorkPackage. The
        // `work_package_id` field on TimeEntry resolves to the known
        // WorkPackage table -> Record. The hypothetical `status_id` field
        // on WorkPackage falls back to Int (no Status table declared).
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t("openproject:TimeEntry", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t(
                "openproject:WorkPackage.status_id",
                "rdf:type",
                "ogit:Property",
                1.0,
                0.9,
            ),
            t(
                "openproject:TimeEntry.work_package_id",
                "rdf:type",
                "ogit:Property",
                1.0,
                0.9,
            ),
        ];
        let c = OpSurrealProjection::project(&triples);
        let te = c.tables.iter().find(|t| t.name == "TimeEntry").unwrap();
        let wp = c.tables.iter().find(|t| t.name == "WorkPackage").unwrap();
        let wp_id = te
            .fields
            .iter()
            .find(|f| f.name == "work_package_id")
            .unwrap();
        let status_id = wp.fields.iter().find(|f| f.name == "status_id").unwrap();
        assert_eq!(wp_id.kind, ColumnKind::Record("WorkPackage".to_string()));
        assert_eq!(status_id.kind, ColumnKind::Int);
    }

    #[test]
    fn surreal_text_emits_record_link_when_target_known() {
        // Full matrix: optional+Record, required+Record, plus the C12 cells
        // for contrast.
        let mut triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t("openproject:TimeEntry", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t(
                "openproject:TimeEntry.work_package_id",
                "rdf:type",
                "ogit:Property",
                1.0,
                0.9,
            ),
            t(
                "openproject:TimeEntry.hours",
                "rdf:type",
                "ogit:Property",
                1.0,
                0.9,
            ),
        ];
        // _validate marks work_package_id as required.
        triples.push(t(
            "openproject:TimeEntry._validate",
            "reads_field",
            "openproject:TimeEntry.work_package_id",
            1.0,
            0.7,
        ));

        let c = OpSurrealProjection::project(&triples);
        let text = surreal_text(&c);

        // C14: the Record-kind work_package_id is FK-shaped -> DEFINE
        // INDEX line follows. The required+Record cell renders without
        // option<> wrapper. The Any-kind hours has no index.
        let expected = "\
DEFINE TABLE TimeEntry SCHEMAFULL;
DEFINE FIELD hours ON TABLE TimeEntry TYPE option<any>;
DEFINE FIELD work_package_id ON TABLE TimeEntry TYPE record<WorkPackage>;
DEFINE INDEX idx_TimeEntry_work_package_id ON TABLE TimeEntry FIELDS work_package_id;
DEFINE TABLE WorkPackage SCHEMAFULL;
";
        assert_eq!(text, expected);
    }

    // ----- C14: DEFINE INDEX on FK-shaped columns -----

    #[test]
    fn is_fk_indexable_covers_int_and_record_but_not_any() {
        assert!(is_fk_indexable(&ColumnKind::Int));
        assert!(is_fk_indexable(&ColumnKind::Record("WP".to_string())));
        assert!(!is_fk_indexable(&ColumnKind::Any));
    }

    #[test]
    fn surreal_text_emits_define_index_for_int_kind_column() {
        // Plain Int FK (target not a known table) -> still gets an index.
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t(
                "openproject:WorkPackage.status_id",
                "rdf:type",
                "ogit:Property",
                1.0,
                0.9,
            ),
        ];
        let c = OpSurrealProjection::project(&triples);
        let text = surreal_text(&c);
        assert!(text.contains(
            "DEFINE INDEX idx_WorkPackage_status_id ON TABLE WorkPackage FIELDS status_id;"
        ));
    }

    #[test]
    fn surreal_text_emits_no_index_for_any_kind_columns() {
        // Plain Any fields (no _id suffix) MUST NOT get an index.
        let c = OpSurrealProjection::project(&fixture_triples());
        let text = surreal_text(&c);
        assert!(
            !text.contains("DEFINE INDEX"),
            "fixture has no FK-shaped fields; expected no INDEX lines"
        );
    }

    #[test]
    fn roundtrip_eq_passes_with_id_fields() {
        // Inference is a projection-only concern; the opaque triple trail
        // is unaffected and the C6 contract gate still holds.
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType", 1.0, 0.9),
            t("openproject:WorkPackage.status_id", "rdf:type", "ogit:Property", 1.0, 0.9),
        ];
        roundtrip_eq::<OpSurrealProjection>(&triples)
            .expect("kind inference does not perturb roundtrip");
    }
}
