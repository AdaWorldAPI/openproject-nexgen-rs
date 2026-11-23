//! `rails-compiler` — Rails applications compiled through the shared
//! AdaWorldAPI architecture.
//!
//! ```text
//! Ruby / Rails source
//!   → ruff_ruby_spo        (ordered Rails arm: ClassOre / RailsEvent; schema stratum)
//!   → harvest              (the same arm over concern modules and `included` blocks)
//!   → ogar_from_ruff       (OGAR Class: identity, associations, callbacks as OGAR sees them)
//!   → semantic             (rails_polyfill::rules: chains, framework callbacks, relations)
//!   → lower                (method bodies → Rust over rails_polyfill)
//!   → emit                 (deterministic, provenance-carrying Rust + report.json)
//! ```
//!
//! The application is chosen by a [`config::Config`] file; no application
//! name appears in this crate.

pub mod behaviour;
pub mod config;
pub mod emit;
pub mod harvest;
pub mod lower;
pub mod schema;
pub mod semantic;
pub mod shadow;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::config::Loaded;
use crate::harvest::Harvest;
use crate::lower::{Lowerer, ModelInfo};
use crate::schema::Schema;
use crate::semantic::{Assembler, Expanded, RelationFact, RelationInput, table_name, underscore};

/// What a chain entry runs, after the slice boundary is applied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Binding {
    /// Body lowered to Rust: the generated fn.
    Lowered(String),
    /// Declared boundary (outside the verbatim slice).
    Host,
    /// ActiveRecord-generated callback, no-op without an association cache.
    Framework,
    /// A validator lowered to data.
    Validator(String),
    /// Could not be lowered although the slice requires it.
    Residual(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainEntry {
    pub phase: String,
    pub name: String,
    pub on: Option<Vec<String>>,
    pub conds: Vec<(String, String)>,
    pub binding: Binding,
    pub rule: String,
    pub provenance: String,
    /// For callback methods outside the slice: whether the compiler could
    /// have lowered the body anyway (measurement only).
    pub would_lower: Option<Result<(), String>>,
}

#[derive(Debug, Serialize)]
pub struct CompiledModel {
    pub name: String,
    pub module: String,
    pub table: String,
    pub table_provenance: String,
    pub source_files: Vec<String>,
    pub ancestors: Vec<String>,
    pub chain: Vec<ChainEntry>,
    pub ogar: OgarView,
    pub lowered_methods: BTreeMap<String, String>,
    pub residual_methods: BTreeMap<String, (String, String)>,
    pub host_calls: BTreeSet<String>,
    pub aliases_used: BTreeSet<String>,
    pub dynamic_attr: BTreeSet<String>,
    pub folded: BTreeSet<String>,
    pub unresolved: Vec<semantic::Unresolved>,
    /// `store :x` attributes (serialized; read as `{}` when NULL).
    pub stores: Vec<String>,
    /// Navigable associations: (name, kind, fk column, target model, order
    /// columns, provenance) — only those whose target is in the slice and
    /// whose foreign key exists.
    pub navigable: Vec<(String, String, String, String, Vec<String>, String)>,
    /// Associations not navigable in the slice, with the reason.
    pub not_navigable: Vec<(String, String)>,
    #[serde(skip)]
    pub code: lower::Lowered,
    #[serde(skip)]
    pub conds: Vec<(String, String)>,
}

/// What OGAR's own lift (`ogar_from_ruff::lift_model`) carries for the
/// class, beside what this compiler derives — the size of the gap is the
/// evidence for the upstream candidates.
#[derive(Debug, Serialize, Default)]
pub struct OgarView {
    pub lifted: bool,
    pub name: String,
    pub parent: Option<String>,
    pub associations: usize,
    pub callbacks: usize,
    pub validations: usize,
    pub mixins: usize,
}

#[derive(Debug, Serialize)]
pub struct Compilation {
    pub app: String,
    pub models: Vec<CompiledModel>,
    pub relations: Vec<RelationFact>,
    pub schema_cross_check: Vec<(String, Vec<String>, Vec<String>)>,
    pub schema_unparsed: Vec<String>,
    pub corpus: CorpusCoverage,
    pub harvest_parse_failures: Vec<String>,
    #[serde(skip)]
    pub schema: Schema,
}

/// Whole-application measurement: every ActiveRecord model, expanded and
/// every callback body attempted.
#[derive(Debug, Serialize, Default)]
pub struct CorpusCoverage {
    pub classes_harvested: usize,
    pub modules_harvested: usize,
    pub concern_modules: usize,
    pub models: usize,
    pub models_with_table: usize,
    pub chain_entries: usize,
    pub chain_entries_from_concerns: usize,
    pub chain_entries_framework_generated: usize,
    pub declared_in_source: BTreeMap<String, usize>,
    pub callback_methods: usize,
    pub callback_methods_lowered: usize,
    pub callback_methods_residual: usize,
    pub object_or_lambda_callbacks: usize,
    pub residual_reasons: BTreeMap<String, usize>,
    /// Effective chain entries carrying `if:` / `unless:` (OGAR's lift drops
    /// these options).
    pub conditional_entries: usize,
    /// Distinct callback/validation declaration sites reached by some model.
    pub source_sites: usize,
    /// …of which live in a module (concern) rather than a class body.
    pub source_sites_in_modules: usize,
    /// Callback declarations ruff's shipped class-body walk emits
    /// (`Model::callbacks`, the `has_callback` triples).
    pub ruff_has_callback_triples: usize,
    pub ogar_callbacks: usize,
}

/// `String#camelize` with no acronyms defined.
#[must_use]
pub fn camelize(s: &str) -> String {
    camelize_with(s, &BTreeMap::new())
}

/// `String#capitalize`: first character upper case, the rest lower case.
fn capitalize(w: &str) -> String {
    let mut c = w.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect())
}

