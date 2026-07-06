//! `PROBE-RENDER-BAKE` — the OP/Redmine render bake, leg 1 (Redmine ERB).
//!
//! Plan + PRE-REGISTRATION (written BEFORE the first measurement run):
//! `.claude/plans/2026-07-06-redmine-op-render-bake-v1.md`. Design (the join,
//! verbatim-signature-verified): the bake-design doc of the same arc.
//!
//! Pipeline under test:
//! ```text
//! app/views/**.erb ──ruff_ruby_spo::views──► per-view (model, field set + raw census)
//!        │  basis = lifted Class.attributes ++ Class.associations  (ONE source:
//!        ▼           the same sequence render_class_with_methods walks)
//!   FieldMask (u64; >64-field classes recorded wide, render-skipped until
//!        │           OGAR render_class_with_methods_wide lands — PR #163)
//!        ▼
//!   askama render ══ bit-walk oracle ══ jinja witness (graceful skip)
//!        ▼
//!   parked bake: .claude/harvest/redmine-view-bake/  (BAKE_OUT=1)
//! ```
//!
//! ## PRE-REGISTERED thresholds (plan §"PRE-REGISTERED", 2026-07-06)
//!
//! - E1 mask coverage (per view: |fields| / |referenced|, honest denominator
//!   from the raw-ident census): median ≥ 0.60 stands · 0.30–0.60 partial
//!   (ships with the uncovered census as the finding) · < 0.30 KILL (assert).
//!   (An earlier revision of THIS comment mis-transcribed the stands bar as
//!   0.80; the plan file — the pre-registration of record, committed before
//!   any run — says 0.60. Corrected 2026-07-06 with the run-2 measurement
//!   noted against BOTH bars for honesty: 0.667 stands@0.60, partial@0.80.)
//! - E2 dual-target parity: EXACTLY 1.00 (assert) — deterministic machinery;
//!   jinja-absent runs are tagged `witnessed=false`, never silently "pass".
//! - E3 mask-reuse ratio (distinct masks / views, per class): reported;
//!   < 1.0 on high-view classes supports the Scope/route-dedup SoC claim.
//!
//! Env-gated + self-skipping (house style): `RAILS_CORPUS_SRC` — Rails app
//! root; `RAILS_CORPUS_NS` (default `redmine`); `BAKE_OUT=1` to park the
//! artifact under `.claude/harvest/redmine-view-bake/`.
//!
//! ## MEASURED — run 2, 2026-07-06 (fuses pinned below, corpus-signature-guarded)
//!
//! Corpus: redmine @ `bfd3c33a`, 506 ERB files. Run 1 was VOID (columnless
//! basis — OP-layout-only schema reader; fixed by ruff's classic-migration
//! fallback, ruff PR #48). Run 2, with the fallback:
//!
//! - **E1 median coverage 0.667** over 342 (view,model) rows → **STANDS**
//!   (plan bar 0.60). Uncovered census ships with the bake regardless.
//! - **E2 244/244** askama == bit-walk oracle; **jinja witnessed OK** (the
//!   `r#type` raw-ident escape initially tripped the oracle — probe parser
//!   bug, fixed; the kit was correct).
//! - **E3 aggregate 161 distinct masks / 333 views ≈ 0.48 < 0.5** — supports
//!   the Scope/route-dedup SoC claim, and precisely where it matters: the
//!   high-view classes reuse hardest (Repository 0.22, Group 0.25, Project
//!   0.29, Query 0.33, User 0.35, Issue 0.47); tiny classes (Board, Journal,
//!   Version at 1.00) trivially don't dedupe.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use lance_graph_contract::class_view::FieldMask;
use ogar_render_askama::render_class_with_methods;
use ogar_vocab::Class;
use ruff_ruby_spo::{ViewFieldSet, ViewTarget, extract_view_field_sets_with_report};

