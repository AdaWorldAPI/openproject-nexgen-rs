//! `PROBE-RENDER-BAKE` — the OP/Redmine render bake, leg 2 (OP representers).
//!
//! Plan + PRE-REGISTRATION (written BEFORE the first measurement run):
//! `.claude/plans/2026-07-06-redmine-op-render-bake-v1.md` §"PRE-REGISTERED —
//! leg 2". Template + helper style copied from leg 1:
//! `crates/op-codegen-pipeline/tests/render_bake_probe.rs`.
//!
//! Pipeline under test:
//! ```text
//! lib/api/v3/**/*_representer.rb ──ruff_ruby_spo::representers──► per-file decls
//!        │  model = singularize(PascalCase(parent dir name)); basis = lifted
//!        ▼  Class.attributes ++ Class.associations (same source leg 1 uses)
//!   FieldMask (narrow, <=64 fields) / WideFieldMask (wide, >64 fields —
//!        │   work_packages is the born use-case, OGAR #163)
//!        ▼
//!   askama render (narrow OR wide entry point) ══ bit-walk oracle ══ jinja
//!        │   witness (mask passed as a hex string; Python bigint carries it)
//!        ▼
//!   CONV-1: Jaccard(Redmine-Issue masks [leg 1 artifact, C4-renamed],
//!        │           OP-WorkPackage masks [this leg])
//!        ▼
//!   parked bake: .claude/harvest/op-representer-bake/  (BAKE_OUT=1)
//! ```
//!
//! ## PRE-REGISTERED thresholds (plan §"PRE-REGISTERED — leg 2", 2026-07-06)
//!
//! - L2-E1 representer coverage: median >= 0.60 stands · 0.30-0.60 partial
//!   (uncovered census ships as the finding) · < 0.30 KILL (assert).
//! - L2-E2 dual-target parity incl. the wide leg: EXACTLY 1.00 (assert).
//! - CONV-1: Jaccard of Redmine-`Issue` (leg 1, C4-renamed) vs OP-`WorkPackage`
//!   (this leg) present-field unions: >= 0.50 convergence stands ·
//!   0.25-0.50 partial (disjoint census IS the deliverable) · < 0.25 refuted
//!   (assert only the < 0.25 KILL bar — the 0.50 "stands" bar is
//!   informational/report-only per the pre-registration).
//!
//! Env-gated + self-skipping (house style): `OP_CORPUS_SRC` — OpenProject
//! checkout root; `OP_CORPUS_NS` (default `openproject`); `BAKE_OUT=1` to
//! park the artifact under `.claude/harvest/op-representer-bake/`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use lance_graph_contract::class_view::{FieldMask, WideFieldMask};
use ogar_render_askama::{render_class_with_methods, render_class_with_methods_wide};
use ogar_vocab::Class;
use ruff_ruby_spo::{RepresenterFieldSet, extract_representer_field_sets};

/// `Issue` -> `issue`, `WorkPackage` -> `work_package` — the conventional
/// Rails receiver ident. Also doubles as the camelCase-decl-name -> snake
/// normalizer (a pure-snake input is a no-op, since there are no interior
/// uppercase letters to split on). Copied verbatim from leg 1.
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

/// The mask-bit basis — the ONE generated source for field<->idx<->bit: the
/// exact name sequence `render_class_with_methods`/`_wide` walks. Copied
/// verbatim from leg 1.
fn basis(class: &Class) -> Vec<String> {
    class
        .attributes
        .iter()
        .map(|a| a.name.clone())
        .chain(class.associations.iter().map(|a| a.name.clone()))
        .collect()
}

/// Parse `pub <ident>:` field declarations out of an askama-rendered struct —
/// the falsifier-#2 present-set reading, replicated locally. Copied verbatim
/// from leg 1 (including the `r#` raw-ident strip).
fn present_field_names(src: &str) -> BTreeSet<String> {
    src.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let rest = l.strip_prefix("pub ")?;
            let (name, _) = rest.split_once(':')?;
            let name = name.trim();
            let name = name.strip_prefix("r#").unwrap_or(name);
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                .then(|| name.to_string())
        })
        .collect()
}

