// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright The Lance Authors

//! Codegen pipeline spine — the four canonical contracts that gate the
//! `triplets → static codegen → askama route SoC → askama GUI shape` layering
//! against duplication and drift.
//!
//! # The four layers, the four contracts
//!
//! ```text
//!   TRIPLETS                  source of truth (lossless, content-addressed)
//!        │
//!        │ ① TripletProjection                must round-trip equal or it lost an iota
//!        ▼
//!   STATIC CODEGEN             per ir.* record-type Rust consts
//!        │
//!        │ ② RouteBucket / OdooMethodKind     one canonical bucket enum, both layers read it
//!        ▼
//!   ASKAMA ROUTE SoC           per-bucket emitter recipe
//!        │
//!        │ ③ WidgetRender                     templates take a bucket, never re-classify
//!        ▼
//!   ASKAMA GUI SHAPE           WidgetView templates per (ObjectType, Bucket)
//!        │
//!        │ ④ Genericity                       marker — codegen const vs runtime-triple-read
//! ```
//!
//! Without these contracts, each layer is free to re-classify the same routes,
//! re-implement the same dependency walks, and re-emit overlapping templates;
//! every fresh session pays a token tax on rediscovery. With them:
//!
//! 1. `TripletProjection::roundtrip_eq` is a build-time test — a projection
//!    that loses triples fails CI, not a code review.
//! 2. `OdooMethodKind` is *one* enum; the static codegen layer, the askama
//!    route SoC emitter, and the GUI widget templates all consume it. A new
//!    method opening = one new variant, propagated by the type system.
//! 3. `WidgetRender` types take `&dyn RouteBucket` (or `&B: RouteBucket`),
//!    never raw triples. A widget that wants to re-classify routes must
//!    declare a new `RouteBucket` impl — visible at the type surface.
//! 4. `Genericity` is the explicit "what NOT to codegen" marker. Agnostic
//!    behaviour reads triples; domain-specific consts get codegen'd.
//!
//! # Zero-dep, std-only
//!
//! Per the contract crate's invariant. `BTreeMap` / `BTreeSet` for the
//! round-trip set comparison; `std::any::type_name` for diagnostics.
//!
//! # Iron-rule cross-refs
//!
//! - `I-VSA-IDENTITIES` (CLAUDE.md) — bucket enum lives at the catalogue
//!   layer (typed, lookup-by-enum), not in fingerprint similarity.
//! - `E-THREE-PLANES-1` (EPIPHANIES.md) — runtime never crosses the layer
//!   boundaries; these traits *are* the compile-time boundary.
//! - `E-FOUNDRY-LAYER-1` (EPIPHANIES.md) — Foundry Ontologie is the typed
//!   domain layer this spine routes between.

use std::collections::BTreeSet;
use std::fmt;

// ---------------------------------------------------------------------------
// The canonical triple type (zero-dep mirror of OntologyTriple in spo)
// ---------------------------------------------------------------------------

/// One ontology triple in the codegen pipeline.
///
/// The (s, p, o) tuple is the identity (used for set-equality in round-trip
/// tests). The (f, c) pair carries NARS truth — compared with tolerance
/// when verifying projections.
///
/// This type is intentionally `String`-keyed and `Clone` rather than
/// fingerprint-typed: it lives at the codegen-spine layer where humans + AST
/// emitters work in IRIs. The fingerprint form is for the runtime
/// `SpoStore` (see `lance_graph::graph::spo::odoo_ontology`).
#[derive(Debug, Clone, PartialEq)]
pub struct Triple {
    pub s: String,
    pub p: String,
    pub o: String,
    pub f: f32,
    pub c: f32,
}

impl Triple {
    /// The identity key of the triple — what `roundtrip_eq` compares as a set.
    pub fn key(&self) -> (String, String, String) {
        (self.s.clone(), self.p.clone(), self.o.clone())
    }
}

