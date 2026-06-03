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
use crate::{
    ASSOCIATION_MACROS, AssociationDecl, AttributeDecl, CallbackDecl, EnumDecl, RubyClass,
    ScopeDecl, StoreAccessorDecl,
};

/// ActiveRecord lifecycle callback event names. A class-body call whose
/// `method_name` matches one of these and whose first arg is either a Sym
/// (method-target form) or whose enclosing statement is a Block (do/end
/// form) is treated as a [`CallbackDecl`]. Closes gap-probe G19 source-side.
const CALLBACK_EVENTS: &[&str] = &[
    "before_save",
    "after_save",
    "around_save",
    "before_create",
    "after_create",
    "around_create",
    "before_update",
    "after_update",
    "around_update",
    "before_destroy",
    "after_destroy",
    "around_destroy",
    "before_validation",
    "after_validation",
    "after_initialize",
    "after_find",
    "after_touch",
    "after_commit",
    "after_create_commit",
    "after_update_commit",
    "after_save_commit",
    "after_destroy_commit",
    "after_rollback",
];

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

    let mut class = RubyClass {
        name,
        body_source,
        superclass,
        ..Default::default()
    };

    // Single pass over the class body. Each statement is one of:
    //   - Send (unqualified or self-qualified — most macros)
    //   - Block (wraps a Send when the macro call ends in `do ... end` or
    //     `{ ... }`, e.g. `after_create do ... end`)
    //   - OpAsgn (operator assignment, used for `self.ignored_columns += …`)
    // Each is dispatched to the right handler. Adding a new construct is
    // a one-arm extension to one of the three handlers.
    for stmt in top_level_statements(&class_node.body) {
        match stmt {
            Node::Send(send) => {
                if send.recv.is_none() {
                    handle_unqualified_send(send, None, source, &mut class);
                } else if matches!(send.recv.as_deref(), Some(Node::Self_(_))) {
                    handle_self_send(send, &mut class);
                }
            }
            Node::Block(block) => {
                // A `Block` wraps a method call with a `{...}` / `do...end`
                // body. We forward to the same handler the bare-Send route
                // uses, plus the block reference for body-source extraction.
                if let Node::Send(send) = &*block.call {
                    if send.recv.is_none() {
                        handle_unqualified_send(send, Some(block), source, &mut class);
                    }
                }
            }
            Node::OpAsgn(op) => handle_op_assign(op, &mut class),
            _ => {}
        }
    }

    Some(class)
}

/// Dispatch a `Foo.bar(...)`-style class-body call (no receiver, i.e. a
/// macro on the class itself) to the right C17a/b/c extractor.
///
/// `block` is `Some` when the call has a `do ... end` or `{ ... }` block
/// attached at the statement level (so the dispatching `match` arm is the
/// `Block` arm in the parent loop). The block reference is needed for
/// callbacks and the `scope` / `default_scope` body extraction.
fn handle_unqualified_send(
    send: &nodes::Send,
    block: Option<&nodes::Block>,
    source: &str,
    class: &mut RubyClass,
) {
    let name = send.method_name.as_str();
    if ASSOCIATION_MACROS.iter().any(|m| *m == name) {
        if let Some(decl) = parse_association_send(send) {
            class.associations.push(decl.name.clone());
            class.association_options.push(decl);
        }
    } else if name == "include" {
        if let Some(c) = send.args.first().and_then(const_name) {
            class.concerns.push(c);
        }
    } else if name == "enum" {
        if let Some(e) = parse_enum_send(send) {
            class.enums.push(e);
        }
    } else if name == "store_accessor" {
        if let Some(s) = parse_store_accessor_send(send) {
            class.store_accessors.push(s);
        }
    } else if name == "attribute" {
        if let Some(a) = parse_attribute_send(send) {
            class.attributes.push(a);
        }
    } else if name == "scope" {
        if let Some(s) = parse_scope_send(send, source) {
            class.scope_definitions.push(s);
        }
    } else if name == "scopes" {
        for arg in &send.args {
            if let Some(n) = sym_name(arg) {
                class.scope_predeclarations.push(n);
            }
        }
    } else if name == "default_scope" {
        if let Some(body) = parse_default_scope_send(send, source) {
            class.default_scope_body = Some(body);
        }
    } else if CALLBACK_EVENTS.iter().any(|e| *e == name) {
        class.callbacks.push(parse_callback(send, block, source));
    }
}

