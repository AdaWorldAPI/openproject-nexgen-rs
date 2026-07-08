//! Field-set harvest bridge — the ERB **ViewFilter source**, the field
//! twin of [`crate::nav_harvest`].
//!
//! [`crate::nav_harvest`] harvests *which screen links to which* (the
//! klickweg, the jump half). THIS module harvests *which fields a view
//! shows* — the ERB `ViewFieldSet` (`ruff_ruby_spo::extract_view_field_sets`,
//! the presentation-tier "detected config becomes data" harvest) minted
//! into the ONE mask brick via
//! [`WideFieldMask::from_universe_present`]`(basis, fields)`.
//!
//! That mask is the **ViewFilter**: the view never pushes data — it is a
//! projection over the in-memory `ClassView` row, and the harvested field
//! set is the *view* operand of `rbac ∩ present ∩ view`
//! (`op-server::viewfilter::view_filter`). op-server's skins mint the same
//! mask from hand-authored order constants today (`WP_BOARD_ORDER` …);
//! this bridge is the harvested source those constants mirror — the
//! `field_harvest_probe` proves an ERB view referencing the board's
//! fields mints the **byte-identical** mask.
//!
//! One brick, three skins: ERB (Rails), askama (op-server), Jinja
//! (`ruff_python_spo::extract_template_field_sets`) all feed the same
//! `from_universe_present` mint, so a view's projection is interchangeable
//! across all three renderers.

use lance_graph_contract::class_view::{WideFieldMask, WideMaskCapError};
use ruff_ruby_spo::{
    extract_view_field_sets_with_report, ViewFieldSet, ViewScanReport, ViewTarget,
};
use std::path::Path;

/// Mint the ViewFilter *view* mask for one harvested [`ViewFieldSet`]
/// over a class `basis` (the position universe — `ClassView` field order).
/// Bit `i` is set iff `basis[i]` is one of the fields the view references
/// — the identical membership rule every consumer mints by.
///
/// # Errors
///
/// [`WideMaskCapError::UniverseExceedsSocCap`] for a >256-field basis —
/// the SoC split signal (see `op-server::viewfilter::bucketized_masks`
/// for the rolling-bucket overflow path).
pub fn view_mask(basis: &[String], set: &ViewFieldSet) -> Result<WideFieldMask, WideMaskCapError> {
    let universe: Vec<&str> = basis.iter().map(String::as_str).collect();
    let present: Vec<&str> = set.fields.iter().map(String::as_str).collect();
    WideFieldMask::from_universe_present(&universe, &present)
}

/// One harvested skin: the view file, the resource it projects, and the
/// minted ViewFilter *view* mask.
#[derive(Debug, Clone, PartialEq)]
pub struct HarvestedViewMask {
    /// View file path relative to the views root (e.g.
    /// `"work_packages/index.html.erb"`).
    pub view: String,
    /// Model/resource name as harvested (e.g. `"WorkPackage"`).
    pub resource: String,
    /// The minted view mask over the class basis.
    pub mask: WideFieldMask,
}

/// Harvest every ERB view under `views_root` for `targets` and mint each
/// view's mask over `basis` — one [`HarvestedViewMask`] per non-empty
/// field set, plus the conservation ledger. The end-to-end
/// harvested-skin path: Rails views in, ViewFilter masks out.
///
/// # Errors
///
/// Propagates the first [`WideMaskCapError`] (a >256-field basis — the
/// SoC split signal; no partial output on a violated cap).
pub fn harvest_view_masks(
    views_root: &Path,
    targets: &[ViewTarget],
    basis: &[String],
) -> Result<(Vec<HarvestedViewMask>, ViewScanReport), WideMaskCapError> {
    let (sets, report) = extract_view_field_sets_with_report(views_root, targets);
    let mut out = Vec::with_capacity(sets.len());
    for set in &sets {
        out.push(HarvestedViewMask {
            view: set.view.clone(),
            resource: set.resource.clone(),
            mask: view_mask(basis, set)?,
        });
    }
    Ok((out, report))
}
