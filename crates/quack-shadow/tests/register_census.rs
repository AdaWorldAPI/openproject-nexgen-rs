// Copyright (C) 2012-2026 Zammad Foundation, https://zammad-foundation.org/

//! What each recorded call cost on the bound read path, counted and never
//! timed: lane checks against the tables read, key resolutions, and the order
//! of a join's key lookups against its folds. The counters change no answer;
//! every call is still equal to PostgreSQL's.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use quack_shadow::{Answer, Catalog, Step, Verdict, shadow_verdict, shadow_verdict_bound};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Ordinal,
    Keyed,
    Join,
    Refused,
}

impl Class {
    fn name(&self) -> &'static str {
        match self {
            Class::Ordinal => "ordinal",
            Class::Keyed => "keyed",
            Class::Join => "join",
            Class::Refused => "refused",
        }
    }
}

struct Rec {
    n: usize,
    label: String,
    class: Class,
    refusal: Option<String>,
    equal: bool,
    returned: Option<usize>,
    literal_keys: usize,
    checks: u64,
    lane_requests: u64,
    bound_requests: u64,
    tables_read: Vec<String>,
    unchecked_tables: Vec<String>,
    folds: u64,
    map_lookups: u64,
    key_resolutions: u64,
    key_as_position: u64,
    resolved_before_last_fold: u64,
    resolved_after_last_fold: u64,
    ordinal_reads: u64,
    ordinal_reads_unchecked: u64,
    bulk_reads_unchecked: u64,
    source_frames: u64,
    source_lane_of: u64,
    derived_frame: u64,
    derived_route: u64,
    derived_schema: u64,
    derived_mapping: u64,
    derived_space_scope: u64,
    expected_lane_of: u64,
    order: String,
}

impl Rec {
    fn join_form(&self) -> &'static str {
        match (self.key_as_position > 0, self.key_resolutions > 0) {
            (true, true) => "mixed",
            (true, false) => "key-as-position",
            (false, true) => "map",
            (false, false) => "none",
        }
    }

    fn json(&self) -> Value {
        let mut out = json!({
            "n": self.n,
            "label": self.label,
            "class": self.class.name(),
            "equal": self.equal,
            "checks": self.checks,
            "lane_requests": self.lane_requests,
            "lane_requests_on_class_bound_tables": self.bound_requests,
            "tables_read": self.tables_read,
            "tables_read_count": self.tables_read.len(),
            "tables_read_without_a_lane_check": self.unchecked_tables,
            "folds": self.folds,
            "map_lookups": self.map_lookups,
            "key_resolutions": self.key_resolutions,
            "key_as_position": self.key_as_position,
            "ordinal_reads": self.ordinal_reads,
            "ordinal_reads_without_a_check": self.ordinal_reads_unchecked,
            "bulk_lane_reads_without_a_check": self.bulk_reads_unchecked,
            "literal_keys": self.literal_keys,
            "returned_rows": self.returned,
            "source_callbacks": {
                "frames": self.source_frames,
                "lane_of": self.source_lane_of,
            },
            "check_parts_derived_from_dt64_source": {
                "frame": self.derived_frame,
                "route": self.derived_route,
                "schema_and_basis": self.derived_schema,
                "field_mapping": self.derived_mapping,
                "space_and_scope": self.derived_space_scope,
            },
            "order": self.order,
        });
        if let Some(refusal) = &self.refusal {
            out["refusal"] = json!(refusal);
        }
        if self.class == Class::Join {
            out["join"] = json!({
                "form": self.join_form(),
                "resolved_before_the_last_fold": self.resolved_before_last_fold,
                "resolved_after_the_last_fold": self.resolved_after_last_fold,
            });
        }
        out
    }
}

/// The literal values a call's WHERE compares with.
fn literal_keys(node: &Value) -> usize {
    let own = match node.get("op").and_then(Value::as_str) {
        Some("eq" | "ne") => usize::from(node.get("value").is_some_and(|v| !v.is_null())),
        Some("in" | "not_in") => node
            .get("values")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        _ => 0,
    };
    let args = node
        .get("args")
        .and_then(Value::as_array)
        .map_or(0, |a| a.iter().map(literal_keys).sum());
    own + args + node.get("arg").map_or(0, literal_keys)
}

