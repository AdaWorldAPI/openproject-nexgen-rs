//! Vendor partial-mirror of `lance-graph-contract` (upstream:
//! [`AdaWorldAPI/lance-graph`](https://github.com/AdaWorldAPI/lance-graph)).
//!
//! **Only** the [`codegen_spine`] module is exposed; the upstream crate has
//! ~90 additional modules (orchestration, NARS, cognition, recipes, RBAC,
//! cam, jit, …) that are not consumed by the OpenProject codegen surface in
//! this repository.
//!
//! Consumers in this repo (e.g. `op-codegen-bucket`) depend on this vendored
//! crate via `path = "../../vendor/…"` so the additive C6 + P2 changes
//! (`RouteBucketTyped<Kind>` + `?Sized` blanket impl) are usable downstream
//! before they land in upstream `lance-graph-contract`.
//!
//! See `vendor/AdaWorldAPI-lance-graph/README.md` for the apply-upstream flow.

pub mod codegen_spine;
