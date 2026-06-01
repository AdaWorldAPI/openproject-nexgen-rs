//! `parse_models` — walk a Rails `app/models/` tree into [`RubyClass`] records.
//! Sprint C4 fanout slot A.
//!
//! Dependency-free: model files are tokenised with [`crate::scan`] line
//! primitives, and `db/schema.rb` is read once for the per-table column
//! baseline. The class → table name mapping (`PascalCase` → `snake_case` →
//! pluralise) is inlined here because it is only needed for this column join.

use std::fs;
use std::path::Path;

use crate::RubyClass;
use crate::scan;

/// The association macros whose leading positional symbol names a relation.
const ASSOCIATION_MACROS: &[&str] = &[
    "belongs_to",
    "has_many",
    "has_one",
    "has_and_belongs_to_many",
];

/// Walk `source_tree/app/models` for `*.rb` files and build a [`RubyClass`]
/// per `class … < …Record` definition, joining DB columns from
/// `source_tree/db/schema.rb`.
///
/// Never panics: an unreadable models dir yields an empty result, an
/// unreadable file is skipped, and a missing `schema.rb` leaves `columns`
/// empty. The returned `Vec` is sorted by class name for determinism.
pub(crate) fn parse_models(source_tree: &Path) -> Vec<RubyClass> {
    let schema = parse_schema(&source_tree.join("db/schema.rb"));

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_rb_files(&source_tree.join("app/models"), &mut files);
    // Stable traversal regardless of directory-entry order.
    files.sort();

    let mut classes: Vec<RubyClass> = Vec::new();
    for path in &files {
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        if let Some(mut class) = parse_class(&source) {
            let table = pluralize(&to_snake(&class.name));
            if let Some(columns) = schema.iter().find(|(t, _)| *t == table) {
                class.columns.clone_from(&columns.1);
            }
            classes.push(class);
        }
    }

    classes.sort_by(|a, b| a.name.cmp(&b.name));
    classes
}

/// Recursively gather `*.rb` files under `dir`. A non-existent or unreadable
/// directory contributes nothing (no panic). Recursion is a bonus over the
/// flat fixture; subdir entries are descended into when present.
fn collect_rb_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rb_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rb") {
            out.push(path);
        }
    }
}

/// Parse a single model file: locate the `class <Name> < <Super>Record`
/// definition, capture its name, body (lines after the `class` line up to but
/// excluding the file's final top-level `end`), and association names.
fn parse_class(source: &str) -> Option<RubyClass> {
    let lines: Vec<&str> = source.lines().collect();

    let class_line = lines.iter().position(|line| class_name(line).is_some())?;
    let name = class_name(lines[class_line])?;

    // The body runs from the line after `class …` up to (not including) the
    // file's final top-level `end`. Use the last line that is exactly `end`
    // (after comment-stripping/trimming) as that terminator.
    let end_line = lines
        .iter()
        .rposition(|line| scan::strip_comment(line).trim() == "end")
        .filter(|&i| i > class_line);

    let body_end = end_line.unwrap_or(lines.len());
    let body_lines = &lines[class_line + 1..body_end];
    let body_source = body_lines.join("\n");

    let mut associations: Vec<String> = Vec::new();
    for line in body_lines {
        for macro_name in ASSOCIATION_MACROS {
            associations.extend(scan::macro_symbols(line, macro_name));
        }
    }

    Some(RubyClass {
        name,
        body_source,
        associations,
        columns: Vec::new(),
    })
}

/// If `line` opens an ActiveRecord class definition
/// (`class <Name> < <Something>`), return `<Name>`. Accepts any superclass —
/// `ApplicationRecord`, `ActiveRecord::Base`, or an STI parent model — since
/// the superclass is not load-bearing for the IR.
fn class_name(line: &str) -> Option<String> {
    let code = scan::strip_comment(line).trim();
    let rest = code.strip_prefix("class ")?;
    let (name, after) = split_identifier(rest.trim_start());
    if name.is_empty() {
        return None;
    }
    // Require an explicit superclass (`< …`) so we don't capture a plain
    // namespacing `class Foo` without an ActiveRecord parent.
    if after.trim_start().starts_with('<') {
        Some(name)
    } else {
        None
    }
}

/// Split off a leading Ruby identifier (alphanumerics + `_`), returning it and
/// the remaining slice.
fn split_identifier(s: &str) -> (String, &str) {
    let end = s
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(s.len(), |(i, _)| i);
    (s[..end].to_string(), &s[end..])
}

