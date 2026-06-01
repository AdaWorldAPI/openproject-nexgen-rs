//! `scan` — dependency-free line/block scanning primitives for the Ruby
//! frontend.
//!
//! This is **not** a Ruby parser; it is a focused scanner for the regular
//! subset of ActiveRecord model files (class defs, association macros,
//! `def … end` blocks, ivar assignments). It exists so the three extractors
//! (`parse`, `fields`, `functions`) agree on how a class body is tokenised,
//! instead of each re-inventing brittle string slicing.
//!
//! Sprint C4 — Ruby/Rails frontend for `ruff_spo_triplet`. Zero external
//! parser deps by design (see the crate `Cargo.toml`).

/// Strip a trailing `# comment` that is outside a `"`/`'` string literal.
/// Returns the code portion (not trimmed).
#[must_use]
pub(crate) fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_s: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match in_s {
            Some(q) => {
                if c == b'\\' {
                    i += 2;
                    continue;
                }
                if c == q {
                    in_s = None;
                }
            }
            None => {
                if c == b'"' || c == b'\'' {
                    in_s = Some(c);
                } else if c == b'#' {
                    return &line[..i];
                }
            }
        }
        i += 1;
    }
    line
}

/// Parse the leading `:symbol, :symbol2` arguments of a Rails macro line.
///
/// `macro_symbols("has_many :time_entries, dependent: :destroy", "has_many")`
/// → `["time_entries"]`. Only the positional leading symbols are collected;
/// scanning stops at the first argument that is not a bare `:symbol` (i.e. an
/// options hash like `dependent: :destroy` or `presence: true`).
#[must_use]
pub(crate) fn macro_symbols(line: &str, macro_name: &str) -> Vec<String> {
    let code = strip_comment(line).trim();
    let rest = match code.strip_prefix(macro_name) {
        Some(r) if r.starts_with([' ', '\t']) || r.starts_with('(') => r,
        _ => return Vec::new(),
    };
    let rest = rest.trim_start().trim_start_matches('(');
    let mut out = Vec::new();
    for raw in rest.split(',') {
        let arg = raw.trim().trim_end_matches(')').trim();
        // An options-hash entry (`dependent: :destroy`, `presence: true`) ends
        // the positional run.
        if arg.contains(':') && !arg.starts_with(':') {
            break;
        }
        if let Some(sym) = arg.strip_prefix(':') {
            let name: String = sym
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                out.push(name);
            }
        } else {
            break;
        }
    }
    out
}

/// A `def name … end` method block extracted from a class body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefBlock {
    /// Method name (`compute_total_hours`). `self.` prefixes and trailing
    /// argument lists are stripped.
    pub name: String,
    /// The inner body source (between the `def` line and its matching `end`),
    /// newline-joined.
    pub body: String,
}

/// Leading block keywords whose presence as a *prefix* of a line opens a
/// block. Each entry ends with a separator (space) so `do_cleanup` / `define`
/// / `case_id` cannot false-open a block by starting with `do` / `def` / `case`.
const BLOCK_OPENERS_PREFIX: &[&str] = &[
    "def ", "if ", "unless ", "case ", "while ", "until ", "for ",
    "class ", "module ",
];

/// Block keywords that legitimately appear alone on a line. `begin` is the
/// only common one (`begin … rescue … end`). It must be tested via exact
/// match — `begin_audit` is a method name, not a block opener.
const BLOCK_OPENERS_EXACT: &[&str] = &["begin"];

fn opens_block(trimmed: &str) -> bool {
    if BLOCK_OPENERS_PREFIX.iter().any(|kw| trimmed.starts_with(kw)) {
        return true;
    }
    if BLOCK_OPENERS_EXACT.iter().any(|kw| trimmed == *kw) {
        return true;
    }
    // Trailing `do` / `do |x|` (block form: `time_entries.each do |t|`). We
    // require a whitespace boundary before `do` so a method name like
    // `do_cleanup` cannot masquerade as a block opener.
    let code = trimmed.trim_end();
    code.ends_with(" do") || (code.ends_with('|') && code.contains(" do |"))
}

fn closes_block(trimmed: &str) -> bool {
    trimmed == "end" || trimmed.starts_with("end ") || trimmed.starts_with("end.")
        || trimmed.starts_with("end)")
}

