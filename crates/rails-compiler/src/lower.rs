//! Ruby method bodies → Rust.
//!
//! Lowering is static wherever the schema makes it so: an attribute read is a
//! column ordinal, an association reader names its foreign key and target
//! model, `self.class.column_names.include?('x')` is a constant
//! (R-FOLD-SCHEMA). Receivers whose model the compiler cannot know fall back
//! to by-name access, which is counted. Any construct outside the supported
//! subset fails the whole method with its source location; the caller turns
//! that into a `Target::Residual` so it can never run as success.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use lib_ruby_parser::Node;

use crate::config::Loaded;
use crate::harvest::{Harvest, Unit, const_name};
use crate::semantic::Assembler;

/// Static facts about a slice model the lowering needs.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    /// The generated module (`ticket_article`).
    pub module: String,
    pub columns: Vec<String>,
    /// `belongs_to` name → (foreign-key column, target model).
    pub belongs_to: BTreeMap<String, (String, String)>,
    pub ancestors: Vec<String>,
    /// `attr_accessor` names.
    pub virtuals: Vec<String>,
}

impl ModelInfo {
    fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ty {
    Val,
    Rec(String),
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Unsupported {
    pub what: String,
    pub at: String,
}

/// The lowering result for one model.
#[derive(Debug, Default)]
pub struct Lowered {
    /// fn name → Rust source.
    pub fns: BTreeMap<String, String>,
    pub regexes: Vec<String>,
    pub host_calls: BTreeSet<String>,
    pub aliases_used: BTreeSet<String>,
    pub dynamic_attr: BTreeSet<String>,
    pub folded: BTreeSet<String>,
    /// method → why it could not be lowered.
    pub failed: BTreeMap<String, Unsupported>,
    /// method → source span.
    pub spans: BTreeMap<String, String>,
}

pub struct Lowerer<'a> {
    pub cfg: &'a Loaded,
    pub h: &'a Harvest,
    pub asm: &'a Assembler<'a>,
    pub models: &'a BTreeMap<String, ModelInfo>,
    pub me: &'a ModelInfo,
    pub out: Lowered,
    in_progress: BTreeSet<String>,
}

/// A Rust identifier for a Ruby method name.
#[must_use]
pub fn fn_ident(prefix: &str, method: &str) -> String {
    let mut s = String::from(prefix);
    for c in method.chars() {
        match c {
            '?' => s.push_str("_q"),
            '!' => s.push_str("_bang"),
            '=' => s.push_str("_set"),
            c if c.is_ascii_alphanumeric() || c == '_' => s.push(c),
            _ => s.push('_'),
        }
    }
    s
}

#[must_use]
pub fn const_ident(col: &str) -> String {
    col.to_ascii_uppercase()
}

fn str_of(n: &Node) -> Option<String> {
    match n {
        Node::Str(s) => Some(s.value.to_string_lossy()),
        Node::Sym(s) => Some(s.name.to_string_lossy()),
        _ => None,
    }
}

fn rust_str(s: &str) -> String {
    format!("{s:?}")
}

struct Fn<'n> {
    locals: BTreeMap<String, Ty>,
    params: Vec<String>,
    unit: &'n Unit,
}

type R<T> = Result<T, Unsupported>;

impl<'a> Lowerer<'a> {
    pub fn new(
        cfg: &'a Loaded,
        h: &'a Harvest,
        asm: &'a Assembler<'a>,
        models: &'a BTreeMap<String, ModelInfo>,
        me: &'a ModelInfo,
    ) -> Self {
        Self {
            cfg,
            h,
            asm,
            models,
            me,
            out: Lowered::default(),
            in_progress: BTreeSet::new(),
        }
    }

    fn col_path(&self, model: &ModelInfo, col: &str) -> String {
        if model.name == self.me.name {
            format!("col::{}", const_ident(col))
        } else {
            format!("crate::{}::col::{}", model.module, const_ident(col))
        }
    }

    fn model_path(&self, model: &ModelInfo) -> String {
        if model.name == self.me.name {
            "&MODEL".into()
        } else {
            format!("&crate::{}::MODEL", model.module)
        }
    }