fn step_text(step: Step, count: u64) -> String {
    match step {
        Step::Read(t) => format!("R{t}"),
        Step::Check(t) => format!("C{t}"),
        Step::Fold(t) => format!("F{t}"),
        Step::ResolveMap => format!("m{count}"),
        Step::ResolveKey => format!("k{count}"),
    }
}

struct Run {
    recs: Vec<Rec>,
}

/// Replays a recorded run and counts what every call did.
fn census(file: &str, append: bool) -> Option<Run> {
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
    let mut catalog = if append {
        Catalog::new_append()
    } else {
        Catalog::new()
    };
    let classes = common::class_map(&common::models_of(&calls))?;
    catalog.set_classes(classes).expect("classes");
    let mut mark = String::new();
    let mut recs = Vec::new();
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let item: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("line {}: {e}", n + 1));
        if let Some(feed) = item.get("feed") {
            catalog
                .apply_feed(feed)
                .unwrap_or_else(|e| panic!("line {}: {e}", n + 1));
            continue;
        }
        if let Some(text) = item.get("mark").and_then(Value::as_str) {
            mark = text.to_owned();
            continue;
        }
        let terminal = item["terminal"].as_str().unwrap_or("?");
        let table = item["table"].as_str().unwrap_or("?");
        let at = recs.len();
        let bound = catalog.bind();
        let (class_bound, bound_fields): (Vec<bool>, Vec<usize>) = match &bound {
            Ok(b) => {
                let report = b.report(&catalog);
                catalog
                    .tables
                    .iter()
                    .map(|t| {
                        let found = report.iter().find(|r| r.table == t.name);
                        (
                            found.is_some_and(|r| r.class.is_some()),
                            found.map_or(0, |r| r.bound.len() * usize::from(r.class.is_some())),
                        )
                    })
                    .unzip()
            }
            Err(_) => (Vec::new(), Vec::new()),
        };
        let before = catalog.census_mark();
        let verdict = match &bound {
            Ok(b) => shadow_verdict_bound(&catalog, b, &item, false),
            Err(refusal) => Verdict::Refused(refusal.clone()),
        };
        let delta = catalog.census_since(&before);
        assert_eq!(
            verdict,
            shadow_verdict(&catalog, &item, false),
            "[{mark}] {}: the counted read answers differently",
            item["sql"]
        );
        let (equal, refusal, returned) = match &verdict {
            Verdict::Equal(answer) => (
                true,
                None,
                match answer {
                    Answer::Rows(rows) => Some(*rows),
                    _ => None,
                },
            ),
            Verdict::Differ { quack, postgres } => panic!(
                "[{mark}] {}: quack {quack:?} but PostgreSQL {postgres}",
                item["sql"]
            ),
            Verdict::Refused(r) => (false, Some(format!("{r:?}")), None),
        };
        let joins = item["joins"].as_i64().unwrap_or(0) > 0;
        let key_steps = delta.key_resolutions + delta.key_as_position;
        let class = if refusal.is_some() {
            Class::Refused
        } else if joins {
            Class::Join
        } else if key_steps > 0 {
            Class::Keyed
        } else {
            Class::Ordinal
        };

        let mut read: BTreeSet<usize> = BTreeSet::new();
        let mut checked: BTreeSet<usize> = BTreeSet::new();
        let (mut derived, mut expected_lane_of) = ([0u64; 5], 0u64);
        let last_fold = delta
            .steps
            .iter()
            .rposition(|(s, _)| matches!(s, Step::Fold(_)));
        let (mut before_fold, mut after_fold) = (0, 0);
        let (mut lane_requests, mut bound_requests) = (0u64, 0u64);
        let mut order = Vec::new();
        for (i, &(step, count)) in delta.steps.iter().enumerate() {
            order.push(step_text(step, count));
            match step {
                Step::Read(t) => {
                    read.insert(t);
                    lane_requests += count;
                    if class_bound.get(t).copied().unwrap_or(false) {
                        bound_requests += count;
                    }
                }
                Step::Check(t) => {
                    checked.insert(t);
                    let fields = bound_fields.get(t).copied().unwrap_or(0) as u64 * count;
                    derived[0] += (2 * count) + fields;
                    derived[1] += count;
                    derived[2] += (2 * count) + 2 * fields;
                    derived[3] += fields;
                    derived[4] += 2 * fields;
                    expected_lane_of += fields;
                }
                Step::ResolveMap | Step::ResolveKey => match last_fold {
                    Some(f) if i < f => before_fold += count,
                    _ => after_fold += count,
                },
                Step::Fold(_) => {}
            }
        }
        let name = |t: &usize| {
            catalog
                .tables
                .get(*t)
                .map_or_else(|| format!("#{t}"), |t| t.name.clone())
        };
        recs.push(Rec {
            n: at,
            label: format!("#{at:03} [{mark}] {terminal} {table}"),
            class,
            refusal,
            equal,
            returned,
            literal_keys: literal_keys(&item["where"]),
            checks: delta.checks,
            lane_requests,
            bound_requests,
            tables_read: read.iter().map(name).collect(),
            unchecked_tables: read.difference(&checked).map(name).collect(),
            folds: delta.folds,
            map_lookups: delta.map_lookups,
            key_resolutions: delta.key_resolutions,
            key_as_position: delta.key_as_position,
            resolved_before_last_fold: before_fold,
            resolved_after_last_fold: after_fold,
            ordinal_reads: delta.ordinal_reads,
            ordinal_reads_unchecked: delta.ordinal_reads_unchecked,
            bulk_reads_unchecked: delta.bulk_reads_unchecked,
            source_frames: delta.source_frames,
            source_lane_of: delta.source_lane_of,
            derived_frame: derived[0],
            derived_route: derived[1],
            derived_schema: derived[2],
            derived_mapping: derived[3],
            derived_space_scope: derived[4],
            expected_lane_of,
            order: order.join(" "),
        });
    }
    Some(Run { recs })
}

