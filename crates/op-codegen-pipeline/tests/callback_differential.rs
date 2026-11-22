//! The AR arm of the behaviour differential: the callbacks and validations
//! ActiveRecord actually ran when real OpenProject executed
//! `WorkPackages::CreateService` (`oracle/work-package-service-trace.json`),
//! compared with the declarations ruff harvests from the Rails class bodies
//! (`ruff_ruby_spo::events`, ordered, with `if:` / `unless:` / `on:` options).
//!
//! Declarations come from `ruff_ruby_spo::events::source_ore_from` (classes and
//! concern modules, with `included do` bodies, `includes` and `superclass`).
//! Callbacks ActiveRecord generates from association macros come from
//! `ruff_ruby_spo::rails_generated::derive`; they match like declared ones but
//! are counted apart and carry the association and the Rails rule that
//! registered them. Rules `derive` cannot decide are `Slag::GeneratedUndecided`.
//!
//! The test also rebuilds the order Rails registers each chain in (superclass
//! first, `include` at its statement, a concern's dependencies before its
//! `included` block, generated callbacks at their association) and compares it
//! with the order the trace recorded. Differences are printed, not asserted.
//!
//! Engine patches (`patches %i[WorkPackage]`, `include_module ... into:`) are
//! applied from `oracle/intake.json`: ruff finds the calls (`DslCall`), the rules
//! in the data file derive the patch module and when it runs, and its
//! declarations are spliced after the model's own. A call the rules cannot
//! resolve gets a [`PatchSlag`], never a silent skip.
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

use ruff_ruby_spo::events::{source_ore_from, ClassOre, DeclOrigin, RailsEvent, Symbols};
use ruff_ruby_spo::rails_generated::{
    derive, Chain as GenChain, Filter as GenFilter, Generated, Kind as GenKind, RailsDefaults,
};
use ruff_ruby_spo::Declaration;
use ruff_spo_triplet::{AssocKind, ConcernKind, ValidationKind};
use serde::Deserialize;
use serde_json::Value;

/// Distinct journal entries matched to a declaration, measured on the committed trace.
const MATCHED_FLOOR: usize = 91;

/// Of those, entries that match only through a generated callback.
const MATCHED_GENERATED_FLOOR: usize = 67;

/// Distinct patch modules the intake applies to `WorkPackage` (include only),
/// measured on the checkout at the trace's revision.
const WORK_PACKAGE_PATCHES_FLOOR: usize = 4;

/// Patch calls whose `into:` target ruff renders as `<expr>`. Falls when ruff
/// renders keyword arguments; a rise means the intake found more calls.
const TARGET_UNREADABLE_CEILING: usize = 38;

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
    /// The association or validator is spelled by a line in a file that IS in
    /// the harvested chain (a patch module the intake applied), yet the harvest
    /// produced no declaration for it: the file registers it in a body shape
    /// ruff does not walk (`def self.included(base) ... base.class_eval do`, or
    /// a `validates_*_of` helper, which `walk` sends to the `DslCall`
    /// catch-all). Stage: ruff extension (walk those shapes).
    ChainFileNotHarvested,
    /// `rails_generated::derive` returned `Undecided` for the association that
    /// causes this callback: a rule applies but an option or application
    /// setting is not a literal (a `has_and_belongs_to_many` middle model's
    /// name needs the inflector). Stage: ruff extension (evaluate the option)
    /// or `RailsDefaults`.
    GeneratedUndecided,
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
            Slag::ChainFileNotHarvested,
            Slag::GeneratedUndecided,
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

/// A class or module found in a walked root and its harvested ore.
struct Ancestor {
    name: String,
    file: String,
    ore: ClassOre,
}

struct Harvest {
    src: PathBuf,
    /// Zeitwerk roots in lookup order (`oracle/intake.json`, `*` expanded).
    roots: Vec<String>,
    /// File -> every class and module in it (`source_ore_from`: modules carry
    /// their `included do` declarations, `includes` and `superclass`).
    cache: BTreeMap<String, Vec<ClassOre>>,
    /// Include targets the walk could not find a unit for, with the includer.
    unresolved: Vec<(String, String)>,
}

impl Harvest {
    fn new(src: &Path, roots: Vec<String>) -> Self {
        Harvest {
            src: src.to_path_buf(),
            roots,
            cache: BTreeMap::new(),
            unresolved: Vec::new(),
        }
    }

    fn load(&mut self, rel: &str) -> &Vec<ClassOre> {
        if !self.cache.contains_key(rel) {
            let text = std::fs::read_to_string(self.src.join(rel)).unwrap_or_default();
            let mut syms = Symbols::default();
            let units = source_ore_from(rel, &text, "op", &mut syms);
            self.cache.insert(rel.to_string(), units);
        }
        &self.cache[rel]
    }

    /// Find constant `name` in the walked roots.
    fn find(&mut self, name: &str) -> Option<(String, ClassOre)> {
        let path = format!("{}.rb", underscore(name));
        for root in self.roots.clone() {
            let rel = format!("{root}/{path}");
            if self.src.join(&rel).is_file() {
                let hit = self.load(&rel).iter().find(|o| o.name == name).cloned();
                if let Some(ore) = hit {
                    return Some((rel, ore));
                }
            }
        }
        None
    }