/// Dispatch a `self.foo = bar`-style class-body assignment. Rails uses
/// these for class-level meta directives — `self.table_name = …`, and
/// `self.inheritance_column = :_type_disabled` to opt OUT of STI dispatch.
fn handle_self_send(send: &nodes::Send, class: &mut RubyClass) {
    match send.method_name.as_str() {
        "table_name=" => {
            if let Some(arg) = send.args.first() {
                // Only literal-string overrides land here. Dynamic
                // (`"#{prefix}users"`) leaves it unset — the body_source
                // still preserves the source line for a future
                // interpolation-aware pass to consume.
                class.table_name_override = str_value(arg);
            }
        }
        "inheritance_column=" => {
            // OpenProject's `View < ApplicationRecord; self.inheritance_column
            // = :_type_disabled` opts out of STI. We treat any `_type_disabled`
            // value as "disabled"; other strings (renaming the column) don't
            // need a flag — STI dispatch still happens, just on a different
            // column. Capturing that would be a follow-up.
            if send.args.first().and_then(sym_name).as_deref() == Some("_type_disabled") {
                class.inheritance_column_disabled = true;
            }
        }
        _ => {}
    }
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
///
/// Leading `::` (root scope, encoded as `Node::Cbase`) is stripped: the
/// fully-qualified form `::Scopes::Scoped` and the relatively-qualified
/// `Scopes::Scoped` both produce `"Scopes::Scoped"`. The downstream
/// graph identity is the constant path, not the lookup-strategy bit.
fn const_name(node: &Node) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = node;
    loop {
        let Node::Const(c) = cur else { return None };
        parts.insert(0, c.name.clone());
        match c.scope.as_deref() {
            Some(Node::Cbase(_)) | None => break,
            Some(scope) => cur = scope,
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
        // C17c: collection callbacks (G20). Only the method-symbol form
        // is captured (`after_remove: :method_name`); a lambda value would
        // need a separate code path.
        "before_add" => decl.before_add = sym_or_str(value),
        "after_add" => decl.after_add = sym_or_str(value),
        "before_remove" => decl.before_remove = sym_or_str(value),
        "after_remove" => decl.after_remove = sym_or_str(value),
        _ => {}
    }
}

/// Parse an `enum :column, { variant: value, … }, scopes: false` call.
///
/// The values dict is positional (`args[1]`), separate from the kwargs
/// hash with `scopes:` etc. — see RUBY-FRONTEND.md. Variant values can be
/// ints (`status: { active: 1, … }`) or strings (`workspace_type:
/// { project: "project", … }`); both are captured as their literal source
/// representation.
fn parse_enum_send(send: &nodes::Send) -> Option<EnumDecl> {
    let column = sym_name(send.args.first()?)?;

    let mut values: Vec<(String, String)> = Vec::new();
    let mut scopes_disabled: Option<bool> = None;

    for (i, arg) in send.args.iter().enumerate().skip(1) {
        match arg {
            // Positional values Hash — typically args[1]. Values come as a
            // Hash (braced) at this position rather than Kwargs because the
            // braces ARE part of the source ({a: 1, b: 2}).
            Node::Hash(h) if i == 1 => {
                for pair in &h.pairs {
                    if let Node::Pair(p) = pair {
                        let Some(k) = sym_name(&p.key) else { continue };
                        let Some(v) = literal_str_repr(&p.value) else { continue };
                        values.push((k, v));
                    }
                }
            }
            // Options come as Kwargs (trailing keyword args) or, if the
            // call is written with explicit braces, as a Hash at i > 1.
            Node::Kwargs(k) => {
                apply_enum_options(&k.pairs, &mut scopes_disabled);
            }
            Node::Hash(h) if i > 1 => {
                apply_enum_options(&h.pairs, &mut scopes_disabled);
            }
            _ => {}
        }
    }

    if values.is_empty() {
        return None;
    }
    Some(EnumDecl {
        column,
        values,
        scopes_disabled,
    })
}

/// Read the `scopes:` option out of a hash/kwargs pair list.
/// `scopes: false` → `Some(true)` (i.e., disabled); `scopes: true` →
/// `Some(false)` (enabled, explicit). Unset stays `None`.
fn apply_enum_options(pairs: &[Node], scopes_disabled: &mut Option<bool>) {
    for pair in pairs {
        if let Node::Pair(p) = pair {
            if sym_name(&p.key).as_deref() == Some("scopes") {
                if let Some(v) = bool_value(&p.value) {
                    *scopes_disabled = Some(!v);
                }
            }
        }
    }
}

/// Parse a `store_accessor :col, %i[a b c], prefix: true` call.
///
/// `%i[…]` parses to `Node::Array` whose elements are `Sym` nodes. The
/// first Array arg is the field list; any subsequent Kwargs/Hash arg
/// carries the `prefix:` option.
fn parse_store_accessor_send(send: &nodes::Send) -> Option<StoreAccessorDecl> {
    let column = sym_name(send.args.first()?)?;

    let mut fields: Vec<String> = Vec::new();
    let mut prefix: Option<bool> = None;

    for arg in send.args.iter().skip(1) {
        match arg {
            Node::Array(a) if fields.is_empty() => {
                for elem in &a.elements {
                    if let Some(s) = sym_name(elem) {
                        fields.push(s);
                    }
                }
            }
            Node::Kwargs(k) => apply_store_accessor_options(&k.pairs, &mut prefix),
            Node::Hash(h) => apply_store_accessor_options(&h.pairs, &mut prefix),
            _ => {}
        }
    }

    if fields.is_empty() {
        return None;
    }
    Some(StoreAccessorDecl {
        column,
        fields,
        prefix,
    })
}

fn apply_store_accessor_options(pairs: &[Node], prefix: &mut Option<bool>) {
    for pair in pairs {
        if let Node::Pair(p) = pair {
            if sym_name(&p.key).as_deref() == Some("prefix") {
                *prefix = bool_value(&p.value);
            }
        }
    }
}

/// Parse an `attribute :name, :type, default: …` call. `default:` is not
/// captured here — it's a value that can be anything (literal, lambda,
/// constant), and the existing `body_source` traversal can recover the
/// source if a future consumer needs it. Capturing it as a typed value
/// is its own design choice and out of scope for this gap-closure pass.
fn parse_attribute_send(send: &nodes::Send) -> Option<AttributeDecl> {
    let name = sym_name(send.args.first()?)?;
    let type_name = send.args.get(1).and_then(sym_name);
    Some(AttributeDecl { name, type_name })
}

/// Parse a `scope :name, -> { body }` call into [`ScopeDecl`].
///
/// The lambda lives in `args[1]` as a `Block` node (wrapping the synthetic
/// `Lambda` call). The body source is sliced from the original source via
/// the Block's `begin_l.end` / `end_l.begin` byte offsets, trimming the
/// `{`/`}` (or `do`/`end`) delimiters themselves.
fn parse_scope_send(send: &nodes::Send, source: &str) -> Option<ScopeDecl> {
    let name = sym_name(send.args.first()?)?;
    let block_arg = send.args.get(1).and_then(|n| match n {
        Node::Block(b) => Some(b),
        _ => None,
    })?;
    let body_source = block_body_text(source, block_arg).to_string();
    Some(ScopeDecl { name, body_source })
}

/// Parse a `default_scope -> { body }` call. The body source is captured
/// the same way as a named scope; downstream consumers know they have a
/// global filter rather than a named one because it lives on
/// `default_scope_body`, not `scope_definitions`.
fn parse_default_scope_send(send: &nodes::Send, source: &str) -> Option<String> {
    let block_arg = send.args.first().and_then(|n| match n {
        Node::Block(b) => Some(b),
        _ => None,
    })?;
    Some(block_body_text(source, block_arg).to_string())
}

/// Build a [`CallbackDecl`] from a lifecycle-event Send + optional Block.
///
/// Two forms are normalised:
/// - `event :method_name` — `send.args[0]` is a Sym, `block` is `None`;
///   `target_method = Some(name)`, `body_source = None`.
/// - `event do ... end` / `event { ... }` — `send.args` is empty, `block`
///   is `Some(b)`; `target_method = None`, `body_source = Some(text)`.
///
/// A call with NEITHER a Sym arg NOR a block (`before_save` bare, no body)
/// still produces a CallbackDecl with both fields `None` — it's still a
/// declared callback, just one whose target the source doesn't pin down.
fn parse_callback(send: &nodes::Send, block: Option<&nodes::Block>, source: &str) -> CallbackDecl {
    let target_method = send.args.first().and_then(sym_name);
    let body_source = block.map(|b| block_body_text(source, b).to_string());
    CallbackDecl {
        event: send.method_name.clone(),
        target_method,
        body_source,
    }
}

/// Slice the body text out of a `Block` node, trimming the `{`/`}` (or
/// `do`/`end`) delimiter bytes. lib-ruby-parser's `begin_l` covers the
/// opening delimiter and `end_l` covers the closing one; the body lives
/// between them.
fn block_body_text<'a>(source: &'a str, block: &nodes::Block) -> &'a str {
    let start = block.begin_l.end;
    let end = block.end_l.begin;
    if start >= end || end > source.len() {
        return "";
    }
    source[start..end].trim_matches(|c: char| c.is_whitespace() || c == ';')
}

