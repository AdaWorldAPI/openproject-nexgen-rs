// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The class proposer: inert groups of attributes that share a witness.
//!
//! A Rails class that mixes many concerns should not become one ON class. The
//! split falls out of shared witnesses: attributes of one model that are
//! written under the same guard, need the same permission or read the same
//! model fields belong together. The crosswalk's attribute facts become rows
//! of categorical codes; a group is every set of two or more rows of one model
//! that share a witness value other than "none".
//!
//! A proposal is a claim bound to its witness. Its truth comes from the oracle
//! trace, not from the rows: the translator of `lance-graph-arm-discovery`
//! turns "in how many compared cases do all members co-vary" into an integer
//! NARS truth. A proposal the trace does not cover has no truth.
//!
//! Association-rule mining is not used. Guards, permissions and models are
//! categorical and have no metric, so the Aerial+ probe has nothing to rank
//! consequents by; under an identity distance it only ever proposes the
//! "none" category and its confidence measures nothing about the witness.
//!
//! Nothing here registers, mints or applies anything: the classid and its
//! DO calls belong to the DT64 registry, never to a proposer.
//!
//! # Copy provenance (OpenProject)
//!
//! Copied from `zammad-rs/crates/rails-codegraph/src/class_proposer.rs`
//! (Zammad Foundation copyright header kept above). Every difference from the
//! original:
//!
//! 1. This module doc section (the only change to the file itself).
//!
//! The `use crate::crosswalk::{AttributeFact, CrosswalkReport}` import is
//! unchanged: this crate's `crosswalk` module defines the same names with the
//! same field names and types, but only the fields the proposer reads. The
//! `crate::RUFF_REV` / `crate::LANCE_GRAPH_REV` provenance constants it names
//! are defined in this crate's `lib.rs`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use lance_graph_arm_discovery::ndjson::to_ndjson;
use lance_graph_arm_discovery::translator::{arm_to_nars, arm_to_truth_u8, CandidateTriple};
use lance_graph_arm_discovery::CandidateRule;
use serde::{Deserialize, Serialize};

use crate::crosswalk::{AttributeFact, CrosswalkReport};

const FEATURES: [&str; 5] = ["model", "guard", "permission", "block", "reads"];
const MODEL: usize = 0;
const GUARD: usize = 1;
const PERMISSION: usize = 2;
const BLOCK: usize = 3;
const READS: usize = 4;
/// The features that can bind attributes together, in the order proposals sort.
const WITNESSES: [usize; 3] = [GUARD, PERMISSION, READS];
const PREDICATE: &str = "shares_witness";

/// `[klickwege.proposer]`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProposerConfig {
    /// Members a group needs to be proposed (at least 2 count anyway).
    pub min_support: u32,
    /// The NARS evidence constant of the truth translation; at least 1.
    pub k: u32,
    /// A trace in the shape of the oracle file, relative to the base directory.
    #[serde(default)]
    pub oracle_trace: Option<String>,
}

