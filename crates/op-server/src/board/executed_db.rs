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
    self, Caller, CreationStore, MutationError, MutationStore, PgMutationStore, Principal,
    StoreFailure,
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
const TARGET_PROJECT: i64 = 5;
const FOREIGN_PROJECT: i64 = 1;
const B_PROJECT: i64 = 7;
const ROLE_A: i64 = 20;
const ROLE_B: i64 = 21;
/// Enabled in project 5 at position 2.
const TYPE_TASK: i64 = 30;
/// Enabled in project 5 at position 1: project 5's first enabled type.
const TYPE_BUG: i64 = 31;
/// Enabled in project 1 only, at position 0.
const TYPE_FEATURE: i64 = 32;
const STATUS_NEW: i64 = 40;
/// The one status with `is_default`.
const STATUS_DEFAULT: i64 = 41;
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

    // Run the journey as its own task so a failed assertion still reaches the cleanup below.
    let outcome = tokio::spawn(journey(db_url)).await;

    let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name} WITH (FORCE)"))
        .execute(&admin)
        .await;
    admin.close().await;

    match outcome {
        Ok(tally) => eprintln!(
            "EXECUTED_DB: journey steps {}/9, rows asserted {}, transactional create: YES (step 7), creation journal: YES (steps 2, 7), update journal: YES (steps 4, 8, 9: no-op, aggregated, new version, closed predecessor, rollback)",
            tally.steps, tally.rows
        ),
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        Err(e) => panic!("journey task failed: {e}"),
    }
}

/// The fixture: two users, three projects, roles, types and statuses, inserted with plain SQL.
async fn seed(pool: &PgPool) {
    let statements = [
        format!(
            "INSERT INTO users (id, login, firstname, lastname, mail, admin, status) VALUES \
             ({USER_A}, '{LOGIN_A}', 'Ada', 'Member', 'a@example.test', false, 1), \
             ({USER_B}, '{LOGIN_B}', 'Bo', 'Outsider', 'b@example.test', false, 1), \
             ({USER_C}, '{LOGIN_C}', 'Cy', 'Colleague', 'c@example.test', false, 1)"
        ),
        format!(
            "INSERT INTO projects (id, name, identifier, active) VALUES \
             ({FOREIGN_PROJECT}, 'Foreign', 'foreign', true), \
             ({TARGET_PROJECT}, 'Target', 'target', true), \
             ({B_PROJECT}, 'B home', 'b-home', true)"
        ),
        format!(
            "INSERT INTO roles (id, name, position) VALUES \
             ({ROLE_A}, 'Member', 1), ({ROLE_B}, 'Editor elsewhere', 2)"
        ),
        format!(
            "INSERT INTO role_permissions (role_id, permission) VALUES \
             ({ROLE_A}, 'view_work_packages'), ({ROLE_A}, 'add_work_packages'), \
             ({ROLE_A}, 'edit_work_packages'), \
             ({ROLE_B}, 'view_work_packages'), ({ROLE_B}, 'add_work_packages'), \
             ({ROLE_B}, 'edit_work_packages')"
        ),
        // A is a member of project 5 only; B of project 7 only. Nobody is a member of project 1.
        format!(
            "INSERT INTO members (id, user_id, project_id) VALUES \
             (50, {USER_A}, {TARGET_PROJECT}), (51, {USER_B}, {B_PROJECT}), \
             (52, {USER_C}, {TARGET_PROJECT})"
        ),
        format!(
            "INSERT INTO member_roles (member_id, role_id) VALUES \
             (50, {ROLE_A}), (51, {ROLE_B}), (52, {ROLE_A})"
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
    t.rows += 3;
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
    eprintln!(
        "EXECUTED_DB step 7: create is one transaction; a failure after the INSERT left \
         {survivors} rows (control committed {committed})"
    );
    t.rows += 4;
    t.steps += 1;

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
    t.steps += 1;

    verify.close().await;
    db.close().await;
    t
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