// ---------------------------------------------------------------------------
// ① TripletProjection — the static-codegen layer's contract
// ---------------------------------------------------------------------------

/// A lossless projection from triples to a static codegen const form.
///
/// **Round-trip equality is the gate.** Any projection that loses
/// information at the (s, p, o) identity level breaks the contract. Truth
/// values may be coarsened (e.g. quantised) but must round-trip within
/// `roundtrip_eq`'s tolerance.
///
/// # Why this trait gates the layer
///
/// Without it, "we project the dependency graph into a CSR adjacency for
/// fast traversal" is an unverified claim. With it, the projection must
/// expose `decompile` and pass `roundtrip_eq` on the input triple set, so
/// information loss is a build failure.
pub trait TripletProjection {
    /// The const form produced by `project` — opaque to the trait, must be
    /// `Clone` so tests can compare before/after.
    type Const: Clone;

    /// A human-readable name for this projection — used in error messages.
    fn name() -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Tolerance for `f`/`c` comparison in `roundtrip_eq`. Default 0.0
    /// (exact equality required); override to allow quantised projections.
    /// The check is ALWAYS run — `0.0` does NOT skip it; it requires the
    /// recovered truth value to match the source exactly.
    fn truth_tolerance() -> f32 {
        0.0
    }

    /// Project a triple set into the const form.
    fn project(triples: &[Triple]) -> Self::Const;

    /// Recover triples from the const form. The recovered set must be
    /// (s, p, o)-equal to the input.
    fn decompile(c: &Self::Const) -> Vec<Triple>;
}

/// The round-trip equality test for any `TripletProjection`.
///
/// Returns `Ok(())` on losslessness, `Err(RoundTripFailure)` describing the
/// missing or extraneous triples otherwise. Use in `#[test]` to gate
/// projections at build time.
pub fn roundtrip_eq<P: TripletProjection>(input: &[Triple]) -> Result<(), RoundTripFailure> {
    let projected = P::project(input);
    let regenerated = P::decompile(&projected);

    let in_keys: BTreeSet<(String, String, String)> = input.iter().map(Triple::key).collect();
    let out_keys: BTreeSet<(String, String, String)> =
        regenerated.iter().map(Triple::key).collect();

    let missing: Vec<_> = in_keys.difference(&out_keys).cloned().collect();
    let extraneous: Vec<_> = out_keys.difference(&in_keys).cloned().collect();

    if !missing.is_empty() || !extraneous.is_empty() {
        return Err(RoundTripFailure {
            projection: P::name(),
            input_count: in_keys.len(),
            output_count: out_keys.len(),
            missing_count: missing.len(),
            extraneous_count: extraneous.len(),
            sample_missing: missing.into_iter().take(3).collect(),
            sample_extraneous: extraneous.into_iter().take(3).collect(),
        });
    }

    // Truth-value tolerance check — always run; tol = 0.0 means strict
    // (any difference fails the `abs() > tol` check naturally).
    let tol = P::truth_tolerance();
    let in_truth: std::collections::BTreeMap<_, _> =
        input.iter().map(|t| (t.key(), (t.f, t.c))).collect();
    for r in &regenerated {
        if let Some((f0, c0)) = in_truth.get(&r.key()) {
            if (r.f - f0).abs() > tol || (r.c - c0).abs() > tol {
                return Err(RoundTripFailure {
                    projection: P::name(),
                    input_count: in_keys.len(),
                    output_count: out_keys.len(),
                    missing_count: 0,
                    extraneous_count: 0,
                    sample_missing: vec![r.key()],
                    sample_extraneous: vec![],
                });
            }
        }
    }

    Ok(())
}

/// Failure mode from `roundtrip_eq` — describes the information loss.
#[derive(Debug, Clone)]
pub struct RoundTripFailure {
    pub projection: &'static str,
    pub input_count: usize,
    pub output_count: usize,
    pub missing_count: usize,
    pub extraneous_count: usize,
    pub sample_missing: Vec<(String, String, String)>,
    pub sample_extraneous: Vec<(String, String, String)>,
}

