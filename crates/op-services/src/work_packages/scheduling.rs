//! `WorkPackages::SetScheduleService`, the pure part: the working-day arithmetic and the dates a
//! rescheduling produces, over work packages already loaded into memory.
//!
//! Ruby (`app/services/work_packages/set_schedule_service.rb`, `schedule_dependency.rb`,
//! `schedule_dependency/dependency.rb`, `schedule_dependency/dependency_graph.rb`,
//! `shared/working_days.rb`, `shared/all_days.rb`, `shared/days.rb`) works on in-memory
//! `WorkPackage` objects that are mutated while the dependencies are walked in schedule order, so
//! a work package scheduled later reads the dates an earlier one just took. [`schedule_following`]
//! does the same over a map of [`SchedWp`]. Loading the work packages, the relations and the
//! calendar, and saving the result, is the store's job (`mutation/reschedule.rs`).
//!
//! The one place this crate has no settings table is `Setting.working_days`: its OpenProject
//! default `Array(1..5)` (`config/constants/settings/definition.rb:1548-1553`, ISO weekdays,
//! 1 = Monday, 7 = Sunday) is the constant [`WORKING_DAYS`], as `JOURNAL_AGGREGATION_TIME_MINUTES`
//! is for its setting.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use chrono::{Datelike, Duration, NaiveDate};
use op_core::traits::Id;

/// `Setting.working_days`, at its default (`definition.rb:1548-1553`): Monday to Friday.
/// `WeekDay#working` is `Setting.working_days.empty? || day.in?(Setting.working_days)`
/// (`app/models/week_day.rb`); the constant is never empty.
pub const WORKING_DAYS: [u32; 5] = [1, 2, 3, 4, 5];

/// `Journal::CausedByWorkPackageRelatedChange#type`
/// (`app/models/journal/caused_by_work_package_related_change.rb:31-37`).
pub const CAUSE_RELATED_CHANGE: &str = "work_package_related_changed_times";

/// `Journal::CausedByWorkPackageRelatedChange.new(work_package:).to_hash`
/// (`CauseOfChange::Base#to_hash`, `cause_of_change/base.rb:38-40`).
pub fn related_change_cause(work_package_id: Id) -> serde_json::Value {
    serde_json::json!({
        "type": CAUSE_RELATED_CHANGE,
        "work_package_id": work_package_id,
    })
}

/// Why a rescheduling could not be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleError(pub String);

impl std::fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The dates that are not working days besides the non-working week days
/// (`NonWorkingDay.pluck(:date)`, `working_days.rb:178-181`).
#[derive(Debug, Clone, Default)]
pub struct Calendar {
    pub non_working_dates: HashSet<NaiveDate>,
}

impl Calendar {
    /// `working_week_day?` and `working_specific_date?` (`working_days.rb:150-156`).
    pub fn working(&self, date: NaiveDate) -> bool {
        WORKING_DAYS.contains(&date.weekday().number_from_monday())
            && !self.non_working_dates.contains(&date)
    }
}

/// `WorkPackages::Shared::Days.for(work_package)` (`shared/days.rb:35-37`): `AllDays` when the work
/// package ignores non-working days, `WorkingDays` otherwise.
#[derive(Debug, Clone, Copy)]
pub struct Days<'a> {
    ignore_non_working_days: bool,
    calendar: &'a Calendar,
}

impl<'a> Days<'a> {
    pub fn for_work_package(ignore_non_working_days: bool, calendar: &'a Calendar) -> Self {
        Self {
            ignore_non_working_days,
            calendar,
        }
    }

    fn working(&self, date: NaiveDate) -> bool {
        self.ignore_non_working_days || self.calendar.working(date)
    }

    /// `duration` (`working_days.rb:49-53`, `all_days.rb:35-39`): the days from `start` to `due`,
    /// both included, that count; `nil` unless both dates are given.
    pub fn duration(&self, start: Option<NaiveDate>, due: Option<NaiveDate>) -> Option<i32> {
        let (start, due) = (start?, due?);
        let mut count = 0;
        let mut day = start;
        while day <= due {
            if self.working(day) {
                count += 1;
            }
            day += Duration::days(1);
        }
        Some(count)
    }

