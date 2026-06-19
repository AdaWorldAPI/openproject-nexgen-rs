//! Compile-time **canonical class-id constants** — the OpenProject view of
//! the OGAR codebook, exposed as named `u16`s so downstream `op-*` crates
//! can dispatch on identity without parsing the snapshot JSON at runtime.
//!
//! ```
//! use op_canon::class_ids;
//!
//! fn dispatch(incoming_id: u16) {
//!     match incoming_id {
//!         class_ids::PROJECT_WORK_ITEM => handle_work_package(),
//!         class_ids::BILLABLE_WORK_ENTRY => handle_time_entry(),
//!         _ => {}
//!     }
//! }
//! # fn handle_work_package() {}
//! # fn handle_time_entry() {}
//! ```
//!
//! Constants are **kept in sync with the vendored snapshot mechanically**:
//! [`tests::constants_match_the_snapshot`] in this module walks every
//! constant and asserts the vendored [`crate::Snapshot`] reports the same
//! `(canonical_concept, id)` pair. Drift is impossible without a failing
//! test.
//!
//! Ids are stable forever (per the OGAR codebook contract). They only
//! arrive — never move, never get re-assigned.

/// `project` (`0x0101`) — the root project container. Both Redmine and
/// OpenProject use `Project` as the curator class.
pub const PROJECT: u16 = 0x0101;
/// `project_work_item` (`0x0102`) — project-scoped work item. OpenProject
/// `WorkPackage` / Redmine `Issue` collapse here.
pub const PROJECT_WORK_ITEM: u16 = 0x0102;
/// `billable_work_entry` (`0x0103`) — booked work / time / cost. The
/// **first cross-domain bridge**: OpenProject `TimeEntry`, Redmine
/// `TimeEntry`, Odoo `account.analytic.line` all converge here.
pub const BILLABLE_WORK_ENTRY: u16 = 0x0103;
/// `project_actor` (`0x0104`) — the actor identity (Principal + User +
/// Group STI chain collapsed).
pub const PROJECT_ACTOR: u16 = 0x0104;
/// `project_status` (`0x0105`) — workflow status. Redmine `IssueStatus`,
/// OpenProject `Status`.
pub const PROJECT_STATUS: u16 = 0x0105;
/// `project_type` (`0x0106`) — work-item type. Redmine `Tracker`,
/// OpenProject `Type`.
pub const PROJECT_TYPE: u16 = 0x0106;
/// `priority` (`0x0107`) — priority enumeration. Both ship `IssuePriority`.
pub const PRIORITY: u16 = 0x0107;
/// `project_membership` (`0x0108`) — actor↔project join. Both ship `Member`.
pub const PROJECT_MEMBERSHIP: u16 = 0x0108;
/// `project_journal` (`0x0109`) — change journal entry.
pub const PROJECT_JOURNAL: u16 = 0x0109;
/// `project_repository` (`0x010A`) — VCS repository.
pub const PROJECT_REPOSITORY: u16 = 0x010A;
/// `project_version` (`0x010B`) — release / milestone.
pub const PROJECT_VERSION: u16 = 0x010B;
/// `project_wiki_page` (`0x010C`).
pub const PROJECT_WIKI_PAGE: u16 = 0x010C;
/// `project_query` (`0x010D`) — saved query.
pub const PROJECT_QUERY: u16 = 0x010D;
/// `project_attachment` (`0x010E`).
pub const PROJECT_ATTACHMENT: u16 = 0x010E;
/// `project_comment` (`0x010F`).
pub const PROJECT_COMMENT: u16 = 0x010F;
/// `project_custom_field` (`0x0110`).
pub const PROJECT_CUSTOM_FIELD: u16 = 0x0110;
/// `project_relation` (`0x0111`) — work-item↔work-item link. Redmine
/// `IssueRelation`, OpenProject `Relation`.
pub const PROJECT_RELATION: u16 = 0x0111;
/// `project_changeset` (`0x0112`) — VCS commit metadata.
pub const PROJECT_CHANGESET: u16 = 0x0112;
/// `project_watcher` (`0x0113`).
pub const PROJECT_WATCHER: u16 = 0x0113;
/// `project_news` (`0x0114`) — project news / blog post.
pub const PROJECT_NEWS: u16 = 0x0114;
/// `project_message` (`0x0115`) — forum / board message.
pub const PROJECT_MESSAGE: u16 = 0x0115;
/// `project_forum` (`0x0116`) — message container. Redmine `Board`,
/// OpenProject `Forum`.
pub const PROJECT_FORUM: u16 = 0x0116;
/// `project_role` (`0x0117`) — RBAC permission-set bundle. Note OpenProject
/// ships **both** `Role` and a `ProjectRole` subclass; both collapse here.
pub const PROJECT_ROLE: u16 = 0x0117;
/// `project_member_role` (`0x0118`) — RBAC join (membership ↔ role).
pub const PROJECT_MEMBER_ROLE: u16 = 0x0118;
/// `project_custom_value` (`0x0119`) — value of a [`PROJECT_CUSTOM_FIELD`]
/// on a record.
pub const PROJECT_CUSTOM_VALUE: u16 = 0x0119;
/// `project_enabled_module` (`0x011A`) — per-project module enablement.
pub const PROJECT_ENABLED_MODULE: u16 = 0x011A;

