//! **APP‖class composition** — the high-u16 render-prefix machinery
//! (`docs/APP-CLASS-CODEBOOK-LAYOUT.md` §0, §4).
//!
//! A full 32-bit `classid` is two orthogonal halves:
//!
//! ```text
//! classid : u32  =  [ hi u16 : APP / render prefix ]  [ lo u16 : concept ]
//!                     0xAAAA (per-app ClassView lens)    0xDDCC (shared RBAC+ontology)
//! ```
//!
//! The per-port reserved prefix is [`PortSpec::APP_PREFIX`] (the §2 allocation
//! table as typed data); this module composes and decomposes the full id so
//! consumers re-export ONE source of the bit math instead of each
//! re-implementing `(prefix << 16) | concept`. `0x0000` is the shared
//! canonical core (every existing id is `0x0000_DDCC` — additive, invariant
//! I-APP1).
//!
//! Composing the high half is **reserving/stamping**, not minting a class_id
//! (§2: "reserving costs nothing") — concept (low-u16) mints stay gated on the
//! 5+3 codebook pass. The low half is the cross-app currency: concept/domain
//! routing reads it alone (`lance_graph_contract::classid_concept_domain`
//! does `… as u16`), so it is identical under every render prefix.

use crate::ports::PortSpec;

/// Compose a full render `classid` from an app `prefix` (high u16) and a
/// canonical `concept` id (low u16): `(prefix << 16) | concept`.
///
/// `render_classid(0x0001, 0x0102)` → `0x0001_0102` (OpenProject's
/// `project_work_item`); the Redmine twin `render_classid(0x0007, 0x0102)` →
/// `0x0007_0102` — same concept, different render lens.
#[must_use]
pub const fn render_classid(prefix: u16, concept: u16) -> u32 {
    ((prefix as u32) << 16) | (concept as u32)
}

/// Compose a render `classid` for a specific port, reading its reserved
/// [`PortSpec::APP_PREFIX`]: `render_classid_for::<OpenProjectPort>(concept)`
/// → `0x0001_DDCC`. This is the helper consumers call so the prefix and the
/// bit math both come from OGAR (one source of truth), not a local literal.
#[must_use]
pub fn render_classid_for<P: PortSpec>(concept: u16) -> u32 {
    render_classid(P::APP_PREFIX, concept)
}

/// The APP / render prefix — the **high u16** of a full `classid`. `0x0000`
/// ([the shared core]) is the abstract/default-`ClassView` anchor; a non-zero
/// value selects an app's render lens. This is the §4 `resolve_codebook`
/// routing key (`classid >> 16`).
///
/// [the shared core]: PortSpec::APP_PREFIX
#[must_use]
pub const fn app_of(classid: u32) -> u16 {
    (classid >> 16) as u16
}

/// The canonical concept id — the **low u16** of a full `classid`. The shared
/// RBAC + ontology + cross-app identity key; concept/domain routing reads only
/// this half, so it is identical for every render prefix.
#[must_use]
pub const fn concept_of(classid: u32) -> u16 {
    classid as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_ids;
    use crate::ports::{OdooPort, OpenProjectPort, RedminePort};

    #[test]
    fn render_classid_composes_the_two_halves() {
        assert_eq!(render_classid(0x0001, 0x0102), 0x0001_0102);
        assert_eq!(render_classid(0x0007, 0x0102), 0x0007_0102);
        assert_eq!(render_classid(0x0002, 0x0202), 0x0002_0202);
    }

    #[test]
    fn render_classid_for_reads_the_port_prefix() {
        // OpenProject (0x0001) and Redmine (0x0007) render the SAME concept
        // under different prefixes — "two renders, one concept" (§1).
        assert_eq!(
            render_classid_for::<OpenProjectPort>(class_ids::PROJECT_WORK_ITEM),
            0x0001_0102,
        );
        assert_eq!(
            render_classid_for::<RedminePort>(class_ids::PROJECT_WORK_ITEM),
            0x0007_0102,
        );
        assert_eq!(
            render_classid_for::<OdooPort>(class_ids::COMMERCIAL_DOCUMENT),
            0x0002_0202,
        );
    }

    #[test]
    fn app_of_and_concept_of_decompose() {
        let cid = render_classid(0x0005, class_ids::PATIENT); // Medcare patient
        assert_eq!(cid, 0x0005_0901);
        assert_eq!(app_of(cid), 0x0005);
        assert_eq!(concept_of(cid), class_ids::PATIENT);
    }

    #[test]
    fn roundtrip_over_prefixes_and_concepts() {
        for prefix in [0x0000u16, 0x0001, 0x0002, 0x0005, 0x0007] {
            for concept in [
                class_ids::PROJECT_WORK_ITEM,
                class_ids::BILLABLE_WORK_ENTRY,
                class_ids::COMMERCIAL_DOCUMENT,
                class_ids::PATIENT,
            ] {
                let cid = render_classid(prefix, concept);
                assert_eq!(app_of(cid), prefix);
                assert_eq!(concept_of(cid), concept);
            }
        }
    }

    #[test]
    fn core_prefix_is_additive_and_bit_identical() {
        // I-APP1/I-APP5: a core (hi=0x0000) classid equals the bare concept
        // widened to u32 — no renumber, no version cost.
        let core = render_classid(0x0000, class_ids::PROJECT_WORK_ITEM);
        assert_eq!(core, 0x0000_0102);
        assert_eq!(core, u32::from(class_ids::PROJECT_WORK_ITEM));
        assert_eq!(app_of(core), 0x0000);
        assert_eq!(concept_of(core), class_ids::PROJECT_WORK_ITEM);
    }

    #[test]
    fn render_prefix_never_changes_the_concept_half() {
        // The high half is the render lens; it must not perturb the low-half
        // concept that RBAC + ontology key on.
        let op = render_classid_for::<OpenProjectPort>(class_ids::BILLABLE_WORK_ENTRY);
        let rm = render_classid_for::<RedminePort>(class_ids::BILLABLE_WORK_ENTRY);
        assert_ne!(app_of(op), app_of(rm), "render lenses differ");
        assert_eq!(concept_of(op), concept_of(rm), "concept is shared");
        assert_eq!(concept_of(op), class_ids::BILLABLE_WORK_ENTRY);
    }
}