    /// `due_date(start_date, duration)` (`working_days.rb:77-87`, `all_days.rb:55-60`). A duration
    /// that is not strictly positive raises in Ruby (`assert_strictly_positive_duration`).
    pub fn due_date(&self, start: NaiveDate, duration: i32) -> Result<NaiveDate, ScheduleError> {
        if duration <= 0 {
            return Err(ScheduleError("duration must be strictly positive".into()));
        }
        if self.ignore_non_working_days {
            return Ok(start + Duration::days(i64::from(duration) - 1));
        }
        let mut remaining = duration;
        let mut due = self.soonest_working_day(start);
        while !(remaining <= 1 && self.working(due)) {
            due += Duration::days(1);
            if self.working(due) {
                remaining -= 1;
            }
        }
        Ok(due)
    }

    /// `start_date(due_date, duration)` (`working_days.rb:68-76`, `all_days.rb:51-56`): the start
    /// of a work package that ends on `due` and takes `duration` days. The caller has refused a
    /// duration that is not strictly positive.
    pub fn start_date(&self, due: NaiveDate, duration: i32) -> NaiveDate {
        if self.ignore_non_working_days {
            return due - Duration::days(i64::from(duration) - 1);
        }
        let mut remaining = duration;
        let mut start = due;
        while !self.working(start) {
            start -= Duration::days(1);
        }
        while !(remaining <= 1 && self.working(start)) {
            start -= Duration::days(1);
            if self.working(start) {
                remaining -= 1;
            }
        }
        start
    }

    /// `soonest_working_day` (`working_days.rb:89-97`, `all_days.rb:67-69`).
    pub fn soonest_working_day(&self, date: NaiveDate) -> NaiveDate {
        let mut date = date;
        while !self.working(date) {
            date += Duration::days(1);
        }
        date
    }
}

/// `WorkingDays#with_lag` (`working_days.rb:61-71`, `add_lag` :134-142, `remove_lag` :144-153).
/// `AllDays#with_lag` delegates to `WorkingDays` too (`all_days.rb:46-49`): a lag is always in
/// working days, whatever the work package ignores.
pub fn with_lag(calendar: &Calendar, date: NaiveDate, lag: Option<i32>) -> NaiveDate {
    let lag = lag.unwrap_or(0);
    if lag >= 0 {
        let mut remaining = lag;
        let mut day = date + Duration::days(1);
        while remaining != 0 {
            if calendar.working(day) {
                remaining -= 1;
            }
            day += Duration::days(1);
        }
        day
    } else {
        let mut remaining = -lag;
        let mut day = date;
        while remaining != 0 {
            if calendar.working(day) {
                remaining -= 1;
            }
            if remaining == 0 {
                continue;
            }
            day -= Duration::days(1);
        }
        day
    }
}

/// The three date attributes of a work package being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateAttrs {
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub duration: Option<i32>,
}

