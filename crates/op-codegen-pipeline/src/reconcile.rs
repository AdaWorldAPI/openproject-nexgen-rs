//! Three-witness reconciliation: a structural fact harvested from the source
//! application (the "Ruby ORE"), the compiled Rust as Charon extracted it (the
//! "Charon ORE"), and an independent runtime axis.
//!
//! A verdict is made per semantic claim, never per method: one method can carry a
//! registration, an admission decision, a guard evaluation, a callback invocation and
//! a write, and each is its own [`ClaimSpec`] with its own [`Verdict`].
//!
//! The module is application-agnostic. A caller supplies the claims (join key, Ruby
//! evidence, the Rust probe) as data; this module reads a Charon store directory and
//! decides. What the module will not do is read the absence of Charon evidence as the
//! absence of Rust behaviour: a missing row means something only when the crate is in
//! the store, the function is defined there, its body was structured, its source file
//! is unchanged since extraction, and the behaviour is not held as data. Every other
//! case is a typed [`SlagReason`].
//!
//! The runtime axis ([`RuntimeEvidence`]) is attached after the structural verdict and
//! never changes it, in either direction.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

// ─────────────────────────────────────────────────────────────────────────
// The Charon store (TSV, `#version` / `#schema` header)
// ─────────────────────────────────────────────────────────────────────────

/// One row of `<crate>.rows.tsv` (version 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreRow {
    pub predicate: String,
    pub subject: String,
    pub object: String,
    pub grade: String,
    /// `None` when Charon recorded no location (`-`), e.g. for synthesized closure shims.
    pub file: Option<String>,
    pub start: Option<usize>,
    pub end: Option<usize>,
}

/// One row of `<crate>.effects.tsv` (version 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRow {
    pub subject: String,
    pub effect: String,
    pub kind: String,
    pub file: String,
    pub start: usize,
    pub end: usize,
}

/// The four artifacts Charon wrote for one crate.
#[derive(Debug, Clone, Default)]
pub struct CrateStore {
    pub rows: Vec<StoreRow>,
    /// `(reason, subject)`.
    pub residuals: Vec<(String, String)>,
    /// Source path -> FNV-1a 64 of its bytes at extraction time.
    pub files: BTreeMap<String, u64>,
    pub effects: Vec<EffectRow>,
}

