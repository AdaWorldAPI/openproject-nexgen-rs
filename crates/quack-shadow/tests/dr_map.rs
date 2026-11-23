// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The id map of a table is DR's `MapTable`: answering reads none of it, a
//! feed reads it once per written row where a table is laid out by key, and
//! the arrival-order fallback pays for every tile. The `DRMAP` lines are the
//! measurements.

mod common;

use quack_shadow::{Catalog, Layout, rows, rows_verdict, shadow_verdict};
use serde_json::{Value, json};

fn read(file: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("oracle file")
}

fn table<'a>(catalog: &'a Catalog, name: &str) -> &'a quack_shadow::Table {
    catalog
        .tables
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no table {name}"))
}

fn live(table: &quack_shadow::Table) -> usize {
    table.live.iter().map(|w| w.count_ones() as usize).sum()
}

#[test]
fn answering_reads_no_map() {
    let data: Value = serde_json::from_str(&read("shadow-joins.pg.json")).expect("capture json");
    let entry = &data["sizes"]["seed"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    let calls = entry["calls"].as_array().expect("calls");
    let before = catalog.map_lookups();
    for call in calls {
        for fused in [false, true] {
            rows_verdict(&catalog, call, fused);
        }
    }
    let after = catalog.map_lookups();
    println!(
        "DRMAP joins capture: {} calls x 2 (plain, fused), map lookups {before} -> {after}",
        calls.len()
    );
    assert_eq!(calls.len(), 12);
    assert_eq!(before, after, "answering a join reads no map");

    let items: Vec<Value> = read("shadow-scenario-wide.pg.ndjson")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("scenario line"))
        .collect();
    let mut catalog = Catalog::new();
    let (mut joins, mut feeds, mut written, mut feed_lookups) = (0, 0, 0usize, 0);
    for item in &items {
        if let Some(feed) = item.get("feed") {
            let before = catalog.map_lookups();
            catalog.apply_feed(feed).expect("feed");
            feed_lookups += catalog.map_lookups() - before;
            feeds += 1;
            for entry in feed["tables"].as_object().expect("tables").values() {
                for key in ["upsert", "delete", "replace"] {
                    written += entry[key].as_array().map_or(0, Vec::len);
                }
            }
        } else if item.get("terminal").is_some() && item["joins"].as_u64() > Some(0) {
            let before = catalog.map_lookups();
            shadow_verdict(&catalog, item, false);
            shadow_verdict(&catalog, item, true);
            assert_eq!(before, catalog.map_lookups(), "{}", item["sql"]);
            joins += 1;
        }
    }
    println!(
        "DRMAP wide scenario: {joins} join calls read no map; {feeds} feeds wrote {written} rows and spent {feed_lookups} lookups"
    );
    assert_eq!(joins, 57);
}

fn big_feed(ids: impl Iterator<Item = u32>) -> Value {
    let columns = json!([{"name": "id", "type": "integer", "null": false}]);
    let upsert: Vec<Value> = ids.map(|id| json!([id])).collect();
    json!({"tables": {"big": {"columns": columns, "upsert": upsert, "delete": []}}})
}

fn spent(catalog: &mut Catalog, feed: &Value) -> u64 {
    let before = catalog.map_lookups();
    catalog.apply_feed(feed).expect("feed");
    catalog.map_lookups() - before
}

fn touch(upsert: &[u32], delete: &[u32]) -> Value {
    let upsert: Vec<Value> = upsert.iter().map(|id| json!([id])).collect();
    json!({"tables": {"big": {"upsert": upsert, "delete": delete}}})
}

#[test]
fn key_as_position_costs_one_lookup_per_written_row() {
    let mut catalog = Catalog::new();
    let load = spent(&mut catalog, &big_feed(1..=5000));
    let big = table(&catalog, "big");
    assert_eq!(big.layout, Layout::Identity);
    let tiles = catalog.map_tiles("big");
    let update = spent(&mut catalog, &touch(&[4321], &[]));
    let delete = spent(&mut catalog, &touch(&[], &[17]));
    let rewrite = spent(&mut catalog, &touch(&(101..=200).collect::<Vec<_>>(), &[]));
    let together = {
        let mut both = Catalog::new();
        spent(&mut both, &big_feed(1..=5000));
        spent(&mut both, &touch(&[4321], &[17]))
    };
    println!(
        "DRMAP identity: tiles {tiles}, lookups by the 5000-row load {load}, by the update of id 4321 {update}, by the delete of id 17 {delete}, by rewriting 100 existing rows {rewrite}, by update and delete in one feed {together}"
    );
    assert_eq!((tiles, load, update, delete), (20, 0, 1, 1));
    assert_eq!((rewrite, together), (100, 2));
    assert_eq!(live(table(&catalog, "big")), 4999);
}

