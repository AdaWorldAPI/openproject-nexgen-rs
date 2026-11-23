// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! The strings of a text column live in DT's code table under the table's own
//! frame store: a literal binds through it, and a change of the strings is one
//! more advance of that store.

use lance_graph_dt64::dr::CodeRead;
use quack_shadow::{Answer, Catalog, Refusal, answer, answer_bound};
use serde_json::{Value, json};

fn feed(catalog: &mut Catalog, entry: &Value) {
    catalog
        .apply_feed(&json!({"tables": {"t": entry}}))
        .expect("feed");
}

/// `t(id, name, n)` with the rows 1 a, 2 b, 3 c, 4 c.
fn catalog() -> Catalog {
    let mut catalog = Catalog::new();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "name", "type": "character varying", "null": true},
                {"name": "n", "type": "integer", "null": true},
            ],
            "upsert": [[1, "a", 10], [2, "b", 20], [3, "c", 30], [4, "c", 40]],
            "delete": [],
        }),
    );
    catalog
}

fn count(op: &str, value: &Value) -> Value {
    let key = if op == "in" { "values" } else { "value" };
    json!({
        "table": "t", "terminal": "count",
        "where": {"table": "t", "col": "name", "op": op, key: value},
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        "answer": 0,
    })
}

fn code_in(
    catalog: &Catalog,
    literal: &str,
    frame: &lance_graph_dt64::dr::Frame,
) -> Result<u32, CodeRead> {
    catalog.code_in("t", "name", literal, frame)
}

#[test]
fn a_text_literal_binds_through_the_code_table() {
    let catalog = catalog();
    assert_eq!(
        answer(&catalog, &count("eq", &json!("b")), false),
        Ok(Answer::Count(1))
    );
    assert_eq!(
        answer(&catalog, &count("eq", &json!("c")), false),
        Ok(Answer::Count(2))
    );
    assert_eq!(
        answer(&catalog, &count("in", &json!(["a", "c", "nope"])), false),
        Ok(Answer::Count(3))
    );
    assert_eq!(
        answer(&catalog, &count("eq", &json!("nope")), false),
        Ok(Answer::Count(0)),
        "a literal that is not in the column matches nothing"
    );
}

#[test]
fn a_new_string_retires_older_bindings() {
    let mut catalog = catalog();
    let (eq_b, eq_z) = (count("eq", &json!("b")), count("eq", &json!("z")));
    let bound = catalog.bind().expect("bind");
    assert_eq!(
        answer_bound(&catalog, &bound, &eq_b, false),
        Ok(Answer::Count(1))
    );
    let sealed = catalog.seal_of("t").expect("table");
    let advances = catalog.code_advances();

    // A rewrite to a string the column already holds: the frame moves as it
    // does for every feed, the code space does not.
    feed(
        &mut catalog,
        &json!({"upsert": [[4, "b", 40]], "delete": []}),
    );
    let after_twin = catalog.seal_of("t").expect("table");
    assert!(after_twin.version() > sealed.version());
    assert_eq!(catalog.code_advances(), advances, "no string is new");
    for literal in ["a", "b", "c"] {
        assert_eq!(
            code_in(&catalog, literal, &sealed),
            code_in(&catalog, literal, &after_twin),
            "{literal}"
        );
        assert!(code_in(&catalog, literal, &after_twin).is_ok(), "{literal}");
    }

    // A string the column has not held: no tile moves, one code advance.
    let rows = catalog.tables[0].rows;
    feed(
        &mut catalog,
        &json!({"upsert": [[2, "z", 20]], "delete": []}),
    );
    assert_eq!(
        catalog.tables[0].rows, rows,
        "an in-place update adds no position"
    );
    assert_eq!(catalog.code_advances(), advances + 1);
    assert!(matches!(
        answer_bound(&catalog, &bound, &eq_b, false),
        Err(Refusal::Stale { .. })
    ));
    let fresh = catalog.bind().expect("rebind");
    assert_eq!(
        answer_bound(&catalog, &fresh, &eq_z, false),
        Ok(Answer::Count(1))
    );
    assert_eq!(
        answer_bound(&catalog, &fresh, &eq_b, false),
        Ok(Answer::Count(1)),
        "row 4 still holds b"
    );
    let now = catalog.seal_of("t").expect("table");
    assert_eq!(code_in(&catalog, "z", &now), Ok(3));
    assert_eq!(
        code_in(&catalog, "z", &after_twin),
        Err(CodeRead::UnknownKey)
    );
}

