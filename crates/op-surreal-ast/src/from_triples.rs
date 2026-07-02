//! `from_triples` — D-AR-5 consumer: build a [`crate::Schema`] from the
//! SPO triples emitted by `ruff_ruby_spo` / `ruff_spo_triplet`.
//!
//! This is the consumer side of the
//! `openproject-ar-shape-extraction-v1` plan (PR Z): downstream of the
//! 27-predicate vocab + Model IR landed by `AdaWorldAPI/ruff#5` and the
//! lib-ruby-parser AST extractor landed by `AdaWorldAPI/ruff#6`, this
//! module takes a `Vec<Triple>` (via
//! `lance_graph_contract::codegen_spine::Triple`, the canonical
//! cross-language carrier) and projects it onto the typed
//! DDL AST in this crate.
//!
//! # Predicates consumed (skeleton)
//!
//! - `rdf:type` with object `ogit:ObjectType` → one
//!   [`crate::TableDefinition`] per subject.
//! - `has_attribute` → one [`crate::FieldDefinition`] per attribute on
//!   the owning table, `Kind::Any` (the OpenProject AR-shape vocab
//!   does not carry static types yet; D-AR-5.1 will fold in
//!   `attribute :x, :type` option-level type info).
//! - `has_field` + `field_type` + `column_not_null` (ruff D-AR-3.5,
//!   the schema stratum) → one typed [`crate::FieldDefinition`] per
//!   physical column; `column_not_null` renders the kind bare instead
//!   of `option<…>`.
//! - `declares_association` → one [`crate::FieldDefinition`] per
//!   association, `Kind::Record([<TargetClass>]).optional()`. Target
//!   class name follows Rails convention (camelcase singular of the
//!   relation name); the convention may be overridden by future
//!   `class_name:` option triples (D-AR-5.2).
//!
//! Every other predicate in the 34-name vocab is recognised by name
//! (so the catch-all `rdf:type`/`has_function`/etc. don't cause a
//! pass to fail) but does not yet influence the Schema — D-AR-5.1
//! folds in callbacks → SurrealQL events, validations → ASSERT,
//! scopes → TYPE RELATION, etc.
//!
//! # Round-trip status
//!
//! D-AR-5 is the **forward** projection only. The full
//! `TripletProjection` round-trip impl (decompile a `Schema` back
//! into `Vec<Triple>`) lands as D-AR-5.3 — it requires the full
//! attribute-options surface to recover the type slot.

use std::collections::BTreeMap;

use lance_graph_contract::codegen_spine::Triple;

use crate::{FieldDefinition, IndexDefinition, Kind, Schema, TableDefinition};