#[test]
fn the_arrival_order_fallback_scans_every_tile() {
    let mut catalog = Catalog::new_append();
    let load = spent(&mut catalog, &big_feed(1..=5000));
    assert_eq!(table(&catalog, "big").layout, Layout::Append);
    let tiles = catalog.map_tiles("big");
    let update = spent(&mut catalog, &touch(&[4321], &[]));
    let delete = spent(&mut catalog, &touch(&[], &[17]));
    let rewrite = spent(&mut catalog, &touch(&(101..=200).collect::<Vec<_>>(), &[]));
    println!(
        "DRMAP append: tiles {tiles}, lookups by the 5000-row load {load}, by the update of id 4321 {update}, by the delete of id 17 {delete}, by rewriting 100 existing rows {rewrite}"
    );
    assert_eq!((tiles, load, update, delete), (20, 0, 20, 20));
    assert_eq!(rewrite, 100 * 20);
    assert_eq!(live(table(&catalog, "big")), 4999);
    assert_eq!(table(&catalog, "big").rows, 5000, "positions are permanent");
}

fn child_feed() -> Value {
    let child_columns = json!([
        {"name": "id", "type": "integer", "null": false},
        {"name": "big_id", "type": "integer", "null": false},
    ]);
    let big_columns = json!([
        {"name": "id", "type": "integer", "null": false},
        {"name": "grp", "type": "integer", "null": false},
    ]);
    let big: Vec<Value> = (1..=5000).map(|id| json!([id, id % 10])).collect();
    let child: Vec<Value> = (1..=5000)
        .map(|id| json!([id, (id * 7) % 5000 + 1]))
        .collect();
    json!({"tables": {
        "big": {"columns": big_columns, "upsert": big, "delete": []},
        "child": {"columns": child_columns, "upsert": child, "delete": []},
    }})
}

fn child_call() -> Value {
    json!({
        "label": "Child.joins(:big).where(bigs: { grp: 3 }).pluck(:id)",
        "table": "child", "terminal": "rows", "select": ["id"],
        "where": {"table": "big", "col": "grp", "op": "eq", "value": 3},
        "order": [], "limit": null, "offset": null, "joins": 1,
        "join": [{
            "type": "inner", "table": "big",
            "left": {"table": "child", "col": "big_id"},
            "right": {"table": "big", "col": "id"},
        }],
        "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

fn multiset(rows: &[Vec<Value>]) -> Vec<String> {
    let mut keys: Vec<String> = rows.iter().map(|r| r[0].to_string()).collect();
    keys.sort_unstable();
    keys
}

#[test]
fn a_join_through_the_fallback_is_refused_and_the_oracle_shows_its_cost() {
    let feed = child_feed();
    let call = child_call();
    let mut by_key = Catalog::new();
    let mut by_arrival = Catalog::new_append();
    for catalog in [&mut by_key, &mut by_arrival] {
        common::declare_edge(catalog, "child", "big_id", "big");
    }
    by_key.apply_feed(&feed).expect("feed");
    by_arrival.apply_feed(&feed).expect("feed");
    assert_eq!(table(&by_key, "big").layout, Layout::Identity);
    assert_eq!(table(&by_arrival, "big").layout, Layout::Append);

    let before = by_key.map_lookups();
    let key = rows(&by_key, &call, false).expect("identity rows");
    let lookups_key = by_key.map_lookups() - before;
    println!(
        "DRMAP join identity: {} rows, map lookups {lookups_key}, {:?}",
        key.rows.len(),
        by_key.link_stats()
    );
    let expected = (1..=5000u32)
        .filter(|id| ((id * 7) % 5000 + 1) % 10 == 3)
        .count();
    assert_eq!(key.rows.len(), expected);
    assert_eq!(lookups_key, 0, "the key lane is the position");
    assert_eq!(by_key.link_stats().derived, 0);

    let before = by_arrival.map_lookups();
    let answered = rows(&by_arrival, &call, false).expect("key set rows");
    let stats = by_arrival.link_stats();
    let lookups_set = by_arrival.map_lookups() - before;
    let tiles = u64::try_from(by_arrival.map_tiles("big")).expect("tiles");
    println!(
        "DRMAP join append, switch off: {} rows through {} key set of {} keys, map lookups {lookups_set}, {stats:?}",
        answered.rows.len(),
        stats.key_sets,
        stats.keys_in_sets,
    );
    assert_eq!(multiset(&key.rows), multiset(&answered.rows));
    assert_eq!((stats.derived, stats.rows_translated), (0, 0));
    assert_eq!((stats.key_sets, stats.keys_in_sets), (1, 500));
    assert_eq!(stats.placed_late, answered.kept as u64);
    assert_eq!((lookups_set, tiles), (10_000, 20));
    let fused = rows(&by_arrival, &call, true).expect("fused rows");
    assert_eq!(multiset(&key.rows), multiset(&fused.rows));

    let before = by_arrival.map_lookups();
    by_arrival.use_translated_positions(true);
    let arrival = rows(&by_arrival, &call, false).expect("oracle rows");
    let stats = by_arrival.link_stats();
    let lookups = by_arrival.map_lookups() - before;
    let tiles = u64::try_from(by_arrival.map_tiles("big")).expect("tiles");
    println!(
        "DRMAP join append, oracle switch on: {} rows, map lookups {lookups}, {stats:?}",
        arrival.rows.len()
    );
    assert_eq!(multiset(&key.rows), multiset(&arrival.rows));
    assert_eq!(stats.derived, 1);
    assert_eq!(stats.rows_translated, 5000);
    assert_eq!(lookups, 5000 * tiles);
}
