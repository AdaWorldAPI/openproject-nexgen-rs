# Handover — the Klickweg + ViewFilter arc (2026-07-08)

**For the next session.** This closes the navigation-topology (`navigates_to`)
and view-projection (`ViewFilter`) arc across the ruff SPO frontends and their
op-nexgen consumer. Everything below is **shipped and merged** unless it's under
"Deferred". Read this once to orient; the doctrine + file map keep you from
re-deriving.

## The one-sentence model

**A rendered view never pushes data — it is a `WideFieldMask` projection over an
in-memory `ClassView` row, and navigation is a screen→screen graph checked for
connectivity by a Core brick.** Two halves of one "topology kit":

- **Jump half** — `navigates_to` edges (which screen opens which), verified by
  `lance_graph_contract::class_view::nav_is_fully_connected` (cycles allowed).
- **Stack half** — a view composing sub-views, acyclic-by-construction, held in
  op-server's `ViewRegistry`.

The projection mask is one brick minted by
`WideFieldMask::from_universe_present(basis, present)` (lance-graph #669),
`ViewFilter = rbac ∩ present ∩ view`.

## What shipped — ruff (the producers, all on `main`)

`navigates_to` + view-field-set arms, one per frontend. `Predicate::NavigatesTo`
+ (C# only) `Predicate::SelectsView` live in the shared `ruff_spo_triplet`
(predicate count = 66). `ViewFieldSet`/`ViewTarget` are shared types in
`ruff_python_spo::templates` — the Odoo arm literally `use`s them.

| Frontend | nav arm | field-set arm | ruff PR | shapes |
|---|---|---|---|---|
| C# (WinForms/DevExpress) | `ruff_csharp_spo` | — | #61, #64/#65 | 3 idioms (Show / UserControl-SPA / ribbon `selects_view`) |
| Ruby (Rails) | `ruff_ruby_spo::navigation` | `ruff_ruby_spo::views` (pre-existing ERB) | #62 | 2 (ERB `link_to` + controller `redirect_to`) |
| Python (Flask/Django) | `ruff_python_spo::navigation` | `ruff_python_spo::templates` (Jinja) | #63 | 1 (code-side `url_for`/`reverse`/`redirect`, AST) |
| Odoo (Python) | `ruff_python_spo::odoo_nav` | `ruff_python_spo::odoo_views` | #66, #67 | 2 (code `act_window` dict + data `<menuitem>` XML) |

ruff `main` tip at handover: **`b5fde66`** (#67). op-nexgen pins track it.

## What shipped — op-nexgen (the consumer, all merged to `main`)

Current `main`: **`2f06876`**. PR ledger for this arc:

| PR | What | Merged |
|---|---|---|
| #89 | Klickweg connectivity via the Core brick `nav_is_fully_connected` (`nav.rs`) | `8e52475` |
| #90 | `nav_harvest` bridge — harvest reproduces `NAV_EDGES` (klickweg jump half) | `caacc37` |
| #91 | **ViewFilter** (`rbac ∩ present ∩ view`) + nested-`ClassView` `ViewRegistry` + rolling-bucket God-object overflow + `field_harvest` (view half) | `d9649ef` |
| #92 / #93 / #94 | ruff pin rebases (#63 → #64/#65/#66 → #67) | `db08c4e` / `7f136b0` / `2f06876` |
| #94 | **Menu-rooted Klickweg parity** (Odoo #66 menuitem-root twin) + ERB↔Odoo synergy/drift finding | `2f06876` |

### File map (op-nexgen)

- `crates/op-server/src/nav.rs` — the nav registry + connectivity.
  - Screen graph: `SCREEN_UNIVERSE`, `NAV_ROOT`, `NAV_EDGES`, `route_for`,
    `nav_edges` (ClassView-association-derived), `klickweg_is_connected()`.
  - **Menu-rooted layer (parity):** `MENU_UNIVERSE` (synthetic `Menu` node +
    screens), `MENU_ROOT`, `MENU_NAV_EDGES`, `menu_target_concept`,
    `menu_klickweg_is_connected()` — the render-side twin of Odoo's
    `menuitem → res_model` roots; **stronger** than the screen check (catches a
    menu-orphan screen). Drift-guarded by `menu_nav_edges_match_derived`.
  - `NOT_YET_NAVIGABLE` — the deferred dead-lane list (status/type/priority/
    author/assignee/journals/relations/time-entries).
- `crates/op-server/src/viewfilter.rs` — `view_filter(rbac ∩ present ∩ view)`,
  `AnonymousRbac` (demo posture AS a real `ClassRbac` impl), `ViewRegistry`
  (intern = acyclic-by-construction stack + constructor amortization + route-arm
  dedup; `verify_stack_order` + `verify_bijective`), rolling buckets
  (`bucketized_masks` / `buckets_have` / `buckets_match_direct`).
- `crates/op-server/src/board.rs` — `skin()` routes every skin through
  `view_filter` (FULL rbac = identity → byte-identical output today);
  `view_registry()` interns the 7 real route skins; `bases_bucket_roundtrip()`.
- `crates/op-server/src/main.rs` — boot affirms BOTH klickweg layers (screen +
  menu) and the view-stack self-check (order + bijection + bucket equivalence).
- `crates/op-codegen-pipeline/src/nav_harvest.rs` + `field_harvest.rs` — the
  harvest bridges (jump + view halves); probes prove harvest ≡ hand-authored
  on synthetic Rails fixtures.

### Doctrine docs (read before touching this surface)

- `.claude/findings/2026-07-08-erb-vs-odoo-klickweg-synergy-drift.md` — the
  6-synergy / 6-divergence map. **Start here** for the cross-frontend picture.
- `.claude/findings/2026-07-08-pure-classview-per-app-not-shared.md` — why the
  render ClassView is per-app-transpiled, not shared-class enrichment.

## Invariants (do not break)

1. **The mask is one brick.** Always mint via `WideFieldMask::from_universe_present`;
   never ad-hoc `from_positions(mask_positions(..))` (skips the 256-SoC guard).
   `from_positions` is correct ONLY for widening an already-positional mask
   (`viewfilter::widen_rbac`).
2. **FULL rbac is identity, not literal-widen.** A >64-field basis would be
   truncated by literally widening `FieldMask::FULL`.
3. **Render masks index `SCREEN_UNIVERSE`; the menu node lives only in
   `MENU_UNIVERSE`.** Never let the synthetic `Menu` node into a render-mask
   position.
4. **Jump allows cycles; stack does not.** `nav_is_fully_connected` for nav;
   `ViewRegistry` intern-order (children-first) for stacks.
5. **>256-field universe is a "split this class" signal**, not a mask to widen —
   the rolling-bucket path is the mid-transcode overflow, not a license to grow
   God objects.
6. **op consumes the RUBY arms only.** Odoo/Flask/C# divergences are inert here.

## Deferred (the next arc — trigger: real OpenProject source in the pipeline)

1. **Real-corpus harvest.** `NAV_EDGES` + the board skins are hand-authored and
   *proven equal* to the harvest on synthetic fixtures. When the actual
   OpenProject Rails tree enters the pipeline input, regenerate them FROM the
   harvest. Two known traps to carry in:
   - **D3 cross-file join** (from the synergy/drift finding): OpenProject's menu
     is Rails-central (`config/routes.rb` + a menu registry), so a per-file nav
     harvest will measure zero real menu edges — exactly the #64/#66
     "green-on-fixture, zero-on-real-app" blind spot. op's `nav_harvest` needs
     the Odoo cross-file join treatment.
   - **Idiom coverage:** the Ruby arm keys on `link_to`/`redirect_to`; a real
     app also navigates via `render partial:`, Turbo frames, helper-generated
     menus, and (OpenProject's modern UI) Angular `routes.ts` — invisible to an
     ERB scan. Run against the real tree, compare `raw_target_refs` vs edges,
     add missing idioms configurably (the #64 pattern).
2. **Dead-lane pages.** `nav::NOT_YET_NAVIGABLE` (status/type/priority/author/
   assignee/journals/relations/time-entries for WP; members/time-entries for
   Project). Each becomes a routed screen → new `route_for` arm + `SCREEN_UNIVERSE`
   entry + nav edges; the menu-rooted check then proves they're menu-reachable.
3. **Auth.** `AnonymousRbac` is the seam. A real role store replaces it (argon2 +
   CSRF first); `view_filter` drops columns per role with zero render-site change.
4. **Downstream siblings.** odoo-rs is the repo that would actually feed Odoo
   Python through #66/#67's two arms (`extract_odoo_nav_edges` +
   `extract_odoo_view_field_sets` against `od_ontology::mint_wide_mask` bases) —
   its own session's lane, not op-nexgen's.

## Ops notes

- ruff PRs are **org-gated**: push the branch off-proxy
  (`git -c http.proxy=$HTTPS_PROXY ... push https://x-access-token:$GH_TOKEN@github.com/AdaWorldAPI/ruff.git`),
  then the operator opens/merges (they arrive here as "N merged" FYIs).
- op-nexgen + OGAR PRs go through MCP normally. OGAR gitignores `Cargo.lock`
  (auto-tracks `branch=main` ruff deps — no rebase commit needed there).
- Every merge FYI = sync the repo, check the PR's crate scope, bump/verify
  consumers, propagate. Additive shared-IR changes (new `Predicate` variants)
  are compile-safe for op (no exhaustive `Predicate` match).
