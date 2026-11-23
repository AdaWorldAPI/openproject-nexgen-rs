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

#[derive(Debug, Clone)]
pub(crate) struct PendingCommit {
    model: &'static ModelSpec,
    record: Record,
    action: Action,
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
    let outer = ctx.txn_depth == 0;
    ctx.txn_depth += 1;
    let result = save_in_transaction(ctx, model, rec, action);
    ctx.txn_depth -= 1;

    let committed = matches!(&result, Ok(s) if s.ok);
    if !committed {
        ctx.store.rollback(snapshot);
        if outer {
            ctx.pending_commit.clear();
            run_chain(ctx, model, &[Phase::AfterRollback], action, rec)?;
        }
        return result;
    }
    ctx.store.release(snapshot);
    ctx.pending_commit.push(PendingCommit {
        model,
        record: rec.clone(),
        action,
    });
    if outer {
        let pending = std::mem::take(&mut ctx.pending_commit);
        for mut p in pending {
            run_chain(ctx, p.model, &[Phase::AfterCommit], p.action, &mut p.record)?;
        }
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
}
