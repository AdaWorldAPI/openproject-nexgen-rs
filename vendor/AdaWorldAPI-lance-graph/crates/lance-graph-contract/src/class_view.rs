//! # `class_view` — the class as a META lookup that flies ABOVE the SoA.
//!
//! ## The XML-parse framing
//!
//! Today OGIT (`lance-graph-ontology::OntologyRegistry`) is a **hashtable doing
//! single lookups**: `uri → row`, `entity_type_id → row` — one key, one value,
//! O(1), leaf. That is the *single* lookup. What a class needs is a **meta
//! lookup**: `class_id → the whole shape` — the ordered field set, labels,
//! template, and the presence-bit basis. The class composes many leaf lookups
//! into one shape — the way an XSD schema composes element declarations.
//!
//! ```text
//!   SoA row          =  the XML document   (agnostic bytes, no meaning)
//!   class / ObjectView =  the XSD schema     (the shape: which fields, in order)
//!   ClassView (this) =  the parser+schema  (projects row → typed view, late-bound)
//!   FieldMask        =  which optional elements are present  (structural)
//!   askama template  =  the XSLT            (renders the projected view)
//! ```
//!
//! ## Classes fly as a meta-DTO ABOVE the SoA — the SoA stays agnostic
//!
//! The load-bearing rule (`cognitive-risc-classes.md`:39 "the meta-DTO resolves;
//! it does not store"; `cognitive-risc-core.md` invariant #1 "nothing semantic in
//! the register file"): the SoA row carries **only** `class_id` + a presence
//! [`FieldMask`] + agnostic columns. **Zero labels in the bytes.** The
//! labels / template / DOLCE-category are resolved *at projection time* by the
//! flying meta-DTO from the OGIT cache — never hand-rolled onto the row.
//!
//! That makes the presence/semantics split (C2) fall out for free:
//! - **bit = presence** — structural, lives on the SoA ("field N is populated").
//! - **bit → field → label → template** — semantic resolution, lives in the
//!   meta-DTO *above* the SoA. A bit NEVER means "field N behaves differently."
//!
//! ## Layering (dependency inversion, same shape as `MailboxSoaView`)
//!
//! - **contract (here, zero-dep):** the agnostic surface — [`FieldMask`] presence
//!   bits + the [`ClassView`] resolver *trait*. Extends the existing
//!   [`crate::ontology::ObjectView`] (the per-class ordered field set = the bit
//!   basis), does not duplicate it.
//! - **ontology (one layer up):** *implements* [`ClassView`] — the "parser" that
//!   walks the class shape and resolves labels late from the OGIT hashmap.
//! - **render (a consumer):** reads the projected view + mask, picks the askama
//!   template, skips off-bits.

use crate::ontology::{DisplayTemplate, FieldRef};

/// Per-row class discriminator — the Cognitive-RISC `class_id` / `shape_id`.
///
/// A `u16` (≤ 65,535 shape-families; OD-CLASSID-WIDTH ratified). It is a
/// *discriminator*, never a content hash — it stays OUTSIDE the CAM identity
/// layer (`I-VSA-IDENTITIES`: never hashed-as-content, never superposed). Reuses
/// the width of the existing [`crate::soa_view::MailboxSoaView::class_id`] accessor.
pub type ClassId = u16;

/// A class's **presence bitmask** — one bit per field of its class
/// [`ObjectView`](crate::ontology::ObjectView), set iff that field is populated
/// on a given instance.
///
/// The instance's *delta from its class* (`cognitive-risc-classes.md`:48), as
/// **pure presence bits**. Bit position `N` = the `N`-th field in the class's
/// ordered field list — stable + append-only (N3): once instances persist, a
/// field's bit position never moves and retired bits are never reused. Zero-dep
/// (`u64`, no `bitflags`); mask width is bounded by the *class's* field count
/// (dozens), never the entity union.
///
/// **Presence, NEVER semantics (C2).** `has(n)` answers "is field n populated
/// here"; it must never gate "field n means something different here."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FieldMask(pub u64);

impl FieldMask {
    /// The empty mask (no fields populated).
    pub const EMPTY: Self = Self(0);

