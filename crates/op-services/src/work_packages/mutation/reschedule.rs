//! `reschedule_related` on a [`PgCreateScope`]: `WorkPackages::CreateService#reschedule_related`
//! (`create_service.rb:90-110`) and `WorkPackages::UpdateService#reschedule_related` /
//! `#update_related` (`update_service.rb:62-65`, :149-170), over `SetScheduleService`
//! (`scheduling.rs` holds the pure part).
//!
//! What this file does is what Ruby does around the pure part: it loads the work packages the
//! rescheduling needs (`ScheduleDependency#preload_scheduling_data`, `schedule_dependency.rb:
//! 160-190`, with `WorkPackage.for_scheduling`, `work_packages/scopes/for_scheduling.rb`), runs
//! [`schedule_following`], and saves every altered work package once
//! (`persist_reschedule_changes` / `consolidated_calls`: each save is `save(validate: false)`, a
//! `lock_version` bump, `updated_at` and one journal with the cause the pure part assigned).
//!
//! Not ported, each precisely:
//!
//! * `work_package_hierarchies`: this schema has no closure table, so the hierarchy is read from
//!   `parent_id` (the same substitution `has_scheduling_predecessors` makes).
//! * `SetAttributesService`'s duration derivation: nothing in the create or update path writes
//!   `work_packages.duration` from the dates (`update_duration`/`derive_dates`,
//!   `set_attributes_service.rb`), so a work package created or edited here carries a NULL
//!   duration until a rescheduling sets one. A row that carries a duration (written by
//!   OpenProject, or by a migration) is rescheduled as Ruby does (`determine_due_date` keeps the
//!   duration), a row without one keeps its due date.
//! * `update_descendants` on a project change (`update_service.rb:67-82`).
//! * The `WorkPackage.visible(user)` check of the former parent (`update_service.rb:155`) models
//!   only what `add_creator_as_watcher` models: an active project and `view_work_packages`
//!   through a membership, or an admin. Non-member access to public projects and enabled modules
//!   are not modelled.
//! * `parent_id_changed?` of `SetScheduleService#schedule_following` (:84-90) is never true here:
//!   both services call it after the save, when Ruby has cleared the dirty state.
//! * Notifications.

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::NaiveDate;

use super::{db_failure, PgCreateScope, StoreFailure, JOURNAL_AGGREGATION_TIME_MINUTES};
use crate::work_packages::scheduling::{
    schedule_following, Calendar, FollowsRelation, SchedWp, ScheduleInput,
};
use op_core::traits::Id;

#[derive(Debug, sqlx::FromRow)]
struct SchedRow {
    id: Id,
    parent_id: Option<Id>,
    schedule_manually: bool,
    ignore_non_working_days: bool,
    start_date: Option<NaiveDate>,
    due_date: Option<NaiveDate>,
    duration: Option<i32>,
    lock_version: i32,
}

impl From<SchedRow> for SchedWp {
    fn from(r: SchedRow) -> Self {
        SchedWp {
            id: r.id,
            parent_id: r.parent_id,
            schedule_manually: r.schedule_manually,
            ignore_non_working_days: r.ignore_non_working_days,
            start_date: r.start_date,
            due_date: r.due_date,
            duration: r.duration,
            lock_version: r.lock_version,
        }
    }
}

const SCHED_COLUMNS: &str = "id, parent_id, schedule_manually, ignore_non_working_days, \
                             start_date, due_date, duration, lock_version";

/// Whether an edit changed an attribute that starts a rescheduling:
/// `%i(start_date due_date parent parent_id schedule_manually).intersect?(changed_attributes)`
/// (`set_schedule_service.rb:42-44`). An edit cannot write `schedule_manually` here.
pub fn scheduling_changes(
    before: &op_db::work_packages::WorkPackageRow,
    after: &op_db::work_packages::WorkPackageRow,
) -> bool {
    before.start_date != after.start_date
        || before.due_date != after.due_date
        || before.parent_id != after.parent_id
}

