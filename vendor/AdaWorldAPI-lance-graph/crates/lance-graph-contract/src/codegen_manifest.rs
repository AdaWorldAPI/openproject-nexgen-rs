//! Codegen manifest types — the Core-side target of the C++ method-resolution
//! manifest emitted by `ruff_cpp_codegen` (the AST-DLL pipeline's stage 2).
//!
//! `ruff_cpp_spo` harvests a C++ corpus into SPO triples; `ruff_spo_triplet`
//! reassembles the method plane; `ruff_cpp_codegen` renders it as Rust source
//! that names [`MethodSig`]. This module is the **compile target** of that
//! generated text.
//!
//! # Why `&'static`, not `String`
//!
//! Every field is `&'static` so a generated
//! `const X: &[MethodSig] = &[MethodSig { .. }]` compiles. The render-side
//! manifest could not target [`crate::class_view::ClassView`]'s `fields()`
//! because `FieldRef` is `String`-backed (not `const`-constructible);
//! `MethodSig` is the method-axis sibling, deliberately all-`&'static`.
//!
//! # What lives here vs. downstream
//!
//! This module provides the **type + lookup**. The **data** — the per-class
//! `const … : &[MethodSig]` tables and the `&[ClassMethods]` registry — is
//! generated downstream (in the consumer repo, e.g. tesseract-rs) and is NOT
//! held here; lance-graph mints no C++ classids and stores no method tables.
//! The classid is bound OGAR-side (the `ocr.rs::to_node_row(classid, …)`
//! precedent — classid is a parameter, never minted by the manifest). A runtime
//! `classid → methods` registry is intentionally deferred: this is the minimal
//! additive Core growth (`MethodSig` is the dispatch signature; the body
//! adapters and a populated registry come later).
//!
//! `MethodSig` carries only the dispatch-relevant signature (name, ordered
//! params, return, `is_const`/`is_static`, override target). The body-shaping
//! flags the harvest also captures (`is_pure_virtual` / `constexpr` /
//! `noexcept` / operator-kind / `requires`) drive body generation, not the
//! signature manifest, and are not represented here.

/// One C++ method's dispatch-relevant signature, in a `const`-constructible
/// shape (all fields `&'static`). This is the exact literal
/// `ruff_cpp_codegen::render` emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodSig {
    /// Bare method name (e.g. `unichar_to_id`, `operator==`).
    pub name: &'static str,
    /// Parameter types in signature order, verbatim
    /// (e.g. `&["const char *", "int"]`).
    pub params: &'static [&'static str],
    /// Return type, verbatim. `None` for void / constructors / destructors.
    pub ret: Option<&'static str>,
    /// `T method() const;` — a const-qualified (read-accessor) member.
    pub is_const: bool,
    /// `static T method();` — a class-level member (no implicit `this`).
    pub is_static: bool,
    /// The fully-qualified overridden base method (cv-aware), if any.
    pub overrides: Option<&'static str>,
}

impl MethodSig {
    /// Number of parameters.
    #[must_use]
    pub const fn arity(&self) -> usize {
        self.params.len()
    }

    /// Whether this method overrides a virtual base method.
    #[must_use]
    pub const fn is_override(&self) -> bool {
        self.overrides.is_some()
    }
}

/// One class's method manifest, keyed by classid — the registry ENTRY the
/// generated code emits. The classid is bound OGAR-side; this type only
/// associates an already-minted classid with its `const` method table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassMethods {
    /// The OGAR classid this manifest belongs to.
    pub classid: u32,
    /// The class's methods (a generated `const` table).
    pub methods: &'static [MethodSig],
}

/// Resolve a classid to its method manifest within a generated `registry`
/// slice. The Core provides the lookup; the `registry` data is generated
/// downstream. Returns an empty slice for an unregistered classid (the
/// zero-fallback ladder: an unknown class resolves to "no methods", never a
/// panic).
#[must_use]
pub fn methods_for(registry: &[ClassMethods], classid: u32) -> &'static [MethodSig] {
    registry
        .iter()
        .find(|entry| entry.classid == classid)
        .map_or(&[], |entry| entry.methods)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The load-bearing property: `MethodSig` is `const`-constructible. This is
    /// the exact shape `ruff_cpp_codegen::render` emits — if it ever stopped
    /// compiling in a `const`, the generated manifests would too. (Contrast
    /// `FieldRef`, which is `String`-backed and cannot appear in a `const`.)
    const SAMPLE: &[MethodSig] = &[
        MethodSig {
            name: "unichar_to_id",
            params: &["const char *"],
            ret: Some("UNICHAR_ID"),
            is_const: true,
            is_static: false,
            overrides: None,
        },
        MethodSig {
            name: "kSpaceId",
            params: &[],
            ret: Some("UNICHAR_ID"),
            is_const: false,
            is_static: true,
            overrides: None,
        },
    ];

    const REGISTRY: &[ClassMethods] = &[ClassMethods {
        classid: 0x0001,
        methods: SAMPLE,
    }];

    #[test]
    fn const_manifest_constructs_and_reads() {
        assert_eq!(SAMPLE.len(), 2);
        assert_eq!(SAMPLE[0].name, "unichar_to_id");
        assert_eq!(SAMPLE[0].arity(), 1);
        assert!(SAMPLE[0].is_const);
        assert!(!SAMPLE[0].is_override());
        assert!(SAMPLE[1].is_static);
        assert_eq!(SAMPLE[1].arity(), 0);
    }

    #[test]
    fn methods_for_resolves_classid_with_zero_fallback() {
        assert_eq!(methods_for(REGISTRY, 0x0001).len(), 2);
        assert!(
            methods_for(REGISTRY, 0xDEAD).is_empty(),
            "an unregistered classid resolves to no methods (zero-fallback)"
        );
    }
}
