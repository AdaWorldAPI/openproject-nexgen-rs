//! Probe: the ERB `ViewFieldSet` harvest mints the **byte-identical**
//! ViewFilter mask that op-server's board hand-authors — the field twin
//! of `nav_harvest_probe` (that one proved the *jump* half; this proves
//! the *view* half of "one brick, three skins").
//!
//! Drives a synthetic ERB fixture (`tests/fixtures/rails_views/`, no
//! proprietary source) whose view references exactly the board columns
//! (`wp.subject` / `wp.project` / `wp.done_ratio`, plus an unknown ident
//! for the honest denominator) through
//! `op_codegen_pipeline::field_harvest` and asserts:
//!   1. the harvested mask == `from_universe_present(basis, BOARD_ORDER)`
//!      — the mask `op-server::board::WP_BOARD_ORDER` mints by hand
//!      (op-server is a binary crate, so the order list is mirrored here,
//!      pinned by this probe exactly like `nav_harvest_probe` pins the
//!      expected screen pairs);
//!   2. the unknown ident lands in the honest denominator (`referenced`)
//!      without perturbing the mask;
//!   3. the ledger accounts for the fixture.

use std::path::PathBuf;

use lance_graph_contract::class_view::WideFieldMask;
use ogar_vocab::project_work_item;
use op_codegen_pipeline::field_harvest::harvest_view_masks;
use ruff_ruby_spo::{extract_view_field_sets, ViewTarget};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rails_views")
}

/// Mirror of `op-server::board::WP_EXTRA_FIELDS` (op-server is a bin
/// crate; the probe pins the same list, as `nav_harvest_probe` pins the
/// expected klickweg pairs).
const WP_EXTRA_FIELDS: &[&str] = &[
    "subject",
    "start_date",
    "due_date",
    "estimated_hours",
    "done_ratio",
    "description",
    "status_id",
    "type_id",
    "priority_id",
    "assigned_to_id",
    "lock_version",
];

/// Mirror of `op-server::board::WP_BOARD_ORDER` — the hand-authored skin
/// this probe proves the harvest reproduces.
const WP_BOARD_ORDER: &[&str] = &["subject", "project", "done_ratio"];

/// Mirror of `op-server::board::basis()` over `project_work_item()`:
/// attributes, then associations, then the extra leaf fields.
fn wp_basis() -> Vec<String> {
    let class = project_work_item();
    let mut out: Vec<String> = class.attributes.iter().map(|a| a.name.clone()).collect();
    out.extend(class.associations.iter().map(|a| a.name.clone()));
    for name in WP_EXTRA_FIELDS {
        if !out.iter().any(|b| b == name) {
            out.push((*name).to_string());
        }
    }
    out
}

fn wp_target(basis: &[String]) -> ViewTarget {
    ViewTarget {
        model: "WorkPackage".to_string(),
        receivers: vec!["wp".to_string(), "@work_package".to_string()],
        fields: basis.to_vec(),
    }
}

/// The harvested ERB view mints the byte-identical mask the board's
/// hand-authored `WP_BOARD_ORDER` skin mints — harvest and constants are
/// the same brick, so the constants are provably the harvest's mirror.
#[test]
fn harvested_view_mask_equals_hand_authored_board_mask() {
    let basis = wp_basis();
    let (masks, report) =
        harvest_view_masks(&fixture_root(), &[wp_target(&basis)], &basis).unwrap();

    assert_eq!(report.erb_files, 1, "{report:?}");
    assert_eq!(masks.len(), 1, "{masks:?}");
    let harvested = &masks[0];
    assert_eq!(harvested.view, "app/views/work_packages/index.html.erb");
    assert_eq!(harvested.resource, "WorkPackage");

    let universe: Vec<&str> = basis.iter().map(String::as_str).collect();
    let hand_authored = WideFieldMask::from_universe_present(&universe, WP_BOARD_ORDER).unwrap();
    assert_eq!(
        harvested.mask, hand_authored,
        "ERB-harvested mask must be byte-identical to the WP_BOARD_ORDER mint"
    );
    assert_eq!(harvested.mask.count(), 3);
}

/// The unknown ident (`wp.frobnicate`) is counted in the honest
/// denominator (`referenced`) but does NOT perturb the minted mask —
/// closed-vocab discipline end to end.
#[test]
fn unknown_ident_stays_out_of_the_mask() {
    let basis = wp_basis();
    let sets = extract_view_field_sets(&fixture_root(), &[wp_target(&basis)]);
    assert_eq!(sets.len(), 1);
    assert!(
        sets[0].referenced.iter().any(|r| r == "frobnicate"),
        "raw denominator must see the unknown ident: {:?}",
        sets[0].referenced
    );
    assert!(
        !sets[0].fields.iter().any(|f| f == "frobnicate"),
        "closed vocab must exclude it from fields: {:?}",
        sets[0].fields
    );
}
