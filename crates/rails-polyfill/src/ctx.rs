//! The execution context a lowered body runs in.

use std::collections::BTreeMap;

use crate::callbacks::Phase;
use crate::record::Record;
use crate::store::{Store, StoreError};
use crate::validations::ValidationError;
use crate::value::Value;

/// Why a body or a save stopped early.
#[derive(Debug, Clone, PartialEq)]
pub enum Halt {
    /// `throw :abort`.
    Abort,
    /// A construct the compiler could not lower was reached at run time.
    Residual(String),
    /// `raise` — an exception escaping the body.
    Raise(String),
    Store(StoreError),
}

impl From<StoreError> for Halt {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

/// One journaled callback firing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub model: &'static str,
    pub phase: Phase,
    pub name: &'static str,
}

/// The application side of every declared boundary.
///
/// `call` answers an out-of-slice method — `Const.method` or
/// `Const#method` with its arguments: the target string is the Ruby
/// spelling, so a host implementation is reviewable against the source.
/// `callback` answers a callback whose body is outside the slice.
pub trait Host {
    fn call(
        &mut self,
        store: &Store,
        target: &str,
        recv: Option<&Record>,
        args: &[Value],
    ) -> Result<Value, Halt>;

    fn callback(
        &mut self,
        _model: &'static str,
        _phase: Phase,
        _name: &'static str,
        _rec: &Record,
    ) -> Result<(), Halt> {
        Ok(())
    }
}

/// A host that answers nothing: every boundary crossing fails. Used by the
/// negative controls.
pub struct NoHost;

impl Host for NoHost {
    fn call(
        &mut self,
        _: &Store,
        target: &str,
        _: Option<&Record>,
        _: &[Value],
    ) -> Result<Value, Halt> {
        Err(Halt::Residual(format!("no host for {target}")))
    }
}

pub struct Ctx {
    pub store: Store,
    /// `Time.zone.now`, in milliseconds since the Unix epoch. A time the body
    /// reads or stores is [`Value::time_ms`] of it.
    pub now: i64,
    pub settings: BTreeMap<String, Value>,
    pub current_user_id: Option<i64>,
    pub journal: Vec<Entry>,
    pub host: Box<dyn Host>,
    /// Errors of the record currently being validated.
    pub errors: Vec<ValidationError>,
    /// After-commit callbacks queued by the current transaction.
    pub(crate) pending_commit: Vec<crate::lifecycle::PendingCommit>,
    pub(crate) txn_depth: u32,
    /// Source of transaction participant tokens; never reused.
    pub(crate) next_participant: u64,
    /// Every boundary crossing, in order, in the oracle's spelling.
    pub boundary_log: Vec<String>,
}

/// `Target(arg,arg)` — the spelling the oracle logs boundary calls with.
fn boundary_spelling(target: &str, args: &[Value]) -> String {
    if args.is_empty() {
        return target.to_string();
    }
    let a: Vec<String> = args
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            other => other.to_string(),
        })
        .collect();
    format!("{target}({})", a.join(","))
}

impl Ctx {
    #[must_use]
    pub fn new(host: Box<dyn Host>, now: i64) -> Self {
        Self {
            store: Store::default(),
            now,
            settings: BTreeMap::new(),
            current_user_id: None,
            journal: Vec::new(),
            host,
            errors: Vec::new(),
            pending_commit: Vec::new(),
            txn_depth: 0,
            next_participant: 0,
            boundary_log: Vec::new(),
        }
    }

    /// `Setting.get(key)` — `nil` when unset. Logged as a boundary crossing.
    pub fn setting(&mut self, key: &str) -> Value {
        self.boundary_log.push(format!("Setting.get({key})"));
        self.settings.get(key).cloned().unwrap_or(Value::Nil)
    }

    pub fn host_call(
        &mut self,
        target: &str,
        recv: Option<&Record>,
        args: &[Value],
    ) -> Result<Value, Halt> {
        self.boundary_log.push(boundary_spelling(target, args));
        self.host.call(&self.store, target, recv, args)
    }
}
