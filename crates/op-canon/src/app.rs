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
//! # One source of truth — the OGAR surface
//!
//! Both halves of the composition come from `ogar-vocab` (OGAR PR #97 + #98):
//!
//! - [`APP_PREFIX`] is a `pub const` re-export of
//!   [`ogar_vocab::ports::OpenProjectPort::APP_PREFIX`] — the typed §2
//!   allocation-table value, not a local literal.
//! - [`render_classid`] re-exports `ogar_vocab::app::render_classid_for::<OpenProjectPort>`
//!   — the central `(prefix << 16) | concept` composition; one place owns the
//!   bit math.
//! - [`app_of`] / [`concept_of`] re-export `ogar_vocab::app::{app_of, concept_of}`
//!   — the inverse decomposition (`0x0001_DDCC` → prefix + concept), so a render
//!   classid round-trips entirely on this crate's surface without reaching into
//!   `ogar_vocab::app`.
//!
//! Same discipline as [`crate::class_ids`] (which re-exports
//! `ogar_vocab::class_ids::*`): the canonical layer mints; this crate
//! re-exports. Drift between local and OGAR is now structurally impossible.

use ogar_vocab::ports::{OpenProjectPort, PortSpec};

/// OpenProject's reserved **APP / render prefix** — the high u16 of a full
/// `classid` (`APP-CLASS-CODEBOOK-LAYOUT.md` §2 allocation table). Pairs with
/// Redmine's `0x0007`: same low-half concept, different render lens.
///
/// `pub const` re-export of [`ogar_vocab::ports::OpenProjectPort::APP_PREFIX`]
/// (OGAR PR #97). Promoted from a local mirror to the typed upstream constant —
/// one source of truth.
pub const APP_PREFIX: u16 = OpenProjectPort::APP_PREFIX;

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
/// OpenProject's prefix: `0x0001_DDCC`.
///
/// Re-export of `ogar_vocab::app::render_classid_for::<OpenProjectPort>(concept)`
/// (OGAR PR #97) — the central composition, not local bit math. Pure stamp: it
/// does not validate that `concept` is a low-half (project-mgmt `0x01XX`) id —
/// composing a foreign concept would still produce a value. Pass a
/// [`crate::class_ids`] constant or a [`class_id_of`] result.
///
/// ```
/// use op_canon::{app, class_ids};
/// assert_eq!(app::render_classid(class_ids::PROJECT_WORK_ITEM), 0x0001_0102);
/// ```
#[must_use]
pub fn render_classid(concept: u16) -> u32 {
    ogar_vocab::app::render_classid_for::<OpenProjectPort>(concept)
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

/// Decompose a 32-bit render classid into its **OpenProject render prefix**
/// (the high u16) — the inverse of [`render_classid`]'s high half.
///
/// Re-export of `ogar_vocab::app::app_of` (OGAR PR #97) — the central
/// decomposition, not local bit math; paired with [`concept_of`]. For any
/// concept, `app_of(render_classid(concept)) == APP_PREFIX`.
///
/// ```
/// use op_canon::{app, class_ids};
/// let cid = app::render_classid(class_ids::PROJECT_WORK_ITEM); // 0x0001_0102
/// assert_eq!(app::app_of(cid), app::APP_PREFIX);               // 0x0001
/// ```
#[must_use]
pub fn app_of(classid: u32) -> u16 {
    ogar_vocab::app::app_of(classid)
}

/// Decompose a 32-bit render classid into its **shared concept** (the low
/// u16) — the inverse of [`render_classid`]'s low half, recovering the
/// cross-app currency a [`class_id_of`] pull returns.
///
/// Re-export of `ogar_vocab::app::concept_of` (OGAR PR #97), paired with
/// [`app_of`]. For any concept, `concept_of(render_classid(concept)) == concept`;
/// this is the id Redmine's twin carries under prefix `0x0007`.
///
/// ```
/// use op_canon::{app, class_ids};
/// let cid = app::render_classid(class_ids::PROJECT_WORK_ITEM); // 0x0001_0102
/// assert_eq!(app::concept_of(cid), class_ids::PROJECT_WORK_ITEM); // 0x0102
/// ```
#[must_use]
pub fn concept_of(classid: u32) -> u16 {
    ogar_vocab::app::concept_of(classid)
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
    fn app_prefix_re_exports_the_typed_ogar_constant() {
        // One source of truth: the local constant IS the upstream typed
        // PortSpec::APP_PREFIX, not a parallel literal. (#97)
        assert_eq!(APP_PREFIX, OpenProjectPort::APP_PREFIX);
        assert_eq!(APP_PREFIX, 0x0001);
    }

    #[test]
    fn render_classid_composes_openproject_prefix() {
        // Full render classid = 0x0001_DDCC (W0 worked table).
        assert_eq!(render_classid(class_ids::PROJECT_WORK_ITEM), 0x0001_0102);
        assert_eq!(render_classid(class_ids::BILLABLE_WORK_ENTRY), 0x0001_0103);
        assert_eq!(render_classid(class_ids::PROJECT_ROLE), 0x0001_0117);
        // Pull + compose.
        assert_eq!(render_classid_of("WorkPackage"), Some(0x0001_0102));
        assert_eq!(render_classid_of("TimeEntry"), Some(0x0001_0103));
    }

    #[test]
    fn render_classid_agrees_with_the_central_ogar_composition() {
        // The local function is exactly OGAR's `render_classid_for::<P>`; no
        // local bit math. If this assertion ever fails, the local impl drifted
        // from the canonical upstream composition.
        for &concept in &[
            class_ids::PROJECT_WORK_ITEM,
            class_ids::PROJECT,
            class_ids::BILLABLE_WORK_ENTRY,
            class_ids::PROJECT_ROLE,
        ] {
            assert_eq!(
                render_classid(concept),
                ogar_vocab::app::render_classid_for::<OpenProjectPort>(concept),
            );
        }
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
            // Decompose via OGAR's central helpers (one source of truth on the
            // bit math, not local shifts).
            assert_eq!(
                ogar_vocab::app::app_of(cid),
                APP_PREFIX,
                "high half = OpenProject lens",
            );
            assert_eq!(
                ogar_vocab::app::concept_of(cid),
                concept,
                "low half = shared concept",
            );
        }
    }

    #[test]
    fn app_of_and_concept_of_invert_render_classid() {
        // The local decomposition surface round-trips render_classid: app_of
        // recovers the OpenProject prefix, concept_of the shared concept.
        for &concept in &[
            class_ids::PROJECT_WORK_ITEM,
            class_ids::PROJECT,
            class_ids::BILLABLE_WORK_ENTRY,
            class_ids::PROJECT_ROLE,
            class_ids::PROJECT_ACTOR,
        ] {
            let cid = render_classid(concept);
            assert_eq!(app_of(cid), APP_PREFIX, "app_of recovers the prefix");
            assert_eq!(concept_of(cid), concept, "concept_of recovers the concept");
        }
    }

    #[test]
    fn app_of_concept_of_are_the_central_ogar_decomposition() {
        // The re-exports ARE OGAR's app_of/concept_of — same one-source-of-truth
        // discipline as render_classid (no local bit math to drift).
        let cid = render_classid(class_ids::PROJECT_WORK_ITEM);
        assert_eq!(app_of(cid), ogar_vocab::app::app_of(cid));
        assert_eq!(concept_of(cid), ogar_vocab::app::concept_of(cid));
        // And the full decomposition reconstructs the classid bit-for-bit.
        assert_eq!(((app_of(cid) as u32) << 16) | (concept_of(cid) as u32), cid,);
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

    /// **Closes** the previously-pinned `port_and_snapshot_membership_vocab_mismatch_is_known`
    /// drift guard — its trigger fired on OGAR `main` after PR #113 (the
    /// Member alias add). Now `class_id_of("Member")` and
    /// `class_id_of("Membership")` BOTH resolve to `PROJECT_MEMBERSHIP`:
    /// the port matches the OpenProject corpus snapshot (`Member`) AND
    /// keeps the deprecated synonym (`Membership`) for any consumer
    /// holding the old name. The vocab divergence is gone; this is the
    /// successor positive assertion.
    #[test]
    fn member_and_membership_both_resolve_after_ogar_113() {
        let s = Snapshot::load();
        let membership = s
            .concept("project_membership")
            .expect("snapshot carries project_membership");
        // Snapshot uses `Member` — that's the OP corpus name.
        assert!(
            membership.curator_classes.iter().any(|c| c == "Member"),
            "snapshot expected to carry `Member` for project_membership; saw {:?}",
            membership.curator_classes,
        );
        // Both surface names now pull the same canonical id (OGAR #113):
        //   - `Member`     — canonical, matches the corpus + Redmine
        //   - `Membership` — deprecated synonym kept for backcompat
        // So the overlap drift guard above now covers `Member` too (no
        // silent skip), and the cross-port convergence is byte-symmetric:
        // OP `Member` ↔ RM `Member` → 0x0108.
        let target = Some(membership.class_id_u16());
        assert_eq!(class_id_of("Member"), target);
        assert_eq!(class_id_of("Membership"), target);
    }
}
