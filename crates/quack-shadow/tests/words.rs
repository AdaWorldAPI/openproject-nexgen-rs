// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! A condition's operators are resolved through the polyfill lens by name:
//! position binds, labels are aliases, and an operator the lens has no word for
//! is named, not guessed.

mod common;

use std::collections::BTreeMap;

use lance_graph_r2il::polyfill::{AliasLanguage, POLYFILL, position_of};
use quack_shadow::words::{CONSTRUCTORS, Construct, Word};
use quack_shadow::{Answer, Catalog, Refusal, Verdict, WordStats, answer, shadow_verdict};
use serde_json::{Value, json};

const EQUAL: &str = "IntEqual";
const AND: &str = "IntAnd";
const OR: &str = "IntOr";
const NOT: &str = "IntNot";

fn position(r2il: &str) -> u8 {
    position_of(r2il).expect("polyfill position")
}

#[test]
fn sql_names_resolve_to_polyfill_positions() {
    for (sql, r2il, quack) in [
        ("=", EQUAL, "Cmp::EqI32"),
        ("AND", AND, "Filter::and"),
        ("OR", OR, "Filter::or"),
        ("NOT", NOT, "Filter::negate"),
    ] {
        let word = Word::named(sql).unwrap_or_else(|r| panic!("{sql}: {r:?}"));
        assert_eq!(word.position(), position(r2il), "{sql}");
        assert_eq!(word.sql(), sql);
        assert_eq!(word.quack(), Some(quack), "{sql}");
        assert!(
            CONSTRUCTORS.iter().any(|(name, _)| *name == quack),
            "{quack} has a constructor"
        );
    }
    for name in ["LIKE", "IN", "<>", "ILIKE", "FOO", "SUM"] {
        assert_eq!(Word::named(name), Err(Refusal::NoWord(name.to_owned())));
    }
}

#[test]
fn every_constructor_is_the_quack_alias_of_its_sql_word() {
    assert_eq!(CONSTRUCTORS.len(), 5);
    for (quack, construct) in CONSTRUCTORS {
        let sql = match construct {
            Construct::Equal => "=",
            Construct::And => "AND",
            Construct::Or => "OR",
            Construct::Negate => "NOT",
        };
        let at = Word::named(sql).expect("word").position();
        assert_eq!(position_of(quack), Some(at), "{quack}");
        assert!(POLYFILL[usize::from(at)].is_alias(quack), "{quack}");
        assert!(
            POLYFILL[usize::from(at)]
                .name_of(AliasLanguage::Sql)
                .is_some_and(|name| name == sql)
        );
    }
}

fn replay(file: &str) -> (WordStats, Vec<Value>) {
    let text = std::fs::read_to_string(format!(
        "{}/../../oracle/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("scenario file");
    let calls: Vec<Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v.get("terminal").is_some())
        .collect();
    let mut catalog = Catalog::new();
    if let Some(classes) = common::class_map(&common::models_of(&calls)) {
        catalog.set_classes(classes).expect("classes");
    }
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let item: Value = serde_json::from_str(line).expect("line");
        if let Some(feed) = item.get("feed") {
            catalog.apply_feed(feed).expect("feed");
        } else if item.get("terminal").is_some() {
            match shadow_verdict(&catalog, &item, false) {
                Verdict::Equal(_) => {}
                other => panic!("{}: {other:?}", item["sql"]),
            }
        }
    }
    (catalog.word_stats(), calls)
}

/// What the recorded conditions contain, read from the files: `eq` leaves and
/// `and` / `or` / `not` nodes outside fragments, and the operator tokens
/// inside fragments.
fn census(calls: &[Value]) -> (BTreeMap<String, u64>, BTreeMap<String, u64>) {
    fn walk(n: &Value, nodes: &mut BTreeMap<String, u64>, tokens: &mut BTreeMap<String, u64>) {
        match n {
            Value::Object(o) => {
                if let Some(op) = o.get("op").and_then(Value::as_str) {
                    *nodes.entry(op.to_owned()).or_default() += 1;
                    if let Some(sql) = o.get("sql").and_then(Value::as_str) {
                        for token in sql.replace(['(', ')'], " ").split_whitespace() {
                            let upper = token.to_ascii_uppercase();
                            if ["=", "AND", "OR", "LIKE"].contains(&upper.as_str()) {
                                *tokens.entry(upper).or_default() += 1;
                            }
                        }
                    }
                }
                o.values().for_each(|v| walk(v, nodes, tokens));
            }
            Value::Array(a) => a.iter().for_each(|v| walk(v, nodes, tokens)),
            _ => {}
        }
    }
    let (mut nodes, mut tokens) = (BTreeMap::new(), BTreeMap::new());
    for call in calls {
        walk(&call["where"], &mut nodes, &mut tokens);
    }
    (nodes, tokens)
}

