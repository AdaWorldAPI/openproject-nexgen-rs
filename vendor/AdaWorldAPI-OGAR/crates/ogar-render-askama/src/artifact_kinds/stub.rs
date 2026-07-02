//! Placeholder emitter — used by [`super::for_kind`] for any
//! [`ArtifactKind`] whose concrete askama template has not yet landed.
//! Mirror of `woa-rs::codegen::handler_kinds::stub`.
//!
//! Emits a single-line marker comment naming the kind + class. The
//! pipeline (lookup, dispatch, return) is exercisable end-to-end against
//! every promoted concept while individual templates are still in flight.

use super::ArtifactEmitter;
use crate::spec::{ArtifactKind, ArtifactSpec};

/// Placeholder emitter — returns a marker comment for any
/// [`ArtifactKind`] whose template has not yet landed.
pub struct Stub {
    /// The kind this stub stands in for. Surfaced in the marker comment.
    pub kind: ArtifactKind,
}

impl ArtifactEmitter for Stub {
    fn emit(&self, spec: &ArtifactSpec<'_>) -> Result<String, askama::Error> {
        Ok(format!(
            "// {kind} stub — {class} ({concept})\n\
             // codegen pending: template not yet landed for this artifact kind.\n",
            kind = self.kind.name(),
            class = spec.class.name,
            concept = spec
                .class
                .canonical_concept
                .as_deref()
                .unwrap_or("<no canonical concept>"),
        ))
    }
}
