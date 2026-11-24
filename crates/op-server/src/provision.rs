// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! Provisioning of a signed-in person from the sign-in map.
//!
//! Writes need a user row `oidc:<subject>` and memberships (`write_guard::load_principal`);
//! nothing else creates them. When the map has `admin=` or `projects=` rows
//! ([`crate::signin_map`]), the first request of a signed-in person (and every request after
//! their role changed) makes the rows match the map:
//!
//! - the user row `login = oidc:<subject>` exists, `admin` as the map says. A new row is
//!   active; an existing row keeps its `status` (a locked person stays locked) and its names.
//! - in the managed projects (`projects=`), a person who is not an admin has exactly one
//!   membership holding the OpenProject role of the map; an admin has none there. Memberships
//!   in other projects, and other users, are never touched.
//!
//! [`plan`] decides, from rows already read, and is tested without a database;
//! [`Provisioner::ensure`] reads, plans and applies on one transaction.
//!
//! Identity: the verified ID token as `web_login` exposes it carries a subject and a verified
//! email, no name. A new row is named by the subject (`firstname`), `lastname` is empty, and
//! `mail` is the verified email or empty (`users.mail` is `NOT NULL DEFAULT ''`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sqlx::{PgPool, Postgres, Transaction};
use web_login::SignedIn;

use crate::signin_map::{ProjectScope, SigninMap};

/// `users.status` of an active user.
const ACTIVE: i32 = 1;

/// After a failed attempt the next request of the same person tries again no sooner than this.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The user row of the person, when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingUser {
    pub admin: bool,
}

/// A membership of the person in a project (roles that are not inherited).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingMember {
    pub member_id: i64,
    pub project_id: i64,
    pub role_ids: Vec<i64>,
}

/// What the memberships in the managed projects should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Membership {
    /// No membership there (an admin, or no `projects=` row names a project for the person).
    None,
    /// Exactly one, holding this role.
    Role(i64),
    /// The role could not be resolved: memberships are left as they are.
    Unresolved,
}

/// The state the map asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub admin: bool,
    pub membership: Membership,
    /// The managed projects (ids), sorted.
    pub projects: BTreeSet<i64>,
}

/// One write, in the order they are applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    CreateUser { admin: bool },
    SetAdmin(bool),
    AddMembership { project_id: i64, role_id: i64 },
    ReplaceRoles { member_id: i64, role_id: i64 },
    RemoveMembership { member_id: i64 },
}

/// The writes that make `user` and `members` match `wanted`; none when they already do.
pub fn plan(
    user: Option<&ExistingUser>,
    members: &[ExistingMember],
    wanted: &Wanted,
) -> Vec<Action> {
    let mut out = Vec::new();
    match user {
        None => out.push(Action::CreateUser {
            admin: wanted.admin,
        }),
        Some(u) if u.admin != wanted.admin => out.push(Action::SetAdmin(wanted.admin)),
        Some(_) => {}
    }
    if wanted.membership == Membership::Unresolved {
        return out;
    }
    let mut seen = BTreeSet::new();
    for m in members
        .iter()
        .filter(|m| wanted.projects.contains(&m.project_id))
    {
        seen.insert(m.project_id);
        match wanted.membership {
            Membership::Role(role_id) if m.role_ids != [role_id] => {
                out.push(Action::ReplaceRoles {
                    member_id: m.member_id,
                    role_id,
                });
            }
            Membership::None => out.push(Action::RemoveMembership {
                member_id: m.member_id,
            }),
            _ => {}
        }
    }
    if let Membership::Role(role_id) = wanted.membership {
        for project_id in wanted.projects.difference(&seen) {
            out.push(Action::AddMembership {
                project_id: *project_id,
                role_id,
            });
        }
    }
    out
}

/// A refusal or a fault of one attempt; only the class is logged, never a claim.
#[derive(Debug)]
enum Failure {
    Database,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Done,
    RetryAfter(Instant),
}

/// Makes the rows of a signed-in person match the sign-in map. At most one attempt per person
/// and role per process while it succeeds; a role change is a new attempt.
pub struct Provisioner {
    pool: PgPool,
    map: Arc<SigninMap>,
    /// subject -> (role at the last attempt, outcome).
    seen: Mutex<HashMap<String, (String, State)>>,
    /// Names already logged as unknown, so each is logged once.
    logged: Mutex<HashSet<String>>,
}