    fn unsupported(&self, f: &Fn<'_>, node: &Node, what: impl Into<String>) -> Unsupported {
        Unsupported {
            what: what.into(),
            at: self.h.loc(f.unit, node.expression().begin),
        }
    }

    /// Lower `method` as defined for this model (Ruby method lookup over the
    /// model's ancestors). Returns the generated fn name.
    pub fn method(&mut self, method: &str) -> Result<String, Unsupported> {
        let ident = fn_ident("m_", method);
        if self.out.fns.contains_key(&ident) || self.in_progress.contains(&ident) {
            return Ok(ident);
        }
        if let Some(u) = self.out.failed.get(method) {
            return Err(u.clone());
        }
        let Some((unit, def)) = self.asm.find_def(&self.me.ancestors, method) else {
            // Methods Ruby/ActiveRecord define without a `def`: an
            // `attr_accessor` reader, a column reader, a `column?` query.
            let synth = if self.me.virtuals.iter().any(|v| v == method) {
                Some(format!("rec.virtual_get({})", rust_str(method)))
            } else if self.me.col(method).is_some() {
                Some(format!("rec.get({}).clone()", self.col_path(self.me, method)))
            } else if let Some(base) = method.strip_suffix('?')
                && self.me.col(base).is_some()
            {
                Some(format!("rp::Value::Bool(rp::ruby::query_attribute(rec.get({})))", self.col_path(self.me, base)))
            } else {
                None
            };
            if let Some(body) = synth {
                let code = format!(
                    "/// `{method}` — generated reader (attr_accessor / attribute method)\n#[allow(unused_variables)]\npub fn {ident}(ctx: &mut rp::Ctx, rec: &mut rp::Record, args: &[rp::Value]) -> Result<rp::Value, rp::Halt> {{\n    Ok({body})\n}}\n"
                );
                self.out.fns.insert(ident.clone(), code);
                self.out.spans.insert(method.to_string(), "generated attribute reader".into());
                return Ok(ident);
            }
            return Err(Unsupported {
                what: format!("method `{method}` not defined in any ancestor of {}", self.me.name),
                at: self.me.name.clone(),
            });
        };
        let Node::Def(d) = def else { unreachable_def() };
        let span = format!(
            "{}-{}",
            self.h.loc(unit, d.expression_l.begin),
            self.h.file(&unit.file).map_or(0, |f| f.line_of(d.expression_l.end))
        );
        self.in_progress.insert(ident.clone());
        let res = self.def_body(unit, def, &ident, method, &span);
        self.in_progress.remove(&ident);
        match res {
            Ok(code) => {
                self.out.fns.insert(ident.clone(), code);
                self.out.spans.insert(method.to_string(), span);
                Ok(ident)
            }
            Err(u) => {
                self.out.failed.insert(method.to_string(), u.clone());
                Err(u)
            }
        }
    }

    fn def_body(&mut self, unit: &Unit, def: &Node, ident: &str, method: &str, span: &str) -> R<String> {
        let Node::Def(d) = def else { unreachable_def() };
        let mut f = Fn {
            locals: BTreeMap::new(),
            params: Vec::new(),
            unit,
        };
        if let Some(Node::Args(a)) = d.args.as_deref() {
            for arg in &a.args {
                match arg {
                    Node::Arg(x) => f.params.push(x.name.clone()),
                    other => return Err(self.unsupported(&f, other, "non-positional parameter")),
                }
            }
        }
        for p in f.params.clone() {
            f.locals.insert(p, Ty::Unknown);
        }
        collect_locals(d.body.as_deref(), &mut f.locals);
        let body = match d.body.as_deref() {
            Some(b) => self.stmts(&mut f, b)?,
            None => "rp::Value::Nil".into(),
        };
        let mut code = String::new();
        let _ = writeln!(code, "/// `{method}` — {span}");
        let _ = writeln!(
            code,
            "#[allow(unused_variables, unused_mut, unused_assignments, clippy::all)]\npub fn {ident}(ctx: &mut rp::Ctx, rec: &mut rp::Record, args: &[rp::Value]) -> Result<rp::Value, rp::Halt> {{"
        );
        for (i, p) in f.params.iter().enumerate() {
            let _ = writeln!(code, "    let mut l_{p} = args.get({i}).cloned().unwrap_or(rp::Value::Nil);");
        }
        for (l, _) in f.locals.iter().filter(|(l, _)| !f.params.contains(l)) {
            let _ = writeln!(code, "    let mut l_{l} = rp::Value::Nil;");
        }
        let _ = writeln!(code, "    Ok({body})\n}}");
        Ok(code)
    }