/// Project a flat triple stream into a typed [`Schema`].
///
/// Output table order: ASCII-sorted by table name (deterministic).
/// Field order within each table: insertion order from the triple
/// stream; the caller is responsible for any pre-sort.
///
/// Idempotent for duplicate triples (de-duplicated by `(s, p, o)`).
#[must_use]
pub fn triples_to_schema(triples: &[Triple]) -> Schema {
    // Group declarations by table IRI (the subject of an
    // `rdf:type` / `has_attribute` / `declares_association` triple).
    // Two-pass to avoid phantom tables: pass 1 finds every subject with
    // an explicit `rdf:type ObjectType` declaration; pass 2 only
    // populates fields/indices on tables seen in pass 1. This guards
    // against truncated triple streams where a body-walk predicate
    // like `reads_field openproject:Missing.name` would otherwise
    // materialise a phantom `Missing` table (codex P2-1).
    let mut tables: BTreeMap<String, TableBuilder> = BTreeMap::new();
    for t in triples {
        if t.p == "rdf:type" && t.o == "ogit:ObjectType" {
            let Some((table_iri, _)) = split_subject(&t.s) else {
                continue;
            };
            tables
                .entry(table_iri.clone())
                .or_insert_with(|| TableBuilder::new(strip_namespace(&table_iri).to_string()));
        }
    }

    // Snapshot the set of known class names (post-strip-namespace) once,
    // so the inner loop can borrow `tables` mutably while still checking
    // membership. Polymorphic associations (`belongs_to :ownable,
    // polymorphic: true`) name a non-existent class; this set lets us
    // fall back to `option<any>` instead of inventing a phantom
    // `record<Ownable>`.
    let known_targets: std::collections::HashSet<String> =
        tables.values().map(|tb| tb.name.clone()).collect();

    // Pre-collect `association_kind` triples (ruff#15) into a map from
    // relation IRI → Rails macro name. The `declares_association` arm
    // below uses this to gate FK-column emission: only `belongs_to`
    // puts a column on the declaring class, so `has_many` / `has_one` /
    // `habtm` / `accepts_nested_attributes_for` get a table-level
    // annotation instead of a phantom column.
    //
    // Missing kind triple (older ndjson without the predicate) falls
    // back to `belongs_to` semantics for backward compatibility — the
    // pre-#15 behaviour treated every association as a FK declaration,
    // so defaulting to `belongs_to` preserves it.
    let assoc_kinds: BTreeMap<String, String> = triples
        .iter()
        .filter(|t| t.p == "association_kind")
        .map(|t| (t.s.clone(), t.o.clone()))
        .collect();

    // Pre-collect `class_name` triples (ruff D-AR-5.4 — emitted when the
    // Rails `AssocDecl.options` carries a `class_name: 'X'` override).
    // Keyed by the relation IRI (`openproject:WorkPackage.owner` →
    // `"User"`). The `declares_association` arm uses these to override
    // the Rails camelcase-singular convention when resolving FK targets.
    //
    // Without the override, `belongs_to :owner, class_name: 'User'`
    // would lower to `option<record<Owner>>` — and `Owner` isn't a real
    // table on the OP corpus, so the polymorphic fallback degrades it
    // to `option<any>`. The override restores the typed link to the
    // intended target.
    let class_name_overrides: BTreeMap<String, String> = triples
        .iter()
        .filter(|t| t.p == "class_name")
        .map(|t| (t.s.clone(), t.o.clone()))
        .collect();

    // Pre-collect `field_type` triples (ruff `attribute :name, :type`
    // option lowering — D-AR-5.2) into a map keyed by the field IRI
    // (`openproject:WorkPackage.subject` → `"string"`). The
    // `has_attribute` arm below looks up the kind and uses
    // `Kind::from_rails_type` to upgrade `Kind::Any` to a concrete
    // SurrealQL kind. Unknown Rails types fall back to `Any`.
    //
    // Nullability: Rails attributes are nullable by default — so the
    // typed kind is wrapped in `option<…>` to preserve the
    // catch-all NONE acceptance. The `validates_constraint` arm
    // already adds `ASSERT $value != NONE` for validated attributes,
    // so the schema-level non-null gate stays correct.
    let field_types: BTreeMap<String, String> = triples
        .iter()
        .filter(|t| t.p == "field_type")
        .map(|t| (t.s.clone(), t.o.clone()))
        .collect();

    // Pre-collect `column_not_null` triples (ruff D-AR-3.5 — the schema
    // stratum's physical NOT NULL constraint from the migration DSL).
    // A field IRI in this set renders as a BARE kind instead of
    // `option<…>`: the DSL guarantees the column can't hold NULL, so
    // the option wrapper would misdeclare it. This is the "ORM shape
    // as bridge" axis — physical nullability types the field; the AR
    // shape (associations) still owns the field's identity/kind.
    let not_null: std::collections::HashSet<String> = triples
        .iter()
        .filter(|t| t.p == "column_not_null" && t.o == "true")
        .map(|t| t.s.clone())
        .collect();

    // Pre-collect `validation_kind` triples (ruff#21 — sibling to
    // `validates_constraint`). Keyed by the attribute IRI, value is
    // a set of recognised Rails kinds (`presence`, `uniqueness`,
    // `numericality`, etc.). The `validates_constraint` arm below
    // uses this to compose richer SurrealQL ASSERT clauses instead
    // of the catch-all `$value != NONE`.
    //
    // Multiple kinds per attribute compose with AND; an absent
    // attribute (no `validation_kind` triple) falls back to the
    // pre-ruff#21 `$value != NONE` ASSERT.
    let mut validation_kinds: BTreeMap<String, std::collections::BTreeSet<String>> =
        BTreeMap::new();
    for t in triples {
        if t.p == "validation_kind" {
            validation_kinds
                .entry(t.s.clone())
                .or_default()
                .insert(t.o.clone());
        }
    }

    // Pre-collect `validation_param` triples (ruff#25 — sibling to
    // `validation_kind` carrying inner-hash option values for
    // parametric Rails validators like
    // `length: { maximum: 255 }`). Keyed by the attribute IRI, value
    // is a set of `<kind>:<inner_key>=<value>` strings.
    //
    // The composer reads these alongside the kinds to lift Rails
    // parametric validators into typed SurrealQL clauses
    // (`length:maximum=N` → `string::len($value) <= N`,
    // `numericality:greater_than=N` → `$value > N`, etc.). Absent
    // params keep the pre-ruff#25 fallback (catch-all presence
    // ASSERT) — additive change.
    let mut validation_params: BTreeMap<String, std::collections::BTreeSet<String>> =
        BTreeMap::new();
    for t in triples {
        if t.p == "validation_param" {
            validation_params
                .entry(t.s.clone())
                .or_default()
                .insert(t.o.clone());
        }
    }

    // Asserts are buffered and applied AFTER the field-population pass
    // so the `validates_constraint` → ASSERT wiring is independent of
    // triple stream order (codex P2 PR #27 r…). A
    // `validates_constraint` triple that lands before its companion
    // `has_attribute` would otherwise miss the field and drop the
    // assertion permanently.
    let mut pending_asserts: Vec<(String, String, String)> = Vec::new();

    for t in triples {
        let Some((table_iri, member)) = split_subject(&t.s) else {
            continue;
        };
        let Some(builder) = tables.get_mut(&table_iri) else {
            // Subject without an `rdf:type ObjectType` declaration —
            // drop body-walk / declarative triples that would otherwise
            // materialise a phantom table.
            continue;
        };

        match t.p.as_str() {
            "rdf:type" if t.o == "ogit:ObjectType" => {
                // Already added in the table-discovery pass above.
            }
            "has_attribute" => {
                let attr_name = member.unwrap_or_else(|| {
                    strip_namespace(&t.o)
                        .rsplit('.')
                        .next()
                        .unwrap_or("")
                        .to_string()
                });
                // Look up the Rails `attribute :name, :type` annotation
                // for this field (ruff#15+ emits the companion
                // `field_type` triple keyed by `<model>.<attr>`). When
                // present and mappable, the field upgrades from
                // `Kind::Any` to a typed `option<T>`; Rails attributes
                // are nullable by default so the option wrapper keeps
                // NONE legal until a `validates_constraint` adds an
                // explicit `ASSERT $value != NONE`.
                let field_iri = format!("{table_iri}.{attr_name}");
                let kind = field_types
                    .get(&field_iri)
                    .and_then(|rails_type| Kind::from_rails_type(rails_type))
                    .map_or(Kind::Any, Kind::optional);
                builder.add_field(attr_name, kind);
            }
            "has_field" => {
                // Schema-stratum column (ruff D-AR-3.5): subject is the
                // model, object the field IRI
                // (`openproject:WorkPackage.subject`). Same typed-kind
                // lookup as `has_attribute`, plus the physical
                // nullability axis: `column_not_null` renders the kind
                // bare instead of `option<…>`.
                //
                // Stream order note: `expand()` sorts triples by
                // (s, p, o), so a model's `declares_association`
                // triples land BEFORE its `has_field` triples —
                // `belongs_to`-derived `record<Target>` FK fields win
                // the name, and the physical column no-ops here (the
                // AR shape owns the kind; see the two-shapes doctrine).
                let field_iri = t.o.clone();
                let name = strip_namespace(&t.o)
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let base = field_types
                    .get(&field_iri)
                    .and_then(|ty| Kind::from_rails_type(ty));
                let required = not_null.contains(&field_iri);
                let kind = match (&base, required) {
                    (Some(k), true) => k.clone(),
                    (Some(k), false) => k.clone().optional(),
                    // Unknown DSL type: `any` (which already admits
                    // NONE, so no option wrapper either way).
                    (None, _) => Kind::Any,
                };
                if builder.add_field(name.clone(), kind) && base.is_none() && required {
                    // Physically NOT NULL but no mappable type: `any`
                    // can't carry the constraint in TYPE, so gate it
                    // with the same assert the validations use.
                    pending_asserts.push((table_iri.clone(), name, "$value != NONE".to_string()));
                }
            }
            "declares_association" => {
                // Object is `openproject:WorkPackage.project` —
                // relation name is the last dotted segment, target
                // class follows Rails camelcase singular convention.
                let relation = strip_namespace(&t.o)
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                // `class_name:` override (ruff D-AR-5.4) takes
                // precedence over the convention — `belongs_to :owner,
                // class_name: 'User'` makes `target = "User"` not
                // `"Owner"`. Absent override falls back to the
                // singularize-and-camelcase rule.
                //
                // Normalize namespaced overrides like
                // `class_name: 'Storages::FileLink'` down to the leaf
                // class name `FileLink` — `triples_to_schema` keys
                // tables by their leaf name (the extractor strips the
                // module-path prefix at `rdf:type` time), so the
                // verbatim namespaced string would miss the
                // `known_targets` membership gate and downgrade the FK
                // to `option<any>` even when the leaf table is
                // declared (codex P2 on #38).
                let target = class_name_overrides
                    .get(&t.o)
                    .map(|s| strip_class_namespace(s).to_string())
                    .unwrap_or_else(|| rails_target_class(&relation));

                // Look up the AssocKind for this relation (ruff#15's
                // sibling `association_kind` triple). Only `belongs_to`
                // puts a FK column on the declaring class — `has_many`,
                // `has_one`, `has_and_belongs_to_many`, and
                // `accepts_nested_attributes_for` keep the FK on the
                // OTHER table (or use a join table). Emitting a phantom
                // `<rel>_id` column for those is a real DB-shape bug
                // (~189 / 332 ≈ 57 % of OP corpus FKs were phantom
                // before this fix).
                //
                // Missing kind triple (older ndjson predating ruff#15)
                // falls back to `belongs_to` so pre-#15 ndjson dumps
                // render identically — preserves the prior contract.
                let assoc_kind = assoc_kinds
                    .get(&t.o)
                    .map(String::as_str)
                    .unwrap_or("belongs_to");

                if assoc_kind == "belongs_to" {
                    // Real FK on the declaring class.
                    // Only emit `record<Target>` when `Target` is a
                    // discovered table (the polymorphic-association
                    // guard from the previous PR — `Remindable` is a
                    // runtime type discriminator, not a real table).
                    let field_name = format!("{relation}_id");
                    // Nullability comes from the schema stratum when
                    // present (`t.references :type, null: false` →
                    // bare `record<Type>`); absent constraint keeps
                    // the Rails-default nullable `option<…>`.
                    let fk_iri = format!("{table_iri}.{field_name}");
                    let base = if known_targets.contains(&target) {
                        Kind::Record(vec![target])
                    } else {
                        Kind::Any
                    };
                    let kind = if not_null.contains(&fk_iri) {
                        base
                    } else {
                        base.optional()
                    };
                    let newly = builder.add_field(field_name.clone(), kind.clone());
                    // AR shape owns the kind: if the schema-stratum
                    // column landed first (unsorted stream), upgrade
                    // its `option<int>` to the typed record link.
                    let upgraded = !newly && builder.upgrade_field_kind(&field_name, &kind);
                    if newly || upgraded {
                        // Companion index once per field — duplicate
                        // `declares_association` triples are already
                        // deduped by the expander (codex P2-2).
                        let idx_name = format!("idx_{}_{field_name}", builder.name);
                        builder.add_index(idx_name, vec![field_name]);
                    }
                } else {
                    // has_many / has_one / habtm / accepts_nested:
                    // surface as a table-level annotation, NOT a
                    // column. The graph relationship is real, but the
                    // schema column lives on the inverse side (or in a
                    // join table for habtm).
                    builder.add_annotation(format!("{assoc_kind}:{relation}→{target}"));
                }
            }
            // ───── D-AR-5.1: Rails AR-shape → schema enrichment ─────
            "validates_constraint" => {
                // Triple shape: `(model, validates_constraint, <attr>)`.
                // Buffer the assertion; apply after all fields are
                // populated so stream-order doesn't cause drops.
                //
                // When ruff#21+ emits sibling `validation_kind`
                // triples for this attribute (`presence`,
                // `numericality`, `acceptance`, ...), compose a
                // kind-aware ASSERT. Without kind info (pre-ruff#21
                // ndjson or block-form `validate { ... }`), fall back
                // to the catch-all `$value != NONE` — the most
                // common load-bearing Rails validation effect.
                let attr_iri = format!("{table_iri}.{}", t.o);
                let empty_params = std::collections::BTreeSet::new();
                let params = validation_params.get(&attr_iri).unwrap_or(&empty_params);
                let expr = validation_kinds
                    .get(&attr_iri)
                    .map(|kinds| compose_validation_assert(kinds, params))
                    .unwrap_or_else(|| "$value != NONE".to_string());
                pending_asserts.push((table_iri.clone(), t.o.clone(), expr));
            }
            "normalizes_attribute" => {
                // `normalizes :attr, with: ->(v) { … }` — the
                // transformation runs on assignment but does NOT imply
                // presence; the column can still be nullable (codex
                // P2 PR #27: `ASSERT $value != NONE` would reject NULL
                // on a nullable normalized column). Surface it as a
                // table-level annotation only until a future sprint
                // lowers the lambda to a SurrealQL `VALUE` expression.
                builder.add_annotation(format!("normalize:{}", t.o));
            }
            "acts_as" => {
                // Triple shape: `(model, acts_as, "<variant>[:<options>]")`.
                // Record the variant in the table-level comment so
                // a downstream consumer can see "this table is
                // `acts_as_list`/`acts_as_tree`/etc.".
                let variant = t.o.split(':').next().unwrap_or(&t.o).to_string();
                builder.add_annotation(format!("acts_as_{variant}"));
            }
            "has_callback" => {
                // Triple shape: `(model, has_callback, "<phase>:<target>")`.
                // The schema can't yet render Ruby callback bodies,
                // but the table-level comment surfaces them so a
                // human or downstream tool can see the lifecycle hooks.
                builder.add_annotation(format!("callback:{}", t.o));
            }
            "includes_module" => {
                // Method-body composition (Ruby `include Mod`).
                // Distinct from STI parents, which now flow through
                // their own `inherits_from` predicate (ruff#19) and
                // get a dedicated `inherits:` annotation below.
                builder.add_annotation(format!("include:{}", t.o));
            }
            "inherits_from" => {
                // Rails STI parent (single-table inheritance with a
                // type-discriminator column). Semantically distinct
                // from `includes_module` — STI shares the physical
                // table; concerns are method mix-ins. Surface as a
                // dedicated `inherits:` annotation so downstream
                // consumers (graph build, schema visualizer) can
                // render the type hierarchy correctly.
                //
                // The `inherits_from` predicate is cross-frontend
                // (ruff also emits it for C++ class inheritance with
                // a different provenance tier). At the schema level
                // they're semantically equivalent — both mean
                // "this table is a subtype of <parent>".
                //
                // Strip the `openproject:` namespace prefix before
                // the annotation so the rendered comment reads
                // `inherits:Issue` (matching table-name conventions)
                // rather than `inherits:openproject:Issue` (codex P2
                // on #40). The bare class name is what downstream
                // graph builders join against `known_targets`.
                builder.add_annotation(format!("inherits:{}", strip_namespace(&t.o)));
            }
            // ───── D-AR-5.5: class-level Rails facts → table annotations
            //
            // These predicates carry CLASS-level facts (one fact per
            // table) rather than function-body facts. Surface them as
            // `<verb>:<object>` annotations so downstream consumers
            // (graph build, schema visualizer) see the AR-shape
            // surface. The follow-on D-AR-5.6 sprint may lift the
            // body-coupled ones (scopes, delegations) to SurrealQL
            // `DEFINE FUNCTION` once the Ruby→SurrealQL lowering is
            // in place.
            "has_scope" => {
                builder.add_annotation(format!("scope:{}", t.o));
            }
            "has_default_scope" => {
                builder.add_annotation(format!("default_scope:{}", t.o));
            }
            "aliases_method" => {
                builder.add_annotation(format!("alias_method:{}", t.o));
            }
            "aliases_attribute" => {
                builder.add_annotation(format!("alias_attr:{}", t.o));
            }
            "delegates_to" => {
                builder.add_annotation(format!("delegate:{}", t.o));
            }
            "extends_module" => {
                builder.add_annotation(format!("extend:{}", t.o));
            }
            "prepends_module" => {
                builder.add_annotation(format!("prepend:{}", t.o));
            }
            "uses_refinement" => {
                builder.add_annotation(format!("using:{}", t.o));
            }
            "mounts_uploader" => {
                builder.add_annotation(format!("mount:{}", t.o));
            }
            "has_paper_trail" => {
                builder.add_annotation(format!("paper_trail:{}", t.o));
            }
            "has_closure_tree" => {
                builder.add_annotation(format!("closure_tree:{}", t.o));
            }
            "counter_cultures" => {
                builder.add_annotation(format!("counter_culture:{}", t.o));
            }
            "auto_strips" => {
                builder.add_annotation(format!("auto_strip:{}", t.o));
            }
            "registers_journal_formatter" => {
                builder.add_annotation(format!("journal_formatter:{}", t.o));
            }
            "registers_journal_formatted_fields" => {
                builder.add_annotation(format!("journal_fields:{}", t.o));
            }
            "has_dsl_call" => {
                // OpenProject + Rails-app-level DSL invocations
                // (`state(:configuring)`, `activity_provider_for(...)`,
                // `has_details_table(...)`, `after_transition(...)`,
                // and the long-tail singletons). The object carries
                // the call surface (`<method>(<args>)`); surface as a
                // table-level `dsl:<call>` annotation so downstream
                // consumers (schema visualizer, graph build) see the
                // DSL surface without a separate side-table.
                builder.add_annotation(format!("dsl:{}", t.o));
            }
            "column_override" => {
                // `(model.column, column_override, "<key>=<value>")` —
                // declarative DSL that overrides a column's behaviour
                // (e.g. `serialize :data, JSON` / `undef_method :foo`).
                // Surface as a table-level marker so consumers can
                // see the non-default column treatment.
                //
                // The subject carries the column name (`model.column`);
                // the object carries the override key=value. Two
                // columns with the same override value (e.g. both
                // `data` and `meta` serialize as JSON) would dedupe
                // to one annotation if we only captured the value,
                // losing one fact (codex P2 on #44). Include the
                // member name so each column-override is uniquely
                // identified: `col_override:<col>:<key>=<value>`.
                let col = member.as_deref().unwrap_or("?");
                builder.add_annotation(format!("col_override:{col}:{}", t.o));
            }
            "defines_method" => {
                // `define_method` dynamic-method declaration. Per the
                // ruff IR doc, default provenance is `Inferred`
                // (dynamic-method finds are heuristic). Schema can't
                // commit to a `DEFINE FUNCTION` for these without
                // the body lowering sprint, but a table-level
                // `dyn_method:<expr>` annotation surfaces the
                // existence-of-dynamic-method fact.
                builder.add_annotation(format!("dyn_method:{}", t.o));
            }
            // Remaining catch-all (has_function, reads_field, raises,
            // traverses_relation, concern_*) carry method-body
            // semantics that need the Ruby→SurrealQL body lowering
            // sprint (D-AR-5.6) before they can lift into
            // `DEFINE FUNCTION` / `DEFINE EVENT`. Class-level facts
            // (one fact per table) are lifted above.
            _ => {}
        }
    }

    // Apply buffered asserts now that every `has_attribute` /
    // `declares_association` triple has populated its field. No-op
    // for asserts whose target field doesn't exist (the phantom-field
    // guard from codex P2 still holds — validations on un-extracted
    // DB columns drop silently until D-AR-3.7 wires schema.rb).
    for (table_iri, field_name, expr) in pending_asserts {
        if let Some(builder) = tables.get_mut(&table_iri) {
            builder.add_field_assert(&field_name, &expr);
        }
    }

    // Emit a UNIQUE index for every attribute whose `validation_kind`
    // set contains `"uniqueness"`. SurrealDB's catalog renders this
    // as `Index::Uniq`; the bridge maps the boolean across via the
    // `IndexDefinition.unique` field. Skips fields not declared via
    // `has_attribute` (phantom-field guard mirrors the assert path).
    //
    // Rails `uniqueness: { scope: [...] }` (ruff#25 sibling
    // `validation_param`) composes a multi-column UNIQUE index: the
    // attribute + each scope column. The index name encodes the
    // full column list so two scoped uniqueness validations on the
    // same attribute (rare but legal) produce distinct indices.
    for (attr_iri, kinds) in &validation_kinds {
        if !kinds.contains("uniqueness") {
            continue;
        }
        let Some((table_iri, Some(attr_name))) = split_subject(attr_iri) else {
            continue;
        };
        if let Some(builder) = tables.get_mut(&table_iri) {
            if !builder.has_field(&attr_name) {
                continue;
            }
            let scope_cols = validation_params
                .get(attr_iri)
                .map(extract_uniqueness_scope)
                .unwrap_or_default();
            let mut cols = vec![attr_name.clone()];
            cols.extend(scope_cols.iter().cloned());
            let idx_name = if scope_cols.is_empty() {
                format!("idx_{}_{attr_name}_unique", builder.name)
            } else {
                format!(
                    "idx_{}_{attr_name}_{}_unique",
                    builder.name,
                    scope_cols.join("_"),
                )
            };
            builder.add_unique_index(idx_name, cols);
        }
    }

    let mut schema = Schema::new();
    for (_iri, builder) in tables {
        schema.tables.push(builder.build());
    }
    schema
}

/// Split a subject IRI into `(table_iri, member?)`.
///
/// `"openproject:WorkPackage"` → `("openproject:WorkPackage", None)`.
/// `"openproject:WorkPackage.subject"` → `("openproject:WorkPackage", Some("subject"))`.
fn split_subject(s: &str) -> Option<(String, Option<String>)> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(dot) = trimmed.find('.') {
        Some((
            trimmed[..dot].to_string(),
            Some(trimmed[dot + 1..].to_string()),
        ))
    } else {
        Some((trimmed.to_string(), None))
    }
}

/// Strip the `openproject:` namespace prefix (if present). Other
/// prefixes pass through unchanged so e.g. `exc:UserError` stays
/// intact.
fn strip_namespace(s: &str) -> &str {
    s.strip_prefix("openproject:").unwrap_or(s)
}

