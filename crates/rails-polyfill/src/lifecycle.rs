//! The ActiveRecord 8.1 persistence pipeline.
//!
//! `save` reproduces the nesting of ActiveRecord's own modules, outermost
//! first: `Transactions#save` (a transaction; rollback on a false result) →
//! `Validations#save` (`valid?`: before_validation, validate, after_validation;
//! a failed validation stops before any save callback) → `Callbacks`
//! (`run_callbacks(:save) { run_callbacks(:create | :update) { … } }`) →
//! `Timestamp#_create_record` / `_update_record` → the row write. After the
//! outermost transaction commits, `after_commit` callbacks run in the order
//! they were defined, which is the Rails ≥ 7.1 default
//! (`run_after_transaction_callbacks_in_order_defined`, on under
//! `config.load_defaults 8.1`).

use crate::callbacks::{Action, Callback, Cond, Phase, Target};
use crate::ctx::{Ctx, Entry, Halt};
use crate::record::Record;
use crate::schema::TableSpec;
use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssocKind {
    BelongsTo,
    HasMany,
    HasOne,
}

/// A navigable association. For `belongs_to` the foreign key is a column of
/// the owner; for `has_many` / `has_one` it is a column of the target.
pub struct AssocSpec {
    pub name: &'static str,
    pub kind: AssocKind,
    pub fk: usize,
    pub target: &'static ModelSpec,
    /// `-> { reorder(...) }` columns of the target.
    pub order: &'static [usize],
    pub provenance: &'static str,
}

/// Navigate `assoc` from `rec`: the target ids, as the read-back step reports
/// them (`nil` / one id / a list).
pub fn navigate(ctx: &mut Ctx, rec: &mut Record, assoc: &AssocSpec) -> Result<Value, Halt> {
    match assoc.kind {
        AssocKind::BelongsTo => {
            let v = belongs_to_cached(ctx, rec, assoc.name, assoc.fk, assoc.target)?;
            Ok(match v {
                Value::Rec(r) => r.id().map_or(Value::Nil, Value::Int),
                _ => Value::Nil,
            })
        }
        AssocKind::HasMany => Ok(Value::List(
            has_many(ctx, rec, assoc.target, assoc.fk, assoc.order)?
                .into_iter()
                .filter_map(|r| r.id().map(Value::Int))
                .collect(),
        )),
        AssocKind::HasOne => Ok(has_many(ctx, rec, assoc.target, assoc.fk, assoc.order)?
            .into_iter()
            .next()
            .and_then(|r| r.id())
            .map_or(Value::Nil, Value::Int)),
    }
}

/// A compiled model: its table and its linearized callback list.
pub struct ModelSpec {
    /// The Ruby class name (`Order::LineItem`).
    pub name: &'static str,
    pub table: &'static TableSpec,
    pub callbacks: &'static [Callback],
}

/// One record taking part in the outermost transaction
/// (`add_to_transaction`, transactions.rb:438). ActiveRecord keeps each
/// record once, in the order it joined, and runs its after_commit chain once
/// at commit, with the action read off the record's final state
/// (`transaction_include_any_action?`).
#[derive(Debug, Clone)]
pub(crate) struct PendingCommit {
    model: &'static ModelSpec,
    record: Record,
    /// `@_new_record_before_last_commit`: new when it joined.
    created: bool,
    /// What it did that triggers after_commit, if anything yet
    /// (`_trigger_update_callback` / `_trigger_destroy_callback`).
    action: Option<Action>,
}

/// The record's slot in the transaction, joining it if it is not in yet.
fn join_transaction(ctx: &mut Ctx, model: &'static ModelSpec, rec: &Record) -> usize {
    if let Some(id) = rec.id() {
        let same = |p: &PendingCommit| std::ptr::eq(p.record.table, rec.table) && p.record.id() == Some(id);
        if let Some(i) = ctx.pending_commit.iter().position(same) {
            return i;
        }
    }
    ctx.pending_commit.push(PendingCommit {
        model,
        record: rec.clone(),
        created: rec.new_record(),
        action: None,
    });
    ctx.pending_commit.len() - 1
}

/// Records what the operation did; a destroy outweighs an earlier save.
fn settle(ctx: &mut Ctx, slot: usize, rec: &Record, action: Option<Action>) {
    let p = &mut ctx.pending_commit[slot];
    p.record = rec.clone();
    p.action = match (p.action, action) {
        (_, Some(Action::Destroy)) => Some(Action::Destroy),
        (Some(Action::Destroy), _) => Some(Action::Destroy),
        (old, None) => old,
        (_, new) => new,
    };
}

/// Runs each participating record's after_commit chain once, in join order.
fn dispatch_commits(ctx: &mut Ctx) -> Result<(), Halt> {
    let pending = std::mem::take(&mut ctx.pending_commit);
    for mut p in pending {
        let Some(did) = p.action else { continue };
        let action = match did {
            Action::Destroy => Action::Destroy,
            _ if p.created => Action::Create,
            _ => Action::Update,
        };
        run_chain(ctx, p.model, &[Phase::AfterCommit], action, &mut p.record)?;
    }
    Ok(())
}

impl std::fmt::Debug for ModelSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