    /// Maximum addressable field positions in one `u64` mask.
    pub const MAX_FIELDS: u32 = 64;

    /// Build a mask from the populated field positions. Positions `>= MAX_FIELDS`
    /// (64) are **ignored** — NOT folded onto a valid bit. Folding (`& 63`) would
    /// alias position 64 onto bit 0 and silently corrupt the presence contract for
    /// an oversized class shape (Codex P2 on #441); ignoring keeps the no-panic
    /// property without misrepresenting which fields are present.
    pub const fn from_positions(positions: &[u8]) -> Self {
        let mut bits = 0u64;
        let mut i = 0;
        while i < positions.len() {
            if (positions[i] as u32) < Self::MAX_FIELDS {
                bits |= 1u64 << positions[i];
            }
            i += 1;
        }
        Self(bits)
    }

    /// Set field position `n` as populated. `n >= MAX_FIELDS` (64) is a no-op
    /// (NOT folded — see [`from_positions`](FieldMask::from_positions)).
    #[inline]
    pub const fn with(self, n: u8) -> Self {
        if (n as u32) < Self::MAX_FIELDS {
            Self(self.0 | (1u64 << n))
        } else {
            self
        }
    }

    /// Is field position `n` populated? (presence — C2). `n >= MAX_FIELDS` (64) is
    /// always `false` — an out-of-range field is never "present" (NOT folded onto
    /// a valid bit).
    #[inline]
    pub const fn has(self, n: u8) -> bool {
        (n as u32) < Self::MAX_FIELDS && self.0 & (1u64 << n) != 0
    }

    /// Number of populated fields.
    #[inline]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Is nothing populated?
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The full mask — every addressable field position present. The
    /// "no projection constraint" default for an RBAC role that has not
    /// narrowed its view (lance-graph-rbac `PermissionSpec::projection`).
    pub const FULL: Self = Self(u64::MAX);

    /// Bitwise intersection — the field positions present in BOTH masks.
    #[inline]
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Bitwise union — the field positions present in EITHER mask. The fold an
    /// RBAC kernel uses to combine the projections a user's several granting
    /// roles each permit (a user sees the union of the columns any of their
    /// roles may see).
    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Do the two masks share NO field position? RBAC uses this to assert
    /// two roles project **distinct** views of the same class — e.g. a
    /// research projection must be disjoint from the identifier fields
    /// (`classid :: role :: membership`, where the role is the projection).
    #[inline]
    pub const fn is_disjoint(self, other: Self) -> bool {
        self.0 & other.0 == 0
    }

    /// Inherit a parent class's presence into this mask — the **mask-inherits-as-
    /// delta** of the HHTL `subClassOf` walk (`wikidata-hhtl-load.md`). A child
    /// IS-A its parent, so its mask carries every field the parent declares
    /// present PLUS its own `delta`: a bitwise union. N3 stable positions mean the
    /// parent's bits never move — the child only adds (multi-parent
    /// "flying-family" facets are orthogonal bits in this same mask, never a
    /// second path). Read `parent.inherit(own_delta)` → the child's full mask;
    /// the union is commutative, so the direction is documentation, not a
    /// constraint. See [`crate::hhtl`].
    #[inline]
    pub const fn inherit(self, delta: FieldMask) -> FieldMask {
        FieldMask(self.0 | delta.0)
    }
}

/// One recompute edge in a class's **compute DAG**: field position `target` is
/// (re)computed from the field positions in `inputs`.
///
/// Harvest-sourced — `target` is the `emitted_by` field, `inputs` are its
/// `depends_on` precedents (Odoo `@api.depends`, an Excel formula's referenced
/// cells, a chess-eval feature's inputs). All fields are `&'static` so a
/// generated `const DAG: &[ComputeEdge] = &[..]` compiles (the harvest IS the
/// manifest — mirrors [`crate::codegen_manifest::MethodSig`] /
/// [`crate::action::ActionDef`]). Positions index the class's [`FieldMask`]
/// (0..[`FieldMask::MAX_FIELDS`]), matching [`ClassView::fields`].
///
/// This is the Core home for recompute *dispatch* (`E-EXCEL-SHADER-PROJECTION` /
/// `probe-excel-compute-dag-v1`): the manifest lives ABOVE the SoA (resolution
/// metadata, stores nothing on the row); no adapter carries its own `@api.depends`
/// table (`core-first-transcode-doctrine` — that would be the Adapter-State-Leak).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputeEdge {
    /// Field position this edge recomputes (the `emitted_by` target).
    pub target: u8,
    /// Field positions this target reads (its `depends_on` precedents).
    pub inputs: &'static [u8],
}