impl PgCreateScope {
    /// `WorkPackages::CreateService#reschedule_related` for the created work package: the new work
    /// package is forced to keep an automatic scheduling mode (`switching_to_automatic_mode`,
    /// `create_service.rb:92-94`) and nothing initiates the change, so no journal carries a cause
    /// (`initiated_by` is nil, `set_schedule_service.rb:185`). Returns the ids saved.
    pub async fn reschedule_after_create(
        &self,
        work_package_id: Id,
        user_id: Id,
    ) -> Result<Vec<Id>, StoreFailure> {
        let manual: Option<bool> =
            sqlx::query_scalar("SELECT schedule_manually FROM work_packages WHERE id = $1")
                .bind(work_package_id)
                .fetch_optional(&mut **self.tx.lock().await)
                .await
                .map_err(db_failure)?;
        let switching: Vec<Id> = match manual {
            Some(false) => vec![work_package_id],
            _ => Vec::new(),
        };
        self.reschedule_related(&[work_package_id], &switching, None, user_id)
            .await
    }

    /// `WorkPackages::UpdateService#reschedule_related` (`update_service.rb:149-161`): the edited
    /// work package and, when its parent just changed, its former parent (if the user may see it,
    /// :155), initiated by the edited work package (`cause_of_rescheduling`, :41).
    pub async fn reschedule_after_edit(
        &self,
        work_package_id: Id,
        parent_before: Option<Id>,
        parent_after: Option<Id>,
        user_id: Id,
    ) -> Result<Vec<Id>, StoreFailure> {
        let mut moved = vec![work_package_id];
        // `parent_just_changed?` (:164-166): the parent changed and there was one.
        if parent_before != parent_after {
            if let Some(former) = parent_before {
                if self.user_may_view(user_id, former).await? {
                    moved.push(former);
                }
            }
        }
        self.reschedule_related(&moved, &[], Some(work_package_id), user_id)
            .await
    }

    /// `WorkPackage.visible(user).find_by(id:)` as far as this schema models it (see the module
    /// documentation): the work package exists, its project is active and the user is an admin or
    /// holds `view_work_packages` through a membership of that project.
    async fn user_may_view(&self, user_id: Id, work_package_id: Id) -> Result<bool, StoreFailure> {
        sqlx::query_scalar(
            "SELECT EXISTS ( \
                 SELECT 1 FROM work_packages w \
                 JOIN projects p ON p.id = w.project_id AND p.active \
                 JOIN users u ON u.id = $1 AND u.status <> 3 \
                 WHERE w.id = $2 AND (u.admin OR EXISTS ( \
                     SELECT 1 FROM members m \
                     JOIN member_roles mr ON mr.member_id = m.id \
                     JOIN role_permissions rp ON rp.role_id = mr.role_id \
                     WHERE m.project_id = w.project_id AND m.user_id = u.id \
                       AND m.entity_type IS NULL AND rp.permission = 'view_work_packages')))",
        )
        .bind(user_id)
        .bind(work_package_id)
        .fetch_one(&mut **self.tx.lock().await)
        .await
        .map_err(db_failure)
    }

