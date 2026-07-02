//! **class‖APP composition** — the render-prefix machinery
//! (`docs/APP-CLASS-CODEBOOK-LAYOUT.md` §0, §4).
//!
//! A full 32-bit `classid` is two orthogonal halves. Since the 2026-07-02
//! canon:custom half-order flip (lance-graph
//! `classid-canon-custom-flip-v1.md` P1/P2 — the operator's `0x07:01::1000`
//! ruling), the CANON concept sits HIGH and the APP/render prefix LOW:
//!
//! ```text
//! classid : u32  =  [ hi u16 : CANON concept ]  [ lo u16 : APP / render prefix ]
//!                     0xDDCC (shared RBAC+ontology)  0xAAAA (per-app ClassView lens)
//! ```
//!
//! The per-port reserved prefix is [`PortSpec::APP_PREFIX`] (the §2 allocation
//! table as typed data; the prefix VALUES are order-invariant); this module
//! composes and decomposes the full id so consumers re-export ONE source of
//! the bit math instead of each re-implementing
//! `((concept as u32) << 16) | prefix`. `0x0000` is the shared canonical core
//! (every existing id renders under the core lens as `0xDDCC_0000` —
//! additive, invariant I-APP1). Pre-flip stored ids (`0x0000_DDCC` /
//! `0xAAAA_DDCC`) are the LEGACY order — readers of persisted data resolve
//! them through concrete legacy-alias registry keys, never by
//! reinterpreting the halves (mint-forward).
//!
//! Composing the prefix half is **reserving/stamping**, not minting a class_id
//! (§2: "reserving costs nothing") — concept mints stay gated on the
//! 5+3 codebook pass. The concept half is the cross-app currency:
//! concept/domain routing reads it alone (`lance_graph_contract::
//! classid_concept_domain` routes the canon half), so it is identical under
//! every render prefix.

use crate::ports::PortSpec;

/// Compose a full render `classid` from an app `prefix` (CUSTOM half, low
/// u16) and a canonical `concept` id (CANON half, high u16):
/// `((concept as u32) << 16) | prefix`.
///
/// `render_classid(0x0001, 0x0102)` → `0x0102_0001` (OpenProject's
/// `project_work_item`); the Redmine twin `render_classid(0x0007, 0x0102)` →
/// `0x0102_0007` — same concept, different render lens.
#[must_use]
pub const fn render_classid(prefix: u16, concept: u16) -> u32 {
    ((concept as u32) << 16) | (prefix as u32)
}

/// Compose a render `classid` for a specific port, reading its reserved
/// [`PortSpec::APP_PREFIX`]: `render_classid_for::<OpenProjectPort>(concept)`
/// → `0xDDCC_0001`. This is the helper consumers call so the prefix and the
/// bit math both come from OGAR (one source of truth), not a local literal.
#[must_use]
pub fn render_classid_for<P: PortSpec>(concept: u16) -> u32 {
    render_classid(P::APP_PREFIX, concept)
}

/// The APP / render prefix — the CUSTOM half (**low u16** since the flip) of
/// a full `classid`. `0x0000` ([the shared core]) is the
/// abstract/default-`ClassView` anchor; a non-zero value selects an app's
/// render lens. This is the §4 `resolve_codebook` routing key
/// (`classid as u16`).
///
/// [the shared core]: PortSpec::APP_PREFIX
#[must_use]
pub const fn app_of(classid: u32) -> u16 {
    classid as u16
}

/// The canonical concept id — the CANON half (**high u16** since the flip) of
/// a full `classid`. The shared RBAC + ontology + cross-app identity key;
/// concept/domain routing reads only this half, so it is identical for every
/// render prefix.
#[must_use]
pub const fn concept_of(classid: u32) -> u16 {
    (classid >> 16) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_ids;
    use crate::ports::{OdooPort, OpenProjectPort, RedminePort};

    #[test]
    fn render_classid_composes_the_two_halves() {
        // Canon-high order (2026-07-02 flip): concept HIGH, prefix LOW.
        assert_eq!(render_classid(0x0001, 0x0102), 0x0102_0001);
        assert_eq!(render_classid(0x0007, 0x0102), 0x0102_0007);
        assert_eq!(render_classid(0x0002, 0x0202), 0x0202_0002);
    }

    #[test]
    fn render_classid_for_reads_the_port_prefix() {
        // OpenProject (0x0001) and Redmine (0x0007) render the SAME concept
        // under different prefixes — "two renders, one concept" (§1).
        assert_eq!(
            render_classid_for::<OpenProjectPort>(class_ids::PROJECT_WORK_ITEM),
            0x0102_0001,
        );
        assert_eq!(
            render_classid_for::<RedminePort>(class_ids::PROJECT_WORK_ITEM),
            0x0102_0007,
        );
        assert_eq!(
            render_classid_for::<OdooPort>(class_ids::COMMERCIAL_DOCUMENT),
            0x0202_0002,
        );
    }

    #[test]
    fn app_of_and_concept_of_decompose() {
        let cid = render_classid(0x0005, class_ids::PATIENT); // Medcare patient
        assert_eq!(cid, 0x0901_0005);
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
        // I-APP1/I-APP5 (post-flip form): a core (prefix=0x0000) classid is
        // the bare concept in the CANON (high) half — no renumber, no version
        // cost; `concept_of` recovers it exactly.
        let core = render_classid(0x0000, class_ids::PROJECT_WORK_ITEM);
        assert_eq!(core, 0x0102_0000);
        assert_eq!(core, u32::from(class_ids::PROJECT_WORK_ITEM) << 16);
        assert_eq!(app_of(core), 0x0000);
        assert_eq!(concept_of(core), class_ids::PROJECT_WORK_ITEM);
    }

    #[test]
    fn render_prefix_never_changes_the_concept_half() {
        // The prefix half is the render lens; it must not perturb the CANON
        // concept that RBAC + ontology key on.
        let op = render_classid_for::<OpenProjectPort>(class_ids::BILLABLE_WORK_ENTRY);
        let rm = render_classid_for::<RedminePort>(class_ids::BILLABLE_WORK_ENTRY);
        assert_ne!(app_of(op), app_of(rm), "render lenses differ");
        assert_eq!(concept_of(op), concept_of(rm), "concept is shared");
        assert_eq!(concept_of(op), class_ids::BILLABLE_WORK_ENTRY);
    }
}
