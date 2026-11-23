//! Source gating shared by the tests that read the OpenProject checkout, the
//! repository's convention (op-codegen-pipeline's `callback_differential`):
//! without `OPENPROJECT_SRC` the test prints UNVERIFIED and passes, unless
//! `REQUIRE_SOURCES=1`, which makes a missing source a failure.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub fn openproject_src(test: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPENPROJECT_SRC") {
        let p = PathBuf::from(p);
        assert!(
            p.join("app/models").is_dir(),
            "OPENPROJECT_SRC={} has no app/models",
            p.display()
        );
        return Some(p);
    }
    assert!(
        std::env::var("REQUIRE_SOURCES").as_deref() != Ok("1"),
        "REQUIRE_SOURCES=1 but OPENPROJECT_SRC is not set"
    );
    eprintln!("UNVERIFIED: {test} needs OPENPROJECT_SRC");
    None
}

pub const STRUCTURE: &str = "oracle/openproject-structure.sql";

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// OpenProject's schema at the pinned revision: its `db/structure.sql`, which
/// OpenProject does not commit, captured in `oracle/openproject-structure.sql`.
pub fn structure() -> std::collections::BTreeMap<String, op_desk::structure::SqlTable> {
    let path = repo_root().join(STRUCTURE);
    let sql = std::fs::read_to_string(&path).expect("oracle/openproject-structure.sql");
    op_desk::structure::parse(&sql)
}