/// `Issue` → `issue`, `TimeEntry` → `time_entry`, `WikiContentVersion` →
/// `wiki_content_version` — the conventional Rails receiver ident.
fn snake(model: &str) -> String {
    let mut out = String::new();
    for (i, c) in model.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The mask-bit basis — the ONE generated source for field↔idx↔bit (design
/// §A iron rule): the exact name sequence `render_class_with_methods` walks.
fn basis(class: &Class) -> Vec<String> {
    class
        .attributes
        .iter()
        .map(|a| a.name.clone())
        .chain(class.associations.iter().map(|a| a.name.clone()))
        .collect()
}

/// Parse `pub <ident>:` field declarations out of an askama-rendered struct —
/// the falsifier-#2 present-set reading, replicated locally (the original is
/// test-local to OGAR's integration binary).
fn present_field_names(src: &str) -> BTreeSet<String> {
    src.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let rest = l.strip_prefix("pub ")?;
            let (name, _) = rest.split_once(':')?;
            // The kit raw-ident-escapes Rust keywords (`type` → `r#type`,
            // rust_struct.rs::escape_rust_ident); strip the escape so the
            // oracle compares source-level field names.
            let name = name.trim();
            let name = name.strip_prefix("r#").unwrap_or(name);
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                .then(|| name.to_string())
        })
        .collect()
}

struct BakeRow {
    model: String,
    view: String,
    mask: u64,
    n_fields: usize,
    present: Vec<String>,
    n_known: usize,
    n_referenced: usize,
    wide: bool,
}

fn json_str_array(items: &[String]) -> String {
    let mut s = String::from("[");
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        // Field names are Ruby idents — no escaping needed beyond quoting.
        let _ = write!(s, "\"{it}\"");
    }
    s.push(']');
    s
}

/// Run the jinja witness over the fixture rows via python3. Returns
/// `Some(parity_ok)` when jinja2 is available, `None` (graceful skip) when
/// not. The template replicates falsifier #2's `render_mask.py.j2` bit-walk.
fn jinja_witness(rows: &[(Vec<String>, u64, BTreeSet<String>)]) -> Option<bool> {
    let mut fixtures = String::from("[");
    for (i, (fields, mask, expected)) in rows.iter().enumerate() {
        if i > 0 {
            fixtures.push(',');
        }
        let exp: Vec<String> = expected.iter().cloned().collect();
        let _ = write!(
            fixtures,
            "{{\"fields\":{},\"mask\":{},\"expected\":{}}}",
            json_str_array(fields),
            mask,
            json_str_array(&exp)
        );
    }
    fixtures.push(']');
    let py = r#"
import json, sys
try:
    import jinja2
except ImportError:
    sys.exit(3)
rows = json.load(sys.stdin)
tpl = jinja2.Template(
    "{% for f in fields %}{% if (mask // 2**loop.index0) % 2 == 1 %}{{ f }}\n{% endif %}{% endfor %}"
)
for r in rows:
    got = set(tpl.render(fields=r["fields"], mask=r["mask"]).split())
    if got != set(r["expected"]):
        print(f"JINJA MISMATCH: mask={r['mask']} got={sorted(got)} want={sorted(r['expected'])}")
        sys.exit(1)
print(f"jinja witness: {len(rows)} rows OK")
"#;
    let mut child = std::process::Command::new("python3")
        .arg("-c")
        .arg(py)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .ok()?;
    use std::io::Write as _;
    child.stdin.as_mut()?.write_all(fixtures.as_bytes()).ok()?;
    let status = child.wait().ok()?;
    match status.code() {
        Some(0) => Some(true),
        Some(3) => None, // jinja2 not importable — graceful skip
        _ => Some(false),
    }
}

