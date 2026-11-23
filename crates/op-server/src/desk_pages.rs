//! The desk the read-only pages read from: a polyfill store plus a Quack
//! catalog over OpenProject's own schema (`op_desk::desk::Desk`).
//!
//! The pages that also write (`/`, `/projects`, `/work_packages`) still read
//! PostgreSQL. They move together with desk writes, because a page that reads
//! the desk while its form writes PostgreSQL would never show an edit
//! (docs/plans/polyfill-quack-migration.md).

use std::sync::{Arc, Mutex};

use op_desk::desk::Desk;
use serde_json::Value as J;

/// The desk as `AppState` holds it. `Desk` is `Send` but not `Sync` (the Quack
/// catalog keeps `Cell`/`RefCell` caches), so it sits behind a mutex. Reads are
/// synchronous, and the lock is never held across an `.await`.
pub type SharedDesk = Arc<Mutex<Desk>>;

// `Desk` must be `Send` to live in `AppState` behind a `Mutex`.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Desk>();
};

/// The env variable naming a seed file to load instead of the demo seed.
pub const SEED_VAR: &str = "OP_DESK_SEED";

/// The desk seeded from the file `OP_DESK_SEED` names, else from the demo seed.
/// A seed that cannot be read or loaded stops the start.
pub fn load_desk(env: impl Fn(&str) -> Option<String>) -> Result<Desk, String> {
    let (origin, text) = match env(SEED_VAR) {
        Some(path) => (
            path.clone(),
            std::fs::read_to_string(&path).map_err(|e| format!("{SEED_VAR}={path}: {e}"))?,
        ),
        None => ("demo seed".to_string(), op_desk::DEMO_SEED.to_string()),
    };
    let doc: J = serde_json::from_str(&text).map_err(|e| format!("{origin}: {e}"))?;
    Desk::from_seed(&doc).map_err(|e| format!("{origin}: {e}"))
}

/// The desk shared by the server.
pub fn shared(desk: Desk) -> SharedDesk {
    Arc::new(Mutex::new(desk))
}

/// A Quack cell as display text. The cell forms are polyfill-feed's:
/// `{"t","epoch","usec"}` (a timestamp), `{"other":"Date","s"}`, `{"f"}` (a
/// float), `{"other":"BigDecimal","s"}`; integers, booleans and strings as
/// themselves; `null` as an absent value. Anything else is shown as compact JSON
/// rather than refused.
pub fn cell_text(cell: &J) -> String {
    match cell {
        J::Null => "—".to_string(),
        J::Bool(true) => "yes".to_string(),
        J::Bool(false) => "no".to_string(),
        J::Number(n) => n.to_string(),
        J::String(s) => s.clone(),
        J::Object(o) => {
            if let Some(J::String(t)) = o.get("t") {
                // `YYYY-MM-DDTHH:MM:SS.uuuuuuZ`, shown to the second.
                return match t.split_once('.') {
                    Some((secs, _)) => format!("{secs}Z"),
                    None => t.clone(),
                };
            }
            if let Some(J::String(f)) = o.get("f") {
                return f.clone();
            }
            match (o.get("other").and_then(J::as_str), o.get("s")) {
                (Some("Date" | "BigDecimal"), Some(J::String(s))) => s.clone(),
                _ => cell.to_string(),
            }
        }
        J::Array(_) => cell.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_cell_form_has_display_text() {
        assert_eq!(cell_text(&J::Null), "—");
        assert_eq!(cell_text(&json!(true)), "yes");
        assert_eq!(cell_text(&json!(false)), "no");
        assert_eq!(cell_text(&json!(42)), "42");
        assert_eq!(cell_text(&json!("In progress")), "In progress");
        assert_eq!(
            cell_text(&json!({"t": "2026-07-08T10:38:44.123456Z", "epoch": 1, "usec": 123456})),
            "2026-07-08T10:38:44Z"
        );
        assert_eq!(cell_text(&json!({"other": "Date", "s": "2026-07-01"})), "2026-07-01");
        assert_eq!(cell_text(&json!({"f": "3.5"})), "3.5");
        assert_eq!(
            cell_text(&json!({"other": "BigDecimal", "s": "12.3456"})),
            "12.3456"
        );
        // An unknown shape is shown, not refused.
        assert_eq!(cell_text(&json!({"x": 1})), r#"{"x":1}"#);
        assert_eq!(cell_text(&json!([1, 2])), "[1,2]");
    }

    #[test]
    fn the_demo_desk_loads_with_its_default_scopes() {
        let desk = load_desk(|_| None).expect("demo desk");
        // Seven users seeded; Principal's default scope hides the deleted one.
        assert_eq!(desk.count("users"), Ok(6));
        let statuses = desk.read_all("statuses").expect("statuses");
        assert!(!statuses.rows.is_empty());
    }

    #[test]
    fn a_seed_file_replaces_the_demo_seed_and_a_bad_one_stops_the_start() {
        let dir = std::env::temp_dir().join(format!("op-desk-seed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let good = dir.join("good.json");
        // One real status row from the demo seed, and nothing else.
        let demo: J = serde_json::from_str(op_desk::DEMO_SEED).expect("demo seed");
        let row = demo["seed"]["statuses"][0].clone();
        std::fs::write(&good, json!({"seed": {"statuses": [row]}}).to_string()).expect("write");
        let path = good.to_string_lossy().into_owned();
        let desk = load_desk(|k| (k == SEED_VAR).then(|| path.clone())).expect("file seed");
        assert_eq!(desk.count("statuses"), Ok(1));
        assert_eq!(desk.count("users"), Ok(0), "the demo seed is not loaded as well");

        let bad = dir.join("bad.json");
        std::fs::write(&bad, r#"{"seed": {"no_such_table": []}}"#).expect("write");
        let path = bad.to_string_lossy().into_owned();
        let Err(err) = load_desk(|k| (k == SEED_VAR).then(|| path.clone())) else {
            panic!("an unknown table stops the start");
        };
        assert!(err.contains("no_such_table"), "{err}");

        let missing = "/nonexistent/seed.json".to_string();
        assert!(load_desk(|k| (k == SEED_VAR).then(|| missing.clone())).is_err());
    }
}