    /// Lower a lambda / block body used as a callback or condition.
    pub fn block(&mut self, unit: &Unit, body: Option<&Node>, ident: &str, label: &str) -> R<String> {
        let mut f = Fn {
            locals: BTreeMap::new(),
            params: Vec::new(),
            unit,
        };
        collect_locals(body, &mut f.locals);
        let b = match body {
            Some(b) => self.stmts(&mut f, b)?,
            None => "rp::Value::Nil".into(),
        };
        let mut code = format!(
            "/// {label}\n#[allow(unused_variables, unused_mut, clippy::all)]\npub fn {ident}(ctx: &mut rp::Ctx, rec: &mut rp::Record) -> Result<rp::Value, rp::Halt> {{\n"
        );
        for l in f.locals.keys() {
            let _ = writeln!(code, "    let mut l_{l} = rp::Value::Nil;");
        }
        let _ = writeln!(code, "    Ok({b})\n}}");
        self.out.fns.insert(ident.to_string(), code);
        Ok(ident.to_string())
    }

    fn stmts(&mut self, f: &mut Fn<'_>, node: &Node) -> R<String> {
        match node {
            Node::Begin(_) | Node::KwBegin(_) => {
                let mut parts = Vec::new();
                for s in &b_statements(node) {
                    parts.push(self.expr(f, s)?.0);
                }
                Ok(seq(&parts))
            }
            other => Ok(self.expr(f, other)?.0),
        }
    }

    fn opt_stmts(&mut self, f: &mut Fn<'_>, node: Option<&Node>) -> R<String> {
        match node {
            Some(n) => self.stmts(f, n),
            None => Ok("rp::Value::Nil".into()),
        }
    }

    fn truthy(&mut self, f: &mut Fn<'_>, node: &Node) -> R<String> {
        let (c, _) = self.expr(f, node)?;
        Ok(format!("({c}).truthy()"))
    }

