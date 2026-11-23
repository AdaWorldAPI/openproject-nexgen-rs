//! `EXECUTED_DB`: the persisted create → read → edit journey of a work package, run against a
//! real PostgreSQL database.
//!
//! What this gate claims, and nothing more: on a database built from `crates/op-db/migrations`,
//! the Rust paths below persist (or refuse to persist) exactly the rows asserted here. Every
//! assertion about state reads the committed row back with its own SQL, on a separate pool, after
//! the call returned. It is NOT callback reconstruction, NOT callback execution and NOT Rails
//! parity, and it is not counted in those ledgers.
//!
//! The paths exercised are the production ones:
//!
//! * create: `op_services::work_packages::mutation::create` over `PgMutationStore` (the boundary
//!   the JSON API's create calls);
//! * read: `PgWpStore::find_wp`, the lookup the detail and edit pages use;
//! * edit: `edit_form_core` (GET, issues the CSRF cookie and token) and `update_core` (POST, RBAC
//!   then CSRF then write), both over `PgWpStore` through the `PageStore` adapter; the update is
//!   `PgMutationStore::update_then`, one transaction with the journal (steps 4 and 8).
//!
//! The database comes from `OP_TEST_DATABASE_URL`. `DATABASE_URL` is deliberately not read: it is
//! the server's runtime setting, and a developer shell that points it at a real database must not
//! have this test write into it. The test creates a fresh database next to the one the URL names
//! (`CREATE DATABASE`, so the role needs `CREATEDB`), migrates it, runs, and drops it.
//!
//! Without `OP_TEST_DATABASE_URL` the test prints a skip line and passes, unless `REQUIRE_DB=1`,
//! which makes a missing database a failure (the CI job sets it).

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::{header, HeaderMap, StatusCode};
use http_body_util::BodyExt as _;
use op_db::{Database, DatabaseConfig};
use op_services::work_packages::mutation::{
    self, Caller, CreationStore, MutationError, MutationStore, PgCreateScope, PgMutationStore,
    Principal, StoreFailure,
};
use op_services::work_packages::WorkPackageParams;
use sqlx::PgPool;

use super::{edit_form_core, update_core, WriteCtx};
use crate::signin::Who;
use crate::write_guard::{self, PgWpStore, WpStore, CSRF_FIELD};

const URL_VAR: &str = "OP_TEST_DATABASE_URL";
const SECRET: &[u8] = b"executed-db-csrf-secret-0123456789";

// Fixture ids. None of them is 1 except the project the member has no rights in, so a default
// that silently falls back to "id 1" cannot pass.
const USER_A: i64 = 10;
const USER_B: i64 = 11;
/// A second member of project 5: the "other user" of the journal aggregation rule.
const USER_C: i64 = 12;
const LOGIN_A: &str = "oidc:member-a";
const LOGIN_B: &str = "oidc:member-b";
const LOGIN_C: &str = "oidc:member-c";
/// A member of project 5 whose role may add work packages but not view them.
const USER_D: i64 = 13;
const LOGIN_D: &str = "oidc:member-d";
const TARGET_PROJECT: i64 = 5;
const FOREIGN_PROJECT: i64 = 1;
const B_PROJECT: i64 = 7;
const ROLE_A: i64 = 20;
const ROLE_B: i64 = 21;
/// `add_work_packages` without `view_work_packages`.
const ROLE_ADD_ONLY: i64 = 22;
/// Enabled in project 5 at position 2.
const TYPE_TASK: i64 = 30;
/// Enabled in project 5 at position 1: project 5's first enabled type.
const TYPE_BUG: i64 = 31;
/// Enabled in project 1 only, at position 0.
const TYPE_FEATURE: i64 = 32;
const STATUS_NEW: i64 = 40;
/// The one status with `is_default`.
const STATUS_DEFAULT: i64 = 41;
/// A status with `excluded_from_totals`: its work packages are left out of an ancestor's totals.
/// Inserted by step 10, so no earlier step sees it.
const STATUS_EXCLUDED: i64 = 42;
/// An `IssuePriority` that is not the default, and the active default one.
const PRIORITY_LOW: i64 = 60;
const PRIORITY_DEFAULT: i64 = 61;
/// Inactive and marked default: `IssuePriority.active.default` must skip it.
const PRIORITY_INACTIVE_DEFAULT: i64 = 62;

/// Counts the committed-state assertions so the summary line can say how many were made.
#[derive(Default)]
struct Tally {
    steps: usize,
    rows: usize,
}

impl Tally {
    fn row(&mut self) {
        self.rows += 1;
    }
}

fn required() -> bool {
    std::env::var("REQUIRE_DB").as_deref() == Ok("1")
}

/// `postgres://u@h:p/db?x` → (`postgres://u@h:p/`, `?x`).
fn split_url(url: &str) -> (String, String) {
    let (base, query) = match url.find('?') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    let after_scheme = base.find("://").map_or(0, |i| i + 3);
    let path_start = base[after_scheme..]
        .find('/')
        .map_or(base.len(), |i| after_scheme + i);
    (format!("{}/", &base[..path_start]), query.to_string())
}

#[tokio::test]
async fn executed_db_work_package_journey() {
    let url = match std::env::var(URL_VAR) {
        Ok(u) if !u.trim().is_empty() => u,
        _ => {
            assert!(!required(), "REQUIRE_DB=1 but {URL_VAR} is not set");
            eprintln!("EXECUTED_DB: SKIPPED ({URL_VAR} is not set; set REQUIRE_DB=1 to fail)");
            return;
        }
    };

    let admin = PgPool::connect(&url)
        .await
        .expect("connect to OP_TEST_DATABASE_URL");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_name = format!("op_executed_db_{}_{nanos}", std::process::id());
    sqlx::query(&format!("CREATE DATABASE {db_name}"))
        .execute(&admin)
        .await
        .expect("CREATE DATABASE (the role needs CREATEDB)");
    let (base, query) = split_url(&url);
    let db_url = format!("{base}{db_name}{query}");

    // Run the journey on a thread of its own, so a failed assertion still reaches the cleanup
    // below and so that the journey's large future has room: in a debug build it overflows the
    // 2 MiB stack of the test thread once it has enough steps (it did at step 13).
    let handle = std::thread::Builder::new()
        .name("executed-db-journey".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the journey's runtime")
                .block_on(journey(db_url))
        })
        .expect("spawn the journey thread");
    let outcome = tokio::task::spawn_blocking(move || handle.join())
        .await
        .expect("the join task");

    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name} WITH (FORCE)"))
        .execute(&admin)
        .await;
    admin.close().await;

    match outcome {
        Ok(tally) => eprintln!(
            "EXECUTED_DB: journey steps {}/13, rows asserted {}, transactional create: YES (step 7), creation journal: YES (steps 2, 7), update journal: YES (steps 4, 8, 9: no-op, aggregated, new version, closed predecessor, rollback), ancestor totals: YES (step 10: derived hours and done ratio up a 3-level chain, excluded status, caused journals, aggregation, rollback), ancestor totals on update: YES (step 11: estimate and status edits, the edited parent's own totals aggregated into its journal, a move recomputing the former and the new chains, no write for a subject edit, rollback), switch to automatic mode and ignore_non_working_days: YES (step 12: only a direct manual parent with one child and no scheduling relation switches, the scheduling relations of automatic ancestors count and a manual ancestor cuts them off, the former parent is not switched, ignore_non_working_days follows the children except for manual ancestors, the former chain is recomputed), rescheduling: YES (step 13: the duration a create and an edit write in working or calendar days and kept by a moved successor, a successor with a lag moved across a weekend and back with an aggregated related-change journal, a manual successor and a manual parent untouched, calendar days for ignore_non_working_days, a non-working day shifting the result, a parent spanning its children, a create under an automatic parent, the former parent recomputed on a move, rollback, a stale rescheduled row a conflict)",
            tally.steps, tally.rows
        ),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// The fixture: two users, three projects, roles, types and statuses, inserted with plain SQL.
async fn seed(pool: &PgPool) {
    let statements = [
        format!(
            "INSERT INTO users (id, login, firstname, lastname, mail, admin, status) VALUES \
             ({USER_A}, '{LOGIN_A}', 'Ada', 'Member', 'a@example.test', false, 1), \
             ({USER_B}, '{LOGIN_B}', 'Bo', 'Outsider', 'b@example.test', false, 1), \
             ({USER_C}, '{LOGIN_C}', 'Cy', 'Colleague', 'c@example.test', false, 1), \
             ({USER_D}, '{LOGIN_D}', 'Di', 'Adder', 'd@example.test', false, 1)"
        ),
        format!(
            "INSERT INTO projects (id, name, identifier, active) VALUES \
             ({FOREIGN_PROJECT}, 'Foreign', 'foreign', true), \
             ({TARGET_PROJECT}, 'Target', 'target', true), \
             ({B_PROJECT}, 'B home', 'b-home', true)"
        ),
        format!(
            "INSERT INTO roles (id, name, position) VALUES \
             ({ROLE_A}, 'Member', 1), ({ROLE_B}, 'Editor elsewhere', 2), \
             ({ROLE_ADD_ONLY}, 'Adder', 3)"
        ),
        format!(
            "INSERT INTO role_permissions (role_id, permission) VALUES \
             ({ROLE_A}, 'view_work_packages'), ({ROLE_A}, 'add_work_packages'), \
             ({ROLE_A}, 'edit_work_packages'), \
             ({ROLE_B}, 'view_work_packages'), ({ROLE_B}, 'add_work_packages'), \
             ({ROLE_B}, 'edit_work_packages'), ({ROLE_ADD_ONLY}, 'add_work_packages')"
        ),
        // A is a member of project 5 only; B of project 7 only. Nobody is a member of project 1.
        format!(
            "INSERT INTO members (id, user_id, project_id) VALUES \
             (50, {USER_A}, {TARGET_PROJECT}), (51, {USER_B}, {B_PROJECT}), \
             (52, {USER_C}, {TARGET_PROJECT}), (53, {USER_D}, {TARGET_PROJECT})"
        ),
        format!(
            "INSERT INTO member_roles (member_id, role_id) VALUES \
             (50, {ROLE_A}), (51, {ROLE_B}), (52, {ROLE_A}), (53, {ROLE_ADD_ONLY})"
        ),
        format!(
            "INSERT INTO types (id, name, position) VALUES \
             ({TYPE_TASK}, 'Task', 2), ({TYPE_BUG}, 'Bug', 1), ({TYPE_FEATURE}, 'Feature', 0)"
        ),
        format!(
            "INSERT INTO projects_types (project_id, type_id) VALUES \
             ({TARGET_PROJECT}, {TYPE_TASK}), ({TARGET_PROJECT}, {TYPE_BUG}), \
             ({FOREIGN_PROJECT}, {TYPE_FEATURE}), ({B_PROJECT}, {TYPE_TASK})"
        ),
        format!(
            "INSERT INTO statuses (id, name, is_default, position) VALUES \
             ({STATUS_NEW}, 'New', false, 1), ({STATUS_DEFAULT}, 'Open', true, 2)"
        ),
        // Position 0 puts the inactive default first: only the `active` filter skips it.
        format!(
            "INSERT INTO enumerations (id, type, name, position, is_default, active) VALUES \
             ({PRIORITY_INACTIVE_DEFAULT}, 'IssuePriority', 'Retired', 0, true, false), \
             ({PRIORITY_LOW}, 'IssuePriority', 'Low', 1, false, true), \
             ({PRIORITY_DEFAULT}, 'IssuePriority', 'Normal', 2, true, true)"
        ),
    ];
    for s in statements {
        sqlx::query(&s).execute(pool).await.expect("seed statement");
    }
}

/// The committed row as JSON text, read on the verification pool.
async fn row_json(verify: &PgPool, id: i64) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT row_to_json(w)::text FROM work_packages w WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(verify)
    .await
    .expect("read committed row")
}

async fn count(verify: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(verify)
        .await
        .expect("count")
}

/// One `journals` row of a work package as the verification pool reads it back.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
struct JRow {
    id: i64,
    version: i32,
    user_id: i64,
    notes: Option<String>,
    restricted: bool,
    data_id: i64,
    cause: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    /// `lower(validity_period)`.
    lo: chrono::DateTime<chrono::Utc>,
    /// `upper(validity_period)`: `None` while the journal is the current one.
    hi: Option<chrono::DateTime<chrono::Utc>>,
}

/// The journals of work package `id`, oldest version first.
async fn journals(verify: &PgPool, id: i64) -> Vec<JRow> {
    sqlx::query_as::<_, JRow>(
        "SELECT id, version, user_id, notes, restricted, data_id, cause, created_at, updated_at, \
                lower(validity_period) AS lo, upper(validity_period) AS hi \
         FROM journals WHERE journable_type = 'WorkPackage' AND journable_id = $1 \
         ORDER BY version",
    )
    .bind(id)
    .fetch_all(verify)
    .await
    .expect("read journals")
}

/// Every journal row and every data row of work package `id`, as JSON text, for byte-identity
/// checks ("nothing was written").
async fn journal_state(verify: &PgPool, id: i64) -> (Vec<String>, Vec<String>) {
    let j = sqlx::query_scalar::<_, String>(
        "SELECT row_to_json(j)::text FROM journals j \
         WHERE j.journable_type = 'WorkPackage' AND j.journable_id = $1 ORDER BY j.version",
    )
    .bind(id)
    .fetch_all(verify)
    .await
    .expect("journal rows");
    let d = sqlx::query_scalar::<_, String>(
        "SELECT row_to_json(d)::text FROM work_package_journals d \
         JOIN journals j ON j.data_id = d.id \
         WHERE j.journable_type = 'WorkPackage' AND j.journable_id = $1 ORDER BY j.version",
    )
    .bind(id)
    .fetch_all(verify)
    .await
    .expect("journal data rows");
    (j, d)
}

async fn data_subject(verify: &PgPool, data_id: i64) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT subject FROM work_package_journals WHERE id = $1")
        .bind(data_id)
        .fetch_optional(verify)
        .await
        .expect("data row")
}

async fn wp_updated_at(verify: &PgPool, id: i64) -> chrono::DateTime<chrono::Utc> {
    sqlx::query_scalar("SELECT updated_at FROM work_packages WHERE id = $1")
        .bind(id)
        .fetch_one(verify)
        .await
        .expect("updated_at")
}

/// The update the JSON API's PATCH builds: the record's current values carried forward, with
/// only the subject replaced, at the record's current `lock_version`.
fn patch_subject(
    row: &op_db::work_packages::WorkPackageRow,
    subject: &str,
) -> op_db::UpdateWorkPackageDto {
    op_db::UpdateWorkPackageDto {
        subject: Some(subject.into()),
        description: row.description.clone(),
        type_id: Some(row.type_id),
        status_id: Some(row.status_id),
        priority_id: row.priority_id,
        assigned_to_id: row.assigned_to_id,
        responsible_id: row.responsible_id,
        start_date: row.start_date,
        due_date: row.due_date,
        estimated_hours: row.estimated_hours,
        done_ratio: Some(row.done_ratio),
        parent_id: row.parent_id,
        version_id: row.version_id,
        category_id: row.category_id,
        lock_version: row.lock_version,
    }
}

fn who(login: &str) -> Who {
    Who::Caller {
        actor: login.to_string(),
        role: "Member".to_string(),
    }
}

fn ctx<'a>(who: &'a Who, headers: &'a HeaderMap) -> WriteCtx<'a> {
    WriteCtx {
        who: Some(who),
        headers,
        allow_anonymous: false,
        secret: SECRET,
        secure_cookie: false,
    }
}

fn headers_with_cookie(cookie: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        header::COOKIE,
        format!("{}={cookie}", write_guard::CSRF_COOKIE)
            .parse()
            .unwrap(),
    );
    h
}

/// GET the edit form as `who`, the way a browser without a cookie does, and return
/// (cookie, token, html) from the real response.
async fn fetch_form(store: &PgWpStore, id: i64, who: &Who) -> (String, String, String) {
    let no_cookie = HeaderMap::new();
    let resp = edit_form_core(store, id, &ctx(who, &no_cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK, "edit form for {who:?}");
    let cookie = resp.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .strip_prefix("op_csrf=")
        .and_then(|r| r.split(';').next())
        .unwrap()
        .to_owned();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes).into_owned();
    let token = write_guard::token_for(SECRET, &cookie, &crate::write_guard::actor_key(Some(who)));
    assert!(
        html.contains(&format!("name=\"{CSRF_FIELD}\" value=\"{token}\"")),
        "the form embeds the token the update accepts"
    );
    (cookie, token, html)
}