impl fmt::Display for RoundTripFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TripletProjection `{}` lost information: \
             input={} output={} missing={} extraneous={}; \
             sample missing={:?} sample extraneous={:?}",
            self.projection,
            self.input_count,
            self.output_count,
            self.missing_count,
            self.extraneous_count,
            self.sample_missing,
            self.sample_extraneous
        )
    }
}

// ---------------------------------------------------------------------------
// ② RouteBucket / OdooMethodKind — the canonical bucket enum
// ---------------------------------------------------------------------------

/// The canonical 16-variant classification of Odoo method bodies.
///
/// Ported from `.claude/odoo/openings_hops.py` (this session). Priority
/// classifier semantics: variants are checked in declaration order;
/// first-match-wins. The order is load-bearing — do NOT reorder without
/// understanding the implications for the classifier.
///
/// **Source-of-truth for the bucket enum across the entire codegen pipeline.**
/// Static codegen reads it. Askama route SoC reads it. GUI widget templates
/// read it. Adding a 17th opening = one variant + one `match` arm in every
/// consumer (compiler-enforced exhaustiveness).
///
/// # Classifier wiring is a separate emitter (TBD)
///
/// This enum is the *bucket catalogue*. The function that takes a method
/// body / AST / `ActionSpec` and returns the matching `OdooMethodKind`
/// lives in a downstream classifier emitter (the Rust port of
/// `.claude/odoo/openings_hops.py`'s priority cascade). It is intentionally
/// NOT wired into `lance_graph::graph::spo::action_emitter` yet —
/// `ActionSpec` carries the structural edges (effects / inputs / raises /
/// reads / traverses); the kind classification gets bolted on by the
/// classifier in a follow-up PR. Until then, consumers that need a kind
/// should resolve it via the priority classifier directly, not by
/// inspecting `ActionSpec`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OdooMethodKind {
    /// `pass` body — explicit no-op framework override.
    PassOverride,
    /// Single statement of the form `super()._compute_X()` — pure delegation.
    SuperDelegationPure,
    /// `super().X()` then additional statements — extension of the parent.
    SuperExtend,
    /// State-machine transition with guard: checks `self.state`, assigns it.
    StateTransitionWithGuard,
    /// `if not self.X: self.Y = False` style cascade clearing.
    OnchangeClearDependentCascade,
    /// `for r in self.filtered(λ): ... raise` — guarded filtered validator.
    IterFilteredRaiseOnViolation,
    /// `for r in self.filtered(λ): r.X = ...` — filtered mutation.
    IterFilteredMutate,
    /// `for r in self: ... raise` — record-iterating validator.
    IterRecordsRaiseOnViolation,
    /// `for r in self: r.X = sum(r.<rel>.mapped(...))` — aggregate relation.
    IterRecordsAggregateRelation,
    /// `for r in self: r.X = r.<rel>.<field>` — compute from related chain.
    /// **The dominant Odoo opening** (~57 % of methods).
    IterRecordsComputeFromRelated,
    /// Uses `.sudo()` for privilege escalation in a lookup.
    SudoEscalationLookup,
    /// Uses `.with_context(...)` to shift query context.
    WithContextQueryShift,
    /// Body raises but doesn't match a more specific validator opening.
    ValidatorOther,
    /// Compute method that doesn't match a more specific compute opening.
    ComputeScalarOther,
    /// `@api.onchange` method that doesn't match a more specific opening.
    OnchangeOther,
    /// Catch-all — extend the classifier rather than letting this grow.
    Other,
}