    fn expr(&mut self, f: &mut Fn<'_>, node: &Node) -> R<(String, Ty)> {
        Ok(match node {
            Node::True(_) => ("rp::Value::Bool(true)".into(), Ty::Val),
            Node::False(_) => ("rp::Value::Bool(false)".into(), Ty::Val),
            Node::Nil(_) => ("rp::Value::Nil".into(), Ty::Val),
            Node::Int(i) => (format!("rp::Value::Int({})", i.value.replace('_', "")), Ty::Val),
            Node::Str(s) => (format!("rp::Value::str({})", rust_str(&s.value.to_string_lossy())), Ty::Val),
            Node::Sym(s) => (format!("rp::Value::str({})", rust_str(&s.name.to_string_lossy())), Ty::Val),
            Node::Self_(_) => ("rp::Value::Rec(Box::new(rec.clone()))".into(), Ty::Rec(self.me.name.clone())),
            Node::Begin(_) | Node::KwBegin(_) => (format!("{{ {} }}", self.stmts(f, node)?), Ty::Val),
            Node::Lvar(v) => {
                let ty = f.locals.get(&v.name).cloned().unwrap_or(Ty::Unknown);
                (format!("l_{}.clone()", v.name), ty)
            }
            Node::Lvasgn(a) => {
                let Some(v) = a.value.as_deref() else {
                    return Err(self.unsupported(f, node, "lvasgn without value"));
                };
                let (c, ty) = self.expr(f, v)?;
                f.locals.insert(a.name.clone(), ty.clone());
                (format!("{{ l_{n} = {c}; l_{n}.clone() }}", n = a.name), ty)
            }
            Node::If(i) => {
                let c = self.truthy(f, &i.cond)?;
                let t = self.opt_stmts(f, i.if_true.as_deref())?;
                let e = self.opt_stmts(f, i.if_false.as_deref())?;
                (format!("if {c} {{ {t} }} else {{ {e} }}"), Ty::Val)
            }
            Node::IfMod(i) => {
                let c = self.truthy(f, &i.cond)?;
                let t = self.opt_stmts(f, i.if_true.as_deref())?;
                let e = self.opt_stmts(f, i.if_false.as_deref())?;
                (format!("if {c} {{ {t} }} else {{ {e} }}"), Ty::Val)
            }
            Node::IfTernary(i) => {
                let c = self.truthy(f, &i.cond)?;
                let (t, _) = self.expr(f, &i.if_true)?;
                let (e, _) = self.expr(f, &i.if_false)?;
                (format!("if {c} {{ {t} }} else {{ {e} }}"), Ty::Val)
            }
            Node::And(a) => {
                let (l, _) = self.expr(f, &a.lhs)?;
                let (r, _) = self.expr(f, &a.rhs)?;
                (format!("{{ let lhs = {l}; if lhs.truthy() {{ {r} }} else {{ lhs }} }}"), Ty::Val)
            }
            Node::Or(o) => {
                let (l, _) = self.expr(f, &o.lhs)?;
                let (r, _) = self.expr(f, &o.rhs)?;
                (format!("{{ let lhs = {l}; if lhs.truthy() {{ lhs }} else {{ {r} }} }}"), Ty::Val)
            }
            Node::Return(r) => {
                let v = match r.args.as_slice() {
                    [] => "rp::Value::Nil".to_string(),
                    [one] => self.expr(f, one)?.0,
                    _ => return Err(self.unsupported(f, node, "multi-value return")),
                };
                (format!("return Ok({v})"), Ty::Val)
            }
            Node::Regexp(_) => return Err(self.unsupported(f, node, "regexp outside a match?/gsub call")),
            Node::Send(s) => self.send(f, node, s.recv.as_deref(), &s.method_name, &s.args, false)?,
            Node::CSend(s) => self.send(f, node, Some(&s.recv), &s.method_name, &s.args, true)?,
            Node::Index(ix) => {
                // `changes_to_save['col']`
                if let Node::Send(s) = &*ix.recv
                    && s.recv.is_none()
                    && s.method_name == "changes_to_save"
                    && let [key] = ix.indexes.as_slice()
                    && let Some(k) = str_of(key)
                {
                    let Some(_) = self.me.col(&k) else {
                        return Err(self.unsupported(f, node, format!("changes_to_save[{k}]: no such column")));
                    };
                    return Ok((format!("rec.changes_to_save({})", self.col_path(self.me, &k)), Ty::Val));
                }
                let (r, _) = self.expr(f, &ix.recv)?;
                match ix.indexes.as_slice() {
                    [Node::Int(i)] => (format!("({r}).index({})", i.value), Ty::Val),
                    _ => return Err(self.unsupported(f, node, "index with a non-integer key")),
                }
            }
            other => {
                return Err(self.unsupported(f, other, format!("node {}", node_kind(other))));
            }
        })
    }

    fn args(&mut self, f: &mut Fn<'_>, args: &[Node]) -> R<Vec<String>> {
        args.iter().map(|a| self.expr(f, a).map(|(c, _)| c)).collect()
    }

    fn regex(&mut self, f: &Fn<'_>, node: &Node) -> R<String> {
        let Node::Regexp(r) = node else {
            return Err(self.unsupported(f, node, "expected a regexp literal"));
        };
        let mut pattern = String::new();
        for p in &r.parts {
            match p {
                Node::Str(s) => pattern.push_str(&s.value.to_string_lossy()),
                _ => return Err(self.unsupported(f, node, "interpolated regexp")),
            }
        }
        let flags = match r.options.as_deref() {
            Some(Node::RegOpt(o)) => o.options.clone().unwrap_or_default(),
            _ => String::new(),
        };
        let translated = rails_polyfill::ruby::translate_regex(&pattern, &flags)
            .map_err(|e| self.unsupported(f, node, format!("regexp: {e}")))?;
        let idx = self.out.regexes.iter().position(|x| *x == translated).unwrap_or_else(|| {
            self.out.regexes.push(translated);
            self.out.regexes.len() - 1
        });
        Ok(format!("&RE_{idx}"))
    }

