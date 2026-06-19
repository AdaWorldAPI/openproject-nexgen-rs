//! **Canonical class-id constants** — re-exported from
//! [`ogar_vocab::class_ids`], the single source of truth.
//!
//! ```
//! use op_canon::class_ids;
//!
//! fn dispatch(incoming_id: u16) {
//!     match incoming_id {
//!         class_ids::PROJECT_WORK_ITEM   => handle_work_package(),
//!         class_ids::BILLABLE_WORK_ENTRY => handle_time_entry(),
//!         _ => {}
//!     }
//! }
//! # fn handle_work_package() {}
//! # fn handle_time_entry() {}
//! ```
//!
//! **These are the same constants `redmine-canon::class_ids` exposes.**
//! Both `-rs` ports re-export from `ogar_vocab` so the values cannot drift
//! across ports: the codebook is minted once in
//! [`AdaWorldAPI/OGAR`](https://github.com/AdaWorldAPI/OGAR) and the typed
//! constants come from there.
//!
//! OGAR carries the forward+reverse drift guards (constants ↔ codebook).
//! This module carries one port-local guard: every concept the **vendored
//! snapshot** promotes must agree with the re-exported constant at the
//! same id — so a regen that drifts from the published codebook fails CI.

pub use ogar_vocab::class_ids::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Snapshot;

    #[test]
    fn snapshot_concepts_match_re_exported_constants() {
        // Port-local drift guard: every concept the vendored snapshot
        // promotes must resolve to the same id via OGAR's codebook. If a
        // regen produces an id that disagrees with the published codebook
        // (which by contract never re-assigns), this fires.
        let s = Snapshot::load();
        for c in &s.concepts {
            let id = ogar_vocab::canonical_concept_id(&c.canonical_concept).unwrap_or_else(|| {
                panic!(
                    "{} promoted in snapshot but absent from OGAR codebook",
                    c.canonical_concept
                )
            });
            assert_eq!(
                c.class_id_u16(),
                id,
                "{}: snapshot 0x{:04X} disagrees with OGAR codebook 0x{id:04X}",
                c.canonical_concept,
                c.class_id_u16(),
            );
        }
    }

    #[test]
    fn re_export_brings_in_the_headline_constants() {
        // Sanity: the `pub use ogar_vocab::class_ids::*` actually pulled
        // the constants this port cares about into scope, at the codebook
        // ids ogar-vocab vouches for.
        assert_eq!(PROJECT_WORK_ITEM, 0x0102);
        assert_eq!(BILLABLE_WORK_ENTRY, 0x0103);
        assert_eq!(PROJECT_FORUM, 0x0116);
        assert_eq!(PROJECT_ROLE, 0x0117);
    }
}