/// Whether a class's `compute_dag` is **acyclic** — the registry-build gate.
///
/// A cyclic recompute DAG (a formula loop `A=B+1, B=A+1`, or a `@api.depends`
/// cycle) MUST be rejected at registry-build: it has no topological order and
/// would never converge. Returns `false` on any cycle (incl. a self-loop
/// `target ∈ inputs`). Considers only positions `< FieldMask::MAX_FIELDS`;
/// out-of-range targets/inputs are ignored (no panic, mirrors
/// [`FieldMask::from_positions`]). Allocation-free (≤ 64 positions).
#[must_use]
pub fn compute_dag_is_acyclic(edges: &[ComputeEdge]) -> bool {
    const N: usize = FieldMask::MAX_FIELDS as usize; // 64
                                                     // deps[t] = bitmask of in-range positions that target `t` depends on.
    let mut deps = [0u64; N];
    let mut is_target = 0u64;
    for e in edges {
        if (e.target as usize) >= N {
            continue;
        }
        is_target |= 1u64 << e.target;
        for &inp in e.inputs {
            if (inp as usize) < N {
                deps[e.target as usize] |= 1u64 << inp;
            }
        }
    }
    // Kahn: leaves (non-targets) are resolved; peel any target whose deps are all
    // resolved. If a round makes no progress with targets remaining → cycle.
    let mut resolved = !is_target;
    let mut remaining = is_target;
    loop {
        if remaining == 0 {
            return true;
        }
        let mut progressed = false;
        let mut r = remaining;
        while r != 0 {
            let t = r.trailing_zeros();
            r &= r - 1;
            if deps[t as usize] & !resolved == 0 {
                resolved |= 1u64 << t;
                remaining &= !(1u64 << t);
                progressed = true;
            }
        }
        if !progressed {
            return false; // stuck with targets remaining → cycle
        }
    }
}

/// A valid **recompute order** for a class's `compute_dag` — the target field
/// positions in an order where every target appears after all targets it
/// (transitively) depends on. `None` if the DAG is cyclic (no topological order).
///
/// Leaves (positions that are only inputs, never targets) are not included — they
/// are the already-present values a recompute reads. Within one Kahn round the
/// resolved targets are mutually independent, so any order among them is valid.
/// The consumer recomputes targets in this order, each gated by the cycle-aware
/// `write_row` (`probe-excel-compute-dag-v1` Inc 2). Allocation = one `Vec` of
/// the target positions (≤ 64).
#[must_use]
pub fn compute_dag_topo_order(edges: &[ComputeEdge]) -> Option<Vec<u8>> {
    const N: usize = FieldMask::MAX_FIELDS as usize; // 64
    let mut deps = [0u64; N];
    let mut is_target = 0u64;
    for e in edges {
        if (e.target as usize) >= N {
            continue;
        }
        is_target |= 1u64 << e.target;
        for &inp in e.inputs {
            if (inp as usize) < N {
                deps[e.target as usize] |= 1u64 << inp;
            }
        }
    }
    let mut resolved = !is_target;
    let mut remaining = is_target;
    let mut order = Vec::with_capacity(is_target.count_ones() as usize);
    loop {
        if remaining == 0 {
            return Some(order);
        }
        let mut progressed = false;
        let mut r = remaining;
        while r != 0 {
            let t = r.trailing_zeros();
            r &= r - 1;
            if deps[t as usize] & !resolved == 0 {
                resolved |= 1u64 << t;
                remaining &= !(1u64 << t);
                order.push(t as u8);
                progressed = true;
            }
        }
        if !progressed {
            return None; // cycle
        }
    }
}