impl OdooMethodKind {
    /// Stable snake_case identifier — for codegen output paths, template
    /// names, and cross-language interop. Never reformat.
    pub const fn id(&self) -> &'static str {
        match self {
            Self::PassOverride => "pass_override",
            Self::SuperDelegationPure => "super_delegation_pure",
            Self::SuperExtend => "super_extend",
            Self::StateTransitionWithGuard => "state_transition_with_guard",
            Self::OnchangeClearDependentCascade => "onchange_clear_dependent_cascade",
            Self::IterFilteredRaiseOnViolation => "iter_filtered_raise_on_violation",
            Self::IterFilteredMutate => "iter_filtered_mutate",
            Self::IterRecordsRaiseOnViolation => "iter_records_raise_on_violation",
            Self::IterRecordsAggregateRelation => "iter_records_aggregate_relation",
            Self::IterRecordsComputeFromRelated => "iter_records_compute_from_related",
            Self::SudoEscalationLookup => "sudo_escalation_lookup",
            Self::WithContextQueryShift => "with_context_query_shift",
            Self::ValidatorOther => "validator_other",
            Self::ComputeScalarOther => "compute_scalar_other",
            Self::OnchangeOther => "onchange_other",
            Self::Other => "other",
        }
    }

    /// All variants in priority order. Used by the classifier (first match
    /// wins) and by exhaustiveness tests.
    pub const ALL: [OdooMethodKind; 16] = [
        Self::PassOverride,
        Self::SuperDelegationPure,
        Self::SuperExtend,
        Self::StateTransitionWithGuard,
        Self::OnchangeClearDependentCascade,
        Self::IterFilteredRaiseOnViolation,
        Self::IterFilteredMutate,
        Self::IterRecordsRaiseOnViolation,
        Self::IterRecordsAggregateRelation,
        Self::IterRecordsComputeFromRelated,
        Self::SudoEscalationLookup,
        Self::WithContextQueryShift,
        Self::ValidatorOther,
        Self::ComputeScalarOther,
        Self::OnchangeOther,
        Self::Other,
    ];

    /// Lookup by stable id. Returns `None` for unknown ids — callers should
    /// treat this as a hard schema error (the id space is closed).
    pub fn from_id(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.id() == s)
    }
}

impl fmt::Display for OdooMethodKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// A route's bucket assignment — what kind it is + how it's identified.
///
/// Consumers of the askama route SoC take `&dyn RouteBucket` or `&B: RouteBucket`,
/// never raw triples. This forces re-classification to happen at the bucket
/// layer, not in templates.
pub trait RouteBucket {
    /// The canonical kind. Drives downstream dispatch + template selection.
    fn kind(&self) -> OdooMethodKind;

    /// Stable identity of this route (e.g. `account.move._compute_amount`).
    fn id(&self) -> &str;

    /// Owned-id escape hatch for async/iterator pipelines that need to
    /// outlive a `&dyn RouteBucket` borrow. Defaults to cloning `id()`;
    /// implementors with a pre-allocated owned string can override.
    fn id_owned(&self) -> String {
        self.id().to_string()
    }
}

// ---------------------------------------------------------------------------
// ② (cont.) RouteBucketTyped — kind-generic sibling for non-Odoo targets
// ---------------------------------------------------------------------------