/// The form a browser submits from the edit page: every field it shows, carrying the row's
/// current values, with only the subject changed.
fn edit_form(
    row: &op_db::work_packages::WorkPackageRow,
    subject: &str,
    token: &str,
) -> HashMap<String, String> {
    let opt = |v: Option<i64>| v.map(|x| x.to_string()).unwrap_or_default();
    HashMap::from([
        ("subject".into(), subject.to_string()),
        (
            "description".into(),
            row.description.clone().unwrap_or_default(),
        ),
        ("type_id".into(), row.type_id.to_string()),
        ("status_id".into(), row.status_id.to_string()),
        ("priority_id".into(), opt(row.priority_id)),
        ("assigned_to_id".into(), opt(row.assigned_to_id)),
        (
            "start_date".into(),
            row.start_date.map(|d| d.to_string()).unwrap_or_default(),
        ),
        (
            "due_date".into(),
            row.due_date.map(|d| d.to_string()).unwrap_or_default(),
        ),
        (
            "estimated_hours".into(),
            row.estimated_hours
                .map(|h| h.to_string())
                .unwrap_or_default(),
        ),
        ("done_ratio".into(), row.done_ratio.to_string()),
        ("lock_version".into(), row.lock_version.to_string()),
        (CSRF_FIELD.into(), token.to_string()),
    ])
}

