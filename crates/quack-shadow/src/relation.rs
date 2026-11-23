// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Relations between tables, declared when a catalog is taken in.
//!
//! A join never decides which column is a key or which table multiplies the
//! answer: the class view of the intake did that before the query. The query
//! only looks the declaration up.

use serde_json::Value as Json;

use super::{CaptureError, Catalog, advance};

/// How the rows of a relation's source table read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// The source row holds at most one edge of this predicate (`belongs_to`).
    Edge,
    /// The source table is a class of occurrences of a pair, each of which counts.
    /// The OGAR basis does not carry a habtm join table or its two columns, so the intake (the feed) declares it.
    Occurrences,
}

/// `source.column` names the key `id` of `target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub source: String,
    pub column: String,
    pub target: String,
    pub predicate: String,
    pub reading: Reading,
}

/// What `Catalog::set_classes` made of the class basis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelationStats {
    /// Edges declared from the basis.
    pub declared: usize,
    /// Edges left out because the target class has no table in the class map.
    pub skipped: usize,
}

fn text<'a>(entry: &'a Json, key: &str) -> Result<&'a str, CaptureError> {
    entry
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| CaptureError(format!("relation without `{key}`")))
}

fn parse(entry: &Json) -> Result<Relation, CaptureError> {
    let reading = match text(entry, "reading")? {
        "edge" => Reading::Edge,
        "occurrences" => Reading::Occurrences,
        other => return Err(CaptureError(format!("relation reading `{other}`"))),
    };
    Ok(Relation {
        source: text(entry, "table")?.to_owned(),
        column: text(entry, "column")?.to_owned(),
        target: text(entry, "target")?.to_owned(),
        predicate: text(entry, "predicate")?.to_owned(),
        reading,
    })
}

impl Catalog {
    /// The declared relations, in declaration order.
    pub fn relations(&self) -> &[Relation] {
        &self.relations
    }

    /// What the last `set_classes` declared and skipped.
    pub fn relation_stats(&self) -> RelationStats {
        self.relation_stats
    }

    pub(crate) fn relation(&self, source: &str, column: &str) -> Option<&Relation> {
        self.relations
            .iter()
            .find(|r| r.source == source && r.column == column)
    }

    /// Declares `relation`. The same declaration again is a no-op; a different one for the same column is refused.
    /// A first declaration advances the frame of the source table: an older binding of it is stale.
    pub fn declare(&mut self, relation: Relation) -> Result<(), CaptureError> {
        if let Some(known) = self.relation(&relation.source, &relation.column) {
            return if *known == relation {
                Ok(())
            } else {
                Err(CaptureError(format!(
                    "{}.{} is already declared differently",
                    relation.source, relation.column
                )))
            };
        }
        let at = self
            .tables
            .binary_search_by(|t| t.name.as_str().cmp(&relation.source));
        if let Ok(i) = at
            && let (Some(table), Some(map)) = (self.tables.get_mut(i), self.maps.get_mut(i))
        {
            table.frame += 1;
            advance(map, table, Vec::new());
        }
        self.relations.push(relation);
        Ok(())
    }

    pub(crate) fn declare_feed(&mut self, relations: &Json) -> Result<(), CaptureError> {
        let list = relations
            .as_array()
            .ok_or_else(|| CaptureError("relations is not an array".to_owned()))?;
        for entry in list {
            self.declare(parse(entry)?)?;
        }
        Ok(())
    }
}
