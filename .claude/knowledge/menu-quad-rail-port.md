# Knowledge transfer — the menu QUAD `(location, purpose, identity, action)`: harvest as quadruplets, lower via the existing radix-trie ontology

> READ BY: any session extending `ruff_ruby_spo`'s menu harvest toward the
> Klickwege **structure** oracle; anyone lowering harvested menu facts into the
> OGAR facet / radix-trie address space. Companion to
> `six-region-layout-port.md` (the *render-region* half — `docked_at`/
> `tab_order`/`contains_control`) and `2026-07-10-routes-arm-spec.md` (RoutesTo,
> the target resolver the quad's `identity`/`action` axes join through).
>
> **Status: [H] — the pattern is proven `[G]` for C#/WinForms (ruff #81); this
> doc ports the QUAD framing to the Rails menu DSL and grades what it takes to
> reach `[G]` here.** No code shipped by this doc — it is the map.
>
> **Source of the pattern:** ruff PR #81 (`part_of` + `purpose` predicates +
> the C# `ruff_csharp_spo` emit arm) + the pre-existing `surfaces_concept` /
> `navigates_to` / `opens_popup` predicates. #81's insight: a legacy UI's
> navigation harvests as a **menu quad** — four axes per menu node — and the
> quad *lowers into the ontology by construction* because one axis (location)
> IS a radix-trie address, not a stored field.

## 1. The quad (the transferable idea)

A menu node is not a row of attributes; it is a **four-axis coordinate** the
ontology already knows how to place:

| axis | predicate(s) | what it answers | ontology target |
|---|---|---|---|
| **identity** | `surfaces_concept` | *what concept is this?* | the **classid** (hi-u16 concept, minted in `ogar-vocab`) |
| **location** | `part_of` | *where in the menu tree?* | the **radix-trie address** — the walked `part_of` rail (HHTL cascade path) |
| **purpose** | `purpose` | *what KIND of surface?* | a facet **role** (closed vocab: `list`/`detail`/`form`/`chart`/`settings`/`action`/`dialog`) |
| **action** | `navigates_to` / `opens_popup` / `invokes_action` | *what does clicking DO?* | the **`EdgeBlock`** (the click edge) |

The load-bearing move (#81, restated): **location is NOT a stored ordinal.**
The V3 LE-contract §3 forbids a label/position slot in a facet, so the menu
position is the `part_of` **rail**, projected. Walking `part_of` from a leaf to
the root menu yields the radix-trie menu **address** by construction — a
projection the `ClassView` computes, never a byte you store. `part_of:is_a` is
the mereological rail pair the LE-contract names; this quad supplies the
`part_of` half for the menu tree.

## 2. Harvest — where each axis lives in the Rails menu DSL

The C# arm had to *infer* two of the four axes; **Rails declares three of them
outright** — the menu DSL is richer than a `Designer.cs` for exactly this.

| axis | C# (#81) source | **Rails source** | delta |
|---|---|---|---|
| **location** `part_of` | INFERRED — post-pass over `navigates_to`: canonical parent = first screen that opens the child | **DECLARED** — `menu.push :x, parent: :y` → `(ns:menu.x, part_of, ns:menu.y)`. Root items (no `parent:`) are `part_of` the menu node itself. | Rails hands you the rail; no first-opener heuristic. Declared → arguably **Authoritative**, not #81's Inferred default (a per-emit tier choice — see §5). |
| **purpose** | control composition (`DataGridView`→list, form-fields→form, `Chart`→chart, `Button`→action) | the target **RESTful action** in the push opts `{controller:, action:}`: `index`→`list`, `show`→`detail`, `new`/`edit`→`form`, a parent-grouping (has children, no leaf target)→`settings`, else→`action`/`detail` | **different signal, same closed vocab.** Rails has no control set on a menu item; the RESTful verb IS the role. |
| **identity** `surfaces_concept` | the screen class → concept | the item symbol + target controller → concept → classid, resolved through the routes arm (`RoutesTo`) + the three-axis mint gate | same as every other consumer; already the concept-as-join-key spine |
| **action** | `navigates_to`/`opens_popup` | the click edge to the target `controller#action` (via `RoutesTo`); `opens_popup` for a dropdown/`partial:` render | same shape as #76/#81 |

**Rails is the easy consumer for `part_of`** (declared, not inferred) and the
*idiomatic* consumer for `purpose` (the REST verb is the role) — but it needs
the harvest **extended**: `menu_regions` today captures `parent:` and
`position` but NOT the target `action:` (the purpose signal) — the second
positional `{controller:, action:}` hash is currently dropped. That extraction
is the one genuinely new harvest step (everything else reuses `parent:` +
`RoutesTo`).

## 3. Lower — the radix-trie address IS the walked `part_of` rail

This is the half that makes the quad *lower into the ontology for free*:

```
harvest:   menu.push :settings_general, parent: :settings   (inside project_menu)
           menu.push :settings,          (root of project_menu)
facts:     (project_menu.settings_general, part_of, project_menu.settings)
           (project_menu.settings,         part_of, project_menu)          [root]
walk:      settings_general → settings → project_menu → (root)
lower:     the walk is the radix-trie ADDRESS — one nibble/tier per hop
           (OGAR FAN_OUT=16, HHTL cascade HEEL/HIP/TWIG). No ordinal stored;
           the ClassView projects the address from the rail.
```

Mapped onto the existing OGAR ontology (`AdaWorldAPI/OGAR` canon):

- **identity → classid.** The concept the item surfaces mints the hi-u16
  (`ogar-vocab`); the app render prefix is the lo-u16. The menu node's facet
  `classid = (concept << 16) | APP_PREFIX`.
- **location → the radix-trie path.** Walking `part_of` = descending the
  16-ary tree (`1 hex = 1 nibble = 1 tier`). The menu address is the tier
  nibbles — the HHTL `HEEL/HIP/TWIG` cascade — computed from the rail, never
  stored. This is precisely the OGAR "the key prerenders nodes with zero value
  decode" property: the menu tree lays out from the `part_of` addresses alone.
- **purpose → a facet role byte.** The closed vocab (7 roles) fits a byte in
  the content-blind 4+12 facet's 12-byte register (V3 `E-V3-FACET-4-PLUS-12`).
- **action → the `EdgeBlock`.** `navigates_to`/`opens_popup` are the out-of-
  facet edges (the click Klickweg).

So one menu node lowers to **one OGAR facet**: `classid(identity) +
radix-address(location, projected from part_of) + role(purpose) +
edge(action)` — a content-blind 4+12 facet, no bespoke menu struct, no stored
position. The quad IS the facet's four readings.

## 4. How it composes with what's already harvested (the hinge is still RoutesTo)

```
menu.push :work_packages, { controller: "/work_packages", action: "index" }, parent: :modules
   part_of      → ns:project_menu.modules            (location — the RAIL, §3)   NEW
   purpose      → "list"   (from action: "index")                                NEW
   surfaces_concept → work_package_concept → classid  (identity)          EXISTS (concept gate)
   navigates_to → /work_packages#index                (action)            EXISTS (routes.rs RoutesTo)
      routes_to <controller#action>                                       EXISTS (#73)
         value oracle: PG/MySQL rows                                      the OTHER oracle
```

The quad is the render-side Klickweg **coordinate**; the routes arm resolves
its `action`/`identity` targets; the value oracle grounds the concept. Location
+ purpose complete the quad — and location lowers to the radix address with no
new storage, which is the whole point.

## 5. Grade + the path to [G]

`[H]`. To reach `[G]` (measured on the OP corpus), the same three-edit furnace
shape:

1. **Vocab: none new.** `part_of` + `purpose` are minted, closed-vocab, shared
   (ruff #81, `ALL.len()==78`). The Ruby arm REUSES them — do NOT mint
   Rails-specific predicates.
2. **Harvester: extend `menu_regions`** to emit, per item: `part_of` (from
   `parent:`, root items → the menu node), and `purpose` (classify the target
   `action:` — the currently-dropped second positional hash — into the 7-role
   vocab; parent-grouping → `settings`). `identity`/`action` already resolve
   via `RoutesTo` + the concept gate.
   - **Tier decision (the one real call):** #81 defaults `part_of` to
     **Inferred** (0.85/0.75) because C# infers it. Rails *declares* `parent:`,
     so Rails `part_of` is arguably **Authoritative**. Recommendation: keep the
     predicate's default (Inferred) for wire-consistency with the C# arm UNLESS
     a council rules the declared-vs-inferred distinction should raise the tier
     — a clean question for a `truth-architect` / provenance review.
3. **Lowering: OGAR consumes the quad** — the `part_of` rail → radix address is
   an OGAR-side projection (`ClassView`), not a harvester concern. The
   harvester's job ends at emitting the four facts; OGAR walks `part_of` to
   compute the HHTL address.

**Probe (the [H]→[G] gate):** harvest the OP menu tree as quads, **walk
`part_of` from each leaf to its root menu**, and assert the resulting radix
address matches the rendered menu nesting (e.g. `settings_general` under
`settings` under `project_menu` → a 3-tier address) — the structure-parity twin
of the value oracle. Pair it with the six-region digest round-trip
(`six-region-layout-port.md` §5): render the frame, re-derive the quad, assert
round-trip identity.

## 6. Honest deltas (write into any spec, do not paper over)

1. **`part_of` vs `contains_control` — do NOT conflate.** The region arm
   already emits `contains_control` from `parent:` (raw control tree). #81's
   `part_of` is the ONE canonical menu-tree parent (the location rail). For a
   Rails menu the `parent:` IS the menu tree, so it earns BOTH: `part_of`
   (child→parent, the rail) AND `contains_control` (parent→child, the tree).
   They point opposite directions and serve different axes; emit both, do not
   replace one with the other.
2. **`purpose` from action-name is a heuristic**, not a declared literal — a
   custom-action menu item (`action: "board"`) falls to `action`/`detail`; a
   non-REST target has no clean role. Inferred tier is honest here (matches
   #81's default). Do not over-claim.
3. **The harvest extension is real work** — `menu_regions` currently drops the
   `{controller:, action:}` positional; `purpose` needs it. This is the one new
   parse, mirroring how the routes arm reads the same hash.
4. **Location is projected, never stored** — resist any urge to add a
   `menu_order`/`depth` field. The rail + the radix trie ARE the address (V3
   LE-contract §3 / OGAR "key prerenders with zero value decode"). A stored
   ordinal is the anti-pattern the quad exists to avoid.

## 7. One-line takeaway

A menu node is a **quad** — `identity`(→classid), `location`(→radix address via
the `part_of` rail), `purpose`(→role byte), `action`(→edge) — and it lowers to
**one content-blind OGAR facet** with no stored position, because walking
`part_of` IS the address. Rails *declares* three of the four axes the C# arm
had to infer; the only new harvest is reading the RESTful `action:` for
`purpose`. Harvest quads, walk the rail, lower into the ontology you already
have.
