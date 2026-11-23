// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The captured `User.find(k).permissions` join, stated as views of
//! lance-graph-xlog's resident cube over the two join tables, against the
//! rows PostgreSQL returned. The cube joins binary relations; the statement
//! also filters on a property of the joined rows (`roles.active`,
//! `permissions.active`), which a cube side cannot express, so the second
//! test restricts the relations before they enter the cube and the first
//! shows what the cube answers without that.

use std::collections::BTreeSet;

use lance_graph_dt64::dr::{LocalMap, MapTable, Width};
use lance_graph_xlog::cube::Cube;
use quack_shadow::Catalog;
use serde_json::Value;

const TILE: u32 = 1;
const USERS: u32 = 6;

fn capture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-joins.pg.json"
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

fn column(catalog: &Catalog, table: &str, name: &str) -> Vec<Value> {
    let rows = catalog
        .tables
        .iter()
        .find(|t| t.name == table)
        .unwrap_or_else(|| panic!("table {table}"))
        .rows;
    (0..rows)
        .map(|row| catalog.cell(table, name, row))
        .collect()
}

fn id(cell: &Value) -> u32 {
    u32::try_from(cell.as_i64().expect("integer id")).expect("id fits u32")
}

fn pairs(catalog: &Catalog, table: &str, a: &str, b: &str) -> Vec<(u32, u32)> {
    column(catalog, table, a)
        .iter()
        .zip(&column(catalog, table, b))
        .map(|(x, y)| (id(x), id(y)))
        .collect()
}

fn active(catalog: &Catalog, table: &str) -> BTreeSet<u32> {
    column(catalog, table, "id")
        .iter()
        .zip(&column(catalog, table, "active"))
        .filter(|(_, on)| on.as_bool() == Some(true))
        .map(|(key, _)| id(key))
        .collect()
}

fn map_over(relations: &[&[(u32, u32)]], users: u32) -> MapTable {
    let mut all: Vec<u64> = relations
        .iter()
        .flat_map(|r| r.iter().flat_map(|&(a, b)| [u64::from(a), u64::from(b)]))
        .chain((1..=users).map(u64::from))
        .collect();
    all.sort_unstable();
    all.dedup();
    let mut table = MapTable::new(1, 1);
    table.advance(
        1,
        1,
        vec![(
            TILE,
            Some(LocalMap::prepare(Width::W256, &all).expect("ids fit one tile")),
        )],
    );
    table
}

/// `roles_users(user, role)` bound to one user, joined to
/// `permissions_roles(role, permission)` on the role.
fn sizes(roles_users: &[(u32, u32)], permissions_roles: &[(u32, u32)]) -> (Vec<i128>, u64, u64) {
    let relations = [roles_users, permissions_roles];
    let table = map_over(&relations, USERS);
    let mut cube = Cube::new(&table, &table.seal(), TILE, &relations).expect("cube");
    let mut joins = Vec::new();
    for user in 1..=USERS {
        let scan = cube.scan(0).expect("scan");
        let bound = cube.bind(&table, scan, 0, user).expect("bind");
        let other = cube.scan(1).expect("scan");
        joins.push(cube.join(bound, 1, other, 0).expect("join"));
    }
    let composed = cube.stats().programs;
    let out = joins
        .iter()
        .map(|&j| cube.join_size(&table, j).expect("join size"))
        .collect();
    (out, composed, cube.stats().rows_materialized)
}

fn postgres_rows(entry: &Value) -> Vec<i128> {
    (1..=USERS)
        .map(|user| {
            let label = format!("User.find({user}).permissions.pluck(:name)");
            let call = entry["calls"]
                .as_array()
                .expect("calls")
                .iter()
                .find(|c| c["label"] == label.as_str())
                .unwrap_or_else(|| panic!("call {label}"));
            call["answer"]["rows"].as_array().expect("rows").len() as i128
        })
        .collect()
}

// Pinned from a measured run; see the CUBE lines.
const UNRESTRICTED: [i128; 6] = [84, 66, 64, 66, 64, 66];

#[test]
fn a_cube_side_cannot_filter_on_a_property_of_the_joined_row() {
    let capture = capture();
    let entry = &capture["sizes"]["seed"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    let roles_users = pairs(&catalog, "roles_users", "user_id", "role_id");
    let permissions_roles = pairs(&catalog, "permissions_roles", "role_id", "permission_id");
    let (got, composed, materialized) = sizes(&roles_users, &permissions_roles);
    let want = postgres_rows(entry);
    println!("CUBE unrestricted {got:?} postgresql {want:?}");
    assert_eq!(composed, 0, "composing views ran a program");
    assert_eq!(materialized, 0);
    assert_eq!(got, UNRESTRICTED);
    assert_ne!(got, want, "the property filters removed nothing");
}

#[test]
fn the_cube_counts_the_join_once_the_relations_are_restricted() {
    let capture = capture();
    let entry = &capture["sizes"]["seed"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    let (roles, permissions) = (active(&catalog, "roles"), active(&catalog, "permissions"));
    let roles_users: Vec<(u32, u32)> = pairs(&catalog, "roles_users", "user_id", "role_id")
        .into_iter()
        .filter(|(_, role)| roles.contains(role))
        .collect();
    let permissions_roles: Vec<(u32, u32)> =
        pairs(&catalog, "permissions_roles", "role_id", "permission_id")
            .into_iter()
            .filter(|(role, permission)| roles.contains(role) && permissions.contains(permission))
            .collect();
    let (got, composed, materialized) = sizes(&roles_users, &permissions_roles);
    let want = postgres_rows(entry);
    println!("CUBE restricted {got:?} postgresql {want:?}");
    assert_eq!((composed, materialized), (0, 0));
    assert_eq!(got, want);
    assert_eq!(want, [68, 28, 52, 28, 52, 28]);
}