/// The date part of `WorkPackages::SetAttributesService#set_calculated_attributes`
/// (`set_attributes_service.rb:92-107`) for a create or an update, in Ruby's order:
/// `unify_milestone_dates` (:383-388, a milestone has one date: the due date, else the start),
/// `shift_dates_to_soonest_working_days` (:390-395, unless the work package ignores non-working
/// days), `update_duration_to_one_day_for_milestones` (:397-399) and
/// `update_derivable_date_attribute` (:158-185): of duration, due date and start date, the first
/// (in that order) that was not given and whose other two are present is derived from them, else
/// the first that was not given and whose other two have exactly one present
/// (`derivable_date_attribute_by_others_presence` / `_absence`, :113-153). The duration is never a
/// submitted attribute here, so it is always "not given". An invalid duration (zero or negative)
/// derives neither date.
pub fn derive_dates(
    calendar: &Calendar,
    ignore_non_working_days: bool,
    milestone: bool,
    start_given: bool,
    due_given: bool,
    attrs: DateAttrs,
) -> DateAttrs {
    let days = Days::for_work_package(ignore_non_working_days, calendar);
    let DateAttrs {
        mut start_date,
        mut due_date,
        mut duration,
    } = attrs;
    if milestone {
        let unified = due_date.or(start_date);
        start_date = unified;
        due_date = unified;
    }
    if !ignore_non_working_days {
        start_date = start_date.map(|d| days.soonest_working_day(d));
        due_date = due_date.map(|d| days.soonest_working_day(d));
    }
    if milestone {
        duration = Some(1);
    }
    // 0 = duration, 1 = due date, 2 = start date.
    let given = [false, due_given, start_given];
    let present = [duration.is_some(), due_date.is_some(), start_date.is_some()];
    let others = |f: usize| [0usize, 1, 2].into_iter().filter(move |o| *o != f);
    let by_presence = (0..3).find(|&f| !given[f] && others(f).all(|o| present[o]));
    let field = by_presence
        .or_else(|| (0..3).find(|&f| !given[f] && others(f).filter(|&o| present[o]).count() == 1));
    let valid_duration = !duration.is_some_and(|d| d <= 0);
    match field {
        Some(0) => {
            duration = if milestone {
                Some(1)
            } else {
                days.duration(start_date, due_date)
            };
        }
        Some(1) if valid_duration => {
            due_date = start_date
                .zip(duration)
                .map(|(s, d)| days.due_date(s, d).unwrap_or(s));
        }
        Some(2) if valid_duration => {
            start_date = due_date.zip(duration).map(|(u, d)| days.start_date(u, d));
        }
        _ => {}
    }
    DateAttrs {
        start_date,
        due_date,
        duration,
    }
}

/// A work package as scheduling sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct SchedWp {
    pub id: Id,
    pub parent_id: Option<Id>,
    pub schedule_manually: bool,
    pub ignore_non_working_days: bool,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub duration: Option<i32>,
    /// The `lock_version` as loaded: the save checks it.
    pub lock_version: i32,
}

/// A `follows` relation: `from_id` (the successor) follows `to_id` (the predecessor)
/// (`relation.rb:196-201`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowsRelation {
    pub from_id: Id,
    pub to_id: Id,
    pub lag: Option<i32>,
}

/// What one `SetScheduleService#call` walks over.
#[derive(Debug, Clone)]
pub struct ScheduleInput {
    /// Every preloaded work package, in load order (`known_work_packages`,
    /// `schedule_dependency.rb:213-233`): moved, moving, unmoving descendants, unmoving
    /// predecessors.
    pub known: Vec<SchedWp>,
    /// The work packages whose dates just changed (`moved_work_packages`).
    pub moved: Vec<Id>,
    /// The work packages `WorkPackage.for_scheduling` found (`moving_work_packages`).
    pub moving: Vec<Id>,
    /// `Relation.follows.where(from_id: known)`.
    pub relations: Vec<FollowsRelation>,
    /// `switching_to_automatic_mode`.
    pub switching_to_automatic_mode: HashSet<Id>,
    /// `initiated_by`: the work package whose change causes the rescheduling; `None` for a create.
    pub initiated_by: Option<Id>,
    pub calendar: Calendar,
}

/// A work package the rescheduling altered, with the journal cause Ruby assigned to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Altered {
    pub id: Id,
    /// `work_package.journal_cause`: `None` is `NoCause`.
    pub cause: Option<serde_json::Value>,
}

/// The result of [`schedule_following`]: the work packages as they are after, and the altered ones
/// in schedule order.
#[derive(Debug, Clone)]
pub struct Scheduled {
    pub nodes: BTreeMap<Id, SchedWp>,
    pub altered: Vec<Altered>,
}

struct Graph {
    children: HashMap<Id, Vec<Id>>,
    parent: HashMap<Id, Option<Id>>,
}