/// A Charon store directory: `<crate>.{rows,residuals,files,effects}.tsv` per crate,
/// plus an optional `PROVENANCE.txt` (`key: value` lines) next to it or one level up.
#[derive(Debug, Clone, Default)]
pub struct CharonStore {
    pub crates: BTreeMap<String, CrateStore>,
    pub provenance: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum StoreError {
    Io(PathBuf, std::io::Error),
    /// A `#version` this reader does not know. Refused, never guessed.
    UnknownVersion {
        file: PathBuf,
        found: String,
    },
    BadSchema {
        file: PathBuf,
        found: String,
    },
    Malformed {
        file: PathBuf,
        line: usize,
    },
    /// A crate has some artifacts but not all four.
    MissingPart {
        krate: String,
        part: &'static str,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(p, e) => write!(f, "{}: {e}", p.display()),
            StoreError::UnknownVersion { file, found } => {
                write!(f, "{}: unknown version {found:?}", file.display())
            }
            StoreError::BadSchema { file, found } => {
                write!(f, "{}: unexpected schema {found:?}", file.display())
            }
            StoreError::Malformed { file, line } => {
                write!(f, "{}:{line}: malformed row", file.display())
            }
            StoreError::MissingPart { krate, part } => {
                write!(f, "crate {krate}: no {part}.tsv")
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// `(part, version, schema columns)` this reader accepts.
const PARTS: &[(&str, &str, &str)] = &[
    (
        "rows",
        "2",
        "predicate\tsubject\tobject\tgrade\tfile\tstart\tend",
    ),
    ("residuals", "1", "reason\tsubject"),
    ("files", "1", "path\tfnv64"),
    ("effects", "1", "subject\teffect\tkind\tfile\tstart\tend"),
];

/// The data lines of one artifact after its header is checked.
fn read_part(path: &Path, version: &str, schema: &str) -> Result<Vec<Vec<String>>, StoreError> {
    let text = std::fs::read_to_string(path).map_err(|e| StoreError::Io(path.into(), e))?;
    let mut lines = text.lines();
    let v = lines.next().unwrap_or_default();
    if v.strip_prefix("#version ") != Some(version) {
        return Err(StoreError::UnknownVersion {
            file: path.into(),
            found: v.to_string(),
        });
    }
    let s = lines.next().unwrap_or_default();
    if s.strip_prefix("#schema ") != Some(schema) {
        return Err(StoreError::BadSchema {
            file: path.into(),
            found: s.to_string(),
        });
    }
    let width = schema.split('\t').count();
    let mut out = Vec::new();
    for (i, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let cols: Vec<String> = line.split('\t').map(str::to_string).collect();
        if cols.len() != width {
            return Err(StoreError::Malformed {
                file: path.into(),
                line: i + 3,
            });
        }
        out.push(cols);
    }
    Ok(out)
}

/// `-` is "no location".
fn opt_num(path: &Path, line: usize, s: &str) -> Result<Option<usize>, StoreError> {
    if s == "-" {
        Ok(None)
    } else {
        num(path, line, s).map(Some)
    }
}

fn num(path: &Path, line: usize, s: &str) -> Result<usize, StoreError> {
    s.parse().map_err(|_| StoreError::Malformed {
        file: path.into(),
        line,
    })
}

impl CharonStore {
    /// Read every crate in `dir`. `.bin` and other files are ignored.
    pub fn load(dir: &Path) -> Result<Self, StoreError> {
        let mut names: BTreeSet<String> = BTreeSet::new();
        let entries = std::fs::read_dir(dir).map_err(|e| StoreError::Io(dir.into(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| StoreError::Io(dir.into(), e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            for (part, _, _) in PARTS {
                if let Some(k) = name.strip_suffix(&format!(".{part}.tsv")) {
                    names.insert(k.to_string());
                }
            }
        }
        let mut store = CharonStore::default();
        for krate in names {
            let mut c = CrateStore::default();
            for (part, version, schema) in PARTS {
                let path = dir.join(format!("{krate}.{part}.tsv"));
                if !path.exists() {
                    return Err(StoreError::MissingPart {
                        krate: krate.clone(),
                        part,
                    });
                }
                let rows = read_part(&path, version, schema)?;
                for (i, r) in rows.into_iter().enumerate() {
                    let line = i + 3;
                    match *part {
                        "rows" => c.rows.push(StoreRow {
                            predicate: r[0].clone(),
                            subject: r[1].clone(),
                            object: r[2].clone(),
                            grade: r[3].clone(),
                            file: (r[4] != "-").then(|| r[4].clone()),
                            start: opt_num(&path, line, &r[5])?,
                            end: opt_num(&path, line, &r[6])?,
                        }),
                        "residuals" => c.residuals.push((r[0].clone(), r[1].clone())),
                        "files" => {
                            let h = u64::from_str_radix(&r[1], 16).map_err(|_| {
                                StoreError::Malformed {
                                    file: path.clone(),
                                    line,
                                }
                            })?;
                            c.files.insert(r[0].clone(), h);
                        }
                        _ => c.effects.push(EffectRow {
                            subject: r[0].clone(),
                            effect: r[1].clone(),
                            kind: r[2].clone(),
                            file: r[3].clone(),
                            start: num(&path, line, &r[4])?,
                            end: num(&path, line, &r[5])?,
                        }),
                    }
                }
            }
            store.crates.insert(krate, c);
        }
        for p in [dir.join("PROVENANCE.txt"), dir.join("../PROVENANCE.txt")] {
            if let Ok(text) = std::fs::read_to_string(&p) {
                for line in text.lines() {
                    if let Some((k, v)) = line.split_once(": ") {
                        store
                            .provenance
                            .insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
                break;
            }
        }
        Ok(store)
    }

    fn krate(&self, symbol: &str) -> Option<&CrateStore> {
        self.crates.get(crate_of(symbol))
    }

    /// The `DEFINES` row of `function` (normalized comparison).
    pub fn defines(&self, function: &str) -> Option<&StoreRow> {
        let want = normalize(function);
        self.krate(function)?
            .rows
            .iter()
            .find(|r| r.predicate == "DEFINES" && normalize(&r.subject) == want)
    }

    /// The residual reason Charon recorded for `function`, if any.
    pub fn residual(&self, function: &str) -> Option<&str> {
        let want = normalize(function);
        self.krate(function)?
            .residuals
            .iter()
            .find(|(_, s)| normalize(s) == want)
            .map(|(r, _)| r.as_str())
    }

    /// A call edge `caller -> callee` (`CALLS_RESOLVED` or `MAY_DISPATCH`).
    pub fn edge(&self, caller: &str, callee: &str) -> Option<&StoreRow> {
        let (c, d) = (normalize(caller), normalize(callee));
        self.krate(caller)?.rows.iter().find(|r| {
            (r.predicate == "CALLS_RESOLVED" || r.predicate == "MAY_DISPATCH")
                && normalize(&r.subject) == c
                && normalize(&r.object) == d
        })
    }

    /// Source files whose bytes under `repo_root` differ from the hash recorded at
    /// extraction (or that no longer exist). Byte offsets into them are stale.
    pub fn stale_files(&self, repo_root: &Path) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for c in self.crates.values() {
            for (path, want) in &c.files {
                match std::fs::read(repo_root.join(path)) {
                    Ok(bytes) if fnv1a64(&bytes) == *want => {}
                    _ => {
                        out.insert(path.clone());
                    }
                }
            }
        }
        out
    }
}

/// FNV-1a, 64 bit: the hash `files.tsv` records.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The crate segment of a Rust path (`op_services::a::b` -> `op_services`).
pub fn crate_of(symbol: &str) -> &str {
    symbol.split("::").next().unwrap_or(symbol)
}

/// A Charon path without generic arguments, trait clauses, impl braces and a
/// `MAY_DISPATCH` " via ..." suffix:
/// `m::{Svc<'a, U>[TraitClause0]}::admit` -> `m::Svc::admit`.
pub fn normalize(symbol: &str) -> String {
    let symbol = symbol.split(" via ").next().unwrap_or(symbol);
    let mut out = String::with_capacity(symbol.len());
    let mut depth = 0usize;
    for ch in symbol.chars() {
        match ch {
            '<' | '[' => depth += 1,
            '>' | ']' => depth = depth.saturating_sub(1),
            '{' | '}' if depth == 0 => {}
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Claims
// ─────────────────────────────────────────────────────────────────────────

/// The canonical join key: `Application/Model/Operation/Phase/Method/Effect`.
/// Bare method names never join two witnesses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct JoinKey {
    pub application: String,
    pub model: String,
    pub operation: String,
    pub phase: String,
    pub method: String,
    pub effect: String,
}

impl fmt::Display for JoinKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}/{}/{}/{}",
            self.application, self.model, self.operation, self.phase, self.method, self.effect
        )
    }
}

/// The semantic level of a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Registration,
    Admission,
    GuardEvaluation,
    CallbackInvocation,
    Persistence,
}

/// A span in the source application, 1-based lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line_start == self.line_end {
            write!(f, "{}:{}", self.file, self.line_start)
        } else {
            write!(f, "{}:{}-{}", self.file, self.line_start, self.line_end)
        }
    }
}

/// What the source application's structural witness says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyEvidence {
    /// A fact an extractor produced. `span` locates it.
    Fact {
        extractor: String,
        span: SourceSpan,
        fact: String,
    },
    /// The witness inspected `span` and the fact is not there.
    Absent {
        extractor: String,
        span: SourceSpan,
        looked_for: String,
    },
    /// No extractor produced the fact; nothing is asserted.
    NotHarvested { why: String },
}

/// How a claim's Rust counterpart is looked up in the Charon store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustProbe {
    /// Call edges `(caller, callee)`; the fact is "every edge exists". A fact inside a
    /// function body is an edge out of that function.
    Edges {
        edges: Vec<(String, String)>,
        /// Edges whose presence contradicts the Ruby fact.
        conflicts: Vec<(String, String)>,
    },
    /// The behaviour is held as data (a generated table): `item` in `file` of `krate`.
    /// A call graph cannot see it, whatever the store holds.
    Data {
        krate: String,
        file: String,
        item: String,
    },
}

