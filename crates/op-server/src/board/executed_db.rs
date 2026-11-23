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
//!   then CSRF then write), both over `PgWpStore` through the `PageStore` adapter.
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
const LOGIN_A: &str = "oidc:member-a";
const LOGIN_B: &str = "oidc:member-b";
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
            "EXECUTED_DB: journey steps {}/7, rows asserted {}, transactional create: YES (step 7)",
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
             ({USER_B}, '{LOGIN_B}', 'Bo', 'Outsider', 'b@example.test', false, 1)"
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
             (50, {USER_A}, {TARGET_PROJECT}), (51, {USER_B}, {B_PROJECT})"
        ),
        format!(
            "INSERT INTO member_roles (member_id, role_id) VALUES (50, {ROLE_A}), (51, {ROLE_B})"
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
    let (project_id, type_id, status_id, author_id, subject, lock_version): (
        i64,
        i64,
        i64,
        i64,
        String,
        i32,
    ) = sqlx::query_as(
        "SELECT project_id, type_id, status_id, author_id, subject, lock_version \
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
    assert_eq!(subject, "Persisted journey");
    assert_eq!(lock_version, 0);
    assert_eq!(
        count(&verify, "SELECT count(*) FROM work_packages").await,
        1
    );
    t.rows += 2;
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
    // observe a failure after the insert (the place a journal write would go), the unit of work
    // runs a failing statement on the same transaction right after the real insert.
    // The insert must roll back with it: no row survives. (Two-sided: the control below inserts
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
    eprintln!(
        "EXECUTED_DB step 7: create is one transaction; a failure after the INSERT left \
         {survivors} rows (control committed {committed})"
    );
    t.rows += 2;
    t.steps += 1;

    verify.close().await;
    db.close().await;
    t
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
    async fn update_wp(
        &self,
        id: i64,
        dto: op_db::UpdateWorkPackageDto,
    ) -> Result<op_db::work_packages::WorkPackageRow, StoreFailure> {
        PgMutationStore {
            pool: self.pool.clone(),
        }
        .update_wp(id, dto)
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
