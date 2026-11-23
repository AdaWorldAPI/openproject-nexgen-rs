// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A SQL fragment with positional binds, as `where("a = ? AND (b = ? OR b LIKE ?)", ...)`
//! sends it: read into the same filter tree the JSON ops give. The reader knows
//! `expr := term (OR term)*`, `term := factor (AND factor)*`,
//! `factor := '(' expr ')' | column ('=' | 'LIKE') '?'` and nothing else; what
//! it does not know is refused by name.

use lance_graph_quack::Filter;
use lance_graph_quack::bind::Pattern;
use serde_json::{Value as Json, json};

use super::words::{self, Construct, Word};
use super::{Catalog, Ctx, LaneKind, Refusal, equal_leaf, laned, matches_none, str_of};

/// What the fragments read so far have cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LikeStats {
    /// Fragments read.
    pub fragments: u64,
    /// LIKE leaves read.
    pub like_leaves: u64,
    /// Codes those leaves matched, together.
    pub matched_codes: u64,
}

impl Catalog {
    /// What the SQL fragments read so far have cost.
    pub fn like_stats(&self) -> LikeStats {
        self.like_stats.get()
    }

    fn note_like(&self, matched: usize) {
        let mut stats = self.like_stats.get();
        stats.like_leaves += 1;
        stats.matched_codes += matched as u64;
        self.like_stats.set(stats);
    }
}

fn refuse<T>(message: impl Into<String>) -> Result<T, Refusal> {
    Err(Refusal::BoundSql(message.into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Open,
    Close,
    Dot,
    Eq,
    Bind,
    Word(String),
    Quoted(String),
}

impl Tok {
    fn show(&self) -> String {
        match self {
            Tok::Open => "(".to_owned(),
            Tok::Close => ")".to_owned(),
            Tok::Dot => ".".to_owned(),
            Tok::Eq => "=".to_owned(),
            Tok::Bind => "?".to_owned(),
            Tok::Word(w) => w.clone(),
            Tok::Quoted(w) => format!("\"{w}\""),
        }
    }
}

fn tokens(sql: &str) -> Result<Vec<Tok>, Refusal> {
    let mut out = Vec::new();
    let mut chars = sql.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '(' | ')' | '.' | '=' | '?' => {
                chars.next();
                out.push(match c {
                    '(' => Tok::Open,
                    ')' => Tok::Close,
                    '.' => Tok::Dot,
                    '=' => Tok::Eq,
                    _ => Tok::Bind,
                });
            }
            '"' => {
                chars.next();
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('"') if chars.peek() == Some(&'"') => {
                            chars.next();
                            name.push('"');
                        }
                        Some('"') => break,
                        Some(c) => name.push(c),
                        None => return refuse("a quoted identifier is not closed"),
                    }
                }
                out.push(Tok::Quoted(name));
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        word.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(Tok::Word(word));
            }
            other => return refuse(format!("`{other}` is not understood")),
        }
    }
    Ok(out)
}

fn is_keyword(word: &str, keyword: &str) -> bool {
    word.eq_ignore_ascii_case(keyword)
}

/// The operator between a column and its `?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    Word(Word),
    Like,
}

/// What the grammar walk builds: the reader a filter, `columns_of` a list of names.
trait Sink {
    type Out;
    fn comparison(&mut self, column: &str, op: Operator) -> Result<Self::Out, Refusal>;
    fn join(&mut self, parts: Vec<Self::Out>, word: Word) -> Result<Self::Out, Refusal>;
}

struct Reader<'a, S> {
    table: &'a str,
    tokens: Vec<Tok>,
    at: usize,
    sink: S,
}

