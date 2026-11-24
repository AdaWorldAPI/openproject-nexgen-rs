// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The sign-in map: which OpenProject role a person holds, given the role keys of their
//! verified ID token. One `key=value` directive per line, `#` starts a comment, an unknown
//! key and a repeated row are errors. Every name lives in the rows; none is known to this
//! crate.
//!
//! - `role=<provider role key>:<OpenProject role>`: holding that role key, in any
//!   organization of the token, gives that OpenProject role (for example
//!   `role=op-admin:Project admin`). Several rows may name one OpenProject role.
//! - `rank=<role>,<role>,…`: the earliest listed of a person's OpenProject roles wins.
//!   Every role a `role=` row names must be listed.
//! - `role.default=<role>` (optional): the role of a person none of whose role keys is
//!   mapped. Without it such a person has no access.
//! - `admin=<OpenProject role>` (optional): a person whose resolved role is this one is
//!   provisioned with `users.admin = true` (and no memberships). The role must be named by a
//!   `role=` row or by `role.default`.
//! - `projects=<identifier>,<identifier>,…` or `projects=*` (optional): the projects in which
//!   a person who is not an admin is provisioned with a membership holding their resolved
//!   OpenProject role (looked up by name in the `roles` table). `*` is every project and
//!   cannot be mixed with identifiers.
//!
//! Without `admin=` and `projects=` nobody is provisioned: a signed-in person is a user
//! row only if someone created it.

use std::collections::{BTreeMap, BTreeSet};

/// A row of the map that cannot be read, or a map that does not hold together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapError(pub String);

impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sign-in map: {}", self.0)
    }
}

impl std::error::Error for MapError {}

fn err<T>(line: usize, message: impl Into<String>) -> Result<T, MapError> {
    Err(MapError(format!("line {line}: {}", message.into())))
}