#[test]
fn render_bake_leg1_redmine() {
    let Some(src) = std::env::var_os("RAILS_CORPUS_SRC") else {
        eprintln!(
            "render_bake_probe: RAILS_CORPUS_SRC not set — skipping (set it to a \
             Rails app root, e.g. a Redmine checkout)."
        );
        return;
    };
    let ns = std::env::var("RAILS_CORPUS_NS").unwrap_or_else(|_| "redmine".to_string());
    let corpus = PathBuf::from(&src);
    let views_root = corpus.join("app/views");

    // 1. Harvest + lift. The lift is 1:1 over graph.models in declaration
    //    order (verified in ogar-from-ruff; OGAR #164 pins the same fact).
    let (graph, _schema_report) = ruff_ruby_spo::extract_app_with_schema(&corpus, &ns);
    let classes: Vec<Class> = ogar_from_ruff::lift_model_graph(&graph);
    assert_eq!(classes.len(), graph.models.len(), "lift must stay 1:1");

    // 2. Bases + view targets — the extraction vocab IS the basis, so
    //    name→position is total on `fields`.
    let mut bases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut targets: Vec<ViewTarget> = Vec::new();
    for class in &classes {
        let b = basis(class);
        if b.is_empty() {
            continue;
        }
        let recv = snake(&class.name);
        targets.push(ViewTarget {
            model: class.name.clone(),
            receivers: vec![recv.clone(), format!("@{recv}")],
            fields: b.clone(),
        });
        bases.insert(class.name.clone(), b);
    }
    let by_name: BTreeMap<&str, &Class> =
        classes.iter().map(|c| (c.name.as_str(), c)).collect();

    // 3. Views → field sets (+ the honest raw census).
    let (sets, report): (Vec<ViewFieldSet>, _) =
        extract_view_field_sets_with_report(&views_root, &targets);
    eprintln!(
        "== render bake leg 1 ({ns}) == {} erb files scanned, {} views with hits, {} (view,model) sets",
        report.erb_files,
        report.views_with_hits,
        sets.len()
    );
    assert!(report.erb_files >= 100, "corpus too small — wrong path?");
    assert!(!sets.is_empty(), "no view field sets — extractor regression");

    // 4. Masks + coverage + parity.
    let mut rows: Vec<BakeRow> = Vec::new();
    let mut coverages: Vec<f64> = Vec::new();
    let mut jinja_rows: Vec<(Vec<String>, u64, BTreeSet<String>)> = Vec::new();
    let mut askama_checked = 0usize;
    let mut sample_renders: Vec<(String, String)> = Vec::new();
    for set in &sets {
        let Some(b) = bases.get(&set.resource) else {
            continue;
        };
        let positions: Vec<u8> = set
            .fields
            .iter()
            .filter_map(|f| b.iter().position(|x| x == f).map(|p| p as u8))
            .collect();
        let wide = b.len() > 64;
        let coverage = if set.referenced.is_empty() {
            1.0
        } else {
            set.fields.len() as f64 / set.referenced.len() as f64
        };
        coverages.push(coverage);
        let mask = if wide {
            0 // recorded wide; u64 mask not meaningful — render-skipped.
        } else {
            FieldMask::from_positions(&positions).0
        };
        // The bit-walk oracle: positions → names.
        let expected: BTreeSet<String> = positions
            .iter()
            .filter(|&&p| (p as usize) < b.len())
            .map(|&p| b[p as usize].clone())
            .collect();
        if !wide && !positions.is_empty() {
            let class = by_name[set.resource.as_str()];
            let rendered = render_class_with_methods(class, FieldMask(mask), &[])
                .expect("narrow render must succeed");
            let got = present_field_names(&rendered);
            assert_eq!(
                got, expected,
                "E2 KILL: askama != bit-walk oracle for {}/{}",
                set.resource, set.view
            );
            askama_checked += 1;
            jinja_rows.push((b.clone(), mask, expected.clone()));
            if sample_renders.len() < 5 {
                sample_renders.push((
                    format!("{}__{}", snake(&set.resource), set.view.replace('/', "_")),
                    rendered,
                ));
            }
        }
        rows.push(BakeRow {
            model: set.resource.clone(),
            view: set.view.clone(),
            mask,
            n_fields: b.len(),
            present: expected.into_iter().collect(),
            n_known: set.fields.len(),
            n_referenced: set.referenced.len(),
            wide,
        });
    }

    // E1 — median coverage.
    coverages.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = coverages[coverages.len() / 2];
    // E3 — mask reuse per class.
    let mut per_class: BTreeMap<&str, (BTreeSet<u64>, usize)> = BTreeMap::new();
    for r in &rows {
        let e = per_class.entry(r.model.as_str()).or_default();
        e.0.insert(r.mask);
        e.1 += 1;
    }
    let jinja = jinja_witness(&jinja_rows);
    let witnessed = jinja.is_some();
    eprintln!(
        "E1 median coverage: {median:.3} over {} views | E2 askama==oracle on {askama_checked} rows, jinja witness: {} | E3 per-class (distinct masks / views):",
        coverages.len(),
        match jinja {
            Some(true) => "OK (witnessed)",
            Some(false) => "MISMATCH",
            None => "SKIPPED (jinja2 absent, witnessed=false)",
        }
    );
    let mut reuse_hits = 0usize;
    for (model, (masks, views)) in per_class.iter().filter(|(_, (_, v))| *v >= 3) {
        let ratio = masks.len() as f64 / *views as f64;
        if ratio < 1.0 {
            reuse_hits += 1;
        }
        eprintln!("  {model}: {}/{} = {ratio:.2}", masks.len(), views);
    }

    // Pre-registered gates.
    assert!(
        median >= 0.30,
        "E1 KILL: median coverage {median:.3} < 0.30 — views are not field-projections"
    );
    if let Some(ok) = jinja {
        assert!(ok, "E2 KILL: jinja witness mismatched the bit-walk oracle");
    }

    // Drift fuses — pinned from run 2 (2026-07-06, redmine @ bfd3c33a),
    // guarded by the corpus content-signature (506 ERB files, ns=redmine) so
    // a different corpus/checkout skips them instead of false-tripping.
    // NEVER pre-fill these; re-pin only from a real run, with the corpus
    // move named in the commit message.
    if ns == "redmine" && report.erb_files == 506 {
        assert_eq!(
            (report.views_with_hits, rows.len()),
            (240, 342),
            "FUSE: view-harvest shape moved on the pinned corpus — extractor drift?"
        );
        assert!(
            (0.60..0.75).contains(&median),
            "FUSE: E1 median {median:.3} left the pinned band [0.60, 0.75) — \
             basis or census drift (pinned 0.667)"
        );
        assert_eq!(
            askama_checked, 244,
            "FUSE: renderable (non-wide) row count moved (pinned 244; wide \
             classes render-skipped until OGAR #163 is wired)"
        );
    }

    // 5. Park the artifact when asked.
    if std::env::var_os("BAKE_OUT").is_some() {
        let out = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.claude/harvest/redmine-view-bake");
        std::fs::create_dir_all(out.join("samples")).unwrap();
        let mut order = String::new();
        for (model, b) in &bases {
            let _ = writeln!(
                order,
                "{{\"model\":\"{model}\",\"n_fields\":{},\"wide\":{},\"order\":{}}}",
                b.len(),
                b.len() > 64,
                json_str_array(b)
            );
        }
        std::fs::write(out.join("field_order.ndjson"), order).unwrap();
        let mut masks_nd = String::new();
        for r in &rows {
            let _ = writeln!(
                masks_nd,
                "{{\"model\":\"{}\",\"view\":\"{}\",\"classid\":null,\"mask_bits_hex\":\"{:#x}\",\"n_fields\":{},\"present\":{},\"n_known\":{},\"n_referenced\":{},\"coverage\":{:.3},\"wide\":{}}}",
                r.model,
                r.view,
                r.mask,
                r.n_fields,
                json_str_array(&r.present),
                r.n_known,
                r.n_referenced,
                if r.n_referenced == 0 { 1.0 } else { r.n_known as f64 / r.n_referenced as f64 },
                r.wide
            );
        }
        std::fs::write(out.join("masks.ndjson"), masks_nd).unwrap();
        for (name, src) in &sample_renders {
            std::fs::write(out.join("samples").join(format!("{name}.rs")), src).unwrap();
        }
        let readme = format!(
            "# redmine-view-bake — leg 1 (Redmine ERB), measured {}\n\n\
             DATA, not code (fuzzy-recipe-codebook §8c). The mask hex is read\n\
             against field_order.ndjson in THIS directory — regenerate both\n\
             together, never independently (I-LEGACY-API-FEATURE-GATED).\n\n\
             - corpus: RAILS_CORPUS_SRC={} (ns={ns})\n\
             - erb files scanned: {} · views with hits: {} · (view,model) rows: {}\n\
             - E1 median coverage: {median:.3} (pre-reg, plan of record: >=0.60 stands, 0.30-0.60 partial, <0.30 KILL)\n\
             - E2: askama==bit-walk on {askama_checked} rows; jinja witnessed={witnessed}\n\
             - E3 reuse: {reuse_hits} class(es) with ratio < 1.0 among >=3-view classes\n\
             - classids: null (v1 bakes namespace-locally; redmine-canon mint is a follow-up)\n\
             - wide classes (>64 fields): recorded, render-skipped until OGAR #163\n\
             \n\
             Probe: crates/op-codegen-pipeline/tests/render_bake_probe.rs\n",
            chrono_free_date(),
            src.to_string_lossy(),
            report.erb_files,
            report.views_with_hits,
            rows.len(),
        );
        std::fs::write(out.join("README.md"), readme).unwrap();
        eprintln!("bake parked at {}", out.display());
    }
}

/// Date without a chrono dep: read from `SOURCE_DATE_EPOCH`-style env or a
/// fixed marker the committer fills — the bake README's provenance line.
fn chrono_free_date() -> String {
    std::env::var("BAKE_DATE").unwrap_or_else(|_| "2026-07-06".to_string())
}
