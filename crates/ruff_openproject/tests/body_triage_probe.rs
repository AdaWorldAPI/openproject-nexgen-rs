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
