// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The `under` condition: the rows of a table that are named members, or that a
//! declared relation puts under one of them.
//!
//! Zammad asks for "each of these permissions and everything under it" with
//! `name = ? OR name LIKE 'x.%'`, because the tree exists only as dots in the
//! name. Here the intake declares the tree as a relation of identities
//! (`permission_ancestors`), so the lookup is answered over identities: a name
//! becomes a member at the text boundary, the relation is read as a key set, and
//! no text is compared after that.

use lance_graph_quack::Filter;
use lance_graph_quack::bind::Binder;
use serde_json::{Value as Json, json};

use super::join::key_set;
use super::{Catalog, Ctx, LaneKind, Refusal, Session, laned, malformed, matches_none, str_of};

/// What the `under` conditions read so far have cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnderStats {
    /// `under` conditions read.
    pub calls: u64,
    /// Names that were members of the named column.
    pub members_named: u64,
    /// Members the relation put under a named member, over all conditions.
    pub members_under: u64,
}

impl Catalog {
    /// What the `under` conditions read so far have cost.
    pub fn under_stats(&self) -> UnderStats {
        self.under_stats.get()
    }

    fn note_under(&self, named: usize, under: usize) {
        let mut stats = self.under_stats.get();
        stats.calls += 1;
        stats.members_named += named as u64;
        stats.members_under += under as u64;
        self.under_stats.set(stats);
    }
}

/// The relation `via.column` must be declared, and name the key of `table`.
fn declared(catalog: &Catalog, via: &str, column: &str, table: &str) -> Result<(), Refusal> {
    match catalog.relation(via, column) {
        Some(relation) if relation.target == table => Ok(()),
        _ => Err(Refusal::UndeclaredRelation {
            table: via.to_owned(),
            column: column.to_owned(),
        }),
    }
}

/// Members the relation puts under `named`: the `member` values of the live
/// rows of `via` whose `of` value is one of `named`.
#[cfg(not(feature = "disable-under-relation"))]
fn beneath(
    session: &Session<'_>,
    via: &str,
    (member, of): (&str, &str),
    named: &[u32],
) -> Result<Vec<u32>, Refusal> {
    let at = session
        .catalog()
        .tables
        .iter()
        .position(|t| t.name == via)
        .ok_or_else(|| Refusal::UnknownTable(via.to_owned()))?;
    let leaf = json!({"op": "in", "col": of, "values": named});
    key_set(session, (at, session.table(at)?), member, &[&leaf], false)
}

// Disable run: the relation puts nothing under a member, so only the named
// members themselves match.
#[cfg(feature = "disable-under-relation")]
fn beneath(
    _session: &Session<'_>,
    _via: &str,
    _columns: (&str, &str),
    _named: &[u32],
) -> Result<Vec<u32>, Refusal> {
    Ok(Vec::new())
}

/// `{"op": "under", "table", "via", "member", "of", "names": {"col", "values"}}`.
pub(crate) fn under(session: &Session<'_>, cx: &Ctx<'_>, n: &Json) -> Result<Filter, Refusal> {
    let (table, via) = (str_of(n, "table")?, str_of(n, "via")?);
    let (member, of) = (str_of(n, "member")?, str_of(n, "of")?);
    let names = n
        .get("names")
        .ok_or_else(|| malformed("under without names"))?;
    let column = str_of(names, "col")?;
    let values = names
        .get("values")
        .and_then(Json::as_array)
        .ok_or_else(|| malformed("under names without values"))?;
    if table != cx.table.name {
        return Err(Refusal::ForeignTableColumn {
            table: table.to_owned(),
            field: column.to_owned(),
        });
    }
    declared(cx.catalog, via, member, table)?;
    declared(cx.catalog, via, of, table)?;
    let at = usize::try_from(cx.id.0).map_err(|_| malformed("table index"))?;

    let texts: Vec<&str> = values
        .iter()
        .map(|v| {
            v.as_str().ok_or_else(|| Refusal::TextLiteral {
                field: column.to_owned(),
            })
        })
        .collect::<Result<_, _>>()?;
    let named = texts
        .iter()
        .filter(|text| cx.catalog.from_string(table, column, text).is_some())
        .count();
    let own = json!({"op": "in", "col": column, "values": texts});
    let held = key_set(session, (at, cx.table), "id", &[&own], false)?;
    let beneath = beneath(session, via, (member, of), &held)?;
    cx.catalog.note_under(named, beneath.len());

    let ids: std::collections::BTreeSet<u32> = held.into_iter().chain(beneath).collect();
    let (lane, field) = laned(cx, "id")?;
    if lane.kind != LaneKind::Int {
        return Err(malformed("under: `id` is not an integer column"));
    }
    if ids.is_empty() {
        return Ok(matches_none(Binder::live(cx.catalog, cx.id)));
    }
    Ok(Filter::in_u32(field.col, ids))
}
