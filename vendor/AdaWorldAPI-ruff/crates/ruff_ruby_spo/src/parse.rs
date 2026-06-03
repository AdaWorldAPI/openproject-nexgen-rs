//! `parse_models` — walk a Rails `app/models/` tree into [`RubyClass`]
//! records, driven by `lib_ruby_parser` (Sprint C17a — parser graduation).
//!
//! Each model file is fed through the full Ruby AST parser; the first
//! top-level `class … < …` node is taken as the model, its superclass and
//! its association-macro Send nodes are captured into the [`RubyClass`]
//! shape, and `db/schema.rb` columns are joined by inflected table name.
//!
//! What the AST adds over the C4 line scanner: see the module-level docs
//! of [`crate`] §"What the AST adds on top of the line scanner". The new
//! [`AssociationDecl`] values are populated here.
//!
//! Determinism: the file walk is sorted; classes are returned sorted by
//! name; association options are captured in source order. Panic-free:
//! an unreadable models dir yields an empty result, an unreadable file is
//! skipped, a parse-error file is skipped (no panic, no partial pollution),
//! a missing `schema.rb` leaves columns empty.

use std::fs;
use std::path::Path;

use lib_ruby_parser::{Bytes, Node, Parser, ParserOptions, nodes};

use crate::scan;
use crate::{ASSOCIATION_MACROS, AssociationDecl, RubyClass};