fn by_name(stats: &WordStats) -> Vec<(&str, u64)> {
    stats
        .by_position
        .iter()
        .map(|(p, n)| (POLYFILL[usize::from(*p)].aliases[0], *n))
        .collect()
}

#[test]
fn the_recorded_conditions_are_counted_by_word() {
    let (stats, calls) = replay("shadow-scenario.pg.ndjson");
    let (nodes, tokens) = census(&calls);
    assert_eq!((nodes["eq"], nodes["and"], tokens.len()), (65, 8, 0));
    // 16 of the 65 `eq` leaves are on the dimensions of 8 joins whose tables hold no
    // rows yet: the key set returns before reading them.
    assert_eq!(by_name(&stats), [(EQUAL, 49), (AND, 8)]);
    assert!(stats.without_word.is_empty());

    let (stats, calls) = replay("shadow-scenario-wide.pg.ndjson");
    let (nodes, tokens) = census(&calls);
    assert_eq!(
        (nodes["eq"], nodes["and"], nodes["bound_sql"]),
        (456, 58, 57)
    );
    assert_eq!(
        (tokens["="], tokens["AND"], tokens["OR"], tokens["LIKE"]),
        (330, 57, 489, 273)
    );
    // Every leaf is read, the fragments' tokens one by one.
    assert_eq!(
        by_name(&stats),
        [(EQUAL, 456 + 330), (AND, 58 + 57), (OR, 489)]
    );
    let names: Vec<(&str, u64)> = stats
        .without_word
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();
    assert_eq!(names, [("LIKE", 273), ("bound_sql", 57)]);
}

fn counted() -> Catalog {
    let mut catalog = Catalog::new();
    let rows: Vec<Value> = (1..=12).map(|i| json!([i, i % 2, i % 3])).collect();
    catalog
        .apply_feed(&json!({"tables": {"t": {
            "columns": [
                {"name": "id", "type": "integer", "null": false},
                {"name": "a", "type": "integer", "null": false},
                {"name": "b", "type": "integer", "null": false},
            ],
            "upsert": rows, "delete": [],
        }}}))
        .expect("feed");
    catalog
}

fn count_call(filter: Value) -> Value {
    json!({
        "table": "t", "terminal": "count", "where": filter,
        "limit": null, "offset": null,
        "joins": 0, "group": 0, "having": 0, "distinct": false, "from": 0, "lock": 0,
        "answer": 0,
    })
}

fn eq(col: &str, value: i64) -> Value {
    json!({"table": "t", "col": col, "op": "eq", "value": value})
}

#[test]
fn a_fragment_and_the_ops_resolve_the_same_words() {
    let catalog = counted();
    let read = |call: &Value| {
        let before = catalog.word_stats();
        let got = answer(&catalog, call, false).expect("answer");
        let after = catalog.word_stats();
        let delta: BTreeMap<u8, u64> = after
            .by_position
            .iter()
            .map(|(p, n)| (*p, n - before.by_position.get(p).copied().unwrap_or(0)))
            .filter(|(_, n)| *n > 0)
            .collect();
        (got, delta)
    };

    let fragment = count_call(json!({
        "op": "bound_sql", "sql": "t.a = ? AND (t.b = ? OR t.b = ?)", "binds": [1, 1, 2],
    }));
    let tree = count_call(json!({"op": "and", "args": [
        eq("a", 1),
        {"op": "or", "args": [eq("b", 1), eq("b", 2)]},
    ]}));
    let (from_fragment, words_of_fragment) = read(&fragment);
    let (from_tree, words_of_tree) = read(&tree);

    assert_eq!(from_fragment, from_tree);
    assert_eq!(from_tree, Answer::Count(4));
    assert_eq!(words_of_fragment, words_of_tree);
    assert_eq!(
        words_of_tree,
        BTreeMap::from([(position(EQUAL), 3), (position(AND), 1), (position(OR), 1)])
    );
}
