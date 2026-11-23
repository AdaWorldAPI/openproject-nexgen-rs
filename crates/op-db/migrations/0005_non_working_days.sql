-- The table `WorkPackages::Shared::WorkingDays#non_working_dates` reads (`NonWorkingDay.pluck(:date)`,
-- app/services/work_packages/shared/working_days.rb:178-181): the dates that are not working days
-- on top of the non-working week days of `Setting.working_days`. Empty by default, as in a fresh
-- OpenProject.
--
-- OpenProject: db/structure.sql:5299-5305 (the columns) and :15401 (the unique index on `date`).

CREATE TABLE IF NOT EXISTS non_working_days (
    id          bigserial PRIMARY KEY,
    name        varchar NOT NULL,
    date        date NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS index_non_working_days_on_date ON non_working_days (date);
