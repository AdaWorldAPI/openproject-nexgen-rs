//! `extract_functions` — Rails class → [`Function`]s. Sprint C4 fanout slot C.
//!
//! Maps a [`RubyClass`] to SPO [`Function`]s with ZERO external parser deps —
//! line scanning over the class body via the shared [`crate::scan`] primitives
//! plus a couple of inline identifier/exception scanners.
//!
//! Two sources of functions:
//! 1. Each top-level `def … end` block becomes a [`Function`]; its body is
//!    scanned for bare attribute reads (column identifiers), `raise`/`errors.add`
//!    guards, and association traversals.
//! 2. Declarative validations (`validates`/`validate`) collapse into ONE
//!    synthetic `_validate` guard that raises `ActiveRecord::RecordInvalid`
//!    (see `SPO_TRIPLET_EXTRACTION.md` §5).

use ruff_spo_triplet::Function;

use crate::scan;
use crate::RubyClass;

/// The exception every declarative `ActiveRecord` validation raises, and the one
/// synthesised for an `errors.add(...)` call.
const RECORD_INVALID: &str = "ActiveRecord::RecordInvalid";

/// Map a Rails class to SPO [`Function`]s.
///
/// Ordering is deterministic: one function per `def` block in source order,
/// then (if the class declares any validation) a single synthetic `_validate`
/// guard appended last.
pub(crate) fn extract_functions(class: &RubyClass) -> Vec<Function> {
    let mut functions = Vec::new();

    for block in scan::def_blocks(&class.body_source) {
        functions.push(Function {
            name: block.name,
            reads: members_in_body(&block.body, &class.columns),
            raises: raises_in_body(&block.body),
            traverses: members_in_body(&block.body, &class.associations),
        });
    }

    if let Some(validated) = validated_columns(class) {
        functions.push(Function {
            name: "_validate".to_string(),
            reads: validated,
            raises: vec![RECORD_INVALID.to_string()],
            traverses: Vec::new(),
        });
    }

    functions
}

/// First-seen, de-duplicated identifier words in `body` that are members of
/// `set`. Used both for column reads and association traversals — membership in
/// the relevant set is what disambiguates a real attribute/relation reference
/// from an ordinary local variable or method call.
fn members_in_body(body: &str, set: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in body.lines() {
        for word in identifiers(scan::strip_comment(raw)) {
            if set.iter().any(|m| m == word) && !out.iter().any(|o| o == word) {
                out.push(word.to_string());
            }
        }
    }
    out
}

/// Exceptions raised in `body`, first-seen and de-duplicated:
/// - `raise <Exception>` → the exception class token verbatim, `::` preserved
///   (e.g. `ActiveRecord::RecordInvalid`).
/// - any `errors.add(` call → `ActiveRecord::RecordInvalid`.
fn raises_in_body(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |exc: String| {
        if !out.contains(&exc) {
            out.push(exc);
        }
    };
    for raw in body.lines() {
        let code = scan::strip_comment(raw).trim();
        if let Some(exc) = raised_exception(code) {
            push(exc);
        }
        if code.contains("errors.add") {
            push(RECORD_INVALID.to_string());
        }
    }
    out
}

/// If `code` is (or contains, as a statement) a `raise <Exception>`, return the
/// exception class token — the first `[A-Za-z0-9_:]+` run after the `raise`
/// keyword. Keeps `::` so `ActiveRecord::RecordInvalid` stays whole. Bare
/// `raise` (re-raise) and `raise "msg"` yield no token.
fn raised_exception(code: &str) -> Option<String> {
    let rest = strip_keyword(code, "raise")?;
    let token: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
        .collect();
    // Require it to look like a constant (start with an upper/`_`, not a digit
    // or a stray `:`), so `raise :sym` / `raise 1` don't masquerade as classes.
    let first = token.chars().next()?;
    if first.is_ascii_alphabetic() || first == '_' {
        Some(token)
    } else {
        None
    }
}

/// Return the remainder of `code` after a leading `keyword ` (keyword followed
/// by whitespace), with that whitespace trimmed. `None` if `code` does not
/// start with the bare keyword.
fn strip_keyword<'a>(code: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = code.strip_prefix(keyword)?;
    if rest.starts_with([' ', '\t']) {
        Some(rest.trim_start())
    } else {
        None
    }
}

