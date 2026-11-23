//! Column and table descriptions, as the migration DSL declares them.

use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Integer,
    String,
    Text,
    Boolean,
    DateTime,
    Decimal,
    Json,
    /// A calendar `date` column: a [`Value::Date`], days since 1970-01-01.
    Date,
    /// A `float` / `double precision` column: a [`Value::Float`] holding an
    /// IEEE-754 `f64`, `NaN` and the infinities included.
    Float,
}

/// A column default as written in the migration (`default: ''`,
/// `default: true`, …). `None` means the column has no default and a new
/// record starts with `nil`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Default {
    None,
    Int(i64),
    Str(&'static str),
    Bool(bool),
}

impl Default {
    #[must_use]
    pub fn value(self) -> Value {
        match self {
            Self::None => Value::Nil,
            Self::Int(i) => Value::Int(i),
            Self::Str(s) => Value::str(s),
            Self::Bool(b) => Value::Bool(b),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Column {
    pub name: &'static str,
    pub ty: ColumnType,
    pub null: bool,
    pub default: Default,
    /// A string's length. On a [`ColumnType::DateTime`] column it is the
    /// fractional-second precision (`limit: 3` is `datetime(3)`), see
    /// [`Column::precision`].
    pub limit: Option<u32>,
}

/// The fractional-second digits a `DateTime` column keeps unless it says
/// otherwise: milliseconds, `datetime(3)`.
pub const DEFAULT_TIME_PRECISION: u32 = 3;

/// The most fractional-second digits a stored time has: microseconds.
pub const MAX_TIME_PRECISION: u32 = 6;

/// ActiveRecord's default `locking_column`.
pub const LOCKING_COLUMN: &str = "lock_version";

impl Column {
    /// The fractional-second digits a write to this `DateTime` column keeps:
    /// `limit` (0 to 6, as PostgreSQL's `timestamp(p)`), milliseconds when
    /// there is none. With feature `disable-usec-precision` no column keeps
    /// more than milliseconds.
    #[must_use]
    pub fn precision(&self) -> u32 {
        let p = self
            .limit
            .unwrap_or(DEFAULT_TIME_PRECISION)
            .min(MAX_TIME_PRECISION);
        if cfg!(feature = "disable-usec-precision") {
            p.min(DEFAULT_TIME_PRECISION)
        } else {
            p
        }
    }

    /// `us` cut down to this column's precision, as ActiveModel's
    /// `apply_seconds_precision` does on the way in: the digits past it are
    /// dropped, not rounded (toward negative infinity before the epoch, where
    /// Ruby's `nsec` is still non-negative).
    #[must_use]
    pub fn truncate_time(&self, us: i64) -> i64 {
        let step = 10_i64.pow(MAX_TIME_PRECISION - self.precision());
        // Floor, as ActiveModel does. Only near `i64::MIN` is the floored
        // multiple not representable; there it is cut toward zero instead (far
        // outside PostgreSQL's timestamp range, so never real data).
        us.checked_sub(us.rem_euclid(step))
            .unwrap_or(us / step * step)
    }
}

#[derive(Debug)]
pub struct TableSpec {
    pub name: &'static str,
    pub columns: &'static [Column],
}

impl TableSpec {
    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name == name)
    }

    #[must_use]
    pub fn has_column(&self, name: &str) -> bool {
        self.index_of(name).is_some()
    }

    /// The optimistic-locking column: `lock_version`, when the table has it
    /// (`lock_optimistically` defaults to true and `locking_column` to
    /// `"lock_version"`, locking/optimistic.rb:56,162-169).
    #[must_use]
    pub fn locking_column(&self) -> Option<usize> {
        self.index_of(LOCKING_COLUMN)
    }
}
