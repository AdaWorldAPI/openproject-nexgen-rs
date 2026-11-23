// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Renders what the write side committed as the feed the read side accepts.
//!
//! The output is `serde_json::Value` in the shape PostgreSQL's change feed has
//! (`oracle/shadow_feed.rb`): `{"tables": {name: {"columns", "upsert",
//! "delete"}}}`. It lives here, not in `rails-polyfill`, so the runtime stays
//! free of any wire format, and not in `quack-shadow`, so the read side does
//! not learn the write side's types.

use rails_polyfill::value::{date_text, parse_date};
use rails_polyfill::{Column, ColumnType, Commit, TableChange, Value};
use serde_json::{Map, Value as J, json};

/// A value no feed cell can carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unrenderable(pub String);

/// The PostgreSQL type name the feed declares for a column. The generated
/// table keeps a column's kind and limit, not whether a `datetime`-kind column
/// was a `date`, so that is spelled by the kind's default. A `DateTime`
/// column's limit is its fractional-second precision
/// ([`Column::precision`]): `timestamp(3)` unless it says otherwise.
#[must_use]
pub fn sql_type(column: &Column) -> String {
    match (column.ty, column.limit) {
        (ColumnType::Integer, _) => "integer".into(),
        (ColumnType::String | ColumnType::Json, Some(n)) => format!("character varying({n})"),
        (ColumnType::String, None) => "character varying".into(),
        (ColumnType::Text, _) | (ColumnType::Json, None) => "text".into(),
        (ColumnType::Boolean, _) => "boolean".into(),
        (ColumnType::DateTime, _) => {
            format!("timestamp({}) without time zone", column.precision())
        }
        (ColumnType::Decimal, _) => "numeric".into(),
        (ColumnType::Date, _) => "date".into(),
        (ColumnType::Float, _) => "double precision".into(),
    }
}

/// `{"t", "epoch", "usec"}` for microseconds since the Unix epoch, spelled as
/// `QuackShadow.cell` spells a Ruby `Time`: `t` is `iso8601(6)` (six fractional
/// digits whatever the column's precision), `epoch` is `to_i`, `usec` is `usec`.
fn timestamp(us: i64) -> J {
    let secs = us.div_euclid(1_000_000);
    let usec = us.rem_euclid(1_000_000);
    let (days, of_day) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (year, month, day) = civil(days);
    json!({
        "t": format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{usec:06}Z",
            of_day / 3600,
            of_day % 3600 / 60,
            of_day % 60
        ),
        "epoch": secs,
        "usec": usec,
    })
}

/// Proleptic Gregorian date of a day count since 1970-01-01.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// A decimal as the feed spells it; a whole number keeps one fractional digit.
fn decimal(text: String) -> J {
    json!({"other": "BigDecimal", "s": text})
}

/// A date as the feed spells it: `QuackShadow.cell` renders a Ruby `Date` as
/// its class name and `Date#to_s` (`YYYY-MM-DD`).
fn date(days: i32) -> J {
    if cfg!(feature = "disable-date-column") {
        return json!(days);
    }
    json!({"other": "Date", "s": date_text(days)})
}

/// Ruby's `Float#to_s` (`flo_to_s`, numeric.c), which is what
/// `QuackShadow.cell` writes into a float cell.
///
/// Digits: the shortest string that reads back to the same `f64`, as
/// `ruby_dtoa(value, 0, ...)` (David Gay's mode 0) produces. Rust's shortest
/// digits agree except where the exact value lies precisely halfway between
/// two equally short candidates (`667962307169479.25` is halfway between
/// `...479.2` and `...479.3`): Rust takes the upper one, Gay's dtoa the one
/// with the even last digit, so a tie is resolved to the even candidate here.
///
/// Notation, with `decpt` the position of the decimal point relative to the
/// digit string (the value is `0.<digits> * 10^decpt`) and `digs` the digit
/// count, exactly as `flo_to_s` branches:
/// - `0 < decpt < digs`: fixed, the point inside the digits;
/// - `digs <= decpt <= 15` (`DBL_DIG`): fixed, zero-padded, then `.0`;
/// - `-4 < decpt <= 0`: `0.` followed by `-decpt` zeros and the digits;
/// - otherwise `d.ddde+XX` / `d.0e-XX` with at least two exponent digits.
///
/// `-0.0`, `Infinity`, `-Infinity` and `NaN` are spelled as Ruby spells them.
#[must_use]
pub fn float_text(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f < 0.0 { "-Infinity" } else { "Infinity" }.into();
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let (digits, decpt) = ruby_digits(f.abs());
    let sign = if f < 0.0 { "-" } else { "" };
    let digs = i32::try_from(digits.len()).unwrap_or(i32::MAX);
    let zeros = |n: i32| "0".repeat(usize::try_from(n).unwrap_or(0));
    let body = if decpt > 0 && decpt < digs {
        let at = usize::try_from(decpt).unwrap_or(0);
        format!("{}.{}", &digits[..at], &digits[at..])
    } else if decpt > 0 && decpt <= 15 {
        format!("{digits}{}.0", zeros(decpt - digs))
    } else if decpt <= 0 && decpt > -4 {
        format!("0.{}{digits}", zeros(-decpt))
    } else {
        let frac = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{}.{frac}e{:+03}", &digits[..1], decpt - 1)
    };
    format!("{sign}{body}")
}