/// The class as a **meta lookup that flies above the SoA** — the resolver trait.
///
/// An implementor (in `lance-graph-ontology`, over the OGIT cache) is the
/// "parser+schema": given a `class_id` it resolves the class's ordered field set,
/// labels, DOLCE category, and render template — all LATE-bound from the cache,
/// none stored on the SoA row. The contract owns only the *vocabulary*; the cache
/// owns the *answers* (dependency inversion, like `PlannerContract`/`MailboxSoaView`).
///
/// "Single lookup" (leaf, today) vs "meta lookup" (the class, this trait): a
/// single lookup is `uri → row`; a meta lookup is `class_id → shape`, composing
/// many leaf lookups into one projected view.
pub trait ClassView {
    /// The class's ordered field set — the bit basis. Position `i` in this slice
    /// is the stable [`FieldMask`] bit `i` (N3 append-only). This IS the
    /// per-class [`ObjectView`](crate::ontology::ObjectView)'s `fields`.
    fn fields(&self, class: ClassId) -> &[FieldRef];

    /// Which askama template renders this class.
    fn template(&self, class: ClassId) -> DisplayTemplate;

    /// The DOLCE upper-category of this class, RESOLVED from the ontology cache
    /// (not a stored enum on the row — OD-DOLCE "use the ontology cache"). Returned
    /// as the cache's opaque category id; the consumer maps it to its own enum.
    fn dolce_category_id(&self, class: ClassId) -> u8;

    /// The label of field position `n` in `class`, resolved late from the cache
    /// (locale resolution is the consumer's job). `None` if `n` is out of range.
    fn field_label(&self, class: ClassId, n: u8) -> Option<&str> {
        self.fields(class).get(n as usize).map(|f| f.label.as_str())
    }

    /// The class's field count (mask width). Must be `<= FieldMask::MAX_FIELDS`.
    #[inline]
    fn field_count(&self, class: ClassId) -> usize {
        self.fields(class).len()
    }