/// Naive English singularizer for a representer directory name — the
/// inverse-ish of Rails' pluralization for the closed set of shapes this
/// corpus actually uses (measured, not a general solver).
fn singularize_dir(name: &str) -> String {
    if name == "statuses" {
        return "status".to_string();
    }
    if let Some(stem) = name.strip_suffix("ies") {
        return format!("{stem}y");
    }
    if let Some(stem) = name.strip_suffix('s') {
        return stem.to_string();
    }
    name.to_string()
}

/// `work_package` -> `WorkPackage` — the inverse of [`snake`], PascalCase.
fn pascal(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}

/// Build the mask hex string from bit positions directly (independent of
/// [`FieldMask`]/[`WideFieldMask`]'s internal representation): 4 x u64
/// limbs, high-to-low, leading zero-limbs dropped entirely, the top limb
/// unpadded and lower limbs zero-padded to 16 hex chars. For narrow masks
/// (single nonzero limb at index 0) this degenerates to the plain `{:x}` of
/// that limb — asserted as a property inside the test below.
fn mask_hex(positions: &[u8]) -> String {
    let mut limbs = [0u64; 4];
    for &p in positions {
        let limb = (p / 64) as usize;
        if limb < limbs.len() {
            limbs[limb] |= 1u64 << (p % 64);
        }
    }
    match limbs.iter().rposition(|&l| l != 0) {
        None => "0x0".to_string(),
        Some(top) => {
            let mut s = format!("0x{:x}", limbs[top]);
            for i in (0..top).rev() {
                let _ = write!(s, "{:016x}", limbs[i]);
            }
            s
        }
    }
}

fn json_str_array(items: &[String]) -> String {
    let mut s = String::from("[");
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "\"{it}\"");
    }
    s.push(']');
    s
}

/// Run the jinja witness over the fixture rows via python3. Unlike leg 1's
/// helper, the mask travels as a HEX STRING (`int(x, 16)` on the Python
/// side) so wide (>64-bit) masks carry losslessly through Python's bigint —
/// no JSON-number precision ceiling. Returns `Some(parity_ok)` when jinja2
/// is available, `None` (graceful skip) when not.
fn jinja_witness_leg2(rows: &[(Vec<String>, String, BTreeSet<String>)]) -> Option<bool> {
    let mut fixtures = String::from("[");
    for (i, (fields, mask_hex, expected)) in rows.iter().enumerate() {
        if i > 0 {
            fixtures.push(',');
        }
        let exp: Vec<String> = expected.iter().cloned().collect();
        let _ = write!(
            fixtures,
            "{{\"fields\":{},\"mask\":\"{}\",\"expected\":{}}}",
            json_str_array(fields),
            mask_hex,
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
    mask_int = int(r["mask"], 16)
    got = set(tpl.render(fields=r["fields"], mask=mask_int).split())
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
        Some(3) => None,
        _ => Some(false),
    }
}

/// Minimal hand-parse of a `"key":[...]` string-array field out of one
/// ndjson line (field names are plain Ruby idents — no escaping to worry
/// about). Returns an empty vec if the key isn't present on this line.
fn extract_str_array(line: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\":[");
    let Some(start) = line.find(&needle) else {
        return Vec::new();
    };
    let after = &line[start + needle.len()..];
    let Some(end) = after.find(']') else {
        return Vec::new();
    };
    let inner = &after[..end];
    if inner.trim().is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect()
}

