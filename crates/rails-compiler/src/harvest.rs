//! Source harvest: every class and module under the autoload roots, run
//! through ruff's ordered Rails arm.
//!
//! ruff's shipped model walk only discovers `class` nodes and records an
//! `included do … end` block as an opaque `ConcernRef::IncludedBlock`, so the
//! callbacks, associations and validations a concern contributes never reach
//! its IR. This pass closes that gap with ruff's own public API: each
//! module body and each `included` block body is handed to
//! [`ruff_ruby_spo::events::class_ore`], exactly as ruff does for a class
//! body. Nothing here re-implements the declaration walk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lib_ruby_parser::{Node, Parser, ParserOptions};
use ruff_ruby_spo::events::{ClassOre, RailsEvent, Symbols, class_ore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    Class,
    Module,
}

/// One class or module definition site.
#[derive(Debug, Clone)]
pub struct Unit {
    pub name: String,
    pub kind: UnitKind,
    /// The superclass as written (`ApplicationModel`), for classes.
    pub superclass: Option<String>,
    /// Lexical nesting at the definition, innermost first, for constant
    /// resolution (`Module.nesting`).
    pub nesting: Vec<String>,
    pub file: PathBuf,
    /// `extend ActiveSupport::Concern` in the module body.
    pub concern: bool,
    /// ruff's ordered Rails arm over the body.
    pub ore: ClassOre,
    /// ruff's ordered Rails arm over the `included do … end` block body, if
    /// any.
    pub included: Option<ClassOre>,
    /// `def`s in the body by name; the last definition wins, as in Ruby.
    pub defs: BTreeMap<String, Node>,
    /// The `included` block body, for anchor lookups.
    pub included_body: Option<Node>,
    pub body: Option<Node>,
}

/// A parsed source file.
#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub rel: String,
    pub src: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    #[must_use]
    pub fn line_of(&self, byte: usize) -> usize {
        match self.line_starts.binary_search(&byte) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }
}

#[derive(Debug, Default)]
pub struct Harvest {
    pub files: Vec<SourceFile>,
    /// Definition sites by qualified name, in load order.
    pub units: BTreeMap<String, Vec<Unit>>,
    pub parse_failures: Vec<String>,
    /// Files the parser reported an error-level diagnostic for, whether or not
    /// it recovered an AST (those with an AST are also in `files`).
    pub syntax_errors: Vec<String>,
    /// `inflect.acronym` words of the application's
    /// `config/initializers/inflections.rb`, keyed by their lower-case form.
    pub acronyms: BTreeMap<String, String>,
    /// `inflect.irregular 'singular', 'plural'` pairs of the same file, in
    /// file order.
    pub irregulars: Vec<(String, String)>,
}

pub fn rb_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            rb_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rb") {
            out.push(p);
        }
    }
}

#[must_use]
pub fn const_name(node: &Node) -> Option<String> {
    match node {
        Node::Const(c) => {
            let base = c.scope.as_deref().and_then(|s| match s {
                Node::Cbase(_) => None,
                other => const_name(other),
            });
            Some(base.map_or_else(|| c.name.clone(), |b| format!("{b}::{}", c.name)))
        }
        _ => None,
    }
}

fn statements(body: &Node) -> Vec<&Node> {
    match body {
        Node::Begin(b) => b.statements.iter().collect(),
        Node::KwBegin(b) => b.statements.iter().collect(),
        other => vec![other],
    }
}

fn is_concern_extend(stmt: &Node) -> bool {
    matches!(stmt, Node::Send(s) if s.recv.is_none() && s.method_name == "extend"
        && s.args.iter().any(|a| const_name(a).as_deref() == Some("ActiveSupport::Concern")))
}

fn included_block(stmt: &Node) -> Option<&Node> {
    match stmt {
        Node::Block(b) => match &*b.call {
            Node::Send(s) if s.recv.is_none() && s.method_name == "included" => b.body.as_deref(),
            _ => None,
        },
        _ => None,
    }
}

/// `::X` names the top-level constant only: Ruby does not search the lexical
/// scopes for it.
fn absolute<'n>(name: &'n str, nesting: &'n [String]) -> (&'n str, &'n [String]) {
    match name.strip_prefix("::") {
        Some(top) => (top, &[]),
        None => (name, nesting),
    }
}

