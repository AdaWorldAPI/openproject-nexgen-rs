//! `PROBE-OGAR-BODY-TRIAGE` — the F17 **Rails TEST leg**.
//!
//! Spec: OGAR `docs/INTEGRATION-MAP.md` F17 row (Odoo control leg RUN
//! 2026-07-06, 94.9% order-free recoverable) + `E-BODY-TRIAGE-ODOO-CONTROL-1`.
//! The control (Odoo `_compute_*`, already declarative) is measured; THIS
//! probe runs the test population — real Rails `before_*`/`after_*` lifecycle
//! hooks — via the `ruff_ruby_spo` writes/calls/reads/raises facts. This is
//! the remaining `[H]` gate on **D-ACCIDENTAL-IMPERATIVE**: the claim that
//! imperative Rails hook bodies are mostly *accidentally* imperative, i.e.
//! recoverable order-free as a declarative `(verb, criteria)` recipe.
//!
//! ## Corpus (env-gated, self-skipping — ruff #44 house style)
//!
//! ```sh
//! RAILS_CORPUS_SRC=/path/to/rails-app RAILS_CORPUS_NS=redmine \
//!     cargo test -p ruff_openproject --test body_triage_probe -- --nocapture
//! ```
//!
//! Without `RAILS_CORPUS_SRC` the probe prints a skip note and exits green,
//! so CI without a corpus checkout is unaffected. The 2026-07-06 measurement
//! leg ran on the **Redmine** corpus (`app/models`, 82 models) — the fork
//! ancestor of OpenProject, i.e. the same object graph per the AR-shape
//! reunion order; an OpenProject-corpus re-run slots in by pointing the env
//! var at an OpenProject checkout.
//!
//! ## Method (mirrors the Odoo control leg; adapted per the F17 row)
//!
//! Static order-signature per hook from harvested facts:
//! - `W` = `writes_field` objects (own-field setter writes, Authoritative)
//! - `R` = `reads_field` objects (Inferred)
//! - `X` = `raises` objects
//! - `C` = `calls` objects (AR-mutator dispatches, `"receiver.method"`,
//!   Inferred) — the Rails leg includes `calls` per the F17 row ("via
//!   `ruff_ruby_spo` writes/calls"); a mutator dispatch is a write whose
//!   target field-set is unknown, so it counts as mutating but can never
//!   count as self-feedback (conservative in BOTH directions — noted below).
//!
//! Verb-class triage (first match wins, top to bottom):
//! - **no-facts**: W∪R∪X∪C = ∅ → unresolved, excluded from the arm.
//! - **self-feedback**: R∩W ≠ ∅ (read-modify-write of the same own field
//!   inside one hook) → FAIL (order-dependent).
//! - **write+raise**: (W∪C ≠ ∅) ∧ (X ≠ ∅) → FAIL (partial-write escape
//!   order).
//! - **guard-pure**: X ≠ ∅ ∧ W∪C = ∅ → PASS (abort criteria, order-free).
//! - **compute-pure**: W∪C ≠ ∅ (∧ X = ∅ ∧ R∩W = ∅) → PASS (set/dispatch
//!   with criteria, order-free).
//! - **read-only**: R ≠ ∅ only → excluded from the behavioural arm
//!   (observers; nothing to recover).
//!
//! Behavioural arm = guard-pure + compute-pure + self-feedback + write+raise.
//! PASS-rate = (guard-pure + compute-pure) / arm.
//!
//! Cross-hook order is NOT counted here (recompute-DAG Kahn-orderability is
//! the F-row's separate assertion). Conservative-bound caveats: (a) a
//! `calls`-dispatch that mutates a field the hook also reads is invisible to
//! the R∩W check (understates the tail); (b) `reads_field` is Inferred and
//! over-captures bare attribute reads in conditions (overstates
//! self-feedback). Both caveats are printed with the decomposition.
//!
//! ## PRE-REGISTERED thresholds (written 2026-07-06 BEFORE the first run —
//! the C5/A-B discipline: the noun side's 26/26 is asserted, so the verb/
//! behaviour side may not borrow it; this leg registers its own KILL)
//!
//! - PASS-rate ≥ 80% of the resolved behavioural arm → supports
//!   D-ACCIDENTAL-IMPERATIVE `[H]` → `[G]` (the operator regrades; this probe
//!   only produces the evidence).
//! - 60% ≤ PASS-rate < 80% → stays `[H]`; the decomposition is the finding.
//! - PASS-rate < 60% → KILL signal: D-ACCIDENTAL-IMPERATIVE is regraded, the
//!   85/15 mechanical-split doctrine loses its behaviour-arm support.
//! - The FAIL tail must decompose into the two named shapes (self-feedback /
//!   write+raise); any third dominant shape is a finding in its own right,
//!   not noise.
//!
//! ## Walker-version gate (compile-time loud)
//!
//! The probe requires three `ruff_ruby_spo`/`ruff_spo_triplet` extensions
//! that are prepared but NOT yet on ruff `main` (this session lacks ruff push
//! access; delivered as patches): (1) the `Node::IfMod` walker arm (postfix
//! `raise X unless cond` — stranded by #44's R-1 port), (2) the
//! `Model::helpers` visibility split (hook targets are conventionally
//! private), (3) one-`Callback`-per-symbol for multi-target declarations
//! (`before_save :a, :b, :c` — issue.rb declares four in one statement).
//! This file references `Model::helpers`, so against an unfixed ruff `main`
//! it fails to COMPILE — the loudest possible gate.
//!
//! ## MEASURED — 2026-07-06, Redmine corpus, fixed walker
//!
//! hooks 114 · arm 62 · **PASS 58 (93.5%)** / FAIL 4 (6.5%: self-feedback 3 +
//! write+raise 1 — exactly the two pre-registered shapes, no third) ·
//! unresolved: no-facts 43 (targets defined in concerns/`lib/` modules
//! outside `app/models`, the walker's scope boundary — excluded, never
//! counted as PASS) + blocks 8 · read-only 1 · guard-pure 0 (Redmine's
//! guards live in `validate` methods, out of callback scope by design).
//! Verdict vs pre-registration: 93.5% ≥ 80% → **supports
//! D-ACCIDENTAL-IMPERATIVE `[H]` → `[G]`**, one point under the Odoo
//! control's 94.9% — the imperative test population behaves like the
//! declarative control, which is precisely the discovery's claim.
//!
//! **Tail characterization (bodies read in source):** the 4 FAIL hooks are
//! NOT ORM-write noise — 2× encoding-membrane sanitizer
//! (`Change#replace_invalid_utf8_of_path`, `Changeset#before_create_cs`:
//! idempotent UTF-8 repair guarding the save; the imperative spelling of
//! Rails `normalizes`, which ruff already harvests declaratively), 1×
//! schema-default surrogate (`User#set_mail_notification`: write-if-blank
//! backfill for a config-sourced default DDL doesn't carry), 1× compensating
//! transaction (`WikiContentVersion#page_update_after_destroy`: manual
//! revert-or-destroy cascade + Rollback — the one genuinely essential
//! imperative). True essential residue ≈ 1/62 (1.6%); the rest are refugees
//! from strata OGAR already owns (schema-stratum defaults, normalizer/Guard
//! recipes). Falsifiable OP prediction: OpenProject's migration-DSL-only
//! schema should push MORE of this shape into hooks — check on the OP
//! corpus re-run.