impl<S: Sink> Reader<'_, S> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Tok> {
        let tok = self.tokens.get(self.at).cloned();
        self.at += usize::from(tok.is_some());
        tok
    }

    /// The word at the cursor when it is a connective of the wanted kind; where a
    /// condition ends, a name is an operator, so the lens decides which.
    fn connective(&self, wanted: Construct) -> Option<Word> {
        let Some(Tok::Word(w)) = self.peek() else {
            return None;
        };
        let word = Word::named(&w.to_ascii_uppercase()).ok()?;
        (words::construct(word).ok()? == wanted).then_some(word)
    }

    fn many(&mut self, or: bool) -> Result<S::Out, Refusal> {
        let wanted = if or { Construct::Or } else { Construct::And };
        let first = self.operand(or)?;
        let Some(mut word) = self.connective(wanted) else {
            return Ok(first);
        };
        let mut parts = vec![first];
        while let Some(found) = self.connective(wanted) {
            self.at += 1;
            word = found;
            parts.push(self.operand(or)?);
        }
        self.sink.join(parts, word)
    }

    fn operand(&mut self, or: bool) -> Result<S::Out, Refusal> {
        if or { self.term() } else { self.factor() }
    }

    fn expr(&mut self) -> Result<S::Out, Refusal> {
        self.many(true)
    }

    fn term(&mut self) -> Result<S::Out, Refusal> {
        self.many(false)
    }

    fn factor(&mut self) -> Result<S::Out, Refusal> {
        if self.peek() == Some(&Tok::Open) {
            self.at += 1;
            let inner = self.expr()?;
            return match self.next() {
                Some(Tok::Close) => Ok(inner),
                Some(other) => refuse(format!("expected `)`, found `{}`", other.show())),
                None => refuse("expected `)`, found the end of the fragment"),
            };
        }
        self.comparison()
    }

    fn identifier(&mut self) -> Result<String, Refusal> {
        match self.next() {
            Some(Tok::Quoted(name)) => Ok(name),
            Some(Tok::Word(w))
                if ["and", "or", "like", "not"]
                    .iter()
                    .any(|k| is_keyword(&w, k)) =>
            {
                refuse(format!("expected a column, found `{w}`"))
            }
            Some(Tok::Word(w)) => Ok(w),
            Some(other) => refuse(format!("expected a column, found `{}`", other.show())),
            None => refuse("expected a column, found the end of the fragment"),
        }
    }

    fn comparison(&mut self) -> Result<S::Out, Refusal> {
        let first = self.identifier()?;
        let (qualifier, column) = if self.peek() == Some(&Tok::Dot) {
            self.at += 1;
            (Some(first), self.identifier()?)
        } else {
            (None, first)
        };
        if self.peek() == Some(&Tok::Open) {
            return refuse(format!(
                "the function call `{column}(...)` is not understood"
            ));
        }
        let table = self.table;
        if let Some(q) = qualifier
            && q != table
        {
            return refuse(format!(
                "the column `{q}.{column}` names another table than `{table}`"
            ));
        }
        let op = match self.next() {
            Some(eq @ Tok::Eq) => Operator::Word(Word::named(&eq.show())?),
            Some(Tok::Word(w)) if words::without_word(&w.to_ascii_uppercase()) == Some("LIKE") => {
                Operator::Like
            }
            Some(other) => {
                return refuse(format!(
                    "expected `=` or LIKE after `{column}`, found `{}`",
                    other.show()
                ));
            }
            None => {
                return refuse(format!(
                    "expected `=` or LIKE after `{column}`, found the end of the fragment"
                ));
            }
        };
        if self.next() != Some(Tok::Bind) {
            self.at -= 1;
            return refuse(format!(
                "expected `?` after the operator on `{column}`, found {}",
                self.peek()
                    .map_or("the end of the fragment".to_owned(), |t| format!(
                        "`{}`",
                        t.show()
                    ))
            ));
        }
        self.sink.comparison(&column, op)
    }

    fn whole(&mut self) -> Result<S::Out, Refusal> {
        let out = self.expr()?;
        if let Some(rest) = self.peek() {
            return refuse(format!(
                "`{}` is not understood after the condition",
                rest.show()
            ));
        }
        Ok(out)
    }
}

struct Filters<'a, 'c> {
    cx: &'a Ctx<'c>,
    binds: &'a [Json],
    used: usize,
}

impl Sink for Filters<'_, '_> {
    type Out = Filter;

