//! Semantic assembly: one harvested class → its effective Rails behaviour.
//!
//! Applies the `rails_polyfill::rules` to ruff's ordered declaration streams:
//! inheritance of chains (R-INHERIT-CHAIN), concern expansion at the include
//! site (R-CONCERN-EXPAND, including ActiveSupport::Concern dependencies),
//! association-generated callbacks (R-AUTOSAVE-*), duplicate re-positioning
//! and prepend (R-CHAIN-*), and records every resulting declaration with the
//! source location and rule that produced it.

use std::collections::{BTreeMap, BTreeSet};

use lib_ruby_parser::Node;
use rails_polyfill::Action;
use rails_polyfill::rules::{self, Chain, Chains, CondSpec, Decl, Filter, Slot, ValidatorSpec};
use ruff_ruby_spo::Declaration;
use ruff_ruby_spo::events::RailsEvent;
use ruff_spo_triplet::AssocKind;
use serde::Serialize;

use crate::harvest::{Harvest, Unit, UnitKind, const_name, decl_node};

/// An association as declared, with its owner and source site.
#[derive(Debug, Clone, Serialize)]
pub struct Assoc {
    pub kind: String,
    pub name: String,
    pub options: BTreeMap<String, String>,
    /// The class that declares it after concern expansion.
    pub declared_in: String,
    pub provenance: String,
    /// `-> { reorder(:a, :b) }` / `-> { order(:a) }` scope columns.
    pub order: Vec<String>,
}

/// A declaration the compiler could not resolve to a rule.
#[derive(Debug, Clone, Serialize)]
pub struct Unresolved {
    pub what: String,
    pub provenance: String,
}

#[derive(Debug, Clone, Default)]
pub struct Expanded {
    pub chains: Chains,
    /// Modules included anywhere in the ancestry, inclusion order.
    pub included: Vec<String>,
    /// Ruby method resolution order: the class, then included modules (last
    /// included first), then the superclass's ancestors.
    pub ancestors: Vec<String>,
    pub assocs: Vec<Assoc>,
    /// Every association declaration in the order the expansion applied it
    /// (`name`, `file:line`), including those a later one replaced: Rails'
    /// `_reflections` keeps only the last, this keeps the history.
    pub assoc_log: Vec<(String, String)>,
    /// The unit whose events produced each `assoc_log` entry (the class, or the
    /// concern whose `included` block declared it), index for index.
    pub assoc_units: Vec<String>,
    pub stores: Vec<String>,
    /// `attr_accessor` / `attr_reader` names (virtual attributes).
    pub virtuals: Vec<String>,
    pub unresolved: Vec<Unresolved>,
    /// Every `(module, decl-kind)` that contributed, for the coverage report.
    pub contributions: Vec<(String, String)>,
    /// Lambda / block filters and conditions by id: declaring unit (name,
    /// file) and the block body.
    pub lambdas: BTreeMap<String, (String, std::path::PathBuf, Option<Node>)>,
}

pub struct Assembler<'h> {
    pub h: &'h Harvest,
    pub fw: rules::FrameworkDefaults,
    pub framework_roots: Vec<String>,
    cache: BTreeMap<String, Expanded>,
    in_progress: BTreeSet<String>,
}

fn sym(n: &Node) -> Option<String> {
    match n {
        Node::Sym(s) => Some(s.name.to_string_lossy()),
        Node::Str(s) => Some(s.value.to_string_lossy()),
        _ => None,
    }
}

fn kwargs(args: &[Node]) -> Vec<(String, Node)> {
    let mut out = Vec::new();
    for a in args {
        let pairs = match a {
            Node::Kwargs(k) => &k.pairs,
            Node::Hash(h) => &h.pairs,
            _ => continue,
        };
        for p in pairs {
            if let Node::Pair(p) = p
                && let Some(k) = sym(&p.key)
            {
                out.push((k, (*p.value).clone()));
            }
        }
    }
    out
}