/// The digits and `decpt` of a positive finite `f64` as `ruby_dtoa` mode 0
/// returns them (no trailing zeros).
fn ruby_digits(x: f64) -> (String, i32) {
    let split = |sci: &str| -> (String, i32) {
        let (mantissa, exp) = sci.split_once('e').unwrap_or((sci, "0"));
        let digits = mantissa.chars().filter(char::is_ascii_digit).collect();
        (digits, exp.parse::<i32>().unwrap_or(0) + 1)
    };
    // `{:e}` yields the shortest round-trip digits as `d[.ddd]e<exp>`.
    let (shortest, decpt) = split(&format!("{x:e}"));
    // An f64 has at most 767 significant digits, so 800 fractional digits of
    // the mantissa are the exact value.
    let (exact, exact_decpt) = split(&format!("{x:.800e}"));
    let n = shortest.len();
    if exact_decpt != decpt || exact.len() <= n {
        return (shortest, decpt);
    }
    let (head, rest) = exact.split_at(n);
    let is_tie = rest.starts_with('5') && rest[1..].bytes().all(|b| b == b'0');
    if !is_tie {
        return (shortest, decpt);
    }
    // Both candidates are equally far away; Gay's dtoa keeps the even one.
    let lower = head.to_string();
    let upper = increment(&lower);
    let even = |s: &str| {
        s.bytes()
            .last()
            .is_some_and(|b| (b - b'0').is_multiple_of(2))
    };
    let chosen = match upper {
        Some(u) if !even(&lower) && even(&u) => u,
        _ => lower,
    };
    let chosen = chosen.trim_end_matches('0');
    let chosen = if chosen.is_empty() { "0" } else { chosen };
    let reads_back = |digits: &str| {
        format!("0.{digits}e{decpt}")
            .parse::<f64>()
            .is_ok_and(|g| g == x)
    };
    if reads_back(chosen) {
        (chosen.to_string(), decpt)
    } else {
        (shortest, decpt)
    }
}

/// `digits + 1` as a decimal digit string of the same length; `None` on a
/// carry out of the first digit.
fn increment(digits: &str) -> Option<String> {
    let mut bytes = digits.as_bytes().to_vec();
    for b in bytes.iter_mut().rev() {
        if *b == b'9' {
            *b = b'0';
        } else {
            *b += 1;
            return String::from_utf8(bytes).ok();
        }
    }
    None
}

/// A float as the feed spells it: `{"f": "<Float#to_s>"}`, so `NaN` and the
/// infinities, which a JSON number cannot hold, keep their value.
fn float(f: f64) -> J {
    json!({"f": float_text(f)})
}

/// The day count of a date cell [`cell`] rendered, or a bare `YYYY-MM-DD`
/// string; `None` for anything else.
#[must_use]
pub fn read_date(cell: &J) -> Option<i32> {
    match cell {
        J::Object(o) if o.get("other").and_then(J::as_str) == Some("Date") => {
            parse_date(o.get("s")?.as_str()?)
        }
        J::String(s) => parse_date(s),
        _ => None,
    }
}

