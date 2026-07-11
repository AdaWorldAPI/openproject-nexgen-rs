# SPEC v2 — collapse the region arms onto one shared `RegionFact` + subject codec

> **v2 supersedes v1 after a 2-reviewer council (convergence-architect +
> baton-handoff-auditor). Both blessed the DTO+lift collapse as
> OPPORTUNITY-grade; both falsified v1's §2 subject recommendation.** The
> corrections are folded in below and flagged `[v2]`. Base = ruff main
> `4806298`, branch `claude/openproject-transcode-status-c6e8in`.
>
> **Status: READY (council-consolidated).** The delicate subject decision is
> resolved (see §2). Two P0s and two P1s the council raised are addressed.

## 0. Council consolidation — what changed from v1 `[v2]`

| v1 said | council found | v2 does |
|---|---|---|
| Only op-nexgen's digest consumes region subjects; not yet wired | `ruff_spo_triplet::nav_digest::build_nav_digest` is a **live in-repo consumer** parsing the **dot** grammar (`nav_digest.rs:24-45`), with a golden test | Treat `nav_digest` as the binding consumer; it is in the edit allowlist and migrates in the SAME PR |
| 2 producers (Rails, Odoo) | **3 producers** — `ruff_csharp_spo/harvester/Program.cs:377` (#76 origin) also emits **dot** via ndjson (out of Rust scope) | Canonical convention chosen to keep C# + Rails unchanged (both already dot); only **Odoo** migrates |
| Recommendation (A): canonicalize on `::`, **Rails** changes | Backwards — Rails+C#+digest already speak dot; `::` would regress the two working arms and strand C# | Canonical = **dot**; the arm that changes is **Odoo** |
| §2 is a "separator vote" | The real invariant is a **subject codec** (`to_iri`/`from_iri`) called by BOTH lift and digest; drift's root cause is the absence of a shared helper | Extract `RegionSubject { screen, control }` with `to_iri`/`from_iri` (decode via `rsplit_once`) |
| §6 fixture asserts `subject.split("::")` | False-green — the real consumer is `build_nav_digest`, which never splits `::` | §6 fixtures drive `build_nav_digest` + round-trip `from_iri(to_iri())` for all 3 producer shapes |
| §4 silent on Rails `dock_token` | Would silently blank the `docked_at` object | `dock_token = self.menu` stated explicitly (§4) |
| §1 "Odoo has its own *Report" | Inaccurate — `odoo_regions` has no report struct | Corrected (§1) |

## 1. What is redundant (measured, from the merged source)

| surface | #78 Rails | #79 Odoo | verdict |
|---|---|---|---|
| DTO | `RegionEntry { menu, item, parent, position, tab_order: Option<u32>, file }` | `RegionFact { screen, control, dock_token, tab_order: usize, opens_popup }` | **one resolved shape** — differ only in per-frontend harvest fields (`position`/`file`) |
| lift | `RegionEntry::to_triples(&self, ns)` | `RegionFact::{docked_at,tab_order,opens_popup}_triple` + free `region_triples` | **same emission**, copy-pasted |
| subject | `"{ns}:{menu}.{item}"` (dot) | `"{screen}::{control}"` (`::`, no ns) | **DIVERGENT — the crux, §2** |
| report | `RegionScanReport { … }` (Rails-local) | *(none — Odoo has no report struct)* `[v2]` | no coupling; stays per-frontend |
| Provenance | `Authoritative` (0.95/0.90) | `Authoritative` (0.95/0.90) | identical |

Both councils confirmed the DTO union `{screen, control, dock_token,
tab_order:Option<u32>, opens_popup:Option<String>, parent:Option<String>}` is
**honest, not lossy-forced**: every optional field gates a predicate that
already exists in the closed vocab (`DockedAt`/`TabOrder`/`OpensPopup`/
`ContainsControl`, `triple.rs:576,622,626,633`), so a frontend that doesn't
produce a fact passes `None` and emits nothing — behaviour-preserving for both.

## 2. THE CRUX — resolved: extract a subject CODEC, canonical = dot, migrate Odoo `[v2]`

The three producers + the one consumer today:

```
C#   #76  Program.cs:377   {ns}:{class}.{control}   dot    (ndjson, out of Rust scope)
Rails#78  menu_regions:766 {ns}:{menu}.{item}       dot
Odoo #79  odoo_regions     {screen}::{control}      ::  (no ns; screen = "{rel}#{view_id}")
consumer  nav_digest:24-45 strip_ns(':') then split_once('.')  → expects DOT
```

**Root cause of the drift (both councils):** there is no shared subject
helper — each arm hand-formats, and `nav_digest` hand-parses. The fix is a
codec, not a separator vote.

### The codec (new, in `ruff_spo_triplet`)

```rust
/// The canonical region-subject IRI codec. ONE encode/decode pair used by
/// BOTH the region lift (§3) AND `nav_digest` (§7), so the subject grammar can
/// never drift between producer and consumer again.
///
/// Canonical form: `"{screen}.{control}"`, where `screen` already carries any
/// namespace prefix the frontend uses (Rails `"{ns}:{menu}"`, Odoo
/// `"{rel}#{view_id}"`, C# `"{ns}:{class}"`). Decoding is `rsplit_once('.')`:
/// the control is the tail after the LAST dot. Sound because **no control on
/// any of the three frontends contains a dot** in the common case — Rails
/// symbols and C# member names never do; Odoo field/button names are Python
/// identifiers. The `.xml` dot in an Odoo screen is NOT the last dot (the
/// control follows it), so `rsplit` recovers `(screen, control)` correctly.
pub struct RegionSubject<'a> { pub screen: &'a str, pub control: &'a str }

impl<'a> RegionSubject<'a> {
    #[must_use]
    pub fn to_iri(&self) -> String { format!("{}.{}", self.screen, self.control) }
    /// `None` if there is no `.` (a malformed / control-less subject).
    #[must_use]
    pub fn from_iri(iri: &'a str) -> Option<(&'a str, &'a str)> { iri.rsplit_once('.') }
}
```

**Why dot, not `::`:** dot keeps **C# and Rails unchanged** (both already emit
it) and keeps the shipped `nav_digest` golden semantically stable; only Odoo
(the single out-of-line Rust arm) migrates. `::` would be more collision-proof
but forces Rails to re-emit AND strands the C# ndjson arm (convergence-architect
Finding 2 — a 3-way baton drop; the C# harvester is out of this PR's Rust
allowlist). Dot is the minimal-blast-radius, C#-safe choice.

