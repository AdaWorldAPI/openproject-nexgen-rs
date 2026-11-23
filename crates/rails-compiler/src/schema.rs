//! The column stratum, from the migration DSL.
//!
//! ruff's schema arm (`ruff_ruby_spo::extract_app_with_schema`) already
//! yields each model's columns with type and nullability; it does not keep
//! column defaults or limits, and a default is observable behaviour
//! (`users.active` starts `true`, `ticket_articles.content_type` starts
//! `'text/plain'`). This arm reads the same `create_table` blocks from the
//! AST and records the full column definition. [`cross_check`] compares its
//! column sets with ruff's so the two strata cannot silently drift; the
//! default/limit fields are the upstream candidate for `ruff_spo_triplet::Field`.

use std::collections::BTreeMap;
use std::path::Path;

use lib_ruby_parser::{Node, Parser, ParserOptions};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Default {
    None,
    Int(i64),
    Str(String),
    Bool(bool),
    /// A default the DSL computes (`default: []`, `default: -> { … }`);
    /// carried as source text and reported, not lowered.
    Expr(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnDef {
    pub name: String,
    pub ty: String,
    pub null: bool,
    pub default: Default,
    pub limit: Option<u32>,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableDef {
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub provenance: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Schema {
    pub tables: BTreeMap<String, TableDef>,
    pub unparsed_column_calls: Vec<String>,
}

fn sym_or_str(n: &Node) -> Option<String> {
    match n {
        Node::Sym(s) => Some(s.name.to_string_lossy()),
        Node::Str(s) => Some(s.value.to_string_lossy()),
        _ => None,
    }
}

fn kwargs(args: &[Node]) -> BTreeMap<String, Node> {
    let mut out = BTreeMap::new();
    for a in args {
        let pairs = match a {
            Node::Kwargs(k) => &k.pairs,
            Node::Hash(h) => &h.pairs,
            _ => continue,
        };
        for p in pairs {
            if let Node::Pair(p) = p
                && let Some(k) = sym_or_str(&p.key)
            {
                out.insert(k, (*p.value).clone());
            }
        }
    }
    out
}

fn default_of(n: Option<&Node>) -> Default {
    match n {
        None | Some(Node::Nil(_)) => Default::None,
        Some(Node::Int(i)) => i.value.parse().map_or_else(|_| Default::Expr(i.value.clone()), Default::Int),
        Some(Node::Str(s)) => Default::Str(s.value.to_string_lossy()),
        Some(Node::True(_)) => Default::Bool(true),
        Some(Node::False(_)) => Default::Bool(false),
        Some(other) => Default::Expr(format!("{:?}", std::mem::discriminant(other))),
    }
}

fn limit_of(n: Option<&Node>) -> Option<u32> {
    match n {
        Some(Node::Int(i)) => i.value.parse().ok(),
        _ => None,
    }
}

fn null_of(opts: &BTreeMap<String, Node>) -> bool {
    !matches!(opts.get("null"), Some(Node::False(_)))
}

fn line(src: &str, byte: usize) -> usize {
    src[..byte.min(src.len())].matches('\n').count() + 1
}

const COLUMN_TYPES: &[&str] = &[
    "string", "text", "integer", "bigint", "boolean", "datetime", "timestamp", "decimal", "float", "date", "json",
    "jsonb", "binary", "time",
];

impl Schema {
    pub fn read(source: &Path, migrations: &[String]) -> Result<Self, String> {
        let mut schema = Self::default();
        for rel in migrations {
            let path = source.join(rel);
            let src = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let parser = Parser::new(src.as_bytes().to_vec(), ParserOptions::default());
            let ast = parser.do_parse().ast.ok_or_else(|| format!("{rel}: parse failed"))?;
            schema.walk(&ast, rel, &src);
        }
        Ok(schema)
    }

    fn walk(&mut self, node: &Node, rel: &str, src: &str) {
        match node {
            Node::Begin(b) => b.statements.iter().for_each(|s| self.walk(s, rel, src)),
            Node::Class(c) => {
                if let Some(b) = &c.body {
                    self.walk(b, rel, src);
                }
            }
            Node::Def(d) if d.name == "up" => {
                if let Some(b) = &d.body {
                    self.walk(b, rel, src);
                }
            }
            Node::Block(b) => self.create_table(b, rel, src),
            Node::Send(s) if s.recv.is_none() && s.method_name == "add_column" => {
                let (Some(t), Some(c), Some(ty)) = (
                    s.args.first().and_then(sym_or_str),
                    s.args.get(1).and_then(sym_or_str),
                    s.args.get(2).and_then(sym_or_str),
                ) else {
                    return;
                };
                let opts = kwargs(&s.args);
                let prov = format!("{rel}:{}", line(src, s.expression_l.begin));
                if let Some(table) = self.tables.get_mut(&t)
                    && !table.columns.iter().any(|x| x.name == c)
                {
                    table.columns.push(ColumnDef {
                        name: c,
                        ty,
                        null: null_of(&opts),
                        default: default_of(opts.get("default")),
                        limit: limit_of(opts.get("limit")),
                        provenance: prov,
                    });
                }
            }
            _ => {}
        }
    }

    fn create_table(&mut self, b: &lib_ruby_parser::nodes::Block, rel: &str, src: &str) {
        let Node::Send(call) = &*b.call else { return };
        if call.recv.is_some() || call.method_name != "create_table" {
            return;
        }
        let Some(name) = call.args.first().and_then(sym_or_str) else { return };
        let topts = kwargs(&call.args);
        let prov = format!("{rel}:{}", line(src, call.expression_l.begin));
        let mut columns = Vec::new();
        if !matches!(topts.get("id"), Some(Node::False(_))) {
            columns.push(ColumnDef {
                name: "id".into(),
                ty: "integer".into(),
                null: false,
                default: Default::None,
                limit: None,
                provenance: prov.clone(),
            });
        }
        let stmts: Vec<&Node> = match b.body.as_deref() {
            Some(Node::Begin(bb)) => bb.statements.iter().collect(),
            Some(n) => vec![n],
            None => vec![],
        };
        for st in stmts {
            let Node::Send(s) = st else { continue };
            if !matches!(s.recv.as_deref(), Some(Node::Lvar(_))) {
                continue;
            }
            let opts = kwargs(&s.args);
            let cprov = format!("{rel}:{}", line(src, s.expression_l.begin));
            let push = |columns: &mut Vec<ColumnDef>, n: String, ty: &str, null: bool, opts: &BTreeMap<String, Node>| {
                columns.retain(|c: &ColumnDef| c.name != n);
                columns.push(ColumnDef {
                    name: n,
                    ty: ty.to_string(),
                    null,
                    default: default_of(opts.get("default")),
                    limit: limit_of(opts.get("limit")),
                    provenance: cprov.clone(),
                });
            };
            let m = s.method_name.as_str();
            match m {
                "timestamps" => {
                    for n in ["created_at", "updated_at"] {
                        push(&mut columns, n.into(), "datetime", null_of(&opts), &BTreeMap::new());
                    }
                }
                "references" | "belongs_to" => {
                    for a in &s.args {
                        if let Some(n) = sym_or_str(a) {
                            push(&mut columns, format!("{n}_id"), "integer", null_of(&opts), &BTreeMap::new());
                            if matches!(opts.get("polymorphic"), Some(Node::True(_))) {
                                push(&mut columns, format!("{n}_type"), "string", null_of(&opts), &BTreeMap::new());
                            }
                        }
                    }
                }
                "column" => {
                    if let (Some(n), Some(ty)) = (s.args.first().and_then(sym_or_str), s.args.get(1).and_then(sym_or_str)) {
                        let ty = if matches!(opts.get("array"), Some(Node::True(_))) { "json".to_string() } else { ty };
                        push(&mut columns, n, &ty, null_of(&opts), &opts);
                    }
                }
                ty if COLUMN_TYPES.contains(&ty) => {
                    let ty = if matches!(opts.get("array"), Some(Node::True(_))) { "json" } else { ty };
                    for a in &s.args {
                        if let Some(n) = sym_or_str(a) {
                            push(&mut columns, n, ty, null_of(&opts), &opts);
                        }
                    }
                }
                "index" => {}
                other => self.unparsed_column_calls.push(format!("{cprov} t.{other}")),
            }
        }
        self.tables.insert(
            name.clone(),
            TableDef {
                name,
                columns,
                provenance: prov,
            },
        );
    }
}

/// Columns ruff's schema arm attaches to each model, compared with this arm's
/// tables: `(model, only_in_ruff, only_here)` for every model that differs.
#[must_use]
pub fn cross_check(
    graph: &ruff_spo_triplet::ModelGraph,
    schema: &Schema,
    table_of: &dyn Fn(&str) -> String,
    models: &[String],
) -> Vec<(String, Vec<String>, Vec<String>)> {
    let mut out = Vec::new();
    for m in graph.models.iter().filter(|m| models.contains(&m.name)) {
        let Some(t) = schema.tables.get(&table_of(&m.name)) else { continue };
        let ruff: std::collections::BTreeSet<_> = m.fields.iter().filter(|f| f.field_type.is_some()).map(|f| f.name.clone()).collect();
        let ours: std::collections::BTreeSet<_> = t.columns.iter().map(|c| c.name.clone()).collect();
        let only_ruff: Vec<_> = ruff.difference(&ours).cloned().collect();
        let only_ours: Vec<_> = ours.difference(&ruff).cloned().collect();
        if !only_ruff.is_empty() || !only_ours.is_empty() {
            out.push((m.name.clone(), only_ruff, only_ours));
        }
    }
    out
}