async fn journey(db_url: String) -> Tally {
    let db = Database::connect(&DatabaseConfig::with_url(&db_url))
        .await
        .expect("connect to the fresh database");
    db.run_migrations().await.expect("migrations apply");
    let pool = db.pool().clone();
    // Committed state is read on its own pool: another connection sees only what was committed.
    let verify = PgPool::connect(&db_url).await.expect("verification pool");
    let mut t = Tally::default();

    // ── 1. fresh database from migrations, plus the fixture ──────────
    assert_eq!(
        count(&verify, "SELECT count(*) FROM work_packages").await,
        0
    );
    t.row();
    seed(&pool).await;
    assert_eq!(
        count(&verify, "SELECT count(*) FROM statuses WHERE is_default").await,
        1
    );
    t.row();
    t.steps += 1;

    // ── 2. A creates in project 5 through the boundary ───────────────
    let store = PgMutationStore { pool: pool.clone() };
    let caller_a = Caller::Actor {
        login: LOGIN_A.into(),
    };
    let mut params = WorkPackageParams::new()
        .with_subject("Persisted journey")
        .with_description("Created by the EXECUTED_DB journey");
    params.project_id = Some(TARGET_PROJECT);
    let created = mutation::create(&store, &caller_a, params)
        .await
        .expect("A may add work packages in project 5");
    let id = created.id;
    let (project_id, type_id, status_id, author_id, subject, lock_version, priority_id): (
        i64,
        i64,
        i64,
        i64,
        String,
        i32,
        Option<i64>,
    ) = sqlx::query_as(
        "SELECT project_id, type_id, status_id, author_id, subject, lock_version, priority_id \
         FROM work_packages WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&verify)
    .await
    .expect("the created row is committed");
    assert_eq!(
        project_id, TARGET_PROJECT,
        "stored in the submitted project"
    );
    assert_ne!(project_id, FOREIGN_PROJECT, "never project 1");
    assert_eq!(
        type_id, TYPE_BUG,
        "project 5's first enabled type by position"
    );
    assert_eq!(status_id, STATUS_DEFAULT, "the status marked is_default");
    assert_eq!(author_id, USER_A, "the author is the caller's users.id");
    assert_eq!(
        priority_id,
        Some(PRIORITY_DEFAULT),
        "IssuePriority.active.default: the active default, not the inactive one at position 0"
    );
    assert_eq!(subject, "Persisted journey");
    assert_eq!(lock_version, 0);
    assert_eq!(
        count(&verify, "SELECT count(*) FROM work_packages").await,
        1
    );
    t.rows += 2;

    // The creation journal (OpenProject: `acts_as_journalized` `after_save :save_journals` ->
    // `Journals::CreateService`, app/services/journals/create_service.rb:465-507 and :389-404):
    // exactly one journal at version 1 for this work package, written by the caller, plus the one
    // data row it points at, copied from the committed work_packages row.
    assert_eq!(
        count(
            &verify,
            &format!(
                "SELECT count(*) FROM journals WHERE journable_type = 'WorkPackage' \
                 AND journable_id = {id}"
            )
        )
        .await,
        1,
        "exactly one journal for the new work package"
    );
    let (version, j_user, notes, restricted, data_type, data_id, cause, stamped_ok): (
        i32,
        i64,
        Option<String>,
        bool,
        String,
        i64,
        serde_json::Value,
        bool,
    ) = sqlx::query_as(
        "SELECT j.version, j.user_id, j.notes, j.restricted, j.data_type, j.data_id, j.cause, \
                (j.created_at = w.updated_at AND j.updated_at = w.updated_at \
                 AND j.validity_period = tstzrange(w.updated_at, NULL)) \
         FROM journals j JOIN work_packages w ON w.id = j.journable_id \
         WHERE j.journable_type = 'WorkPackage' AND j.journable_id = $1",
    )
    .bind(id)
    .fetch_one(&verify)
    .await
    .expect("the creation journal is committed");
    assert_eq!(version, 1, "a create writes version 1");
    assert_eq!(j_user, USER_A, "the journal's user is the caller");
    assert_eq!(notes.as_deref(), Some(""), "NoCause create: empty notes");
    assert!(!restricted);
    assert_eq!(data_type, "Journal::WorkPackageJournal");
    assert_eq!(cause, serde_json::json!({}), "NoCause#to_hash is {{}}");
    assert!(
        stamped_ok,
        "created_at = updated_at = the work package's updated_at; validity_period = [that, ∞)"
    );
    let data_matches: bool = sqlx::query_scalar(
        "SELECT (d.type_id, d.project_id, d.subject, COALESCE(d.description, ''), d.status_id, \
                 d.author_id, d.done_ratio, d.parent_id, d.schedule_manually) \
                IS NOT DISTINCT FROM \
                (w.type_id, w.project_id, w.subject, COALESCE(w.description, ''), w.status_id, \
                 w.author_id, w.done_ratio, w.parent_id, w.schedule_manually) \
         FROM work_package_journals d, work_packages w WHERE d.id = $1 AND w.id = $2",
    )
    .bind(data_id)
    .bind(id)
    .fetch_one(&verify)
    .await
    .expect("the journal points at exactly one data row");
    assert!(data_matches, "the data row is a copy of the work package");
    let data_priority: i64 =
        sqlx::query_scalar("SELECT priority_id FROM work_package_journals WHERE id = $1")
            .bind(data_id)
            .fetch_one(&verify)
            .await
            .expect("the journal data row carries a priority (NOT NULL)");
    assert_eq!(
        data_priority, PRIORITY_DEFAULT,
        "the snapshot carries the default priority"
    );
    // Reading the journals back decodes `priority_id` as a non-null i64; a NULL would panic here.
    let read_back = op_db::journals::JournalRepository::new(verify.clone())
        .find_work_package_journals_with_data(id, op_db::repository::Pagination::default())
        .await
        .expect("the created work package's journals read back");
    assert_eq!(read_back.items.len(), 1);
    assert_eq!(
        read_back.items[0].data.as_ref().map(|d| d.priority_id),
        Some(PRIORITY_DEFAULT)
    );
    assert_eq!(
        count(&verify, "SELECT count(*) FROM work_package_journals").await,
        1,
        "one data row for the one journal"
    );
    // `after_create`'s `set_user_as_watcher` (create_service.rb:82): A, who may view work
    // packages in project 5, watches the work package A created.
    assert_eq!(
        watcher_users(&verify, id).await,
        vec![USER_A],
        "the creator becomes the one watcher"
    );
    t.rows += 4;
    t.steps += 1;

    // ── 3. read it back through the page read path ───────────────────
    let page_store = PgWpStore { pool: pool.clone() };
    let read = page_store
        .find_wp(id)
        .await
        .expect("read")
        .expect("the record is found by id");
    assert_eq!(
        (
            read.project_id,
            read.type_id,
            read.status_id,
            read.author_id,
            read.subject.as_str(),
            read.description.as_deref(),
        ),
        (
            TARGET_PROJECT,
            TYPE_BUG,
            STATUS_DEFAULT,
            USER_A,
            "Persisted journey",
            Some("Created by the EXECUTED_DB journey"),
        )
    );
    assert_eq!(read.lock_version, lock_version);
    t.steps += 1;

    // ── 4. A edits through the CSRF/RBAC form path ───────────────────
    let before_edit: serde_json::Value =
        serde_json::from_str(&row_json(&verify, id).await.unwrap()).unwrap();
    let who_a = who(LOGIN_A);
    let (cookie_a, token_a, html) = fetch_form(&page_store, id, &who_a).await;
    assert!(
        html.contains(&format!(
            "<input type=\"hidden\" name=\"lock_version\" value=\"{lock_version}\">"
        )),
        "the form carries the committed lock_version"
    );
    let headers_a = headers_with_cookie(&cookie_a);
    // Without a valid form token the same edit is refused and nothing is written: a token minted
    // for another browser cookie, and no token at all.
    let unchanged = row_json(&verify, id).await.unwrap();
    let foreign_token = write_guard::token_for(SECRET, &write_guard::new_cookie_value(), LOGIN_A);
    let mut missing = edit_form(&read, "forged edit", &token_a);
    missing.remove(CSRF_FIELD);
    for form in [edit_form(&read, "forged edit", &foreign_token), missing] {
        let resp = update_core(&page_store, id, &ctx(&who_a, &headers_a), &form).await;
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "a bad CSRF token is refused"
        );
        assert_eq!(
            row_json(&verify, id).await.unwrap(),
            unchanged,
            "no write without a valid CSRF token"
        );
        t.row();
    }
    let j_before = journals(&verify, id).await;
    assert_eq!(j_before.len(), 1);
    let old_data_id = j_before[0].data_id;
    let resp = update_core(
        &page_store,
        id,
        &ctx(&who_a, &headers_a),
        &edit_form(&read, "Persisted journey, edited", &token_a),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER, "the edit is accepted");
    let after_edit: serde_json::Value =
        serde_json::from_str(&row_json(&verify, id).await.unwrap()).unwrap();
    assert_eq!(after_edit["subject"], "Persisted journey, edited");
    assert_eq!(
        after_edit["lock_version"].as_i64(),
        Some(i64::from(lock_version) + 1),
        "lock_version incremented"
    );
    // Every column the edit did not touch is unchanged.
    let mut untouched_before = before_edit.as_object().unwrap().clone();
    let mut untouched_after = after_edit.as_object().unwrap().clone();
    for k in ["subject", "lock_version", "updated_at"] {
        untouched_before.remove(k);
        untouched_after.remove(k);
    }
    assert_eq!(
        untouched_before, untouched_after,
        "untouched columns are unchanged"
    );
    t.rows += 3;

    // The journal of the edit. A created the work package seconds ago, so A's edit is within the
    // aggregation window (`journal_aggregation_time_minutes`, default 5,
    // config/constants/settings/definition.rb:770-773) by the SAME user with an empty cause and no
    // note: `Journals::CreateService#aggregatable?` (create_service.rb:595-601) is true, so the
    // creation journal is REWRITTEN, not followed by a version 2 (`update_journal_sql`,
    // :437-461): same id, version, created_at and validity_period; the old data row deleted
    // (`cleanup_predecessor_data_sql`, :118-120); a new data row; updated_at = the work package's
    // updated_at (`fetch_time`, :331-392).
    let j_after = journals(&verify, id).await;
    assert_eq!(
        j_after.len(),
        1,
        "same user within the window: no version 2, the creation journal is aggregated"
    );
    let (b, a) = (&j_before[0], &j_after[0]);
    assert_eq!(
        (a.id, a.version, a.user_id, a.created_at, a.lo, a.hi),
        (b.id, 1, USER_A, b.created_at, b.lo, None),
        "aggregation keeps id, version, user, created_at and the open validity period"
    );
    assert_eq!(a.hi, None);
    assert_ne!(a.data_id, old_data_id, "a new data row");
    assert_eq!(
        data_subject(&verify, old_data_id).await,
        None,
        "the predecessor's data row is deleted"
    );
    assert_eq!(
        data_subject(&verify, a.data_id).await.as_deref(),
        Some("Persisted journey, edited"),
        "the journal now snapshots the edited work package"
    );
    assert_eq!(
        a.updated_at,
        wp_updated_at(&verify, id).await,
        "the aggregated journal's updated_at is the work package's updated_at"
    );
    assert!(
        a.updated_at > b.updated_at,
        "the aggregated journal moved forward"
    );
    assert_eq!(
        count(&verify, "SELECT count(*) FROM work_package_journals").await,
        1,
        "still exactly one data row: the old one was replaced, not kept"
    );
    t.rows += 6;

    // A no-op edit (the form resubmitted as it stands): nothing journaled changed, so the
    // `changes` CTE is empty and no CTE writes (create_service.rb:425-427, helpers.rb:54-60).
    // Control: the edit above changed the subject and DID rewrite the journal (a.data_id differs).
    let current = page_store.find_wp(id).await.unwrap().unwrap();
    let state_before_noop = journal_state(&verify, id).await;
    let resp = update_core(
        &page_store,
        id,
        &ctx(&who_a, &headers_a),
        &edit_form(&current, &current.subject, &token_a),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::SEE_OTHER,
        "the no-op edit is accepted"
    );
    assert_eq!(
        journal_state(&verify, id).await,
        state_before_noop,
        "a no-op edit writes no journal and touches no data row (byte-identical)"
    );
    t.rows += 1;
    t.steps += 1;

    // ── 5. B attempts the same edit: refused, the row is byte-identical
    let who_b = who(LOGIN_B);
    let before_b = row_json(&verify, id).await.unwrap();
    // B gets no edit form for a record in a project where B holds nothing.
    let no_cookie = HeaderMap::new();
    let form_resp = edit_form_core(&page_store, id, &ctx(&who_b, &no_cookie)).await;
    assert_eq!(form_resp.status(), StatusCode::FORBIDDEN);
    // And a well-formed post with B's own valid token is refused too.
    let cookie_b = write_guard::new_cookie_value();
    let token_b = write_guard::token_for(SECRET, &cookie_b, LOGIN_B);
    let headers_b = headers_with_cookie(&cookie_b);
    let current = page_store.find_wp(id).await.unwrap().unwrap();
    let resp = update_core(
        &page_store,
        id,
        &ctx(&who_b, &headers_b),
        &edit_form(&current, "B was here", &token_b),
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "B may not edit in project 5"
    );
    assert_eq!(
        row_json(&verify, id).await.unwrap(),
        before_b,
        "the committed row is byte-identical after B's attempt"
    );
    t.row();
    t.steps += 1;

    // ── 6. A attempts to create in project 1: refused, nothing inserted
    let total_before = count(&verify, "SELECT count(*) FROM work_packages").await;
    let mut foreign = WorkPackageParams::new().with_subject("Smuggled into project 1");
    foreign.project_id = Some(FOREIGN_PROJECT);
    let r = mutation::create(&store, &caller_a, foreign).await;
    assert!(matches!(r, Err(MutationError::Forbidden)), "{r:?}");
    assert_eq!(
        count(&verify, "SELECT count(*) FROM work_packages").await,
        total_before,
        "no row inserted"
    );
    assert_eq!(
        count(
            &verify,
            "SELECT count(*) FROM work_packages WHERE project_id = 1"
        )
        .await,
        0
    );
    t.rows += 2;
    t.steps += 1;

    // ── 7. transactionality of the create ────────────────────────────
    // `create` runs authorization reads, defaults and the INSERT on one transaction
    // (`PgMutationStore::create_then`), as OpenProject's `in_user_context` does. To
    // observe a failure after the insert and the journal write, the unit of work runs a failing
    // statement on the same transaction right after them.
    // The insert and the journal must roll back with it: no row survives. (Two-sided: the control below inserts
    // through the same wrapper without the failure and the row is committed.)
    let probe = FailAfterInsert {
        fail: true,
        pool: pool.clone(),
    };
    let mut p = WorkPackageParams::new().with_subject("transaction probe");
    p.project_id = Some(TARGET_PROJECT);
    let r = mutation::create(&probe, &caller_a, p).await;
    assert!(matches!(r, Err(MutationError::Store(_))), "{r:?}");
    let survivors = count(
        &verify,
        "SELECT count(*) FROM work_packages WHERE subject = 'transaction probe'",
    )
    .await;
    assert_eq!(
        survivors, 0,
        "the create is transactional: a post-insert failure must leave no row"
    );
    // The failure comes after the journal write, so the journal must have rolled back with it:
    // still only step 2's journal and data row.
    assert_eq!(
        count(&verify, "SELECT count(*) FROM journals").await,
        1,
        "a post-insert failure leaves no journal"
    );
    assert_eq!(
        count(&verify, "SELECT count(*) FROM work_package_journals").await,
        1,
        "a post-insert failure leaves no journal data row"
    );
    assert_eq!(
        count(
            &verify,
            "SELECT count(*) FROM watchers w WHERE NOT EXISTS \
             (SELECT 1 FROM work_packages p WHERE p.id = w.watchable_id)"
        )
        .await,
        0,
        "a post-insert failure leaves no watcher"
    );
    // Control: the same wrapper without the injected failure commits, so the 0 above is the
    // rollback and not a create that never inserts.
    let control = FailAfterInsert {
        fail: false,
        pool: pool.clone(),
    };
    let mut p = WorkPackageParams::new().with_subject("transaction control");
    p.project_id = Some(TARGET_PROJECT);
    mutation::create(&control, &caller_a, p)
        .await
        .expect("the control create succeeds");
    let committed = count(
        &verify,
        "SELECT count(*) FROM work_packages WHERE subject = 'transaction control'",
    )
    .await;
    assert_eq!(committed, 1, "the control create commits its row");
    assert_eq!(
        count(
            &verify,
            "SELECT count(*) FROM journals j JOIN work_packages w ON w.id = j.journable_id \
             WHERE w.subject = 'transaction control' AND j.version = 1"
        )
        .await,
        1,
        "the control create commits its journal too (so the 1 above is the rollback)"
    );
    assert_eq!(
        count(
            &verify,
            "SELECT count(*) FROM watchers w JOIN work_packages p ON p.id = w.watchable_id \
             WHERE p.subject = 'transaction control' AND w.user_id = 10"
        )
        .await,
        1,
        "and its watcher (so the 0 above is the rollback)"
    );
    eprintln!(
        "EXECUTED_DB step 7: create is one transaction; a failure after the INSERT left \
         {survivors} rows (control committed {committed})"
    );
    t.rows += 6;
    t.steps += 1;

    // D may add work packages in project 5 but not view them: the create succeeds and D does
    // not become a watcher (`Watcher#validate_user_allowed_to_watch` fails, and
    // `set_user_as_watcher` ignores the failure, create_service.rb:118-122).
    let caller_d = Caller::Actor {
        login: LOGIN_D.into(),
    };
    let mut p = WorkPackageParams::new().with_subject("added by D");
    p.project_id = Some(TARGET_PROJECT);
    let by_d = mutation::create(&store, &caller_d, p)
        .await
        .expect("D may add work packages in project 5");
    assert_eq!(
        journals(&verify, by_d.id).await.len(),
        1,
        "D's create is committed and journaled"
    );
    assert!(
        watcher_users(&verify, by_d.id).await.is_empty(),
        "a creator who may not view the work package does not watch it"
    );
    t.rows += 2;

    // ── 8. the update journal: other user, closed predecessor, window, cause, rollback ──
    let store_c = PgMutationStore { pool: pool.clone() };
    let caller_c = Caller::Actor {
        login: LOGIN_C.into(),
    };
    let v1 = journals(&verify, id).await;
    assert_eq!(v1.len(), 1);
    let v1_state = journal_state(&verify, id).await;

    // A stale lock_version is a conflict and rolls the whole unit back: no journal either.
    let current = page_store.find_wp(id).await.unwrap().unwrap();
    let mut stale = patch_subject(&current, "stale write");
    stale.lock_version -= 1;
    let r = mutation::update(&store_c, &caller_c, id, stale).await;
    assert!(matches!(r, Err(MutationError::Conflict(_))), "{r:?}");
    assert_eq!(
        journal_state(&verify, id).await,
        v1_state,
        "a conflict writes no journal"
    );
    t.row();

    // The authorization is re-decided INSIDE the transaction (`update_in`), so a caller who got
    // past the ticket check but lost the right in the meantime is refused with nothing written.
    // B holds nothing in project 5; the store is called directly, without the ticket.
    let caller_b = Caller::Actor {
        login: LOGIN_B.into(),
    };
    let r = store_c
        .update_unit_of_work(&caller_b, id, patch_subject(&current, "B, inside the unit"))
        .await;
    assert!(matches!(r, Err(MutationError::Forbidden)), "{r:?}");
    assert_eq!(
        journal_state(&verify, id).await,
        v1_state,
        "a refused update writes no journal"
    );
    t.row();

    // C (another member of project 5) edits: a different user cannot aggregate
    // (`same_user?`, create_service.rb:611-613), so version 2 is added and version 1 is closed.
    let r = mutation::update(
        &store_c,
        &caller_c,
        id,
        patch_subject(&current, "edited by C"),
    )
    .await
    .expect("C may edit in project 5");
    assert_eq!(r.subject, "edited by C");
    let two = journals(&verify, id).await;
    assert_eq!(two.len(), 2, "another user's edit adds version 2");
    let (p, n) = (&two[0], &two[1]);
    let wp_ts = wp_updated_at(&verify, id).await;
    assert_eq!((n.version, n.user_id), (2, USER_C));
    assert_eq!((n.notes.as_deref(), n.restricted), (Some(""), false));
    assert_eq!(n.cause, serde_json::json!({}));
    assert_eq!(
        (n.created_at, n.updated_at, n.lo, n.hi),
        (wp_ts, wp_ts, wp_ts, None),
        "created_at = updated_at = lower(validity_period) = the work package's updated_at; open"
    );
    assert_eq!(
        (p.id, p.version, p.created_at, p.lo, p.data_id),
        (v1[0].id, 1, v1[0].created_at, v1[0].lo, v1[0].data_id),
        "version 1 keeps its identity and its snapshot"
    );
    assert_eq!(
        p.hi,
        Some(n.created_at),
        "the predecessor's validity period is closed at the new journal's start"
    );
    assert_eq!(
        data_subject(&verify, p.data_id).await.as_deref(),
        Some("Persisted journey, edited"),
        "version 1 still snapshots the state before C's edit"
    );
    assert_eq!(
        data_subject(&verify, n.data_id).await.as_deref(),
        Some("edited by C")
    );
    let wp_row = page_store.find_wp(id).await.unwrap().unwrap();
    let (nd_matches,): (bool,) = sqlx::query_as(
        "SELECT (d.type_id, d.project_id, d.status_id, d.author_id, d.done_ratio, d.priority_id, \
                 COALESCE(d.description, '')) \
                IS NOT DISTINCT FROM \
                (w.type_id, w.project_id, w.status_id, w.author_id, w.done_ratio, w.priority_id, \
                 COALESCE(w.description, '')) \
         FROM work_package_journals d, work_packages w WHERE d.id = $1 AND w.id = $2",
    )
    .bind(n.data_id)
    .bind(id)
    .fetch_one(&verify)
    .await
    .unwrap();
    assert!(nd_matches, "the new data row is a copy of the work package");
    assert_eq!(wp_row.lock_version, current.lock_version + 1);
    t.rows += 9;

    // C edits again within the window: now the last journal IS C's, so it is aggregated: same
    // id/version/created_at/validity, a new data row, the old one deleted, version 1 untouched.
    let before_c2 = journals(&verify, id).await;
    let v1_json_before = journal_state(&verify, id).await.0[0].clone();
    let current = page_store.find_wp(id).await.unwrap().unwrap();
    mutation::update(
        &store_c,
        &caller_c,
        id,
        patch_subject(&current, "edited by C again"),
    )
    .await
    .expect("C edits again");
    let after_c2 = journals(&verify, id).await;
    assert_eq!(
        after_c2.len(),
        2,
        "same user within the window aggregates into version 2"
    );
    assert_eq!(
        (
            after_c2[1].id,
            after_c2[1].version,
            after_c2[1].created_at,
            after_c2[1].lo,
            after_c2[1].hi
        ),
        (
            before_c2[1].id,
            2,
            before_c2[1].created_at,
            before_c2[1].lo,
            None
        )
    );
    assert_ne!(after_c2[1].data_id, before_c2[1].data_id);
    assert_eq!(data_subject(&verify, before_c2[1].data_id).await, None);
    assert_eq!(
        data_subject(&verify, after_c2[1].data_id).await.as_deref(),
        Some("edited by C again")
    );
    assert_eq!(
        journal_state(&verify, id).await.0[0],
        v1_json_before,
        "aggregating into version 2 does not touch version 1"
    );
    t.rows += 5;

    // A edits after C: the last journal is C's, so A cannot aggregate. First the failing
    // attempt: a statement that fails AFTER the UPDATE and the journal, on the same transaction
    // (`PgMutationStore::update_then`'s hook), must roll BOTH back
    // (service_context.rb:34-49 `raise ActiveRecord::Rollback if result.failure?`).
    let current = page_store.find_wp(id).await.unwrap().unwrap();
    let wp_before_fail = row_json(&verify, id).await.unwrap();
    let state_before_fail = journal_state(&verify, id).await;
    let r = pg_store(&pool)
        .update_then(
            &caller_a,
            id,
            patch_subject(&current, "A after C"),
            Some("SELECT 1 / 0"),
        )
        .await;
    assert!(matches!(r, Err(MutationError::Store(_))), "{r:?}");
    assert_eq!(
        row_json(&verify, id).await.unwrap(),
        wp_before_fail,
        "a failure after the journal leaves the work package byte-identical (no partial update)"
    );
    assert_eq!(
        journal_state(&verify, id).await,
        state_before_fail,
        "...and the journals and data rows byte-identical (no journal)"
    );
    t.rows += 2;
    // Control: the same call without the failing statement commits, so the identity above is
    // the rollback, not an update that never writes. It is a new version 3 by A.
    pg_store(&pool)
        .update_then(&caller_a, id, patch_subject(&current, "A after C"), None)
        .await
        .expect("the control update commits");
    let three = journals(&verify, id).await;
    assert_eq!(three.len(), 3, "the control commits version 3");
    assert_eq!((three[2].version, three[2].user_id), (3, USER_A));
    assert_eq!(
        three[1].hi,
        Some(three[2].created_at),
        "version 2 is closed at version 3's start"
    );
    assert_eq!(three[2].hi, None);
    assert_eq!(
        three[0].hi,
        Some(two[1].created_at),
        "version 1 stays closed where it was"
    );
    t.rows += 4;

    // Outside the window: A is the last journal's user again, but its updated_at is older than
    // the window (`within_aggregation_time?`, :603-607), so a new version is added. Age the
    // journal by ten minutes (created_at, updated_at and the start of its validity period), as
    // if the edit had been made then.
    sqlx::query(
        "UPDATE journals SET created_at = created_at - interval '10 minutes', \
             updated_at = updated_at - interval '10 minutes', \
             validity_period = tstzrange(lower(validity_period) - interval '10 minutes', \
                                         upper(validity_period)) \
         WHERE id = $1",
    )
    .bind(three[2].id)
    .execute(&pool)
    .await
    .expect("age the journal");
    let current = page_store.find_wp(id).await.unwrap().unwrap();
    mutation::update(
        &store,
        &caller_a,
        id,
        patch_subject(&current, "A, much later"),
    )
    .await
    .expect("A edits");
    let four = journals(&verify, id).await;
    assert_eq!(
        four.len(),
        4,
        "same user, but outside the window: a new version"
    );
    assert_eq!((four[3].version, four[3].user_id), (4, USER_A));
    assert_eq!(four[2].hi, Some(four[3].created_at), "version 3 is closed");
    t.rows += 3;

    // A non-empty cause on the last journal blocks aggregation (`only_one_or_same_cause?`:
    // NoCause merges into nothing, :615-617), even for the same user within the window.
    sqlx::query("UPDATE journals SET cause = '{\"type\": \"system_update\"}'::jsonb WHERE id = $1")
        .bind(four[3].id)
        .execute(&pool)
        .await
        .expect("set a cause");
    let current = page_store.find_wp(id).await.unwrap().unwrap();
    mutation::update(
        &store,
        &caller_a,
        id,
        patch_subject(&current, "A, after a caused journal"),
    )
    .await
    .expect("A edits");
    let five = journals(&verify, id).await;
    assert_eq!(
        five.len(),
        5,
        "a predecessor with a cause is not aggregated into"
    );
    assert_eq!(five[4].cause, serde_json::json!({}));
    assert_eq!(five[3].hi, Some(five[4].created_at));
    t.rows += 3;
    t.steps += 1;

    // ── 9. a work package that predates journalling gets a version-1 journal on its first edit
    // (the `changes` comparison against no data row finds every column different; there is no
    // predecessor to close or aggregate).
    let legacy: i64 = sqlx::query_scalar(
        "INSERT INTO work_packages (subject, project_id, type_id, status_id, priority_id, \
                                    author_id, done_ratio, lock_version) \
         VALUES ('legacy', $1, $2, $3, $4, $5, 0, 0) RETURNING id",
    )
    .bind(TARGET_PROJECT)
    .bind(TYPE_BUG)
    .bind(STATUS_DEFAULT)
    .bind(PRIORITY_DEFAULT)
    .bind(USER_A)
    .fetch_one(&pool)
    .await
    .expect("a legacy work package without journals");
    assert!(journals(&verify, legacy).await.is_empty());
    let row = page_store.find_wp(legacy).await.unwrap().unwrap();
    mutation::update(
        &store,
        &caller_a,
        legacy,
        patch_subject(&row, "legacy, edited"),
    )
    .await
    .expect("A edits the legacy work package");
    let lj = journals(&verify, legacy).await;
    assert_eq!(lj.len(), 1, "a first journal appears");
    assert_eq!((lj[0].version, lj[0].user_id, lj[0].hi), (1, USER_A, None));
    assert_eq!(
        data_subject(&verify, lj[0].data_id).await.as_deref(),
        Some("legacy, edited")
    );
    t.rows += 3;

    // `touch_journable`: when the last journal is newer than the UPDATE's timestamp (here moved an
    // hour ahead, as a concurrent note edit would), the journal write advances the work package's
    // updated_at past it. The row the update returns must carry that committed timestamp. Run on
    // the legacy work package, last, so the shifted clock reaches no other step.
    let ahead: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "UPDATE journals SET updated_at = updated_at + interval '1 hour' \
         WHERE id = $1 RETURNING updated_at",
    )
    .bind(lj[0].id)
    .fetch_one(&verify)
    .await
    .unwrap();
    let row = page_store.find_wp(legacy).await.unwrap().unwrap();
    let r = mutation::update(
        &store,
        &caller_a,
        legacy,
        patch_subject(&row, "legacy, journal ahead"),
    )
    .await
    .expect("A edits with the last journal ahead of the clock");
    let committed = wp_updated_at(&verify, legacy).await;
    assert!(
        committed > ahead,
        "the work package is touched past the last journal"
    );
    assert_eq!(
        r.updated_at, committed,
        "the returned row carries the committed updated_at, not the UPDATE's"
    );
    t.rows += 2;

    // The demo posture's anonymous caller may not edit (docs/decisions/0001-journaled-edits.md
    // §2): with no `users.type`, the edit could not be journaled, so it is refused before any
    // lookup and nothing is written.
    let anon = Caller::Anonymous { allowed: true };
    let current = page_store.find_wp(id).await.unwrap().unwrap();
    let wp_before = row_json(&verify, id).await.unwrap();
    let state_before = journal_state(&verify, id).await;
    let r = store
        .update_unit_of_work(&anon, id, patch_subject(&current, "anonymous demo edit"))
        .await;
    assert!(matches!(r, Err(MutationError::Unauthenticated)), "{r:?}");
    assert_eq!(
        row_json(&verify, id).await.unwrap(),
        wp_before,
        "the anonymous edit is not applied"
    );
    assert_eq!(
        journal_state(&verify, id).await,
        state_before,
        "and writes no journal"
    );
    t.rows += 2;

    // A work package stored without a priority (the column is nullable): an edit that does not
    // set one is refused with `priority: blank` and writes nothing (`validates :priority,
    // presence: true`, work_package/validations.rb:34, via ModelContract#valid?), where the
    // journal insert would otherwise fail on `work_package_journals.priority_id NOT NULL`. The
    // same edit with a priority is applied and journaled.
    let no_priority: i64 = sqlx::query_scalar(
        "INSERT INTO work_packages (subject, project_id, type_id, status_id, priority_id, \
                                    author_id, done_ratio, lock_version) \
         VALUES ('no priority', $1, $2, $3, NULL, $4, 0, 0) RETURNING id",
    )
    .bind(TARGET_PROJECT)
    .bind(TYPE_BUG)
    .bind(STATUS_DEFAULT)
    .bind(USER_A)
    .fetch_one(&pool)
    .await
    .expect("a work package without a priority");
    let row = page_store.find_wp(no_priority).await.unwrap().unwrap();
    let wp_before = row_json(&verify, no_priority).await.unwrap();
    let r = mutation::update(
        &store,
        &caller_a,
        no_priority,
        patch_subject(&row, "no priority, edited"),
    )
    .await;
    match r {
        Err(MutationError::Invalid(errors)) => assert_eq!(
            errors.get("priority"),
            Some(&vec!["blank".to_string()]),
            "refused with priority: blank"
        ),
        r => panic!("expected priority: blank, got {r:?}"),
    }
    assert_eq!(
        row_json(&verify, no_priority).await.unwrap(),
        wp_before,
        "nothing written"
    );
    assert!(
        journals(&verify, no_priority).await.is_empty(),
        "no journal"
    );
    let mut with_priority = patch_subject(&row, "no priority, edited");
    with_priority.priority_id = Some(PRIORITY_DEFAULT);
    mutation::update(&store, &caller_a, no_priority, with_priority)
        .await
        .expect("the edit that sets a priority is applied");
    assert_eq!(
        journals(&verify, no_priority).await.len(),
        1,
        "and journaled"
    );
    t.rows += 4;
    t.steps += 1;

    // ── 10. ancestor totals on create ────────────────────────────────
    // `WorkPackages::CreateService#after_create` runs `multi_update_ancestors`
    // (create_service.rb:75-77): `UpdateAncestorsService` derives, for each ancestor of the new
    // work package, `derived_estimated_hours`, `derived_remaining_hours` (the sum over the
    // ancestor and its descendants whose status is not `excluded_from_totals`,
    // update_ancestors_service.rb:219-228) and `derived_done_ratio` (work-weighted average:
    // `((estimated - remaining) / estimated * 100).round`, :129-138), saves only an ancestor whose
    // values changed (:62-69) and journals it with cause `work_package_children_changed_values`
    // (:99-102, caused_by_work_package_child_values_change.rb).
    //
    // Setting a parent needs `manage_subtasks` (`parent`, generated_create_admission.rs): the
    // member role of project 5 gets it here, and the add-only member D still does not have it.
    sqlx::query(
        "INSERT INTO role_permissions (role_id, permission) VALUES ($1, 'manage_subtasks')",
    )
    .bind(ROLE_A)
    .execute(&pool)
    .await
    .expect("grant manage_subtasks");
    sqlx::query(
        "INSERT INTO statuses (id, name, is_default, position, excluded_from_totals) \
         VALUES ($1, 'Rejected', false, 3, true)",
    )
    .bind(STATUS_EXCLUDED)
    .execute(&pool)
    .await
    .expect("a status excluded from totals");
    let cause = |of: i64| serde_json::json!({"type": "work_package_children_changed_values", "work_package_id": of});
    // Hours of G (100 / 100), P (60 / 59), C1 (40 / 40), C2 (1000 / 0, excluded), C3 (10 / 5),
    // C5 (20 / 20). G > P > C*.
    let g = create_hours(&store, &caller_a, "ancestor G", None, 100.0, 100.0, None)
        .await
        .expect("G");
    assert_eq!(
        derived(&verify, g).await,
        Derived {
            est: None,
            rem: None,
            ratio: None,
            lock_version: 0
        },
        "a work package with no descendants has no derived values"
    );
    // D may not set a parent: `parent` needs `manage_subtasks`. Nothing is stored.
    let wps_before = count(&verify, "SELECT count(*) FROM work_packages").await;
    let refused = create_hours(&store, &caller_d, "D's child", Some(g), 1.0, 1.0, None).await;
    assert!(
        matches!(refused, Err(MutationError::Invalid(_))),
        "a parent without manage_subtasks is refused: {refused:?}"
    );
    assert_eq!(
        count(&verify, "SELECT count(*) FROM work_packages").await,
        wps_before,
        "the refused child is not stored"
    );
    t.rows += 3;

    // P under G, both by A. G: estimated 100 + 60 = 160, remaining 100 + 59 = 159, done ratio
    // round(1 / 160 * 100) = round(0.625) = 1. G's journal 1 is A's, empty cause, seconds old:
    // aggregated (same user, within the window, `only_one_or_same_cause?` since the predecessor's
    // cause is empty): rewritten in place with the new cause, still one journal.
    let p = create_hours(&store, &caller_a, "ancestor P", Some(g), 60.0, 59.0, None)
        .await
        .expect("P under G");
    assert_eq!(
        derived(&verify, g).await,
        Derived {
            est: Some(160.0),
            rem: Some(159.0),
            ratio: Some(1),
            lock_version: 1
        },
        "G: 100 + 60, 100 + 59, round(0.625) = 1; saved once"
    );
    let gj = journals(&verify, g).await;
    assert_eq!(
        gj.len(),
        1,
        "same user, empty predecessor cause: aggregated"
    );
    assert_eq!((gj[0].version, gj[0].user_id, gj[0].hi), (1, USER_A, None));
    assert_eq!(
        gj[0].cause,
        cause(p),
        "the rewritten journal takes the new cause"
    );
    assert_eq!(
        data_derived(&verify, gj[0].data_id).await,
        (Some(160.0), Some(159.0), Some(1)),
        "its snapshot carries the derived values"
    );
    assert_eq!(
        derived(&verify, p).await,
        Derived {
            est: None,
            rem: None,
            ratio: None,
            lock_version: 0
        },
        "the new work package itself has no descendants: nothing derived"
    );
    assert_eq!(journals(&verify, p).await.len(), 1);
    t.rows += 6;

    // C1 under P, by A. P: 60 + 40 = 100, 59 + 40 = 99, round(1 / 100 * 100) = 1 (P's journal
    // is A's with an empty cause: aggregated). G: 100 + 60 + 40 = 200, 100 + 59 + 40 = 199,
    // round(1 / 200 * 100) = round(0.5) = 1: Ruby's `Float#round` goes half AWAY from zero (a
    // half-to-even rounding would give 0). G's last journal is A's with the cause of P, which
    // differs from the cause of C1 and is not mergeable (`Base#merge_into` is nil): NOT
    // aggregated, so G gets version 2 and version 1 is closed.
    let c1 = create_hours(&store, &caller_a, "ancestor C1", Some(p), 40.0, 40.0, None)
        .await
        .expect("C1 under P");
    assert_eq!(
        derived(&verify, p).await,
        Derived {
            est: Some(100.0),
            rem: Some(99.0),
            ratio: Some(1),
            lock_version: 1
        },
        "P: 60 + 40, 59 + 40, round(1.0) = 1"
    );
    assert_eq!(
        derived(&verify, g).await,
        Derived {
            est: Some(200.0),
            rem: Some(199.0),
            ratio: Some(1),
            lock_version: 2
        },
        "G, the grandparent, is updated too: 200, 199, round(0.5) = 1 (half away from zero)"
    );
    let pj = journals(&verify, p).await;
    assert_eq!(pj.len(), 1, "P's journal is aggregated");
    assert_eq!(pj[0].cause, cause(c1));
    let gj2 = journals(&verify, g).await;
    assert_eq!(
        gj2.len(),
        2,
        "a predecessor with a different cause is not aggregated into"
    );
    assert_eq!((gj2[1].version, gj2[1].user_id), (2, USER_A));
    assert_eq!(gj2[1].cause, cause(c1));
    assert_eq!(gj2[0].cause, cause(p), "version 1 keeps its cause");
    assert_eq!(
        gj2[0].hi,
        Some(gj2[1].created_at),
        "version 1 is closed at version 2's start"
    );
    assert_eq!(
        data_derived(&verify, gj2[0].data_id).await,
        (Some(160.0), Some(159.0), Some(1)),
        "version 1 still snapshots the totals before C1"
    );
    assert_eq!(
        data_derived(&verify, gj2[1].data_id).await,
        (Some(200.0), Some(199.0), Some(1))
    );
    assert_eq!(
        derived(&verify, c1).await,
        Derived {
            est: None,
            rem: None,
            ratio: None,
            lock_version: 0
        }
    );
    t.rows += 10;

    // C2 under P by C, in a status excluded from totals (1000 / 0): left out of every total, so
    // P's and G's derived values are unchanged, nothing is saved and nothing is journaled
    // (byte-identical rows). Two-sided with C3 below, which does count.
    let (p_row, g_row) = (
        row_json(&verify, p).await.unwrap(),
        row_json(&verify, g).await.unwrap(),
    );
    let (p_state, g_state) = (
        journal_state(&verify, p).await,
        journal_state(&verify, g).await,
    );
    let c2 = create_hours(
        &store,
        &caller_c,
        "ancestor C2 (excluded)",
        Some(p),
        1000.0,
        0.0,
        Some(STATUS_EXCLUDED),
    )
    .await
    .expect("C2 under P");
    assert_eq!(row_json(&verify, p).await.unwrap(), p_row, "P is not saved");
    assert_eq!(row_json(&verify, g).await.unwrap(), g_row, "G is not saved");
    assert_eq!(
        journal_state(&verify, p).await,
        p_state,
        "P is not journaled"
    );
    assert_eq!(
        journal_state(&verify, g).await,
        g_state,
        "G is not journaled"
    );
    assert_eq!(
        journals(&verify, c2).await.len(),
        1,
        "C2 has its own creation journal"
    );
    t.rows += 5;

    // C3 under P by C (10 / 5, default status). P: 60 + 40 + 10 = 110, 59 + 40 + 5 = 104,
    // round(6 / 110 * 100) = round(5.45) = 5. P's last journal is A's: C is another user, so
    // version 2. G: 100 + 60 + 40 + 10 = 210, 100 + 59 + 40 + 5 = 204, round(6 / 210 * 100) =
    // round(2.86) = 3; G's last journal (version 2) is A's: version 3 by C.
    let c3 = create_hours(&store, &caller_c, "ancestor C3", Some(p), 10.0, 5.0, None)
        .await
        .expect("C3 under P");
    assert_eq!(
        derived(&verify, p).await,
        Derived {
            est: Some(110.0),
            rem: Some(104.0),
            ratio: Some(5),
            lock_version: 2
        },
        "P: C2 (excluded) is not summed; 110, 104, round(5.45) = 5"
    );
    assert_eq!(
        derived(&verify, g).await,
        Derived {
            est: Some(210.0),
            rem: Some(204.0),
            ratio: Some(3),
            lock_version: 3
        },
        "G: 210, 204, round(2.86) = 3"
    );
    let pj = journals(&verify, p).await;
    assert_eq!(pj.len(), 2, "another user's change adds version 2");
    assert_eq!((pj[1].version, pj[1].user_id), (2, USER_C));
    assert_eq!(pj[1].cause, cause(c3));
    assert_eq!(pj[0].hi, Some(pj[1].created_at));
    assert_eq!(
        data_derived(&verify, pj[1].data_id).await,
        (Some(110.0), Some(104.0), Some(5))
    );
    assert_eq!(
        data_derived(&verify, pj[0].data_id).await,
        (Some(100.0), Some(99.0), Some(1)),
        "P's version 1 keeps the totals it was aggregated with"
    );
    let gj3 = journals(&verify, g).await;
    assert_eq!(gj3.len(), 3);
    assert_eq!((gj3[2].version, gj3[2].user_id), (3, USER_C));
    assert_eq!(gj3[2].cause, cause(c3));
    assert_eq!(
        data_derived(&verify, gj3[2].data_id).await,
        (Some(210.0), Some(204.0), Some(3))
    );
    t.rows += 11;

    // The ancestor update is on the create's transaction: a failure after it (the hook runs
    // after the ancestors and the watcher) leaves P and G, their journals, and the child exactly as
    // they were.
    let (p_row, g_row) = (
        row_json(&verify, p).await.unwrap(),
        row_json(&verify, g).await.unwrap(),
    );
    let (p_state, g_state) = (
        journal_state(&verify, p).await,
        journal_state(&verify, g).await,
    );
    let failing = FailAfterInsert {
        fail: true,
        pool: pool.clone(),
    };
    let r = create_hours(
        &failing,
        &caller_a,
        "ancestor C4 (rolled back)",
        Some(p),
        50.0,
        50.0,
        None,
    )
    .await;
    assert!(matches!(r, Err(MutationError::Store(_))), "{r:?}");
    assert_eq!(
        row_json(&verify, p).await.unwrap(),
        p_row,
        "P is unchanged after the rollback"
    );
    assert_eq!(
        row_json(&verify, g).await.unwrap(),
        g_row,
        "G is unchanged after the rollback"
    );
    assert_eq!(
        journal_state(&verify, p).await,
        p_state,
        "no P journal after the rollback"
    );
    assert_eq!(
        journal_state(&verify, g).await,
        g_state,
        "no G journal after the rollback"
    );
    assert_eq!(
        count(
            &verify,
            "SELECT count(*) FROM work_packages WHERE subject = 'ancestor C4 (rolled back)'"
        )
        .await,
        0,
        "the child is not stored"
    );
    // Control: the same wrapper without the failure commits, so the identity above is the
    // rollback. C5 (20 / 20) by A: P 130 / 124, round(6 / 130 * 100) = round(4.6) = 5, P version
    // 3 (the last journal is C's); G 230 / 224, round(6 / 230 * 100) = round(2.6) = 3, version 4.
    let control = FailAfterInsert {
        fail: false,
        pool: pool.clone(),
    };
    let c5 = create_hours(
        &control,
        &caller_a,
        "ancestor C5",
        Some(p),
        20.0,
        20.0,
        None,
    )
    .await
    .expect("the control create commits");
    assert_eq!(
        derived(&verify, p).await,
        Derived {
            est: Some(130.0),
            rem: Some(124.0),
            ratio: Some(5),
            lock_version: 3
        },
        "the control changes P"
    );
    assert_eq!(
        derived(&verify, g).await,
        Derived {
            est: Some(230.0),
            rem: Some(224.0),
            ratio: Some(3),
            lock_version: 4
        },
        "the control changes G"
    );
    let (pj, gj) = (journals(&verify, p).await, journals(&verify, g).await);
    assert_eq!((pj.len(), gj.len()), (3, 4));
    assert_eq!(
        (pj[2].version, pj[2].user_id, &pj[2].cause),
        (3, USER_A, &cause(c5))
    );
    assert_eq!(
        (gj[3].version, gj[3].user_id, &gj[3].cause),
        (4, USER_A, &cause(c5))
    );
    t.rows += 12;
    t.steps += 1;

    // Two creates under P at the same time. A recomputes P's totals and saves P, holding its row
    // lock until A commits; B has read P (and P's descendants, without A's uncommitted child)
    // before that and then waits on the lock. ActiveRecord's optimistic locking makes B's save
    // `UPDATE ... WHERE lock_version = <as loaded>` find no row once A commits, and
    // `StaleObjectError` fails B: B must not overwrite P with totals that miss A's child.
    let p_before = derived(&verify, p).await;
    let race = |subject: &str| {
        let mut params = WorkPackageParams::new()
            .with_subject(subject)
            .with_estimated_hours(8.0)
            .with_remaining_hours(8.0);
        params.project_id = Some(TARGET_PROJECT);
        params.parent_id = Some(p);
        params
    };
    let a = PgCreateScope::begin(&pool).await.expect("A begins");
    let row_a = mutation::create_in(&a, &caller_a, race("race A"))
        .await
        .expect("A inserts");
    a.update_ancestors_of_created(row_a.id, row_a.parent_id, USER_A)
        .await
        .expect("A saves P");
    let b = PgCreateScope::begin(&pool).await.expect("B begins");
    let row_b = mutation::create_in(&b, &caller_a, race("race B"))
        .await
        .expect("B inserts");
    let b_task = tokio::spawn(async move {
        let r = b
            .update_ancestors_of_created(row_b.id, row_b.parent_id, USER_A)
            .await;
        b.rollback().await;
        r
    });
    // Wait until B is blocked on P's row lock, so B has loaded P before A commits.
    let mut waiting = false;
    for _ in 0..200 {
        let n = count(
            &verify,
            "SELECT count(*) FROM pg_stat_activity \
             WHERE wait_event_type = 'Lock' AND datname = current_database()",
        )
        .await;
        if n > 0 {
            waiting = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(waiting, "B waits on P's row lock held by A");
    a.commit().await.expect("A commits");
    let r = b_task.await.expect("B's task");
    assert!(
        matches!(r, Err(StoreFailure::Conflict(_))),
        "B's stale save of P is a conflict, not a lost update: {r:?}"
    );
    assert_eq!(
        derived(&verify, p).await,
        // 130 + 8, 124 + 8; round((138 - 132) / 138 * 100) = round(4.35) = 4.
        Derived {
            est: Some(138.0),
            rem: Some(132.0),
            ratio: Some(4),
            lock_version: p_before.lock_version + 1
        },
        "P carries A's child (saved once, by A) and nothing of B's"
    );
    assert_eq!(
        count(
            &verify,
            "SELECT count(*) FROM work_packages WHERE subject = 'race B'"
        )
        .await,
        0,
        "B rolled back"
    );
    t.rows += 3;

    // ── 11. ancestor totals on update ────────────────────────────────
    // `WorkPackages::UpdateService#update_related_work_packages` (update_service.rb:54-81) runs
    // `UpdateAncestorsService` with the attributes the edit changed. The initiator, its ancestors
    // and (after a parent change) the former parent's ancestors are re-derived; only the
    // attributes of `modified_attributes_justify_derivation?` (update_ancestors_service.rb:239-248)
    // start it, and `derive_attributes` (:96-121) runs only the derivations those attributes name.
    let cause_of = |of: i64| serde_json::json!({"type": "work_package_children_changed_values", "work_package_id": of});
    let edit =
        |id: i64, who: Caller, f: Box<dyn FnOnce(&mut op_db::UpdateWorkPackageDto) + Send>| {
            let store = pg_store(&pool);
            async move {
                let cur = store.find_wp(id).await.unwrap().unwrap();
                let mut dto = patch_subject(&cur, &cur.subject);
                f(&mut dto);
                store.update_unit_of_work(&who, id, dto).await
            }
        };

    // Tree 1: GG (100 / 100) > PP (50 / 50) > K (10 / 10).
    // GG 160 / 160, round(0 / 160 * 100) = 0 (lock 2: saved by PP's and K's creation); PP 60 / 60,
    // 0 (lock 1); K has no descendants. GG: v1 (cause PP, snapshot 150 / 150 / 0), v2 (cause K).
    let gg = create_hours(&store, &caller_a, "update GG", None, 100.0, 100.0, None)
        .await
        .expect("GG");
    let pp = create_hours(&store, &caller_a, "update PP", Some(gg), 50.0, 50.0, None)
        .await
        .expect("PP");
    let k = create_hours(&store, &caller_a, "update K", Some(pp), 10.0, 10.0, None)
        .await
        .expect("K");
    assert_eq!(
        derived(&verify, gg).await,
        Derived {
            est: Some(160.0),
            rem: Some(160.0),
            ratio: Some(0),
            lock_version: 2
        }
    );
    assert_eq!(
        derived(&verify, pp).await,
        Derived {
            est: Some(60.0),
            rem: Some(60.0),
            ratio: Some(0),
            lock_version: 1
        }
    );
    assert_eq!(journals(&verify, gg).await.len(), 2);
    assert_eq!(journals(&verify, pp).await.len(), 1);
    t.rows += 4;

    // A. A sets K's estimate to 30 (remaining stays 10). PP: 50 + 30 = 80, 50 + 10 = 60,
    // round(20 / 80 * 100) = 25, lock 2. GG: 100 + 50 + 30 = 180, 100 + 50 + 10 = 160,
    // round(20 / 180 * 100) = round(11.11) = 11, lock 3. K's own journal is A's, empty cause,
    // seconds old: the edit is aggregated (one journal, estimate 30), a normal edit journal with
    // no cause. PP's last journal has A's cause of K (same cause): aggregated, still one journal.
    // GG's last journal (v2) has the same cause: aggregated, still two; v1 keeps 150 / 150 / 0.
    let k_row = edit(
        k,
        caller_a.clone(),
        Box::new(|d| d.estimated_hours = Some(30.0)),
    )
    .await
    .expect("A edits K's estimate");
    assert_eq!(
        k_row.lock_version, 1,
        "K saved once; it has no totals of its own"
    );
    assert_eq!(
        derived(&verify, pp).await,
        Derived {
            est: Some(80.0),
            rem: Some(60.0),
            ratio: Some(25),
            lock_version: 2
        },
        "PP: 50 + 30, 50 + 10, round(0.25 * 100) = 25"
    );
    assert_eq!(
        derived(&verify, gg).await,
        Derived {
            est: Some(180.0),
            rem: Some(160.0),
            ratio: Some(11),
            lock_version: 3
        },
        "GG, the grandparent, too: 180, 160, round(11.11) = 11"
    );
    let kj = journals(&verify, k).await;
    assert_eq!(
        kj.len(),
        1,
        "K's edit is aggregated into its creation journal"
    );
    assert_eq!(kj[0].cause, serde_json::json!({}), "an edit has no cause");
    assert_eq!(data_est(&verify, kj[0].data_id).await, Some(30.0));
    let pj = journals(&verify, pp).await;
    assert_eq!((pj.len(), &pj[0].cause), (1, &cause_of(k)));
    assert_eq!(
        data_derived(&verify, pj[0].data_id).await,
        (Some(80.0), Some(60.0), Some(25))
    );
    let gj = journals(&verify, gg).await;
    assert_eq!((gj.len(), &gj[1].cause), (2, &cause_of(k)));
    assert_eq!(
        data_derived(&verify, gj[1].data_id).await,
        (Some(180.0), Some(160.0), Some(11))
    );
    assert_eq!(
        data_derived(&verify, gj[0].data_id).await,
        (Some(150.0), Some(150.0), Some(0)),
        "GG's v1 keeps its totals"
    );
    t.rows += 11;

    // B. C moves K to a status excluded from totals: K's 30 / 10 drop out of every sum. PP: own
    // 50 / 50 only, round(0) = 0, lock 3; GG: 100 + 50 = 150 / 150, 0, lock 4. A status change alone
    // starts the derivation. C is another user: K, PP and GG each get a new version by C
    // (PP v2, GG v3), with the cause of K, and the predecessors' snapshots stay.
    let k_row = edit(
        k,
        caller_c.clone(),
        Box::new(|d| d.status_id = Some(STATUS_EXCLUDED)),
    )
    .await
    .expect("C edits K's status");
    assert_eq!(k_row.lock_version, 2);
    assert_eq!(
        derived(&verify, pp).await,
        Derived {
            est: Some(50.0),
            rem: Some(50.0),
            ratio: Some(0),
            lock_version: 3
        },
        "PP: K is excluded"
    );
    assert_eq!(
        derived(&verify, gg).await,
        Derived {
            est: Some(150.0),
            rem: Some(150.0),
            ratio: Some(0),
            lock_version: 4
        },
        "GG: K is excluded"
    );
    let kj = journals(&verify, k).await;
    assert_eq!(
        (kj.len(), kj[1].user_id, &kj[1].cause),
        (2, USER_C, &serde_json::json!({}))
    );
    let pj = journals(&verify, pp).await;
    assert_eq!(
        (pj.len(), pj[1].user_id, &pj[1].cause),
        (2, USER_C, &cause_of(k))
    );
    assert_eq!(
        data_derived(&verify, pj[1].data_id).await,
        (Some(50.0), Some(50.0), Some(0))
    );
    assert_eq!(
        data_derived(&verify, pj[0].data_id).await,
        (Some(80.0), Some(60.0), Some(25))
    );
    let gj = journals(&verify, gg).await;
    assert_eq!(
        (gj.len(), gj[2].user_id, &gj[2].cause),
        (3, USER_C, &cause_of(k))
    );
    assert_eq!(
        data_derived(&verify, gj[2].data_id).await,
        (Some(150.0), Some(150.0), Some(0))
    );
    assert_eq!(
        data_derived(&verify, gj[1].data_id).await,
        (Some(180.0), Some(160.0), Some(11))
    );
    t.rows += 12;

    // C. A edits PP, which has a child: PP's own estimate 50 -> 70. PP is its own initiator: its
    // totals are 70 / 50 (K excluded: only PP's own 70 and 50), round(20 / 70 * 100) =
    // round(28.57) = 29. PP's lock_version: 3 -> 4 (the UPDATE) -> 5 (the initiator's re-save), and
    // the answer carries 5. The update's journal is a new version 3 by A (C wrote v2) with no cause,
    // and the re-save is aggregated into it (same user, empty cause): one journal whose snapshot has
    // both the new estimate 70 and the derived 70 / 50 / 29. GG: 100 + 70 = 170, 100 + 50 = 150,
    // round(20 / 170 * 100) = round(11.76) = 12, lock 5, a new version 4 by A (v3 is C's) with the
    // cause of PP.
    let pp_row = edit(
        pp,
        caller_a.clone(),
        Box::new(|d| d.estimated_hours = Some(70.0)),
    )
    .await
    .expect("A edits PP's estimate");
    assert_eq!(
        pp_row.lock_version, 5,
        "UPDATE, then the initiator's own re-save"
    );
    assert_eq!(
        derived(&verify, pp).await,
        Derived {
            est: Some(70.0),
            rem: Some(50.0),
            ratio: Some(29),
            lock_version: 5
        },
        "PP's own totals follow its own children (K excluded): 70, 50, round(28.57) = 29"
    );
    let pj = journals(&verify, pp).await;
    assert_eq!(
        pj.len(),
        3,
        "the re-save is aggregated into the edit's journal"
    );
    assert_eq!(
        (pj[2].version, pj[2].user_id, &pj[2].cause),
        (3, USER_A, &serde_json::json!({}))
    );
    assert_eq!(data_est(&verify, pj[2].data_id).await, Some(70.0));
    assert_eq!(
        data_derived(&verify, pj[2].data_id).await,
        (Some(70.0), Some(50.0), Some(29)),
        "the one journal snapshots the edit and the derived values"
    );
    assert_eq!(
        derived(&verify, gg).await,
        Derived {
            est: Some(170.0),
            rem: Some(150.0),
            ratio: Some(12),
            lock_version: 5
        },
        "GG: 100 + 70, 100 + 50, round(11.76) = 12"
    );
    let gj = journals(&verify, gg).await;
    assert_eq!(
        (gj.len(), gj[3].version, gj[3].user_id, &gj[3].cause),
        (4, 4, USER_A, &cause_of(pp))
    );
    assert_eq!(
        data_derived(&verify, gj[3].data_id).await,
        (Some(170.0), Some(150.0), Some(12))
    );
    t.rows += 11;

    // Tree 2: R1 (100 / 100) > M1 (20 / 20) > X (10 / 4); R2 (200 / 180) > M2 (40 / 30).
    // M1 30 / 24, round(20.0) = 20; R1 130 / 124, round(4.615) = 5; R2 240 / 210,
    // round(12.5) = 13 (half away from zero).
    let r1 = create_hours(&store, &caller_a, "move R1", None, 100.0, 100.0, None)
        .await
        .expect("R1");
    let m1 = create_hours(&store, &caller_a, "move M1", Some(r1), 20.0, 20.0, None)
        .await
        .expect("M1");
    let x = create_hours(&store, &caller_a, "move X", Some(m1), 10.0, 4.0, None)
        .await
        .expect("X");
    let r2 = create_hours(&store, &caller_a, "move R2", None, 200.0, 180.0, None)
        .await
        .expect("R2");
    let m2 = create_hours(&store, &caller_a, "move M2", Some(r2), 40.0, 30.0, None)
        .await
        .expect("M2");
    assert_eq!(
        derived(&verify, m1).await,
        Derived {
            est: Some(30.0),
            rem: Some(24.0),
            ratio: Some(20),
            lock_version: 1
        }
    );
    assert_eq!(
        derived(&verify, r1).await,
        Derived {
            est: Some(130.0),
            rem: Some(124.0),
            ratio: Some(5),
            lock_version: 2
        }
    );
    assert_eq!(
        derived(&verify, r2).await,
        Derived {
            est: Some(240.0),
            rem: Some(210.0),
            ratio: Some(13),
            lock_version: 1
        }
    );
    let counts = |ids: [i64; 4]| {
        let verify = verify.clone();
        async move {
            let mut out = Vec::new();
            for id in ids {
                out.push(journals(&verify, id).await.len());
            }
            out
        }
    };
    let before = counts([m1, r1, m2, r2]).await;
    t.rows += 4;

    // D. C moves X from M1 to M2 (parent_id changes). The former chain M1 > R1 is recomputed too:
    // M1 has no descendants any more (all three derived values nil, lock 2); R1: 100 + 20 = 120,
    // 100 + 20 = 120, round(0) = 0, lock 3. The new chain: M2: 40 + 10 = 50, 30 + 4 = 34,
    // round(16 / 50 * 100) = 32, lock 1; R2: 250, 214, round(36 / 250 * 100) = round(14.4) = 14,
    // lock 2. Every one of the four gets a new journal by C (their last journals are A's) with the
    // cause of X. X itself has no descendants: its derived values stay nil and it is saved once
    // (lock 1).
    let x_row = edit(
        x,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(m2)),
    )
    .await
    .expect("C moves X under M2");
    assert_eq!(x_row.lock_version, 1, "X is not re-saved: it has no totals");
    for (id, want) in [
        (
            m1,
            Derived {
                est: None,
                rem: None,
                ratio: None,
                // 2 from the totals, 3 from the rescheduling of the former parent: M1 is an
                // automatically scheduled parent (its first child switched it) that has just lost
                // its last child, with no predecessor, so `SetScheduleService#reschedule` turns it
                // manual (set_schedule_service.rb:108-116, `scheduled.schedule_manually = true`).
                lock_version: 3,
            },
        ),
        (
            r1,
            Derived {
                est: Some(120.0),
                rem: Some(120.0),
                ratio: Some(0),
                lock_version: 3,
            },
        ),
        (
            m2,
            Derived {
                est: Some(50.0),
                rem: Some(34.0),
                ratio: Some(32),
                lock_version: 1,
            },
        ),
        (
            r2,
            Derived {
                est: Some(250.0),
                rem: Some(214.0),
                ratio: Some(14),
                lock_version: 2,
            },
        ),
    ] {
        assert_eq!(derived(&verify, id).await, want, "work package {id}");
    }
    let after = counts([m1, r1, m2, r2]).await;
    for i in 0..4 {
        // M1 (index 0) is saved twice by C: its totals (child-values cause), then the rescheduling
        // of the former parent (no cause: no date changed), which cannot aggregate into a journal
        // with a different non-empty cause.
        assert_eq!(
            after[i],
            before[i] + if i == 0 { 2 } else { 1 },
            "chain member {i} got its new journals"
        );
    }
    let m1_journals = journals(&verify, m1).await;
    let (m1_last, m1_totals) = (
        &m1_journals[m1_journals.len() - 1],
        &m1_journals[m1_journals.len() - 2],
    );
    assert_eq!(
        (m1_last.user_id, &m1_last.cause, &m1_totals.cause),
        (USER_C, &serde_json::json!({}), &cause_of(x))
    );
    assert_eq!(
        data_sched(&verify, m1_last.data_id).await,
        (Some(true), false),
        "M1 is manual again"
    );
    assert_eq!(
        data_sched(&verify, m1_totals.data_id).await,
        (Some(false), false),
        "and was automatic in the totals' journal"
    );
    for (id, snap) in [
        (m1, (None, None, None)),
        (r1, (Some(120.0), Some(120.0), Some(0))),
        (m2, (Some(50.0), Some(34.0), Some(32))),
        (r2, (Some(250.0), Some(214.0), Some(14))),
    ] {
        let j = journals(&verify, id).await;
        let last = j.last().unwrap();
        let want_cause = if id == m1 {
            serde_json::json!({})
        } else {
            cause_of(x)
        };
        assert_eq!(
            (last.user_id, &last.cause),
            (USER_C, &want_cause),
            "work package {id}"
        );
        assert_eq!(
            data_derived(&verify, last.data_id).await,
            snap,
            "work package {id}"
        );
    }
    assert_eq!(
        journals(&verify, x).await.len(),
        2,
        "X: creation + C's edit"
    );
    t.rows += 15;

    // F. Rollback: A raises X's estimate 10 -> 20, and a failing statement after the ancestor
    // writes (`update_then`'s hook, as in step 8) rolls everything back: X, M2, R2 and their
    // journals are byte-identical. The control with a passing hook commits: M2 60 / 34,
    // round(26 / 60 * 100) = round(43.33) = 43, lock 2; R2 260 / 214, round(46 / 260 * 100) =
    // round(17.69) = 18, lock 3. The estimate did not change the parent, so the former chain
    // (M1, R1) is untouched.
    let snap = |ids: [i64; 5]| {
        let verify = verify.clone();
        async move {
            let mut out = Vec::new();
            for id in ids {
                out.push((
                    row_json(&verify, id).await.unwrap(),
                    journal_state(&verify, id).await,
                ));
            }
            out
        }
    };
    let all5 = [x, m2, r2, m1, r1];
    let frozen = snap(all5).await;
    let cur = pg_store(&pool).find_wp(x).await.unwrap().unwrap();
    let mut dto = patch_subject(&cur, &cur.subject);
    dto.estimated_hours = Some(20.0);
    let failed = pg_store(&pool)
        .update_then(&caller_a, x, dto, Some("SELECT 1 / 0"))
        .await;
    assert!(matches!(failed, Err(MutationError::Store(_))), "{failed:?}");
    assert_eq!(
        snap(all5).await,
        frozen,
        "the rollback leaves ancestors and journals untouched"
    );
    let cur = pg_store(&pool).find_wp(x).await.unwrap().unwrap();
    let mut dto = patch_subject(&cur, &cur.subject);
    dto.estimated_hours = Some(20.0);
    pg_store(&pool)
        .update_then(&caller_a, x, dto, Some("SELECT 1"))
        .await
        .expect("the control commits");
    assert_eq!(
        derived(&verify, m2).await,
        Derived {
            est: Some(60.0),
            rem: Some(34.0),
            ratio: Some(43),
            lock_version: 2
        }
    );
    assert_eq!(
        derived(&verify, r2).await,
        Derived {
            est: Some(260.0),
            rem: Some(214.0),
            ratio: Some(18),
            lock_version: 3
        }
    );
    let after_control = snap(all5).await;
    assert_eq!(
        after_control[3], frozen[3],
        "M1 is not touched: the parent did not change"
    );
    assert_eq!(
        after_control[4], frozen[4],
        "R1 is not touched: the parent did not change"
    );
    t.rows += 6;

    // E. An edit that changes none of the derivation attributes (the subject) writes no ancestor
    // and no ancestor journal. R2's derived estimate is first corrupted by hand (999): a recompute
    // would repair it to 260 and bump the lock, so identical rows prove no recompute ran.
    sqlx::query("UPDATE work_packages SET derived_estimated_hours = 999 WHERE id = $1")
        .bind(r2)
        .execute(&pool)
        .await
        .expect("corrupt R2's derived estimate");
    let frozen = snap(all5).await;
    let cur = pg_store(&pool).find_wp(x).await.unwrap().unwrap();
    pg_store(&pool)
        .update_unit_of_work(&caller_a, x, patch_subject(&cur, "move X, renamed"))
        .await
        .expect("a subject-only edit");
    let now = snap(all5).await;
    assert_eq!(
        (&now[1], &now[2], &now[3], &now[4]),
        (&frozen[1], &frozen[2], &frozen[3], &frozen[4]),
        "no ancestor row and no ancestor journal changed"
    );
    assert_eq!(
        derived(&verify, r2).await.est,
        Some(999.0),
        "R2 was not recomputed"
    );

    // A done-ratio-only edit runs only `derive_done_ratio` (update_ancestors_service.rb:112-113):
    // R2's derived ratio is recomputed from its derived hours AS LOADED (999 / 214):
    // round(785 / 999 * 100) = round(78.58) = 79, lock 4, while its derived estimate stays 999.
    // M2's ratio, from 60 / 34, is still 43: not saved, rows identical.
    let m2_frozen = snap([m2, m2, m2, m2, m2]).await.remove(0);
    edit(x, caller_a.clone(), Box::new(|d| d.done_ratio = Some(50)))
        .await
        .expect("a done-ratio edit");
    assert_eq!(
        derived(&verify, r2).await,
        Derived {
            est: Some(999.0),
            rem: Some(214.0),
            ratio: Some(79),
            lock_version: 4
        },
        "only the done ratio was derived, from the loaded hours"
    );
    assert_eq!(
        snap([m2, m2, m2, m2, m2]).await.remove(0),
        m2_frozen,
        "M2's ratio is unchanged"
    );
    t.rows += 5;
    t.steps += 1;

    // ── 12. switch_to_automatic_mode and derive_ignore_non_working_days ──
    // The two remaining derivations of `UpdateAncestorsService#derive_attributes`
    // (update_ancestors_service.rb:96-121), both selected by a parent change (create with a
    // parent, or an update that moves a work package): `switch_to_automatic_mode` (:157-181) and
    // `derive_ignore_non_working_days` (:183-199). The work packages here carry no hours, so every
    // derived total stays nil and an ancestor that is saved at all is saved for its scheduling
    // flags alone. New work packages are manually scheduled (`schedule_manually` defaults true).
    let create_plain_by_a = |subject: &'static str, parent: Option<i64>| {
        let (store, caller) = (pg_store(&pool), caller_a.clone());
        async move {
            create_plain(&store, &caller, subject, parent)
                .await
                .expect(subject)
        }
    };
    let freeze = |id: i64| {
        let verify = verify.clone();
        async move {
            (
                row_json(&verify, id).await.unwrap(),
                journal_state(&verify, id).await,
            )
        }
    };
    let set_manual = |id: i64, manual: bool| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE work_packages SET schedule_manually = $2 WHERE id = $1")
                .bind(id)
                .bind(manual)
                .execute(&pool)
                .await
                .expect("set schedule_manually");
        }
    };
    let set_ignore = |id: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE work_packages SET ignore_non_working_days = true WHERE id = $1")
                .bind(id)
                .execute(&pool)
                .await
                .expect("set ignore_non_working_days");
        }
    };
    let follows = |from: i64, to: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO relations (from_id, to_id, relation_type) VALUES ($1, $2, 'follows')",
            )
            .bind(from)
            .bind(to)
            .execute(&pool)
            .await
            .expect("a follows relation: `from` follows `to`");
        }
    };

    // A. A create under a manually scheduled parent with no other child and no scheduling relation
    // switches the parent to automatic: the parent's only change is `schedule_manually`, and it is
    // saved once (lock 0 -> 1) and journaled with the child-values cause. The journal is A's own
    // creation journal with an empty cause: aggregated, still one journal, now with C1's cause and
    // a snapshot of the switched parent. The initiator C1 is not touched.
    let p1 = create_plain_by_a("switch P1", None).await;
    assert_eq!(
        sched(&verify, p1).await,
        (true, false, 0),
        "a new work package is manual"
    );
    let c1 = create_plain_by_a("switch C1", Some(p1)).await;
    assert_eq!(
        sched(&verify, p1).await,
        (false, false, 1),
        "P1: manual, one child, no relation: automatic; saved once"
    );
    let j = journals(&verify, p1).await;
    assert_eq!(
        (j.len(), j[0].user_id, &j[0].cause),
        (1, USER_A, &cause_of(c1))
    );
    assert_eq!(
        data_sched(&verify, j[0].data_id).await,
        (Some(false), false)
    );
    assert_eq!(
        sched(&verify, c1).await,
        (true, false, 0),
        "the initiator is not touched"
    );
    assert_eq!(journals(&verify, c1).await.len(), 1);
    t.rows += 7;

    // B. A second child: the parent has another child, so it stays manual and is not saved at
    // all. P2 is made manual again by hand after its first child switched it.
    let p2 = create_plain_by_a("switch P2", None).await;
    let c2a = create_plain_by_a("switch C2a", Some(p2)).await;
    assert_eq!(
        sched(&verify, p2).await,
        (false, false, 1),
        "the first child switches P2"
    );
    set_manual(p2, true).await;
    let frozen = freeze(p2).await;
    let c2b = create_plain_by_a("switch C2b", Some(p2)).await;
    assert_eq!(
        sched(&verify, p2).await,
        (true, false, 1),
        "P2 has two children: it stays manual"
    );
    assert_eq!(
        freeze(p2).await,
        frozen,
        "and no row and no journal of P2 changed"
    );
    assert_ne!(c2a, c2b);
    t.rows += 4;

    // C. A scheduling predecessor of the parent: P3 follows Q (`relations.from_id` = the
    // successor, `relation_type = 'follows'`, Relation::TYPE_FOLLOWS, relation.rb:41-44 and the
    // `follows` scope). `Relation.used_for_scheduling_of(P3)` is not empty: P3 stays manual.
    let q = create_plain_by_a("switch Q", None).await;
    let p3 = create_plain_by_a("switch P3", None).await;
    follows(p3, q).await;
    let frozen = freeze(p3).await;
    create_plain_by_a("switch C3", Some(p3)).await;
    assert_eq!(
        sched(&verify, p3).await,
        (true, false, 0),
        "P3 has a predecessor: manual"
    );
    assert_eq!(freeze(p3).await, frozen);
    // The direction matters: when Q follows P3b, P3b is the predecessor, not the successor, and
    // nothing is scheduled by it. P3b switches.
    let p3b = create_plain_by_a("switch P3b", None).await;
    follows(q, p3b).await;
    create_plain_by_a("switch C3b", Some(p3b)).await;
    assert_eq!(
        sched(&verify, p3b).await,
        (false, false, 1),
        "Q follows P3b: P3b has no predecessor and switches"
    );
    t.rows += 3;

    // D. An automatically scheduled ancestor's predecessor counts: G4 (automatic, switched by P4)
    // follows Q, P4 is manual and has no relation of its own. `used_for_scheduling_of(P4)` takes
    // the relations of P4 and of its automatically scheduled ancestors (used_for_scheduling_of.rb:
    // 44-52): G4's counts, so P4 stays manual, and neither P4 nor G4 is saved.
    let g4 = create_plain_by_a("switch G4", None).await;
    let p4 = create_plain_by_a("switch P4", Some(g4)).await;
    assert_eq!(
        sched(&verify, g4).await,
        (false, false, 1),
        "P4 switched G4"
    );
    follows(g4, q).await;
    let (frozen_g4, frozen_p4) = (freeze(g4).await, freeze(p4).await);
    create_plain_by_a("switch C4", Some(p4)).await;
    assert_eq!(
        sched(&verify, p4).await,
        (true, false, 0),
        "G4's predecessor holds P4 manual"
    );
    assert_eq!((freeze(g4).await, freeze(p4).await), (frozen_g4, frozen_p4));
    // ... but a manually scheduled ancestor cuts the chain (:55-59): G4b is manual and follows Q,
    // yet its predecessor is not involved in scheduling P4b, which therefore switches. G4b itself
    // is not the direct parent and stays manual (not saved).
    let g4b = create_plain_by_a("switch G4b", None).await;
    let p4b = create_plain_by_a("switch P4b", Some(g4b)).await;
    set_manual(g4b, true).await;
    follows(g4b, q).await;
    let frozen_g4b = freeze(g4b).await;
    create_plain_by_a("switch C4b", Some(p4b)).await;
    assert_eq!(
        sched(&verify, p4b).await,
        (false, false, 1),
        "a manual ancestor's predecessor is not used: P4b switches"
    );
    assert_eq!(
        freeze(g4b).await,
        frozen_g4b,
        "G4b stays manual and untouched"
    );
    t.rows += 6;

    // E. An update moving MV from OLD to NEW. Only the new parent is the initiator's direct
    // parent: NEW (manual, MV its only child) switches, and C's edit is another user's, so NEW
    // gets a new version 2 with MV's cause. OLD keeps one child (STAY) and is manual, which would
    // qualify but for not being the direct parent: it is untouched.
    let old = create_plain_by_a("move OLD", None).await;
    let mv = create_plain_by_a("move MV", Some(old)).await;
    let stay = create_plain_by_a("move STAY", Some(old)).await;
    let new = create_plain_by_a("move NEW", None).await;
    set_manual(old, true).await;
    let frozen_old = freeze(old).await;
    let mv_row = edit(
        mv,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(new)),
    )
    .await
    .expect("C moves MV under NEW");
    assert_eq!(mv_row.lock_version, 1, "MV is saved by its own update only");
    assert_eq!(sched(&verify, new).await, (false, false, 1), "NEW switches");
    let j = journals(&verify, new).await;
    assert_eq!((j.len(), j[1].version, j[1].user_id), (2, 2, USER_C));
    assert_eq!(j[1].cause, cause_of(mv));
    assert_eq!(
        data_sched(&verify, j[1].data_id).await,
        (Some(false), false)
    );
    assert_eq!(
        data_sched(&verify, j[0].data_id).await,
        (Some(true), false),
        "v1 keeps manual"
    );
    assert_eq!(
        sched(&verify, old).await,
        (true, false, 1),
        "OLD keeps its one child STAY and is not the direct parent: manual"
    );
    assert_eq!(
        freeze(old).await,
        frozen_old,
        "OLD's row and journals did not change"
    );
    assert_ne!(stay, mv);
    t.rows += 8;

    // F. ignore_non_working_days. CH has it set (by hand: no edit writes it) and is moved under
    // PA, which is manual with CH as its only child: PA switches first and, being automatic by
    // then (:96-121 runs the two in this order), takes CH's true. One save: lock 0 -> 1; C's edit
    // is a new version 2 with CH's cause.
    let pa = create_plain_by_a("ignore PA", None).await;
    let ch = create_plain_by_a("ignore CH", None).await;
    set_ignore(ch).await;
    edit(
        ch,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(pa)),
    )
    .await
    .expect("C moves CH under PA");
    assert_eq!(
        sched(&verify, pa).await,
        (false, true, 1),
        "PA: switched, then ignores like CH"
    );
    let j = journals(&verify, pa).await;
    assert_eq!(
        (j.len(), j[1].user_id, &j[1].cause),
        (2, USER_C, &cause_of(ch))
    );
    assert_eq!(data_sched(&verify, j[1].data_id).await, (Some(false), true));
    assert_eq!(data_sched(&verify, j[0].data_id).await, (Some(true), false));
    // An automatically scheduled parent with another child: no switch (it is automatic), the
    // flag follows any child. PB: PBX (false) and CH2 (true): true; lock 1 (PBX) + 1.
    let pb = create_plain_by_a("ignore PB", None).await;
    create_plain_by_a("ignore PBX", Some(pb)).await;
    assert_eq!(sched(&verify, pb).await, (false, false, 1));
    let ch2 = create_plain_by_a("ignore CH2", None).await;
    set_ignore(ch2).await;
    edit(
        ch2,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(pb)),
    )
    .await
    .expect("C moves CH2 under PB");
    assert_eq!(
        sched(&verify, pb).await,
        (false, true, 2),
        "any child true: PB true"
    );
    t.rows += 8;

    // G. A manually scheduled parent is skipped (:185). PM has two children (so it does not
    // switch) and is manual again; CH3 (true) moves under it: PM is neither switched nor changed.
    let pm = create_plain_by_a("ignore PM", None).await;
    create_plain_by_a("ignore PMX", Some(pm)).await;
    set_manual(pm, true).await;
    let ch3 = create_plain_by_a("ignore CH3", None).await;
    set_ignore(ch3).await;
    create_plain_by_a("ignore PMY", Some(pm)).await;
    let frozen_pm = freeze(pm).await;
    edit(
        ch3,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(pm)),
    )
    .await
    .expect("C moves CH3 under PM");
    assert_eq!(
        sched(&verify, pm).await,
        (true, false, 1),
        "manual: ignore_non_working_days skipped"
    );
    assert_eq!(freeze(pm).await, frozen_pm, "PM is not saved");
    t.rows += 2;

    // H. The former parent is recomputed: F (automatic) holds A_ (true) and B_ (false), so true.
    // A_ moves away to N: F's remaining child B_ is false, so F is reset to false (the former
    // chain is in the loader, :139-146). N switches and takes true. F's last journal is C's with
    // A_'s cause (the move into F) and the move out has the same cause: aggregated, so version 2
    // is rewritten to the reset snapshot.
    let f = create_plain_by_a("former F", None).await;
    let a_ = create_plain_by_a("former A_", None).await;
    set_ignore(a_).await;
    edit(
        a_,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(f)),
    )
    .await
    .expect("C moves A_ under F");
    create_plain_by_a("former B_", Some(f)).await;
    assert_eq!(
        sched(&verify, f).await,
        (false, true, 1),
        "F: true from A_; B_'s create changes nothing (not saved)"
    );
    let n = create_plain_by_a("former N", None).await;
    edit(
        a_,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(n)),
    )
    .await
    .expect("C moves A_ under N");
    assert_eq!(
        sched(&verify, f).await,
        (false, false, 2),
        "F lost its true child: reset to false (B_ is false)"
    );
    let j = journals(&verify, f).await;
    assert_eq!(
        (j.len(), j[1].user_id, &j[1].cause),
        (2, USER_C, &cause_of(a_))
    );
    assert_eq!(
        data_sched(&verify, j[1].data_id).await,
        (Some(false), false)
    );
    assert_eq!(
        sched(&verify, n).await,
        (false, true, 1),
        "N switched and takes A_'s true"
    );
    t.rows += 6;

    // I. ...but with NO child left Ruby keeps the value, contrary to its own comment
    // (:179-182 says "reset to false"; the code at :190-192 assigns the ancestor's own value when
    // `ignore_non_working_days_of_descendants` is nil): F2 stays true and is not saved.
    let f2 = create_plain_by_a("former F2", None).await;
    let a2 = create_plain_by_a("former A2", None).await;
    set_ignore(a2).await;
    edit(
        a2,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(f2)),
    )
    .await
    .expect("C moves A2 under F2");
    assert_eq!(sched(&verify, f2).await, (false, true, 1));
    let frozen_f2 = freeze(f2).await;
    let n2 = create_plain_by_a("former N2", None).await;
    edit(
        a2,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(n2)),
    )
    .await
    .expect("C moves A2 under N2");
    // The value is kept by the totals (nothing is saved there), but the rescheduling of the former
    // parent (update_service.rb:149-161) finds an automatic work package with no descendant and no
    // predecessor and turns it manual (set_schedule_service.rb:108-116): `schedule_manually` true,
    // one save (lock 1 -> 2), `ignore_non_working_days` still true.
    assert_eq!(
        sched(&verify, f2).await,
        (true, true, 2),
        "no child left: the flag is kept, as the code (not the comment) says; the rescheduling \
         turns the childless automatic parent manual"
    );
    assert_ne!(freeze(f2).await, frozen_f2, "the rescheduling wrote F2");
    assert_eq!(sched(&verify, n2).await, (false, true, 1));
    t.rows += 4;
    t.steps += 1;

    // ── 13. reschedule_related ───────────────────────────────────────
    // A function of its own: the journey's future is already large and a debug build overflows
    // the test thread's stack with more locals in it.
    Box::pin(reschedule_step(&pool, &verify, &mut t)).await;

    verify.close().await;
    db.close().await;
    t
}