    /// `SetScheduleService#call` followed by saving what it altered. Each altered work package is
    /// saved once, `UPDATE ... WHERE id = ? AND lock_version = <as loaded>` (ActiveRecord
    /// optimistic locking: no row updated raises `StaleObjectError`, here
    /// [`StoreFailure::Conflict`], which rolls the create or update back), with a `lock_version`
    /// bump and `updated_at`, and journaled with the cause the rescheduling assigned (none is
    /// `NoCause`) through [`PgCreateScope::write_journal`], so the aggregation rules apply.
    /// Returns the saved ids in schedule order.
    pub async fn reschedule_related(
        &self,
        moved: &[Id],
        switching_to_automatic_mode: &[Id],
        initiated_by: Option<Id>,
        user_id: Id,
    ) -> Result<Vec<Id>, StoreFailure> {
        let switching: HashSet<Id> = switching_to_automatic_mode.iter().copied().collect();

        // The moved work packages, in the order given.
        let loaded = self.load_sched(moved).await?;
        let mut moved_nodes: Vec<SchedWp> = Vec::new();
        for id in moved {
            if let Some(w) = loaded.iter().find(|w| w.id == *id) {
                moved_nodes.push(w.clone());
            }
        }
        let moved_ids: Vec<Id> = moved_nodes.iter().map(|w| w.id).collect();

        // `moving_work_packages` (`WorkPackage.for_scheduling`).
        let moving_ids = self.for_scheduling(&moved_ids, &switching).await?;
        let mut known: Vec<SchedWp> = moved_nodes;
        known.extend(self.load_sched(&moving_ids).await?);

        // `fetch_unmoving_descendants`: the descendants of everything loaded so far.
        let known_ids: Vec<Id> = known.iter().map(|w| w.id).collect();
        let descendants: Vec<Id> = {
            let mut tx = self.tx.lock().await;
            sqlx::query_scalar(
                "WITH RECURSIVE down(id) AS ( \
                     SELECT id FROM work_packages WHERE parent_id = ANY($1) \
                     UNION \
                     SELECT w.id FROM work_packages w JOIN down d ON w.parent_id = d.id) \
                 SELECT id FROM down WHERE id <> ALL($1) ORDER BY id",
            )
            .bind(&known_ids)
            .fetch_all(&mut **tx)
            .await
            .map_err(db_failure)?
        };
        known.extend(self.load_sched(&descendants).await?);

        // `preload_follows_relations`, then `fetch_unmoving_predecessors`.
        let known_ids: Vec<Id> = known.iter().map(|w| w.id).collect();
        let relations: Vec<(Id, Id, Option<i32>)> = {
            let mut tx = self.tx.lock().await;
            sqlx::query_as(
                "SELECT from_id, to_id, lag FROM relations \
                 WHERE relation_type = 'follows' AND from_id = ANY($1) ORDER BY id",
            )
            .bind(&known_ids)
            .fetch_all(&mut **tx)
            .await
            .map_err(db_failure)?
        };
        let loaded_ids: HashSet<Id> = known_ids.iter().copied().collect();
        let missing: Vec<Id> = relations
            .iter()
            .map(|r| r.1)
            .filter(|id| !loaded_ids.contains(id))
            .collect();
        known.extend(self.load_sched(&missing).await?);

        let calendar = Calendar {
            non_working_dates: {
                let mut tx = self.tx.lock().await;
                sqlx::query_scalar::<_, NaiveDate>("SELECT date FROM non_working_days")
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(db_failure)?
                    .into_iter()
                    .collect()
            },
        };
        let originals: HashMap<Id, SchedWp> = known.iter().map(|w| (w.id, w.clone())).collect();
        let input = ScheduleInput {
            known,
            moved: moved_ids,
            moving: moving_ids,
            relations: relations
                .into_iter()
                .map(|(from_id, to_id, lag)| FollowsRelation {
                    from_id,
                    to_id,
                    lag,
                })
                .collect(),
            switching_to_automatic_mode: switching,
            initiated_by,
            calendar,
        };
        let scheduled = schedule_following(&input).map_err(|e| StoreFailure::Other(e.0))?;

        let mut saved = Vec::new();
        for altered in &scheduled.altered {
            let node = &scheduled.nodes[&altered.id];
            // `select { it.result.changed? }` (`persist_reschedule_changes`, :99-101).
            if originals.get(&altered.id) == Some(node) {
                continue;
            }
            {
                let mut tx = self.tx.lock().await;
                let updated = sqlx::query(
                    "UPDATE work_packages SET start_date = $2, due_date = $3, duration = $4, \
                         schedule_manually = $5, lock_version = lock_version + 1, \
                         updated_at = statement_timestamp() \
                     WHERE id = $1 AND lock_version = $6",
                )
                .bind(node.id)
                .bind(node.start_date)
                .bind(node.due_date)
                .bind(node.duration)
                .bind(node.schedule_manually)
                .bind(node.lock_version)
                .execute(&mut **tx)
                .await
                .map_err(db_failure)?
                .rows_affected();
                if updated != 1 {
                    return Err(StoreFailure::Conflict(format!(
                        "work package {} was modified by another user while it was rescheduled",
                        node.id
                    )));
                }
            }
            let cause = altered
                .cause
                .clone()
                .unwrap_or_else(|| serde_json::json!({}));
            self.write_journal(node.id, user_id, JOURNAL_AGGREGATION_TIME_MINUTES, &cause)
                .await?;
            saved.push(node.id);
        }
        Ok(saved)
    }

    async fn load_sched(&self, ids: &[Id]) -> Result<Vec<SchedWp>, StoreFailure> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self.tx.lock().await;
        let rows: Vec<SchedRow> = sqlx::query_as(&format!(
            "SELECT {SCHED_COLUMNS} FROM work_packages WHERE id = ANY($1) ORDER BY id"
        ))
        .bind(ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(db_failure)?;
        Ok(rows.into_iter().map(SchedWp::from).collect())
    }