    /// Project an instance: iterate `(field, populated?)` pairs in class order,
    /// gating each field by the presence `mask`. This is the render surface — the
    /// consumer skips off-bits (`cognitive-risc-classes.md`:49). The SoA supplied
    /// only `(class, mask)`; the labels come from the cache, above the SoA.
    fn project<'a>(&'a self, class: ClassId, mask: FieldMask) -> ClassProjection<'a> {
        ClassProjection {
            fields: self.fields(class),
            mask,
            pos: 0,
        }
    }

    /// The **render rows** for an instance: only the populated `(label, predicate)`
    /// pairs, off-bits skipped (`cognitive-risc-classes.md`:49). This is the
    /// template-agnostic render surface — an askama/jinja per-class template iterates
    /// these rows; the engine choice (F3, askama) lives in the deferred render crate.
    ///
    /// Presence-only (C2): a row appears iff its bit is set; the mask NEVER changes a
    /// row's meaning, only its presence. The labels are the meta-DTO's late resolution
    /// (above the SoA), the mask is the SoA's structural delta.
    fn render_rows<'a>(&'a self, class: ClassId, mask: FieldMask) -> Vec<RenderRow<'a>> {
        self.project(class, mask)
            .filter(|(_, present)| *present)
            .map(|(f, _)| RenderRow {
                label: f.label.as_str(),
                predicate: f.predicate_iri.as_str(),
            })
            .collect()
    }

    /// Which edge-codec flavor this class reads its node edge block with.
    ///
    /// Default is
    /// [`EdgeCodecFlavor::CoarseOnly`](crate::canonical_node::EdgeCodecFlavor::CoarseOnly)
    /// — the canon zero-fallback reading (each edge byte is a palette index). An
    /// implementor overrides this to let a class opt into residue or PQ fidelity.
    /// This is *selection only*: every flavor shares the SAME byte layout, so the
    /// choice never changes `NODE_ROW_STRIDE` (canon "registry-resolved via
    /// `classid → ClassView`", never a stride change).
    #[inline]
    fn edge_codec_flavor(&self, _class: ClassId) -> crate::canonical_node::EdgeCodecFlavor {
        crate::canonical_node::EdgeCodecFlavor::CoarseOnly
    }

    /// Which value-slab schema preset this class materialises in
    /// [`NodeRow::value`](crate::canonical_node::NodeRow::value).
    ///
    /// **TEMPORARY (POC default, 2026-06-15):** returns
    /// [`ValueSchema::Full`](crate::canonical_node::ValueSchema::Full) — every
    /// *unconfigured* class (incl. the default `classid 0x0000_0000`) materialises
    /// the whole value slab so downstream consumers (tesseract-rs / woa-rs /
    /// medcare-rs / q2) can transcode against a fully-populated `NodeRow` POC.
    /// Specialisation is **opt-IN, not opt-out**: a consumer that needs to save
    /// memory mints a class that overrides this to a smaller preset (`Cognitive` /
    /// `Compressed` / `Bootstrap`); a consumer that needs denser/specialised data
    /// mints a *separate* class. Selection only: every preset carves within the
    /// reserved 480-byte value slab, so the choice never changes `NODE_ROW_STRIDE`
    /// (canon "registry-resolved via `classid → ClassView`", never a stride change)
    /// — flipping the default is layout-preserving and a one-line revert to
    /// `Bootstrap` (the canon zero-fallback) before merge. The type-level
    /// [`ValueSchema::default()`](crate::canonical_node::ValueSchema) stays
    /// `Bootstrap`, so the substrate zero-fallback semantics are untouched; only
    /// the class→schema *resolution* default is Full.
    #[inline]
    fn value_schema(&self, _class: ClassId) -> crate::canonical_node::ValueSchema {
        // TEMPORARY POC default — see doc above. Revert to `ValueSchema::Bootstrap`
        // (canon zero-fallback) before merge. No invention: `Full` activates the
        // already-existing, already-tested 9 ValueTenants (helix-48 / turbovec /
        // signed / fingerprint / …), it adds no new property.
        crate::canonical_node::ValueSchema::Full
    }

    /// The class's **recompute DAG** — the topological manifest of which fields
    /// recompute from which (the `emitted_by` + `depends_on` harvest), the Core
    /// home for computed-field dispatch (Odoo `@api.depends`, Excel formulas,
    /// chess-eval features; `probe-excel-compute-dag-v1`).
    ///
    /// Default `&[]` — the zero-fallback: an unconfigured class has no computed
    /// fields (mirrors `compute_dag`'s no-panic siblings). An implementor returns
    /// a generated `const &[ComputeEdge]`; the registry MUST validate it with
    /// [`compute_dag_is_acyclic`] at build (a cyclic DAG is rejected, never
    /// recomputed). Layout-preserving: resolution metadata above the SoA, stores
    /// nothing on the row, never a `NODE_ROW_STRIDE`/`ENVELOPE_LAYOUT_VERSION`
    /// change. The instance recompute that consumes this is gated per-cell by the
    /// cycle-aware `write_row` (`E-SOA-CYCLE-OWNERSHIP`).
    #[inline]
    fn compute_dag(&self, _class: ClassId) -> &[ComputeEdge] {
        &[]
    }
}

/// One populated field to render — the late-resolved `label` + its `predicate` key.
/// Produced only for set bits (off-bits are skipped), so a template never branches
/// on presence (C2): it just iterates the rows it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderRow<'a> {
    /// The display label, resolved late from the OGIT cache (above the SoA).
    pub label: &'a str,
    /// The field's predicate IRI (the stable key behind the label).
    pub predicate: &'a str,
}

/// An iterator over a class's fields paired with their presence bit — the
/// projected view a render template consumes (off-bits are still yielded with
/// `present = false` so the template can `{% if present %}`-skip them).
pub struct ClassProjection<'a> {
    fields: &'a [FieldRef],
    mask: FieldMask,
    pos: usize,
}