use std::collections::BTreeSet;
use std::path::Path;

use ruff_ruby_spo::extract_app_with;

/// Lifecycle phases counted as hooks (the Rails `before_*`/`after_*`/
/// `around_*` family, incl. the validation-boundary and commit-boundary
/// phases). `validate` method-style declarations are Validation entries, not
/// callbacks, and are deliberately out of scope here — the guard family
/// enters through `before_validation`-style hooks and in-body
/// `raise`/`errors.add`.
const LIFECYCLE_PHASES: &[&str] = &[
    "before_validation",
    "after_validation",
    "before_save",
    "around_save",
    "after_save",
    "before_create",
    "around_create",
    "after_create",
    "before_update",
    "around_update",
    "after_update",
    "before_destroy",
    "around_destroy",
    "after_destroy",
    "after_commit",
    "after_create_commit",
    "after_update_commit",
    "after_destroy_commit",
    "after_save_commit",
    "after_rollback",
    "after_initialize",
    "after_find",
    "after_touch",
];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    /// Block/lambda-bodied hooks — no named target to resolve. Unresolved.
    blocks: usize,
    /// Named target with no harvested def (concern/plugin/`lib/` module) or
    /// a def with zero captured facts. Unresolved.
    no_facts: usize,
    self_feedback: usize,
    write_raise: usize,
    guard_pure: usize,
    compute_pure: usize,
    read_only: usize,
}

