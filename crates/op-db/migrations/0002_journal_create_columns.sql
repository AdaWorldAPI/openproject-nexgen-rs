-- Columns the work-package creation journal writes, which 0001 left out.
--
-- OpenProject's `Journals::CreateService` (app/services/journals/create_service.rb:465-507,
-- `insert_journal_sql`) writes `journals.validity_period = tstzrange(<created_at>, NULL)` and
-- copies every column of `work_package_journals` from the work package (`insert_data_sql`,
-- :389-404; the journaled columns are every column of that table except `id`,
-- app/models/journal/base_journal.rb:40-41). Schema: db/structure.sql `journals` (:3965) and
-- `work_package_journals` (:8858).
--
-- Not ported: the CHECK `journals_validity_period_not_empty` (it would reject journal rows other
-- code in this repository inserts without a validity_period), and the `work_packages` columns
-- this schema has never had (derived_*, ignore_non_working_days is kept at its default).

ALTER TABLE journals ADD COLUMN IF NOT EXISTS validity_period tstzrange;

ALTER TABLE work_package_journals ADD COLUMN IF NOT EXISTS story_points integer;
ALTER TABLE work_package_journals ADD COLUMN IF NOT EXISTS remaining_hours double precision;

-- `work_package_journals.priority_id` stays NOT NULL, as in OpenProject (structure.sql:8868). The
-- column on `work_packages` is nullable (:9020), but the create defaults the priority before the
-- save (`IssuePriority.active.default`, set_attributes_service.rb:237-239) and the model
-- validation `validates :priority, presence: true` (work_package/validations.rb:34) refuses a
-- work package without one, so the snapshot never holds NULL; the Rust create does the same.
