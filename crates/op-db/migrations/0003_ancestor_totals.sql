-- Columns the ancestor-totals update of a work-package create needs.
--
-- OpenProject's `WorkPackages::UpdateAncestorsService` derives, for every ancestor of a changed
-- work package, `derived_estimated_hours`, `derived_remaining_hours` and `derived_done_ratio`
-- (app/services/work_packages/update_ancestors_service.rb:210-235, :103-135) and skips descendants
-- whose status is `excluded_from_totals`
-- (`WorkPackage#included_in_totals_calculation?`, app/models/work_package.rb:295-297).
-- Schema: db/structure.sql `work_packages` (:9010; derived columns :9030, :9035-9036) and
-- `statuses` (:7688; `excluded_from_totals` :7698).
--
-- `work_package_journals` already has the three derived columns (0001), so the journal snapshot
-- needs nothing new. `ignore_non_working_days` stays out of `work_packages`: nothing here derives it.

ALTER TABLE work_packages ADD COLUMN IF NOT EXISTS derived_estimated_hours double precision;
ALTER TABLE work_packages ADD COLUMN IF NOT EXISTS derived_remaining_hours double precision;
ALTER TABLE work_packages ADD COLUMN IF NOT EXISTS derived_done_ratio integer;

-- structure.sql:7698: `boolean DEFAULT false NOT NULL`.
ALTER TABLE statuses ADD COLUMN IF NOT EXISTS excluded_from_totals boolean NOT NULL DEFAULT false;
