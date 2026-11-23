// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! `::X` is the top-level constant only. The fixture is the shape of Zammad's
//! `ChecklistTemplate::HasAuditLogs`, which does `include ::HasAuditLogs`;
//! `ruby` on the same files says which module that names.

use std::path::Path;
use std::process::Command;

use rails_compiler::harvest::Harvest;

const TOP: &str = "module HasAuditLogs\nend\n";
const NESTED: &str = "module Holder\n  module HasAuditLogs\n    include ::HasAuditLogs\n  end\nend\n";

// Each test writes its own copy: tests run in parallel, and a rewrite
// truncates the file another test may be reading.
fn fixture(test: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("absolute_constants").join(test);
    let models = dir.join("app/models");
    std::fs::create_dir_all(models.join("holder")).expect("fixture dir");
    std::fs::write(models.join("has_audit_logs.rb"), TOP).expect("top");
    std::fs::write(models.join("holder/has_audit_logs.rb"), NESTED).expect("nested");
    dir
}

#[test]
fn an_absolute_name_resolves_at_the_top_level_only() {
    let dir = fixture("rust");
    let h = Harvest::collect(&dir, &["app/models".to_string()], "Fixture");
    let nesting = ["Holder::HasAuditLogs".to_string(), "Holder".to_string()];
    assert_eq!(h.resolve("::HasAuditLogs", &nesting).as_deref(), Some("HasAuditLogs"));
    assert_eq!(h.resolve("HasAuditLogs", &nesting).as_deref(), Some("Holder::HasAuditLogs"));
    let (found, absent) = h.resolve_with_absent("::HasAuditLogs", &nesting);
    assert_eq!((found.as_deref(), absent.len()), (Some("HasAuditLogs"), 0));
}

#[test]
fn ruby_agrees() {
    let dir = fixture("ruby");
    let script = format!(
        "load '{0}/app/models/has_audit_logs.rb'; load '{0}/app/models/holder/has_audit_logs.rb'; \
         print Holder::HasAuditLogs.ancestors.map(&:name).join(',')",
        dir.display()
    );
    let Ok(out) = Command::new("ruby").args(["-e", &script]).output() else {
        eprintln!("ruby not found: skipped");
        return;
    };
    assert_eq!(String::from_utf8_lossy(&out.stdout), "Holder::HasAuditLogs,HasAuditLogs");
}
