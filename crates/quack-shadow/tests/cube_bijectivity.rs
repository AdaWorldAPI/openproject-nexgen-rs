// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The DR placement of every table is a bijection between its live keys and
//! its live positions, after every feed, in both layouts, across the tiles of a
//! table (the size-1025 and size-4097 captures span 5 and 17 tiles).
//!
//! A single tile is covered by lance-graph's own check of the cube; this one
//! reads zammad-rs's placement: `Catalog::position_of` and the key read back at
//! the position it names.

use std::collections::{BTreeMap, BTreeSet};

use quack_shadow::{Catalog, Layout};
use serde_json::{Value, json};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Tally {
    /// Feeds applied (a snapshot counts as the first).
    feeds: usize,
    /// `(table, feed)` pairs checked.
    table_feeds: usize,
    /// Live rows whose key resolved to their position and read back as the same key.
    live_rows: usize,
    /// Dead keys that resolved to no position.
    dead_keys: usize,
    /// Keys no row ever had that resolved to no position.
    absent_keys: usize,
    /// `(table, feed)` pairs of a table without an integer `id`, which has no key to place.
    keyless_table_feeds: usize,
    /// Per table: the fewest and the most tiles its DR map held across the feeds.
    tiles: BTreeMap<String, (usize, usize)>,
    identity_tables: BTreeSet<String>,
    append_tables: BTreeSet<String>,
}

fn id_at(catalog: &Catalog, table: &str, at: usize) -> Option<i64> {
    catalog.cell(table, "id", at).as_i64()
}

fn is_live(live: &[u64], at: usize) -> bool {
    live.get(at / 64).is_some_and(|w| (w >> (at % 64)) & 1 == 1)
}

fn check_table(catalog: &Catalog, name: &str, tally: &mut Tally, context: &str) {
    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == name)
        .expect("table");
    let keyed = (0..table.rows).any(|p| id_at(catalog, name, p).is_some());
    if !table.columns.iter().any(|c| c.name == "id") || (!keyed && table.rows > 0) {
        tally.keyless_table_feeds += 1;
        return;
    }
    tally.table_feeds += 1;
    match table.layout {
        Layout::Identity => tally.identity_tables.insert(name.to_owned()),
        Layout::Append => tally.append_tables.insert(name.to_owned()),
    };
    let tiles = catalog.map_tiles(name);
    let seen = tally.tiles.entry(name.to_owned()).or_insert((tiles, tiles));
    *seen = (seen.0.min(tiles), seen.1.max(tiles));

    let mut positions = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for p in (0..table.rows).filter(|&p| is_live(&table.live, p)) {
        let key = id_at(catalog, name, p)
            .unwrap_or_else(|| panic!("{context} {name}: live position {p} has no key"));
        let placed = catalog.position_of(name, key);
        assert_eq!(placed, Some(p), "{context} {name}: key {key}");
        let back = placed.and_then(|at| id_at(catalog, name, at));
        assert_eq!(back, Some(key), "{context} {name}: round trip of key {key}");
        assert!(
            positions.insert(p) && keys.insert(key),
            "{context} {name}: key {key} or position {p} is shared"
        );
        tally.live_rows += 1;
    }
    let live_rows = (0..table.rows).filter(|&p| is_live(&table.live, p)).count();
    assert_eq!(positions.len(), live_rows, "{context} {name}: injective");
    assert_eq!(keys.len(), live_rows, "{context} {name}: distinct keys");

    for p in (0..table.rows).filter(|&p| !is_live(&table.live, p)) {
        let Some(key) = id_at(catalog, name, p) else {
            continue;
        };
        if keys.contains(&key) {
            continue;
        }
        assert_eq!(
            catalog.position_of(name, key),
            None,
            "{context} {name}: dead key {key}"
        );
        tally.dead_keys += 1;
    }
    let beyond = i64::try_from(table.rows).expect("rows") + 5_000;
    let largest = keys.iter().next_back().copied().unwrap_or(0);
    for key in [-1, beyond, largest + 1_000_000, i64::MAX] {
        if !keys.contains(&key) {
            assert_eq!(
                catalog.position_of(name, key),
                None,
                "{context} {name}: absent key {key}"
            );
            tally.absent_keys += 1;
        }
    }
}