/// ActiveSupport's `camelize(term)` (upper first letter) under `acronyms`,
/// keyed by the lower-case form as `inflect.acronym` stores them.
#[must_use]
pub fn camelize_with(s: &str, acronyms: &BTreeMap<String, String>) -> String {
    let word = |w: &str| acronyms.get(w).cloned().unwrap_or_else(|| capitalize(w));
    let lead = s.find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit())).unwrap_or(s.len());
    let mut out = word(&s[..lead]);
    let mut rest = &s[lead..];
    // `gsub(/(?:_|(\/))([a-z\d]*)/i)`: each `_` or `/` and the run after it.
    while let Some(sep) = rest.find(['_', '/']) {
        out.push_str(&rest[..sep]);
        let after = &rest[sep + 1..];
        let end = after.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(after.len());
        if rest.as_bytes()[sep] == b'/' {
            out.push_str("::");
        }
        out.push_str(&word(&after[..end]));
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// ActiveSupport's default English singular rules, irregulars and
/// uncountables (`active_support/inflections.rb`), in definition order. Later
/// rules win, as in ActiveSupport. An application's own initializer rules are
/// not included.
const SINGULAR_RULES: &[(&str, &str)] = &[
    (r"(?i)s$", ""),
    (r"(?i)(ss)$", "${1}"),
    (r"(?i)(n)ews$", "${1}ews"),
    (r"(?i)([ti])a$", "${1}um"),
    (r"(?i)((a)naly|(b)a|(d)iagno|(p)arenthe|(p)rogno|(s)ynop|(t)he)(sis|ses)$", "${1}sis"),
    (r"(?i)(^analy)(sis|ses)$", "${1}sis"),
    (r"(?i)([^f])ves$", "${1}fe"),
    (r"(?i)(hive)s$", "${1}"),
    (r"(?i)(tive)s$", "${1}"),
    (r"(?i)([lr])ves$", "${1}f"),
    (r"(?i)([^aeiouy]|qu)ies$", "${1}y"),
    (r"(?i)(s)eries$", "${1}eries"),
    (r"(?i)(m)ovies$", "${1}ovie"),
    (r"(?i)(x|ch|ss|sh)es$", "${1}"),
    (r"(?i)^(m|l)ice$", "${1}ouse"),
    (r"(?i)(bus)(es)?$", "${1}"),
    (r"(?i)(o)es$", "${1}"),
    (r"(?i)(shoe)s$", "${1}"),
    (r"(?i)(cris|test)(is|es)$", "${1}is"),
    (r"(?i)^(a)x[ie]s$", "${1}xis"),
    (r"(?i)(octop|vir)(us|i)$", "${1}us"),
    (r"(?i)(alias|status)(es)?$", "${1}"),
    (r"(?i)^(ox)en", "${1}"),
    (r"(?i)(vert|ind)ices$", "${1}ex"),
    (r"(?i)(matr)ices$", "${1}ix"),
    (r"(?i)(quiz)zes$", "${1}"),
    (r"(?i)(database)s$", "${1}"),
    // irregular(singular, plural) adds `/(p)lural$/i -> '\1ingular'` and keeps
    // the singular unchanged.
    (r"(?i)(p)eople$", "${1}erson"),
    (r"(?i)(p)erson$", "${1}erson"),
    (r"(?i)(m)en$", "${1}an"),
    (r"(?i)(m)an$", "${1}an"),
    (r"(?i)(c)hildren$", "${1}hild"),
    (r"(?i)(c)hild$", "${1}hild"),
    (r"(?i)(s)exes$", "${1}ex"),
    (r"(?i)(s)ex$", "${1}ex"),
    (r"(?i)(m)oves$", "${1}ove"),
    (r"(?i)(m)ove$", "${1}ove"),
    (r"(?i)(z)ombies$", "${1}ombie"),
    (r"(?i)(z)ombie$", "${1}ombie"),
];

const UNCOUNTABLE: &[&str] =
    &["equipment", "information", "rice", "money", "species", "series", "fish", "sheep", "jeans", "police"];

fn singular_rules() -> &'static [(regex::Regex, &'static str)] {
    static RULES: std::sync::OnceLock<Vec<(regex::Regex, &'static str)>> = std::sync::OnceLock::new();
    RULES.get_or_init(|| {
        SINGULAR_RULES
            .iter()
            .rev()
            .map(|&(re, to)| (regex::Regex::new(re).unwrap_or_else(|e| panic!("singular rule {re}: {e}")), to))
            .collect()
    })
}

/// The singular rules `inflect.irregular(singular, plural)` adds, highest
/// priority first: ActiveSupport prepends them, so the last added is tried first.
fn irregular_rules(irregulars: &[(String, String)]) -> Vec<(regex::Regex, String)> {
    let esc = |s: &str| regex::escape(s);
    let lit = |s: &str| s.replace('$', "$$");
    let mut added: Vec<(String, String)> = Vec::new();
    for (singular, plural) in irregulars {
        let (Some(s0), Some(p0)) = (singular.chars().next(), plural.chars().next()) else { continue };
        let (srest, prest) = (&singular[s0.len_utf8()..], &plural[p0.len_utf8()..]);
        if s0.to_uppercase().eq(p0.to_uppercase()) {
            added.push((format!("(?i)({}){}$", esc(&s0.to_string()), esc(srest)), format!("${{1}}{}", lit(srest))));
            added.push((format!("(?i)({}){}$", esc(&p0.to_string()), esc(prest)), format!("${{1}}{}", lit(srest))));
        } else {
            let (su, sl) = (s0.to_uppercase().to_string(), s0.to_lowercase().to_string());
            let (pu, pl) = (p0.to_uppercase().to_string(), p0.to_lowercase().to_string());
            for (first, rest, to) in [(&su, srest, &su), (&sl, srest, &sl), (&pu, prest, &su), (&pl, prest, &sl)] {
                added.push((format!("{}(?i:{})$", esc(first), esc(rest)), format!("{}{}", lit(to), lit(srest))));
            }
        }
    }
    added
        .into_iter()
        .rev()
        .map(|(re, to)| (regex::Regex::new(&re).unwrap_or_else(|e| panic!("irregular rule {re}: {e}")), to))
        .collect()
}

/// `String#singularize` under ActiveSupport's default English rules.
#[must_use]
pub fn singular(s: &str) -> String {
    singular_with(s, &[])
}

/// `String#singularize` after the application's initializer ran
/// `inflect.irregular(singular, plural)` for each of `irregulars`, in order.
/// Those rules take priority over the defaults, and both words stop being
/// uncountable.
#[must_use]
pub fn singular_with(s: &str, irregulars: &[(String, String)]) -> String {
    let lower = s.to_ascii_lowercase();
    // ActiveSupport tests `/\b#{word}\Z/i`; `_` is a word character, so
    // `user_information` is not uncountable.
    let uncountable = UNCOUNTABLE.iter().any(|u| {
        !irregulars.iter().any(|(a, b)| a == u || b == u)
            && lower.strip_suffix(u).is_some_and(|rest| rest.is_empty() || !rest.ends_with(|c: char| c.is_alphanumeric() || c == '_'))
    });
    if s.is_empty() || uncountable {
        return s.to_string();
    }
    let own = irregular_rules(irregulars);
    for (re, to) in own.iter().map(|(r, t)| (r, t.as_str())).chain(singular_rules().iter().map(|(r, t)| (r, *t))) {
        if re.is_match(s) {
            return re.replace(s, to).into_owned();
        }
    }
    s.to_string()
}

#[must_use]
pub fn unquote(s: &str) -> String {
    s.trim().trim_matches(|c| c == '"' || c == '\'' || c == ':').to_string()
}

#[must_use]
pub fn module_name(model: &str) -> String {
    underscore(model).replace('/', "_")
}

/// The class an association points at, resolved like ActiveRecord's
/// `compute_type`: `class_name:` if given, else the camelized (singular)
/// name, looked up from the declaring model's namespace outward.
pub fn assoc_target(h: &Harvest, model: &str, a: &semantic::Assoc) -> String {
    let written = a.options.get("class_name").map_or_else(
        || {
            let base = if matches!(a.kind.as_str(), "has_many" | "has_and_belongs_to_many") {
                h.singular(&a.name)
            } else {
                a.name.clone()
            };
            h.camelize(&base)
        },
        |c| unquote(c),
    );
    h.resolve(&written, &[model.to_string()]).unwrap_or(written)
}

fn phase_names(on: &Option<Vec<rails_polyfill::Action>>) -> Option<Vec<String>> {
    on.as_ref().map(|v| {
        v.iter()
            .map(|a| match a {
                rails_polyfill::Action::Create => "create".to_string(),
                rails_polyfill::Action::Update => "update".to_string(),
                rails_polyfill::Action::Destroy => "destroy".to_string(),
            })
            .collect()
    })
}

/// Classes whose body sets `self.table_name` to a literal, by class name.
fn explicit_tables(asm: &Assembler<'_>, h: &Harvest) -> BTreeMap<String, String> {
    h.units.keys().filter_map(|n| asm.explicit_table_name(n).map(|t| (n.clone(), t))).collect()
}

const AR_INTERNAL: &[&str] = &["normalize_changed_in_place_attributes", "_ensure_no_duplicate_errors"];

pub fn compile(cfg: &Loaded) -> Result<Compilation, String> {
    let app = &cfg.config.app;
    let h = Harvest::collect(&cfg.source, &app.autoload_roots, &app.namespace);
    let (graph, _schema_report) = ruff_ruby_spo::extract_app_with_schema(&cfg.source, &app.namespace);
    let schema = Schema::read(&cfg.source, &app.migrations)?;
    let fw = rules::FrameworkDefaults::for_version(app.rails_load_defaults.as_deref());
    let mut asm = Assembler::new(&h, fw, app.framework_roots.clone());

    let is_model = |n: &str| h.units.get(n).is_some_and(|us| us.iter().any(|u| u.kind == harvest::UnitKind::Class));
    let explicit = explicit_tables(&asm, &h);
    let tbl = |n: &str| explicit.get(n).cloned().unwrap_or_else(|| table_name(n, &is_model, &h.irregulars));

    // Expand every model in the slice.
    let slice = cfg.config.slice.models.clone();
    let mut expanded: BTreeMap<String, Expanded> = BTreeMap::new();
    for m in &slice {
        if !h.units.contains_key(m) {
            return Err(format!("slice model {m} not found in the harvest"));
        }
        expanded.insert(m.clone(), asm.expand(m));
    }

    let infos: BTreeMap<String, ModelInfo> = slice
        .iter()
        .map(|m| {
            let e = &expanded[m];
            let t = tbl(m);
            let columns = schema
                .tables
                .get(&t)
                .map(|t| t.columns.iter().map(|c| c.name.clone()).collect())
                .unwrap_or_default();
            let belongs_to = e
                .assocs
                .iter()
                .filter(|a| a.kind == "belongs_to")
                .map(|a| {
                    let fk = a.options.get("foreign_key").map_or_else(|| format!("{}_id", a.name), |f| unquote(f));
                    (a.name.clone(), (fk, assoc_target(&h, m, a)))
                })
                .collect();
            (
                m.clone(),
                ModelInfo {
                    name: m.clone(),
                    module: module_name(m),
                    columns,
                    belongs_to,
                    ancestors: e.ancestors.clone(),
                    virtuals: e.virtuals.clone(),
                },
            )
        })
        .collect();

    let verbatim: BTreeSet<&str> = cfg.verbatim.iter().map(String::as_str).collect();
    let mut models = Vec::new();
    for m in &slice {
        let e = &expanded[m];
        let me = &infos[m];
        let table = tbl(m);
        let Some(tdef) = schema.tables.get(&table) else {
            return Err(format!("{m}: table {table} not found in the configured migrations"));
        };
        let mut lw = Lowerer::new(cfg, &h, &asm, &infos, me);
        let mut probe = Lowerer::new(cfg, &h, &asm, &infos, me);
        let mut chain = Vec::new();
        let mut conds_code: Vec<(String, String)> = Vec::new();
        for (phase, d) in e.chains.execution_order() {
            let name = d.filter.name();
            let mut would_lower = None;
            let binding = match &d.filter {
                rules::Filter::Method(meth) if d.rule != "source" || AR_INTERNAL.contains(&meth.as_str()) => Binding::Framework,
                rules::Filter::Method(meth) if verbatim.contains(meth.as_str()) => match lw.method(meth) {
                    Ok(f) => Binding::Lowered(f),
                    Err(u) => Binding::Residual(format!("{} at {}", u.what, u.at)),
                },
                rules::Filter::Method(meth) => {
                    would_lower = Some(probe.method(meth).map(|_| ()).map_err(|u| u.what));
                    Binding::Host
                }
                rules::Filter::Validator(rules::ValidatorSpec::Presence(a)) if me.columns.contains(a) => Binding::Validator(format!("presence:{a}")),
                rules::Filter::Validator(rules::ValidatorSpec::BelongsToRequired { assoc, only_if_fk_missing_or_changed }) => {
                    match me.belongs_to.get(assoc) {
                        Some((fk, target)) if infos.contains_key(target) && me.columns.contains(fk) => {
                            Binding::Validator(format!("belongs_to_required:{assoc}:{fk}:{target}:{only_if_fk_missing_or_changed}"))
                        }
                        Some((_, target)) => Binding::Residual(format!("required belongs_to :{assoc} targets {target}, outside the slice")),
                        None => Binding::Residual(format!("required belongs_to :{assoc} not resolved")),
                    }
                }
                rules::Filter::Validator(rules::ValidatorSpec::Length { attr, min, max }) if me.columns.contains(attr) => {
                    Binding::Validator(format!("length:{attr}:{}:{}", min.map_or("-".into(), |v| v.to_string()), max.map_or("-".into(), |v| v.to_string())))
                }
                _ => Binding::Host,
            };
            // Conditions decide whether a callback fires, so they are lowered
            // whatever the binding of the callback itself.
            let mut conds = Vec::new();
            for c in &d.conds {
                let (kind, f) = match c {
                    rules::CondSpec::If(f) => ("if", f),
                    rules::CondSpec::Unless(f) => ("unless", f),
                };
                let code = match f {
                    rules::Filter::Method(meth) => match meth.as_str() {
                        "new_record?" | "persisted?" => format!("builtin:{meth}"),
                        _ => match lw.method(meth) {
                            Ok(fname) => format!("fn:{fname}"),
                            Err(u) => format!("residual:{} at {}", u.what, u.at),
                        },
                    },
                    rules::Filter::Lambda { id, .. } => match e.lambdas.get(id) {
                        Some((uname, file, body)) => {
                            let unit = h.units.get(uname).and_then(|us| us.iter().find(|u| &u.file == file));
                            let ident = format!("lam_{}", conds_code.len());
                            match unit.map(|u| lw.block(u, body.as_ref(), &ident, id)) {
                                Some(Ok(fname)) => {
                                    conds_code.push((ident, id.clone()));
                                    format!("fn:{fname}")
                                }
                                Some(Err(u)) => format!("residual:{} at {}", u.what, u.at),
                                None => format!("residual:lambda unit not found for {id}"),
                            }
                        }
                        None => format!("residual:lambda body not harvested for {id}"),
                    },
                    other => format!("residual:condition {other:?}"),
                };
                conds.push((kind.to_string(), code));
            }
            chain.push(ChainEntry {
                phase: phase.as_str().to_string(),
                name,
                on: phase_names(&d.on),
                conds,
                binding,
                rule: d.rule.to_string(),
                provenance: d.provenance.clone(),
                would_lower,
            });
        }
        let ogar = graph.models.iter().find(|x| &x.name == m).map_or_else(OgarView::default, |rm| {
            let c = ogar_from_ruff::lift_model(rm);
            OgarView {
                lifted: true,
                name: c.name.clone(),
                parent: c.parent.clone(),
                associations: c.associations.len(),
                callbacks: c.callbacks.len(),
                validations: c.validations.len(),
                mixins: c.mixins.len(),
            }
        });
        let mut navigable = Vec::new();
        let mut not_navigable = Vec::new();
        for a in &e.assocs {
            let target = assoc_target(&h, m, a);
            let Some(ti) = infos.get(&target) else {
                not_navigable.push((a.name.clone(), format!("target {target} outside the slice")));
                continue;
            };
            if a.options.contains_key("through") {
                not_navigable.push((a.name.clone(), "has_many :through".into()));
                continue;
            }
            if a.options.contains_key("as") || a.options.get("polymorphic").is_some_and(|v| v == "true") {
                not_navigable.push((a.name.clone(), "polymorphic".into()));
                continue;
            }
            let fk = a.options.get("foreign_key").map(|f| unquote(f));
            let (fk, owner_cols) = match a.kind.as_str() {
                "belongs_to" => (fk.unwrap_or_else(|| format!("{}_id", a.name)), &me.columns),
                _ => (
                    fk.unwrap_or_else(|| format!("{}_id", underscore(m.rsplit("::").next().unwrap_or(m)))),
                    &ti.columns,
                ),
            };
            if !owner_cols.contains(&fk) {
                not_navigable.push((a.name.clone(), format!("foreign key {fk} not in schema")));
                continue;
            }
            if a.kind == "has_and_belongs_to_many" {
                not_navigable.push((a.name.clone(), "habtm join table".into()));
                continue;
            }
            navigable.push((a.name.clone(), a.kind.clone(), fk, target, a.order.clone(), a.provenance.clone()));
        }
        let code = std::mem::take(&mut lw.out);
        let source_files = h
            .units
            .get(m)
            .map(|us| {
                us.iter()
                    .filter_map(|u| h.file(&u.file).map(|f| f.rel.clone()))
                    .collect()
            })
            .unwrap_or_default();
        models.push(CompiledModel {
            name: m.clone(),
            module: me.module.clone(),
            table: table.clone(),
            table_provenance: tdef.provenance.clone(),
            source_files,
            ancestors: e.ancestors.clone(),
            chain,
            ogar,
            lowered_methods: code.spans.clone(),
            residual_methods: code
                .failed
                .iter()
                .map(|(k, v)| (k.clone(), (v.what.clone(), v.at.clone())))
                .collect(),
            host_calls: code.host_calls.clone(),
            aliases_used: code.aliases_used.clone(),
            dynamic_attr: code.dynamic_attr.clone(),
            folded: code.folded.clone(),
            unresolved: e.unresolved.clone(),
            stores: e.stores.clone(),
            navigable,
            not_navigable,
            code,
            conds: conds_code,
        });
    }

    // Relation semantics over the slice.
    let inputs: Vec<RelationInput<'_>> = slice
        .iter()
        .map(|m| {
            let e = &expanded[m];
            let sup = asm.superclass(m);
            let sup_abstract = sup.as_deref().is_some_and(|s| asm.is_abstract(s));
            let sup_type = sup
                .as_deref()
                .and_then(|s| schema.tables.get(&tbl(s)))
                .is_some_and(|t| t.columns.iter().any(|c| c.name == "type"));
            RelationInput {
                model: m,
                superclass: sup,
                superclass_abstract: sup_abstract,
                superclass_has_type_column: sup_type,
                included: &e.included,
                assocs: &e.assocs,
            }
        })
        .collect();
    let fk_required = |model: &str, fk: &str| -> (bool, Vec<String>) {
        let mut why = Vec::new();
        if let Some(c) = schema.tables.get(&tbl(model)).and_then(|t| t.columns.iter().find(|c| c.name == fk))
            && !c.null
        {
            why.push(format!("{}.{fk} NOT NULL ({})", tbl(model), c.provenance));
        }
        if let Some(cm) = models.iter().find(|x| x.name == model)
            && cm.chain.iter().any(|c| c.name == format!("validates:presence:{fk}"))
        {
            why.push(format!("validates :{fk}, presence: true"));
        }
        (!why.is_empty(), why)
    };
    let relations = semantic::relations(&inputs, &|m, a| assoc_target(&h, m, a), &fk_required);

    let schema_cross_check = schema::cross_check(&graph, &schema, &tbl, &slice);
    let corpus = corpus_coverage(cfg, &h, &graph, &schema, &tbl, &mut asm);
    Ok(Compilation {
        app: app.name.clone(),
        models,
        relations,
        schema_cross_check,
        schema_unparsed: schema.unparsed_column_calls.clone(),
        corpus,
        harvest_parse_failures: h.parse_failures.clone(),
        schema,
    })
}

/// The OGAR classes for `models`, and the classes whose typed columns had to
/// be filled in from this compiler's own schema arm.
#[derive(Debug, Default)]
pub struct OgarClasses {
    pub classes: Vec<ogar_vocab::Class>,
    /// Per model, the columns ruff's classic migration replay attached no
    /// typed field for (namespaced tables such as `Ticket::State`, or a
    /// partly replayed table); they come from [`Schema`]. An upstream gap,
    /// recorded and not hidden.
    pub ruff_schema_gap_filled: BTreeMap<String, Vec<String>>,
    /// Per lifted model, its table, when that table is not in the configured
    /// migrations: an absent column there is not evidence the column does not
    /// exist.
    pub unharvested_tables: BTreeMap<String, String>,
    /// Per lifted model, every column of its table with the schema's NOT NULL.
    /// OGAR's `Association` carries no column nullability, so this is the only
    /// source for a foreign key's.
    pub column_not_null: BTreeMap<String, BTreeMap<String, bool>>,
    /// Per model, the typed ruff fields whose NOT NULL the configured baseline
    /// migrations overrode, as `(column, ruff_not_null, schema_not_null)`.
    pub ruff_nullability_overridden: BTreeMap<String, Vec<(String, bool, bool)>>,
    /// Requested models ruff's graph does not contain.
    pub missing: Vec<String>,
    /// Per lifted model, the table Rails names for it (an explicit
    /// `self.table_name`, else the naming convention), harvested or not.
    pub tables: BTreeMap<String, String>,
}

/// Which concern-provided associations a shadow lift adds to the lifted class.
/// Never used by the lift the basis and the matrices are built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowLift {
    /// Every concern-provided association, in Rails' `_reflections` order.
    AllKinds,
    /// Every concern-provided association, appended after the class's own (the
    /// order of the reverted experiment; explains the `Checklist::Item` move).
    AllKindsAppended,
    BelongsToOnly,
    WithoutBelongsTo,
}