fn sum(recs: &[&Rec], f: impl Fn(&Rec) -> u64) -> u64 {
    recs.iter().map(|r| f(r)).sum()
}

fn summary(run: &Run) -> Value {
    let recs: Vec<&Rec> = run.recs.iter().collect();
    let mut by_class: BTreeMap<&str, usize> = BTreeMap::new();
    for r in &recs {
        *by_class.entry(r.class.name()).or_default() += 1;
    }
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    for r in &recs {
        *histogram
            .entry(format!(
                "checks={} tables_read={}",
                r.checks,
                r.tables_read.len()
            ))
            .or_default() += 1;
    }
    let extra: Vec<&str> = recs
        .iter()
        .filter(|r| r.checks > r.tables_read.len() as u64)
        .map(|r| r.label.as_str())
        .collect();
    let per_row: Vec<&str> = recs
        .iter()
        .filter(|r| r.key_resolutions + r.key_as_position > 0)
        .map(|r| r.label.as_str())
        .collect();
    json!({
        "calls": recs.len(),
        "equal": recs.iter().filter(|r| r.equal).count(),
        "refused": recs.iter().filter(|r| !r.equal).count(),
        "by_class": by_class,
        "checks_against_tables_read": histogram,
        "calls_with_more_checks_than_tables_read": extra.len(),
        "calls_with_key_resolutions": per_row.len(),
        "totals": {
            "checks": sum(&recs, |r| r.checks),
            "lane_requests": sum(&recs, |r| r.lane_requests),
            "lane_requests_on_class_bound_tables": sum(&recs, |r| r.bound_requests),
            "calls_reading_a_table_without_a_lane_check": recs.iter().filter(|r| !r.unchecked_tables.is_empty()).count(),
            "calls_checking_a_table_more_than_once": recs.iter().filter(|r| r.bound_requests > r.tables_read.len() as u64 - r.unchecked_tables.len() as u64).count(),
            "joins_resolving_before_the_last_fold": recs.iter().filter(|r| r.class == Class::Join && r.resolved_before_last_fold > 0).count(),
            "resolved_before_the_last_fold": sum(&recs, |r| r.resolved_before_last_fold),
            "resolved_after_the_last_fold": sum(&recs, |r| r.resolved_after_last_fold),
            "tables_read": recs.iter().map(|r| r.tables_read.len() as u64).sum::<u64>(),
            "folds": sum(&recs, |r| r.folds),
            "map_lookups": sum(&recs, |r| r.map_lookups),
            "key_resolutions": sum(&recs, |r| r.key_resolutions),
            "key_as_position": sum(&recs, |r| r.key_as_position),
            "ordinal_reads": sum(&recs, |r| r.ordinal_reads),
            "ordinal_reads_without_a_check": sum(&recs, |r| r.ordinal_reads_unchecked),
            "bulk_lane_reads_without_a_check": sum(&recs, |r| r.bulk_reads_unchecked),
            "check_parts_derived_from_dt64_source": {
                "frame": sum(&recs, |r| r.derived_frame),
                "route": sum(&recs, |r| r.derived_route),
                "schema_and_basis": sum(&recs, |r| r.derived_schema),
                "field_mapping": sum(&recs, |r| r.derived_mapping),
                "space_and_scope": sum(&recs, |r| r.derived_space_scope),
            },
            "source_callbacks": {
                "frames": sum(&recs, |r| r.source_frames),
                "lane_of": sum(&recs, |r| r.source_lane_of),
            },
        },
    })
}