/// Sibling to [`RouteBucket`] for codegen targets whose handler kinds are
/// not the Odoo set. Generic over the kind type so a non-Odoo target
/// (e.g. OpenProject, Wikidata, a future framework) can reuse the bucket
/// abstraction without forcing its kinds into [`OdooMethodKind`].
///
/// # Why this exists
///
/// [`RouteBucket::kind`] returns [`OdooMethodKind`] by value — concrete,
/// not a generic or associated type. That hardcodes the Odoo handler-kind
/// taxonomy into the trait surface, so a non-Odoo consumer cannot
/// `impl RouteBucket` with its own kind enum (no additive escape).
///
/// `RouteBucketTyped` parameterises the kind so each target can bring its
/// own enum. The existing [`RouteBucket`] is **not** modified; the
/// [`blanket impl`][impl_blanket] below makes every `RouteBucket` automatically
/// a `RouteBucketTyped<Kind = OdooMethodKind>` so generic consumers can
/// accept both shapes (`fn f<B: RouteBucketTyped<Kind = MyKind>>(b: &B)` and
/// `fn g<B: RouteBucketTyped<Kind = OdooMethodKind>>(b: &B)` both work).
///
/// # Coherence note
///
/// The blanket impl pins `Kind = OdooMethodKind` for every `RouteBucket`
/// implementor. A type that *also* needs `RouteBucketTyped` with a
/// **different** kind must NOT impl `RouteBucket` (it would conflict).
/// Non-Odoo targets simply skip the legacy trait and impl this one directly.
pub trait RouteBucketTyped {
    /// The handler-kind enum specific to this target. Must be `Copy + Eq`
    /// so it can drive dispatcher tables / `match` arms / hash keys.
    type Kind: Copy + Eq;

    /// The kind of this route in this target's taxonomy.
    fn kind(&self) -> Self::Kind;

    /// Stable identity of this route (same contract as [`RouteBucket::id`]).
    fn id(&self) -> &str;

    /// Owned-id escape hatch (same contract as [`RouteBucket::id_owned`]).
    fn id_owned(&self) -> String {
        self.id().to_string()
    }
}

/// Bridge impl: every [`RouteBucket`] is automatically a
/// [`RouteBucketTyped`] with `Kind = OdooMethodKind`. This is the back-compat
/// seam — existing consumers that only know about `RouteBucket` continue to
/// work, and new generic code can take `RouteBucketTyped` and accept both.
///
/// [impl_blanket]: #impl-RouteBucketTyped-for-T
impl<T: RouteBucket> RouteBucketTyped for T {
    type Kind = OdooMethodKind;

    fn kind(&self) -> OdooMethodKind {
        RouteBucket::kind(self)
    }

    fn id(&self) -> &str {
        RouteBucket::id(self)
    }

    fn id_owned(&self) -> String {
        RouteBucket::id_owned(self)
    }
}

// ---------------------------------------------------------------------------
// ③ WidgetRender — the askama GUI shape contract
// ---------------------------------------------------------------------------

/// A widget template's render contract: takes a bucket, emits rendered text.
///
/// Implementations are expected to be `askama::Template`-derived structs;
/// `render` then delegates to the askama-generated method. The bucket
/// argument is the *only* input — widgets must not peek at raw triples or
/// re-implement the bucket classification.
pub trait WidgetRender<B: RouteBucket> {
    /// Render the widget against the given bucket. Errors propagate from
    /// the askama layer (template-not-found, IO, etc.).
    fn render(bucket: &B) -> Result<String, WidgetRenderError>;
}

/// Errors from widget rendering. Kept opaque + small per zero-dep ethos;
/// concrete `askama::Error` lives at the consumer crate.
#[derive(Debug, Clone)]
pub struct WidgetRenderError {
    pub message: String,
}

impl fmt::Display for WidgetRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for WidgetRenderError {}

// ---------------------------------------------------------------------------
// ④ Genericity — what to codegen vs what to read from the triple store
// ---------------------------------------------------------------------------

/// Whether a piece of behaviour is agnostic (read triples at runtime) or
/// domain-specific (codegen as a const).
///
/// This is a marker, not an enforcement primitive — it tells the next
/// session whether to generalise a piece of logic or duplicate it. Mark
/// every new emitter with one of these in its docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Genericity {
    /// The behaviour is identical across addons / domains. Reads triples
    /// at runtime, no codegen. Examples: dependency-graph traversal,
    /// recompute fan-out, validation cascade.
    Agnostic,
    /// The behaviour is domain-specific. Codegen as a `const`. Examples:
    /// SKR04 chart of accounts, UStG §12 tax rate, ISO 3166 country codes.
    Domain,
}