/// The validated column names for a class, if it declares any validation.
///
/// Scans every line of the class body for `validates :a, :b, …` (collecting the
/// leading symbols via [`scan::macro_symbols`]) and for bare `validate …`
/// callbacks (which name no column but still mark the class as validated).
/// Returns `Some` iff at least one validation line is present; the vector holds
/// the de-duplicated validated names that are real columns, in first-seen order.
fn validated_columns(class: &RubyClass) -> Option<Vec<String>> {
    let mut any = false;
    let mut out: Vec<String> = Vec::new();
    for raw in class.body_source.lines() {
        let code = scan::strip_comment(raw).trim();
        let symbols = scan::macro_symbols(code, "validates");
        if !symbols.is_empty() {
            any = true;
            for name in symbols {
                if class.columns.contains(&name) && !out.contains(&name) {
                    out.push(name);
                }
            }
        } else if strip_keyword(code, "validate").is_some() {
            // A custom `validate :method` callback — still a raising guard, but
            // names a method, not a column, so it contributes no reads.
            any = true;
        }
    }
    if any {
        Some(out)
    } else {
        None
    }
}

/// Tokenise `line` into identifier words matching `[A-Za-z_][A-Za-z0-9_]*`.
/// A leading `@`/`@@` (ivar) is not part of an identifier char class, so
/// `@total_hours` tokenises to `total_hours` — the bare attribute name.
fn identifiers(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if is_ident_start(bytes[i]) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_continue(bytes[i]) {
                i += 1;
            }
            out.push(&line[start..i]);
        } else {
            i += 1;
        }
    }
    out
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(body: &str, associations: &[&str], columns: &[&str]) -> RubyClass {
        RubyClass {
            name: "T".to_string(),
            body_source: body.to_string(),
            associations: associations.iter().map(|s| (*s).to_string()).collect(),
            columns: columns.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn work_package_def_and_validate() {
        let body = "  belongs_to :project\n\
                    \x20 has_many :time_entries\n\
                    \n\
                    \x20 validates :subject, presence: true\n\
                    \n\
                    \x20 def compute_total_hours\n\
                    \x20   raise ActiveRecord::RecordInvalid unless status\n\
                    \x20   @total_hours ||= time_entries.hours\n\
                    \x20 end\n";
        let c = class(
            body,
            &["project", "time_entries"],
            &[
                "subject",
                "description",
                "status_id",
                "status",
                "created_at",
                "updated_at",
            ],
        );
        let fns = extract_functions(&c);
        assert_eq!(fns.len(), 2);

        assert_eq!(fns[0].name, "compute_total_hours");
        assert_eq!(fns[0].reads, ["status"]);
        assert_eq!(fns[0].raises, ["ActiveRecord::RecordInvalid"]);
        assert_eq!(fns[0].traverses, ["time_entries"]);

        assert_eq!(fns[1].name, "_validate");
        assert_eq!(fns[1].reads, ["subject"]);
        assert_eq!(fns[1].raises, ["ActiveRecord::RecordInvalid"]);
        assert!(fns[1].traverses.is_empty());
    }

    #[test]
    fn time_entry_validate_only() {
        let body = "  belongs_to :work_package\n\
                    \x20 belongs_to :user\n\
                    \n\
                    \x20 validates :hours, presence: true\n";
        let c = class(
            body,
            &["work_package", "user"],
            &["work_package_id", "user_id", "hours", "spent_on"],
        );
        let fns = extract_functions(&c);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "_validate");
        assert_eq!(fns[0].reads, ["hours"]);
        assert_eq!(fns[0].raises, ["ActiveRecord::RecordInvalid"]);
        assert!(fns[0].traverses.is_empty());
    }

    #[test]
    fn no_validation_means_no_synthetic_guard() {
        let body = "  def noop\n    1\n  end\n";
        let c = class(body, &[], &[]);
        let fns = extract_functions(&c);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "noop");
    }

    #[test]
    fn errors_add_maps_to_record_invalid() {
        let body = "  def check\n    errors.add(:base, \"bad\") if status\n  end\n";
        let c = class(body, &[], &["status"]);
        let fns = extract_functions(&c);
        assert_eq!(fns[0].raises, ["ActiveRecord::RecordInvalid"]);
        assert_eq!(fns[0].reads, ["status"]);
    }

    #[test]
    fn explicit_raise_keeps_namespaced_token() {
        assert_eq!(
            raised_exception("raise ActiveRecord::RecordInvalid unless status").as_deref(),
            Some("ActiveRecord::RecordInvalid")
        );
        assert_eq!(raised_exception("raise").as_deref(), None);
        assert_eq!(raised_exception("raise \"boom\"").as_deref(), None);
        assert_eq!(raised_exception("status = 1").as_deref(), None);
    }

    #[test]
    fn bare_validate_callback_marks_class_validated() {
        let body = "  validate :consistent_dates\n";
        let c = class(body, &[], &["start_date"]);
        let fns = extract_functions(&c);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "_validate");
        assert!(fns[0].reads.is_empty());
        assert_eq!(fns[0].raises, ["ActiveRecord::RecordInvalid"]);
    }

    #[test]
    fn identifiers_strip_ivar_sigil() {
        assert_eq!(identifiers("@total_hours ||= time_entries.hours"), [
            "total_hours",
            "time_entries",
            "hours"
        ]);
    }
}
