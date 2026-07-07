-- op-db :: kanban_seed.sql — a SEED BATCH, NOT a migration.
-- generated 2026-07-07
--
-- This file lives in seeds/ (outside migrations/) ON PURPOSE: it is loaded
-- only when the server boots with HYDRATE=1 (Database::seed_kanban), never
-- by sqlx::migrate!, and it owns no slot in the migration version space —
-- do NOT move it into migrations/ or number it (council S4: an earlier
-- header mislabelled it "0002", inviting exactly that mistake).
--
-- Mock project-management kanban data for a fresh database: 3 projects,
-- 5 kanban-column statuses, 4 work package types, 4 priorities, 5 users,
-- 2 roles, cross-project memberships, and 40 work packages spread across
-- projects/statuses/types/authors/assignees with a done_ratio that
-- correlates with status (New=0, In progress=~40, Resolved=90, Closed=100,
-- Rejected=0).
--
-- Deterministic, hardcoded ids throughout; every INSERT uses
-- `ON CONFLICT (id) DO NOTHING` (or the natural unique key for join tables)
-- so the batch is safe to re-run against a partially-seeded database; the
-- whole batch executes as one implicit transaction (all-or-nothing).

-- =====================================================================
-- statuses (the 5 kanban columns)
-- =====================================================================
INSERT INTO statuses (id, name, is_closed, is_default, is_readonly, position, default_done_ratio, created_at, updated_at)
VALUES
    (1, 'New',          false, true,  false, 1, 0,   now(), now()),
    (2, 'In progress',  false, false, false, 2, 40,  now(), now()),
    (3, 'Resolved',     false, false, false, 3, 90,  now(), now()),
    (4, 'Closed',       true,  false, true,  4, 100, now(), now()),
    (5, 'Rejected',     true,  false, true,  5, 0,   now(), now())
ON CONFLICT (id) DO NOTHING;

-- =====================================================================
-- types (work package types)
-- =====================================================================
INSERT INTO types (id, name, position, is_default, is_in_roadmap, is_milestone, is_standard, created_at, updated_at)
VALUES
    (1, 'Task',      1, true,  true, false, true,  now(), now()),
    (2, 'Bug',       2, false, true, false, false, now(), now()),
    (3, 'Feature',   3, false, true, false, false, now(), now()),
    (4, 'Milestone', 4, false, true, true,  false, now(), now())
ON CONFLICT (id) DO NOTHING;

-- =====================================================================
-- enumerations: priorities (type = 'IssuePriority')
-- =====================================================================
INSERT INTO enumerations (id, type, name, position, is_default, active, created_at, updated_at)
VALUES
    (1, 'IssuePriority', 'Low',       1, false, true, now(), now()),
    (2, 'IssuePriority', 'Normal',    2, true,  true, now(), now()),
    (3, 'IssuePriority', 'High',      3, false, true, now(), now()),
    (4, 'IssuePriority', 'Immediate', 4, false, true, now(), now())
ON CONFLICT (id) DO NOTHING;

-- =====================================================================
-- users (1 admin + 4 members; fake data only, no real PII)
-- =====================================================================
INSERT INTO users (id, login, firstname, lastname, mail, admin, status, language, created_at, updated_at)
VALUES
    (1, 'sysadmin', 'System', 'Admin',   'sysadmin@example.com', true,  1, 'en', now(), now()),
    (2, 'alice',    'Alice',  'Anderson','alice@example.com',    false, 1, 'en', now(), now()),
    (3, 'bob',      'Bob',    'Brown',   'bob@example.com',      false, 1, 'en', now(), now()),
    (4, 'carol',    'Carol',  'Clark',   'carol@example.com',    false, 1, 'en', now(), now()),
    (5, 'dave',     'Dave',   'Diaz',    'dave@example.com',     false, 1, 'en', now(), now())
ON CONFLICT (id) DO NOTHING;

