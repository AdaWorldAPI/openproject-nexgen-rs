// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Binding and running through DT64 and the native R2IL seam.
//!
//! The fixture is `lance-graph-r2il/tests/integration/polyfill.rs`: a
//! `BindStore` with the polyfill lens declared, classes registered through
//! `WithPolyfill`, one route position per class, and the frame / focus /
//! scope guards of the seam. Nothing here evaluates anything: `prepare` binds,
//! `prepare_native` compiles the folds once, `run_native` runs them.

use std::collections::BTreeMap;

use lance_graph_contract::class_view::ClassView;
use lance_graph_dt64::dr::{Focus, FrameRevisions, FrameStore, Guards, Scope};
use lance_graph_contract::class_view::WideFieldMask;
use lance_graph_dt64::{
    BindStore, ClassSchema, Extent, LaneBinding, LaneRefusal, LaneRequest, LaneSource, MethodBinding, MethodId,
    Refusal, RouteAt, RouteVersion, SchemaRev,
};
use lance_graph_mask_risc::{LaneRef, Planes};
use lance_graph_r2il::binding::{
    ExecPermit, ExecRefusal, FieldLanes, FieldPresence, Prepared, R2ilLens, Resolution, prepare, run,
};
use lance_graph_r2il::binding_native::{NativePlan, NativeScratch, prepare_native_present, run_native};
use lance_graph_r2il::polyfill::{AliasLanguage, POLYFILL_LENS, PolyfillView, WithPolyfill, polyfill_arities};
use lance_graph_r2il::FoldDialect;

use crate::ogar_basis::OgarBasis;

pub const REV: SchemaRev = SchemaRev(1);
const LENS: R2ilLens = R2ilLens { lens: POLYFILL_LENS, schema_rev: REV };
const FOCUS: Focus = Focus { revision: 1 };
const SCOPE: Scope = Scope { id: 1 };
const NOW: (Focus, Scope) = (FOCUS, SCOPE);
const REVS: FrameRevisions = FrameRevisions { source: 1, relation_map: 1, coordinates: 1 };

/// A registered method: where it lives and where its answer is left.
#[derive(Debug, Clone)]
pub struct MethodHandle {
    pub class: String,
    pub classid: u32,
    pub id: u32,
    pub position: u16,
    pub result_field: u8,
}

/// A method prepared and natively compiled over one set of lanes.
pub struct Compiled {
    pub handle: MethodHandle,
    pub prepared: Prepared,
    pub native: NativePlan,
    scratch: NativeScratch,
    permit: ExecPermit,
}

pub struct Engine {
    basis: OgarBasis,
    polyfill: PolyfillView,
    store: BindStore,
    frames: FrameStore,
    routes: BTreeMap<u32, (RouteVersion, u16)>,
}

fn extent_for(field_count: usize) -> Extent {
    if field_count <= 64 { Extent::DR64_CUBE } else { Extent::DR256_SQ_X64 }
}

