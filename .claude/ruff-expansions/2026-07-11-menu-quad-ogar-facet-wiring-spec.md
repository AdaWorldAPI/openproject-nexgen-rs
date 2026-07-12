# SPEC — wire the menu-quad triples into an OGAR facet (the radix lowering, OGAR side)

> Lane B of the 2026-07-11 autonomous burst. The menu-quad plane (`part_of` /
> `purpose` / `surfaces_concept` / `navigates_to`) is emitted by ruff (Rails +
> Odoo) and lowered to a text digest by `nav_digest::menu_address`. This spec
> wires it into a real **OGAR facet** — the `(classid, radix-address)` the
> transpile substrate mints. Grounded in the Lane-B scout (read-only map). Needs
> **council** — the axis-homonym is a live hazard. Base = ruff main `15d3433` +
> OGAR main.

## 0. The seam + the gap (from the scout, with citations)

- **Minter exists:** `ruff_spo_address::mint_with_classid(triples, classid_of)`
  (`ruff/crates/ruff_spo_address/src/lib.rs:298-361`) packs a 16-byte `Facet`
  (byte-identical to `lance_graph_contract::facet::FacetCascade`) from a
  `(part_of_rank, is_a_rank)` per node. Invoked via
  `ogar_from_ruff::mint::compile_graph_*` (`OGAR/crates/ogar-from-ruff/src/mint.rs`).
- **But its `part_of` forest is hardwired** to `has_field`/`has_function`
  (member→class), and `is_a` to `inherits_from`/`rdf:type`
  (`ruff_spo_address/src/lib.rs:308-332`). Every OTHER predicate — including the
  menu quad's `Predicate::PartOf` — hits the catch-all `_ => nodes.insert(s)`
  (`:328-330`) and contributes **nothing** to the mint.
- **No consumer reads the quad ndjson.** `grep MenuQuad|build_nav_digest|
  PurposeRole` across `/home/user` returns only producer-side ruff files. The
  menu-quad's only consumer today is `nav_digest::menu_address`
  (`ruff_spo_triplet/src/nav_digest.rs:57-79`) — a private radix walk into a
  **text digest string**, not a facet, not exported.

## 1. THE CRUX — `part_of` is a homonym; the two forests MUST stay disjoint

`Predicate::PartOf` names **two unrelated containment axes**:
- **class-membership** (`has_field`/`has_function` inverted) — what
  `mint_with_classid` already builds its `part_of` forest from.