    /// Ruby evaluates the receiver, then the arguments left to right, then
    /// calls — so each is bound before the call (which also keeps `ctx`
    /// borrows sequential).
    fn host(&mut self, spelling: String, recv: Option<String>, args: Vec<String>) -> (String, Ty) {
        let mut code = String::from("{ ");
        let recv_arg = match recv {
            Some(r) => {
                let _ = write!(code, "let recv = {r}; ");
                "match &recv { rp::Value::Rec(r) => Some(&**r), _ => None }".to_string()
            }
            None => "None".to_string(),
        };
        let names: Vec<String> = (0..args.len()).map(|i| format!("a{i}")).collect();
        for (n, a) in names.iter().zip(&args) {
            let _ = write!(code, "let {n} = {a}; ");
        }
        let _ = write!(code, "ctx.host_call({}, {recv_arg}, &[{}])? }}", rust_str(&spelling), names.join(", "));
        self.out.host_calls.insert(spelling);
        (code, Ty::Val)
    }

    #[expect(clippy::too_many_lines, reason = "the dispatch table for sends")]
    fn send(&mut self, f: &mut Fn<'_>, node: &Node, recv: Option<&Node>, m: &str, args: &[Node], safe: bool) -> R<(String, Ty)> {
        let me = self.me;
        // ── receiver: self (implicit or explicit) ──────────────────────────
        let self_recv = matches!(recv, None | Some(Node::Self_(_)));
        if self_recv {
            if args.is_empty() {
                if let Some(_c) = me.col(m) {
                    return Ok((format!("rec.get({}).clone()", self.col_path(me, m)), Ty::Val));
                }
                if let Some(base) = m.strip_suffix('?')
                    && me.col(base).is_some()
                {
                    return Ok((format!("rp::Value::Bool(rp::ruby::query_attribute(rec.get({})))", self.col_path(me, base)), Ty::Val));
                }
                if let Some(base) = m.strip_suffix("_changed?")
                    && me.col(base).is_some()
                {
                    return Ok((format!("rp::Value::Bool(rec.changed({}))", self.col_path(me, base)), Ty::Val));
                }
                if let Some((fk, target)) = me.belongs_to.get(m) {
                    let Some(t) = self.models.get(target) else {
                        return Err(self.unsupported(f, node, format!("association `{m}` targets {target}, outside the slice")));
                    };
                    let mp = self.model_path(t);
                    return Ok((
                        format!("rp::lifecycle::belongs_to_cached(ctx, rec, {}, {}, {mp})?", rust_str(m), self.col_path(me, fk)),
                        Ty::Rec(target.clone()),
                    ));
                }
                if me.virtuals.iter().any(|v| v == m) {
                    return Ok((format!("rec.virtual_get({})", rust_str(m)), Ty::Val));
                }
                match m {
                    "new_record?" => return Ok(("rp::Value::Bool(rec.new_record())".into(), Ty::Val)),
                    "persisted?" => return Ok(("rp::Value::Bool(rec.persisted())".into(), Ty::Val)),
                    _ => {}
                }
            }
            if let Some(attr) = m.strip_suffix('=')
                && me.col(attr).is_some()
                && let [v] = args
            {
                let (c, _) = self.expr(f, v)?;
                return Ok((
                    format!("{{ let v = {c}; rec.set({}, v.clone()); v }}", self.col_path(me, attr)),
                    Ty::Val,
                ));
            }
            if let Some(attr) = m.strip_suffix('=')
                && me.virtuals.iter().any(|v| v == attr)
                && let [v] = args
            {
                let (c, _) = self.expr(f, v)?;
                return Ok((format!("{{ let v = {c}; rec.virtual_set({}, v.clone()); v }}", rust_str(attr)), Ty::Val));
            }
            if m == "has_attribute?"
                && let [a] = args
                && let Some(col) = str_of(a)
            {
                self.out.folded.insert(format!("has_attribute?({col})"));
                return Ok((format!("rp::Value::Bool({})", me.col(&col).is_some()), Ty::Val));
            }
            if recv.is_none()
                && let Some(alias) = self.cfg.alias_for(None, m)
            {
                return self.alias(f, node, alias, None, args);
            }
            if m == "!" {
                return Err(self.unsupported(f, node, "unary ! on self"));
            }
            // Own method.
            if self.asm.find_def(&me.ancestors, m).is_some() {
                let ident = self.method(m).map_err(|u| Unsupported {
                    what: format!("calls `{m}`, which does not lower: {}", u.what),
                    at: u.at,
                })?;
                let mut binds = String::new();
                let mut names = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    let (c, _) = self.expr(f, arg)?;
                    let _ = write!(binds, "let a{i} = {c}; ");
                    names.push(format!("a{i}"));
                }
                return Ok((format!("{{ {binds}{ident}(ctx, rec, &[{}])? }}", names.join(", ")), Ty::Val));
            }
            return Err(self.unsupported(f, node, format!("call to `{m}` on self")));
        }
        let recv = recv.unwrap_or_else(|| unreachable_def());

