//! Lowers the `default_scope`s of the slice models into
//! `src/generated/default_scopes.rs` (`op_desk::scopes::DefaultScope`).
//!
//! The scopes come from rails-f000's `ScopeCatalog` over the compiler's
//! harvest, so a model gets every default scope of its ancestry (`User` gets
//! `Principal`'s). ActiveRecord applies an ancestor's default scope before the
//! class's own, so scopes are lowered from the farthest ancestor inwards.
//!
//! The forms read, all from OpenProject's own source:
//!
//! - `order(:col)`, `order(col: :asc|:desc)`, `order("table.col ASC")`, and a
//!   string interpolating `X.table_name` (Enumeration);
//! - `where(col: lit)` and `where.not(col: lit)`, where `lit` is an integer, a
//!   string, a boolean, `nil`, or `Model.<enum plural>[:key]` resolved from the
//!   literal `enum :name, { … }` hash in the harvest (Principal);
//! - a bare call to a class method of the model or an ancestor
//!   (`def self.order_by_position`), inlined (Status);
//! - `includes` / `preload` / `eager_load` / `references`, which change how
//!   associations load but not which rows a read returns or their order (Role).
//!
//! Anything else is recorded as not lowered, with its Ruby text, and the desk
//! refuses reads of that table.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use lib_ruby_parser::Node;
use rails_compiler::harvest::Harvest;
use rails_f000::transpose::ScopeCatalog;

/// One lowered default scope set, as the generator writes it.
#[derive(Debug, Default)]
pub struct Lowered {
    pub model: String,
    pub table: String,
    pub order: Vec<(String, bool)>,
    pub filters: Vec<(String, bool, String)>,
    pub no_row_effect: Vec<String>,
    pub unlowered: Vec<(String, String)>,
    pub provenance: Vec<String>,
}

const NO_ROW_EFFECT: &[&str] = &["includes", "preload", "eager_load", "references"];

fn bytes(b: &lib_ruby_parser::Bytes) -> String {
    b.to_string_lossy()
}

fn src_text(src: &str, n: &Node) -> String {
    let l = n.expression();
    src.get(l.begin..l.end).unwrap_or("").to_string()
}

struct Ctx<'a> {
    h: &'a Harvest,
    ancestry: Vec<String>,
    table: &'a str,
    /// Table name of each harvest class the slice knows, for `X.table_name`.
    tables: &'a BTreeMap<String, String>,
}

impl Ctx<'_> {
    /// `def self.<name>` in the body of the model or an ancestor, nearest first.
    fn class_method(&self, name: &str) -> Option<(Node, String)> {
        for class in &self.ancestry {
            for u in self.h.units.get(class).into_iter().flatten() {
                let (Some(body), Some(file)) = (&u.body, self.h.file(&u.file)) else {
                    continue;
                };
                for stmt in stmts(body) {
                    let Node::Defs(d) = stmt else { continue };
                    if d.name != name || !matches!(*d.definee, Node::Self_(_)) || d.args.is_some() {
                        continue;
                    }
                    if let Some(b) = &d.body {
                        return Some(((**b).clone(), file.src.clone()));
                    }
                }
            }
        }
        None
    }

    /// `enum :<name>, { key: value, … }` in the body of `class` or its
    /// ancestors whose plural is `plural`; the integer value of `key`.
    fn enum_value(&self, plural: &str, key: &str) -> Option<String> {
        for class in &self.ancestry {
            for u in self.h.units.get(class).into_iter().flatten() {
                let Some(body) = &u.body else { continue };
                for stmt in stmts(body) {
                    let Node::Send(s) = stmt else { continue };
                    if s.recv.is_some() || s.method_name != "enum" {
                        continue;
                    }
                    let Some(Node::Sym(name)) = s.args.first() else {
                        continue;
                    };
                    let name = bytes(&name.name);
                    if plural != format!("{name}es") && plural != format!("{name}s") {
                        continue;
                    }
                    let Some(Node::Hash(map)) = s.args.get(1) else {
                        continue;
                    };
                    for p in &map.pairs {
                        let Node::Pair(p) = p else { continue };
                        if sym(&p.key).as_deref() == Some(key) {
                            return literal(&p.value);
                        }
                    }
                }
            }
        }
        None
    }
}

fn stmts(body: &Node) -> Vec<&Node> {
    match body {
        Node::Begin(b) => b.statements.iter().collect(),
        other => vec![other],
    }
}

/// Rust source of a `scopes::Lit` for a literal node.
fn literal(n: &Node) -> Option<String> {
    Some(match n {
        Node::Int(i) => format!("Lit::Int({})", i.value.replace('_', "")),
        Node::Str(s) => format!("Lit::Str({:?})", bytes(&s.value)),
        Node::True(_) => "Lit::Bool(true)".into(),
        Node::False(_) => "Lit::Bool(false)".into(),
        Node::Nil(_) => "Lit::Nil".into(),
        _ => return None,
    })
}

