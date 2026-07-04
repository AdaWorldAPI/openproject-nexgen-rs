//! `class_view` — **focus of attention** over the classview.
//!
//! The mask does not drop fields (that was a compress, and wrong). It keeps
//! **100% of the class** and marks which fields are *in focus* — an attention
//! overlay, never a filter. Off-focus fields stay present with
//! `in_focus = false`.
//!
//! This is a straight knowledge-transfer of the 17-year-old Redmine
//! `_list.html.erb` / `Query` pattern: `available_columns` is always the full
//! set; `column_names` is the *focus*, not a deletion. It is the same shape
//! the contract already ships — `ClassView::project` yields `(field, present)`
//! for **every** field — transferred onto `ogar_vocab::Class`. Lazy,
//! borrowing, nothing materialized: the mask is a lens of attention above the
//! whole class.

use lance_graph_contract::class_view::FieldMask;
use ogar_vocab::{Attribute, Class};

/// Focus the class through the mask: yield **every** attribute paired with
/// its `in_focus` bit, in class order. 100% of the class is preserved — the
/// mask is the attention overlay, not a filter. Bit position `i` =
/// `class.attributes[i]`. Lazy and borrowing; the class is untouched and
/// nothing is materialized.
pub fn focus(class: &Class, mask: FieldMask) -> impl Iterator<Item = (&Attribute, bool)> {
    class
        .attributes
        .iter()
        .enumerate()
        .map(move |(i, a)| (a, mask.has(i as u8)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::project_role;

    /// Attention keeps 100% and marks focus — it never drops a field.
    #[test]
    fn focus_keeps_the_whole_class_and_marks_attention() {
        let class = project_role(); // name, position, permissions

        // Focus [0, 1]: ALL THREE fields are still yielded — permissions is
        // simply out of focus (`false`), not removed.
        let focused: Vec<(&str, bool)> = focus(&class, FieldMask::from_positions(&[0, 1]))
            .map(|(a, on)| (a.name.as_str(), on))
            .collect();
        assert_eq!(
            focused,
            [("name", true), ("position", true), ("permissions", false)],
            "100% preserved; the mask marks focus, it does not compress"
        );

        // The class is untouched (nothing materialized, nothing dropped).
        assert_eq!(class.attributes.len(), 3);

        // EMPTY focus → still all three fields, all out of focus.
        let none: Vec<bool> = focus(&class, FieldMask::EMPTY).map(|(_, on)| on).collect();
        assert_eq!(none, [false, false, false]);

        // Full focus → all three in focus. Attention yields the same 3 rows
        // regardless of the mask; only the focus bits change.
        let all: Vec<bool> = focus(&class, FieldMask::from_positions(&[0, 1, 2]))
            .map(|(_, on)| on)
            .collect();
        assert_eq!(all, [true, true, true]);
    }
}
