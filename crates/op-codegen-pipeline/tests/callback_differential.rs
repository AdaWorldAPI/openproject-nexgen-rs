//! The AR arm of the behaviour differential: the callbacks and validations
//! ActiveRecord actually ran when real OpenProject executed
//! `WorkPackages::CreateService` (`oracle/work-package-service-trace.json`),
//! compared with the declarations ruff harvests from the Rails class bodies
//! (`ruff_ruby_spo::events`, ordered, with `if:` / `unless:` / `on:` options).
//!
//! The contract arm (`writable:` guards) is a separate differential; see
//! `oracle/README.md` for how the two relate.
//!
//! Every fired callback is either MATCHED to a declaration by
//! `(chain, kind, filter)` or assigned exactly one [`Slag`] reason; every
//! declared callback that never fired is assigned exactly one [`Silent`]
//! reason. Both enums are closed: there is no catch-all variant, and each
//! variant's doc names the stage that would have to supply the missing fact.
//!
//! Needs `OPENPROJECT_SRC` (a checkout at the trace's revision). Without it the
//! test prints UNVERIFIED and passes, unless `REQUIRE_SOURCES=1`, which makes a
//! missing source a failure.
//!
//! Evidence the harvest cannot give (where a filter is defined, which
//! associations exist anywhere in the tree) comes from a plain line scan of the
//! whole checkout. The scan only chooses between slag reasons; it never turns a
//! fired callback into a match.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use lib_ruby_parser::{Node, Parser, ParserOptions};
use ruff_ruby_spo::events::{class_ore, Symbols};
use ruff_ruby_spo::Declaration;
use ruff_spo_triplet::{AssocKind, ConcernKind, ValidationKind};
use serde_json::Value;

/// Distinct journal entries matched to a declaration, measured on the committed trace.
const MATCHED_FLOOR: usize = 24;

/// Directories (relative to the checkout) a Zeitwerk constant is looked up in.
const WALKED_ROOTS: &[&str] = &["app/models", "app/models/concerns", "lib"];

// ─────────────────────────────────────────────────────────────────────────
// Reasons
// ─────────────────────────────────────────────────────────────────────────

/// Why a callback that fired has no declaration in the harvested ancestors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Slag {
    /// `autosave_associated_records_for_X` / `validate_associated_records_for_X`
    /// where `X` is an association the harvest DID find. Rails writes these
    /// itself for every association. No new fact is needed: a Rails polyfill
    /// (association declaration -> autosave/validate callback pair) derives them.
    AssociationAutosave,
    /// The same Rails-generated pair, but the association is declared in a
    /// file under `modules/` that mentions the model (an engine patch the walk
    /// does not enter). Stage: codegraph config, walk `modules/*/app` and the
    /// `to_prepare` patch includes; then the same Rails polyfill as
    /// [`Slag::AssociationAutosave`].
    AssociationFromModule,
    /// The same pair, but the association is declared in an in-tree plugin
    /// (`lib_static/`, `vendored-plugins/`) by a macro the model calls
    /// (`acts_as_attachable`, `acts_as_journalized`, ...). Stage: ruff
    /// extension (expand the `acts_as_*` macro) plus the Rails polyfill.
    AssociationFromPlugin,
    /// The same pair, but no association line for `X` that concerns the model
    /// exists in the checkout. These are the hidden join associations of
    /// `has_and_belongs_to_many`, associations a gem macro adds
    /// (closure_tree, paper_trail), a declaration written in a form the line
    /// scan does not read (`has_many(:x`), and a name that only an unrelated
    /// walked-root class declares. Stage: Rails/gem polyfill.
    AssociationNotInCheckout,
    /// The filter has a `def` in a file inside the walked roots that is not in
    /// the harvested ancestor chain, so something registered it without a
    /// callback macro in a walked class body (a macro call, `class_eval`, an
    /// include through `included do` we did not follow). Stage: ruff extension
    /// (expand the registering macro).
    DefinedInWalkedRootsNotDeclared,
    /// The filter's only `def` is under `modules/` (an OpenProject engine that
    /// patches `WorkPackage`). Stage: codegraph config (walk `modules/*`).
    DefinedInModule,
    /// The filter's only `def` is under `lib_static/` or `vendored-plugins/`
    /// (in-tree plugins such as `acts_as_customizable`). Stage: codegraph config
    /// (walk the plugins) plus a ruff extension for the `acts_as_*` macro that
    /// registers it.
    DefinedInPlugin,
    /// No `def` of the filter exists anywhere in the checkout: it comes from
    /// ActiveRecord, ActiveModel or a gem (closure_tree, acts_as_list, ...).
    /// Stage: Rails/gem polyfill (a table of what each framework call
    /// registers). ruff and codegraph cannot see it in OPENPROJECT_SRC.
    FromGem,
    /// A `validates` validator (`validates:KIND:attrs`) with no matching
    /// declaration whose attribute is validated by a `validates` line under
    /// `modules/`. Stage: codegraph config (walk `modules/*`).
    ValidatorFromModule,
    /// A `validates` validator with no matching declaration and no `validates`
    /// line naming its attribute anywhere in the checkout: generated by Rails
    /// (e.g. `belongs_to` required) or a gem. Stage: Rails polyfill.
    ValidatorFromFramework,
    /// A `validates_with` validator with no matching declaration. Stage: ruff
    /// extension if the class is declared in the walked roots (`with_options`,
    /// `included do` inside a non-followed concern), codegraph config otherwise.
    ValidatesWithUndeclared,
    /// The record's class has no file to harvest: it is generated at runtime
    /// (closure_tree's `WorkPackageHierarchy`). Stage: Rails/gem polyfill.
    ModelWithoutClassFile,
}

