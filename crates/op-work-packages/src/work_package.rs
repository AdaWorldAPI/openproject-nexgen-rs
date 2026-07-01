//! Work package model
//!
//! Mirrors: `app/models/work_package.rb`
//!
//! The headline OpenProject entity — a project-scoped unit of work (task,
//! bug, feature, …). Converges with Redmine's `Issue` on the canonical
//! concept `project_work_item` (codebook `0x0102`); the canonical-class-id
//! wiring lives in `op-canon` and is layered on in a follow-up.

use chrono::{DateTime, NaiveDate, Utc};
use op_core::traits::{Entity, Id, Identifiable, Lockable, ProjectScoped, Timestamped};
use op_core::types::Formattable;
use serde::{Deserialize, Serialize};

/// Progress of a work package as a whole-number percentage `0..=100`
/// (OpenProject's `done_ratio`).
///
/// Construction via [`DoneRatio::new`] / [`From<u8>`] **clamps** into the
/// band, so a constructed value is always valid. Deserialized values are
/// trusted to the source (OpenProject's DB column enforces `0..=100`), the
/// same trust boundary the Rails model relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct DoneRatio(u8);

impl DoneRatio {
    /// Clamp `percent` into `0..=100`.
    pub fn new(percent: u8) -> Self {
        Self(percent.min(100))
    }

    /// The percentage value — always `0..=100`.
    pub fn value(self) -> u8 {
        self.0
    }

    /// Whether the work is fully done (`100%`).
    pub fn is_complete(self) -> bool {
        self.0 == 100
    }
}

impl From<u8> for DoneRatio {
    fn from(percent: u8) -> Self {
        Self::new(percent)
    }
}