/// Parse `db/schema.rb` into `(table_name, column_names)` pairs. Each
/// `create_table "<table>" … do |t|` block contributes its `t.<type> "<col>"`
/// column names, in declaration order. A missing/unreadable file yields an
/// empty `Vec` (no panic).
fn parse_schema(path: &Path) -> Vec<(String, Vec<String>)> {
    let Ok(source) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut tables: Vec<(String, Vec<String>)> = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for raw in source.lines() {
        let code = scan::strip_comment(raw).trim();
        if let Some(table) = create_table_name(code) {
            // Defensive: flush any unterminated previous block before starting.
            if let Some(block) = current.take() {
                tables.push(block);
            }
            current = Some((table, Vec::new()));
            continue;
        }
        let Some((_, columns)) = current.as_mut() else {
            continue;
        };
        if code == "end" {
            if let Some(block) = current.take() {
                tables.push(block);
            }
            continue;
        }
        if let Some(column) = column_name(code) {
            columns.push(column);
        }
    }
    if let Some(block) = current.take() {
        tables.push(block);
    }
    tables
}

/// If `code` begins a `create_table "<table>", …` block, return `<table>`.
fn create_table_name(code: &str) -> Option<String> {
    let rest = code.strip_prefix("create_table")?;
    if !rest.starts_with([' ', '\t', '(']) {
        return None;
    }
    first_string_literal(rest)
}

/// Non-column `t.*` helpers seen in real `db/schema.rb` files. The first
/// string literal on these lines names an index, a referenced table, etc. —
/// not a column on the current table — so they must be skipped before reading
/// any string literal (Codex PR #4 P2: `t.index ["work_package_id"], name:
/// "idx_wp"` was leaking `work_package_id` as a duplicate column).
const NON_COLUMN_HELPERS: &[&str] = &[
    "index",
    "foreign_key",
    "references",
    "belongs_to",
    "primary_key",
    "check_constraint",
    "timestamps", // `t.timestamps` adds created_at/updated_at; not a single named column.
];

/// If `code` is a column declaration (`t.<type> "<col>"…`), return `<col>`.
/// The `t.index`/`t.foreign_key` etc. helpers are skipped via
/// [`NON_COLUMN_HELPERS`] so their referenced names are never mistaken for
/// columns of the current table.
fn column_name(code: &str) -> Option<String> {
    let rest = code.strip_prefix("t.")?;
    let (kind, after) = split_identifier(rest);
    if kind.is_empty() || NON_COLUMN_HELPERS.iter().any(|h| *h == kind) {
        return None;
    }
    first_string_literal(after)
}

/// Extract the first `"…"` or `'…'` string literal's contents from `s`.
fn first_string_literal(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            let quote = c;
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != quote {
                j += 1;
            }
            if j <= bytes.len() {
                return Some(s[start..j].to_string());
            }
        }
        i += 1;
    }
    None
}

/// `WorkPackage` → `work_package`. PascalCase/camelCase → snake_case: insert
/// `_` before an uppercase letter that follows a lowercase/digit or precedes a
/// lowercase run inside an acronym, then lowercase.
fn to_snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_numeric());
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
            if i > 0 && (prev_lower || (chars[i - 1].is_uppercase() && next_lower)) {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Inflect a snake_case singular table stem to plural, covering the common
/// English rules ActiveRecord's default inflector applies for these fixtures:
/// `…<consonant>y` → `…ies`, `…(s|x|ch|sh)` → `…es`, else `…s`.
fn pluralize(word: &str) -> String {
    if let Some(stem) = word.strip_suffix('y') {
        let preceded_by_vowel = stem.chars().next_back().is_some_and(is_vowel);
        if !stem.is_empty() && !preceded_by_vowel {
            return format!("{stem}ies");
        }
    }
    if word.ends_with('s')
        || word.ends_with('x')
        || word.ends_with("ch")
        || word.ends_with("sh")
    {
        return format!("{word}es");
    }
    format!("{word}s")
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_name_accepts_scalar_columns() {
        assert_eq!(column_name(r#"t.string "subject", null: false"#).as_deref(), Some("subject"));
        assert_eq!(column_name(r#"t.integer "status_id""#).as_deref(), Some("status_id"));
        assert_eq!(column_name(r#"t.datetime "created_at""#).as_deref(), Some("created_at"));
    }

    #[test]
    fn column_name_skips_non_column_helpers() {
        // Codex PR #4 P2: real schema.rb has `t.index [...]` lines whose first
        // string is an index name (or, with `["work_package_id"]`, the first
        // member of a multi-column index). Neither is a column on the table.
        assert_eq!(column_name(r#"t.index ["work_package_id"], name: "idx_wp_id""#), None);
        assert_eq!(column_name(r#"t.foreign_key "users", column: "author_id""#), None);
        assert_eq!(column_name(r#"t.references "project", null: false"#), None);
        assert_eq!(column_name(r#"t.primary_key "id""#), None);
        assert_eq!(column_name(r#"t.check_constraint "x > 0", name: "x_pos""#), None);
        assert_eq!(column_name(r#"t.timestamps null: false"#), None);
        // `belongs_to` inside create_table is the schema-helper shape, not the
        // association macro of the same name. Same skip.
        assert_eq!(column_name(r#"t.belongs_to "author""#), None);
    }
}