/// Whether the two witnesses speak about the same level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparability {
    Same,
    /// E.g. a DSL declaration against a direct call.
    Different {
        ruby: String,
        rust: String,
    },
}

/// One semantic claim to reconcile.
#[derive(Debug, Clone)]
pub struct ClaimSpec {
    pub key: JoinKey,
    pub level: Level,
    pub ruby: RubyEvidence,
    pub rust: RustProbe,
    pub comparability: Comparability,
    /// The extraction stage that would supply a missing fact if the row ends as slag.
    pub repair: String,
}

// ─────────────────────────────────────────────────────────────────────────
// Verdicts
// ─────────────────────────────────────────────────────────────────────────

/// Why a row cannot be decided. Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SlagReason {
    /// The crate holding the Rust function is not in the store.
    RustSideNotExtracted,
    /// The crate is in the store but the function has no `DEFINES` row.
    RustSymbolNotInStore,
    /// The function's source file changed since extraction; its offsets are stale.
    RustEvidenceStale,
    /// Charon listed the function in `residuals.tsv`: its body was not structured.
    RustBodyNotStructured,
    /// The behaviour lives in data (a generated table), invisible to a call graph.
    RepresentedAsData,
    /// No source-side extractor produced the fact.
    RubySideNotHarvested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Both witnesses establish the same comparable fact.
    Agreement,
    /// Both establish conflicting facts.
    Disagreement {
        detail: String,
    },
    /// Ruby fact; the Rust counterpart was inspected (extracted, defined, structured,
    /// current) and is absent.
    RubyOnly,
    /// Rust fact; the Ruby counterpart was inspected and is absent.
    CharonOnly,
    Slag(SlagReason),
    NotComparable {
        detail: String,
    },
}