fn names(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The projects a `projects=` row names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectScope {
    /// `projects=*`: every project.
    All,
    /// Project identifiers, in the order of the row.
    Listed(Vec<String>),
}

/// What the map asks to provision for a person holding one OpenProject role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provisioning {
    /// The person is an OpenProject admin (and gets no membership).
    pub admin: bool,
    /// The role of the memberships in `scope`: set for a non-admin when `projects=` is set.
    pub member_role: Option<String>,
    /// The projects the map manages for this person, `None` without a `projects=` row.
    pub scope: Option<ProjectScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigninMap {
    roles: BTreeMap<String, String>,
    rank: Vec<String>,
    default: Option<String>,
    admin: Option<String>,
    projects: Option<ProjectScope>,
}

impl SigninMap {
    pub fn parse(text: &str) -> Result<SigninMap, MapError> {
        let mut roles = BTreeMap::new();
        let mut rank: Option<Vec<String>> = None;
        let mut default: Option<String> = None;
        let mut admin: Option<String> = None;
        let mut projects: Option<ProjectScope> = None;
        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let row = raw.split('#').next().unwrap_or("").trim();
            if row.is_empty() {
                continue;
            }
            let Some((key, value)) = row.split_once('=') else {
                return err(line, "a row without `=`");
            };
            let (key, value) = (key.trim(), value.trim());
            if value.is_empty() {
                return err(line, format!("`{key}` has no value"));
            }
            match key {
                "role" => {
                    let pair = value
                        .rsplit_once(':')
                        .map(|(a, b)| (a.trim(), b.trim()))
                        .filter(|(a, b)| !a.is_empty() && !b.is_empty());
                    let Some((role_key, op_role)) = pair else {
                        return err(line, "`role` needs `<role key>:<OpenProject role>`");
                    };
                    if roles
                        .insert(role_key.to_owned(), op_role.to_owned())
                        .is_some()
                    {
                        return err(line, "a role key is mapped twice");
                    }
                }
                "rank" => {
                    let list = names(value);
                    if list.iter().collect::<BTreeSet<_>>().len() != list.len() {
                        return err(line, "`rank` names a role twice");
                    }
                    if rank.replace(list).is_some() {
                        return err(line, "`rank` is set twice");
                    }
                }
                "role.default" => {
                    if default.replace(value.to_owned()).is_some() {
                        return err(line, "`role.default` is set twice");
                    }
                }
                "admin" => {
                    if admin.replace(value.to_owned()).is_some() {
                        return err(line, "`admin` is set twice");
                    }
                }
                "projects" => {
                    let list = names(value);
                    if list.is_empty() {
                        return err(line, "`projects` names no project");
                    }
                    if list.iter().collect::<BTreeSet<_>>().len() != list.len() {
                        return err(line, "`projects` names a project twice");
                    }
                    let scope = if list.iter().any(|p| p == "*") {
                        if list.len() != 1 {
                            return err(line, "`projects=*` cannot be mixed with identifiers");
                        }
                        ProjectScope::All
                    } else {
                        ProjectScope::Listed(list)
                    };
                    if projects.replace(scope).is_some() {
                        return err(line, "`projects` is set twice");
                    }
                }
                other => return err(line, format!("unknown key `{other}`")),
            }
        }
        Ok(SigninMap {
            roles,
            rank: rank.unwrap_or_default(),
            default,
            admin,
            projects,
        })
    }

    /// Whether the map holds together: every role a `role=` row names is ranked, and the
    /// `admin=` role is one the map can give. (Zammad's
    /// map is also checked against `desk.conf`; OpenProject has no such list here, the
    /// names live in the rows only.)
    pub fn check(&self) -> Result<(), MapError> {
        for role in self.roles.values() {
            if !self.rank.contains(role) {
                return Err(MapError(format!(
                    "maps role `{role}` but `rank` leaves it out"
                )));
            }
        }
        let given = |admin: &String| {
            self.roles.values().any(|r| r == admin) || self.default.as_ref() == Some(admin)
        };
        if let Some(admin) = self.admin.as_ref().filter(|a| !given(a)) {
            return Err(MapError(format!(
                "`admin` names role `{admin}` but no `role=` row or `role.default` gives it"
            )));
        }
        Ok(())
    }

    /// Whether the map has an `admin=` or a `projects=` row.
    pub fn provisions(&self) -> bool {
        self.admin.is_some() || self.projects.is_some()
    }

    /// What to provision for a person who holds OpenProject role `role`; `None` when the map
    /// has neither `admin=` nor `projects=` (nobody is provisioned).
    pub fn provisioning_for(&self, role: &str) -> Option<Provisioning> {
        if !self.provisions() {
            return None;
        }
        let admin = self.admin.as_deref() == Some(role);
        Some(Provisioning {
            admin,
            member_role: (!admin && self.projects.is_some()).then(|| role.to_owned()),
            scope: self.projects.clone(),
        })
    }

    /// The OpenProject role of a person with these provider role keys: the best ranked
    /// mapped one, else the default, else none.
    pub fn op_role<'a>(&self, role_keys: impl IntoIterator<Item = &'a str>) -> Option<&str> {
        let held: BTreeSet<&str> = role_keys
            .into_iter()
            .filter_map(|k| self.roles.get(k).map(String::as_str))
            .collect();
        self.rank
            .iter()
            .find(|r| held.contains(r.as_str()))
            .or(self.default.as_ref())
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_read_and_a_repeat_or_an_unknown_key_is_refused() {
        let map = SigninMap::parse("role=a:x\nrole=b:y # c\nrank=y,x\nrole.default=x\n").unwrap();
        assert_eq!(map.op_role(["a"]), Some("x"));
        assert_eq!(map.op_role(["a", "b"]), Some("y"));
        assert_eq!(map.op_role(["zz"]), Some("x"));
        for bad in [
            "role=a:x\nrole=a:y",
            "rank=x\nrank=y",
            "role.default=x\nrole.default=y",
            "rank=x,x",
            "nothing",
            "role=a",
            "role=:x",
            "rank=",
            "other=1",
            "customers.all=x",
        ] {
            assert!(SigninMap::parse(bad).is_err(), "{bad:?}");
        }
    }

    /// FAILS IF: an `admin=` or `projects=` row is not read, a non-admin does not get the
    /// member role and the scope, or an admin gets a member role.
    #[test]
    fn admin_and_projects_rows_are_read() {
        let map = SigninMap::parse(
            "role=a:Boss\nrole=b:Member\nrank=Boss,Member\nadmin=Boss\nprojects=alpha, beta\n",
        )
        .unwrap();
        map.check().unwrap();
        assert_eq!(
            map.provisioning_for("Boss"),
            Some(Provisioning {
                admin: true,
                member_role: None,
                scope: Some(ProjectScope::Listed(vec!["alpha".into(), "beta".into()])),
            })
        );
        assert_eq!(
            map.provisioning_for("Member")
                .unwrap()
                .member_role
                .as_deref(),
            Some("Member")
        );
        let all = SigninMap::parse("role=b:Member\nrank=Member\nprojects=*\n").unwrap();
        let p = all.provisioning_for("Member").unwrap();
        assert!(!p.admin);
        assert_eq!(p.scope, Some(ProjectScope::All));
    }

    /// FAILS IF: a map with neither row yields a provisioning wish for anyone.
    #[test]
    fn without_admin_and_projects_rows_nobody_is_provisioned() {
        let map = SigninMap::parse("role=a:x\nrank=x\n").unwrap();
        assert_eq!(map.provisioning_for("x"), None);
    }

    /// FAILS IF: a repeated, empty, duplicated or `*`-mixed row parses, or `check` accepts an
    /// `admin=` role no `role=` row or `role.default` gives.
    #[test]
    fn a_bad_admin_or_projects_row_is_refused() {
        for bad in [
            "admin=x\nadmin=y",
            "projects=a\nprojects=b",
            "projects=a,a",
            "projects=*,a",
            "projects=",
            "projects=,",
            "admin=",
        ] {
            assert!(SigninMap::parse(bad).is_err(), "{bad:?}");
        }
        // Parses, but `check` refuses an admin role no row gives.
        let unknown = SigninMap::parse("role=a:x\nrank=x\nadmin=ghost\n").unwrap();
        assert!(unknown.check().is_err());
        let by_default = SigninMap::parse("role=a:x\nrank=x\nrole.default=y\nadmin=y\n").unwrap();
        assert!(by_default.check().is_ok());
    }

    #[test]
    fn a_mapped_role_that_rank_leaves_out_is_refused_by_check() {
        assert!(
            SigninMap::parse("role=a:Project admin\nrank=Project admin\n")
                .unwrap()
                .check()
                .is_ok()
        );
        assert!(SigninMap::parse("role=a:x\nrank=y\n")
            .unwrap()
            .check()
            .is_err());
        assert!(SigninMap::parse("role=a:x\n").unwrap().check().is_err());
    }

    #[test]
    fn without_a_default_an_unmapped_person_has_no_role() {
        let map = SigninMap::parse("role=a:x\nrank=x\n").unwrap();
        assert_eq!(map.op_role(["zz"]), None);
        assert_eq!(map.op_role([]), None);
    }
}