impl Counts {
    fn arm(&self) -> usize {
        self.guard_pure + self.compute_pure + self.self_feedback + self.write_raise
    }
    fn pass(&self) -> usize {
        self.guard_pure + self.compute_pure
    }
    fn fail(&self) -> usize {
        self.self_feedback + self.write_raise
    }
    fn total(&self) -> usize {
        self.arm() + self.blocks + self.no_facts + self.read_only
    }
}

fn triage(corpus: &Path, ns: &str) -> Counts {
    let graph = extract_app_with(corpus, ns);
    let mut c = Counts::default();
    for m in &graph.models {
        for cb in &m.callbacks {
            if !LIFECYCLE_PHASES.contains(&cb.phase.as_str()) {
                continue;
            }
            if cb.target == "<block>" {
                c.blocks += 1;
                continue;
            }
            // Hook bodies are conventionally private → search actions AND
            // helpers (the `Model::helpers` split exists for exactly this).
            let Some(f) = m
                .functions
                .iter()
                .chain(m.helpers.iter())
                .find(|f| f.name == cb.target)
            else {
                c.no_facts += 1;
                continue;
            };
            let r: BTreeSet<&str> = f.reads.iter().map(String::as_str).collect();
            let w: BTreeSet<&str> = f.writes.iter().map(String::as_str).collect();
            let mutating = !f.writes.is_empty() || !f.calls.is_empty();
            if r.is_empty() && !mutating && f.raises.is_empty() {
                c.no_facts += 1;
            } else if r.intersection(&w).next().is_some() {
                c.self_feedback += 1;
            } else if mutating && !f.raises.is_empty() {
                c.write_raise += 1;
            } else if !f.raises.is_empty() {
                c.guard_pure += 1;
            } else if mutating {
                c.compute_pure += 1;
            } else {
                c.read_only += 1;
            }
        }
    }
    c
}

#[test]
fn rails_test_leg_body_triage() {
    let Some(src) = std::env::var_os("RAILS_CORPUS_SRC") else {
        eprintln!(
            "body_triage_probe: RAILS_CORPUS_SRC not set — skipping the F17 \
             Rails test leg (set it to a Rails app root, e.g. a Redmine or \
             OpenProject checkout)."
        );
        return;
    };
    let ns = std::env::var("RAILS_CORPUS_NS").unwrap_or_else(|_| "redmine".to_string());
    let corpus = Path::new(&src);
    let c = triage(corpus, &ns);

    let arm = c.arm();
    let resolved_pct = |n: usize| 100.0 * n as f64 / arm as f64;
    eprintln!("== F17 Rails test leg — body triage ({ns} corpus) ==");
    eprintln!("hooks total (lifecycle, phase-filtered): {}", c.total());
    eprintln!(
        "verb-classes: guard-pure {} / compute-pure {} / self-feedback {} / write+raise {} / read-only {} / no-facts {} / blocks {}",
        c.guard_pure, c.compute_pure, c.self_feedback, c.write_raise, c.read_only, c.no_facts, c.blocks
    );
    eprintln!(
        "behavioural arm {}: PASS {} ({:.1}%) order-free recoverable / FAIL {} ({:.1}%) order-dependent tail",
        arm,
        c.pass(),
        resolved_pct(c.pass()),
        c.fail(),
        resolved_pct(c.fail()),
    );
    eprintln!(
        "caveats: calls-dispatch field-sets unknown (tail understated); reads_field is Inferred, \
         bare condition reads over-capture (self-feedback overstated). Cross-hook order not counted."
    );

    // Sanity floor: a real corpus must produce a meaningful population.
    assert!(
        c.total() >= 20,
        "corpus produced only {} lifecycle hooks — wrong path or harvest regression",
        c.total()
    );
    assert!(arm > 0, "behavioural arm is empty — harvest regression");

    // ── Drift fuses — pinned from the MEASURED 2026-07-06 Redmine run
    // (fixed walker; see module docs). Guarded on the corpus signature so
    // other corpora print their decomposition fuse-free.
    if ns == "redmine" && c.total() == 114 {
        assert_eq!(
            c,
            Counts {
                blocks: 8,
                no_facts: 43,
                self_feedback: 3,
                write_raise: 1,
                guard_pure: 0,
                compute_pure: 58,
                read_only: 1,
            },
            "Redmine drift fuse tripped — the verb-class decomposition moved"
        );
    }
}