#[test]
fn codes_are_read_at_the_frame_they_were_sealed_under() {
    let mut catalog = Catalog::new();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "name", "type": "character varying", "null": true},
            ],
            "upsert": [[1, "b"], [2, "c"]],
            "delete": [],
        }),
    );
    let sealed = catalog.seal_of("t").expect("table");
    assert_eq!(code_in(&catalog, "b", &sealed), Ok(0));

    // `a` sorts before `b`, but a string keeps its code: `a` takes the next index.
    feed(&mut catalog, &json!({"upsert": [[3, "a"]], "delete": []}));
    let now = catalog.seal_of("t").expect("table");
    assert_eq!(
        code_in(&catalog, "b", &sealed),
        Ok(0),
        "the old frame keeps the old code"
    );
    assert_eq!(code_in(&catalog, "c", &sealed), Ok(1));
    assert_eq!(code_in(&catalog, "a", &sealed), Err(CodeRead::UnknownKey));
    assert_eq!(code_in(&catalog, "b", &now), Ok(0));
    assert_eq!(code_in(&catalog, "c", &now), Ok(1));
    assert_eq!(code_in(&catalog, "a", &now), Ok(2));
    assert_eq!(
        answer(&catalog, &count("eq", &json!("b")), false),
        Ok(Answer::Count(1))
    );
}

#[test]
fn an_unchanged_column_advances_no_code_space() {
    let mut catalog = catalog();
    let (all, table) = (catalog.code_advances(), catalog.table_code_advances("t"));
    let before = catalog.seal_of("t").expect("table");
    feed(
        &mut catalog,
        &json!({"upsert": [[1, "a", 11], [3, "c", 31]], "delete": []}),
    );
    assert_eq!(
        (catalog.code_advances(), catalog.table_code_advances("t")),
        (all, table)
    );
    assert!(catalog.seal_of("t").expect("table").version() > before.version());
}

fn codes_of(catalog: &Catalog, literals: &[&str]) -> Vec<Result<u32, CodeRead>> {
    let now = catalog.seal_of("t").expect("table");
    literals.iter().map(|l| code_in(catalog, l, &now)).collect()
}

#[test]
fn a_strings_code_never_changes() {
    let mut catalog = Catalog::new();
    feed(
        &mut catalog,
        &json!({
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "name", "type": "character varying", "null": true},
            ],
            "upsert": [[1, "b"], [2, "c"]],
            "delete": [],
        }),
    );
    let mut frames = vec![catalog.seal_of("t").expect("table")];
    // `a` sorts first, then every row holding `b` goes, then `b` comes back.
    for entry in [
        json!({"upsert": [[3, "a"]], "delete": []}),
        json!({"upsert": [], "delete": [1]}),
        json!({"upsert": [[4, "b"]], "delete": []}),
    ] {
        feed(&mut catalog, &entry);
        frames.push(catalog.seal_of("t").expect("table"));
    }
    for (at, frame) in frames.iter().enumerate() {
        assert_eq!(code_in(&catalog, "b", frame), Ok(0), "b at frame {at}");
        assert_eq!(code_in(&catalog, "c", frame), Ok(1), "c at frame {at}");
        let a = code_in(&catalog, "a", frame);
        if at == 0 {
            assert_eq!(a, Err(CodeRead::UnknownKey));
        } else {
            assert_eq!(a, Ok(2), "a at frame {at}");
        }
    }
    assert_eq!(
        answer(&catalog, &count("eq", &json!("b")), false),
        Ok(Answer::Count(1))
    );
}

