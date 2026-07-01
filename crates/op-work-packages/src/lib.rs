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

pub use work_package::{DoneRatio, WorkPackage};
