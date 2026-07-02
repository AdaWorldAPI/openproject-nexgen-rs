//! The UniCharSet **keystone** — `classid → ClassView → content store → adapter`,
//! steps 2–3 of `PROBE-OGAR-ADAPTER-UNICHARSET`.
//!
//! [`crate::unicharset`] proved step 1: the `UniCharSet` content adapter is
//! byte-identical to libtesseract (112/112). This module is the wiring the
//! core-first doctrine calls "mechanical once the adapter is proven": it
//! composes that adapter through the OGAR Core's three movable parts and shows a
//! dispatch reaches the proven leaf without the adapter carrying any state.
//!
//! | Core movable part | Here |
//! |---|---|
//! | identity = `classid` | the `u32` classid parameter (bound OGAR-side, never minted here) |
//! | composition = `classid → ClassView` | [`methods_for`] over the harvested `has_function` manifest gates which methods a class composes |
//! | state = classid-keyed content tier | [`UniCharSetStore`] resolves `classid → &UniCharSet` (consumer-provided; the adapter holds NO state — `I-VSA-IDENTITIES` content-store tier) |
//! | invocation | [`invoke_unicharset`] — the thin DO-in ([`UniCharCall`]) / DO-out ([`UniCharOut`]) shape a [`crate::orchestration::UnifiedStep`] would call |
//!
//! # Why this is "the keystone, not another adapter"
//!
//! It is the first place all three axes meet: the composition gate
//! ([`methods_for`]), the content-store tier ([`UniCharSetStore`]), and the
//! proven leaf ([`UniCharSet`]). Byte-parity is **inherited** from `UniCharSet`
//! (already green vs the C++ oracle); what this proves is that the dispatch path
//! is faithful (the parity edge survives it) and that the ClassView composition
//! gate works — i.e. the doctrine's iron guard holds (no adapter-state-leak, no
//! Core gap: the variable-length bijection rides the content tier cleanly).
//!
//! The full [`crate::orchestration::OrchestrationBridge`] is the *cross-subsystem*
//! router (Crew / Ladybug / LanceGraph / …); routing one in-class method call
//! through it would be over-wiring. [`invoke_unicharset`] is the adapter-
//! invocation primitive that the broader `UnifiedStep` orchestration dispatches.

use crate::codegen_manifest::{methods_for, ClassMethods};
use crate::unicharset::UniCharSet;

/// The classid-keyed **content-store tier**: resolve a `classid` to its loaded
/// [`UniCharSet`]. Implemented by the consumer (e.g. a `HashMap<u32, UniCharSet>`
/// loaded from `.unicharset` files); the contract owns only this vocabulary
/// (dependency inversion, exactly like [`crate::class_view::ClassView`] and
/// [`crate::plan::PlannerContract`]).
///
/// The content NEVER lives on the node row or the adapter — a variable-length
/// bijection is not fixed-width SoA state; it rides this tier, keyed by the
/// identity (`I-VSA-IDENTITIES`; the core-first doctrine's "content-store tier,
/// `deepnsm::Vocabulary`-shaped").
pub trait UniCharSetStore {
    /// The [`UniCharSet`] bound to `classid`, or `None` if no store is bound.
    fn unicharset(&self, classid: u32) -> Option<&UniCharSet>;
}

/// A typed call into the UniCharSet adapter set — the **DO-in**. Each variant's
/// [`method_name`](UniCharCall::method_name) is the C++ method the harvested
/// `has_function` manifest must list for the class to compose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniCharCall<'a> {
    /// `const char *id_to_unichar(UNICHAR_ID id) const` — the id→unichar read.
    IdToUnichar(u32),
    /// `UNICHAR_ID unichar_to_id(const char *repr) const` — the inverse lookup.
    UnicharToId(&'a str),
}

impl UniCharCall<'_> {
    /// The C++ method name this call dispatches to — the key the ClassView method
    /// manifest must contain for the call to be composed. These match the
    /// `UNICHARSET` member names the `ruff_cpp_spo` harvest emits.
    #[must_use]
    pub const fn method_name(&self) -> &'static str {
        match self {
            Self::IdToUnichar(_) => "id_to_unichar",
            Self::UnicharToId(_) => "unichar_to_id",
        }
    }
}

/// The adapter's typed result — the **DO-out**. Borrows the unichar string from
/// the content store (zero-copy; the store outlives the call).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniCharOut<'a> {
    /// The unichar at an id (`None` = out of range), from [`UniCharCall::IdToUnichar`].
    Unichar(Option<&'a str>),
    /// The id of a unichar (`None` = absent, the C++ `INVALID_UNICHAR_ID`
    /// sentinel), from [`UniCharCall::UnicharToId`].
    Id(Option<u32>),
}

