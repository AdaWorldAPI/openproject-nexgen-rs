//! WAVE 0 — the harvest-denominator smoke probe.
//!
//! NO code emission here; this only establishes counts (the denominators
//! the later transpile waves get measured against). Runs the V3 harvest
//! pipeline (`extract_app_with_schema` -> `filter_to_core` ->
//! `compile_op::<OpenProjectPort>`) over a real OpenProject checkout and
//! prints a ledger of counts, with drift-fuse assertions pinned to the
//! FIRST measured run.
//!
//! ## Corpus (env-gated, self-skipping — ruff #44 house style, same env
//! var `ruff_openproject`'s `body_triage_probe.rs` uses)
//!
//! ```sh
//! RAILS_CORPUS_SRC=/home/user/openproject \
//!     cargo test -p op-codegen-pipeline --features ogar-emit \
//!     --test harvest_denominator_probe -- --nocapture
//! ```
//!
//! Without `RAILS_CORPUS_SRC` the probe prints a skip note and exits
//! green, so CI without a corpus checkout is unaffected.

#![cfg(feature = "ogar-emit")]

use std::path::Path;

use op_codegen_pipeline::ogar_consumer::compile_op;
use op_codegen_pipeline::{filter_to_core, CORE_V3_RESOURCES, NAMESPACE};
use ogar_vocab::ports::OpenProjectPort;

#[test]
fn harvest_denominator_probe() {
    let Some(src) = std::env::var_os("RAILS_CORPUS_SRC") else {
        eprintln!(
            "harvest_denominator_probe: RAILS_CORPUS_SRC not set — skipping WAVE 0 \
             smoke probe (set it to a Rails app root, e.g. an OpenProject checkout)."
        );
        return;
    };
    let corpus = Path::new(&src);

    // extract_app_with_schema -> ModelGraph (pre-filter) -> filter_to_core
    // -> compile_op::<OpenProjectPort> -> Vec<CompiledClass>.
    let (mut graph, _report) = ruff_ruby_spo::extract_app_with_schema(corpus, NAMESPACE);
    let total_models_extracted = graph.models.len();

    filter_to_core(&mut graph);
    let compiled = compile_op::<OpenProjectPort>(&graph);

    let core_classes = compiled.len();
    let sum_fields: usize = compiled
        .iter()
        .map(|cc| cc.class.attributes.len() + cc.class.associations.len())
        .sum();
    let sum_actions: usize = compiled.iter().map(|cc| cc.actions.len()).sum();
    let workpackage_fields = compiled
        .iter()
        .find(|cc| cc.class.name == "WorkPackage")
        .map(|cc| cc.class.attributes.len() + cc.class.associations.len())
        .unwrap_or(0);

    // Census: which curated core resources survived extract+compile and
    // which DROPPED. The first measurement compiled 16 of 18 — name the 2
    // gaps so W2 chases them, don't bury them behind a bare count.
    let compiled_names: std::collections::BTreeSet<&str> =
        compiled.iter().map(|cc| cc.class.name.as_str()).collect();
    let dropped: Vec<&str> = CORE_V3_RESOURCES
        .iter()
        .copied()
        .filter(|r| !compiled_names.contains(r))
        .collect();

    eprintln!("== WAVE 0 — harvest-denominator smoke probe (OpenProject corpus) ==");
    eprintln!("total_models_extracted (pre-filter_to_core): {total_models_extracted}");
    eprintln!("core_classes (post-filter_to_core, compiled): {core_classes} of {}", CORE_V3_RESOURCES.len());
    eprintln!("core resources DROPPED (curated but not extract+compiled): {dropped:?}");
    eprintln!("sum_fields (Σ attributes+associations over compiled classes): {sum_fields}");
    eprintln!("sum_actions (Σ .actions.len() over compiled classes): {sum_actions}");
    eprintln!("workpackage_fields (WorkPackage attributes+associations): {workpackage_fields}");

    // Re-pinned to the FIRST real OpenProject-corpus measurement (re-pinned 2026-07-07: corpus bumped
    // to upstream dev d333d164, +373 commits — 945->950 models, WP 40->42). The earlier 18/113 were pre-corpus
    // estimates; measured truth is 16 compiled of 18 curated (2 dropped, see
    // `dropped` above — a W2 input) and a full-tree extract of 945 models
    // (walks modules/ + nested, not just app/models/*.rb).
    assert_eq!(
        core_classes, 16,
        "core_classes drifted from the pinned 16-of-18 baseline; check `dropped` — a \
         curated resource stopped surviving extract+compile"
    );
    assert!(sum_actions > 0, "sum_actions is zero — harvest regression (no ActionDefs lifted)");

    // DRIFT FUSE — EXPECTED to blow when the W3 migration-replay schema
    // stratum lands (it changes both the model census and WorkPackage's
    // field count) — a signal to re-measure and re-pin, not to silently patch.
    assert_eq!(
        total_models_extracted, 950,
        "total_models_extracted drifted — re-measure and re-pin for the new baseline \
         (expected to blow when the W3 migration-replay lands)"
    );
    assert_eq!(
        workpackage_fields, 42,
        "workpackage_fields drifted — re-measure and re-pin for the new baseline \
         (expected to blow when the W3 migration-replay lands)"
    );
}