/// Handle a class-body `OpAsgn` (operator assignment). The C17c case is
/// `self.ignored_columns += [...]` — append the columns to the running
/// ignored-columns list. Other operator assignments (e.g. `self.foo *= 2`)
/// are silently ignored — they don't carry semantic weight for the model
/// graph at the moment.
fn handle_op_assign(op: &nodes::OpAsgn, class: &mut RubyClass) {
    // The lhs of `self.ignored_columns += [..]` is a Send with recv = self
    // and method_name = "ignored_columns" (the GETTER name, not the
    // assignment form — that's how Ruby compiles `+=`).
    let Node::Send(lhs) = &*op.recv else { return };
    if !matches!(lhs.recv.as_deref(), Some(Node::Self_(_))) {
        return;
    }
    if lhs.method_name != "ignored_columns" {
        return;
    }
    if op.operator != "+" {
        return; // Only `+=` extends the blacklist; `-=` is rare and we'd want a different field.
    }
    if let Node::Array(arr) = &*op.value {
        for elem in &arr.elements {
            if let Some(s) = str_value(elem) {
                class.ignored_columns.push(s);
            }
        }
    }
}

/// Stringify a literal value node into its source form. Used by enum
/// value-dict capture so both `1` (int-backed) and `"active"` (string-
/// backed) variants land as `String`s the consumer can `parse::<i64>()`
/// or read as-is.
fn literal_str_repr(node: &Node) -> Option<String> {
    match node {
        Node::Int(i) => Some(i.value.clone()),
        Node::Str(s) => Some(bytes_to_string(&s.value)),
        Node::Sym(s) => Some(bytes_to_string(&s.name)),
        Node::True(_) => Some("true".to_string()),
        Node::False(_) => Some("false".to_string()),
        Node::Nil(_) => Some("nil".to_string()),
        _ => None,
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

    // -----------------------------------------------------------------
    // C17b — concerns, enums, store_accessors, attributes, class-meta
    // -----------------------------------------------------------------

    #[test]
    fn captures_concerns_in_source_order_with_namespacing() {
        // G14 — `include` chains were invisible to the C4 scanner. The
        // 15 concerns on WorkPackage (mini-fixture here) are now captured
        // with their full path; `::`-prefixed paths preserved.
        let src = "class WorkPackage < ApplicationRecord\n\
                   include WorkPackage::SemanticIdentifier\n\
                   include ::Scopes::Scoped\n\
                   include HasMembers\n\
                   include OpenProject::Journal::AttachmentHelper\n\
                   end\n";
        let c = parse(src);
        assert_eq!(
            c.concerns,
            [
                "WorkPackage::SemanticIdentifier",
                "Scopes::Scoped",
                "HasMembers",
                "OpenProject::Journal::AttachmentHelper",
            ]
        );
    }

    #[test]
    fn captures_int_backed_enum_with_scopes_disabled() {
        // G8 — Principal-style enum. `scopes: false` is captured as
        // scopes_disabled = Some(true); the 5 status variants are
        // captured in source order with their int values stringified.
        let src = "class Principal < ApplicationRecord\n\
                   enum :status, { active: 1, registered: 2, locked: 3, \
                   invited: 4, deleted: 5 }, scopes: false\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.enums.len(), 1);
        let e = &c.enums[0];
        assert_eq!(e.column, "status");
        assert_eq!(
            e.values,
            [
                ("active".to_string(), "1".to_string()),
                ("registered".to_string(), "2".to_string()),
                ("locked".to_string(), "3".to_string()),
                ("invited".to_string(), "4".to_string()),
                ("deleted".to_string(), "5".to_string()),
            ]
        );
        assert_eq!(e.scopes_disabled, Some(true));
    }

    #[test]
    fn captures_string_backed_enum() {
        // Project's workspace_type enum is string-backed
        // ({project: "project", program: "program", portfolio: "portfolio"}).
        let src = "class Project < ApplicationRecord\n\
                   enum :workspace_type, { project: \"project\", program: \"program\", \
                   portfolio: \"portfolio\" }, validate: true\n\
                   end\n";
        let c = parse(src);
        let e = &c.enums[0];
        assert_eq!(e.column, "workspace_type");
        assert_eq!(
            e.values,
            [
                ("project".to_string(), "project".to_string()),
                ("program".to_string(), "program".to_string()),
                ("portfolio".to_string(), "portfolio".to_string()),
            ]
        );
        // `validate: true` doesn't drive `scopes_disabled`; left unset.
        assert_eq!(e.scopes_disabled, None);
    }

    #[test]
    fn captures_store_accessor_with_prefix() {
        // G9 — Journal's `cause` store_accessor with 8 fields and
        // prefix: true. The 8 pseudo-fields are now visible at parse
        // time; consumers can synthesize cause_type / cause_feature / …
        // baseline Fields.
        let src = "class Journal < ApplicationRecord\n\
                   store_accessor :cause, %i[type feature import_history \
                   work_package_id changed_days status_name status_id \
                   status_changes], prefix: true\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.store_accessors.len(), 1);
        let s = &c.store_accessors[0];
        assert_eq!(s.column, "cause");
        assert_eq!(
            s.fields,
            [
                "type",
                "feature",
                "import_history",
                "work_package_id",
                "changed_days",
                "status_name",
                "status_id",
                "status_changes",
            ]
        );
        assert_eq!(s.prefix, Some(true));
    }

    #[test]
    fn captures_typed_attribute() {
        // G10 — `attribute :foo, :type` declarations. Type is captured;
        // default value left to body_source (see parse_attribute_send
        // docs for the rationale).
        let src = "class Foo < ApplicationRecord\n\
                   attribute :display_id, :string\n\
                   attribute :total_hours, :decimal, default: 0\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.attributes.len(), 2);
        assert_eq!(c.attributes[0].name, "display_id");
        assert_eq!(c.attributes[0].type_name.as_deref(), Some("string"));
        assert_eq!(c.attributes[1].name, "total_hours");
        assert_eq!(c.attributes[1].type_name.as_deref(), Some("decimal"));
    }

    #[test]
    fn captures_literal_table_name_override() {
        // G11 (partial) — Journal's literal `self.table_name = "journals"`.
        // The dynamic Principal form `"#{prefix}users#{suffix}"` is NOT
        // captured (intentional under-extraction).
        let src = "class Journal < ApplicationRecord\n\
                   self.table_name = \"journals\"\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.table_name_override.as_deref(), Some("journals"));
    }

    #[test]
    fn ignores_dynamic_table_name() {
        // Principal-style runtime expression. Extractor leaves the field
        // unset rather than emit a misleading partial string.
        let src = "class Principal < ApplicationRecord\n\
                   self.table_name = \"#{table_name_prefix}users#{table_name_suffix}\"\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.table_name_override, None);
    }

    #[test]
    fn captures_inheritance_column_disabled() {
        // G12 — View opts out of STI. Without this signal, downstream
        // could falsely assume `class X < View` files form an STI
        // hierarchy.
        let src = "class View < ApplicationRecord\n\
                   self.inheritance_column = :_type_disabled\n\
                   belongs_to :query\n\
                   end\n";
        let c = parse(src);
        assert!(c.inheritance_column_disabled);
        // Renaming the column (rare but legal) does NOT set the flag —
        // STI dispatch still happens, just on a different column.
        let src2 = "class Foo < ApplicationRecord\n\
                    self.inheritance_column = :kind\n\
                    end\n";
        let c2 = parse(src2);
        assert!(!c2.inheritance_column_disabled);
    }

    #[test]
    fn empty_class_body_leaves_all_lists_empty() {
        // Determinism + default sanity check: no surprises in the
        // optional fields when the class is bare.
        let src = "class Foo < ApplicationRecord\nend\n";
        let c = parse(src);
        assert!(c.concerns.is_empty());
        assert!(c.enums.is_empty());
        assert!(c.store_accessors.is_empty());
        assert!(c.attributes.is_empty());
        assert!(c.table_name_override.is_none());
        assert!(!c.inheritance_column_disabled);
    }

    // -----------------------------------------------------------------
    // C17c — ignored_columns, scopes, default_scope, callbacks,
    //        collection callbacks (G13, G15, G16, G17, G19, G20)
    // -----------------------------------------------------------------

    #[test]
    fn captures_ignored_columns_via_op_assign() {
        // G13 — Journal's `self.ignored_columns += ["activity_type"]`.
        // Multiple `+=` statements stack into the same list.
        let src = "class Journal < ApplicationRecord\n\
                   self.ignored_columns += [\"activity_type\"]\n\
                   self.ignored_columns += [\"deprecated_col\", \"other\"]\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.ignored_columns, ["activity_type", "deprecated_col", "other"]);
    }

    #[test]
    fn captures_scope_definition_with_body() {
        // G15 — `scope :name, -> { body }`. Body source is verbatim from
        // the original Ruby; consumers can re-parse if they need it as a
        // SQL/Ruby AST.
        let src = "class WorkPackage < ApplicationRecord\n\
                   scope :recently_updated, -> { order(updated_at: :desc) }\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.scope_definitions.len(), 1);
        let s = &c.scope_definitions[0];
        assert_eq!(s.name, "recently_updated");
        assert!(s.body_source.contains("order(updated_at: :desc)"));
        // Body source is trimmed of the `{`/`}` delimiters.
        assert!(!s.body_source.starts_with('{'));
        assert!(!s.body_source.ends_with('}'));
    }

    #[test]
    fn captures_multiple_scope_definitions_in_source_order() {
        let src = "class Project < ApplicationRecord\n\
                   scope :active, -> { where(active: true) }\n\
                   scope :for_user, ->(u) { joins(:members).where(members: { user_id: u.id }) }\n\
                   scope :recent, -> { order(created_at: :desc) }\n\
                   end\n";
        let c = parse(src);
        let names: Vec<&str> = c.scope_definitions.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["active", "for_user", "recent"]);
        // The `for_user` scope retains its arg-using body — we don't parse
        // the lambda arity separately but `u` and `u.id` are in the source.
        let for_user = &c.scope_definitions[1];
        assert!(for_user.body_source.contains("u.id"));
    }

    #[test]
    fn captures_scopes_predeclaration_list() {
        // G16 — Principal-style `scopes :like, :human, :visible`. The
        // bodies live in the corresponding concern; only the names are
        // declared at the class-body level.
        let src = "class Principal < ApplicationRecord\n\
                   scopes :like, :human, :visible, :not_builtin, :status\n\
                   end\n";
        let c = parse(src);
        assert_eq!(
            c.scope_predeclarations,
            ["like", "human", "visible", "not_builtin", "status"]
        );
        // Pre-declarations are NOT in scope_definitions (no body here).
        assert!(c.scope_definitions.is_empty());
    }

    #[test]
    fn captures_default_scope_body() {
        // G17 — Principal's `default_scope -> { where.not(status: …) }`.
        // The body source is the verbatim filter expression.
        let src = "class Principal < ApplicationRecord\n\
                   default_scope -> { where.not(status: 5) }\n\
                   end\n";
        let c = parse(src);
        assert_eq!(
            c.default_scope_body.as_deref(),
            Some("where.not(status: 5)")
        );
    }

    #[test]
    fn captures_method_target_callback() {
        // G19 — `before_save :update_total` form. target_method = method
        // name, body_source = None.
        let src = "class WorkPackage < ApplicationRecord\n\
                   before_save :update_total\n\
                   after_create :send_notification\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.callbacks.len(), 2);
        assert_eq!(c.callbacks[0].event, "before_save");
        assert_eq!(c.callbacks[0].target_method.as_deref(), Some("update_total"));
        assert!(c.callbacks[0].body_source.is_none());
        assert_eq!(c.callbacks[1].event, "after_create");
        assert_eq!(
            c.callbacks[1].target_method.as_deref(),
            Some("send_notification")
        );
    }

    #[test]
    fn captures_block_form_callback() {
        // G19 — `after_create do ... end` form. target_method = None,
        // body_source = the block body text.
        let src = "class WorkPackage < ApplicationRecord\n\
                   after_create do\n\
                     notify(:created)\n\
                     log_activity\n\
                   end\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.callbacks.len(), 1);
        assert_eq!(c.callbacks[0].event, "after_create");
        assert_eq!(c.callbacks[0].target_method, None);
        let body = c.callbacks[0]
            .body_source
            .as_deref()
            .expect("block form has body source");
        assert!(body.contains("notify(:created)"));
        assert!(body.contains("log_activity"));
    }

    #[test]
    fn captures_around_and_commit_callbacks() {
        // Confirms the full callback event list covers around_* and the
        // commit-suffixed variants (`after_create_commit` etc.).
        let src = "class Project < ApplicationRecord\n\
                   around_destroy :archive_then_destroy\n\
                   after_create_commit :index_for_search\n\
                   after_rollback :clear_cache\n\
                   end\n";
        let c = parse(src);
        let events: Vec<&str> = c.callbacks.iter().map(|cb| cb.event.as_str()).collect();
        assert_eq!(
            events,
            ["around_destroy", "after_create_commit", "after_rollback"]
        );
    }

    #[test]
    fn captures_collection_callbacks_on_association() {
        // G20 — `has_many :enabled_modules, after_remove: :module_disabled`
        // is Project's real OP example. Other 3 collection callbacks
        // (before_add, after_add, before_remove) captured the same way.
        let src = "class Project < ApplicationRecord\n\
                   has_many :enabled_modules, dependent: :delete_all, after_remove: :module_disabled\n\
                   has_many :members, before_add: :before_add_member, after_add: :after_add_member\n\
                   end\n";
        let c = parse(src);
        let modules = c
            .association_options
            .iter()
            .find(|a| a.name == "enabled_modules")
            .unwrap();
        assert_eq!(modules.after_remove.as_deref(), Some("module_disabled"));
        // The other 3 stay None for this assoc.
        assert!(modules.before_add.is_none());
        let members = c
            .association_options
            .iter()
            .find(|a| a.name == "members")
            .unwrap();
        assert_eq!(members.before_add.as_deref(), Some("before_add_member"));
        assert_eq!(members.after_add.as_deref(), Some("after_add_member"));
    }

    #[test]
    fn mixed_body_captures_each_kind_in_one_pass() {
        // Smoke test: a single class body with one of each construct,
        // all extracted in a single AST walk. Catches any cross-kind
        // dispatch regression (a Send mis-routed to the wrong handler).
        let src = "class Mixed < ApplicationRecord\n\
                   include ::Scopes::Scoped\n\
                   self.table_name = \"mixed_table\"\n\
                   self.inheritance_column = :_type_disabled\n\
                   belongs_to :owner, class_name: \"Principal\"\n\
                   enum :state, { open: 0, closed: 1 }\n\
                   store_accessor :meta, %i[note], prefix: false\n\
                   attribute :virtual_field, :string\n\
                   end\n";
        let c = parse(src);
        assert_eq!(c.concerns, ["Scopes::Scoped"]);
        assert_eq!(c.table_name_override.as_deref(), Some("mixed_table"));
        assert!(c.inheritance_column_disabled);
        assert_eq!(c.associations, ["owner"]);
        assert_eq!(
            c.association_options[0].class_name.as_deref(),
            Some("Principal")
        );
        assert_eq!(c.enums.len(), 1);
        assert_eq!(c.enums[0].column, "state");
        assert_eq!(c.store_accessors.len(), 1);
        assert_eq!(c.store_accessors[0].prefix, Some(false));
        assert_eq!(c.attributes.len(), 1);
        assert_eq!(c.attributes[0].name, "virtual_field");
    }
}
