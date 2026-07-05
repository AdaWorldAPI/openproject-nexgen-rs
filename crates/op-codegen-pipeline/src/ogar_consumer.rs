//! The ogar-emit consumer path (§5 steps 4-5 of the ogar-v3 consumer
//! migration plan): `source -> ruff -> OGAR lift/mint -> Class -> adapter
//! emit`, wired *alongside* — never replacing — the crate's native
//! `ruff -> op-surreal-ast -> SurrealQL` path (see [`render_typed_surreal`]
//! and [`render_surreal_from_ruff`] in the crate root). `op-codegen-pipeline`
//! adds **zero transpiler logic** here: every step is a thin call into an
//! upstream OGAR crate.
//!
//! ```text
//!   Rails source
//!        │  ruff_ruby_spo::extract_app_with_schema
//!        ▼
//!   ruff_spo_triplet::ModelGraph
//!        │  filter_to_core (this crate's curated-resource filter)
//!        ▼
//!   ogar_from_ruff::mint::compile_graph_ruby::<P>   (lift + mint, port P)
//!        ▼
//!   Vec<CompiledClass { class, facet }>
//!        │  .class
//!        ▼
//!   ogar_adapter_surrealql::emit_surrealql_ddl
//!        ▼
//!   SurrealQL DDL text
//! ```
//!
//! The native path (`op-surreal-ast`) stays the production path until §5
//! step 6 (full test-porting) lands; this module is additive and
//! feature-gated behind `ogar-emit` so opting in costs nothing for
//! consumers that don't.

use std::path::Path;

use ogar_from_ruff::mint::{compile_graph_ruby, CompiledClass};
use ogar_vocab::ports::{OpenProjectPort, PortSpec};
use ruff_spo_triplet::ModelGraph;

/// Compile a Rails-shaped [`ModelGraph`] into rail-shaped [`CompiledClass`]es
/// via port `P`. Thin wrapper over
/// [`ogar_from_ruff::mint::compile_graph_ruby`] — this crate contributes no
/// lift/mint logic of its own.
#[must_use]
pub fn compile_op<P: PortSpec>(graph: &ModelGraph) -> Vec<CompiledClass> {
    compile_graph_ruby::<P>(graph)
}

/// Compile `graph` through the [`OpenProjectPort`] and emit SurrealQL DDL
/// via the OGAR adapter ([`ogar_adapter_surrealql::emit_surrealql_ddl`]).
#[must_use]
pub fn emit_surreal_via_ogar(graph: &ModelGraph) -> String {
    let classes: Vec<_> = compile_op::<OpenProjectPort>(graph)
        .into_iter()
        .map(|cc| cc.class)
        .collect();
    ogar_adapter_surrealql::emit_surrealql_ddl(&classes)
}

/// End-to-end: extract a Rails source tree WITH the schema stratum
/// ([`ruff_ruby_spo::extract_app_with_schema`]), filter to the curated core
/// ([`crate::filter_to_core`]), and emit SurrealQL DDL through the OGAR
/// consumer path.
#[must_use]
pub fn render_surreal_via_ogar(rails_root: &Path) -> String {
    let (mut graph, _report) = ruff_ruby_spo::extract_app_with_schema(rails_root, crate::NAMESPACE);
    crate::filter_to_core(&mut graph);
    emit_surreal_via_ogar(&graph)
}

/// Resolve a port-public model name to its full render classid via port
/// `P`: `class_id(name).map(render_classid_for::<P>)`. Accessor only — no
/// bit-shifting in this crate; the composition lives in
/// [`ogar_vocab::app::render_classid_for`].
#[must_use]
pub fn render_classid_of<P: PortSpec>(surface_name: &str) -> Option<u32> {
    P::class_id(surface_name).map(ogar_vocab::app::render_classid_for::<P>)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::ports::RedminePort;

    /// The convergence pin this consumer path exists to preserve: the SAME
    /// canonical concept resolves for OpenProject's and Redmine's public
    /// names, while the two ports' render prefixes stay distinct (two
    /// render skins, one concept — see `ogar_vocab::ports` docs).
    #[test]
    fn openproject_and_redmine_converge_on_shared_concepts() {
        for (op_name, rm_name) in [
            ("WorkPackage", "Issue"),
            ("TimeEntry", "TimeEntry"),
            ("Project", "Project"),
        ] {
            assert_eq!(
                OpenProjectPort::class_id(op_name),
                RedminePort::class_id(rm_name),
                "OpenProject `{op_name}` and Redmine `{rm_name}` must converge on one concept",
            );
        }
        assert_ne!(
            OpenProjectPort::APP_PREFIX,
            RedminePort::APP_PREFIX,
            "the two ports must render under distinct app prefixes",
        );
    }

    /// `render_classid_of` composes the OGAR-canonical render classid: the
    /// concept half (`project_work_item` = `0x0102`) high, the
    /// `OpenProjectPort` render prefix (`0x0001`) low.
    #[test]
    fn render_classid_of_composes_openprojects_work_package() {
        assert_eq!(
            render_classid_of::<OpenProjectPort>("WorkPackage"),
            Some(0x0102_0001),
        );
    }

    #[test]
    fn render_classid_of_unknown_name_resolves_to_none() {
        assert_eq!(render_classid_of::<OpenProjectPort>("NotAConcept"), None);
    }
}