impl<'a> Iterator for ClassProjection<'a> {
    /// `(field, present)` — `present` is the C2 presence bit, never a semantics bit.
    type Item = (&'a FieldRef, bool);

    fn next(&mut self) -> Option<Self::Item> {
        let f = self.fields.get(self.pos)?;
        let present = self.mask.has(self.pos as u8);
        self.pos += 1;
        Some((f, present))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{DisplayTemplate, FieldRef};

    /// A tiny in-contract ClassView fake — proves the trait is satisfiable and the
    /// meta-DTO projects above an agnostic (class, mask) input, no labels stored.
    struct FakeClasses {
        // class 7 = a 3-field shape ("invoice": amount, tax, partner)
        invoice: Vec<FieldRef>,
    }

    impl FakeClasses {
        fn new() -> Self {
            Self {
                invoice: vec![
                    FieldRef::new("amount_total", "Total"),
                    FieldRef::new("amount_tax", "Tax"),
                    FieldRef::new("partner_id", "Partner"),
                ],
            }
        }
    }

    impl ClassView for FakeClasses {
        fn fields(&self, class: ClassId) -> &[FieldRef] {
            match class {
                7 => &self.invoice,
                _ => &[],
            }
        }
        fn template(&self, _class: ClassId) -> DisplayTemplate {
            DisplayTemplate::Detail
        }
        fn dolce_category_id(&self, _class: ClassId) -> u8 {
            0 // Endurant, resolved from the cache in the real impl
        }
    }

    // ── compute_dag (probe-excel-compute-dag-v1, Inc 0) ──────────────────────

    /// Default `compute_dag` is the zero-fallback empty manifest (no computed
    /// fields for an unconfigured class).
    #[test]
    fn compute_dag_default_is_empty() {
        let c = FakeClasses { invoice: vec![] };
        assert!(c.compute_dag(7).is_empty());
        assert!(c.compute_dag(0).is_empty());
    }

    /// `const`-constructible manifest — the exact shape a generated
    /// `const DAG: &[ComputeEdge]` emits (a chain: f2 = g(f1), f1 = h(f0)).
    const SAMPLE_DAG: &[ComputeEdge] = &[
        ComputeEdge {
            target: 1,
            inputs: &[0],
        },
        ComputeEdge {
            target: 2,
            inputs: &[1],
        },
    ];

    #[test]
    fn compute_dag_acyclic_chain_passes() {
        assert!(
            compute_dag_is_acyclic(SAMPLE_DAG),
            "a dependency chain f0→f1→f2 is acyclic"
        );
        assert!(compute_dag_is_acyclic(&[]), "empty dag is acyclic");
        // a target reading a non-computed leaf is fine
        assert!(compute_dag_is_acyclic(&[ComputeEdge {
            target: 5,
            inputs: &[3, 4]
        }]));
    }

    #[test]
    fn compute_dag_cycle_is_rejected() {
        // f0 = g(f1), f1 = h(f0) — a 2-cycle, no topological order.
        let two_cycle = &[
            ComputeEdge {
                target: 0,
                inputs: &[1],
            },
            ComputeEdge {
                target: 1,
                inputs: &[0],
            },
        ];
        assert!(
            !compute_dag_is_acyclic(two_cycle),
            "a formula loop must be rejected at registry-build"
        );
        // self-loop f0 = g(f0)
        assert!(!compute_dag_is_acyclic(&[ComputeEdge {
            target: 0,
            inputs: &[0]
        }]));
        // 3-cycle f0→f1→f2→f0
        assert!(!compute_dag_is_acyclic(&[
            ComputeEdge {
                target: 1,
                inputs: &[0]
            },
            ComputeEdge {
                target: 2,
                inputs: &[1]
            },
            ComputeEdge {
                target: 0,
                inputs: &[2]
            },
        ]));
    }

    #[test]
    fn compute_dag_out_of_range_positions_ignored() {
        // target/inputs >= MAX_FIELDS (64) are ignored, never folded → no panic,
        // no false cycle (mirrors FieldMask::from_positions).
        assert!(compute_dag_is_acyclic(&[
            ComputeEdge {
                target: 64,
                inputs: &[0]
            }, // ignored target
            ComputeEdge {
                target: 5,
                inputs: &[200]
            }, // input ignored → leaf-only target
        ]));
    }

    #[test]
    fn compute_dag_topo_order_respects_dependencies() {
        // chain f0→f1→f2: f1 must come before f2; f0 is a leaf, not emitted.
        let order = compute_dag_topo_order(SAMPLE_DAG).expect("acyclic has an order");
        assert_eq!(
            order.len(),
            2,
            "two targets (f1, f2); f0 is a read-only leaf"
        );
        let pos1 = order.iter().position(|&t| t == 1).unwrap();
        let pos2 = order.iter().position(|&t| t == 2).unwrap();
        assert!(pos1 < pos2, "f1 recomputed before its dependent f2");
        // empty manifest → empty order, not None.
        assert_eq!(compute_dag_topo_order(&[]), Some(vec![]));
    }

    #[test]
    fn compute_dag_topo_order_none_on_cycle() {
        // a 2-cycle has no topological order — None, matching is_acyclic == false.
        let two_cycle = &[
            ComputeEdge {
                target: 0,
                inputs: &[1],
            },
            ComputeEdge {
                target: 1,
                inputs: &[0],
            },
        ];
        assert!(compute_dag_topo_order(two_cycle).is_none());
        assert!(!compute_dag_is_acyclic(two_cycle));
    }

    #[test]
    fn compute_dag_topo_order_diamond() {
        // f3 = g(f1, f2); f1 = h(f0); f2 = k(f0). f0 leaf. f1,f2 before f3.
        let diamond = &[
            ComputeEdge {
                target: 1,
                inputs: &[0],
            },
            ComputeEdge {
                target: 2,
                inputs: &[0],
            },
            ComputeEdge {
                target: 3,
                inputs: &[1, 2],
            },
        ];
        let order = compute_dag_topo_order(diamond).expect("acyclic");
        let p = |t: u8| order.iter().position(|&x| x == t).unwrap();
        assert!(
            p(1) < p(3) && p(2) < p(3),
            "both precedents before the join"
        );
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn field_mask_is_presence_bits() {
        let m = FieldMask::from_positions(&[0, 2]); // amount + partner populated, tax absent
        assert!(m.has(0) && !m.has(1) && m.has(2));
        assert_eq!(m.count(), 2);
        assert!(!m.is_empty() && FieldMask::EMPTY.is_empty());
        assert_eq!(
            FieldMask::EMPTY.with(1).with(1),
            FieldMask::from_positions(&[1])
        );

        // Out-of-range positions are IGNORED, never folded onto a valid bit
        // (Codex P2 #441): position 64 must NOT alias to bit 0.
        assert_eq!(
            FieldMask::from_positions(&[64]),
            FieldMask::EMPTY,
            "position 64 must be ignored, not aliased to bit 0"
        );
        assert!(
            !FieldMask::EMPTY.with(64).has(0),
            "with(64) must not set bit 0"
        );
        assert!(
            !FieldMask::from_positions(&[0]).has(64),
            "has(64) must be false, not bit-0 aliased"
        );
        // In-range bit 0 unaffected by the out-of-range guard.
        assert!(FieldMask::from_positions(&[0, 64]).has(0));
        assert_eq!(
            FieldMask::from_positions(&[0, 64]).count(),
            1,
            "only the in-range bit 0 is set"
        );
    }

    #[test]
    fn field_mask_inherit_is_nondestructive_union() {
        // inherit = bitwise OR — a child IS-A its parent: it carries the parent's
        // present fields PLUS its own delta (focused cover, CodeRabbit #442).
        let parent = FieldMask::from_positions(&[0, 2]);
        let delta = FieldMask::from_positions(&[1, 2]); // bit 2 overlaps
        let child = parent.inherit(delta);
        assert_eq!(
            child,
            FieldMask(parent.0 | delta.0),
            "inherit is the bitwise union"
        );
        assert!(child.has(0) && child.has(1) && child.has(2));
        assert_eq!(
            child.count(),
            3,
            "the overlapping bit is not double-counted"
        );
        // EMPTY is the identity, both directions; the union is commutative.
        assert_eq!(parent.inherit(FieldMask::EMPTY), parent);
        assert_eq!(FieldMask::EMPTY.inherit(parent), parent);
        assert_eq!(parent.inherit(delta), delta.inherit(parent), "commutative");
        // FieldMask is Copy — neither operand is mutated by inherit.
        assert_eq!(parent, FieldMask::from_positions(&[0, 2]));
        assert_eq!(delta, FieldMask::from_positions(&[1, 2]));
    }

    #[test]
    fn meta_dto_projects_above_agnostic_class_mask() {
        let classes = FakeClasses::new();
        // The SoA supplied ONLY (class_id=7, mask) — no labels. The meta-DTO
        // resolves the labels from above.
        let mask = FieldMask::from_positions(&[0, 2]); // tax (pos 1) is off
        let projected: Vec<(&str, bool)> = classes
            .project(7, mask)
            .map(|(f, present)| (f.label.as_str(), present))
            .collect();
        assert_eq!(
            projected,
            vec![("Total", true), ("Tax", false), ("Partner", true)],
            "labels come from the cache above the SoA; presence comes from the mask"
        );
        // The render template skips off-bits: only present fields surface.
        let rendered: Vec<&str> = classes
            .project(7, mask)
            .filter(|(_, present)| *present)
            .map(|(f, _)| f.label.as_str())
            .collect();
        assert_eq!(rendered, vec!["Total", "Partner"], "off-bit (Tax) skipped");
    }

    #[test]
    fn field_label_resolves_late_from_class_not_row() {
        let classes = FakeClasses::new();
        assert_eq!(classes.field_label(7, 1), Some("Tax"));
        assert_eq!(classes.field_label(7, 9), None); // out of range
        assert_eq!(classes.field_count(7), 3);
        assert_eq!(classes.field_count(999), 0); // unknown class
    }

    #[test]
    fn render_rows_skips_off_bits_presence_only() {
        let classes = FakeClasses::new();
        // Tax (pos 1) is off → it must NOT produce a render row (C2: off-bits skipped).
        let rows = classes.render_rows(7, FieldMask::from_positions(&[0, 2]));
        assert_eq!(rows.len(), 2, "only the 2 populated fields render");
        assert_eq!(
            rows[0],
            RenderRow {
                label: "Total",
                predicate: "amount_total"
            }
        );
        assert_eq!(
            rows[1],
            RenderRow {
                label: "Partner",
                predicate: "partner_id"
            }
        );
        // Empty mask → zero rows (no template branch needed, just an empty iteration).
        assert!(classes.render_rows(7, FieldMask::EMPTY).is_empty());
        // Full mask → all 3 rows, in class order (the bit basis).
        let all = classes.render_rows(7, FieldMask::from_positions(&[0, 1, 2]));
        assert_eq!(
            all.iter().map(|r| r.label).collect::<Vec<_>>(),
            vec!["Total", "Tax", "Partner"]
        );
    }

    #[test]
    fn value_schema_default_is_full_temporary_poc() {
        // TEMPORARY (2026-06-15 POC): the blanket ClassView default materialises the
        // FULL value slab so consumers (tesseract-rs / woa-rs / medcare-rs / q2)
        // transcode against a populated NodeRow. Specialisation is opt-IN (override
        // to a smaller preset). When the POC phase ends, revert the default to
        // `ValueSchema::Bootstrap` AND this test together.
        use crate::canonical_node::{EdgeCodecFlavor, ValueSchema};
        let classes = FakeClasses::new();
        // The default class (classid 0x0000_0000) and any unconfigured class both
        // resolve to Full while the POC default is active.
        assert_eq!(classes.value_schema(0), ValueSchema::Full);
        assert_eq!(classes.value_schema(7), ValueSchema::Full);
        // The edge-codec axis is SEPARATE and untouched (still the CoarseOnly
        // zero-fallback) — only the value slab flipped to Full.
        assert_eq!(classes.edge_codec_flavor(0), EdgeCodecFlavor::CoarseOnly);
        // The TYPE-level default is unchanged: substrate zero-fallback stays Bootstrap.
        assert_eq!(ValueSchema::default(), ValueSchema::Bootstrap);
    }
}
