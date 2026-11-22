//! Runs the ruff UI/nav harvests (nav, menu, actions, widgets, view field sets)
//! over a REAL OpenProject checkout and prints each ledger, so the synthetic
//! fixtures are not the only evidence for those arms.
//!
//!   OPENPROJECT_SRC=/path/to/openproject \
//!     cargo run -p op-codegen-pipeline --example real_corpus_harvest
//!
//! Prints counts only. A raw count with few resolved edges means the closed
//! vocabulary or the single-line scan misses real idioms; the output is the
//! evidence, not a pass/fail.

use std::path::PathBuf;

use op_codegen_pipeline::nav_harvest::{
    harvest_klickweg_with_report, harvest_menu_klickweg_with_report,
};
use ruff_ruby_spo::{extract_action_edges_with_report, extract_widget_edges_with_report};

fn main() {
    let Ok(src) = std::env::var("OPENPROJECT_SRC") else {
        println!("OPENPROJECT_SRC unset; nothing to do");
        return;
    };
    let root = PathBuf::from(src);

    let (nav, nav_r) = harvest_klickweg_with_report(&root);
    println!("nav     edges={:<5} {nav_r:?}", nav.len());

    let (menu, menu_r) = harvest_menu_klickweg_with_report(&root);
    println!("menu    edges={:<5} {menu_r:?}", menu.len());

    for sub in ["app/views", "app/components"] {
        let views = root.join(sub);
        let (actions, a_r) = extract_action_edges_with_report(&views);
        println!("actions[{sub}] edges={:<5} {a_r:?}", actions.len());
        let (widgets, w_r) = extract_widget_edges_with_report(&views);
        println!("widgets[{sub}] edges={:<5} {w_r:?}", widgets.len());
    }
}