/// Split a class body into its top-level `def … end` method blocks, using
/// Ruby keyword depth counting. Nested `if`/`do`/`case` inside a method are
/// folded into that method's body; only methods at class-body depth are
/// returned.
#[must_use]
pub(crate) fn def_blocks(class_body: &str) -> Vec<DefBlock> {
    let mut out = Vec::new();
    let mut lines = class_body.lines().peekable();
    let mut current: Option<(String, Vec<String>, i32)> = None;
    while let Some(raw) = lines.next() {
        let trimmed = strip_comment(raw).trim();
        if let Some((_, body, depth)) = current.as_mut() {
            // Already inside a def: adjust depth, capture body until depth 0.
            if opens_block(trimmed) {
                *depth += 1;
            }
            if closes_block(trimmed) {
                *depth -= 1;
                if *depth == 0 {
                    let (name, body, _) = current.take().unwrap();
                    out.push(DefBlock {
                        name,
                        body: body.join("\n"),
                    });
                    continue;
                }
            }
            body.push(raw.to_string());
            continue;
        }
        // Not inside a def: look for `def NAME`.
        if let Some(after) = trimmed.strip_prefix("def ") {
            let name = after
                .trim_start_matches("self.")
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>();
            current = Some((name, Vec::new(), 1));
        }
    }
    out
}

/// Instance-variable assignment targets in a method body: `@x ||= …` and
/// `@x = …` → `["x"]`. The leading `@` is stripped. Ordered, de-duplicated.
#[must_use]
pub(crate) fn ivar_assignments(method_body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in method_body.lines() {
        let code = strip_comment(raw).trim();
        let Some(rest) = code.strip_prefix('@') else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let after = &rest[name.len()..];
        let after = after.trim_start();
        if (after.starts_with("||=") || after.starts_with('=') && !after.starts_with("=="))
            && !out.contains(&name)
        {
            out.push(name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comment_respects_strings() {
        assert_eq!(strip_comment("raise X # boom"), "raise X ");
        assert_eq!(strip_comment(r#"a = "b # c""#), r#"a = "b # c""#);
        assert_eq!(strip_comment("plain"), "plain");
    }

    #[test]
    fn macro_symbols_collects_leading_symbols_only() {
        assert_eq!(macro_symbols("has_many :time_entries", "has_many"), ["time_entries"]);
        assert_eq!(
            macro_symbols("has_many :time_entries, dependent: :destroy", "has_many"),
            ["time_entries"]
        );
        assert_eq!(
            macro_symbols("validates :subject, :author, presence: true", "validates"),
            ["subject", "author"]
        );
        assert_eq!(macro_symbols("belongs_to :project", "has_many").len(), 0);
    }

    #[test]
    fn def_blocks_handles_modifier_unless() {
        let body = "  belongs_to :project\n\
                    \n  def compute_total_hours\n\
                    \x20   raise ActiveRecord::RecordInvalid unless status\n\
                    \x20   @total_hours ||= time_entries.hours\n\
                    \x20 end\n";
        let blocks = def_blocks(body);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "compute_total_hours");
        assert!(blocks[0].body.contains("raise ActiveRecord::RecordInvalid"));
        assert!(blocks[0].body.contains("@total_hours"));
    }

    #[test]
    fn def_blocks_handles_nested_block_and_do() {
        let body = "  def total\n\
                    \x20   sum = 0\n\
                    \x20   time_entries.each do |t|\n\
                    \x20     sum += t.hours if t.billable\n\
                    \x20   end\n\
                    \x20   sum\n\
                    \x20 end\n";
        let blocks = def_blocks(body);
        assert_eq!(blocks.len(), 1, "nested do/if must not close the def early");
        assert_eq!(blocks[0].name, "total");
        assert!(blocks[0].body.contains("time_entries.each"));
    }

    #[test]
    fn ivar_assignments_finds_memoized() {
        assert_eq!(
            ivar_assignments("  @total_hours ||= time_entries.hours\n"),
            ["total_hours"]
        );
        assert_eq!(ivar_assignments("  @x = 1\n  @y ||= 2\n"), ["x", "y"]);
        // `==` is a comparison, not an assignment.
        assert_eq!(ivar_assignments("  @x == 1\n").len(), 0);
    }

    #[test]
    fn def_blocks_handles_method_names_starting_with_block_keywords() {
        // Regression: a body line like `do_cleanup` must NOT false-open a
        // block. Codex PR #4 P2: `trimmed.starts_with("do")` was matching
        // method-name prefixes (`do_cleanup`, `download_attachment`,
        // `define_method`, `begin_audit`), so every following method in the
        // class was silently dropped.
        let body = "  def first\n\
                    \x20   do_cleanup if pending?\n\
                    \x20   download_attachment(@id)\n\
                    \x20   define_method(:foo) { }\n\
                    \x20   begin_audit\n\
                    \x20 end\n\
                    \n  def second\n\
                    \x20   :ok\n\
                    \x20 end\n";
        let blocks = def_blocks(body);
        assert_eq!(
            blocks.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
            ["first", "second"],
            "both methods must be emitted — block-keyword prefixes in method names must not open a block"
        );
        // The standalone `begin … end` form still opens a block correctly.
        let begun = "  def with_begin\n\
                     \x20   begin\n\
                     \x20     do_stuff\n\
                     \x20   rescue => e\n\
                     \x20     log(e)\n\
                     \x20   end\n\
                     \x20 end\n";
        let blocks = def_blocks(begun);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "with_begin");
        assert!(blocks[0].body.contains("rescue => e"));
    }
}