**The Odoo migration:** `odoo_regions` builds its `RegionFact` (unchanged
harvest), then emits via the shared lift → `to_iri` produces
`"{rel}#{view_id}.{control}"`. Odoo's screen keeps `#`, gains no ns prefix
(none today); `nav_digest`'s `strip_ns` is a no-op on a colon-free Odoo screen,
then `rsplit_once('.')` splits screen/control. Odoo's emitted subject changes
`{screen}::{control}` → `{screen}.{control}` — but Odoo's digest path does not
work today anyway (it was never `.`-parseable), so this is a fix, not a
regression.

### Fenced edge `[v2]`
A **dotted Odoo control** (a related-field path like
`<field name="currency_id.symbol"/>`) would make `rsplit_once('.')` grab
`symbol` as the control. This is rare (region-docked leaves are normally
top-level fields/buttons, not related-field paths), but real. **Fence:** the
Odoo harvest MUST count any control containing `.` into a new
`RegionScanReport`-style `dotted_control` counter and (debug) assert it stays 0
on the corpus; if a real corpus surfaces one, escalate the separator to `::`
with the C# migration filed as a paired follow-up. Documented [H], not a silent
gap.

## 3. The shared DTO + lift (in `ruff_spo_triplet`, a new `region` module)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionFact {
    /// Fully-qualified screen id (namespace ALREADY applied — Rails
    /// `"{ns}:{menu}"`, Odoo `"{rel}#{view_id}"`). Subject = `{screen}.{control}`.
    pub screen: String,
    pub control: String,
    /// Raw innermost container token; `region=` config maps it downstream.
    pub dock_token: String,
    /// Resolved 0-based sibling ordinal. `Option` so a harvester that fails to
    /// assign one surfaces as `None` (+ a report counter), not a silent 0.
    pub tab_order: Option<u32>,
    /// `Some(action)` for a popup-opening control (Odoo `<button type=action>`;
    /// Rails `None` — Primer/Angular deferred).
    pub opens_popup: Option<String>,
    /// `Some(parent_control)` for a nesting edge (Rails `parent:`; Odoo `None`).
    pub parent: Option<String>,
}

