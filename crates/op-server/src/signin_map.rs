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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigninMap {
    roles: BTreeMap<String, String>,
    rank: Vec<String>,
    default: Option<String>,
}

impl SigninMap {
    pub fn parse(text: &str) -> Result<SigninMap, MapError> {
        let mut roles = BTreeMap::new();
        let mut rank: Option<Vec<String>> = None;
        let mut default: Option<String> = None;
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
                other => return err(line, format!("unknown key `{other}`")),
            }
        }
        Ok(SigninMap {
            roles,
            rank: rank.unwrap_or_default(),
            default,
        })
    }

    /// Whether the map holds together: every role a `role=` row names is ranked. (Zammad's
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
        Ok(())
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
