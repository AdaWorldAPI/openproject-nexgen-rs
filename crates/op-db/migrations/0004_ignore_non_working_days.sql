-- The column `UpdateAncestorsService#derive_ignore_non_working_days` derives for an ancestor.
--
-- OpenProject: `work_packages.ignore_non_working_days boolean DEFAULT false NOT NULL`
-- (db/structure.sql:9034). `work_package_journals.ignore_non_working_days` already exists (0001,
-- structure.sql:8879), so the journal snapshot needs no new column; the journal insert and its
-- "changed" comparison now copy and compare this one (0002 had kept it at its default).
-- The derivation itself: app/services/work_packages/update_ancestors_service.rb:183-199.

ALTER TABLE work_packages ADD COLUMN IF NOT EXISTS ignore_non_working_days boolean NOT NULL DEFAULT false;