impl Graph {
    fn new(known: &[SchedWp]) -> Self {
        let mut children: HashMap<Id, Vec<Id>> = HashMap::new();
        for w in known {
            if let Some(p) = w.parent_id {
                children.entry(p).or_default().push(w.id);
            }
        }
        Self {
            children,
            parent: known.iter().map(|w| (w.id, w.parent_id)).collect(),
        }
    }

    /// `ScheduleDependency#descendants` (:121-143): breadth first, cycles cut.
    fn descendants(&self, id: Id) -> Vec<Id> {
        let mut queue = VecDeque::from([id]);
        let mut processed = HashSet::new();
        let mut out = Vec::new();
        while let Some(current) = queue.pop_front() {
            processed.insert(current);
            let kids: Vec<Id> = self
                .children
                .get(&current)
                .map(|c| {
                    c.iter()
                        .copied()
                        .filter(|k| !processed.contains(k))
                        .collect()
                })
                .unwrap_or_default();
            out.extend(&kids);
            queue.extend(kids);
        }
        out
    }

    /// `ScheduleDependency#parent_of`: only a known parent.
    fn parent_of(&self, id: Id) -> Option<Id> {
        self.parent
            .get(&id)
            .copied()
            .flatten()
            .filter(|p| self.parent.contains_key(p))
    }
}

/// `ScheduleDependency#automatically_scheduled_ancestors` (:96-119), over the current state.
fn automatically_scheduled_ancestors(
    graph: &Graph,
    nodes: &BTreeMap<Id, SchedWp>,
    id: Id,
) -> Vec<Id> {
    let mut queue = VecDeque::from([id]);
    let mut processed = HashSet::new();
    let mut out = Vec::new();
    while let Some(current) = queue.pop_front() {
        processed.insert(current);
        if let Some(parent) = graph.parent_of(current) {
            if !nodes[&parent].schedule_manually {
                if parent != id {
                    out.push(parent);
                }
                if !processed.contains(&parent) {
                    queue.push_back(parent);
                }
            }
        }
    }
    out
}

/// One `Dependency` (`schedule_dependency/dependency.rb`): who it is, its follows relations (as
/// indexes into the input's relations) and the ids its dates depend on.
struct Dep {
    id: Id,
    follows: Vec<usize>,
    dependent_ids: Vec<Id>,
}

/// `DependencyGraph#schedule_order` (`dependency_graph.rb:34-51`): each dependency is inserted
/// after the last already inserted one it depends on, directly or transitively, else first.
fn schedule_order(deps: &[Dep]) -> Vec<usize> {
    let direct: HashMap<Id, &Vec<Id>> = deps.iter().map(|d| (d.id, &d.dependent_ids)).collect();
    let mut cache: HashMap<Id, HashSet<Id>> = HashMap::new();
    let mut full = |id: Id| -> HashSet<Id> {
        cache
            .entry(id)
            .or_insert_with(|| {
                let mut visited = HashSet::new();
                let mut stack = vec![id];
                let mut result = HashSet::new();
                while let Some(current) = stack.pop() {
                    visited.insert(current);
                    for &dep in direct.get(&current).map_or(&[][..], |v| &v[..]) {
                        if dep != id {
                            result.insert(dep);
                        }
                        if !visited.contains(&dep) {
                            stack.push(dep);
                        }
                    }
                }
                result
            })
            .clone()
    };
    let mut order: Vec<usize> = Vec::new();
    for (i, dep) in deps.iter().enumerate() {
        let needs = full(dep.id);
        match order.iter().rposition(|&j| needs.contains(&deps[j].id)) {
            Some(index) => order.insert(index + 1, i),
            None => order.insert(0, i),
        }
    }
    order
}