fn write_census(runs: &[(&str, &str, &Run)]) {
    let mut doc = serde_json::Map::new();
    doc.insert(
        "about".to_owned(),
        json!("Per recorded call, from what the bound read path did: lane checks, tables read, key resolutions, order of a join's key lookups against its folds. Counted, never timed. check_parts_derived_from_dt64_source are read off LaneBinding::check in lance-graph-dt64, not measured: only the source callbacks (frames, lane_of) are observable from zammad-rs."),
    );
    for &(name, file, run) in runs {
        doc.insert(
            name.to_owned(),
            json!({
                "file": file,
                "summary": summary(run),
                "calls": run.recs.iter().map(Rec::json).collect::<Vec<_>>(),
            }),
        );
    }
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harvest/register-census-zammad.json"
    );
    let mut text = serde_json::to_string_pretty(&Value::Object(doc)).expect("census json");
    text.push('\n');
    std::fs::write(path, text).expect("write census");
}

fn print_run(name: &str, run: &Run) {
    let s = summary(run);
    println!("CENSUS {name} {}", s["by_class"]);
    println!(
        "CENSUS {name} checks against tables read {}",
        s["checks_against_tables_read"]
    );
    println!("CENSUS {name} totals {}", s["totals"]);
    for r in &run.recs {
        if r.checks > r.tables_read.len() as u64 {
            println!(
                "CENSUS {name} EXTRA CHECKS {} checks={} tables={:?} order={}",
                r.label, r.checks, r.tables_read, r.order
            );
        }
        if r.key_resolutions + r.key_as_position > 0 {
            println!(
                "CENSUS {name} KEYS {} class={} form={} map={} key={} before_fold={} after_fold={} literals={} returned={:?} map_lookups={}",
                r.label,
                r.class.name(),
                r.join_form(),
                r.key_resolutions,
                r.key_as_position,
                r.resolved_before_last_fold,
                r.resolved_after_last_fold,
                r.literal_keys,
                r.returned,
                r.map_lookups
            );
        }
    }
}