fn check_all(catalog: &Catalog, tally: &mut Tally, context: &str) {
    tally.feeds += 1;
    let names: Vec<String> = catalog.tables.iter().map(|t| t.name.clone()).collect();
    for name in names {
        check_table(catalog, &name, tally, context);
    }
}

fn read_json(file: &str) -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

/// The feeds of the wide workload, replayed into an empty catalog of either layout.
fn wide(append: bool) -> Tally {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-scenario-wide.pg.ndjson"
    ))
    .expect("scenario file");
    let mut catalog = if append {
        Catalog::new_append()
    } else {
        Catalog::new()
    };
    let mut tally = Tally::default();
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let item: Value = serde_json::from_str(line).expect("line");
        let Some(feed) = item.get("feed") else {
            continue;
        };
        catalog
            .apply_feed(feed)
            .unwrap_or_else(|e| panic!("line {}: {e}", n + 1));
        check_all(&catalog, &mut tally, &format!("wide line {}", n + 1));
    }
    tally
}

/// A new row of `table`: the cells of the live row at `from`, under the key `key`.
fn row_like(catalog: &Catalog, table: &str, from: usize, key: i64) -> Value {
    let columns: Vec<String> = catalog
        .tables
        .iter()
        .find(|t| t.name == table)
        .expect("table")
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    Value::Array(
        columns
            .iter()
            .map(|c| {
                if c == "id" {
                    json!(key)
                } else {
                    catalog.cell(table, c, from)
                }
            })
            .collect(),
    )
}

/// A feed that adds, removes, revives and rewrites rows across the tiles of every keyed table.
fn feeds(catalog: &Catalog) -> Vec<Value> {
    let mut one: BTreeMap<String, Value> = BTreeMap::new();
    let (mut two, mut three, mut four) = (one.clone(), one.clone(), one.clone());
    for table in &catalog.tables {
        let live: Vec<usize> = (0..table.rows)
            .filter(|&p| is_live(&table.live, p))
            .collect();
        let keys: Vec<i64> = live
            .iter()
            .filter_map(|&p| id_at(catalog, &table.name, p))
            .collect();
        let (Some(&first), Some(&last)) = (live.first(), live.last()) else {
            continue;
        };
        if keys.len() != live.len() {
            continue;
        }
        let max = keys.iter().copied().max().unwrap_or(0);
        let middle = keys[keys.len() / 2];
        let name = &table.name;
        let row = |key| row_like(catalog, name, last, key);
        one.insert(
            name.clone(),
            json!({"upsert": [row(max + 1), row(max + 2)], "delete": []}),
        );
        two.insert(
            name.clone(),
            json!({"upsert": [], "delete": [keys[0], middle, max, max + 1]}),
        );
        three.insert(
            name.clone(),
            json!({"upsert": [row_like(catalog, name, first, middle), row(keys[0])], "delete": []}),
        );
        four.insert(
            name.clone(),
            json!({"upsert": (max + 3..max + 303).map(row).collect::<Vec<_>>(), "delete": [max + 5]}),
        );
    }
    [one, two, three, four]
        .into_iter()
        .map(|tables| json!({"tables": tables}))
        .collect()
}

/// A capture of one size, under one layout, with `feeds` applied after it. The
/// size-4097 capture holds no `id` column, so there is no key to place in it.
fn captured(size: &str, file: &str, append: bool) -> Tally {
    let data = read_json(file);
    let entry = &data["sizes"][size];
    let mut catalog = if append {
        Catalog::from_capture_append(entry)
    } else {
        Catalog::from_capture(entry)
    }
    .expect("catalog");
    let mut tally = Tally::default();
    let context = format!("{size} {}", if append { "append" } else { "keyed" });
    check_all(&catalog, &mut tally, &format!("{context} snapshot"));
    for (i, feed) in feeds(&catalog).iter().enumerate() {
        catalog
            .apply_feed(feed)
            .unwrap_or_else(|e| panic!("{context} feed {i}: {e}"));
        check_all(&catalog, &mut tally, &format!("{context} feed {i}"));
    }
    tally
}