/// `WorkPackages::SetScheduleService#schedule_following` (`set_schedule_service.rb:80-103`) for
/// `input`: the dependencies are the moved and moving work packages that are automatically
/// scheduled (or switching to it, `schedule_dependency.rb:197-201`), walked in schedule order;
/// each one is switched to automatic mode if it is in `switching_to_automatic_mode`, then
/// rescheduled by its descendants, else by its predecessors, else (unless forced automatic)
/// switched to manual (:123-130).
pub fn schedule_following(input: &ScheduleInput) -> Result<Scheduled, ScheduleError> {
    let mut nodes: BTreeMap<Id, SchedWp> = input.known.iter().map(|w| (w.id, w.clone())).collect();
    let originals = nodes.clone();
    let graph = Graph::new(&input.known);
    let moving_set: HashSet<Id> = input.moved.iter().chain(&input.moving).copied().collect();
    let switching = &input.switching_to_automatic_mode;

    // `create_dependencies`: moved and moving, in that order, the automatically scheduled ones.
    let mut dep_ids: Vec<Id> = Vec::new();
    for id in input.moved.iter().chain(&input.moving) {
        let Some(w) = nodes.get(id) else { continue };
        if !dep_ids.contains(id) && (!w.schedule_manually || switching.contains(id)) {
            dep_ids.push(*id);
        }
    }
    let mut by_from: HashMap<Id, Vec<usize>> = HashMap::new();
    for (i, r) in input.relations.iter().enumerate() {
        by_from.entry(r.from_id).or_default().push(i);
    }
    // `follows_relations`: the work package's own and those of its automatically scheduled
    // ancestors (:145-155, 163-165), as the state is when the walk starts.
    let deps: Vec<Dep> = dep_ids
        .iter()
        .map(|&id| {
            let mut owners = vec![id];
            owners.extend(automatically_scheduled_ancestors(&graph, &nodes, id));
            let mut follows: Vec<usize> = Vec::new();
            for owner in owners {
                for &i in by_from.get(&owner).map_or(&[][..], |v| &v[..]) {
                    if !follows.contains(&i) {
                        follows.push(i);
                    }
                }
            }
            let mut dependent_ids = graph.descendants(id);
            for &i in &follows {
                let to = input.relations[i].to_id;
                if moving_set.contains(&to) {
                    dependent_ids.push(to);
                }
            }
            let mut seen = HashSet::new();
            dependent_ids.retain(|d| seen.insert(*d));
            Dep {
                id,
                follows,
                dependent_ids,
            }
        })
        .collect();

    let mut causes: HashMap<Id, serde_json::Value> = HashMap::new();
    let mut altered = Vec::new();
    for index in schedule_order(&deps) {
        let dep = &deps[index];
        let before = nodes[&dep.id].clone();
        if switching.contains(&dep.id) {
            nodes.get_mut(&dep.id).expect("known").schedule_manually = false;
        }
        reschedule(input, &graph, &originals, &mut nodes, &mut causes, dep)?;
        if nodes[&dep.id] != before {
            altered.push(Altered {
                id: dep.id,
                cause: causes.get(&dep.id).cloned(),
            });
        }
    }
    // A cause assigned in an earlier step stays on the work package (it is an attribute of the
    // object): report the final one.
    for a in &mut altered {
        a.cause = causes.get(&a.id).cloned();
    }
    Ok(Scheduled { nodes, altered })
}