    /// Resolve `module` as written inside `from`: innermost enclosing namespace first.
    fn resolve(&mut self, from: &str, module: &str) -> Option<(String, String, ClassOre)> {
        let module = module.trim_start_matches("::");
        let mut ns: Vec<&str> = from.split("::").collect();
        loop {
            let candidate = if ns.is_empty() {
                module.to_string()
            } else {
                format!("{}::{module}", ns.join("::"))
            };
            if let Some((file, ore)) = self.find(&candidate) {
                return Some((candidate, file, ore));
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
        let Some((file, ore)) = self.find(name) else {
            return;
        };
        // A class's own include/prepend are `Concern` declarations; a module
        // lists them in `includes` (and keeps the markers too, so dedupe).
        let mut includes: Vec<String> = Vec::new();
        let mut push = |kind: ConcernKind, module: &str| {
            if matches!(kind, ConcernKind::Include | ConcernKind::Prepend)
                && !module.is_empty()
                && !includes.iter().any(|m| m == module)
            {
                includes.push(module.to_string());
            }
        };
        for e in &ore.declarations {
            if let Declaration::Concern(c) = &e.decl {
                push(c.kind, &c.module);
            }
        }
        for i in &ore.includes {
            push(i.kind, &i.module);
        }
        let superclass = ore.superclass.clone();
        out.push(Ancestor {
            name: name.to_string(),
            file,
            ore,
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
// Intake: engine patches (`oracle/intake.json`)
// ─────────────────────────────────────────────────────────────────────────
//
// OpenProject plugin modules change core models through a DSL, not by where
// their files sit: `patches %i[WorkPackage]` in an engine's class body queues
// `WorkPackage.include(<Plugin>::Patches::WorkPackagePatch)` for `to_prepare`
// (`lib/open_project/plugins/acts_as_op_engine.rb:105-114`). The rules that say
// which call includes which module are data in `oracle/intake.json`; this code
// interprets them and names no OpenProject module.
//
// The calls come from ruff: `Declaration::DslCall` carries every class-body
// call it does not promote, in source order, with its arguments rendered.

#[derive(Debug, Deserialize)]
struct IntakeCfg {
    roots: RootsCfg,
    engines: EnginesCfg,
    engine_order: EngineOrderCfg,
    patch_rules: Vec<PatchRule>,
    phase: PhaseCfg,
}

#[derive(Debug, Deserialize)]
struct RootsCfg {
    list: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EnginesCfg {
    search: Vec<String>,
    file_name: String,
    superclass: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EngineOrderCfg {
    file: String,
    line_marker: String,
    line_marker_end: String,
}

#[derive(Debug, Deserialize)]
struct PhaseCfg {
    applies: String,
}

#[derive(Debug, Deserialize)]
struct PatchRule {
    call: String,
    /// `symbol_array` (`[:A,:B]`), `symbol_path` (`:A, :B, :C`) or `module_into`
    /// (`"Mod", into: ...`).
    args: String,
    /// Constant name templates tried in order: `{plugin_module}`, `{class}`,
    /// `{qualified}`, `{module}`.
    patch: Vec<String>,
    how: String,
    cite: String,
}

fn intake_cfg() -> IntakeCfg {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/intake.json");
    let cfg: IntakeCfg = serde_json::from_str(&std::fs::read_to_string(p).expect("intake.json"))
        .expect("intake.json parses");
    assert_eq!(
        cfg.phase.applies, "after_class_body",
        "the splice below implements only after_class_body"
    );
    cfg
}

fn join_rel(base: &str, seg: &str) -> String {
    if base.is_empty() {
        seg.to_string()
    } else {
        format!("{base}/{seg}")
    }
}

/// Directories matching `pattern` (a `*` segment is one directory).
fn expand_dirs(src: &Path, pattern: &str) -> Vec<String> {
    let mut cur = vec![String::new()];
    for seg in pattern.split('/') {
        let mut next = Vec::new();
        for base in &cur {
            if seg == "*" {
                let mut names: Vec<String> = std::fs::read_dir(src.join(base))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();
                names.sort();
                next.extend(names.iter().map(|n| join_rel(base, n)));
            } else {
                next.push(join_rel(base, seg));
            }
        }
        cur = next;
    }
    cur.into_iter().filter(|d| src.join(d).is_dir()).collect()
}

fn files_named(src: &Path, dir: &str, name: &str, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(src.join(dir))
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let n = e.file_name().to_string_lossy().to_string();
        let rel = join_rel(dir, &n);
        if e.path().is_dir() {
            files_named(src, &rel, name, out);
        } else if n == name {
            out.push(rel);
        }
    }
}

/// Module directories in the order `file` lists them.
fn engine_order(src: &Path, cfg: &EngineOrderCfg) -> Vec<String> {
    let text = std::fs::read_to_string(src.join(&cfg.file)).unwrap_or_default();
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some((_, rest)) = line.split_once(cfg.line_marker.as_str()) {
            if let Some((name, _)) = rest.split_once(cfg.line_marker_end.as_str()) {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// `[:A,:B]` -> `["A", "B"]`.
fn parse_symbol_array(args: &str) -> Option<Vec<String>> {
    let inner = args.trim().strip_prefix('[')?.strip_suffix(']')?;
    inner
        .split(',')
        .map(|t| t.trim().strip_prefix(':').map(str::to_string))
        .collect()
}

/// `:A, :B` -> `["A", "B"]`.
fn parse_symbol_path(args: &str) -> Option<Vec<String>> {
    args.split(',')
        .map(|t| t.trim().strip_prefix(':').map(str::to_string))
        .collect()
}

/// `"Mod", <rest>` -> (`Mod`, targets in `rest`). Ruff renders keyword
/// arguments it does not know as `<expr>`; with no readable target the result
/// is `Err`.
fn parse_module_into(args: &str) -> Result<(String, Vec<String>), String> {
    let rest = args
        .trim()
        .strip_prefix('"')
        .ok_or("first argument is not a string")?;
    let (module, rest) = rest.split_once('"').ok_or("unterminated module string")?;
    if rest.contains("<expr>") {
        return Err("ruff renders the `into:` keyword argument as <expr>".into());
    }
    let targets: Vec<String> = rest
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    if targets.is_empty() {
        return Err("no `into:` target in the rendered arguments".into());
    }
    Ok((module.to_string(), targets))
}

fn fill(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum How {
    Include,
    Prepend,
}

/// A patch the intake resolved to a file in the roots.
#[derive(Debug, Clone)]
struct AppliedPatch {
    module_dir: String,
    engine: String,
    call: String,
    seq: u32,
    target: String,
    how: How,
    patch_name: String,
    patch_file: String,
    cite: String,
}

/// Why a patch call has no (readable) patch module. Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PatchSlag {
    /// The call's arguments do not have the shape the rule expects.
    ArgsUnreadable,
    /// The call names a patch module but ruff renders its target (`into:`) as
    /// `<expr>`. Stage: ruff extension (render keyword arguments in
    /// `walk::render_node`, which today handles `Hash` and not `Kwargs`).
    TargetUnreadable,
    /// No file in the roots defines the module the rule derives.
    PatchFileNotFound,
    /// The module's file exists, but it registers its declarations in
    /// `def self.included(base) ... base.class_eval do ... end`, which
    /// `module_ore` does not walk (only a direct `included do`). Stage: ruff
    /// extension (walk `base.class_eval` bodies inside `self.included`).
    BodyUnreadable,
}

impl PatchSlag {
    fn all() -> &'static [PatchSlag] {
        &[
            PatchSlag::ArgsUnreadable,
            PatchSlag::TargetUnreadable,
            PatchSlag::PatchFileNotFound,
            PatchSlag::BodyUnreadable,
        ]
    }
}

#[derive(Debug, Clone)]
struct PatchUnresolved {
    reason: PatchSlag,
    engine: String,
    call: String,
    detail: String,
}

#[derive(Default)]
struct PatchIntake {
    engines: usize,
    /// Calls matched by a rule.
    calls: usize,
    applied: Vec<AppliedPatch>,
    unresolved: Vec<PatchUnresolved>,
}

impl PatchIntake {
    fn for_model<'a>(&'a self, model: &'a str) -> impl Iterator<Item = &'a AppliedPatch> {
        self.applied.iter().filter(move |p| p.target == model)
    }
}

fn discover_patches(src: &Path, cfg: &IntakeCfg, h: &mut Harvest) -> PatchIntake {
    let order = engine_order(src, &cfg.engine_order);
    let rank = |dir: &str| order.iter().position(|d| d == dir).unwrap_or(order.len());
    let mut files = Vec::new();
    for pat in &cfg.engines.search {
        for dir in expand_dirs(src, pat) {
            files_named(src, &dir, &cfg.engines.file_name, &mut files);
        }
    }
    let mut out = PatchIntake::default();
    let mut found: Vec<(usize, AppliedPatch)> = Vec::new();
    for file in files {
        let module_dir = file.split('/').nth(1).unwrap_or_default().to_string();
        let text = std::fs::read_to_string(src.join(&file)).unwrap_or_default();
        let mut syms = Symbols::default();
        for ore in source_ore_from(&file, &text, "op", &mut syms) {
            if !ore
                .superclass
                .as_deref()
                .is_some_and(|s| cfg.engines.superclass.iter().any(|c| c == s))
            {
                continue;
            }
            out.engines += 1;
            let plugin_module = ore
                .name
                .rsplit_once("::")
                .map(|(m, _)| m.to_string())
                .unwrap_or_default();
            for e in &ore.declarations {
                let Declaration::DslCall(call) = &e.decl else {
                    continue;
                };
                let Some(rule) = cfg.patch_rules.iter().find(|r| r.call == call.name) else {
                    continue;
                };
                out.calls += 1;
                let fail = |reason, detail: String| PatchUnresolved {
                    reason,
                    engine: ore.name.clone(),
                    call: format!("{} {}", call.name, call.args),
                    detail,
                };
                // (target, vars for the patch name templates)
                let mut jobs: Vec<(String, Vec<(&str, String)>)> = Vec::new();
                match rule.args.as_str() {
                    "symbol_array" => match parse_symbol_array(&call.args) {
                        Some(names) => {
                            for n in names {
                                jobs.push((
                                    n.clone(),
                                    vec![("class", n.clone()), ("qualified", n)],
                                ));
                            }
                        }
                        None => {
                            out.unresolved
                                .push(fail(PatchSlag::ArgsUnreadable, "not a symbol array".into()));
                            continue;
                        }
                    },
                    "symbol_path" => match parse_symbol_path(&call.args) {
                        Some(path) if !path.is_empty() => {
                            let class = path[path.len() - 1].clone();
                            let qualified = path.join("::");
                            jobs.push((
                                qualified.clone(),
                                vec![("class", class), ("qualified", qualified)],
                            ));
                        }
                        _ => {
                            out.unresolved
                                .push(fail(PatchSlag::ArgsUnreadable, "not a symbol path".into()));
                            continue;
                        }
                    },
                    "module_into" => match parse_module_into(&call.args) {
                        Ok((module, targets)) => {
                            for t in targets {
                                jobs.push((t, vec![("module", module.clone())]));
                            }
                        }
                        Err(why) => {
                            let reason = if why.starts_with("ruff") || why.starts_with("no `into") {
                                PatchSlag::TargetUnreadable
                            } else {
                                PatchSlag::ArgsUnreadable
                            };
                            out.unresolved.push(fail(reason, why));
                            continue;
                        }
                    },
                    other => panic!("intake.json: unknown args form {other:?}"),
                }
                let how = match rule.how.as_str() {
                    "include" => How::Include,
                    "prepend" => How::Prepend,
                    other => panic!("intake.json: unknown how {other:?}"),
                };
                for (target, vars) in jobs {
                    let mut vars: Vec<(&str, &str)> =
                        vars.iter().map(|(k, v)| (*k, v.as_str())).collect();
                    vars.push(("plugin_module", plugin_module.as_str()));
                    let hit = rule.patch.iter().find_map(|t| {
                        let name = fill(t, &vars);
                        h.find(&name).map(|(file, ore)| (name, file, ore))
                    });
                    let Some((patch_name, patch_file, patch_ore)) = hit else {
                        out.unresolved.push(fail(
                            PatchSlag::PatchFileNotFound,
                            format!("target {target}: none of {:?} in the roots", rule.patch),
                        ));
                        continue;
                    };
                    let reads_included_block = patch_ore
                        .declarations
                        .iter()
                        .any(|d| matches!(d.origin, DeclOrigin::Included { .. }));
                    // The scan only picks the reason; the module is applied either way.
                    let legacy_hook = std::fs::read_to_string(src.join(&patch_file))
                        .is_ok_and(|t| t.contains("def self.included"));
                    if how == How::Include && !reads_included_block && legacy_hook {
                        out.unresolved.push(fail(
                            PatchSlag::BodyUnreadable,
                            format!("target {target}: {patch_name} in {patch_file}"),
                        ));
                    }
                    found.push((
                        rank(&module_dir),
                        AppliedPatch {
                            module_dir: module_dir.clone(),
                            engine: ore.name.clone(),
                            call: call.name.clone(),
                            seq: e.seq,
                            target,
                            how,
                            patch_name,
                            patch_file,
                            cite: rule.cite.clone(),
                        },
                    ));
                }
            }
        }
    }
    // to_prepare order: engine (gem require) order, then statement order.
    found.sort_by(|(ra, a), (rb, b)| (ra, &a.module_dir, a.seq).cmp(&(rb, &b.module_dir, b.seq)));
    out.applied = found.into_iter().map(|(_, p)| p).collect();
    out
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
    /// `Some` when ActiveRecord generated this callback from an association
    /// macro (`ruff_ruby_spo::rails_generated`), `None` when written in source.
    generated: Option<GeneratedFrom>,
}

/// Provenance of a generated callback: the association declaration that caused
/// it and the Rails rule (`file:line` in the vendored gem) that registers it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GeneratedFrom {
    association: String,
    rule: String,
}

/// `config.active_record.belongs_to_required_by_default` and
/// `ActiveRecord.belongs_to_required_validates_foreign_key` as OpenProject
/// (`2725194`) sets them, read from source:
///
/// - `belongs_to_required_by_default = false`: `config/application.rb:79`
///   (explicit, overriding `load_defaults 6.1`, `config/application.rb:63`).
/// - `belongs_to_required_validates_foreign_key = true`: not set by OpenProject
///   (the 7.1 initializer line is commented out,
///   `config/initializers/new_framework_defaults_7_1.rb:206`). `load_defaults 6.1`
///   predates 7.1, so `railties-8.1.4.1/lib/rails/application/configuration.rb:285`
///   (the only place that turns it off, inside the 7.1 branch) does not run, and
///   the framework default stays: `activerecord-8.1.4.1/lib/active_record/railtie.rb:40`
///   and `active_record.rb:352`.
const OP_RAILS_DEFAULTS: RailsDefaults = RailsDefaults {
    belongs_to_required_by_default: Some(false),
    belongs_to_required_validates_foreign_key: Some(true),
};

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

fn declared_of(origin: &str, events: &[RailsEvent]) -> Vec<Declared> {
    let mut out = Vec::new();
    for d in events.iter().map(|e| &e.decl) {
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
                    generated: None,
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
                                generated: None,
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
                        generated: None,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// What `rails_generated` returns for one ancestor, as matchable callbacks, plus
/// the rules it could not decide.
struct GeneratedOf {
    /// Each generated callback with the seq of the declaration that caused it.
    callbacks: Vec<(u32, Declared)>,
    undecided: Vec<UndecidedRow>,
}

#[derive(Debug, Clone)]
struct UndecidedRow {
    origin: String,
    association: String,
    rule: String,
    reason: String,
}

fn generated_of(origin: &str, ore: &ClassOre) -> GeneratedOf {
    let mut out = GeneratedOf {
        callbacks: Vec::new(),
        undecided: Vec::new(),
    };
    for g in derive(&ore.declarations, OP_RAILS_DEFAULTS) {
        match g {
            Generated::Callback(cb) => {
                let chain = match cb.chain {
                    GenChain::Save => "save",
                    GenChain::Create => "create",
                    GenChain::Update => "update",
                    GenChain::Validate => "validate",
                    GenChain::Validation => "validation",
                };
                let kind = match cb.kind {
                    GenKind::Before => "before",
                    GenKind::After => "after",
                    GenKind::Around => "around",
                };
                let filter = match cb.filter {
                    GenFilter::Method(m) => Filter::Method(m),
                    GenFilter::PresenceValidator { attribute } => Filter::Validator {
                        kind: "presence".into(),
                        attr: attribute,
                    },
                };
                out.callbacks.push((
                    cb.origin.declaration_seq,
                    Declared {
                        chain: chain.into(),
                        kind: kind.into(),
                        filter,
                        origin: origin.to_string(),
                        options: cb.options,
                        implied_on: None,
                        generated: Some(GeneratedFrom {
                            association: cb.association,
                            rule: cb.origin.rule,
                        }),
                    },
                ));
            }
            Generated::Undecided(u) => out.undecided.push(UndecidedRow {
                origin: origin.to_string(),
                association: u.association,
                rule: u.rule,
                reason: u.reason,
            }),
        }
    }
    out
}

fn association_names(ancestors: &[Ancestor]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for a in ancestors {
        for d in a.ore.declarations.iter().map(|e| &e.decl) {
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
// Effective registration order
// ─────────────────────────────────────────────────────────────────────────
//
// Four orders are kept apart here:
//   (a) source order of declarations inside one class or module body;
//   (b) concern inclusion order;
//   (c) superclass expansion;
//   (d) the order Rails registers callbacks in, which is what a chain runs in.
// `Harvest::effective` rebuilds (d) from (a)-(c):
//   - a superclass is fully expanded before the class's own body
//     (a class body runs after `class A < B` has loaded B);
//   - `include M` runs M's `included` block at the include statement, in
//     statement order (`ActiveSupport::Concern#append_features`,
//     activesupport-8.1.4.1/lib/active_support/concern.rb:129-140);
//   - first the concern's own dependencies, i.e. the modules it includes in its
//     body (`concern.rb:131` records them, `:135` includes them into the real
//     base, before `:138` runs the included block);
//   - a module already in the ancestors is not re-run (`concern.rb:134`
//     `return false if base < self`), including one a superclass or an earlier
//     concern already brought in;
//   - `prepend` runs the prepended block only (`concern.rb:142-152`), never the
//     included block, so it does not touch callback registration order;
//   - a generated association callback is registered at its association
//     declaration's position (`autosave_association.rb`, called from the macro);
//   - re-registering a symbol filter on the same chain and kind replaces the
//     earlier one and moves it to the end (ActiveSupport::Callbacks,
//     activesupport-8.1.4.1/lib/active_support/callbacks.rb:645-663,
//     `append_one` -> `remove_duplicates` -> `@chain.push`).
// A chain then runs in registration order for before, around and (because
// ActiveModel registers `after_*` with `prepend: true`,
// activemodel-8.1.4.1/lib/active_model/callbacks.rb:146, and the sequence
// compiles the chain in reverse, activesupport callbacks.rb:619-621 with
// `after` pushing at :535-539) after callbacks too.

struct EffState {
    included: BTreeSet<String>,
    out: Vec<Declared>,
}

impl EffState {
    fn push(&mut self, d: Declared) {
        if let Filter::Method(name) = &d.filter {
            self.out.retain(|e| {
                !(e.chain == d.chain
                    && e.kind == d.kind
                    && matches!(&e.filter, Filter::Method(n) if n == name))
            });
        }
        self.out.push(d);
    }
}

impl Harvest {
    /// The model's callbacks in Rails registration order. `patches` are the
    /// engine includes (`oracle/intake.json`), already in `to_prepare` order;
    /// they run after the class body, so their declarations come last. A
    /// `prepend` runs only the prepended block and registers nothing.
    fn effective(&mut self, model: &str, patches: &PatchIntake) -> Vec<Declared> {
        let mut st = EffState {
            included: BTreeSet::new(),
            out: Vec::new(),
        };
        let mut guard = BTreeSet::new();
        self.eff_class(model, &mut st, &mut guard);
        for p in patches.for_model(model).filter(|p| p.how == How::Include) {
            if let Some((_, ore)) = self.find(&p.patch_name) {
                self.eff_resolved(&p.patch_name, &ore, &mut st);
            }
        }
        st.out
    }

    fn eff_class(&mut self, name: &str, st: &mut EffState, guard: &mut BTreeSet<String>) {
        if !guard.insert(name.to_string()) {
            return;
        }
        let Some((_, ore)) = self.find(name) else {
            return;
        };
        if let Some(sup) = ore.superclass.as_deref() {
            if let Some((resolved, ..)) = self.resolve(name, sup) {
                self.eff_class(&resolved, st, guard);
            }
        }
        self.eff_body(name, &ore, false, st);
    }

    /// Walk one body in source order. For a module (`included_only`), only
    /// the `included do` events run on the includer.
    fn eff_body(&mut self, name: &str, ore: &ClassOre, included_only: bool, st: &mut EffState) {
        let generated = generated_of(name, ore);
        for e in &ore.declarations {
            let runs = !included_only || matches!(e.origin, DeclOrigin::Included { .. });
            if !runs {
                continue;
            }
            if let Declaration::Concern(c) = &e.decl {
                if matches!(c.kind, ConcernKind::Include) && !c.module.is_empty() {
                    self.eff_include(name, &c.module, st);
                }
                continue;
            }
            for d in declared_of(name, std::slice::from_ref(e)) {
                st.push(d);
            }
            for (seq, g) in &generated.callbacks {
                if *seq == e.seq {
                    st.push(g.clone());
                }
            }
        }
    }

    fn eff_include(&mut self, from: &str, module: &str, st: &mut EffState) {
        let Some((resolved, _, ore)) = self.resolve(from, module) else {
            return;
        };
        self.eff_resolved(&resolved, &ore, st);
    }

    /// `include` of an already resolved module: not again if it is an
    /// ancestor, its dependencies first, then its `included` block.
    fn eff_resolved(&mut self, resolved: &str, ore: &ClassOre, st: &mut EffState) {
        let resolved = resolved.to_string();
        if !st.included.insert(resolved.clone()) {
            return;
        }
        // Dependencies: modules included in the module body, before its block.
        let mut deps: Vec<String> = Vec::new();
        for e in &ore.declarations {
            if let (Declaration::Concern(c), DeclOrigin::Body) = (&e.decl, &e.origin) {
                if matches!(c.kind, ConcernKind::Include) && !c.module.is_empty() {
                    deps.push(c.module.clone());
                }
            }
        }
        for i in &ore.includes {
            if matches!(i.kind, ConcernKind::Include)
                && matches!(i.origin, DeclOrigin::Body)
                && !deps.contains(&i.module)
            {
                deps.push(i.module.clone());
            }
        }
        for d in deps {
            self.eff_include(&resolved, &d, st);
        }
        self.eff_body(&resolved, ore, true, st);
    }
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
    /// Declared in source, ancestors in the walk's visit order.
    declared: Vec<Declared>,
    /// Generated by ActiveRecord from the association declarations.
    generated: Vec<Declared>,
    /// Rules `rails_generated` could not decide for this model.
    undecided: Vec<UndecidedRow>,
    /// Associations declared with `has_and_belongs_to_many`.
    habtm: BTreeSet<String>,
    /// Source plus generated callbacks in Rails registration order.
    effective: Vec<Declared>,
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
                    if let Some(u) = ctx.undecided.iter().find(|u| u.association == x) {
                        return (
                            Slag::GeneratedUndecided,
                            format!("association :{x}: {} ({})", u.reason, u.rule),
                        );
                    }
                    return (
                        Slag::AssociationAutosave,
                        format!("association :{x} declared"),
                    );
                }
                // The middle has_many of a habtm is named
                // `[<class>.downcase.pluralize, <assoc>].sort.join("_")`
                // (builder/has_and_belongs_to_many.rb:228-230). This guess only
                // picks the slag reason; it never produces a match.
                let plural = format!("{}s", model.to_lowercase());
                if let Some(h) = ctx.habtm.iter().find(|h| {
                    let mut pair = [plural.as_str(), h.as_str()];
                    pair.sort_unstable();
                    pair.join("_") == x
                }) {
                    return (
                        Slag::GeneratedUndecided,
                        format!("middle has_many of habtm :{h} (name needs the inflector)"),
                    );
                }
                if let Some(f) = ev
                    .assocs
                    .get(x)
                    .and_then(|fs| fs.iter().find(|f| ctx.ancestor_files.contains(*f)))
                {
                    return (
                        Slag::ChainFileNotHarvested,
                        format!("association :{x} is spelled in {f}, a harvested file; no declaration came out"),
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
        FiredFilter::Validator { kind, attr } => {
            if kind == "presence" {
                if let Some(u) = ctx.undecided.iter().find(|u| &u.association == attr) {
                    return (
                        Slag::GeneratedUndecided,
                        format!("association :{attr}: {} ({})", u.reason, u.rule),
                    );
                }
            }
            if let Some(f) = ev
                .validates
                .get(attr)
                .and_then(|fs| fs.iter().find(|f| ctx.ancestor_files.contains(*f)))
            {
                return (
                    Slag::ChainFileNotHarvested,
                    format!("validation of :{attr} is spelled in {f}, a harvested file; no declaration came out"),
                );
            }
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
    /// Of `matched`: entries that needed a generated callback for at least one
    /// of their expansions (no source declaration matches it).
    matched_generated: usize,
    /// Provenance of those: entry, association, Rails rule.
    generated_matches: Vec<String>,
    total: usize,
    slag: BTreeMap<Slag, Vec<String>>,
    /// Silent reasons of source-declared callbacks.
    silent: BTreeMap<Silent, Vec<String>>,
    /// Silent reasons of generated callbacks.
    generated_silent: BTreeMap<Silent, Vec<String>>,
    declared_total: usize,
    generated_total: usize,
    undecided_total: usize,
}

fn run(
    ctxs: &BTreeMap<String, ModelCtx>,
    entries: &BTreeMap<EntryKey, Vec<Fired>>,
    ev: &Evidence,
    transform: impl Fn(&Declared) -> Declared,
) -> Outcome {
    // Source declarations first, generated ones after; `n_source` splits them.
    let declared: BTreeMap<&String, (usize, Vec<Declared>)> = ctxs
        .iter()
        .map(|(m, c)| {
            let all: Vec<Declared> = c
                .declared
                .iter()
                .chain(c.generated.iter())
                .map(&transform)
                .collect();
            (m, (c.declared.len(), all))
        })
        .collect();
    let mut used: BTreeMap<&String, BTreeSet<usize>> = BTreeMap::new();
    let mut fired_chains: BTreeMap<&String, BTreeSet<String>> = BTreeMap::new();
    let mut out = Outcome {
        matched: 0,
        matched_generated: 0,
        generated_matches: Vec::new(),
        total: entries.len(),
        slag: BTreeMap::new(),
        silent: BTreeMap::new(),
        generated_silent: BTreeMap::new(),
        declared_total: declared.values().map(|(n, _)| *n).sum(),
        generated_total: declared.values().map(|(n, v)| v.len() - n).sum(),
        undecided_total: ctxs.values().map(|c| c.undecided.len()).sum(),
    };
    for ((model, phase, label), expanded) in entries {
        let ctx = &ctxs[model];
        let (n_source, decls) = &declared[model];
        fired_chains
            .entry(model)
            .or_default()
            .extend(expanded.iter().map(|f| f.chain.clone()));
        let mut all = true;
        let mut via_generated: Vec<String> = Vec::new();
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
            if hits.iter().all(|i| i >= n_source) {
                if let Some(g) = &decls[hits[0]].generated {
                    via_generated.push(format!(
                        "{model} {phase} {label}  <- :{} ({})",
                        g.association, g.rule
                    ));
                }
            }
            used.entry(model).or_default().extend(hits);
        }
        if all {
            out.matched += 1;
            if let Some(first) = via_generated.into_iter().next() {
                out.matched_generated += 1;
                out.generated_matches.push(first);
            }
        }
    }
    for (model, (n_source, decls)) in &declared {
        let empty = BTreeSet::new();
        let u = used.get(model).unwrap_or(&empty);
        let chains = fired_chains.get(model).cloned().unwrap_or_default();
        for (i, d) in decls.iter().enumerate() {
            if !u.contains(&i) {
                let row = format!(
                    "{model} {}_{} {:?} (from {}{}) options={:?}",
                    d.kind,
                    d.chain,
                    d.filter,
                    d.origin,
                    d.generated.as_ref().map_or(String::new(), |g| format!(
                        ", generated for :{} by {}",
                        g.association, g.rule
                    )),
                    d.options
                );
                let map = if i < *n_source {
                    &mut out.silent
                } else {
                    &mut out.generated_silent
                };
                map.entry(silent_reason(d, &chains)).or_default().push(row);
            }
        }
    }
    out
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "fired (distinct entries): {}", self.total)?;
        writeln!(f, "  matched to a declaration: {}", self.matched)?;
        writeln!(
            f,
            "    of which only a generated callback matches: {}",
            self.matched_generated
        )?;
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
        writeln!(
            f,
            "generated callbacks: {} (undecided rules: {})",
            self.generated_total, self.undecided_total
        )?;
        for s in Silent::all() {
            writeln!(
                f,
                "  generated, never fired, {:?}: {}",
                s,
                self.generated_silent.get(s).map_or(0, Vec::len)
            )?;
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The test
// ─────────────────────────────────────────────────────────────────────────

fn harvest_for(src: &Path, cfg: &IntakeCfg) -> Harvest {
    let roots = cfg
        .roots
        .list
        .iter()
        .flat_map(|pat| expand_dirs(src, pat))
        .collect();
    Harvest::new(src, roots)
}

fn contexts(
    h: &mut Harvest,
    models: &BTreeSet<String>,
    patches: &PatchIntake,
) -> (BTreeMap<String, ModelCtx>, Vec<(String, String)>) {
    h.unresolved.clear();
    let mut out = BTreeMap::new();
    for m in models {
        let mut ancestors = h.linearize(m);
        let has_file = !ancestors.is_empty();
        let mut seen: BTreeSet<String> = ancestors.iter().map(|a| a.name.clone()).collect();
        for p in patches.for_model(m).filter(|p| p.how == How::Include) {
            for a in h.linearize(&p.patch_name) {
                if seen.insert(a.name.clone()) {
                    ancestors.push(a);
                }
            }
        }
        let mut declared = Vec::new();
        let mut generated = Vec::new();
        let mut undecided = Vec::new();
        let mut habtm = BTreeSet::new();
        for a in &ancestors {
            declared.extend(declared_of(&a.name, &a.ore.declarations));
            let g = generated_of(&a.name, &a.ore);
            generated.extend(g.callbacks.into_iter().map(|(_, d)| d));
            undecided.extend(g.undecided);
            for e in &a.ore.declarations {
                if let Declaration::Association(assoc) = &e.decl {
                    if assoc.kind == AssocKind::HasAndBelongsToMany {
                        habtm.insert(assoc.name.clone());
                    }
                }
            }
        }
        out.insert(
            m.clone(),
            ModelCtx {
                has_file,
                assocs: association_names(&ancestors),
                ancestor_files: ancestors.iter().map(|a| a.file.clone()).collect(),
                declared,
                generated,
                undecided,
                habtm,
                effective: h.effective(m, patches),
            },
        );
    }
    (out, h.unresolved.clone())
}

// ─────────────────────────────────────────────────────────────────────────
// Order comparison (reported, not asserted)
// ─────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct OrderReport {
    /// (case, model, phase) groups with at least two matched entries.
    groups: usize,
    /// Groups whose recorded order differs from the reconstructed one.
    differing: usize,
    /// Adjacent pairs recorded in the opposite order to the reconstruction.
    inversions: usize,
    examples: Vec<String>,
}

/// Compare the order the trace recorded, per case and `(model, phase)`, with
/// the rank of each matched entry in `chain_of(model)`. Membership is not in
/// question here: an entry with an expansion that has no declaration is left
/// out of the comparison.
fn order_report<'a>(
    chain_of: impl Fn(&str) -> &'a [Declared],
    show_examples: usize,
) -> OrderReport {
    let t = trace();
    let mut rep = OrderReport::default();
    for case in t["cases"].as_array().expect("cases") {
        let case_name = case["name"].as_str().unwrap_or("?");
        let mut groups: BTreeMap<(String, String), Vec<(String, usize)>> = BTreeMap::new();
        let mut seen: BTreeSet<EntryKey> = BTreeSet::new();
        for e in case["journal"].as_array().expect("journal") {
            let model = e["model"].as_str().expect("model");
            let phase = e["phase"].as_str().expect("phase");
            let label = e["callback"].as_str().expect("callback");
            if !seen.insert((model.into(), phase.into(), label.into())) {
                continue;
            }
            let Some(expanded) = parse_fired(model, phase, label) else {
                continue;
            };
            let chain = chain_of(model);
            let ranks: Option<Vec<usize>> = expanded
                .iter()
                .map(|f| chain.iter().position(|d| declared_matches(d, f)))
                .collect();
            if let Some(r) = ranks.and_then(|r| r.into_iter().min()) {
                groups
                    .entry((model.into(), phase.into()))
                    .or_default()
                    .push((label.into(), r));
            }
        }
        for ((model, phase), seq) in groups {
            if seq.len() < 2 {
                continue;
            }
            rep.groups += 1;
            let inv = seq.windows(2).filter(|w| w[0].1 > w[1].1).count();
            if inv > 0 {
                rep.differing += 1;
                rep.inversions += inv;
                if rep.examples.len() < show_examples {
                    let mut expected = seq.clone();
                    expected.sort_by_key(|(_, r)| *r);
                    let rec: Vec<&str> = seq.iter().map(|(l, _)| l.as_str()).collect();
                    let exp: Vec<&str> = expected.iter().map(|(l, _)| l.as_str()).collect();
                    let at = rec.iter().zip(&exp).position(|(a, b)| a != b).unwrap_or(0);
                    let window = |v: &[&str]| v[at..(at + 3).min(v.len())].join(" | ");
                    rep.examples.push(format!(
                        "{case_name} {model} {phase} ({} entries), first difference at #{at}: recorded [{}] vs reconstructed [{}]",
                        rec.len(),
                        window(&rec),
                        window(&exp)
                    ));
                }
            }
        }
    }
    rep
}

fn print_order(label: &str, r: &OrderReport) {
    eprintln!(
        "order [{label}]: {} groups compared, {} differ, {} adjacent inversions",
        r.groups, r.differing, r.inversions
    );
    for e in &r.examples {
        eprintln!("     {e}");
    }
}

#[test]
fn fired_callbacks_are_matched_or_named_slag() {
    let Some(src) = source() else { return };
    let (entries_total, entries) = fired_entries();
    let models: BTreeSet<String> = entries.keys().map(|(m, _, _)| m.clone()).collect();
    let cfg = intake_cfg();
    let mut h = harvest_for(&src, &cfg);
    let patches = discover_patches(&src, &cfg, &mut h);
    let (ctxs, unresolved) = contexts(&mut h, &models, &patches);
    let ev = scan_tree(&src, &models);

    eprintln!(
        "intake: {} engines, {} patch calls matched by a rule, {} resolved, {} unresolved",
        patches.engines,
        patches.calls,
        patches.applied.len(),
        patches.unresolved.len()
    );
    for model in &models {
        for p in patches.for_model(model) {
            eprintln!(
                "  {model} <- {:?} {} [{}: {} seq {}] {}  ({})",
                p.how, p.patch_name, p.module_dir, p.call, p.seq, p.patch_file, p.cite
            );
        }
    }
    for r in PatchSlag::all() {
        let rows: Vec<&PatchUnresolved> = patches
            .unresolved
            .iter()
            .filter(|u| u.reason == *r)
            .collect();
        eprintln!("  patch slag {r:?}: {}", rows.len());
        for u in rows.iter().take(40) {
            eprintln!("     {} {}  [{}]", u.engine, u.call, u.detail);
        }
    }

    eprintln!(
        "journal entries: {entries_total}; distinct (model, phase, label): {}",
        entries.len()
    );
    for (m, c) in &ctxs {
        eprintln!(
            "  {m}: {} ancestors files, {} declared callbacks, {} generated callbacks, {} undecided rules, {} associations",
            c.ancestor_files.len(),
            c.declared.len(),
            c.generated.len(),
            c.undecided.len(),
            c.assocs.len()
        );
        for u in &c.undecided {
            eprintln!(
                "     undecided :{} (from {}) {} [{}]",
                u.association, u.origin, u.reason, u.rule
            );
        }
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
    eprintln!(
        "-- matched only through a generated callback ({})",
        o.matched_generated
    );
    for r in o.generated_matches.iter().take(8) {
        eprintln!("     {r}");
    }
    for (s, rows) in &o.generated_silent {
        eprintln!("-- generated, never fired, {s:?} ({})", rows.len());
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
    let gen_silent_total: usize = o.generated_silent.values().map(Vec::len).sum();
    assert!(gen_silent_total <= o.generated_total);

    // Measured at OpenProject 2725194 (trace revision). A drop means the harvest
    // lost declarations; a rise means it gained some: re-measure and move the floor.
    assert!(
        o.matched >= MATCHED_FLOOR,
        "matched {} of {}, floor {MATCHED_FLOOR}",
        o.matched,
        o.total
    );
    assert!(
        o.matched_generated >= MATCHED_GENERATED_FLOOR,
        "matched through generated callbacks: {}, floor {MATCHED_GENERATED_FLOOR}",
        o.matched_generated
    );
    // A declared, unconditioned callback on an exercised chain that never fired
    // would be a real discrepancy, not slag.
    assert!(
        o.generated_silent
            .get(&Silent::Unexplained)
            .is_none_or(Vec::is_empty),
        "generated callbacks that never fired and have no explanation: {:?}",
        o.generated_silent.get(&Silent::Unexplained)
    );
    assert!(
        o.silent.get(&Silent::Unexplained).is_none_or(Vec::is_empty),
        "declared callbacks that never fired and have no explanation: {:?}",
        o.silent.get(&Silent::Unexplained)
    );

    // Negative control: every declared filter, source and generated, spelled
    // backwards: nothing may match.
    let control = run(&ctxs, &entries, &ev, reversed);
    eprintln!(
        "negative control (all reversed): matched {} of {}",
        control.matched, control.total
    );
    assert!(
        control.matched * 50 <= o.matched,
        "the matcher does not discriminate: {} matched with reversed names",
        control.matched
    );
    // Generated callbacks only: the matches that came from source must stay,
    // the ones that needed a generated callback must go.
    let control_gen = run(&ctxs, &entries, &ev, |d| {
        if d.generated.is_some() {
            reversed(d)
        } else {
            d.clone()
        }
    });
    eprintln!(
        "negative control (generated reversed): matched {} of {}, via generated {}",
        control_gen.matched, control_gen.total, control_gen.matched_generated
    );
    assert_eq!(
        control_gen.matched_generated, 0,
        "a generated callback matched with its name reversed"
    );
    assert_eq!(
        control_gen.matched + o.matched_generated,
        o.matched,
        "reversing generated names must drop exactly the generated matches"
    );

    // Attribution: the same run without the engine patches is what ruff alone
    // gives. Patches may only add matches.
    let (bare_ctxs, _) = contexts(&mut h, &models, &PatchIntake::default());
    let bare = run(&bare_ctxs, &entries, &ev, Clone::clone);
    let bare_order = order_report(|m| bare_ctxs[m].effective.as_slice(), 0);
    eprintln!(
        "without the intake: matched {} of {}, via generated {}; order {} groups, {} differ",
        bare.matched, bare.total, bare.matched_generated, bare_order.groups, bare_order.differing
    );
    for s in Slag::all() {
        eprintln!(
            "  without the intake, slag {s:?}: {}",
            bare.slag.get(s).map_or(0, Vec::len)
        );
    }
    assert!(
        o.matched >= bare.matched,
        "the intake lost matches: {} with, {} without",
        o.matched,
        bare.matched
    );
    // Every call a rule matched is applied or typed; none is dropped.
    let typed_before_apply = patches
        .unresolved
        .iter()
        .filter(|u| u.reason != PatchSlag::BodyUnreadable)
        .count();
    assert_eq!(
        patches.calls,
        patches
            .applied
            .iter()
            .map(|p| (p.engine.as_str(), p.call.as_str(), p.seq))
            .collect::<BTreeSet<_>>()
            .len()
            + typed_before_apply,
        "a patch call was neither applied nor given a PatchSlag"
    );
    let count = |r: PatchSlag| patches.unresolved.iter().filter(|u| u.reason == r).count();
    assert_eq!(
        count(PatchSlag::ArgsUnreadable),
        0,
        "a rule did not parse a call"
    );
    assert_eq!(
        count(PatchSlag::PatchFileNotFound),
        0,
        "a patch module the rule derives has no file in the roots"
    );
    assert!(
        count(PatchSlag::TargetUnreadable) <= TARGET_UNREADABLE_CEILING,
        "more unreadable patch targets than measured"
    );
    let wp_patches = patches
        .for_model("WorkPackage")
        .filter(|p| p.how == How::Include)
        .count();
    assert!(
        wp_patches >= WORK_PACKAGE_PATCHES_FLOOR,
        "WorkPackage patches: {wp_patches}, floor {WORK_PACKAGE_PATCHES_FLOOR}"
    );

    // Order: reported, not asserted (yet). Only that the comparison ran.
    let effective = order_report(|m| ctxs[m].effective.as_slice(), 6);
    print_order("reconstructed Rails registration order", &effective);
    let naive: BTreeMap<String, Vec<Declared>> = ctxs
        .iter()
        .map(|(m, c)| {
            (
                m.clone(),
                c.declared
                    .iter()
                    .chain(c.generated.iter())
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    let naive_rep = order_report(|m| naive[m].as_slice(), 3);
    print_order(
        "file order of the walk, generated last (not runtime order)",
        &naive_rep,
    );
    assert!(
        effective.groups > 0,
        "the order comparison compared nothing"
    );
}

#[test]
fn intake_argument_forms() {
    assert_eq!(
        parse_symbol_array("[:PermittedParams,:WorkPackage]"),
        Some(vec!["PermittedParams".into(), "WorkPackage".into()])
    );
    assert_eq!(parse_symbol_array(":A"), None);
    assert_eq!(
        parse_symbol_path(":API, :V3, :Schema"),
        Some(vec!["API".into(), "V3".into(), "Schema".into()])
    );
    assert_eq!(parse_symbol_path(":A, <expr>"), None);
    // What ruff renders today.
    assert!(parse_module_into("\"WorkPackages::Costs\", <expr>").is_err());
    // What a rendered keyword hash would look like.
    assert_eq!(
        parse_module_into("\"WorkPackages::Costs\", {:into: \"WorkPackage\"}"),
        Ok(("WorkPackages::Costs".into(), vec!["WorkPackage".into()]))
    );
    assert_eq!(
        parse_module_into("\"M\", {:into: [\"A\",\"B\"]}").map(|(_, t)| t),
        Ok(vec!["A".into(), "B".into()])
    );
    assert!(parse_module_into("\"M\"").is_err());
    assert_eq!(
        fill(
            "{plugin_module}::Patches::{class}Patch",
            &[("plugin_module", "X::Y"), ("class", "Z")]
        ),
        "X::Y::Patches::ZPatch"
    );
}