/// The `f64` of a float cell [`cell`] rendered, or a bare JSON number; `None`
/// for anything else. Ruby's spellings of the non-finite values are read as
/// the values they name.
#[must_use]
pub fn read_float(cell: &J) -> Option<f64> {
    match cell {
        J::Object(o) if o.len() == 1 => match o.get("f")?.as_str()? {
            "NaN" => Some(f64::NAN),
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            text => text.parse().ok().filter(|f: &f64| f.is_finite()),
        },
        J::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// One cell, as PostgreSQL's feed renders the same value.
pub fn cell(value: &Value, column: &Column) -> Result<J, Unrenderable> {
    Ok(match value {
        Value::Nil => J::Null,
        Value::Bool(b) => J::Bool(*b),
        Value::Int(i) if column.ty == ColumnType::Decimal => decimal(format!("{i}.0")),
        #[expect(
            clippy::cast_precision_loss,
            reason = "an Integer in a float column reads as a Float"
        )]
        Value::Int(i) if column.ty == ColumnType::Float => float(*i as f64),
        Value::Int(i) => json!(i),
        Value::Float(f) if column.ty == ColumnType::Float => float(*f),
        Value::Float(f) if column.ty == ColumnType::Decimal => decimal(if f.fract() == 0.0 {
            format!("{f:.1}")
        } else {
            f.to_string()
        }),
        Value::Float(f) => json!(f),
        Value::Str(s) | Value::Json(s) => J::String(s.clone()),
        Value::Time(us) => timestamp(*us),
        Value::Date(d) => date(*d),
        Value::List(_) | Value::Rec(_) => {
            return Err(Unrenderable(format!("{} in column {}", value, column.name)));
        }
    })
}

fn table_entry(change: &TableChange) -> Result<J, Unrenderable> {
    let spec = change.spec;
    if spec.index_of("id").is_none() {
        return Err(Unrenderable(format!("{} has no id column", spec.name)));
    }
    let columns: Vec<J> = spec
        .columns
        .iter()
        .map(|c| json!({"name": c.name, "type": sql_type(c), "null": c.null}))
        .collect();
    let upsert = change
        .upserts
        .iter()
        .map(|row| {
            spec.columns
                .iter()
                .zip(row)
                .map(|(column, value)| cell(value, column))
                .collect::<Result<Vec<J>, _>>()
                .map(J::Array)
        })
        .collect::<Result<Vec<J>, _>>()?;
    Ok(json!({"columns": columns, "upsert": upsert, "delete": change.deletes}))
}

