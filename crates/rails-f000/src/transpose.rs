// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Classification and lowering of Rails scopes onto the F000 polyfill lens.
//!
//! A scope is transposed only when every part of it is a population fold the
//! lens already has a word for: `where`/`where.not` over required boolean or
//! integer columns of the SAME OGAR class, ended by `count` or `exists?`.
//! Anything else is a named refusal. No word is minted, no evaluator is
//! added: the output is an ordered list of DT64 [`Effect`]s over polyfill
//! positions, selected by their R2IL names.

use std::collections::BTreeMap;
use std::sync::Arc;

use lance_graph_dt64::{AlgebraRef, Arg, Effect, MethodBinding, MethodId, SigRev, ValueDemand};
use lance_graph_r2il::polyfill::{POLYFILL_LENS, position_of};
use lib_ruby_parser::{Node, Parser, ParserOptions};
use serde::Serialize;

use crate::ogar_basis::{FieldMeta, OgarBasis};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Terminal {
    Count,
    Exists,
}

impl Terminal {
    /// The R2IL name of the polyfill word that ends the plan.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Terminal::Count => "PopCount",
            Terminal::Exists => "ANY",
        }
    }
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Terminal::Count => "count",
            Terminal::Exists => "exists",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RefuseReason {
    /// Not an OGAR attribute or association of THIS class.
    UnknownField { class: String, field: String },
    /// The class's table is not in the configured migrations, so a missing
    /// field proves nothing. The verdict is `Residual`, not `Refuse`.
    SchemaNotHarvested { table: String },
    /// The implicit foreign key of an association scope is NULL-able. A `where`
    /// comparison on a NULL-able column is ANDed with its presence instead.
    NullableComparison { field: String },
    /// A string/text literal needs a CodeTable witness (#1982).
    CodeSpaceRequired { field: String, type_name: String },
    UnsupportedType { field: String, type_name: String },
    /// Negative, or larger than `u32::MAX`, or of another kind than the lane.
    LiteralOutOfLane { field: String, literal: String },
    /// A method call, variable or lambda parameter where a literal is needed.
    NonLiteral { what: String },
    DefaultScopeFilters { what: String },
    /// A callback is not a population fold.
    EffectBoundary { phase: String, why: String },
    Join { what: String },
    /// `where.not` with several conditions: NAND since Rails 6.1, NOR before.
    NotSemantics { what: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Verdict {
    Alias,
    Compose,
    /// Reserved: no learned mapping exists in this experiment.
    Learned,
    Residual(String),
    Refuse(RefuseReason),
    Defer(String),
}

impl Verdict {
    /// Whether the verdict admits an executable plan.
    #[must_use]
    pub fn admits(&self) -> bool {
        matches!(self, Verdict::Alias | Verdict::Compose)
    }
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::Alias => "Alias",
            Verdict::Compose => "Compose",
            Verdict::Learned => "Learned",
            Verdict::Residual(_) => "Residual",
            Verdict::Refuse(_) => "Refuse",
            Verdict::Defer(_) => "Defer",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Transposed {
    pub class: String,
    pub method: String,
    pub provenance: String,
    pub ruby: String,
    /// `(basis position, predicate_iri, literal, negated)` in source order.
    pub predicates: Vec<(usize, String, u32, bool)>,
    /// `(basis position, predicate_iri, negated)` of each `where(x: nil)`
    /// (`negated`: `where.not(x: nil)`) on a NULL-able column, read from the
    /// column's presence plane (EXO-294 decision B, EXO-296).
    pub null_tests: Vec<(usize, String, bool)>,
    pub terminal: Terminal,
    pub effects: Vec<Effect>,
    pub reads: Vec<u8>,
    pub verdict: Verdict,
    pub evidence: Vec<String>,
    /// The reason behind a `Refuse` or a `Defer`.
    pub reason: Option<RefuseReason>,
    /// The basis position the plan leaves its answer in.
    pub result_field: Option<u8>,
    /// The basis position of the selection mask.
    pub mask_field: Option<u8>,
    /// R2IL names of the words the plan uses, in order.
    pub words: Vec<String>,
}

impl Transposed {
    /// The DT64 binding of an admitted plan; `None` for any other verdict.
    #[must_use]
    pub fn binding(&self, id: u32) -> Option<MethodBinding> {
        if !self.verdict.admits() {
            return None;
        }
        let reads = lance_graph_contract::class_view::WideFieldMask::from_positions(&self.reads);
        Some(MethodBinding {
            id: MethodId(id),
            sig_rev: SigRev(1),
            name: self.method.clone(),
            on: reads.clone(),
            when: None,
            reads,
            effects: self.effects.clone(),
            population: lance_graph_contract::set_rules::Population::Distinct,
            truth: lance_graph_contract::set_rules::Truth::TwoValued,
            demand: ValueDemand::Values,
        })
    }

    fn refused(class: &str, method: &str, provenance: &str, ruby: &str, terminal: Terminal, reason: RefuseReason) -> Self {
        let verdict = match &reason {
            RefuseReason::CodeSpaceRequired { field, type_name } => Verdict::Defer(format!(
                "{field} is {type_name}: needs a CodeTable witness (#1982) before a literal can be a lane code"
            )),
            RefuseReason::SchemaNotHarvested { table } => {
                Verdict::Residual(format!("UNKNOWN: table {table} not in the configured migrations"))
            }
            r => Verdict::Refuse(r.clone()),
        };
        Self {
            class: class.to_string(),
            method: method.to_string(),
            provenance: provenance.to_string(),
            ruby: ruby.to_string(),
            predicates: vec![],
            null_tests: vec![],
            terminal,
            effects: vec![],
            reads: vec![],
            verdict,
            evidence: vec![],
            reason: Some(reason),
            result_field: None,
            mask_field: None,
            words: vec![],
        }
    }
}

// ------------------------------------------------------------------ catalog

/// One `scope` / `default_scope` declaration with its body parsed.
#[derive(Debug, Clone)]
pub struct ScopeBody {
    pub class: String,
    pub name: String,
    pub params: usize,
    pub body: Option<Node>,
    pub src: Arc<str>,
    pub ruby: String,
    pub provenance: String,
    /// Byte offset of the declaration in its file (ruff's anchor).
    pub anchor: usize,
    pub default: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ScopeCatalog {
    pub scopes: Vec<ScopeBody>,
}

#[must_use]
pub fn parse_ruby(src: &str) -> Option<Node> {
    Parser::new(src.as_bytes().to_vec(), ParserOptions::default()).do_parse().ast.map(|b| *b)
}

fn sym_or_str(n: &Node) -> Option<String> {
    match n {
        Node::Sym(s) => Some(s.name.to_string_lossy()),
        Node::Str(s) => Some(s.value.to_string_lossy()),
        _ => None,
    }
}

fn text(src: &str, n: &Node) -> String {
    let l = n.expression();
    src.get(l.begin..l.end).unwrap_or("").to_string()
}

fn line_at(src: &str, byte: usize) -> usize {
    src[..byte.min(src.len())].matches('\n').count() + 1
}

fn statements(body: &Node) -> Vec<&Node> {
    match body {
        Node::Begin(b) => b.statements.iter().collect(),
        Node::KwBegin(b) => b.statements.iter().collect(),
        other => vec![other],
    }
}

/// `-> { … }`, `lambda { … }`, `proc { … }`: the parameter count and body.
fn lambda_parts(n: &Node) -> Option<(usize, Option<Node>)> {
    match n {
        Node::Block(b) => {
            let is_lambda = match &*b.call {
                Node::Lambda(_) => true,
                Node::Send(s) => s.recv.is_none() && matches!(s.method_name.as_str(), "lambda" | "proc"),
                _ => false,
            };
            if !is_lambda {
                return None;
            }
            let params = match b.args.as_deref() {
                Some(Node::Args(a)) => a.args.len(),
                Some(_) => 1,
                None => 0,
            };
            Some((params, b.body.as_deref().cloned()))
        }
        Node::Numblock(b) => Some((usize::from(b.numargs), Some((*b.body).clone()))),
        Node::Lambda(_) => Some((0, None)),
        _ => None,
    }
}

impl ScopeCatalog {
    /// Scope declarations in a class body, in source order. `rel` is the file
    /// name used in provenance.
    pub fn collect_body(&mut self, class: &str, body: &Node, src: &Arc<str>, rel: &str) {
        for stmt in statements(body) {
            match stmt {
                Node::Send(s) if s.recv.is_none() && matches!(s.method_name.as_str(), "scope" | "default_scope") => {
                    let default = s.method_name == "default_scope";
                    let (name, rest) = if default { (String::new(), &s.args[..]) } else {
                        match s.args.first().and_then(sym_or_str) {
                            Some(n) => (n, &s.args[1..]),
                            None => continue,
                        }
                    };
                    let parts = rest.iter().find_map(lambda_parts);
                    let (params, body) = parts.unwrap_or((0, None));
                    self.scopes.push(ScopeBody {
                        class: class.to_string(),
                        name,
                        params,
                        body,
                        src: src.clone(),
                        ruby: text(src, stmt),
                        provenance: format!("{rel}:{}", line_at(src, stmt.expression().begin)),
                        anchor: stmt.expression().begin,
                        default,
                    });
                }
                Node::Block(b) => {
                    let Node::Send(s) = &*b.call else { continue };
                    if s.recv.is_some() {
                        continue;
                    }
                    match s.method_name.as_str() {
                        "scope" | "default_scope" => {
                            let default = s.method_name == "default_scope";
                            let name = if default { Some(String::new()) } else { s.args.first().and_then(sym_or_str) };
                            let Some(name) = name else { continue };
                            let params = match b.args.as_deref() {
                                Some(Node::Args(a)) => a.args.len(),
                                Some(_) => 1,
                                None => 0,
                            };
                            self.scopes.push(ScopeBody {
                                class: class.to_string(),
                                name,
                                params,
                                body: b.body.as_deref().cloned(),
                                src: src.clone(),
                                ruby: text(src, stmt),
                                provenance: format!("{rel}:{}", line_at(src, stmt.expression().begin)),
                        anchor: stmt.expression().begin,
                                default,
                            });
                        }
                        "included" => {
                            if let Some(inner) = b.body.as_deref() {
                                self.collect_body(class, inner, src, rel);
                            }
                        }
                        _ => {}
                    }
                }
                Node::Numblock(b) => {
                    let Node::Send(s) = &*b.call else { continue };
                    if s.recv.is_none() && s.method_name == "scope"
                        && let Some(name) = s.args.first().and_then(sym_or_str) {
                            self.scopes.push(ScopeBody {
                                class: class.to_string(),
                                name,
                                params: usize::from(b.numargs),
                                body: Some((*b.body).clone()),
                                src: src.clone(),
                                ruby: text(src, stmt),
                                provenance: format!("{rel}:{}", line_at(src, stmt.expression().begin)),
                        anchor: stmt.expression().begin,
                                default: false,
                            });
                        }
                }
                _ => {}
            }
        }
    }

    /// A catalog from the text of one class body (a test and a bench aid).
    #[must_use]
    pub fn from_class_source(class: &str, src: &str) -> Self {
        let mut c = Self::default();
        if let Some(ast) = parse_ruby(src) {
            let arc: Arc<str> = Arc::from(src);
            c.collect_body(class, &ast, &arc, &format!("{class}.rb"));
        }
        c
    }

    /// Every scope declared in a class or module body of a harvest.
    #[must_use]
    pub fn from_harvest(h: &rails_compiler::harvest::Harvest) -> Self {
        let mut c = Self::default();
        let mut cache: BTreeMap<std::path::PathBuf, Arc<str>> = BTreeMap::new();
        for (name, units) in &h.units {
            for u in units {
                let Some(file) = h.file(&u.file) else { continue };
                let src = cache.entry(u.file.clone()).or_insert_with(|| Arc::from(file.src.as_str())).clone();
                if let Some(b) = &u.body {
                    c.collect_body(name, b, &src, &file.rel);
                }
            }
        }
        c
    }

    #[must_use]
    pub fn scope(&self, class: &str, name: &str) -> Option<&ScopeBody> {
        self.scopes.iter().find(|s| !s.default && s.class == class && s.name == name)
    }

    #[must_use]
    pub fn default_scope(&self, class: &str) -> Option<&ScopeBody> {
        self.scopes.iter().find(|s| s.default && s.class == class)
    }
}

// ------------------------------------------------------------------ chains

#[derive(Debug, Clone)]
enum Link {
    Where { pairs: Vec<(String, Node)>, negated: bool },
    Scope(String),
    Order(String),
}

fn pairs_of(args: &[Node], src: &str) -> Result<Vec<(String, Node)>, RefuseReason> {
    let mut out = Vec::new();
    let Some(first) = args.first() else {
        return Err(RefuseReason::NonLiteral { what: "where without conditions".into() });
    };
    let list = match first {
        Node::Kwargs(k) => &k.pairs,
        Node::Hash(h) => &h.pairs,
        Node::Str(_) | Node::Dstr(_) => {
            return Err(RefuseReason::NonLiteral { what: format!("SQL fragment {}", text(src, first)) });
        }
        other => return Err(RefuseReason::NonLiteral { what: format!("where argument {}", text(src, other)) }),
    };
    for p in list {
        let Node::Pair(p) = p else {
            return Err(RefuseReason::NonLiteral { what: format!("splat in where {}", text(src, p)) });
        };
        match &*p.key {
            Node::Sym(_) | Node::Str(_) => {
                if let Node::Hash(_) = &*p.value {
                    return Err(RefuseReason::Join { what: format!("nested table condition {}", text(src, p.value.as_ref())) });
                }
                out.push((sym_or_str(&p.key).unwrap_or_default(), (*p.value).clone()));
            }
            other => return Err(RefuseReason::NonLiteral { what: format!("where key {}", text(src, other)) }),
        }
    }
    Ok(out)
}

fn chain(n: &Node, src: &str, out: &mut Vec<Link>) -> Result<(), RefuseReason> {
    match n {
        Node::Begin(b) if b.statements.len() == 1 => chain(&b.statements[0], src, out),
        Node::Send(s) => {
            let m = s.method_name.as_str();
            match m {
                "not" => {
                    let Some(r) = s.recv.as_deref() else {
                        return Err(RefuseReason::NonLiteral { what: text(src, n) });
                    };
                    match r {
                        Node::Send(w) if w.method_name == "where" && w.args.is_empty() => {
                            if let Some(rr) = w.recv.as_deref() {
                                chain(rr, src, out)?;
                            }
                            let pairs = pairs_of(&s.args, src)?;
                            if pairs.len() > 1 {
                                return Err(RefuseReason::NotSemantics {
                                    what: format!(
                                        "{}: where.not over several conditions is NOT(a AND b) since Rails 6.1 and NOT a AND NOT b before",
                                        text(src, n)
                                    ),
                                });
                            }
                            out.push(Link::Where { pairs, negated: true });
                            Ok(())
                        }
                        _ => Err(RefuseReason::NonLiteral { what: text(src, n) }),
                    }
                }
                "where" => {
                    if s.args.is_empty() {
                        return Err(RefuseReason::NonLiteral { what: "where proxy without not".into() });
                    }
                    if let Some(r) = s.recv.as_deref() {
                        chain(r, src, out)?;
                    }
                    out.push(Link::Where { pairs: pairs_of(&s.args, src)?, negated: false });
                    Ok(())
                }
                "all" if s.args.is_empty() => {
                    if let Some(r) = s.recv.as_deref() {
                        chain(r, src, out)?;
                    }
                    Ok(())
                }
                "order" | "reorder" => {
                    if let Some(r) = s.recv.as_deref() {
                        chain(r, src, out)?;
                    }
                    out.push(Link::Order(text(src, n)));
                    Ok(())
                }
                "joins" | "includes" | "left_joins" | "left_outer_joins" | "eager_load" | "preload" | "references" => {
                    Err(RefuseReason::Join { what: text(src, n) })
                }
                name if s.args.is_empty() => {
                    if let Some(r) = s.recv.as_deref() {
                        chain(r, src, out)?;
                    }
                    out.push(Link::Scope(name.to_string()));
                    Ok(())
                }
                _ => Err(RefuseReason::NonLiteral { what: text(src, n) }),
            }
        }
        other => Err(RefuseReason::NonLiteral { what: text(src, other) }),
    }
}

// ------------------------------------------------------------------ transposition

/// What a scope is invoked on.
#[derive(Debug, Clone)]
pub enum Receiver<'a> {
    /// `Class.scope`.
    Class(&'a str),
    /// `owner.assoc.scope` with the owner's primary key as a literal:
    /// the implicit foreign-key predicate is added.
    Assoc { owner: &'a str, assoc: &'a str, owner_id: u32 },
}

#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub receiver: Receiver<'a>,
    /// `None` is `X.count` / `X.exists?` with no scope.
    pub scope: Option<&'a str>,
    pub terminal: Terminal,
}

struct Acc {
    predicates: Vec<(usize, String, u32, bool)>,
    null_tests: Vec<(usize, String, bool)>,
    evidence: Vec<String>,
}

fn is_integer_type(t: &str) -> bool {
    matches!(t, "integer" | "bigint" | "references")
}

fn literal(
    meta: &FieldMeta,
    key: &str,
    value: &Node,
    src: &str,
    acc: &mut Acc,
) -> Result<u32, RefuseReason> {
    let (type_name, name) = match meta {
        FieldMeta::Column { type_name, name, .. } => (type_name.as_str(), name.as_str()),
        FieldMeta::AssocFk { column, .. } => ("fk", column.as_str()),
        FieldMeta::Output { method } => {
            return Err(RefuseReason::UnsupportedType { field: method.clone(), type_name: "output".into() });
        }
    };
    let gate = !cfg!(feature = "disable-type-gate");
    if gate {
        match type_name {
            "boolean" | "fk" => {}
            t if is_integer_type(t) => {}
            "string" | "text" => {
                return Err(RefuseReason::CodeSpaceRequired { field: name.to_string(), type_name: type_name.to_string() });
            }
            other => return Err(RefuseReason::UnsupportedType { field: name.to_string(), type_name: other.to_string() }),
        }
    } else if matches!(type_name, "string" | "text") {
        // The disabled gate: a string literal becomes a hashed "code".
        let s = match value {
            Node::Str(s) => s.value.to_string_lossy(),
            other => text(src, other),
        };
        let mut h: u32 = 0x811c_9dc5;
        for b in s.bytes() {
            h ^= u32::from(b);
            h = h.wrapping_mul(0x0100_0193);
        }
        return Ok(h);
    }
    let shown = text(src, value);
    let out_of_lane = || RefuseReason::LiteralOutOfLane { field: name.to_string(), literal: shown.clone() };
    match (type_name, value) {
        ("boolean", Node::True(_)) => Ok(1),
        ("boolean", Node::False(_)) => Ok(0),
        (_, Node::Nil(_)) => Err(out_of_lane()),
        ("boolean", Node::Int(_) | Node::Str(_) | Node::Sym(_) | Node::Float(_)) => Err(out_of_lane()),
        (_, Node::Int(i)) => {
            let n: i128 = i.value.replace('_', "").parse().map_err(|_| out_of_lane())?;
            let v = u32::try_from(n).map_err(|_| out_of_lane())?;
            if is_integer_type(type_name) || type_name == "fk" {
                acc.evidence.push(format!(
                    "{name} = {v}: an integer/FK value is an identity key, not a dictionary code"
                ));
            }
            Ok(v)
        }
        ("fk" | "integer" | "bigint" | "references", Node::True(_) | Node::False(_) | Node::Str(_) | Node::Sym(_) | Node::Float(_)) => {
            Err(out_of_lane())
        }
        _ => Err(RefuseReason::NonLiteral { what: format!("{key}: {shown}") }),
    }
}

/// A field the class does not have: a refusal when the class's table is
/// known, a residual when the table is outside the configured migrations.
fn unknown_field(basis: &OgarBasis, class: &str, field: String) -> RefuseReason {
    match basis.unharvested_table(class) {
        Some(table) => RefuseReason::SchemaNotHarvested { table: table.to_string() },
        None => RefuseReason::UnknownField { class: class.to_string(), field },
    }
}

#[allow(clippy::too_many_arguments)]
fn collect(
    basis: &OgarBasis,
    cat: &ScopeCatalog,
    class: &str,
    body: &Node,
    src: &str,
    depth: usize,
    acc: &mut Acc,
) -> Result<(), RefuseReason> {
    if depth > 8 {
        return Err(RefuseReason::NonLiteral { what: "scope chain deeper than 8".into() });
    }
    let mut links = Vec::new();
    chain(body, src, &mut links)?;
    for link in links {
        match link {
            Link::Order(t) => acc.evidence.push(format!("{t} is order-only; COUNT/EXISTS are order-insensitive")),
            Link::Scope(name) => {
                let Some(sb) = cat.scope(class, &name) else {
                    return Err(RefuseReason::NonLiteral { what: format!("call to `{name}`, not a scope of {class}") });
                };
                if sb.params > 0 {
                    return Err(RefuseReason::NonLiteral { what: format!("scope {name} takes {} lambda parameter(s)", sb.params) });
                }
                let Some(inner) = &sb.body else {
                    return Err(RefuseReason::NonLiteral { what: format!("scope {name} has no body") });
                };
                acc.evidence.push(format!("scope {name} expanded from {}", sb.provenance));
                collect(basis, cat, class, inner, &sb.src, depth + 1, acc)?;
            }
            Link::Where { pairs, negated } => {
                for (key, value) in pairs {
                    let Some((pos, meta)) = basis.resolve(class, &key) else {
                        return Err(unknown_field(basis, class, key));
                    };
                    let required = match meta {
                        FieldMeta::Column { required, .. } | FieldMeta::AssocFk { required, .. } => *required,
                        FieldMeta::Output { .. } => false,
                    };
                    if !required && matches!(value, Node::Nil(_)) {
                        let iri = basis.field(class, pos).map(|f| f.predicate_iri.clone()).unwrap_or_default();
                        let sql = if negated { "IS NOT NULL" } else { "IS NULL" };
                        acc.evidence.push(format!(
                            "{class}#{key} {sql} -> presence plane of position {pos}, {iri}: two-valued, absent = presence 0 (decision B)"
                        ));
                        acc.null_tests.push((pos, iri, negated));
                        continue;
                    }
                    let lit = literal(meta, &key, &value, src, acc)?;
                    let iri = basis.field(class, pos).map(|f| f.predicate_iri.clone()).unwrap_or_default();
                    acc.evidence.push(format!("{class}#{key} -> position {pos}, {iri}"));
                    // SQL `=` / `!=` drop a NULL row; under decision B that is `AND present`. The
                    // disable run keeps the unguarded comparison, which reads a NULL as a stored 0.
                    if !required && !cfg!(feature = "disable-null-gate") && !acc.null_tests.iter().any(|t| t.0 == pos && t.2) {
                        acc.evidence.push(format!(
                            "{class}#{key} is NULL-able: SQL {} drops NULL rows, so the comparison is ANDed with its presence plane",
                            if negated { "`!=`" } else { "`=`" }
                        ));
                        acc.null_tests.push((pos, iri.clone(), true));
                    }
                    acc.predicates.push((pos, iri, lit, negated));
                }
            }
        }
    }
    Ok(())
}

fn default_scope_evidence(cat: &ScopeCatalog, class: &str, acc: &mut Acc) -> Result<(), RefuseReason> {
    let Some(d) = cat.default_scope(class) else { return Ok(()) };
    let Some(body) = &d.body else {
        return Err(RefuseReason::DefaultScopeFilters { what: format!("default_scope of {class} has no analysable body ({})", d.provenance) });
    };
    let mut links = Vec::new();
    match chain(body, &d.src, &mut links) {
        Ok(()) if links.iter().all(|l| matches!(l, Link::Order(_))) => {
            let t = links.iter().filter_map(|l| if let Link::Order(t) = l { Some(t.as_str()) } else { None }).collect::<Vec<_>>().join(", ");
            acc.evidence.push(format!("default_scope {t} is order-only; COUNT/EXISTS are order-insensitive"));
            Ok(())
        }
        _ => Err(RefuseReason::DefaultScopeFilters {
            what: format!("default_scope of {class} is not order-only: {} ({})", d.ruby, d.provenance),
        }),
    }
}

fn word_effect(target: u8, name: &str, args: Vec<Arg>) -> Effect {
    Effect {
        target,
        algebra: AlgebraRef {
            lens: POLYFILL_LENS,
            word: position_of(name).unwrap_or(u8::MAX),
        },
        args,
    }
}

/// Transpose one scope invocation. On success the basis has grown by the
/// plan's output fields; on a refusal it is unchanged.
pub fn transpose(basis: &mut OgarBasis, cat: &ScopeCatalog, req: &Request<'_>) -> Transposed {
    let (class, method, via) = match &req.receiver {
        Receiver::Class(c) => ((*c).to_string(), req.scope.unwrap_or("all").to_string(), None),
        Receiver::Assoc { owner, assoc, owner_id } => {
            let Some(t) = basis.association_target(owner, assoc) else {
                return Transposed::refused(
                    owner, assoc, "", "", req.terminal,
                    RefuseReason::UnknownField { class: (*owner).to_string(), field: (*assoc).to_string() },
                );
            };
            (t, format!("{owner}#{assoc}.{}@{owner_id}", req.scope.unwrap_or("all")), Some((*owner, *assoc, *owner_id)))
        }
    };
    let scope_body = req.scope.and_then(|s| cat.scope(&class, s));
    let provenance = scope_body.map_or_else(String::new, |s| s.provenance.clone());
    let ruby = {
        let recv = match &req.receiver {
            Receiver::Class(c) => (*c).to_string(),
            Receiver::Assoc { assoc, .. } => (*assoc).to_string(),
        };
        let sc = req.scope.map_or(String::new(), |s| format!(".{s}"));
        let t = match req.terminal {
            Terminal::Count => "count",
            Terminal::Exists => "exists?",
        };
        format!("{recv}{sc}.{t}")
    };
    let refuse = |r| Transposed::refused(&class, &method, &provenance, &ruby, req.terminal, r);

    if basis.class(&class).is_none() {
        return refuse(RefuseReason::UnknownField { class: class.clone(), field: "<class not in the OGAR basis>".into() });
    }
    let mut acc = Acc { predicates: vec![], null_tests: vec![], evidence: vec![] };
    if let Err(r) = default_scope_evidence(cat, &class, &mut acc) {
        return refuse(r);
    }
    if let Some((owner, assoc, owner_id)) = via {
        let Some((col, pos)) = basis.inverse_fk(owner, assoc) else {
            return refuse(unknown_field(basis, &class, format!("<foreign key of {owner}.{assoc}>")));
        };
        let required = matches!(
            basis.class(&class).map(|c| &c.meta[pos]),
            Some(FieldMeta::AssocFk { required: true, .. } | FieldMeta::Column { required: true, .. })
        );
        if !required && !cfg!(feature = "disable-null-gate") {
            return refuse(RefuseReason::NullableComparison { field: col });
        }
        let iri = basis.field(&class, pos).map(|f| f.predicate_iri.clone()).unwrap_or_default();
        acc.evidence.push(format!(
            "{owner}.{assoc} adds the implicit foreign key {class}#{col} = {owner_id} (position {pos}, {iri}); an FK id is an identity key, not a dictionary code"
        ));
        acc.evidence.push("fk required: the column's NOT NULL from the compiler's schema (absent column = nullable); belongs_to optional: is a Rails presence rule, not the column; OGAR's Association does not carry the column's NOT NULL".to_string());
        acc.predicates.push((pos, iri, owner_id, false));
    }
    if let Some(s) = req.scope {
        let Some(sb) = scope_body else {
            return Transposed {
                verdict: Verdict::Residual(format!("scope {s} is not declared in the body of {class}")),
                ..Transposed::refused(&class, &method, &provenance, &ruby, req.terminal, RefuseReason::NonLiteral { what: format!("scope {s} not found") })
            };
        };
        if sb.params > 0 {
            return refuse(RefuseReason::NonLiteral { what: format!("scope {s} takes {} lambda parameter(s)", sb.params) });
        }
        let Some(b) = &sb.body else {
            return Transposed {
                verdict: Verdict::Residual(format!("scope {s} has no analysable body ({})", sb.provenance)),
                ..Transposed::refused(&class, &method, &provenance, &ruby, req.terminal, RefuseReason::NonLiteral { what: format!("scope {s} has no body") })
            };
        };
        if let Err(r) = collect(basis, cat, &class, b, &sb.src, 0, &mut acc) {
            return refuse(r);
        }
    }

    let n = acc.predicates.len() + acc.null_tests.len();
    let terminal_word = req.terminal.word();
    let mut words: Vec<String> = Vec::new();
    let mut effects: Vec<Effect> = Vec::new();
    let mut reads: Vec<u8> = Vec::new();
    let (mask, result);
    if n == 0 {
        // X.count / X.exists? with no predicate: the terminal alone.
        let Some((_, r)) = basis.append_output(&class, &method, req.terminal.name()) else {
            return refuse(RefuseReason::UnsupportedType { field: method.clone(), type_name: "basis full".into() });
        };
        mask = None;
        result = r;
        let id_pos = basis.resolve(&class, "id").map_or(0, |(p, _)| p);
        let lane = u8::try_from(id_pos).unwrap_or(0);
        reads.push(lane);
        effects.push(word_effect(result, terminal_word, vec![Arg::Field(lane)]));
        words.push(terminal_word.to_string());
        acc.evidence.push(
            "no predicate: the terminal word alone. Its operand must be a mask plane; FieldLanes binds lanes only, so prepare_native refuses it (WrongOperandKind)"
                .to_string(),
        );
    } else {
        let Some((m, r)) = basis.append_output(&class, &method, req.terminal.name()) else {
            return refuse(RefuseReason::UnsupportedType { field: method.clone(), type_name: "basis full".into() });
        };
        mask = Some(m);
        result = r;
        let tmp = if n > 1 {
            // A second term needs its own scratch mask; reusing the
            // accumulator would overwrite the first term's result.
            let Some((t, _)) = basis.append_output(&class, &format!("{method}:tmp"), "mask") else {
                return refuse(RefuseReason::UnsupportedType { field: method.clone(), type_name: "basis full".into() });
            };
            Some(t)
        } else {
            None
        };
        let terms = acc
            .predicates
            .iter()
            .map(|(pos, _, lit, negated)| (*pos, Some(*lit), *negated))
            .chain(acc.null_tests.iter().map(|(pos, _, negated)| (*pos, None, *negated)));
        for (i, (pos, lit, negated)) in terms.enumerate() {
            let col = u8::try_from(pos).unwrap_or(0);
            if !reads.contains(&col) {
                reads.push(col);
            }
            let target = if i == 0 { m } else { tmp.unwrap_or(m) };
            let mut push = |t: u8, w: &str, args: Vec<Arg>| {
                effects.push(word_effect(t, w, args));
                words.push(w.into());
            };
            let operand = match (lit, negated) {
                (Some(lit), _) => {
                    push(target, "IntEqual", vec![Arg::Field(col), Arg::Const(i64::from(lit))]);
                    if negated {
                        push(target, "IntNot", vec![Arg::Field(target)]);
                    }
                    Arg::Field(target)
                }
                // IS NULL: presence 0. The disable run reads the value lane as if a NULL were a stored 0.
                (None, false) if cfg!(feature = "disable-presence-operand") => {
                    push(target, "IntEqual", vec![Arg::Field(col), Arg::Const(0)]);
                    Arg::Field(target)
                }
                (None, false) => {
                    push(target, "IntNot", vec![Arg::Present(col)]);
                    Arg::Field(target)
                }
                // IS NOT NULL: the presence plane itself; alone, AND it with itself to land in the mask.
                (None, true) if i == 0 => {
                    push(m, "IntAnd", vec![Arg::Present(col), Arg::Present(col)]);
                    Arg::Field(m)
                }
                (None, true) => Arg::Present(col),
            };
            if i > 0 {
                push(m, "IntAnd", vec![Arg::Field(m), operand]);
            }
        }
        effects.push(word_effect(r, terminal_word, vec![Arg::Field(m)]));
        words.push(terminal_word.to_string());
        reads.sort_unstable();
    }
    let verdict = if effects.len() == 1 { Verdict::Alias } else { Verdict::Compose };
    Transposed {
        class,
        method,
        provenance,
        ruby,
        predicates: acc.predicates,
        null_tests: acc.null_tests,
        terminal: req.terminal,
        effects,
        reads,
        verdict,
        evidence: acc.evidence,
        reason: None,
        result_field: Some(result),
        mask_field: mask,
        words,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::{Association, AssociationKind, Attribute, Class};

    fn attr(name: &str, ty: &str, required: bool) -> Attribute {
        let mut a = Attribute::new(name);
        a.type_name = Some(ty.to_string());
        a.options.required = Some(required);
        a
    }

    fn class(name: &str, attrs: Vec<Attribute>, assocs: Vec<Association>) -> Class {
        let mut c = Class::new(name);
        c.attributes = attrs;
        c.associations = assocs;
        c
    }

    fn not_null(entries: &[(&str, &str)]) -> BTreeMap<String, BTreeMap<String, bool>> {
        let mut m: BTreeMap<String, BTreeMap<String, bool>> = BTreeMap::new();
        for (class, column) in entries {
            m.entry((*class).to_string()).or_default().insert((*column).to_string(), true);
        }
        m
    }

    fn basis() -> OgarBasis {
        basis_with(not_null(&[("List::Item", "list_id")]))
    }

    fn basis_with(column_not_null: BTreeMap<String, BTreeMap<String, bool>>) -> OgarBasis {
        let mut state = class(
            "State",
            vec![
                attr("id", "integer", true),
                attr("name", "string", true),
                attr("active", "boolean", true),
                attr("default_create", "boolean", true),
                attr("note", "string", false),
                attr("flag", "boolean", false),
                attr("rank", "integer", true),
                attr("score", "decimal", true),
            ],
            vec![],
        );
        state.default_scope = Some("0..1".into());
        let mut parent = Association::new(AssociationKind::BelongsTo, "parent");
        parent.optional = Some(true);
        let cat = class(
            "Cat",
            vec![attr("id", "integer", true), attr("label", "string", true)],
            vec![Association::new(AssociationKind::BelongsTo, "state"), parent],
        );
        let mut items = Association::new(AssociationKind::HasMany, "items");
        items.inverse_of = Some("list".into());
        let list = class(
            "List",
            vec![attr("id", "integer", true)],
            vec![items],
        );
        let item = class(
            "List::Item",
            vec![attr("id", "integer", true), attr("checked", "boolean", true)],
            vec![Association::new(AssociationKind::BelongsTo, "list")],
        );
        OgarBasis::new(vec![
            ("t".into(), state, 0x7F10),
            ("t".into(), cat, 0x7F11),
            ("t".into(), list, 0x7F12),
            ("t".into(), item, 0x7F13),
        ])
        .with_column_not_null(column_not_null)
    }

    fn cat_fk_required(b: &OgarBasis, assoc: &str) -> bool {
        b.class("Cat")
            .unwrap()
            .meta
            .iter()
            .find_map(|m| match m {
                FieldMeta::AssocFk { assoc: a, required, .. } if a == assoc => Some(*required),
                _ => None,
            })
            .unwrap()
    }

    const STATE: &str = "default_scope { order(id: :asc) }\n\
        scope :active, -> { where(active: true) }\n\
        scope :inactive, -> { where.not(active: true) }\n\
        scope :create_active, -> { where(default_create: true).where(active: true) }\n\
        scope :mixed, -> { where(active: true).where.not(default_create: true) }\n\
        scope :nullable, -> { where(flag: true) }\n\
        scope :nullable_not, -> { where.not(flag: true) }\n\
        scope :named, -> { where(name: 'x') }\n\
        scope :negative, -> { where(rank: -1) }\n\
        scope :huge, -> { where(rank: 4294967296) }\n\
        scope :ranked, -> { where(rank: 7) }\n\
        scope :decimal, -> { where(score: 1) }\n\
        scope :unknown, -> { where(nope: 1) }\n\
        scope :param, ->(v) { where(active: v) }\n\
        scope :var, -> { where(active: some_call) }\n\
        scope :joined, -> { joins(:cat).where(active: true) }\n\
        scope :two_not, -> { where.not(active: true, default_create: true) }\n\
        scope :chained, -> { active.where(default_create: false) }\n\
        scope :sorted, -> { order(name: :asc) }\n\
        scope :sql, -> { where('active = 1') }\n\
        scope :nilly, -> { where(rank: nil) }\n";

    fn run(scope: &str, terminal: Terminal) -> Transposed {
        let mut b = basis();
        let cat = ScopeCatalog::from_class_source("State", STATE);
        transpose(&mut b, &cat, &Request { receiver: Receiver::Class("State"), scope: Some(scope), terminal })
    }

    /// With room for the result pair but not for the scratch mask, a
    /// two-term scope is refused. It used to reuse the accumulator as the
    /// scratch mask, so the second `IntEqual` overwrote the first term and
    /// the count ignored it.
    #[test]
    fn a_two_term_scope_without_room_for_its_scratch_mask_is_refused() {
        let mut b = basis();
        let mut i = 0;
        while b.class("State").unwrap().fields.len() + 2 <= 254 {
            b.append_output("State", &format!("filler{i}"), "count").expect("room");
            i += 1;
        }
        let len = b.class("State").unwrap().fields.len();
        assert!(len == 253 || len == 254, "{len}");
        let cat = ScopeCatalog::from_class_source("State", STATE);
        let req = Request { receiver: Receiver::Class("State"), scope: Some("create_active"), terminal: Terminal::Count };
        let t = transpose(&mut b, &cat, &req);
        assert_eq!(
            t.verdict,
            Verdict::Refuse(RefuseReason::UnsupportedType { field: "create_active".into(), type_name: "basis full".into() })
        );
    }

    #[test]
    fn one_predicate_and_a_terminal_compose_two_words() {
        let t = run("active", Terminal::Count);
        assert_eq!(t.verdict, Verdict::Compose);
        assert_eq!(t.words, ["IntEqual", "PopCount"]);
        assert_eq!(t.predicates.len(), 1);
        assert!(t.predicates[0].1.starts_with("ogar:t:State#active@boolean!"));
        assert_eq!(t.predicates[0].2, 1);
        assert!(t.evidence.iter().any(|e| e.contains("default_scope") && e.contains("order-only")));
        let e = run("active", Terminal::Exists);
        assert_eq!(e.words, ["IntEqual", "ANY"]);
    }

    #[test]
    fn where_not_wraps_intnot_and_chains_combine_with_intand() {
        let t = run("inactive", Terminal::Count);
        assert_eq!(t.words, ["IntEqual", "IntNot", "PopCount"]);
        assert!(t.predicates[0].3);
        let c = run("create_active", Terminal::Count);
        assert_eq!(c.words, ["IntEqual", "IntEqual", "IntAnd", "PopCount"]);
        assert_eq!(c.verdict, Verdict::Compose);
        let m = run("mixed", Terminal::Count);
        assert_eq!(m.words, ["IntEqual", "IntEqual", "IntNot", "IntAnd", "PopCount"]);
        let r = run("chained", Terminal::Count);
        assert_eq!(r.predicates.iter().map(|p| p.2).collect::<Vec<_>>(), [1, 0]);
    }

    #[cfg(not(feature = "disable-null-gate"))]
    #[test]
    fn a_nullable_column_compared_with_a_literal_is_anded_with_its_presence() {
        let eq = run("nullable", Terminal::Count);
        assert_eq!(eq.words, ["IntEqual", "IntAnd", "PopCount"], "{:?}", eq.verdict);
        let not = run("nullable_not", Terminal::Count);
        assert_eq!(not.words, ["IntEqual", "IntNot", "IntAnd", "PopCount"], "{:?}", not.verdict);
        for t in [&eq, &not] {
            let p = u8::try_from(t.predicates[0].0).unwrap();
            assert_eq!(t.null_tests, vec![(t.predicates[0].0, t.predicates[0].1.clone(), true)]);
            assert!(t.effects.iter().any(|e| e.args.contains(&Arg::Present(p))));
        }
    }

    #[cfg(not(feature = "disable-type-gate"))]
    #[test]
    fn string_columns_defer_and_other_types_refuse() {
        let t = run("named", Terminal::Count);
        assert!(matches!(t.verdict, Verdict::Defer(_)));
        assert_eq!(t.reason, Some(RefuseReason::CodeSpaceRequired { field: "name".into(), type_name: "string".into() }));
        let d = run("decimal", Terminal::Count);
        assert_eq!(d.verdict, Verdict::Refuse(RefuseReason::UnsupportedType { field: "score".into(), type_name: "decimal".into() }));
    }

    #[test]
    fn literals_must_fit_the_lane() {
        assert_eq!(run("ranked", Terminal::Count).predicates[0].2, 7);
        for s in ["negative", "huge", "nilly"] {
            let t = run(s, Terminal::Count);
            assert!(matches!(t.verdict, Verdict::Refuse(RefuseReason::LiteralOutOfLane { .. })), "{s}: {:?}", t.verdict);
        }
    }

    #[cfg(not(feature = "disable-field-resolution"))]
    #[test]
    fn an_unknown_field_is_refused_with_the_class() {
        let t = run("unknown", Terminal::Count);
        assert_eq!(t.verdict, Verdict::Refuse(RefuseReason::UnknownField { class: "State".into(), field: "nope".into() }));
    }

    #[cfg(not(feature = "disable-field-resolution"))]
    #[test]
    fn a_missing_field_of_an_unharvested_table_is_residual_not_refused() {
        let mut b = basis().with_unharvested(BTreeMap::from([("State".to_string(), "states".to_string())]));
        let cat = ScopeCatalog::from_class_source("State", STATE);
        let t = transpose(&mut b, &cat, &Request { receiver: Receiver::Class("State"), scope: Some("unknown"), terminal: Terminal::Count });
        assert_eq!(t.verdict, Verdict::Residual("UNKNOWN: table states not in the configured migrations".into()));
        assert_eq!(t.reason, Some(RefuseReason::SchemaNotHarvested { table: "states".into() }));
    }

    #[test]
    fn non_literals_joins_and_not_semantics_are_named() {
        for s in ["param", "var", "sql"] {
            assert!(matches!(run(s, Terminal::Count).verdict, Verdict::Refuse(RefuseReason::NonLiteral { .. })), "{s}");
        }
        assert!(matches!(run("joined", Terminal::Count).verdict, Verdict::Refuse(RefuseReason::Join { .. })));
        assert!(matches!(run("two_not", Terminal::Count).verdict, Verdict::Refuse(RefuseReason::NotSemantics { .. })));
    }

    #[test]
    fn an_order_only_scope_has_no_predicate_and_is_an_alias() {
        let t = run("sorted", Terminal::Count);
        assert_eq!(t.verdict, Verdict::Alias);
        assert_eq!(t.words, ["PopCount"]);
    }

    #[test]
    fn a_missing_scope_is_residual() {
        let t = run("ghost", Terminal::Count);
        assert!(matches!(t.verdict, Verdict::Residual(_)));
    }

    #[test]
    fn a_default_scope_with_a_where_refuses_and_an_order_one_passes() {
        let mut b = basis();
        let cat = ScopeCatalog::from_class_source(
            "State",
            "default_scope { where(active: true) }\nscope :ranked, -> { where(rank: 7) }\n",
        );
        let t = transpose(&mut b, &cat, &Request { receiver: Receiver::Class("State"), scope: Some("ranked"), terminal: Terminal::Count });
        assert!(matches!(t.verdict, Verdict::Refuse(RefuseReason::DefaultScopeFilters { .. })));
    }

    #[test]
    fn a_scope_through_has_many_adds_the_foreign_key_of_the_target() {
        let mut b = basis();
        let mut cat = ScopeCatalog::from_class_source("List::Item", "scope :incomplete, -> { where(checked: false) }\n");
        cat.scopes.extend(ScopeCatalog::from_class_source("List", "").scopes);
        let t = transpose(
            &mut b,
            &cat,
            &Request { receiver: Receiver::Assoc { owner: "List", assoc: "items", owner_id: 3 }, scope: Some("incomplete"), terminal: Terminal::Count },
        );
        assert_eq!(t.class, "List::Item");
        assert_eq!(t.verdict, Verdict::Compose, "{:?}", t.reason);
        assert_eq!(t.words, ["IntEqual", "IntEqual", "IntAnd", "PopCount"]);
        assert_eq!(t.predicates[0].2, 3);
        assert!(t.predicates[0].1.contains("#list->List@fk!"));
        assert!(t.evidence.iter().any(|e| e.contains("identity key")));
    }

    #[cfg(not(feature = "disable-null-gate"))]
    #[test]
    fn an_optional_belongs_to_compared_with_a_literal_reads_its_presence() {
        let mut b = basis();
        let cat = ScopeCatalog::from_class_source("Cat", "scope :under, -> { where(parent: 3) }\n");
        let t = transpose(&mut b, &cat, &Request { receiver: Receiver::Class("Cat"), scope: Some("under"), terminal: Terminal::Count });
        assert_eq!((t.verdict.clone(), t.words.clone()), (Verdict::Compose, vec!["IntEqual".into(), "IntAnd".into(), "PopCount".into()]));
        assert_eq!(t.null_tests.len(), 1);
    }

    /// `IS NULL` / `IS NOT NULL` on a NULL-able column read its presence plane.
    #[cfg(not(feature = "disable-presence-operand"))]
    #[test]
    fn a_nil_test_on_a_nullable_column_reads_its_presence_plane() {
        let mut b = basis();
        let cat = ScopeCatalog::from_class_source(
            "Cat",
            "scope :root, -> { where(parent: nil) }\nscope :child, -> { where.not(parent: nil) }\nscope :root_ranked, -> { where(parent: nil).where(id: 7) }\n",
        );
        let run = |b: &mut OgarBasis, s| transpose(b, &cat, &Request { receiver: Receiver::Class("Cat"), scope: Some(s), terminal: Terminal::Count });
        let root = run(&mut b, "root");
        assert_eq!((root.verdict.clone(), root.words.clone()), (Verdict::Compose, vec!["IntNot".to_string(), "PopCount".into()]));
        let p = u8::try_from(root.null_tests[0].0).unwrap();
        assert_eq!(root.effects[0].args, vec![Arg::Present(p)]);
        assert_eq!(root.reads, vec![p]);
        let child = run(&mut b, "child");
        assert_eq!(child.words, ["IntAnd", "PopCount"]);
        assert_eq!(child.effects[0].args, vec![Arg::Present(p), Arg::Present(p)]);
        let both = run(&mut b, "root_ranked");
        assert_eq!(both.words, ["IntEqual", "IntNot", "IntAnd", "PopCount"]);
        assert!(both.null_tests.len() == 1 && both.predicates.len() == 1);
    }

    #[cfg(not(feature = "disable-null-gate"))]
    #[test]
    fn a_non_optional_belongs_to_over_a_nullable_column_is_not_required() {
        let mut b = basis_with(not_null(&[("Cat", "parent_id")]));
        assert!(!cat_fk_required(&b, "state"));
        let cat = ScopeCatalog::from_class_source("Cat", "scope :in_state, -> { where(state: 5) }\n");
        let t = transpose(&mut b, &cat, &Request { receiver: Receiver::Class("Cat"), scope: Some("in_state"), terminal: Terminal::Count });
        assert_eq!(t.null_tests.len(), 1, "a NULL-able foreign key is guarded by its presence: {:?}", t.verdict);
    }

    #[test]
    fn an_optional_belongs_to_over_a_not_null_column_is_required() {
        let b = basis_with(not_null(&[("Cat", "parent_id")]));
        assert!(cat_fk_required(&b, "parent"));
        assert!(b.class("Cat").unwrap().fields.iter().any(|f| f.predicate_iri.contains("#parent->") && f.predicate_iri.ends_with("@fk!")));
    }

    #[test]
    fn a_column_missing_from_the_map_is_not_required() {
        let b = basis_with(BTreeMap::new());
        assert!(!cat_fk_required(&b, "state"));
        assert!(!cat_fk_required(&b, "parent"));
        let b = basis_with(not_null(&[("Cat", "other_id")]));
        assert!(!cat_fk_required(&b, "state"));
    }

    #[test]
    fn the_basis_grows_only_for_an_admitted_plan() {
        let mut b = basis();
        let before = b.class("State").unwrap().fields.len();
        let cat = ScopeCatalog::from_class_source("State", STATE);
        let _ = transpose(&mut b, &cat, &Request { receiver: Receiver::Class("State"), scope: Some("unknown"), terminal: Terminal::Count });
        assert_eq!(b.class("State").unwrap().fields.len(), before);
        let t = transpose(&mut b, &cat, &Request { receiver: Receiver::Class("State"), scope: Some("active"), terminal: Terminal::Count });
        assert_eq!(b.class("State").unwrap().fields.len(), before + 2);
        assert_eq!(t.result_field, Some(u8::try_from(before + 1).unwrap()));
    }

    #[test]
    fn only_polyfill_positions_are_selected_by_r2il_name() {
        let t = run("mixed", Terminal::Count);
        for e in &t.effects {
            assert_eq!(e.algebra.lens, 0xFFFF_F000);
            assert!(usize::from(e.algebra.word) < lance_graph_r2il::polyfill::POLYFILL.len());
        }
    }
}
