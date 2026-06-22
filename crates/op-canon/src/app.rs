//! **APP‖class — pull OGAR via class, no bridge.**
//!
//! The landing shape of [lance-graph#589][589] for the OpenProject (project)
//! consumer: resolve an OpenProject surface name to its canonical class-id by
//! **pulling the OGAR port** ([`ogar_vocab::ports::OpenProjectPort::class_id`]),
//! not a bridge and not a hand-rolled registry. The vendored snapshot
//! ([`crate::Snapshot::concept_of_class`]) stays as a **checked mirror**: the
//! [`tests::port_pull_agrees_with_the_snapshot`] drift guard proves the port
//! and the snapshot agree **on every surface name the port resolves** (names
//! the port doesn't alias are not cross-checked — see the guard's note), so
//! the port can be the resolver over its covered surface while the snapshot
//! remains the corpus evidence.
//!
//! [589]: https://github.com/AdaWorldAPI/lance-graph/pull/589
//!
//! # The two halves of a classid (`APP-CLASS-CODEBOOK-LAYOUT.md`)
//!
//! ```text
//! classid : u32  =  [ hi u16 : APP / render prefix ]  [ lo u16 : concept ]
//!                     0x0001 (OpenProject)              0xDDCC (shared)
//! ```
//!
//! - **low u16 — the shared canonical concept.** What the object *is*: the
//!   RBAC + ontology + cross-app identity key. This is what the port pull
//!   returns, and it is identical to the id Redmine's `Issue` pulls
//!   (`project_work_item = 0x0102`). The shared currency.
//! - **high u16 — OpenProject's render prefix [`APP_PREFIX`] (`0x0001`).**
//!   *Whose* rendering: OpenProject's `ClassView` / template lens. Reserved
//!   for OpenProject in `APP-CLASS-CODEBOOK-LAYOUT.md` §2; Redmine's twin is
//!   `0x0007`. A full render classid is `0x0001_DDCC`.
//!
//! This module composes only OpenProject's `0x0001` render of the W0
//! "two renders, one concept" pair; the cross-fork convergence itself (that
//! OpenProject `WorkPackage` and Redmine `Issue` share the low half `0x0102`)
//! is machine-checked **upstream** in OGAR's port tests
//! (`openproject_and_redmine_converge_on_shared_concepts`), not here.
//!
//! Composing the high half here is the consumer **stamping its own reserved
//! prefix** (`APP-CLASS-CODEBOOK-LAYOUT.md` §1, §3d), not minting an OGAR
//! codebook class — that mint is gated on OGAR's 5+3 pass. The pull (low
//! half) is the part that is canonical and available today.
//!
//! > **Follow-up:** [`APP_PREFIX`] is a local mirror of the §2 allocation
//! > table. Once OGAR exports the prefix as a typed port constant
//! > (`OpenProjectPort::APP_PREFIX`), re-export that instead — same
//! > one-source-of-truth discipline `class_ids` / `class_view` already follow.

use ogar_vocab::ports::{OpenProjectPort, PortSpec};

/// OpenProject's reserved **APP / render prefix** — the high u16 of a full
/// `classid` (`APP-CLASS-CODEBOOK-LAYOUT.md` §2 allocation table). Pairs with
/// Redmine's `0x0007`: same low-half concept, different render lens.
///
/// Local mirror of the §2 table (SPEC-status, pending OGAR's 5+3 pass); to be
/// replaced by a re-export of the typed upstream constant when OGAR ships it.
pub const APP_PREFIX: u16 = 0x0001;

/// Pull the canonical class-id for an OpenProject surface name **via the OGAR
/// port** — the #589 "pull OGAR via class" path (no bridge, no registry).
/// `None` for a name the codebook does not carry.
///
/// Named `class_id_of` (not `class_id`) to read as a resolver — `class_id_of("WorkPackage")`
/// — distinct from the [`crate::class_ids`] constant module, and symmetric with
/// [`render_classid_of`] and [`crate::Snapshot::concept_of_class`].
///
/// ```
/// use op_canon::{app, class_ids};
/// assert_eq!(app::class_id_of("WorkPackage"), Some(class_ids::PROJECT_WORK_ITEM));
/// assert_eq!(app::class_id_of("TimeEntry"), Some(class_ids::BILLABLE_WORK_ENTRY));
/// assert_eq!(app::class_id_of("NotAnOpenProjectClass"), None);
/// ```
#[must_use]
pub fn class_id_of(surface_name: &str) -> Option<u16> {
    OpenProjectPort::class_id(surface_name)
}

/// Compose the full 32-bit **render** classid for a shared `concept` under
/// OpenProject's prefix: `(APP_PREFIX << 16) | concept` → `0x0001_DDCC`.
///
/// Pure bit-stamp: it does not validate that `concept` is a low-half
/// (project-mgmt `0x01XX`) id — composing a foreign concept would still
/// produce a value. Pass a [`crate::class_ids`] constant or a [`class_id_of`]
/// result.
///
/// ```
/// use op_canon::{app, class_ids};
/// assert_eq!(app::render_classid(class_ids::PROJECT_WORK_ITEM), 0x0001_0102);
/// ```
#[must_use]
pub const fn render_classid(concept: u16) -> u32 {
    ((APP_PREFIX as u32) << 16) | (concept as u32)
}