fn actions(n: &Node) -> Option<Vec<Action>> {
    let one = |n: &Node| match sym(n).as_deref() {
        Some("create") => Some(Action::Create),
        Some("update") => Some(Action::Update),
        Some("destroy") => Some(Action::Destroy),
        _ => None,
    };
    match n {
        Node::Array(a) => a.elements.iter().map(one).collect(),
        other => one(other).map(|a| vec![a]),
    }
}

type Lambdas = Vec<(String, Option<Node>)>;

/// A callback filter from its AST argument. Lambda bodies are collected into
/// `lam` under the filter's id.
fn filter_of(arg: &Node, site: &str, idx: usize, lam: &mut Lambdas) -> Option<Filter> {
    match arg {
        Node::Sym(_) | Node::Str(_) => sym(arg).map(Filter::Method),
        Node::Const(_) => const_name(arg).map(Filter::Object),
        Node::Send(s) if s.method_name == "new" => s.recv.as_deref().and_then(const_name).map(Filter::Object),
        Node::Block(b) => {
            let id = format!("{site}#{idx}");
            lam.push((id.clone(), b.body.as_deref().cloned()));
            Some(Filter::Lambda { id, src: String::new() })
        }
        Node::Lambda(_) => {
            let id = format!("{site}#{idx}");
            lam.push((id.clone(), None));
            Some(Filter::Lambda { id, src: String::new() })
        }
        _ => None,
    }
}

fn cond_filters(n: &Node, site: &str, tag: &str, lam: &mut Lambdas) -> Vec<Filter> {
    match n {
        Node::Array(a) => a
            .elements
            .iter()
            .enumerate()
            .filter_map(|(i, e)| filter_of(e, &format!("{site}:{tag}"), i, lam))
            .collect(),
        other => filter_of(other, &format!("{site}:{tag}"), 0, lam).into_iter().collect(),
    }
}

/// `if:` / `unless:` of a macro call; `-> { … }` values parse as a `Block`
/// whose call is `Lambda`.
fn conds_of(opts: &[(String, Node)], site: &str, lam: &mut Lambdas) -> Vec<CondSpec> {
    let mut out = Vec::new();
    for (k, v) in opts {
        match k.as_str() {
            "if" => out.extend(cond_filters(v, site, "if", lam).into_iter().map(CondSpec::If)),
            "unless" => out.extend(cond_filters(v, site, "unless", lam).into_iter().map(CondSpec::Unless)),
            _ => {}
        }
    }
    out
}

/// Rails `compute_table_name` for the naming convention (R-TABLE-NAME).
#[must_use]
pub fn table_name(model: &str, is_model: &dyn Fn(&str) -> bool, irregulars: &[(String, String)]) -> String {
    let parts: Vec<&str> = model.split("::").collect();
    let leaf = underscore(parts.last().copied().unwrap_or(model));
    if parts.len() > 1 {
        let parent = parts[..parts.len() - 1].join("::");
        if is_model(&parent) {
            return format!("{}_{}", crate::singular_with(&table_name(&parent, is_model, irregulars), irregulars), pluralize(&leaf));
        }
    }
    pluralize(&leaf)
}

#[must_use]
pub fn underscore(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if i > 0 && (prev_lower || (next_lower && chars[i - 1].is_uppercase())) {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else if *c == ':' {
            if out.ends_with('/') {
                continue;
            }
            out.push('/');
        } else {
            out.push(*c);
        }
    }
    out
}

fn pluralize(s: &str) -> String {
    if s.ends_with('y') && !s.ends_with("ay") && !s.ends_with("ey") && !s.ends_with("oy") {
        format!("{}ies", &s[..s.len() - 1])
    } else if s.ends_with('s') || s.ends_with('x') || s.ends_with("ch") || s.ends_with("sh") {
        format!("{s}es")
    } else {
        format!("{s}s")
    }
}


impl<'h> Assembler<'h> {
    #[must_use]
    pub fn new(h: &'h Harvest, fw: rules::FrameworkDefaults, framework_roots: Vec<String>) -> Self {
        Self {
            h,
            fw,
            framework_roots,
            cache: BTreeMap::new(),
            in_progress: BTreeSet::new(),
        }
    }