        // ── `self.class.column_names.include?('x')` (R-FOLD-SCHEMA) ─────────
        if matches!(m, "include?" | "exclude?")
            && let Node::Send(cn) = recv
            && cn.method_name == "column_names"
            && let Some(Node::Send(cls)) = cn.recv.as_deref()
            && cls.method_name == "class"
            && matches!(cls.recv.as_deref(), Some(Node::Self_(_)) | None)
            && let [a] = args
            && let Some(col) = str_of(a)
        {
            let has = me.col(&col).is_some();
            self.out.folded.insert(format!("column_names.{m}({col})"));
            return Ok((format!("rp::Value::Bool({})", if m == "include?" { has } else { !has }), Ty::Val));
        }

        // ── `Time.zone.now` ────────────────────────────────────────────────
        if m == "now"
            && let Node::Send(z) = recv
            && z.method_name == "zone"
            && z.recv.as_deref().and_then(const_name).as_deref() == Some("Time")
        {
            return Ok(("rp::Value::time_ms(ctx.now)".into(), Ty::Val));
        }

        // ── constant receivers: aliases, boundaries, model finders ─────────
        if let Some(c) = const_name(recv) {
            let resolved = self.h.resolve(&c, &f.unit.nesting).unwrap_or_else(|| c.clone());
            let spelling = format!("{resolved}.{m}");
            if self.cfg.is_boundary(&spelling) {
                let a = self.args(f, args)?;
                return Ok(self.host(spelling, None, a));
            }
            if let Some(alias) = self.cfg.alias_for(Some(&resolved), m) {
                return self.alias(f, node, alias, Some(&resolved), args);
            }
            if m == "find_by" {
                return self.find_by(f, node, &resolved, args);
            }
            return Err(self.unsupported(f, node, format!("class method {spelling}")));
        }

        // ── `logger.*` and other aliased receivers ─────────────────────────
        if let Node::Send(r) = recv
            && r.recv.is_none()
            && r.args.is_empty()
            && let Some(alias) = self.cfg.alias_for(Some(&r.method_name), m)
        {
            return self.alias(f, node, alias, Some(&r.method_name.clone()), args);
        }

        // ── `attr.gsub!(re, str)` on a self attribute ──────────────────────
        if m == "gsub!"
            && let Node::Send(r) = recv
            && r.recv.is_none()
            && r.args.is_empty()
            && me.col(&r.method_name).is_some()
            && let [re, rep] = args
            && let Some(rep) = str_of(rep)
        {
            let re = self.regex(f, re)?;
            let col = self.col_path(me, &r.method_name);
            return Ok((
                format!(
                    "{{ let old = rec.get({col}).clone(); let new = rp::ruby::gsub(&old, {re}, {})?; if new != old {{ rec.set({col}, new.clone()); new }} else {{ rp::Value::Nil }} }}",
                    rust_str(&rep)
                ),
                Ty::Val,
            ));
        }