impl Verdict {
    pub fn state(&self) -> &'static str {
        match self {
            Verdict::Agreement => "Agreement",
            Verdict::Disagreement { .. } => "Disagreement",
            Verdict::RubyOnly => "RubyOnly",
            Verdict::CharonOnly => "CharonOnly",
            Verdict::Slag(_) => "Slag",
            Verdict::NotComparable { .. } => "NotComparable",
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Slag(r) => write!(f, "Slag({r:?})"),
            Verdict::Disagreement { detail } => write!(f, "Disagreement ({detail})"),
            Verdict::NotComparable { detail } => write!(f, "NotComparable ({detail})"),
            v => f.write_str(v.state()),
        }
    }
}

/// The independent runtime axis. It never changes a structural verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvidence {
    /// Production code ran against a frozen oracle in this test run.
    Executed {
        by: String,
        compared: usize,
        mismatches: usize,
    },
    /// A gate elsewhere asserts this; it was not run here.
    Cited {
        gate: String,
        asserts: String,
    },
    NotExecuted {
        why: String,
    },
}

impl fmt::Display for RuntimeEvidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeEvidence::Executed {
                by,
                compared,
                mismatches,
            } => write!(
                f,
                "EXECUTED {compared} compared, {mismatches} mismatches ({by})"
            ),
            RuntimeEvidence::Cited { gate, asserts } => {
                write!(f, "CITED, not run here: {gate} asserts {asserts}")
            }
            RuntimeEvidence::NotExecuted { why } => write!(f, "none ({why})"),
        }
    }
}

/// A Rust source location: file plus byte range from the store, and the line range
/// when the file is current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustSpan {
    pub file: String,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub lines: Option<(usize, usize)>,
    /// The store row it came from (`PREDICATE subject -> object`) or the data item.
    pub what: String,
}

impl fmt::Display for RustSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.start, self.end) {
            (Some(a), Some(b)) => write!(f, "{} bytes {a}..{b}", self.file)?,
            _ => write!(f, "{} (no location)", self.file)?,
        }
        if let Some((a, b)) = self.lines {
            if a == b {
                write!(f, " (line {a})")?;
            } else {
                write!(f, " (lines {a}-{b})")?;
            }
        }
        Ok(())
    }
}

/// One reconciled claim.
#[derive(Debug, Clone)]
pub struct ReconciledRow {
    pub spec: ClaimSpec,
    pub verdict: Verdict,
    pub rust_span: Option<RustSpan>,
    /// What each witness contributed, in words.
    pub coverage: String,
    pub runtime: RuntimeEvidence,
}

/// Extractor revisions every row carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Revisions {
    pub source_extractor: String,
    pub charon: String,
    pub charon_extractor: String,
    pub target: String,
    pub source_app: String,
}

impl fmt::Display for Revisions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "source extractor {}; charon {}; charon extractor {}; target {}; source app {}",
            self.source_extractor, self.charon, self.charon_extractor, self.target, self.source_app
        )
    }
}

#[derive(Debug, Clone)]
pub struct Reconciliation {
    pub title: String,
    pub revisions: Revisions,
    /// Crates the store holds.
    pub extracted: Vec<String>,
    pub stale_files: BTreeSet<String>,
    pub rows: Vec<ReconciledRow>,
}