/// Walk `source_tree/app/models` for `*.rb` files and build a [`RubyClass`]
/// per `class … < …` definition, joining DB columns from
/// `source_tree/db/schema.rb`.
pub(crate) fn parse_models(source_tree: &Path) -> Vec<RubyClass> {
    let schema = parse_schema(&source_tree.join("db/schema.rb"));

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    collect_rb_files(&source_tree.join("app/models"), &mut files);
    files.sort();

    let mut classes: Vec<RubyClass> = Vec::new();
    for path in &files {
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        if let Some(mut class) = parse_class_via_ast(&source) {
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
/// directory contributes nothing (no panic).
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

/// Parse a Ruby model file via `lib_ruby_parser`, locate its top-level
/// class definition, and build a [`RubyClass`].
///
/// Returns `None` if the file does not contain a parseable Ruby AST, or if
/// the first class node has no explicit superclass (matches the C4
/// "ActiveRecord-style only" filter — a bare `class Foo … end` without a
/// parent is not a model).
fn parse_class_via_ast(source: &str) -> Option<RubyClass> {
    let options = ParserOptions {
        buffer_name: "model.rb".to_string(),
        ..Default::default()
    };
    let parser = Parser::new(source.as_bytes().to_vec(), options);
    let result = parser.do_parse();
    let ast = result.ast?;

    let class_node = find_first_class(&ast)?;
    let name = leaf_const_name(&class_node.name)?;
    let superclass_node = class_node.superclass.as_ref()?;
    let superclass = const_name(superclass_node);

    let body_source = extract_body_source(source, class_node);

    let mut associations: Vec<String> = Vec::new();
    let mut association_options: Vec<AssociationDecl> = Vec::new();
    for stmt in top_level_statements(&class_node.body) {
        if let Node::Send(send) = stmt {
            if send.recv.is_none()
                && ASSOCIATION_MACROS.iter().any(|m| *m == send.method_name.as_str())
            {
                if let Some(decl) = parse_association_send(send) {
                    associations.push(decl.name.clone());
                    association_options.push(decl);
                }
            }
        }
    }

    Some(RubyClass {
        name,
        body_source,
        associations,
        columns: Vec::new(),
        superclass,
        association_options,
    })
}

/// Walk the AST root for the first `Class` node. Looks through `Begin`
/// (multi-statement file body) and into `Module` bodies — OP has a couple
/// of models wrapped in `module Plugin; class Foo < Bar; end; end`.
fn find_first_class(node: &Node) -> Option<&nodes::Class> {
    match node {
        Node::Class(c) => Some(c),
        Node::Begin(b) => {
            for stmt in &b.statements {
                if let Some(c) = find_first_class(stmt) {
                    return Some(c);
                }
            }
            None
        }
        Node::Module(m) => m.body.as_deref().and_then(find_first_class),
        _ => None,
    }
}

/// Leaf name of a `Const` node: `WorkPackage`, or `Bar` from `Foo::Bar`.
/// Used for the class's own name (we capture just the leaf — matches C4).
fn leaf_const_name(node: &Node) -> Option<String> {
    if let Node::Const(c) = node {
        Some(c.name.clone())
    } else {
        None
    }
}

/// Full dotted name of a `Const` chain — `A::B::C` from a Const whose
/// scope is `A::B`. Used for superclass capture so an STI parent like
/// `ActiveRecord::Base` stays whole.
fn const_name(node: &Node) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = node;
    loop {
        let Node::Const(c) = cur else { return None };
        parts.insert(0, c.name.clone());
        match c.scope.as_deref() {
            Some(scope) => cur = scope,
            None => break,
        }
    }
    if parts.is_empty() { None } else { Some(parts.join("::")) }
}

/// The class body as a flat list of top-level statements. Handles all three
/// shapes lib_ruby_parser can return: `None` (empty body), `Some(Begin)`
/// (multiple statements), or `Some(single_node)` (single statement).
fn top_level_statements(body: &Option<Box<Node>>) -> Vec<&Node> {
    match body.as_deref() {
        None => Vec::new(),
        Some(Node::Begin(b)) => b.statements.iter().collect(),
        Some(node) => vec![node],
    }
}

/// Parse one of the four association macros into [`AssociationDecl`].
///
/// `send` is expected to be unqualified (`send.recv` is `None`) and its
/// `method_name` to be one of [`ASSOCIATION_MACROS`]; both are checked at
/// the call site. Returns `None` if the first positional arg is not a
/// `Sym` (the macro requires a leading symbol; any other shape is
/// non-Rails-canonical and we'd rather under-extract than mis-extract).
fn parse_association_send(send: &nodes::Send) -> Option<AssociationDecl> {
    let first = send.args.first()?;
    let name = sym_name(first)?;

    let mut decl = AssociationDecl {
        macro_name: send.method_name.clone(),
        name,
        ..Default::default()
    };

    // Subsequent args: options come as a Kwargs node (Ruby 3.0+ trailing
    // keyword args, the modern form — `belongs_to :x, dependent: :destroy`)
    // or a Hash (legacy braced form — `belongs_to :x, { dependent: :destroy
    // }`). Both shapes carry the same `Pair` children. An optional `->{...}`
    // scope-block can sit between the leading symbol and the kwargs; it is
    // ignored on purpose — its body is queried via the `body_source`
    // traversal layer (`fields` / `functions`), not the option-set layer.
    for arg in send.args.iter().skip(1) {
        let pairs: Option<&Vec<Node>> = match arg {
            Node::Hash(h) => Some(&h.pairs),
            Node::Kwargs(k) => Some(&k.pairs),
            _ => None,
        };
        let Some(pairs) = pairs else { continue };
        for pair in pairs {
            if let Node::Pair(p) = pair {
                apply_pair(&mut decl, &p.key, &p.value);
            }
        }
    }
    Some(decl)
}

/// Map one option-hash pair onto an [`AssociationDecl`] field. Unknown
/// keys are silently dropped — Rails grows new association options over
/// time and we shouldn't blow up on ones we don't model.
fn apply_pair(decl: &mut AssociationDecl, key: &Node, value: &Node) {
    let Some(k) = sym_name(key) else { return };
    match k.as_str() {
        "class_name" => decl.class_name = str_value(value),
        "foreign_key" => decl.foreign_key = sym_or_str(value),
        "polymorphic" => decl.polymorphic = bool_value(value),
        "through" => decl.through = sym_or_str(value),
        "source" => decl.source = sym_or_str(value),
        "as" => decl.as_target = sym_or_str(value),
        "dependent" => decl.dependent = sym_or_str(value),
        "optional" => decl.optional = bool_value(value),
        "inverse_of" => decl.inverse_of = sym_or_str(value),
        _ => {}
    }
}

fn sym_name(node: &Node) -> Option<String> {
    if let Node::Sym(s) = node {
        Some(bytes_to_string(&s.name))
    } else {
        None
    }
}

fn str_value(node: &Node) -> Option<String> {
    if let Node::Str(s) = node {
        Some(bytes_to_string(&s.value))
    } else {
        None
    }
}

/// Either a `:symbol` or a `"string"` literal. Both forms are accepted by
/// Rails for options like `dependent: :destroy` (Sym) vs `foreign_key:
/// "user_id"` (Str); some codebases mix them freely so we collapse both
/// into the same `Option<String>`.
fn sym_or_str(node: &Node) -> Option<String> {
    sym_name(node).or_else(|| str_value(node))
}

fn bool_value(node: &Node) -> Option<bool> {
    match node {
        Node::True(_) => Some(true),
        Node::False(_) => Some(false),
        _ => None,
    }
}

/// `lib_ruby_parser::Bytes` → owned String, lossy-UTF8. OpenProject models
/// are ASCII so the lossy conversion is a no-op in practice; it's just here
/// so a stray non-UTF-8 byte in a comment can't panic the whole walk.
fn bytes_to_string(b: &Bytes) -> String {
    String::from_utf8_lossy(&b.raw).into_owned()
}

/// Extract the class body text (`fields.rs` / `functions.rs` scan this
/// string with the C4 line primitives). Uses the AST's `expression_l` for
/// the class node and its `end_l` for the closing `end` — both are byte
/// offsets into the original source. Strips the `class X < Y` opening line
/// and the trailing `end` to match the C4 line-scanner's body shape.
fn extract_body_source(source: &str, class: &nodes::Class) -> String {
    let begin = class.expression_l.begin;
    let end = class.end_l.end;
    if begin >= end || end > source.len() {
        return String::new();
    }
    let span = &source[begin..end];
    // Body starts after the first newline (end of `class X < Y` line).
    let body_start_in_span = span.find('\n').map_or(span.len(), |p| p + 1);
    // Body ends before the closing `end` (3 bytes). end_l covers the full
    // `end` keyword so we just trim that fixed-width suffix.
    let body_end_in_span = span.len().saturating_sub(3);
    if body_start_in_span >= body_end_in_span {
        return String::new();
    }
    span[body_start_in_span..body_end_in_span].to_string()
}

// ---------------------------------------------------------------------------
// db/schema.rb (unchanged from C4 — schema.rb is structured Ruby DSL where
// the line scanner is already the right tool; no AST needed)
// ---------------------------------------------------------------------------

/// Parse `db/schema.rb` into `(table_name, column_names)` pairs.
fn parse_schema(path: &Path) -> Vec<(String, Vec<String>)> {
    let Ok(source) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut tables: Vec<(String, Vec<String>)> = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;
    for raw in source.lines() {
        let code = scan::strip_comment(raw).trim();
        if let Some(table) = create_table_name(code) {
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

fn create_table_name(code: &str) -> Option<String> {
    let rest = code.strip_prefix("create_table")?;
    if !rest.starts_with([' ', '\t', '(']) {
        return None;
    }
    first_string_literal(rest)
}

/// `t.*` helpers whose first string literal does NOT name a column.
const NON_COLUMN_HELPERS: &[&str] = &[
    "index",
    "foreign_key",
    "references",
    "belongs_to",
    "primary_key",
    "check_constraint",
    "timestamps",
];

fn column_name(code: &str) -> Option<String> {
    let rest = code.strip_prefix("t.")?;
    let (kind, after) = split_identifier(rest);
    if kind.is_empty() || NON_COLUMN_HELPERS.iter().any(|h| *h == kind) {
        return None;
    }
    first_string_literal(after)
}

fn split_identifier(s: &str) -> (String, &str) {
    let end = s
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map_or(s.len(), |(i, _)| i);
    (s[..end].to_string(), &s[end..])
}

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

    fn parse(source: &str) -> RubyClass {
        parse_class_via_ast(source).expect("source should parse into a RubyClass")
    }

    #[test]
    fn captures_class_name_and_superclass() {
        let c = parse("class WorkPackage < ApplicationRecord\nend\n");
        assert_eq!(c.name, "WorkPackage");
        assert_eq!(c.superclass.as_deref(), Some("ApplicationRecord"));
    }

    #[test]
    fn captures_namespaced_superclass() {
        let c = parse("class Foo < ActiveRecord::Base\nend\n");
        assert_eq!(c.superclass.as_deref(), Some("ActiveRecord::Base"));
    }

    #[test]
    fn captures_sti_subclass_parent() {
        // STI: parent is a model, not Record/Base. This is the case our
        // downstream consumers need to detect a hierarchy.
        let c = parse("class Group < Principal\nend\n");
        assert_eq!(c.superclass.as_deref(), Some("Principal"));
    }

    #[test]
    fn rejects_class_without_superclass() {
        // Matches C4 behaviour: a bare `class Foo` without an explicit
        // parent is not an ActiveRecord-style model and is skipped.
        assert!(parse_class_via_ast("class Foo\nend\n").is_none());
    }

    #[test]
    fn captures_plain_associations_same_as_c4() {
        let src = "class WorkPackage < ApplicationRecord\n\
                   belongs_to :project\n\
                   has_many :time_entries\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.associations, ["project", "time_entries"]);
        assert_eq!(c.association_options.len(), 2);
        assert_eq!(c.association_options[0].macro_name, "belongs_to");
        assert_eq!(c.association_options[0].name, "project");
        assert_eq!(c.association_options[1].macro_name, "has_many");
        assert_eq!(c.association_options[1].name, "time_entries");
    }

    #[test]
    fn captures_class_name_option() {
        // G1 (macro options) / G6 (`::`-namespaced class_name) — the C4
        // scanner saw `assigned_to` only; the new parser sees the target
        // type too.
        let src = "class WorkPackage < ApplicationRecord\n\
                   belongs_to :assigned_to, class_name: \"Principal\"\n\
                   belongs_to :file_link, class_name: \"Storages::FileLink\"\n\
                   end\n";
        let c = parse(src);
        assert_eq!(
            c.association_options[0].class_name.as_deref(),
            Some("Principal")
        );
        assert_eq!(
            c.association_options[1].class_name.as_deref(),
            Some("Storages::FileLink")
        );
    }

    #[test]
    fn captures_polymorphic_belongs_to() {
        // G2 — Journal-style polymorphic belongs_to is now visible.
        let src = "class Journal < ApplicationRecord\n\
                   belongs_to :journable, polymorphic: true\n\
                   belongs_to :user\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.association_options[0].polymorphic, Some(true));
        // A non-polymorphic belongs_to leaves the flag unset (None) — not
        // the same as `polymorphic: false`.
        assert_eq!(c.association_options[1].polymorphic, None);
    }

    #[test]
    fn captures_reverse_side_polymorphic_as() {
        // G3 — `has_many :time_entries, as: :entity` makes the OTHER side's
        // belongs_to polymorphic on `:entity_type`/`:entity_id`.
        let src = "class WorkPackage < ApplicationRecord\n\
                   has_many :time_entries, dependent: :delete_all, as: :entity\n\
                   end\n";
        let c = parse(src);
        let opts = &c.association_options[0];
        assert_eq!(opts.as_target.as_deref(), Some("entity"));
        assert_eq!(opts.dependent.as_deref(), Some("delete_all"));
    }

    #[test]
    fn captures_through_and_source() {
        // G4 + G5 — `has_many :users, through: :members, source: :principal`
        // is the canonical Project.users → Member.principal join. Both
        // options must be captured to reconstruct the join semantics.
        let src = "class Project < ApplicationRecord\n\
                   has_many :members\n\
                   has_many :users, through: :members, source: :principal\n\
                   end\n";
        let c = parse(src);
        let users = c
            .association_options
            .iter()
            .find(|a| a.name == "users")
            .expect("users assoc captured");
        assert_eq!(users.through.as_deref(), Some("members"));
        assert_eq!(users.source.as_deref(), Some("principal"));
    }

    #[test]
    fn captures_optional_inverse_of_and_foreign_key() {
        let src = "class WorkPackage < ApplicationRecord\n\
                   belongs_to :assigned_to, class_name: \"Principal\", \
                   optional: true, foreign_key: \"assigned_to_id\", \
                   inverse_of: :work_packages\n\
                   end\n";
        let c = parse(src);
        let a = &c.association_options[0];
        assert_eq!(a.optional, Some(true));
        assert_eq!(a.foreign_key.as_deref(), Some("assigned_to_id"));
        assert_eq!(a.inverse_of.as_deref(), Some("work_packages"));
    }

    #[test]
    fn ignores_options_block_scope_before_hash() {
        // `has_many :x, -> { … }, dependent: :destroy` — the lambda is
        // arg[1], the hash is arg[2]. We must still find the options.
        let src = "class Project < ApplicationRecord\n\
                   has_many :work_packages, -> { order(:created_at) }, dependent: :destroy\n\
                   end\n";
        let c = parse(src);
        let a = &c.association_options[0];
        assert_eq!(a.name, "work_packages");
        assert_eq!(a.dependent.as_deref(), Some("destroy"));
    }

    #[test]
    fn body_source_preserves_extractor_input() {
        // `fields.rs` / `functions.rs` consume body_source as a string and
        // expect the class definition's open + closing `end` stripped. This
        // shape must match what the C4 parse_class produced.
        let src = "class WorkPackage < ApplicationRecord\n\
                   belongs_to :project\n\
                   has_many :time_entries\n\
                   \n\
                   def compute_total_hours\n\
                   raise ActiveRecord::RecordInvalid unless status\n\
                   @total_hours ||= time_entries.hours\n\
                   end\n\
                   end\n";
        let c = parse(src);
        assert!(c.body_source.contains("belongs_to :project"));
        assert!(c.body_source.contains("def compute_total_hours"));
        assert!(c.body_source.contains("@total_hours"));
        // The opening `class …` line is NOT in body_source.
        assert!(!c.body_source.contains("class WorkPackage <"));
    }

    #[test]
    fn column_name_accepts_scalar_columns() {
        assert_eq!(
            column_name(r#"t.string "subject", null: false"#).as_deref(),
            Some("subject")
        );
        assert_eq!(
            column_name(r#"t.integer "status_id""#).as_deref(),
            Some("status_id")
        );
    }

    #[test]
    fn column_name_skips_non_column_helpers() {
        assert_eq!(
            column_name(r#"t.index ["work_package_id"], name: "idx_wp_id""#),
            None
        );
        assert_eq!(
            column_name(r#"t.foreign_key "users", column: "author_id""#),
            None
        );
        assert_eq!(column_name(r#"t.timestamps null: false"#), None);
    }
}