/// The method chain of a scope body, receiver first: `where.not(x)` is
/// `[where(), not(x)]`. `None` when the chain does not start at `self`.
fn chain(n: &Node) -> Option<Vec<&lib_ruby_parser::nodes::Send>> {
    let Node::Send(s) = n else { return None };
    let mut out = match s.recv.as_deref() {
        None => Vec::new(),
        Some(r) => chain(r)?,
    };
    out.push(s);
    Some(out)
}

fn hash_pairs(args: &[Node]) -> Option<Vec<(&Node, &Node)>> {
    let [arg] = args else { return None };
    let pairs = match arg {
        Node::Hash(h) => &h.pairs,
        Node::Kwargs(k) => &k.pairs,
        _ => return None,
    };
    pairs
        .iter()
        .map(|p| match p {
            Node::Pair(p) => Some((&*p.key, &*p.value)),
            _ => None,
        })
        .collect()
}

fn sym(n: &Node) -> Option<String> {
    match n {
        Node::Sym(s) => Some(bytes(&s.name)),
        _ => None,
    }
}

impl Ctx<'_> {
    /// A `where` value: a literal, or `Model.<plural>[:key]`.
    fn where_value(&self, n: &Node) -> Option<String> {
        if let Some(l) = literal(n) {
            return Some(l);
        }
        let Node::Index(ix) = n else { return None };
        let Node::Send(s) = &*ix.recv else {
            return None;
        };
        if !s.args.is_empty() {
            return None;
        }
        let [key] = &ix.indexes[..] else { return None };
        let v = self.enum_value(&s.method_name, &sym(key)?)?;
        Some(v)
    }

    /// One `order` argument: `(column, ascending)` keys.
    fn order_keys(&self, n: &Node, src: &str) -> Option<Vec<(String, bool)>> {
        match n {
            Node::Sym(s) => Some(vec![(bytes(&s.name), true)]),
            Node::Hash(_) | Node::Kwargs(_) => hash_pairs(std::slice::from_ref(n))?
                .into_iter()
                .map(|(k, v)| {
                    let asc = match sym(v)?.as_str() {
                        "asc" => true,
                        "desc" => false,
                        _ => return None,
                    };
                    Some((sym(k)?, asc))
                })
                .collect(),
            Node::Str(s) => self.order_text(&bytes(&s.value)),
            Node::Dstr(d) => {
                let mut text = String::new();
                for p in &d.parts {
                    match p {
                        Node::Str(s) => text.push_str(&bytes(&s.value)),
                        Node::Begin(b) => {
                            let [Node::Send(s)] = &b.statements[..] else {
                                return None;
                            };
                            let Some(Node::Const(c)) = s.recv.as_deref() else {
                                return None;
                            };
                            if s.method_name != "table_name" || c.scope.is_some() {
                                return None;
                            }
                            text.push_str(self.tables.get(&c.name)?);
                        }
                        _ => return None,
                    }
                }
                let _ = src;
                self.order_text(&text)
            }
            _ => None,
        }
    }

    /// `"col"`, `"col ASC"`, `"table.col DESC"`, comma-separated; a qualified
    /// column must name this model's table.
    fn order_text(&self, text: &str) -> Option<Vec<(String, bool)>> {
        text.split(',')
            .map(|part| {
                let mut words = part.split_whitespace();
                let col = words.next()?;
                let asc = match words.next().map(str::to_ascii_lowercase).as_deref() {
                    None | Some("asc") => true,
                    Some("desc") => false,
                    _ => return None,
                };
                if words.next().is_some() {
                    return None;
                }
                let col = match col.split_once('.') {
                    Some((t, c)) if t == self.table => c,
                    Some(_) => return None,
                    None => col,
                };
                let ok =
                    !col.is_empty() && col.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
                ok.then(|| (col.to_string(), asc))
            })
            .collect()
    }

    /// Lowers one scope body into `out`. `depth` bounds class-method inlining.
    fn lower(&self, body: &Node, src: &str, out: &mut Lowered, depth: usize) {
        let ruby = src_text(src, body);
        let Some(links) = chain(body) else {
            out.unlowered
                .push((ruby, "not a method chain on self".into()));
            return;
        };
        // A bare class-method call inlines that method's body.
        let bare = match &links[..] {
            [only] => Some(*only).filter(|o| {
                o.args.is_empty()
                    && !NO_ROW_EFFECT.contains(&o.method_name.as_str())
                    && !matches!(o.method_name.as_str(), "where" | "order" | "all")
            }),
            _ => None,
        };
        if let Some(only) = bare {
            match self.class_method(&only.method_name) {
                Some((inner, inner_src)) if depth < 4 => {
                    self.lower(&inner, &inner_src, out, depth + 1);
                }
                _ => out
                    .unlowered
                    .push((ruby, format!("class method {} not found", only.method_name))),
            }
            return;
        }
        let mut i = 0;
        while i < links.len() {
            let l = links[i];
            let name = l.method_name.as_str();
            match name {
                "all" if l.args.is_empty() => {}
                n if NO_ROW_EFFECT.contains(&n) => out.no_row_effect.push(src_text(src, body)),
                "order" if !l.args.is_empty() => {
                    for a in &l.args {
                        match self.order_keys(a, src) {
                            Some(keys) => out.order.extend(keys),
                            None => {
                                out.unlowered.push((ruby, "order argument".into()));
                                return;
                            }
                        }
                    }
                }
                "where" => {
                    let negated = l.args.is_empty()
                        && links.get(i + 1).is_some_and(|n| n.method_name == "not");
                    let args = if negated {
                        i += 1;
                        &links[i].args
                    } else {
                        &l.args
                    };
                    let Some(pairs) = hash_pairs(args) else {
                        out.unlowered
                            .push((ruby, "where argument is not a hash".into()));
                        return;
                    };
                    for (k, v) in pairs {
                        match (sym(k), self.where_value(v)) {
                            (Some(col), Some(lit)) => out.filters.push((col, negated, lit)),
                            _ => {
                                out.unlowered.push((ruby, "where value".into()));
                                return;
                            }
                        }
                    }
                }
                _ => {
                    out.unlowered.push((ruby, format!("method {name}")));
                    return;
                }
            }
            i += 1;
        }
    }
}