/// Why a keystone dispatch could not run (a typed refusal, never a panic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchError {
    /// The classid's ClassView does not compose the called method — the harvested
    /// `has_function` manifest ([`methods_for`]) has no such [`MethodSig`](crate::codegen_manifest::MethodSig).
    /// This is the **composition gate** firing; an unconfigured classid composes
    /// nothing (zero-fallback), so it always lands here.
    MethodNotComposed {
        /// The classid whose manifest lacked the method.
        classid: u32,
        /// The C++ method name that was not composed.
        method: &'static str,
    },
    /// The method is composed, but no [`UniCharSet`] is bound to this classid in
    /// the content-store tier.
    NoContentStore {
        /// The classid with no bound content store.
        classid: u32,
    },
}

impl core::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MethodNotComposed { classid, method } => write!(
                f,
                "classid {classid:#010x} does not compose method `{method}`"
            ),
            Self::NoContentStore { classid } => {
                write!(
                    f,
                    "no UniCharSet content store bound to classid {classid:#010x}"
                )
            }
        }
    }
}

impl std::error::Error for DispatchError {}

/// **The keystone.** Invoke a `UniCharSet` adapter through the
/// `classid → ClassView` method manifest and the classid-keyed content store —
/// steps 2–3 of `PROBE-OGAR-ADAPTER-UNICHARSET`.
///
/// 1. **ClassView composition gate**: [`methods_for(registry, classid)`](methods_for)
///    must contain a method whose name is the call's
///    [`method_name`](UniCharCall::method_name) — i.e. the harvested
///    `has_function` manifest says this class composes this adapter. Otherwise
///    [`DispatchError::MethodNotComposed`] (an unconfigured classid composes
///    nothing — the zero-fallback ladder).
/// 2. **Content-store tier**: [`store.unicharset(classid)`](UniCharSetStore::unicharset)
///    supplies the loaded bijection; the adapter holds no state of its own.
/// 3. **Adapter leaf**: route to [`UniCharSet::id_to_unichar`] /
///    [`UniCharSet::unichar_to_id`].
///
/// Byte-parity is inherited from [`UniCharSet`] (112/112 vs libtesseract); this
/// proves the dispatch path is faithful and the composition gate holds.
///
/// # Errors
///
/// [`DispatchError::MethodNotComposed`] if the class's manifest does not list the
/// called method; [`DispatchError::NoContentStore`] if no `UniCharSet` is bound
/// to the classid.
pub fn invoke_unicharset<'a, S: UniCharSetStore + ?Sized>(
    registry: &[ClassMethods],
    store: &'a S,
    classid: u32,
    call: &UniCharCall<'_>,
) -> Result<UniCharOut<'a>, DispatchError> {
    let method = call.method_name();
    // 1. ClassView composition gate (the harvested has_function manifest).
    if !methods_for(registry, classid)
        .iter()
        .any(|m| m.name == method)
    {
        return Err(DispatchError::MethodNotComposed { classid, method });
    }
    // 2. Content-store tier — state lives here, never on the adapter.
    let unicharset = store
        .unicharset(classid)
        .ok_or(DispatchError::NoContentStore { classid })?;
    // 3. The proven adapter leaf.
    Ok(match *call {
        UniCharCall::IdToUnichar(id) => UniCharOut::Unichar(unicharset.id_to_unichar(id)),
        UniCharCall::UnicharToId(repr) => UniCharOut::Id(unicharset.unichar_to_id(repr)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_manifest::MethodSig;
    use std::collections::HashMap;

    /// A test OGAR classid for the Tesseract `UNICHARSET` class (bound OGAR-side;
    /// the keystone never mints it).
    const TESS_UNICHARSET: u32 = 0x0001_0001;

    /// The ClassView method manifest the `ruff_cpp_spo` harvest emits for
    /// `UNICHARSET` — the two byte-parity-proven leaves.
    const UNICHARSET_METHODS: &[MethodSig] = &[
        MethodSig {
            name: "id_to_unichar",
            params: &["UNICHAR_ID"],
            ret: Some("const char *"),
            is_const: true,
            is_static: false,
            overrides: None,
        },
        MethodSig {
            name: "unichar_to_id",
            params: &["const char *"],
            ret: Some("UNICHAR_ID"),
            is_const: true,
            is_static: false,
            overrides: None,
        },
    ];

    const REGISTRY: &[ClassMethods] = &[ClassMethods {
        classid: TESS_UNICHARSET,
        methods: UNICHARSET_METHODS,
    }];

    /// An in-memory content store — what a consumer builds from `.unicharset`
    /// files. The contract owns the trait; the consumer owns the answers.
    struct MemStore(HashMap<u32, UniCharSet>);

    impl UniCharSetStore for MemStore {
        fn unicharset(&self, classid: u32) -> Option<&UniCharSet> {
            self.0.get(&classid)
        }
    }

    fn store_with_null_sample() -> MemStore {
        // id 0 = NULL→space (the byte-parity edge), id 1 = "a".
        let u = UniCharSet::load_from_str("2\nNULL 0 Common 0\na 3 0 a Left a a\n").expect("valid");
        let mut m = HashMap::new();
        m.insert(TESS_UNICHARSET, u);
        MemStore(m)
    }

    /// The full keystone path: classid → ClassView (methods_for) → content store
    /// → adapter leaf. The `NULL`→space parity edge survives the whole dispatch.
    #[test]
    fn keystone_dispatches_with_inherited_byte_parity() {
        let store = store_with_null_sample();
        assert_eq!(
            invoke_unicharset(
                REGISTRY,
                &store,
                TESS_UNICHARSET,
                &UniCharCall::IdToUnichar(0)
            ),
            Ok(UniCharOut::Unichar(Some(" "))),
            "the NULL->space edge survives classid -> ClassView -> store -> adapter"
        );
        assert_eq!(
            invoke_unicharset(
                REGISTRY,
                &store,
                TESS_UNICHARSET,
                &UniCharCall::UnicharToId(" ")
            ),
            Ok(UniCharOut::Id(Some(0)))
        );
        assert_eq!(
            invoke_unicharset(
                REGISTRY,
                &store,
                TESS_UNICHARSET,
                &UniCharCall::IdToUnichar(1)
            ),
            Ok(UniCharOut::Unichar(Some("a")))
        );
        assert_eq!(
            invoke_unicharset(
                REGISTRY,
                &store,
                TESS_UNICHARSET,
                &UniCharCall::IdToUnichar(99)
            ),
            Ok(UniCharOut::Unichar(None)),
            "out-of-range id is a clean None, not a panic"
        );
    }

    /// The composition gate: a class whose manifest composes only `id_to_unichar`
    /// refuses `unichar_to_id`, even though the content store could answer it.
    #[test]
    fn keystone_classview_gate_rejects_uncomposed_method() {
        const ID_ONLY: &[ClassMethods] = &[ClassMethods {
            classid: TESS_UNICHARSET,
            methods: &[MethodSig {
                name: "id_to_unichar",
                params: &["UNICHAR_ID"],
                ret: Some("const char *"),
                is_const: true,
                is_static: false,
                overrides: None,
            }],
        }];
        let store = store_with_null_sample();
        assert_eq!(
            invoke_unicharset(
                ID_ONLY,
                &store,
                TESS_UNICHARSET,
                &UniCharCall::UnicharToId(" ")
            ),
            Err(DispatchError::MethodNotComposed {
                classid: TESS_UNICHARSET,
                method: "unichar_to_id",
            })
        );
    }

    /// Zero-fallback: an unregistered classid composes nothing, so the gate
    /// refuses before the store is even consulted.
    #[test]
    fn keystone_unregistered_classid_composes_nothing() {
        let store = store_with_null_sample();
        assert_eq!(
            invoke_unicharset(REGISTRY, &store, 0xDEAD_BEEF, &UniCharCall::IdToUnichar(0)),
            Err(DispatchError::MethodNotComposed {
                classid: 0xDEAD_BEEF,
                method: "id_to_unichar",
            })
        );
    }

    /// A composed method with no bound content store is a typed error, not a panic.
    #[test]
    fn keystone_missing_content_store_is_typed_error() {
        let empty = MemStore(HashMap::new());
        assert_eq!(
            invoke_unicharset(
                REGISTRY,
                &empty,
                TESS_UNICHARSET,
                &UniCharCall::IdToUnichar(0)
            ),
            Err(DispatchError::NoContentStore {
                classid: TESS_UNICHARSET,
            })
        );
    }

    #[test]
    fn call_method_names_match_the_harvest() {
        assert_eq!(UniCharCall::IdToUnichar(0).method_name(), "id_to_unichar");
        assert_eq!(UniCharCall::UnicharToId("x").method_name(), "unichar_to_id");
    }
}
