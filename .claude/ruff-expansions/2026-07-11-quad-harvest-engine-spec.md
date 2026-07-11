# SPEC — the reusable menu-QUAD harvest engine + per-frontend config

> **Applies `.claude/knowledge/menu-quad-rail-port.md` AND the operator
> directive "make C# / Rails / Odoo / Python a config over reusable."** One
> shared quad-harvest engine in `ruff_spo_triplet`; each frontend becomes a
> small config declaring where its four signals live. Base = ruff main
> (post #81, `44e27e4`), branch `claude/openproject-transcode-status-c6e8in`.
>
> **Status: SHIPPED — engine + Rails (ruff `c6bcbd5`); Odoo staged.**
> Council-consolidated (convergence-architect OPPORTUNITY-NOW + baton
> CATCH-CRITICAL, both folded into §0.v2). The reusable `quad` engine
> (count-based `classify_purpose` + `MenuQuad` bare-node lift + the C# parity
> test) and the Rails config/adapter shipped + gated (166+165+51 green,
> count-lock 78 untouched). Odoo config (§5 step 3 — new `<menuitem parent=>`
> parse) is the remaining follow-up.

## 0.v2 — council consolidation (SUPERSEDES conflicting text below)

1. **The engine is COUNT-based, not existential** (convergence-architect, the
   load-bearing fix). C#'s `form` rule is `count(input-controls) ≥ 2` — a
   substring-"any-hit" model fires `form` at 1 input (a regression vs the C#
   golden). Corrected shape (this is what §2 builds):
   ```rust
   pub struct PurposeRule {
       pub needles: &'static [&'static str], // a token hits if it CONTAINS any needle
       pub role: PurposeRole,
       pub min_hits: usize,                  // matching tokens required; 1 = existential
   }
   pub fn classify_purpose(tokens: &[&str], rules: &[PurposeRule], fallback: PurposeRole)
       -> PurposeRole {
       for r in rules {
           let hits = tokens.iter().filter(|t| r.needles.iter().any(|n| t.contains(n))).count();
           if hits >= r.min_hits { return r.role; }
       }
       fallback
   }
   ```
   `min_hits: 1` is byte-identical to existential — Rails/Odoo never exercise
   the count path; C#'s table (chart:1, list:1, form:2, action:1) transcribes
   exactly. THIS is what makes "one engine, four configs" literal.
2. **Drop `LocationSource`.** It is a 1:1 rename of `Provenance {Authoritative,
   Inferred}` (declared parent → Authoritative, inferred first-opener →
   Inferred). Use the tier directly; the resolution *strategy* lives in the
   per-frontend adapter, not the shared type.
3. **The quad node subject is a BARE `{ns}:{name}` IRI — NOT `RegionSubject`**
   (baton P0, the CATCH-CRITICAL). The shipped grammar for `part_of` /
   `surfaces_concept` / `navigates_to` is bare-screen (C# golden
   `csharp:OrderScreen part_of csharp:MainScreen`; Rails `navigation.rs:105`
   emits `navigates_to` as bare `{ns}:{name}`). A dotted `{screen}.{node}`
   would fork the predicate into two grammars AND break the intra-arm join (the
   quad's `surfaces_concept` must join the nav arm's). So `MenuQuad` carries a
   single `node: String` bare IRI; `RegionSubject` stays region-arm-only (the
   region plane is control-granular *within* a menu screen — a different plane).
   This also dissolves the Odoo dotted-`module.xmlid` risk: a menuitem id is an
   opaque bare node, never `rsplit`.
4. **Add `PurposeRole::ALL`** (mirroring `Predicate::ALL`) so vocab-validity is
   machine-derived, not a hand-maintained list. Do NOT re-assert a role *count*
   in the ruby/csharp arms (the "monitor N pins" anti-pattern the count-lock
   comment forbids).
5. **C# conformance = a pure-Rust engine-MODEL parity test, not ndjson-alphabet
   validation** (convergence-architect + baton P1). Encode C#'s `ClassifyPurpose`
   rules as a `PurposeRule[min_hits]` slice, feed a fixture of control-type
   token-sets through `classify_purpose`, and assert parity against C#'s known
   outputs. That test passes ONLY if the engine model can BE the C# classifier —
   which, with `min_hits`, it is. No dotnet needed (dotnet is unavailable here).
   Cross-arm *semantic* parity (does C# classify a 2-input screen the same way
   Rails would?) is explicitly NOT covered — a `truth-architect`/corpus question,
   deferred.

**Build order (v2):** (1) `quad` engine + the C# parity test [this pass];
(2) Rails config + adapter — read `action:`, emit the quad with bare `{ns}:{item}`
nodes [this pass, the operator's "apply" ask]; (3) Odoo config — NEW parse of
`<menuitem parent=>` in `odoo_nav` (currently reads `action=` only), bare
menuitem-id nodes [follow-up]; (4) — no separate C# code change (parity test in
step 1 is the conformance).

---


## 1. The quad (recap) and the four-frontend signal table (grounded)

A menu node harvests as `(identity, location, purpose, action)` — see the
knowledge doc. The FOUR arms differ ONLY in where two of the axes' signals live
(identity/action already resolve via `surfaces_concept` + `navigates_to`/
`RoutesTo`, identically across arms):

| arm | crate | **location** (`part_of`) source | **purpose** signal → rules |
|---|---|---|---|
| C# WinForms | `ruff_csharp_spo` (dotnet) | **inferred** — post-pass over `navigates_to`, canonical parent = first opener | **control composition**: Chart→chart; DataGridView/Grid/ListView→list; ≥2 input controls→form; Button→action; else detail |
| Rails | `ruff_ruby_spo` | **declared** — `menu.push :x, parent: :y` | **REST verb** (target `action:`): index→list; show→detail; new/edit→form; parent-grouping→settings; else action/detail |
| Odoo | `ruff_python_spo` | **declared** — `<menuitem parent="…">` (`odoo_nav::scan_menuitems`) | **view tag / view_mode**: graph/pivot→chart; tree/list/kanban→list; form→form; calendar/activity/gantt→detail; else detail |
| (other Python) | `ruff_python_spo` | (same as Odoo, or inferred) | (view/route rules, same engine) |

**The convergence:** identical quad, identical output vocab, identical lift and
radix lowering. Only `LocationSource` and the `purpose` rule table vary — those
two ARE the per-frontend config.

## 2. The reusable core (new `ruff_spo_triplet::quad` module)

```rust
/// The usability role of a surface — the `purpose` axis' closed vocab. Matches
/// the `Predicate::Purpose` doc vocab (list/detail/form/chart/settings/action/
/// dialog). Frontend-agnostic; every arm classifies INTO this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurposeRole { List, Detail, Form, Chart, Settings, Action, Dialog }

impl PurposeRole {
    pub fn as_str(self) -> &'static str { /* "list" | "detail" | … */ }
}

/// One priority-ordered classification rule: if any needle is a substring of
/// any signal token, the surface is `role`. Rules are tried in order; first
/// match wins (so a frontend orders chart > list > form > action > detail).
pub struct PurposeRule { pub needles: &'static [&'static str], pub role: PurposeRole }

/// The reusable classifier — the SAME engine for every frontend. `tokens` are
/// the frontend's raw signal strings (control type names, or [the REST verb],
/// or [the view tag]); `rules` is the frontend's config table. `fallback` is
/// its else-role. No app-specific logic — pure ordered substring match.
#[must_use]
pub fn classify_purpose(tokens: &[&str], rules: &[PurposeRule], fallback: PurposeRole)
    -> PurposeRole { /* first rule whose needle hits any token; else fallback */ }

/// Where a frontend's `part_of` location rail comes from. The engine builds the
/// `part_of` facts from this; the predicate + radix semantics are shared.
pub enum LocationSource {
    /// Declared parent (Rails `parent:`, Odoo `<menuitem parent=>`): the fact is
    /// (child, part_of, parent) verbatim, Authoritative-eligible.
    DeclaredParent,
    /// Inferred first-opener (C#): canonical parent = first `navigates_to`
    /// source; Inferred tier.
    InferredFirstOpener,
}

/// One resolved menu-node quad. Frontend-agnostic; each arm builds these and
/// the shared lift emits the facts (subject via the shared `RegionSubject`
/// codec from the region collapse — one subject grammar for regions AND quads).
pub struct MenuQuad {
    pub screen: String,             // node subject screen half
    pub node: String,               // node subject control half
    pub identity_concept: Option<String>, // surfaces_concept object (→ classid)
    pub parent: Option<String>,     // part_of object (None = root: part_of the menu)
    pub purpose: PurposeRole,
    pub location_tier: LocationSource, // provenance selector for part_of
    // action edges (navigates_to / opens_popup) stay with the existing
    // nav/routes arms — the quad references them, doesn't re-emit them.
}

impl MenuQuad {
    /// Emit the quad's facts: `part_of` (tier per LocationSource), `purpose`,
    /// and `surfaces_concept` when identity is bound. Subject via RegionSubject.
    pub fn to_triples(&self) -> Vec<Triple> { /* … */ }
}
```

**What is reusable (in `ruff_spo_triplet`):** `PurposeRole` vocab,
`classify_purpose` engine, `LocationSource`, `MenuQuad` + lift, and the radix
lowering contract (below). **What is config (per Rust frontend):** a
`const PURPOSE_RULES: &[PurposeRule]` + a fallback role + the `LocationSource`.

## 3. Per-frontend config (the "config over reusable")

Each Rust frontend shrinks to a config table + a thin adapter that reads its
signal tokens and calls the shared engine:

```rust
// ruff_ruby_spo (Rails) — config is ~8 lines, not a hand-rolled classifier:
const RAILS_PURPOSE: &[PurposeRule] = &[
    PurposeRule { needles: &["index"], role: PurposeRole::List },
    PurposeRule { needles: &["show"], role: PurposeRole::Detail },
    PurposeRule { needles: &["new", "edit"], role: PurposeRole::Form },
];
// location = DeclaredParent (parent:); tokens = [target action]; fallback = Action
```

```rust
// ruff_python_spo (Odoo) — same engine, different table:
const ODOO_PURPOSE: &[PurposeRule] = &[
    PurposeRule { needles: &["graph", "pivot"], role: PurposeRole::Chart },
    PurposeRule { needles: &["tree", "list", "kanban"], role: PurposeRole::List },
    PurposeRule { needles: &["form"], role: PurposeRole::Form },
];
// location = DeclaredParent (<menuitem parent=>); tokens = [view tags]; fallback = Detail
```

**C# (`ruff_csharp_spo`, dotnet — the cross-language arm):** it CANNOT call the
Rust engine. Two honest options for the council:
- **(A) Conformance-only (recommended):** C# keeps its `ClassifyPurpose` in
  Program.cs, but it is understood/documented as "the C# config" — it already
  emits the SAME closed vocab (list/detail/form/chart/action) and the SAME
  `part_of` post-pass. The reusable *contract* (the `PurposeRole` vocab + the
  quad shape) is shared; C#'s executor stays C#. Add a Rust-side test asserting
  the C# golden ndjson's `purpose` objects are all valid `PurposeRole::as_str`
  values — so C# conformance is machine-checked even though its executor is
  separate.
- **(B) Shared data table:** lift the rules into a data file both the Rust
  engine and the C# harvester read. Heavier (a config format + a C# parser);
  probably over-engineering for 3 small tables. Council decides; (A) is the
  minimal-blast-radius default and mirrors how the region collapse left C#
  emitting compatible output without forcing it onto the Rust codec.

## 4. Lowering — the radix-trie address (unchanged from the knowledge doc)

The harvester emits the four facts; **OGAR lowers**: walking `part_of` leaf→root
yields the radix-trie menu address (FAN_OUT=16, HHTL cascade), `identity`→
classid, `purpose`→a role byte, `action`→the `EdgeBlock`. No stored ordinal
(V3 LE-contract §3). The engine's job ends at emitting a well-formed quad; the
address is an OGAR `ClassView` projection. (This spec does NOT build the OGAR
lowering — it builds the harvest engine that feeds it.)

## 5. Build order

1. **Reusable core** in `ruff_spo_triplet::quad` (vocab + engine + `MenuQuad`
   + lift + tests). No mint (`part_of`/`purpose` already exist from #81).
2. **Rails config + adapter** (the operator's "apply" ask): extend
   `menu_regions` to read the target `action:` (currently dropped), classify via
   `RAILS_PURPOSE`, emit the quad (`part_of` from `parent:`). Corpus probe:
   assert quads over the OP menu tree, and that walking `part_of` reproduces the
   nesting.
3. **Odoo config + adapter**: `ODOO_PURPOSE` over the view tag; `part_of` from
   `<menuitem parent=>` (join `odoo_nav`). Behaviour-preserving for the existing
   region facts.
4. **C# conformance test** (option A): Rust-side check that the C# golden's
   `purpose`/`part_of` shapes validate against the shared vocab.

Ships incrementally — step 1+2 is the operator's immediate ask; 3+4 follow.

## 6. Grade + [H]→[G] + honest deltas

`[H]` until the corpus probe is green. Deltas:
1. **C# stays C#** — "config over reusable" is a shared *contract* + engine for
   the Rust arms; C#'s executor conforms to the vocab, it is not literally
   rewired through the Rust engine (option A). Do not over-claim "one engine
   runs all four."
2. **`purpose` is Inferred** everywhere (heuristic), even where the signal is
   declared — a custom REST action / an unknown view tag falls to the fallback.
3. **`part_of` tier differs by `LocationSource`** — declared (Rails/Odoo) is
   Authoritative-eligible, inferred (C#) is Inferred; the tier is a per-arm
   config output, not a global constant. (Council: confirm the declared-parent
   Authoritative bump vs #81's flat Inferred — a `truth-architect` question.)
4. **The Rails `action:` extraction is genuinely new** parse work (mirrors the
   routes arm reading the same `{controller:, action:}` hash).

## 7. Gate + council

Central: `cargo test -p ruff_spo_triplet -p ruff_ruby_spo -p ruff_python_spo`
green · clippy + fmt clean · corpus probes green. Edit allowlist:
`ruff_spo_triplet/src/{quad.rs(new), lib.rs}`, `ruff_ruby_spo/src/menu_regions.rs`,
`ruff_python_spo/src/{odoo_regions.rs or a new odoo_quad adapter}`,
`ruff_csharp_spo/src/lib.rs` (conformance test only). **No `triple.rs` mint.**

Council:
1. **convergence-architect** — is the reusable-engine/config split real
   convergence or over-abstraction? Is `PurposeRule`+`classify_purpose` the
   right seam, or do the frontends' purpose signals differ too much to share
   one engine? Is the C# option-(A) conformance the honest boundary?
2. **baton-handoff-auditor** — the new `quad` module in the shared count-locked
   crate; the C# cross-language contract; the `MenuQuad` subject reusing the
   `RegionSubject` codec; no mint / count-lock untouched.