/// The application's acronyms, read from the initializer Rails loads at boot.
/// Only literal `inflect.acronym 'X'` / `("X")` calls are read.
fn acronyms(source: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(source.join("config/initializers/inflections.rb")) else {
        return BTreeMap::new();
    };
    let call = regex::Regex::new(r#"^\s*\w+\.acronym\s*\(?\s*['"]([^'"]+)['"]"#).unwrap_or_else(|e| panic!("{e}"));
    text.lines()
        .filter_map(|l| call.captures(l))
        .map(|c| (c[1].to_lowercase(), c[1].to_string()))
        .collect()
}

/// The application's `inflect.irregular 'singular', 'plural'` calls, in file
/// order. Only literal string arguments are read.
fn irregulars(source: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(source.join("config/initializers/inflections.rb")) else {
        return Vec::new();
    };
    let call = regex::Regex::new(r#"^\s*\w+\.irregular\s*\(?\s*['"]([^'"]+)['"]\s*,\s*['"]([^'"]+)['"]"#)
        .unwrap_or_else(|e| panic!("{e}"));
    text.lines().filter_map(|l| call.captures(l)).map(|c| (c[1].to_string(), c[2].to_string())).collect()
}

impl Harvest {
    /// `singularize` under this application's irregulars.
    #[must_use]
    pub fn singular(&self, s: &str) -> String {
        crate::singular_with(s, &self.irregulars)
    }

    /// `camelize` under this application's acronyms.
    #[must_use]
    pub fn camelize(&self, s: &str) -> String {
        crate::camelize_with(s, &self.acronyms)
    }

    /// Parse every `*.rb` under `roots` (relative to `source`).
    pub fn collect(source: &Path, roots: &[String], namespace: &str) -> Self {
        let mut h = Self { acronyms: acronyms(source), irregulars: irregulars(source), ..Self::default() };
        let mut syms = Symbols::default();
        let mut paths = Vec::new();
        for r in roots {
            rb_files(&source.join(r), &mut paths);
        }
        paths.sort();
        paths.dedup();
        h.collect_paths(source, &paths, namespace, &mut syms);
        h
    }

    /// Parse exactly `paths` (absolute or under `source`) with the same walk as
    /// [`Self::collect`]: the units those files define, and nothing else.
    pub fn collect_files(source: &Path, paths: &[PathBuf], namespace: &str) -> Self {
        let mut h = Self { acronyms: acronyms(source), irregulars: irregulars(source), ..Self::default() };
        h.collect_paths(source, paths, namespace, &mut Symbols::default());
        h
    }

    fn collect_paths(&mut self, source: &Path, paths: &[PathBuf], namespace: &str, syms: &mut Symbols) {
        let h = self;
        for path in paths {
            let Ok(src) = std::fs::read_to_string(path) else { continue };
            let rel = path
                .strip_prefix(source)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            let parser = Parser::new(
                src.as_bytes().to_vec(),
                ParserOptions {
                    buffer_name: rel.clone(),
                    ..Default::default()
                },
            );
            let parsed = parser.do_parse();
            let has_error = parsed.diagnostics.iter().any(|d| d.level == lib_ruby_parser::ErrorLevel::Error);
            if has_error {
                h.syntax_errors.push(rel.clone());
            }
            // No AST and no error: an empty program (nothing, whitespace or
            // comments only), which is valid Ruby with no units.
            let ast = match parsed.ast {
                Some(ast) => Some(ast),
                None if has_error => {
                    h.parse_failures.push(rel);
                    continue;
                }
                None => None,
            };
            let mut line_starts = vec![0];
            line_starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
            h.files.push(SourceFile {
                path: path.clone(),
                rel,
                src,
                line_starts,
            });
            if let Some(ast) = ast {
                h.walk(&ast, &[], path, namespace, syms);
            }
        }
    }

    fn walk(&mut self, node: &Node, nesting: &[String], file: &Path, ns: &str, syms: &mut Symbols) {
        match node {
            Node::Begin(b) => {
                for s in &b.statements {
                    self.walk(s, nesting, file, ns, syms);
                }
            }
            Node::Class(c) => {
                let local = const_name(&c.name).unwrap_or_default();
                let name = qualify(nesting, &local);
                let superclass = c.superclass.as_deref().and_then(const_name);
                self.unit(UnitKind::Class, name, superclass, c.body.as_deref(), nesting, file, ns, syms);
            }
            Node::Module(m) => {
                let local = const_name(&m.name).unwrap_or_default();
                let name = qualify(nesting, &local);
                self.unit(UnitKind::Module, name, None, m.body.as_deref(), nesting, file, ns, syms);
            }
            _ => {}
        }
    }

    #[expect(clippy::too_many_arguments, reason = "one call site; the arguments are the walk state")]
    fn unit(
        &mut self,
        kind: UnitKind,
        name: String,
        superclass: Option<String>,
        body: Option<&Node>,
        outer: &[String],
        file: &Path,
        ns: &str,
        syms: &mut Symbols,
    ) {
        let iri = format!("{ns}:{name}");
        let empty = Node::Nil(lib_ruby_parser::nodes::Nil {
            expression_l: lib_ruby_parser::Loc { begin: 0, end: 0 },
        });
        let ore = class_ore(&iri, &name, body.unwrap_or(&empty), syms);
        let stmts = body.map(statements).unwrap_or_default();
        let concern = stmts.iter().any(|s| is_concern_extend(s));
        let included_body = stmts.iter().find_map(|s| included_block(s)).cloned();
        let included = included_body
            .as_ref()
            .map(|b| class_ore(&format!("{iri}#included"), &name, b, syms));
        let mut defs = BTreeMap::new();
        for s in &stmts {
            collect_defs(s, &mut defs);
        }
        let mut nesting = vec![name.clone()];
        nesting.extend(outer.iter().rev().cloned());
        self.units.entry(name.clone()).or_default().push(Unit {
            name: name.clone(),
            kind,
            superclass,
            nesting: nesting.clone(),
            file: file.to_path_buf(),
            concern,
            ore,
            included,
            defs,
            included_body,
            body: body.cloned(),
        });
        // Nested definitions (`class A; module B; …`).
        let mut inner = outer.to_vec();
        inner.push(name.clone());
        for s in stmts {
            if matches!(s, Node::Class(_) | Node::Module(_)) {
                self.walk(s, &inner, file, ns, syms);
            }
        }
    }

    #[must_use]
    pub fn file(&self, path: &Path) -> Option<&SourceFile> {
        self.files.iter().find(|f| f.path == path)
    }

    /// `path:line` for a byte anchor in a unit's file.
    #[must_use]
    pub fn loc(&self, unit: &Unit, anchor: usize) -> String {
        match self.file(&unit.file) {
            Some(f) => format!("{}:{}", f.rel, f.line_of(anchor)),
            None => format!("{}@{anchor}", unit.file.display()),
        }
    }

    /// [`Self::resolve`] plus every candidate name tried before the answer (all
    /// of them when nothing resolves) that is not a harvested unit: the names
    /// whose later appearance would change the answer. Same order as `resolve`.
    #[must_use]
    pub fn resolve_with_absent(&self, name: &str, nesting: &[String]) -> (Option<String>, Vec<String>) {
        let (name, nesting) = absolute(name, nesting);
        let mut absent = Vec::new();
        for scope in nesting {
            let mut parts: Vec<&str> = scope.split("::").collect();
            loop {
                let candidate = if parts.is_empty() { name.to_string() } else { format!("{}::{name}", parts.join("::")) };
                if self.units.contains_key(&candidate) {
                    return (Some(candidate), absent);
                }
                absent.push(candidate);
                if parts.pop().is_none() {
                    break;
                }
            }
        }
        if self.units.contains_key(name) {
            return (Some(name.to_string()), absent);
        }
        absent.push(name.to_string());
        (None, absent)
    }

    /// Resolve a constant reference the way Ruby + an autoloader would: each
    /// lexical scope outward, then the top level.
    #[must_use]
    pub fn resolve(&self, name: &str, nesting: &[String]) -> Option<String> {
        let (name, nesting) = absolute(name, nesting);
        for scope in nesting {
            let mut parts: Vec<&str> = scope.split("::").collect();
            loop {
                let candidate = if parts.is_empty() {
                    name.to_string()
                } else {
                    format!("{}::{name}", parts.join("::"))
                };
                if self.units.contains_key(&candidate) {
                    return Some(candidate);
                }
                if parts.pop().is_none() {
                    break;
                }
            }
        }
        self.units.contains_key(name).then(|| name.to_string())
    }
}

fn qualify(nesting: &[String], local: &str) -> String {
    match nesting.last() {
        Some(outer) => format!("{outer}::{local}"),
        None => local.to_string(),
    }
}

fn collect_defs(stmt: &Node, out: &mut BTreeMap<String, Node>) {
    match stmt {
        Node::Def(d) => {
            out.insert(d.name.clone(), stmt.clone());
        }
        // `private def foo` / `protected def foo`.
        Node::Send(s) if s.recv.is_none() && matches!(s.method_name.as_str(), "private" | "protected" | "public") => {
            for a in &s.args {
                collect_defs(a, out);
            }
        }
        _ => {}
    }
}

/// The Send node a [`RailsEvent`] was harvested from, and the block passed
/// to it (`after_initialize do … end`), if any.
#[must_use]
pub fn decl_node<'a>(body: &'a Node, ev: &RailsEvent) -> Option<(&'a Node, Option<&'a Node>)> {
    let anchor = ev.anchor as usize;
    statements(body).into_iter().find_map(|s| send_at(s, anchor))
}

fn send_at(node: &Node, anchor: usize) -> Option<(&Node, Option<&Node>)> {
    match node {
        Node::Send(s) if s.expression_l.begin == anchor => Some((node, None)),
        Node::Block(b) if b.expression_l.begin == anchor || b.call.expression().begin == anchor => {
            Some((&b.call, b.body.as_deref()))
        }
        _ => None,
    }
}
