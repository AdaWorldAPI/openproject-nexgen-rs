// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The OGAR-derived class basis.
//!
//! Every field of a class is a position in an ordered basis, and every
//! position carries a `predicate_iri` that names the class, the column and its
//! type. DT64 digests those IRIs when a class is registered and refuses a
//! binding whose basis moved, so a column of another class that happens to
//! share a label and a type is a different field here, and a reordered basis
//! is a refusal and not a silently different answer.
//!
//! Classids are lab identities for this experiment: `compose_classid(concept,
//! 0x0001)` with concept ids from [`CONCEPT_BASE`] upward, in input order.
//! They are not minted identities.

use std::collections::BTreeMap;

use lance_graph_contract::class_view::{ClassId, ClassView};
use lance_graph_contract::ogar_codebook::compose_classid;
use lance_graph_contract::ontology::{DisplayTemplate, FieldRef};
use ogar_vocab::{AssociationKind, Class};

/// First lab concept id; class `i` of the input is `CONCEPT_BASE + i`.
/// Lab classid, not a minted identity.
pub const CONCEPT_BASE: u16 = 0x7F10;

/// Which `predicate_iri` convention a basis uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IriMode {
    /// `ogar:{ns}:{Class}#{attr}@{type}{!|?}`: carries class, name, type and
    /// nullability.
    Identity,
    /// OGAR's own `ogar-class-view` convention: the bare name as written.
    /// Exists only for the negative oracle and the disable run.
    BareNames,
}

/// What a basis position is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldMeta {
    /// A column OGAR carries as an attribute.
    Column {
        name: String,
        type_name: String,
        required: bool,
    },
    /// The foreign-key column of a `belongs_to`.
    AssocFk {
        assoc: String,
        target: String,
        column: String,
        required: bool,
    },
    /// A field a transposed method writes: its selection mask or its result.
    Output { method: String },
}

/// One class of the basis.
#[derive(Debug, Clone)]
pub struct ClassBasis {
    pub namespace: String,
    pub class: Class,
    pub concept: u16,
    pub fields: Vec<FieldRef>,
    pub meta: Vec<FieldMeta>,
}

impl ClassBasis {
    /// The lab classid of the class: `compose_classid(concept, 0x0001)`.
    #[must_use]
    pub fn classid(&self) -> u32 {
        compose_classid(self.concept, 0x0001)
    }
}

/// The ordered OGAR basis of a set of classes; a [`ClassView`].
#[derive(Debug, Clone)]
pub struct OgarBasis {
    classes: Vec<ClassBasis>,
    mode: IriMode,
    /// Class -> table, for classes whose table is not in the configured
    /// migrations: their column set is unknown, not known to be smaller.
    unharvested: BTreeMap<String, String>,
}

fn camelize(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        })
        .collect()
}

/// The class a written constant names, looked up from the owner's namespace
/// outward (ActiveRecord's `compute_type`), among the known classes.
fn resolve_name(written: &str, owner: &str, known: &[String]) -> String {
    // `::X` names the top-level constant only.
    if let Some(top) = written.strip_prefix("::") {
        return top.to_string();
    }
    let mut scope: Vec<&str> = owner.split("::").collect();
    loop {
        let candidate = if scope.is_empty() { written.to_string() } else { format!("{}::{written}", scope.join("::")) };
        if known.contains(&candidate) {
            return candidate;
        }
        if scope.pop().is_none() {
            break;
        }
    }
    written.to_string()
}

fn mark(required: bool) -> char {
    if required { '!' } else { '?' }
}

impl OgarBasis {
    /// The identity-carrying basis. Under the `disable-ogar-identity` feature
    /// it behaves as [`OgarBasis::bare_names`]: the disable run of the claim
    /// that OGAR identity is what makes a swapped field detectable.
    #[must_use]
    pub fn new(classes: Vec<(String, Class, u16)>) -> Self {
        if cfg!(feature = "disable-ogar-identity") {
            Self::build(classes, IriMode::BareNames)
        } else {
            Self::build(classes, IriMode::Identity)
        }
    }