/// Strip a Ruby module-path prefix off a class name, returning the
/// leaf class. `Storages::FileLink` → `FileLink`, `User` → `User`.
///
/// The triples extractor declares tables by their leaf class name
/// (the `rdf:type ogit:ObjectType` triple's subject IRI is
/// `openproject:FileLink`, NOT `openproject:Storages::FileLink`).
/// A `class_name: 'Storages::FileLink'` override therefore must be
/// stripped before the `known_targets.contains(&target)` membership
/// check, or the FK degrades to `option<any>` despite the leaf table
/// being present (codex P2 on #38).
fn strip_class_namespace(s: &str) -> &str {
    s.rsplit("::").next().unwrap_or(s)
}

/// Compose a kind-aware SurrealQL ASSERT clause from the recognised
/// Rails validation kinds on an attribute.
///
/// The 9-key Rails set (ruff#21) maps to SurrealQL like this:
///
/// | Rails kind     | clause fragment                  | parametric? |
/// |----------------|----------------------------------|-------------|
/// | `presence`     | `$value != NONE`                 | no          |
/// | `numericality` | `type::is_number($value)`        | no          |
/// | `absence`      | `$value == NONE`                 | no          |
/// | `acceptance`   | `$value == true`                 | no          |
/// | `uniqueness`   | `$value != NONE` (presence-style)| no (index-level too — TODO) |
/// | `length`       | `$value != NONE`                 | yes (skip)  |
/// | `format`       | `$value != NONE`                 | yes (skip)  |
/// | `inclusion`    | `$value != NONE`                 | yes (skip)  |
/// | `exclusion`    | `$value != NONE`                 | yes (skip)  |
/// | `confirmation` | `$value != NONE`                 | composite   |
///
/// Multiple kinds compose with `AND` in a stable order.
/// Parametric kinds (length/format/inclusion/exclusion) need the
/// option values that ruff#21 doesn't carry on the kind triple
/// itself (a future predicate could surface them); for now they
/// fall back to the presence-style `$value != NONE` so the
/// constraint isn't silently dropped.
///
/// Unknown kinds in the set are skipped (forward-compat: a future
/// `Predicate::ValidationKind` enrichment shouldn't fail the
/// downstream).
fn compose_validation_assert(
    kinds: &std::collections::BTreeSet<String>,
    params: &std::collections::BTreeSet<String>,
) -> String {
    let mut clauses: Vec<String> = Vec::new();
    let mut has_non_none = false;
    for kind in kinds {
        match kind.as_str() {
            // SurrealDB 3.0.0-beta renamed the `type::is::*`
            // colon-namespace functions to underscore form
            // (`type::is_number`, `type::is_string`, etc.) — codex P1
            // on #41. Older SurrealDB versions had `type::is::number`;
            // the underscore form is the documented current API.
            //
            // `numericality: { greater_than: N, less_than: M }`
            // contributes range clauses via the params loop below;
            // the kind itself emits the type check.
            "numericality" => clauses.push("type::is_number($value)".to_string()),
            "acceptance" => clauses.push("$value == true".to_string()),
            // `validates :foo, absence: true` is the inverse of
            // presence — the attribute MUST be nil. SurrealDB
            // represents nil as NONE.
            "absence" => clauses.push("$value == NONE".to_string()),
            // Presence-equivalent kinds (the parameter-less ones in
            // the catch-all fall here too).
            "presence" | "uniqueness" | "length" | "format" | "inclusion" | "exclusion"
            | "confirmation" | "comparison"
                if !has_non_none => {
                    clauses.push("$value != NONE".to_string());
                    has_non_none = true;
                }
            _ => {} // forward-compat: unknown kind, skip
        }
    }
    // Parametric kind options (`length:maximum=255`,
    // `numericality:greater_than=0`, …) emitted by ruff#25's
    // `validation_param` predicate. Each becomes its own AND-clause.
    // The kind-level presence/numericality clauses above already
    // fired; params add range/length/regex checks on top.
    for param in params {
        if let Some(clause) = param_clause(param) {
            clauses.push(clause);
        }
    }
    if clauses.is_empty() {
        // Empty set (or only unknown kinds) — keep the load-bearing
        // catch-all so the validation doesn't silently drop.
        return "$value != NONE".to_string();
    }
    clauses.join(" AND ")
}

/// Lower a single `validation_param` triple object
/// (`<kind>:<inner_key>=<value>`) to a SurrealQL clause fragment.
/// Returns `None` for unrecognised shapes (caller skips silently).
///
/// Recognised cases (every `<value>` MUST parse as a numeric
/// literal — see the safety note below):
///
/// | Param                                     | Clause                       |
/// |-------------------------------------------|------------------------------|
/// | `length:maximum=N`                        | `string::len($value) <= N`   |
/// | `length:minimum=N`                        | `string::len($value) >= N`   |
/// | `length:is=N`                             | `string::len($value) == N`   |
/// | `numericality:greater_than=N`             | `$value > N`                 |
/// | `numericality:less_than=N`                | `$value < N`                 |
/// | `numericality:greater_than_or_equal_to=N` | `$value >= N`                |
/// | `numericality:less_than_or_equal_to=N`    | `$value <= N`                |
/// | `numericality:equal_to=N`                 | `$value == N`                |
/// | `comparison:greater_than=N`               | `$value > N`                 |
/// | `comparison:less_than=N`                  | `$value < N`                 |
/// | `comparison:greater_than_or_equal_to=N`   | `$value >= N`                |
/// | `comparison:less_than_or_equal_to=N`      | `$value <= N`                |
/// | `comparison:equal_to=N`                   | `$value == N`                |
///
/// **Safety: numeric-literal-only values** (codex P2 on #46).
///
/// Rails apps commonly write `length: { maximum: MAX_NAME_LENGTH }`
/// where the value is a Ruby constant reference, not a literal —
/// ruff forwards the raw text `MAX_NAME_LENGTH`. Splicing that
/// verbatim into the ASSERT produces invalid SurrealQL like
/// `string::len($value) <= MAX_NAME_LENGTH`. The catalog would
/// either reject the schema outright or compare against an
/// unintended identifier.
///
/// Gate every numeric-shaped param on `i64::from_str` (positive,
/// negative, zero) so non-literal values fall back to the safe
/// presence assertion (`None` here → caller skips, the kind-level
/// `$value != NONE` from `length`/`numericality`/`comparison`
/// still fires).
fn param_clause(param: &str) -> Option<String> {
    let (kind_key, value) = param.split_once('=')?;
    let (kind, key) = kind_key.split_once(':')?;
    let trimmed = value.trim();
    // Only emit if the value parses as a numeric literal — see
    // safety note above.
    trimmed.parse::<i64>().ok()?;
    match (kind, key) {
        ("length", "maximum") => Some(format!("string::len($value) <= {trimmed}")),
        ("length", "minimum") => Some(format!("string::len($value) >= {trimmed}")),
        ("length", "is") => Some(format!("string::len($value) == {trimmed}")),
        // `numericality` and `comparison` share the same operator
        // set in Rails (greater_than / less_than / etc.) — lower
        // both kinds via the same arms.
        ("numericality" | "comparison", "greater_than") => Some(format!("$value > {trimmed}")),
        ("numericality" | "comparison", "less_than") => Some(format!("$value < {trimmed}")),
        ("numericality" | "comparison", "greater_than_or_equal_to") => {
            Some(format!("$value >= {trimmed}"))
        }
        ("numericality" | "comparison", "less_than_or_equal_to") => {
            Some(format!("$value <= {trimmed}"))
        }
        ("numericality" | "comparison", "equal_to") => Some(format!("$value == {trimmed}")),
        _ => None, // forward-compat: unknown kind:key, skip
    }
}

/// Extract the scope column list from Rails `uniqueness: { scope:
/// ... }` `validation_param` triples on an attribute. Returns the
/// scope column names in declaration order (with the leading `:`
/// symbol prefix stripped); empty Vec if no `uniqueness:scope=...`
/// param is present.
///
/// Value shapes the ruff producer emits:
///
/// - Single symbol: `uniqueness:scope=:project_id`
///   → `["project_id"]`
/// - Array of symbols: `uniqueness:scope=[:project_id]` or
///   `[:type, :project_id]` → `["project_id"]` / `["type",
///   "project_id"]`
/// - Anything else (constants like `SCOPE_COLS`, `<expr>` etc.)
///   → empty Vec (safer than splicing a Ruby identifier into the
///   `DEFINE INDEX FIELDS` list).
fn extract_uniqueness_scope(params: &std::collections::BTreeSet<String>) -> Vec<String> {
    for param in params {
        let Some(rest) = param.strip_prefix("uniqueness:scope=") else {
            continue;
        };
        // Array form `[:a, :b]` (or `[:a,:b]` — comma-spacing may
        // vary in render_node output).
        if let Some(inner) = rest.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            return inner
                .split(',')
                .filter_map(|seg| {
                    let trimmed = seg.trim();
                    let stripped = trimmed.strip_prefix(':').unwrap_or(trimmed);
                    // Reject non-identifier-shaped segments
                    // (forward-compat: an `<expr>` or constant
                    // reference shouldn't splice into the FIELDS
                    // list).
                    if stripped.is_empty()
                        || !stripped
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        return None;
                    }
                    Some(stripped.to_string())
                })
                .collect();
        }
        // Single symbol form `:project_id`.
        if let Some(sym) = rest.strip_prefix(':')
            && !sym.is_empty() && sym.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return vec![sym.to_string()];
            }
        // Anything else (bare constant, <expr>) — skip silently.
        return Vec::new();
    }
    Vec::new()
}