-- =====================================================================
-- roles
-- =====================================================================
INSERT INTO roles (id, name, position, builtin, type, created_at, updated_at)
VALUES
    (1, 'Manager', 1, 0, 'Role', now(), now()),
    (2, 'Member',  2, 0, 'Role', now(), now())
ON CONFLICT (id) DO NOTHING;

INSERT INTO role_permissions (id, role_id, permission, created_at, updated_at)
VALUES
    (1, 1, 'view_work_packages', now(), now()),
    (2, 1, 'edit_work_packages', now(), now()),
    (3, 1, 'manage_members',     now(), now()),
    (4, 2, 'view_work_packages', now(), now()),
    (5, 2, 'edit_work_packages', now(), now())
ON CONFLICT (id) DO NOTHING;

-- =====================================================================
-- projects
-- =====================================================================
INSERT INTO projects (id, name, description, identifier, public, parent_id, lft, rgt, active, created_at, updated_at)
VALUES
    (1, 'Website Relaunch', 'Marketing site redesign and rebuild.',       'website-relaunch', true,  NULL, 1, 2, true, now(), now()),
    (2, 'Mobile App',       'Native iOS/Android client.',                 'mobile-app',        true,  NULL, 3, 4, true, now(), now()),
    (3, 'Internal Tools',   'Internal tooling and developer platform.',   'internal-tools',    false, NULL, 5, 6, true, now(), now())
ON CONFLICT (id) DO NOTHING;

-- Enable all 4 types on all 3 projects.
INSERT INTO projects_types (project_id, type_id)
VALUES
    (1, 1), (1, 2), (1, 3), (1, 4),
    (2, 1), (2, 2), (2, 3), (2, 4),
    (3, 1), (3, 2), (3, 3), (3, 4)
ON CONFLICT (project_id, type_id) DO NOTHING;

-- =====================================================================
-- members + member_roles (cross-project memberships)
-- =====================================================================
INSERT INTO members (id, user_id, project_id, created_at, updated_at)
VALUES
    (1, 2, 1, now(), now()), -- alice  @ Website Relaunch
    (2, 3, 1, now(), now()), -- bob    @ Website Relaunch
    (3, 4, 2, now(), now()), -- carol  @ Mobile App
    (4, 5, 2, now(), now()), -- dave   @ Mobile App
    (5, 2, 3, now(), now()), -- alice  @ Internal Tools
    (6, 3, 3, now(), now()), -- bob    @ Internal Tools
    (7, 4, 1, now(), now()), -- carol  @ Website Relaunch
    (8, 5, 3, now(), now())  -- dave   @ Internal Tools
ON CONFLICT (id) DO NOTHING;

INSERT INTO member_roles (id, member_id, role_id)
VALUES
    (1, 1, 1), -- alice  Manager @ Website Relaunch
    (2, 2, 2), -- bob    Member  @ Website Relaunch
    (3, 3, 1), -- carol  Manager @ Mobile App
    (4, 4, 2), -- dave   Member  @ Mobile App
    (5, 5, 2), -- alice  Member  @ Internal Tools
    (6, 6, 1), -- bob    Manager @ Internal Tools
    (7, 7, 2), -- carol  Member  @ Website Relaunch
    (8, 8, 2)  -- dave   Member  @ Internal Tools
ON CONFLICT (id) DO NOTHING;

