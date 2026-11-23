// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The transposition matrix: one auditable row per scope declaration and per
//! call site found in a Rails application, each with the verdict the
//! transposer reached and everything needed to check it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rails_compiler::config::Loaded;
use rails_compiler::harvest::Harvest;
use regex::Regex;
use serde::Serialize;

use crate::exec::{Engine, field_lanes};
use crate::ogar_basis::{CONCEPT_BASE, FieldMeta, OgarBasis};
use crate::transpose::{Receiver, RefuseReason, Request, ScopeCatalog, Terminal, Transposed, Verdict, transpose};

#[derive(Debug, Serialize)]
pub struct FieldIdentity {
    pub name: String,
    pub predicate_iri: String,
    pub type_name: String,
    pub required: bool,
}

#[derive(Debug, Serialize)]
pub struct OgarIdentity {
    pub class: String,
    pub lab_classid: Option<String>,
    pub fields: Vec<FieldIdentity>,
}

#[derive(Debug, Serialize)]
pub struct RuffEvidence {
    pub decl_kind: String,
    pub anchor_byte: Option<usize>,
    /// Whether ruff's lifted OGAR class lists this scope name.
    pub listed_by_ruff: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct Admission {
    /// `prepared`, `registered`, `refused` or `not_attempted`.
    pub status: String,
    pub refusal: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub id: String,
    pub kind: String,
    pub ruby: String,
    pub location: String,
    pub rails_version: String,
    pub ruff_evidence: RuffEvidence,
    pub ogar_identity: Option<OgarIdentity>,
    pub candidate_words: Vec<String>,
    pub operand_types: Vec<String>,
    pub result_type: String,
    pub null_behaviour: String,
    pub ordering_effects: Vec<String>,
    pub version_witness: serde_json::Value,
    pub admission: Admission,
    pub oracle: String,
    pub counterexample: Option<String>,
    pub smallest_test: String,
    pub verdict: String,
    pub verdict_detail: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Metrics {
    pub constructs_harvested: usize,
    pub scope_declarations: usize,
    pub default_scopes: usize,
    pub default_scopes_order_only: usize,
    pub call_sites: usize,
    pub call_sites_class_unknown: usize,
    /// Rows whose class is an OGAR class and whose body parsed.
    pub recognized: usize,
    pub alias: usize,
    pub compose: usize,
    pub learned: usize,
    pub residual: usize,
    /// Refused for F000 because the body needs general Ruby / Rails runtime
    /// semantics (non-literal, join, lambda parameter, NOT semantics, filtering
    /// default scope, effect): these stay native Rust (KEEP_RUST). Distinct
    /// from an explicit host boundary, which no scope row uses.
    pub keep_rust: usize,
    /// Refused or deferred on a named semantic difference (nullable, code
    /// space, type, literal, unknown field).
    pub refuse_unresolved: usize,
    pub behaviourally_verified: usize,
    pub prepared: usize,
}

#[derive(Debug, Serialize)]
pub struct Matrix {
    pub app: String,
    pub activerecord: String,
    pub load_defaults: Option<String>,
    pub ruff_schema_gap_filled: BTreeMap<String, Vec<String>>,
    pub ogar_classes: usize,
    /// Per class: scope names ruff lists but the AST catalog does not, and the reverse.
    pub catalog_vs_ruff: Vec<(String, Vec<String>, Vec<String>)>,
    pub metrics: Metrics,
    pub rows: Vec<Row>,
}

struct Job {
    reg_class: String,
    receiver_owner: Option<(String, String)>,
    scope: String,
    terminal: Terminal,
}

fn activerecord_version(src: &Path) -> String {
    let lock = std::fs::read_to_string(src.join("Gemfile.lock")).unwrap_or_default();
    Regex::new(r"(?m)^\s{4}activerecord \(([0-9][^)]*)\)")
        .ok()
        .and_then(|r| r.captures(&lock).map(|c| c[1].to_string()))
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn identity(basis: &OgarBasis, class: &str, used: &[usize]) -> Option<OgarIdentity> {
    let c = basis.class(class)?;
    let fields = used
        .iter()
        .filter_map(|&p| {
            let f = c.fields.get(p)?;
            let (type_name, required) = match c.meta.get(p)? {
                FieldMeta::Column { type_name, required, .. } => (type_name.clone(), *required),
                FieldMeta::AssocFk { required, .. } => ("fk".to_string(), *required),
                FieldMeta::Output { .. } => return None,
            };
            Some(FieldIdentity { name: f.label.clone(), predicate_iri: f.predicate_iri.clone(), type_name, required })
        })
        .collect();
    Some(OgarIdentity { class: class.to_string(), lab_classid: Some(format!("{:#010x}", c.classid())), fields })
}

fn fingerprint(basis: &OgarBasis, class: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    if let Some(c) = basis.class(class) {
        for f in &c.fields {
            for b in f.predicate_iri.bytes().chain([0]) {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    format!("{h:016x}")
}

/// What the oracle traces answer for a scope, if anything.
fn oracle_for(app: &str, class: &str, scope: &str, terminal: Terminal, via_assoc: bool) -> Option<(&'static str, &'static str)> {
    match (app, class, scope, terminal, via_assoc) {
        ("zammad", "Ticket::State", "active", Terminal::Count, false) => {
            Some(("oracle/f000-trace.json answers.state_active_count", "differential_zammad::state_active_count_equals_activerecord"))
        }
        ("zammad", "Ticket::State", "active", Terminal::Exists, false) => {
            Some(("oracle/f000-trace.json answers.state_active_exists", "differential_zammad::state_active_exists_equals_activerecord"))
        }
        ("zammad", "Checklist::Item", "incomplete", Terminal::Count, true) => {
            Some(("oracle/f000-trace.json answers.checklist_incomplete", "differential_zammad::checklist_incomplete_equals_activerecord"))
        }
        ("openproject", "Project", "active", Terminal::Count, false) => Some((
            "oracle/openproject-f000-trace.json answers.project_active_count",
            "differential_openproject::project_active_count_equals_activerecord",
        )),
        ("openproject", "Project", "active", Terminal::Exists, false) => Some((
            "oracle/openproject-f000-trace.json answers.project_active_exists",
            "differential_openproject::project_active_exists_equals_activerecord",
        )),
        _ => None,
    }
}

fn counterexample(reason: &RefuseReason) -> Option<String> {
    Some(match reason {
        RefuseReason::NullableComparison { .. } => {
            "SQL `col = v` / `col != v` drops NULL rows; IntNot(IntEqual) over a lane without validity keeps them \
             (negative_oracles::c2_where_not_on_a_nullable_column_is_guarded_and_equals_activerecord)"
                .to_string()
        }
        RefuseReason::CodeSpaceRequired { .. } => {
            "a string literal has no lane code until a CodeTable witness exists (#1982); comparing a hash or an index would be a different relation"
                .to_string()
        }
        RefuseReason::UnknownField { .. } => {
            "a label shared with another class is not this class's field (negative_oracles::a1_wrong_field_identity_is_refused_at_prepare)".to_string()
        }
        _ => return None,
    })
}

fn smallest_test(reason: Option<&RefuseReason>, verdict: &Verdict) -> String {
    match (reason, verdict) {
        (Some(RefuseReason::NullableComparison { .. }), _) => "transpose::tests".to_string(),
        (Some(RefuseReason::CodeSpaceRequired { .. }), _) => "negative_oracles::c3_string_literal_needs_a_code_table".to_string(),
        (Some(RefuseReason::UnknownField { .. }), _) => "negative_oracles::a3_a_column_that_is_not_an_ogar_attribute_is_unknown_field".to_string(),
        (Some(_), _) => "transpose::tests".to_string(),
        (None, Verdict::Compose | Verdict::Alias) => "transpose::tests::one_predicate_and_a_terminal_compose_two_words".to_string(),
        _ => "transpose::tests".to_string(),
    }
}

fn operand_types(t: &Transposed, basis: &OgarBasis) -> Vec<String> {
    t.predicates
        .iter()
        .map(|(p, _, _, _)| match basis.class(&t.class).and_then(|c| c.meta.get(*p)) {
            Some(FieldMeta::Column { type_name, .. }) => format!("U32 lane (column {type_name})"),
            Some(FieldMeta::AssocFk { .. }) => "U32 lane (foreign key)".to_string(),
            _ => "UNKNOWN".to_string(),
        })
        .chain(t.null_tests.iter().map(|(p, _, _)| match basis.class(&t.class).and_then(|c| c.meta.get(*p)) {
            Some(FieldMeta::Column { type_name, .. }) => format!("presence plane (column {type_name})"),
            Some(FieldMeta::AssocFk { .. }) => "presence plane (foreign key)".to_string(),
            _ => "UNKNOWN".to_string(),
        }))
        .collect()
}

fn null_behaviour(t: &Transposed) -> String {
    if !t.null_tests.is_empty() && t.reason.is_none() {
        "NULL-able columns read their presence plane: IS NULL is presence 0, and `=` / `!=` are ANDed with presence as SQL drops NULL rows (decision B); two-valued".to_string()
    } else if t.predicates.is_empty() && t.reason.is_none() {
        "no predicate: no column is compared".to_string()
    } else if let Some(RefuseReason::NullableComparison { field }) = &t.reason {
        format!("{field} is the NULL-able implicit foreign key of an association scope; not guarded by its presence yet; refused")
    } else if t.reason.is_some() {
        "not reached".to_string()
    } else {
        "every compared column is required (NOT NULL by OGAR); two-valued".to_string()
    }
}

/// Scan `.rb` files below `dirs` for `X.scope.count|exists?|any?`.
fn call_sites(src: &Path, dirs: &[&str], names: &BTreeSet<String>) -> Vec<(String, usize, String, String, Terminal, String)> {
    if names.is_empty() {
        return vec![];
    }
    let alt = names.iter().map(|n| regex::escape(n)).collect::<Vec<_>>().join("|");
    let Ok(re) = Regex::new(&format!(r"\b([A-Z][A-Za-z0-9_:]*|[a-z_][a-z0-9_]*)\.({alt})\.(count|exists\?|any\?)")) else {
        return vec![];
    };
    let mut files = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut es: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        es.sort();
        for p in es {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rb") {
                out.push(p);
            }
        }
    }
    for d in dirs {
        walk(&src.join(d), &mut files);
    }
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let rel = f.strip_prefix(src).unwrap_or(&f).to_string_lossy().into_owned();
        for (i, line) in text.lines().enumerate() {
            for c in re.captures_iter(line) {
                let terminal = if &c[3] == "count" { Terminal::Count } else { Terminal::Exists };
                out.push((rel.clone(), i + 1, c[1].to_string(), c[2].to_string(), terminal, line.trim().to_string()));
            }
        }
    }
    out
}

/// Build the matrix of an application.
#[allow(clippy::too_many_lines)]
pub fn build(cfg: &Loaded) -> Result<Matrix, String> {
    let app = &cfg.config.app;
    let h = Harvest::collect(&cfg.source, &app.autoload_roots, &app.namespace);
    let cat = ScopeCatalog::from_harvest(&h);
    let lifted = rails_compiler::ogar_classes_all(cfg)?;
    let ruff_listed: BTreeMap<String, BTreeSet<String>> = lifted
        .classes
        .iter()
        .map(|c| (c.name.clone(), c.scopes.iter().map(|s| s.name.clone()).collect()))
        .collect();
    let input: Vec<(String, ogar_vocab::Class, u16)> = lifted
        .classes
        .into_iter()
        .zip(CONCEPT_BASE..)
        .map(|(c, concept)| (app.namespace.clone(), c, concept))
        .collect();
    let basis = OgarBasis::new(input).with_column_not_null(lifted.column_not_null).with_unharvested(lifted.unharvested_tables);
    let ar = activerecord_version(&cfg.source);
    let rails_version = format!(
        "activerecord {ar}, load_defaults {}",
        app.rails_load_defaults.as_deref().unwrap_or("UNKNOWN")
    );

    let mut catalog_vs_ruff = Vec::new();
    for (class, ruff) in &ruff_listed {
        let ours: BTreeSet<String> = cat.scopes.iter().filter(|s| !s.default && &s.class == class).map(|s| s.name.clone()).collect();
        let only_ruff: Vec<String> = ruff.difference(&ours).cloned().collect();
        let only_ours: Vec<String> = ours.difference(&ruff.clone()).cloned().collect();
        if !only_ruff.is_empty() || !only_ours.is_empty() {
            catalog_vs_ruff.push((class.clone(), only_ruff, only_ours));
        }
    }

    // Call sites: a constant receiver names its class; a lowercase receiver is
    // resolved only through an association of a class declared in the same file.
    let names: BTreeSet<String> = cat.scopes.iter().filter(|s| !s.default).map(|s| s.name.clone()).collect();
    let file_classes: BTreeMap<String, Vec<String>> = {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, us) in &h.units {
            for u in us {
                if let Some(f) = h.file(&u.file) {
                    m.entry(f.rel.clone()).or_default().push(name.clone());
                }
            }
        }
        m
    };
    let sites = call_sites(&cfg.source, &["app", "lib"], &names);

    // Jobs: every declaration (count), every call site's (class, scope, terminal).
    struct Site {
        rel: String,
        line: usize,
        text: String,
        class: Option<String>,
        owner: Option<(String, String)>,
        scope: String,
        terminal: Terminal,
    }
    let mut site_rows: Vec<Site> = Vec::new();
    for (rel, line, recv, scope, terminal, text) in sites {
        let mut class = None;
        let mut owner = None;
        if recv.chars().next().is_some_and(char::is_uppercase) {
            let nesting = file_classes.get(&rel).cloned().unwrap_or_default();
            class = h.resolve(&recv, &nesting).filter(|c| cat.scope(c, &scope).is_some());
        } else if let Some(units) = file_classes.get(&rel) {
            for u in units {
                if let Some(target) = basis.association_target(u, &recv)
                    && cat.scope(&target, &scope).is_some()
                {
                    class = Some(target);
                    owner = Some((u.clone(), recv.clone()));
                    break;
                }
            }
        }
        site_rows.push(Site { rel, line, text, class, owner, scope, terminal });
    }

    let mut jobs: Vec<Job> = Vec::new();
    let mut seen: BTreeSet<(String, String, String, Terminal)> = BTreeSet::new();
    let mut push_job = |reg_class: &str, owner: Option<(String, String)>, scope: &str, terminal: Terminal| {
        let key = (
            reg_class.to_string(),
            owner.as_ref().map_or(String::new(), |(o, a)| format!("{o}#{a}")),
            scope.to_string(),
            terminal,
        );
        if seen.insert(key) {
            jobs.push(Job { reg_class: reg_class.to_string(), receiver_owner: owner, scope: scope.to_string(), terminal });
        }
    };
    for s in cat.scopes.iter().filter(|s| !s.default) {
        push_job(&s.class, None, &s.name, Terminal::Count);
    }
    for s in &site_rows {
        if let Some(c) = &s.class {
            push_job(c, s.owner.clone(), &s.scope, s.terminal);
        }
    }

    // Transpose and admit, class by class.
    struct Outcome {
        t: Transposed,
        admission: Admission,
        fingerprint: String,
        identity: Option<OgarIdentity>,
        operands: Vec<String>,
    }
    let mut outcomes: BTreeMap<(String, String, String, Terminal), Outcome> = BTreeMap::new();
    let mut by_class: BTreeMap<String, Vec<&Job>> = BTreeMap::new();
    for j in &jobs {
        by_class.entry(j.reg_class.clone()).or_default().push(j);
    }
    for (class, js) in &by_class {
        let key_of = |j: &Job| {
            (
                j.reg_class.clone(),
                j.receiver_owner.as_ref().map_or(String::new(), |(o, a)| format!("{o}#{a}")),
                j.scope.clone(),
                j.terminal,
            )
        };
        if basis.class(class).is_none() {
            continue;
        }
        let mut names_in: Vec<&str> = vec![class.as_str()];
        for j in js {
            if let Some((o, _)) = &j.receiver_owner {
                names_in.push(o.as_str());
            }
        }
        let mut mini = basis.subset(&names_in);
        let mut done: Vec<(&Job, Transposed)> = Vec::new();
        for j in js {
            let receiver = match &j.receiver_owner {
                Some((o, a)) => Receiver::Assoc { owner: o, assoc: a, owner_id: 0 },
                None => Receiver::Class(class),
            };
            let t = transpose(&mut mini, &cat, &Request { receiver, scope: Some(&j.scope), terminal: j.terminal });
            done.push((j, t));
        }
        // Registration is per class and once: only the admitted plans.
        let mut bindings = Vec::new();
        let mut ids: BTreeMap<usize, u32> = BTreeMap::new();
        for (i, (_, t)) in done.iter().enumerate() {
            let id = u32::try_from(bindings.len() + 1).unwrap_or(u32::MAX);
            if let Some(b) = t.binding(id) {
                ids.insert(i, id);
                bindings.push(b);
            }
        }
        let mut handles = BTreeMap::new();
        let mut register_err = None;
        if !bindings.is_empty() {
            match Engine::new(&mini).and_then(|mut e| e.register(class, bindings).map(|h| (e, h))) {
                Ok((e, hs)) => {
                    for h in hs {
                        handles.insert(h.id, h);
                    }
                    // keep the engine for compilation below
                    let engine = e;
                    for (i, (j, t)) in done.iter().enumerate() {
                        let admission = match ids.get(&i).and_then(|id| handles.get(id)) {
                            None => Admission {
                                status: "refused".into(),
                                refusal: t.reason.as_ref().map(|r| format!("{r:?}")).or_else(|| Some(format!("{:?}", t.verdict))),
                            },
                            Some(h) => {
                                let zeros = vec![0u32; 1];
                                let lanes: Vec<lance_graph_mask_risc::LaneRef<'_>> =
                                    t.reads.iter().map(|_| lance_graph_mask_risc::LaneRef::U32(&zeros)).collect();
                                let fcount = mini.class(class).map_or(0, |c| c.fields.len());
                                let bound: Vec<(usize, u16)> =
                                    t.reads.iter().enumerate().map(|(k, p)| (usize::from(*p), u16::try_from(k).unwrap_or(0))).collect();
                                let fl = field_lanes(fcount, &bound);
                                let empty = [0u64; 1];
                                let masks: Vec<&[u64]> = t.null_tests.iter().map(|_| &empty[..]).collect();
                                let present: Vec<(usize, u16)> = t
                                    .null_tests
                                    .iter()
                                    .enumerate()
                                    .map(|(k, (p, _, _))| (*p, u16::try_from(k).unwrap_or(0)))
                                    .collect();
                                let fp = field_lanes(fcount, &present);
                                match engine.compile_present(h, 1, &lanes, &masks, &fl, &fp) {
                                    Ok(_) => Admission { status: "prepared".into(), refusal: None },
                                    Err(e) => Admission { status: "registered".into(), refusal: Some(format!("{e:?}")) },
                                }
                            }
                        };
                        let used: Vec<usize> = t.predicates.iter().map(|p| p.0).chain(t.null_tests.iter().map(|p| p.0)).collect();
                        outcomes.insert(
                            key_of(j),
                            Outcome {
                                admission,
                                fingerprint: fingerprint(&mini, class),
                                identity: identity(&mini, class, &used),
                                operands: operand_types(t, &mini),
                                t: t.clone(),
                            },
                        );
                    }
                    continue;
                }
                Err(e) => register_err = Some(format!("{e:?}")),
            }
        }
        for (j, t) in done {
            let used: Vec<usize> = t.predicates.iter().map(|p| p.0).chain(t.null_tests.iter().map(|p| p.0)).collect();
            let admission = if t.verdict.admits() {
                Admission { status: "refused".into(), refusal: register_err.clone() }
            } else {
                Admission {
                    status: "refused".into(),
                    refusal: t.reason.as_ref().map(|r| format!("{r:?}")).or_else(|| Some(format!("{:?}", t.verdict))),
                }
            };
            outcomes.insert(
                key_of(j),
                Outcome {
                    admission,
                    fingerprint: fingerprint(&mini, class),
                    identity: identity(&mini, class, &used),
                    operands: operand_types(&t, &mini),
                    t,
                },
            );
        }
    }

    // Rows.
    let mut metrics = Metrics::default();
    let mut rows = Vec::new();
    let tally = |m: &mut Metrics, v: &Verdict| match v {
        Verdict::Alias => m.alias += 1,
        Verdict::Compose => m.compose += 1,
        Verdict::Learned => m.learned += 1,
        Verdict::Residual(_) => m.residual += 1,
        Verdict::Refuse(RefuseReason::NonLiteral { .. } | RefuseReason::Join { .. } | RefuseReason::NotSemantics { .. } | RefuseReason::DefaultScopeFilters { .. } | RefuseReason::EffectBoundary { .. }) => {
            m.keep_rust += 1;
        }
        Verdict::Refuse(_) | Verdict::Defer(_) => m.refuse_unresolved += 1,
    };
    let make_row = |id: String,
                    kind: &str,
                    ruby: String,
                    location: String,
                    decl_kind: &str,
                    anchor: Option<usize>,
                    class: &str,
                    scope: &str,
                    terminal: Terminal,
                    via: bool,
                    o: Option<&Outcome>|
     -> Row {
        let listed = ruff_listed.get(class).map(|s| s.contains(scope));
        let (oracle, test) = oracle_for(&app.name, class, scope, terminal, via).map_or(("none".to_string(), String::new()), |(a, b)| (a.to_string(), b.to_string()));
        match o {
            Some(o) => {
                let t = &o.t;
                Row {
                    id,
                    kind: kind.to_string(),
                    ruby,
                    location,
                    rails_version: rails_version.clone(),
                    ruff_evidence: RuffEvidence { decl_kind: decl_kind.to_string(), anchor_byte: anchor, listed_by_ruff: listed },
                    ogar_identity: o.identity.as_ref().map(|i| OgarIdentity {
                        class: i.class.clone(),
                        lab_classid: i.lab_classid.clone(),
                        fields: i
                            .fields
                            .iter()
                            .map(|f| FieldIdentity {
                                name: f.name.clone(),
                                predicate_iri: f.predicate_iri.clone(),
                                type_name: f.type_name.clone(),
                                required: f.required,
                            })
                            .collect(),
                    }),
                    candidate_words: if t.words.is_empty() { vec![t.terminal.word().to_string()] } else { t.words.clone() },
                    operand_types: o.operands.clone(),
                    result_type: match t.terminal {
                        Terminal::Count => "i64 scalar (row count)".into(),
                        Terminal::Exists => "i64 scalar (1 or 0)".into(),
                    },
                    null_behaviour: null_behaviour(t),
                    ordering_effects: {
                        let mut e = t.evidence.clone();
                        if via {
                            e.push("owner id is a runtime parameter at the call site: one plan per literal id; the matrix binds the placeholder 0".to_string());
                        }
                        e
                    },
                    version_witness: serde_json::json!({
                        "schema_rev": 1,
                        "basis_digest": "UNKNOWN (BasisDigest is crate-private in lance-graph-dt64; registered state is the proof)",
                        "basis_fingerprint_fnv64_of_predicate_iris": o.fingerprint,
                    }),
                    admission: Admission { status: o.admission.status.clone(), refusal: o.admission.refusal.clone() },
                    oracle,
                    counterexample: t.reason.as_ref().and_then(counterexample),
                    smallest_test: if test.is_empty() { smallest_test(t.reason.as_ref(), &t.verdict) } else { test },
                    verdict: t.verdict.label().to_string(),
                    verdict_detail: match &t.verdict {
                        Verdict::Refuse(r) => format!("{r:?}"),
                        Verdict::Defer(d) | Verdict::Residual(d) => d.clone(),
                        _ => format!("{} word(s): {}", t.words.len(), t.words.join(" -> ")),
                    },
                }
            }
            None => Row {
                id,
                kind: kind.to_string(),
                ruby,
                location,
                rails_version: rails_version.clone(),
                ruff_evidence: RuffEvidence { decl_kind: decl_kind.to_string(), anchor_byte: anchor, listed_by_ruff: listed },
                ogar_identity: None,
                candidate_words: vec![],
                operand_types: vec![],
                result_type: "UNKNOWN".into(),
                null_behaviour: "UNKNOWN".into(),
                ordering_effects: vec![],
                version_witness: serde_json::json!({"schema_rev": "UNKNOWN", "basis_digest": "UNKNOWN"}),
                admission: Admission { status: "not_attempted".into(), refusal: Some(format!("{class} is not an OGAR class of this application (a module/concern or not a model)")) },
                oracle,
                counterexample: None,
                smallest_test: "none".into(),
                verdict: "Residual".into(),
                verdict_detail: format!("UNKNOWN: {class} has no OGAR class"),
            },
        }
    };

    for s in &cat.scopes {
        metrics.constructs_harvested += 1;
        if s.default {
            metrics.default_scopes += 1;
            let mut acc_cat = ScopeCatalog::default();
            acc_cat.scopes.push(s.clone());
            let order_only = {
                let mut b = basis.subset(&[&s.class]);
                let t = transpose(&mut b, &acc_cat, &Request { receiver: Receiver::Class(&s.class), scope: None, terminal: Terminal::Count });
                !matches!(t.verdict, Verdict::Refuse(RefuseReason::DefaultScopeFilters { .. }))
            };
            if order_only {
                metrics.default_scopes_order_only += 1;
            }
            rows.push(Row {
                id: format!("{}.default_scope", s.class),
                kind: "default_scope".into(),
                ruby: s.ruby.clone(),
                location: s.provenance.clone(),
                rails_version: rails_version.clone(),
                ruff_evidence: RuffEvidence { decl_kind: "ScopeKind::DefaultScope".into(), anchor_byte: Some(s.anchor), listed_by_ruff: None },
                ogar_identity: None,
                candidate_words: vec![],
                operand_types: vec![],
                result_type: "n/a".into(),
                null_behaviour: "n/a".into(),
                ordering_effects: vec![if order_only { "order-only: accepted as evidence".into() } else { "filters rows: every scope of the class is refused".into() }],
                version_witness: serde_json::json!({"schema_rev": 1, "basis_digest": "UNKNOWN"}),
                admission: Admission { status: "not_attempted".into(), refusal: None },
                oracle: "none".into(),
                counterexample: None,
                smallest_test: "transpose::tests::a_default_scope_with_a_where_refuses_and_an_order_one_passes".into(),
                verdict: if order_only { "accepted-order-only" } else { "Refuse" }.into(),
                verdict_detail: String::new(),
            });
            continue;
        }
        metrics.scope_declarations += 1;
        let key = (s.class.clone(), String::new(), s.name.clone(), Terminal::Count);
        let o = outcomes.get(&key);
        if let Some(o) = o {
            metrics.recognized += usize::from(s.body.is_some());
            tally(&mut metrics, &o.t.verdict);
            if o.admission.status == "prepared" {
                metrics.prepared += 1;
            }
        } else {
            metrics.residual += 1;
        }
        let r = make_row(
            format!("{}.{}.count", s.class, s.name),
            "declaration",
            s.ruby.clone(),
            s.provenance.clone(),
            "ScopeKind::Scope",
            Some(s.anchor),
            &s.class,
            &s.name,
            Terminal::Count,
            false,
            o,
        );
        if r.oracle != "none" {
            metrics.behaviourally_verified += 1;
        }
        rows.push(r);
    }
    for s in &site_rows {
        metrics.constructs_harvested += 1;
        metrics.call_sites += 1;
        let Some(class) = &s.class else {
            metrics.call_sites_class_unknown += 1;
            metrics.residual += 1;
            rows.push(Row {
                id: format!("{}:{}", s.rel, s.line),
                kind: "call_site".into(),
                ruby: s.text.clone(),
                location: format!("{}:{}", s.rel, s.line),
                rails_version: rails_version.clone(),
                ruff_evidence: RuffEvidence { decl_kind: "grep-equivalent call site".into(), anchor_byte: None, listed_by_ruff: None },
                ogar_identity: None,
                candidate_words: vec![],
                operand_types: vec![],
                result_type: "UNKNOWN".into(),
                null_behaviour: "UNKNOWN".into(),
                ordering_effects: vec![],
                version_witness: serde_json::json!({"schema_rev": "UNKNOWN", "basis_digest": "UNKNOWN"}),
                admission: Admission { status: "not_attempted".into(), refusal: Some("UNKNOWN: receiver class not resolved".into()) },
                oracle: "none".into(),
                counterexample: None,
                smallest_test: "none".into(),
                verdict: "Residual".into(),
                verdict_detail: format!("UNKNOWN: receiver of `.{}` is not a resolvable class or association", s.scope),
            });
            continue;
        };
        let key = (
            class.clone(),
            s.owner.as_ref().map_or(String::new(), |(o, a)| format!("{o}#{a}")),
            s.scope.clone(),
            s.terminal,
        );
        let o = outcomes.get(&key);
        if let Some(o) = o {
            metrics.recognized += 1;
            tally(&mut metrics, &o.t.verdict);
            if o.admission.status == "prepared" {
                metrics.prepared += 1;
            }
        } else {
            metrics.residual += 1;
        }
        let r = make_row(
            format!("{}:{}", s.rel, s.line),
            "call_site",
            s.text.clone(),
            format!("{}:{}", s.rel, s.line),
            "grep-equivalent call site",
            None,
            class,
            &s.scope,
            s.terminal,
            s.owner.is_some(),
            o,
        );
        if r.oracle != "none" {
            metrics.behaviourally_verified += 1;
        }
        rows.push(r);
    }
    Ok(Matrix {
        app: app.name.clone(),
        activerecord: ar,
        load_defaults: app.rails_load_defaults.clone(),
        ruff_schema_gap_filled: lifted.ruff_schema_gap_filled,
        ogar_classes: basis.classes().len(),
        catalog_vs_ruff,
        metrics,
        rows,
    })
}
