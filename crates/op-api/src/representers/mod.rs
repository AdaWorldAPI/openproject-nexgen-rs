//! API Representers
//!
//! HAL+JSON representers for OpenProject API v3 compatibility.
//! These convert domain models to API response format.

pub mod hal;
pub mod news;
pub mod project;
pub mod query;
pub mod user;
pub mod work_package;

// Re-exports
pub use hal::{HalCollection, HalEmbedded, HalError, HalLink, HalLinks, HalResource};
pub use work_package::{EmbedOptions, WorkPackageData, WorkPackageRepresenter};