/// The phases each chain runs in one pass. `around_*` entries sit in the same
/// list as the `before_*` ones because ActiveSupport interleaves them by
/// declaration order.
const SAVE_BEFORE: &[Phase] = &[Phase::BeforeSave, Phase::AroundSave];
const CREATE_BEFORE: &[Phase] = &[Phase::BeforeCreate, Phase::AroundCreate];
const UPDATE_BEFORE: &[Phase] = &[Phase::BeforeUpdate, Phase::AroundUpdate];
const DESTROY_BEFORE: &[Phase] = &[Phase::BeforeDestroy, Phase::AroundDestroy];

fn conds_pass(ctx: &mut Ctx, rec: &mut Record, conds: &[Cond]) -> Result<bool, Halt> {
    for cond in conds {
        let ok = match *cond {
            Cond::If(f) => f(ctx, rec)?.truthy(),
            Cond::Unless(f) => !f(ctx, rec)?.truthy(),
            Cond::Residual(what) => return Err(Halt::Residual(what.to_string())),
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Run every callback of `model` whose phase is in `phases` and whose `on:`
/// covers `action`, in list order.
pub fn run_chain(
    ctx: &mut Ctx,
    model: &'static ModelSpec,
    phases: &[Phase],
    action: Action,
    rec: &mut Record,
) -> Result<(), Halt> {
    for cb in model.callbacks {
        if !phases.contains(&cb.phase) || !cb.on.applies(action) {
            continue;
        }
        if !conds_pass(ctx, rec, cb.conds)? {
            continue;
        }
        ctx.journal.push(Entry {
            model: model.name,
            phase: cb.phase,
            name: cb.name,
        });
        match cb.target {
            Target::Lowered(f) => {
                f(ctx, rec)?;
            }
            Target::Host => ctx.host.callback(model.name, cb.phase, cb.name, rec)?,
            Target::Residual(what) => return Err(Halt::Residual(what.to_string())),
            Target::Framework => {}
            Target::Validator(crate::validations::Validator::BelongsToRequired {
                assoc,
                fk,
                target,
                only_if_fk_missing_or_changed,
            }) => {
                if !only_if_fk_missing_or_changed || rec.get(fk).is_nil() || rec.changed(fk) {
                    let v = belongs_to_cached(ctx, rec, assoc, fk, target)?;
                    if v.blank() {
                        ctx.errors.push(crate::validations::ValidationError { attr: assoc, kind: "blank" });
                    }
                }
            }
            Target::Validator(v) => v.run(rec, &mut ctx.errors),
        }
    }
    Ok(())
}

/// `Model.new(attrs)`: defaults, then the assignments, then `after_initialize`.
pub fn build(ctx: &mut Ctx, model: &'static ModelSpec, attrs: &[(&str, Value)]) -> Result<Record, Halt> {
    let mut rec = Record::new(model.table);
    for (name, v) in attrs {
        if !rec.set_named(name, v.clone()) {
            return Err(Halt::Raise(format!(
                "ActiveModel::UnknownAttributeError: unknown attribute '{name}' for {}",
                model.name
            )));
        }
    }
    run_chain(ctx, model, &[Phase::AfterInitialize], Action::Create, &mut rec)?;
    Ok(rec)
}

/// A record coming out of the store: `after_find`, then `after_initialize`.
fn instantiate(ctx: &mut Ctx, model: &'static ModelSpec, mut rec: Record) -> Result<Record, Halt> {
    run_chain(ctx, model, &[Phase::AfterFind], Action::Update, &mut rec)?;
    run_chain(ctx, model, &[Phase::AfterInitialize], Action::Update, &mut rec)?;
    Ok(rec)
}

/// `Model.find_by(id: id)`.
pub fn find(ctx: &mut Ctx, model: &'static ModelSpec, id: i64) -> Result<Option<Record>, Halt> {
    match ctx.store.find(model.table, id)? {
        Some(rec) => instantiate(ctx, model, rec).map(Some),
        None => Ok(None),
    }
}

/// `Model.find_by(col: v, …)`.
pub fn find_by(ctx: &mut Ctx, model: &'static ModelSpec, preds: &[(usize, Value)]) -> Result<Option<Record>, Halt> {
    match ctx.store.find_by(model.table, preds)? {
        Some(rec) => instantiate(ctx, model, rec).map(Some),
        None => Ok(None),
    }
}

/// `Model.where(col: v, …).to_a`, primary-key order.
pub fn where_eq(ctx: &mut Ctx, model: &'static ModelSpec, preds: &[(usize, Value)]) -> Result<Vec<Record>, Halt> {
    let rows = ctx.store.where_eq(model.table, preds)?;
    rows.into_iter().map(|r| instantiate(ctx, model, r)).collect()
}

/// `record.reload`.
pub fn reload(ctx: &mut Ctx, model: &'static ModelSpec, rec: &Record) -> Result<Option<Record>, Halt> {
    match rec.id() {
        Some(id) => find(ctx, model, id),
        None => Ok(None),
    }
}

/// The outcome of `save`. `errors` is non-empty exactly when validation
/// failed; `aborted` when a callback threw `:abort`.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    pub ok: bool,
    pub errors: Vec<crate::validations::ValidationError>,
    pub aborted: bool,
}

/// `now` is `ctx.now`, in milliseconds.
fn set_time(rec: &mut Record, col: &str, now: i64) {
    rec.set_named(col, Value::time_ms(now));
}

/// `record.save`.
pub fn save(ctx: &mut Ctx, model: &'static ModelSpec, rec: &mut Record) -> Result<Saved, Halt> {
    let action = if rec.new_record() { Action::Create } else { Action::Update };
    let snapshot = ctx.store.begin();
    let record_state = rec.transaction_state();
    let outer = ctx.txn_depth == 0;
    let slot = join_transaction(ctx, model, rec);
    ctx.txn_depth += 1;
    let result = save_in_transaction(ctx, model, rec, action);
    ctx.txn_depth -= 1;

    let committed = matches!(&result, Ok(s) if s.ok);
    if !committed {
        if !outer {
            // A nested record transaction joins the outer one (no savepoint,
            // `requires_new: false`): its `ActiveRecord::Rollback` is swallowed
            // and nothing is rolled back here; only the outermost transaction
            // rolls back (oracle/lock_rollback.rb, nested scenario).
            ctx.store.release(snapshot);
            return result;
        }
        ctx.store.rollback(snapshot);
        ctx.pending_commit.clear();
        // `rolledback!` runs after_rollback, then restores the record state
        // in an `ensure` (transactions.rb:415-423).
        let rolled_back = run_chain(ctx, model, &[Phase::AfterRollback], action, rec);
        rec.restore_transaction_state(record_state);
        rolled_back?;
        return result;
    }
    ctx.store.release(snapshot);
    settle(ctx, slot, rec, Some(action));
    if outer {
        dispatch_commits(ctx)?;
    }
    result
}

fn save_in_transaction(
    ctx: &mut Ctx,
    model: &'static ModelSpec,
    rec: &mut Record,
    action: Action,
) -> Result<Saved, Halt> {
    // Validations#save → valid?
    let outer_errors = std::mem::take(&mut ctx.errors);
    let validated = (|| {
        run_chain(ctx, model, &[Phase::BeforeValidation], action, rec)?;
        run_chain(ctx, model, &[Phase::Validate], action, rec)?;
        run_chain(ctx, model, &[Phase::AfterValidation], action, rec)
    })();
    let errors = std::mem::replace(&mut ctx.errors, outer_errors);
    match validated {
        Err(Halt::Abort) => return Ok(Saved { ok: false, errors, aborted: true }),
        Err(e) => return Err(e),
        Ok(()) => {}
    }
    if !errors.is_empty() {
        return Ok(Saved { ok: false, errors, aborted: false });
    }

    // Callbacks: run_callbacks(:save) { run_callbacks(:create|:update) { write } }
    let body = (|| {
        run_chain(ctx, model, SAVE_BEFORE, action, rec)?;
        match action {
            Action::Create => {
                run_chain(ctx, model, CREATE_BEFORE, action, rec)?;
                // Timestamp#_create_record: fill unset timestamp columns.
                for col in ["created_at", "updated_at"] {
                    if model.table.has_column(col) && rec.get_named(col).is_nil() {
                        set_time(rec, col, ctx.now);
                    }
                }
                ctx.store.insert(rec)?;
                run_chain(ctx, model, &[Phase::AfterCreate], action, rec)?;
            }
            Action::Update => {
                run_chain(ctx, model, UPDATE_BEFORE, action, rec)?;
                // Timestamp#_update_record: only when something will be
                // written, and never over an explicit change.
                if rec.has_changes_to_save()
                    && let Some(c) = model.table.index_of("updated_at")
                    && !rec.changed(c)
                {
                    rec.set(c, Value::time_ms(ctx.now));
                }
                ctx.store.update(rec)?;
                run_chain(ctx, model, &[Phase::AfterUpdate], action, rec)?;
            }
            Action::Destroy => {}
        }
        run_chain(ctx, model, &[Phase::AfterSave], action, rec)
    })();
    match body {
        Ok(()) => Ok(Saved { ok: true, errors, aborted: false }),
        Err(Halt::Abort) => Ok(Saved { ok: false, errors, aborted: true }),
        Err(e) => Err(e),
    }
}

/// `record.save!` / `Model.create!` — a failed save is an error.
pub fn save_bang(ctx: &mut Ctx, model: &'static ModelSpec, rec: &mut Record) -> Result<(), Halt> {
    let saved = save(ctx, model, rec)?;
    if saved.ok {
        Ok(())
    } else if saved.aborted {
        Err(Halt::Raise(format!("ActiveRecord::RecordNotSaved: Failed to save the record ({})", model.name)))
    } else {
        Err(Halt::Raise(format!(
            "ActiveRecord::RecordInvalid: {}",
            saved
                .errors
                .iter()
                .map(|e| format!("{} {}", e.attr, e.kind))
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }
}

/// `record.destroy`: `true` when it ran to completion, `false` when a
/// `before_destroy` threw `:abort`. A stale `lock_version` raises
/// (`StaleObject`), after the rollback.
///
/// Nesting, outermost first, as in ActiveRecord: `Transactions#destroy` (a
/// transaction; rollback on a false result) → `Callbacks#destroy`
/// (`run_callbacks(:destroy) { … }`) → `Persistence#destroy` (the row delete,
/// then `@destroyed = true`). `after_commit` runs only when a row was really
/// deleted (`@_trigger_destroy_callback`, persistence.rb:456).
pub fn destroy(ctx: &mut Ctx, model: &'static ModelSpec, rec: &mut Record) -> Result<bool, Halt> {
    let snapshot = ctx.store.begin();
    let record_state = rec.transaction_state();
    let outer = ctx.txn_depth == 0;
    let slot = join_transaction(ctx, model, rec);
    let mut row_deleted = false;
    ctx.txn_depth += 1;
    let result = destroy_in_transaction(ctx, model, rec, &mut row_deleted);
    ctx.txn_depth -= 1;

    let deleted = match result {
        Ok(Some(deleted)) => deleted,
        other => {
            if !outer {
                // Joined to the outer transaction: nothing is rolled back here
                // (see `save`). A row already deleted stays deleted, and its
                // after_commit runs if the outer transaction commits, even
                // when an after_destroy raised afterwards
                // (`@_trigger_destroy_callback` is set by the delete).
                ctx.store.release(snapshot);
                if row_deleted {
                    settle(ctx, slot, rec, Some(Action::Destroy));
                }
                return other.map(|_| false);
            }
            ctx.store.rollback(snapshot);
            ctx.pending_commit.clear();
            // `rolledback!`: after_rollback, then the record state as it was
            // when the destroy's transaction began, `@destroyed` and the dirty
            // baseline included (transactions.rb:415-423, 494-517).
            let rolled_back = run_chain(ctx, model, &[Phase::AfterRollback], Action::Destroy, rec);
            rec.restore_transaction_state(record_state);
            rolled_back?;
            return other.map(|_| false);
        }
    };
    ctx.store.release(snapshot);
    if deleted {
        settle(ctx, slot, rec, Some(Action::Destroy));
    } else {
        // Destroyed without a deleted row: no destroy trigger, and a
        // destroyed record takes no create or update callback either.
        ctx.pending_commit[slot].record = rec.clone();
        ctx.pending_commit[slot].action = None;
    }
    if outer {
        dispatch_commits(ctx)?;
    }
    Ok(true)
}

/// `Some(deleted)` once the chain ran; `None` when a callback aborted.
fn destroy_in_transaction(
    ctx: &mut Ctx,
    model: &'static ModelSpec,
    rec: &mut Record,
    row_deleted: &mut bool,
) -> Result<Option<bool>, Halt> {
    let body = (|| {
        run_chain(ctx, model, DESTROY_BEFORE, Action::Destroy, rec)?;
        // Persistence#destroy: `persisted? && destroy_row > 0`.
        let deleted = rec.persisted() && ctx.store.delete_locked(rec)?;
        *row_deleted = deleted;
        rec.set_destroyed(true);
        run_chain(ctx, model, &[Phase::AfterDestroy], Action::Destroy, rec)?;
        Ok(deleted)
    })();
    match body {
        Ok(deleted) => Ok(Some(deleted)),
        Err(Halt::Abort) => Ok(None),
        Err(e) => Err(e),
    }
}

/// `record.destroy!` — an aborted destroy is `RecordNotDestroyed`
/// (persistence.rb:469-471, 981-987).
pub fn destroy_bang(ctx: &mut Ctx, model: &'static ModelSpec, rec: &mut Record) -> Result<(), Halt> {
    if destroy(ctx, model, rec)? {
        Ok(())
    } else {
        Err(Halt::Raise(format!(
            "ActiveRecord::RecordNotDestroyed: Failed to destroy {} with id={}",
            model.name,
            rec.id().map_or_else(|| "nil".to_string(), |id| id.to_string())
        )))
    }
}

/// `Model.create!(attrs)`.
pub fn create_bang(ctx: &mut Ctx, model: &'static ModelSpec, attrs: &[(&str, Value)]) -> Result<Record, Halt> {
    let mut rec = build(ctx, model, attrs)?;
    save_bang(ctx, model, &mut rec)?;
    Ok(rec)
}

/// `record.update!(attrs)`.
pub fn update_bang(
    ctx: &mut Ctx,
    model: &'static ModelSpec,
    rec: &mut Record,
    attrs: &[(&str, Value)],
) -> Result<(), Halt> {
    for (name, v) in attrs {
        if !rec.set_named(name, v.clone()) {
            return Err(Halt::Raise(format!("unknown attribute '{name}' for {}", model.name)));
        }
    }
    save_bang(ctx, model, rec)
}

/// `belongs_to` navigation: the target by foreign key.
pub fn belongs_to(
    ctx: &mut Ctx,
    rec: &Record,
    fk: usize,
    target: &'static ModelSpec,
) -> Result<Option<Record>, Halt> {
    match rec.get(fk).as_int() {
        Some(id) => find(ctx, target, id),
        None => Ok(None),
    }
}

/// A `belongs_to` reader on `self` (`owner`, `state`): reuses the loaded
/// target while the foreign key is unchanged, otherwise loads and caches it.
/// A `nil` foreign key answers `nil` without a query.
pub fn belongs_to_cached(
    ctx: &mut Ctx,
    rec: &mut Record,
    assoc: &'static str,
    fk: usize,
    target: &'static ModelSpec,
) -> Result<Value, Halt> {
    let key = rec.get(fk).clone();
    if let Some(hit) = rec.cached_assoc(assoc, &key) {
        return Ok(hit.map_or(Value::Nil, |r| Value::Rec(Box::new(r))));
    }
    let loaded = match key.as_int() {
        Some(id) => find(ctx, target, id)?,
        None => None,
    };
    rec.cache_assoc(assoc, key, loaded.clone());
    Ok(loaded.map_or(Value::Nil, |r| Value::Rec(Box::new(r))))
}

/// `Model.find_by(...)` as a Ruby value: the record, or `nil`.
pub fn find_by_value(ctx: &mut Ctx, model: &'static ModelSpec, preds: &[(usize, Value)]) -> Result<Value, Halt> {
    Ok(find_by(ctx, model, preds)?.map_or(Value::Nil, |r| Value::Rec(Box::new(r))))
}

/// `has_many` navigation: every target whose foreign key points here, sorted
/// by the association's `reorder(...)` columns and then by primary key.
pub fn has_many(
    ctx: &mut Ctx,
    rec: &Record,
    target: &'static ModelSpec,
    fk: usize,
    order: &[usize],
) -> Result<Vec<Record>, Halt> {
    let Some(id) = rec.id() else {
        return Ok(Vec::new());
    };
    let mut rows = where_eq(ctx, target, &[(fk, Value::Int(id))])?;
    rows.sort_by(|a, b| {
        for &c in order {
            let ord = cmp_values(a.get(c), b.get(c));
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        a.id().cmp(&b.id())
    });
    Ok(rows)
}

fn cmp_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) | (Value::Time(x), Value::Time(y)) => x.cmp(y),
        (Value::Date(x), Value::Date(y)) => x.cmp(y),
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Nil, Value::Nil) => Ordering::Equal,
        (Value::Nil, _) => Ordering::Less,
        (_, Value::Nil) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn dates_order_by_day_including_before_the_epoch() {
        assert_eq!(cmp_values(&Value::Date(-3), &Value::Date(2)), Ordering::Less);
        assert_eq!(cmp_values(&Value::Date(7), &Value::Date(7)), Ordering::Equal);
        assert_eq!(cmp_values(&Value::Date(9), &Value::Date(-9)), Ordering::Greater);
    }

    use crate::callbacks::On;
    use crate::ctx::NoHost;
    use crate::schema::{Column, ColumnType, Default as ColDefault};
    use crate::store::StoreError;

    const fn col(name: &'static str, ty: ColumnType, null: bool, default: ColDefault) -> Column {
        Column { name, ty, null, default, limit: None }
    }

    static LOCKED: TableSpec = TableSpec {
        name: "locked",
        columns: &[
            col("id", ColumnType::Integer, false, ColDefault::None),
            col("name", ColumnType::String, true, ColDefault::None),
            col("lock_version", ColumnType::Integer, false, ColDefault::Int(0)),
        ],
    };

    static PLAIN: TableSpec = TableSpec {
        name: "plain",
        columns: &[
            col("id", ColumnType::Integer, false, ColDefault::None),
            col("name", ColumnType::String, true, ColDefault::None),
        ],
    };

    fn rows_before(ctx: &mut Ctx, _: &mut Record) -> Result<Value, Halt> {
        let n = ctx.store.count(&LOCKED) as i64;
        ctx.settings.insert("rows_before".into(), Value::Int(n));
        Ok(Value::Nil)
    }

    fn rows_after(ctx: &mut Ctx, _: &mut Record) -> Result<Value, Halt> {
        let n = ctx.store.count(&LOCKED) as i64;
        ctx.settings.insert("rows_after".into(), Value::Int(n));
        Ok(Value::Nil)
    }

    fn noop(_: &mut Ctx, _: &mut Record) -> Result<Value, Halt> {
        Ok(Value::Nil)
    }

    fn abort(_: &mut Ctx, _: &mut Record) -> Result<Value, Halt> {
        Err(Halt::Abort)
    }

    fn boom(_: &mut Ctx, _: &mut Record) -> Result<Value, Halt> {
        Err(Halt::Raise("boom".into()))
    }

    const fn cb(phase: Phase, name: &'static str, on: On, f: crate::callbacks::BodyFn) -> Callback {
        Callback { phase, name, on, conds: &[], target: Target::Lowered(f), provenance: "test" }
    }

    static ORDERED: ModelSpec = ModelSpec {
        name: "Ordered",
        table: &LOCKED,
        callbacks: &[
            cb(Phase::BeforeDestroy, "before", On::ANY, rows_before),
            cb(Phase::AroundDestroy, "around", On::ANY, noop),
            cb(Phase::AfterDestroy, "after", On::ANY, rows_after),
            cb(Phase::AfterCommit, "commit_destroy", On::DESTROY, noop),
            cb(Phase::AfterCommit, "commit_create", On::CREATE, noop),
        ],
    };

    static HALTING: ModelSpec = ModelSpec {
        name: "Halting",
        table: &LOCKED,
        callbacks: &[
            cb(Phase::BeforeDestroy, "veto", On::ANY, abort),
            cb(Phase::AfterDestroy, "after", On::ANY, noop),
        ],
    };

    static EXPLODING: ModelSpec = ModelSpec {
        name: "Exploding",
        table: &LOCKED,
        callbacks: &[cb(Phase::AfterDestroy, "boom", On::ANY, boom)],
    };

    static EXPLODING_SAVE: ModelSpec = ModelSpec {
        name: "ExplodingSave",
        table: &LOCKED,
        callbacks: &[cb(Phase::AfterSave, "boom", On::ANY, boom)],
    };

    static WATCHED_PLAIN: ModelSpec = ModelSpec {
        name: "WatchedPlain",
        table: &PLAIN,
        callbacks: &[cb(Phase::AfterCommit, "commit_destroy", On::DESTROY, noop)],
    };

    fn ctx_with(table: &'static TableSpec) -> Ctx {
        let mut ctx = Ctx::new(Box::new(NoHost), 0);
        ctx.store.register(table);
        ctx
    }

    fn phases(ctx: &Ctx) -> Vec<Phase> {
        ctx.journal.iter().map(|e| e.phase).collect()
    }

    fn seeded(ctx: &mut Ctx, model: &'static ModelSpec) -> Record {
        let id = ctx.store.seed(model.table, &[("name", Value::str("a"))]).unwrap();
        find(ctx, model, id).unwrap().unwrap()
    }

    #[test]
    fn destroy_runs_before_then_around_then_after_around_the_row_delete() {
        let mut ctx = ctx_with(&LOCKED);
        let mut rec = seeded(&mut ctx, &ORDERED);
        assert_eq!(destroy(&mut ctx, &ORDERED, &mut rec), Ok(true));
        assert_eq!(
            phases(&ctx),
            [Phase::BeforeDestroy, Phase::AroundDestroy, Phase::AfterDestroy, Phase::AfterCommit],
            "only the on: :destroy commit callback fires"
        );
        assert_eq!(ctx.journal[3].name, "commit_destroy");
        assert_eq!(ctx.settings["rows_before"], Value::Int(1), "before_destroy sees the row");
        assert_eq!(ctx.settings["rows_after"], Value::Int(0), "after_destroy sees it gone");
        assert!(rec.destroyed() && !rec.persisted());
        assert_eq!(ctx.store.count(&LOCKED), 0);
    }

    #[test]
    fn a_halting_before_destroy_keeps_the_row_and_destroy_bang_raises() {
        let mut ctx = ctx_with(&LOCKED);
        let mut rec = seeded(&mut ctx, &HALTING);
        assert_eq!(destroy(&mut ctx, &HALTING, &mut rec), Ok(false));
        assert_eq!(ctx.store.count(&LOCKED), 1, "the row is still there");
        assert!(!rec.destroyed() && rec.persisted());
        assert_eq!(phases(&ctx), [Phase::BeforeDestroy], "no after_destroy after the veto");
        let err = destroy_bang(&mut ctx, &HALTING, &mut rec).unwrap_err();
        assert!(
            matches!(&err, Halt::Raise(m) if m.starts_with("ActiveRecord::RecordNotDestroyed: Failed to destroy Halting with id=1")),
            "{err:?}"
        );
        assert_eq!(ctx.store.count(&LOCKED), 1);
    }

    #[test]
    fn a_stale_destroy_raises_after_rolling_back_and_deletes_nothing() {
        let mut ctx = ctx_with(&LOCKED);
        let mut stale_copy = seeded(&mut ctx, &ORDERED);
        let mut writer = find(&mut ctx, &ORDERED, stale_copy.id().unwrap()).unwrap().unwrap();
        writer.set_named("name", Value::str("b"));
        assert!(save(&mut ctx, &ORDERED, &mut writer).unwrap().ok);
        let err = destroy(&mut ctx, &ORDERED, &mut stale_copy).unwrap_err();
        assert_eq!(err, Halt::Store(StoreError::StaleObject { table: "locked", id: 1 }));
        assert_eq!(ctx.store.count(&LOCKED), 1);
        assert!(!stale_copy.destroyed(), "a failed destroy is not destroyed");
        // The current record destroys fine.
        assert_eq!(destroy(&mut ctx, &ORDERED, &mut writer), Ok(true));
        assert_eq!(ctx.store.count(&LOCKED), 0);
    }

    #[test]
    fn an_exception_after_the_delete_rolls_the_row_and_destroyed_back() {
        let mut ctx = ctx_with(&LOCKED);
        let mut rec = seeded(&mut ctx, &EXPLODING);
        assert_eq!(destroy(&mut ctx, &EXPLODING, &mut rec), Err(Halt::Raise("boom".into())));
        assert_eq!(ctx.store.count(&LOCKED), 1, "the delete was rolled back");
        assert!(!rec.destroyed() && rec.persisted(), "@destroyed is restored");
    }

    /// ActiveRecord 8.1.4.1, run against SQLite (oracle/lock_rollback.rb): an
    /// after_save that raises after the UPDATE leaves the record's
    /// lock_version incremented while the stored row is rolled back, so a
    /// retry of the same record raises StaleObjectError. Rails keeps the
    /// value because `_update_row` succeeded (its rescue does not run) and
    /// the rollback reapplies current values (transactions.rb:500-503).
    #[test]
    fn a_rolled_back_locked_save_leaves_the_record_stale_as_rails_does() {
        let mut ctx = ctx_with(&LOCKED);
        let mut rec = seeded(&mut ctx, &EXPLODING_SAVE);
        rec.set(1, Value::str("b"));
        assert_eq!(save(&mut ctx, &EXPLODING_SAVE, &mut rec), Err(Halt::Raise("boom".into())));
        let stored = find(&mut ctx, &ORDERED, 1).unwrap().unwrap();
        assert_eq!((rec.get(2), stored.get(2)), (&Value::Int(1), &Value::Int(0)));
        assert_eq!(stored.get(1), &Value::str("a"), "the row is rolled back");
        // Rails: changed == ["subject", "lock_version"] (baseline restored,
        // current values kept).
        assert_eq!(rec.changed_columns(), [1, 2]);
        rec.set(1, Value::str("c"));
        let err = save(&mut ctx, &ORDERED, &mut rec).unwrap_err();
        assert_eq!(err, Halt::Store(StoreError::StaleObject { table: "locked", id: 1 }));
    }

    fn save_then_abort(ctx: &mut Ctx, rec: &mut Record) -> Result<Value, Halt> {
        save(ctx, &PLAIN_MODEL, rec)?;
        Err(Halt::Abort)
    }

    static PLAIN_MODEL: ModelSpec = ModelSpec { name: "Plain", table: &PLAIN, callbacks: &[] };

    static SAVE_THEN_ABORT: ModelSpec = ModelSpec {
        name: "SaveThenAbort",
        table: &PLAIN,
        callbacks: &[cb(Phase::BeforeDestroy, "save_then_abort", On::ANY, save_then_abort)],
    };

    /// ActiveRecord 8.1.4.1 (oracle/lock_rollback.rb, second scenario): a
    /// before_destroy that saves and then aborts leaves the row at "a" and
    /// the record at "b" with `changed == ["name"]`, `destroyed? == false`.
    #[test]
    fn a_rolled_back_destroy_restores_the_dirty_baseline() {
        let mut ctx = ctx_with(&PLAIN);
        let mut rec = seeded(&mut ctx, &SAVE_THEN_ABORT);
        rec.set(1, Value::str("b"));
        assert_eq!(destroy(&mut ctx, &SAVE_THEN_ABORT, &mut rec), Ok(false));
        let stored = find(&mut ctx, &PLAIN_MODEL, 1).unwrap().unwrap();
        assert_eq!(stored.get(1), &Value::str("a"), "the nested save is rolled back");
        assert_eq!(rec.get(1), &Value::str("b"));
        assert_eq!(rec.changed_columns(), [1], "b is pending again");
        assert!(!rec.destroyed());
        // So the retry persists it.
        assert_eq!(save(&mut ctx, &PLAIN_MODEL, &mut rec).map(|s| s.ok), Ok(true));
        assert_eq!(find(&mut ctx, &PLAIN_MODEL, 1).unwrap().unwrap().get(1), &Value::str("b"));
    }

    static OUTERS: TableSpec = TableSpec {
        name: "outers",
        columns: &[
            col("id", ColumnType::Integer, false, ColDefault::None),
            col("name", ColumnType::String, true, ColDefault::None),
        ],
    };

    fn destroy_first_child_then_abort(ctx: &mut Ctx, _: &mut Record) -> Result<Value, Halt> {
        let mut child = find(ctx, &CHILD, 1)?.expect("child row");
        destroy(ctx, &CHILD, &mut child)?;
        Err(Halt::Abort)
    }

    fn destroy_first_parent(ctx: &mut Ctx, _: &mut Record) -> Result<Value, Halt> {
        let mut parent = find(ctx, &PARENT, 1)?.expect("parent row");
        let destroyed = destroy(ctx, &PARENT, &mut parent)?;
        ctx.settings.insert("parent_destroyed".into(), Value::Bool(destroyed));
        Ok(Value::Nil)
    }

    static CHILD: ModelSpec = ModelSpec {
        name: "Child",
        table: &PLAIN,
        callbacks: &[cb(Phase::AfterCommit, "child_commit_destroy", On::DESTROY, noop)],
    };

    static PARENT: ModelSpec = ModelSpec {
        name: "Parent",
        table: &LOCKED,
        callbacks: &[cb(Phase::BeforeDestroy, "destroy_child_then_abort", On::ANY, destroy_first_child_then_abort)],
    };

    static OUTER: ModelSpec = ModelSpec {
        name: "Outer",
        table: &OUTERS,
        callbacks: &[cb(Phase::AfterSave, "destroy_parent", On::ANY, destroy_first_parent)],
    };

    /// ActiveRecord 8.1.4.1 (oracle/lock_rollback.rb, nested scenario): a
    /// destroy nested in an outer save joins its transaction (no savepoint),
    /// so the parent's aborted destroy rolls nothing back. The outer save
    /// succeeds, the child stays deleted, and its after_commit runs when the
    /// outer save commits.
    #[test]
    fn an_aborted_nested_destroy_rolls_nothing_back_as_rails_does() {
        let mut ctx = ctx_with(&PLAIN);
        ctx.store.register(&LOCKED);
        ctx.store.register(&OUTERS);
        ctx.store.seed(&PLAIN, &[("name", Value::str("c"))]).unwrap();
        ctx.store.seed(&LOCKED, &[("name", Value::str("p"))]).unwrap();
        let mut outer = build(&mut ctx, &OUTER, &[("name", Value::str("o"))]).unwrap();
        assert_eq!(save(&mut ctx, &OUTER, &mut outer).map(|s| s.ok), Ok(true));
        assert_eq!(ctx.settings.get("parent_destroyed"), Some(&Value::Bool(false)));
        assert_eq!(
            (ctx.store.count(&PLAIN), ctx.store.count(&LOCKED), ctx.store.count(&OUTERS)),
            (0, 1, 1),
            "children=0 parents=1 outers=1"
        );
        let child_commits = ctx.journal.iter().filter(|e| e.name == "child_commit_destroy").count();
        assert_eq!(child_commits, 1, "the child's after_commit runs");
    }

    fn destroy_kid_rescuing(ctx: &mut Ctx, _: &mut Record) -> Result<Value, Halt> {
        let mut kid = find(ctx, &KID, 1)?.expect("kid row");
        match destroy(ctx, &KID, &mut kid) {
            Err(Halt::Raise(_)) => Ok(Value::Nil),
            other => other.map(|_| Value::Nil),
        }
    }

    static KID: ModelSpec = ModelSpec {
        name: "Kid",
        table: &PLAIN,
        callbacks: &[
            cb(Phase::AfterDestroy, "kid_boom", On::ANY, boom),
            cb(Phase::AfterCommit, "kid_commit_destroy", On::DESTROY, noop),
        ],
    };

    static MOM: ModelSpec = ModelSpec {
        name: "Mom",
        table: &LOCKED,
        callbacks: &[
            cb(Phase::BeforeDestroy, "destroy_kid_rescuing", On::ANY, destroy_kid_rescuing),
            cb(Phase::AfterCommit, "mom_commit_destroy", On::DESTROY, noop),
        ],
    };

    fn commits(ctx: &Ctx) -> Vec<&'static str> {
        ctx.journal.iter().filter(|e| e.phase == Phase::AfterCommit).map(|e| e.name).collect()
    }

    /// ActiveRecord 8.1.4.1 (oracle/lock_rollback.rb, rescued-raise
    /// scenario): a nested destroy whose after_destroy raises, rescued by the
    /// outer before_destroy, keeps its row deleted and its after_commit runs,
    /// after the outer record's, in the order the records joined.
    #[test]
    fn a_nested_destroy_that_deleted_keeps_its_commit_callback_after_a_rescued_raise() {
        let mut ctx = ctx_with(&PLAIN);
        ctx.store.register(&LOCKED);
        ctx.store.seed(&PLAIN, &[("name", Value::str("k"))]).unwrap();
        let mut mom = seeded(&mut ctx, &MOM);
        assert_eq!(destroy(&mut ctx, &MOM, &mut mom), Ok(true));
        assert_eq!((ctx.store.count(&PLAIN), ctx.store.count(&LOCKED)), (0, 0));
        assert_eq!(commits(&ctx), ["mom_commit_destroy", "kid_commit_destroy"]);
    }

    fn rename_and_save(ctx: &mut Ctx, rec: &mut Record) -> Result<Value, Halt> {
        rec.set(1, Value::str("changed"));
        save(ctx, &SOLO, rec)?;
        Ok(Value::Nil)
    }

    static SOLO: ModelSpec = ModelSpec {
        name: "Solo",
        table: &PLAIN,
        callbacks: &[
            cb(Phase::BeforeDestroy, "rename_and_save", On::ANY, rename_and_save),
            cb(Phase::AfterCommit, "commit_update", On::UPDATE, noop),
            cb(Phase::AfterCommit, "commit_destroy", On::DESTROY, noop),
            cb(Phase::AfterCommit, "commit_any", On::ANY, noop),
        ],
    };

    /// ActiveRecord 8.1.4.1 (oracle/lock_rollback.rb, save-then-destroy
    /// scenario): a record saved in its own before_destroy takes part in the
    /// transaction once; at commit it is destroyed, so no update callback
    /// runs and each matching after_commit runs once.
    #[test]
    fn a_record_saved_then_destroyed_commits_once_as_a_destroy() {
        let mut ctx = ctx_with(&PLAIN);
        let mut solo = seeded(&mut ctx, &SOLO);
        assert_eq!(destroy(&mut ctx, &SOLO, &mut solo), Ok(true));
        assert_eq!(ctx.store.count(&PLAIN), 0);
        let mut got = commits(&ctx);
        got.sort_unstable();
        assert_eq!(got, ["commit_any", "commit_destroy"]);
    }

    /// A rolled-back create restores the pre-save id and `new_record?`
    /// (transactions.rb:497, 514-516), so the record can be saved again.
    #[test]
    fn a_rolled_back_create_is_new_again_without_an_id() {
        let mut ctx = ctx_with(&LOCKED);
        let mut rec = build(&mut ctx, &EXPLODING_SAVE, &[("name", Value::str("n"))]).unwrap();
        assert_eq!(save(&mut ctx, &EXPLODING_SAVE, &mut rec), Err(Halt::Raise("boom".into())));
        assert_eq!(ctx.store.count(&LOCKED), 0);
        assert!(rec.new_record());
        assert_eq!(rec.get(0), &Value::Nil, "the assigned id is restored to nil");
        assert_eq!(save(&mut ctx, &ORDERED, &mut rec).map(|s| s.ok), Ok(true));
        assert_eq!(ctx.store.count(&LOCKED), 1);
    }

    #[test]
    fn destroying_a_row_that_is_already_gone_fires_no_commit_callback() {
        let mut ctx = ctx_with(&PLAIN);
        let mut rec = seeded(&mut ctx, &WATCHED_PLAIN);
        ctx.store.delete(&PLAIN, rec.id().unwrap()).unwrap();
        assert_eq!(destroy(&mut ctx, &WATCHED_PLAIN, &mut rec), Ok(true));
        assert!(rec.destroyed());
        assert!(phases(&ctx).is_empty(), "destroy_row > 0 was false: {:?}", ctx.journal);
        // The control: a real delete does fire it.
        let mut live = seeded(&mut ctx, &WATCHED_PLAIN);
        assert_eq!(destroy(&mut ctx, &WATCHED_PLAIN, &mut live), Ok(true));
        assert_eq!(phases(&ctx), [Phase::AfterCommit]);
    }

    #[test]
    fn destroying_a_new_record_deletes_nothing_and_marks_it_destroyed() {
        let mut ctx = ctx_with(&LOCKED);
        let mut rec = build(&mut ctx, &ORDERED, &[("name", Value::str("n"))]).unwrap();
        assert_eq!(destroy(&mut ctx, &ORDERED, &mut rec), Ok(true));
        assert!(rec.destroyed() && rec.new_record());
        assert!(phases(&ctx).contains(&Phase::BeforeDestroy), "callbacks still run");
    }
}