fn lift_models(cfg: &Loaded, models: Option<&[String]>) -> Result<OgarClasses, String> {
    lift_models_with(cfg, models, None)
}

/// OGAR classes for every model with the effective concern-provided
/// associations of `mode` added to each lifted class, in Rails' `_reflections`
/// order. In memory only: the shadow behind the concern-association report.
pub fn ogar_classes_all_shadow(cfg: &Loaded, mode: ShadowLift) -> Result<OgarClasses, String> {
    lift_models_with(cfg, None, Some(mode))
}

fn lift_models_with(cfg: &Loaded, models: Option<&[String]>, shadow: Option<ShadowLift>) -> Result<OgarClasses, String> {
    let app = &cfg.config.app;
    let h = Harvest::collect(&cfg.source, &app.autoload_roots, &app.namespace);
    let (graph, _) = ruff_ruby_spo::extract_app_with_schema(&cfg.source, &app.namespace);
    let schema = Schema::read(&cfg.source, &app.migrations)?;
    let is_model = |n: &str| h.units.get(n).is_some_and(|us| us.iter().any(|u| u.kind == harvest::UnitKind::Class));
    let mut asm = Assembler::new(&h, rules::FrameworkDefaults::for_version(app.rails_load_defaults.as_deref()), app.framework_roots.clone());
    let explicit = explicit_tables(&asm, &h);
    let tbl = |n: &str| explicit.get(n).cloned().unwrap_or_else(|| table_name(n, &is_model, &h.irregulars));
    let wanted: Vec<String> = models.map_or_else(|| graph.models.iter().map(|m| m.name.clone()).collect(), <[String]>::to_vec);
    let mut out = OgarClasses::default();
    for m in &wanted {
        let Some(model) = graph.models.iter().find(|x| &x.name == m) else {
            out.missing.push(m.clone());
            continue;
        };
        let mut model = model.clone();
        let table = tbl(m);
        out.tables.insert(m.clone(), table.clone());
        if let Some(t) = schema.tables.get(&table) {
            let mut filled = Vec::new();
            let mut overridden = Vec::new();
            for c in &t.columns {
                if let Some(f) = model.fields.iter_mut().find(|f| f.field_type.is_some() && f.name == c.name) {
                    // ruff replays every migration, including upgrade-only ones guarded at
                    // runtime (Zammad's `system_init_done`); the configured baseline is the
                    // schema a fresh install has, so its NOT NULL wins.
                    let ruff = f.not_null.unwrap_or(false);
                    if ruff != !c.null {
                        f.not_null = Some(!c.null);
                        overridden.push((c.name.clone(), ruff, !c.null));
                    }
                    continue;
                }
                // An untyped ruff field of the same name is replaced; a typed one never is.
                model.fields.retain(|f| f.name != c.name);
                model.fields.push(ruff_spo_triplet::Field {
                    name: c.name.clone(),
                    field_type: Some(c.ty.clone()),
                    not_null: Some(!c.null),
                    ..Default::default()
                });
                filled.push(c.name.clone());
            }
            out.column_not_null.insert(m.clone(), t.columns.iter().map(|c| (c.name.clone(), !c.null)).collect());
            if !filled.is_empty() {
                out.ruff_schema_gap_filled.insert(m.clone(), filled);
            }
            if !overridden.is_empty() {
                out.ruff_nullability_overridden.insert(m.clone(), overridden);
            }
        } else {
            out.unharvested_tables.insert(m.clone(), table);
        }
        if let Some(mode) = shadow {
            shadow_associations(&h, &mut asm, m, &mut model, mode);
        }
        out.classes.push(ogar_from_ruff::lift_model(&model));
    }
    Ok(out)
}