/// Apply the Rails AR convention: a relation name (`time_entries`,
/// `project`) maps to a target class name (`TimeEntry`, `Project`).
///
/// Algorithm: singularize the trailing `s`/`es`/`ies` (best-effort),
/// then split on `_` and camelcase each segment.
fn rails_target_class(relation: &str) -> String {
    let singular = singularize(relation);
    singular
        .split('_')
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

/// Best-effort singularization. Handles the common Rails cases:
/// `entries` → `entry`, `accesses` → `access`, `users` → `user`,
/// `boxes` → `box`, `watches` → `watch`, `news` → `news`. Cases
/// not covered fall through to a bare `-s` strip; the D-AR-5.2
/// sprint can wire a fuller irregular-forms table if needed.
///
/// The old `-ses → -s` rule was too greedy: it transformed `phases`
/// into `phas` (target class `Phas` instead of `Phase`) and
/// `responses` into `respons`. Narrowed to `-sses → -ss` so it only
/// fires on actual double-s plurals like `accesses → access`.
fn singularize(s: &str) -> String {
    // Irregular plurals: the trailing `_`-segment is rewritten in place.
    // Covers the most common English irregulars; the OP corpus only
    // surfaces `children` today (PR follow-up to #34) but the others
    // are cheap insurance.
    const IRREGULAR: &[(&str, &str)] = &[
        ("children", "child"),
        ("people", "person"),
        ("men", "man"),
        ("women", "woman"),
        ("feet", "foot"),
        ("teeth", "tooth"),
        ("mice", "mouse"),
        ("geese", "goose"),
    ];
    let trailing_start = s.rfind('_').map_or(0, |i| i + 1);
    let trailing = &s[trailing_start..];
    if let Some(&(_, sing)) = IRREGULAR.iter().find(|(p, _)| *p == trailing) {
        return format!("{}{sing}", &s[..trailing_start]);
    }

    // Uncountable Rails-style nouns that surfaced on the OP corpus —
    // their singular and plural form match. Falls back to the
    // length-aware rules below if a name isn't on the list.
    const UNCOUNTABLE: &[&str] = &[
        "news",
        "series",
        "species",
        "equipment",
        "information",
        "money",
        "fish",
        "sheep",
        "deer",
        "rice",
        "staff",
        "data",
    ];
    if UNCOUNTABLE.contains(&trailing) {
        return s.to_string();
    }
    // Latin-derived `-us` singulars whose plurals are `-uses` (or
    // Latin `-i`). The relation name is already singular — must
    // NOT be stripped to `-u`. Explicit list, NOT the broad
    // "stem ends in u" heuristic (codex P2 on #35: `menus → Menu`
    // is a real Rails plural that the heuristic would falsely keep
    // as `Menus`).
    const SINGULAR_US: &[&str] = &[
        "status", "bus", "virus", "bonus", "focus", "radius", "chorus", "genus", "cactus",
        "octopus", "fungus", "locus", "nucleus", "syllabus", "alumnus", "stimulus", "surplus",
        "campus", "census", "circus", "corpus",
    ];
    if SINGULAR_US.contains(&trailing) {
        return s.to_string();
    }
    if let Some(stem) = s.strip_suffix("ies") {
        return format!("{stem}y");
    }
    // `-es` plural: only valid when the underlying singular ends in
    // a sibilant/affricate cluster (`-ch`, `-sh`, `-ss`, `-x`, `-z`).
    // Otherwise the `-es` is just `-s` glued onto a final `-e`
    // (`phases → phase`, `responses → response`) — fall through to
    // the bare `-s` strip below. The old `-ses → -s` rule fired on
    // those too eagerly.
    if let Some(stem) = s.strip_suffix("es")
        && (stem.ends_with("ch")
            || stem.ends_with("sh")
            || stem.ends_with("ss")
            || stem.ends_with('x')
            || stem.ends_with('z'))
        {
            return stem.to_string();
        }
    if let Some(stem) = s.strip_suffix('s') {
        if stem.ends_with('s') {
            // `-ss` like `class` keeps the trailing s (`mass`, `glass`).
            return s.to_string();
        }
        return stem.to_string();
    }
    s.to_string()
}

/// Mutable builder for one table — captures fields and indexes as
/// they arrive, deferring the `TableDefinition::with_*` chain until
/// the final `build()` so insertion order is preserved.
struct TableBuilder {
    name: String,
    fields: Vec<FieldDefinition>,
    indices: Vec<IndexDefinition>,
    seen_fields: std::collections::HashSet<String>,
    /// Table-level AR-shape facts to fold into `TableDefinition.comment`
    /// (D-AR-5.1). Captured here in insertion order, deduplicated at
    /// `build()` time, then joined into a single `// <fact>; <fact>; …`
    /// comment string.
    annotations: Vec<String>,
}

impl TableBuilder {
    fn new(name: String) -> Self {
        Self {
            name,
            fields: Vec::new(),
            indices: Vec::new(),
            seen_fields: std::collections::HashSet::new(),
            annotations: Vec::new(),
        }
    }

    /// Returns `true` if the field was newly added, `false` if a field
    /// of that name was already present and the call was a no-op. The
    /// caller uses this to gate companion-index emission so duplicate
    /// declarations don't produce duplicate `DEFINE INDEX` statements.
    fn add_field(&mut self, name: String, kind: Kind) -> bool {
        if self.seen_fields.insert(name.clone()) {
            self.fields
                .push(FieldDefinition::new(name, self.name.clone(), kind));
            true
        } else {
            false
        }
    }

    /// Replace an existing field's kind — the AR-shape upgrade path:
    /// `belongs_to` promotes a schema-stratum `option<int>` FK column
    /// to `record<Target>` when the column triple happened to land
    /// first. Returns `true` when the kind actually changed; `false`
    /// when the field is absent or already carries the kind.
    fn upgrade_field_kind(&mut self, name: &str, kind: &Kind) -> bool {
        if let Some(f) = self.fields.iter_mut().find(|f| f.name == name)
            && &f.kind != kind {
                f.kind = kind.clone();
                return true;
            }
        false
    }

    /// `true` if this builder has a field with the given name. Used
    /// by the UNIQUE-index post-pass to guard against
    /// validation-on-phantom-field cases — mirrors the phantom-field
    /// guard on `add_field_assert`.
    fn has_field(&self, name: &str) -> bool {
        self.seen_fields.contains(name)
    }

    fn add_index(&mut self, name: String, fields: Vec<String>) {
        self.indices
            .push(IndexDefinition::new(name, self.name.clone(), fields));
    }

    /// Add a UNIQUE index, deduplicated by name. Returns `true` if the
    /// index was newly added.
    fn add_unique_index(&mut self, name: String, fields: Vec<String>) -> bool {
        if self.indices.iter().any(|i| i.name == name) {
            return false;
        }
        self.indices
            .push(IndexDefinition::new(name, self.name.clone(), fields).unique());
        true
    }

    /// Attach a `validates_constraint`-style ASSERT clause to the
    /// existing field with the matching name. No-op if the field
    /// doesn't exist on this table (Rails can validate DB columns
    /// that the AR-shape extractor doesn't surface via
    /// `has_attribute` — those land as `Vec::new()` field stubs
    /// until D-AR-3.7 wires `db/schema.rb`).
    ///
    /// Last-writer-wins on multiple validations of the same attribute
    /// — Rails composes them at runtime; the schema captures the most
    /// recent fact.
    fn add_field_assert(&mut self, field_name: &str, expr: &str) {
        if let Some(field) = self.fields.iter_mut().find(|f| f.name == field_name) {
            field.assert = Some(expr.to_string());
        }
    }

    /// Push an AR-shape fact onto the table-level annotation list.
    /// Deduplicated at `build()` time.
    fn add_annotation(&mut self, note: String) {
        self.annotations.push(note);
    }

    fn build(mut self) -> TableDefinition {
        // Dedup-preserving-order: drop second+ occurrences of an
        // identical annotation. A model that `include`s `Acts::List`
        // and `acts_as_list` will produce two `include:` and one
        // `acts_as_list` annotation — the first survives, repeats drop.
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.annotations.retain(|a| seen.insert(a.clone()));

        let comment = if self.annotations.is_empty() {
            None
        } else {
            Some(self.annotations.join("; "))
        };

        let mut t = TableDefinition::new(self.name).with_comment(comment);
        for f in self.fields {
            t = t.with_field(f);
        }
        for i in self.indices {
            t = t.with_index(i);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToSql;

    fn t(s: &str, p: &str, o: &str) -> Triple {
        Triple {
            s: s.to_string(),
            p: p.to_string(),
            o: o.to_string(),
            f: 1.0,
            c: 1.0,
        }
    }

    #[test]
    fn rdf_type_object_creates_table() {
        let triples = vec![t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType")];
        let schema = triples_to_schema(&triples);
        assert_eq!(schema.tables.len(), 1);
        assert_eq!(schema.tables[0].name, "WorkPackage");
    }

    #[test]
    fn has_attribute_adds_field_with_any_kind() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        assert_eq!(wp.fields.len(), 1);
        assert_eq!(wp.fields[0].name, "subject");
        assert_eq!(wp.fields[0].kind, Kind::Any);
    }

    #[test]
    fn declares_association_adds_record_field_and_index() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // Project MUST be a known table for the FK to lower to
            // `record<Project>` — without it, the association would
            // fall back to `option<any>` (polymorphic-safe default).
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = schema
            .tables
            .iter()
            .find(|t| t.name == "WorkPackage")
            .unwrap();
        // FieldDefinition: project_id : option<record<Project>>
        let project = wp.fields.iter().find(|f| f.name == "project_id").unwrap();
        assert_eq!(
            project.kind,
            Kind::Record(vec!["Project".to_string()]).optional()
        );
        // Companion IndexDefinition on project_id.
        assert!(
            wp.indices
                .iter()
                .any(|i| i.name == "idx_WorkPackage_project_id"),
            "expected an index on project_id"
        );
    }

    /// **Polymorphic-association regression** — `belongs_to :ownable,
    /// polymorphic: true` names a runtime type discriminator
    /// (`Ownable`), not a real table. The FK must fall back to
    /// `option<any>` instead of inventing a phantom `record<Ownable>`
    /// pointing at a table that doesn't exist.
    #[test]
    fn polymorphic_association_falls_back_to_option_any() {
        let triples = vec![
            t("openproject:Reminder", "rdf:type", "ogit:ObjectType"),
            // No `Remindable` class — polymorphic association.
            t(
                "openproject:Reminder",
                "declares_association",
                "openproject:Reminder.remindable",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let reminder = &schema.tables[0];
        let fk = reminder
            .fields
            .iter()
            .find(|f| f.name == "remindable_id")
            .unwrap();
        // Polymorphic → `option<any>`, NOT `option<record<Remindable>>`.
        assert_eq!(
            fk.kind,
            Kind::Any.optional(),
            "polymorphic association must fall back to option<any>; got {:?}",
            fk.kind,
        );
        // The companion index still exists — SurrealQL allows indexes
        // on any-typed columns and the `_id` FK pattern stays
        // queryable.
        assert!(
            reminder
                .indices
                .iter()
                .any(|i| i.name == "idx_Reminder_remindable_id")
        );
    }

    /// **Polymorphic-vs-known-target lock** — a mixed triple set
    /// where one association targets a known table and another
    /// targets a polymorphic name produces the correct mix:
    /// `record<X>` for the known, `option<any>` for the polymorphic.
    #[test]
    fn mixed_known_and_polymorphic_associations_lower_correctly() {
        let triples = vec![
            t("openproject:Member", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            // user is a known table → record<User>
            t(
                "openproject:Member",
                "declares_association",
                "openproject:Member.user",
            ),
            // entity is polymorphic → option<any>
            t(
                "openproject:Member",
                "declares_association",
                "openproject:Member.entity",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let member = schema.tables.iter().find(|t| t.name == "Member").unwrap();
        let user_fk = member.fields.iter().find(|f| f.name == "user_id").unwrap();
        let entity_fk = member
            .fields
            .iter()
            .find(|f| f.name == "entity_id")
            .unwrap();
        assert_eq!(
            user_fk.kind,
            Kind::Record(vec!["User".to_string()]).optional()
        );
        assert_eq!(entity_fk.kind, Kind::Any.optional());
    }

    #[test]
    fn singularize_handles_common_rails_plurals() {
        assert_eq!(singularize("entries"), "entry");
        assert_eq!(singularize("users"), "user");
        assert_eq!(singularize("boxes"), "box");
        assert_eq!(singularize("accesses"), "access");
        assert_eq!(singularize("project"), "project");
    }

    /// **Regression** — corpus run surfaced four singularization
    /// quirks where the old `-ses → -s` rule was too greedy or
    /// where uncountable nouns weren't recognised. Lock each one.
    #[test]
    fn singularize_handles_op_corpus_quirks() {
        // -ses no longer triggers on -aSes/-nSes/-rSes etc.; only
        // genuine double-s plurals (-sses) do.
        assert_eq!(singularize("phases"), "phase");
        assert_eq!(singularize("responses"), "response");
        // -ches and -shes drop only -es, keep the ch/sh.
        assert_eq!(singularize("watches"), "watch");
        assert_eq!(singularize("dishes"), "dish");
        // Uncountable: singular == plural.
        assert_eq!(singularize("news"), "news");
        assert_eq!(singularize("series"), "series");
        assert_eq!(singularize("species"), "species");
        // -ss like `class`/`mass` stays unchanged (no false stripping).
        assert_eq!(singularize("class"), "class");
    }

    /// **Regression** — the AR-shape target class for a `has_many`
    /// uses the singularized relation name. Lock the camel-cased
    /// outputs for the four corpus-surfaced cases so the
    /// `has_many:<rel>→<Target>` annotations stay correct.
    #[test]
    fn rails_target_class_lowers_corpus_quirks_correctly() {
        assert_eq!(rails_target_class("phases"), "Phase");
        assert_eq!(rails_target_class("watches"), "Watch");
        assert_eq!(rails_target_class("news"), "News");
        // Compound name with the -ses pitfall on the trailing segment.
        assert_eq!(
            rails_target_class("recurring_meeting_interim_responses"),
            "RecurringMeetingInterimResponse",
        );
    }

    /// **Regression (post-#34 corpus)** — `children` and `job_status`
    /// were the two remaining target-class quirks after the
    /// singularization tightening. Lock both.
    #[test]
    fn singularize_handles_irregular_plural_and_us_suffix() {
        // Irregular plural: `children → child`.
        assert_eq!(singularize("children"), "child");
        // Compound irregular: trailing segment is irregular.
        assert_eq!(singularize("wiki_children"), "wiki_child");
        // -us suffix words are already singular and must NOT have the
        // trailing s stripped.
        assert_eq!(singularize("status"), "status");
        assert_eq!(singularize("job_status"), "job_status");
        assert_eq!(singularize("bus"), "bus");
        assert_eq!(singularize("virus"), "virus");
        // Other irregular plurals (cheap insurance for future corpora).
        assert_eq!(singularize("people"), "person");
        assert_eq!(singularize("men"), "man");
        assert_eq!(singularize("women"), "woman");
        assert_eq!(singularize("mice"), "mouse");
    }

    /// **Regression (codex P2 on #35)** — `menus`, `gnus`, etc. are
    /// regular plurals whose stem ends in `u` but whose singular
    /// drops the `s`. An over-broad "stem ends in u → keep s" rule
    /// would falsely return `Menus`. The explicit `SINGULAR_US`
    /// whitelist must NOT include these, so they fall through to
    /// the bare `-s` strip and singularize correctly.
    #[test]
    fn regular_us_plurals_singularize_normally() {
        // Plurals with -u+s ending: singular drops the s.
        assert_eq!(singularize("menus"), "menu");
        assert_eq!(singularize("gnus"), "gnu");
        assert_eq!(singularize("gurus"), "guru");
        assert_eq!(singularize("emus"), "emu");
        // Compound form: only the trailing segment is checked.
        assert_eq!(singularize("nav_menus"), "nav_menu");
        // Camel-cased target class.
        assert_eq!(rails_target_class("menus"), "Menu");
        assert_eq!(rails_target_class("nav_menus"), "NavMenu");
    }

    /// **Regression (post-#34 corpus)** — the camel-cased
    /// target-class output for the irregular and `-us` cases.
    #[test]
    fn rails_target_class_handles_irregular_and_us() {
        assert_eq!(rails_target_class("children"), "Child");
        assert_eq!(rails_target_class("job_status"), "JobStatus");
        assert_eq!(rails_target_class("people"), "Person");
    }

    #[test]
    fn rails_target_class_camelcases_compound_names() {
        assert_eq!(rails_target_class("time_entries"), "TimeEntry");
        assert_eq!(rails_target_class("work_packages"), "WorkPackage");
        assert_eq!(rails_target_class("project"), "Project");
    }

    #[test]
    fn tables_sorted_by_name_in_output() {
        let triples = vec![
            t("openproject:Zebra", "rdf:type", "ogit:ObjectType"),
            t("openproject:Alpha", "rdf:type", "ogit:ObjectType"),
            t("openproject:Mango", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "Mango", "Zebra"]);
    }

    #[test]
    fn duplicate_has_attribute_collapses() {
        let triples = vec![
            t("openproject:WP", "rdf:type", "ogit:ObjectType"),
            t("openproject:WP", "has_attribute", "subject"),
            t("openproject:WP", "has_attribute", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        assert_eq!(schema.tables[0].fields.len(), 1);
    }

    /// **Codex P2 regression (PR #26 r3418308887)** — a body-walk
    /// predicate on a subject that lacks an `rdf:type ObjectType`
    /// declaration must NOT materialise a phantom table.
    #[test]
    fn body_walk_predicate_without_rdf_type_does_not_create_phantom_table() {
        let triples = vec![
            // Note: NO `rdf:type ObjectType` for `Missing`.
            t(
                "openproject:Missing.some_fn",
                "reads_field",
                "openproject:Missing.name",
            ),
            // A real table to confirm the filter is precise.
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["WorkPackage"]);
    }

    /// **Codex P2 regression (PR #26 r3418308894)** — a duplicate
    /// `declares_association` triple must NOT produce a duplicate
    /// `DEFINE INDEX` statement (the field is deduped via
    /// `seen_fields`; the companion index must follow the field's
    /// add-or-skip decision).
    #[test]
    fn duplicate_declares_association_does_not_emit_duplicate_index() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        // Field is deduped (the pre-existing guarantee).
        assert_eq!(wp.fields.len(), 1);
        // Index follows the same deduplication.
        let project_indices: Vec<_> = wp
            .indices
            .iter()
            .filter(|i| i.name == "idx_WorkPackage_project_id")
            .collect();
        assert_eq!(
            project_indices.len(),
            1,
            "duplicate declares_association produced duplicate index",
        );
    }

    /// **D-AR-5 end-to-end** — a multi-class triple set produces a
    /// SurrealQL output that matches the hand-built shape from the
    /// crate's own `rails_mini_e2e_byte_for_byte_with_legacy_emission`
    /// test (PR #19 baseline). Differences allowed: `subject` becomes
    /// `Kind::Any` not `Kind::Int` (triples don't carry types yet),
    /// and the index name uses the `_id`-suffixed field.
    #[test]
    fn end_to_end_triples_render_to_define_ddl() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            t("openproject:Project", "has_attribute", "identifier"),
        ];
        let schema = triples_to_schema(&triples);
        let sql = schema.to_sql();
        assert!(sql.contains("DEFINE TABLE WorkPackage SCHEMAFULL;"));
        assert!(sql.contains("DEFINE FIELD subject ON TABLE WorkPackage TYPE any;"));
        assert!(sql.contains(
            "DEFINE FIELD project_id ON TABLE WorkPackage TYPE option<record<Project>>;"
        ));
        assert!(sql.contains(
            "DEFINE INDEX idx_WorkPackage_project_id ON TABLE WorkPackage FIELDS project_id;"
        ));
        assert!(sql.contains("DEFINE TABLE Project SCHEMAFULL;"));
        assert!(sql.contains("DEFINE FIELD identifier ON TABLE Project TYPE any;"));
    }

    // ────────────────── D-AR-5.1 enrichment tests ──────────────────

    /// **D-AR-5.1** — `validates_constraint` triple targets a field
    /// declared via `has_attribute`. The schema gains an
    /// `ASSERT $value != NONE` clause on that field's DEFINE FIELD.
    #[test]
    fn validates_constraint_adds_assert_to_matching_field() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
            t("openproject:WorkPackage", "validates_constraint", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let subj = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert_eq!(
            subj.assert.as_deref(),
            Some("$value != NONE"),
            "validates_constraint must wire ASSERT onto matching field",
        );
        // Field with no validation has no assert.
        let triples2 = vec![
            t("openproject:Plain", "rdf:type", "ogit:ObjectType"),
            t("openproject:Plain", "has_attribute", "anything"),
        ];
        let schema2 = triples_to_schema(&triples2);
        let plain = &schema2.tables[0];
        let anything = plain.fields.iter().find(|f| f.name == "anything").unwrap();
        assert_eq!(anything.assert, None);
    }

    /// **D-AR-5.1** — validation on an attribute we don't extract
    /// (e.g. a DB column from `db/schema.rb`) is silently dropped.
    /// The constraint is preserved IF and only IF the field exists.
    /// This guards against materialising phantom assert-only fields.
    #[test]
    fn validates_constraint_on_unknown_field_is_noop() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // No `has_attribute` for `nonexistent` — validation has no
            // matching field to attach to.
            t(
                "openproject:WorkPackage",
                "validates_constraint",
                "nonexistent",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        assert!(
            wp.fields.is_empty(),
            "validates_constraint must not materialise a phantom field; got {:?}",
            wp.fields,
        );
    }

    /// **D-AR-5.1** — assert is rendered after TYPE in SurrealQL.
    #[test]
    fn assert_clause_renders_after_type_in_surrealql() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
            t("openproject:WorkPackage", "validates_constraint", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let sql = schema.to_sql();
        assert!(
            sql.contains(
                "DEFINE FIELD subject ON TABLE WorkPackage TYPE any ASSERT $value != NONE;"
            ),
            "expected ASSERT clause in rendered SQL; got: {sql}",
        );
    }

    /// **D-AR-5.1** — `acts_as`, `has_callback`, and `includes_module`
    /// triples land as a deduplicated `COMMENT '<facts>'` clause on
    /// the table.
    #[test]
    fn ar_shape_facts_aggregate_into_table_comment() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "acts_as", "list"),
            t("openproject:WorkPackage", "acts_as", "watchable"),
            t(
                "openproject:WorkPackage",
                "has_callback",
                "before_save:set_default_status",
            ),
            t(
                "openproject:WorkPackage",
                "includes_module",
                "Acts::Customizable",
            ),
            // Dedup: a second identical includes_module triple must NOT
            // produce a duplicate annotation in the comment.
            t(
                "openproject:WorkPackage",
                "includes_module",
                "Acts::Customizable",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let comment = wp
            .comment
            .as_deref()
            .expect("expected comment with AR facts");
        assert!(comment.contains("acts_as_list"));
        assert!(comment.contains("acts_as_watchable"));
        assert!(comment.contains("callback:before_save:set_default_status"));
        assert!(comment.contains("include:Acts::Customizable"));
        // Dedup: `Acts::Customizable` appears once.
        assert_eq!(
            comment.matches("Acts::Customizable").count(),
            1,
            "duplicate includes_module triple must dedup in comment",
        );
    }

    /// **D-AR-5.1** — comment is rendered in the DEFINE TABLE line.
    #[test]
    fn table_comment_renders_in_define_table_line() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "acts_as", "list"),
        ];
        let schema = triples_to_schema(&triples);
        let sql = schema.to_sql();
        assert!(
            sql.contains("DEFINE TABLE WorkPackage SCHEMAFULL COMMENT 'acts_as_list';"),
            "expected COMMENT clause in DEFINE TABLE; got: {sql}",
        );
    }

    /// **Codex P2 (PR #27 r…)** — `normalizes_attribute` does NOT
    /// imply presence: `normalizes :email` allows the column to stay
    /// nullable. The previous emission of `ASSERT $value != NONE`
    /// would reject `NONE` on what is a legitimately nullable
    /// normalized column. The fix: surface normalization as a
    /// table-level annotation only (`normalize:<attr>`); the field's
    /// `assert` stays `None` unless a real `validates_constraint`
    /// triple fires.
    #[test]
    fn normalizes_attribute_does_not_force_non_null_assert() {
        let triples = vec![
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "has_attribute", "email"),
            t("openproject:User", "normalizes_attribute", "email"),
        ];
        let schema = triples_to_schema(&triples);
        let user = &schema.tables[0];
        let email = user.fields.iter().find(|f| f.name == "email").unwrap();
        // Field stays nullable (no ASSERT).
        assert_eq!(
            email.assert, None,
            "normalize must NOT force $value != NONE",
        );
        // Annotation surfaces the normalization fact at the table level.
        assert!(
            user.comment
                .as_deref()
                .is_some_and(|c| c.contains("normalize:email")),
            "expected `normalize:email` in table COMMENT; got {:?}",
            user.comment,
        );
    }

    /// **Codex P2 (PR #27 r…)** — `validates_constraint` must be
    /// stream-order-independent. A triple set with the constraint
    /// listed BEFORE the field-defining `has_attribute` must still
    /// land the ASSERT on the field after population.
    #[test]
    fn validates_constraint_order_independent_with_has_attribute() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // Validation arrives FIRST.
            t("openproject:WorkPackage", "validates_constraint", "subject"),
            // Field arrives SECOND.
            t("openproject:WorkPackage", "has_attribute", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let subj = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert_eq!(
            subj.assert.as_deref(),
            Some("$value != NONE"),
            "validates_constraint must apply regardless of triple order",
        );
    }

    /// **D-AR-5.1** — a phantom-table guard still holds for the new
    /// predicates: a `validates_constraint` / `acts_as` / `has_callback`
    /// triple on an undeclared subject must NOT materialise a table.
    /// (Same invariant the codex P2-1 fix introduced for `has_attribute`.)
    #[test]
    fn new_predicates_respect_phantom_table_guard() {
        let triples = vec![
            // No `rdf:type ObjectType` for `Ghost`.
            t("openproject:Ghost", "acts_as", "list"),
            t("openproject:Ghost", "has_callback", "before_save:hook"),
            t("openproject:Ghost", "validates_constraint", "field"),
            // Real table to confirm the filter is precise.
            t("openproject:Real", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Real"]);
    }

    /// **FK-direction lock (ruff#15 sibling triple)** — an explicit
    /// `belongs_to` `association_kind` triple keeps the existing
    /// behaviour: the declaring class gets a `<rel>_id` column + a
    /// companion index. This locks the contract on the
    /// FK-on-declarer side.
    #[test]
    fn belongs_to_triple_emits_fk_column_and_index() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
            // ruff#15: explicit `belongs_to` kind.
            t(
                "openproject:WorkPackage.project",
                "association_kind",
                "belongs_to",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = schema
            .tables
            .iter()
            .find(|t| t.name == "WorkPackage")
            .unwrap();
        let fk = wp.fields.iter().find(|f| f.name == "project_id").unwrap();
        assert_eq!(
            fk.kind,
            Kind::Record(vec!["Project".to_string()]).optional(),
            "belongs_to must emit a typed `option<record<Target>>` FK",
        );
        assert!(
            wp.indices
                .iter()
                .any(|i| i.name == "idx_WorkPackage_project_id"),
            "belongs_to must emit the companion FK index",
        );
        // The annotation surface stays clean for the canonical
        // belongs_to case — no `belongs_to:project→Project` clutter.
        let comment = wp.comment.as_deref().unwrap_or("");
        assert!(
            !comment.contains("belongs_to:project"),
            "belongs_to should NOT leak into the table annotation; got {comment:?}",
        );
    }

    /// **FK-direction lock (ruff#15 sibling triple)** — a
    /// `has_many` `association_kind` triple must NOT emit a phantom
    /// `<rel>_id` column on the declaring class (the FK lives on the
    /// inverse side in Rails). Instead, the relationship surfaces as a
    /// table-level `has_many:<rel>→<Target>` annotation.
    ///
    /// This is the load-bearing fix: ~189 / 332 (≈ 57 %) of FKs
    /// emitted on the real OP corpus before this change pointed at
    /// columns that don't exist in the actual DB.
    #[test]
    fn has_many_triple_emits_annotation_no_fk_column() {
        let triples = vec![
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:Project",
                "declares_association",
                "openproject:Project.work_packages",
            ),
            // ruff#15: explicit `has_many` kind — FK lives on
            // WorkPackage, NOT on Project.
            t(
                "openproject:Project.work_packages",
                "association_kind",
                "has_many",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let project = schema.tables.iter().find(|t| t.name == "Project").unwrap();
        // No phantom `work_packages_id` column.
        assert!(
            !project.fields.iter().any(|f| f.name == "work_packages_id"),
            "has_many must NOT emit a `<rel>_id` column on the declaring class; \
             fields = {:?}",
            project.fields.iter().map(|f| &f.name).collect::<Vec<_>>(),
        );
        // No phantom companion index either.
        assert!(
            !project
                .indices
                .iter()
                .any(|i| i.name == "idx_Project_work_packages_id"),
            "has_many must NOT emit a companion FK index",
        );
        // The relationship surfaces as a table-level annotation
        // instead — downstream consumers (graph build, codegen) can
        // still see the edge.
        let comment = project.comment.as_deref().unwrap_or("");
        assert!(
            comment.contains("has_many:work_packages→WorkPackage"),
            "has_many must emit a table annotation; got comment {comment:?}",
        );
    }

    /// **Backward compatibility** — `declares_association` triples
    /// from ndjson dumps predating ruff#15 (no companion
    /// `association_kind` triple) default to `belongs_to` semantics.
    /// This preserves the pre-#15 contract bit-for-bit so older
    /// dumps render identically.
    #[test]
    fn missing_association_kind_defaults_to_belongs_to_for_backward_compat() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            // Note: NO `association_kind` triple — older ndjson dump.
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = schema
            .tables
            .iter()
            .find(|t| t.name == "WorkPackage")
            .unwrap();
        // Defaults to belongs_to → FK + index emitted, identical to
        // the pre-#15 shape (the field name and kind match
        // `declares_association_adds_record_field_and_index`).
        let fk = wp.fields.iter().find(|f| f.name == "project_id").unwrap();
        assert_eq!(
            fk.kind,
            Kind::Record(vec!["Project".to_string()]).optional()
        );
        assert!(
            wp.indices
                .iter()
                .any(|i| i.name == "idx_WorkPackage_project_id"),
        );
    }

    /// **FK-direction lock — non-belongs_to kinds enumerated** —
    /// `has_one`, `has_and_belongs_to_many`, and
    /// `accepts_nested_attributes_for` ALL keep the FK off the
    /// declaring class (same as `has_many`). One test asserts the
    /// full non-belongs_to enumeration so a future drift in any
    /// single variant is caught.
    #[test]
    fn non_belongs_to_kinds_all_skip_fk_column_emission() {
        let triples = vec![
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            t("openproject:Page", "rdf:type", "ogit:ObjectType"),
            t("openproject:Tag", "rdf:type", "ogit:ObjectType"),
            t("openproject:Slot", "rdf:type", "ogit:ObjectType"),
            // has_one
            t(
                "openproject:Project",
                "declares_association",
                "openproject:Project.page",
            ),
            t("openproject:Project.page", "association_kind", "has_one"),
            // has_and_belongs_to_many
            t(
                "openproject:Project",
                "declares_association",
                "openproject:Project.tags",
            ),
            t(
                "openproject:Project.tags",
                "association_kind",
                "has_and_belongs_to_many",
            ),
            // accepts_nested_attributes_for
            t(
                "openproject:Project",
                "declares_association",
                "openproject:Project.slots",
            ),
            t(
                "openproject:Project.slots",
                "association_kind",
                "accepts_nested_attributes_for",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let project = schema.tables.iter().find(|t| t.name == "Project").unwrap();
        for phantom in ["page_id", "tags_id", "slots_id"] {
            assert!(
                !project.fields.iter().any(|f| f.name == phantom),
                "non-belongs_to kind must NOT emit `{phantom}` on declaring class; \
                 fields = {:?}",
                project.fields.iter().map(|f| &f.name).collect::<Vec<_>>(),
            );
        }
        let comment = project.comment.as_deref().unwrap_or("");
        for expected in [
            "has_one:page→Page",
            "has_and_belongs_to_many:tags→Tag",
            "accepts_nested_attributes_for:slots→Slot",
        ] {
            assert!(
                comment.contains(expected),
                "expected `{expected}` in table annotation; got {comment:?}",
            );
        }
    }

    /// **D-AR-5.2** — a `field_type` triple paired with `has_attribute`
    /// upgrades the field from `Kind::Any` to a typed
    /// `option<T>`. The optional wrapper preserves Rails' default
    /// nullable semantics; `validates_constraint` still gates
    /// non-null via the existing ASSERT mechanism.
    #[test]
    fn field_type_triple_upgrades_attribute_kind() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
            // Companion `field_type` triple keyed by the field IRI.
            t("openproject:WorkPackage.subject", "field_type", "string"),
            t("openproject:WorkPackage", "has_attribute", "version"),
            t("openproject:WorkPackage.version", "field_type", "integer"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert_eq!(
            subject.kind,
            Kind::String.optional(),
            "string-typed attribute lowers to option<string>",
        );
        let version = wp.fields.iter().find(|f| f.name == "version").unwrap();
        assert_eq!(
            version.kind,
            Kind::Int.optional(),
            "integer-typed attribute lowers to option<int>",
        );
    }

    /// **D-AR-5.2 backward compat** — a `has_attribute` triple
    /// without a companion `field_type` triple keeps the prior
    /// `Kind::Any` behaviour (no wrapper). This is the pre-#29 ndjson
    /// dump shape; landing this PR can't regress those.
    #[test]
    fn field_type_absent_falls_back_to_kind_any() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let subject = wp.fields.iter().find(|f| f.name == "subject").unwrap();
        assert_eq!(
            subject.kind,
            Kind::Any,
            "missing field_type triple keeps Kind::Any (no option wrapper)",
        );
    }

    /// **D-AR-5.2** — an unknown rails type falls through to
    /// `Kind::Any` (no option wrapper) so a Rails sprint shipping
    /// a brand-new attribute type doesn't accidentally make the
    /// schema reject NONE.
    #[test]
    fn field_type_unknown_rails_type_falls_back_to_any() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "weird"),
            t("openproject:WorkPackage.weird", "field_type", "tachyon"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let weird = wp.fields.iter().find(|f| f.name == "weird").unwrap();
        assert_eq!(weird.kind, Kind::Any);
    }

    /// **D-AR-5.2** — when a validates_constraint pairs with a
    /// field_type, the ASSERT clause must apply to the option-wrapped
    /// typed field (the order-independence guarantee already locked
    /// by PR #27/#28).
    #[test]
    fn field_type_and_validates_constraint_compose() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_attribute", "subject"),
            t("openproject:WorkPackage.subject", "field_type", "string"),
            t("openproject:WorkPackage", "validates_constraint", "subject"),
        ];
        let schema = triples_to_schema(&triples);
        let subject = schema.tables[0]
            .fields
            .iter()
            .find(|f| f.name == "subject")
            .unwrap();
        assert_eq!(subject.kind, Kind::String.optional());
        assert_eq!(
            subject.assert.as_deref(),
            Some("$value != NONE"),
            "ASSERT must land on the typed field; the option-wrap and \
             non-null gate together produce: TYPE option<string> ASSERT $value != NONE",
        );
    }

    /// **D-AR-5.2 — kind table** — every Rails type literal that
    /// ruff emits via `field_type` maps to the expected SurrealQL
    /// kind. Locks the mapping contract.
    #[test]
    fn rails_type_to_kind_mapping_table() {
        assert_eq!(Kind::from_rails_type("integer"), Some(Kind::Int));
        // PostgreSQL `bigint` is an 8-byte signed integer (fits
        // SurrealQL int / i64).
        assert_eq!(Kind::from_rails_type("bigint"), Some(Kind::Int));
        // Rails `:big_integer` (ActiveModel::Type::BigInteger) wraps
        // Ruby's arbitrary-precision Integer — DOES NOT fit i64.
        // Routed to Decimal (arbitrary-precision in SurrealDB) so
        // values outside the i64 range that Rails accepts aren't
        // rejected by the generated schema (codex P2 on #37).
        assert_eq!(Kind::from_rails_type("big_integer"), Some(Kind::Decimal),);
        assert_eq!(Kind::from_rails_type("string"), Some(Kind::String));
        assert_eq!(Kind::from_rails_type("text"), Some(Kind::String));
        // Rails `Type::ImmutableString` — verbatim symbol.
        assert_eq!(
            Kind::from_rails_type("immutable_string"),
            Some(Kind::String),
        );
        assert_eq!(Kind::from_rails_type("boolean"), Some(Kind::Bool));
        assert_eq!(Kind::from_rails_type("float"), Some(Kind::Float));
        assert_eq!(Kind::from_rails_type("decimal"), Some(Kind::Decimal));
        // Rails 8+ `:numeric` is an alias for decimal.
        assert_eq!(Kind::from_rails_type("numeric"), Some(Kind::Decimal));
        assert_eq!(Kind::from_rails_type("datetime"), Some(Kind::Datetime));
        assert_eq!(Kind::from_rails_type("timestamp"), Some(Kind::Datetime));
        assert_eq!(Kind::from_rails_type("date"), Some(Kind::Datetime));
        assert_eq!(Kind::from_rails_type("time"), Some(Kind::Datetime));
        assert_eq!(Kind::from_rails_type("binary"), Some(Kind::Bytes));
        assert_eq!(Kind::from_rails_type("uuid"), Some(Kind::Uuid));
        // Unknown types fall back to None — caller substitutes Kind::Any.
        assert_eq!(Kind::from_rails_type("nonsense"), None);
        assert_eq!(Kind::from_rails_type(""), None);
    }

    /// Unit-level lock on the namespace-stripper — covers single,
    /// multi-segment, and bare-name inputs.
    #[test]
    fn strip_class_namespace_returns_leaf_class() {
        assert_eq!(strip_class_namespace("Storages::FileLink"), "FileLink");
        assert_eq!(strip_class_namespace("My::Deeply::Nested::Class"), "Class",);
        // Bare name: pass through unchanged.
        assert_eq!(strip_class_namespace("User"), "User");
        // Empty string: pass through.
        assert_eq!(strip_class_namespace(""), "");
    }

    /// **D-AR-5.4** — when a `class_name` triple (ruff#18) carries an
    /// FK target override, the schema uses the override's class name
    /// in preference to the Rails camelcase-singular convention on
    /// the relation name. Locks the contract on the positive case.
    #[test]
    fn class_name_triple_overrides_rails_convention_for_fk_target() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // The override target is a real table.
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.owner",
            ),
            t(
                "openproject:WorkPackage.owner",
                "association_kind",
                "belongs_to",
            ),
            // `belongs_to :owner, class_name: 'User'`.
            t("openproject:WorkPackage.owner", "class_name", "User"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = schema
            .tables
            .iter()
            .find(|t| t.name == "WorkPackage")
            .unwrap();
        let owner_id = wp.fields.iter().find(|f| f.name == "owner_id").unwrap();
        assert_eq!(
            owner_id.kind,
            Kind::Record(vec!["User".to_string()]).optional(),
            "class_name override must route the FK target to User, not Owner",
        );
    }

    /// **D-AR-5.4 backward compat** — absence of a `class_name` triple
    /// means "use the Rails convention". Pre-ruff#18 ndjson dumps
    /// render bit-for-bit identical to the prior contract.
    #[test]
    fn declares_association_without_class_name_uses_rails_convention() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:Project", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.project",
            ),
            // Note: NO class_name triple — convention applies.
        ];
        let schema = triples_to_schema(&triples);
        let wp = schema
            .tables
            .iter()
            .find(|t| t.name == "WorkPackage")
            .unwrap();
        let project_id = wp.fields.iter().find(|f| f.name == "project_id").unwrap();
        assert_eq!(
            project_id.kind,
            Kind::Record(vec!["Project".to_string()]).optional(),
            "no class_name triple → use rails convention (Project)",
        );
    }

    /// **D-AR-5.4 — namespaced override (codex P2 on #38)** — Rails
    /// often namespaces a `class_name:` override like
    /// `class_name: 'Storages::FileLink'`, but the extractor declares
    /// tables by their leaf class name (`FileLink`). The bridge must
    /// strip the module-path prefix before the `known_targets`
    /// membership check, otherwise the FK degrades to `option<any>`
    /// despite the leaf table being present.
    #[test]
    fn class_name_override_strips_module_namespace_before_target_lookup() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // Table declared by its leaf name (the extractor strips
            // module paths at rdf:type time).
            t("openproject:FileLink", "rdf:type", "ogit:ObjectType"),
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.file_link",
            ),
            t(
                "openproject:WorkPackage.file_link",
                "association_kind",
                "belongs_to",
            ),
            // Namespaced override: should normalize to `FileLink`.
            t(
                "openproject:WorkPackage.file_link",
                "class_name",
                "Storages::FileLink",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = schema
            .tables
            .iter()
            .find(|t| t.name == "WorkPackage")
            .unwrap();
        let fk = wp.fields.iter().find(|f| f.name == "file_link_id").unwrap();
        assert_eq!(
            fk.kind,
            Kind::Record(vec!["FileLink".to_string()]).optional(),
            "namespaced override must strip `Storages::` and resolve to leaf table FileLink",
        );
    }

    /// **D-AR-5.4 — polymorphic-safe override** — when the
    /// `class_name` override names a table that isn't in the known
    /// targets set (e.g. the override points at an external/abstract
    /// class), the FK still falls back to `option<any>` rather than
    /// inventing a phantom `record<...>`. The override only changes
    /// the candidate target NAME, not the existence check.
    #[test]
    fn class_name_override_falls_back_to_any_when_target_unknown() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // No `Principal` table declared — but the override points at it.
            t(
                "openproject:WorkPackage",
                "declares_association",
                "openproject:WorkPackage.owner",
            ),
            t(
                "openproject:WorkPackage.owner",
                "association_kind",
                "belongs_to",
            ),
            t("openproject:WorkPackage.owner", "class_name", "Principal"),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let owner_id = wp.fields.iter().find(|f| f.name == "owner_id").unwrap();
        // Override class `Principal` isn't a known table → `option<any>`.
        assert_eq!(owner_id.kind, Kind::Any.optional());
    }

    /// **D-AR-5.5** — class-level Rails predicates lift to
    /// `<verb>:<object>` table annotations. Lock the contract on
    /// the 15 newly-wired predicates so a future drop of any single
    /// arm fails this test loudly.
    #[test]
    fn class_level_predicates_lift_to_table_annotations() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            t("openproject:WorkPackage", "has_scope", "active"),
            t("openproject:WorkPackage", "has_default_scope", "visible"),
            t("openproject:WorkPackage", "aliases_method", "old=new"),
            t("openproject:WorkPackage", "aliases_attribute", "label=name"),
            t(
                "openproject:WorkPackage",
                "delegates_to",
                "name=>via:project",
            ),
            t("openproject:WorkPackage", "extends_module", "Reportable"),
            t(
                "openproject:WorkPackage",
                "prepends_module",
                "ModerationGuard",
            ),
            t(
                "openproject:WorkPackage",
                "uses_refinement",
                "Refinements::Money",
            ),
            t("openproject:WorkPackage", "mounts_uploader", "attachment"),
            t("openproject:WorkPackage", "has_paper_trail", "default"),
            t("openproject:WorkPackage", "has_closure_tree", "true"),
            t(
                "openproject:WorkPackage",
                "counter_cultures",
                "project=>count_of:work_packages",
            ),
            t("openproject:WorkPackage", "auto_strips", "subject"),
            t(
                "openproject:WorkPackage",
                "registers_journal_formatter",
                "diff:description",
            ),
            t(
                "openproject:WorkPackage",
                "registers_journal_formatted_fields",
                "description",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let comment = wp
            .comment
            .as_deref()
            .expect("expected comment with AR facts");
        for expected in [
            "scope:active",
            "default_scope:visible",
            "alias_method:old=new",
            "alias_attr:label=name",
            "delegate:name=>via:project",
            "extend:Reportable",
            "prepend:ModerationGuard",
            "using:Refinements::Money",
            "mount:attachment",
            "paper_trail:default",
            "closure_tree:true",
            "counter_culture:project=>count_of:work_packages",
            "auto_strip:subject",
            "journal_formatter:diff:description",
            "journal_fields:description",
        ] {
            assert!(
                comment.contains(expected),
                "expected `{expected}` in annotation; got {comment:?}",
            );
        }
    }

    /// **D-AR-5.5 phantom-table guard** — class-level annotation
    /// predicates on a subject WITHOUT a corresponding `rdf:type
    /// ObjectType` declaration must NOT materialise a phantom table.
    /// (Same invariant the codex P2-1 fix introduced for
    /// `has_attribute`.)
    #[test]
    fn class_level_annotations_respect_phantom_table_guard() {
        let triples = vec![
            // No `rdf:type ObjectType` for `Ghost`.
            t("openproject:Ghost", "has_scope", "visible"),
            t("openproject:Ghost", "delegates_to", "name=>via:owner"),
            t("openproject:Ghost", "mounts_uploader", "avatar"),
            // Real table to confirm the filter is precise.
            t("openproject:Real", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Real"]);
    }

    /// **D-AR-5.7** — `inherits_from` predicate (ruff#19) lifts to a
    /// dedicated `inherits:<Parent>` table annotation, separate from
    /// `include:` (concerns). STI is semantically distinct from
    /// method-body composition — both signals are now available in
    /// the schema output.
    ///
    /// Wire shape: ruff emits the parent as a namespaced IRI
    /// (`openproject:Issue`, mirroring the C++ `InheritsFrom` arm)
    /// so the parent is joinable to its own `ObjectType` triple.
    /// The schema annotation strips the namespace for the display
    /// form (`inherits:Issue`).
    #[test]
    fn inherits_from_lifts_to_dedicated_annotation_with_stripped_namespace() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // STI: WorkPackage inherits from openproject:Issue
            // (ruff#19 emits the namespaced IRI).
            t(
                "openproject:WorkPackage",
                "inherits_from",
                "openproject:Issue",
            ),
            // Distinct from regular includes.
            t(
                "openproject:WorkPackage",
                "includes_module",
                "Acts::Customizable",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let wp = &schema.tables[0];
        let comment = wp.comment.as_deref().expect("expected comment");
        assert!(
            comment.contains("inherits:Issue"),
            "STI parent must lift to `inherits:` annotation with stripped namespace; got {comment:?}",
        );
        // The `openproject:` prefix MUST NOT leak into the annotation
        // (codex P2 on #40).
        assert!(
            !comment.contains("inherits:openproject:"),
            "namespace prefix must be stripped before the annotation; got {comment:?}",
        );
        assert!(
            comment.contains("include:Acts::Customizable"),
            "concern must still lift to `include:` annotation; got {comment:?}",
        );
        // The STI parent must NOT also leak into the include: prefix.
        assert!(
            !comment.contains("include:Issue"),
            "STI parent must not appear under `include:`; got {comment:?}",
        );
    }

    /// **D-AR-5.7 — bare-name fallback** — if a pre-ruff#19 dump or
    /// a non-OP-namespaced parent name arrives (e.g. a third-party
    /// gem's class), the bare form passes through `strip_namespace`
    /// unchanged so the annotation still renders.
    #[test]
    fn inherits_from_bare_name_passes_through_unchanged() {
        let triples = vec![
            t("openproject:WorkPackage", "rdf:type", "ogit:ObjectType"),
            // Bare class name (no `openproject:` prefix).
            t("openproject:WorkPackage", "inherits_from", "Issue"),
        ];
        let schema = triples_to_schema(&triples);
        let comment = schema.tables[0].comment.as_deref().unwrap_or("");
        assert!(
            comment.contains("inherits:Issue"),
            "bare parent name should pass through; got {comment:?}",
        );
    }

    /// **D-AR-5.7 phantom-table guard** — same invariant as the
    /// other predicates: `inherits_from` on an undeclared subject
    /// must NOT materialise a phantom table.
    #[test]
    fn inherits_from_respects_phantom_table_guard() {
        let triples = vec![
            // No `rdf:type ObjectType` for `Ghost`.
            t("openproject:Ghost", "inherits_from", "openproject:Parent"),
            t("openproject:Real", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Real"]);
    }

    /// **D-AR-5.8** — `validation_kind` triple (ruff#21) lifts to a
    /// kind-aware ASSERT clause. The 9-key Rails set composes into
    /// SurrealQL with AND.
    #[test]
    fn validation_kind_lifts_to_kind_aware_assert() {
        let triples = vec![
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "has_attribute", "age"),
            t("openproject:User", "validates_constraint", "age"),
            t("openproject:User.age", "validation_kind", "presence"),
            t("openproject:User.age", "validation_kind", "numericality"),
        ];
        let schema = triples_to_schema(&triples);
        let age = schema.tables[0]
            .fields
            .iter()
            .find(|f| f.name == "age")
            .unwrap();
        let assert_str = age.assert.as_deref().unwrap();
        // Both clauses present, composed with AND. Order depends on
        // BTreeSet iteration (alphabetical), so numericality precedes
        // presence — that's stable.
        assert!(
            assert_str.contains("$value != NONE"),
            "presence clause expected; got {assert_str:?}",
        );
        assert!(
            assert_str.contains("type::is_number($value)"),
            "numericality clause expected (SurrealDB 3.x underscore form); got {assert_str:?}",
        );
        assert!(
            assert_str.contains(" AND "),
            "composed with AND; got {assert_str:?}",
        );
    }

    /// **D-AR-5.8 backward compat** — absence of `validation_kind`
    /// triples keeps the pre-ruff#21 ASSERT shape (`$value != NONE`).
    #[test]
    fn validates_constraint_without_kind_falls_back_to_presence() {
        let triples = vec![
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "has_attribute", "name"),
            // No validation_kind triples — pre-ruff#21 ndjson shape.
            t("openproject:User", "validates_constraint", "name"),
        ];
        let schema = triples_to_schema(&triples);
        let name = schema.tables[0]
            .fields
            .iter()
            .find(|f| f.name == "name")
            .unwrap();
        assert_eq!(
            name.assert.as_deref(),
            Some("$value != NONE"),
            "absent validation_kind → catch-all `$value != NONE`",
        );
    }

    /// **D-AR-5.8 — acceptance** — `validates :tos, acceptance: true`
    /// (Rails ToS-checkbox pattern) lowers to `ASSERT $value == true`.
    #[test]
    fn validation_kind_acceptance_lowers_to_value_eq_true() {
        let triples = vec![
            t("openproject:Account", "rdf:type", "ogit:ObjectType"),
            t("openproject:Account", "has_attribute", "tos"),
            t("openproject:Account", "validates_constraint", "tos"),
            t("openproject:Account.tos", "validation_kind", "acceptance"),
        ];
        let schema = triples_to_schema(&triples);
        let tos = schema.tables[0]
            .fields
            .iter()
            .find(|f| f.name == "tos")
            .unwrap();
        assert_eq!(
            tos.assert.as_deref(),
            Some("$value == true"),
            "acceptance kind → boolean-equality ASSERT",
        );
    }

    /// **D-AR-5.8 — composer unit lock** — `compose_validation_assert`
    /// covers every recognised kind plus the empty / unknown-only
    /// edge cases.
    #[test]
    fn compose_validation_assert_composes_each_kind() {
        use std::collections::BTreeSet;
        let mk =
            |kinds: &[&str]| -> BTreeSet<String> { kinds.iter().map(|s| s.to_string()).collect() };
        let no_params = BTreeSet::new();
        // Single kinds.
        assert_eq!(
            compose_validation_assert(&mk(&["presence"]), &no_params),
            "$value != NONE",
        );
        assert_eq!(
            compose_validation_assert(&mk(&["numericality"]), &no_params),
            "type::is_number($value)",
        );
        assert_eq!(
            compose_validation_assert(&mk(&["acceptance"]), &no_params),
            "$value == true",
        );
        // `absence: true` is the inverse of presence.
        assert_eq!(
            compose_validation_assert(&mk(&["absence"]), &no_params),
            "$value == NONE",
        );
        // `comparison` is parametric (greater_than/less_than/etc.);
        // falls back to presence-style until ruff carries the
        // comparand on the wire.
        assert_eq!(
            compose_validation_assert(&mk(&["comparison"]), &no_params),
            "$value != NONE",
        );
        // Parametric kinds without params fall back to presence-style.
        assert_eq!(
            compose_validation_assert(&mk(&["length"]), &no_params),
            "$value != NONE",
        );
        // Multiple presence-equivalent kinds dedup to one clause.
        assert_eq!(
            compose_validation_assert(&mk(&["presence", "uniqueness", "length"]), &no_params),
            "$value != NONE",
        );
        // Empty set / only-unknown → catch-all.
        assert_eq!(
            compose_validation_assert(&mk(&[]), &no_params),
            "$value != NONE",
        );
        assert_eq!(
            compose_validation_assert(&mk(&["tachyon", "unknown_kind"]), &no_params),
            "$value != NONE",
        );
    }

    /// **D-AR-5.11** — parametric validation lift (ruff#25 sibling):
    /// `validation_param` triples carry inner-hash values
    /// (`length:maximum=255`, `numericality:greater_than=0`, etc.)
    /// — the composer emits typed SurrealQL clauses for each.
    #[test]
    fn compose_validation_assert_lifts_parametric_kinds() {
        use std::collections::BTreeSet;
        let mk_set =
            |items: &[&str]| -> BTreeSet<String> { items.iter().map(|s| s.to_string()).collect() };
        // length:maximum=255 → string::len($value) <= 255
        let kinds = mk_set(&["length"]);
        let params = mk_set(&["length:maximum=255"]);
        assert_eq!(
            compose_validation_assert(&kinds, &params),
            "$value != NONE AND string::len($value) <= 255",
        );
        // length:maximum=N AND length:minimum=M (BTreeSet
        // alphabetical: maximum precedes minimum).
        let params = mk_set(&["length:maximum=255", "length:minimum=3"]);
        let composed = compose_validation_assert(&kinds, &params);
        assert!(
            composed.contains("string::len($value) <= 255"),
            "maximum clause missing: {composed:?}",
        );
        assert!(
            composed.contains("string::len($value) >= 3"),
            "minimum clause missing: {composed:?}",
        );
        // numericality kind + greater_than param → both clauses fire.
        let kinds = mk_set(&["numericality"]);
        let params = mk_set(&["numericality:greater_than=0"]);
        let composed = compose_validation_assert(&kinds, &params);
        assert!(
            composed.contains("type::is_number($value)"),
            "numericality kind clause missing: {composed:?}",
        );
        assert!(
            composed.contains("$value > 0"),
            "greater_than param clause missing: {composed:?}",
        );
        // Range constraint (greater_than + less_than).
        let params = mk_set(&["numericality:greater_than=0", "numericality:less_than=150"]);
        let composed = compose_validation_assert(&kinds, &params);
        assert!(composed.contains("$value > 0"));
        assert!(composed.contains("$value < 150"));
    }

    /// **D-AR-5.11 — unknown param shapes** — a `validation_param`
    /// triple with an unrecognised kind:key skips silently. This is
    /// the forward-compat path for new Rails validators that ruff
    /// hasn't taught the consumer to lower yet.
    #[test]
    fn compose_validation_assert_skips_unknown_params() {
        use std::collections::BTreeSet;
        let mk_set =
            |items: &[&str]| -> BTreeSet<String> { items.iter().map(|s| s.to_string()).collect() };
        let kinds = mk_set(&["length"]);
        let params = mk_set(&[
            "length:maximum=255",
            "length:custom_op=42", // unknown key, but numeric value
        ]);
        let composed = compose_validation_assert(&kinds, &params);
        assert!(composed.contains("string::len($value) <= 255"));
        // The unknown param leaves no trace.
        assert!(!composed.contains("custom_op"));
    }

    /// **D-AR-5.11 — non-literal value safety (codex P2 on #46)** —
    /// Rails commonly writes `length: { maximum: MAX_NAME_LENGTH }`
    /// where the value is a constant reference, not a numeric
    /// literal. Splicing the raw text into the ASSERT would emit
    /// invalid SurrealQL. Non-numeric values MUST be skipped at
    /// the param-clause layer; the kind-level catch-all
    /// (`$value != NONE`) still fires.
    #[test]
    fn param_clause_skips_non_numeric_values() {
        // Constant reference — must NOT splice.
        assert_eq!(param_clause("length:maximum=MAX_NAME_LENGTH"), None);
        // Symbol — must NOT splice.
        assert_eq!(param_clause("comparison:greater_than=:start_date"), None);
        // String literal — must NOT splice.
        assert_eq!(param_clause("length:maximum=\"255\""), None);
        // Float — currently not supported (i64 only); skip
        // until SurrealQL float semantics are decided.
        assert_eq!(param_clause("numericality:less_than=3.14"), None);
        // Numeric literal IS allowed (positive, negative, zero).
        assert_eq!(
            param_clause("length:maximum=255"),
            Some("string::len($value) <= 255".to_string()),
        );
        assert_eq!(
            param_clause("numericality:greater_than=-1"),
            Some("$value > -1".to_string()),
        );
        assert_eq!(
            param_clause("numericality:equal_to=0"),
            Some("$value == 0".to_string()),
        );
        // Whitespace tolerated (`extract_hash_options` trims, but
        // be defensive).
        assert_eq!(
            param_clause("length:maximum= 255 "),
            Some("string::len($value) <= 255".to_string()),
        );
    }

    /// **D-AR-5.11 — comparison kind params (codex P2 on #46)** —
    /// Rails `comparison` validator shares the operator set with
    /// `numericality` (greater_than / less_than / equal_to etc.).
    /// `comparison:greater_than=0` lifts the same way as
    /// `numericality:greater_than=0`.
    #[test]
    fn param_clause_handles_comparison_kind() {
        assert_eq!(
            param_clause("comparison:greater_than=0"),
            Some("$value > 0".to_string()),
        );
        assert_eq!(
            param_clause("comparison:less_than=100"),
            Some("$value < 100".to_string()),
        );
        assert_eq!(
            param_clause("comparison:greater_than_or_equal_to=18"),
            Some("$value >= 18".to_string()),
        );
        assert_eq!(
            param_clause("comparison:less_than_or_equal_to=65"),
            Some("$value <= 65".to_string()),
        );
        assert_eq!(
            param_clause("comparison:equal_to=42"),
            Some("$value == 42".to_string()),
        );
    }

    /// **D-AR-5.11 — end-to-end through `triples_to_schema`** —
    /// a `length: { maximum: 255 }` triple stream lowers to a
    /// `FieldDefinition.assert` carrying both the presence-style
    /// `$value != NONE` (from the `validation_kind` "length") AND
    /// the `string::len($value) <= 255` (from the
    /// `validation_param` "length:maximum=255").
    #[test]
    fn validation_param_end_to_end_through_triples_to_schema() {
        let triples = vec![
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "has_attribute", "name"),
            t("openproject:User", "validates_constraint", "name"),
            t("openproject:User.name", "validation_kind", "length"),
            t(
                "openproject:User.name",
                "validation_param",
                "length:maximum=255",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let name = schema.tables[0]
            .fields
            .iter()
            .find(|f| f.name == "name")
            .unwrap();
        let assert_str = name.assert.as_deref().unwrap();
        assert!(
            assert_str.contains("$value != NONE"),
            "presence clause missing: {assert_str:?}",
        );
        assert!(
            assert_str.contains("string::len($value) <= 255"),
            "length-max param clause missing: {assert_str:?}",
        );
    }

    /// **D-AR-5.9** — `validates :email, uniqueness: true` lifts to
    /// a `DEFINE INDEX ... UNIQUE` statement on the attribute, on
    /// top of the existing presence-style ASSERT. The downstream
    /// catalog bridge maps `unique: true` to `Index::Uniq`.
    #[test]
    fn validation_kind_uniqueness_emits_unique_index() {
        let triples = vec![
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "has_attribute", "email"),
            t("openproject:User", "validates_constraint", "email"),
            t("openproject:User.email", "validation_kind", "uniqueness"),
        ];
        let schema = triples_to_schema(&triples);
        let user = &schema.tables[0];
        let unique_idx = user
            .indices
            .iter()
            .find(|i| i.name == "idx_User_email_unique")
            .expect("uniqueness validation must emit a UNIQUE index");
        assert!(
            unique_idx.unique,
            "the emitted index must carry unique=true",
        );
        assert_eq!(unique_idx.fields, vec!["email".to_string()]);

        // The render must include the UNIQUE keyword.
        let sql = unique_idx.to_sql();
        assert!(
            sql.contains("UNIQUE"),
            "SQL render must include UNIQUE; got {sql:?}",
        );
    }

    /// **D-AR-5.9 — phantom-field guard** — a `validation_kind=
    /// uniqueness` triple on an attribute that wasn't declared via
    /// `has_attribute` must NOT emit a phantom UNIQUE index.
    #[test]
    fn uniqueness_on_phantom_field_emits_no_index() {
        let triples = vec![
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            // No has_attribute for `ghost_attr` — but validation
            // declared.
            t("openproject:User", "validates_constraint", "ghost_attr"),
            t(
                "openproject:User.ghost_attr",
                "validation_kind",
                "uniqueness",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let user = &schema.tables[0];
        assert!(
            !user.indices.iter().any(|i| i.unique),
            "phantom-field uniqueness must NOT emit a UNIQUE index",
        );
    }

    /// **D-AR-5.9 — combined uniqueness + presence** — `validates
    /// :email, presence: true, uniqueness: true` emits both the
    /// presence-style ASSERT AND the UNIQUE index — they're
    /// complementary, not exclusive.
    #[test]
    fn presence_and_uniqueness_emit_both_assert_and_unique_index() {
        let triples = vec![
            t("openproject:User", "rdf:type", "ogit:ObjectType"),
            t("openproject:User", "has_attribute", "email"),
            t("openproject:User", "validates_constraint", "email"),
            t("openproject:User.email", "validation_kind", "presence"),
            t("openproject:User.email", "validation_kind", "uniqueness"),
        ];
        let schema = triples_to_schema(&triples);
        let user = &schema.tables[0];
        // The ASSERT clause lands on the field.
        let email = user.fields.iter().find(|f| f.name == "email").unwrap();
        assert_eq!(email.assert.as_deref(), Some("$value != NONE"));
        // The UNIQUE index lands separately.
        assert!(
            user.indices
                .iter()
                .any(|i| i.name == "idx_User_email_unique" && i.unique),
            "uniqueness must also emit a UNIQUE index alongside the ASSERT",
        );
    }

    /// **D-AR-5.10** — `has_dsl_call`, `column_override`,
    /// `defines_method` lift to dedicated table annotations
    /// (`dsl:`, `col_override:`, `dyn_method:`). These are the
    /// largest remaining catch-all predicates from the OP corpus
    /// (154 `has_dsl_call` triples on the live dump).
    #[test]
    fn dsl_class_level_predicates_lift_to_table_annotations() {
        let triples = vec![
            t(
                "openproject:Import::JiraImportStateMachine",
                "rdf:type",
                "ogit:ObjectType",
            ),
            t(
                "openproject:Import::JiraImportStateMachine",
                "has_dsl_call",
                "state(:configuring)",
            ),
            t(
                "openproject:Import::JiraImportStateMachine",
                "has_dsl_call",
                "after_transition(<expr>)",
            ),
            t(
                "openproject:Import::JiraImportStateMachine.data",
                "column_override",
                "serialize=JSON",
            ),
            t(
                "openproject:Import::JiraImportStateMachine",
                "defines_method",
                "method_for_state=<body>",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let machine = &schema.tables[0];
        let comment = machine.comment.as_deref().expect("expected comment");
        for expected in [
            "dsl:state(:configuring)",
            "dsl:after_transition(<expr>)",
            "col_override:data:serialize=JSON",
            "dyn_method:method_for_state=<body>",
        ] {
            assert!(
                comment.contains(expected),
                "expected `{expected}` in annotation; got {comment:?}",
            );
        }
    }

    /// **D-AR-5.10 phantom-table guard** — `dsl:` / `col_override:`
    /// / `dyn_method:` annotations on undeclared subjects must NOT
    /// materialise phantom tables.
    #[test]
    fn dsl_predicates_respect_phantom_table_guard() {
        let triples = vec![
            t("openproject:Ghost", "has_dsl_call", "foo(...)"),
            t("openproject:Ghost.col", "column_override", "key=val"),
            t("openproject:Ghost", "defines_method", "name=body"),
            t("openproject:Real", "rdf:type", "ogit:ObjectType"),
        ];
        let schema = triples_to_schema(&triples);
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Real"]);
    }

    /// **D-AR-5.10 — column-override dedup safety (codex P2 on #44)** —
    /// two columns with the same override value (e.g. `data` and
    /// `meta` both `serialize=JSON`) must produce two distinct
    /// annotations, not one deduped via the `TableBuilder::build`
    /// annotation-dedup pass. Including the column name in the
    /// annotation prefix keeps each fact uniquely identified.
    #[test]
    fn column_override_includes_column_name_to_avoid_dedup() {
        let triples = vec![
            t("openproject:Account", "rdf:type", "ogit:ObjectType"),
            // Two distinct columns, identical override value.
            t(
                "openproject:Account.preferences",
                "column_override",
                "serialize=JSON",
            ),
            t(
                "openproject:Account.metadata",
                "column_override",
                "serialize=JSON",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let comment = schema.tables[0].comment.as_deref().expect("comment");
        // Both overrides must surface — no dedup collapse.
        assert!(
            comment.contains("col_override:preferences:serialize=JSON"),
            "preferences override missing: {comment:?}",
        );
        assert!(
            comment.contains("col_override:metadata:serialize=JSON"),
            "metadata override missing: {comment:?}",
        );
    }

    /// **D-AR-5.12** — `validates :name, uniqueness: { scope:
    /// :project_id }` (single-symbol form) lifts to a composite
    /// UNIQUE index covering `[name, project_id]`.
    #[test]
    fn uniqueness_scope_single_symbol_emits_composite_unique_index() {
        let triples = vec![
            t("openproject:Category", "rdf:type", "ogit:ObjectType"),
            t("openproject:Category", "has_attribute", "name"),
            t("openproject:Category", "validates_constraint", "name"),
            t("openproject:Category.name", "validation_kind", "uniqueness"),
            t(
                "openproject:Category.name",
                "validation_param",
                "uniqueness:scope=:project_id",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let category = &schema.tables[0];
        let unique = category
            .indices
            .iter()
            .find(|i| i.unique)
            .expect("composite UNIQUE index must be emitted");
        assert_eq!(
            unique.fields,
            vec!["name".to_string(), "project_id".to_string()]
        );
        assert_eq!(unique.name, "idx_Category_name_project_id_unique");
    }

    /// **D-AR-5.12** — `validates :name, uniqueness: { scope:
    /// [:type, :project_id] }` (array form) lifts to a composite
    /// UNIQUE index with the attr + every scope column.
    #[test]
    fn uniqueness_scope_array_emits_multi_column_unique_index() {
        let triples = vec![
            t("openproject:Enumeration", "rdf:type", "ogit:ObjectType"),
            t("openproject:Enumeration", "has_attribute", "name"),
            t("openproject:Enumeration", "validates_constraint", "name"),
            t(
                "openproject:Enumeration.name",
                "validation_kind",
                "uniqueness",
            ),
            t(
                "openproject:Enumeration.name",
                "validation_param",
                "uniqueness:scope=[:type,:project_id]",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let enumeration = &schema.tables[0];
        let unique = enumeration
            .indices
            .iter()
            .find(|i| i.unique)
            .expect("composite UNIQUE index must be emitted");
        assert_eq!(
            unique.fields,
            vec![
                "name".to_string(),
                "type".to_string(),
                "project_id".to_string(),
            ],
        );
        assert_eq!(unique.name, "idx_Enumeration_name_type_project_id_unique");
    }

    /// **D-AR-5.12 — non-symbol scope skip** — when the scope value
    /// is a constant reference (`SCOPE_COLS`) or `<expr>` (unrendered),
    /// the safety check rejects it and the index falls back to the
    /// single-column form rather than splicing a Ruby identifier
    /// into the `DEFINE INDEX FIELDS` clause.
    #[test]
    fn uniqueness_scope_with_non_symbol_value_falls_back_to_single_column() {
        let triples = vec![
            t(
                "openproject:CalculatedValueError",
                "rdf:type",
                "ogit:ObjectType",
            ),
            t(
                "openproject:CalculatedValueError",
                "has_attribute",
                "error_code",
            ),
            t(
                "openproject:CalculatedValueError",
                "validates_constraint",
                "error_code",
            ),
            t(
                "openproject:CalculatedValueError.error_code",
                "validation_kind",
                "uniqueness",
            ),
            // Constant reference — must NOT splice.
            t(
                "openproject:CalculatedValueError.error_code",
                "validation_param",
                "uniqueness:scope=SCOPE_COLS",
            ),
        ];
        let schema = triples_to_schema(&triples);
        let unique = schema.tables[0].indices.iter().find(|i| i.unique).unwrap();
        // No scope columns added — single-column UNIQUE.
        assert_eq!(unique.fields, vec!["error_code".to_string()]);
        assert_eq!(unique.name, "idx_CalculatedValueError_error_code_unique");
    }

    /// **D-AR-5.12** — `extract_uniqueness_scope` unit-level lock
    /// across the value shapes.
    #[test]
    fn extract_uniqueness_scope_handles_value_shapes() {
        use std::collections::BTreeSet;
        let mk = |s: &str| -> BTreeSet<String> { std::iter::once(s.to_string()).collect() };
        // Single symbol.
        assert_eq!(
            extract_uniqueness_scope(&mk("uniqueness:scope=:project_id")),
            vec!["project_id".to_string()],
        );
        // Array of one.
        assert_eq!(
            extract_uniqueness_scope(&mk("uniqueness:scope=[:project_id]")),
            vec!["project_id".to_string()],
        );
        // Array of many (spaced and unspaced separators).
        assert_eq!(
            extract_uniqueness_scope(&mk("uniqueness:scope=[:type, :project_id]")),
            vec!["type".to_string(), "project_id".to_string()],
        );
        assert_eq!(
            extract_uniqueness_scope(&mk("uniqueness:scope=[:a,:b,:c]")),
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        );
        // Non-symbol-shaped values — skip silently.
        assert!(extract_uniqueness_scope(&mk("uniqueness:scope=SCOPE_COLS")).is_empty());
        assert!(extract_uniqueness_scope(&mk("uniqueness:scope=<expr>")).is_empty());
        // Other params — ignored (no `uniqueness:scope=` prefix).
        assert!(extract_uniqueness_scope(&mk("length:maximum=255")).is_empty());
        // Empty.
        assert!(extract_uniqueness_scope(&BTreeSet::new()).is_empty());
    }
}
