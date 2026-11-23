//! The default-scope lowering (`tests/gen/scopes.rs`) on hand-written Ruby.
//!
//! The PostgreSQL oracle reads the same lowered data as the desk, so it cannot
//! see a lowering error; these cases pin the lowering itself, including the
//! shapes no OpenProject slice model has yet.

mod gen;

use std::path::{Path, PathBuf};

use gen::scopes::{lower, Lowered, ModelInput};
use rails_compiler::harvest::Harvest;
use rails_compiler::semantic::Assembler;

/// Writes `files` (path under app/models → Ruby) into a fresh directory and
/// lowers the default scopes of `model` on `table` with `columns`.
fn lowered(
    name: &str,
    files: &[(&str, &str)],
    model: &str,
    table: &str,
    columns: &[&str],
) -> Lowered {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("scope-lowering-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, src) in files {
        let path = dir.join("app/models").join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, src).expect("write");
    }
    let h = Harvest::collect(&dir, &["app/models".into()], "fixture");
    let fw = rails_polyfill::rules::FrameworkDefaults::for_version(Some("6.1"));
    let asm = Assembler::new(
        &h,
        fw,
        vec!["ApplicationRecord".into(), "ActiveRecord::Base".into()],
    );
    let mut chain = vec![model.to_string()];
    while let Some(sup) = chain.last().and_then(|c| asm.superclass(c)) {
        if chain.contains(&sup) || !h.units.contains_key(&sup) {
            break;
        }
        chain.push(sup);
    }
    let input = ModelInput {
        model: model.into(),
        table: table.into(),
        chain,
        columns: columns.iter().map(|c| (*c).to_string()).collect(),
    };
    let mut out = lower(&h, &[input]);
    assert_eq!(
        out.len(),
        1,
        "{model} has default scopes: {}",
        dir_list(&dir)
    );
    out.remove(0)
}

fn dir_list(dir: &Path) -> String {
    dir.display().to_string()
}

fn order(l: &Lowered) -> Vec<(&str, bool)> {
    l.order.iter().map(|(c, a)| (c.as_str(), *a)).collect()
}

#[test]
fn two_scopes_in_one_class_fold_in_declaration_order() {
    // Line 9 before line 34: compared as text, "x.rb:34" < "x.rb:9".
    let mut src = String::from("class Thing < ApplicationRecord\n");
    for _ in 0..7 {
        src.push_str("  # filler\n");
    }
    src.push_str("  default_scope { order(:a) }\n");
    for _ in 0..24 {
        src.push_str("  # filler\n");
    }
    src.push_str("  default_scope { order(b: :desc) }\nend\n");
    let l = lowered(
        "decl-order",
        &[("thing.rb", &src)],
        "Thing",
        "things",
        &["id", "a", "b"],
    );
    assert!(l.unlowered.is_empty(), "{:?}", l.unlowered);
    assert_eq!(order(&l), [("a", true), ("b", false)]);
}

#[test]
fn a_superclass_scope_folds_before_the_subclass_scope() {
    let l = lowered(
        "superclass",
        &[
            ("base_thing.rb", "class BaseThing < ApplicationRecord\n  default_scope { order(:a) }\nend\n"),
            ("thing.rb", "class Thing < BaseThing\n  default_scope { where.not(state: 5) }\n  default_scope { order(\"things.b DESC\") }\nend\n"),
        ],
        "Thing",
        "things",
        &["id", "a", "b", "state"],
    );
    assert!(l.unlowered.is_empty(), "{:?}", l.unlowered);
    assert_eq!(order(&l), [("a", true), ("b", false)]);
    assert_eq!(
        l.filters,
        [("state".to_string(), true, "Lit::Int(5)".to_string())]
    );
}

#[test]
fn a_default_scope_from_an_included_module_is_not_lowered() {
    let l = lowered(
        "module",
        &[
            ("concerns/sorted.rb", "module Sorted\n  extend ActiveSupport::Concern\n  included do\n    default_scope { order(:a) }\n  end\nend\n"),
            ("thing.rb", "class Thing < ApplicationRecord\n  include Sorted\n  default_scope { order(:b) }\nend\n"),
        ],
        "Thing",
        "things",
        &["id", "a", "b"],
    );
    assert!(
        l.unlowered
            .iter()
            .any(|(_, why)| why.contains("included module Sorted")),
        "{:?}",
        l.unlowered
    );
}