/// `reschedule` (`set_schedule_service.rb:108-116`).
fn reschedule(
    input: &ScheduleInput,
    graph: &Graph,
    originals: &BTreeMap<Id, SchedWp>,
    nodes: &mut BTreeMap<Id, SchedWp>,
    causes: &mut HashMap<Id, serde_json::Value>,
    dep: &Dep,
) -> Result<(), ScheduleError> {
    let id = dep.id;
    let descendants = graph.descendants(id);
    if !descendants.is_empty() {
        // `reschedule_by_descendants` (:127-132): start is the earliest and due the latest of all
        // the descendants' dates, the due dates first and then the start dates
        // (`alive_descendants_dates`, `dependency.rb:92-94`).
        let dates: Vec<NaiveDate> = descendants
            .iter()
            .filter_map(|d| nodes[d].due_date)
            .chain(descendants.iter().filter_map(|d| nodes[d].start_date))
            .collect();
        set_dates(
            &input.calendar,
            nodes.get_mut(&id).expect("known"),
            dates.iter().min().copied(),
            dates.iter().max().copied(),
        );
        assign_cause(input, originals, nodes, causes, id);
    } else if !dep.follows.is_empty() {
        // `reschedule_by_predecessors` (:151-157), with `soonest_start_date` (`dependency.rb:
        // 53-58`).
        let soonest = dep
            .follows
            .iter()
            .filter_map(|&i| {
                let r = &input.relations[i];
                let pred = nodes.get(&r.to_id)?;
                // `Relation#successor_soonest_start` (`relation.rb:196-209`).
                let date = pred.due_date.or(pred.start_date)?;
                Some(with_lag(&input.calendar, date, r.lag))
            })
            .max();
        let days = |n: &SchedWp| Days::for_work_package(n.ignore_non_working_days, &input.calendar);
        let Some(soonest) = soonest.map(|d| days(&nodes[&id]).soonest_working_day(d)) else {
            return Ok(());
        };
        let new_due = determine_due_date(&input.calendar, &nodes[&id], soonest)?;
        set_dates(
            &input.calendar,
            nodes.get_mut(&id).expect("known"),
            Some(soonest),
            new_due,
        );
        assign_cause(input, originals, nodes, causes, id);
    } else if !input.switching_to_automatic_mode.contains(&id) {
        // Do not switch to manual a work package forced to automatic.
        nodes.get_mut(&id).expect("known").schedule_manually = true;
    }
    Ok(())
}

/// `determine_due_date` (:159-172): the due date is only set if the work package already has one
/// or a duration; a duration wins; a due date before the start becomes the start.
fn determine_due_date(
    calendar: &Calendar,
    wp: &SchedWp,
    start: NaiveDate,
) -> Result<Option<NaiveDate>, ScheduleError> {
    if wp.due_date.is_none() && wp.duration.is_none() {
        return Ok(None);
    }
    let due = match wp.duration {
        Some(duration) => Days::for_work_package(wp.ignore_non_working_days, calendar)
            .due_date(start, duration)?,
        None => wp.due_date.expect("checked above"),
    };
    Ok(Some(start.max(due)))
}

/// `set_dates` (:174-178).
fn set_dates(
    calendar: &Calendar,
    wp: &mut SchedWp,
    start: Option<NaiveDate>,
    due: Option<NaiveDate>,
) {
    wp.start_date = start;
    wp.due_date = due;
    wp.duration = Days::for_work_package(wp.ignore_non_working_days, calendar).duration(start, due);
}