        // ── general receivers ──────────────────────────────────────────────
        let (r, rty) = self.expr(f, recv)?;
        let un = |op: &str| format!("rp::Value::Bool({op})");
        Ok(match (m, args) {
            ("!", []) => (un(&format!("!({r}).truthy()")), Ty::Val),
            ("blank?", []) => (un(&format!("({r}).blank()")), Ty::Val),
            ("present?", []) => (un(&format!("({r}).present()")), Ty::Val),
            ("nil?", []) => (un(&format!("({r}).is_nil()")), Ty::Val),
            ("zero?", []) => (un(&format!("({r}).is_zero()")), Ty::Val),
            ("==", [b]) => {
                let (b, _) = self.expr(f, b)?;
                (un(&format!("rp::ruby::eq(&{r}, &{b})")), Ty::Val)
            }
            ("!=", [b]) => {
                let (b, _) = self.expr(f, b)?;
                (un(&format!("!rp::ruby::eq(&{r}, &{b})")), Ty::Val)
            }
            ("include?", [b]) => {
                let (b, _) = self.expr(f, b)?;
                (un(&format!("({r}).includes(&{b})")), Ty::Val)
            }
            ("exclude?", [b]) => {
                let (b, _) = self.expr(f, b)?;
                (un(&format!("!({r}).includes(&{b})")), Ty::Val)
            }
            ("match?", [re]) => {
                let re = self.regex(f, re)?;
                (un(&format!("rp::ruby::is_match(&{r}, {re})?")), Ty::Val)
            }
            ("sec", []) => (format!("({r}).time_sec().map_or(rp::Value::Nil, rp::Value::Int)"), Ty::Val),
            ("change", [Node::Kwargs(k)])
                if k.pairs.len() == 1
                    && matches!(&k.pairs[0], Node::Pair(p) if str_of(&p.key).as_deref() == Some("sec") && matches!(&*p.value, Node::Int(i) if i.value == "0")) =>
            {
                (format!("({r}).time_change_sec_zero()"), Ty::Val)
            }
            (attr, []) => match &rty {
                Ty::Rec(model) => {
                    let Some(t) = self.models.get(model) else {
                        return Err(self.unsupported(f, node, format!("`.{attr}` on {model}, outside the slice")));
                    };
                    if let Some(_c) = t.col(attr) {
                        let col = self.col_path(t, attr);
                        if safe {
                            (format!("rp::ruby::attr_safe(&{r}, {col})"), Ty::Val)
                        } else {
                            (format!("rp::ruby::attr(&{r}, {col}, {})?", rust_str(attr)), Ty::Val)
                        }
                    } else if let Some(base) = attr.strip_suffix('?')
                        && t.col(base).is_some()
                    {
                        let col = self.col_path(t, base);
                        (format!("rp::Value::Bool(rp::ruby::query_attribute(&rp::ruby::attr(&{r}, {col}, {})?))", rust_str(base)), Ty::Val)
                    } else if self.cfg.is_boundary(&format!("{model}#{attr}")) {
                        self.host(format!("{model}#{attr}"), Some(r), vec![])
                    } else {
                        return Err(self.unsupported(f, node, format!("`{model}#{attr}`")));
                    }
                }
                _ => {
                    self.out.dynamic_attr.insert(attr.to_string());
                    if safe {
                        (format!("(match &{r} {{ rp::Value::Nil => rp::Value::Nil, v => rp::ruby::attr_named(v, {})? }})", rust_str(attr)), Ty::Val)
                    } else {
                        (format!("rp::ruby::attr_named(&{r}, {})?", rust_str(attr)), Ty::Val)
                    }
                }
            },
            (method, _) => match &rty {
                Ty::Rec(model) if self.cfg.is_boundary(&format!("{model}#{method}")) => {
                    let a = self.args(f, args)?;
                    self.host(format!("{model}#{method}"), Some(r), a)
                }
                _ => return Err(self.unsupported(f, node, format!("`.{method}` with arguments"))),
            },
        })
    }

    fn alias(&mut self, f: &mut Fn<'_>, node: &Node, alias: &str, recv: Option<&str>, args: &[Node]) -> R<(String, Ty)> {
        self.out.aliases_used.insert(format!("{}.{alias}", recv.unwrap_or("")));
        Ok(match alias {
            "setting" => match args {
                [k] if str_of(k).is_some() => (format!("ctx.setting({})", rust_str(&str_of(k).unwrap_or_default())), Ty::Val),
                _ => return Err(self.unsupported(f, node, "Setting.get with a non-literal key")),
            },
            "current_user_id" => ("rp::Value::from(ctx.current_user_id)".into(), Ty::Val),
            "noop" => ("rp::Value::Nil".into(), Ty::Val),
            "find_by" => {
                let Some(model) = recv else {
                    return Err(self.unsupported(f, node, "find_by alias without a class receiver"));
                };
                return self.find_by(f, node, model, args);
            }
            other => return Err(self.unsupported(f, node, format!("unknown alias target {other}"))),
        })
    }

    fn find_by(&mut self, f: &mut Fn<'_>, node: &Node, model: &str, args: &[Node]) -> R<(String, Ty)> {
        let Some(t) = self.models.get(model) else {
            return Err(self.unsupported(f, node, format!("{model}.find_by: model outside the slice")));
        };
        let [Node::Kwargs(k)] = args else {
            return Err(self.unsupported(f, node, "find_by without keyword arguments"));
        };
        let mut preds = Vec::new();
        let mut binds = String::new();
        for p in &k.pairs {
            let Node::Pair(p) = p else {
                return Err(self.unsupported(f, node, "find_by splat"));
            };
            let Some(key) = str_of(&p.key) else {
                return Err(self.unsupported(f, node, "find_by non-literal key"));
            };
            if t.col(&key).is_none() {
                return Err(self.unsupported(f, node, format!("find_by({key}): no such column on {model}")));
            }
            let (v, _) = self.expr(f, &p.value)?;
            let n = format!("k{}", preds.len());
            let _ = write!(binds, "let {n} = {v}; ");
            preds.push(format!("({}, {n})", self.col_path(t, &key)));
        }
        Ok((
            format!("{{ {binds}rp::lifecycle::find_by_value(ctx, {}, &[{}])? }}", self.model_path(t), preds.join(", ")),
            Ty::Rec(model.to_string()),
        ))
    }
}

