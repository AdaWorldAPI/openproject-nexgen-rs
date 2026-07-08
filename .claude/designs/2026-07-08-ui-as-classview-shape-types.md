# UI as ClassView shape types — layout regions, widgets, part_of/is_a, codegen

**Date:** 2026-07-08 · **Status:** design (operator-articulated) · **Repos:**
ruff (harvest) · OGAR (ClassView type system + codegen) · op-nexgen (consumer)

## The core inversion

op-nexgen renders a **read-only preview** (board → detail → Edit) with *zero
visible actions, no side menu, dead lanes, no admin/user*. The instinct to
hand-build those in op-server is **wrong**. The UI itself — regions, menus,
widgets, action buttons — is a **harvested shape that OGAR codegen emits**,
exactly as fields (`views.rs` / `odoo_views.rs`) and nav edges
(`navigation.rs` / `odoo_nav.rs`) already are. op is a *hollow preview
precisely because nothing harvests the UI affordances*, so codegen has nothing
to emit. The fix is upstream shapes, not op-local HTML.

Everything below maps onto machinery **that already exists** — this is
composition of known bricks, not new invention.

## Layer 0 — the page is a ClassView of regions (root + on/off bitmask)

A page is a ClassView whose *fields* are layout regions:

```
region universe = [ global, header, left_menu, right_menu, footer ]
page mask       = WideFieldMask::from_universe_present(regions, present)   // on/off
```

- The mask is the **same brick** every skin mints (`from_universe_present`,
  lance-graph #669) — bit `i` on ⇒ region `i` is present in this page.
- `global` is the always-on chrome; `left_menu` is the **side menu** that is
  "completely missing" today — it becomes a region bit, not bespoke HTML.

## Drill-down — each region is itself a ClassView + mask (nested)

"Drill down like websites": each region is a child ClassView with its own
basis + mask, composed through the **nested `ViewRegistry`** already built in
`op-server::viewfilter`:

- `ViewRegistry::intern(concept, mask, children)` — a page node stacks its
  region nodes as `children`; a region node stacks its widget nodes; etc.
- Acyclic **by construction** (children interned before parents), one-pass
  order check (`verify_stack_order`) + bijective round trip
  (`verify_bijective`) — the *stack half* of the topology kit. The nav
  *jump half* (`nav_is_fully_connected`) is the orthogonal per-screen graph.
- So the layout tree IS a stacked-ClassView tree; no new container type.

## Widgets as reusable shape types — part_of / is_a, behaviour inherited

Every UI element is a **reusable ClassView shape type**, related to others by
the two canon edges OGAR's GUID facet already carries (`part_of:is_a` rails,
`classid → ClassView`):

- **`part_of`** (composition): `input` part-of `form` part-of `region`
  part-of `page`. The layout tree above is a `part_of` spine.
- **`is_a`** (inheritance): `dropdown` is_a `input`; `checkbox` is_a `input`;
  `edit_checkmark` is_a `action_button`. **Behaviour inherits with the class
  type** — a `dropdown` inherits `input`'s render + validation; an
  `edit_checkmark` inherits `action_button`'s `invokes_action` (verb=patch).

Because these are OGAR classids resolving to ClassViews, the inheritance and
composition are **free** — the same `classid → ClassView` resolution the
consumer already uses for `ProjectWorkItem` / `Project`. A widget type is just
another class in the codebook; its behaviour is a property of the class it
`is_a`, never re-authored per site.

## Actions — the `invokes_action` plane (shipped)

`ruff_ruby_spo::actions` (ruff PR `claude/ruby-invokes-action-arm`) harvests
the mutating affordances: `Predicate::InvokesAction`
`(screen, invokes_action, resource#member)` + the HTTP verb on the edge. An
`edit_checkmark` widget `is_a action_button` whose `invokes_action` edge names
the mutation (`verb=patch`). This is the harvest half of "actions/buttons";
codegen emits a `<button method=verb>` from it. Kept off `navigates_to` (a
button is not a GET link), same discipline as `selects_view` (#64).

## What harvests what (the shape inventory)

| UI concept | Harvest shape | Where | Status |
|---|---|---|---|
| Fields shown | `ViewFieldSet` → `from_universe_present` | `views.rs` / `odoo_views.rs` | ✅ shipped |
| Screen→screen nav (Klickweg) | `navigates_to` | `navigation.rs` / `odoo_nav.rs` | ✅ shipped |
| Tab/ribbon selector | `selects_view` | `ruff_csharp_spo` | ✅ shipped (#64) |
| **Mutating buttons** | **`invokes_action`** | **`ruff_ruby_spo::actions`** | ✅ **just shipped (pending PR)** |
| **Side menu / regions** | menu-DSL + a `layout_region` shape | Rails `.rb` menu DSL (new arm) | ⬜ next |
| **Widget types (input/dropdown/checkbox)** | `is_a` / `part_of` in the ClassView codebook | OGAR ClassView + facet | ⬜ (OGAR) |
| Codegen emit (regions→HTML) | `ogar_render_askama` region/widget skins | OGAR | ⬜ (OGAR) |

## Build sequence

1. **[shipped]** `invokes_action` ERB arm (ruff) — the button shape.
2. **Layout-region category (op-local, next):** an `op-server::layout` module —
   region universe `[global, header, left_menu, right_menu, footer]` + a region
   `WideFieldMask`, each region a `ViewRegistry` node stacked under a page node.
   Renders a real side menu + chrome from the region mask, on the existing kit.
   Ships without any upstream change; proves the Layer-0/drill-down model.
3. **Side-menu harvest arm (ruff):** a Rails menu-DSL (`.rb`) scanner —
   `Redmine::MenuManager`-style `menu.push :label, path` → the left-menu region's
   entries. Sibling of the nav arm; the "menu = entry points" (#66 Odoo lesson)
   feeds `nav_is_fully_connected`.
4. **Widget shape types (OGAR):** register `input`/`dropdown`/`checkbox`/
   `action_button` as ClassViews in the codebook with `is_a`/`part_of` edges;
   behaviour inherits via `classid → ClassView`. Council-gated (shared canon).
5. **Codegen emit (OGAR):** `ogar_render_askama` gains region + widget skins so
   codegen emits the composed page; op consumes, stops hand-building.
   (JS "writes the classes in the CSS" — the client-side styling layer over the
   codegen'd class names — is the final, separate presentation concern.)

## Invariant

Every layer is a `WideFieldMask` over a universe + a `classid → ClassView`
resolution. No new container/struct per layer: a page, a region, a widget are
all ClassViews; composition is `part_of` (the nested `ViewRegistry` stack),
inheritance is `is_a` (the OGAR facet). The UI is data (masks + classids) that
codegen renders — never hand-authored HTML in the consumer.
