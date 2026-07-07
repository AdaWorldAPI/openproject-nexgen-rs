-- op-db :: 0001_init.sql
-- generated 2026-07-07
--
-- Schema derived mechanically from crates/op-db/src/*.rs (every `FromRow`
-- struct, every `FROM`/`INTO`/`JOIN` table name, every column referenced in a
-- SELECT list, WHERE clause, ORDER BY, or UPDATE SET). Cross-referenced
-- against the OpenProject Rails corpus at
-- /tmp/op-corpus/db/migrate/tables/*.rb for authoritative column types and
-- nullability. This is NOT a faithful port of the OpenProject schema — it is
-- exactly what op-db's SQL needs, and no less. Columns present in the real
-- OpenProject schema but never touched by op-db (e.g. projects.templated,
-- users.type, types.attribute_groups) are intentionally omitted.
--
-- Divergences from the Rails corpus, called out where op-db's need wins:
--   * relations: op-db's RelationRow requires created_at/updated_at
--     (NOT NULL); the real OpenProject `relations` table has no timestamps
--     at all (see relations.rb, "Rails/CreateTableWithTimestamps" disabled).
--   * query_menu_items: op-db queries a dedicated `query_menu_items` table
--     directly. Real OpenProject uses a single-table-inheritance
--     `menu_items` table (type = 'Queries::MenuItem' or similar) via
--     `navigatable_id`. We model only the columns op-db's queries.rs
--     actually touches: id, navigatable_id, name, title.
--   * work_packages gains `position`, `story_points`, `remaining_hours`
--     columns beyond the base Rails schema because
--     query_executor.rs::WorkPackageRow selects them (schedule_manually and
--     duration ARE in the Rails schema already).
--
-- Idempotent: safe to re-run (CREATE TABLE IF NOT EXISTS throughout).

-- =====================================================================
-- users
-- Mirrors: users.rs::UserRow. Corpus: db/migrate/tables/users.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS users (
    id                bigserial PRIMARY KEY,
    login             text NOT NULL DEFAULT '',
    firstname         text NOT NULL DEFAULT '',
    lastname          text NOT NULL DEFAULT '',
    mail              text NOT NULL DEFAULT '',
    admin             boolean NOT NULL DEFAULT false,
    status            integer NOT NULL DEFAULT 1,
    language          text,
    hashed_password   text,
    salt              text,
    last_login_on     timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now()
);

-- =====================================================================
-- roles
-- Mirrors: roles.rs::RoleRow. Corpus: db/migrate/tables/roles.rb
-- Note: DB column is literally "type" (sqlx renames it to `role_type`).
-- =====================================================================
CREATE TABLE IF NOT EXISTS roles (
    id          bigserial PRIMARY KEY,
    name        text NOT NULL DEFAULT '',
    position    integer NOT NULL DEFAULT 1,
    builtin     integer NOT NULL DEFAULT 0,
    type        text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

-- =====================================================================
-- role_permissions
-- Mirrors: roles.rs::RolePermissionRow + add_permission() INSERT.
-- Corpus: db/migrate/tables/role_permissions.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS role_permissions (
    id          bigserial PRIMARY KEY,
    role_id     bigint,
    permission  text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_role_permissions_role_id ON role_permissions (role_id);

-- =====================================================================
-- projects
-- Mirrors: projects.rs::ProjectRow. Corpus: db/migrate/tables/projects.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS projects (
    id          bigserial PRIMARY KEY,
    name        text NOT NULL DEFAULT '',
    description text,
    identifier  text NOT NULL,
    public      boolean NOT NULL DEFAULT true,
    parent_id   bigint,
    lft         integer,
    rgt         integer,
    active      boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

-- =====================================================================
-- types (work package types)
-- Mirrors: types.rs::TypeRow. Corpus: db/migrate/tables/types.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS types (
    id             bigserial PRIMARY KEY,
    name           text NOT NULL DEFAULT '',
    position       integer NOT NULL DEFAULT 1,
    is_default     boolean NOT NULL DEFAULT false,
    is_in_roadmap  boolean NOT NULL DEFAULT true,
    is_milestone   boolean NOT NULL DEFAULT false,
    is_standard    boolean NOT NULL DEFAULT false,
    color_id       bigint,
    description    text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

-- =====================================================================
-- projects_types (join table: which types are enabled for which project)
-- Mirrors: types.rs::enable_for_project / find_by_project.
-- Corpus: db/migrate/tables/projects_types.rb (id: false in Rails)
-- =====================================================================
CREATE TABLE IF NOT EXISTS projects_types (
    project_id  bigint NOT NULL,
    type_id     bigint NOT NULL,
    UNIQUE (project_id, type_id)
);

CREATE INDEX IF NOT EXISTS idx_projects_types_project_id ON projects_types (project_id);
CREATE INDEX IF NOT EXISTS idx_projects_types_type_id ON projects_types (type_id);

-- =====================================================================
-- statuses (work package statuses / kanban columns)
-- Mirrors: statuses.rs::StatusRow. Corpus: db/migrate/tables/statuses.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS statuses (
    id                   bigserial PRIMARY KEY,
    name                 text NOT NULL DEFAULT '',
    is_closed            boolean NOT NULL DEFAULT false,
    is_default           boolean NOT NULL DEFAULT false,
    is_readonly          boolean NOT NULL DEFAULT false,
    position             integer NOT NULL DEFAULT 1,
    default_done_ratio   integer NOT NULL DEFAULT 0,
    color_id             bigint,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now()
);

-- =====================================================================
-- enumerations (shared table: IssuePriority rows + TimeEntryActivity rows,
-- discriminated by `type`)
-- Mirrors: priorities.rs::PriorityRow, activities.rs::ActivityRow.
-- Corpus: db/migrate/tables/enumerations.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS enumerations (
    id          bigserial PRIMARY KEY,
    type        text NOT NULL,
    name        text NOT NULL DEFAULT '',
    position    integer NOT NULL DEFAULT 1,
    is_default  boolean NOT NULL DEFAULT false,
    active      boolean NOT NULL DEFAULT true,
    color_id    bigint,
    project_id  bigint,
    parent_id   bigint,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_enumerations_type ON enumerations (type);
CREATE INDEX IF NOT EXISTS idx_enumerations_project_id ON enumerations (project_id);

-- =====================================================================
-- versions
-- Mirrors: versions.rs::VersionRow. Corpus: db/migrate/tables/versions.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS versions (
    id               bigserial PRIMARY KEY,
    project_id       bigint NOT NULL,
    name             text NOT NULL DEFAULT '',
    description      text,
    effective_date   date,
    start_date       date,
    status           text NOT NULL DEFAULT 'open',
    sharing          text NOT NULL DEFAULT 'none',
    wiki_page_title  text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_versions_project_id ON versions (project_id);

-- =====================================================================
-- members
-- Mirrors: members.rs::MemberRow. Corpus: db/migrate/tables/members.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS members (
    id           bigserial PRIMARY KEY,
    user_id      bigint NOT NULL,
    project_id   bigint,
    entity_type  text,
    entity_id    bigint,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_members_project_id ON members (project_id);
CREATE INDEX IF NOT EXISTS idx_members_user_id ON members (user_id);

-- =====================================================================
-- member_roles
-- Mirrors: members.rs::MemberRoleRow + get_role_ids/set_roles queries.
-- Corpus: db/migrate/tables/member_roles.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS member_roles (
    id               bigserial PRIMARY KEY,
    member_id        bigint NOT NULL,
    role_id          bigint NOT NULL,
    inherited_from   bigint
);

CREATE INDEX IF NOT EXISTS idx_member_roles_member_id ON member_roles (member_id);
CREATE INDEX IF NOT EXISTS idx_member_roles_role_id ON member_roles (role_id);

-- =====================================================================
-- categories
-- Mirrors: categories.rs::CategoryRow. Corpus: db/migrate/tables/categories.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS categories (
    id              bigserial PRIMARY KEY,
    project_id      bigint NOT NULL,
    name            text NOT NULL DEFAULT '',
    assigned_to_id  bigint,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_categories_project_id ON categories (project_id);

-- =====================================================================
-- work_packages
-- Mirrors: work_packages.rs::WorkPackageRow AND
-- query_executor.rs::WorkPackageRow (the latter adds position, story_points,
-- remaining_hours, schedule_manually, duration). Corpus:
-- db/migrate/tables/work_packages.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS work_packages (
    id                 bigserial PRIMARY KEY,
    subject            text NOT NULL DEFAULT '',
    description        text,
    project_id         bigint NOT NULL,
    type_id            bigint NOT NULL,
    status_id          bigint NOT NULL,
    priority_id        bigint,
    author_id          bigint NOT NULL,
    assigned_to_id     bigint,
    responsible_id     bigint,
    category_id        bigint,
    version_id         bigint,
    parent_id          bigint,
    start_date         date,
    due_date           date,
    estimated_hours    double precision,
    done_ratio         integer NOT NULL DEFAULT 0,
    lock_version       integer NOT NULL DEFAULT 0,
    position           integer,
    story_points       integer,
    remaining_hours    double precision,
    schedule_manually  boolean NOT NULL DEFAULT true,
    duration           integer,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_work_packages_project_id ON work_packages (project_id);
CREATE INDEX IF NOT EXISTS idx_work_packages_status_id ON work_packages (status_id);
CREATE INDEX IF NOT EXISTS idx_work_packages_assigned_to_id ON work_packages (assigned_to_id);
CREATE INDEX IF NOT EXISTS idx_work_packages_parent_id ON work_packages (parent_id);

-- =====================================================================
-- work_package_journals (versioned snapshot data referenced by journals.data_id
-- when journals.data_type = 'Journal::WorkPackageJournal')
-- Mirrors: journals.rs::WorkPackageJournalRow.
-- Corpus: db/migrate/tables/work_package_journals.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS work_package_journals (
    id                        bigserial PRIMARY KEY,
    type_id                   bigint NOT NULL,
    project_id                bigint NOT NULL,
    subject                   text NOT NULL,
    description               text,
    due_date                  date,
    category_id               bigint,
    status_id                 bigint NOT NULL,
    assigned_to_id            bigint,
    priority_id               bigint NOT NULL,
    version_id                bigint,
    author_id                 bigint NOT NULL,
    done_ratio                integer,
    estimated_hours           double precision,
    start_date                date,
    parent_id                 bigint,
    responsible_id            bigint,
    derived_estimated_hours   double precision,
    schedule_manually         boolean,
    duration                  integer,
    ignore_non_working_days   boolean NOT NULL DEFAULT false,
    derived_remaining_hours   double precision,
    derived_done_ratio        integer
);

-- =====================================================================
-- journals (polymorphic change history: journable_type/journable_id)
-- Mirrors: journals.rs::JournalRow. Corpus: db/migrate/tables/journals.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS journals (
    id               bigserial PRIMARY KEY,
    journable_type   text NOT NULL,
    journable_id     bigint NOT NULL,
    user_id          bigint NOT NULL,
    notes            text,
    version          integer NOT NULL DEFAULT 0,
    data_type        text NOT NULL,
    data_id          bigint NOT NULL,
    cause            jsonb NOT NULL DEFAULT '{}',
    restricted       boolean NOT NULL DEFAULT false,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_journals_journable ON journals (journable_type, journable_id);
CREATE INDEX IF NOT EXISTS idx_journals_user_id ON journals (user_id);

-- =====================================================================
-- attachments
-- Mirrors: attachments.rs::AttachmentRow. Corpus: db/migrate/tables/attachments.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS attachments (
    id              bigserial PRIMARY KEY,
    container_id    bigint,
    container_type  text,
    filename        text,
    disk_filename   text,
    filesize        bigint NOT NULL DEFAULT 0,
    content_type    text,
    digest          text,
    downloads       integer NOT NULL DEFAULT 0,
    author_id       bigint NOT NULL,
    description     text,
    status          integer NOT NULL DEFAULT 0,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_attachments_container ON attachments (container_type, container_id);
CREATE INDEX IF NOT EXISTS idx_attachments_author_id ON attachments (author_id);

-- =====================================================================
-- relations (predecessor/successor/blocks/etc between work packages)
-- Mirrors: relations.rs::RelationRow. Corpus: db/migrate/tables/relations.rb
-- NOTE: op-db's RelationRow requires created_at/updated_at; the real
-- OpenProject `relations` table has no timestamps at all. op-db's need wins.
-- =====================================================================
CREATE TABLE IF NOT EXISTS relations (
    id             bigserial PRIMARY KEY,
    from_id        bigint NOT NULL,
    to_id          bigint NOT NULL,
    relation_type  text NOT NULL,
    lag            integer,
    description    text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_relations_from_id ON relations (from_id);
CREATE INDEX IF NOT EXISTS idx_relations_to_id ON relations (to_id);

-- =====================================================================
-- time_entries (timesheet / time tracking)
-- Mirrors: time_entries.rs::TimeEntryRow. Not present in the Rails corpus
-- snapshot bundled at /tmp/op-corpus (no time_entries.rb file there);
-- shaped entirely from op-db's own Row struct.
-- =====================================================================
CREATE TABLE IF NOT EXISTS time_entries (
    id                 bigserial PRIMARY KEY,
    project_id         bigint NOT NULL,
    user_id            bigint NOT NULL,
    work_package_id    bigint,
    hours              double precision NOT NULL,
    comments           text,
    activity_id        bigint NOT NULL,
    spent_on           date NOT NULL,
    tyear              integer NOT NULL,
    tmonth             integer NOT NULL,
    tweek              integer NOT NULL,
    overridden_costs   double precision,
    costs              double precision,
    rate_id            bigint,
    logged_by_id       bigint,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_time_entries_project_id ON time_entries (project_id);
CREATE INDEX IF NOT EXISTS idx_time_entries_user_id ON time_entries (user_id);
CREATE INDEX IF NOT EXISTS idx_time_entries_work_package_id ON time_entries (work_package_id);

-- =====================================================================
-- watchers
-- Mirrors: watchers.rs::WatcherRow. Corpus: db/migrate/tables/watchers.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS watchers (
    id              bigserial PRIMARY KEY,
    watchable_type  text NOT NULL DEFAULT '',
    watchable_id    bigint NOT NULL,
    user_id         bigint NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_watchers_watchable ON watchers (watchable_type, watchable_id);
CREATE INDEX IF NOT EXISTS idx_watchers_user_id ON watchers (user_id);

-- =====================================================================
-- news
-- Mirrors: op-models::news::News (via news.rs repository).
-- Corpus: db/migrate/tables/news.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS news (
    id               bigserial PRIMARY KEY,
    project_id       bigint,
    title            text NOT NULL DEFAULT '',
    summary          text DEFAULT '',
    description      text,
    author_id        bigint NOT NULL,
    comments_count   integer NOT NULL DEFAULT 0,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_news_project_id ON news (project_id);

-- =====================================================================
-- queries (saved work-package table views / filters)
-- Mirrors: queries.rs::QueryRow. Corpus: db/migrate/tables/queries.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS queries (
    id                    bigserial PRIMARY KEY,
    project_id            bigint,
    user_id               bigint NOT NULL,
    name                  text NOT NULL,
    filters               text,
    column_names          text,
    sort_criteria         text,
    group_by              text,
    display_sums          boolean NOT NULL DEFAULT false,
    show_hierarchies      boolean NOT NULL DEFAULT true,
    include_subprojects   boolean NOT NULL DEFAULT true,
    timeline_visible      boolean NOT NULL DEFAULT false,
    timestamps            text,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_queries_project_id ON queries (project_id);
CREATE INDEX IF NOT EXISTS idx_queries_user_id ON queries (user_id);

-- =====================================================================
-- query_menu_items ("starred" queries)
-- Mirrors: queries.rs::star/unstar/is_starred. NOT the real OpenProject
-- schema: real OpenProject stores this as a single-table-inheritance row in
-- the polymorphic `menu_items` table. op-db queries a dedicated
-- `query_menu_items` table directly, so we model that table as op-db needs
-- it, not the STI original.
-- =====================================================================
CREATE TABLE IF NOT EXISTS query_menu_items (
    id              bigserial PRIMARY KEY,
    navigatable_id  bigint,
    name            text,
    title           text
);

CREATE INDEX IF NOT EXISTS idx_query_menu_items_navigatable_id ON query_menu_items (navigatable_id);

-- =====================================================================
-- views (used to detect "public"/pinned work-package-table queries via
-- v.type = 'Views::WorkPackagesTable')
-- Mirrors: queries.rs::find_visible/delete. Corpus: db/migrate/tables/views.rb
-- =====================================================================
CREATE TABLE IF NOT EXISTS views (
    id         bigserial PRIMARY KEY,
    query_id   bigint NOT NULL,
    type       text NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_views_query_id ON views (query_id);
