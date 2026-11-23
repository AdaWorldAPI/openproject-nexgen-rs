//! Ruby value semantics, as far as ActiveRecord attributes expose them.
//!
//! The rules that matter for lowering Rails code are not Rust's:
//! - only `nil` and `false` are falsy (`0` and `""` are truthy);
//! - `blank?` is true for `nil`, `false`, empty or whitespace-only strings and
//!   empty collections, and false for every number;
//! - `==` between an Integer and a String is false, never a coercion.

use std::fmt;

/// Microseconds since the Unix epoch, UTC. PostgreSQL `timestamp` keeps
/// microseconds, and a column's precision (see [`crate::Column::precision`])
/// decides how many of them a write keeps: `datetime(3)`, the default, keeps
/// whole milliseconds, so a persisted time round-trips without loss.
pub type Timestamp = i64;

/// Microseconds in a millisecond.
pub const US_PER_MS: i64 = 1000;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Time(Timestamp),
    /// A calendar date (`date` column, Ruby `Date`): days since 1970-01-01 in
    /// the proleptic Gregorian calendar, negative before it.
    Date(i32),
    /// A Ruby array, as returned by `changes_to_save['x']` (`[old, new]`) or a
    /// relation materialised into a list.
    List(Vec<Value>),
    /// A serialized attribute (`store :preferences`) kept as its JSON text.
    Json(String),
    /// An ActiveRecord instance as a Ruby value (`state = Order::Status.find_by(…)`).
    Rec(Box<crate::record::Record>),
}

impl Value {
    pub fn str(s: impl Into<String>) -> Self {
        Self::Str(s.into())
    }

    /// A time from milliseconds since the Unix epoch.
    #[must_use]
    pub fn time_ms(ms: i64) -> Self {
        Self::Time(ms.saturating_mul(US_PER_MS))
    }

    /// Ruby truthiness.
    #[must_use]
    pub fn truthy(&self) -> bool {
        !matches!(self, Self::Nil | Self::Bool(false))
    }

    /// `Object#blank?` from ActiveSupport.
    #[must_use]
    pub fn blank(&self) -> bool {
        match self {
            Self::Nil | Self::Bool(false) => true,
            Self::Bool(true)
            | Self::Int(_)
            | Self::Float(_)
            | Self::Time(_)
            | Self::Date(_)
            | Self::Rec(_) => false,
            Self::Str(s) => s.chars().all(char::is_whitespace),
            Self::List(l) => l.is_empty(),
            Self::Json(j) => {
                let t = j.trim();
                t.is_empty() || t == "{}" || t == "[]" || t == "null"
            }
        }
    }

    #[must_use]
    pub fn present(&self) -> bool {
        !self.blank()
    }

    #[must_use]
    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }

    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The time in microseconds since the Unix epoch.
    #[must_use]
    pub fn as_time(&self) -> Option<Timestamp> {
        match self {
            Self::Time(t) => Some(*t),
            _ => None,
        }
    }

    /// The time in whole milliseconds since the Unix epoch, rounded toward
    /// negative infinity.
    #[must_use]
    pub fn as_time_ms(&self) -> Option<i64> {
        self.as_time().map(|us| us.div_euclid(US_PER_MS))
    }

    #[must_use]
    pub fn as_date(&self) -> Option<i32> {
        match self {
            Self::Date(d) => Some(*d),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Ruby `Array#[]` with an integer index; `nil` past the end or on a
    /// non-array receiver that is itself `nil` (`changes_to_save['x'][1]` is
    /// only ever reached after a presence test, but the lowering does not rely
    /// on that).
    #[must_use]
    pub fn index(&self, i: i64) -> Value {
        match self {
            Self::List(l) => {
                let len = i64::try_from(l.len()).unwrap_or(i64::MAX);
                let at = if i < 0 { len + i } else { i };
                usize::try_from(at)
                    .ok()
                    .and_then(|u| l.get(u))
                    .cloned()
                    .unwrap_or(Self::Nil)
            }
            _ => Self::Nil,
        }
    }

    /// Ruby `Time#sec` for a `Time` value.
    #[must_use]
    pub fn time_sec(&self) -> Option<i64> {
        self.as_time()
            .map(|us| us.div_euclid(1_000_000).rem_euclid(60))
    }

    /// Ruby `Time#change(sec: 0)` — also clears sub-second precision, exactly
    /// as ActiveSupport does when `sec:` is given without `usec:`.
    #[must_use]
    pub fn time_change_sec_zero(&self) -> Value {
        match self {
            Self::Time(us) => {
                let secs = us.div_euclid(1_000_000);
                Self::Time((secs - secs.rem_euclid(60)) * 1_000_000)
            }
            other => other.clone(),
        }
    }

    /// Ruby `Integer#zero?`.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        matches!(self, Self::Int(0)) || matches!(self, Self::Float(f) if *f == 0.0)
    }

    /// Ruby `Array#include?`.
    #[must_use]
    pub fn includes(&self, needle: &Value) -> bool {
        match self {
            Self::List(l) => l.contains(needle),
            _ => false,
        }
    }
}