/// The feed of one committed transaction.
pub fn feed_json(commit: &Commit) -> Result<J, Unrenderable> {
    let mut tables = Map::new();
    for change in &commit.tables {
        tables.insert(change.spec.name.to_string(), table_entry(change)?);
    }
    Ok(json!({"tables": tables}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_renders_as_the_three_key_object() {
        assert_eq!(
            timestamp(1_793_959_200_000_000),
            json!({"t": "2026-11-06T10:00:00.000000Z", "epoch": 1_793_959_200, "usec": 0})
        );
        assert_eq!(
            timestamp(1_794_048_896_789_000),
            json!({"t": "2026-11-07T10:54:56.789000Z", "epoch": 1_794_048_896, "usec": 789_000})
        );
        assert_eq!(timestamp(0)["t"], "1970-01-01T00:00:00.000000Z");
        assert_eq!(timestamp(-1_000)["t"], "1969-12-31T23:59:59.999000Z");
        assert_eq!(
            timestamp(1_794_048_896_123_456),
            json!({"t": "2026-11-07T10:54:56.123456Z", "epoch": 1_794_048_896, "usec": 123_456})
        );
        assert_eq!(timestamp(-1)["t"], "1969-12-31T23:59:59.999999Z");
    }

    #[test]
    fn a_datetime_column_declares_its_precision() {
        let mut col = column(ColumnType::DateTime);
        assert_eq!(sql_type(&col), "timestamp(3) without time zone");
        col.limit = Some(3);
        assert_eq!(sql_type(&col), "timestamp(3) without time zone");
        col.limit = Some(6);
        assert_eq!(
            sql_type(&col),
            if cfg!(feature = "disable-usec-precision") {
                "timestamp(3) without time zone"
            } else {
                "timestamp(6) without time zone"
            }
        );
    }

    #[test]
    fn a_decimal_renders_as_postgresql_spells_it_whatever_value_holds_it() {
        let decimal = Column {
            name: "time_unit",
            ty: ColumnType::Decimal,
            null: true,
            default: rails_polyfill::Default::None,
            limit: None,
        };
        let big = |s: &str| json!({"other": "BigDecimal", "s": s});
        assert_eq!(cell(&Value::Float(2.5), &decimal).unwrap(), big("2.5"));
        assert_eq!(cell(&Value::Float(3.0), &decimal).unwrap(), big("3.0"));
        assert_eq!(cell(&Value::Int(2), &decimal).unwrap(), big("2.0"));
        assert_eq!(cell(&Value::Nil, &decimal).unwrap(), J::Null);
        let integer = Column {
            ty: ColumnType::Integer,
            ..decimal
        };
        assert_eq!(cell(&Value::Int(2), &integer).unwrap(), json!(2));
    }

    fn column(ty: ColumnType) -> Column {
        Column {
            name: "c",
            ty,
            null: true,
            default: rails_polyfill::Default::None,
            limit: None,
        }
    }

    #[test]
    fn the_feed_declares_date_and_double_precision() {
        assert_eq!(sql_type(&column(ColumnType::Date)), "date");
        assert_eq!(sql_type(&column(ColumnType::Float)), "double precision");
    }

    #[test]
    fn a_date_cell_renders_as_iso_text_and_parses_back_equal() {
        let col = column(ColumnType::Date);
        // 1969-12-31, the epoch, a leap day, the day after it, a pre-1900 date.
        for (days, text) in [
            (-1, "1969-12-31"),
            (0, "1970-01-01"),
            (11_016, "2000-02-29"),
            (11_017, "2000-03-01"),
            (-25_567, "1900-01-01"),
        ] {
            let got = cell(&Value::Date(days), &col).unwrap();
            assert_eq!(got, json!({"other": "Date", "s": text}), "{days}");
            assert_eq!(read_date(&got), Some(days), "{text}");
        }
        assert_eq!(cell(&Value::Nil, &col).unwrap(), J::Null);
        assert_eq!(
            read_date(&json!("2000-02-29")),
            Some(11_016),
            "a bare text reads too"
        );
        assert_eq!(
            read_date(&json!({"other": "Date", "s": "2001-02-29"})),
            None
        );
    }

    #[test]
    fn a_float_cell_renders_as_rubys_float_to_s_and_parses_back_equal() {
        let col = column(ColumnType::Float);
        let spelled = [
            (2.5, "2.5"),
            (3.0, "3.0"),
            (-0.0, "-0.0"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1.0 / 3.0, "0.3333333333333333"),
            (123_456_789.123_456_78, "123456789.12345678"),
            (1e15, "1.0e+15"),
            (1_000_000_000_000_001.0, "1.000000000000001e+15"),
            (123_456_789_012_345.6, "123456789012345.6"),
            (f64::from_bits(0x4302_fc10_c056_663a), "667962307169479.2"),
            (999_999_999_999_999.0, "999999999999999.0"),
            (1e16, "1.0e+16"),
            (1.5e-4, "0.00015"),
            (1e-4, "0.0001"),
            (1e-5, "1.0e-05"),
            (-2.5e-300, "-2.5e-300"),
            (f64::MAX, "1.7976931348623157e+308"),
            (f64::MIN_POSITIVE, "2.2250738585072014e-308"),
            (5e-324, "5.0e-324"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
        ];
        for (f, text) in spelled {
            let got = cell(&Value::Float(f), &col).unwrap();
            assert_eq!(got, json!({"f": text}), "{f:e}");
            let back = read_float(&got).unwrap();
            assert_eq!(back.to_bits(), f.to_bits(), "{text}");
        }
        let nan = cell(&Value::Float(f64::NAN), &col).unwrap();
        assert_eq!(nan, json!({"f": "NaN"}));
        assert!(read_float(&nan).unwrap().is_nan());
        assert_eq!(cell(&Value::Int(7), &col).unwrap(), json!({"f": "7.0"}));
        assert_eq!(read_float(&json!(2.5)), Some(2.5));
        assert_eq!(read_float(&json!({"f": "junk"})), None);
    }

    #[test]
    fn a_float_in_another_column_type_still_renders_as_a_bare_number() {
        let integer = column(ColumnType::Integer);
        assert_eq!(cell(&Value::Float(2.5), &integer).unwrap(), json!(2.5));
    }
}