/// Step 13 of the journey (see the call).
async fn reschedule_step(pool: &PgPool, verify: &PgPool, t: &mut Tally) {
    let (pool, verify) = (pool.clone(), verify.clone());
    let store = PgMutationStore { pool: pool.clone() };
    let caller_a = Caller::Actor {
        login: LOGIN_A.into(),
    };
    let caller_c = Caller::Actor {
        login: LOGIN_C.into(),
    };
    let edit =
        |id: i64, who: Caller, f: Box<dyn FnOnce(&mut op_db::UpdateWorkPackageDto) + Send>| {
            let store = pg_store(&pool);
            async move {
                let cur = store.find_wp(id).await.unwrap().unwrap();
                let mut dto = patch_subject(&cur, &cur.subject);
                f(&mut dto);
                store.update_unit_of_work(&who, id, dto).await
            }
        };

    // ── 13. reschedule_related ───────────────────────────────────────
    // `WorkPackages::SetScheduleService` (set_schedule_service.rb:30-203) after a create
    // (create_service.rb:90-110) and an update (update_service.rb:62-65, :149-170). Dates are
    // fixed: 2026-03-02 is a Monday, so 03-06 is a Friday, 03-07/08 the weekend, 03-09 a Monday.
    // The working week is Monday to Friday (`Setting.working_days` default, `Array(1..5)`). A
    // rescheduled work package is journaled with `Journal::CausedByWorkPackageRelatedChange`
    // (set_schedule_service.rb:202; the predecessor- and child-specific causes are commented out in
    // Ruby, :194-201), `{"type": "work_package_related_changed_times", "work_package_id":
    // <initiator>}`. Work packages are seeded by a create (so each has A's creation journal) and
    // then given dates, a duration and a scheduling mode by plain SQL (a Rust create or edit does
    // not write `duration`; nothing here depends on it for the work package being moved).
    let day = |s: &str| -> chrono::NaiveDate { s.parse().expect("a date") };
    let related = |of: i64| serde_json::json!({"type": "work_package_related_changed_times", "work_package_id": of});
    #[allow(clippy::type_complexity)]
    let seed_dated = |subject: &'static str,
                      start: &'static str,
                      due: &'static str,
                      duration: i32,
                      manual: bool| {
        let (store, caller, pool) = (pg_store(&pool), caller_a.clone(), pool.clone());
        async move {
            let id = create_plain(&store, &caller, subject, None)
                .await
                .expect(subject);
            sqlx::query(
                "UPDATE work_packages SET start_date = $2::date, due_date = $3::date, \
                     duration = $4, schedule_manually = $5 WHERE id = $1",
            )
            .bind(id)
            .bind(start)
            .bind(due)
            .bind(duration)
            .bind(manual)
            .execute(&pool)
            .await
            .expect("seed dates");
            id
        }
    };
    let follows_lag = |from: i64, to: i64, lag: i32| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO relations (from_id, to_id, relation_type, lag) \
                 VALUES ($1, $2, 'follows', $3)",
            )
            .bind(from)
            .bind(to)
            .bind(lag)
            .execute(&pool)
            .await
            .expect("a follows relation with a lag");
        }
    };
    let child_of = |child: i64, parent: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE work_packages SET parent_id = $2 WHERE id = $1")
                .bind(child)
                .bind(parent)
                .execute(&pool)
                .await
                .expect("set parent");
        }
    };
    let set_due = |id: i64, due: &'static str| {
        let store = pg_store(&pool);
        let caller = caller_c.clone();
        async move {
            let cur = store.find_wp(id).await.unwrap().unwrap();
            let mut dto = patch_subject(&cur, &cur.subject);
            dto.due_date = Some(due.parse().expect("a date"));
            store.update_unit_of_work(&caller, id, dto).await
        }
    };
    let freeze13 = |id: i64| {
        let verify = verify.clone();
        async move {
            (
                row_json(&verify, id).await.unwrap(),
                journal_state(&verify, id).await,
            )
        }
    };

    // A. A successor with a lag, across a weekend. A (manual): Mon 03-02 .. Wed 03-04. B follows
    // A with lag 2, is automatic, duration 2, and sits where it belongs: with_lag(Wed 4, 2) counts
    // the two lag days Thu 5, Fri 6 and starts the day after, Sat 7, the soonest working day is
    // Mon 9; two working days: Mon 9 .. Tue 10. BM follows A too but is scheduled manually.
    let a13 = seed_dated("resched A", "2026-03-02", "2026-03-04", 3, true).await;
    let b13 = seed_dated("resched B", "2026-03-09", "2026-03-10", 2, false).await;
    let bm13 = seed_dated("resched BM", "2026-03-02", "2026-03-03", 2, true).await;
    follows_lag(b13, a13, 2).await;
    follows_lag(bm13, a13, 0).await;
    assert_eq!(
        dates(&verify, b13).await,
        (
            Some(day("2026-03-09")),
            Some(day("2026-03-10")),
            Some(2),
            false,
            0
        )
    );
    let frozen_bm = freeze13(bm13).await;
    // C moves A's due date to Fri 03-06. with_lag(Fri 6, 2): Sat 7 and Sun 8 are not working, Mon 9
    // is lag day 1, Tue 10 lag day 2, the day after is Wed 11 (a working day). Two working days
    // from Wed 11: Wed 11 .. Thu 12. The duration is recomputed from the new dates: 2.
    set_due(a13, "2026-03-06")
        .await
        .expect("C moves A's due date");
    assert_eq!(
        dates(&verify, b13).await,
        (
            Some(day("2026-03-11")),
            Some(day("2026-03-12")),
            Some(2),
            false,
            1
        ),
        "B moved by the weekend and the lag: Wed 11 .. Thu 12, saved once"
    );
    let bj = journals(&verify, b13).await;
    assert_eq!(
        (bj.len(), bj[1].version, bj[1].user_id, &bj[1].cause),
        (2, 2, USER_C, &related(a13)),
        "B: A's creation journal, then C's related-change journal"
    );
    assert_eq!(
        data_dates(&verify, bj[1].data_id).await,
        (Some(day("2026-03-11")), Some(day("2026-03-12")), Some(2))
    );
    assert_eq!(
        data_dates(&verify, bj[0].data_id).await,
        (None, None, None),
        "v1 keeps the creation snapshot"
    );
    assert_eq!(
        dates(&verify, a13).await.4,
        1,
        "A itself is saved by its own edit only"
    );
    assert_eq!(
        freeze13(bm13).await,
        frozen_bm,
        "a manually scheduled successor stays put: row and journals"
    );
    t.rows += 8;
    // Moved earlier, the successor follows (reschedule_by_predecessors assigns the soonest start
    // whichever way the predecessor moved). Due back to Wed 03-04: B is Mon 9 .. Tue 10 again,
    // lock 2; C's second edit is the same user's, same cause, within the window: aggregated.
    set_due(a13, "2026-03-04")
        .await
        .expect("C moves A's due date back");
    assert_eq!(
        dates(&verify, b13).await,
        (
            Some(day("2026-03-09")),
            Some(day("2026-03-10")),
            Some(2),
            false,
            2
        )
    );
    let bj = journals(&verify, b13).await;
    assert_eq!((bj.len(), &bj[1].cause), (2, &related(a13)), "aggregated");
    assert_eq!(
        data_dates(&verify, bj[1].data_id).await,
        (Some(day("2026-03-09")), Some(day("2026-03-10")), Some(2))
    );
    t.rows += 3;

    // B. `ignore_non_working_days` on the successor: calendar days (AllDays). A2 due Wed 4, B2
    // follows with lag 0, ignores non-working days, duration 3: Thu 5 .. Sat 7. A2's due date to
    // Fri 6: with_lag(Fri 6, 0) is Sat 7, which AllDays keeps; three calendar days: Sat 7 ..
    // Mon 9 (working days would give Mon 9 .. Wed 11).
    let a2 = seed_dated("resched A2", "2026-03-02", "2026-03-04", 3, true).await;
    let b2 = seed_dated("resched B2", "2026-03-05", "2026-03-07", 3, false).await;
    sqlx::query("UPDATE work_packages SET ignore_non_working_days = true WHERE id = $1")
        .bind(b2)
        .execute(&pool)
        .await
        .expect("B2 ignores non-working days");
    follows_lag(b2, a2, 0).await;
    set_due(a2, "2026-03-06")
        .await
        .expect("C moves A2's due date");
    assert_eq!(
        dates(&verify, b2).await,
        (
            Some(day("2026-03-07")),
            Some(day("2026-03-09")),
            Some(3),
            false,
            1
        ),
        "calendar days: Sat 7 .. Mon 9"
    );
    t.rows += 1;

    // C. A non-working day in the window. Holiday Wed 2026-03-11. A3 (Mon 2 .. Wed 4), B3 follows
    // with lag 2, duration 2, at Mon 9 .. Tue 10. A3's due date to Fri 6 gives, as in A, a start of
    // Wed 11; that is no working day now, so the soonest working day is Thu 12; two working days:
    // Thu 12, Fri 13. One day later than case A.
    let a3 = seed_dated("resched A3", "2026-03-02", "2026-03-04", 3, true).await;
    let b3 = seed_dated("resched B3", "2026-03-09", "2026-03-10", 2, false).await;
    follows_lag(b3, a3, 2).await;
    sqlx::query("INSERT INTO non_working_days (name, date) VALUES ('Holiday', '2026-03-11')")
        .execute(&pool)
        .await
        .expect("a non-working day");
    set_due(a3, "2026-03-06")
        .await
        .expect("C moves A3's due date");
    assert_eq!(
        dates(&verify, b3).await,
        (
            Some(day("2026-03-12")),
            Some(day("2026-03-13")),
            Some(2),
            false,
            1
        ),
        "Wed 11 is a holiday: Thu 12 .. Fri 13"
    );
    sqlx::query("DELETE FROM non_working_days")
        .execute(&pool)
        .await
        .expect("remove the holiday");
    t.rows += 1;

    // D. A parent takes its children's dates. P (automatic): children C1 Mon 16 .. Tue 17, C2 Wed
    // 18 .. Fri 20, P Mon 16 .. Fri 20 (5). C moves C2's due date to Tue 24: P starts at the
    // earliest date of the children (Mon 16) and ends at the latest (Tue 24); its duration is the
    // working days Mon 16 .. Fri 20 (5) and Mon 23, Tue 24 (2): 7. P is journaled with C2's cause.
    let p13 = seed_dated("resched P", "2026-03-16", "2026-03-20", 5, false).await;
    let c1_13 = seed_dated("resched C1", "2026-03-16", "2026-03-17", 2, true).await;
    let c2_13 = seed_dated("resched C2", "2026-03-18", "2026-03-20", 3, true).await;
    child_of(c1_13, p13).await;
    child_of(c2_13, p13).await;
    set_due(c2_13, "2026-03-24")
        .await
        .expect("C moves C2's due date");
    assert_eq!(
        dates(&verify, p13).await,
        (
            Some(day("2026-03-16")),
            Some(day("2026-03-24")),
            Some(7),
            false,
            1
        ),
        "P spans its children: Mon 16 .. Tue 24, 7 working days"
    );
    let pj = journals(&verify, p13).await;
    assert_eq!(
        (pj.len(), pj[1].user_id, &pj[1].cause),
        (2, USER_C, &related(c2_13))
    );
    assert_eq!(
        data_dates(&verify, pj[1].data_id).await,
        (Some(day("2026-03-16")), Some(day("2026-03-24")), Some(7))
    );
    // A manually scheduled parent does not: PM (manual) keeps Mon 16 .. Wed 18 when its child D2's
    // due date moves to Fri 20, and neither its row nor its journals change.
    let pm13 = seed_dated("resched PM", "2026-03-16", "2026-03-18", 3, true).await;
    let d1_13 = seed_dated("resched D1", "2026-03-16", "2026-03-16", 1, true).await;
    let d2_13 = seed_dated("resched D2", "2026-03-17", "2026-03-18", 2, true).await;
    child_of(d1_13, pm13).await;
    child_of(d2_13, pm13).await;
    let frozen_pm = freeze13(pm13).await;
    set_due(d2_13, "2026-03-20")
        .await
        .expect("C moves D2's due date");
    assert_eq!(
        freeze13(pm13).await,
        frozen_pm,
        "a manual parent is not rescheduled"
    );
    // Nor when it is the work package that is edited: PM's own due date moves to Tue 17 and stays
    // there (it is not recomputed from D1 Mon 16 and D2 Tue 17 .. Fri 20, which would give Fri 20).
    // The edit derives the duration from the new dates: Mon 16, Tue 17 is 2 (the seeded 3 is
    // replaced).
    set_due(pm13, "2026-03-17")
        .await
        .expect("C moves PM's due date");
    assert_eq!(
        dates(&verify, pm13).await,
        (
            Some(day("2026-03-16")),
            Some(day("2026-03-17")),
            Some(2),
            true,
            1
        ),
        "an edited manual parent keeps the dates it was given"
    );
    // The traversal stops at a manually scheduled work package: AM (manual) is edited, MM (manual)
    // follows AM and S (automatic) follows MM with dates that are deliberately stale (Mon 2 ..
    // Tue 3 where MM's Tue 10 would give Wed 11). S is only reachable through MM, so it is not
    // loaded and not rescheduled; MM stays put as well.
    let am13 = seed_dated("resched AM", "2026-03-02", "2026-03-04", 3, true).await;
    let mm13 = seed_dated("resched MM", "2026-03-09", "2026-03-10", 2, true).await;
    let s13 = seed_dated("resched S", "2026-03-02", "2026-03-03", 2, false).await;
    follows_lag(mm13, am13, 0).await;
    follows_lag(s13, mm13, 0).await;
    let (frozen_mm, frozen_s) = (freeze13(mm13).await, freeze13(s13).await);
    set_due(am13, "2026-03-06")
        .await
        .expect("C moves AM's due date");
    assert_eq!(
        (freeze13(mm13).await, freeze13(s13).await),
        (frozen_mm, frozen_s),
        "nothing past a manually scheduled work package is rescheduled"
    );
    t.rows += 8;

    // E. A create with dates under an automatic parent reschedules the parent. PA (automatic) has
    // E1 (Mon 2 .. Tue 3), PA Mon 2 .. Tue 3 (2). A creates E2 under PA, Wed 4 .. Fri 6: PA is
    // Mon 2 .. Fri 6, 5 working days. A created PA moments ago: the save is A's, with no cause
    // (a create has no initiator), so it aggregates into PA's creation journal: one journal.
    let pa13 = seed_dated("resched PA", "2026-03-02", "2026-03-03", 2, false).await;
    let e1_13 = seed_dated("resched E1", "2026-03-02", "2026-03-03", 2, true).await;
    child_of(e1_13, pa13).await;
    let mut params = WorkPackageParams::new()
        .with_subject("resched E2")
        .with_dates(day("2026-03-04"), day("2026-03-06"));
    params.project_id = Some(TARGET_PROJECT);
    params.parent_id = Some(pa13);
    mutation::create(&store, &caller_a, params)
        .await
        .expect("A creates E2 under PA");
    assert_eq!(
        dates(&verify, pa13).await,
        (
            Some(day("2026-03-02")),
            Some(day("2026-03-06")),
            Some(5),
            false,
            1
        ),
        "PA spans E1 and E2"
    );
    let paj = journals(&verify, pa13).await;
    assert_eq!((paj.len(), &paj[0].cause), (1, &serde_json::json!({})));
    assert_eq!(
        data_dates(&verify, paj[0].data_id).await,
        (Some(day("2026-03-02")), Some(day("2026-03-06")), Some(5))
    );
    t.rows += 3;

    // F. Moving a child between parents recomputes the former parent. OP (automatic) holds M1 Mon 2
    // .. Tue 3 and M2 Wed 4 .. Fri 6, OP Mon 2 .. Fri 6. NP (automatic) holds N1 Mon 9 .. Tue 10.
    // C moves M2 under NP: OP is Mon 2 .. Tue 3 (2 working days); NP spans N1 and M2: Wed 4 ..
    // Tue 10, Wed, Thu, Fri, Mon, Tue = 5. Both are journaled with M2's cause by C.
    let op13 = seed_dated("resched OP", "2026-03-02", "2026-03-06", 5, false).await;
    let m1_13 = seed_dated("resched M1", "2026-03-02", "2026-03-03", 2, true).await;
    let m2_13 = seed_dated("resched M2", "2026-03-04", "2026-03-06", 3, true).await;
    let np13 = seed_dated("resched NP", "2026-03-09", "2026-03-10", 2, false).await;
    let n1_13 = seed_dated("resched N1", "2026-03-09", "2026-03-10", 2, true).await;
    child_of(m1_13, op13).await;
    child_of(m2_13, op13).await;
    child_of(n1_13, np13).await;
    edit(
        m2_13,
        caller_c.clone(),
        Box::new(move |d| d.parent_id = Some(np13)),
    )
    .await
    .expect("C moves M2 under NP");
    assert_eq!(
        dates(&verify, op13).await,
        (
            Some(day("2026-03-02")),
            Some(day("2026-03-03")),
            Some(2),
            false,
            1
        ),
        "the former parent is recomputed"
    );
    assert_eq!(
        dates(&verify, np13).await,
        (
            Some(day("2026-03-04")),
            Some(day("2026-03-10")),
            Some(5),
            false,
            1
        ),
        "the new parent spans M2 and N1"
    );
    for id in [op13, np13] {
        let j = journals(&verify, id).await;
        assert_eq!(
            (j.len(), j[1].user_id, &j[1].cause),
            (2, USER_C, &related(m2_13)),
            "work package {id}"
        );
    }
    t.rows += 4;

    // I. The duration a Rust create and edit write (`set_calculated_attributes`,
    // set_attributes_service.rb:92-107, :158-185). S1 is created by A with Mon 9 .. Thu 12: four
    // working days, so the INSERT carries duration 4 (no SQL seed). It is then made automatic and
    // follows X1 (manual, Mon 2 .. Wed 4) with lag 2. X1's due date to Fri 6: start is Wed 11
    // (as in A); four working days from Wed 11 are Wed 11, Thu 12, Fri 13, Mon 16, so S1 is
    // Wed 11 .. Mon 16 and keeps its duration 4. Without the stored duration the due date would
    // stay Tue 10 and be clamped to the start: Wed 11 .. Wed 11.
    let x1_13 = seed_dated("resched X1", "2026-03-02", "2026-03-04", 3, true).await;
    let mut params = WorkPackageParams::new()
        .with_subject("resched S1")
        .with_dates(day("2026-03-09"), day("2026-03-12"));
    params.project_id = Some(TARGET_PROJECT);
    let s1_13 = mutation::create(&store, &caller_a, params)
        .await
        .expect("A creates S1")
        .id;
    assert_eq!(
        dates(&verify, s1_13).await,
        (
            Some(day("2026-03-09")),
            Some(day("2026-03-12")),
            Some(4),
            true,
            0
        ),
        "the create derived the duration: Mon 9 .. Thu 12 is 4 working days"
    );
    sqlx::query("UPDATE work_packages SET schedule_manually = false WHERE id = $1")
        .bind(s1_13)
        .execute(&pool)
        .await
        .expect("S1 is automatic");
    follows_lag(s1_13, x1_13, 2).await;
    set_due(x1_13, "2026-03-06")
        .await
        .expect("C moves X1's due date");
    assert_eq!(
        dates(&verify, s1_13).await,
        (
            Some(day("2026-03-11")),
            Some(day("2026-03-16")),
            Some(4),
            false,
            1
        ),
        "S1 keeps its four working days: Wed 11 .. Mon 16"
    );
    // An edit derives the duration from the dates: W1 due Mon 2 .. Tue 10 is Mon 2-6 and Mon 9,
    // Tue 10: 7. A due date on a Saturday (Sat 14) is shifted to the Monday (16): Mon 2-6, 9-13
    // and 16: 11. W2 ignores non-working days: Mon 2 .. Tue 10 is 9 calendar days.
    let w1 = seed_dated("resched W1", "2026-03-02", "2026-03-04", 3, true).await;
    let w2 = seed_dated("resched W2", "2026-03-02", "2026-03-04", 3, true).await;
    sqlx::query("UPDATE work_packages SET ignore_non_working_days = true WHERE id = $1")
        .bind(w2)
        .execute(&pool)
        .await
        .expect("W2 ignores non-working days");
    set_due(w1, "2026-03-10").await.expect("C edits W1");
    assert_eq!(
        dates(&verify, w1).await,
        (
            Some(day("2026-03-02")),
            Some(day("2026-03-10")),
            Some(7),
            true,
            1
        )
    );
    set_due(w1, "2026-03-14").await.expect("C edits W1 again");
    assert_eq!(
        dates(&verify, w1).await,
        (
            Some(day("2026-03-02")),
            Some(day("2026-03-16")),
            Some(11),
            true,
            2
        ),
        "a Saturday due date moves to Monday 16"
    );
    set_due(w2, "2026-03-10").await.expect("C edits W2");
    assert_eq!(
        dates(&verify, w2).await,
        (
            Some(day("2026-03-02")),
            Some(day("2026-03-10")),
            Some(9),
            true,
            1
        ),
        "calendar days: 9"
    );
    t.rows += 5;

    // G. Rollback. AR (manual) Mon 2 .. Wed 4, BR follows it (lag 0, duration 2) at Thu 5 .. Fri 6.
    // AR's due date to Fri 6 would move BR to Mon 9 .. Tue 10; a failing statement after the
    // rescheduling rolls AR, BR and their journals back byte-identical. The control with a
    // passing statement commits: BR is Mon 9 .. Tue 10 (with_lag(Fri 6, 0) = Sat 7 -> Mon 9).
    let ar13 = seed_dated("resched AR", "2026-03-02", "2026-03-04", 3, true).await;
    let br13 = seed_dated("resched BR", "2026-03-05", "2026-03-06", 2, false).await;
    follows_lag(br13, ar13, 0).await;
    let (frozen_ar, frozen_br) = (freeze13(ar13).await, freeze13(br13).await);
    let dto_for = |id: i64| {
        let store = pg_store(&pool);
        async move {
            let cur = store.find_wp(id).await.unwrap().unwrap();
            let mut dto = patch_subject(&cur, &cur.subject);
            dto.due_date = Some("2026-03-06".parse().expect("a date"));
            dto
        }
    };
    let failed = pg_store(&pool)
        .update_then(&caller_c, ar13, dto_for(ar13).await, Some("SELECT 1 / 0"))
        .await;
    assert!(matches!(failed, Err(MutationError::Store(_))), "{failed:?}");
    assert_eq!(
        (freeze13(ar13).await, freeze13(br13).await),
        (frozen_ar, frozen_br),
        "rolled back: rows and journals of AR and BR"
    );
    pg_store(&pool)
        .update_then(&caller_c, ar13, dto_for(ar13).await, Some("SELECT 1"))
        .await
        .expect("the control commits");
    assert_eq!(
        dates(&verify, br13).await,
        (
            Some(day("2026-03-09")),
            Some(day("2026-03-10")),
            Some(2),
            false,
            1
        ),
        "the control rescheduled BR"
    );
    assert_eq!(journals(&verify, br13).await.len(), 2);
    t.rows += 3;

    // H. A stale rescheduled row is a conflict. XR (manual) due moves to Fri 6 (committed, nothing
    // rescheduled yet), YR follows it at Thu 5 .. Fri 6 (lag 0, 2 days). Transaction 1 reschedules
    // YR to Mon 9 .. Tue 10 and holds YR's row lock; transaction 2 has loaded YR (stale) and waits
    // on the lock. Once 1 commits, 2's `UPDATE ... WHERE lock_version = <as loaded>` finds no row:
    // `StaleObjectError`, here a Conflict, and nothing of 2 is written.
    let xr13 = seed_dated("resched XR", "2026-03-02", "2026-03-04", 3, true).await;
    let yr13 = seed_dated("resched YR", "2026-03-05", "2026-03-06", 2, false).await;
    follows_lag(yr13, xr13, 0).await;
    sqlx::query("UPDATE work_packages SET due_date = '2026-03-06' WHERE id = $1")
        .bind(xr13)
        .execute(&pool)
        .await
        .expect("XR's due date");
    let one = PgCreateScope::begin(&pool).await.expect("1 begins");
    let saved = one
        .reschedule_related(&[xr13], &[], Some(xr13), USER_C)
        .await
        .expect("1 reschedules YR");
    assert_eq!(saved, vec![yr13]);
    let two = PgCreateScope::begin(&pool).await.expect("2 begins");
    let two_task = tokio::spawn(async move {
        let r = two
            .reschedule_related(&[xr13], &[], Some(xr13), USER_C)
            .await;
        two.rollback().await;
        r
    });
    let mut waiting = false;
    for _ in 0..200 {
        let n = count(
            &verify,
            "SELECT count(*) FROM pg_stat_activity \
             WHERE wait_event_type = 'Lock' AND datname = current_database()",
        )
        .await;
        if n > 0 {
            waiting = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(waiting, "2 waits on YR's row lock held by 1");
    one.commit().await.expect("1 commits");
    let r = two_task.await.expect("2's task");
    assert!(
        matches!(r, Err(StoreFailure::Conflict(_))),
        "2's stale save of YR is a conflict, not a lost update: {r:?}"
    );
    assert_eq!(
        dates(&verify, yr13).await,
        (
            Some(day("2026-03-09")),
            Some(day("2026-03-10")),
            Some(2),
            false,
            1
        ),
        "YR carries 1's reschedule, saved once"
    );
    assert_eq!(journals(&verify, yr13).await.len(), 2, "2 wrote no journal");
    t.rows += 3;
    t.steps += 1;
}

/// Create a work package of project 5 under `parent` with the given hours (and status) through the
/// boundary.
async fn create_hours<S: CreationStore>(
    store: &S,
    caller: &Caller,
    subject: &str,
    parent: Option<i64>,
    estimated: f64,
    remaining: f64,
    status: Option<i64>,
) -> Result<i64, MutationError> {
    let mut p = WorkPackageParams::new()
        .with_subject(subject)
        .with_estimated_hours(estimated)
        .with_remaining_hours(remaining);
    p.project_id = Some(TARGET_PROJECT);
    p.parent_id = parent;
    p.status_id = status;
    mutation::create(store, caller, p).await.map(|row| row.id)
}

/// Create a work package of project 5 under `parent` with no hours and the default status, so
/// the derived totals of its ancestors stay `nil` and only a scheduling change makes one save.
async fn create_plain<S: CreationStore>(
    store: &S,
    caller: &Caller,
    subject: &str,
    parent: Option<i64>,
) -> Result<i64, MutationError> {
    let mut p = WorkPackageParams::new().with_subject(subject);
    p.project_id = Some(TARGET_PROJECT);
    p.parent_id = parent;
    mutation::create(store, caller, p).await.map(|row| row.id)
}

/// `(schedule_manually, ignore_non_working_days, lock_version)` of a work package, as committed.
async fn sched(verify: &PgPool, id: i64) -> (bool, bool, i32) {
    sqlx::query_as(
        "SELECT schedule_manually, ignore_non_working_days, lock_version \
         FROM work_packages WHERE id = $1",
    )
    .bind(id)
    .fetch_one(verify)
    .await
    .expect("scheduling flags")
}

/// The two scheduling flags a journal data row snapshots.
async fn data_sched(verify: &PgPool, data_id: i64) -> (Option<bool>, bool) {
    sqlx::query_as(
        "SELECT schedule_manually, ignore_non_working_days FROM work_package_journals WHERE id = $1",
    )
    .bind(data_id)
    .fetch_one(verify)
    .await
    .expect("journal data scheduling flags")
}

/// The derived columns of a work package and its `lock_version`, as committed.
#[derive(Debug, PartialEq, sqlx::FromRow)]
struct Derived {
    est: Option<f64>,
    rem: Option<f64>,
    ratio: Option<i32>,
    lock_version: i32,
}

async fn derived(verify: &PgPool, id: i64) -> Derived {
    sqlx::query_as(
        "SELECT derived_estimated_hours AS est, derived_remaining_hours AS rem, \
                derived_done_ratio AS ratio, lock_version \
         FROM work_packages WHERE id = $1",
    )
    .bind(id)
    .fetch_one(verify)
    .await
    .expect("derived values")
}

/// The `estimated_hours` a journal data row snapshots.
async fn data_est(verify: &PgPool, data_id: i64) -> Option<f64> {
    sqlx::query_scalar("SELECT estimated_hours FROM work_package_journals WHERE id = $1")
        .bind(data_id)
        .fetch_one(verify)
        .await
        .expect("journal data estimate")
}

/// The derived columns a journal data row snapshots.
async fn data_derived(verify: &PgPool, data_id: i64) -> (Option<f64>, Option<f64>, Option<i32>) {
    sqlx::query_as(
        "SELECT derived_estimated_hours, derived_remaining_hours, derived_done_ratio \
         FROM work_package_journals WHERE id = $1",
    )
    .bind(data_id)
    .fetch_one(verify)
    .await
    .expect("journal data derived values")
}

/// `(start_date, due_date, duration, schedule_manually, lock_version)` of a work package, as
/// committed.
async fn dates(
    verify: &PgPool,
    id: i64,
) -> (
    Option<chrono::NaiveDate>,
    Option<chrono::NaiveDate>,
    Option<i32>,
    bool,
    i32,
) {
    sqlx::query_as(
        "SELECT start_date, due_date, duration, schedule_manually, lock_version \
         FROM work_packages WHERE id = $1",
    )
    .bind(id)
    .fetch_one(verify)
    .await
    .expect("dates")
}

/// The dates and duration a journal data row snapshots.
async fn data_dates(
    verify: &PgPool,
    data_id: i64,
) -> (
    Option<chrono::NaiveDate>,
    Option<chrono::NaiveDate>,
    Option<i32>,
) {
    sqlx::query_as("SELECT start_date, due_date, duration FROM work_package_journals WHERE id = $1")
        .bind(data_id)
        .fetch_one(verify)
        .await
        .expect("journal data dates")
}

/// The users watching work package `id`, by id.
async fn watcher_users(verify: &PgPool, id: i64) -> Vec<i64> {
    sqlx::query_scalar(
        "SELECT user_id FROM watchers WHERE watchable_type = 'WorkPackage' AND watchable_id = $1 \
         ORDER BY user_id",
    )
    .bind(id)
    .fetch_all(verify)
    .await
    .expect("watchers")
}

fn pg_store(pool: &PgPool) -> PgMutationStore {
    PgMutationStore { pool: pool.clone() }
}

/// [`PgMutationStore`] whose unit of work, when `fail` is set, runs a failing statement on the
/// create's own transaction right after the real insert.
struct FailAfterInsert {
    fail: bool,
    pool: PgPool,
}

impl MutationStore for FailAfterInsert {
    async fn find_wp(
        &self,
        id: i64,
    ) -> Result<Option<op_db::work_packages::WorkPackageRow>, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .find_wp(id)
        .await
    }
    async fn project_active(&self, project_id: i64) -> Result<bool, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .project_active(project_id)
        .await
    }
    async fn principal(&self, caller: &Caller, project_id: i64) -> Result<Principal, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .principal(caller, project_id)
        .await
    }
    async fn update_unit_of_work(
        &self,
        caller: &Caller,
        id: i64,
        dto: op_db::UpdateWorkPackageDto,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .update_unit_of_work(caller, id, dto)
        .await
    }
}

