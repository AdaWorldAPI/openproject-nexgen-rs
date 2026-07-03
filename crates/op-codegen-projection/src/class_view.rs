//! `class_view` — the ERB render leg, wired to **classview × askama +
//! bitmask** (operator equation, 2026-07-03).
//!
//! The kit's [`ogar_render_askama`] emitters are explicitly *"modelled on
//! Redmine's `_list.html.erb` / `_form.html.erb` / `show.html.erb`"* and
//! build their columns from `Class.attributes`, with the documented
//! contract: **"the columns are pre-shaped by the Rust side; the template
//! iterates."** So the ERB column selection — Redmine's
//! `Query#column_names`, the *bitmask* — is the consumer's to apply,
//! *before* render. This module is that application, composing three
//! vendored surfaces **unmodified**:
//!
//! ```text
//!   classview  =  ogar_vocab::Class                 (the AR shape, ordered attributes)
//!   bitmask    =  lance_graph_contract::…::FieldMask (which attribute positions show)
//!   askama     =  ogar_render_askama::render(…)      (the ERB template — the XSLT)
//!
//!   ERB view   =  render( select_by_mask(class, mask), kind )
//! ```
//!
//! # The one honest seam (`[H]`, cite-don't-fabricate)
//!
//! [`FieldMask`]'s bit basis is documented (`class_view.rs`:55) as "the
//! N-th field in the class's **ordered field list**". Here that list is
//! `ogar_vocab::Class::attributes` in declaration order — the natural
//! ordered field list of the calcified AR shape. The *contract's* own
//! `ClassView::fields()` sources its ordering from the OGIT/ontology cache
//! (`lance-graph-ontology`, not vendored here). The two orderings are
//! meant to converge once the upstream lift
//! (`ogar_from_ruff::lift_model_graph`) populates the `Class` from the same
//! basis — until then, this module keys the mask on the `Class`'s own
//! attribute order. Convergence of the two bit-bases is an upstream
//! alignment question, not settled here. No canon is asserted.

use lance_graph_contract::class_view::FieldMask;
use ogar_render_askama::{render, ArtifactKind};
use ogar_vocab::Class;

/// Apply the ERB **bitmask** to a classview: return a copy of `class`
/// keeping only the attributes whose position bit is set in `mask`.
///
/// The bit basis is `class.attributes` in declaration order (position
/// `i` = bit `i`; see the module `[H]` seam). Off-bit attributes are
/// dropped so the downstream emitter's column synthesis
/// (`for attr in &class.attributes`) yields exactly the selected columns —
/// the Redmine `Query#column_names` behaviour, pure and consumer-side.
///
/// Only `attributes` are masked (the emitter's inline-column source);
/// `associations` and the other class facets pass through untouched —
/// a follow-up can extend the mask basis to them once a view needs it.
#[must_use]
pub fn select_by_mask(class: &Class, mask: FieldMask) -> Class {
    let mut reduced = class.clone();
    reduced.attributes = class
        .attributes
        .iter()
        .enumerate()
        .filter(|(i, _)| mask.has(*i as u8))
        .map(|(_, a)| a.clone())
        .collect();
    reduced
}

/// Render `class` as an ERB view (`kind`) with only the `mask`-selected
/// attributes — `render(select_by_mask(class, mask), kind)`, the whole
/// equation in one call.
///
/// # Errors
///
/// Propagates the kit's `askama::Error` (only fires on an emitter arity
/// mismatch, which the codebook-driven path does not hit).
pub fn render_masked(
    class: &Class,
    kind: ArtifactKind,
    mask: FieldMask,
) -> Result<String, askama::Error> {
    render(&select_by_mask(class, mask), kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::project_role;

    /// The bitmask selects attributes: `project_role` has 3
    /// (`name`, `position`, `permissions`); masking to positions [0, 1]
    /// keeps name + position and drops permissions.
    #[test]
    fn select_by_mask_keeps_only_set_positions() {
        let full = project_role();
        assert_eq!(full.attributes.len(), 3, "exemplar has 3 attributes");

        let reduced = select_by_mask(&full, FieldMask::from_positions(&[0, 1]));
        let names: Vec<&str> = reduced.attributes.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["name", "position"], "off-bit `permissions` dropped");

        // EMPTY → no attributes; FULL → all three, untouched order.
        assert!(select_by_mask(&full, FieldMask::EMPTY)
            .attributes
            .is_empty());
        let all = select_by_mask(&full, FieldMask::from_positions(&[0, 1, 2]));
        assert_eq!(all.attributes.len(), 3);
    }

    /// FALSIFIER — the whole equation end-to-end: the same classview
    /// renders through the ERB askama list template, and the bitmask
    /// controls which columns appear. `name` is bit 0 (always kept here),
    /// `permissions` is bit 2 (masked out) — so the masked ERB list has a
    /// `name` header but NOT a `permissions` header, while the full render
    /// has both. Proves `ERB = classview × askama + bitmask`, consumer-side,
    /// vendored crates unmodified.
    #[test]
    fn erb_list_columns_follow_the_bitmask() {
        let class = project_role();

        let full = render_masked(
            &class,
            ArtifactKind::HtmlListView,
            FieldMask::from_positions(&[0, 1, 2]),
        )
        .expect("full render");
        assert!(
            full.contains(">Name<") || full.contains("name"),
            "full has name col:\n{full}"
        );
        assert!(
            full.contains("permissions"),
            "full has permissions col:\n{full}"
        );

        let masked = render_masked(
            &class,
            ArtifactKind::HtmlListView,
            FieldMask::from_positions(&[0, 1]),
        )
        .expect("masked render");
        assert!(masked.contains("name"), "masked keeps name col:\n{masked}");
        assert!(
            !masked.contains("permissions"),
            "masked DROPS permissions col — the bitmask selected it out:\n{masked}"
        );
    }
}