/// Pull + compose in one step: an OpenProject surface name → its full render
/// classid `0x0001_DDCC`, via the OGAR port. `None` if the port does not carry
/// the name.
///
/// ```
/// use op_canon::app;
/// assert_eq!(app::render_classid_of("WorkPackage"), Some(0x0001_0102));
/// ```
#[must_use]
pub fn render_classid_of(surface_name: &str) -> Option<u32> {
    class_id_of(surface_name).map(render_classid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{class_ids, Snapshot};

    #[test]
    fn port_pull_resolves_headline_surface_names() {
        // The #589 AFTER pattern: surface name → class_id via the port.
        assert_eq!(
            class_id_of("WorkPackage"),
            Some(class_ids::PROJECT_WORK_ITEM)
        );
        assert_eq!(class_id_of("Project"), Some(class_ids::PROJECT));
        assert_eq!(
            class_id_of("TimeEntry"),
            Some(class_ids::BILLABLE_WORK_ENTRY)
        );
        assert_eq!(class_id_of("Role"), Some(class_ids::PROJECT_ROLE));
        // STI fold: User / Principal / Group all pull project_actor.
        assert_eq!(class_id_of("User"), Some(class_ids::PROJECT_ACTOR));
        assert_eq!(class_id_of("Principal"), Some(class_ids::PROJECT_ACTOR));
        assert_eq!(class_id_of("Group"), Some(class_ids::PROJECT_ACTOR));
        // Unknown names resolve to None (not a panic, not a bridge fallthrough).
        assert_eq!(class_id_of("NotAnOpenProjectClass"), None);
        assert_eq!(class_id_of(""), None);
    }

    #[test]
    fn render_classid_composes_openproject_prefix() {
        assert_eq!(APP_PREFIX, 0x0001);
        // Full render classid = 0x0001_DDCC (W0 worked table).
        assert_eq!(render_classid(class_ids::PROJECT_WORK_ITEM), 0x0001_0102);
        assert_eq!(render_classid(class_ids::BILLABLE_WORK_ENTRY), 0x0001_0103);
        assert_eq!(render_classid(class_ids::PROJECT_ROLE), 0x0001_0117);
        // Pull + compose.
        assert_eq!(render_classid_of("WorkPackage"), Some(0x0001_0102));
        assert_eq!(render_classid_of("TimeEntry"), Some(0x0001_0103));
    }

    #[test]
    fn render_classid_keeps_concept_in_the_low_half() {
        // The low half is the shared concept (== the port pull); the high
        // half is OpenProject's render lens. Redmine's twin carries the SAME
        // low half under prefix 0x0007 (pinned in OGAR's port tests).
        for &concept in &[
            class_ids::PROJECT_WORK_ITEM,
            class_ids::PROJECT,
            class_ids::BILLABLE_WORK_ENTRY,
            class_ids::PROJECT_ROLE,
        ] {
            let cid = render_classid(concept);
            assert_eq!(
                (cid >> 16) as u16,
                APP_PREFIX,
                "high half = OpenProject lens"
            );
            assert_eq!(cid as u16, concept, "low half = shared concept");
        }
    }

    #[test]
    fn port_pull_agrees_with_the_snapshot() {
        // The drift guard that lets the PORT be the resolver and the snapshot
        // be a checked mirror: every snapshot curator class the port ALSO
        // resolves must agree on the id. It checks the OVERLAP only — snapshot
        // names the port doesn't alias return `None` and are skipped (that is
        // not a disagreement, but it is also not verified; see
        // `port_and_snapshot_membership_vocab_mismatch_is_known` for the one
        // live skip that is a real vocab divergence).
        let s = Snapshot::load();
        let mut checked = 0u32;
        let mut skipped: Vec<&str> = Vec::new();
        for concept in &s.concepts {
            for curator_class in &concept.curator_classes {
                match class_id_of(curator_class) {
                    Some(pulled) => {
                        assert_eq!(
                            pulled,
                            concept.class_id_u16(),
                            "port `{curator_class}` -> 0x{pulled:04X} disagrees with snapshot \
                             `{}` 0x{:04X}",
                            concept.canonical_concept,
                            concept.class_id_u16(),
                        );
                        checked += 1;
                    }
                    None => skipped.push(curator_class),
                }
            }
        }
        // Sanity: the guard actually exercised the bulk of the surface (the
        // overlap is ~26 of 31 curator names), not a vacuous pass.
        assert!(
            checked >= 20,
            "expected the port to cover most snapshot surface names, only matched \
             {checked} (skipped: {skipped:?})",
        );
    }

    /// Pins the one snapshot↔port vocabulary divergence the overlap guard
    /// silently skips (flagged by the 5+3 review, R4 + B1): the OpenProject
    /// corpus snapshot carries **`Member`** for `project_membership` (0x0108),
    /// but `OPENPROJECT_ALIASES` aliases that concept under **`Membership`**.
    /// So `class_id_of("Member")` is `None` today and the drift guard skips it.
    ///
    /// This test makes the divergence VISIBLE and tracked: when OGAR aligns
    /// the alias to the corpus name (`Member`), this test flips and reminds us
    /// to drop the pin. (The OGAR-side fix is the proper resolution; here we
    /// only document the current state so it can't hide.)
    #[test]
    fn port_and_snapshot_membership_vocab_mismatch_is_known() {
        let s = Snapshot::load();
        let membership = s
            .concept("project_membership")
            .expect("snapshot carries project_membership");
        assert!(
            membership.curator_classes.iter().any(|c| c == "Member"),
            "snapshot is expected to carry `Member` for project_membership; saw {:?}",
            membership.curator_classes,
        );
        // KNOWN GAP: the port aliases this concept as `Membership`, not `Member`.
        assert_eq!(
            class_id_of("Member"),
            None,
            "if this is now Some(_), OGAR aligned the alias to the corpus name — \
             update the OPENPROJECT_ALIASES follow-up and remove this pin",
        );
        // The port DOES resolve `Membership` to the same concept id, so the
        // concept itself converges; only the surface spelling differs.
        assert_eq!(class_id_of("Membership"), Some(membership.class_id_u16()));
    }
}