impl Provisioner {
    /// `None` when the map has no `admin=` and no `projects=` row: nobody is provisioned.
    pub fn new(pool: PgPool, map: Arc<SigninMap>) -> Option<Provisioner> {
        map.provisions().then(|| Provisioner {
            pool,
            map,
            seen: Mutex::new(HashMap::new()),
            logged: Mutex::new(HashSet::new()),
        })
    }

    fn log_once(&self, what: String) {
        if self.logged.lock().expect("lock").insert(what.clone()) {
            tracing::warn!("sign-in provisioning: {what}");
        }
    }

    /// Provision `who`, who holds OpenProject role `role` (the caller has already checked that
    /// the map admits them). A database error leaves them signed in and unprovisioned.
    pub async fn ensure(&self, who: &SignedIn, role: &str) {
        {
            let seen = self.seen.lock().expect("lock");
            let settled = match seen.get(&who.subject) {
                Some((r, State::Done)) if r == role => true,
                Some((r, State::RetryAfter(at))) if r == role => Instant::now() < *at,
                _ => false,
            };
            if settled {
                return;
            }
        }
        let state = match self.apply(who, role).await {
            Ok(()) => State::Done,
            Err(Failure::Database) => {
                tracing::warn!("sign-in provisioning: database error, person left unprovisioned");
                State::RetryAfter(Instant::now() + RETRY_AFTER)
            }
        };
        self.seen
            .lock()
            .expect("lock")
            .insert(who.subject.clone(), (role.to_owned(), state));
    }

    async fn apply(&self, who: &SignedIn, role: &str) -> Result<(), Failure> {
        let Some(wish) = self.map.provisioning_for(role) else {
            return Ok(());
        };
        let login = format!("oidc:{}", who.subject);
        let db = |_: sqlx::Error| Failure::Database;
        let mut tx = self.pool.begin().await.map_err(db)?;
        // One writer per person at a time: two first requests must not both insert.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&login)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        let membership = match &wish.member_role {
            _ if wish.admin => Membership::None,
            None => Membership::None,
            Some(name) => match role_id(&mut tx, name).await.map_err(db)? {
                Some(id) => Membership::Role(id),
                None => {
                    self.log_once(format!(
                        "role `{name}` is not a givable role of the roles table; memberships left as they are"
                    ));
                    Membership::Unresolved
                }
            },
        };
        let projects = match &wish.scope {
            None => BTreeSet::new(),
            Some(scope) => {
                let (found, missing) = project_ids(&mut tx, scope).await.map_err(db)?;
                for identifier in missing {
                    self.log_once(format!(
                        "project `{identifier}` of `projects=` does not exist"
                    ));
                }
                found
            }
        };
        let wanted = Wanted {
            admin: wish.admin,
            membership,
            projects,
        };

        let user = sqlx::query_as::<_, (i64, bool)>("SELECT id, admin FROM users WHERE login = $1")
            .bind(&login)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let members = match user {
            Some((id, _)) => members_of(&mut tx, id).await.map_err(db)?,
            None => Vec::new(),
        };
        let existing = user.map(|(_, admin)| ExistingUser { admin });
        let actions = plan(existing.as_ref(), &members, &wanted);
        if actions.is_empty() {
            return tx.commit().await.map_err(db);
        }