impl Slag {
    fn all() -> &'static [Slag] {
        &[
            Slag::AssociationAutosave,
            Slag::AssociationFromModule,
            Slag::AssociationFromPlugin,
            Slag::AssociationNotInCheckout,
            Slag::DefinedInWalkedRootsNotDeclared,
            Slag::DefinedInModule,
            Slag::DefinedInPlugin,
            Slag::FromGem,
            Slag::ValidatorFromModule,
            Slag::ValidatorFromFramework,
            Slag::ValidatesWithUndeclared,
            Slag::ModelWithoutClassFile,
        ]
    }
}

/// Why a declared callback never fired in any recorded create case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Silent {
    /// The declaration has no symbol filter (`before_save { ... }`, a lambda, an
    /// object). The journal wraps only symbol filters and validators, so such a
    /// callback can never appear in it. Stage: oracle (journal procs).
    BlockOrLambdaFilter,
    /// The record never ran this chain (`destroy`, `update`, `touch`, ...): a
    /// create does not exercise it. Stage: more oracle cases.
    ChainNotExercised,
    /// `on:` names an event this case set never triggers (`on: :update`) and the
    /// chain itself ran. Also `after_update_commit` / `after_destroy_commit`.
    /// Stage: more oracle cases.
    OnOptionNotExercised,
    /// `if:` / `unless:` (or an `on:` that includes create) is present: the
    /// condition may have been false. Whether it was is a fact the harvest does
    /// not evaluate. Stage: ruff extension (lower the condition to a guard
    /// expression, as the contract arm does for `writable:`).
    ConditionalOptionNotEvaluated,
    /// No condition, an exercised chain, a symbol filter, and still nothing
    /// fired. A real discrepancy (an override, a `skip_callback`, or a
    /// declaration the class never reaches). Stage: none, investigate.
    Unexplained,
}

impl Silent {
    fn all() -> &'static [Silent] {
        &[
            Silent::BlockOrLambdaFilter,
            Silent::ChainNotExercised,
            Silent::OnOptionNotExercised,
            Silent::ConditionalOptionNotEvaluated,
            Silent::Unexplained,
        ]
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Source access
// ─────────────────────────────────────────────────────────────────────────

fn source() -> Option<PathBuf> {
    match std::env::var("OPENPROJECT_SRC") {
        Ok(s) if !s.is_empty() => Some(PathBuf::from(s)),
        _ => {
            assert!(
                std::env::var("REQUIRE_SOURCES").as_deref() != Ok("1"),
                "REQUIRE_SOURCES=1 but OPENPROJECT_SRC is not set"
            );
            eprintln!("UNVERIFIED: callback_differential needs OPENPROJECT_SRC");
            None
        }
    }
}

fn trace() -> Value {
    let p =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/work-package-service-trace.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("committed trace"))
        .expect("trace is JSON")
}

// ─────────────────────────────────────────────────────────────────────────
// Harvest: classes and concern modules -> ordered declarations
// ─────────────────────────────────────────────────────────────────────────

/// One class or module in a file, with every declaration ruff harvests from it.
/// For a module, the declarations include those inside `included do ... end`.
struct Unit {
    name: String,
    superclass: Option<String>,
    decls: Vec<Declaration>,
}

fn const_name(node: &Node) -> Option<String> {
    match node {
        Node::Const(c) => {
            let base = c.scope.as_deref().and_then(const_name);
            Some(base.map_or_else(|| c.name.clone(), |b| format!("{b}::{}", c.name)))
        }
        _ => None,
    }
}

/// Bodies of `included do ... end` blocks that are direct statements of a module body.
fn included_bodies(body: &Node) -> Vec<&Node> {
    let stmts: Vec<&Node> = match body {
        Node::Begin(b) => b.statements.iter().collect(),
        single => vec![single],
    };
    let mut out = Vec::new();
    for s in stmts {
        if let Node::Block(blk) = s {
            if let Node::Send(call) = &*blk.call {
                if call.recv.is_none() && call.method_name == "included" {
                    if let Some(b) = blk.body.as_deref() {
                        out.push(b);
                    }
                }
            }
        }
    }
    out
}