impl CreationStore for FailAfterInsert {
    async fn author_id(&self, caller: &Caller) -> Result<Option<i64>, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .author_id(caller)
        .await
    }
    async fn first_enabled_type(&self, project_id: i64) -> Result<Option<i64>, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .first_enabled_type(project_id)
        .await
    }
    async fn default_status(&self) -> Result<Option<i64>, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .default_status()
        .await
    }
    async fn default_priority(&self) -> Result<Option<i64>, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .default_priority()
        .await
    }
    async fn status_is_closed(&self, status_id: i64) -> Result<Option<bool>, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .status_is_closed(status_id)
        .await
    }
    async fn insert_wp(
        &self,
        dto: op_db::CreateWorkPackageDto,
    ) -> Result<op_db::work_packages::WorkPackageRow, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .insert_wp(dto)
        .await
    }
    async fn create_unit_of_work(
        &self,
        caller: &Caller,
        params: WorkPackageParams,
    ) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
        // The production transaction, with a failing statement after the insert when `fail`.
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .create_then(caller, params, self.fail.then_some("SELECT 1 / 0"))
        .await
    }
}

#[test]
fn split_url_keeps_credentials_host_and_query() {
    assert_eq!(
        split_url("postgres://u:p@h:5432/db?sslmode=disable"),
        ("postgres://u:p@h:5432/".into(), "?sslmode=disable".into())
    );
    assert_eq!(
        split_url("postgres://h/db"),
        ("postgres://h/".into(), String::new())
    );
    assert_eq!(
        split_url("postgres://h"),
        ("postgres://h/".into(), String::new())
    );
}