/// The proleptic Gregorian `(year, month, day)` of a day count since
/// 1970-01-01.
#[must_use]
pub fn civil_from_days(days: i32) -> (i64, u32, u32) {
    let z = i64::from(days) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// The largest `|year|` [`days_from_civil`] looks at. An `i32` day count spans
/// about +-5.88 million years, so any year past this bound cannot fit and is
/// refused before it reaches the day arithmetic, which stays far from `i64`
/// overflow for every year inside the bound.
const MAX_YEAR_MAGNITUDE: i64 = 6_000_000;

/// [`days_from_civil`] without the final `i32` narrowing; `None` for a date
/// that does not exist or whose year is past [`MAX_YEAR_MAGNITUDE`].
fn days_from_civil_wide(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(-MAX_YEAR_MAGNITUDE..=MAX_YEAR_MAGNITUDE).contains(&year) {
        return None;
    }
    let leap = year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0);
    let in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if day == 0 || day > in_month {
        return None;
    }
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// The day count since 1970-01-01 of a proleptic Gregorian date; `None` for a
/// date that does not exist (month 13, 30 February) or does not fit an `i32`
/// (including any year too large for the arithmetic to run safely).
#[must_use]
pub fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i32> {
    i32::try_from(days_from_civil_wide(year, month, day)?).ok()
}

/// `YYYY-MM-DD`, as Ruby's `Date#to_s` and PostgreSQL's `date` output spell
/// it; a year below 1 keeps a leading `-` (`-0001-01-01`), a year past 9999
/// its extra digits.
#[must_use]
pub fn date_text(days: i32) -> String {
    let (year, month, day) = civil_from_days(days);
    let sign = if year < 0 { "-" } else { "" };
    format!("{sign}{:04}-{month:02}-{day:02}", year.abs())
}

/// The day count of a `YYYY-MM-DD` text (see [`date_text`]); `None` for
/// anything else, including a date that does not exist.
#[must_use]
pub fn parse_date(text: &str) -> Option<i32> {
    let (neg, rest) = text.strip_prefix('-').map_or((false, text), |r| (true, r));
    let mut parts = rest.rsplitn(3, '-');
    let (day, month, year) = (parts.next()?, parts.next()?, parts.next()?);
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if day.len() != 2
        || month.len() != 2
        || year.len() < 4
        || ![day, month, year].iter().all(|s| digits(s))
    {
        return None;
    }
    let year: i64 = year.parse().ok()?;
    days_from_civil(
        if neg { -year } else { year },
        month.parse().ok()?,
        day.parse().ok()?,
    )
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Str(s.to_string())
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(o: Option<T>) -> Self {
        o.map_or(Self::Nil, Into::into)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("nil"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(x) => write!(f, "{x}"),
            Self::Str(s) => write!(f, "{s:?}"),
            // Whole milliseconds print as before (`@1767262837250`); a time
            // with microseconds keeps them (`@1767262837250123us`).
            Self::Time(t) if t.rem_euclid(US_PER_MS) == 0 => write!(f, "@{}", t / US_PER_MS),
            Self::Time(t) => write!(f, "@{t}us"),
            Self::Date(d) => f.write_str(&date_text(*d)),
            Self::List(l) => {
                f.write_str("[")?;
                for (i, v) in l.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str("]")
            }
            Self::Json(j) => f.write_str(j),
            Self::Rec(r) => write!(f, "#<{} id: {:?}>", r.table.name, r.id()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruby_truthiness_is_not_rust_truthiness() {
        assert!(Value::Int(0).truthy());
        assert!(Value::str("").truthy());
        assert!(!Value::Nil.truthy());
        assert!(!Value::Bool(false).truthy());
    }

    #[test]
    fn blank_follows_active_support() {
        assert!(Value::str("  \t").blank());
        assert!(!Value::Int(0).blank());
        assert!(Value::List(vec![]).blank());
        assert!(!Value::List(vec![Value::Nil]).blank());
    }

    #[test]
    fn integer_and_string_never_compare_equal() {
        assert_ne!(Value::Int(1), Value::str("1"));
    }

    #[test]
    fn a_date_is_days_since_the_epoch_and_prints_as_iso() {
        assert_eq!(date_text(0), "1970-01-01");
        assert_eq!(date_text(-1), "1969-12-31");
        assert_eq!(date_text(11_016), "2000-02-29", "a leap day");
        assert_eq!(Value::Date(20_453).to_string(), "2025-12-31");
        assert!(Value::Date(0).present(), "a date is never blank");
    }

    #[test]
    fn a_date_text_parses_back_to_its_day_count() {
        for d in [
            0,
            -1,
            1,
            11_016,
            -719_162,
            -719_528,
            2_932_896,
            i32::MIN / 1000,
            i32::MAX / 1000,
        ] {
            assert_eq!(parse_date(&date_text(d)), Some(d), "{d} = {}", date_text(d));
        }
        assert_eq!(parse_date("2025-02-29"), None, "not a leap year");
        assert_eq!(parse_date("2024-02-29"), Some(19_782));
        assert_eq!(parse_date("2025-13-01"), None);
        assert_eq!(parse_date("2025-1-01"), None);
        assert_eq!(parse_date("2025-01-01T00:00:00"), None);
    }

    #[test]
    fn a_huge_year_is_refused_without_overflowing() {
        assert_eq!(
            parse_date("9223372036854775807-01-01"),
            None,
            "i64::MAX year"
        );
        assert_eq!(
            parse_date("-9223372036854775807-01-01"),
            None,
            "huge negative year"
        );
        assert_eq!(
            parse_date("9223372036854775808-01-01"),
            None,
            "past i64::MAX"
        );
        assert_eq!(
            parse_date("-9223372036854775808-01-01"),
            None,
            "i64::MIN magnitude"
        );
        assert_eq!(
            parse_date("99999999999999999999999-01-01"),
            None,
            "past any integer"
        );
        assert_eq!(days_from_civil(i64::MAX, 1, 1), None);
        assert_eq!(days_from_civil(i64::MIN, 1, 1), None);
        assert_eq!(days_from_civil(MAX_YEAR_MAGNITUDE + 1, 1, 1), None);
        assert_eq!(days_from_civil(-MAX_YEAR_MAGNITUDE - 1, 1, 1), None);
    }

    #[test]
    fn the_i32_day_edges_round_trip_and_one_past_each_is_refused() {
        assert_eq!(date_text(i32::MAX), "5881580-07-11");
        assert_eq!(date_text(i32::MIN), "-5877641-06-23");
        assert_eq!(parse_date("5881580-07-11"), Some(i32::MAX));
        assert_eq!(parse_date("-5877641-06-23"), Some(i32::MIN));
        // One day past each edge is a real date whose day count only fits an i64.
        assert_eq!(
            days_from_civil_wide(5_881_580, 7, 12),
            Some(i64::from(i32::MAX) + 1)
        );
        assert_eq!(
            days_from_civil_wide(-5_877_641, 6, 22),
            Some(i64::from(i32::MIN) - 1)
        );
        assert_eq!(parse_date("5881580-07-12"), None);
        assert_eq!(parse_date("-5877641-06-22"), None);
        // The anti-vacuity half: the neighbours inside the range still parse.
        assert_eq!(parse_date("5881580-07-10"), Some(i32::MAX - 1));
        assert_eq!(parse_date("-5877641-06-24"), Some(i32::MIN + 1));
    }

    #[test]
    fn change_sec_zero_truncates_to_the_minute() {
        // 2026-01-01T10:20:37.250Z
        let t = Value::time_ms(1_767_262_837_250);
        assert_eq!(t.time_sec(), Some(37));
        assert_eq!(t.time_change_sec_zero(), Value::time_ms(1_767_262_800_000));
        // The microseconds go too.
        let t = Value::Time(1_767_262_837_250_123);
        assert_eq!(t.time_change_sec_zero(), Value::time_ms(1_767_262_800_000));
    }

    #[test]
    fn a_time_is_microseconds_and_prints_whole_milliseconds_as_before() {
        assert_eq!(
            Value::time_ms(1_767_262_837_250),
            Value::Time(1_767_262_837_250_000)
        );
        assert_eq!(
            Value::time_ms(1_767_262_837_250).to_string(),
            "@1767262837250"
        );
        assert_eq!(
            Value::Time(1_767_262_837_250_123).to_string(),
            "@1767262837250123us"
        );
        assert_eq!(
            Value::Time(-1).as_time_ms(),
            Some(-1),
            "floors toward negative infinity"
        );
        assert_eq!(Value::Time(1_999).as_time_ms(), Some(1));
    }
}
