# Finding: the pure-ClassView path is a PER-APP ClassView from the transpile — not enriching the shared canonical class

**Date:** 2026-07-08 · **Context:** the render-bake board (PR #86) renders
`/`, detail, and edit pages via `ClassView × WideFieldMask → ogar_render_askama`.

## The observed gap
`crates/op-server/src/board.rs::wp_basis()` = `basis(project_work_item(),
WP_EXTRA_FIELDS)`. `ogar_vocab::project_work_item()` carries only
**associations** (project/status/type/priority/author/assignee + has_many
journals/relations/time_entries) and **zero scalar attributes**, so the board
augments the basis with `WP_EXTRA_FIELDS` = the DB-row scalars (subject,
description, done_ratio, start/due_date, estimated_hours, lock_version) plus
the form FK keys (status_id/type_id/priority_id/assigned_to_id). The projection
is therefore not yet *pure* ClassView.

## The decision (autonomous, 2026-07-08): do NOT hand-enrich the shared class
`project_work_item()` is the **shared canonical concept** (OGAR hi-u16). Per
`OGAR/docs/OGAR-CONSUMER-BEST-PRACTICES.md` + the OGAR CLAUDE.md non-negotiables:
the **lo-u16 per-app ClassView** chooses the render skin; the shared concept is
RBAC/ontology identity, not an app's field list. Adding attributes to
`project_work_item()` would silently change the render of **every** consumer
(odoo-rs, medcare-rs, …) that renders the same canonical class — a fleet-wide
behavior change that OGAR doctrine routes through the 5+3 council + operator
sign-off, never a solo drive-by. So op-nexgen keeps `WP_EXTRA_FIELDS` as an
honest, documented bridge.

## The correct pure path (the real follow-up)
A **per-app OpenProject `WorkPackage` ClassView** whose `attributes` carry the
OP scalar columns — produced by the **transpile** (`ogar_from_ruff` /
`ogar_from_rails` lifting the real OP/Redmine `WorkPackage` model, or baked via
`op-generated`), resolved by classid → ClassView. Then
`wp_basis() = basis(op_work_package_classview(), &[])` with NO augmentation, and
`from_universe_present(universe = pure ClassView basis, present = skin fields)`
mints against the real attribute set.

Two legitimate residues either way:
- The **form FK keys** (`status_id`…) are the wire-encoding of the belongs_to
  edges (and `assignee → assigned_to_id` doesn't even match the role name), so
  they are a form concern, not ClassView attributes — they stay explicit.
- `lock_version` is an optimistic-lock concurrency field (op-db `WHERE
  lock_version=$`), arguably infra rather than a canonical attribute.

## Verification hook to add when the per-app ClassView lands
A test asserting `wp_basis()` (minus the FK-form-keys) == the renderable
`op_db::work_packages::WorkPackageRow` scalar columns — the field-level "1:1
check" (the nav crawl's sibling, for fields not links), so the render basis
can't drift from the DB shape.
