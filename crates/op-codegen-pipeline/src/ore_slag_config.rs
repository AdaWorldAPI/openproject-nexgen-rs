//! Declarative ORE/SLAG reconciliation config: the data of a reconciliation (claims,
//! residual proposals, gates) in a TOML file, with generic loading, schema checking
//! and rendering here. Application-specific code supplies only a small binding:
//! named evidence evaluators ([`EvidenceSource`]), generators for generated claim
//! families, and the location/revision lookups of [`RenderCtx`].
//!
//! ```text
//!   <config>.toml ──load──► OreSlagConfig ──claim_specs(evaluators)──► Vec<ClaimSpec>
//!                                  │
//!                                  └─render_residual(row context)───► Residual
//! ```
//!
//! # Schema
//!
//! Every table rejects unknown keys. Claim keys and residual keys are unique.
//!
//! - `[scope]`: `application`, `model`, `operation` (the `JoinKey` prefix) and the
//!   names of the two roots, `source_root` and `target_root`.
//! - `[extractors]`: label name → extractor text. A claim names its extractor, or
//!   several joined with ` + `; every part must be declared.
//! - `[[claim]]`: `phase`, `method`, `effect`, `level`, `ruby`, `rust`,
//!   `comparability`, `repair`. `ruby` is one of
//!   `{kind="fact", produced, extractor, file, needle, fact}`,
//!   `{kind="not_harvested", why}`,
//!   `{kind="absent_in_body", body, must_not_contain, looked_for, extractor, file}`,
//!   `{kind="witness", name}`. `produced` is a predicate name, optionally
//!   `name:argument`, resolved by the application's evaluator.
//! - `[[claim_family]]`: a claim generated from the harvest by a named generator.
//! - `[[residual]]`: the proposal of one slag row, by key. `[residual_default.<Reason>]`:
//!   the proposal for any slag row of that reason without its own entry. String
//!   fields may contain `{charon_residual}` (see [`RenderCtx`]); `{{` and `}}` are
//!   literal braces.
//! - `[gates]`: the gate names and the companion evidence kinds.
//! - `[[learned]]`: the status and reason each residual last computed. Written by the
//!   application's update mode after [`LEARNED_MARKER`]; never edited by hand.
//!
//! # Autolearn
//!
//! [`render_learned`] and [`rewrite_learned`] keep the `[[learned]]` block current. A
//! slag row with neither a `[[residual]]` entry nor a default for its reason is a
//! [`ConfigError::MissingResidual`]; [`append_stub`] adds a stub whose proposal
//! fields are all `"TODO"`, and [`OreSlagConfig::todo_keys`] names the stubs still
//! unfilled.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde::Deserialize;

use crate::reconcile::{
    locate, ClaimSpec, Comparability, JoinKey, Level, RubyEvidence, RustProbe, SlagReason,
    SourceSpan,
};
use crate::resolution::{EvidenceKind, OwnerStage, Proposal, Residual};

/// The line that starts the machine-written `[[learned]]` block (to the end of file).
pub const LEARNED_MARKER: &str = "# ── learned (written by UPDATE_ORE_SLAG=1; do not edit) ──";

