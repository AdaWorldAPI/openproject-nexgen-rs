//! A feed entry's cells are read by its own `columns` declaration, not by
//! position in the table: a feed that declares `[id, a]` for a table built
//! as `[a, id]` updates the row it names instead of inventing another one.
//! Not in zammad-rs (nexgen divergence, see COPIED_FROM.md).

use quack_shadow::{Catalog, rows};
use serde_json::{Value, json};

fn feed(catalog: &mut Catalog, entry: &Value) -> Result<(), String> {
    catalog
        .apply_feed(&json!({"tables": {"t": entry}}))
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

fn catalog_a_id() -> Catalog {
    let mut catalog = Catalog::new();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "a", "type": "integer", "null": true},
                {"name": "id", "type": "integer", "null": false}
            ],
            "upsert": [[10, 1]]
        }),
    )
    .expect("first feed");
    catalog
}

fn rows_of(catalog: &Catalog) -> Vec<Vec<Value>> {
    // Shaped like a captured call (oracle/shadow-rows.pg.json): ids 1, 10 and 20,
    // which covers the row a positional read would invent.
    let call = json!({"table": "t", "terminal": "rows",
                      "where": {"table": "t", "col": "id", "op": "in", "values": [1, 10, 20]},
                      "select": "*", "order": [{"table": "t", "col": "id", "dir": "asc"}],
                      "limit": null, "offset": null, "joins": 0, "join": [], "group": 0,
                      "having": 0, "distinct": false, "from": 0, "lock": 0, "dirty_tables": []});
    rows(catalog, &call, false).map_err(|e| format!("{e:?}")).expect("rows").rows
}

#[test]
fn a_feed_declaring_another_column_order_updates_the_named_row() {
    let mut catalog = catalog_a_id();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "a", "type": "integer", "null": true}
            ],
            "upsert": [[1, 20]]
        }),
    )
    .expect("second feed");
    // One row, id 1 with a = 20: updated in place, no row with id 20.
    assert_eq!(rows_of(&catalog), [vec![json!(20), json!(1)]], "columns are [a, id]");
}

#[test]
fn a_feed_declaring_other_columns_is_refused() {
    let mut catalog = catalog_a_id();
    let got = feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "b", "type": "integer", "null": true}
            ],
            "upsert": [[1, 20]]
        }),
    );
    assert!(got.is_err(), "{got:?}");
}