- **menu-tree parent** (`MenuQuad`'s `part_of`) — the navigation rail.

Feeding both into ONE `mint_with_classid` call **conflates two address
spaces** — corrupting both (the scout's #5 anti-pattern; `ruff_spo_address::soc`
exists to lint exactly this axis-duplication). The wiring MUST build a
**menu-scoped** forest, parallel to and never merged with the structural forest.

## 2. Design — a `mint_menu_with_classid` sibling in `ogar-from-ruff`

A NEW entry point (NOT a change to `mint_with_classid`'s predicate set):

```rust
// ogar-from-ruff (mechanical logic minted into OGAR, per OGAR-TRANSPILE-SUBSTRATE 85/15)
pub fn mint_menu_facets(
    menu_triples: &[Triple],           // the harvested menu-quad ndjson, filtered
    classid_of: impl Fn(&str) -> u32,  // node surfaces_concept -> classid (PortSpec::class_id)
) -> Mint {
    // Build a MENU part_of forest ONLY from Predicate::PartOf among menu-node
    // subjects — never has_field/has_function. `is_a` is empty for menu nodes
    // (no class inheritance); the facet's is_a_rank is 0/unused for this arm.
    // Radix address = the part_of rank (root-first ancestor chain), exactly the
    // shape nav_digest::menu_address computes — but returning a Facet, not text.
}
```

- The **radix address** comes from the menu `part_of` forest (root-first
  ancestor rank), reusing the SAME walk semantics as `nav_digest::menu_address`
  (cycle-guarded, depth-bounded) — **do NOT hand-roll a second radix walker**
  (scout anti-pattern); if the logic isn't shareable as-is, lift it to a shared
  helper both call, don't duplicate.
- **classid** resolves from each node's `surfaces_concept` object via the
  consuming app's `PortSpec::class_id`, mirroring `classid_for_node::<P>`
  (`mint.rs:219-224`).

## 3. Sub-gap — menu nodes don't emit `surfaces_concept` YET

`MenuQuad::identity_concept` is `None` today (Rails + Odoo both defer it to the
concept gate), so menu nodes carry **no `surfaces_concept` fact** → the mint has
no classid to resolve. Consequence: **this wiring lands in two slices.**

- **Slice 1 (this spec): the ADDRESS.** Build the menu `part_of` forest → radix
  address; classid falls back to `0`/default (bare-name rank), exactly as
  `nav_digest::menu_address` falls back to the screen name when the concept is
  unresolved. The facet's location axis is real; its identity axis is dormant
  (the zero-fallback ladder — `classid==0` = default class, per the OGAR canon).
- **Slice 2 (future): the IDENTITY.** Wire `identity_concept` on the harvest
  (menu item → concept via the three-axis gate), then the same mint resolves a
  real classid. No mint change — just a non-None `surfaces_concept`.

This matches the OGAR "reserve, don't reclaim" ladder: `classid==0` is *not
consulted*, never *compacted away*; a later non-zero mint wakes identity with
zero facet-layout change.

## 4. The consumer call site (thin, per Core-First doctrine)

op-nexgen gets a one-line wrapper mirroring
`op-codegen-pipeline::ogar_consumer::compile_op` (`ogar_consumer.rs:42-44`):
harvest menu quads (`ruff_ruby_spo::extract_menu_quads` /
`ruff_python_spo::odoo_quad::extract_menu_quads`) → `to_ndjson` → the OGAR
`mint_menu_facets` → the app's `PortSpec`. **No `MenuBridge`, no codebook copy,
no parallel menu AST** (the scout's anti-pattern catalogue; the classid is pure
address, the magic is at the Core node it resolves to).

## 5. Anti-patterns (from the scout — bake into the spec as guards)

1. **Axis conflation** — menu `part_of` and structural `part_of` in one mint
   call. The whole reason for a separate `mint_menu_facets`. (`soc` lint.)
2. **Constructing a `*Bridge`** instead of calling the OGAR entry + `PortSpec::
   class_id` (`OGAR-CONSUMER-BEST-PRACTICES.md` §2/§3).
3. **Copying the codebook** locally instead of resolving through the Port alias.
4. **A parallel menu AST** — `od-ontology/src/ogar.rs` documents deleting one
   already; don't repeat.
5. **Hand-rolling a second radix walker** duplicating `menu_address` — lift to a
   shared helper or reuse; never copy (`consumer-transcode-furnace-playbook.md`
   no-hand-roll rule).
6. **Behavior smuggled into triples** — `navigates_to`/`part_of` are pure
   address/edge facts; screen-open logic resolves at the `ActionDef`+`KausalSpec`
   Core node the facet addresses (`SURREAL-AST-TRAP-PREFLIGHT.md` negative-beauty
   hijack).

## 6. Council questions

1. **convergence-architect** — is a **separate `mint_menu_facets`** the right
   shape (vs. a predicate-set parameter on `mint_with_classid`)? Does the
   Slice-1 address-only facet (classid=0 dormant) converge with the OGAR
   zero-fallback ladder, or is an identity-less facet a mis-fit that should wait
   for Slice 2? Is reusing `menu_address`'s walk (lift to shared helper) clean,
   or do the two callers' needs (String vs Facet-rank) diverge enough to justify
   two walkers?
2. **baton-handoff-auditor** — the OGAR-canon boundary: the new `Facet` from a
   menu forest must be byte-identical `FacetCascade` shape; the axis-homonym
   must be structurally impossible to conflate (separate entry, separate forest);
   the `classid==0` dormant path must round-trip through the ladder without
   reclaim; no cross-repo count/vocab drift.

**Do not build until both rule.** If the council says the identity-less facet is
premature, STAGE Slice 1 behind the identity binding (Slice 2 first).

## 7. Gate + allowlist (if greenlit)

Central: `cargo test -p ogar-from-ruff -p ruff_spo_address` green (OGAR side) +
the op-nexgen consumer wrapper compiles. Allowlist: `ogar-from-ruff/src/mint.rs`
(the sibling entry), possibly `ruff_spo_address/src/lib.rs` (if the menu forest
needs a shared builder), the op-nexgen consumer wrapper. **No change to
`mint_with_classid`'s existing predicate set** (axis-purity). Board hygiene lands
in OGAR (EPIPHANIES) per the OGAR canon, mirrored on the nexgen board.