#[test]
fn only_a_new_string_advances_a_code_space() {
    let mut catalog = catalog();
    let start = catalog.table_code_advances("t");

    feed(
        &mut catalog,
        &json!({"upsert": [[1, "b", 11], [4, "a", 41]], "delete": []}),
    );
    assert_eq!(catalog.table_code_advances("t"), start, "rewrites");

    // Row 1 held `a`, and so does row 4 now; remove both: `a` has no live row.
    feed(&mut catalog, &json!({"upsert": [], "delete": [1, 4]}));
    assert_eq!(
        catalog.table_code_advances("t"),
        start,
        "the last row of a string went"
    );

    feed(
        &mut catalog,
        &json!({"upsert": [[5, "z", 50]], "delete": []}),
    );
    assert_eq!(catalog.table_code_advances("t"), start + 1, "z is new");
    assert!(catalog.frames_agree());
    assert_eq!(
        codes_of(&catalog, &["a", "b", "c", "z"]),
        [Ok(0), Ok(1), Ok(2), Ok(3)]
    );
}

#[test]
fn a_string_without_a_live_row_matches_no_row() {
    let mut catalog = catalog();
    feed(&mut catalog, &json!({"upsert": [], "delete": [2]}));
    let now = catalog.seal_of("t").expect("table");
    assert!(code_in(&catalog, "b", &now).is_ok(), "b keeps its code");
    assert_eq!(
        answer(&catalog, &count("eq", &json!("b")), false),
        Ok(Answer::Count(0))
    );
    let like = json!({
        "table": "t", "terminal": "count",
        "where": {"op": "bound_sql", "sql": "name LIKE ?", "binds": ["b%"]},
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        "answer": 0,
    });
    assert_eq!(answer(&catalog, &like, false), Ok(Answer::Count(0)));
}

/// The code advances of a replayed scenario, over all tables and for each table that had one.
fn replayed(file: &str) -> (u64, Vec<(String, u64)>) {
    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("scenario file");
    let mut catalog = Catalog::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let item: Value = serde_json::from_str(line).expect("scenario line");
        if let Some(feed) = item.get("feed") {
            catalog.apply_feed(feed).expect("feed");
            #[cfg(not(feature = "disable-frame-advance"))]
            assert!(catalog.frames_agree(), "table.frame and the store drifted");
        }
    }
    let per_table = catalog
        .tables
        .iter()
        .map(|t| (t.name.clone(), catalog.table_code_advances(&t.name)))
        .filter(|(_, n)| *n > 0)
        .collect();
    (catalog.code_advances(), per_table)
}

#[test]
fn the_scenarios_advance_codes_a_pinned_number_of_times() {
    for (file, pinned) in [
        ("shadow-scenario.pg.ndjson", SCENARIO),
        ("shadow-scenario-wide.pg.ndjson", WIDE),
    ] {
        let (total, per_table) = replayed(file);
        println!("CODES {file} total={total} per_table={per_table:?}");
        #[cfg(not(feature = "disable-code-advance"))]
        {
            let got: Vec<(&str, u64)> = per_table.iter().map(|(t, n)| (t.as_str(), *n)).collect();
            assert_eq!((total, got.as_slice()), pinned, "{file}");
        }
        #[cfg(feature = "disable-code-advance")]
        {
            let _ = pinned;
            assert_eq!(total, 0, "{file}");
        }
    }
}

type Pinned = (u64, &'static [(&'static str, u64)]);

// Pinned from a measured run; see the CODES lines.
const SCENARIO: Pinned = (
    10,
    &[
        ("groups", 1),
        ("organizations", 1),
        ("ticket_article_senders", 1),
        ("ticket_article_types", 1),
        ("ticket_articles", 1),
        ("ticket_priorities", 1),
        ("ticket_state_types", 1),
        ("ticket_states", 1),
        ("tickets", 1),
        ("users", 1),
    ],
);
const WIDE: Pinned = (
    35,
    &[
        ("groups", 1),
        ("organizations", 1),
        ("permissions", 1),
        ("roles", 1),
        ("ticket_article_senders", 1),
        ("ticket_article_types", 1),
        ("ticket_articles", 15),
        ("ticket_priorities", 1),
        ("ticket_state_types", 1),
        ("ticket_states", 1),
        ("tickets", 10),
        ("users", 1),
    ],
);
