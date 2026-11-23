// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! D-QSP-VROW's PostgreSQL gate on one table: the cells of
//! `knowledge_base_categories` read through one `borrow_all_selected_present`
//! check equal what PostgreSQL returns for the same SELECT (multiset, column
//! types, NULLs). The oracle is loaded from the captured values, never from
//! the lanes.
//!
//! The test issues the position lists itself: which component issues them in
//! production is open (EXO-329). Each position comes from the catalog's own DR
//! placement of a key, never from the key.
//!
//! `VROW_REQUIRE_PG=1` turns a missing server or class source into a failure:
//! a SKIP is not a pass.

mod common;

use std::collections::BTreeSet;

use pg_parity_harness::{Answer, Order, Outcome, Pg, Row, Rows, compare};
use quack_shadow::{Bound, Catalog, Refusal};
use serde_json::{Value, json};

const TABLE: &str = "knowledge_base_categories";

fn require_pg() -> bool {
    std::env::var("VROW_REQUIRE_PG").is_ok_and(|v| v == "1")
}

fn capture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/shadow-rows.pg.json"
    ))
    .expect("capture file");
    serde_json::from_str(&text).expect("capture json")
}

/// The catalog of the size-1025 capture with its classes; `None` is a SKIP.
fn catalog_of(entry: &Value) -> Option<Catalog> {
    catalog_in(entry, false)
}

/// [`catalog_of`], with every table laid out in arrival order when `append`.
fn catalog_in(entry: &Value, append: bool) -> Option<Catalog> {
    let calls = entry["calls"].as_array().expect("calls");
    let classes = common::class_map(&common::models_of(calls));
    let Some(classes) = classes else {
        assert!(
            !require_pg(),
            "VROW_REQUIRE_PG=1 and the class source is not available: a skipped gate is not a pass"
        );
        return None;
    };
    let mut catalog = if append {
        Catalog::from_capture_append(entry)
    } else {
        Catalog::from_capture(entry)
    }
    .expect("catalog");
    catalog.set_classes(classes).expect("classes");
    Some(catalog)
}

struct Captured<'a> {
    columns: &'a [Value],
}

impl<'a> Captured<'a> {
    fn of(entry: &'a Value) -> Captured<'a> {
        Captured {
            columns: entry["tables"][TABLE]["columns"]
                .as_array()
                .expect("columns"),
        }
    }

    fn values(&self, name: &str) -> &'a [Value] {
        self.columns
            .iter()
            .find(|c| c["name"] == name)
            .and_then(|c| c["values"].as_array())
            .expect("column")
    }

    fn ints(&self, name: &str) -> Vec<Option<i64>> {
        self.values(name).iter().map(Value::as_i64).collect()
    }
}

fn is_live(catalog: &Catalog, position: usize) -> bool {
    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == TABLE)
        .expect("table");
    table
        .live
        .get(position / 64)
        .is_some_and(|word| (word >> (position % 64)) & 1 == 1)
}

/// The presence bit and value of `column` at `position`, straight from its lane.
fn lane_cell(catalog: &Catalog, column: &str, position: usize) -> (bool, u32) {
    let lane = catalog
        .tables
        .iter()
        .find(|t| t.name == TABLE)
        .and_then(|t| t.columns.iter().find(|c| c.name == column))
        .and_then(|c| c.lane.as_ref())
        .expect("lane");
    let bit = (lane.validity[position / 64] >> (position % 64)) & 1 == 1;
    (bit, lane.values[position])
}

fn position(catalog: &Catalog, key: i64) -> Option<u32> {
    catalog
        .position_of(TABLE, key)
        .map(|at| u32::try_from(at).expect("a position"))
}

/// The positions of the rows whose `parent_id` is not NULL and names a row,
/// in row order, both placed by the catalog; and how many name no row.
struct Edges {
    children: Vec<u32>,
    parents: Vec<u32>,
    /// Children whose parent is no live row: a left join NULL-extends them.
    absent_parents: usize,
}

/// Issues the edge positions. The test issues them (which component does so in
/// production is open, EXO-329); a position is emitted only for a live row.
fn edge_positions(catalog: &Catalog, captured: &Captured<'_>) -> Edges {
    let (ids, parents) = (captured.ints("id"), captured.ints("parent_id"));
    let mut edges = Edges {
        children: Vec::new(),
        parents: Vec::new(),
        absent_parents: 0,
    };
    for (id, parent) in ids.iter().zip(&parents) {
        let (Some(id), Some(parent)) = (id, parent) else {
            continue;
        };
        let Some(child) = position(catalog, *id) else {
            continue;
        };
        match position(catalog, *parent) {
            Some(target) => {
                edges.children.push(child);
                edges.parents.push(target);
            }
            None => edges.absent_parents += 1,
        }
    }
    edges
}

