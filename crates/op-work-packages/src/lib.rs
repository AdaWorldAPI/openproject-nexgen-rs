//! # op-work-packages
//!
//! Work package model + services for OpenProject RS.
//!
//! Mirrors: `app/models/work_package.rb`
//!
//! The headline OpenProject entity — a project-scoped unit of work (task,
//! bug, feature, …). Converges with Redmine's `Issue` on the canonical
//! concept `project_work_item` (codebook `0x0102`); see `op-canon`.

pub mod work_package;
pub mod work_package_service;

// `CanonicalWorkPackage` is the OGAR-rendered `project_work_item` struct from
// `op-generated` (W6: wiring a real consumer to the transpile landing zone).
// It's additive, not a replacement — see the doc comment on the re-export in
// `work_package.rs` and the `From<&WorkPackage>` mapping next to it.
pub use work_package::{CanonicalWorkPackage, DoneRatio, WorkPackage};
pub use work_package_service::{
    MemoryWorkPackageStore, NewWorkPackage, UpdateWorkPackage, WorkPackageError, WorkPackageResult,
    WorkPackageService, WorkPackageStore,
};