    /// `WorkPackage.for_scheduling(moved, switching_to_automatic_mode:)`
    /// (`work_packages/scopes/for_scheduling.rb:119-127`, the recursive CTE of
    /// `scheduling_paths_sql`, :171-262), over `parent_id` instead of `work_package_hierarchies`.
    ///
    /// The CTE walks rows `(id, manually, hierarchy_up, origin)` from the moved work packages
    /// `(id, false, false, true)`. From a row it reaches: the successors (`follows` relations whose
    /// predecessor is the row), unless the row is manual; and, unless the row is manual (an origin
    /// row always), its children when the walk has not just come up, and all its ancestors
    /// (`hierarchy_up`). A reached work package is `manually` if it is itself manual and not
    /// switching to automatic, or if all its descendants are (`bool_and` over them, switching ones
    /// counting as automatic) and the walk did not go up from an origin. The result is the ids
    /// reached with `manually = false`, minus the moved ones; sorted by id, as Ruby's `IN` query
    /// has no order.
    async fn for_scheduling(
        &self,
        moved: &[Id],
        switching: &HashSet<Id>,
    ) -> Result<Vec<Id>, StoreFailure> {
        if moved.is_empty() {
            return Ok(Vec::new());
        }
        let switching_ids: Vec<Id> = switching.iter().copied().collect();
        // (id, manually, hierarchy_up, origin)
        let mut seen: BTreeSet<(Id, bool, bool, bool)> = BTreeSet::new();
        let mut work: Vec<(Id, bool, bool, bool)> = Vec::new();
        for &id in moved {
            if seen.insert((id, false, false, true)) {
                work.push((id, false, false, true));
            }
        }
        while let Some((id, manually, hierarchy_up, origin)) = work.pop() {
            let mut related: BTreeSet<(Id, bool)> = BTreeSet::new();
            let mut tx = self.tx.lock().await;
            if !manually {
                let successors: Vec<Id> = sqlx::query_scalar(
                    "SELECT from_id FROM relations WHERE to_id = $1 AND relation_type = 'follows'",
                )
                .bind(id)
                .fetch_all(&mut **tx)
                .await
                .map_err(db_failure)?;
                related.extend(successors.into_iter().map(|s| (s, false)));
            }
            if !manually || origin {
                if !hierarchy_up {
                    let children: Vec<Id> =
                        sqlx::query_scalar("SELECT id FROM work_packages WHERE parent_id = $1")
                            .bind(id)
                            .fetch_all(&mut **tx)
                            .await
                            .map_err(db_failure)?;
                    related.extend(children.into_iter().map(|c| (c, false)));
                }
                let ancestors: Vec<Id> = sqlx::query_scalar(
                    "WITH RECURSIVE up(id) AS ( \
                         SELECT parent_id FROM work_packages WHERE id = $1 AND parent_id IS NOT NULL \
                         UNION \
                         SELECT p.parent_id FROM up JOIN work_packages p ON p.id = up.id \
                         WHERE p.parent_id IS NOT NULL) \
                     SELECT id FROM up",
                )
                .bind(id)
                .fetch_all(&mut **tx)
                .await
                .map_err(db_failure)?;
                related.extend(ancestors.into_iter().map(|a| (a, true)));
            }
            for (from_id, up) in related {
                let own_manual: Option<bool> =
                    sqlx::query_scalar("SELECT schedule_manually FROM work_packages WHERE id = $1")
                        .bind(from_id)
                        .fetch_optional(&mut **tx)
                        .await
                        .map_err(db_failure)?;
                let descendants_manual: Option<bool> = sqlx::query_scalar(
                    "WITH RECURSIVE down(id) AS ( \
                         SELECT id FROM work_packages WHERE parent_id = $1 \
                         UNION \
                         SELECT w.id FROM work_packages w JOIN down d ON w.parent_id = d.id) \
                     SELECT bool_and(w.schedule_manually AND NOT (w.id = ANY($2))) \
                     FROM work_packages w WHERE w.id IN (SELECT id FROM down)",
                )
                .bind(from_id)
                .bind(&switching_ids)
                .fetch_one(&mut **tx)
                .await
                .map_err(db_failure)?;
                let next_manually = (own_manual.unwrap_or(false) && !switching.contains(&from_id))
                    || (descendants_manual.unwrap_or(false) && !(origin && up));
                let row = (from_id, next_manually, up, false);
                if seen.insert(row) {
                    work.push(row);
                }
            }
        }
        let moved: HashSet<Id> = moved.iter().copied().collect();
        let mut ids: Vec<Id> = seen
            .iter()
            .filter(|(id, manually, _, _)| !manually && !moved.contains(id))
            .map(|(id, ..)| *id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }
}