// ─────────────────────────────────────────────────────────────────────────
// PROBE-OGAR-RECIPE-CODEBOOK — the "rolling bucket" refinement of F17.
//
// The deduction (operator, `[shape]{shape×lift×fuzzy}[shape]`): each hook is a
// FUZZY encoding of a canonical recipe that ALREADY EXISTS in the lifted
// codebook. So fingerprint each body by its (W,R,X,C) fact-set and correlate
// to the nearest recipe centroid — GENERIC over any AR frontend (Rails/Odoo),
// because the centroids are pure fact-set predicates, zero language tokens.
//
// This REFINES the coarse F17 triage: the coarse `self-feedback` bucket
// (R∩W≠∅ → FAIL) is itself fuzzy — it cannot tell an idempotent self-map
// (order-free) from genuine accumulation (order-dependent). Rolling the coarse
// FAILs through the recipe codebook recovers the idempotent ones and leaves
// only the irreducible imperative core ("won the game").
//
// The recipe codebook (centroids, first-match — the "existing lifting" each
// fuzzy body correlates with):
//   Compensate  C∧X          manual txn (rollback) — NO recipe, essential
//   Cascade     C, ¬X        relation.method dispatch → `dependent:` / assoc cb
//   Guard       X, ¬W        abort criteria          → validation
//   WriteRaise  W∧X          partial-write escape    — essential
//   Compute     W⊄R (fresh)  derive target≠inputs    → `emitted_by` compute edge
//   SelfMap     W⊆R          idempotent self-transform→ `normalizes` | default
//   Observe     R only       read-only observer      → excluded from arm
//
// MEASURED 2026-07-06 (Redmine, fixed walker) — arm 62:
//   Cascade 46 · Compute 13 · SelfMap 2 · Compensate 1
//   recoverable 61/62 = 98.4% (upper) — the coarse FAIL 4 rolls to
//   {SelfMap 2 + Compute 1 recovered} + {Compensate 1 essential}.
//   Authoritative-only lower bound (drop the Inferred-`calls` Cascade bucket
//   from num+denom): 15/16 = 93.75%. Irreducible = 1 Compensate in BOTH.
//
// The JITTER CODEBOOK (residuals the current fact-set cannot fully resolve —
// each names one more fact ruff must capture to close the game):
//   J1 SelfMap degeneracy — `normalizes` vs schema-default are identical under
//      (W,R); splitting them needs the GUARD PREDICATE fact (`x.blank?` ⇒
//      default; pure transform ⇒ normalize). Both order-free, so PASS is
//      unaffected — only the emit TARGET differs.
//   J2 Cascade rests on Inferred `calls` (receiver.method); the residual is the
//      receiver→`dependent:`-kind codebook (page.destroy, line_ids.update_all).
//      Hence the 93.75/98.4 band.
//   J3 Composite body — `Changeset#before_create_cs` is normalize(committer,
//      comments) + compute(user) in one hook; recipe = the SET, not one entry.
//      Order-free (both sub-recipes are), so it stays recoverable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Recipe {
    Compensate,
    Cascade,
    Guard,
    WriteRaise,
    Compute,
    SelfMap,
    Observe,
    Empty,
}