    /// OGAR's `ogar-class-view` convention (`lift_object_view`): the
    /// `predicate_iri` is the bare name as written.
    #[must_use]
    pub fn bare_names(classes: Vec<(String, Class, u16)>) -> Self {
        Self::build(classes, IriMode::BareNames)
    }

    fn build(input: Vec<(String, Class, u16)>, mode: IriMode) -> Self {
        Self::build_with(input, mode, &BTreeMap::new())
    }

    fn build_with(
        input: Vec<(String, Class, u16)>,
        mode: IriMode,
        column_not_null: &BTreeMap<String, BTreeMap<String, bool>>,
    ) -> Self {
        let known: Vec<String> = input.iter().map(|(_, c, _)| c.name.clone()).collect();
        let classes = input
            .into_iter()
            .map(|(namespace, class, concept)| {
                let mut fields = Vec::new();
                let mut meta = Vec::new();
                for a in &class.attributes {
                    let type_name = a.type_name.clone().unwrap_or_else(|| "untyped".to_string());
                    let required = a.options.required == Some(true);
                    let iri = match mode {
                        IriMode::Identity => {
                            format!("ogar:{namespace}:{}#{}@{type_name}{}", class.name, a.name, mark(required))
                        }
                        IriMode::BareNames => a.name.clone(),
                    };
                    fields.push(FieldRef::new(iri, a.name.clone()));
                    meta.push(FieldMeta::Column { name: a.name.clone(), type_name, required });
                }
                for assoc in class.associations.iter().filter(|a| a.kind == AssociationKind::BelongsTo) {
                    let column = assoc.foreign_key.clone().unwrap_or_else(|| format!("{}_id", assoc.name));
                    if meta.iter().any(|m| matches!(m, FieldMeta::Column { name, .. } if *name == column)) {
                        continue;
                    }
                    let written = assoc.class_name.clone().unwrap_or_else(|| camelize(&assoc.name));
                    let target = resolve_name(&written, &class.name, &known);
                    let required = column_not_null.get(&class.name).and_then(|c| c.get(&column)).copied().unwrap_or(false);
                    let iri = match mode {
                        IriMode::Identity => format!(
                            "ogar:{namespace}:{}#{}->{target}@fk{}",
                            class.name,
                            assoc.name,
                            mark(required)
                        ),
                        IriMode::BareNames => assoc.name.clone(),
                    };
                    fields.push(FieldRef::new(iri, column.clone()));
                    meta.push(FieldMeta::AssocFk { assoc: assoc.name.clone(), target, column, required });
                }
                ClassBasis { namespace, class, concept, fields, meta }
            })
            .collect();
        Self { classes, mode, unharvested: BTreeMap::new() }
    }

    /// Supply the schema's NOT NULL per class and column; a foreign key is
    /// `required` only when its column is listed as NOT NULL (an absent
    /// column is treated as nullable). Rebuilds the basis, so call it first.
    #[must_use]
    pub fn with_column_not_null(self, map: BTreeMap<String, BTreeMap<String, bool>>) -> Self {
        let input = self.classes.into_iter().map(|c| (c.namespace, c.class, c.concept)).collect();
        let mut rebuilt = Self::build_with(input, self.mode, &map);
        rebuilt.unharvested = self.unharvested;
        rebuilt
    }

    /// Record which classes' tables the compiler's schema does not hold.
    #[must_use]
    pub fn with_unharvested(mut self, tables: BTreeMap<String, String>) -> Self {
        self.unharvested = tables;
        self
    }

    /// The table of `class` when it is not in the configured migrations.
    #[must_use]
    pub fn unharvested_table(&self, class: &str) -> Option<&str> {
        self.unharvested.get(class).map(String::as_str)
    }

    #[must_use]
    pub fn mode(&self) -> IriMode {
        self.mode
    }