    fn comparison(&mut self, column: &str, op: Operator) -> Result<Filter, Refusal> {
        let Some(bind) = self.binds.get(self.used) else {
            return refuse(format!(
                "`?` number {} has no bind: {} given",
                self.used + 1,
                self.binds.len()
            ));
        };
        self.used += 1;
        match op {
            Operator::Word(word) => {
                self.cx.catalog.note_word(word);
                self.equal(column, bind, word)
            }
            Operator::Like => {
                self.cx.catalog.note_without_word("LIKE");
                self.like(column, bind)
            }
        }
    }

    fn join(&mut self, parts: Vec<Filter>, word: Word) -> Result<Filter, Refusal> {
        for _ in 1..parts.len() {
            self.cx.catalog.note_word(word);
        }
        words::junction(word, parts)
    }
}

impl Filters<'_, '_> {
    fn equal(&self, column: &str, bind: &Json, word: Word) -> Result<Filter, Refusal> {
        if bind.is_null() {
            return refuse(format!(
                "`{column} = ?` with a NULL bind compares nothing in SQL"
            ));
        }
        let node = json!({
            "table": self.cx.table.name, "col": column, "op": "eq", "value": bind,
        });
        equal_leaf(self.cx, &node, word)
    }

    fn like(&mut self, column: &str, bind: &Json) -> Result<Filter, Refusal> {
        let (lane, field) = laned(self.cx, column)?;
        if lane.kind != LaneKind::Text {
            return refuse(format!("LIKE needs a text column, `{column}` is not one"));
        }
        let Some(pattern) = bind.as_str() else {
            return refuse(format!("LIKE on `{column}` needs a string bind"));
        };
        let Some(pattern) = Pattern::like(pattern) else {
            return refuse("a LIKE pattern must not end with an escape character");
        };
        let Some(codes) =
            super::Binder::codes_matching(self.cx.catalog, self.cx.id, field.col, &pattern)
        else {
            return refuse(format!("LIKE on `{column}`: its strings cannot be listed"));
        };
        self.cx.catalog.note_like(codes.len());
        if codes.is_empty() {
            return Ok(matches_none(super::Binder::live(
                self.cx.catalog,
                self.cx.id,
            )));
        }
        Ok(Filter::in_u32(field.col, codes))
    }
}

struct Names(Vec<String>);

impl Sink for Names {
    type Out = ();

    fn comparison(&mut self, column: &str, _op: Operator) -> Result<(), Refusal> {
        if !self.0.iter().any(|seen| seen == column) {
            self.0.push(column.to_owned());
        }
        Ok(())
    }

    fn join(&mut self, _parts: Vec<()>, _word: Word) -> Result<(), Refusal> {
        Ok(())
    }
}

/// The distinct columns a fragment names, in order of first appearance, read by the
/// grammar the reader uses.
#[doc(hidden)]
pub fn columns_of(table: &str, sql: &str) -> Result<Vec<String>, String> {
    let message = |r: Refusal| match r {
        Refusal::BoundSql(message) => message,
        other => format!("{other:?}"),
    };
    let mut reader = Reader {
        table,
        tokens: tokens(sql).map_err(message)?,
        at: 0,
        sink: Names(Vec::new()),
    };
    reader.whole().map_err(message)?;
    Ok(reader.sink.0)
}

/// The filter of a `{"op": "bound_sql", "sql", "binds"}` node.
pub(crate) fn bound_sql(cx: &Ctx<'_>, n: &Json) -> Result<Filter, Refusal> {
    let sql = str_of(n, "sql")?;
    let binds = n
        .get("binds")
        .and_then(Json::as_array)
        .ok_or_else(|| super::malformed("bound_sql without binds"))?;
    let mut reader = Reader {
        table: &cx.table.name,
        tokens: tokens(sql)?,
        at: 0,
        sink: Filters { cx, binds, used: 0 },
    };
    let filter = reader.whole()?;
    if reader.sink.used != binds.len() {
        return refuse(format!(
            "{} binds given for {} `?`",
            binds.len(),
            reader.sink.used
        ));
    }
    let mut stats = cx.catalog.like_stats.get();
    stats.fragments += 1;
    cx.catalog.like_stats.set(stats);
    Ok(filter)
}
