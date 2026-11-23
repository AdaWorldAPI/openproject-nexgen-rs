//! The effective associations of a model, derived from Rails' ancestor
//! expansion. A shadow: nothing here feeds the OGAR lift, the field basis or
//! admission.
//!
//! Rails semantics this follows (activerecord 8.1.4.1):
//! - a concern's `included do` block is `class_eval`ed on the including class
//!   (activesupport `concern.rb:138`), so its associations register on the
//!   includer through `Reflection.add_reflection` (`reflection.rb:23-27`):
//!   `_reflections.except(name).merge!(name => reflection)`. A later
//!   declaration of a name replaces the earlier one and moves to the end;
//!   `_reflections` is a `class_attribute`, so a subclass starts from its
//!   superclass's reflections.
//! - `class_name` resolves through `compute_type` from the reflection-holding
//!   class's namespace outward (`reflection.rb:490-496`,
//!   `inheritance.rb:247-268`): the includer, not the concern.
//! - `belongs_to` is required unless `optional: true` (or `required: false`),
//!   when `belongs_to_required_by_default` holds (`builder/belongs_to.rb:128-141`).
//!
//! Associations whose name is not a literal and `Other.has_many` written in a
//! concern are not resolved by guessing: each is a typed [`ShadowSlag`] with
//! its source span.

use std::collections::{BTreeMap, BTreeSet};

use lib_ruby_parser::Node;
use lib_ruby_parser::nodes::Send;
use lib_ruby_parser::traverse::visitor::{Visitor, visit_send};
use serde::Serialize;

use crate::harvest::{Harvest, Unit, UnitKind, const_name};
use crate::semantic::{Assembler, Assoc, underscore};
use crate::unquote;

