// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A fed `date` and `double precision` column has no lane: it keeps its cells
//! and `rows` renders them back unchanged; querying on it is refused by name.

use quack_shadow::{Catalog, Refusal, rows};
use serde_json::{Value, json};

fn catalog() -> Catalog {
    let mut catalog = Catalog::new();
    catalog
        .apply_feed(&json!({"tables": {"t": {
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "on", "type": "date", "null": true},
                {"name": "x", "type": "double precision", "null": true},
            ],
            "upsert": [
                [1, {"other": "Date", "s": "2000-02-29"}, {"f": "0.30000000000000004"}],
                [2, {"other": "Date", "s": "1969-12-31"}, {"f": "-Infinity"}],
                [3, null, null],
            ],
            "delete": [],
        }}}))
        .expect("feed");
    catalog
}

fn call(where_: Value) -> Value {
    json!({
        "table": "t", "terminal": "rows", "where": where_, "select": "*",
        "order": [{"table": "t", "col": "id", "dir": "asc"}],
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
    })
}

#[test]
fn a_fed_date_and_float_render_back_unchanged() {
    let catalog = catalog();
    let got = rows(&catalog, &call(Value::Null), false).expect("rows");
    assert_eq!(
        got.rows,
        vec![
            vec![
                json!(1),
                json!({"other": "Date", "s": "2000-02-29"}),
                json!({"f": "0.30000000000000004"})
            ],
            vec![
                json!(2),
                json!({"other": "Date", "s": "1969-12-31"}),
                json!({"f": "-Infinity"})
            ],
            vec![json!(3), Value::Null, Value::Null],
        ]
    );
}

#[test]
fn a_filter_on_a_date_or_float_is_refused_by_name() {
    let catalog = catalog();
    for col in ["on", "x"] {
        let filter = json!({"table": "t", "col": col, "op": "eq", "value": 1});
        assert_eq!(
            rows(&catalog, &call(filter), false).err(),
            Some(Refusal::UnlanedColumn {
                table: "t".into(),
                field: col.into()
            })
        );
    }
}