// ---------------------------------------------------------------------------
// Tests — the four traits compile + work end-to-end on a tiny triple set
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference projection: index triples by predicate, lossless by
    /// construction (just shuffles the same data into a Vec-per-predicate
    /// map). Tests that the round-trip framework itself works.
    struct PredicateIndex;

    impl TripletProjection for PredicateIndex {
        type Const = std::collections::BTreeMap<String, Vec<Triple>>;

        fn project(triples: &[Triple]) -> Self::Const {
            let mut m: std::collections::BTreeMap<String, Vec<Triple>> = Default::default();
            for t in triples {
                m.entry(t.p.clone()).or_default().push(t.clone());
            }
            m
        }

        fn decompile(c: &Self::Const) -> Vec<Triple> {
            c.values().flatten().cloned().collect()
        }
    }

    /// Lossy projection (drops the `f` field) — must fail round-trip when
    /// `truth_tolerance` is 0.
    struct LossyDropFrequency;

    impl TripletProjection for LossyDropFrequency {
        type Const = Vec<(String, String, String, f32)>;

        fn project(triples: &[Triple]) -> Self::Const {
            triples
                .iter()
                .map(|t| (t.s.clone(), t.p.clone(), t.o.clone(), t.c))
                .collect()
        }

        fn decompile(c: &Self::Const) -> Vec<Triple> {
            c.iter()
                .map(|(s, p, o, c2)| Triple {
                    s: s.clone(),
                    p: p.clone(),
                    o: o.clone(),
                    f: 0.0, // <-- the loss
                    c: *c2,
                })
                .collect()
        }

        fn truth_tolerance() -> f32 {
            0.01 // strict; the dropped frequency will exceed this
        }
    }

    fn fixture() -> Vec<Triple> {
        vec![
            Triple {
                s: "odoo:account_move".into(),
                p: "rdf:type".into(),
                o: "ogit:ObjectType".into(),
                f: 1.0,
                c: 1.0,
            },
            Triple {
                s: "odoo:account_move.amount_total".into(),
                p: "emitted_by".into(),
                o: "odoo:account_move._compute_amount".into(),
                f: 0.95,
                c: 0.90,
            },
            Triple {
                s: "odoo:account_move.amount_total".into(),
                p: "depends_on".into(),
                o: "odoo:account_move.line_ids.balance".into(),
                f: 0.95,
                c: 0.90,
            },
        ]
    }

    #[test]
    fn lossless_projection_passes_roundtrip() {
        let input = fixture();
        roundtrip_eq::<PredicateIndex>(&input).expect("predicate-index projection is lossless");
    }

    #[test]
    fn lossy_projection_fails_roundtrip() {
        let input = fixture();
        let result = roundtrip_eq::<LossyDropFrequency>(&input);
        // Identity is preserved (s,p,o all match), so `missing/extraneous` is 0;
        // the failure is on the truth tolerance check.
        match result {
            Err(failure) => {
                // The failure should name the lossy projection.
                assert!(failure.projection.contains("LossyDropFrequency"));
            }
            Ok(()) => panic!("LossyDropFrequency should fail truth-tolerance check"),
        }
    }

    /// Identity-preserving but truth-mangling projection — every (s,p,o)
    /// round-trips, but (f, c) come back as (0.0, 0.0). With the default
    /// `truth_tolerance() = 0.0`, this MUST fail the roundtrip check.
    struct TruthMangler;

    impl TripletProjection for TruthMangler {
        type Const = Vec<(String, String, String)>;

        fn project(triples: &[Triple]) -> Self::Const {
            triples
                .iter()
                .map(|t| (t.s.clone(), t.p.clone(), t.o.clone()))
                .collect()
        }

        fn decompile(c: &Self::Const) -> Vec<Triple> {
            c.iter()
                .map(|(s, p, o)| Triple {
                    s: s.clone(),
                    p: p.clone(),
                    o: o.clone(),
                    f: 0.0,
                    c: 0.0,
                })
                .collect()
        }
        // No truth_tolerance() override — default 0.0.
    }

    #[test]
    fn default_tolerance_requires_exact_truth_match() {
        let input = fixture();
        let result = roundtrip_eq::<TruthMangler>(&input);
        // Default tolerance is 0.0 → must reject any truth mismatch
        // (input has f=1.0 / 0.95, decompiled has f=0.0).
        match result {
            Err(failure) => {
                assert!(failure.projection.contains("TruthMangler"));
            }
            Ok(()) => {
                panic!("TruthMangler must fail with default tolerance 0.0 (truth values differ)");
            }
        }
    }

    #[test]
    fn odoo_method_kind_ids_are_unique_and_stable() {
        let mut seen = BTreeSet::new();
        for k in OdooMethodKind::ALL {
            assert!(seen.insert(k.id()), "duplicate id: {}", k.id());
            // Round-trip through from_id.
            assert_eq!(OdooMethodKind::from_id(k.id()), Some(k));
        }
        assert_eq!(seen.len(), 16);
    }

    #[test]
    fn route_bucket_trait_compiles_with_concrete_impl() {
        struct ConcreteRoute {
            id: String,
            kind: OdooMethodKind,
        }
        impl RouteBucket for ConcreteRoute {
            fn kind(&self) -> OdooMethodKind {
                self.kind
            }
            fn id(&self) -> &str {
                &self.id
            }
        }

        let r = ConcreteRoute {
            id: "account.move._compute_amount".into(),
            kind: OdooMethodKind::IterRecordsAggregateRelation,
        };
        // UFCS disambiguation: both `RouteBucket::kind` and (via the C6
        // blanket impl) `RouteBucketTyped::kind` are in scope here through
        // `use super::*`; downstream consumers that import only one trait
        // do NOT need this. Semantics unchanged.
        assert_eq!(RouteBucket::kind(&r).id(), "iter_records_aggregate_relation");
        assert_eq!(RouteBucket::id(&r), "account.move._compute_amount");
    }

    #[test]
    fn widget_render_trait_compiles_with_dummy_impl() {
        struct DummyRoute(OdooMethodKind);
        impl RouteBucket for DummyRoute {
            fn kind(&self) -> OdooMethodKind {
                self.0
            }
            fn id(&self) -> &str {
                "dummy"
            }
        }

        struct DummyWidget;
        impl WidgetRender<DummyRoute> for DummyWidget {
            fn render(bucket: &DummyRoute) -> Result<String, WidgetRenderError> {
                // UFCS — see disambiguation note above.
                Ok(format!("widget for kind={}", RouteBucket::kind(bucket)))
            }
        }

        let r = DummyRoute(OdooMethodKind::PassOverride);
        let out = DummyWidget::render(&r).expect("render");
        assert_eq!(out, "widget for kind=pass_override");
    }

    #[test]
    fn genericity_marker_distinguishes_codegen_targets() {
        // Agnostic example: dependency-graph traversal reads triples,
        // identical across all addons.
        let dep_traversal = Genericity::Agnostic;
        // Domain example: SKR04 chart of accounts gets codegen'd per Odoo
        // l10n_de addon.
        let skr04 = Genericity::Domain;
        assert_ne!(dep_traversal, skr04);
        assert_eq!(dep_traversal, Genericity::Agnostic);
        assert_eq!(skr04, Genericity::Domain);
    }

    // -----------------------------------------------------------------------
    // RouteBucketTyped — kind-generic sibling trait (additive, non-Odoo
    // targets bring their own kind enum)
    // -----------------------------------------------------------------------

    /// A non-Odoo target's handler-kind taxonomy. Stand-in for, e.g.,
    /// OpenProject's `list_for_tenant` / `detail_for_tenant` /
    /// `template_get` / `csrf_form_post_engine_call` set. Used here only to
    /// exercise that `RouteBucketTyped` accepts an arbitrary `Kind`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum OpKindFixture {
        ListForTenant,
        DetailForTenant,
        TemplateGet,
    }

    /// An OP-style bucket: impls **only** `RouteBucketTyped`, NOT
    /// `RouteBucket`. Proves a non-Odoo target plugs in additively without
    /// touching the legacy trait or its enum.
    struct OpBucket {
        kind: OpKindFixture,
        id: String,
    }

    impl RouteBucketTyped for OpBucket {
        type Kind = OpKindFixture;
        fn kind(&self) -> OpKindFixture {
            self.kind
        }
        fn id(&self) -> &str {
            &self.id
        }
    }

    #[test]
    fn route_bucket_typed_accepts_non_odoo_kind() {
        let b = OpBucket {
            kind: OpKindFixture::ListForTenant,
            id: "projects.list_work_packages".to_string(),
        };
        assert_eq!(b.kind(), OpKindFixture::ListForTenant);
        assert_eq!(b.id(), "projects.list_work_packages");
        assert_eq!(b.id_owned(), "projects.list_work_packages");
    }

    #[test]
    fn route_bucket_typed_generic_consumer_accepts_op_kind() {
        // A generic consumer parameterised on the kind enum compiles + runs
        // for the OP kind — the whole point of the additive trait.
        fn dispatch_one<B: RouteBucketTyped<Kind = OpKindFixture>>(b: &B) -> &'static str {
            match b.kind() {
                OpKindFixture::ListForTenant => "list",
                OpKindFixture::DetailForTenant => "detail",
                OpKindFixture::TemplateGet => "template",
            }
        }
        let b = OpBucket {
            kind: OpKindFixture::DetailForTenant,
            id: "projects.get_work_package".to_string(),
        };
        assert_eq!(dispatch_one(&b), "detail");
    }

    /// A back-compat Odoo bucket: impls `RouteBucket` only. The blanket impl
    /// MUST make it usable as `RouteBucketTyped<Kind = OdooMethodKind>`
    /// without any additional code.
    struct OdooBucketCompat {
        kind: OdooMethodKind,
        id: &'static str,
    }
    impl RouteBucket for OdooBucketCompat {
        fn kind(&self) -> OdooMethodKind {
            self.kind
        }
        fn id(&self) -> &str {
            self.id
        }
    }

    #[test]
    fn route_bucket_blanket_impl_preserves_odoo_consumers() {
        let b = OdooBucketCompat {
            kind: OdooMethodKind::PassOverride,
            id: "account.move._compute_amount",
        };
        // Direct RouteBucket access — unchanged from before C6.
        assert_eq!(RouteBucket::kind(&b), OdooMethodKind::PassOverride);
        assert_eq!(RouteBucket::id(&b), "account.move._compute_amount");
        // Same bucket, via the new RouteBucketTyped trait — the blanket impl
        // pins Kind = OdooMethodKind so this resolves without any extra impl.
        let typed: &dyn RouteBucketTyped<Kind = OdooMethodKind> = &b;
        assert_eq!(typed.kind(), OdooMethodKind::PassOverride);
        assert_eq!(typed.id(), "account.move._compute_amount");
        assert_eq!(typed.id_owned(), "account.move._compute_amount");
    }

    #[test]
    fn route_bucket_typed_generic_consumer_accepts_odoo_via_blanket() {
        // A generic consumer parameterised on `OdooMethodKind` accepts an
        // implementor that only knows about `RouteBucket` — proving the
        // blanket impl is the back-compat bridge, not just for show.
        fn name<B: RouteBucketTyped<Kind = OdooMethodKind>>(b: &B) -> &'static str {
            b.kind().id()
        }
        let b = OdooBucketCompat {
            kind: OdooMethodKind::IterRecordsComputeFromRelated,
            id: "x.y",
        };
        assert_eq!(name(&b), "iter_records_compute_from_related");
    }
}
