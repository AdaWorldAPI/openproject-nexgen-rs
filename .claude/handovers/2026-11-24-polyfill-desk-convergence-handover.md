# Handover: polyfill + Quack desk, Klickwege pages, review-shape corpus (2026-11-24)

Branch: `claude/op-redmine-convergence-handover-m54i4j`, fast-forwarded to `origin/main` `924a325`
(it had no commits of its own; it was 192 behind).

Scope of this document: everything the session that ran PRs #165-#179 (plus the review of #180/#181 from
another session) knows to be open. Every claim below was checked against `origin/main` `924a325` on
2026-11-24 unless it says otherwise. The rows in `docs/deferred-bugs.md` stay the canonical bug ledger; this
file points into it and adds the work items, decisions and traps that are not bugs.

Reading order for the next session:
1. This file, §0-§2.
2. `docs/decisions/0001-journaled-edits.md`, `docs/decisions/0002-polyfill-quack-storage.md` (read §2.3 below
   first: 0002 is partly superseded).
3. `docs/deferred-bugs.md` (97 lines, every known gap with an id).
4. `docs/plans/polyfill-quack-migration.md` §5 (PR sequence). It is dated 2026-11-23 and written before the
   copy decision; §3 below maps its PRs to what is done.
5. `docs/ore-slag-reconciliation.md` §7, §10, §12 and `.claude/findings/2026-11-22-klickwege-work-packages.md`.

---

## 0. State in one screen

