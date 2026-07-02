//! VSA (Vector Symbolic Architecture) modules.
//!
//! Currently provides the 256-D CAM-PQ role-key constants for the
//! `WitnessIndexCamPq` SPO adapter (D-CSV-16, sprint-13).
//!
//! **Scope:** 256-D f32 multiply-add VSA algebra for CAM-PQ witness indexing.
//! NOT the 16,384-D binary VSA in `grammar::role_keys` — that is a separate
//! algebra (binary XOR-bind, 16K dims).

pub mod roles;