fn collect_units(node: &Node, ns: &[String], syms: &mut Symbols, out: &mut Vec<Unit>) {
    match node {
        Node::Begin(b) => {
            for s in &b.statements {
                collect_units(s, ns, syms, out);
            }
        }
        Node::Module(m) => {
            let mut nested = ns.to_vec();
            let local = const_name(&m.name).unwrap_or_default();
            nested.push(local);
            let qualified = nested.join("::");
            if let Some(body) = m.body.as_deref() {
                let mut decls: Vec<Declaration> = class_ore(&qualified, &qualified, body, syms)
                    .declarations
                    .into_iter()
                    .map(|e| e.decl)
                    .collect();
                for inc in included_bodies(body) {
                    decls.extend(
                        class_ore(&qualified, &qualified, inc, syms)
                            .declarations
                            .into_iter()
                            .map(|e| e.decl),
                    );
                }
                out.push(Unit {
                    name: qualified,
                    superclass: None,
                    decls,
                });
                collect_units(body, &nested, syms, out);
            }
        }
        Node::Class(c) => {
            let local = const_name(&c.name).unwrap_or_default();
            let qualified = if ns.is_empty() {
                local.clone()
            } else {
                format!("{}::{local}", ns.join("::"))
            };
            if let Some(body) = c.body.as_deref() {
                let decls = class_ore(&qualified, &qualified, body, syms)
                    .declarations
                    .into_iter()
                    .map(|e| e.decl)
                    .collect();
                out.push(Unit {
                    name: qualified,
                    superclass: c.superclass.as_deref().and_then(const_name),
                    decls,
                });
                let mut nested = ns.to_vec();
                nested.push(local);
                collect_units(body, &nested, syms, out);
            }
        }
        _ => {}
    }
}

fn parse_units(src: &str) -> Vec<Unit> {
    let options = ParserOptions {
        buffer_name: "<callback-differential>".to_string(),
        ..Default::default()
    };
    let Some(ast) = Parser::new(src.as_bytes().to_vec(), options).do_parse().ast else {
        return Vec::new();
    };
    let mut syms = Symbols::default();
    let mut out = Vec::new();
    collect_units(&ast, &[], &mut syms, &mut out);
    out
}