/// The lowered default scopes of every slice model, in model order.
pub fn lower(h: &Harvest, models: &[(String, String)]) -> Vec<Lowered> {
    let catalog = ScopeCatalog::from_harvest(h);
    let tables: BTreeMap<String, String> = models.iter().cloned().collect();
    let mut out = Vec::new();
    for (model, table) in models {
        let scopes: Vec<_> = catalog.default_scopes(model).collect();
        if scopes.is_empty() {
            continue;
        }
        let mut ancestry = vec![model.clone()];
        let mut i = 0;
        while i < ancestry.len() {
            for p in catalog.parents.get(&ancestry[i]).into_iter().flatten() {
                if !ancestry.contains(p) {
                    ancestry.push(p.clone());
                }
            }
            i += 1;
        }
        let ctx = Ctx {
            h,
            ancestry: ancestry.clone(),
            table,
            tables: &tables,
        };
        let mut l = Lowered {
            model: model.clone(),
            table: table.clone(),
            ..Lowered::default()
        };
        // Farthest ancestor first, then source order within a class.
        let mut ordered = scopes;
        ordered.sort_by_key(|s| {
            (
                std::cmp::Reverse(ancestry.iter().position(|a| a == &s.class).unwrap_or(0)),
                s.provenance.clone(),
                s.anchor,
            )
        });
        for s in ordered {
            l.provenance.push(s.provenance.clone());
            match &s.body {
                Some(b) => ctx.lower(b, &s.src, &mut l, 0),
                None => l.unlowered.push((s.ruby.clone(), "no body".into())),
            }
        }
        out.push(l);
    }
    out
}

/// `src/generated/default_scopes.rs`.
pub fn emit(lowered: &[Lowered], header: &str) -> String {
    let mut s = String::new();
    s.push_str(header);
    s.push_str("//! The lowered `default_scope`s of the slice models (`op_desk::scopes`).\n\n");
    s.push_str("// Glob: which of these the data names depends on OpenProject's source.\nuse crate::scopes::*;\n\n");
    s.push_str("/// Every slice model that declares (or inherits) a default scope.\n");
    s.push_str("pub static DEFAULT_SCOPES: &[DefaultScope] = &[\n");
    for l in lowered {
        let _ = writeln!(s, "    DefaultScope {{");
        let _ = writeln!(s, "        model: {:?},", l.model);
        let _ = writeln!(s, "        table: {:?},", l.table);
        s.push_str("        order: &[");
        for (c, asc) in &l.order {
            let _ = write!(
                s,
                "Order {{ col: {c:?}, dir: Dir::{} }}, ",
                if *asc { "Asc" } else { "Desc" }
            );
        }
        s.push_str("],\n        filters: &[");
        for (c, neg, lit) in &l.filters {
            let _ = write!(
                s,
                "Filter {{ col: {c:?}, op: Op::{}, lit: {lit} }}, ",
                if *neg { "Ne" } else { "Eq" }
            );
        }
        s.push_str("],\n        no_row_effect: &[");
        for r in &l.no_row_effect {
            let _ = write!(s, "{r:?}, ");
        }
        s.push_str("],\n        unlowered: &[");
        for (r, why) in &l.unlowered {
            let _ = write!(s, "Unlowered {{ ruby: {r:?}, reason: {why:?} }}, ");
        }
        s.push_str("],\n        provenance: &[");
        for p in &l.provenance {
            let _ = write!(s, "{p:?}, ");
        }
        s.push_str("],\n    },\n");
    }
    s.push_str("];\n");
    s
}