/// Adds the concern-provided effective associations to the ruff model and
/// orders its associations as Rails' `_reflections` does.
pub fn shadow_associations(h: &Harvest, asm: &mut Assembler<'_>, name: &str, model: &mut ruff_spo_triplet::Model, mode: ShadowLift) {
    use ruff_spo_triplet::{AssocDecl, AssocKind};
    let eff = shadow::Shadow::new(h).effective(asm, name);
    let mut ordered: Vec<AssocDecl> = Vec::new();
    let mut replaced: Vec<String> = Vec::new();
    for a in &eff.assocs {
        let kind = match a.kind.as_str() {
            "belongs_to" => AssocKind::BelongsTo,
            "has_many" => AssocKind::HasMany,
            "has_one" => AssocKind::HasOne,
            _ => AssocKind::HasAndBelongsToMany,
        };
        let concern = matches!(a.origin, shadow::Origin::Concern { .. });
        let wanted = match mode {
            ShadowLift::AllKinds | ShadowLift::AllKindsAppended => true,
            ShadowLift::BelongsToOnly => kind == AssocKind::BelongsTo,
            ShadowLift::WithoutBelongsTo => kind != AssocKind::BelongsTo,
        };
        // `option_splats`: the effective-association scan (`shadow::EffectiveAssoc`) keeps the
        // option hash as a map and does not record `**splat` entries, so a lifted concern
        // declaration carries none. A concern association written with a splat is not modelled.
        let effective = || AssocDecl {
            kind,
            name: a.name.clone(),
            options: a.options.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            option_splats: Vec::new(),
        };
        match model.associations.iter().find(|x| x.name == a.name) {
            // The concern's declaration won (`add_reflection`: the later one
            // replaces the class's): lift it, not the class's.
            Some(_) if concern && wanted => {
                replaced.push(a.name.clone());
                ordered.push(effective());
            }
            Some(own) => ordered.push(own.clone()),
            None if concern && wanted => ordered.push(effective()),
            None => {}
        }
    }
    for own in &model.associations {
        if !ordered.iter().any(|x| x.name == own.name) {
            ordered.push(own.clone());
        }
    }
    if mode == ShadowLift::AllKindsAppended {
        // Own first, then the concern-provided ones in effective order.
        let own: Vec<AssocDecl> = model.associations.iter().filter(|o| !replaced.contains(&o.name)).cloned().collect();
        let mut appended: Vec<AssocDecl> = ordered.into_iter().filter(|a| !own.iter().any(|o| o.name == a.name)).collect();
        let mut all = own;
        all.append(&mut appended);
        ordered = all;
    }
    model.associations = ordered;
}

