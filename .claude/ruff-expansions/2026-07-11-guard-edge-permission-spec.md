# SPEC — the GuardEdge permission arm (`requires_permission`) — DRAFT, needs a MINT + council

> **Status: GREENLIT — mint `guarded_by_permission` (OQ-GUARD-1 measured, building).**
> The probe ran on the real OP corpus: 126 pushes, 90 with `if:`; permission-bearing
> = single 2 / conjunction 0 / **disjunction 1** / mixed 7 / dynamic 2; 11 distinct
> symbols; receiver-style 2. Only **1 pure disjunction / 90** (~1%, well under the
> ≲10% mint threshold — the architect had eyeballed ~4, but the multi-condition ones
> bin as `mixed`). **Encoding resolved:** mint the visibility-honest
> **`guarded_by_permission`** (NOT `requires_permission`) — it asserts a permission
> *appears in* the guard, never necessity, so it is correct across single/conjunction/
> mixed AND the lone disjunction without mis-encoding. All three council OPPORTUNITY-NOW
> conditions met (disjunctions rare · receiver-style extracted · dynamic counted).
> **MERGED — ruff PR #86 into main `498ff55`.** Mint `guarded_by_permission` (78→79, single count-site,
> Inferred) + the `if:`-proc harvest (bare-node subject, receiver-style handled,
> dynamic not fabricated). Gate: 169+167 green incl. 5 guard fixtures + real-corpus
> probe; count-lock 79; clippy+fmt clean.
> [SUPERSEDES the STAGED status + the `requires_permission` name throughout below.]
>
> Gap-ledger arm (d) from the 2026-07-09 choreography ruling: the **GuardEdge**
> — "what becomes legal after policy C" — the permission condition on a click
> path. Unlike the region/quad arms, this one needs a **predicate mint**
> (count-lock 78→79), so it is council-gated before any code. Base = ruff main
> (post #84, `7b0304f`).

## 1. The signal (grounded in the OP corpus)

Rails menu items carry their guard in the `if:` proc:

```ruby
menu.push :work_packages, { controller: "/work_packages", action: "index" },
  if: ->(_) { User.current.allowed_in_any_work_package?(:view_work_packages) && ... }
```

The **permission is the symbol argument** to an `allowed_*?` send:
`allowed_to?(:perm)`, `allowed_in_project?(:perm)`, `allowed_in_any_work_package?(:perm)`,
`allowed_globally?(:perm)`. Statically harvestable: walk the `if:` proc body's
AST for `Send{ method starts_with "allowed_", args: [Sym(perm), ...] }` and emit
one fact per distinct permission symbol. (Controller `before_action` filters are
a second, coarser source — deferred; the menu `if:` proc is the per-node signal
that joins the quad.)

## 2. The proposed predicate (THE mint decision — council rules)

`requires_permission` `(node, requires_permission, "<permission_symbol>")` —
the node is the bare `{ns}:{item}` menu-node IRI (same grammar as the quad's
`part_of`/`purpose`, so GuardEdge joins the quad by node). Tier:
**Inferred** (a proc-body heuristic — a proc can guard on non-permission
conditions too, and `&&`/`||`/method-indirection means extraction is
best-effort, not a declared literal).

This completes the menu quad's guard axis alongside the render/nav axes: a menu
node now carries identity (`surfaces_concept`) · location (`part_of`) · purpose
(`purpose`) · action (`navigates_to`) · **guard (`requires_permission`)**.

**Mint discipline (the delicate part):**
- `ruff_spo_triplet::Predicate` gains ONE variant `RequiresPermission`
  (`as_str`/`from_str`/`ALL`/`default_provenance = Inferred`); `ALL.len()`
  78 → 79; the count assertion moves to `predicate_count_locked_at_79` in the
  ONE assertion site (the #77 single-site lesson — no cross-crate re-assertion).
- No other predicate touched.

## 3. Harvest (Rails, `ruff_ruby_spo::menu_regions`)

Extend the existing `menu.push` walk (which already reads `parent:`/`action:`)
to also read the `if:` kwarg's value: when it is a proc/lambda (`Block` with a
`Send`-tree body), collect every `allowed_*?(:sym)` permission symbol. Add
`permissions: Vec<String>` to the harvest record + a `to_guard_triples(&self,
ns) -> Vec<Triple>` (one `requires_permission` per symbol). Root/leaf items with
no `if:` proc, or a proc with no `allowed_*?` call, emit nothing (honest — many
items are unguarded or guarded by non-permission conditions).

## 4. Honest deltas (do not paper over)

1. **Proc bodies are fuzzy.** `if: SomeClass.method` (method indirection),
   `if: ->(_) { Setting.foo? }` (non-permission guard), and `&&`/`||` chains
   mean extraction is best-effort. Inferred tier + a `with_permission` report
   counter (how many items yielded ≥1 permission) keeps the denominator honest.
2. **Controller `before_action` guards are NOT harvested** here — a coarser,
   different source (the whole-controller filter), deferred to a controller arm.
3. **The permission vocabulary is open** — `:view_work_packages` etc. are app
   symbols, emitted verbatim as the object (like `surfaces_concept` tokens); the
   classid/RBAC binding is downstream (the concept gate), not this arm's job.

## 5. Council questions (the gate before code)

1. **convergence-architect** — is `requires_permission` the right predicate and
   the right node grammar (bare `{ns}:{item}`, joining the quad)? Is the
   `allowed_*?(:sym)` proc-walk a clean-enough signal to mint for NOW, or is it
   too fuzzy — should the GuardEdge stage until a cleaner source (controller
   filters / a declared `:permission` kwarg) is available? Does it converge with
   the menu-quad plane or is it a separate concern?
2. **baton-handoff-auditor** — the mint: `ALL.len()` 78→79, single assertion
   site, `default_provenance = Inferred`, no cross-crate count re-assertion; the
   node-IRI grammar matches the quad's bare `{ns}:{item}`; no collision with the
   existing 78 predicates.

**Do not implement until both rule.** If the council says the signal is too
fuzzy to mint now, STAGE it (record the deferral) rather than shipping a
low-confidence predicate — the mint is permanent vocabulary.

## 6. Gate + allowlist (if greenlit)

Central: `cargo test -p ruff_spo_triplet -p ruff_ruby_spo` green · clippy + fmt
clean · `predicate_count_locked_at_79`. Allowlist: `ruff_spo_triplet/src/triple.rs`
(the one mint), `ruff_ruby_spo/src/menu_regions.rs` (the proc-walk + guard emit).
