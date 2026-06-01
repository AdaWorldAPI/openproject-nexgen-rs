//! `extract_fields` — Rails class → [`Field`]s. Sprint C4 fanout slot B.
//!
//! Maps an `ActiveRecord` class to SPO [`Field`]s with no external parser:
//!
//! 1. **Baseline columns** — every `db/schema.rb` column on the class becomes a
//!    plain [`Field`] (no dependencies, not emitted by anything).
//! 2. **Derived/memoized attrs** — each instance variable assigned in a method
//!    body (`@total_hours ||= …`) becomes a [`Field`] `emitted_by` that method,
//!    whose `depends_on` is the set of association chains (`assoc.member`) the
//!    method reads.
//!
//! Zero external parser deps by design — relies only on the shared
//! [`crate::scan`] primitives plus an inlined association-chain finder.

use ruff_spo_triplet::Field;

use crate::scan;
use crate::RubyClass;

/// Build the [`Field`] list for a Rails class: schema columns first (in
/// declaration order), then derived/memoized attributes in method order.
///
/// See [`crate::extract`] and the module docs for the Rails→IR mapping.
pub(crate) fn extract_fields(class: &RubyClass) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();

    // 1. Baseline column fields — one per schema column, in order.
    for col in &class.columns {
        fields.push(Field {
            name: col.clone(),
            depends_on: Vec::new(),
            emitted_by: None,
        });
    }

    // 2. Derived/memoized fields — ivars assigned inside `def … end` blocks.
    for block in scan::def_blocks(&class.body_source) {
        let depends_on = association_chains(&block.body, &class.associations);
        for attr in scan::ivar_assignments(&block.body) {
            // A baseline column of the same name already covers this attr.
            if fields.iter().any(|f| f.name == attr) {
                continue;
            }
            fields.push(Field {
                name: attr,
                depends_on: depends_on.clone(),
                emitted_by: Some(block.name.clone()),
            });
        }
    }

    fields
}

/// Find association chains (`<assoc>.<member>`) read in a method body.
///
/// For each (comment-stripped) line and each declared `assoc`, look for `assoc`
/// at an identifier boundary immediately followed by `.` and an identifier.
/// Returns the dotted paths verbatim (`time_entries.hours`), de-duplicated in
/// first-seen order.
fn association_chains(body: &str, associations: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in body.lines() {
        let code = scan::strip_comment(raw);
        for assoc in associations {
            if assoc.is_empty() {
                continue;
            }
            for member in chain_members(code, assoc) {
                let path = format!("{assoc}.{member}");
                if !out.contains(&path) {
                    out.push(path);
                }
            }
        }
    }
    out
}

/// All `<member>` identifiers `m` such that `<assoc>.m` appears in `line` with
/// `assoc` standing as a whole identifier (not a substring of a longer name).
fn chain_members(line: &str, assoc: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let alen = assoc.len();
    let mut members = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = line[search_from..].find(assoc) {
        let start = search_from + rel;
        let end = start + alen;
        // Advance past this occurrence regardless of whether it matches, so a
        // failed boundary check can't loop forever.
        search_from = start + 1;

        // Left boundary: the char before `assoc` must not be an identifier char
        // (so `my_project` does not match the assoc `project`).
        if start > 0 && is_ident_byte(bytes[start - 1]) {
            continue;
        }
        // Must be followed immediately by a `.` then at least one ident char.
        if bytes.get(end) != Some(&b'.') {
            continue;
        }
        let member: String = line[end + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !member.is_empty() {
            members.push(member);
        }
    }
    members
}

/// Whether `b` is an ASCII identifier byte (`[A-Za-z0-9_]`). Identifiers in the
/// Rails subset we scan are ASCII, so a byte test is sufficient for boundaries.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_package() -> RubyClass {
        RubyClass {
            name: "WorkPackage".to_string(),
            body_source: "  belongs_to :project\n\
                          \x20 has_many :time_entries\n\
                          \n\
                          \x20 def compute_total_hours\n\
                          \x20   raise ActiveRecord::RecordInvalid unless status\n\
                          \x20   @total_hours ||= time_entries.hours\n\
                          \x20 end\n"
                .to_string(),
            associations: vec!["project".to_string(), "time_entries".to_string()],
            columns: vec![
                "subject".to_string(),
                "description".to_string(),
                "status_id".to_string(),
                "status".to_string(),
                "created_at".to_string(),
                "updated_at".to_string(),
            ],
        }
    }

    #[test]
    fn work_package_yields_columns_then_derived() {
        let fields = extract_fields(&work_package());
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "subject",
                "description",
                "status_id",
                "status",
                "created_at",
                "updated_at",
                "total_hours",
            ]
        );

        // Columns are bare baseline fields.
        for f in &fields[..6] {
            assert!(f.depends_on.is_empty());
            assert_eq!(f.emitted_by, None);
        }

        let total = fields.last().expect("total_hours field");
        assert_eq!(total.name, "total_hours");
        assert_eq!(total.depends_on, ["time_entries.hours"]);
        assert_eq!(total.emitted_by.as_deref(), Some("compute_total_hours"));
    }

    #[test]
    fn time_entry_yields_columns_only() {
        let class = RubyClass {
            name: "TimeEntry".to_string(),
            body_source: "  belongs_to :work_package\n  validates :hours, presence: true\n"
                .to_string(),
            associations: vec!["work_package".to_string(), "user".to_string()],
            columns: vec![
                "hours".to_string(),
                "work_package_id".to_string(),
                "created_at".to_string(),
                "updated_at".to_string(),
            ],
        };
        let fields = extract_fields(&class);
        assert_eq!(fields.len(), 4);
        assert!(fields.iter().all(|f| f.emitted_by.is_none() && f.depends_on.is_empty()));
    }

    #[test]
    fn chain_finder_respects_identifier_boundaries() {
        // `my_project` must NOT match the assoc `project`.
        assert!(chain_members("x = my_project.name", "project").is_empty());
        // Whole-identifier match returns the member.
        assert_eq!(chain_members("project.name", "project"), ["name"]);
        // Trailing `.` with no identifier is not a chain.
        assert!(chain_members("project.", "project").is_empty());
    }

    #[test]
    fn chain_finder_dedups_in_first_seen_order() {
        let body = "  a = time_entries.hours\n\
                    \x20 b = time_entries.minutes\n\
                    \x20 c = time_entries.hours\n";
        let chains = association_chains(body, &["time_entries".to_string()]);
        assert_eq!(chains, ["time_entries.hours", "time_entries.minutes"]);
    }
}