/// Every row whose `id % 7 == 0`, each taken twice, in row order.
fn doubled_positions(catalog: &Catalog, captured: &Captured<'_>) -> Vec<u32> {
    captured
        .ints("id")
        .into_iter()
        .flatten()
        .filter(|id| id % 7 == 0)
        .filter_map(|id| position(catalog, id))
        .flat_map(|at| [at; 2])
        .collect()
}

fn literal(sql_type: &str, cell: &Value) -> String {
    match cell {
        Value::Null => "NULL".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) if sql_type.starts_with("timestamp") => {
            format!("(to_timestamp({n}) AT TIME ZONE 'UTC')")
        }
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Object(o) => format!("'{}'", o["t"].as_str().expect("a timestamp text")),
        Value::Array(_) => panic!("an array cell"),
    }
}

/// The table in PostgreSQL's scratch schema, with the captured types,
/// nullability and values.
fn load(pg: &Pg, captured: &Captured<'_>) {
    let defs: Vec<String> = captured
        .columns
        .iter()
        .map(|c| {
            format!(
                "{} {}{}",
                c["name"].as_str().expect("name"),
                c["type"].as_str().expect("type"),
                if c["null"] == false { " NOT NULL" } else { "" }
            )
        })
        .collect();
    let rows = captured.values("id").len();
    let mut sql = format!("CREATE TABLE {TABLE} ({});\n", defs.join(", "));
    for row in 0..rows {
        let cells: Vec<String> = captured
            .columns
            .iter()
            .map(|c| literal(c["type"].as_str().expect("type"), &c["values"][row]))
            .collect();
        sql.push_str(&format!(
            "INSERT INTO {TABLE} VALUES ({});\n",
            cells.join(", ")
        ));
    }
    pg.script(&sql).expect("load the captured rows");
}

fn nulls(rows: &[Row], column: usize) -> usize {
    rows.iter().filter(|row| row[column].is_none()).count()
}

fn null_counts(rows: &Rows) -> Vec<usize> {
    (0..rows.types.len())
        .map(|c| nulls(&rows.rows, c))
        .collect()
}

/// A nullable column holding both NULL and non-NULL cells.
fn mixed(rows: &Rows) -> bool {
    null_counts(rows)
        .iter()
        .any(|&n| n > 0 && n < rows.rows.len())
}

fn multiset(rows: &[Row]) -> Vec<&Row> {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort();
    sorted
}

struct Gate {
    label: &'static str,
    selection: usize,
    distinct: usize,
    columns: Vec<String>,
    pg: Rows,
    ours: Rows,
}

/// One selection and the SELECT it must equal.
struct Arm<'a> {
    label: &'static str,
    sel: u16,
    select: &'a str,
    selection: &'a [u32],
    /// All-absent rows emitted without a position (a left join's missing side).
    absent: usize,
}

/// One selection against its SELECT, which must be an exact multiset match.
fn gate(pg: &Pg, catalog: &Catalog, bound: &Bound, arm: Arm<'_>) -> Gate {
    let Arm {
        label,
        sel,
        select,
        selection,
        absent,
    } = arm;
    let (types, mut rows) = bound.read_selection(catalog, TABLE, sel).expect("read");
    // A left join's missing side is the absent value on every bound lane
    // (presence 0, value 0; lance_graph_contract::lane_codec, EXO-294): no
    // position names it, so no lane is read.
    let missing = bound.absent_row(catalog, TABLE).expect("absent row");
    rows.extend((0..absent).map(|_| missing.clone()));
    let ours = Rows { types, rows };
    let theirs = pg.query(select);
    let outcome = compare(&theirs, &Answer::Rows(ours.clone()), Order::Unordered);
    println!("GATE {label}: {select}");
    match outcome {
        Outcome::Pass => {}
        Outcome::Fail(diff) => panic!("{label}: {diff}"),
        other => panic!("{label}: {other:?}"),
    }
    let theirs = theirs.expect("rows");
    println!(
        "GATE {label}: rows={} NULLs per column PostgreSQL={:?} lanes={:?}",
        theirs.rows.len(),
        null_counts(&theirs),
        null_counts(&ours)
    );
    assert_eq!(null_counts(&theirs), null_counts(&ours), "{label}: NULLs");
    Gate {
        label,
        selection: selection.len(),
        distinct: selection.iter().collect::<BTreeSet<_>>().len(),
        columns: ours.types.clone(),
        pg: theirs,
        ours,
    }
}