-- =====================================================================
-- work_packages: 40 cards, the kanban board itself.
--
-- project 1 (Website Relaunch): ids  1-14
-- project 2 (Mobile App):       ids 15-27
-- project 3 (Internal Tools):   ids 28-40
--
-- status/type/priority/author cycle deterministically per project; assignee
-- is NULL for status 1 (New, unassigned backlog); done_ratio correlates
-- with status (New=0, In progress=40, Resolved=90, Closed=100, Rejected=0);
-- start_date/due_date are populated only for statuses 2/3/4 (work that has
-- actually started), spread across 2026 via `id`-derived offsets.
-- =====================================================================
INSERT INTO work_packages (
    id, subject, description, project_id, type_id, status_id, priority_id,
    author_id, assigned_to_id, start_date, due_date, estimated_hours,
    done_ratio, lock_version, schedule_manually, created_at, updated_at
)
VALUES
    -- Project 1: Website Relaunch
    (1,  'Set up CI pipeline for frontend build',        NULL, 1, 1, 1, 1, 2, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (2,  'Fix login redirect bug on Safari',              NULL, 1, 2, 2, 2, 3, 4,
         (DATE '2026-04-01' + (2 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (2 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (3,  'Design new homepage hero section',              NULL, 1, 3, 3, 3, 4, 5,
         (DATE '2026-04-01' + (3 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (3 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now()),
    (4,  'Migrate CSS to Tailwind',                       NULL, 1, 4, 4, 4, 5, 2,
         (DATE '2026-04-01' + (4 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (4 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         24.0, 100, 0, true, now(), now()),
    (5,  'Implement dark mode toggle',                    NULL, 1, 1, 5, 1, 2, 3,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (6,  'Optimize image loading with lazy-load',         NULL, 1, 2, 1, 2, 3, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (7,  'Fix broken contact form validation',            NULL, 1, 3, 2, 3, 4, 5,
         (DATE '2026-04-01' + (7 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (7 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (8,  'Add sitemap.xml generation',                    NULL, 1, 4, 3, 4, 5, 2,
         (DATE '2026-04-01' + (8 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (8 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now()),
    (9,  'Set up staging environment on Railway',         NULL, 1, 1, 4, 1, 2, 3,
         (DATE '2026-04-01' + (9 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (9 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         24.0, 100, 0, true, now(), now()),
    (10, 'Write end-to-end tests for checkout flow',      NULL, 1, 2, 5, 2, 3, 4,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (11, 'Fix mobile navigation menu overlap',            NULL, 1, 3, 1, 3, 4, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (12, 'Add cookie consent banner',                     NULL, 1, 4, 2, 4, 5, 2,
         (DATE '2026-04-01' + (12 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (12 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (13, 'Improve Lighthouse performance score',          NULL, 1, 1, 3, 1, 2, 3,
         (DATE '2026-04-01' + (13 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (13 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now()),
    (14, 'Draft content for About Us page',               NULL, 1, 2, 4, 2, 3, 4,
         (DATE '2026-04-01' + (14 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (14 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         24.0, 100, 0, true, now(), now()),

    -- Project 2: Mobile App
    (15, 'Implement push notifications for iOS',         NULL, 2, 1, 1, 1, 2, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (16, 'Fix crash on Android low-memory devices',      NULL, 2, 2, 2, 2, 3, 4,
         (DATE '2026-04-01' + (16 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (16 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (17, 'Add biometric login support',                  NULL, 2, 3, 3, 3, 4, 5,
         (DATE '2026-04-01' + (17 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (17 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now()),
    (18, 'Set up TestFlight beta distribution',          NULL, 2, 4, 4, 4, 5, 2,
         (DATE '2026-04-01' + (18 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (18 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         24.0, 100, 0, true, now(), now()),
    (19, 'Improve offline sync conflict resolution',     NULL, 2, 1, 5, 1, 2, 3,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (20, 'Fix scroll jank on product list screen',       NULL, 2, 2, 1, 2, 3, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (21, 'Add dark mode to settings screen',             NULL, 2, 3, 2, 3, 4, 5,
         (DATE '2026-04-01' + (21 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (21 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (22, 'Integrate Sentry crash reporting',             NULL, 2, 4, 3, 4, 5, 2,
         (DATE '2026-04-01' + (22 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (22 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now()),
    (23, 'Optimize app startup time',                    NULL, 2, 1, 4, 1, 2, 3,
         (DATE '2026-04-01' + (23 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (23 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         24.0, 100, 0, true, now(), now()),
    (24, 'Write unit tests for cart repository',         NULL, 2, 2, 5, 2, 3, 4,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (25, 'Fix deep link handling for shared items',      NULL, 2, 3, 1, 3, 4, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (26, 'Add accessibility labels to buttons',          NULL, 2, 4, 2, 4, 5, 2,
         (DATE '2026-04-01' + (26 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (26 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (27, 'Prepare release notes for v2.4',               NULL, 2, 1, 3, 1, 2, 3,
         (DATE '2026-04-01' + (27 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (27 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now()),

    -- Project 3: Internal Tools
    (28, 'Automate weekly backup verification script',   NULL, 3, 1, 1, 1, 2, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (29, 'Fix flaky nightly build on internal CI',       NULL, 3, 2, 2, 2, 3, 4,
         (DATE '2026-04-01' + (29 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (29 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (30, 'Add Slack alert for failed deploys',           NULL, 3, 3, 3, 3, 4, 5,
         (DATE '2026-04-01' + (30 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (30 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now()),
    (31, 'Migrate internal wiki to new host',            NULL, 3, 4, 4, 4, 5, 2,
         (DATE '2026-04-01' + (31 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (31 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         24.0, 100, 0, true, now(), now()),
    (32, 'Write runbook for on-call rotation',           NULL, 3, 1, 5, 1, 2, 3,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (33, 'Clean up unused feature flags',                NULL, 3, 2, 1, 2, 3, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (34, 'Add audit log export tool',                    NULL, 3, 3, 2, 3, 4, 5,
         (DATE '2026-04-01' + (34 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (34 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (35, 'Fix permissions bug in admin dashboard',       NULL, 3, 4, 3, 4, 5, 2,
         (DATE '2026-04-01' + (35 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (35 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now()),
    (36, 'Upgrade internal Postgres to v16',             NULL, 3, 1, 4, 1, 2, 3,
         (DATE '2026-04-01' + (36 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (36 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         24.0, 100, 0, true, now(), now()),
    (37, 'Document onboarding checklist for new hires',  NULL, 3, 2, 5, 2, 3, 4,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (38, 'Add rate limiting to internal API gateway',    NULL, 3, 3, 1, 3, 4, NULL,
         NULL, NULL, NULL, 0, 0, true, now(), now()),
    (39, 'Investigate memory leak in worker process',    NULL, 3, 4, 2, 4, 5, 2,
         (DATE '2026-04-01' + (39 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (39 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         8.0, 40, 0, true, now(), now()),
    (40, 'Set up dependency vulnerability scanning',     NULL, 3, 1, 3, 1, 2, 3,
         (DATE '2026-04-01' + (40 * INTERVAL '3 days'))::date,
         (DATE '2026-04-01' + (40 * INTERVAL '3 days') + INTERVAL '14 days')::date,
         16.0, 90, 0, true, now(), now())
ON CONFLICT (id) DO NOTHING;

-- Keep the bigserial sequences ahead of the hardcoded ids above so that any
-- subsequent application-level INSERT (which lets `id` default) does not
-- collide with the seeded rows.
SELECT setval(pg_get_serial_sequence('statuses', 'id'), (SELECT MAX(id) FROM statuses));
SELECT setval(pg_get_serial_sequence('types', 'id'), (SELECT MAX(id) FROM types));
SELECT setval(pg_get_serial_sequence('enumerations', 'id'), (SELECT MAX(id) FROM enumerations));
SELECT setval(pg_get_serial_sequence('users', 'id'), (SELECT MAX(id) FROM users));
SELECT setval(pg_get_serial_sequence('roles', 'id'), (SELECT MAX(id) FROM roles));
SELECT setval(pg_get_serial_sequence('role_permissions', 'id'), (SELECT MAX(id) FROM role_permissions));
SELECT setval(pg_get_serial_sequence('projects', 'id'), (SELECT MAX(id) FROM projects));
SELECT setval(pg_get_serial_sequence('members', 'id'), (SELECT MAX(id) FROM members));
SELECT setval(pg_get_serial_sequence('member_roles', 'id'), (SELECT MAX(id) FROM member_roles));
SELECT setval(pg_get_serial_sequence('work_packages', 'id'), (SELECT MAX(id) FROM work_packages));