fn summary(tally: &Tally) -> Value {
    json!({
        "feeds": tally.feeds,
        "table_feeds": tally.table_feeds,
        "live_rows": tally.live_rows,
        "dead_keys": tally.dead_keys,
        "absent_keys": tally.absent_keys,
        "keyless_table_feeds": tally.keyless_table_feeds,
        "max_tiles": tally.tiles.values().map(|t| t.1).max(),
        "tables_over_one_tile": tally.tiles.iter().filter(|(_, t)| t.1 > 1).map(|(n, t)| (n.clone(), [t.0, t.1])).collect::<BTreeMap<_, _>>(),
        "keyed_layout_tables": tally.identity_tables.len(),
        "arrival_layout_tables": tally.append_tables.len(),
    })
}

/// Pinned from a measured run; see the BIJECTION lines.
const EXPECTED: &str = r#"{
  "rows_1025_arrival": {
    "absent_keys": 60,
    "arrival_layout_tables": 3,
    "dead_keys": 27,
    "feeds": 5,
    "keyed_layout_tables": 0,
    "keyless_table_feeds": 0,
    "live_rows": 16272,
    "max_tiles": 6,
    "table_feeds": 15,
    "tables_over_one_tile": {
      "import_jobs": [
        5,
        6
      ],
      "knowledge_base_categories": [
        5,
        6
      ],
      "ticket_states": [
        5,
        6
      ]
    }
  },
  "rows_1025_keyed": {
    "absent_keys": 60,
    "arrival_layout_tables": 0,
    "dead_keys": 27,
    "feeds": 5,
    "keyed_layout_tables": 3,
    "keyless_table_feeds": 0,
    "live_rows": 16272,
    "max_tiles": 6,
    "table_feeds": 15,
    "tables_over_one_tile": {
      "import_jobs": [
        5,
        6
      ],
      "knowledge_base_categories": [
        5,
        6
      ],
      "ticket_states": [
        5,
        6
      ]
    }
  },
  "wide_arrival": {
    "absent_keys": 2456,
    "arrival_layout_tables": 14,
    "dead_keys": 1,
    "feeds": 51,
    "keyed_layout_tables": 0,
    "keyless_table_feeds": 153,
    "live_rows": 2881,
    "max_tiles": 1,
    "table_feeds": 614,
    "tables_over_one_tile": {}
  },
  "wide_keyed": {
    "absent_keys": 2456,
    "arrival_layout_tables": 0,
    "dead_keys": 1,
    "feeds": 51,
    "keyed_layout_tables": 14,
    "keyless_table_feeds": 153,
    "live_rows": 2881,
    "max_tiles": 1,
    "table_feeds": 614,
    "tables_over_one_tile": {}
  }
}"#;

#[test]
fn the_placement_is_a_bijection_after_every_feed_in_both_layouts() {
    let mut got = serde_json::Map::new();
    for (name, append) in [("wide_keyed", false), ("wide_arrival", true)] {
        let tally = wide(append);
        println!("BIJECTION {name} {}", summary(&tally));
        got.insert(name.to_owned(), summary(&tally));
    }
    for (name, size, file) in [("rows_1025", "1025", "shadow-rows.pg.json")] {
        for append in [false, true] {
            let tally = captured(size, file, append);
            let key = format!("{name}_{}", if append { "arrival" } else { "keyed" });
            println!("BIJECTION {key} {}", summary(&tally));
            got.insert(key, summary(&tally));
        }
    }
    let expected: Value = serde_json::from_str(EXPECTED).expect("expected json");
    assert_eq!(Value::Object(got), expected);
}