/// OGAR classes for `models`, with the gap fill and the missing models
/// reported.
pub fn ogar_classes_report(cfg: &Loaded, models: &[String]) -> Result<OgarClasses, String> {
    lift_models(cfg, Some(models))
}

/// OGAR classes for every model ruff's graph holds.
pub fn ogar_classes_all(cfg: &Loaded) -> Result<OgarClasses, String> {
    lift_models(cfg, None)
}

/// OGAR classes for `models`: ruff's model, columns filled from the
/// compiler's schema where ruff has none, lifted by `ogar_from_ruff`. A model
/// ruff does not know is an error.
pub fn ogar_classes(cfg: &Loaded, models: &[String]) -> Result<Vec<ogar_vocab::Class>, String> {
    let r = lift_models(cfg, Some(models))?;
    if r.missing.is_empty() {
        Ok(r.classes)
    } else {
        Err(format!("not found in the ruff model graph: {}", r.missing.join(", ")))
    }
}

/// The whole-application measurement alone, for an application with no
/// compiled slice (the generalization runs on a second corpus).
pub fn corpus(cfg: &Loaded) -> Result<CorpusCoverage, String> {
    let app = &cfg.config.app;
    let h = Harvest::collect(&cfg.source, &app.autoload_roots, &app.namespace);
    let (graph, _) = ruff_ruby_spo::extract_app_with_schema(&cfg.source, &app.namespace);
    let schema = Schema::read(&cfg.source, &app.migrations)?;
    let fw = rules::FrameworkDefaults::for_version(app.rails_load_defaults.as_deref());
    let mut asm = Assembler::new(&h, fw, app.framework_roots.clone());
    let is_model = |n: &str| h.units.get(n).is_some_and(|us| us.iter().any(|u| u.kind == harvest::UnitKind::Class));
    let explicit = explicit_tables(&asm, &h);
    let tbl = |n: &str| explicit.get(n).cloned().unwrap_or_else(|| table_name(n, &is_model, &h.irregulars));
    Ok(corpus_coverage(cfg, &h, &graph, &schema, &tbl, &mut asm))
}