/// `WorkPackage::SchedulingRules` -> `work_package/scheduling_rules`.
fn underscore(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == ':' {
            if out.ends_with('/') {
                continue;
            }
            out.push('/');
            continue;
        }
        if c.is_uppercase() {
            let prev = if i > 0 { Some(chars[i - 1]) } else { None };
            let next = chars.get(i + 1).copied();
            let boundary = match prev {
                Some(p) if p.is_lowercase() || p.is_ascii_digit() => true,
                Some(p) if p.is_uppercase() => next.is_some_and(char::is_lowercase),
                _ => false,
            };
            if boundary && !out.ends_with('/') && !out.is_empty() {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A class or module found in a walked root and its harvested declarations.
struct Ancestor {
    name: String,
    file: String,
    decls: Vec<Declaration>,
}

/// `(name, superclass, declarations)` of each class or module in one file.
type FileUnits = Vec<(String, Option<String>, Vec<Declaration>)>;

struct Harvest {
    src: PathBuf,
    cache: BTreeMap<String, FileUnits>,
    /// Include targets the walk could not find a unit for, with the includer.
    unresolved: Vec<(String, String)>,
}

impl Harvest {
    fn new(src: &Path) -> Self {
        Harvest {
            src: src.to_path_buf(),
            cache: BTreeMap::new(),
            unresolved: Vec::new(),
        }
    }

    fn load(&mut self, rel: &str) -> &FileUnits {
        if !self.cache.contains_key(rel) {
            let text = std::fs::read_to_string(self.src.join(rel)).unwrap_or_default();
            let units = parse_units(&text)
                .into_iter()
                .map(|u| (u.name, u.superclass, u.decls))
                .collect();
            self.cache.insert(rel.to_string(), units);
        }
        &self.cache[rel]
    }

    /// Find constant `name` in the walked roots.
    fn find(&mut self, name: &str) -> Option<(String, Option<String>, Vec<Declaration>)> {
        let path = format!("{}.rb", underscore(name));
        for root in WALKED_ROOTS {
            let rel = format!("{root}/{path}");
            if self.src.join(&rel).is_file() {
                let hit = self
                    .load(&rel)
                    .iter()
                    .find(|(n, _, _)| n == name)
                    .map(|(_, s, d)| (s.clone(), d.clone()));
                if let Some((s, d)) = hit {
                    return Some((rel, s, d));
                }
            }
        }
        None
    }

    /// Resolve `module` as written inside `from`: innermost enclosing namespace first.
    fn resolve(
        &mut self,
        from: &str,
        module: &str,
    ) -> Option<(String, String, Option<String>, Vec<Declaration>)> {
        let module = module.trim_start_matches("::");
        let mut ns: Vec<&str> = from.split("::").collect();
        loop {
            let candidate = if ns.is_empty() {
                module.to_string()
            } else {
                format!("{}::{module}", ns.join("::"))
            };
            if let Some((file, s, d)) = self.find(&candidate) {
                return Some((candidate, file, s, d));
            }
            ns.pop()?;
        }
    }

    /// The class, its included/prepended concerns and its superclass chain, in
    /// the order the walk meets them. Cycle-safe.
    fn linearize(&mut self, root: &str) -> Vec<Ancestor> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        self.visit(root, &mut seen, &mut out);
        out
    }

    fn visit(&mut self, name: &str, seen: &mut BTreeSet<String>, out: &mut Vec<Ancestor>) {
        if !seen.insert(name.to_string()) {
            return;
        }
        let Some((file, superclass, decls)) = self.find(name) else {
            return;
        };
        let includes: Vec<String> = decls
            .iter()
            .filter_map(|d| match d {
                Declaration::Concern(c)
                    if matches!(c.kind, ConcernKind::Include | ConcernKind::Prepend)
                        && !c.module.is_empty() =>
                {
                    Some(c.module.clone())
                }
                _ => None,
            })
            .collect();
        out.push(Ancestor {
            name: name.to_string(),
            file,
            decls,
        });
        for m in includes {
            match self.resolve(name, &m) {
                Some((resolved, ..)) => self.visit(&resolved, seen, out),
                None => self.unresolved.push((name.to_string(), m)),
            }
        }
        if let Some(sup) = superclass {
            match self.resolve(name, &sup) {
                Some((resolved, ..)) => self.visit(&resolved, seen, out),
                None => self.unresolved.push((name.to_string(), format!("< {sup}"))),
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Declared callbacks
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Filter {
    /// A symbol filter: `before_save :foo`, `validate :foo`.
    Method(String),
    /// One attribute of a `validates` call, for one validator kind.
    Validator { kind: String, attr: String },
    /// `validates_with Klass`.
    ValidatesWith(String),
    /// A block, lambda or object filter. The journal cannot name it.
    Block,
}

#[derive(Debug, Clone)]
struct Declared {
    chain: String,
    kind: String,
    filter: Filter,
    origin: String,
    options: Vec<(String, String)>,
    /// `on:` implied by the macro name (`after_update_commit`).
    implied_on: Option<&'static str>,
}

const VALIDATES_COMMON_OPTIONS: &[&str] = &[
    "allow_nil",
    "allow_blank",
    "if",
    "unless",
    "on",
    "message",
    "strict",
    "prepend",
];

/// `before_validation` -> (`before`, `validation`); `after_create_commit` ->
/// (`after`, `commit`, implied `create`).
fn split_phase(phase: &str) -> Option<(String, String, Option<&'static str>)> {
    let (kind, rest) = phase.split_once('_')?;
    if !matches!(kind, "before" | "after" | "around") {
        return None;
    }
    let (chain, implied) = match rest {
        "create_commit" => ("commit", Some("create")),
        "update_commit" => ("commit", Some("update")),
        "destroy_commit" => ("commit", Some("destroy")),
        "save_commit" => ("commit", Some("save")),
        other => (other, None),
    };
    Some((kind.to_string(), chain.to_string(), implied))
}

fn declared_of(origin: &str, decls: &[Declaration]) -> Vec<Declared> {
    let mut out = Vec::new();
    for d in decls {
        match d {
            Declaration::Callback(cb) => {
                let Some((kind, chain, implied_on)) = split_phase(&cb.phase) else {
                    continue;
                };
                let filter = if cb.target.starts_with('<') {
                    Filter::Block
                } else {
                    Filter::Method(cb.target.clone())
                };
                out.push(Declared {
                    chain,
                    kind,
                    filter,
                    origin: origin.to_string(),
                    options: cb.options.clone(),
                    implied_on,
                });
            }
            Declaration::Validation(v) => {
                let (filter, ok) = match v.kind {
                    ValidationKind::Validate => (
                        if v.target.starts_with('<') {
                            Filter::Block
                        } else {
                            Filter::Method(v.target.clone())
                        },
                        true,
                    ),
                    ValidationKind::ValidatesWith => {
                        (Filter::ValidatesWith(v.target.clone()), true)
                    }
                    ValidationKind::ValidatesEach => (Filter::Block, true),
                    ValidationKind::Validates => {
                        // One entry per validator kind named in the options.
                        for (k, _) in &v.options {
                            if VALIDATES_COMMON_OPTIONS.contains(&k.as_str()) {
                                continue;
                            }
                            out.push(Declared {
                                chain: "validate".into(),
                                kind: "before".into(),
                                filter: Filter::Validator {
                                    kind: k.clone(),
                                    attr: v.target.clone(),
                                },
                                origin: origin.to_string(),
                                options: v.options.clone(),
                                implied_on: None,
                            });
                        }
                        (Filter::Block, false)
                    }
                    ValidationKind::ValidatesAssociated | ValidationKind::Normalizes => {
                        (Filter::Block, false)
                    }
                };
                if ok {
                    out.push(Declared {
                        chain: "validate".into(),
                        kind: "before".into(),
                        filter,
                        origin: origin.to_string(),
                        options: v.options.clone(),
                        implied_on: None,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

fn association_names(ancestors: &[Ancestor]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for a in ancestors {
        for d in &a.decls {
            if let Declaration::Association(assoc) = d {
                if matches!(
                    assoc.kind,
                    AssocKind::BelongsTo
                        | AssocKind::HasMany
                        | AssocKind::HasOne
                        | AssocKind::HasAndBelongsToMany
                ) {
                    out.insert(assoc.name.clone());
                }
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Fired callbacks
// ─────────────────────────────────────────────────────────────────────────

/// What the journal can name: never a block.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum FiredFilter {
    Method(String),
    Validator { kind: String, attr: String },
    ValidatesWith(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Fired {
    model: String,
    chain: String,
    kind: String,
    filter: FiredFilter,
    /// The journal's own label, for printing.
    label: String,
}

fn parse_fired(model: &str, phase: &str, label: &str) -> Option<Vec<Fired>> {
    let (kind, chain) = phase.split_once('_')?;
    let mk = |filter| Fired {
        model: model.to_string(),
        chain: chain.to_string(),
        kind: kind.to_string(),
        filter,
        label: label.to_string(),
    };
    if let Some(rest) = label.strip_prefix("validates_with:") {
        return Some(vec![mk(FiredFilter::ValidatesWith(rest.to_string()))]);
    }
    if let Some(rest) = label.strip_prefix("validates:") {
        let (vk, attrs) = rest.split_once(':')?;
        return Some(
            attrs
                .split(',')
                .map(|a| {
                    mk(FiredFilter::Validator {
                        kind: vk.to_string(),
                        attr: a.to_string(),
                    })
                })
                .collect(),
        );
    }
    Some(vec![mk(FiredFilter::Method(label.to_string()))])
}

/// The distinct fired callbacks over every recorded case, grouped by the
/// journal entry they came from (one entry may expand to several validator
/// attributes). Returns `(entries, distinct)`: `distinct` keys the entry by
/// `(model, phase, label)`.
/// `(model, phase, journal label)`.
type EntryKey = (String, String, String);

fn fired_entries() -> (usize, BTreeMap<EntryKey, Vec<Fired>>) {
    let t = trace();
    let mut total = 0usize;
    let mut distinct = BTreeMap::new();
    for case in t["cases"].as_array().expect("cases") {
        for e in case["journal"].as_array().expect("journal") {
            total += 1;
            let model = e["model"].as_str().expect("model");
            let phase = e["phase"].as_str().expect("phase");
            let label = e["callback"].as_str().expect("callback");
            let key = (model.to_string(), phase.to_string(), label.to_string());
            if let Some(f) = parse_fired(model, phase, label) {
                distinct.insert(key, f);
            }
        }
    }
    (total, distinct)
}

// ─────────────────────────────────────────────────────────────────────────
// Evidence scan (chooses between slag reasons, never creates a match)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Bucket {
    /// `app/` or `lib/`: the roots the harvest walks.
    Walked,
    /// `modules/` or `extensions/`: OpenProject engines.
    Module,
    /// `lib_static/` or `vendored-plugins/`: in-tree plugins.
    Plugin,
}

fn bucket(rel: &str) -> Bucket {
    if rel.starts_with("modules/") || rel.starts_with("extensions/") {
        Bucket::Module
    } else if rel.starts_with("lib_static/") || rel.starts_with("vendored-plugins/") {
        Bucket::Plugin
    } else {
        Bucket::Walked
    }
}

#[derive(Default)]
struct Evidence {
    /// method name -> files with a `def` of it
    defs: BTreeMap<String, BTreeSet<String>>,
    /// association name -> files with a `has_many` / `belongs_to` / ... line
    assocs: BTreeMap<String, BTreeSet<String>>,
    /// attribute -> files with a `validates` / `validates_*_of` line naming it
    validates: BTreeMap<String, BTreeSet<String>>,
    /// file -> the model names its text mentions (only files that carry a
    /// line above are recorded)
    mentions: BTreeMap<String, BTreeSet<String>>,
}

impl Evidence {
    /// The files in `files` that can concern `model` and are outside its
    /// harvested chain: an in-tree plugin (a macro file any model may call) or
    /// a file whose text names the model. The scan reads names, not classes, so
    /// this keeps `has_many :children` in `menu_item.rb` from counting for
    /// `WorkPackage`.
    fn relevant<'a>(
        &self,
        files: Option<&'a BTreeSet<String>>,
        model: &str,
        ctx: &ModelCtx,
    ) -> Vec<&'a String> {
        files
            .into_iter()
            .flatten()
            .filter(|f| {
                !ctx.ancestor_files.contains(*f)
                    && (bucket(f) == Bucket::Plugin
                        || self.mentions.get(*f).is_some_and(|m| m.contains(model)))
            })
            .collect()
    }
}

fn ident(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '?' | '!'))
        .collect()
}

/// `validates :a, :b, presence: true` and `validates_numericality_of :a, ...`.
fn validates_args(t: &str) -> Option<&str> {
    if let Some(rest) = t.strip_prefix("validates ") {
        return Some(rest);
    }
    let rest = t.strip_prefix("validates_")?;
    let (head, args) = rest.split_once(' ')?;
    head.ends_with("_of").then_some(args)
}

fn scan_file(rel: &str, text: &str, models: &BTreeSet<String>, ev: &mut Evidence) {
    let mut hit = false;
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("def ") {
            let rest = rest.strip_prefix("self.").unwrap_or(rest);
            let name = ident(rest);
            if !name.is_empty() {
                ev.defs.entry(name).or_default().insert(rel.to_string());
                hit = true;
            }
        } else if let Some(rest) = [
            "has_many ",
            "has_one ",
            "belongs_to ",
            "has_and_belongs_to_many ",
        ]
        .iter()
        .find_map(|m| t.strip_prefix(m))
        {
            if let Some(sym) = rest.trim_start().strip_prefix(':') {
                let name = ident(sym);
                if !name.is_empty() {
                    ev.assocs.entry(name).or_default().insert(rel.to_string());
                    hit = true;
                }
            }
        } else if let Some(rest) = validates_args(t) {
            for tok in rest.split(',') {
                let Some(sym) = tok.trim().strip_prefix(':') else {
                    break;
                };
                let name = ident(sym);
                // `key: value` pairs end the attribute list.
                if name.is_empty() || sym[name.len()..].starts_with(':') {
                    break;
                }
                ev.validates
                    .entry(name)
                    .or_default()
                    .insert(rel.to_string());
                hit = true;
            }
        }
    }
    if hit {
        let named: BTreeSet<String> = models
            .iter()
            .filter(|m| text.contains(m.as_str()))
            .cloned()
            .collect();
        ev.mentions.insert(rel.to_string(), named);
    }
}

/// Scan every `.rb` file of the checkout except tests, assets and installed
/// gems (`vendor/`: its presence varies between checkouts, and a filter defined
/// only there is, by definition, not OpenProject's own).
fn scan_tree(src: &Path, models: &BTreeSet<String>) -> Evidence {
    const SKIP: &[&str] = &[
        ".git",
        "node_modules",
        "spec",
        "frontend",
        "docs",
        "tmp",
        "log",
        "public",
        "lookbook",
        "features",
        "db",
        "config",
        "packaging",
        "docker",
        "help",
        "files",
        "vendor",
    ];
    fn walk(root: &Path, dir: &Path, models: &BTreeSet<String>, ev: &mut Evidence) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    walk(root, &path, models, ev);
                }
            } else if name.ends_with(".rb") {
                if let (Ok(text), Ok(rel)) =
                    (std::fs::read_to_string(&path), path.strip_prefix(root))
                {
                    scan_file(&rel.to_string_lossy(), &text, models, ev);
                }
            }
        }
    }
    let mut ev = Evidence::default();
    walk(src, src, models, &mut ev);
    ev
}

// ─────────────────────────────────────────────────────────────────────────
// Matching and classification
// ─────────────────────────────────────────────────────────────────────────

fn filter_matches(d: &Filter, f: &FiredFilter) -> bool {
    match (d, f) {
        (Filter::Method(a), FiredFilter::Method(b)) => a == b,
        (
            Filter::Validator { kind: k1, attr: a1 },
            FiredFilter::Validator { kind: k2, attr: a2 },
        ) => k1 == k2 && a1 == a2,
        (Filter::ValidatesWith(a), FiredFilter::ValidatesWith(b)) => {
            let a = a.trim_start_matches("::");
            b == a || b.ends_with(&format!("::{a}"))
        }
        _ => false,
    }
}

fn declared_matches(d: &Declared, f: &Fired) -> bool {
    d.chain == f.chain && d.kind == f.kind && filter_matches(&d.filter, &f.filter)
}

/// The negative control: every declared filter spelled backwards.
fn reversed(d: &Declared) -> Declared {
    let rev = |s: &str| s.chars().rev().collect::<String>();
    let mut out = d.clone();
    out.filter = match &d.filter {
        Filter::Method(n) => Filter::Method(rev(n)),
        Filter::Validator { kind, attr } => Filter::Validator {
            kind: rev(kind),
            attr: rev(attr),
        },
        Filter::ValidatesWith(n) => Filter::ValidatesWith(rev(n)),
        Filter::Block => Filter::Block,
    };
    out
}

struct ModelCtx {
    declared: Vec<Declared>,
    assocs: BTreeSet<String>,
    ancestor_files: BTreeSet<String>,
    has_file: bool,
}

fn classify(f: &Fired, ctx: &ModelCtx, ev: &Evidence) -> (Slag, String) {
    if !ctx.has_file {
        return (
            Slag::ModelWithoutClassFile,
            "no class file in the walked roots".into(),
        );
    }
    let model = f.model.as_str();
    match &f.filter {
        FiredFilter::Method(name) => {
            let assoc = name
                .strip_prefix("autosave_associated_records_for_")
                .or_else(|| name.strip_prefix("validate_associated_records_for_"));
            if let Some(x) = assoc {
                if ctx.assocs.contains(x) {
                    return (
                        Slag::AssociationAutosave,
                        format!("association :{x} declared"),
                    );
                }
                let files = ev.relevant(ev.assocs.get(x), model, ctx);
                // A plugin or engine file is the more specific explanation; a
                // lone file in the walked roots that merely names the model is
                // not evidence of an association on it.
                let best = files.iter().max_by_key(|p| bucket(p));
                return match best.map(|p| (p, bucket(p))) {
                    Some((p, Bucket::Plugin)) => (
                        Slag::AssociationFromPlugin,
                        format!("association :{x} declared in {p}"),
                    ),
                    Some((p, Bucket::Module)) => (
                        Slag::AssociationFromModule,
                        format!("association :{x} declared in {p}"),
                    ),
                    _ => (
                        Slag::AssociationNotInCheckout,
                        format!("no declaration of :{x} that concerns {model}"),
                    ),
                };
            }
            let files = ev.relevant(ev.defs.get(name), model, ctx);
            match files.iter().map(|p| bucket(p)).max() {
                None => (
                    Slag::FromGem,
                    "no `def` in the checkout that concerns the model".into(),
                ),
                Some(b) => {
                    let slag = match b {
                        Bucket::Walked => Slag::DefinedInWalkedRootsNotDeclared,
                        Bucket::Module => Slag::DefinedInModule,
                        Bucket::Plugin => Slag::DefinedInPlugin,
                    };
                    (slag, format!("def in {}", files[0]))
                }
            }
        }
        FiredFilter::Validator { attr, .. } => {
            let files = ev.relevant(ev.validates.get(attr), model, ctx);
            match files.iter().find(|p| bucket(p) == Bucket::Module) {
                Some(p) => (
                    Slag::ValidatorFromModule,
                    format!("validation of :{attr} in {p}"),
                ),
                None => (
                    Slag::ValidatorFromFramework,
                    format!("no module line validating :{attr} for {model}"),
                ),
            }
        }
        FiredFilter::ValidatesWith(c) => {
            (Slag::ValidatesWithUndeclared, format!("validates_with {c}"))
        }
    }
}

/// `if:` / `unless:` on the declaration, or inside the option hash of the
/// validator kind it declares (`validates :a, presence: { if: -> { ... } }`).
fn has_condition(d: &Declared) -> bool {
    let cond = |v: &str| v.contains(":if") || v.contains(":unless");
    d.options.iter().any(|(k, v)| {
        k == "if"
            || k == "unless"
            || matches!(&d.filter, Filter::Validator { kind, .. } if kind == k && cond(v))
    })
}

fn silent_reason(d: &Declared, fired_chains: &BTreeSet<String>) -> Silent {
    if d.filter == Filter::Block {
        return Silent::BlockOrLambdaFilter;
    }
    if !fired_chains.contains(&d.chain) {
        return Silent::ChainNotExercised;
    }
    let on = d
        .options
        .iter()
        .find(|(k, _)| k == "on")
        .map(|(_, v)| v.clone())
        .or_else(|| d.implied_on.map(|s| format!(":{s}")));
    if let Some(on) = &on {
        if !on.contains("create") {
            return Silent::OnOptionNotExercised;
        }
    }
    if on.is_some() || has_condition(d) {
        return Silent::ConditionalOptionNotEvaluated;
    }
    Silent::Unexplained
}

struct Outcome {
    /// distinct journal entries (model, phase, label) matched / total
    matched: usize,
    total: usize,
    slag: BTreeMap<Slag, Vec<String>>,
    silent: BTreeMap<Silent, Vec<String>>,
    declared_total: usize,
}

fn run(
    ctxs: &BTreeMap<String, ModelCtx>,
    entries: &BTreeMap<EntryKey, Vec<Fired>>,
    ev: &Evidence,
    transform: impl Fn(&Declared) -> Declared,
) -> Outcome {
    let declared: BTreeMap<&String, Vec<Declared>> = ctxs
        .iter()
        .map(|(m, c)| (m, c.declared.iter().map(&transform).collect()))
        .collect();
    let mut used: BTreeMap<&String, BTreeSet<usize>> = BTreeMap::new();
    let mut fired_chains: BTreeMap<&String, BTreeSet<String>> = BTreeMap::new();
    let mut out = Outcome {
        matched: 0,
        total: entries.len(),
        slag: BTreeMap::new(),
        silent: BTreeMap::new(),
        declared_total: declared.values().map(Vec::len).sum(),
    };
    for ((model, phase, label), expanded) in entries {
        let ctx = &ctxs[model];
        let decls = &declared[model];
        fired_chains
            .entry(model)
            .or_default()
            .extend(expanded.iter().map(|f| f.chain.clone()));
        let mut all = true;
        for f in expanded {
            let hits: Vec<usize> = decls
                .iter()
                .enumerate()
                .filter(|(_, d)| declared_matches(d, f))
                .map(|(i, _)| i)
                .collect();
            if hits.is_empty() {
                all = false;
                let (slag, why) = classify(f, ctx, ev);
                out.slag
                    .entry(slag)
                    .or_default()
                    .push(format!("{model} {phase} {label}  [{why}]"));
                break;
            }
            used.entry(model).or_default().extend(hits);
        }
        if all {
            out.matched += 1;
        }
    }
    for (model, decls) in &declared {
        let empty = BTreeSet::new();
        let u = used.get(model).unwrap_or(&empty);
        let chains = fired_chains.get(model).cloned().unwrap_or_default();
        for (i, d) in decls.iter().enumerate() {
            if !u.contains(&i) {
                out.silent
                    .entry(silent_reason(d, &chains))
                    .or_default()
                    .push(format!(
                        "{model} {}_{} {:?} (from {}) options={:?}",
                        d.kind, d.chain, d.filter, d.origin, d.options
                    ));
            }
        }
    }
    out
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "fired (distinct entries): {}", self.total)?;
        writeln!(f, "  matched to a declaration: {}", self.matched)?;
        for s in Slag::all() {
            writeln!(
                f,
                "  slag {:?}: {}",
                s,
                self.slag.get(s).map_or(0, Vec::len)
            )?;
        }
        writeln!(f, "declared callbacks: {}", self.declared_total)?;
        for s in Silent::all() {
            writeln!(
                f,
                "  declared, never fired, {:?}: {}",
                s,
                self.silent.get(s).map_or(0, Vec::len)
            )?;
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The test
// ─────────────────────────────────────────────────────────────────────────

fn contexts(
    src: &Path,
    models: &BTreeSet<String>,
) -> (BTreeMap<String, ModelCtx>, Vec<(String, String)>) {
    let mut h = Harvest::new(src);
    let mut out = BTreeMap::new();
    for m in models {
        let ancestors = h.linearize(m);
        let mut declared = Vec::new();
        for a in &ancestors {
            declared.extend(declared_of(&a.name, &a.decls));
        }
        out.insert(
            m.clone(),
            ModelCtx {
                has_file: !ancestors.is_empty(),
                assocs: association_names(&ancestors),
                ancestor_files: ancestors.iter().map(|a| a.file.clone()).collect(),
                declared,
            },
        );
    }
    (out, h.unresolved)
}

#[test]
fn fired_callbacks_are_matched_or_named_slag() {
    let Some(src) = source() else { return };
    let (entries_total, entries) = fired_entries();
    let models: BTreeSet<String> = entries.keys().map(|(m, _, _)| m.clone()).collect();
    let (ctxs, unresolved) = contexts(&src, &models);
    let ev = scan_tree(&src, &models);

    eprintln!(
        "journal entries: {entries_total}; distinct (model, phase, label): {}",
        entries.len()
    );
    for (m, c) in &ctxs {
        eprintln!(
            "  {m}: {} ancestors files, {} declared callbacks, {} associations",
            c.ancestor_files.len(),
            c.declared.len(),
            c.assocs.len()
        );
    }
    for (from, m) in &unresolved {
        eprintln!("  unresolved include: {from} -> {m}");
    }

    let o = run(&ctxs, &entries, &ev, Clone::clone);
    eprintln!("{o}");
    for (s, rows) in &o.slag {
        eprintln!("-- slag {s:?} ({})", rows.len());
        for r in rows.iter().take(6) {
            eprintln!("     {r}");
        }
    }
    for (s, rows) in &o.silent {
        eprintln!("-- declared, never fired, {s:?} ({})", rows.len());
        for r in rows.iter().take(6) {
            eprintln!("     {r}");
        }
    }

    // Closed classification: every fired entry is matched or in exactly one slag bucket.
    let slag_total: usize = o.slag.values().map(Vec::len).sum();
    assert_eq!(
        o.matched + slag_total,
        o.total,
        "an entry is neither matched nor classified"
    );
    let silent_total: usize = o.silent.values().map(Vec::len).sum();
    assert!(silent_total <= o.declared_total);

    // Measured at OpenProject 2725194 (trace revision). A drop means the harvest
    // lost declarations; a rise means it gained some: re-measure and move the floor.
    assert!(
        o.matched >= MATCHED_FLOOR,
        "matched {} of {}, floor {MATCHED_FLOOR}",
        o.matched,
        o.total
    );
    // A declared, unconditioned callback on an exercised chain that never fired
    // would be a real discrepancy, not slag.
    assert!(
        o.silent.get(&Silent::Unexplained).is_none_or(Vec::is_empty),
        "declared callbacks that never fired and have no explanation: {:?}",
        o.silent.get(&Silent::Unexplained)
    );

    // Negative control: spelled backwards, nothing may match.
    let control = run(&ctxs, &entries, &ev, reversed);
    eprintln!(
        "negative control: matched {} of {}",
        control.matched, control.total
    );
    assert!(
        control.matched * 50 <= o.matched,
        "the matcher does not discriminate: {} matched with reversed names",
        control.matched
    );
}