    fn class_units(&self, name: &str) -> Vec<&'h Unit> {
        self.h
            .units
            .get(name)
            .map(|us| us.iter().filter(|u| u.kind == UnitKind::Class).collect())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn is_abstract(&self, name: &str) -> bool {
        // `self.abstract_class = true` in the class body.
        let sets_abstract = |n: &Node| {
            matches!(n, Node::Send(s) if s.method_name == "abstract_class="
                && matches!(s.recv.as_deref(), Some(Node::Self_(_)))
                && matches!(s.args.as_slice(), [Node::True(_)]))
        };
        self.class_units(name).iter().any(|u| {
            u.ore.declarations.iter().any(|e| matches!(&e.decl, Declaration::Sti(s) if s.abstract_class))
                || u.body.as_ref().is_some_and(|b| match b {
                    Node::Begin(bb) => bb.statements.iter().any(sets_abstract),
                    other => sets_abstract(other),
                })
        })
    }

    /// `self.table_name = '<literal>'` in the class body, which overrides the
    /// naming convention of [`table_name`].
    #[must_use]
    pub fn explicit_table_name(&self, model: &str) -> Option<String> {
        let literal = |n: &Node| match n {
            Node::Send(s) if s.method_name == "table_name="
                && matches!(s.recv.as_deref(), Some(Node::Self_(_))) =>
            {
                match s.args.as_slice() {
                    [Node::Str(v)] => Some(v.value.to_string_lossy()),
                    _ => None,
                }
            }
            _ => None,
        };
        self.class_units(model).iter().find_map(|u| {
            u.body.as_ref().and_then(|b| match b {
                Node::Begin(bb) => bb.statements.iter().find_map(literal),
                other => literal(other),
            })
        })
    }

    #[must_use]
    pub fn superclass(&self, name: &str) -> Option<String> {
        let u = self.class_units(name).into_iter().find(|u| u.superclass.is_some())?;
        let sup = u.superclass.clone()?;
        if self.framework_roots.contains(&sup) {
            return Some(sup);
        }
        // `class Foo::Bar < Bar` names the top-level Bar: a class can never
        // be its own superclass, so skip the scope that would say it is.
        match self.h.resolve(&sup, &u.nesting[1..]) {
            Some(r) if r != name => Some(r),
            _ => self.h.units.contains_key(&sup).then_some(sup.clone()).filter(|s| s != name).or(Some(sup)),
        }
    }

    #[must_use]
    pub fn cached(&self, name: &str) -> Option<Expanded> {
        self.cache.get(name).cloned()
    }

    /// The effective callback chains, ancestry and associations of `name`.
    pub fn expand(&mut self, name: &str) -> Expanded {
        if let Some(e) = self.cache.get(name) {
            return e.clone();
        }
        if !self.in_progress.insert(name.to_string()) {
            let mut e = Expanded::default();
            e.unresolved.push(Unresolved {
                what: format!("inheritance cycle through {name}"),
                provenance: name.to_string(),
            });
            return e;
        }
        let mut exp = match self.superclass(name) {
            Some(sup) if self.framework_roots.contains(&sup) => {
                let mut e = Expanded::default();
                for d in rules::base_decls() {
                    e.chains.push(d);
                }
                e.ancestors.push(sup);
                e
            }
            Some(sup) if self.h.units.contains_key(&sup) => self.expand(&sup),
            Some(sup) => {
                let mut e = Expanded::default();
                e.unresolved.push(Unresolved {
                    what: format!("superclass {sup} not in harvest"),
                    provenance: name.to_string(),
                });
                e
            }
            None => Expanded::default(),
        };
        let mut own_modules = Vec::new();
        for u in self.class_units(name) {
            let events = u.ore.declarations.clone();
            self.stream(&events, u, u.body.as_ref(), name, &mut exp, &mut own_modules);
        }
        let mut ancestors = vec![name.to_string()];
        ancestors.extend(own_modules.iter().rev().cloned());
        ancestors.append(&mut exp.ancestors);
        exp.ancestors = ancestors;
        self.in_progress.remove(name);
        self.cache.insert(name.to_string(), exp.clone());
        exp
    }

