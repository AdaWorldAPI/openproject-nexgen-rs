//! # lance-graph-contract — The Single Source of Truth
//!
//! Zero-dependency trait crate that defines the contract between:
//! - **lance-graph-planner** (implements these traits)
//! - **ladybug-rs** (calls Planner + CamPq + OrchestrationBridge)
//!
//! **EVICTED 2026-06-21 (operator):** `crewai-rust` and `n8n-rs` are NO LONGER
//! binding consumers of this contract — superseded by ladybug-rs + the in-tree
//! thinking-engine / planner. Their roles (agent orchestration, workflow DAG /
//! JIT styles) fold in-tree. Consequence: the contract is free to evolve without
//! a crewai/n8n multi-repo bump; only IN-TREE consumers (planner, callcenter,
//! smb-bridge, symbiont) gate API changes. Scattered `// replaces crewai-rust's
//! X` / `// n8n-rs calls Y` doc-comments below are retained as historical
//! PROVENANCE (not live consumer claims); `StepDomain::{Crew, N8n}` stay as
//! reserved-dormant routing tags (reserve-don't-reclaim). See EPIPHANIES
//! `E-CREWAI-N8N-EVICTED`.
//!
//! # Why This Exists
//!
//! Before this crate, each consumer duplicated thinking style enums,
//! field modulation structs, and query plan types. Now:
//!
//! ```text
//! ladybug-rs ───┐
//!               ├── depend on ──► lance-graph-contract (traits only)
//! in-tree ──────┘   (planner / callcenter / smb-bridge / symbiont)
//!
//! lance-graph-planner ──► implements lance-graph-contract traits
//! (crewai-rust / n8n-rs: EVICTED 2026-06-21 — no longer consumers)
//! ```
//!
//! # Module Layout
//!
//! - [`thinking`] — 36 thinking styles, 6 clusters, τ addresses, 23D vectors
//! - [`mul`] — MUL assessment (Dunning-Kruger, trust, flow, compass)
//! - [`plan`] — Query planning traits (PlanStrategy, PlanResult)
//! - [`cam`] — CAM-PQ distance contract (6-byte fingerprint ops)
//! - [`jit`] — JIT compilation contract (jitson template → kernel)
//! - [`orchestration`] — Bridge trait for single-binary routing
//! - [`nars`] — NARS inference types shared across all consumers
//! - [`collapse_gate`] — Per-row write airgap (`GateDecision`, `MergeMode`)
//! - [`cycle_accumulator`] — Per-cadence flush gate; absorbs the L1↔L3
//!   speed ratio. Distinct from `collapse_gate` per topology I-4.

pub mod cognition;
pub mod transaction;

pub mod a2a_blackboard;
pub mod action;
pub mod aiwar;
pub mod atoms;
pub mod auth;
pub mod callcenter;
pub mod cam;
pub mod canonical_node;
pub mod class_view;
/// D-V3-W6a — classid adoption-scan counting logic (`ClassidForm`,
/// `classify_form`, `AdoptionCounts`, `count_adoption`). See
/// `.claude/v3/soa_layout/routing.md` §5.
pub mod classid_scan;
/// D-GV2-2 — per-family codebook (`family → Codebook`), gated on the v2 tail.
#[cfg(feature = "guid-v2-tail")]
pub mod codebook;
pub mod codegen_manifest;
pub mod codegen_spine;
pub mod cognitive_shader;
pub mod collapse_gate;
pub mod container;
pub mod content_store;
pub mod counterfactual;
pub mod crystal;
pub mod cycle_accumulator;
pub mod distance;
/// D-V3-W6a — DDL typed-emission counting logic (`TypedForm`,
/// `classify_ddl_type`, `EmissionCounts`, `count_emission`), sibling of
/// [`classid_scan`]. Requested by the op-nexgen consumer session.
pub mod emission_scan;
pub mod episodic_edges;
pub mod escalation;
pub mod exploration;
pub mod external_membrane;
pub mod facet;
pub mod facet_schema;
pub mod faculty;
pub mod grammar;
pub mod graph_render;
pub mod hash;
pub mod head2head;
pub mod hhtl;
pub mod high_heel;
pub mod jit;
pub mod kanban;
pub mod literal_graph;
pub mod mail;
pub mod manifest;
pub mod mul;
pub mod nan_projection;
pub mod nars;
pub mod ocr;
/// D-OVC-1 — OGAR concept codebook (`0xDDCC` domain layout), wire-compat mirror.
pub mod ogar_codebook;
pub mod ontology;
pub mod orchestration;
pub mod orchestration_mode;
pub mod pearl_junction;
pub mod persona;
pub mod plan;
pub mod property;
pub mod proprioception;
pub mod qualia;
pub mod rbac;
pub use qualia::{
    axis_index, axis_label, qualia_to_state, QualiaI4_16D, QualiaVector, AXIS_LABELS, MIDPOINT,
    QUALIA_DIMS, QUALIA_I4_DIMS, QUALIA_I4_LABELS, ZERO,
};
pub mod materialize;
pub mod reasoning;
pub mod recipe_kernels;
pub mod recipes;
pub mod repository;
pub mod savants;
pub mod scenario;
pub mod scheduler;
pub mod sensorium;
pub mod sigma_propagation;
pub mod sla;
pub mod soa_envelope;
pub mod soa_graph;
pub mod soa_view;
pub mod splat;
pub mod tax;
/// Per-tenant SoA update counters — debug instrumentation (feature `tenant-counters`).
pub mod tenant_counter;
pub mod thinking;
pub mod unichar;
pub mod unicharset;
pub mod unicharset_adapter;
pub mod view_angle;
pub mod vsa;
pub mod witness_table;
pub mod world_map;
pub mod world_model;

// Re-exports for the most commonly used collapse_gate types.
pub use canonical_node::{
    classid_read_mode, node_rows_from_le_bytes, EdgeBlock, EdgeCodecFlavor, GuidParts,
    KanbanTenant, NodeGuid, NodeRow, NodeRowPacket, ReadMode, ValueSchema, ValueTenant,
    VALUE_TENANTS,
};
pub use class_view::{ClassId, ClassProjection, ClassView, FieldMask, RenderRow};
pub use collapse_gate::{GateDecision, MailboxId, MergeMode};
pub use episodic_edges::{EdgeRef, EpisodicEdges64};
pub use head2head::{CompetitionOutcome, Head2Head, WinnerCriterion};
pub use kanban::{ExecTarget, KanbanColumn, KanbanMove, RubiconTransitionError};
pub use ogar_codebook::{
    canonical_concept_domain, canonical_concept_id, classid_app_prefix, classid_concept,
    classid_concept_domain, render_classid, render_classid_for_concept, source_domain_concept,
    AppPrefix, ConceptDomain, LabelDTO, CODEBOOK,
};
pub use scheduler::{DatasetVersion, NextPhaseScheduler, VersionScheduler};
pub use soa_graph::{
    nearest_anchor, project_snapshot, AnchorHop, DomainSpec, ERP, FMA_ANATOMY, OSINT_GOTHAM,
    PROJECT,
};
pub use soa_view::{MailboxSoaOwner, MailboxSoaView};
pub use view_angle::ViewAngle;