/// Expand every class that reaches ActiveRecord and try to lower every
/// callback method it runs. Measurement only; nothing here is emitted.
fn corpus_coverage(
    cfg: &Loaded,
    h: &Harvest,
    graph: &ruff_spo_triplet::ModelGraph,
    schema: &Schema,
    tbl: &dyn Fn(&str) -> String,
    asm: &mut Assembler<'_>,
) -> CorpusCoverage {
    let mut cov = CorpusCoverage {
        classes_harvested: h.units.values().filter(|us| us.iter().any(|u| u.kind == harvest::UnitKind::Class)).count(),
        modules_harvested: h.units.values().filter(|us| us.iter().all(|u| u.kind == harvest::UnitKind::Module)).count(),
        concern_modules: h.units.values().filter(|us| us.iter().any(|u| u.concern)).count(),
        ..Default::default()
    };
    // A model: a class whose superclass chain ends at a framework root.
    let names: Vec<String> = h.units.keys().cloned().collect();
    let mut model_names = Vec::new();
    for n in &names {
        let mut cur = n.clone();
        let mut depth = 0;
        let is_model = loop {
            depth += 1;
            if depth > 20 {
                break false;
            }
            match asm.superclass(&cur) {
                Some(s) if cfg.config.app.framework_roots.contains(&s) => break !asm.is_abstract(n),
                Some(s) if h.units.contains_key(&s) => cur = s,
                _ => break false,
            }
        };
        if is_model {
            model_names.push(n.clone());
        }
    }
    cov.models = model_names.len();
    let mut infos = BTreeMap::new();
    for m in &model_names {
        let e = asm.expand(m);
        // Columns from the migration arm when the table is in the configured
        // migrations, otherwise from ruff's own schema stratum.
        let cols: Vec<String> = schema.tables.get(&tbl(m)).map_or_else(
            || {
                graph
                    .models
                    .iter()
                    .find(|x| &x.name == m)
                    .map(|x| {
                        let mut v: Vec<String> = vec!["id".into()];
                        v.extend(x.fields.iter().filter(|f| f.field_type.is_some()).map(|f| f.name.clone()));
                        v
                    })
                    .unwrap_or_default()
            },
            |t| t.columns.iter().map(|c| c.name.clone()).collect(),
        );
            if cols.len() > 1 {
            cov.models_with_table += 1;
        }
        infos.insert(
            m.clone(),
            ModelInfo {
                name: m.clone(),
                module: module_name(m),
                columns: cols,
                belongs_to: e
                    .assocs
                    .iter()
                    .filter(|a| a.kind == "belongs_to")
                    .map(|a| (a.name.clone(), (format!("{}_id", a.name), assoc_target(h, m, a))))
                    .collect(),
                ancestors: e.ancestors.clone(),
                virtuals: e.virtuals.clone(),
            },
        );
    }
    let asm_ref: &Assembler<'_> = asm;
    let mut sites: BTreeSet<String> = BTreeSet::new();
    let mut module_sites: BTreeSet<String> = BTreeSet::new();
    let module_files: BTreeSet<String> = h
        .units
        .values()
        .flatten()
        .filter(|u| u.kind == harvest::UnitKind::Module)
        .filter_map(|u| h.file(&u.file).map(|f| f.rel.clone()))
        .collect();
    for m in &model_names {
        let e = asm_ref_expand_cached(asm_ref, m);
        let Some(e) = e else { continue };
        let me = &infos[m];
        let mut lw = Lowerer::new(cfg, h, asm_ref, &infos, me);
        let own_files: BTreeSet<String> = h.units[m]
            .iter()
            .filter_map(|u| h.file(&u.file).map(|f| f.rel.clone()))
            .collect();
        for (_, d) in e.chains.execution_order() {
            cov.chain_entries += 1;
            if d.rule != "source" {
                cov.chain_entries_framework_generated += 1;
                continue;
            }
            *cov.declared_in_source.entry(format!("{:?}", d.chain)).or_default() += 1;
            if !d.conds.is_empty() {
                cov.conditional_entries += 1;
            }
            sites.insert(d.provenance.clone());
            if module_files.contains(d.provenance.rsplit_once(':').map_or("", |(f, _)| f)) {
                module_sites.insert(d.provenance.clone());
            }
            let prov_file = d.provenance.rsplit_once(':').map_or(d.provenance.as_str(), |(f, _)| f);
            if !own_files.contains(prov_file) {
                cov.chain_entries_from_concerns += 1;
            }
            match &d.filter {
                rules::Filter::Method(meth) if !AR_INTERNAL.contains(&meth.as_str()) => {
                    cov.callback_methods += 1;
                    match lw.method(meth) {
                        Ok(_) => cov.callback_methods_lowered += 1,
                        Err(u) => {
                            cov.callback_methods_residual += 1;
                            *cov.residual_reasons.entry(reason_class(&u.what)).or_default() += 1;
                        }
                    }
                }
                rules::Filter::Object(_) | rules::Filter::Lambda { .. } => cov.object_or_lambda_callbacks += 1,
                _ => {}
            }
        }
    }
    cov.source_sites = sites.len();
    cov.source_sites_in_modules = module_sites.len();
    cov.ruff_has_callback_triples = graph.models.iter().map(|m| m.callbacks.len()).sum();
    cov.ogar_callbacks = graph.models.iter().map(|m| ogar_from_ruff::lift_model(m).callbacks.len()).sum();
    cov
}

fn asm_ref_expand_cached(asm: &Assembler<'_>, m: &str) -> Option<Expanded> {
    asm.cached(m)
}

/// Bucket a lowering failure by its first clause so the histogram is small.
fn reason_class(what: &str) -> String {
    let w = what.split(", which does not lower:").last().unwrap_or(what).trim();
    let w = w.split(" at ").next().unwrap_or(w);
    let mut parts = w.split_whitespace();
    let first = parts.next().unwrap_or("");
    match first {
        "class" => "class method outside the slice/aliases".into(),
        "call" => "call to an own helper that does not lower / is unknown".into(),
        "node" => format!("node {}", parts.next().unwrap_or("")),
        "method" => "method not found in ancestors".into(),
        _ if w.starts_with('`') && w.contains("with arguments") => "method with arguments on a value".into(),
        _ if w.starts_with('`') => "attribute/method on a record outside the slice".into(),
        _ => w.chars().take(60).collect(),
    }
}

pub use rails_polyfill::rules;