    #[must_use]
    pub fn classes(&self) -> &[ClassBasis] {
        &self.classes
    }

    #[must_use]
    pub fn class(&self, name: &str) -> Option<&ClassBasis> {
        self.classes.iter().find(|c| c.class.name == name)
    }

    #[must_use]
    pub fn by_concept(&self, concept: u16) -> Option<&ClassBasis> {
        self.classes.iter().find(|c| c.concept == concept)
    }

    /// A basis of only the named classes, in the order given (concept ids
    /// and field order unchanged).
    #[must_use]
    pub fn subset(&self, names: &[&str]) -> Self {
        let mut classes = Vec::new();
        for n in names {
            if let Some(c) = self.class(n)
                && !classes.iter().any(|k: &ClassBasis| k.class.name == c.class.name)
            {
                classes.push(c.clone());
            }
        }
        Self { classes, mode: self.mode, unharvested: self.unharvested.clone() }
    }

    /// The lab classid of `name`.
    #[must_use]
    pub fn classid(&self, name: &str) -> Option<u32> {
        self.class(name).map(ClassBasis::classid)
    }

    fn class_mut(&mut self, name: &str) -> Option<&mut ClassBasis> {
        self.classes.iter_mut().find(|c| c.class.name == name)
    }

    /// The class `assoc` of `owner` points at, by the owner's association.
    #[must_use]
    pub fn association_target(&self, owner: &str, assoc: &str) -> Option<String> {
        let c = self.class(owner)?;
        let a = c.class.associations.iter().find(|a| a.name == assoc)?;
        let known: Vec<String> = self.classes.iter().map(|c| c.class.name.clone()).collect();
        let written = a.class_name.clone().unwrap_or_else(|| {
            let base = if matches!(a.kind, AssociationKind::HasMany | AssociationKind::HasAndBelongsToMany) {
                rails_compiler::singular(&a.name)
            } else {
                a.name.clone()
            };
            camelize(&base)
        });
        Some(resolve_name(&written, owner, &known))
    }

    /// The position of `key` in `class`'s own basis: a column, the foreign-key
    /// column of a `belongs_to`, or the `belongs_to` name itself. A key is
    /// never looked up in another class.
    #[must_use]
    pub fn resolve(&self, class: &str, key: &str) -> Option<(usize, &FieldMeta)> {
        let own = self.class(class).and_then(|c| {
            c.meta.iter().enumerate().find(|(_, m)| match m {
                FieldMeta::Column { name, .. } => name == key,
                FieldMeta::AssocFk { assoc, column, .. } => assoc == key || column == key,
                FieldMeta::Output { .. } => false,
            })
        });
        if cfg!(feature = "disable-field-resolution") && own.is_none() {
            // The disabled gate: resolve by label in whichever class has it.
            return self.classes.iter().find_map(|c| {
                c.meta.iter().enumerate().find(|(_, m)| match m {
                    FieldMeta::Column { name, .. } => name == key,
                    FieldMeta::AssocFk { assoc, column, .. } => assoc == key || column == key,
                    FieldMeta::Output { .. } => false,
                })
            });
        }
        own
    }

    /// The `belongs_to` of `target` that points back at `owner` through the
    /// owner's `has_many`/`has_one` `assoc`: by `inverse_of`, else by
    /// `foreign_key`, else by Rails' default `<owner>_id`.
    #[must_use]
    pub fn inverse_fk(&self, owner: &str, assoc: &str) -> Option<(String, usize)> {
        let target = self.association_target(owner, assoc)?;
        let a = self.class(owner)?.class.associations.iter().find(|a| a.name == assoc)?;
        let t = self.class(&target)?;
        let by_assoc = |name: &str| {
            t.meta.iter().enumerate().find_map(|(i, m)| match m {
                FieldMeta::AssocFk { assoc, .. } if assoc == name => Some(i),
                _ => None,
            })
        };
        let by_column = |col: &str| {
            t.meta.iter().enumerate().find_map(|(i, m)| match m {
                FieldMeta::AssocFk { column, .. } if column == col => Some(i),
                FieldMeta::Column { name, .. } if name == col => Some(i),
                _ => None,
            })
        };
        let position = a
            .inverse_of
            .as_deref()
            .and_then(by_assoc)
            .or_else(|| a.foreign_key.as_deref().and_then(by_column))
            .or_else(|| {
                let base = owner.rsplit("::").next().unwrap_or(owner);
                let default = format!("{}_id", rails_compiler::semantic::underscore(base));
                by_column(&default)
            })?;
        let name = match &t.meta[position] {
            FieldMeta::AssocFk { column, .. } | FieldMeta::Column { name: column, .. } => column.clone(),
            FieldMeta::Output { method } => method.clone(),
        };
        Some((name, position))
    }