impl RegionFact {
    #[must_use]
    pub fn subject(&self) -> String {
        RegionSubject { screen: &self.screen, control: &self.control }.to_iri()
    }
    #[must_use]
    pub fn to_triples(&self) -> Vec<Triple> {
        let s = self.subject();
        let mut out = vec![Triple::new(
            s.clone(), Predicate::DockedAt, self.dock_token.clone(), Provenance::Authoritative,
        )];
        if let Some(n) = self.tab_order {
            out.push(Triple::new(s.clone(), Predicate::TabOrder, n.to_string(), Provenance::Authoritative));
        }
        if let Some(a) = &self.opens_popup {
            out.push(Triple::new(s.clone(), Predicate::OpensPopup, a.clone(), Provenance::Authoritative));
        }
        if let Some(p) = &self.parent {
            let parent_subject = RegionSubject { screen: &self.screen, control: p }.to_iri();
            out.push(Triple::new(parent_subject, Predicate::ContainsControl, s, Provenance::Authoritative));
        }
        out
    }
}

#[must_use]
pub fn region_triples(facts: &[RegionFact]) -> Vec<Triple> {
    facts.iter().flat_map(RegionFact::to_triples).collect()
}
```

Emission order: `docked_at, [tab_order], [opens_popup], [contains_control]`.
Union of both arms; each predicate gated on its `Option`.

## 4. What each frontend keeps (harvest untouched) `[v2 corrections inline]`

- **#78 `menu_regions.rs`:** keep `Position`, the single-pass `resolve_group`
  TreeNode replay, `RegionScanReport`, the AST walk, the dynamic each-loop
  expansion. Its output adapter builds the shared `RegionFact`:
  - `screen = format!("{ns}:{menu}")` (fold the namespace in — Rails' `to_triples`
    took `ns` as a param; the shared lift does not),
  - `control = item`,
  - **`dock_token = menu`** (the bare region token — the object of `docked_at`;
    v1 was silent on this) `[v2]`,
  - `parent = parent`, `tab_order = <resolved Option<u32>>`, `opens_popup = None`.
  Delete `RegionEntry::to_triples`. `RegionEntry` MAY stay as the harvest-time
  record (it still needs `position`/`file` for the report) + gain
  `fn to_fact(&self) -> RegionFact`. Port fixture (h) (`menu_regions.rs:1036`,
  asserts `docked_at → top_menu`) to the shared lift so the `dock_token`
  mapping is guarded.
- **#79 `odoo_regions.rs`:** keep `REGION_CONTAINERS`, the element-stack scan,
  the depth-0 comodel rule. `RegionFact` becomes a re-export of the shared type;
  delete the three `*_triple` methods + local `region_triples`.
  - `tab_order: usize → Some(u32::try_from(order).unwrap_or(u32::MAX))` — the
    non-panicking cast, mirroring `menu_regions.rs:402` (NOT `as`, NOT `.unwrap()`) `[v2]`,
  - `parent = None`, `opens_popup` unchanged,
  - add the `dotted_control` fence counter (§2).
  Odoo's emitted subject shifts `::` → `.` (via the codec) — behaviour-changing
  on the wire but its digest path was never `.`-parseable, so net-fix.

## 5. Predicate plane — untouched (council-verified CLEAN)

No mint. `RegionFact`/`RegionSubject`/lift live in a new `region` module beside
`triple`/`nav_digest`; they add NO `Predicate` variant and use only the four
existing predicates. `predicate_count_locked_at_76` (`triple.rs:1214`,
`ALL.len()==76`) stays green with **no edit** — verify, don't touch. `[v2]` The
new module MUST be registered in `ruff_spo_triplet/src/lib.rs` (`mod region;` +
`pub use`) — a lib.rs-orphan tripwire the baton auditor flagged (BAP3).

## 6. Tests `[v2 rewritten]`

- **The lift contract moves to `ruff_spo_triplet`** — port #79's six triple-shape
  fixtures + #78's `to_triples_emits_*` to test the shared `RegionFact::to_triples`.
- **Codec round-trip fixture (the falsifier for the crux):** for all THREE
  producer subject shapes — Rails `openproject:top_menu` / `projects`, Odoo
  `widget_views.xml#view` / `partner_id`, C# `app:CipherPanel` / `grid1` —
  assert `RegionSubject::from_iri(fact.to_iri()) == Some((screen, control))`.
  Include a dotted-Odoo-screen case (`widget_views.xml#view`) to prove the
  `.xml` dot is not mis-taken as the boundary.