- **Storage migration (decision 0002):** S1 (desk), S2 (PostgreSQL parity oracle), S2b (default scopes), S2c
  (ORE/SLAG as config) and S3 (11 read-only desk pages) are merged. **Nothing writes through the desk yet.**
  Every write still goes to PostgreSQL through SQLx (`op_services::work_packages::mutation`,
  `op-server/src/write_guard.rs`, and since #181 `op-server/src/provision.rs`). `sqlx` is still a runtime
  dependency of `op-server` (`crates/op-server/Cargo.toml:31`). Next stage: **S4 = writes through the
  polyfill** (§3).
- **Pages:** `/`, `/work_packages/:id`, `/work_packages/:id/edit`, `/projects`, `/projects/:id`,
  `/projects/:id/edit` read PostgreSQL. `/statuses`, `/types`, `/priorities`, `/users`, `/journals`,
  `/relations`, `/time_entries` (list + detail each) read the desk (`crates/op-server/src/board/desk_views.rs`).
  `nav::NOT_YET_NAVIGABLE` is empty; every association of `ProjectWorkItem` and `Project` routes to a page.
- **Open PRs:** none in `openproject-nexgen-rs`, none in `crai-rs` (crai-rs #4 merged 2026-11-23).
- **Uncommitted / unpushed work anywhere:** none that matters (§9 lists the one local-only commit and why it
  can be dropped).
- **Deploy:** op.msgraph.de runs `main`. Sign-in (`OP_SIGNIN_*`) is not configured there (per #181), so
  everyone is anonymous and every edit is refused by design. The desk pages are served from the bundled demo
  seed (`crates/op-desk/seed/demo.json`), not from the Railway database.

---

## 1. What landed in this arc (for orientation, newest first)

| PR | What | Notes for the next session |
|---|---|---|
| #181 (other session) | sign-in map gains `admin=` / `projects=`; `provision.rs` creates the `oidc:<subject>` user row and memberships **in PostgreSQL** on a verified request | must be ported to the desk in S4 (§3.4) |
| #180 (other session) | `/api/v3` accepts verified bearer tokens (`OP_API_CLIENTS`, `OP_API_CALLERS`); `integration/work-items.toml` maps report-rs's four work-item calls onto `/api/v3` | OpenProject's `/api/v3` is Grape and not harvested by rails-codegraph; the mapping is checked against nexgen's own router only |
| #179 | desk pages for users, journals, relations, time entries; author/assignee links; has-many related links; nav empty of deferred lanes | review findings logged as M6-M9 |
| #178 | `AppState.desk` (`Arc<Mutex<Desk>>`), `OP_DESK_SEED`; statuses/types/priorities pages; WP detail links status/type/priority | |
| #177 | rails-polyfill: `save` on a destroyed record returns false after before_validation/before_save; after_rollback only when the row was written | resolved P1, narrowed P11 |
| #176 | ORE/SLAG reconciliation data in `oracle/charon/ore-slag/openproject.toml`, `[[learned]]` block, `UPDATE_ORE_SLAG=1` | |
| #175 | op-desk reads apply OpenProject's lowered `default_scope`s (generated `crates/op-desk/src/generated/default_scopes.rs`) | |
| #172-#174 | op-desk crate: polyfill Store + Quack catalog over OpenProject's own schema; PostgreSQL oracle test; µs timestamps | |
| #165-#168 | rails-polyfill copy hardened: optimistic locking, destroy, rollback state restore, commit coalescing, participant identity | ten review findings, graded by ActiveRecord; corpus now in crai-rs |

---

## 2. Decisions in force (and one stale record)

### 2.1 Owner decisions that bind S4 and later

1. **Storage:** writes through the Rails polyfill, reads through Quack (`quack-shadow` over lance-graph), state
   in a state directory. PostgreSQL is the parity oracle only, never runtime storage (decision 0002).
2. **Schema:** OpenProject's own (`oracle/openproject-structure.sql`, parsed by `op_desk::structure`), not
   nexgen's hand-written `op-db/migrations`. Differences are pinned in `oracle/op-desk-schema-divergence.txt`
   (111 at last count, M1) and fixed as the runtime moves.
3. **Copy, never depend:** the zammad-rs crates are copied into `crates/` (`rails-polyfill`, `rails-compiler`,
   `rails-f000`, `quack-shadow`, `polyfill-feed`, `web-login`) with a `COPIED_FROM.md` each. nexgen has no
   zammad-rs dependency. Fixes land in nexgen first; zammad-rs keeps its bugs until ported (deferred-bugs §2,
   Z1-Z9). The copies are meant to move to OGAR later, starting from nexgen's versions.
4. **Timestamps:** microseconds (done in the copy; #174 made op-desk keep PostgreSQL's precision).
5. **`quack-shadow`'s dependency weight** (rails-f000 → rails-compiler, ruff, OGAR) is accepted.
6. **Brand:** report-rs company identity as config (`OP_BRAND_*`, DATAGROUP default, #171). Layout
   `210 | 820 | 820 | 70`, one column on phones (#170).
7. **Dead menus become pages via the polyfill** (done for read-only pages in S3).
8. **ORE/SLAG data is autolearned config** (#176): generic code in `src/ore_slag_config.rs`, data in the toml.
9. **A behavioural divergence from OpenProject needs a decision record first** (write
   `docs/decisions/000N-*.md`, then the code).

### 2.2 Standing constraints

- Never modify `/home/user/adaworldapi/openproject` (read-only source) and never commit its `vendor/bundle`.
- `crates/web-login` stays byte-identical to zammad-rs.
- The zammad-copied crates keep zammad formatting: **never `cargo fmt` them**, never `cargo fmt --all`. Use
  `cargo fmt -p <nexgen crate>`. (This session broke the rule once in #177 and had to restore 9 files.)
- No token in any Dockerfile, ever. No secrets committed. No model identifier in commits or PR bodies.
- Commit trailer and PR footer as in the session's attribution reminder.

### 2.3 Stale record: decision 0002 says "upstream", the owner later said "copy"

`docs/decisions/0002-polyfill-quack-storage.md` rows "Types the polyfill and Quack lack … **Added upstream**, in
zammad-rs's `rails-polyfill`" and "PR 1 (upstream)" predate the copy decision. In fact Date, Float, µs
DateTime, optimistic locking and destroy were all added **in nexgen's copy**
(`crates/rails-polyfill/src/schema.rs:6-19` has `Date` and `Float`; `crates/rails-polyfill/COPIED_FROM.md`
"Changes made here since the copy"). **Open point D-0002:** write `docs/decisions/0003-copy-zammad-crates.md`
superseding those two rows (append-only: annotate 0002, do not rewrite it). Also `polyfill-quack-migration.md`
§5 PR 1/PR 2 describe a zammad-rs upstream PR and hand-declared specs; both are superseded.

---

## 3. Next stage: S4, writes through the polyfill (the main open work)

### 3.1 Where the migration plan stands, PR by PR

`docs/plans/polyfill-quack-migration.md` §5, mapped to `main` `924a325`:

| Plan PR | Status | Where |
|---|---|---|
| PR 0 deploy as is | done | op.msgraph.de; the pre-decision SQLx server is on branch `sqlx` with `Dockerfile.sqlx` |
| PR 1 polyfill types (Date, Float, µs, locking, destroy) | **done in nexgen's copy**, not upstream (§2.3) | `crates/rails-polyfill`, #165-#168, #177 |
| PR 2 op-desk crate, specs, seed | done, with specs generated from OpenProject's schema (not hand-declared) | `crates/op-desk`, #172-#175 |
| PR 3 EXECUTED_DB capture | done | `oracle/executed-db.pg.ndjson` (#162), compared in CI (`.github/workflows/ci.yml:21`) |
| PR 4 reads through Quack | **partly**: read-only pages only (S3). The API (`op-api`) and the WP/project pages still read PostgreSQL | `board/desk_views.rs` |
| PR 5 create through the polyfill | open | — |
| PR 6 update, locking, journal aggregation | open | — |
| PR 7 delete + remaining API resources | open | — |
| PR 8 state directory | open | — |
| PR 9 switch runtime, SQLx oracle-only | open | — |
| PR 10 Railway volume | open | — |

### 3.2 What the desk can and cannot do today (read before designing S4)

- `op_desk::desk::Desk { store: Store, catalog: quack_shadow::Catalog, unfed: Vec<Commit>, seed_time }`
  (`crates/op-desk/src/desk.rs:68-74`). Public API: `from_seed`, `read_all`, `read(table, col, value)`,
  `count`, `store()` (read-only `&Store`), `unfed()`, `seed_time()`. **There is no write method.** Committed
  store writes are journaled as `Commit`s, rendered by `polyfill_feed::feed_json` and applied to the catalog
  (module doc `desk.rs:1-17`); that delivery path exists but only the seed exercises it.
- `Desk` is `Send` but not `Sync` (the Quack catalog keeps `Cell`/`RefCell` caches), so `AppState` holds
  `Arc<Mutex<Desk>>` (`crates/op-server/src/desk_pages.rs:21`). The polyfill `Ctx` (needed to run
  `lifecycle::save` with callbacks) holds a `Box<dyn Host>` and is **not `Send`**. zammad-rs solves this with a
  dedicated desk thread plus oneshot replies (`zammad-ticket/src/service.rs`). Decide: **(a)** desk thread
  (the zammad-rs way, plan §1.4), or **(b)** keep the mutex and build a fresh `Ctx` per write over the locked
  store. (a) is the precedent and also gives the single-writer serialization that replaces the PostgreSQL
  advisory lock (`pg_advisory_xact_lock` in `mutation.rs` `update_in`, and the per-login lock in
  `provision.rs`).
- Desk reads apply lowered default scopes (filters ANDed via Quack `and`/`ne`; order = scope order, then id).
  Quack's integer lane holds `u32`: an id outside it is refused, and the pages pre-check with
  `u32::try_from` to return 404. Writes will hit the same lane bound for ids; allocation must stay below
  `u32::MAX` or Quack must widen the lane (lance-graph).
- Not applied by desk reads: the STI type condition (M6). Text `ORDER BY` and filters on unlaned columns
  (date, float, jsonb) are refused by Quack (M4). A DateTime filter has no settled encoding (M5: the oracle
  reads µs, Quack's timestamp lane seconds).

### 3.3 The write seams that already exist

- `op_services::work_packages::mutation` (3227 lines) is the one work-package write boundary. Its store traits
  are the seam to implement over the desk:
  - `MutationStore` (`mutation.rs:195`), `DeleteStore: MutationStore` (`:221`), `CreationStore: MutationStore`
    (`:228`).
  - PostgreSQL impls: `PgMutationStore` (`:990-1096`), `PgCreateScope` (`:1251`, `:2208`, `:2283`), the
    transaction wrapper.
  - A `Fake` impl drives the decision-logic tests (`:2504-2560`), so the decisions are already store-agnostic.
- `op-server/src/write_guard.rs`: `WpStore` trait (`:221`), `PgWpStore` (`:246`, `:275`): the page edit path
  (CSRF, RBAC, journaled edit per decision 0001).
- `op-server/src/provision.rs` (684 lines, #181): writes `users` and `members`/`member_roles` in one
  PostgreSQL transaction with an advisory lock on the login.

### 3.4 S4 work items, in a workable order

Each is one PR with a falsifier and a disable run, as before.

1. **Desk write path + the delivery loop.** Give `Desk` a write entry (`with_ctx(|ctx| …)` or a message API on a
   desk thread, see §3.2). After a committed write, `feed_json` → `catalog.apply_feed`, and `unfed` drains to 0.
   Falsifier: a write followed by `read` returns the new row; disable run: skip delivery, the read test fails
   (Quack's `UncommittedRead`/`dirty_tables` refusal may surface; check `quack-shadow` before assuming).
2. **WP create over the desk** (`DeskMutationStore: MutationStore + CreationStore`), running `create_in`
   unchanged. Port the post-insert steps from SQL to Rust over the store, in this order:
   - creation journal: `journals` row version `max+1`, `validity_period` (M3: no tstzrange type; decide the
     representation, plan §6 item 4), and the `work_package_journals` data row copy with `\r\n` → `\n`;
   - ancestor totals (`update_ancestors_of_created`);
   - `reschedule_after_create` (`reschedule.rs`; the pure `scheduling.rs` part is reusable);
   - creator as watcher.

   Parity: replay the create steps of `oracle/executed-db.pg.ndjson`. **Trap:** that capture records
   nexgen's SQL schema (e.g. `work_package_journals`, `done_ratio` NOT NULL), while the desk is OpenProject's
   schema. A declared column map is needed, built from `oracle/op-desk-schema-divergence.txt`; mismatches
   that are real nexgen-port bugs get fixed, not mapped away (owner decision 2).
3. **WP update** over the desk:
   - lock_version check (the polyfill now has optimistic locking, deferred-bugs Z1, so use it rather than
     hand-comparing);
   - `update_row` dates;
   - save;
   - `write_journal` (5-minute aggregation, cause rules, closing the predecessor's validity with
     `upper + 1µs`);
   - totals;
   - reschedule.

   Concurrency test: two page edits through the handle, the second gets `Conflict`.
4. **Provisioning over the desk** (port #181's `provision.rs` plan/apply to the store). Without this, a
   signed-in person is `Unprovisioned` as soon as reads move to the desk.
5. **WP and project pages read the desk** (`/`, `/work_packages/:id`, `/projects…`). They move **together**
   with the writes behind them, not before: a page that reads the desk while its form writes PostgreSQL never
   shows an edit (`desk_pages.rs` module doc). This ends M9 (cross-store ids).
6. **WP delete.** The polyfill now has a destroy pipeline; OpenProject's WP destroy cascades (journals,
   relations, watchers, children) were never read. Read OpenProject's `WorkPackage` destroy callbacks and
   `WorkPackages::DeleteService` first. A bare delete (today's behaviour) is a divergence that needs a
   decision record (§2.1 item 9).
7. **op-api over the desk** (105 routes in `op-api/src/routes.rs`; watchers, relations, time entries,
   categories, versions, queries and attachments call op-db repositories directly). Sorting by text is refused
   by Quack (M4): sort in Rust after `rows`, or record the refusal. This changes API behaviour if left
   refused; owner decision pending (plan §6 item 9). #180's bearer-token path resolves the caller via a user
   row, which must also come from the desk by then.
8. **State directory** (`OP_STATE_DIR`: `base.json` + `records/<seq:020>.json`, atomic writes, lock file,
   replay, compaction), ported from `zammad-ticket/src/store.rs`; `op_desk::snapshot` already gives the
   base-file shape.
9. **Switch:** `main.rs` builds no pool; `/health/ready` checks the desk and the state directory; `sqlx` leaves
   the runtime graph of op-server/op-api/op-services/op-models (falsifier: `cargo tree -e normal -p op-server`
   has no `sqlx`); `op-db` becomes the oracle crate.
10. **Railway:** volume at `/state`, `OP_STATE_DIR=/state`, seed via `OP_DESK_SEED`, drop the Postgres plugin.
    Migrating any live Railway rows into a base file is a one-off (plan §6 item 10; `op_desk::snapshot` can
    write it). The owner has not said whether the Railway database holds data worth keeping; **ask before
    dropping it.**

### 3.5 S4 decisions still open (from decision 0002 "Still open", plus new ones)

| # | Question | Context |
|---|---|---|
| Q1 | `journals.validity_period` (tstzrange) and `journals.cause` (jsonb) representation | M3; op-desk maps them to Text/Json today |
| Q2 | desk thread vs mutex + per-write `Ctx` | §3.2 |
| Q3 | Reads on the desk thread or a read-only catalog clone | plan §6 item 7 |
| Q4 | Pin lance-graph revisions? (`quack-shadow` and `rails-f000` use git `branch = "main"`, no Cargo.lock) | any lance-graph push can break the build |
| Q5 | Text sorts / date filters refused by Quack: sort in Rust or refuse | M4, plan §6 item 9 |
| Q6 | Live Railway data: migrate or start from seed | §3.4 item 10 |
| Q7 | WP delete semantics: bare vs OpenProject's destroy | §3.4 item 6 |
| Q8 | DateTime filter encoding (µs vs seconds) before the first such read | M5 |
| Q9 | STI condition lowering for `users` (and any other STI table) | M6; needed before the desk holds Groups / PlaceholderUsers |

---

## 4. Klickwege parity (the UI side of "zammad-rs parity")

Inventory: `.claude/findings/2026-11-22-klickwege-work-packages.md` (40 click paths from OpenProject's 21 work-
package feature specs, with Redmine ERB analogues). It was written against a 2026-11-22 tree; since then the
desk pages and the WP context-line links landed, but **none of its 20 MISSING rows was closed**. Still true on
`main`:

- **No create form** (rows 26-31, 32, 33): `/work_packages/new` is not routed. The request falls into
  `/work_packages/:id` with `Path<i64>` and gets axum's **plain-text 400** (`board.rs:622-623`), not a page.
  Smallest fix: route `GET/POST /work_packages/new` (optional `project_id`) before `:id`, reusing
  `render_wp_edit`'s form on an empty row. In S4 it must write through the desk (§3.4 item 2), or it lands on
  PostgreSQL and is rewritten weeks later.
- **No `/work_packages` or `/projects/:id/work_packages` list routes** (rows 1, 2), no "All open" default
  filter, and the not-found WP page returns HTTP 200 (row 6).
- **No activity/relations tabs** on the WP page (row 8). The desk now serves `/journals` and `/relations`
  globally, but not filtered to one WP (M8). A `?work_package_id=` filter on those desk lists closes both M8
  and most of row 8 cheaply, because `Desk::read(table, col, value)` already does an equality read.
- **No saved queries, search, My page** (rows 9-11). **No watch/delete/log-time HTML controls** (rows 35-37).
- **Field basis gaps** on the WP detail/edit (row 5, row 14): category, version, responsible, remaining time,
  spent time, custom fields. This is a basis change (`WP_EXTRA_FIELDS` in `board.rs`), not a layout change.
- **Write-side RBAC per field** (row 18): the render mask exists (`viewfilter::current_rbac_field_mask`), but
  the provider is `AnonymousRbac` (all positions) and `parse_wp_update` applies every posted field. A render
  mask is not a write gate.
- **Workflow-filtered status options** (rows 19, 20): the status select lists every status.
- **Phone navigation gap (new, found during this handover):** on phones the side menu region is hidden
  (`board.rs:2396`, `aside.sidemenu-region { display: none; }`) and the top bar shows only `nav::menu()` =
  Board and Projects (`nav.rs:363-365`, `board.rs:2267-2272`). The 11 desk pages have **no menu entry on a
  phone**. Fix: a collapsible menu or the side-menu entries in the wrapped top bar; keep the
  `MENU_NAV_EDGES` derivation test honest.
- **Klickwege G1/G2 first slice** (from the 2026-11-23 research, not yet started): harvest
  `app/views/work_packages`/Redmine `show.html.erb`'s `issue_fields_rows` into an oracle of field order and
  half-split, and compare against `WP_FIELD_SPLIT` in `board.rs`, the same way the reconciliation compares
  Ruby and Charon.

nav graph invariants to keep (`crates/op-server/src/nav.rs`): every new screen joins `SCREEN_UNIVERSE` and
`MENU_UNIVERSE`, every rendered cross-page link is a `ComputeEdge`, and the derivation tests
(`nav_edges_match_static_table`, `menu_nav_edges_match_derived`) stay green. Known under-statement: desk-page
outgoing links are not edges (M7).

---

## 5. Reconciliation (Rails vs Charon, ORE/SLAG)

- Ledger: `oracle/charon/reconcile-create-admission.ledger.json`, data in
  `oracle/charon/ore-slag/openproject.toml` (generic code `src/ore_slag_config.rs`, doc
  `docs/ore-slag-reconciliation.md` §12). Scorecard: 20 claims, 8 resolved, 1 evidenced, 4 open.
- Open residuals R1-R5 (`docs/deferred-bugs.md` §6), task #15 in the session list:
  - R1 `writable?` per submitted attribute: ruff does not lower `BaseContract#writable?`. Also undecided
    whether to compare against the base contract or the effective create contract. Owner: ContractHarvest
    (ruff side).
  - R2-R4: Charon cannot structure the **async** body of `op_services::work_packages::mutation::create`; R4
    also needs the Grape `/api/v3` endpoint harvested on the Ruby side (same gap #180 notes). Owner:
    RustExtraction. Options: a sync wrapper Charon can read, or teach the driver async state machines.
  - R5 is EVIDENCED only (drift check), not RESOLVED: needs a data witness.
- `RustBodyNotStructured` residuals want a per-symbol template (deferred from S2c).
- When a residual is fixed, `UPDATE_ORE_SLAG=1` rewrites the `[[learned]]` block. A brand-new residual gets a
  TODO stub and the test fails until it is filled. That is intended; do not silence it.

---

## 6. rails-polyfill and copied crates: bugs still open

From `docs/deferred-bugs.md` §1-§4, grouped by what unblocks them:

- **Needs per-participant restore snapshots:** P6 (state restore for every participant on outer rollback) and
  P11 (after_rollback recipients on destroy and for non-writing participants). The same root, one change.
- **Callback runner that can resume:** P5 (around_* callbacks run in the before slot).
- **Small, independent:** P2 destroy re-entrancy guard, P3 readonly records, P4 frozen destroyed record, P7
  per-model `lock_optimistically = false`, P8 stale-vs-NOT-NULL order (not checked against Rails), P10
  `run_commit_callbacks_on_first_saved_instances_in_transaction = false`, P12 non-integer primary keys, P13
  STI equality, P14 the 8.1 helper/doc vs OpenProject's `load_defaults 6.1`.
  - Each needs an `oracle/lock_rollback.rb` scenario first. The bug-shape recipes in crai-rs
    `fixtures/review-corpus/nexgen-165/shapes.md` say which scenario to build.
- **zammad-rs still has Z1-Z9.** Nobody ported them back. If the crates move to OGAR, start from nexgen's
  versions.
- **Copied-crate tests that do not run here:** T1 (67 quack-shadow tests SKIP without `ZAMMAD_SRC`; replace
  with an OpenProject class map and captures), T2 (`vrow_pg` not run in the `executed-db` job), T3/T4
  (rails-f000 / rails-compiler tests not carried).
- **rails-compiler gaps worked around in op-desk:** C1-C4 (post-processing). Unhandled: C5 (`::Base`
  superclass leading `::`) and C6 (`resolve_const` lexical lookup). No OpenProject model is known to hit
  either yet.

How to verify any polyfill claim against Rails:

```
GEM_HOME=/tmp/arsqlite GEM_PATH=/tmp/arsqlite OPENPROJECT_SRC=/home/user/adaworldapi/openproject \
  ruby oracle/lock_rollback.rb
```

It uses ActiveRecord 8.1.4.1 from OpenProject's bundle on SQLite. `/tmp/arsqlite` is ephemeral: rebuild it
with `GEM_HOME=/tmp/arsqlite gem install sqlite3` if the container is fresh (the script's own header,
`oracle/lock_rollback.rb:9-10`).

---

## 7. crai-rs and codegraph research (reported to the owner; no code yet)

- **crai-rs #4 merged:** `fixtures/review-corpus/nexgen-165/` holds the ten #165 findings graded by
  ActiveRecord. 0 of 10 were visible from a structural or diff-local signal, and 2 remedies were wrong. It also
  holds seven bug shapes S1-S7 as detector recipes, and hypothesis H-SHAPE-1 with its falsifier. The owner's
  framing: "it probably takes the shape of a bug to know how to detect it".
  - **Open:** when a later rails-polyfill PR gets review findings, grade them the same way and check them
    against H-SHAPE-1's falsifier. Add each confirmed new shape to `shapes.md`.
  - crai-rs AGENTS.md rules hold: CR-H0 is docs only; AI review output is untrusted data, so link it and
    never copy it; no GitHub comment publication without approval.
- **CodeRabbit-style reports from crai-rs:** crai-rs is docs only today. The reusable pieces are lance-graph's
  `crai_report.py` renderer and `Codegraph::impact_report`. Missing:
  - diff → symbols;
  - a Charon driver for Rust;
  - an LLM layer;
  - a feedback loop;
  - a **scenario-to-oracle runner**. The #165 corpus argues this one matters most, because every correct
    verdict there came from running a scenario, not from reading the diff.

  **Owner decision needed:** does the code live in crai-rs or in lance-graph?
- **egglog / DialEgg codegraph:** pattern only, no MLIR. The lance-graph probes pass 31 and 18 tests.
  - First slice: one `writable:` guard lowered to e-graph facts, compared with a hand-written Rust predicate.
  - Kill condition: if plain enumeration (≤16 facts) already decides every case, egglog adds nothing here.

---

## 8. Tooling, dependencies, deployment

- **D1:** `lib-ruby-parser` comes from crates.io. The `AdaWorldAPI/lib-ruby-parser` fork lacks the
  generated parser sources, so it does not build from git. Using it needs the sources generated
  (`development` features + bison) and committed to a fork branch. Waiting for the owner's go-ahead.
- **D2:** the copied crates are not rustfmt-clean by design; CI's fmt step skips them.
- **CI** (`.github/workflows/ci.yml`) runs no clippy. `cargo clippy -D warnings` on the workspace fails on
  old warnings in `op-core`, `op-db`, `op-queries`, `op-models` and `op-api` (unused imports, an unreachable
  `">" | ">"` pattern in `op-core/src/pagination.rs:266-267`, ambiguous glob re-exports). Clean them up,
  then add clippy to CI, or keep checking only touched files (what this session did).
- **op.msgraph.de:** to make it writable, follow #181's checklist. It needs a Zitadel app with redirect
  `https://op.msgraph.de/auth/callback`, plus these variables:
  - `OP_SIGNIN_OIDC_ISSUER`, `OP_SIGNIN_OIDC_CLIENT_ID`, `OP_SIGNIN_OIDC_REDIRECT_URL`,
    `OP_SIGNIN_SESSION_KEY`;
  - `OP_SIGNIN_MAP_TEXT` with `role=`/`rank=` rows plus `admin=`/`projects=`.

  For report-rs as a machine client (#180), also set `OP_API_CLIENTS` and `OP_API_CALLERS`. These are the
  owner's to set; they hold secrets, so never commit them or put them in a Dockerfile.
- **Permission/PII before real data** (M9): with sign-in off, `/users` shows every email and `/journals/:id`
  shows restricted journals. With sign-in on, the desk pages sit behind the page gate (`main.rs`
  `page_routes`), but they still apply no per-row permission check.

---

## 9. Local state in this container (all safe to drop)

- Every nexgen worktree (`/home/user/nexgen-*`, `/tmp/nx-*`) is clean and pushed, with one exception:
  `/tmp/nx-lock` (`claude/polyfill-locking-destroy`) has one local commit, `6ce2673`. Its patch is already on
  `main` as `0ddf7e9` (#168); `git cherry` shows `-`.
- `/tmp/zr-lock` (zammad-rs, `claude/polyfill-locking-destroy`) has a local-only commit `10d5d5d "wip
  locking+destroy"`, never pushed. It was the first attempt to add locking and destroy upstream in zammad-rs,
  before the copy decision. nexgen's copy supersedes it (Z1-Z4). It can be dropped; if Z1-Z4 are ever ported
  to zammad-rs, start from nexgen's `crates/rails-polyfill`, not from this wip.
- Build residue was deleted on 2026-11-23 to free disk (only `target/` directories). The worktrees under
  `/tmp` are ephemeral and can be removed with `git worktree prune`.

---

## 10. Suggested first moves for the next session

1. Write `docs/decisions/0003-copy-zammad-crates.md` (§2.3). It is small and removes a contradiction the next
   reader will trip on.
2. Decide Q2 (desk thread vs mutex) with the owner, or default to the zammad-rs desk thread and state it in the
   PR. Then build §3.4 item 1, the desk write path plus delivery loop, with its falsifier.
3. Do §3.4 items 2 and 4 (create + provisioning) together with the create form from §4, so the first HTML
   create lands on the desk directly.
4. Fix the phone navigation gap (§4). It is a few lines, and today 11 pages are unreachable on phones.
5. Keep grading review findings against the shape corpus (§7). It is the cheapest way to test H-SHAPE-1.
