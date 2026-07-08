# ERB (Rails) vs Odoo (Python) Klickweg — synergy / drift map

**Date:** 2026-07-08 · **Trigger:** ruff #66/#67 landed the Odoo
`navigates_to` (two-shape) + `ir.ui.view` field-set arms; this audits them
against the Ruby/ERB arms (#62, `ruff_ruby_spo::{navigation,views}`) that
op-nexgen actually consumes, and finalizes op's Klickweg structure parity.

## Bottom line

The two arms **converge on structure and vocabulary, diverge only on
framework-shaped surface details** — and every divergence is documented,
principled, and *not* accidental drift. op-nexgen consumes the **Ruby**
arms, so the Odoo-specific divergences are inert here; the *synergies* are
what carry across, and the one op-relevant lesson is the **cross-file menu
join** (see Drift-3).

## Synergies (the shared bricks — these are the point)

| # | Shared thing | Evidence |
|---|---|---|
| S1 | **One predicate** — `Predicate::NavigatesTo` | Ruby `RubyNavEdge`, Odoo `OdooNavEdge`, C# harvester, Flask `PyNavEdge` all emit it. Screen→screen is frontend-agnostic. |
| S2 | **One field-set type** — `ViewFieldSet`/`ViewTarget` | `odoo_views.rs` literally `use crate::templates::{ViewFieldSet, ViewTarget}` — the SAME types, not parallel ones. |
| S3 | **One mask mint** — `WideFieldMask::from_universe_present(basis, fields)` | ERB · askama · Jinja · **Odoo XML** = four renderers over one projection brick. |
| S4 | **Two-shape nav** | Ruby (ERB `link_to` + controller `redirect_to`) and Odoo (code `act_window` dict + data `menuitem` XML) BOTH split the Klickweg across two artifacts — same structural insight. |
| S5 | **Honest denominator** | `fields ⊆ referenced` / `raw_*_refs` in every arm; closed vocab is the gate, raw count is the truth. |
| S6 | **Menu = entry-point root** | Odoo `<menuitem> → res_model` gives `nav_is_fully_connected` its roots; op's top-nav `menu()` is the identical structure — **this finding wires it the same way** (see Parity below). |

## Drift (divergences — each principled, none accidental)

| # | Axis | ERB (Rails) | Odoo (Python) | Why it's correct, not drift |
|---|---|---|---|---|
| D1 | **View scoping** | receiver-scoped (`ViewTarget::receivers` — `issue`/`@issue` bind the model in-template) | record-scoped (`<record model="account.move">` IS the receiver; `receivers` ignored) | The artifact carries its model differently: ERB binds via a local/ivar, Odoo declares it on the record. |
| D2 | **Target match** | singular/plural tolerant (`project`↔`projects`) | EXACT (`account.move` ≠ `account.move.line`) | Rails targets are route *stems* (fuzzy); Odoo targets are canonical model *identifiers*. |
| D3 | **Join locality** | per-file (a view/controller names its own edges) | **cross-file** (actions in per-model view files, `<menuitem>`s central in `account_menuitem.xml`) | Measured: a per-file join found ZERO menu edges on the real `account` addon. **This is the op-relevant lesson** (see below). |
| D4 | **Nav idiom** | `link_to`/`button_to`/`redirect_to` | `act_window` dict + `menuitem` XML (SPA — `url_for` is 2 stray noise hits) | The Klickweg lives in a different construct per framework; the harvester must know the framework's idiom. (The #64 "synthetic-fixture blind spot": green on a fixture, zero on the real app.) |
| D5 | **Selector plane** | none | none (C# only: `selects_view`, the ribbon/tab selector that is NOT a screen jump, #64/#65) | Keeps `navigates_to` a pure screen→screen graph; a tab-selector-without-a-jump is a DevExpress-shaped concept neither Rails nor Odoo has. |
| D6 | **Meta/arch split** | none (every `receiver.field` is a projection) | load-bearing (`name`/`model`/`inherit_id` are meta; only `arch` `<field/>` count) | Odoo XML bundles record metadata with the projection; ERB doesn't. |

## Consequences for op-nexgen

- **Inert here:** D1/D2/D4/D5/D6 are Odoo-frontend specifics. op consumes
  the Ruby arms (`ruff_ruby_spo`), so none of these change op today.
- **The one that matters — D3 (cross-file join):** op's `nav_harvest`
  is per-file, which is correct for its current `link_to`/`redirect_to`
  shapes. But OpenProject's real menu wiring is Rails-central
  (`config/routes.rb` + a menu registry), so when the **real corpus**
  enters the pipeline, op's harvest will need the Odoo cross-file-join
  lesson — the same "measured zero on the real app" trap #64 and D3 both
  hit. Tracked as the deferred real-corpus harvest arc (also the trigger
  for regenerating `NAV_EDGES` from harvest rather than hand-authoring).

## Parity finalized this PR (the "necessary dependent doing")

op's Klickweg was rooted at a *screen* (`NAV_ROOT = ProjectWorkItem`);
connectivity meant "reachable from that screen." The cross-frontend model
(S6) roots it at the **menu**. This PR adds the render-side twin of Odoo's
menuitem roots to `op-server::nav`:

- `MENU_UNIVERSE` = synthetic `Menu` node + served screens;
  `MENU_NAV_EDGES` = `Menu → <tab-target>` (one per `menu()` tab) + the
  inter-screen `NAV_EDGES` (+1 shifted).
- `menu_klickweg_is_connected()` = `nav_is_fully_connected(MENU_ROOT, …)`
  — **every screen must be reachable by clicking from the top nav**, which
  catches a menu-orphan screen that the screen-only check
  (`klickweg_is_connected`) would pass.
- `menu_nav_edges_match_derived` pins the static table against
  `menu()` + `NAV_EDGES` (no drift); boot affirms both layers.

Same Core brick (`nav_is_fully_connected`), same "menu gives entry points"
shape as Odoo — op's Klickweg is now structurally at parity with the
cross-frontend model, screen-graph AND menu-reachability.
