# 0001 — Edits without a journal author, and work packages without a priority

Status: accepted (owner, 2026-11-23). Follows nexgen #147, which journals every work-package edit.

## 1. A work package without a priority

**OpenProject.** `work_packages.priority_id` is nullable (`db/structure.sql`, `work_packages`), but
the model validates `validates :priority, ..., presence: true`
(`app/models/work_package/validations.rb:34`), and `ModelContract#valid?` runs the model
validations on every contract (`app/contracts/model_contract.rb:85-88`, `validate_model?` is
`true`), the update contract included. The default priority is only set for a new record
(`app/services/work_packages/set_attributes_service.rb:112-113`). So an edit of a work package
whose priority is NULL, which does not set one, is refused with `priority: blank` (HTTP 422) and
nothing is written.

**Port before this decision.** The edit passed admission, and the journal insert then failed on
`work_package_journals.priority_id NOT NULL`: HTTP 500, rolled back.

**Decision.** Port the presence validation: the edit is refused with `priority: blank` before any
write. This is parity, not a divergence. An edit that supplies a priority succeeds and is
journaled.

**Not done: backfilling.** Setting the default priority on existing rows would make them editable
without the user choosing a priority, but it writes data nobody entered and that OpenProject never
writes. Revisit if such rows turn up in a deployed database (`SELECT count(*) FROM work_packages
WHERE priority_id IS NULL`).

## 2. Edits by the anonymous demo caller

**OpenProject.** An anonymous edit, where the Anonymous role is granted `edit_work_packages`, is
journaled as the `AnonymousUser` row (`users.type = 'AnonymousUser'`).

**Port.** This schema has no `users.type` column, so no anonymous user can be named. Before this
decision the demo posture (`OP_ALLOW_ANONYMOUS`) applied such edits without any journal.

**Decision (deliberate divergence).** The anonymous caller may not edit, demo posture or not
(HTTP 401, nothing written). Viewing is unchanged. An edit the history cannot attribute is worse
than no edit in a demo.

**Revisit when** `users.type` (and the `AnonymousUser`, `SystemUser` and `DeletedUser` rows) is
ported: then journal anonymous edits as OpenProject does and drop this refusal.
