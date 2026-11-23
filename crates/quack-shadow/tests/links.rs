// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The translation oracle: an edge read as row positions is derived from the
//! key lane and the target's DR map on every request and kept nowhere, so a
//! feed has nothing to maintain, and a join answer is a list of virtual tuples
//! before any cell is read. The catalogs run with `use_translated_positions(true)`;
//! the default path that needs no translation is in `position_map.rs`.

mod common;

use quack_shadow::{Catalog, RowsAnswer, rows};
use serde_json::{Value, json};

const CALL: &str = "User.find(1).permissions.pluck(:name)";

fn capture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-joins.pg.json"
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

fn find<'a>(entry: &'a Value, wanted: &str) -> &'a Value {
    entry["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .find(|c| c["label"].as_str() == Some(wanted))
        .unwrap_or_else(|| panic!("no call {wanted}"))
}

fn multiset(answer: &RowsAnswer) -> Vec<String> {
    let mut rows: Vec<String> = answer
        .rows
        .iter()
        .map(|r| serde_json::to_string(r).expect("row json"))
        .collect();
    rows.sort_unstable();
    rows
}

/// The live rows of `roles_users`, one `[user_id, role_id]` per position.
fn live_pairs(catalog: &Catalog) -> Vec<Value> {
    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == "roles_users")
        .expect("roles_users");
    (0..table.rows)
        .filter(|p| (table.live[p / 64] >> (p % 64)) & 1 == 1)
        .map(|p| {
            Value::Array(
                table
                    .columns
                    .iter()
                    .map(|c| catalog.cell("roles_users", &c.name, p))
                    .collect(),
            )
        })
        .collect()
}

fn replace_feed(pairs: &[Value]) -> Value {
    json!({"tables": {"roles_users": {"replace": pairs}}})
}

fn live_bits(catalog: &Catalog) -> (usize, u32) {
    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == "roles_users")
        .expect("roles_users");
    (table.rows, table.live.iter().map(|w| w.count_ones()).sum())
}

#[test]
fn the_translation_is_derived_on_every_request() {
    let data = capture();
    let entry = &data["sizes"]["seed"];
    let mut catalog = common::captured(entry);
    catalog.use_translated_positions(true);
    let call = find(entry, CALL);

    rows(&catalog, call, false).expect("first run");
    let first = catalog.link_stats();
    println!("LINKS first run: {first:?}");
    assert_eq!(first.derived, 3, "derived");
    assert_eq!(first.rows_translated, 130 + 130 + 37, "rows translated");

    rows(&catalog, call, false).expect("second run");
    let second = catalog.link_stats();
    println!("LINKS second run: {second:?}");
    assert_eq!(second.derived, 2 * first.derived, "derived again");
    assert_eq!(
        second.rows_translated,
        2 * first.rows_translated,
        "translated again"
    );

    let default_path = common::captured(entry);
    rows(&default_path, call, false).expect("run without the switch");
    assert_eq!(
        default_path.link_stats().derived,
        0,
        "no translation without the switch"
    );
}

#[test]
fn a_feed_needs_no_link_maintenance() {
    let data = capture();
    let entry = &data["sizes"]["seed"];
    let mut catalog = common::captured(entry);
    catalog.use_translated_positions(true);
    let call = find(entry, CALL);
    let before = rows(&catalog, call, false).expect("first run");

    let mut pairs = live_pairs(&catalog);
    pairs.push(json!([1, 2]));
    let feed = replace_feed(&pairs);
    catalog.apply_feed(&feed).expect("feed");
    let after = rows(&catalog, call, false).expect("run after the feed");
    println!("LINKS after the feed: {:?}", catalog.link_stats());

    let mut fresh = common::captured(entry);
    fresh.apply_feed(&feed).expect("fresh feed");
    let expected = rows(&fresh, call, false).expect("fresh run");
    assert_eq!(multiset(&after), multiset(&expected));
    assert_eq!(after.kept, expected.kept);
    assert_ne!(
        multiset(&after),
        multiset(&before),
        "the pair changes the answer"
    );
}

#[test]
fn a_removed_pair_keeps_its_position_with_presence_zero() {
    let data = capture();
    let entry = &data["sizes"]["seed"];
    let mut catalog = common::captured(entry);
    catalog.use_translated_positions(true);
    let call = find(entry, CALL);
    let before = rows(&catalog, call, false).expect("first run");
    let (rows_before, live_before) = live_bits(&catalog);

    let mut pairs = live_pairs(&catalog);
    let gone = pairs
        .iter()
        .position(|p| *p == json!([1, 5]))
        .expect("pair (1, 5)");
    pairs.remove(gone);
    let feed = replace_feed(&pairs);
    catalog.apply_feed(&feed).expect("feed");
    let (rows_after, live_after) = live_bits(&catalog);
    assert_eq!(rows_after, rows_before, "positions are permanent");
    assert_eq!(live_after, live_before - 1, "one live bit fewer");

    let after = rows(&catalog, call, false).expect("run after the feed");
    let mut fresh = common::captured(entry);
    fresh.apply_feed(&feed).expect("fresh feed");
    let expected = rows(&fresh, call, false).expect("fresh run");
    assert_eq!(multiset(&after), multiset(&expected));
    assert_eq!(after.kept, expected.kept);
    assert_ne!(
        multiset(&after),
        multiset(&before),
        "the pair changes the answer"
    );
}

#[test]
fn a_join_answer_is_virtual_tuples() {
    let data = capture();
    let entry = &data["sizes"]["seed"];
    let catalog = common::captured(entry);
    let call = find(entry, CALL);

    let got = rows(&catalog, call, false).expect("rows");
    let join = got.join.expect("join stats");
    assert_eq!(
        join.tables,
        ["permissions", "permissions_roles", "roles", "roles_users"]
    );
    assert_eq!(join.sibling.as_deref(), Some("roles_users"));
    assert_eq!(
        join.tuples.iter().map(|t| t.times).sum::<usize>(),
        got.rows.len()
    );
    let sibling = join.tables.len() - 1;
    let mut expected = Vec::new();
    for tuple in &join.tuples {
        assert_eq!(tuple.positions.len(), join.tables.len());
        assert!(tuple.times > 0);
        let from = tuple.positions[0].expect("FROM position") as usize;
        assert_eq!(tuple.positions[sibling], None, "the sibling is counted");
        assert!(tuple.positions[1..sibling].iter().all(Option::is_some));
        for _ in 0..tuple.times {
            expected.push(vec![json!(catalog.text_at("permissions", "name", from))]);
        }
    }
    assert_eq!(got.rows, expected);
}
