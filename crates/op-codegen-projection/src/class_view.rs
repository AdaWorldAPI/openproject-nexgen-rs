//! `class_view` — the mask as a **lazy lens**, not a materialization.
//!
//! A field mask selects at the point of consumption — the software analogue
//! of a SIMD masked-compress (`vpcompressd` / `_mm512_maskz_*`): it walks the
//! lanes and yields the on-bit ones, borrowing, allocating nothing. It does
//! **not** clone the class and rebuild a reduced one — that would *materialize
//! during thinking*, and the substrate's rule is the opposite: *"the meta-DTO
//! resolves; it does not store"* (`lance_graph_contract::class_view` header).
//! The class stays whole and agnostic; the mask is a lens above it.
//!
//! (The prior `select_by_mask -> Class` was a parse wearing a mask's name —
//! it collected a new `Vec` into a new `Class`. Reverted; this is the lens.)

use lance_graph_contract::class_view::FieldMask;
use ogar_vocab::{Attribute, Class};

/// A **lazy, borrowing** masked view of a class's attributes: yields
/// `&Attribute` for each on-bit position, in class order, allocating nothing
/// and leaving `class` untouched. The mask is applied *at iteration* — the
/// compress-select — never by reconstructing the class.
///
/// Bit position `i` = `class.attributes[i]` (the class's ordered field list,
/// per `FieldMask`'s documented basis). Composable: the caller can chain,
/// count, or feed it straight into a render that iterates fields, without a
/// materialized intermediate.
pub fn masked_attributes(
    class: &Class,
    mask: FieldMask,
) -> impl Iterator<Item = &Attribute> {
    class
        .attributes
        .iter()
        .enumerate()
        .filter(move |(i, _)| mask.has(*i as u8))
        .map(|(_, a)| a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::project_role;

    /// The lens selects the on-bit fields **by reference**, in order, and
    /// leaves the class whole — nothing materialized, nothing stored.
    #[test]
    fn masked_attributes_is_a_lazy_lens_not_a_reduction() {
        let class = project_role(); // name, position, permissions (3 attrs)

        // Mask [0, 1] → name, position; permissions (bit 2) gated off.
        let names: Vec<&str> = masked_attributes(&class, FieldMask::from_positions(&[0, 1]))
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["name", "position"]);

        // The class is UNTOUCHED — the lens didn't reduce or rebuild it.
        assert_eq!(class.attributes.len(), 3, "the lens must not mutate the class");

        // The yielded items BORROW from the class (same addresses), proving
        // no clone/materialization happened.
        let borrowed: Vec<*const Attribute> =
            masked_attributes(&class, FieldMask::from_positions(&[0, 2]))
                .map(|a| a as *const Attribute)
                .collect();
        assert_eq!(borrowed[0], &class.attributes[0] as *const Attribute, "yields &class.attributes[0]");
        assert_eq!(borrowed[1], &class.attributes[2] as *const Attribute, "yields &class.attributes[2]");

        // EMPTY → nothing; FULL positions → all three, in order, still borrowed.
        assert_eq!(masked_attributes(&class, FieldMask::EMPTY).count(), 0);
        let all: Vec<&str> = masked_attributes(&class, FieldMask::from_positions(&[0, 1, 2]))
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(all, ["name", "position", "permissions"]);
    }
}