    fn stream(
        &mut self,
        events: &[RailsEvent],
        unit: &'h Unit,
        body: Option<&'h Node>,
        owner: &str,
        exp: &mut Expanded,
        modules: &mut Vec<String>,
    ) {
        let mut seen_anchor: BTreeSet<u32> = BTreeSet::new();
        let mut i = 0;
        while i < events.len() {
            let ev = &events[i];
            match &ev.decl {
                Declaration::Concern(c) if matches!(c.kind, ruff_spo_triplet::ConcernKind::Include) => {
                    // `include A, B` appends features in reverse argument order.
                    let mut group = vec![c.module.clone()];
                    while let Some(next) = events.get(i + 1)
                        && next.anchor == ev.anchor
                        && let Declaration::Concern(c2) = &next.decl
                        && matches!(c2.kind, ruff_spo_triplet::ConcernKind::Include)
                    {
                        group.push(c2.module.clone());
                        i += 1;
                    }
                    for m in group.into_iter().rev() {
                        self.include(&m, unit, owner, exp, modules);
                    }
                }
                Declaration::Callback(_) if seen_anchor.insert(ev.anchor) => {
                    self.callback(ev, unit, body, exp);
                }
                Declaration::Validation(_) if seen_anchor.insert(ev.anchor) => {
                    self.validation(ev, unit, body, exp);
                }
                Declaration::DslCall(d) if d.name == "validates_with" && seen_anchor.insert(ev.anchor) => {
                    self.validation(ev, unit, body, exp);
                }
                Declaration::Attribute(a)
                    if matches!(a.kind, ruff_spo_triplet::AttrKind::AttrAccessor | ruff_spo_triplet::AttrKind::AttrReader) =>
                {
                    if !exp.virtuals.contains(&a.name) {
                        exp.virtuals.push(a.name.clone());
                    }
                }
                Declaration::DslCall(d) if d.name == "store" => {
                    exp.stores.extend(d.args.split(',').map(|a| a.trim().trim_start_matches(':').to_string()));
                }
                Declaration::Association(a) if !matches!(a.kind, AssocKind::AcceptsNestedAttributesFor) => {
                    self.association(ev, a, unit, body, owner, exp);
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn include(&mut self, written: &str, unit: &'h Unit, owner: &str, exp: &mut Expanded, modules: &mut Vec<String>) {
        let Some(m) = self.h.resolve(written, &unit.nesting) else {
            // Framework modules (`ActiveModel::Validations`) are not in the
            // harvest; re-including one is a no-op in Ruby either way.
            if !written.starts_with("Active") {
                exp.unresolved.push(Unresolved {
                    what: format!("include {written}"),
                    provenance: owner.to_string(),
                });
            }
            return;
        };
        if exp.included.contains(&m) {
            return;
        }
        let Some(munit) = self.h.units.get(&m).and_then(|us| us.iter().find(|u| u.kind == UnitKind::Module)) else {
            return;
        };
        // ActiveSupport::Concern dependencies: concerns included at the
        // module's top level are included into the base first.
        let deps: Vec<String> = munit
            .ore
            .declarations
            .iter()
            .filter_map(|e| match &e.decl {
                Declaration::Concern(c) if matches!(c.kind, ruff_spo_triplet::ConcernKind::Include) => Some(c.module.clone()),
                _ => None,
            })
            .collect();
        // Marked before its dependencies so a dependency cycle terminates,
        // as Ruby's own `include` does on an already-included module.
        exp.included.push(m.clone());
        if munit.concern {
            for d in deps {
                self.include(&d, munit, owner, exp, modules);
            }
        }
        modules.push(m.clone());
        exp.contributions.push((m.clone(), "include".into()));
        if munit.concern
            && let Some(inc) = &munit.included
        {
            let events = inc.declarations.clone();
            self.stream(&events, munit, munit.included_body.as_ref(), owner, exp, modules);
        }
    }

    fn callback(&mut self, ev: &RailsEvent, unit: &'h Unit, body: Option<&'h Node>, exp: &mut Expanded) {
        let site = self.h.loc(unit, ev.anchor as usize);
        let Some((Node::Send(call), block)) = body.and_then(|b| decl_node(b, ev)) else {
            exp.unresolved.push(Unresolved {
                what: "callback declaration not found at anchor".into(),
                provenance: site,
            });
            return;
        };
        let mut lam: Lambdas = Vec::new();
        let Some((chain, slot, implied_on)) = rules::resolve_macro(&call.method_name) else {
            return;
        };
        let opts = kwargs(&call.args);
        let on = opts
            .iter()
            .find(|(k, _)| k == "on")
            .and_then(|(_, v)| actions(v))
            .or(implied_on);
        let user_prepend = opts.iter().any(|(k, v)| k == "prepend" && matches!(v, Node::True(_)));
        let conds = conds_of(&opts, &site, &mut lam);
        let mut filters: Vec<Filter> = call
            .args
            .iter()
            .enumerate()
            .filter(|(_, a)| !matches!(a, Node::Kwargs(_) | Node::Hash(_)))
            .filter_map(|(i, a)| filter_of(a, &site, i, &mut lam))
            .collect();
        if filters.is_empty() {
            let id = format!("{site}#block");
            lam.push((id.clone(), block.cloned()));
            filters.push(Filter::Lambda { id, src: String::new() });
        }
        for (id, node) in lam {
            exp.lambdas.insert(id, (unit.name.clone(), unit.file.clone(), node));
        }
        for f in filters {
            exp.contributions.push((unit.name.clone(), "callback".into()));
            exp.chains.push(Decl {
                chain,
                slot,
                filter: f,
                on: on.clone(),
                conds: conds.clone(),
                prepend: rules::effective_prepend_with(self.fw, chain, slot, user_prepend),
                provenance: site.clone(),
                rule: "source",
            });
        }
    }

    fn validation(&mut self, ev: &RailsEvent, unit: &'h Unit, body: Option<&'h Node>, exp: &mut Expanded) {
        let site = self.h.loc(unit, ev.anchor as usize);
        let Some((Node::Send(call), block)) = body.and_then(|b| decl_node(b, ev)) else {
            exp.unresolved.push(Unresolved {
                what: "validation declaration not found at anchor".into(),
                provenance: site,
            });
            return;
        };
        let mut lam: Lambdas = Vec::new();
        let opts = kwargs(&call.args);
        let on = opts.iter().find(|(k, _)| k == "on").and_then(|(_, v)| actions(v));
        let conds = conds_of(&opts, &site, &mut lam);
        if call.method_name == "validate" && block.is_some() {
            lam.push((format!("{site}#block"), block.cloned()));
        }
        for (id, node) in &lam {
            exp.lambdas.insert(id.clone(), (unit.name.clone(), unit.file.clone(), node.clone()));
        }
        let block_id = (call.method_name == "validate" && block.is_some()).then(|| format!("{site}#block"));
        let positional: Vec<&Node> = call
            .args
            .iter()
            .filter(|a| !matches!(a, Node::Kwargs(_) | Node::Hash(_)))
            .collect();
        let mut push = |filter: Filter| {
            exp.contributions.push((unit.name.clone(), "validation".into()));
            exp.chains.push(Decl {
                chain: Chain::Validate,
                slot: Slot::Before,
                filter,
                on: on.clone(),
                conds: conds.clone(),
                prepend: false,
                provenance: site.clone(),
                rule: "source",
            });
        };
        match call.method_name.as_str() {
            "validate" => {
                let mut extra: Lambdas = Vec::new();
                for (i, a) in positional.iter().enumerate() {
                    if let Some(f) = filter_of(a, &site, i, &mut extra) {
                        push(f);
                    }
                }
                if let Some(id) = block_id {
                    push(Filter::Lambda { id, src: String::new() });
                }
            }
            "validates_with" => {
                for a in positional {
                    if let Some(k) = const_name(a) {
                        push(Filter::Validator(ValidatorSpec::Other { kind: k, attr: String::new() }));
                    }
                }
            }
            "validates" => {
                let attrs: Vec<String> = positional.iter().filter_map(|a| sym(a)).collect();
                let joined = attrs.join(",");
                const NOT_KINDS: &[&str] = &["on", "if", "unless", "allow_nil", "allow_blank", "strict", "message"];
                for (k, v) in &opts {
                    if NOT_KINDS.contains(&k.as_str()) {
                        continue;
                    }
                    let spec = match k.as_str() {
                        "presence" if matches!(v, Node::True(_)) => ValidatorSpec::Presence(joined.clone()),
                        "length" => {
                            let lo = kwargs(std::slice::from_ref(v));
                            let num = |key: &str| {
                                lo.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
                                    Node::Int(i) => i.value.parse().ok(),
                                    _ => None,
                                })
                            };
                            ValidatorSpec::Length {
                                attr: joined.clone(),
                                min: num("minimum"),
                                max: num("maximum"),
                            }
                        }
                        other => ValidatorSpec::Other {
                            kind: other.to_string(),
                            attr: joined.clone(),
                        },
                    };
                    push(Filter::Validator(spec));
                }
            }
            _ => {}
        }
    }

    fn association(
        &mut self,
        ev: &RailsEvent,
        a: &ruff_spo_triplet::AssocDecl,
        unit: &'h Unit,
        body: Option<&'h Node>,
        owner: &str,
        exp: &mut Expanded,
    ) {
        let order = body
            .and_then(|b| decl_node(b, ev))
            .and_then(|(call, _)| match call {
                Node::Send(s) => s.args.iter().find_map(|arg| match arg {
                    Node::Block(b) if matches!(&*b.call, Node::Lambda(_)) => match b.body.as_deref() {
                        Some(Node::Send(o)) if matches!(o.method_name.as_str(), "reorder" | "order") && o.recv.is_none() => {
                            Some(o.args.iter().filter_map(sym).collect::<Vec<_>>())
                        }
                        _ => None,
                    },
                    _ => None,
                }),
                _ => None,
            })
            .unwrap_or_default();
        let kind = match a.kind {
            AssocKind::BelongsTo => "belongs_to",
            AssocKind::HasMany => "has_many",
            AssocKind::HasOne => "has_one",
            AssocKind::HasAndBelongsToMany => "has_and_belongs_to_many",
            AssocKind::AcceptsNestedAttributesFor => return,
        };
        let site = self.h.loc(unit, ev.anchor as usize);
        let options: BTreeMap<String, String> = a.options.iter().cloned().collect();
        let opt = |k: &str| options.get(k).map(String::as_str);
        let optional = opt("optional").map(|v| v == "true").or_else(|| opt("required").map(|v| v != "true"));
        let autosave = opt("autosave") == Some("true");
        let validate = opt("validate").map(|v| v == "true");
        for d in rules::association_decls(self.fw, kind, &a.name, optional, autosave, validate, &site) {
            exp.chains.push(d);
        }
        exp.assoc_log.push((a.name.clone(), site.clone()));
        exp.assoc_units.push(unit.name.clone());
        exp.assocs.retain(|x| x.name != a.name);
        exp.assocs.push(Assoc {
            kind: kind.into(),
            name: a.name.clone(),
            options,
            declared_in: owner.to_string(),
            provenance: site,
            order,
        });
        exp.contributions.push((unit.name.clone(), "association".into()));
    }

    /// Ruby method lookup: the first ancestor whose units define `method`.
    #[must_use]
    pub fn find_def(&self, ancestors: &[String], method: &str) -> Option<(&'h Unit, &'h Node)> {
        for a in ancestors {
            if let Some(us) = self.h.units.get(a) {
                for u in us.iter().rev() {
                    if let Some(d) = u.defs.get(method) {
                        return Some((u, d));
                    }
                }
            }
        }
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Relation semantics (R-REL-*)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RelationFact {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub rule: String,
    pub evidence: Vec<String>,
}

/// `(model, fk) → (required?, evidence)`.
pub type FkRequired<'a> = dyn Fn(&str, &str) -> (bool, Vec<String>) + 'a;

pub struct RelationInput<'a> {
    pub model: &'a str,
    pub superclass: Option<String>,
    pub superclass_abstract: bool,
    pub superclass_has_type_column: bool,
    pub included: &'a [String],
    pub assocs: &'a [Assoc],
}

/// Derive `is_a` / `part_of` / role / reference / lifecycle facts for every
/// model in `inputs`. `target_of` resolves an association to its class name;
/// `fk_required` says whether a model's foreign-key column is mandatory
/// (NOT NULL or presence-validated).
pub fn relations(
    inputs: &[RelationInput<'_>],
    target_of: &dyn Fn(&str, &Assoc) -> String,
    fk_required: &FkRequired<'_>,
) -> Vec<RelationFact> {
    let mut out = Vec::new();
    for inp in inputs {
        if let Some(sup) = &inp.superclass {
            let (rule, pred) = if inp.superclass_abstract || sup.starts_with("ActiveRecord") || sup == "ApplicationRecord" {
                ("R-REL-IS-A-FRAMEWORK", "is_a[framework]")
            } else if inp.superclass_has_type_column {
                ("R-REL-IS-A-STI", "is_a")
            } else {
                ("R-REL-IS-A-FRAMEWORK", "is_a[ruby-subclass]")
            };
            out.push(RelationFact {
                subject: inp.model.into(),
                predicate: pred.into(),
                object: sup.clone(),
                rule: rule.into(),
                evidence: vec![format!("class {} < {sup}", inp.model)],
            });
        }
        if let Some((outer, _)) = inp.model.rsplit_once("::") {
            out.push(RelationFact {
                subject: inp.model.into(),
                predicate: "namespaced_under".into(),
                object: outer.into(),
                rule: "R-REL-NAMESPACE".into(),
                evidence: vec![format!("constant {}", inp.model)],
            });
        }
        for m in inp.included {
            out.push(RelationFact {
                subject: inp.model.into(),
                predicate: "has_behaviour".into(),
                object: m.clone(),
                rule: "R-REL-MIXIN".into(),
                evidence: vec![format!("include {m}")],
            });
        }
        for a in inp.assocs {
            let target = target_of(inp.model, a);
            match a.kind.as_str() {
                "belongs_to" => {
                    let unq = |v: &str| v.trim_matches(|c| c == ':' || c == '"' || c == '\'').to_string();
                    let fk = a.options.get("foreign_key").map_or_else(|| format!("{}_id", a.name), |f| unq(f));
                    // The inverse collection on the target: same class, same
                    // foreign key, and (if declared) `inverse_of:` this name.
                    let inverse = inputs.iter().find(|o| o.model == target).and_then(|o| {
                        o.assocs.iter().find(|b| {
                            let b_fk = b.options.get("foreign_key").map_or_else(
                                || format!("{}_id", underscore(o.model.rsplit("::").next().unwrap_or(o.model))),
                                |f| unq(f),
                            );
                            matches!(b.kind.as_str(), "has_many" | "has_one")
                                && target_of(o.model, b) == inp.model
                                && b_fk == fk
                                && b.options.get("inverse_of").is_none_or(|i| unq(i) == a.name)
                        })
                    });
                    let cascade = inverse.filter(|b| {
                        matches!(b.options.get("dependent").map(String::as_str), Some(":destroy" | ":delete_all"))
                    });
                    let is_role = underscore(target.rsplit("::").next().unwrap_or(&target)) != a.name;
                    let (required, why) = fk_required(inp.model, &fk);
                    if let Some(c) = cascade
                        && required
                        && !is_role
                    {
                        let mut ev = vec![
                            format!("{} {} :{} dependent: {}", target, c.kind, c.name, c.options.get("dependent").cloned().unwrap_or_default()),
                            format!("{} belongs_to :{} ({})", inp.model, a.name, a.provenance),
                        ];
                        ev.extend(why);
                        out.push(RelationFact {
                            subject: inp.model.into(),
                            predicate: "part_of".into(),
                            object: target.clone(),
                            rule: "R-REL-PART-OF".into(),
                            evidence: ev,
                        });
                    } else if is_role {
                        let mut ev = vec![format!("belongs_to :{} class_name {target} ({})", a.name, a.provenance)];
                        if let Some(c) = cascade {
                            // Existence dependence through a role is a
                            // lifecycle obligation of the target, not
                            // containment (User#customer_tickets: deleting a
                            // customer deletes their tickets; a ticket is not
                            // part of its customer).
                            ev.push(format!(
                                "{target} {} :{} dependent: {} — cascade through a role, recorded as lifecycle obligation, not part_of",
                                c.kind,
                                c.name,
                                c.options.get("dependent").cloned().unwrap_or_default()
                            ));
                        }
                        out.push(RelationFact {
                            subject: inp.model.into(),
                            predicate: format!("role:{}", a.name),
                            object: target.clone(),
                            rule: "R-REL-ROLE".into(),
                            evidence: ev,
                        });
                    } else {
                        out.push(RelationFact {
                            subject: inp.model.into(),
                            predicate: "references".into(),
                            object: target.clone(),
                            rule: "R-REL-REFERENCE".into(),
                            evidence: vec![format!("belongs_to :{} ({})", a.name, a.provenance)],
                        });
                    }
                }
                "has_many" | "has_one" => {
                    if let Some(dep) = a.options.get("dependent") {
                        out.push(RelationFact {
                            subject: inp.model.into(),
                            predicate: format!("lifecycle_obligation:{}", dep.trim_start_matches(':')),
                            object: target.clone(),
                            rule: "R-REL-LIFECYCLE".into(),
                            evidence: vec![format!("{} :{} dependent: {dep} ({})", a.kind, a.name, a.provenance)],
                        });
                    }
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harvest::Harvest;

    fn harvest_of(src: &str, tag: &str) -> Harvest {
        let dir = std::env::temp_dir().join(format!("rails-compiler-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("app/models/cti")).unwrap();
        std::fs::write(dir.join("app/models/cti/log.rb"), src).unwrap();
        let h = Harvest::collect(&dir, &["app/models".to_string()], "Zammad");
        std::fs::remove_dir_all(&dir).ok();
        h
    }

    #[test]
    fn an_explicit_table_name_overrides_the_convention() {
        let h = harvest_of("class Cti::Log < ApplicationModel\n  self.table_name = 'cti_logs'\nend\n", "explicit");
        let asm = Assembler::new(&h, rules::FrameworkDefaults::for_version(None), vec!["ApplicationModel".into()]);
        assert_eq!(asm.explicit_table_name("Cti::Log").as_deref(), Some("cti_logs"));
        assert_eq!(table_name("Cti::Log", &|_| false, &[]), "logs");
    }

    #[test]
    fn a_namespaced_table_singularizes_its_parent_under_the_applications_irregulars() {
        let is_model = |n: &str| n == "KnowledgeBase";
        let irregulars = [("base".to_string(), "bases".to_string())];
        assert_eq!(table_name("KnowledgeBase::Category", &is_model, &irregulars), "knowledge_base_categories");
        assert_eq!(table_name("KnowledgeBase::Category", &is_model, &[]), "knowledge_basis_categories");
    }

    #[test]
    fn no_explicit_table_name_without_the_assignment() {
        let h = harvest_of("class Cti::Log < ApplicationModel\n  self.abstract_class = false\nend\n", "none");
        let asm = Assembler::new(&h, rules::FrameworkDefaults::for_version(None), vec!["ApplicationModel".into()]);
        assert_eq!(asm.explicit_table_name("Cti::Log"), None);
    }
}