/// Minimal hand-parse of a `"key":"value"` string field out of one ndjson
/// line.
fn extract_str_field(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

struct Leg2Row {
    model: String,
    file: String,
    mask_hex: String,
    n_fields: usize,
    present: Vec<String>,
    n_declared: usize,
    coverage: f64,
    wide: bool,
    rendered: bool,
}

#[test]
fn render_bake_leg2_op_representers() {
    // Sanity: `mask_hex` must degenerate to plain `{:x}` for narrow (single,
    // non-top) limbs — the property leg 2's jinja witness relies on to be
    // simultaneously correct for narrow AND wide masks.
    {
        let positions = [0u8, 2, 5, 63];
        let narrow_u64 = FieldMask::from_positions(&positions).0;
        assert_eq!(
            mask_hex(&positions),
            format!("0x{narrow_u64:x}"),
            "mask_hex must degenerate to plain hex for narrow (<64) masks"
        );
    }

    let Some(src) = std::env::var_os("OP_CORPUS_SRC") else {
        eprintln!(
            "render_bake_leg2_probe: OP_CORPUS_SRC not set — skipping (set it to an \
             OpenProject checkout root, e.g. /tmp/op-corpus)."
        );
        return;
    };
    let ns = std::env::var("OP_CORPUS_NS").unwrap_or_else(|_| "openproject".to_string());
    let corpus = PathBuf::from(&src);

    // 1. Harvest + lift (identical to leg 1).
    let (graph, _schema_report) = ruff_ruby_spo::extract_app_with_schema(&corpus, &ns);
    let classes: Vec<Class> = ogar_from_ruff::lift_model_graph(&graph);
    assert_eq!(classes.len(), graph.models.len(), "lift must stay 1:1");

    let mut bases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for class in &classes {
        let b = basis(class);
        if b.is_empty() {
            continue;
        }
        bases.insert(class.name.clone(), b);
    }
    let by_name: BTreeMap<&str, &Class> = classes.iter().map(|c| (c.name.as_str(), c)).collect();

    // 2. Representer extraction.
    let repr_root = corpus.join("lib/api/v3");
    let sets: Vec<RepresenterFieldSet> = extract_representer_field_sets(&repr_root);
    eprintln!(
        "== render bake leg 2 ({ns}) == {} representer files with declarations under lib/api/v3",
        sets.len()
    );
    assert!(!sets.is_empty(), "no representer field sets — extractor regression");

    // 3+4. File -> model mapping, field resolution, masks + parity.
    let mut rows: Vec<Leg2Row> = Vec::new();
    let mut unmapped_files: Vec<String> = Vec::new();
    let mut coverages: Vec<f64> = Vec::new();
    let mut jinja_rows: Vec<(Vec<String>, String, BTreeSet<String>)> = Vec::new();
    let mut askama_checked = 0usize;
    let mut sample_wide: Vec<(String, String)> = Vec::new();
    let mut sample_narrow: Vec<(String, String)> = Vec::new();

    for set in &sets {
        let file_path = Path::new(&set.file);
        let dirname = file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty());
        let Some(dirname) = dirname else {
            unmapped_files.push(set.file.clone());
            continue;
        };
        let model_name = pascal(&singularize_dir(dirname));
        let Some(basis_vec) = bases.get(&model_name) else {
            unmapped_files.push(set.file.clone());
            continue;
        };

        let n_declared = set.decls.len();
        let mut positions: Vec<u8> = Vec::new();
        for decl in &set.decls {
            let snaked = snake(&decl.name);
            if let Some(p) = basis_vec.iter().position(|x| x == &snaked) {
                positions.push(p as u8);
            }
        }
        let wide = basis_vec.len() > 64;
        let coverage = if n_declared == 0 {
            1.0
        } else {
            positions.len() as f64 / n_declared as f64
        };
        coverages.push(coverage);

        let expected: BTreeSet<String> = positions
            .iter()
            .filter(|&&p| (p as usize) < basis_vec.len())
            .map(|&p| basis_vec[p as usize].clone())
            .collect();
        let mhex = mask_hex(&positions);
        let mut rendered_flag = false;

        if !positions.is_empty() {
            let class = by_name[model_name.as_str()];
            let rendered = if wide {
                // Sanctioned mask-minter brick (lance-graph #669):
                // `WideFieldMask::from_universe_present(universe = basis,
                // present = declared skin fields)` mints the IDENTICAL mask
                // odoo-rs's `view_mask::mint_wide_mask` and the ERB
                // `ViewFieldSet` mint — the interchangeable-across-consumers
                // guarantee — plus the 256-field SOC-split guard.
                //
                // Duplicate-field guard (codex P2, #87): the brick sets EVERY
                // universe position whose name is present (membership), whereas
                // this probe's first-match `positions` oracle (expected / mhex /
                // coverage) records only the first. `basis()` = attributes ++
                // associations is NOT deduped, so a lifted class CAN carry a
                // duplicate name. So: use the brick when the basis is unique
                // (the norm — where the interchangeability win lives, and where
                // membership is provably bit-identical to `from_positions`);
                // fall back to the first-match `from_positions(&positions)` when
                // it isn't, so the minted mask never diverges from this probe's
                // own artifacts.
                let basis_unique = {
                    let uniq: BTreeSet<&String> = basis_vec.iter().collect();
                    uniq.len() == basis_vec.len()
                };
                let mask = if basis_unique {
                    let basis_refs: Vec<&str> = basis_vec.iter().map(String::as_str).collect();
                    let present: Vec<String> =
                        set.decls.iter().map(|d| snake(&d.name)).collect();
                    let present_refs: Vec<&str> = present.iter().map(String::as_str).collect();
                    WideFieldMask::from_universe_present(&basis_refs, &present_refs)
                        .expect("basis within the 256-field SOC cap")
                } else {
                    WideFieldMask::from_positions(&positions)
                };
                render_class_with_methods_wide(class, &mask, &[]).expect("wide render must succeed")
            } else {
                let mask = FieldMask::from_positions(&positions);
                render_class_with_methods(class, mask, &[]).expect("narrow render must succeed")
            };
            let got = present_field_names(&rendered);
            assert_eq!(
                got, expected,
                "L2-E2 KILL: askama != bit-walk oracle for {model_name}/{}",
                set.file
            );
            askama_checked += 1;
            rendered_flag = true;
            jinja_rows.push((basis_vec.clone(), mhex.clone(), expected.clone()));

            let sample_name = format!(
                "{}__{}",
                snake(&model_name),
                set.file.replace('/', "_").trim_end_matches(".rb")
            );
            if wide {
                if sample_wide.len() < 5 {
                    sample_wide.push((sample_name, rendered));
                }
            } else if sample_narrow.len() < 5 {
                sample_narrow.push((sample_name, rendered));
            }
        }

        rows.push(Leg2Row {
            model: model_name,
            file: set.file.clone(),
            mask_hex: mhex,
            n_fields: basis_vec.len(),
            present: expected.into_iter().collect(),
            n_declared,
            coverage,
            wide,
            rendered: rendered_flag,
        });
    }

    // Sample selection: up to 5, WIDE first (must include >=1 wide if any exist).
    let mut sample_renders: Vec<(String, String)> = Vec::new();
    sample_renders.extend(sample_wide.into_iter());
    for s in sample_narrow {
        if sample_renders.len() >= 5 {
            break;
        }
        sample_renders.push(s);
    }
    sample_renders.truncate(5);

    // L2-E1 — median coverage.
    coverages.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = if coverages.is_empty() {
        0.0
    } else {
        coverages[coverages.len() / 2]
    };
    let wide_row_count = rows.iter().filter(|r| r.wide).count();

    let jinja = jinja_witness_leg2(&jinja_rows);
    let witnessed = jinja.is_some();
    eprintln!(
        "L2-E1 median coverage: {median:.3} over {} mapped rows | L2-E2 askama==oracle on \
         {askama_checked} rows ({wide_row_count} wide), jinja witness: {}",
        rows.len(),
        match jinja {
            Some(true) => "OK (witnessed)",
            Some(false) => "MISMATCH",
            None => "SKIPPED (jinja2 absent, witnessed=false)",
        }
    );

    // Pre-registered gates (leg 2).
    assert!(
        median >= 0.30,
        "L2-E1 KILL: median coverage {median:.3} < 0.30 — representers are not \
         field-projections; the bake claim is regraded (plan §PRE-REGISTERED — leg 2, L2-E1)"
    );
    if let Some(ok) = jinja {
        assert!(
            ok,
            "L2-E2 KILL: jinja witness mismatched the bit-walk oracle \
             (plan §PRE-REGISTERED — leg 2, L2-E2)"
        );
    }

    // Drift fuses — pinned from the first green run (2026-07-06, op-corpus @
    // 46c1fda2), guarded by the corpus content-signature (104 representer
    // files with declarations, ns=openproject) so a different corpus skips
    // them instead of false-tripping. NEVER pre-fill; re-pin only from a
    // real run with the corpus move named in the commit message.
    // Known limitation pinned WITH the numbers: the OP-layout schema reader
    // consumes the db/migrate/tables/*.rb baseline only, so WorkPackage
    // measured 40 fields (narrow) — post-baseline add_column migrations are
    // not applied. If the reader grows migration-replay, these fuses move
    // (expected: WorkPackage crosses 64 and the wide path lights up).
    if ns == "openproject" && sets.len() == 104 {
        assert_eq!(
            (rows.len(), askama_checked, wide_row_count),
            (52, 36, 0),
            "FUSE: leg-2 harvest shape moved on the pinned corpus — mapper or \
             extractor drift?"
        );
        assert!(
            (0.30..0.60).contains(&median),
            "FUSE: L2-E1 median {median:.3} left the pinned partial band \
             [0.30, 0.60) (pinned 0.429)"
        );
    }

    // CONV-1 — Jaccard(Redmine-Issue [leg 1 artifact, C4-renamed], OP-WorkPackage [this leg]).
    let leg1_masks_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude/harvest/redmine-view-bake/masks.ndjson");
    let leg1_content = std::fs::read_to_string(&leg1_masks_path).unwrap_or_else(|e| {
        panic!(
            "CONV-1: failed to read committed leg-1 artifact {}: {e}",
            leg1_masks_path.display()
        )
    });
    let mut redmine_fields: BTreeSet<String> = BTreeSet::new();
    for line in leg1_content.lines() {
        if line.contains("\"model\":\"Issue\"") {
            for f in extract_str_array(line, "present") {
                redmine_fields.insert(f);
            }
        }
    }

    let seed_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude/harvest/c4-rename-seed.ndjson");
    let seed_content = std::fs::read_to_string(&seed_path)
        .unwrap_or_else(|e| panic!("CONV-1: failed to read C4 rename seed {}: {e}", seed_path.display()));
    let mut rename_map: BTreeMap<String, String> = BTreeMap::new();
    for line in seed_content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let from_class = extract_str_field(line, "from_class").unwrap_or_default();
        let kind = extract_str_field(line, "kind").unwrap_or_default();
        if from_class == "Issue" && kind == "rename" {
            if let (Some(from_field), Some(to_field)) = (
                extract_str_field(line, "from_field"),
                extract_str_field(line, "to_field"),
            ) {
                rename_map.insert(from_field, to_field);
            }
        }
    }

    let mut redmine_after_rename: BTreeSet<String> = BTreeSet::new();
    let mut renames_applied = 0usize;
    for f in &redmine_fields {
        if let Some(to) = rename_map.get(f) {
            redmine_after_rename.insert(to.clone());
            renames_applied += 1;
        } else {
            redmine_after_rename.insert(f.clone());
        }
    }

    let op_fields: BTreeSet<String> = rows
        .iter()
        .filter(|r| r.model == "WorkPackage")
        .flat_map(|r| r.present.iter().cloned())
        .collect();

    let intersection: Vec<String> = redmine_after_rename.intersection(&op_fields).cloned().collect();
    let union_count = redmine_after_rename.union(&op_fields).count();
    let jaccard = if union_count == 0 {
        0.0
    } else {
        intersection.len() as f64 / union_count as f64
    };
    let redmine_only: Vec<String> = redmine_after_rename.difference(&op_fields).cloned().collect();
    let op_only: Vec<String> = op_fields.difference(&redmine_after_rename).cloned().collect();

    eprintln!(
        "CONV-1: jaccard={jaccard:.3} intersection={intersection:?} redmine_only={redmine_only:?} \
         op_only={op_only:?} renames_applied={renames_applied}"
    );

    // Pre-registered gate: only the <0.25 KILL bar is asserted; the >=0.50
    // "stands" bar is informational/report-only per the pre-registration.
    assert!(
        jaccard >= 0.25,
        "CONV-1 REFUTED: jaccard {jaccard:.3} < 0.25 — Redmine-Issue and OP-WorkPackage field \
         masks do not converge at the field level (plan §PRE-REGISTERED — leg 2, CONV-1)"
    );

    // CONV-1 drift fuse (same corpus-signature guard as above): pinned 0.464
    // with the seeded C4 v0 (2 of 4 renames applied — leg-1 stores
    // association NAMES, so the *_id column renames never match; the
    // association-level pairs tracker→type / fixed_version→version are the
    // census-identified C4 v1 candidates, to be applied only under a NEW
    // pre-registered CONV run, never retrofitted into this one).
    if ns == "openproject" && sets.len() == 104 {
        assert!(
            (0.40..0.55).contains(&jaccard),
            "FUSE: CONV-1 jaccard {jaccard:.3} left the pinned band [0.40, 0.55) \
             (pinned 0.464 with C4 seed v0)"
        );
    }

    let conv1_verdict = if jaccard >= 0.50 {
        "convergence stands"
    } else if jaccard >= 0.25 {
        "partial (disjoint census is the deliverable)"
    } else {
        "refuted"
    };
    eprintln!(
        "== leg 2 summary == {} representer files scanned | {} mapped rows | {} unmapped files | \
         median coverage {median:.3} | {wide_row_count} wide rows | askama_checked {askama_checked} | \
         jinja {} | CONV-1 jaccard {jaccard:.3} ({conv1_verdict})",
        sets.len(),
        rows.len(),
        unmapped_files.len(),
        match jinja {
            Some(true) => "witnessed OK",
            Some(false) => "MISMATCH",
            None => "skipped (jinja2 absent)",
        }
    );

    // 5. Park the artifact when asked.
    if std::env::var_os("BAKE_OUT").is_some() {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude/harvest/op-representer-bake");
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
                "{{\"model\":\"{}\",\"file\":\"{}\",\"classid\":null,\"mask_bits_hex\":\"{}\",\"n_fields\":{},\"present\":{},\"n_declared\":{},\"coverage\":{:.3},\"wide\":{},\"rendered\":{}}}",
                r.model,
                r.file,
                r.mask_hex,
                r.n_fields,
                json_str_array(&r.present),
                r.n_declared,
                r.coverage,
                r.wide,
                r.rendered,
            );
        }
        std::fs::write(out.join("masks.ndjson"), masks_nd).unwrap();

        let conv1 = format!(
            "{{\"jaccard\":{jaccard:.3},\"intersection\":{},\"redmine_only\":{},\"op_only\":{},\"renames_applied\":{renames_applied}}}\n",
            json_str_array(&intersection),
            json_str_array(&redmine_only),
            json_str_array(&op_only),
        );
        std::fs::write(out.join("conv1.json"), conv1).unwrap();

        for (name, src) in &sample_renders {
            std::fs::write(out.join("samples").join(format!("{name}.rs")), src).unwrap();
        }

        let readme = format!(
            "# op-representer-bake — leg 2 (OP representers), measured {}\n\n\
             DATA, not code (fuzzy-recipe-codebook §8c). The mask hex is read\n\
             against field_order.ndjson in THIS directory — regenerate both\n\
             together, never independently (I-LEGACY-API-FEATURE-GATED).\n\n\
             - corpus: OP_CORPUS_SRC={} (ns={ns})\n\
             - representer files with declarations: {} · mapped rows: {} · unmapped files: {}\n\
             - L2-E1 median coverage: {median:.3} (pre-reg: >=0.60 stands, 0.30-0.60 partial, <0.30 KILL)\n\
             - L2-E2: askama==bit-walk on {askama_checked} rows ({wide_row_count} wide); jinja witnessed={witnessed}\n\
             - CONV-1: jaccard={jaccard:.3} vs Redmine-Issue (leg 1 artifact, C4-renamed); \
             renames_applied={renames_applied} ({conv1_verdict})\n\
             - classids: null (v1 bakes namespace-locally)\n\
             \n\
             Probe: crates/op-codegen-pipeline/tests/render_bake_leg2_probe.rs\n\
             Note: leg 2, pre-reg L2-E1/L2-E2/CONV-1; wide leg wired via OGAR #163.\n",
            chrono_free_date(),
            src.to_string_lossy(),
            sets.len(),
            rows.len(),
            unmapped_files.len(),
        );
        std::fs::write(out.join("README.md"), readme).unwrap();
        eprintln!("bake parked at {}", out.display());
    }
}

/// Date without a chrono dep — the bake README's provenance line. Copied
/// verbatim from leg 1.
fn chrono_free_date() -> String {
    std::env::var("BAKE_DATE").unwrap_or_else(|_| "2026-07-06".to_string())
}