#[test]
fn the_cells_read_through_a_selection_equal_postgresql() {
    equal_postgresql(false);
}

#[test]
fn the_cells_read_in_arrival_order_equal_postgresql() {
    equal_postgresql(true);
}

fn equal_postgresql(append: bool) {
    let data = capture();
    let entry = &data["sizes"]["1025"];
    let Some(mut catalog) = catalog_in(entry, append) else {
        return;
    };
    let pg = match Pg::connect() {
        Ok(pg) => pg,
        Err(skip) => {
            assert!(
                !require_pg(),
                "VROW_REQUIRE_PG=1 and there is no server ({}): a SKIP is not a pass",
                skip.0
            );
            println!("SKIP: {}", skip.0);
            return;
        }
    };
    println!("PG_VERSION {} layout_append={append}", pg.version());
    let captured = Captured::of(entry);
    load(&pg, &captured);

    let bound = catalog.bind().expect("bind");
    let report = bound.report(&catalog);
    let binding = report
        .iter()
        .find(|t| t.table == TABLE)
        .expect("table binding");
    println!(
        "TABLE {TABLE} positions={} bound={:?}",
        catalog
            .tables
            .iter()
            .find(|t| t.name == TABLE)
            .expect("table")
            .rows,
        binding.bound
    );
    assert_eq!(binding.bound, ["knowledge_base_id", "parent_id"]);
    let bound_list = binding
        .bound
        .iter()
        .map(|c| format!("p.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let own_list = binding
        .bound
        .iter()
        .map(|c| format!("c.{c}"))
        .collect::<Vec<_>>()
        .join(", ");

    let edges = edge_positions(&catalog, &captured);
    let doubled = doubled_positions(&catalog, &captured);
    let with_parent = captured.ints("parent_id").iter().flatten().count();
    let distinct = |positions: &[u32]| positions.iter().collect::<BTreeSet<_>>().len();
    println!(
        "EDGE children_with_parent_id={with_parent} inner={} distinct_parents={} dangling={}",
        edges.parents.len(),
        distinct(&edges.parents),
        with_parent - edges.parents.len()
    );
    assert_eq!(with_parent, 684);
    assert_eq!(edges.parents.len(), 547, "children whose parent is a row");
    assert_eq!(edges.absent_parents, 137);
    assert!(
        distinct(&edges.parents) < edges.parents.len(),
        "the edge selection is a bag with repeats"
    );
    assert!(
        edges
            .parents
            .iter()
            .all(|&at| is_live(&catalog, at as usize))
    );

    let sel_inner = catalog
        .issue_selection(TABLE, edges.parents.clone())
        .expect("issue");
    let sel_doubled = catalog
        .issue_selection(TABLE, doubled.clone())
        .expect("issue");
    let bound = catalog.bind().expect("bind");

    let inner_select =
        format!("SELECT {bound_list} FROM {TABLE} c JOIN {TABLE} p ON p.id = c.parent_id");
    let left_select = format!(
        "SELECT {bound_list} FROM {TABLE} c LEFT JOIN {TABLE} p ON p.id = c.parent_id \
         WHERE c.parent_id IS NOT NULL"
    );
    // The doubled arm runs first and always: its NULLs come from lanes alone.
    let second = {
        let select = format!(
            "SELECT {own_list} FROM {TABLE} c WHERE c.id % 7 = 0 \
             UNION ALL SELECT {own_list} FROM {TABLE} c WHERE c.id % 7 = 0"
        );
        let second = gate(
            &pg,
            &catalog,
            &bound,
            Arm {
                label: "doubled",
                sel: sel_doubled,
                select: &select,
                selection: &doubled,
                absent: 0,
            },
        );
        assert_eq!(second.selection, 292);
        assert_eq!(second.distinct, 146);
        assert!(second.distinct < second.selection);
        assert!(
            mixed(&second.pg),
            "the second selection holds both NULL and non-NULL cells"
        );
        second
    };
    let first = gate(
        &pg,
        &catalog,
        &bound,
        Arm {
            label: "edge",
            sel: sel_inner,
            select: &inner_select,
            selection: &edges.parents,
            absent: 0,
        },
    );
    assert_eq!(first.selection, 547);
    let left_gate = gate(
        &pg,
        &catalog,
        &bound,
        Arm {
            label: "edge-left",
            sel: sel_inner,
            select: &left_select,
            selection: &edges.parents,
            absent: edges.absent_parents,
        },
    );

    let gates = vec![first, left_gate, second];
    for g in &gates {
        let nullable: Vec<String> = g
            .columns
            .iter()
            .zip(null_counts(&g.pg))
            .map(|(t, n)| format!("{t}:{n}"))
            .collect();
        println!(
            "RESULT {} selection={} distinct={} rows={} column types:NULLs={nullable:?} Pass",
            g.label,
            g.selection,
            g.distinct,
            g.pg.rows.len()
        );
        assert_eq!(g.pg.types, g.ours.types);
    }
    // The disable runs must change what is read, judged on PostgreSQL and the
    // capture alone: the first rows of the table are not the selected ones.
    let first_rows: Vec<Row> = captured
        .ints("knowledge_base_id")
        .into_iter()
        .zip(captured.ints("parent_id"))
        .take(gates[0].pg.rows.len())
        .map(|(a, b)| vec![a.map(|v| v.to_string()), b.map(|v| v.to_string())])
        .collect();
    assert_ne!(
        multiset(&first_rows),
        multiset(&gates[0].pg.rows),
        "the first rows are not the selected rows"
    );
    // The presence disable binds on NULLs that come from lanes: the doubled
    // selection reads only lanes, and PostgreSQL returns NULLs for it.
    assert_eq!(gates.len(), 3, "the doubled arm ran");
    let doubled_gate = gates.last().expect("a gate");
    assert_eq!(doubled_gate.label, "doubled");
    assert!(null_counts(&doubled_gate.pg).iter().any(|&n| n > 0));
}

/// A deleted parent is no parent: its cells stay in the lanes, only its live
/// bit is cleared, and PostgreSQL's left join NULL-extends its children.
#[test]
fn a_deleted_parent_is_read_as_no_parent() {
    deleted_parent(false);
}

#[test]
fn a_deleted_parent_is_read_as_no_parent_in_arrival_order() {
    deleted_parent(true);
}

fn deleted_parent(append: bool) {
    let data = capture();
    let entry = &data["sizes"]["1025"];
    let Some(mut catalog) = catalog_in(entry, append) else {
        return;
    };
    let pg = match Pg::connect() {
        Ok(pg) => pg,
        Err(skip) => {
            assert!(
                !require_pg(),
                "VROW_REQUIRE_PG=1 and there is no server ({}): a SKIP is not a pass",
                skip.0
            );
            println!("SKIP: {}", skip.0);
            return;
        }
    };
    let captured = Captured::of(entry);
    load(&pg, &captured);

    let parent = 1;
    let at = catalog.position_of(TABLE, parent).expect("the parent row");
    catalog
        .apply_feed(&json!({"tables": {TABLE: {"upsert": [], "delete": [parent]}}}))
        .expect("delete");
    pg.script(&format!("DELETE FROM {TABLE} WHERE id = {parent};"))
        .expect("delete on PostgreSQL");

    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == TABLE)
        .expect("table");
    assert!(!is_live(&catalog, at), "the parent row is dead");
    for name in ["knowledge_base_id", "parent_id"] {
        let lane = table
            .columns
            .iter()
            .find(|c| c.name == name)
            .and_then(|c| c.lane.as_ref())
            .expect("lane");
        assert_ne!(
            lane.values[at], 0,
            "the dead row's {name} is still in its lane"
        );
        assert!(
            lane_cell(&catalog, name, at).0,
            "the dead row keeps presence 1: it is not -0"
        );
    }

    let edges = edge_positions(&catalog, &captured);
    println!(
        "DELETED parent id={parent} position={at} matched={} absent={}",
        edges.parents.len(),
        edges.absent_parents
    );
    let sel = catalog
        .issue_selection(TABLE, edges.parents.clone())
        .expect("issue");
    let bound = catalog.bind().expect("bind");
    let select = format!(
        "SELECT p.knowledge_base_id, p.parent_id FROM {TABLE} c LEFT JOIN {TABLE} p \
         ON p.id = c.parent_id WHERE c.parent_id IS NOT NULL"
    );
    let g = gate(
        &pg,
        &catalog,
        &bound,
        Arm {
            label: "deleted-parent",
            sel,
            select: &select,
            selection: &edges.parents,
            absent: edges.absent_parents,
        },
    );
    println!(
        "RESULT {} selection={} distinct={} rows={} NULLs={:?} Pass",
        g.label,
        g.selection,
        g.distinct,
        g.pg.rows.len(),
        null_counts(&g.pg)
    );
    assert!(edges.parents.iter().all(|&p| is_live(&catalog, p as usize)));
    assert_eq!(g.pg.rows.len(), 683);
    assert_eq!(null_counts(&g.pg), [683, 683]);
}

/// Positions issued in another row order are refused, never read through
/// these lanes.
#[test]
fn a_selection_in_another_row_order_is_refused() {
    let data = capture();
    let entry = &data["sizes"]["1025"];
    let Some(mut catalog) = catalog_of(entry) else {
        return;
    };
    let rows = catalog
        .tables
        .iter()
        .find(|t| t.name == TABLE)
        .expect("table")
        .rows;
    let own = catalog
        .issue_selection(TABLE, vec![1, 2, 3])
        .expect("issue");
    let other = catalog
        .issue_selection_in_order_of(TABLE, vec![1, 2, 3], rows + 5)
        .expect("issue");
    let bound = catalog.bind().expect("bind");
    let (_, read) = bound.read_selection(&catalog, TABLE, own).expect("read");
    assert_eq!(read.len(), 3);
    match bound.read_selection(&catalog, TABLE, other) {
        Err(Refusal::Bind(message)) => {
            assert!(message.contains("SelectionSpace"), "{message}");
            println!("REFUSED {message}");
        }
        Ok((_, rows)) => panic!("a selection of another row order read {} rows", rows.len()),
        Err(other) => panic!("{other:?}"),
    }
}

/// A read fails closed on a selection that names a row that is not live: the
/// deleted row's cells stay in the lanes and are never returned. The selection
/// is issued after the delete, because a feed also makes older ones unreadable.
#[test]
fn a_dead_position_is_refused_at_read() {
    let data = capture();
    let entry = &data["sizes"]["1025"];
    for append in [false, true] {
        let Some(mut catalog) = catalog_in(entry, append) else {
            return;
        };
        let at = position(&catalog, 3).expect("a live row");
        catalog
            .apply_feed(&json!({"tables": {TABLE: {"upsert": [], "delete": [3]}}}))
            .expect("delete");
        assert!(!is_live(&catalog, at as usize));
        // The dead row's cells are not -0: presence 1 and a nonzero value, so
        // the refusal, not the absent rule, is what blocks the read.
        assert_eq!(
            lane_cell(&catalog, "knowledge_base_id", at as usize),
            (true, 1)
        );
        let (before, after) = (
            position(&catalog, 2).expect("live"),
            position(&catalog, 4).expect("live"),
        );
        let positions = vec![before, at, after];
        let sel = catalog
            .issue_selection(TABLE, positions.clone())
            .expect("issue admits it");
        let live = catalog
            .issue_selection(TABLE, vec![before, after])
            .expect("issue");
        let bound = catalog.bind().expect("bind");
        match bound.read_selection(&catalog, TABLE, sel) {
            Err(Refusal::DeadPosition { table, position }) => {
                assert_eq!((table.as_str(), position), (TABLE, at));
                println!("REFUSED layout_append={append} DeadPosition {table} {position}");
            }
            Ok((_, rows)) => panic!("a dead row was read: {} rows", rows.len()),
            Err(other) => panic!("{other:?}"),
        }
        let (_, rows) = bound.read_selection(&catalog, TABLE, live).expect("read");
        assert_eq!(rows.len(), 2);
    }
}

/// A key that names no row resolves to no position, also one inside the last
/// tile, which holds keys past the last row.
#[test]
fn a_key_past_the_last_row_has_no_position() {
    let data = capture();
    let entry = &data["sizes"]["1025"];
    for append in [false, true] {
        let Some(catalog) = catalog_in(entry, append) else {
            return;
        };
        let rows = catalog
            .tables
            .iter()
            .find(|t| t.name == TABLE)
            .expect("table")
            .rows as i64;
        assert!(catalog.position_of(TABLE, 1025).is_some());
        for key in [rows, rows + 1, 1100, 1279, 1280, 1 << 20, -1] {
            if key == 1025 {
                continue;
            }
            assert_eq!(catalog.position_of(TABLE, key), None, "key {key}");
        }
    }
}

fn touching_feed(captured: &Captured<'_>) -> Value {
    let row: Vec<&Value> = captured.columns.iter().map(|c| &c["values"][1]).collect();
    json!({"tables": {TABLE: {"upsert": [row], "delete": []}}})
}

#[test]
fn a_selection_is_refused_once_a_feed_touched_its_table() {
    let data = capture();
    let entry = &data["sizes"]["1025"];
    let Some(mut catalog) = catalog_of(entry) else {
        return;
    };
    let captured = Captured::of(entry);
    let parents = edge_positions(&catalog, &captured).parents;
    let sel = catalog
        .issue_selection(TABLE, parents.clone())
        .expect("issue");

    let bound = catalog.bind().expect("bind");
    let (_, rows) = bound.read_selection(&catalog, TABLE, sel).expect("read");
    assert_eq!(rows.len(), parents.len());

    catalog.apply_feed(&touching_feed(&captured)).expect("feed");

    match bound.read_selection(&catalog, TABLE, sel) {
        Err(Refusal::Stale { table, .. }) => assert_eq!(table, TABLE),
        other => panic!("the old binding still reads: {other:?}"),
    }
    let fresh = catalog.bind().expect("rebind");
    match fresh.read_selection(&catalog, TABLE, sel) {
        Err(Refusal::Bind(message)) => {
            assert!(message.contains("SelectionUnbound"), "{message}");
            println!("REFUSED {message}");
        }
        Ok((_, rows)) => panic!("the old selection read {} rows", rows.len()),
        Err(other) => panic!("{other:?}"),
    }

    let renewed = catalog
        .issue_selection(TABLE, parents.clone())
        .expect("issue");
    let (_, rows) = fresh
        .read_selection(&catalog, TABLE, renewed)
        .expect("a selection issued after the feed reads");
    assert_eq!(rows.len(), parents.len());
}

#[test]
fn a_selection_beyond_the_rows_is_refused_at_issue() {
    let data = capture();
    let entry = &data["sizes"]["1025"];
    let mut catalog = Catalog::from_capture(entry).expect("catalog");
    let rows = catalog
        .tables
        .iter()
        .find(|t| t.name == TABLE)
        .expect("table")
        .rows;
    let beyond = u32::try_from(rows).expect("rows");
    assert!(catalog.issue_selection(TABLE, vec![1, beyond]).is_err());
    assert!(catalog.issue_selection(TABLE, vec![1, beyond - 1]).is_ok());
    assert!(catalog.issue_selection("no_such_table", vec![]).is_err());
}

/// A timestamp lane is spelled by the shared contract exactly as PostgreSQL
/// prints the `timestamp(3) without time zone` column it was captured from,
/// NULLs included. No class binding is needed: the cell is read from the
/// table's own lane.
#[test]
fn a_timestamp_lane_prints_what_postgresql_prints() {
    let data = capture();
    let entry = &data["sizes"]["1025"];
    let catalog = Catalog::from_capture(entry).expect("catalog");
    let pg = match Pg::connect() {
        Ok(pg) => pg,
        Err(skip) => {
            assert!(
                !require_pg(),
                "VROW_REQUIRE_PG=1 and there is no server ({}): a SKIP is not a pass",
                skip.0
            );
            println!("SKIP: {}", skip.0);
            return;
        }
    };
    let captured = Captured::of(entry);
    load(&pg, &captured);

    let table = catalog
        .tables
        .iter()
        .find(|t| t.name == TABLE)
        .expect("table");
    let timestamps: Vec<(&str, &str)> = table
        .columns
        .iter()
        .filter(|c| {
            c.sql_type == "timestamp(3) without time zone"
                && c.lane
                    .as_ref()
                    .is_some_and(|lane| lane.kind == quack_shadow::LaneKind::Timestamp)
        })
        .map(|c| (c.name.as_str(), c.sql_type.as_str()))
        .collect();
    assert!(!timestamps.is_empty(), "the table has a laned timestamp column");
    for (name, sql_type) in timestamps {
        let rows: Vec<Row> = (0..table.rows)
            .filter(|&at| is_live(&catalog, at))
            .map(|at| vec![catalog.spell_cell(TABLE, name, at).expect("spelled")])
            .collect();
        assert!(
            rows.iter().any(|row| row[0].is_some()),
            "{name}: some cell is not NULL"
        );
        let ours = Rows {
            types: vec![sql_type.to_owned()],
            rows,
        };
        let theirs = pg.query(&format!("SELECT {name} FROM {TABLE}"));
        match compare(&theirs, &Answer::Rows(ours), Order::Unordered) {
            Outcome::Pass => println!("GATE timestamp {name}: Pass"),
            Outcome::Fail(diff) => panic!("{name}: {diff}"),
            other => panic!("{name}: {other:?}"),
        }
    }
}
