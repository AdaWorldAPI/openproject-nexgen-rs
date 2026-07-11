# SPEC — `ruff_ruby_spo` region arm (the six-region layout plane for Rails)

> **Applies the knowledge transfer in `.claude/knowledge/six-region-layout-port.md`.**
> Takes the region-grammar plane from `[H]` → `[G]` on the Rails side, reusing
> ruff #76's three shared predicates. Base = ruff main (post #76/#77), branch
> `claude/openproject-transcode-status-c6e8in`.
>
> **Corpus reality (measured):** 117 `menu.push` in `config/initializers/menus.rb`
> + more in module engines; `parent:` ~418, `last:` ~132, `after:` ~41,
> `before:` ~20, `first:` 3 across all menu files; **multi-line calls are the
> norm** (`menu.push :x,\n {url},\n if:,\n caption:` spans 4+ lines). 9 core
> menus (`top_menu`/`quick_add_menu`/`account_menu`/`global_menu`/
> `notifications_menu`/`my_menu`/`admin_menu`/`project_menu`/`work_package_split_view`).

## 1. Predicates — REUSE the shared plane, mint nothing

| fact | predicate (wire) | source | already exists |
|---|---|---|---|
| region placement | `DockedAt` (`docked_at`) | the enclosing `MenuManager.map :NAME` menu | ✅ #76 |
| nesting tree | `ContainsControl` (`contains_control`) | `parent: :X` kwarg | ✅ #72 |
| sibling order | `TabOrder` (`tab_order`) | declaration order + `first/last/before/after` | ✅ #76 |