fn b_statements(node: &Node) -> Vec<Node> {
    match node {
        Node::Begin(b) => b.statements.clone(),
        Node::KwBegin(b) => b.statements.clone(),
        other => vec![other.clone()],
    }
}

fn seq(parts: &[String]) -> String {
    match parts {
        [] => "rp::Value::Nil".into(),
        [one] => one.clone(),
        many => {
            let mut s = String::from("{ ");
            for p in &many[..many.len() - 1] {
                let _ = write!(s, "let _ = {p}; ");
            }
            let _ = write!(s, "{} }}", many[many.len() - 1]);
            s
        }
    }
}

fn collect_locals(node: Option<&Node>, out: &mut BTreeMap<String, Ty>) {
    let Some(node) = node else { return };
    match node {
        Node::Lvasgn(a) => {
            out.entry(a.name.clone()).or_insert(Ty::Unknown);
            collect_locals(a.value.as_deref(), out);
        }
        Node::Begin(b) => b.statements.iter().for_each(|s| collect_locals(Some(s), out)),
        Node::KwBegin(b) => b.statements.iter().for_each(|s| collect_locals(Some(s), out)),
        Node::If(i) => {
            collect_locals(Some(&i.cond), out);
            collect_locals(i.if_true.as_deref(), out);
            collect_locals(i.if_false.as_deref(), out);
        }
        Node::IfMod(i) => {
            collect_locals(Some(&i.cond), out);
            collect_locals(i.if_true.as_deref(), out);
            collect_locals(i.if_false.as_deref(), out);
        }
        Node::And(a) => {
            collect_locals(Some(&a.lhs), out);
            collect_locals(Some(&a.rhs), out);
        }
        Node::Or(a) => {
            collect_locals(Some(&a.lhs), out);
            collect_locals(Some(&a.rhs), out);
        }
        Node::Send(s) => {
            collect_locals(s.recv.as_deref(), out);
            s.args.iter().for_each(|a| collect_locals(Some(a), out));
        }
        _ => {}
    }
}

#[must_use]
pub fn node_kind(n: &Node) -> String {
    let dbg = format!("{n:?}");
    dbg.split(['(', ' ', '{']).next().unwrap_or("?").to_string()
}

fn unreachable_def() -> ! {
    // Callers only pass nodes they matched as `Def` / receivers they checked
    // are `Some`; reaching here is a compiler bug, not an input condition.
    panic!("rails-compiler internal invariant violated")
}