/// The literal an unfilled stub carries in every proposal field.
pub const TODO: &str = "TODO";

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: String,
        error: String,
    },
    Parse(String),
    DuplicateClaim(String),
    DuplicateResidual(String),
    BadResidualEntry(String),
    /// A slag row with neither its own entry nor a default for its reason.
    MissingResidual {
        key: String,
        reason: SlagReason,
    },
    /// A stub still carries `"TODO"` fields.
    Todo {
        key: String,
    },
    UnknownName {
        what: &'static str,
        name: String,
    },
    Render {
        key: String,
        error: String,
    },
    Evaluator(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io { path, error } => write!(f, "{path}: {error}"),
            ConfigError::Parse(e) => write!(f, "config does not parse: {e}"),
            ConfigError::DuplicateClaim(k) => write!(f, "duplicate claim key: {k}"),
            ConfigError::DuplicateResidual(k) => write!(f, "duplicate residual key: {k}"),
            ConfigError::BadResidualEntry(e) => write!(f, "{e}"),
            ConfigError::MissingResidual { key, reason } => write!(
                f,
                "missing residual: no [[residual]] entry and no [residual_default.{reason:?}] for {key}"
            ),
            ConfigError::Todo { key } => write!(f, "residual stub still carries TODO fields: {key}"),
            ConfigError::UnknownName { what, name } => write!(f, "unknown {what}: {name}"),
            ConfigError::Render { key, error } => write!(f, "{key}: {error}"),
            ConfigError::Evaluator(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

// ─────────────────────────────────────────────────────────────────────────
// Schema
// ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OreSlagConfig {
    pub scope: Scope,
    #[serde(default)]
    pub extractors: BTreeMap<String, String>,
    #[serde(default)]
    pub claim: Vec<ClaimCfg>,
    #[serde(default)]
    pub claim_family: Vec<FamilyCfg>,
    #[serde(default)]
    pub residual: Vec<ResidualCfg>,
    #[serde(default)]
    pub residual_default: BTreeMap<String, ResidualCfg>,
    pub gates: Gates,
    #[serde(default)]
    pub learned: Vec<Learned>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub application: String,
    pub model: String,
    pub operation: String,
    /// Name of the source application's root, as `locate` and `revision` callbacks
    /// know it.
    pub source_root: String,
    /// Name of the target's root.
    pub target_root: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LevelCfg {
    Registration,
    Admission,
    GuardEvaluation,
    CallbackInvocation,
    Persistence,
}

impl From<LevelCfg> for Level {
    fn from(l: LevelCfg) -> Level {
        match l {
            LevelCfg::Registration => Level::Registration,
            LevelCfg::Admission => Level::Admission,
            LevelCfg::GuardEvaluation => Level::GuardEvaluation,
            LevelCfg::CallbackInvocation => Level::CallbackInvocation,
            LevelCfg::Persistence => Level::Persistence,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataCfg {
    pub krate: String,
    pub file: String,
    pub item: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RustCfg {
    Edges(Vec<[String; 2]>),
    Data(DataCfg),
}

impl RustCfg {
    fn probe(&self) -> RustProbe {
        match self {
            RustCfg::Edges(e) => RustProbe::Edges {
                edges: e.iter().map(|[a, b]| (a.clone(), b.clone())).collect(),
                conflicts: Vec::new(),
            },
            RustCfg::Data(d) => RustProbe::Data {
                krate: d.krate.clone(),
                file: d.file.clone(),
                item: d.item.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DifferentCfg {
    pub ruby: String,
    pub rust: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComparabilityCfg {
    Same,
    Different(DifferentCfg),
}

impl ComparabilityCfg {
    fn comparability(&self) -> Comparability {
        match self {
            ComparabilityCfg::Same => Comparability::Same,
            ComparabilityCfg::Different(d) => Comparability::Different {
                ruby: d.ruby.clone(),
                rust: d.rust.clone(),
            },
        }
    }
}

/// The declarative Ruby (source-side) evidence of a claim.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RubyCfg {
    /// A fact the extractor named by `produced` did or did not produce, located at the
    /// first `needle` in `file`.
    Fact {
        produced: String,
        extractor: String,
        file: String,
        needle: String,
        fact: String,
    },
    NotHarvested {
        why: String,
    },
    /// The witness inspected the named body and `must_not_contain` is not there.
    AbsentInBody {
        body: String,
        must_not_contain: String,
        looked_for: String,
        extractor: String,
        file: String,
    },
    /// A custom evaluator the application binds by name.
    Witness {
        name: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimCfg {
    pub phase: String,
    pub method: String,
    pub effect: String,
    pub level: LevelCfg,
    pub ruby: RubyCfg,
    pub rust: RustCfg,
    pub comparability: ComparabilityCfg,
    pub repair: String,
}

/// A family of claims produced from the harvest by a named generator. The generator
/// yields the `method`, `effect`, span and fact of each instance; everything else is
/// declared here.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyCfg {
    pub generator: String,
    pub phase: String,
    pub level: LevelCfg,
    pub extractor: String,
    pub rust: RustCfg,
    pub comparability: ComparabilityCfg,
    pub repair: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocateCfg {
    pub file: String,
    pub needle: String,
    pub root: String,
}

/// Where a residual sits in source: a located literal, or a name of the row's own
/// locations (`ruby_loc`, `rust_loc`, `ruby_loc;rust_loc`).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LocationCfg {
    Locate { locate: LocateCfg },
    Name(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidualCfg {
    /// Set on a `[[residual]]` entry, absent on a `[residual_default.*]` template.
    pub key: Option<String>,
    pub missing_fact: String,
    pub supplier: String,
    pub supplier_exists: bool,
    pub claim_change: String,
    pub validating_evidence: String,
    pub negative_control: String,
    pub sub_reason: String,
    pub owner_stage: String,
    pub source_location: LocationCfg,
    /// A root name, or `both` (the source then the target revision).
    pub source_revision: String,
    /// The symbol whose Charon residual `{charon_residual}` stands for.
    #[serde(default)]
    pub charon_residual_symbol: Option<String>,
}

impl ResidualCfg {
    fn todo_fields(&self) -> bool {
        let loc_todo = matches!(&self.source_location, LocationCfg::Name(n) if n == TODO);
        loc_todo
            || [
                &self.missing_fact,
                &self.supplier,
                &self.claim_change,
                &self.validating_evidence,
                &self.negative_control,
                &self.sub_reason,
                &self.source_revision,
            ]
            .iter()
            .any(|s| s.as_str() == TODO)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gates {
    pub witness: String,
    pub drift: String,
    /// Names of [`EvidenceKind`]s that must accompany the witness.
    #[serde(default)]
    pub required_companions: Vec<String>,
}

impl Gates {
    pub fn companions(&self) -> Result<Vec<EvidenceKind>, ConfigError> {
        self.required_companions
            .iter()
            .map(|n| {
                evidence_kind(n).ok_or(ConfigError::UnknownName {
                    what: "evidence kind",
                    name: n.clone(),
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Learned {
    pub key: String,
    pub status: String,
    pub reason: String,
}

fn evidence_kind(name: &str) -> Option<EvidenceKind> {
    Some(match name {
        "DriftCheck" => EvidenceKind::DriftCheck,
        "DataWitness" => EvidenceKind::DataWitness,
        "RuntimeExecuted" => EvidenceKind::RuntimeExecuted,
        "RuntimeCited" => EvidenceKind::RuntimeCited,
        _ => return None,
    })
}

fn owner_stage(name: &str) -> Option<OwnerStage> {
    Some(match name {
        "IntakeConfig" => OwnerStage::IntakeConfig,
        "RubyExtraction" => OwnerStage::RubyExtraction,
        "RailsPolyfill" => OwnerStage::RailsPolyfill,
        "ContractHarvest" => OwnerStage::ContractHarvest,
        "RustExtraction" => OwnerStage::RustExtraction,
        "DataWitness" => OwnerStage::DataWitness,
        "RuntimeWitness" => OwnerStage::RuntimeWitness,
        _ => return None,
    })
}

const REASONS: [SlagReason; 6] = [
    SlagReason::RustSideNotExtracted,
    SlagReason::RustSymbolNotInStore,
    SlagReason::RustEvidenceStale,
    SlagReason::RustBodyNotStructured,
    SlagReason::RepresentedAsData,
    SlagReason::RubySideNotHarvested,
];

/// The name `[residual_default.<name>]` and `[[learned]]` use for a reason.
pub fn reason_name(reason: SlagReason) -> String {
    format!("{reason:?}")
}

// ─────────────────────────────────────────────────────────────────────────
// Evaluators (the application binding)
// ─────────────────────────────────────────────────────────────────────────

/// What the application supplies to turn declarative Ruby evidence into
/// [`RubyEvidence`]. Unknown names are errors (`Err`), not "not produced".
pub trait EvidenceSource {
    /// `Some(why)` when the source side is not available at all; every evidence kind
    /// but `not_harvested` then reads as not harvested for that reason.
    fn unavailable(&self) -> Option<String>;
    /// Did the extractor produce the fact the predicate names? `argument` is the
    /// text after the first `:` of the `produced` value.
    fn predicate(&self, name: &str, argument: Option<&str>) -> Result<bool, String>;
    /// The text of `file`, for locating a needle.
    fn text(&self, file: &str) -> Option<&str>;
    /// A named body: `(first line, last line, text)`, 1-based.
    fn body(&self, name: &str) -> Result<Option<(usize, usize, String)>, String>;
    /// A custom evaluator.
    fn witness(&self, name: &str) -> Result<RubyEvidence, String>;
}

/// One instance of a generated claim family.
#[derive(Debug, Clone)]
pub struct GeneratedClaim {
    pub method: String,
    pub effect: String,
    pub span: SourceSpan,
    pub fact: String,
}

/// A fact the extractor produced (`produced`), located at the first `needle` in
/// `file`. Not produced, or not locatable: not harvested.
pub fn fact_evidence(
    produced: bool,
    extractor: &str,
    file: &str,
    needle: &str,
    fact: &str,
    text: Option<&str>,
) -> RubyEvidence {
    match (produced, text.and_then(|t| locate(t, needle))) {
        (true, Some(line)) => RubyEvidence::Fact {
            extractor: extractor.into(),
            span: SourceSpan {
                file: file.into(),
                line_start: line,
                line_end: line,
            },
            fact: fact.into(),
        },
        (false, _) => RubyEvidence::NotHarvested {
            why: format!("{extractor} did not produce: {fact}"),
        },
        (true, None) => RubyEvidence::NotHarvested {
            why: format!("{needle:?} not found in {file}"),
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Loading and checking
// ─────────────────────────────────────────────────────────────────────────

impl OreSlagConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
            path: path.display().to_string(),
            error: e.to_string(),
        })?;
        Self::parse(&text)
    }

    /// Parse and check: unknown keys are rejected by the schema; claim keys and
    /// residual keys must be unique; `[[residual]]` entries need a key and defaults
    /// must not carry one and must name a [`SlagReason`].
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let cfg: OreSlagConfig =
            toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        let mut seen = BTreeSet::new();
        for c in &cfg.claim {
            let k = cfg.join_key(&c.phase, &c.method, &c.effect).to_string();
            if !seen.insert(k.clone()) {
                return Err(ConfigError::DuplicateClaim(k));
            }
        }
        let mut seen = BTreeSet::new();
        for r in &cfg.residual {
            let Some(k) = &r.key else {
                return Err(ConfigError::BadResidualEntry(
                    "a [[residual]] entry has no key".into(),
                ));
            };
            if !seen.insert(k.clone()) {
                return Err(ConfigError::DuplicateResidual(k.clone()));
            }
        }
        for (name, d) in &cfg.residual_default {
            if !REASONS.iter().any(|r| reason_name(*r) == *name) {
                return Err(ConfigError::UnknownName {
                    what: "slag reason in [residual_default]",
                    name: name.clone(),
                });
            }
            if d.key.is_some() {
                return Err(ConfigError::BadResidualEntry(format!(
                    "[residual_default.{name}] must not carry a key"
                )));
            }
        }
        Ok(cfg)
    }

    pub fn join_key(&self, phase: &str, method: &str, effect: &str) -> JoinKey {
        JoinKey {
            application: self.scope.application.clone(),
            model: self.scope.model.clone(),
            operation: self.scope.operation.clone(),
            phase: phase.into(),
            method: method.into(),
            effect: effect.into(),
        }
    }

    /// The extractor text for a name, or several joined with ` + `.
    pub fn extractor(&self, spec: &str) -> Result<String, ConfigError> {
        let parts: Result<Vec<&str>, ConfigError> = spec
            .split(" + ")
            .map(|n| {
                self.extractors
                    .get(n)
                    .map(String::as_str)
                    .ok_or(ConfigError::UnknownName {
                        what: "extractor",
                        name: n.to_string(),
                    })
            })
            .collect();
        Ok(parts?.join(" + "))
    }

    // ── claims ──

    /// The claims in file order, then the generated families' instances.
    pub fn claim_specs(
        &self,
        source: &dyn EvidenceSource,
        generators: &dyn Fn(&str) -> Option<Vec<GeneratedClaim>>,
    ) -> Result<Vec<ClaimSpec>, ConfigError> {
        let mut out = Vec::new();
        for c in &self.claim {
            out.push(ClaimSpec {
                key: self.join_key(&c.phase, &c.method, &c.effect),
                level: c.level.into(),
                ruby: self.ruby_evidence(&c.ruby, source)?,
                rust: c.rust.probe(),
                comparability: c.comparability.comparability(),
                repair: c.repair.clone(),
            });
        }
        for f in &self.claim_family {
            let extractor = self.extractor(&f.extractor)?;
            let instances = generators(&f.generator).ok_or(ConfigError::UnknownName {
                what: "generator",
                name: f.generator.clone(),
            })?;
            for g in instances {
                out.push(ClaimSpec {
                    key: self.join_key(&f.phase, &g.method, &g.effect),
                    level: f.level.into(),
                    ruby: RubyEvidence::Fact {
                        extractor: extractor.clone(),
                        span: g.span,
                        fact: g.fact,
                    },
                    rust: f.rust.probe(),
                    comparability: f.comparability.comparability(),
                    repair: f.repair.clone(),
                });
            }
        }
        let mut seen = BTreeSet::new();
        for s in &out {
            let k = s.key.to_string();
            if !seen.insert(k.clone()) {
                return Err(ConfigError::DuplicateClaim(k));
            }
        }
        Ok(out)
    }

    fn ruby_evidence(
        &self,
        spec: &RubyCfg,
        source: &dyn EvidenceSource,
    ) -> Result<RubyEvidence, ConfigError> {
        if !matches!(spec, RubyCfg::NotHarvested { .. }) {
            if let Some(why) = source.unavailable() {
                return Ok(RubyEvidence::NotHarvested { why });
            }
        }
        let eval = |e: String| ConfigError::Evaluator(e);
        match spec {
            RubyCfg::NotHarvested { why } => Ok(RubyEvidence::NotHarvested { why: why.clone() }),
            RubyCfg::Fact {
                produced,
                extractor,
                file,
                needle,
                fact,
            } => {
                let (name, argument) = match produced.split_once(':') {
                    Some((n, a)) => (n, Some(a)),
                    None => (produced.as_str(), None),
                };
                let produced = source.predicate(name, argument).map_err(eval)?;
                Ok(fact_evidence(
                    produced,
                    &self.extractor(extractor)?,
                    file,
                    needle,
                    fact,
                    source.text(file),
                ))
            }
            RubyCfg::AbsentInBody {
                body,
                must_not_contain,
                looked_for,
                extractor,
                file,
            } => Ok(match source.body(body).map_err(eval)? {
                Some((a, b, text)) if !text.contains(must_not_contain.as_str()) => {
                    RubyEvidence::Absent {
                        extractor: self.extractor(extractor)?,
                        span: SourceSpan {
                            file: file.clone(),
                            line_start: a,
                            line_end: b,
                        },
                        looked_for: looked_for.clone(),
                    }
                }
                Some(_) => RubyEvidence::NotHarvested {
                    why: format!("the body mentions {must_not_contain}"),
                },
                None => RubyEvidence::NotHarvested {
                    why: format!("the body {body} was not found"),
                },
            }),
            RubyCfg::Witness { name } => source.witness(name).map_err(eval),
        }
    }

    // ── residuals ──

    /// The template for a slag row: its own entry, else the default of its reason.
    /// With `fallback`, a row of a reason without a default uses the fallback
    /// reason's default instead of failing (synthetic controls only).
    pub fn residual_template(
        &self,
        key: &str,
        reason: SlagReason,
        fallback: Option<SlagReason>,
    ) -> Result<&ResidualCfg, ConfigError> {
        if let Some(r) = self.residual.iter().find(|r| r.key.as_deref() == Some(key)) {
            return Ok(r);
        }
        let default = |r: SlagReason| self.residual_default.get(&reason_name(r));
        default(reason)
            .or_else(|| fallback.and_then(default))
            .ok_or(ConfigError::MissingResidual {
                key: key.to_string(),
                reason,
            })
    }

    /// The residual of one slag row, rendered from its template.
    pub fn render_residual(
        &self,
        key: &str,
        reason: SlagReason,
        fallback: Option<SlagReason>,
        ctx: &RenderCtx<'_>,
    ) -> Result<Residual, ConfigError> {
        let t = self.residual_template(key, reason, fallback)?;
        if t.todo_fields() {
            return Err(ConfigError::Todo { key: key.into() });
        }
        let bad = |error: String| ConfigError::Render {
            key: key.to_string(),
            error,
        };
        let charon = match &t.charon_residual_symbol {
            Some(sym) => Some((ctx.charon_residual)(sym).map_err(bad)?),
            None => None,
        };
        let sub = |s: &str| substitute(s, charon.as_deref()).map_err(bad);
        let location = match &t.source_location {
            LocationCfg::Locate { locate: l } => {
                (ctx.locate)(&l.root, &l.file, &l.needle).map_err(bad)?
            }
            LocationCfg::Name(n) => match n.as_str() {
                "ruby_loc" => ctx.ruby_loc.clone(),
                "rust_loc" => ctx.rust_loc.clone(),
                "ruby_loc;rust_loc" => format!("{} ; {}", ctx.ruby_loc, ctx.rust_loc),
                other => {
                    return Err(ConfigError::UnknownName {
                        what: "source_location",
                        name: other.to_string(),
                    })
                }
            },
        };
        let revision = match t.source_revision.as_str() {
            "both" => format!(
                "{}; {}",
                (ctx.revision)(&self.scope.source_root).map_err(bad)?,
                (ctx.revision)(&self.scope.target_root).map_err(bad)?
            ),
            name => (ctx.revision)(name).map_err(bad)?,
        };
        let proposal = Proposal {
            missing_fact: sub(&t.missing_fact)?,
            supplier: sub(&t.supplier)?,
            supplier_exists: t.supplier_exists,
            source_location: location,
            source_revision: revision,
            claim_change: sub(&t.claim_change)?,
            validating_evidence: sub(&t.validating_evidence)?,
            negative_control: sub(&t.negative_control)?,
        };
        let mut r = Residual::new(key.to_string(), reason, proposal);
        r.sub_reason = sub(&t.sub_reason)?;
        r.owner_stage = owner_stage(&t.owner_stage).ok_or(ConfigError::UnknownName {
            what: "owner stage",
            name: t.owner_stage.clone(),
        })?;
        r.locator = format!("ruby {}; rust {}", ctx.ruby_loc, ctx.rust_loc);
        Ok(r)
    }

    /// Keys (or default names) of stubs that still carry `"TODO"` fields.
    pub fn todo_keys(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .residual
            .iter()
            .filter(|r| r.todo_fields())
            .filter_map(|r| r.key.clone())
            .collect();
        out.extend(
            self.residual_default
                .iter()
                .filter(|(_, d)| d.todo_fields())
                .map(|(n, _)| format!("[residual_default.{n}]")),
        );
        out
    }
}

/// What a residual's strings are rendered against: the row's own locations and the
/// application's lookups. Each lookup returns `Err(text)` for a name it does not know.
pub struct RenderCtx<'a> {
    pub ruby_loc: String,
    pub rust_loc: String,
    /// `(root name, file, needle)` → `file:line` text (the application owns the
    /// fallback when the root or the needle is missing).
    pub locate: &'a dyn Fn(&str, &str, &str) -> Result<String, String>,
    /// Root name → revision text.
    pub revision: &'a dyn Fn(&str) -> Result<String, String>,
    /// Symbol → the residual reason Charon recorded for it (`{charon_residual}`).
    pub charon_residual: &'a dyn Fn(&str) -> Result<String, String>,
}

/// `{charon_residual}` → `charon`; `{{` and `}}` → literal braces; any other brace is
/// an error.
pub fn substitute(template: &str, charon: Option<&str>) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut it = template.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        match c {
            '{' if matches!(it.peek(), Some((_, '{'))) => {
                it.next();
                out.push('{');
            }
            '}' if matches!(it.peek(), Some((_, '}'))) => {
                it.next();
                out.push('}');
            }
            '{' => {
                let rest = &template[i + 1..];
                let end = rest
                    .find('}')
                    .ok_or_else(|| format!("unclosed placeholder in {template:?}"))?;
                match &rest[..end] {
                    "charon_residual" => out.push_str(charon.ok_or_else(|| {
                        "{charon_residual} used without a charon_residual_symbol".to_string()
                    })?),
                    other => return Err(format!("unknown placeholder {{{other}}}")),
                }
                for _ in 0..end + 1 {
                    it.next();
                }
            }
            '}' => return Err(format!("unmatched `}}` in {template:?}")),
            c => out.push(c),
        }
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────
// Autolearn: the [[learned]] block and TODO stubs
// ─────────────────────────────────────────────────────────────────────────

/// A TOML basic string.
pub fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The expected text from the marker to the end of file, one entry per residual:
/// `(key, status, reason)`.
pub fn render_learned(entries: &[(String, String, String)]) -> String {
    let mut out = format!("{LEARNED_MARKER}\n");
    for (key, status, reason) in entries {
        out.push_str(&format!(
            "\n[[learned]]\nkey = {}\nstatus = {}\nreason = {}\n",
            toml_string(key),
            toml_string(status),
            toml_string(reason)
        ));
    }
    out
}

/// The file's text from the marker to the end, if the marker is there.
pub fn learned_tail(text: &str) -> Option<&str> {
    text.find(LEARNED_MARKER).map(|at| &text[at..])
}

/// `text` with everything from the marker on replaced by `block` (which starts with
/// the marker, as [`render_learned`] returns it). Without a marker the block is
/// appended. The text before the marker is untouched.
pub fn rewrite_learned(text: &str, block: &str) -> String {
    match text.find(LEARNED_MARKER) {
        Some(at) => format!("{}{block}", &text[..at]),
        None => {
            let sep = if text.is_empty() || text.ends_with("\n\n") {
                ""
            } else if text.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            format!("{text}{sep}{block}")
        }
    }
}

/// `text` with a stub `[[residual]]` for `key` inserted before the `[[learned]]`
/// block: every proposal field `"TODO"`, the owner the reason's default.
pub fn append_stub(text: &str, key: &str, reason: SlagReason) -> String {
    let stub = format!(
        "[[residual]]\nkey = {}\nmissing_fact = \"{TODO}\"\nsupplier = \"{TODO}\"\nsupplier_exists = false\nclaim_change = \"{TODO}\"\nvalidating_evidence = \"{TODO}\"\nnegative_control = \"{TODO}\"\nsub_reason = \"{TODO}\"\nowner_stage = {}\nsource_location = \"{TODO}\"\nsource_revision = \"{TODO}\"\n",
        toml_string(key),
        toml_string(&format!("{:?}", OwnerStage::default_for(reason))),
    );
    match text.find(LEARNED_MARKER) {
        Some(at) => format!("{}{stub}\n{}", &text[..at], &text[at..]),
        None => {
            let sep = if text.ends_with("\n\n") || text.is_empty() {
                ""
            } else if text.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            format!("{text}{sep}{stub}")
        }
    }
}

/// Write `text` to `path` through a rename, so a concurrent reader never sees half a
/// file.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
[scope]
application = "app"
model = "Thing"
operation = "make"
source_root = "src"
target_root = "dst"

[extractors]
scan = "the scanner"
lit = "a literal"

[gates]
witness = "w"
drift = "d"
required_companions = ["DriftCheck"]

[[claim]]
phase = "admission"
method = "m1"
effect = "e1"
level = "admission"
repair = "-"
comparability = "same"
ruby = { kind = "fact", produced = "has_x", extractor = "scan + lit", file = "f.rb", needle = "needle", fact = "the fact" }
rust.edges = [["a::b", "a::c"]]

[[claim]]
phase = "admission"
method = "m2"
effect = "e2"
level = "persistence"
repair = "r"
comparability.different = { ruby = "dsl", rust = "call" }
ruby = { kind = "not_harvested", why = "nobody reads it" }
rust.data = { krate = "a", file = "g.rs", item = "TABLE" }

[residual_default.RustBodyNotStructured]
missing_fact = "a body for {charon_residual} (literal {{braces}})"
supplier = "s"
supplier_exists = false
claim_change = "c"
validating_evidence = "v"
negative_control = "n"
sub_reason = "Charon residual {charon_residual}"
owner_stage = "RustExtraction"
source_location = "rust_loc"
source_revision = "both"
charon_residual_symbol = "a::b"
"#;

    struct Src {
        available: bool,
        produced: bool,
    }

    impl EvidenceSource for Src {
        fn unavailable(&self) -> Option<String> {
            (!self.available).then(|| "no source".to_string())
        }
        fn predicate(&self, name: &str, _: Option<&str>) -> Result<bool, String> {
            match name {
                "has_x" => Ok(self.produced),
                other => Err(format!("unknown predicate {other}")),
            }
        }
        fn text(&self, _: &str) -> Option<&str> {
            Some("line one\nthe needle is here\n")
        }
        fn body(&self, _: &str) -> Result<Option<(usize, usize, String)>, String> {
            Ok(None)
        }
        fn witness(&self, name: &str) -> Result<RubyEvidence, String> {
            Err(format!("unknown witness {name}"))
        }
    }

    fn no_generators(_: &str) -> Option<Vec<GeneratedClaim>> {
        Some(Vec::new())
    }

    #[test]
    fn load_rejects_an_unknown_key() {
        // At the top level, in a claim, and inside a tagged ruby spec.
        for (needle, extra) in [
            ("[gates]", "stray = 1\n[gates]"),
            ("level = \"admission\"", "level = \"admission\"\nstray = 1"),
            ("fact = \"the fact\"", "fact = \"the fact\", stray = 1"),
        ] {
            let bad = BASE.replacen(needle, extra, 1);
            assert_ne!(bad, BASE, "the edit applied to {needle}");
            let e = OreSlagConfig::parse(&bad).expect_err(needle).to_string();
            assert!(e.contains("stray"), "{needle}: {e}");
        }
        assert!(OreSlagConfig::parse(BASE).is_ok());
    }

    #[test]
    fn load_rejects_a_duplicate_claim_key() {
        let dup = format!(
            "{BASE}\n[[claim]]\nphase = \"admission\"\nmethod = \"m1\"\neffect = \"e1\"\nlevel = \"admission\"\nrepair = \"x\"\ncomparability = \"same\"\nruby = {{ kind = \"not_harvested\", why = \"w\" }}\nrust.edges = [[\"a::b\", \"a::c\"]]\n"
        );
        let e = OreSlagConfig::parse(&dup)
            .expect_err("duplicate")
            .to_string();
        assert!(e.contains("duplicate claim key"), "{e}");
        assert!(e.contains("app/Thing/make/admission/m1/e1"), "{e}");
    }

    #[test]
    fn load_rejects_a_duplicate_residual_key() {
        let one = "\n[[residual]]\nkey = \"k\"\nmissing_fact = \"m\"\nsupplier = \"s\"\nsupplier_exists = true\nclaim_change = \"c\"\nvalidating_evidence = \"v\"\nnegative_control = \"n\"\nsub_reason = \"r\"\nowner_stage = \"DataWitness\"\nsource_location = \"ruby_loc\"\nsource_revision = \"src\"\n";
        assert!(OreSlagConfig::parse(&format!("{BASE}{one}")).is_ok());
        let e = OreSlagConfig::parse(&format!("{BASE}{one}{one}"))
            .expect_err("duplicate")
            .to_string();
        assert!(e.contains("duplicate residual key: k"), "{e}");
    }

    #[test]
    fn a_template_placeholder_renders() {
        assert_eq!(
            substitute(
                "Charon residual {charon_residual}",
                Some("BODY_NOT_STRUCTURED")
            )
            .unwrap(),
            "Charon residual BODY_NOT_STRUCTURED"
        );
        assert_eq!(substitute("{{a}} {{", None).unwrap(), "{a} {");
        assert!(substitute("{nope}", Some("x"))
            .unwrap_err()
            .contains("nope"));
        assert!(substitute("{charon_residual}", None).is_err());
        assert!(substitute("a } b", None).is_err());
    }

    fn ctx<'a>(
        locate: &'a dyn Fn(&str, &str, &str) -> Result<String, String>,
        revision: &'a dyn Fn(&str) -> Result<String, String>,
        charon: &'a dyn Fn(&str) -> Result<String, String>,
    ) -> RenderCtx<'a> {
        RenderCtx {
            ruby_loc: "RB".into(),
            rust_loc: "RS".into(),
            locate,
            revision,
            charon_residual: charon,
        }
    }

    #[test]
    fn a_default_renders_with_placeholders_locations_and_revisions() {
        let cfg = OreSlagConfig::parse(BASE).unwrap();
        let locate = |_: &str, _: &str, _: &str| Ok(String::new());
        let revision = |n: &str| Ok(format!("rev-{n}"));
        let charon = |s: &str| Ok(format!("RESIDUAL({s})"));
        let c = ctx(&locate, &revision, &charon);
        let r = cfg
            .render_residual("any/key", SlagReason::RustBodyNotStructured, None, &c)
            .unwrap();
        assert_eq!(
            r.proposal.missing_fact,
            "a body for RESIDUAL(a::b) (literal {braces})"
        );
        assert_eq!(r.sub_reason, "Charon residual RESIDUAL(a::b)");
        assert_eq!(r.proposal.source_location, "RS");
        assert_eq!(r.proposal.source_revision, "rev-src; rev-dst");
        assert_eq!(r.locator, "ruby RB; rust RS");
        assert_eq!(r.owner_stage, OwnerStage::RustExtraction);
    }

    #[test]
    fn a_claim_key_without_a_residual_is_a_missing_residual_naming_the_key() {
        let cfg = OreSlagConfig::parse(BASE).unwrap();
        let locate = |_: &str, _: &str, _: &str| Ok(String::new());
        let revision = |n: &str| Ok(n.to_string());
        let charon = |_: &str| Ok(String::new());
        let c = ctx(&locate, &revision, &charon);
        // The default covers RustBodyNotStructured; RubySideNotHarvested has none.
        let e = cfg
            .render_residual(
                "app/Thing/make/admission/m1/e1",
                SlagReason::RubySideNotHarvested,
                None,
                &c,
            )
            .expect_err("missing");
        assert!(matches!(e, ConfigError::MissingResidual { .. }), "{e}");
        let text = e.to_string();
        assert!(text.contains("missing residual"), "{text}");
        assert!(text.contains("app/Thing/make/admission/m1/e1"), "{text}");
        assert!(text.contains("RubySideNotHarvested"), "{text}");
        // The synthetic-control fallback borrows another reason's default.
        assert!(cfg
            .render_residual(
                "app/Thing/make/admission/m1/e1",
                SlagReason::RubySideNotHarvested,
                Some(SlagReason::RustBodyNotStructured),
                &c
            )
            .is_ok());
    }

    #[test]
    fn claims_follow_the_declared_evidence() {
        let cfg = OreSlagConfig::parse(BASE).unwrap();
        let specs = cfg
            .claim_specs(
                &Src {
                    available: true,
                    produced: true,
                },
                &no_generators,
            )
            .unwrap();
        assert_eq!(specs.len(), 2);
        match &specs[0].ruby {
            RubyEvidence::Fact {
                extractor, span, ..
            } => {
                assert_eq!(extractor, "the scanner + a literal");
                assert_eq!((span.file.as_str(), span.line_start), ("f.rb", 2));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(specs[1].ruby, RubyEvidence::NotHarvested { .. }));
        assert!(matches!(
            specs[1].comparability,
            Comparability::Different { .. }
        ));
        assert!(matches!(specs[1].rust, RustProbe::Data { .. }));
        // Not produced, and no source at all: each is not harvested, for its own reason.
        let not = cfg
            .claim_specs(
                &Src {
                    available: true,
                    produced: false,
                },
                &no_generators,
            )
            .unwrap();
        assert!(
            matches!(&not[0].ruby, RubyEvidence::NotHarvested { why } if why.contains("did not produce"))
        );
        let none = cfg
            .claim_specs(
                &Src {
                    available: false,
                    produced: true,
                },
                &no_generators,
            )
            .unwrap();
        assert!(matches!(&none[0].ruby, RubyEvidence::NotHarvested { why } if why == "no source"));
    }

    #[test]
    fn an_unknown_predicate_extractor_or_generator_is_an_error() {
        let src = Src {
            available: true,
            produced: true,
        };
        let bad_predicate = BASE.replace("has_x", "has_y");
        let e = OreSlagConfig::parse(&bad_predicate)
            .unwrap()
            .claim_specs(&src, &no_generators)
            .expect_err("predicate");
        assert!(e.to_string().contains("has_y"), "{e}");
        let bad_extractor = BASE.replace("scan + lit", "scan + lot");
        let e = OreSlagConfig::parse(&bad_extractor)
            .unwrap()
            .claim_specs(&src, &no_generators)
            .expect_err("extractor");
        assert!(e.to_string().contains("lot"), "{e}");
        let with_family = format!(
            "{BASE}\n[[claim_family]]\ngenerator = \"nope\"\nphase = \"guard\"\nlevel = \"guard_evaluation\"\nextractor = \"scan\"\nrepair = \"r\"\ncomparability = \"same\"\nrust.edges = [[\"a::b\", \"a::c\"]]\n"
        );
        let e = OreSlagConfig::parse(&with_family)
            .unwrap()
            .claim_specs(&src, &|_| None)
            .expect_err("generator");
        assert!(e.to_string().contains("nope"), "{e}");
    }

    #[test]
    fn a_family_yields_one_claim_per_generated_instance() {
        let with_family = format!(
            "{BASE}\n[[claim_family]]\ngenerator = \"gen\"\nphase = \"guard\"\nlevel = \"guard_evaluation\"\nextractor = \"scan\"\nrepair = \"r\"\ncomparability = \"same\"\nrust.edges = [[\"a::b\", \"a::c\"]]\n"
        );
        let cfg = OreSlagConfig::parse(&with_family).unwrap();
        let gen = |_: &str| {
            let one = |m: &str| GeneratedClaim {
                method: m.into(),
                effect: "guard:lowered".into(),
                span: SourceSpan {
                    file: "g.rb".into(),
                    line_start: 3,
                    line_end: 3,
                },
                fact: "f".into(),
            };
            Some(vec![one("A"), one("B")])
        };
        let src = Src {
            available: true,
            produced: true,
        };
        let specs = cfg.claim_specs(&src, &gen).unwrap();
        assert_eq!(specs.len(), 4);
        assert_eq!(
            specs[3].key.to_string(),
            "app/Thing/make/guard/B/guard:lowered"
        );
        // The same instance twice is a duplicate claim.
        let twice = |_: &str| {
            let one = GeneratedClaim {
                method: "A".into(),
                effect: "e".into(),
                span: SourceSpan {
                    file: "g.rb".into(),
                    line_start: 3,
                    line_end: 3,
                },
                fact: "f".into(),
            };
            Some(vec![one.clone(), one])
        };
        assert!(matches!(
            cfg.claim_specs(&src, &twice),
            Err(ConfigError::DuplicateClaim(_))
        ));
    }

    #[test]
    fn the_learned_block_rewrites_only_the_tail() {
        let entries = vec![
            (
                "a/b".to_string(),
                "OPEN".to_string(),
                "RustBodyNotStructured".to_string(),
            ),
            (
                "q\"uote".to_string(),
                "RESOLVED(DataWitness)".to_string(),
                "RepresentedAsData".to_string(),
            ),
        ];
        let block = render_learned(&entries);
        assert!(block.starts_with(LEARNED_MARKER));
        let hand_written = format!("{BASE}\n");
        let file = rewrite_learned(&hand_written, &block);
        assert!(file.starts_with(&hand_written));
        assert_eq!(learned_tail(&file), Some(block.as_str()));
        // The written file parses, and the learned entries round-trip.
        let cfg = OreSlagConfig::parse(&file).unwrap();
        assert_eq!(cfg.learned.len(), 2);
        assert_eq!(cfg.learned[1].key, "q\"uote");
        // A second rewrite replaces the tail and leaves the head byte for byte.
        let next = render_learned(&entries[..1]);
        let again = rewrite_learned(&file, &next);
        assert!(again.starts_with(&hand_written));
        assert_eq!(learned_tail(&again), Some(next.as_str()));
        assert_ne!(learned_tail(&again), learned_tail(&file));
    }

    #[test]
    fn a_stub_is_inserted_before_the_learned_block_and_blocks_until_filled() {
        let file = rewrite_learned(&format!("{BASE}\n"), &render_learned(&[]));
        let stubbed = append_stub(
            &file,
            "app/Thing/make/admission/m1/e1",
            SlagReason::RubySideNotHarvested,
        );
        let at_stub = stubbed.find("[[residual]]").expect("stub");
        let at_marker = stubbed.find(LEARNED_MARKER).expect("marker");
        assert!(at_stub < at_marker, "the learned block stays last");
        assert_eq!(learned_tail(&stubbed), learned_tail(&file));
        let cfg = OreSlagConfig::parse(&stubbed).unwrap();
        assert_eq!(cfg.todo_keys(), ["app/Thing/make/admission/m1/e1"]);
        assert_eq!(
            cfg.residual[0].owner_stage,
            format!(
                "{:?}",
                OwnerStage::default_for(SlagReason::RubySideNotHarvested)
            )
        );
        // Rendering a stub refuses; the stub resolves the missing-residual error.
        let locate = |_: &str, _: &str, _: &str| Ok(String::new());
        let revision = |n: &str| Ok(n.to_string());
        let charon = |_: &str| Ok(String::new());
        let c = ctx(&locate, &revision, &charon);
        let e = cfg
            .render_residual(
                "app/Thing/make/admission/m1/e1",
                SlagReason::RubySideNotHarvested,
                None,
                &c,
            )
            .expect_err("todo");
        assert!(matches!(e, ConfigError::Todo { .. }), "{e}");
        // The file without the stub reports nothing to do.
        assert!(OreSlagConfig::parse(&file).unwrap().todo_keys().is_empty());
    }
}
