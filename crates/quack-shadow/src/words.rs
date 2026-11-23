// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The one place an operator name becomes a filter constructor.
//!
//! The polyfill lens `0xFFFF :: 0xF000` of `lance-graph-r2il` holds the
//! relational words by position; SQL keywords and quack variants are aliases of
//! those positions. A condition names an operator in SQL, `Word::named` finds
//! its position, and the position's Quack name picks the constructor.

use std::collections::BTreeMap;

use lance_graph_quack::{Cmp, Col, Filter, ForeignLane};
use lance_graph_r2il::polyfill::{AliasLanguage, POLYFILL, position_of};

use super::{Catalog, Refusal};

/// A position of the polyfill lens that SQL has a name for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Word(u8);

impl Word {
    /// The word whose SQL name is `sql_name`.
    pub fn named(sql_name: &str) -> Result<Word, Refusal> {
        let wanted = if cfg!(feature = "disable-word-lookup") && sql_name == "AND" {
            "OR"
        } else {
            sql_name
        };
        position_of(wanted)
            .filter(|&p| {
                POLYFILL[usize::from(p)]
                    .name_of(AliasLanguage::Sql)
                    .is_some()
            })
            .map(Word)
            .ok_or_else(|| Refusal::NoWord(sql_name.to_owned()))
    }

    /// The position in [`POLYFILL`].
    pub fn position(self) -> u8 {
        self.0
    }

    /// The word's SQL name.
    pub fn sql(self) -> &'static str {
        POLYFILL[usize::from(self.0)].name_in(AliasLanguage::Sql)
    }

    /// The word's Quack name, if Quack has one.
    pub fn quack(self) -> Option<&'static str> {
        POLYFILL[usize::from(self.0)].name_of(AliasLanguage::Quack)
    }
}

/// What this crate builds for a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Construct {
    Equal,
    And,
    Or,
    Negate,
}

/// Quack name -> constructor: the only table of constructors in the crate.
/// `Cmp::EqU32` is the equality of this crate's `u32` lanes.
pub const CONSTRUCTORS: [(&str, Construct); 5] = [
    ("Cmp::EqI32", Construct::Equal),
    ("Cmp::EqU32", Construct::Equal),
    ("Filter::and", Construct::And),
    ("Filter::or", Construct::Or),
    ("Filter::negate", Construct::Negate),
];

/// The JSON condition ops that have a word, with the SQL name of that word.
const JSON_OPS: [(&str, &str); 4] = [("eq", "="), ("and", "AND"), ("or", "OR"), ("not", "NOT")];

/// The operators that have no word in the polyfill lens: they are implemented
/// here, not read from it.
const WITHOUT_WORD: [&str; 6] = ["ne", "in", "not_in", "under", "bound_sql", "LIKE"];

/// The SQL name of the word a JSON op stands for.
pub(crate) fn sql_name_of(op: &str) -> Option<&'static str> {
    JSON_OPS.iter().find(|(o, _)| *o == op).map(|(_, sql)| *sql)
}

/// Names `op` when it has no word.
pub(crate) fn without_word(op: &str) -> Option<&'static str> {
    WITHOUT_WORD.iter().copied().find(|name| *name == op)
}

/// The constructor of `word`, by its Quack name.
pub(crate) fn construct(word: Word) -> Result<Construct, Refusal> {
    word.quack()
        .and_then(|quack| CONSTRUCTORS.iter().find(|(name, _)| *name == quack))
        .map(|(_, construct)| *construct)
        .ok_or_else(|| Refusal::NoWord(word.sql().to_owned()))
}

fn shape(word: Word, wanted: &[Construct]) -> Result<Construct, Refusal> {
    let got = construct(word)?;
    if wanted.contains(&got) {
        Ok(got)
    } else {
        Err(Refusal::NoWord(word.sql().to_owned()))
    }
}

pub(crate) fn equal(word: Word, col: Col, value: u32) -> Result<Filter, Refusal> {
    shape(word, &[Construct::Equal])?;
    Ok(Filter::cmp(col, Cmp::EqU32(value)))
}

/// Equality read through a foreign lane: the same word, over a linked column.
pub(crate) fn equal_via(
    word: Word,
    col: Col,
    key: ForeignLane,
    value: u32,
) -> Result<Filter, Refusal> {
    shape(word, &[Construct::Equal])?;
    Ok(Filter::eq_u32_via(col, key, value))
}

pub(crate) fn junction(word: Word, parts: Vec<Filter>) -> Result<Filter, Refusal> {
    Ok(match shape(word, &[Construct::And, Construct::Or])? {
        Construct::And => Filter::and(parts),
        _ => Filter::or(parts),
    })
}

pub(crate) fn negate(word: Word, part: Filter) -> Result<Filter, Refusal> {
    shape(word, &[Construct::Negate])?;
    Ok(Filter::negate(part))
}

/// What the conditions read so far have resolved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordStats {
    /// Occurrences resolved, by position of the polyfill lens.
    pub by_position: BTreeMap<u8, u64>,
    /// Occurrences of operators that have no word, by name.
    pub without_word: BTreeMap<String, u64>,
}

impl Catalog {
    /// What the conditions read so far have resolved.
    pub fn word_stats(&self) -> WordStats {
        self.word_stats.borrow().clone()
    }

    pub(crate) fn note_word(&self, word: Word) {
        *self
            .word_stats
            .borrow_mut()
            .by_position
            .entry(word.position())
            .or_default() += 1;
    }

    /// The word a JSON op stands for, counted.
    pub(crate) fn word_of_op(&self, op: &str) -> Result<Word, Refusal> {
        let sql = sql_name_of(op).ok_or_else(|| Refusal::NoWord(op.to_owned()))?;
        let word = Word::named(sql)?;
        self.note_word(word);
        Ok(word)
    }

    /// Whether `op` is a connective of the `wanted` kind. A hit is counted.
    pub(crate) fn is_junction(&self, op: &str, wanted: Construct) -> bool {
        let Some(word) = sql_name_of(op).and_then(|sql| Word::named(sql).ok()) else {
            return false;
        };
        let hit = construct(word).is_ok_and(|c| c == wanted);
        if hit {
            self.note_word(word);
        }
        hit
    }

    /// Counts an occurrence of `op` when it has no word.
    pub(crate) fn note_without_word(&self, op: &str) {
        if let Some(name) = without_word(op) {
            *self
                .word_stats
                .borrow_mut()
                .without_word
                .entry(name.to_owned())
                .or_default() += 1;
        }
    }
}