- **Digest-driven fixture (kills the v1 false-green):** build a mixed vec of
  Rails + Odoo (+ synthetic C#) `RegionFact`s, run `region_triples` →
  `build_nav_digest`, and assert the actual `[regions]` section groups
  `(screen, region)` correctly — NOT a standalone `split`. Update the existing
  `[regions]` golden (`nav_digest.rs:454-478`, currently `app:CipherPanel.grid1`
  dot-shape) and re-validate it against the migrated `rsplit` decoder.
- **Keep** both frontends' harvest fixtures + corpus probes unchanged (now
  asserting `RegionFact` fields). Rails 137 items / 0 unresolved and Odoo 51
  facts must stay green — the behaviour-preserving proof. Add the Odoo
  `dotted_control == 0` corpus assertion.
- **The `[H]→[G]` probe** (`six-region-layout-port.md:142`): mixed Rails+Odoo
  triples → `nav_digest [regions]` groups correctly. CONJECTURE until green;
  this is what promotes the structure oracle.

## 7. Gate + process `[v2]`

Central: `cargo test -p ruff_ruby_spo -p ruff_python_spo -p ruff_spo_triplet`
green · clippy + fmt clean on the three touched crates · both corpus probes
green under their env gates.

**Edit allowlist (v2, expanded):**
- `ruff_spo_triplet/src/region.rs` (new — `RegionSubject` + `RegionFact` + lift),
- `ruff_spo_triplet/src/lib.rs` (register `mod region;` + re-export),
- **`ruff_spo_triplet/src/nav_digest.rs`** (migrate `strip_ns`/`screen_of`/
  `control_of` to call `RegionSubject::from_iri` = `rsplit_once('.')`; update
  the `[regions]` golden) `[v2 — the P0 the baton auditor caught]`,
- `ruff_ruby_spo/src/menu_regions.rs` (adapter + delete lift, keep harvest),
- `ruff_python_spo/src/odoo_regions.rs` (re-export + delete lift + `try_from` +
  `dotted_control` fence).
- **NO `triple.rs` predicate edit.**

**Out of this PR (filed, not silent):** the C# `ruff_csharp_spo/harvester`
already emits canonical dot, so no change needed — but note in the PR body that
IF the fenced dotted-Odoo-control edge ever forces `::`, C#'s harvester
migration becomes a paired follow-up (route to PP-15).

Council done (this v2 IS the consolidation). Implementation may start.