/// A work package — OpenProject's core unit of work.
///
/// The foreign keys (`type_id`, `status_id`, `priority_id`, …) reference the
/// respective lookup tables, mirroring the Rails schema; this model carries
/// the ids, not embedded records. Hierarchy is a self-reference via
/// [`WorkPackage::parent_id`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkPackage {
    /// Primary key. `None` for a new (unpersisted) record.
    pub id: Option<Id>,
    /// One-line headline. Required.
    pub subject: String,
    /// Long-form body (markdown → HTML via [`Formattable`]).
    pub description: Formattable,
    /// Owning project (`project_id` FK). Required — every WP is scoped.
    pub project_id: Id,
    /// Work-package type FK (Task / Bug / Feature / …).
    pub type_id: Id,
    /// Status FK (New / In progress / Closed / …).
    pub status_id: Id,
    /// Priority FK (Low / Normal / High / …). Optional — the OpenProject
    /// column is nullable (`op-db` carries `Option<i64>`), so a work package
    /// may legitimately have no explicit priority.
    pub priority_id: Option<Id>,
    /// Author FK — who created the WP.
    pub author_id: Id,
    /// Assignee FK — who is working on it (optional).
    pub assigned_to_id: Option<Id>,
    /// Responsible/accountable FK (optional).
    pub responsible_id: Option<Id>,
    /// Parent WP FK for the hierarchy (optional; `None` = top-level).
    pub parent_id: Option<Id>,
    /// Scheduled start (optional).
    pub start_date: Option<NaiveDate>,
    /// Scheduled finish (optional).
    pub due_date: Option<NaiveDate>,
    /// Estimated effort in hours (optional).
    pub estimated_hours: Option<f64>,
    /// Progress `0..=100`.
    pub done_ratio: DoneRatio,
    /// Optimistic-locking counter (mirrors Rails' `lock_version`).
    pub lock_version: i32,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last-update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl WorkPackage {
    /// Create a new (unpersisted) work package from its required (NOT NULL)
    /// attributes. Timestamps are stamped now; optional fields — including
    /// `priority_id` (nullable in OpenProject) — default to empty / `None`;
    /// `done_ratio` starts at `0` and `lock_version` at `0`. Attach an
    /// explicit priority with [`WorkPackage::with_priority`].
    pub fn new(
        subject: impl Into<String>,
        project_id: Id,
        type_id: Id,
        status_id: Id,
        author_id: Id,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            subject: subject.into(),
            description: Formattable::default(),
            project_id,
            type_id,
            status_id,
            priority_id: None,
            author_id,
            assigned_to_id: None,
            responsible_id: None,
            parent_id: None,
            start_date: None,
            due_date: None,
            estimated_hours: None,
            done_ratio: DoneRatio::default(),
            lock_version: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Set the description body.
    pub fn with_description(mut self, description: Formattable) -> Self {
        self.description = description;
        self
    }

    /// Assign the WP to a user.
    pub fn with_assignee(mut self, user_id: Id) -> Self {
        self.assigned_to_id = Some(user_id);
        self
    }

    /// Attach an explicit priority (the column is otherwise nullable).
    pub fn with_priority(mut self, priority_id: Id) -> Self {
        self.priority_id = Some(priority_id);
        self
    }

    /// Nest this WP under a parent (builds the hierarchy).
    pub fn with_parent(mut self, parent_id: Id) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set the start / due schedule.
    pub fn with_dates(mut self, start: Option<NaiveDate>, due: Option<NaiveDate>) -> Self {
        self.start_date = start;
        self.due_date = due;
        self
    }

    /// Set progress (clamped to `0..=100`).
    pub fn with_done_ratio(mut self, done_ratio: impl Into<DoneRatio>) -> Self {
        self.done_ratio = done_ratio.into();
        self
    }

    /// Fully complete (`done_ratio == 100`).
    pub fn is_complete(&self) -> bool {
        self.done_ratio.is_complete()
    }

    /// Whether this WP is nested under a parent.
    pub fn is_child(&self) -> bool {
        self.parent_id.is_some()
    }

    /// Whether it has an assignee.
    pub fn has_assignee(&self) -> bool {
        self.assigned_to_id.is_some()
    }

    /// Overdue relative to `today`: a due date strictly in the past **and**
    /// the work package still **open**.
    ///
    /// Openness is a property of the *status* record (OpenProject's
    /// `Status.is_closed`), **not** of `done_ratio` — a closed status can sit
    /// below 100%, and an open one can reach it — so the caller supplies
    /// `status_is_closed` rather than the helper inferring it from progress.
    /// This matches `op-queries`' overdue scope (`overdue().open()`). A WP with
    /// no due date, or a closed one, is never overdue.
    pub fn is_overdue(&self, today: NaiveDate, status_is_closed: bool) -> bool {
        !status_is_closed && self.due_date.is_some_and(|due| due < today)
    }
}

impl Identifiable for WorkPackage {
    fn id(&self) -> Option<Id> {
        self.id
    }
}

impl Timestamped for WorkPackage {
    fn created_at(&self) -> Option<DateTime<Utc>> {
        Some(self.created_at)
    }
    fn updated_at(&self) -> Option<DateTime<Utc>> {
        Some(self.updated_at)
    }
}

impl ProjectScoped for WorkPackage {
    fn project_id(&self) -> Option<Id> {
        Some(self.project_id)
    }
}

impl Lockable for WorkPackage {
    fn lock_version(&self) -> i32 {
        self.lock_version
    }
}

impl Entity for WorkPackage {
    const TABLE_NAME: &'static str = "work_packages";
    const TYPE_NAME: &'static str = "WorkPackage";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WorkPackage {
        // subject, project, type, status, author (priority is optional)
        WorkPackage::new("Fix the thing", 1, 2, 3, 10)
    }

    #[test]
    fn new_sets_required_fields_and_sane_defaults() {
        let wp = sample();
        assert_eq!(wp.subject, "Fix the thing");
        assert_eq!(wp.project_id, 1);
        assert_eq!(wp.type_id, 2);
        assert_eq!(wp.status_id, 3);
        assert_eq!(wp.author_id, 10);
        // Priority is nullable and unset by `new` (matches op-db Option<i64>).
        assert!(wp.priority_id.is_none());
        // Unpersisted, no optional relations, zero progress, lock 0.
        assert!(wp.id.is_none());
        assert!(!wp.is_child());
        assert!(!wp.has_assignee());
        assert_eq!(wp.done_ratio.value(), 0);
        assert_eq!(wp.lock_version, 0);
        assert!(wp.description.is_empty());
    }

    #[test]
    fn done_ratio_clamps_into_0_100() {
        assert_eq!(DoneRatio::new(0).value(), 0);
        assert_eq!(DoneRatio::new(50).value(), 50);
        assert_eq!(DoneRatio::new(100).value(), 100);
        assert_eq!(DoneRatio::new(250).value(), 100, "over-100 clamps");
        assert_eq!(DoneRatio::from(200u8).value(), 100);
        assert!(DoneRatio::new(100).is_complete());
        assert!(!DoneRatio::new(99).is_complete());
    }

    #[test]
    fn builder_sets_optional_relations_and_progress() {
        let wp = sample()
            .with_assignee(20)
            .with_parent(7)
            .with_priority(5)
            .with_done_ratio(150u8) // clamps to 100
            .with_description(Formattable::plain("body"));
        assert_eq!(wp.assigned_to_id, Some(20));
        assert!(wp.has_assignee());
        assert_eq!(wp.parent_id, Some(7));
        assert!(wp.is_child());
        assert_eq!(wp.priority_id, Some(5), "with_priority sets the FK");
        assert!(wp.is_complete(), "done_ratio clamped to 100 → complete");
        assert_eq!(wp.description.raw, "body");
    }

    #[test]
    fn is_overdue_keys_off_open_status_not_done_ratio() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let yesterday = NaiveDate::from_ymd_opt(2026, 6, 30).unwrap();
        let tomorrow = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();
        const OPEN: bool = false; // status_is_closed = false
        const CLOSED: bool = true;

        let past_due = sample().with_dates(None, Some(yesterday));
        // Open + past due → overdue.
        assert!(past_due.is_overdue(today, OPEN));
        // Closed + past due → NOT overdue (even though done_ratio is 0 < 100).
        assert!(!past_due.is_overdue(today, CLOSED));

        // Completion no longer gates it: an OPEN, past-due WP at 100% is still
        // overdue — only the status closes it out (the bug this PR fixes).
        let done_but_open = sample()
            .with_dates(None, Some(yesterday))
            .with_done_ratio(100u8);
        assert!(done_but_open.is_overdue(today, OPEN));

        // Future due → NOT overdue regardless of status.
        let future = sample().with_dates(None, Some(tomorrow));
        assert!(!future.is_overdue(today, OPEN));

        // No due date → never overdue.
        assert!(!sample().is_overdue(today, OPEN));
    }

    #[test]
    fn implements_core_entity_traits() {
        let wp = sample();
        // Identifiable
        assert!(wp.id().is_none());
        assert!(wp.is_new_record());
        // Timestamped
        assert!(wp.created_at().is_some());
        assert!(wp.updated_at().is_some());
        // ProjectScoped
        assert_eq!(wp.project_id(), Some(1));
        // Lockable
        assert_eq!(Lockable::lock_version(&wp), 0);
        // Entity consts
        assert_eq!(WorkPackage::TABLE_NAME, "work_packages");
        assert_eq!(WorkPackage::TYPE_NAME, "WorkPackage");
    }

    #[test]
    fn round_trips_through_json() {
        let wp = sample().with_assignee(20).with_done_ratio(40u8);
        let json = serde_json::to_string(&wp).unwrap();
        let back: WorkPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.subject, wp.subject);
        assert_eq!(back.assigned_to_id, Some(20));
        assert_eq!(back.done_ratio.value(), 40);
    }
}