impl Engine {
    /// An engine over a copy of `basis`, with the polyfill lens declared.
    pub fn new(basis: &OgarBasis) -> Result<Self, ExecRefusal> {
        let mut store = BindStore::new();
        store
            .declare_algebra(POLYFILL_LENS, REV, Extent::DR64_CUBE, polyfill_arities())
            .map_err(ExecRefusal::Binding)?;
        Ok(Self {
            basis: basis.clone(),
            polyfill: PolyfillView::new(AliasLanguage::R2il),
            store,
            frames: FrameStore::new(REVS),
            routes: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn basis(&self) -> &OgarBasis {
        &self.basis
    }

    /// Register every method of `class` (a class is bound once) and route a
    /// position to it. The result field of a method is the target of its
    /// last effect.
    pub fn register(&mut self, class: &str, methods: Vec<MethodBinding>) -> Result<Vec<MethodHandle>, ExecRefusal> {
        let Some(cb) = self.basis.class(class) else {
            return Err(ExecRefusal::Binding(Refusal::UnknownClass(0)));
        };
        let classid = cb.classid();
        let field_count = cb.fields.len();
        let position = u16::try_from(self.routes.len()).unwrap_or(u16::MAX);
        let ids: Vec<(u32, u8)> = methods
            .iter()
            .map(|m| (m.id.0, m.effects.last().map_or(0, |e| e.target)))
            .collect();
        let view = WithPolyfill::new(&self.basis, &self.polyfill);
        self.store
            .register(
                &view,
                classid,
                ClassSchema { schema_rev: REV, field_count, extent: extent_for(field_count) },
                methods,
            )
            .map_err(ExecRefusal::Binding)?;
        let version = self.store.route(position, classid).map_err(ExecRefusal::Binding)?;
        self.routes.insert(classid, (version, position));
        Ok(ids
            .into_iter()
            .map(|(id, result_field)| MethodHandle {
                class: class.to_string(),
                classid,
                id,
                position,
                result_field,
            })
            .collect())
    }

    /// Bind basis positions `fields` of a registered `class` to `source`'s
    /// lanes, under the source's current frame, its row order and its scope.
    /// `frames` must be the source's own store (`ForeignFrame` otherwise).
    pub fn bind_lanes<S: LaneSource>(
        &self,
        class: &str,
        fields: &[u8],
        frames: &FrameStore,
        source: &S,
        selection: lance_graph_dt64::CoordSpace,
        scope: Scope,
    ) -> Result<LaneBinding, LaneRefusal> {
        let classid = self.basis.class(class).map_or(0, |c| c.classid());
        let &(version, position) = self.routes.get(&classid).ok_or(Refusal::UnknownClass(classid))?;
        let view = WithPolyfill::new(&self.basis, &self.polyfill);
        let frame = source.frames().seal();
        self.store.bind_lanes(
            &view,
            frames,
            source,
            LaneRequest {
                route: RouteAt { position, version },
                rev: REV,
                fields: &WideFieldMask::from_positions(fields),
                frame: &frame,
                scope,
                selection,
            },
        )
    }

    /// The store and the view a [`LaneBinding`] made by [`Engine::bind_lanes`]
    /// is checked against.
    pub fn lane_check_parts(&self) -> (&BindStore, WithPolyfill<'_, OgarBasis>) {
        (&self.store, WithPolyfill::new(&self.basis, &self.polyfill))
    }

    fn guards(&self) -> Guards {
        Guards { frame: self.frames.seal(), focus: FOCUS, scope: SCOPE }
    }

    /// Bind `h` reading the class basis through `view`. A view that
    /// disagrees with the basis the class was registered against is refused
    /// here, cold.
    pub fn prepare_under<V: ClassView + ?Sized>(&self, view: &V, h: &MethodHandle) -> Result<Prepared, ExecRefusal> {
        let Some(&(version, position)) = self.routes.get(&h.classid) else {
            return Err(ExecRefusal::Binding(Refusal::UnknownClass(h.classid)));
        };
        let r = self.store.method_ref(h.classid, MethodId(h.id)).map_err(ExecRefusal::Binding)?;
        let g = self.guards();
        let wrapped = WithPolyfill::new(view, &self.polyfill);
        prepare(
            &self.store,
            &wrapped,
            LENS,
            &Resolution { method: &r, schema_rev: REV, route: (version, position), guards: &g },
            &self.frames,
            NOW,
        )
    }

    pub fn prepare(&self, h: &MethodHandle) -> Result<Prepared, ExecRefusal> {
        self.prepare_under(&self.basis, h)
    }

    /// Prepare and natively compile `h` over lanes of `n_rows` rows;
    /// `field_lanes[p]` is the lane that basis position `p` is read from.
    pub fn compile(
        &self,
        h: &MethodHandle,
        n_rows: usize,
        lanes: &[LaneRef<'_>],
        field_lanes: &[Option<u16>],
    ) -> Result<Compiled, ExecRefusal> {
        self.compile_under(&self.basis, h, n_rows, lanes, field_lanes)
    }

    pub fn compile_under<V: ClassView + ?Sized>(
        &self,
        view: &V,
        h: &MethodHandle,
        n_rows: usize,
        lanes: &[LaneRef<'_>],
        field_lanes: &[Option<u16>],
    ) -> Result<Compiled, ExecRefusal> {
        self.compile_present_under(view, h, n_rows, lanes, &[], field_lanes, &[])
    }

    /// [`Engine::compile`] with presence planes: `field_presence[p]` is the
    /// index into `masks` of basis position `p`'s presence plane, read by an
    /// `Arg::Present(p)` effect.
    pub fn compile_present(
        &self,
        h: &MethodHandle,
        n_rows: usize,
        lanes: &[LaneRef<'_>],
        masks: &[&[u64]],
        field_lanes: &[Option<u16>],
        field_presence: &[Option<u16>],
    ) -> Result<Compiled, ExecRefusal> {
        self.compile_present_under(&self.basis, h, n_rows, lanes, masks, field_lanes, field_presence)
    }

    #[allow(clippy::too_many_arguments)]
    fn compile_present_under<V: ClassView + ?Sized>(
        &self,
        view: &V,
        h: &MethodHandle,
        n_rows: usize,
        lanes: &[LaneRef<'_>],
        masks: &[&[u64]],
        field_lanes: &[Option<u16>],
        field_presence: &[Option<u16>],
    ) -> Result<Compiled, ExecRefusal> {
        let prepared = self.prepare_under(view, h)?;
        let planes = Planes { n_rows, masks, lanes };
        let native = prepare_native_present(
            &prepared,
            &self.store,
            &self.frames,
            NOW,
            FieldLanes { lanes: field_lanes },
            FieldPresence { planes: field_presence },
            &planes,
        )?;
        let scratch = NativeScratch::for_plan(&native);
        let permit = ExecPermit { methods: vec![prepared.method().clone()] };
        Ok(Compiled { handle: h.clone(), prepared, native, scratch, permit })
    }

    /// Run the compiled plan hot. Allocates nothing; reads only the lanes.
    pub fn run_native(&self, c: &mut Compiled, n_rows: usize, lanes: &[LaneRef<'_>]) -> Result<i64, ExecRefusal> {
        self.run_native_present(c, n_rows, lanes, &[])
    }

    /// [`Engine::run_native`] over the presence planes the plan was compiled
    /// against, in the same `masks` order.
    pub fn run_native_present(
        &self,
        c: &mut Compiled,
        n_rows: usize,
        lanes: &[LaneRef<'_>],
        masks: &[&[u64]],
    ) -> Result<i64, ExecRefusal> {
        let planes = Planes { n_rows, masks, lanes };
        let receipt = run_native(&c.native, &self.store, &self.frames, NOW, &c.permit, &planes, &mut c.scratch)?;
        let field = c.handle.result_field;
        receipt
            .values()
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, v)| *v)
            .ok_or(ExecRefusal::DataScalarNotPrepared { field })
    }

    /// The same plan through the interpretive R1 path, for cross-checks.
    pub fn run_r1(
        &self,
        prepared: &Prepared,
        n_rows: usize,
        lanes: &[LaneRef<'_>],
        field_lanes: &[Option<u16>],
    ) -> Result<i64, ExecRefusal> {
        let planes = Planes { n_rows, masks: &[], lanes };
        let foreign = lance_graph_mask_risc::Foreign::NONE;
        let mut d = FoldDialect::new(&planes, &foreign);
        let permit = ExecPermit { methods: vec![prepared.method().clone()] };
        let receipt = run(prepared, &self.store, &self.frames, NOW, &permit, FieldLanes { lanes: field_lanes }, &mut d)?;
        let last = receipt.output.values.last().map(|(_, v)| *v);
        last.ok_or(ExecRefusal::DataScalarNotPrepared { field: 0 })
    }
}

/// `field_lanes` for a basis of `field_count` positions where `bound[i] =
/// (position, lane)`.
#[must_use]
pub fn field_lanes(field_count: usize, bound: &[(usize, u16)]) -> Vec<Option<u16>> {
    let mut out = vec![None; field_count];
    for &(p, l) in bound {
        if let Some(slot) = out.get_mut(p) {
            *slot = Some(l);
        }
    }
    out
}