/// Every `(canonical_concept_name, id)` pair the OpenProject snapshot
/// promotes. Walked by the drift-guard test below; consumers reaching for
/// a specific id should use the named constant above, not this slice.
pub const ALL: &[(&str, u16)] = &[
    ("project", PROJECT),
    ("project_work_item", PROJECT_WORK_ITEM),
    ("billable_work_entry", BILLABLE_WORK_ENTRY),
    ("project_actor", PROJECT_ACTOR),
    ("project_status", PROJECT_STATUS),
    ("project_type", PROJECT_TYPE),
    ("priority", PRIORITY),
    ("project_membership", PROJECT_MEMBERSHIP),
    ("project_journal", PROJECT_JOURNAL),
    ("project_repository", PROJECT_REPOSITORY),
    ("project_version", PROJECT_VERSION),
    ("project_wiki_page", PROJECT_WIKI_PAGE),
    ("project_query", PROJECT_QUERY),
    ("project_attachment", PROJECT_ATTACHMENT),
    ("project_comment", PROJECT_COMMENT),
    ("project_custom_field", PROJECT_CUSTOM_FIELD),
    ("project_relation", PROJECT_RELATION),
    ("project_changeset", PROJECT_CHANGESET),
    ("project_watcher", PROJECT_WATCHER),
    ("project_news", PROJECT_NEWS),
    ("project_message", PROJECT_MESSAGE),
    ("project_forum", PROJECT_FORUM),
    ("project_role", PROJECT_ROLE),
    ("project_member_role", PROJECT_MEMBER_ROLE),
    ("project_custom_value", PROJECT_CUSTOM_VALUE),
    ("project_enabled_module", PROJECT_ENABLED_MODULE),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Snapshot;

    #[test]
    fn constants_match_the_snapshot() {
        // The drift guard. Every (name, id) pair in ALL must be exactly
        // what the vendored snapshot reports — a regen that changes an id
        // or drops a concept fails THIS test before any consumer breaks.
        let s = Snapshot::load();
        for (name, id) in ALL {
            let c = s
                .concept(name)
                .unwrap_or_else(|| panic!("{name} promoted in ALL but missing from snapshot"));
            assert_eq!(
                c.class_id_u16(),
                *id,
                "{name}: constant 0x{id:04X} disagrees with snapshot 0x{:04X}",
                c.class_id_u16(),
            );
        }
    }

    #[test]
    fn every_snapshot_concept_has_a_constant() {
        // The reverse drift guard: if the snapshot promotes a new concept,
        // ALL (and the named const block above) must learn it. Catches the
        // "regen forgot to update class_ids.rs" case.
        let s = Snapshot::load();
        let known: std::collections::HashSet<&str> = ALL.iter().map(|(n, _)| *n).collect();
        for c in &s.concepts {
            assert!(
                known.contains(c.canonical_concept.as_str()),
                "{} promoted in snapshot but missing from class_ids::ALL",
                c.canonical_concept,
            );
        }
    }

    #[test]
    fn constants_are_unique() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for (name, id) in ALL {
            assert!(seen.insert(*id), "duplicate id 0x{id:04X} (saw at {name})");
        }
    }

    #[test]
    fn divergent_curator_names_share_one_constant() {
        // The whole point of the codebook, in code: an OpenProject
        // WorkPackage and a Redmine Issue both route on PROJECT_WORK_ITEM.
        // A consumer dispatching on incoming codebook ids needs the SAME
        // arm for both. Pinning a few of the headline pairs.
        assert_eq!(PROJECT_WORK_ITEM, 0x0102);
        assert_eq!(PROJECT_STATUS, 0x0105);
        assert_eq!(PROJECT_TYPE, 0x0106);
        assert_eq!(PROJECT_FORUM, 0x0116);
        assert_eq!(BILLABLE_WORK_ENTRY, 0x0103);
    }
}