    /// Append the two output fields of a transposed method to `class`: a
    /// selection mask and a result. Returns `(mask, result)` positions.
    pub fn append_output(&mut self, class: &str, method: &str, terminal: &str) -> Option<(u8, u8)> {
        let mode = self.mode;
        let c = self.class_mut(class)?;
        let at = c.fields.len();
        if at + 2 > 256 {
            return None;
        }
        let ns = c.namespace.clone();
        let cname = c.class.name.clone();
        for (suffix, label) in [("mask".to_string(), format!("{method}:mask")), (terminal.to_string(), method.to_string())] {
            let iri = match mode {
                IriMode::Identity => format!("ogar:{ns}:{cname}#scope:{method}.{suffix}"),
                IriMode::BareNames => label.clone(),
            };
            c.fields.push(FieldRef::new(iri, label));
            c.meta.push(FieldMeta::Output { method: method.to_string() });
        }
        Some((u8::try_from(at).ok()?, u8::try_from(at + 1).ok()?))
    }

    /// The field at `position` of `class`.
    #[must_use]
    pub fn field(&self, class: &str, position: usize) -> Option<&FieldRef> {
        self.class(class).and_then(|c| c.fields.get(position))
    }

    /// A copy with `class`'s field `position` replaced.
    #[must_use]
    pub fn with_field_replaced(&self, class: &str, position: usize, field: FieldRef) -> Self {
        let mut b = self.clone();
        if let Some(slot) = b.class_mut(class).and_then(|c| c.fields.get_mut(position)) {
            *slot = field;
        }
        b
    }

    /// A copy with two positions of `class` exchanged (a reorder that keeps the length).
    #[must_use]
    pub fn with_swapped(&self, class: &str, a: usize, b: usize) -> Self {
        let mut out = self.clone();
        if let Some(c) = out.class_mut(class)
            && a < c.fields.len()
            && b < c.fields.len()
        {
            c.fields.swap(a, b);
            c.meta.swap(a, b);
        }
        out
    }

    /// A copy with the label of `class`'s field `position` replaced; the
    /// `predicate_iri` is untouched.
    #[must_use]
    pub fn with_label(&self, class: &str, position: usize, label: &str) -> Self {
        let mut b = self.clone();
        if let Some(f) = b.class_mut(class).and_then(|c| c.fields.get_mut(position)) {
            f.label = label.to_string();
        }
        b
    }

    /// The first `FieldRef` of `class` whose label is `label`: the field of
    /// another class a wrong-identity swap puts at the same position.
    #[must_use]
    pub fn field_labelled(&self, class: &str, label: &str) -> Option<FieldRef> {
        self.class(class)?.fields.iter().find(|f| f.label == label).cloned()
    }
}

impl ClassView for OgarBasis {
    fn fields(&self, class: ClassId) -> &[FieldRef] {
        self.by_concept(class).map_or(&[], |c| c.fields.as_slice())
    }
    fn template(&self, _class: ClassId) -> DisplayTemplate {
        DisplayTemplate::Detail
    }
    fn dolce_category_id(&self, _class: ClassId) -> u8 {
        0
    }
}