fn recipe_of(f: &ruff_spo_triplet::Function) -> Recipe {
    let r: BTreeSet<&str> = f.reads.iter().map(String::as_str).collect();
    let w: BTreeSet<&str> = f.writes.iter().map(String::as_str).collect();
    let (hw, hr, hx, hc) = (
        !w.is_empty(),
        !r.is_empty(),
        !f.raises.is_empty(),
        !f.calls.is_empty(),
    );
    let w_sub_r = hw && w.iter().all(|x| r.contains(x));
    let w_fresh = w.iter().any(|x| !r.contains(x));
    if hc && hx {
        Recipe::Compensate
    } else if hc {
        Recipe::Cascade
    } else if hx && !hw {
        Recipe::Guard
    } else if hx && hw {
        Recipe::WriteRaise
    } else if w_fresh {
        Recipe::Compute
    } else if w_sub_r {
        Recipe::SelfMap
    } else if hr {
        Recipe::Observe
    } else {
        Recipe::Empty
    }
}

#[test]
fn rails_recipe_codebook_correlation() {
    let Some(src) = std::env::var_os("RAILS_CORPUS_SRC") else {
        eprintln!("recipe_codebook: RAILS_CORPUS_SRC not set — skipping.");
        return;
    };
    let ns = std::env::var("RAILS_CORPUS_NS").unwrap_or_else(|_| "redmine".to_string());
    let graph = extract_app_with(Path::new(&src), &ns);

    let mut hist: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for m in &graph.models {
        for cb in &m.callbacks {
            if !LIFECYCLE_PHASES.contains(&cb.phase.as_str()) || cb.target == "<block>" {
                continue;
            }
            let Some(f) = m
                .functions
                .iter()
                .chain(m.helpers.iter())
                .find(|f| f.name == cb.target)
            else {
                continue;
            };
            let key = match recipe_of(f) {
                Recipe::Compensate => "Compensate",
                Recipe::Cascade => "Cascade",
                Recipe::Guard => "Guard",
                Recipe::WriteRaise => "WriteRaise",
                Recipe::Compute => "Compute",
                Recipe::SelfMap => "SelfMap",
                Recipe::Observe => "Observe",
                Recipe::Empty => "Empty",
            };
            *hist.entry(key).or_default() += 1;
        }
    }
    let n = |k: &str| *hist.get(k).unwrap_or(&0);
    // Behavioural arm = everything with facts except pure Observe/Empty.
    let arm = n("Cascade") + n("Compute") + n("SelfMap") + n("Guard")
        + n("Compensate") + n("WriteRaise");
    let essential = n("Compensate") + n("WriteRaise");
    let recoverable = arm - essential;
    let upper = 100.0 * recoverable as f64 / arm as f64;
    // Lower bound: drop the Inferred-calls Cascade bucket from num AND denom.
    let arm_auth = arm - n("Cascade");
    let lower = 100.0 * (arm_auth - essential) as f64 / arm_auth as f64;
    eprintln!("== F17 recipe-codebook (rolling bucket) — {ns} ==");
    for (k, v) in &hist {
        eprintln!("  {k:>11} {v}");
    }
    eprintln!(
        "arm {arm}: recoverable {recoverable}/{arm} = {upper:.1}% (upper) .. {lower:.1}% \
         (Authoritative-only, Cascade dropped); irreducible essential = {essential} \
         (Compensate {} + WriteRaise {})",
        n("Compensate"),
        n("WriteRaise"),
    );

    assert!(arm >= 15, "arm too small — harvest regression");
    // WON: the irreducible core is ONLY the essential kind — no recoverable
    // recipe left stranded in a FAIL bucket. This is the "won the game" gate.
    assert!(
        essential <= 2,
        "irreducible essential core grew to {essential} — a new order-dependent \
         shape appeared (finding, not noise — characterize it)"
    );

    // Drift fuse — pinned from the MEASURED 2026-07-06 Redmine run.
    if ns == "redmine" && arm == 62 {
        assert_eq!(n("Cascade"), 46, "Cascade bucket moved");
        assert_eq!(n("Compute"), 13, "Compute bucket moved");
        assert_eq!(n("SelfMap"), 2, "SelfMap bucket moved");
        assert_eq!(n("Compensate"), 1, "essential Compensate moved");
        assert_eq!(n("WriteRaise"), 0, "WriteRaise bucket moved");
    }
}
