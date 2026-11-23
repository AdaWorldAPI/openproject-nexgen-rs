//! An ActiveRecord instance: attribute values plus dirty tracking.
//!
//! Attributes are addressed by column ordinal, never by a hashed name, so a
//! generated accessor is an array index. The name-based accessors exist for
//! the few Rails APIs that take a string (`changes_to_save['state_id']`,
//! `has_attribute?(:owner_id)`), which the compiler resolves to an ordinal at
//! generation time whenever the argument is a literal.

use crate::schema::TableSpec;
use crate::value::Value;

#[derive(Debug, Clone)]
pub struct Record {
    pub table: &'static TableSpec,
    values: Vec<Value>,
    /// Values as last loaded or persisted. `changes_to_save` is the diff.
    original: Vec<Value>,
    new_record: bool,
    /// `saved_changes` after the last successful save.
    saved: Vec<(usize, Value, Value)>,
    /// Loaded `belongs_to` targets: `(association, foreign key at load, target)`.
    /// ActiveRecord reuses a loaded target until its foreign key changes
    /// (`stale_target?`), which is observable — a second `owner` read runs no
    /// query and no `after_initialize`.
    assoc_cache: Vec<(&'static str, Value, Option<Box<Record>>)>,
    /// `attr_accessor` instance state: not columns, not persisted, `nil`
    /// until assigned.
    virtuals: Vec<(&'static str, Value)>,
}

/// Two records are the same Ruby value when they are the same row: same
/// table, same id (ActiveRecord::Core#==). New records equal only themselves,
/// which a value comparison cannot express, so they compare by content.
impl PartialEq for Record {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.table, other.table)
            && match (self.id(), other.id()) {
                (Some(a), Some(b)) => a == b,
                _ => self.values == other.values,
            }
    }
}

impl Record {
    /// `Model.new` — every column at its migration default.
    #[must_use]
    pub fn new(table: &'static TableSpec) -> Self {
        let values: Vec<Value> = table.columns.iter().map(|c| c.default.value()).collect();
        Self {
            table,
            original: values.clone(),
            values,
            new_record: true,
            saved: Vec::new(),
            assoc_cache: Vec::new(),
            virtuals: Vec::new(),
        }
    }

    /// A record as loaded from the store.
    #[must_use]
    pub fn loaded(table: &'static TableSpec, values: Vec<Value>) -> Self {
        Self {
            table,
            original: values.clone(),
            values,
            new_record: false,
            saved: Vec::new(),
            assoc_cache: Vec::new(),
            virtuals: Vec::new(),
        }
    }

    #[must_use]
    pub fn new_record(&self) -> bool {
        self.new_record
    }

    #[must_use]
    pub fn persisted(&self) -> bool {
        !self.new_record
    }

    #[must_use]
    pub fn id(&self) -> Option<i64> {
        self.table.index_of("id").and_then(|i| self.values[i].as_int())
    }

    #[must_use]
    pub fn get(&self, col: usize) -> &Value {
        &self.values[col]
    }

    pub fn set(&mut self, col: usize, v: Value) {
        self.values[col] = v;
    }

    #[must_use]
    pub fn get_named(&self, name: &str) -> Value {
        self.table
            .index_of(name)
            .map_or(Value::Nil, |i| self.values[i].clone())
    }

    pub fn set_named(&mut self, name: &str, v: Value) -> bool {
        match self.table.index_of(name) {
            Some(i) => {
                self.values[i] = v;
                true
            }
            None => false,
        }
    }

    #[must_use]
    pub fn values(&self) -> &[Value] {
        &self.values
    }

    /// `attribute_changed?` / `will_save_change_to_attribute?`.
    #[must_use]
    pub fn changed(&self, col: usize) -> bool {
        self.values[col] != self.original[col]
    }

    /// `changes_to_save[col]` — `[old, new]`, or `nil` when unchanged.
    #[must_use]
    pub fn changes_to_save(&self, col: usize) -> Value {
        if self.changed(col) {
            Value::List(vec![self.original[col].clone(), self.values[col].clone()])
        } else {
            Value::Nil
        }
    }

    #[must_use]
    pub fn has_changes_to_save(&self) -> bool {
        (0..self.values.len()).any(|c| self.changed(c))
    }

    #[must_use]
    pub fn changed_columns(&self) -> Vec<usize> {
        (0..self.values.len()).filter(|&c| self.changed(c)).collect()
    }

    /// `saved_change_to_attribute?` after the last save.
    #[must_use]
    pub fn saved_change(&self, col: usize) -> bool {
        self.saved.iter().any(|(c, _, _)| *c == col)
    }

    /// An `attr_accessor` reader.
    #[must_use]
    pub fn virtual_get(&self, name: &str) -> Value {
        self.virtuals
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(Value::Nil, |(_, v)| v.clone())
    }

    /// An `attr_accessor` writer.
    pub fn virtual_set(&mut self, name: &'static str, v: Value) {
        self.virtuals.retain(|(n, _)| *n != name);
        self.virtuals.push((name, v));
    }

    /// The cached target of `assoc`, if it was loaded for the current value
    /// of its foreign key.
    #[must_use]
    pub fn cached_assoc(&self, assoc: &str, fk: &Value) -> Option<Option<Record>> {
        self.assoc_cache
            .iter()
            .find(|(a, k, _)| *a == assoc && k == fk)
            .map(|(_, _, r)| r.as_deref().cloned())
    }

    pub fn cache_assoc(&mut self, assoc: &'static str, fk: Value, target: Option<Record>) {
        self.assoc_cache.retain(|(a, _, _)| *a != assoc);
        self.assoc_cache.push((assoc, fk, target.map(Box::new)));
    }

    /// Called by the store once a row is written: the record is no longer
    /// new, its changes become `saved_changes`, and its baseline moves.
    pub(crate) fn mark_persisted(&mut self) {
        self.saved = self
            .changed_columns()
            .into_iter()
            .map(|c| (c, self.original[c].clone(), self.values[c].clone()))
            .collect();
        self.original.clone_from(&self.values);
        self.new_record = false;
    }
}