`ContainsControl` object is a target IRI (`ns:Screen.child`), same slot as
`OpensPopup` — the `parent:` nesting is exactly WinForms `Controls.Add`. No new
predicate, no count-lock bump. (The routes-arm lesson: do NOT re-assert any
global predicate count in this arm's tests.)

## 2. Subject / object shapes

Model: **the menu is the "screen"; each item is a "control."** For a
`menu.push :item, url, opts` inside `MenuManager.map :menu_name`:

- `DockedAt`: `(ns:<menu_name>.<item>, docked_at, "<menu_name>")` — object is the
  raw menu-name token; the six-region mapping is downstream config (`region=`
  directive), never hardcoded here. (Harvest the raw token; reimagine downstream.)
- `TabOrder`: `(ns:<menu_name>.<item>, tab_order, "<ordinal>")` — decimal string,
  the resolved position **within its sibling group** (see §3). Verb-identical
  wire shape to the C# arm.
- `ContainsControl`: `(ns:<menu_name>.<parent>, contains_control, ns:<menu_name>.<item>)`
  — emitted only when `parent:` is present. Root items (no `parent:`) get no
  `contains_control`; their container is the menu itself (implicit).

All three `Provenance::Authoritative` (f=0.95/c=0.90), matching #76.

## 3. The `tab_order` resolution algorithm (THE delicate part — spell it out)

**Global two-pass, per menu_name, across ALL files** — because Rails merges every
`map :menu_name` registration (core `menus.rb` + module engines) before resolving
order. A per-file resolution would mis-resolve a module's `after: :core_item`.

Pass 1 — collect: walk every file; for each `menu.push` accumulate
`Registration { menu_name, item, parent: Option<Sym>, position: Position }` where
`Position ∈ { Append (default), First, Last, Before(sym), After(sym) }`. Preserve
**file+line declaration order** as the stable base (files sorted by path, lines
ascending — the same deterministic order Rails' initializer + engine load gives,
approximated; documented as the one ordering assumption).

Pass 2 — resolve, per `(menu_name, parent)` **sibling group** independently:
1. Seed the group list with its members in declaration order.
2. Apply `First` items: move to the front, preserving their relative declaration
   order (Rails `add_first`).
3. Apply `Last` items: move to the end, preserving relative order.
4. Apply `Before(anchor)`/`After(anchor)`: reposition the item immediately
   before/after `anchor` **if `anchor` is in the same sibling group**; else
   **fall back to append** (this is Rails' own documented missing-anchor
   behavior, `mapper.rb`). Resolve in declaration order so a chain
   (`after: :a` then `after: :b`) is deterministic.
5. The `tab_order` ordinal = final 0-based index within the sibling group.

**Never emit a wrong ordinal:** if a group's ordering is genuinely
unresolvable (a `before/after` cycle — not present in this corpus), emit NO
`tab_order` for the affected items (they sort last per #76's `u32::MAX` rule) and
count them in a `unresolved_order` report field. Cycles are a report signal, not
a guess.

## 4. Module shape (AST walk, like routes.rs — NOT a line scanner)

New module `crates/ruff_ruby_spo/src/menu_regions.rs`. `lib-ruby-parser` AST walk
(multi-line/nested/option-rich `menu.push` demands it; `menu.rs`'s line scanner
cannot). Reuse `routes.rs`/`walk.rs` idioms.

```rust
pub struct RegionEntry {
    pub menu: String,               // menu_name (the region token, pre-config)
    pub item: String,               // pushed symbol
    pub parent: Option<String>,     // parent: kwarg
    pub position: Position,         // Append | First | Last | Before(s) | After(s)
    pub tab_order: Option<u32>,     // resolved in pass 2; None = unresolved/cyclic
    pub file: String,
}
pub enum Position { Append, First, Last, Before(String), After(String) }

pub struct RegionScanReport {
    pub files_scanned: usize,
    pub map_blocks: usize,          // MenuManager.map blocks seen
    pub items: usize,               // menu.push items harvested
    pub with_parent: usize,
    pub with_position: usize,       // non-Append
    pub unresolved_order: usize,    // cyclic / unresolvable → no tab_order
    pub menus: Vec<String>,         // distinct menu_names, sorted
}

pub fn extract_regions(root: &Path) -> Vec<RegionEntry>;
pub fn extract_regions_with_report(root: &Path, namespace: &str)
    -> (Vec<RegionEntry>, RegionScanReport);
// walks <root>/config/initializers/menus.rb + <root>/**/engine.rb + lib menu files
```

AST walk: find `Send{recv: MenuManager-ish, method: "map", args: [Sym(name), block]}`
(and module `menu :name do` form) → push `menu_name` context; inside, find
`Send{recv ends "menu", method: "push", args}` → extract item symbol (first arg),
`parent:`/`first:`/`last:`/`before:`/`after:` from the options hash. `to_triples`
emits the three facts per §1/§2.

## 5. Region= config table for OpenProject (consumer-side, ships with the digest)

```
region=top_menu:top_bar
region=quick_add_menu:top_bar
region=account_menu:top_bar
region=notifications_menu:top_bar
region=project_menu:left_nav
region=global_menu:left_nav
region=admin_menu:left_nav
region=my_menu:left_nav
region=work_package_split_view:center
```
`bottom_bar` intentionally has NO row (OpenProject has no footer — the region is
legitimately empty; the frame is a superset).

## 6. Tests

Unit fixtures (`#[cfg(test)]` in menu_regions.rs): (a) single map block, plain
appends → declaration-order tab_order; (b) `parent:` nesting → contains_control +
per-parent sibling ordering; (c) `first:`/`last:` reordering; (d) `before:`/`after:`
with in-group anchor; (e) `after:` with **missing** anchor → append fallback; (f)
cross-file: two files pushing to the same menu → merged order; (g) multi-line push
(item + url + if: + caption: across lines) parses; (h) docked_at menu-name token
+ to_triples shapes; (i) a before/after cycle → `unresolved_order`, no tab_order.

Corpus probe (env-gated `RAILS_CORPUS_SRC`, self-skipping): run over
`/home/user/openproject`; assert `map_blocks == 9` (the core menus; ≥9 with
modules), `items >= 117`, `with_parent > 0`, `unresolved_order == 0` (pin the
measured value), and a spot-check: `:settings` under `:project_menu` resolves to
`left_nav` with a `contains_control` to its `settings_*` children (menus.rb:809
loop), and `:top_menu` items land in `top_bar`. Measure-don't-claim: pin the real
`items`/`map_blocks` at first green run.

## 7. Honest deltas (write into the module doc, do not paper over)

1. **`opens_popup` is NOT harvested** — the Rails popup binding is a Primer
   component render / Angular `OpContextMenuService` registration, not a clean
   `ContextMenuStrip =` assignment. Deferred (the knowledge doc's [H] note).
2. **`bottom_bar` empty for OpenProject** — no footer region; the frame is a
   superset (§5).
3. **Load-order assumption** — pass 1 uses path-sorted + line order as the
   deterministic base; Rails' true order is initializer + engine `require` order.
   Documented; the probe's `unresolved_order == 0` + spot-checks bound the risk.
4. **`content_for :sidebar` is a left_nav sub-panel, not a region** (base.html.erb:126).

## 8. Gate + process

Central: `cargo test -p ruff_ruby_spo -p ruff_spo_triplet` green · `cargo clippy
-p ruff_ruby_spo` clean · per-file `rustfmt --check` on `menu_regions.rs` + `lib.rs` ·
corpus probe green under `RAILS_CORPUS_SRC`. Edit allowlist: `menu_regions.rs`
(new) + `lib.rs` (register + re-export). NO triple.rs edit (all three predicates
already exist). One correctness-adversary review on §3 (the order-resolution
replay vs Rails TreeNode) before merge.