/// `assign_cause_for_journaling` (:184-203): nothing without `initiated_by` and nothing unless a
/// date or the duration differs from the stored one; a work package initiator gives the generic
/// related-change cause (the specialised ones are commented out in Ruby, :194-201).
fn assign_cause(
    input: &ScheduleInput,
    originals: &BTreeMap<Id, SchedWp>,
    nodes: &BTreeMap<Id, SchedWp>,
    causes: &mut HashMap<Id, serde_json::Value>,
    id: Id,
) {
    let Some(initiator) = input.initiated_by else {
        return;
    };
    let (now, was) = (&nodes[&id], &originals[&id]);
    if now.start_date != was.start_date
        || now.due_date != was.due_date
        || now.duration != was.duration
    {
        causes.insert(id, related_change_cause(initiator));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    // 2026-03-02 is a Monday; 03-06 Friday; 03-07 Saturday; 03-08 Sunday; 03-09 Monday.
    #[test]
    fn working_week_is_monday_to_friday() {
        let cal = Calendar::default();
        assert!(cal.working(d("2026-03-06")));
        assert!(!cal.working(d("2026-03-07")));
        assert!(!cal.working(d("2026-03-08")));
        assert!(cal.working(d("2026-03-09")));
    }

    #[test]
    fn duration_counts_working_days_inclusive() {
        let cal = Calendar::default();
        let days = Days::for_work_package(false, &cal);
        // Fri 6 .. Tue 10: Fri, Mon, Tue.
        assert_eq!(
            days.duration(Some(d("2026-03-06")), Some(d("2026-03-10"))),
            Some(3)
        );
        let all = Days::for_work_package(true, &cal);
        assert_eq!(
            all.duration(Some(d("2026-03-06")), Some(d("2026-03-10"))),
            Some(5)
        );
        assert_eq!(days.duration(None, Some(d("2026-03-10"))), None);
    }

    #[test]
    fn due_date_skips_non_working_days() {
        let mut cal = Calendar::default();
        let days = Days::for_work_package(false, &cal);
        // 3 working days from Fri 6: Fri, Mon, Tue.
        assert_eq!(days.due_date(d("2026-03-06"), 3).unwrap(), d("2026-03-10"));
        // Starting on a Saturday begins on the Monday.
        assert_eq!(days.due_date(d("2026-03-07"), 1).unwrap(), d("2026-03-09"));
        assert!(days.due_date(d("2026-03-06"), 0).is_err());
        cal.non_working_dates.insert(d("2026-03-09"));
        let days = Days::for_work_package(false, &cal);
        assert_eq!(days.due_date(d("2026-03-06"), 3).unwrap(), d("2026-03-11"));
        let all = Days::for_work_package(true, &cal);
        assert_eq!(all.due_date(d("2026-03-06"), 3).unwrap(), d("2026-03-08"));
    }

    #[test]
    fn lag_is_counted_in_working_days() {
        let cal = Calendar::default();
        // Predecessor ends Thu 5: lag 0 -> Fri 6 (the next day), lag 2 -> Mon 9 skips the
        // weekend (Fri, Mon are the two lag days, the soonest start is the day after: Tue 10).
        assert_eq!(with_lag(&cal, d("2026-03-05"), Some(0)), d("2026-03-06"));
        assert_eq!(with_lag(&cal, d("2026-03-05"), None), d("2026-03-06"));
        assert_eq!(with_lag(&cal, d("2026-03-05"), Some(2)), d("2026-03-10"));
        // A negative lag moves back: 1 working day back from Tue 10 is Tue 10 itself.
        assert_eq!(with_lag(&cal, d("2026-03-10"), Some(-1)), d("2026-03-10"));
        assert_eq!(with_lag(&cal, d("2026-03-10"), Some(-2)), d("2026-03-09"));
    }

    #[test]
    fn dates_derive_the_duration_and_shift_to_working_days() {
        let cal = Calendar::default();
        let a = |s: &str, u: &str| DateAttrs {
            start_date: Some(d(s)),
            due_date: Some(d(u)),
            duration: None,
        };
        // Fri 6 .. Tue 10: Fri, Mon, Tue.
        let out = derive_dates(
            &cal,
            false,
            false,
            true,
            true,
            a("2026-03-06", "2026-03-10"),
        );
        assert_eq!(out.duration, Some(3));
        // A Saturday start moves to Monday; Sat 7 .. Tue 10 is Mon 9 .. Tue 10: 2.
        let out = derive_dates(
            &cal,
            false,
            false,
            true,
            true,
            a("2026-03-07", "2026-03-10"),
        );
        assert_eq!(
            (out.start_date, out.duration),
            (Some(d("2026-03-09")), Some(2))
        );
        // Calendar days keep Saturday: Sat 7 .. Tue 10 is 4.
        let out = derive_dates(&cal, true, false, true, true, a("2026-03-07", "2026-03-10"));
        assert_eq!(
            (out.start_date, out.duration),
            (Some(d("2026-03-07")), Some(4))
        );
        // A milestone has one date and duration 1.
        let out = derive_dates(&cal, false, true, true, true, a("2026-03-06", "2026-03-10"));
        assert_eq!(
            (out.start_date, out.due_date, out.duration),
            (Some(d("2026-03-10")), Some(d("2026-03-10")), Some(1))
        );
        // Only a start: nothing to derive from.
        let only = DateAttrs {
            start_date: Some(d("2026-03-06")),
            due_date: None,
            duration: None,
        };
        assert_eq!(
            derive_dates(&cal, false, false, true, false, only).duration,
            None
        );
    }

    #[test]
    fn a_start_date_is_derived_from_due_and_duration() {
        let cal = Calendar::default();
        let days = Days::for_work_package(false, &cal);
        // 3 working days ending Tue 10: Fri 6, Mon 9, Tue 10.
        assert_eq!(days.start_date(d("2026-03-10"), 3), d("2026-03-06"));
        assert_eq!(days.start_date(d("2026-03-08"), 1), d("2026-03-06"));
        let all = Days::for_work_package(true, &cal);
        assert_eq!(all.start_date(d("2026-03-10"), 3), d("2026-03-08"));
    }

    fn wp(
        id: Id,
        parent: Option<Id>,
        manual: bool,
        start: &str,
        due: &str,
        dur: Option<i32>,
    ) -> SchedWp {
        SchedWp {
            id,
            parent_id: parent,
            schedule_manually: manual,
            ignore_non_working_days: false,
            start_date: (!start.is_empty()).then(|| d(start)),
            due_date: (!due.is_empty()).then(|| d(due)),
            duration: dur,
            lock_version: 0,
        }
    }

    #[test]
    fn order_puts_predecessors_before_successors_and_children_before_parents() {
        // 3 follows 2, 2 follows 1 (1 is moved), 4 is the parent of 3.
        let deps = vec![
            Dep {
                id: 4,
                follows: vec![],
                dependent_ids: vec![3],
            },
            Dep {
                id: 3,
                follows: vec![],
                dependent_ids: vec![2],
            },
            Dep {
                id: 2,
                follows: vec![],
                dependent_ids: vec![1],
            },
        ];
        let order: Vec<Id> = schedule_order(&deps)
            .into_iter()
            .map(|i| deps[i].id)
            .collect();
        assert_eq!(order, vec![2, 3, 4]);
    }

    #[test]
    fn a_successor_is_pushed_and_keeps_its_duration() {
        // 1 (moved) ends Fri 6; 2 follows 1 with lag 0, duration 2, was Mon 2 .. Tue 3.
        let input = ScheduleInput {
            known: vec![
                wp(1, None, true, "2026-03-02", "2026-03-06", Some(5)),
                wp(2, None, false, "2026-03-02", "2026-03-03", Some(2)),
            ],
            moved: vec![1],
            moving: vec![2],
            relations: vec![FollowsRelation {
                from_id: 2,
                to_id: 1,
                lag: Some(0),
            }],
            switching_to_automatic_mode: HashSet::new(),
            initiated_by: Some(1),
            calendar: Calendar::default(),
        };
        let out = schedule_following(&input).unwrap();
        // Soonest start is Sat 7, moved to Mon 9; two working days: Mon 9 .. Tue 10.
        assert_eq!(out.nodes[&2].start_date, Some(d("2026-03-09")));
        assert_eq!(out.nodes[&2].due_date, Some(d("2026-03-10")));
        assert_eq!(out.altered.len(), 1);
        assert_eq!(out.altered[0].cause, Some(related_change_cause(1)));
    }

    #[test]
    fn a_manual_leaf_with_nothing_to_follow_is_left_alone_and_an_automatic_one_turns_manual() {
        let input = ScheduleInput {
            known: vec![wp(1, None, false, "", "", None)],
            moved: vec![1],
            moving: vec![],
            relations: vec![],
            switching_to_automatic_mode: HashSet::new(),
            initiated_by: None,
            calendar: Calendar::default(),
        };
        let out = schedule_following(&input).unwrap();
        assert!(out.nodes[&1].schedule_manually);
        assert_eq!(out.altered.len(), 1);
        let mut forced = input.clone();
        forced.switching_to_automatic_mode.insert(1);
        let out = schedule_following(&forced).unwrap();
        assert!(
            !out.nodes[&1].schedule_manually,
            "forced automatic stays automatic"
        );
    }
}