/// Pinned from a measured run; see the CENSUS lines.
const EXPECTED: &str = r#"{
  "scenario": {
    "by_class": {
      "join": 8,
      "ordinal": 57
    },
    "calls": 65,
    "calls_with_key_resolutions": 0,
    "calls_with_more_checks_than_tables_read": 0,
    "checks_against_tables_read": {
      "checks=0 tables_read=1": 8,
      "checks=1 tables_read=1": 57
    },
    "equal": 65,
    "refused": 0,
    "totals": {
      "bulk_lane_reads_without_a_check": 32,
      "calls_checking_a_table_more_than_once": 0,
      "calls_reading_a_table_without_a_lane_check": 8,
      "check_parts_derived_from_dt64_source": {
        "field_mapping": 1502,
        "frame": 1616,
        "route": 57,
        "schema_and_basis": 3118,
        "space_and_scope": 3004
      },
      "checks": 57,
      "folds": 65,
      "joins_resolving_before_the_last_fold": 0,
      "key_as_position": 0,
      "key_resolutions": 0,
      "lane_requests": 73,
      "lane_requests_on_class_bound_tables": 57,
      "map_lookups": 0,
      "ordinal_reads": 1522,
      "ordinal_reads_without_a_check": 1522,
      "resolved_after_the_last_fold": 0,
      "resolved_before_the_last_fold": 0,
      "source_callbacks": {
        "frames": 57,
        "lane_of": 1502
      },
      "tables_read": 65
    }
  },
  "wide": {
    "by_class": {
      "join": 57,
      "ordinal": 455
    },
    "calls": 512,
    "calls_with_key_resolutions": 57,
    "calls_with_more_checks_than_tables_read": 0,
    "checks_against_tables_read": {
      "checks=1 tables_read=1": 455,
      "checks=1 tables_read=4": 57
    },
    "equal": 512,
    "refused": 0,
    "totals": {
      "bulk_lane_reads_without_a_check": 228,
      "calls_checking_a_table_more_than_once": 0,
      "calls_reading_a_table_without_a_lane_check": 57,
      "check_parts_derived_from_dt64_source": {
        "field_mapping": 12058,
        "frame": 13082,
        "route": 512,
        "schema_and_basis": 25140,
        "space_and_scope": 24116
      },
      "checks": 512,
      "folds": 683,
      "joins_resolving_before_the_last_fold": 0,
      "key_as_position": 546,
      "key_resolutions": 0,
      "lane_requests": 740,
      "lane_requests_on_class_bound_tables": 512,
      "map_lookups": 0,
      "ordinal_reads": 29910,
      "ordinal_reads_without_a_check": 29910,
      "resolved_after_the_last_fold": 546,
      "resolved_before_the_last_fold": 0,
      "source_callbacks": {
        "frames": 512,
        "lane_of": 12058
      },
      "tables_read": 683
    }
  },
  "wide_append": {
    "by_class": {
      "join": 57,
      "ordinal": 455
    },
    "calls": 512,
    "calls_with_key_resolutions": 57,
    "calls_with_more_checks_than_tables_read": 0,
    "checks_against_tables_read": {
      "checks=1 tables_read=1": 455,
      "checks=1 tables_read=4": 57
    },
    "equal": 512,
    "refused": 0,
    "totals": {
      "bulk_lane_reads_without_a_check": 228,
      "calls_checking_a_table_more_than_once": 0,
      "calls_reading_a_table_without_a_lane_check": 57,
      "check_parts_derived_from_dt64_source": {
        "field_mapping": 12058,
        "frame": 13082,
        "route": 512,
        "schema_and_basis": 25140,
        "space_and_scope": 24116
      },
      "checks": 512,
      "folds": 683,
      "joins_resolving_before_the_last_fold": 0,
      "key_as_position": 0,
      "key_resolutions": 546,
      "lane_requests": 740,
      "lane_requests_on_class_bound_tables": 512,
      "map_lookups": 546,
      "ordinal_reads": 29910,
      "ordinal_reads_without_a_check": 29910,
      "resolved_after_the_last_fold": 546,
      "resolved_before_the_last_fold": 0,
      "source_callbacks": {
        "frames": 512,
        "lane_of": 12058
      },
      "tables_read": 683
    }
  }
}"#;

#[cfg(not(feature = "disable-sql-where"))]
#[test]
fn the_register_census_of_the_recorded_calls() {
    let sources = [
        (
            "scenario",
            "oracle/shadow-scenario.pg.ndjson",
            "shadow-scenario.pg.ndjson",
            false,
        ),
        (
            "wide",
            "oracle/shadow-scenario-wide.pg.ndjson",
            "shadow-scenario-wide.pg.ndjson",
            false,
        ),
        (
            "wide_append",
            "oracle/shadow-scenario-wide.pg.ndjson",
            "shadow-scenario-wide.pg.ndjson",
            true,
        ),
    ];
    let mut runs = Vec::new();
    for (name, path, file, append) in sources {
        let Some(run) = census(file, append) else {
            return;
        };
        print_run(name, &run);
        runs.push((name, path, run));
    }
    let listed: Vec<(&str, &str, &Run)> = runs.iter().map(|(n, p, r)| (*n, *p, r)).collect();
    write_census(&listed);

    for (name, _, run) in &runs {
        for r in run.recs.iter().filter(|r| r.class == Class::Ordinal) {
            assert_eq!(
                (r.key_resolutions, r.key_as_position),
                (0, 0),
                "{name} {}",
                r.label
            );
        }
        // Every lane request on a class-bound table is exactly one lane check.
        for r in &run.recs {
            assert_eq!(
                r.checks, r.bound_requests,
                "{name} {}: lane checks against lane requests on class-bound tables",
                r.label
            );
        }
        // The observable half of a check: one `frames` and one `lane_of` per bound field.
        for r in &run.recs {
            assert_eq!(r.source_frames, r.checks, "{name} {}: frames", r.label);
            assert_eq!(
                r.source_lane_of, r.expected_lane_of,
                "{name} {}: lane_of",
                r.label
            );
        }
        let expected: Value = serde_json::from_str(EXPECTED).expect("expected json");
        assert_eq!(summary(run), expected[*name], "{name}: census totals");
    }
}
