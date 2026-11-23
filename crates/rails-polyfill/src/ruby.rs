//! Ruby core semantics the lowered bodies call.

use regex::Regex;

use crate::ctx::Halt;
use crate::value::Value;

/// Translate a Ruby (Onigmo) regexp literal to `regex` crate syntax, keeping
/// the meaning rather than the spelling:
///
/// - `^` / `$` are line anchors in Ruby, always — `(?m)` makes them so here;
/// - `\s`, `\d`, `\w` are ASCII classes in Ruby, Unicode classes here unless
///   scoped with `(?-u:…)`;
/// - the `i` / `m` / `x` flags: Ruby `m` is "dot matches newline", which is
///   `s` here.
///
/// Constructs with no equivalent (look-behind, backreferences, possessive
/// quantifiers, `\h`, `\A`-style anchors are fine) return an error so the
/// compiler records a residual instead of guessing.
pub fn translate_regex(pattern: &str, flags: &str) -> Result<String, String> {
    let mut out = String::from("(?m");
    if flags.contains('i') {
        out.push('i');
    }
    if flags.contains('m') {
        out.push('s');
    }
    if flags.contains('x') {
        out.push('x');
    }
    out.push(')');
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(e @ ('s' | 'd' | 'w' | 'S' | 'D' | 'W')) => {
                    out.push_str("(?-u:\\");
                    out.push(e);
                    out.push(')');
                }
                Some('h') => out.push_str("[0-9a-fA-F]"),
                Some(d @ '1'..='9') => return Err(format!("backreference \\{d}")),
                Some('k') => return Err("named backreference".into()),
                Some(e) => {
                    out.push('\\');
                    out.push(e);
                }
                None => return Err("trailing backslash".into()),
            }
        } else if c == '(' && chars.peek() == Some(&'?') {
            let mut look = chars.clone();
            look.next();
            match look.next() {
                Some('<') if matches!(look.peek(), Some('=' | '!')) => {
                    return Err("look-behind".into());
                }
                Some('=' | '!') => return Err("look-ahead".into()),
                Some('>') => return Err("atomic group".into()),
                _ => out.push(c),
            }
        } else if matches!(c, '+' | '*' | '?' | '}') && chars.peek() == Some(&'+') {
            return Err("possessive quantifier".into());
        } else {
            out.push(c);
        }
    }
    Regex::new(&out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn want_str<'a>(v: &'a Value, what: &str) -> Result<&'a str, Halt> {
    v.as_str()
        .ok_or_else(|| Halt::Raise(format!("NoMethodError: undefined method `{what}' for {v}")))
}

/// `String#gsub(re, replacement)` with a literal replacement (no `\1`).
pub fn gsub(v: &Value, re: &Regex, replacement: &str) -> Result<Value, Halt> {
    let s = want_str(v, "gsub")?;
    Ok(Value::Str(
        re.replace_all(s, regex::NoExpand(replacement)).into_owned(),
    ))
}

/// `String#match?(re)`.
pub fn is_match(v: &Value, re: &Regex) -> Result<bool, Halt> {
    Ok(re.is_match(want_str(v, "match?")?))
}

fn no_method(method: &str, recv: &Value) -> Halt {
    Halt::Raise(format!(
        "NoMethodError: undefined method `{method}' for {recv}"
    ))
}

/// `record.attribute` on a record value whose model is known at compile time.
pub fn attr(v: &Value, col: usize, name: &str) -> Result<Value, Halt> {
    match v {
        Value::Rec(r) => Ok(r.get(col).clone()),
        other => Err(no_method(name, other)),
    }
}

/// `record&.attribute`.
#[must_use]
pub fn attr_safe(v: &Value, col: usize) -> Value {
    match v {
        Value::Rec(r) => r.get(col).clone(),
        _ => Value::Nil,
    }
}

/// `record.attribute` when the receiver's model is not known statically:
/// resolved by name at run time. Counted by the compiler as dynamic dispatch.
pub fn attr_named(v: &Value, name: &str) -> Result<Value, Halt> {
    match v {
        Value::Rec(r) if r.table.has_column(name) => Ok(r.get_named(name)),
        other => Err(no_method(name, other)),
    }
}

/// ActiveRecord `attribute?` (`query_attribute`): booleans as themselves,
/// numbers by non-zero, everything else by `present?`.
#[must_use]
pub fn query_attribute(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        other => other.present(),
    }
}

/// Ruby `==`.
#[must_use]
pub fn eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => {
            #[expect(
                clippy::cast_precision_loss,
                reason = "Ruby Integer==Float compares numerically"
            )]
            let xf = *x as f64;
            xf == *y
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_is_a_line_anchor_like_ruby() {
        let re = Regex::new(&translate_regex("^pending", "i").unwrap()).unwrap();
        assert!(re.is_match("Pending reminder"));
        assert!(re.is_match("x\npending close"));
    }

    #[test]
    fn whitespace_class_is_ascii_like_ruby() {
        let re = Regex::new(&translate_regex(r"\s|\t|\r", "").unwrap()).unwrap();
        // U+00A0 NO-BREAK SPACE is \s in Unicode, not in Ruby.
        assert_eq!(
            gsub(&Value::str("a\u{a0}b\tc"), &re, " ").unwrap(),
            Value::str("a\u{a0}b c")
        );
    }

    #[test]
    fn unsupported_constructs_are_refused_not_approximated() {
        assert!(translate_regex(r"(?<=a)b", "").is_err());
        assert!(translate_regex(r"(a)\1", "").is_err());
    }
}
