# Knowledge transfer — the six-region layout plane, ported from ruff #76 (WinForms) to the OpenProject/Rails harvest

> READ BY: any session extending `ruff_ruby_spo`'s ERB/menu harvest toward the
> Klickwege **structure**-parity oracle; anyone wiring the OpenProject render
> frame (`op-server` board/render-bake). Companion to
> `ruff/.claude/knowledge/consumer-transcode-furnace-playbook.md` §6 (the
> six-region render equation) and this repo's `2026-07-10-routes-arm-spec.md`
> (RoutesTo/RouteScope — the routes half of the structure oracle).
>
> **Status: [G] harvester — built, corpus-verified, adversary-reviewed
> (ruff PR #78); [H] end-to-end oracle — the digest round-trip is the one
> remaining gate.** The pattern was proven `[G]` for C#/WinForms (ruff #76);
> the Rails region arm now ships in `ruff_ruby_spo::menu_regions`
> (`docked_at`/`tab_order`/`contains_control`, reusing the shared predicate
> plane — no mint). Corpus probe over the real OpenProject tree is green: 45
> files, 16 `map_blocks`, 137 items, 64 with `parent:`, **0 unresolved**. The
> `tab_order` derivation is a faithful single-pass replay of Rails
> `MenuManager::TreeNode` (a correctness adversary caught two phase-separation
> divergences — `first:` LIFO and the `after:`-onto-`last:` live-boundary
> case — both fixed with regression fixtures). What is still `[H]`: the
> **structure-parity round-trip** (feed the `region=` table into
> `nav_digest`'s `[regions]` section, render the frame, re-parse, re-derive,
> assert identity — §5's render→parse→re-derive test). Harvest side done;
> digest wiring is the last link to full `[G]`.
>
> **Source of the pattern:** ruff PR #76 (region-grammar plane) +
> `ruff_csharp_spo` harvester (`docked_at`/`tab_order`/`opens_popup`) +
> `ruff_spo_triplet::{exam_config, nav_digest, triple}`. The furnace playbook
> §6 already generalizes it language-agnostically; this is the Rails instance.

## 1. What #76 established (the transferable method)

The **render-side half of the Klickwege structure oracle**: a frontend's own
layout-intent facts become SPO triples; a consumer *reimagines* them into a
universal **six-region frame** — never emulates the source toolkit. Three
predicates (all `Provenance::Authoritative`, f=0.95/c=0.90), minted closed-vocab
in `ruff_spo_triplet::Predicate` and **already shared** across all `ruff_*_spo`
frontends:

| Predicate | wire | WinForms source | object shape |
|---|---|---|---|
| `DockedAt` | `docked_at` | `control.Dock = DockStyle.X` | lowercased dock token (`top`/`left`/`right`/`bottom`/`fill`/`none`) |
| `TabOrder` | `tab_order` | `control.TabIndex = N` | decimal string verbatim |
| `OpensPopup` | `opens_popup` | `control.ContextMenuStrip = this.M` | the menu control's IRI (`ns:Screen.M`) |

**The load-bearing separation (harvest vs reimagine):** the harvester emits the
**raw** token (`docked_at → "fill"`); the **dock-token → region-name** mapping
is 100% config-driven downstream — `ExamConfig.regions: Vec<(String,String)>`
fed by the `region=<token>:<name>` directive (`exam_config.rs:131`). The six
region names (`top_bar/left_nav/right_panel/bottom_bar/center/popup`) are a
**convention, not an enforced enum**; an unmapped token renders `unmapped:<tok>`
(`nav_digest.rs:220`), never dropped. `nav_digest`'s `[regions]` section groups
`(screen, region) → controls` ordered by `tab_order` asc (missing = `u32::MAX`,
ties lexicographic); an `opens_popup` subject gets a `→popup` suffix.

The render equation (playbook §6): `render(R) = live(R).ordered_by(harvested_order)
.as(interaction[predicate])`, `live(R) = region_basis[R] ∩ global_mask ∩
local_mask`, with **RBAC/`user_right` = the global mask**. The frame is a
*projection under two masks*, never a new struct.

## 2. The Rails mapping — where the analog lives, and where it DIFFERS

**Key finding: the machine-readable layout source in Rails is the MENU DSL, not
the ERB layout.** WinForms `Designer.cs` carries per-control `Dock = DockStyle.X`
assignments (clean AST literals). Rails `base.html.erb` regions are div-nesting /
`render` / `content_for` (semi-structured, weak to static harvest). But
`Redmine::MenuManager.map(menu_name)` + `menu.push(name, url, options)`
(`lib/redmine/menu_manager/mapper.rb:51`) **is** the clean analog — and
`ruff_ruby_spo::menu.rs` already parses those exact calls (today it lifts only
`menu → target` as `NavigatesTo`). So the region arm is an **extension of the
existing menu harvest**, reading the same `menu.push` sites for two more facts.

| #76 predicate | Rails analog | source (file:line) | delta to flag |
|---|---|---|---|
| `docked_at` | the **menu the item is pushed into** = the region. `MenuManager.map(:top_menu)` → `top_bar`; `:project_menu`/`:global_menu`/`:admin_menu`/`:my_menu`/`:notifications_menu` → `left_nav`; `:account_menu`/`:quick_add_menu` → `top_bar` (header widgets). | `config/initializers/menus.rb` (`:top_menu` :33, `:project_menu` :707, `:admin_menu` :307, `:account_menu` :142, `:global_menu` :181, `:my_menu` :272) | **Region is the MENU, not a per-item property.** The token the harvester emits is the `menu_name`; the six-region mapping is the same downstream `region=` config (`region=top_menu:top_bar`, `region=project_menu:left_nav`, …). |
| `tab_order` | the item's **relative position** — `options[:first/:last/:before/:after/:parent]` | `mapper.rb:36-68` (`add_at(..., position_of(...))`) | **Rails has NO numeric TabIndex.** Order is relative (`before:`/`after:`/`first`/`last`) resolved by `TreeNode` position at render. The arm must **derive an ordinal** by replaying the menu builder's declaration order + first/last/before/after into the same topological position Rails computes, and emit that integer as `tab_order` — OR emit the relative directive verbatim and defer resolution. Faithful default = derive the ordinal (matches what Rails renders). |
| `opens_popup` | Primer `ActionMenu`/`Dialog` (server ERB) or the Angular `OpContextMenuService` (SPA) | `app/components/projects/row_actions_component.html.erb:2`; `op-context-menu.service.ts:31` | **Weakest static signal in Rails** — the popup binding is a component render / Angular registration, not a clean assignment. **Defer** (like #76 defers nothing, but Rails genuinely lacks the `ContextMenuStrip =` clarity). A `menu.push … partial:` that renders a dropdown is the closest static hook. |

## 3. The six-region config table for OpenProject (the `region=` map)

| Region | OpenProject artifact | fill source |
|---|---|---|
| `top_bar` | `<header class="op-app-header">` | `:top_menu` + `:account_menu` + `:quick_add_menu` + `:notifications_menu` (bell) — `base.html.erb:69-85` |
| `left_nav` | `<nav id="main-menu">` | `render_main_menu` → `:project_menu` / `:global_menu` / `:admin_menu` / `:my_menu` — `base.html.erb:91-131` |
| `right_panel` | `turbo_frame "content-bodyRight"` → `content_for :content_body_right` | notification split view — `base.html.erb:179` |
| `center` | `div#content` → `:content_header` + `:content_body` + bare `yield` | `base.html.erb:143-178` |
| `popup` | `opce-modal-overlay` / Primer dialogs / `OpContextMenuService` | `base.html.erb:58-63` |
| `bottom_bar` | **∅ no analog** — OpenProject's app shell has no footer/status bar | — |

**Honest deltas (write these into any spec, do not paper over):**
1. **`bottom_bar` is empty for OpenProject.** The six-region frame is a
   *superset*; an empty `region_basis[bottom_bar]` is correct, not a bug (the
   WinForms `none` token + `unmapped:` fallback already model "no region"). Do
   NOT invent a footer to fill it.
2. **`tab_order` is derived, not read.** WinForms hands you the integer; Rails
   makes you compute it from the relative-position graph. That derivation is the
   one genuinely new piece of logic (everything else reuses `menu.rs`'s parse).
3. **`content_for :sidebar` is NOT a sixth region** — its `<div id="sidebar">`
   nests *inside* `<nav id="main-menu">` (`base.html.erb:126`), i.e. a sub-panel
   of `left_nav`, not a right panel.

## 4. How it composes with what this repo already harvests (the hinge is RoutesTo)

The furnace playbook's **concept-as-join-key**: a screen `surfaces_concept X`;
the consumer maps `X → /route`; RBAC gates `X`. The render-side Klickwege chain
for Rails is already *mostly built* — the region arm is the last link:

```
menu.push item                                     ← menu.rs (NavigatesTo) + NEW region arm
   docked_at   <menu_name→region>                  ← NEW  (docked_at)
   tab_order   <derived ordinal>                    ← NEW  (tab_order)
   navigates_to <target screen>                     ← menu.rs / navigation.rs  (EXISTS)
      routes_to <controller#action>                 ← routes.rs  RoutesTo       (EXISTS — the hinge)
         surfaces_concept X                          ← concept binding (three-axis gate)
            value oracle: PG/MySQL rows for X        ← the OTHER oracle
```

`RoutesTo` is why the render-side half joins the value oracle at all: the menu
item's target resolves (helper stem/`controller: action:` → `controller#action`)
through the routes arm shipped in ruff #73, then to the concept, then to the DB
row. Region + order + popup is the *layout projection over* that spine — exactly
the playbook's "region frame is a projection under two masks, not a new struct."

## 5. Grade + the path to [G] (if built)

`[H]`. To reach `[G]` (measured byte-parity on the OP corpus), three edits —
same shape as every furnace arm:

1. **Vocab: none.** `DockedAt`/`TabOrder`/`OpensPopup` are already minted,
   closed-vocab, and shared (`ruff_spo_triplet`). The Ruby arm reuses them — do
   NOT mint Rails-specific predicates (that would fork the shared plane).
2. **Harvester: a new `ruff_ruby_spo` region arm** — extend the `menu.push`
   parse (`menu.rs`) to emit, per item: `docked_at = menu_name`, `tab_order =
   derived ordinal` (the new logic: replay first/last/before/after into a
   TreeNode position), and — deferred — `opens_popup` from `partial:`/Primer
   hooks. Emit `Triple{s,p,o,f,c}` at 0.95/0.90, identical wire shape to the C#
   arm so `from_ndjson` needs no transform.
3. **Digest: reuse `nav_digest`'s `[regions]` section as-is** — it is already
   frontend-agnostic (it reads `docked_at`/`tab_order`/`opens_popup` triples +
   the `region=` config, source-language-blind).

**Probe (the [H]→[G] gate):** run the arm over `config/initializers/menus.rb`
(+ module engines), feed the OP `region=` table from §3, and assert the
`nav_digest [regions]` output matches a hand-verified projection for
`:project_menu` (left_nav, the settings→children ordering) and `:top_menu`
(top_bar). The **structure-parity twin** of the value oracle: render the frame,
re-parse it, re-derive the region facts, and assert round-trip identity
(playbook §6's render→parse→re-derive test).

## 6. One-line takeaway

WinForms hands you the layout as per-control `Dock=` literals; **Rails hands you
the layout as the menu DSL** — `menu_name` is the region, the relative-position
options are the order, and the item's target routes (via the RoutesTo arm
already shipped) into the concept/value oracle. Same three predicates, same
config-driven reimagining, same six-region frame; the only new logic is deriving
`tab_order` from Rails' relative positioning. `bottom_bar` stays legitimately
empty. Harvest + reimagine, never emulate.