#[test]
fn a_class_method_is_inlined_but_a_named_scope_shadows_it() {
    let inlined = lowered(
        "classmethod",
        &[("thing.rb", "class Thing < ApplicationRecord\n  default_scope { by_pos }\n  def self.by_pos\n    order(:position)\n  end\nend\n")],
        "Thing",
        "things",
        &["id", "position"],
    );
    assert!(inlined.unlowered.is_empty(), "{:?}", inlined.unlowered);
    assert_eq!(order(&inlined), [("position", true)]);

    // The subclass's `scope :by_pos` shadows the superclass's `def self.by_pos`.
    let shadowed = lowered(
        "shadowed",
        &[
            ("base_thing.rb", "class BaseThing < ApplicationRecord\n  def self.by_pos\n    order(:position)\n  end\nend\n"),
            ("thing.rb", "class Thing < BaseThing\n  scope :by_pos, -> { order(:name) }\n  default_scope { by_pos }\nend\n"),
        ],
        "Thing",
        "things",
        &["id", "position", "name"],
    );
    assert!(shadowed.order.is_empty(), "{:?}", shadowed.order);
    assert!(!shadowed.unlowered.is_empty());
}

#[test]
fn an_enum_value_resolves_only_against_the_models_own_enum() {
    let own = lowered(
        "enum-own",
        &[("thing.rb", "class Thing < ApplicationRecord\n  enum :status, { active: 1, deleted: 5 }\n  default_scope -> { where.not(status: Thing.statuses[:deleted]) }\nend\n")],
        "Thing",
        "things",
        &["id", "status"],
    );
    assert_eq!(
        own.filters,
        [("status".to_string(), true, "Lit::Int(5)".to_string())]
    );

    let other = lowered(
        "enum-other",
        &[("thing.rb", "class Thing < ApplicationRecord\n  enum :status, { active: 1, deleted: 5 }\n  default_scope -> { where.not(status: Other.statuses[:deleted]) }\nend\n")],
        "Thing",
        "things",
        &["id", "status"],
    );
    assert!(
        other.filters.is_empty() && !other.unlowered.is_empty(),
        "{:?}",
        other.filters
    );

    // Rails casts a string on an enum column to the enum's value.
    let string = lowered(
        "enum-string",
        &[("thing.rb", "class Thing < ApplicationRecord\n  enum :status, { active: 1, deleted: 5 }\n  default_scope -> { where(status: \"active\") }\nend\n")],
        "Thing",
        "things",
        &["id", "status"],
    );
    assert!(
        string.filters.is_empty() && !string.unlowered.is_empty(),
        "{:?}",
        string.filters
    );
}

#[test]
fn an_octal_literal_or_an_unknown_column_is_not_lowered() {
    let octal = lowered(
        "octal",
        &[(
            "thing.rb",
            "class Thing < ApplicationRecord\n  default_scope { where(kind: 017) }\nend\n",
        )],
        "Thing",
        "things",
        &["id", "kind"],
    );
    assert!(
        octal.filters.is_empty() && !octal.unlowered.is_empty(),
        "{:?}",
        octal.filters
    );

    let assoc = lowered(
        "assoc",
        &[(
            "thing.rb",
            "class Thing < ApplicationRecord\n  default_scope { where(project: nil) }\nend\n",
        )],
        "Thing",
        "things",
        &["id", "project_id"],
    );
    assert!(
        assoc.filters.is_empty() && !assoc.unlowered.is_empty(),
        "{:?}",
        assoc.filters
    );

    let nil = lowered(
        "nil",
        &[("thing.rb", "class Thing < ApplicationRecord\n  default_scope { where(project_id: nil).includes(:x).preload(:y) }\nend\n")],
        "Thing",
        "things",
        &["id", "project_id"],
    );
    assert_eq!(
        nil.filters,
        [("project_id".to_string(), false, "Lit::Nil".to_string())]
    );
    assert_eq!(
        nil.no_row_effect.len(),
        1,
        "recorded once: {:?}",
        nil.no_row_effect
    );
}