impl ProposerConfig {
    fn validate(&self) -> Result<(), String> {
        if self.k == 0 {
            return Err("klickwege.proposer: k must be at least 1".into());
        }
        if self.min_support == 0 {
            return Err("klickwege.proposer: min_support must be at least 1".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Witness {
    pub feature: String,
    pub value: String,
    pub code: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Truth {
    pub frequency: u8,
    pub confidence: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Evidence {
    /// Compared cases in which all members co-vary.
    pub cooccur: u32,
    /// Cases that list two or more members.
    pub antecedent_count: u32,
    /// Cases in the trace.
    pub window: u32,
}

/// What the oracle trace says about a proposal's members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Oracle {
    /// In every case that lists two or more members, all are writable or none.
    CoVaries {
        compared_cases: usize,
    },
    /// In `case`, the first of `compared - co_varying` such cases, some members
    /// are writable and some are not.
    Diverges {
        case: String,
        contract: String,
        members: Vec<(String, bool)>,
        compared: usize,
        co_varying: usize,
    },
    NotCovered {
        why: String,
    },
}

impl Oracle {
    #[must_use]
    pub fn verdict(&self) -> &'static str {
        match self {
            Self::CoVaries { .. } => "co-varies",
            Self::Diverges { .. } => "diverges",
            Self::NotCovered { .. } => "not covered",
        }
    }

    fn detail(&self) -> serde_json::Value {
        match self {
            Self::CoVaries { compared_cases } => {
                serde_json::json!({ "compared_cases": compared_cases })
            }
            Self::Diverges {
                case,
                contract,
                members,
                compared,
                co_varying,
            } => serde_json::json!({
                "case": case,
                "contract": contract,
                "compared_cases": compared,
                "co_varying_cases": co_varying,
                "members": members
                    .iter()
                    .map(|(m, w)| serde_json::json!({ "member": m, "writable": w }))
                    .collect::<Vec<_>>(),
            }),
            Self::NotCovered { why } => serde_json::json!({ "why": why }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassProposal {
    pub id: String,
    pub model: String,
    pub witness: Witness,
    /// `Class#attribute`, in source order.
    pub members: Vec<String>,
    /// From the oracle evidence; `None` when the trace does not cover the proposal.
    pub truth: Option<Truth>,
    pub evidence: Option<Evidence>,
    /// `None` when no oracle trace is configured.
    pub oracle: Option<Oracle>,
    pub file: String,
    pub line: u32,
}

/// Where every attribute row of one model went.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPlacement {
    pub model: String,
    pub rows: usize,
    /// Rows that are a member of at least one proposal, each counted once.
    pub clustered: usize,
    /// The first proposal each clustered row is a member of.
    pub first_in: BTreeMap<String, String>,
    pub unclustered: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OracleCoverage {
    pub trace: String,
    pub cases: usize,
    /// Cases whose contract is not a harvested class or has no model.
    pub cases_without_model: usize,
    /// Proposals per `<witness feature>/<verdict>`.
    pub verdicts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProposerCoverage {
    /// Attribute declarations the crosswalk classified.
    pub attributes_classified: usize,
    pub rows: usize,
    /// Attributes of modules: no model of their own, so no row.
    pub skipped_in_modules: usize,
    /// A second declaration of an attribute the class already declared.
    pub redeclared: usize,
    /// Rows whose `permission:` has a value that is not a literal.
    pub permission_values_not_literal: usize,
    pub models: usize,
    pub proposals: usize,
    pub proposals_by_feature: BTreeMap<String, usize>,
    pub unclustered: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oracle: Option<OracleCoverage>,
}

#[derive(Debug, Clone)]
pub struct ClassProposals {
    pub config: ProposerConfig,
    pub run_status: &'static str,
    /// Per feature, its dictionary: the value of every code.
    pub dictionaries: Vec<(String, Vec<String>)>,
    pub proposals: Vec<ClassProposal>,
    pub models: Vec<ModelPlacement>,
    pub coverage: ProposerCoverage,
    ndjson: String,
}

struct Dictionary {
    values: Vec<String>,
    at: HashMap<String, u32>,
}

impl Dictionary {
    fn new() -> Self {
        Self {
            values: vec!["none".to_string()],
            at: HashMap::new(),
        }
    }

    /// Codes are assigned in first-seen order; 0 is "none".
    fn code(&mut self, value: &str) -> u32 {
        if let Some(&c) = self.at.get(value) {
            return c;
        }
        let c = self.values.len() as u32;
        self.values.push(value.to_string());
        self.at.insert(value.to_string(), c);
        c
    }
}

struct Row {
    label: String,
    attribute: String,
    model: String,
    file: String,
    line: u32,
    codes: [u32; 5],
}

fn render_guard(f: &AttributeFact) -> Option<String> {
    f.guard
        .as_ref()
        .map(|g| {
            format!(
                "self_calls: {}; receiver_reads: {}",
                g.self_calls.join(", "),
                g.receiver_reads.join(", ")
            )
        })
        .or_else(|| f.writable_literal.map(|w| format!("writable={w}")))
}

fn build_rows(
    facts: &[AttributeFact],
    dicts: &mut [Dictionary; 5],
    coverage: &mut ProposerCoverage,
) -> Vec<Row> {
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut rows = Vec::new();
    coverage.attributes_classified = facts.len();
    for f in facts {
        let Some(model) = &f.model else {
            coverage.skipped_in_modules += 1;
            continue;
        };
        if !seen.insert((f.class.as_str(), f.name.as_str())) {
            coverage.redeclared += 1;
            continue;
        }
        coverage.permission_values_not_literal += usize::from(f.permission_not_literal);
        let joined = |names: &[String]| (!names.is_empty()).then(|| names.join(","));
        let mut code =
            |feature: usize, value: Option<String>| value.map_or(0, |v| dicts[feature].code(&v));
        let codes = [
            code(MODEL, Some(model.clone())),
            code(GUARD, render_guard(f)),
            code(PERMISSION, joined(&f.permissions)),
            u32::from(f.block),
            code(READS, joined(&f.block_reads)),
        ];
        rows.push(Row {
            label: format!("{}#{}", f.class, f.name),
            attribute: f.name.clone(),
            model: model.clone(),
            file: f.file.clone(),
            line: f.line,
            codes,
        });
    }
    coverage.rows = rows.len();
    rows
}

/// `(model code, witness feature, witness code)`.
type GroupKey = (u32, usize, u32);

/// Groups the rows of the crosswalk's attribute facts by shared witness and
/// takes each group's truth from the oracle trace.
pub fn propose(
    report: &CrosswalkReport,
    config: &ProposerConfig,
    base: &Path,
) -> Result<ClassProposals, String> {
    config.validate()?;
    let mut dicts: [Dictionary; 5] = std::array::from_fn(|_| Dictionary::new());
    dicts[BLOCK].code("attached");
    let mut coverage = ProposerCoverage::default();
    let rows = build_rows(&report.attribute_facts, &mut dicts, &mut coverage);
    if coverage.attributes_classified
        != coverage.rows + coverage.skipped_in_modules + coverage.redeclared
    {
        return Err(format!(
            "class-proposer: {} attributes != {} rows + {} in modules + {} redeclared",
            coverage.attributes_classified,
            coverage.rows,
            coverage.skipped_in_modules,
            coverage.redeclared
        ));
    }

    let mut rows_of: BTreeMap<GroupKey, Vec<usize>> = BTreeMap::new();
    for (at, row) in rows.iter().enumerate() {
        for feature in WITNESSES {
            if row.codes[feature] != 0 {
                rows_of
                    .entry((row.codes[MODEL], feature, row.codes[feature]))
                    .or_default()
                    .push(at);
            }
        }
    }
    let floor = config.min_support.max(2) as usize;
    let mut groups: Vec<(&GroupKey, &Vec<usize>)> =
        rows_of.iter().filter(|(_, m)| m.len() >= floor).collect();
    let name_of = |key: &GroupKey| dicts[MODEL].values[key.0 as usize].clone();
    groups.sort_by_key(|(k, _)| (name_of(k), k.1, k.2));

    let mut proposals = Vec::new();
    let mut member_rows: Vec<&Vec<usize>> = Vec::new();
    for (key, members) in groups {
        let first = &rows[members[0]];
        proposals.push(ClassProposal {
            id: format!("P{}", proposals.len() + 1),
            model: name_of(key),
            witness: Witness {
                feature: FEATURES[key.1].to_string(),
                value: dicts[key.1].values[key.2 as usize].clone(),
                code: key.2,
            },
            members: members.iter().map(|&m| rows[m].label.clone()).collect(),
            truth: None,
            evidence: None,
            oracle: None,
            file: first.file.clone(),
            line: first.line,
        });
        member_rows.push(members);
    }
    coverage.proposals = proposals.len();
    for p in &proposals {
        *coverage
            .proposals_by_feature
            .entry(p.witness.feature.clone())
            .or_default() += 1;
    }

    let mut triples = Vec::new();
    if let Some(trace) = &config.oracle_trace {
        coverage.oracle = Some(check_oracle(
            &base.join(trace),
            trace,
            report,
            &rows,
            &mut proposals,
            &member_rows,
            config.k,
        )?);
        for p in &proposals {
            if let Some(e) = &p.evidence {
                let rule = CandidateRule {
                    antecedent: Vec::new(),
                    consequent: Vec::new(),
                    cooccur: e.cooccur,
                    antecedent_count: e.antecedent_count,
                    window: e.window,
                };
                let t = arm_to_nars(&rule, config.k);
                triples.push(CandidateTriple {
                    s: format!(
                        "model={}&{}={}",
                        p.model, p.witness.feature, p.witness.value
                    ),
                    p: PREDICATE.to_string(),
                    o: "writable_together".to_string(),
                    f: t.frequency,
                    c: t.confidence,
                });
            }
        }
    }

    let models = placements(&rows, &proposals)?;
    coverage.models = models.len();
    coverage.unclustered = models.iter().map(|m| m.unclustered.len()).sum();
    Ok(ClassProposals {
        config: config.clone(),
        run_status: if report.coverage.partial {
            "PARTIAL"
        } else {
            "COMPLETE"
        },
        dictionaries: FEATURES
            .iter()
            .zip(&dicts)
            .map(|(n, d)| ((*n).to_string(), d.values.clone()))
            .collect(),
        proposals,
        models,
        coverage,
        ndjson: to_ndjson(&triples),
    })
}

/// Every row of a model is a member of a proposal or listed as unclustered,
/// once. A row in several proposals is placed in the first.
fn placements(rows: &[Row], proposals: &[ClassProposal]) -> Result<Vec<ModelPlacement>, String> {
    let mut by_model: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for row in rows {
        by_model.entry(row.model.as_str()).or_default().push(row);
    }
    let mut out = Vec::new();
    for (model, model_rows) in by_model {
        let mut first_in: BTreeMap<String, String> = BTreeMap::new();
        for p in proposals.iter().filter(|p| p.model == model) {
            for member in &p.members {
                first_in
                    .entry(member.clone())
                    .or_insert_with(|| p.id.clone());
            }
        }
        let unclustered: Vec<String> = model_rows
            .iter()
            .filter(|r| !first_in.contains_key(&r.label))
            .map(|r| r.label.clone())
            .collect();
        if model_rows.len() != first_in.len() + unclustered.len() {
            return Err(format!(
                "class-proposer: {model}: {} rows != {} clustered + {} unclustered",
                model_rows.len(),
                first_in.len(),
                unclustered.len()
            ));
        }
        out.push(ModelPlacement {
            model: model.to_string(),
            rows: model_rows.len(),
            clustered: first_in.len(),
            first_in,
            unclustered,
        });
    }
    Ok(out)
}

#[derive(Deserialize)]
struct Trace {
    cases: Vec<TraceCase>,
}

#[derive(Deserialize)]
struct TraceCase {
    contract: String,
    name: String,
    writable: BTreeMap<String, bool>,
}

/// Checks every proposal against the trace: in each case whose contract
/// resolves to the proposal's model, the members the trace lists must be all
/// writable or all not.
///
/// Members are matched to a case by attribute name, not through the ancestor
/// chain between the case's contract and the member's class: that chain is
/// the crosswalk's reading, and a check of the crosswalk's output must not
/// depend on it.
fn check_oracle(
    path: &Path,
    shown: &str,
    report: &CrosswalkReport,
    rows: &[Row],
    proposals: &mut [ClassProposal],
    member_rows: &[&Vec<usize>],
    k: u32,
) -> Result<OracleCoverage, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let trace: Trace =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let classes: HashMap<&str, _> = report
        .classes
        .iter()
        .map(|c| (c.class.as_str(), c))
        .collect();
    let model_of = |case: &TraceCase| {
        classes
            .get(case.contract.as_str())
            .and_then(|c| c.model.as_deref())
    };
    let mut coverage = OracleCoverage {
        trace: shown.to_string(),
        cases: trace.cases.len(),
        cases_without_model: trace.cases.iter().filter(|c| model_of(c).is_none()).count(),
        verdicts: BTreeMap::new(),
    };
    let window = trace.cases.len() as u32;
    for (proposal, members) in proposals.iter_mut().zip(member_rows) {
        let cases: Vec<&TraceCase> = trace
            .cases
            .iter()
            .filter(|c| model_of(c) == Some(proposal.model.as_str()))
            .collect();
        let verdict = if cases.is_empty() {
            Oracle::NotCovered {
                why: "no case of the trace has a contract that resolves to this model".into(),
            }
        } else {
            let mut compared = 0;
            let mut co_varying = 0;
            let mut first_divergence = None;
            for case in cases {
                let values: Vec<(String, bool)> = members
                    .iter()
                    .map(|&m| &rows[m])
                    .filter_map(|r| {
                        case.writable
                            .get(&r.attribute)
                            .map(|&w| (r.label.clone(), w))
                    })
                    .collect();
                if values.len() < 2 {
                    continue;
                }
                compared += 1;
                if values.iter().all(|(_, w)| *w == values[0].1) {
                    co_varying += 1;
                } else if first_divergence.is_none() {
                    first_divergence = Some((case.name.clone(), case.contract.clone(), values));
                }
            }
            match first_divergence {
                _ if compared == 0 => Oracle::NotCovered {
                    why: "no case lists two members of the proposal".into(),
                },
                None => Oracle::CoVaries {
                    compared_cases: compared,
                },
                Some((case, contract, members)) => Oracle::Diverges {
                    case,
                    contract,
                    members,
                    compared,
                    co_varying,
                },
            }
        };
        let (compared, co_varying) = match &verdict {
            Oracle::CoVaries { compared_cases } => (*compared_cases, *compared_cases),
            Oracle::Diverges {
                compared,
                co_varying,
                ..
            } => (*compared, *co_varying),
            Oracle::NotCovered { .. } => (0, 0),
        };
        if compared > 0 {
            let evidence = Evidence {
                cooccur: co_varying as u32,
                antecedent_count: compared as u32,
                window,
            };
            let t = arm_to_truth_u8(
                &CandidateRule {
                    antecedent: Vec::new(),
                    consequent: Vec::new(),
                    cooccur: evidence.cooccur,
                    antecedent_count: evidence.antecedent_count,
                    window,
                },
                k,
            );
            proposal.truth = Some(Truth {
                frequency: t.frequency,
                confidence: t.confidence,
            });
            proposal.evidence = Some(evidence);
        }
        *coverage
            .verdicts
            .entry(format!(
                "{}/{}",
                proposal.witness.feature,
                verdict.verdict()
            ))
            .or_default() += 1;
        proposal.oracle = Some(verdict);
    }
    Ok(coverage)
}

impl ClassProposals {
    /// The rules behind the proposals as arm-discovery ndjson, predicate
    /// `shares_witness`.
    #[must_use]
    pub fn ndjson(&self) -> &str {
        &self.ndjson
    }

    #[must_use]
    pub fn to_json(&self, app: &str, source_rev: &str) -> serde_json::Value {
        let provenance = serde_json::json!({
            "tool": "rails-codegraph",
            "ruff_rev": crate::RUFF_REV,
            "lance_graph_rev": crate::LANCE_GRAPH_REV,
            "source_rev": source_rev,
            "scope": "corpus",
            "run_status": self.run_status,
        });
        let proposals: Vec<serde_json::Value> = self
            .proposals
            .iter()
            .map(|p| {
                let mut body = serde_json::json!({
                    "kind": "class_split_candidate",
                    "id": p.id,
                    "model": p.model,
                    "witness": p.witness,
                    "members": p.members,
                    "truth": p.truth,
                    "evidence": p.evidence,
                    "applied": false,
                });
                let uncovered = Oracle::NotCovered {
                    why: "no oracle trace is configured".into(),
                };
                {
                    let o = p.oracle.as_ref().unwrap_or(&uncovered);
                    body["oracle"] = o.verdict().into();
                    body["oracle_detail"] = o.detail();
                    if p.witness.feature == "guard" && matches!(o, Oracle::Diverges { .. }) {
                        body["defect"] = "a guard proposal that diverges is a defect in the proposer or the crosswalk, not a finding about the application".into();
                    }
                }
                serde_json::json!({
                    "fileName": p.file,
                    "start": p.line,
                    "end": p.line,
                    "symbol": format!("{}:{}", p.model, p.id),
                    "comment": format!(
                        "Class split candidate: {} attributes of {} share {} {:?}. Source-structure evidence; nothing is registered, minted or applied.",
                        p.members.len(), p.model, p.witness.feature, p.witness.value
                    ),
                    "severity": "info",
                    "evidence_class": "SOURCE_STRUCTURE",
                    "qualifier": null,
                    "confidence": "UNKNOWN",
                    "provenance": provenance,
                    "lifecycle": { "run_status": self.run_status, "status": "proposed" },
                    "proposal": body,
                })
            })
            .collect();
        serde_json::json!({
            "app": app,
            "tool": "rails-codegraph",
            "kind": "class_proposals",
            "applied": false,
            "provenance": provenance,
            "config": {
                "min_support": self.config.min_support,
                "k": self.config.k,
                "oracle_trace": self.config.oracle_trace,
            },
            "identity": "attributes classified == rows + skipped in modules + redeclared; per model, rows == clustered + unclustered; truth only where the oracle compared cases",
            "coverage": self.coverage,
            "dictionaries": self
                .dictionaries
                .iter()
                .map(|(n, v)| (n.clone(), serde_json::json!(v)))
                .collect::<serde_json::Map<_, _>>(),
            "proposals": proposals,
            "models": self.models,
        })
    }
}