impl Reconciliation {
    /// Rows per state name.
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut m = BTreeMap::new();
        for r in &self.rows {
            *m.entry(r.verdict.state()).or_default() += 1;
        }
        m
    }

    pub fn row(&self, key: &str) -> Option<&ReconciledRow> {
        self.rows.iter().find(|r| r.spec.key.to_string() == key)
    }

    /// Attach the runtime axis to the row with `key`. The structural verdict is not
    /// touched. Returns `false` when no row has that key.
    pub fn attach_runtime(&mut self, key: &str, evidence: RuntimeEvidence) -> bool {
        match self.rows.iter_mut().find(|r| r.spec.key.to_string() == key) {
            Some(r) => {
                r.runtime = evidence;
                true
            }
            None => false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The decision
// ─────────────────────────────────────────────────────────────────────────

/// Reconciles claims against one store.
pub struct Reconciler<'a> {
    store: &'a CharonStore,
    /// The target repository at the revision the store was extracted from; used to
    /// detect stale files and to turn byte offsets into lines. `None`: neither.
    repo_root: Option<PathBuf>,
    stale: BTreeSet<String>,
}

fn line_of(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte.min(text.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

/// The 1-based line of the first occurrence of `needle` in `text`.
pub fn locate(text: &str, needle: &str) -> Option<usize> {
    text.find(needle).map(|at| line_of(text, at))
}

impl<'a> Reconciler<'a> {
    pub fn new(store: &'a CharonStore, repo_root: Option<&Path>) -> Self {
        let stale = repo_root.map(|r| store.stale_files(r)).unwrap_or_default();
        Reconciler {
            store,
            repo_root: repo_root.map(Path::to_path_buf),
            stale,
        }
    }

    fn span(&self, file: &str, start: Option<usize>, end: Option<usize>, what: String) -> RustSpan {
        let lines = match (&self.repo_root, self.stale.contains(file), start, end) {
            (Some(root), false, Some(a), Some(b)) => std::fs::read_to_string(root.join(file))
                .ok()
                .map(|t| (line_of(&t, a), line_of(&t, b))),
            _ => None,
        };
        RustSpan {
            file: file.to_string(),
            start,
            end,
            lines,
            what,
        }
    }

    fn row_span(&self, r: &StoreRow) -> RustSpan {
        self.span(
            r.file.as_deref().unwrap_or("-"),
            r.start,
            r.end,
            format!(
                "{} {} -> {}",
                r.predicate,
                normalize(&r.subject),
                normalize(&r.object)
            ),
        )
    }

    /// Is the body of `function` inspectable? `Err` is the slag reason it is not.
    fn inspect(&self, function: &str) -> Result<&StoreRow, (SlagReason, Option<RustSpan>)> {
        if !self.store.crates.contains_key(crate_of(function)) {
            return Err((SlagReason::RustSideNotExtracted, None));
        }
        let Some(def) = self.store.defines(function) else {
            return Err((SlagReason::RustSymbolNotInStore, None));
        };
        let def_span = self.row_span(def);
        if def.file.as_ref().is_some_and(|f| self.stale.contains(f)) {
            return Err((SlagReason::RustEvidenceStale, Some(def_span)));
        }
        if self.store.residual(function).is_some() {
            return Err((SlagReason::RustBodyNotStructured, Some(def_span)));
        }
        Ok(def)
    }

    /// The verdict of one claim. Order: the Rust side's inspectability, then the
    /// Ruby side's harvest, then comparability, then the comparison.
    pub fn decide(&self, spec: &ClaimSpec) -> ReconciledRow {
        let (verdict, rust_span, rust_cov) = self.decide_rust_first(spec);
        let ruby_cov = match &spec.ruby {
            RubyEvidence::Fact { extractor, .. } => format!("ruby: fact ({extractor})"),
            RubyEvidence::Absent { extractor, .. } => {
                format!("ruby: inspected, absent ({extractor})")
            }
            RubyEvidence::NotHarvested { .. } => "ruby: not harvested".to_string(),
        };
        ReconciledRow {
            spec: spec.clone(),
            verdict,
            rust_span,
            coverage: format!("{ruby_cov}; {rust_cov}"),
            runtime: RuntimeEvidence::NotExecuted {
                why: "no runtime gate attached".into(),
            },
        }
    }

    fn decide_rust_first(&self, spec: &ClaimSpec) -> (Verdict, Option<RustSpan>, String) {
        match &spec.rust {
            RustProbe::Data { krate, file, item } => {
                let span = self.data_span(file, item);
                let Some(c) = self.store.crates.get(krate) else {
                    return (
                        Verdict::Slag(SlagReason::RustSideNotExtracted),
                        span,
                        format!("rust: crate {krate} not in store"),
                    );
                };
                if !c.files.contains_key(file) {
                    return (
                        Verdict::Slag(SlagReason::RustSymbolNotInStore),
                        span,
                        format!("rust: {file} not in {krate}.files"),
                    );
                }
                if self.stale.contains(file) {
                    return (
                        Verdict::Slag(SlagReason::RustEvidenceStale),
                        span,
                        format!("rust: {file} changed since extraction"),
                    );
                }
                (
                    Verdict::Slag(SlagReason::RepresentedAsData),
                    span,
                    format!("rust: data item {item}, file extracted, no call-graph fact"),
                )
            }
            RustProbe::Edges { edges, conflicts } => {
                let mut present = Vec::new();
                let mut first_span = None;
                for (caller, _) in edges.iter().chain(conflicts) {
                    match self.inspect(caller) {
                        Err((reason, span)) => {
                            return (
                                Verdict::Slag(reason),
                                span,
                                format!("rust: {} ({reason:?})", normalize(caller)),
                            );
                        }
                        Ok(def) => {
                            if first_span.is_none() {
                                first_span = Some(self.row_span(def));
                            }
                        }
                    }
                }
                for (caller, callee) in edges {
                    if let Some(r) = self.store.edge(caller, callee) {
                        present.push(r);
                    }
                }
                let all = present.len() == edges.len();
                let conflict = conflicts
                    .iter()
                    .find_map(|(a, b)| self.store.edge(a, b).map(|r| (a, b, r)));
                let span = present.first().map(|r| self.row_span(r)).or(first_span);
                let cov = format!(
                    "rust: {}/{} edges ({})",
                    present.len(),
                    edges.len(),
                    present
                        .iter()
                        .map(|r| format!("{} {}", r.predicate, r.grade))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                (
                    self.compare(spec, all, conflict.map(|(a, b, _)| (a, b))),
                    span,
                    cov,
                )
            }
        }
    }

    fn compare(
        &self,
        spec: &ClaimSpec,
        all_present: bool,
        conflict: Option<(&String, &String)>,
    ) -> Verdict {
        if let RubyEvidence::NotHarvested { .. } = spec.ruby {
            return Verdict::Slag(SlagReason::RubySideNotHarvested);
        }
        if let Comparability::Different { ruby, rust } = &spec.comparability {
            return Verdict::NotComparable {
                detail: format!("ruby: {ruby}; rust: {rust}"),
            };
        }
        match (&spec.ruby, all_present, conflict) {
            (RubyEvidence::Fact { .. }, _, Some((a, b))) => Verdict::Disagreement {
                detail: format!("rust has {} -> {}", normalize(a), normalize(b)),
            },
            (RubyEvidence::Fact { .. }, true, None) => Verdict::Agreement,
            (RubyEvidence::Fact { .. }, false, None) => Verdict::RubyOnly,
            (RubyEvidence::Absent { .. }, true, _) => Verdict::CharonOnly,
            // Both inspected, neither has it.
            (RubyEvidence::Absent { .. }, false, _) => Verdict::Agreement,
            (RubyEvidence::NotHarvested { .. }, _, _) => {
                Verdict::Slag(SlagReason::RubySideNotHarvested)
            }
        }
    }

    fn data_span(&self, file: &str, item: &str) -> Option<RustSpan> {
        // A promoted static has a SYNTHETIC row with its byte range; a `const` has none.
        let want = format!("::{item}::promoted_const");
        let row = self.store.crates.values().flat_map(|c| &c.rows).find(|r| {
            r.predicate == "SYNTHETIC"
                && r.file.as_deref() == Some(file)
                && normalize(&r.subject).ends_with(&want)
        });
        if let Some(r) = row {
            return Some(self.row_span(r));
        }
        let root = self.repo_root.as_ref()?;
        let text = std::fs::read_to_string(root.join(file)).ok()?;
        let at = text.find(&format!(" {item}"))?;
        let end = at + text[at..].find('\n').unwrap_or(0);
        Some(self.span(
            file,
            Some(at + 1),
            Some(end),
            format!("source item {item} (no store row)"),
        ))
    }

    pub fn reconcile(
        &self,
        title: &str,
        revisions: Revisions,
        specs: &[ClaimSpec],
    ) -> Reconciliation {
        Reconciliation {
            title: title.to_string(),
            revisions,
            extracted: self.store.crates.keys().cloned().collect(),
            stale_files: self.stale.clone(),
            rows: specs.iter().map(|s| self.decide(s)).collect(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The drilldown report
// ─────────────────────────────────────────────────────────────────────────

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn ruby_span(e: &RubyEvidence) -> String {
    match e {
        RubyEvidence::Fact { span, .. } | RubyEvidence::Absent { span, .. } => span.to_string(),
        RubyEvidence::NotHarvested { .. } => "-".into(),
    }
}

fn ruby_text(e: &RubyEvidence) -> String {
    match e {
        RubyEvidence::Fact { fact, .. } => fact.clone(),
        RubyEvidence::Absent { looked_for, .. } => format!("absent: {looked_for}"),
        RubyEvidence::NotHarvested { why } => format!("not harvested: {why}"),
    }
}

/// What evidence would resolve a slag row.
pub fn resolution_hint(reason: SlagReason) -> &'static str {
    match reason {
        SlagReason::RustSideNotExtracted => {
            "Run Charon on the crate that holds the function and add its four artifacts to the store."
        }
        SlagReason::RustSymbolNotInStore => {
            "Re-extract: the crate is in the store but the function is not (renamed, moved or cfg'd out)."
        }
        SlagReason::RustEvidenceStale => {
            "Re-extract at the current revision: the file changed since the store was written."
        }
        SlagReason::RustBodyNotStructured => {
            "A structured body for the function: Charon structuring of the async body, or a synchronous seam the call graph can see."
        }
        SlagReason::RepresentedAsData => {
            "A data witness: compare the generated table to its generator input (the drift check), or execute it (runtime axis); a call graph never shows it."
        }
        SlagReason::RubySideNotHarvested => {
            "A source-side extractor that produces the fact (ruff lowering), instead of nothing."
        }
    }
}

/// Markdown: one table of every claim with all columns, then a drilldown per slag row.
pub fn render_markdown(r: &Reconciliation) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# {}\n", r.title);
    let _ = writeln!(
        o,
        "Generated by `crates/op-codegen-pipeline/tests/reconcile_create_admission.rs`. Do not edit by hand.\n"
    );
    let _ = writeln!(o, "Revisions: {}.\n", r.revisions);
    let _ = writeln!(o, "Crates in the Charon store: {}.", r.extracted.join(", "));
    if r.stale_files.is_empty() {
        let _ = writeln!(
            o,
            "Every store file hash matches the checkout (FNV-1a 64), so byte offsets are current.\n"
        );
    } else {
        let _ = writeln!(
            o,
            "Stale store files (hash differs from the checkout): {}.\n",
            r.stale_files.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    let counts = r.counts();
    let line: Vec<String> = counts.iter().map(|(k, v)| format!("{k} {v}")).collect();
    let _ = writeln!(o, "Rows: {}; {}.\n", r.rows.len(), line.join(", "));
    o.push_str("Structural verdicts compare the Ruby and Charon witnesses only. The runtime column is a separate axis and never changes a verdict.\n\n");
    o.push_str("| # | join key | level | verdict | Ruby fact | Ruby span | Rust span | witness coverage | runtime | repair stage |\n");
    o.push_str("|---|---|---|---|---|---|---|---|---|---|\n");
    for (i, row) in r.rows.iter().enumerate() {
        let rust = row
            .rust_span
            .as_ref()
            .map_or("-".to_string(), |s| format!("{s}: {}", s.what));
        let _ = writeln!(
            o,
            "| {} | `{}` | {:?} | {} | {} | {} | {} | {} | {} | {} |",
            i + 1,
            row.spec.key,
            row.spec.level,
            cell(&row.verdict.to_string()),
            cell(&ruby_text(&row.spec.ruby)),
            cell(&ruby_span(&row.spec.ruby)),
            cell(&rust),
            cell(&row.coverage),
            cell(&row.runtime.to_string()),
            cell(&row.spec.repair),
        );
    }
    o.push_str("\n## Slag drilldown: what evidence would resolve each row\n\n");
    let mut any = false;
    for (i, row) in r.rows.iter().enumerate() {
        if let Verdict::Slag(reason) = row.verdict {
            any = true;
            let _ = writeln!(o, "### {}. `{}`\n", i + 1, row.spec.key);
            let _ = writeln!(o, "- reason: `{reason:?}`");
            let _ = writeln!(o, "- resolves with: {}", resolution_hint(reason));
            let _ = writeln!(o, "- repair stage: {}", row.spec.repair);
            let _ = writeln!(o, "- runtime axis (unchanged verdict): {}\n", row.runtime);
        }
    }
    if !any {
        o.push_str("No slag rows.\n");
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(m: &str) -> JoinKey {
        JoinKey {
            application: "app".into(),
            model: "M".into(),
            operation: "create".into(),
            phase: "admission".into(),
            method: m.into(),
            effect: "e".into(),
        }
    }

    fn store_with(rows: &[(&str, &str, &str)], residual: &[&str]) -> CharonStore {
        let mut c = CrateStore::default();
        for (p, s, o) in rows {
            c.rows.push(StoreRow {
                predicate: (*p).into(),
                subject: (*s).into(),
                object: (*o).into(),
                grade: "COMPILER".into(),
                file: Some("src/a.rs".into()),
                start: Some(0),
                end: Some(1),
            });
        }
        for r in residual {
            c.residuals
                .push(("BODY_NOT_STRUCTURED".into(), (*r).into()));
        }
        c.files.insert("src/a.rs".into(), 0);
        let mut s = CharonStore::default();
        s.crates.insert("k".into(), c);
        s
    }

    fn fact() -> RubyEvidence {
        RubyEvidence::Fact {
            extractor: "x".into(),
            span: SourceSpan {
                file: "a.rb".into(),
                line_start: 1,
                line_end: 1,
            },
            fact: "f".into(),
        }
    }

    fn spec(edges: &[(&str, &str)], conflicts: &[(&str, &str)]) -> ClaimSpec {
        ClaimSpec {
            key: key("m"),
            level: Level::Admission,
            ruby: fact(),
            rust: RustProbe::Edges {
                edges: edges
                    .iter()
                    .map(|(a, b)| ((*a).into(), (*b).into()))
                    .collect(),
                conflicts: conflicts
                    .iter()
                    .map(|(a, b)| ((*a).into(), (*b).into()))
                    .collect(),
            },
            comparability: Comparability::Same,
            repair: String::new(),
        }
    }

    #[test]
    fn normalize_strips_generics_braces_and_via() {
        assert_eq!(
            normalize("m::{Svc<'a, U>[TraitClause0, TraitClause1]}::admit"),
            "m::Svc::admit"
        );
        assert_eq!(normalize("m::T::f via Clause"), "m::T::f");
    }

    #[test]
    fn present_edge_agrees_missing_edge_is_ruby_only() {
        let s = store_with(
            &[
                ("DEFINES", "k::a", "k::a"),
                ("CALLS_RESOLVED", "k::a", "k::b"),
            ],
            &[],
        );
        let r = Reconciler::new(&s, None);
        assert_eq!(
            r.decide(&spec(&[("k::a", "k::b")], &[])).verdict,
            Verdict::Agreement
        );
        assert_eq!(
            r.decide(&spec(&[("k::a", "k::c")], &[])).verdict,
            Verdict::RubyOnly
        );
    }

    #[test]
    fn a_conflicting_edge_is_a_disagreement() {
        let s = store_with(
            &[
                ("DEFINES", "k::a", "k::a"),
                ("CALLS_RESOLVED", "k::a", "k::b"),
            ],
            &[],
        );
        let r = Reconciler::new(&s, None);
        let v = r
            .decide(&spec(&[("k::a", "k::c")], &[("k::a", "k::b")]))
            .verdict;
        assert!(matches!(v, Verdict::Disagreement { .. }), "{v:?}");
    }

    #[test]
    fn absence_of_evidence_is_slag_never_ruby_only() {
        let s = store_with(&[("DEFINES", "k::a", "k::a")], &["k::a"]);
        let r = Reconciler::new(&s, None);
        // Body not structured.
        assert_eq!(
            r.decide(&spec(&[("k::a", "k::b")], &[])).verdict,
            Verdict::Slag(SlagReason::RustBodyNotStructured)
        );
        // Crate not extracted.
        assert_eq!(
            r.decide(&spec(&[("other::a", "k::b")], &[])).verdict,
            Verdict::Slag(SlagReason::RustSideNotExtracted)
        );
        // Function not defined.
        assert_eq!(
            r.decide(&spec(&[("k::zz", "k::b")], &[])).verdict,
            Verdict::Slag(SlagReason::RustSymbolNotInStore)
        );
    }

    #[test]
    fn absent_ruby_with_present_rust_is_charon_only() {
        let s = store_with(
            &[
                ("DEFINES", "k::a", "k::a"),
                ("MAY_DISPATCH", "k::a", "k::b via Clause"),
            ],
            &[],
        );
        let r = Reconciler::new(&s, None);
        let mut sp = spec(&[("k::a", "k::b")], &[]);
        sp.ruby = RubyEvidence::Absent {
            extractor: "x".into(),
            span: SourceSpan {
                file: "a.rb".into(),
                line_start: 1,
                line_end: 2,
            },
            looked_for: "b".into(),
        };
        assert_eq!(r.decide(&sp).verdict, Verdict::CharonOnly);
    }

    #[test]
    fn unknown_version_is_refused() {
        let dir = std::env::temp_dir().join(format!("reconcile-ver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (part, v, schema) in PARTS {
            let v = if *part == "rows" { "3" } else { v };
            std::fs::write(
                dir.join(format!("k.{part}.tsv")),
                format!("#version {v}\n#schema {schema}\n"),
            )
            .unwrap();
        }
        let err = CharonStore::load(&dir).unwrap_err();
        std::fs::remove_dir_all(&dir).ok();
        assert!(matches!(err, StoreError::UnknownVersion { .. }), "{err}");
    }

    #[test]
    fn fnv_matches_reference_vector() {
        // FNV-1a 64 of "a".
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