        let mut user_id = user.map(|(id, _)| id);
        for action in &actions {
            match action {
                Action::CreateUser { admin } => {
                    let id = sqlx::query_scalar::<_, i64>(
                        "INSERT INTO users (login, firstname, lastname, mail, admin, status, \
                         created_at, updated_at) \
                         VALUES ($1, $2, '', $3, $4, $5, NOW(), NOW()) RETURNING id",
                    )
                    .bind(&login)
                    .bind(&who.subject)
                    .bind(who.email.as_deref().unwrap_or(""))
                    .bind(admin)
                    .bind(ACTIVE)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?;
                    user_id = Some(id);
                }
                Action::SetAdmin(admin) => {
                    sqlx::query("UPDATE users SET admin = $2, updated_at = NOW() WHERE id = $1")
                        .bind(user_id)
                        .bind(admin)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                }
                Action::AddMembership {
                    project_id,
                    role_id,
                } => {
                    let member_id = sqlx::query_scalar::<_, i64>(
                        "INSERT INTO members (user_id, project_id, created_at, updated_at) \
                         VALUES ($1, $2, NOW(), NOW()) RETURNING id",
                    )
                    .bind(user_id)
                    .bind(project_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?;
                    add_role(&mut tx, member_id, *role_id).await.map_err(db)?;
                }
                Action::ReplaceRoles { member_id, role_id } => {
                    sqlx::query(
                        "DELETE FROM member_roles WHERE member_id = $1 AND inherited_from IS NULL",
                    )
                    .bind(member_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                    add_role(&mut tx, *member_id, *role_id).await.map_err(db)?;
                }
                Action::RemoveMembership { member_id } => {
                    sqlx::query("DELETE FROM member_roles WHERE member_id = $1")
                        .bind(member_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    sqlx::query("DELETE FROM members WHERE id = $1")
                        .bind(member_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                }
            }
        }
        tx.commit().await.map_err(db)
    }
}

async fn add_role(
    tx: &mut Transaction<'_, Postgres>,
    member_id: i64,
    role_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO member_roles (member_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(member_id)
    .bind(role_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A role a member can hold (not built in), by name as `RoleRepository::find_by_name` matches.
async fn role_id(
    tx: &mut Transaction<'_, Postgres>,
    name: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM roles WHERE LOWER(name) = LOWER($1) AND builtin = 0 ORDER BY id LIMIT 1",
    )
    .bind(name)
    .fetch_optional(&mut **tx)
    .await
}

/// The ids of the projects of `scope`, and the listed identifiers no project has.
async fn project_ids(
    tx: &mut Transaction<'_, Postgres>,
    scope: &ProjectScope,
) -> Result<(BTreeSet<i64>, Vec<String>), sqlx::Error> {
    match scope {
        ProjectScope::All => {
            let ids = sqlx::query_scalar::<_, i64>("SELECT id FROM projects")
                .fetch_all(&mut **tx)
                .await?;
            Ok((ids.into_iter().collect(), Vec::new()))
        }
        ProjectScope::Listed(identifiers) => {
            let rows = sqlx::query_as::<_, (i64, String)>(
                "SELECT id, identifier FROM projects WHERE identifier = ANY($1)",
            )
            .bind(identifiers)
            .fetch_all(&mut **tx)
            .await?;
            let found: BTreeSet<&str> = rows.iter().map(|(_, i)| i.as_str()).collect();
            let missing = identifiers
                .iter()
                .filter(|i| !found.contains(i.as_str()))
                .cloned()
                .collect();
            Ok((rows.iter().map(|(id, _)| *id).collect(), missing))
        }
    }
}

/// The project memberships of a user with the roles they hold themselves.
async fn members_of(
    tx: &mut Transaction<'_, Postgres>,
    user_id: i64,
) -> Result<Vec<ExistingMember>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (i64, i64, Option<i64>)>(
        "SELECT m.id, m.project_id, mr.role_id \
         FROM members m \
         LEFT JOIN member_roles mr ON mr.member_id = m.id AND mr.inherited_from IS NULL \
         WHERE m.user_id = $1 AND m.project_id IS NOT NULL AND m.entity_type IS NULL \
         ORDER BY m.id, mr.role_id",
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut by_member: BTreeMap<i64, ExistingMember> = BTreeMap::new();
    for (member_id, project_id, role_id) in rows {
        let entry = by_member.entry(member_id).or_insert(ExistingMember {
            member_id,
            project_id,
            role_ids: Vec::new(),
        });
        entry.role_ids.extend(role_id);
    }
    Ok(by_member.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMBER: i64 = 7;
    const OTHER_ROLE: i64 = 9;

    fn wanted(admin: bool, membership: Membership, projects: &[i64]) -> Wanted {
        Wanted {
            admin,
            membership,
            projects: projects.iter().copied().collect(),
        }
    }

    fn member(member_id: i64, project_id: i64, role_ids: &[i64]) -> ExistingMember {
        ExistingMember {
            member_id,
            project_id,
            role_ids: role_ids.to_vec(),
        }
    }

    /// FAILS IF: the first sign-in of a person the map admits does not create the user row
    /// with the admin flag of the map and one membership per managed project, or adds a
    /// membership in a project the map does not manage.
    #[test]
    fn a_first_sign_in_creates_the_user_and_the_memberships() {
        let actions = plan(None, &[], &wanted(false, Membership::Role(MEMBER), &[1, 2]));
        assert_eq!(
            actions,
            vec![
                Action::CreateUser { admin: false },
                Action::AddMembership {
                    project_id: 1,
                    role_id: MEMBER
                },
                Action::AddMembership {
                    project_id: 2,
                    role_id: MEMBER
                },
            ]
        );
    }

    /// FAILS IF: an admin is given a membership, or the admin flag is not set on creation.
    #[test]
    fn an_admin_is_created_as_admin_with_no_membership() {
        let actions = plan(None, &[], &wanted(true, Membership::None, &[1, 2]));
        assert_eq!(actions, vec![Action::CreateUser { admin: true }]);
    }

    /// FAILS IF: a person who became admin keeps the memberships the map gave them as a
    /// member, or a person who stopped being admin keeps `admin = true`.
    #[test]
    fn becoming_or_ceasing_to_be_admin_changes_the_flag_and_the_managed_memberships() {
        let user = ExistingUser { admin: false };
        let held = [member(100, 1, &[MEMBER]), member(101, 3, &[MEMBER])];
        let actions = plan(Some(&user), &held, &wanted(true, Membership::None, &[1, 2]));
        assert_eq!(
            actions,
            vec![
                Action::SetAdmin(true),
                Action::RemoveMembership { member_id: 100 }
            ],
            "project 3 is not managed and stays"
        );
        let user = ExistingUser { admin: true };
        let actions = plan(
            Some(&user),
            &[],
            &wanted(false, Membership::Role(MEMBER), &[1]),
        );
        assert_eq!(
            actions,
            vec![
                Action::SetAdmin(false),
                Action::AddMembership {
                    project_id: 1,
                    role_id: MEMBER
                }
            ]
        );
    }

    /// FAILS IF: a role change in the identity provider does not replace the role of a
    /// managed membership, or touches a membership in a project the map does not list.
    #[test]
    fn a_role_change_replaces_the_roles_in_the_managed_projects_only() {
        let user = ExistingUser { admin: false };
        let held = [
            member(100, 1, &[OTHER_ROLE]),
            member(101, 2, &[MEMBER, OTHER_ROLE]),
            member(102, 3, &[OTHER_ROLE]),
        ];
        let actions = plan(
            Some(&user),
            &held,
            &wanted(false, Membership::Role(MEMBER), &[1, 2]),
        );
        assert_eq!(
            actions,
            vec![
                Action::ReplaceRoles {
                    member_id: 100,
                    role_id: MEMBER
                },
                Action::ReplaceRoles {
                    member_id: 101,
                    role_id: MEMBER
                },
            ]
        );
    }

    /// FAILS IF: a second sign-in of an unchanged person writes anything.
    #[test]
    fn a_repeated_sign_in_is_a_no_op() {
        let user = ExistingUser { admin: false };
        let held = [member(100, 1, &[MEMBER]), member(102, 3, &[OTHER_ROLE])];
        assert!(plan(
            Some(&user),
            &held,
            &wanted(false, Membership::Role(MEMBER), &[1])
        )
        .is_empty());
        let admin = ExistingUser { admin: true };
        assert!(plan(Some(&admin), &[], &wanted(true, Membership::None, &[1])).is_empty());
    }

    /// FAILS IF: a role the roles table does not give makes the plan remove or replace a
    /// membership (an unknown role name must be a refusal, never a grant or a revocation).
    #[test]
    fn an_unresolved_role_leaves_memberships_alone() {
        let user = ExistingUser { admin: false };
        let held = [member(100, 1, &[OTHER_ROLE])];
        assert!(plan(
            Some(&user),
            &held,
            &wanted(false, Membership::Unresolved, &[1, 2])
        )
        .is_empty());
        assert_eq!(
            plan(None, &[], &wanted(false, Membership::Unresolved, &[1])),
            vec![Action::CreateUser { admin: false }],
            "the person is still created, with no membership"
        );
    }

    /// FAILS IF: a map without `admin=` and `projects=` provisions anyone, or a person the map
    /// refuses (no role) has a provisioning wish.
    #[test]
    fn a_map_without_the_rows_or_a_refused_person_provisions_nothing() {
        let plain = SigninMap::parse("role=a:Member\nrank=Member\n").unwrap();
        assert_eq!(plain.provisioning_for("Member"), None);
        let map = SigninMap::parse("role=a:Member\nrank=Member\nprojects=*\n").unwrap();
        // A person with no mapped role key has no role, so `ensure` is never reached.
        assert_eq!(map.op_role(["unmapped"]), None);
        assert!(map.provisioning_for("Member").is_some());
    }
}

/// Against a real PostgreSQL: `OP_TEST_DATABASE_URL` names a migrated database with the kanban
/// seed (roles `Manager`, `Member`; projects `website-relaunch`, `mobile-app`). Without it the
/// test prints a skip line and passes, unless `REQUIRE_DB=1`.
#[cfg(test)]
mod db_tests {
    use super::*;

    fn signed_in(subject: &str) -> SignedIn {
        SignedIn {
            issuer: "https://idp.example.test".into(),
            subject: subject.into(),
            org: None,
            roles: Vec::new(),
            email: Some(format!("{subject}@example.test")),
            expires_at: u64::MAX,
        }
    }

    async fn memberships(pool: &PgPool, login: &str) -> Vec<(String, String)> {
        sqlx::query_as::<_, (String, String)>(
            "SELECT p.identifier, r.name FROM members m \
             JOIN users u ON u.id = m.user_id JOIN projects p ON p.id = m.project_id \
             JOIN member_roles mr ON mr.member_id = m.id JOIN roles r ON r.id = mr.role_id \
             WHERE u.login = $1 ORDER BY 1, 2",
        )
        .bind(login)
        .fetch_all(pool)
        .await
        .expect("memberships")
    }

    async fn admin_flag(pool: &PgPool, login: &str) -> Option<bool> {
        sqlx::query_scalar::<_, bool>("SELECT admin FROM users WHERE login = $1")
            .bind(login)
            .fetch_optional(pool)
            .await
            .expect("user")
    }

    /// FAILS IF: a first sign-in does not create the user and exactly the listed memberships,
    /// a role change does not replace the role in the listed projects, an unlisted project's
    /// membership is touched, or the admin role does not set the admin flag.
    #[tokio::test]
    async fn provisioning_writes_the_rows_the_map_implies() {
        let Ok(url) = std::env::var("OP_TEST_DATABASE_URL") else {
            assert!(
                std::env::var("REQUIRE_DB").as_deref() != Ok("1"),
                "REQUIRE_DB=1 but OP_TEST_DATABASE_URL is unset"
            );
            eprintln!("skip: OP_TEST_DATABASE_URL unset");
            return;
        };
        let pool = PgPool::connect(&url).await.expect("connect");
        let map = SigninMap::parse(
            "role=op-admin:Manager\nrole=op-member:Member\nrank=Manager,Member\n\
             admin=Manager\nprojects=website-relaunch,mobile-app\n",
        )
        .expect("map");
        map.check().expect("map checks");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let sub = format!("sub-{stamp}");
        let login = format!("oidc:{sub}");

        // The membership in a project the map does not list must survive every change.
        let p = Provisioner::new(pool.clone(), Arc::new(map)).expect("provisions");
        p.ensure(&signed_in(&sub), "Member").await;
        assert_eq!(admin_flag(&pool, &login).await, Some(false));
        assert_eq!(
            memberships(&pool, &login).await,
            vec![
                ("mobile-app".to_owned(), "Member".to_owned()),
                ("website-relaunch".to_owned(), "Member".to_owned())
            ]
        );
        sqlx::query(
            "INSERT INTO members (user_id, project_id, created_at) \
             SELECT u.id, p.id, now() FROM users u, projects p \
             WHERE u.login = $1 AND p.identifier = 'internal-tools'",
        )
        .bind(&login)
        .execute(&pool)
        .await
        .expect("unlisted member");
        sqlx::query(
            "INSERT INTO member_roles (member_id, role_id) SELECT m.id, r.id FROM members m \
             JOIN users u ON u.id = m.user_id JOIN projects p ON p.id = m.project_id, roles r \
             WHERE u.login = $1 AND p.identifier = 'internal-tools' AND r.name = 'Member'",
        )
        .bind(&login)
        .execute(&pool)
        .await
        .expect("unlisted role");

        p.ensure(&signed_in(&sub), "Member").await;
        assert_eq!(
            memberships(&pool, &login).await.len(),
            3,
            "a repeat is a no-op"
        );

        p.ensure(&signed_in(&sub), "Manager").await;
        assert_eq!(admin_flag(&pool, &login).await, Some(true));
        assert_eq!(
            memberships(&pool, &login).await,
            vec![("internal-tools".to_owned(), "Member".to_owned())],
            "an admin holds no listed memberships; the unlisted one is untouched"
        );
    }
}