const ASSOC_MACROS: [&str; 4] = ["belongs_to", "has_many", "has_one", "has_and_belongs_to_many"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Origin {
    /// Declared in the model's own class body (any file reopening it).
    Own,
    /// Provided by an included concern.
    Concern { module: String },
    /// Inherited from a superclass's reflections.
    Inherited { from: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveAssoc {
    pub name: String,
    pub kind: String,
    pub options: BTreeMap<String, String>,
    pub origin: Origin,
    /// The unit whose source declares it (the concern, or the class).
    pub declaring_unit: String,
    /// `file:line` of the declaration.
    pub site: String,
    /// The class whose reflections hold it (the model this was derived for, or
    /// the superclass that holds an inherited one).
    pub includer: String,
    /// The class name as written, or implied by the association name.
    pub written_class: Option<String>,
    /// `compute_type` from the holder's namespace; `None` when the name is not
    /// a literal, polymorphic, or not harvested.
    pub target: Option<String>,
    pub foreign_key: Option<String>,
    /// `belongs_to` only.
    pub required: Option<bool>,
    /// The declaration this one replaced (same name, earlier), by `file:line`.
    pub replaced: Option<String>,
    /// Position in the effective (`_reflections`) order.
    pub position: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ShadowSlagKind {
    DynamicAssociationName,
    CrossClassAssociationDeclaration,
    /// `class_name:` is an expression, not a literal.
    DynamicAssociationClassName,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShadowSlag {
    pub kind: ShadowSlagKind,
    /// `file:line`.
    pub site: String,
    pub span: (u32, u32),
    pub detail: String,
    /// The unit whose source holds it.
    pub unit: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Effective {
    pub model: String,
    pub assocs: Vec<EffectiveAssoc>,
    pub slag: Vec<ShadowSlag>,
}

struct Scan<'a> {
    unit: &'a Unit,
    h: &'a Harvest,
    out: Vec<ShadowSlag>,
}

impl Visitor for Scan<'_> {
    fn on_send(&mut self, n: &Send) {
        if ASSOC_MACROS.contains(&n.method_name.as_str()) {
            let span = (n.expression_l.begin as u32, n.expression_l.end as u32);
            let site = self.h.loc(self.unit, span.0 as usize);
            match n.recv.as_deref() {
                None | Some(Node::Self_(_)) => {
                    if !matches!(n.args.first(), Some(Node::Sym(_) | Node::Str(_))) {
                        self.out.push(ShadowSlag { kind: ShadowSlagKind::DynamicAssociationName, site, span, detail: n.method_name.clone(), unit: self.unit.name.clone() });
                    }
                }
                Some(recv) => {
                    let who = const_name(recv).unwrap_or_else(|| "<expr>".into());
                    self.out.push(ShadowSlag { kind: ShadowSlagKind::CrossClassAssociationDeclaration, site, span, detail: format!("{who}.{}", n.method_name), unit: self.unit.name.clone() });
                }
            }
        }
        visit_send(self, n);
    }
}

fn scan_unit(h: &Harvest, u: &Unit) -> Vec<ShadowSlag> {
    let mut s = Scan { unit: u, h, out: Vec::new() };
    for b in [&u.body, &u.included_body].into_iter().flatten() {
        s.visit(b);
    }
    // The included body is part of the body: deduplicate by span.
    let mut seen = BTreeSet::new();
    s.out.retain(|x| seen.insert(x.span));
    s.out
}

fn foreign_key(model: &str, kind: &str, name: &str, options: &BTreeMap<String, String>) -> Option<String> {
    if options.contains_key("through") {
        return None;
    }
    if let Some(fk) = options.get("foreign_key") {
        return Some(unquote(fk));
    }
    match kind {
        "belongs_to" => Some(format!("{name}_id")),
        "has_and_belongs_to_many" => None,
        _ => Some(match options.get("as") {
            Some(a) => format!("{}_id", unquote(a)),
            None => format!("{}_id", underscore(model.rsplit("::").next().unwrap_or(model))),
        }),
    }
}

/// Per-harvest caches for the derivation (scans by unit).
pub struct Shadow<'h> {
    h: &'h Harvest,
    scans: BTreeMap<String, Vec<ShadowSlag>>,
}

impl<'h> Shadow<'h> {
    #[must_use]
    pub fn new(h: &'h Harvest) -> Self {
        Self { h, scans: BTreeMap::new() }
    }

    /// The unit that produced the declaration: carried through the expansion
    /// (`Expanded::assoc_units`), so a concern and its includer written in one
    /// file are told apart.
    fn declaring_unit(e: &crate::semantic::Expanded, a: &Assoc) -> String {
        e.assoc_log.iter().zip(&e.assoc_units).rev().find(|((n, site), _)| *n == a.name && *site == a.provenance).map_or_else(|| a.declared_in.clone(), |(_, u)| u.clone())
    }

    fn scan(&mut self, name: &str) -> Vec<ShadowSlag> {
        if let Some(v) = self.scans.get(name) {
            return v.clone();
        }
        let v: Vec<ShadowSlag> = self.h.units.get(name).into_iter().flatten().flat_map(|u| scan_unit(self.h, u)).collect();
        self.scans.insert(name.to_string(), v.clone());
        v
    }

    /// The effective associations of `model`, in `_reflections` order, with the
    /// typed Slag for what is not resolved.
    pub fn effective(&mut self, asm: &mut Assembler<'_>, model: &str) -> Effective {
        let h = self.h;
        let e = asm.expand(model);
        let mut out = Effective { model: model.to_string(), ..Effective::default() };
        let mut position = 0;
        for a in &e.assocs {
            let decl_unit = Self::declaring_unit(&e, a);
            let history: Vec<&String> = e.assoc_log.iter().filter(|(n, _)| *n == a.name).map(|(_, s)| s).collect();
            let replaced = history.len().checked_sub(2).and_then(|k| history.get(k)).map(|s| (*s).clone());
            // `declared_in` is the class the declaration was expanded for: the
            // superclass for an inherited one, the model otherwise.
            let from_super = a.declared_in != model;
            let origin = if from_super {
                Origin::Inherited { from: a.declared_in.clone() }
            } else if h.units.get(&decl_unit).is_some_and(|us| us.iter().any(|u| u.kind == UnitKind::Module)) {
                Origin::Concern { module: decl_unit.clone() }
            } else {
                Origin::Own
            };
            let poly = a.options.get("polymorphic").is_some_and(|v| v == "true");
            let class_expr = a.options.get("class_name").is_some_and(|v| v == "<expr>");
            let written = if poly || class_expr {
                None
            } else {
                Some(a.options.get("class_name").map_or_else(
                    || h.camelize(&if matches!(a.kind.as_str(), "has_many" | "has_and_belongs_to_many") { h.singular(&a.name) } else { a.name.clone() }),
                    |c| unquote(c),
                ))
            };
            // compute_type runs from the reflection holder (`active_record`): the
            // superclass for an inherited association, the includer otherwise.
            let holder = if cfg!(feature = "disable-shadow-holder") { model } else { a.declared_in.as_str() };
            let target = written.as_ref().and_then(|w| h.resolve(w, &[holder.to_string()]));
            let optional = a.options.get("optional").map(|v| v == "true").or_else(|| a.options.get("required").map(|v| v != "true"));
            let required = (a.kind == "belongs_to").then(|| optional.map_or(asm.fw.belongs_to_required_by_default, |o| !o));
            if cfg!(feature = "disable-shadow-concerns") && matches!(origin, Origin::Concern { .. }) {
                continue;
            }
            position += 1;
            out.assocs.push(EffectiveAssoc {
                name: a.name.clone(),
                kind: a.kind.clone(),
                options: a.options.clone(),
                origin,
                declaring_unit: decl_unit,
                site: a.provenance.clone(),
                includer: a.declared_in.clone(),
                written_class: written,
                target,
                foreign_key: foreign_key(holder, &a.kind, &a.name, &a.options),
                required,
                replaced,
                position: position - 1,
            });
            if class_expr && let Some(u) = h.units.get(model).and_then(|us| us.first()) {
                out.slag.push(ShadowSlag { kind: ShadowSlagKind::DynamicAssociationClassName, site: a.provenance.clone(), span: (0, 0), detail: format!("{}: class_name is an expression", a.name), unit: u.name.clone() });
            }
        }
        // Dynamic names and cross-class declarations, in every unit on the chain.
        let mut seen = BTreeSet::new();
        for anc in &e.ancestors {
            for s in self.scan(anc) {
                if seen.insert((s.site.clone(), s.kind)) {
                    out.slag.push(s);
                }
            }
        }
        out
    }
}
