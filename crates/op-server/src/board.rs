//! Kanban board page — server-rendered HTML at `/`.
//!
//! Renders the OGAR render-bake pipeline (`ClassView` × `FieldMask`
//! projected through `ogar_render_askama::render_list`, using the
//! `ogar_vocab::project_work_item` canonical concept) as a kanban-style
//! board: rows are work packages, grouped under a status header (ordered
//! by `Status::position`) so each status reads as a board "column"
//! within the render-bake list view. Real data comes from op-db via the
//! `Repository` trait against the shared `sqlx::PgPool`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Html;

use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, GroupHeader, RenderColumn,
    RowSource,
};
// `project` (the ogar_vocab canonical-class constructor) is aliased to
// `project_class` — the tests module below defines its own `fn project(...)`
// fake-row helper, and a bare `project` import would collide with it (same
// value namespace, same name, `use super::*;` would pull both in).
use ogar_vocab::{canonical_concept_id, project as project_class, project_work_item};

use op_db::repository::Repository;
use op_db::{
    PriorityRepository, ProjectRepository, ProjectRow, StatusRepository, StatusRow,
    TypeRepository, UserRepository,
};

use crate::health::AppState;

/// Owned per-row data, built BEFORE any `RowSource`/`CellSource` borrows
/// are taken. `RowSource<'a>`/`CellSource<'a>` borrow `&str` data, so the
/// owned strings (and this `Vec`) must outlive the `Vec<RowSource>` that
/// references them — hence two passes: build owned data, then build rows.
struct OwnedRow {
    id: u64,
    subject: String,
    wp_href: String,
    project: String,
    project_href: String,
    done: u8,
    status_label: String,
    group_count: u32,
    is_group_head: bool,
}

/// GET `/` — the kanban board.
pub async fn board_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("OpenProject RS — Board", "the board");
    };

    let statuses = StatusRepository::new(pool.clone())
        .find_all(200, 0)
        .await
        .unwrap_or_default();
    let wps = op_db::WorkPackageRepository::new(pool.clone())
        .find_all(500, 0)
        .await
        .unwrap_or_default();
    let projects = ProjectRepository::new(pool)
        .find_all(200, 0)
        .await
        .unwrap_or_default();

    let header = format!(
        "<header><h1>OpenProject RS — Board</h1><p>{} work packages · {} projects · {} statuses</p></header>",
        wps.len(),
        projects.len(),
        statuses.len(),
    );
    let fragment = render_board(&statuses, &wps, &projects);

    Html(page_shell(
        "OpenProject RS — Board",
        &format!("{header}<main>{fragment}</main>"),
    ))
}

/// Build the kanban board fragment from status/work-package/project rows.
///
/// Statuses are sorted by `position` (the OpenProject/Redmine board-column
/// order); work packages are grouped under their status so the first row
/// of each status carries a `GroupHeader` (rendered as the `tr.group`
/// separator by the `html_list_view` spine template) — that separator is
/// the kanban "column" boundary within the render-bake list view.
fn render_board(
    statuses: &[StatusRow],
    wps: &[op_db::work_packages::WorkPackageRow],
    projects: &[ProjectRow],
) -> String {
    let inline_columns: Vec<RenderColumn> = vec![
        RenderColumn::new("id", "#", ColumnKind::IdLink),
        RenderColumn::new("subject", "Subject", ColumnKind::PrimaryLink).sortable(),
        RenderColumn::new("project", "Project", ColumnKind::RecordRef),
        RenderColumn::new("done_ratio", "% Done", ColumnKind::ProgressBar),
    ];
    let block_columns: Vec<RenderColumn> = Vec::new();

    let project_names: HashMap<i64, String> =
        projects.iter().map(|p| (p.id, p.name.clone())).collect();

    let mut sorted_statuses: Vec<&StatusRow> = statuses.iter().collect();
    sorted_statuses.sort_by_key(|s| s.position);

    // Pass 1: owned data, in kanban (status, then work-package) order.
    let mut owned: Vec<OwnedRow> = Vec::new();
    for status in &sorted_statuses {
        let mut in_status: Vec<&op_db::work_packages::WorkPackageRow> =
            wps.iter().filter(|w| w.status_id == status.id).collect();
        in_status.sort_by_key(|w| w.id);
        let count = in_status.len() as u32;

        for (i, wp) in in_status.into_iter().enumerate() {
            let project = project_names
                .get(&wp.project_id)
                .cloned()
                .unwrap_or_else(|| format!("#{}", wp.project_id));
            owned.push(OwnedRow {
                id: wp.id as u64,
                subject: wp.subject.clone(),
                wp_href: format!("/work_packages/{}", wp.id),
                project,
                project_href: format!("/projects/{}", wp.project_id),
                done: wp.done_ratio.clamp(0, 100) as u8,
                status_label: status.name.clone(),
                group_count: count,
                is_group_head: i == 0,
            });
        }
    }

    // Pass 2: rows borrowing from `owned` + `inline_columns`.
    let rows: Vec<RowSource> = owned
        .iter()
        .map(|o| RowSource {
            record_id: o.id,
            css_classes: "",
            group: if o.is_group_head {
                Some(GroupHeader {
                    label: &o.status_label,
                    count: o.group_count,
                })
            } else {
                None
            },
            inline: vec![
                CellSource {
                    column: &inline_columns[0],
                    css_classes: "",
                    data: CellData::IdLink {
                        id: o.id,
                        href: &o.wp_href,
                    },
                },
                CellSource {
                    column: &inline_columns[1],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &o.subject,
                        href: &o.wp_href,
                    },
                },
                CellSource {
                    column: &inline_columns[2],
                    css_classes: "",
                    data: CellData::RecordRef {
                        label: &o.project,
                        href: &o.project_href,
                        target_concept: "project",
                    },
                },
                CellSource {
                    column: &inline_columns[3],
                    css_classes: "",
                    data: CellData::ProgressBar { pct: o.done },
                },
            ],
            block: vec![],
        })
        .collect();

    let class = project_work_item();
    let concept = class.canonical_concept.clone().unwrap_or_default();
    let class_id = canonical_concept_id(&concept).unwrap_or(0);

    render_list(
        "Work packages",
        class_id,
        &concept,
        &inline_columns,
        &block_columns,
        &rows,
    )
    .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>"))
}

/// Minimal HTML-escape for untrusted text interpolated into `headline_html`
/// / page `<title>` / `RichText` bodies — the only three spots the
/// render-bake spine does NOT auto-escape for us (see `html_detail_view.rs`
/// doc comment on `render_detail`'s `headline_html` param, and `cells.rs`'s
/// `RichTextCell` which is `escape = "none"` by design so pre-rendered
/// prose HTML can pass through). Order matters: `&` first.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Shared "database not connected" fallback page — used by the board and
/// both detail routes when `state.db` is `None`.
fn not_connected_page(title: &str, subject: &str) -> Html<String> {
    Html(page_shell(
        title,
        &format!(
            "<main><p class=\"empty\">Database not connected — {subject} needs a live DB pool.</p></main>"
        ),
    ))
}

/// A small breadcrumb back to the board, prepended above every detail page.
const BREADCRUMB: &str = "<p class=\"breadcrumb\"><a href=\"/\">← Board</a></p>";

// ── Work package detail ──────────────────────────────────────────────

/// GET `/work_packages/:id` — rendered detail page for one work package.
///
/// Resolves the work package's family-edge ids (status/project/type/
/// priority/author/assignee) to display names via their repositories,
/// then hands off to [`render_wp_detail`] (the pure, DB-less render path
/// covered by the unit test below).
pub async fn work_package_detail(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
) -> Html<String> {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("Work package", "this page");
    };

    let Some(wp) = op_db::WorkPackageRepository::new(pool.clone())
        .find_by_id(id)
        .await
        .ok()
        .flatten()
    else {
        return Html(page_shell(
            "Work package not found",
            &format!("<main><p class=\"empty\">Work package #{id} not found.</p></main>"),
        ));
    };

    let status_name = StatusRepository::new(pool.clone())
        .find_by_id(wp.status_id)
        .await
        .ok()
        .flatten()
        .map(|s: StatusRow| s.name)
        .unwrap_or_else(|| format!("#{}", wp.status_id));

    let project_name = ProjectRepository::new(pool.clone())
        .find_by_id(wp.project_id)
        .await
        .ok()
        .flatten()
        .map(|p: ProjectRow| p.name)
        .unwrap_or_else(|| format!("#{}", wp.project_id));

    let type_name = TypeRepository::new(pool.clone())
        .find_by_id(wp.type_id)
        .await
        .ok()
        .flatten()
        .map(|t| t.name)
        .unwrap_or_else(|| format!("#{}", wp.type_id));

    let priority_name = match wp.priority_id {
        Some(pid) => PriorityRepository::new(pool.clone())
            .find_by_id(pid)
            .await
            .ok()
            .flatten()
            .map(|p| p.name)
            .unwrap_or_else(|| format!("#{pid}")),
        None => "—".to_string(),
    };

    let author_name = UserRepository::new(pool.clone())
        .find_by_id(wp.author_id)
        .await
        .ok()
        .flatten()
        .map(|u| u.full_name())
        .unwrap_or_else(|| format!("#{}", wp.author_id));

    let assignee_name = match wp.assigned_to_id {
        Some(uid) => UserRepository::new(pool)
            .find_by_id(uid)
            .await
            .ok()
            .flatten()
            .map(|u| u.full_name())
            .unwrap_or_else(|| format!("#{uid}")),
        None => "—".to_string(),
    };

    let fragment = render_wp_detail(
        &wp,
        &status_name,
        &project_name,
        &type_name,
        &priority_name,
        &author_name,
        &assignee_name,
    );

    Html(page_shell(
        &format!("Work package #{id} — {}", esc(&wp.subject)),
        &format!("{BREADCRUMB}<main>{fragment}</main>"),
    ))
}

/// Render the work-package detail fragment (pure — no DB access), given
/// the row plus the already-resolved display names for its family-edge
/// fields (status/project/type/priority/author/assignee). Kept separate
/// from the async handler so it's testable without a live DB pool
/// (mirrors [`render_board`]'s split).
fn render_wp_detail(
    wp: &op_db::work_packages::WorkPackageRow,
    status_name: &str,
    project_name: &str,
    type_name: &str,
    priority_name: &str,
    author_name: &str,
    assignee_name: &str,
) -> String {
    let columns: Vec<RenderColumn> = vec![
        RenderColumn::new("id", "#", ColumnKind::IdLink),
        RenderColumn::new("status", "Status", ColumnKind::Plain),
        RenderColumn::new("project", "Project", ColumnKind::RecordRef),
        RenderColumn::new("type", "Type", ColumnKind::Plain),
        RenderColumn::new("priority", "Priority", ColumnKind::Plain),
        RenderColumn::new("author", "Author", ColumnKind::Plain),
        RenderColumn::new("assignee", "Assignee", ColumnKind::Plain),
        RenderColumn::new("start_date", "Start date", ColumnKind::Plain),
        RenderColumn::new("due_date", "Due date", ColumnKind::Plain),
        RenderColumn::new("estimated_hours", "Estimated hours", ColumnKind::Plain),
        RenderColumn::new("done_ratio", "% Done", ColumnKind::ProgressBar),
        RenderColumn::new("description", "Description", ColumnKind::RichText).block(),
    ];

    let wp_href = format!("/work_packages/{}", wp.id);
    let project_href = format!("/projects/{}", wp.project_id);
    let start_date = wp
        .start_date
        .map(|d| d.to_string())
        .unwrap_or_else(|| "—".to_string());
    let due_date = wp
        .due_date
        .map(|d| d.to_string())
        .unwrap_or_else(|| "—".to_string());
    let estimated_hours = wp
        .estimated_hours
        .map(|h| format!("{h:.2}"))
        .unwrap_or_else(|| "—".to_string());
    let description_html = format!(
        "<p>{}</p>",
        esc(wp.description.as_deref().unwrap_or("—"))
    );
    let done = wp.done_ratio.clamp(0, 100) as u8;

    let cells: Vec<CellSource<'_>> = vec![
        CellSource {
            column: &columns[0],
            css_classes: "",
            data: CellData::IdLink {
                id: wp.id as u64,
                href: &wp_href,
            },
        },
        CellSource {
            column: &columns[1],
            css_classes: "",
            data: CellData::Plain { value: status_name },
        },
        CellSource {
            column: &columns[2],
            css_classes: "",
            data: CellData::RecordRef {
                label: project_name,
                href: &project_href,
                target_concept: "project",
            },
        },
        CellSource {
            column: &columns[3],
            css_classes: "",
            data: CellData::Plain { value: type_name },
        },
        CellSource {
            column: &columns[4],
            css_classes: "",
            data: CellData::Plain {
                value: priority_name,
            },
        },
        CellSource {
            column: &columns[5],
            css_classes: "",
            data: CellData::Plain { value: author_name },
        },
        CellSource {
            column: &columns[6],
            css_classes: "",
            data: CellData::Plain {
                value: assignee_name,
            },
        },
        CellSource {
            column: &columns[7],
            css_classes: "",
            data: CellData::Plain { value: &start_date },
        },
        CellSource {
            column: &columns[8],
            css_classes: "",
            data: CellData::Plain { value: &due_date },
        },
        CellSource {
            column: &columns[9],
            css_classes: "",
            data: CellData::Plain {
                value: &estimated_hours,
            },
        },
        CellSource {
            column: &columns[10],
            css_classes: "",
            data: CellData::ProgressBar { pct: done },
        },
        CellSource {
            column: &columns[11],
            css_classes: "",
            data: CellData::RichText {
                body: &description_html,
            },
        },
    ];

    let class = project_work_item();
    let concept = class.canonical_concept.clone().unwrap_or_default();
    let class_id = canonical_concept_id(&concept).unwrap_or(0);

    let headline = esc(&wp.subject);
    let subtitle = format!("#{} · {status_name} · {project_name}", wp.id);

    render_detail(
        class_id,
        &concept,
        wp.id as u64,
        &headline,
        &subtitle,
        &columns,
        &cells,
    )
    .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>"))
}

// ── Project detail ───────────────────────────────────────────────────

/// GET `/projects/:id` — rendered detail page for one project.
pub async fn project_detail(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
) -> Html<String> {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("Project", "this page");
    };

    let Some(row) = ProjectRepository::new(pool)
        .find_by_id(id)
        .await
        .ok()
        .flatten()
    else {
        return Html(page_shell(
            "Project not found",
            &format!("<main><p class=\"empty\">Project #{id} not found.</p></main>"),
        ));
    };

    let fragment = render_project_detail(&row);

    Html(page_shell(
        &format!("Project — {}", esc(&row.name)),
        &format!("{BREADCRUMB}<main>{fragment}</main>"),
    ))
}

/// Render the project detail fragment (pure — no DB access). Kept
/// separate from the async handler so it's testable without a live DB
/// pool (mirrors [`render_wp_detail`] / [`render_board`]).
fn render_project_detail(row: &ProjectRow) -> String {
    let columns: Vec<RenderColumn> = vec![
        RenderColumn::new("id", "#", ColumnKind::IdLink),
        RenderColumn::new("identifier", "Identifier", ColumnKind::Plain),
        RenderColumn::new("public", "Public", ColumnKind::Plain),
        RenderColumn::new("active", "Active", ColumnKind::Plain),
        RenderColumn::new("created_at", "Created", ColumnKind::Plain),
        RenderColumn::new("description", "Description", ColumnKind::RichText).block(),
    ];

    let project_href = format!("/projects/{}", row.id);
    let public_label = if row.public { "yes" } else { "no" };
    let active_label = if row.active { "yes" } else { "no" };
    let created_label = row.created_at.to_rfc3339();
    let description_html = format!(
        "<p>{}</p>",
        esc(row.description.as_deref().unwrap_or("—"))
    );

    let cells: Vec<CellSource<'_>> = vec![
        CellSource {
            column: &columns[0],
            css_classes: "",
            data: CellData::IdLink {
                id: row.id as u64,
                href: &project_href,
            },
        },
        CellSource {
            column: &columns[1],
            css_classes: "",
            data: CellData::Plain {
                value: &row.identifier,
            },
        },
        CellSource {
            column: &columns[2],
            css_classes: "",
            data: CellData::Plain {
                value: public_label,
            },
        },
        CellSource {
            column: &columns[3],
            css_classes: "",
            data: CellData::Plain {
                value: active_label,
            },
        },
        CellSource {
            column: &columns[4],
            css_classes: "",
            data: CellData::Plain {
                value: &created_label,
            },
        },
        CellSource {
            column: &columns[5],
            css_classes: "",
            data: CellData::RichText {
                body: &description_html,
            },
        },
    ];

    let class = project_class();
    let concept = class.canonical_concept.clone().unwrap_or_default();
    let class_id = canonical_concept_id(&concept).unwrap_or(0);

    let headline = esc(&row.name);
    let subtitle = row.identifier.clone();

    render_detail(
        class_id,
        &concept,
        row.id as u64,
        &headline,
        &subtitle,
        &columns,
        &cells,
    )
    .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>"))
}

/// Wrap a rendered body fragment in a full, self-contained HTML page —
/// no external assets, readable in light and dark, kanban-ish styling on
/// top of the `html_list_view` spine markup (`.ogar-list`, `table.list`,
/// `tr.group`, `.progress`).
fn page_shell(title: &str, body_html: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  :root {{ color-scheme: light dark; }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    background: #f4f5f7;
    color: #172b4d;
  }}
  header {{ padding: 1rem 1.5rem; background: #0747a6; color: #fff; }}
  header h1 {{ margin: 0 0 0.25rem; font-size: 1.35rem; }}
  header p {{ margin: 0; font-size: 0.85rem; opacity: 0.85; }}
  main {{ padding: 1.25rem; overflow-x: auto; }}
  .empty {{ padding: 3rem 1rem; text-align: center; color: #6b778c; }}
  .breadcrumb {{ margin: 0; padding: 0.6rem 1.25rem 0; font-size: 0.85rem; }}
  .ogar-list h2 {{ font-size: 1.05rem; margin: 0 0 0.75rem; }}
  table.list {{
    width: 100%;
    border-collapse: collapse;
    background: #fff;
    box-shadow: 0 1px 2px rgba(9, 30, 66, 0.15);
    font-size: 0.88rem;
  }}
  table.list th {{
    text-align: left;
    padding: 0.5rem 0.75rem;
    background: #ebecf0;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    white-space: nowrap;
  }}
  table.list td {{ padding: 0.5rem 0.75rem; border-top: 1px solid #ebecf0; vertical-align: middle; }}
  table.list th.checkbox, table.list td.checkbox, table.list th.buttons, table.list td.buttons {{ width: 1%; }}
  tr.group td {{
    background: #dfe1e6;
    font-weight: 600;
    padding-top: 0.6rem;
    padding-bottom: 0.6rem;
  }}
  tr.group .badge {{
    display: inline-block;
    margin-left: 0.4rem;
    padding: 0 0.4rem;
    border-radius: 10px;
    background: #6b778c;
    color: #fff;
    font-size: 0.72rem;
    font-weight: 700;
  }}
  a {{ color: #0052cc; text-decoration: none; }}
  a:hover {{ text-decoration: underline; }}
  .record-id {{ font-variant-numeric: tabular-nums; color: #6b778c; }}
  .primary-link {{ font-weight: 600; }}
  .progress {{ position: relative; width: 100px; height: 10px; border-radius: 5px; background: #ebecf0; overflow: hidden; }}
  .progress .progress-bar {{ height: 100%; background: #36b37e; }}
  .progress .progress-label {{ position: absolute; left: 108px; top: -3px; font-size: 0.75rem; color: #6b778c; }}
  td.nodata {{ text-align: center; color: #6b778c; padding: 2rem; }}
  @media (prefers-color-scheme: dark) {{
    body {{ background: #1d2125; color: #b6c2cf; }}
    table.list {{ background: #22272b; }}
    table.list th {{ background: #2c333a; color: #9fadbc; }}
    table.list td {{ border-top-color: #3a3f45; }}
    tr.group td {{ background: #2c333a; }}
    a {{ color: #579dff; }}
    .record-id, .progress .progress-label {{ color: #9fadbc; }}
    .progress {{ background: #3a3f45; }}
  }}
</style>
</head>
<body>
{body_html}
</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn status(id: i64, name: &str, position: i32) -> StatusRow {
        StatusRow {
            id,
            name: name.to_string(),
            is_closed: false,
            is_default: false,
            is_readonly: false,
            position,
            default_done_ratio: 0,
            color_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn work_package(
        id: i64,
        subject: &str,
        project_id: i64,
        status_id: i64,
        done_ratio: i32,
    ) -> op_db::work_packages::WorkPackageRow {
        op_db::work_packages::WorkPackageRow {
            id,
            subject: subject.to_string(),
            description: None,
            project_id,
            type_id: 1,
            status_id,
            priority_id: None,
            author_id: 1,
            assigned_to_id: None,
            responsible_id: None,
            start_date: None,
            due_date: None,
            estimated_hours: None,
            done_ratio,
            parent_id: None,
            version_id: None,
            category_id: None,
            lock_version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn project(id: i64, name: &str) -> ProjectRow {
        ProjectRow {
            id,
            name: name.to_string(),
            description: None,
            identifier: name.to_lowercase(),
            public: true,
            parent_id: None,
            lft: 1,
            rgt: 2,
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Proves the render path works end-to-end without a DB: fake
    /// Status/WorkPackage/Project rows in, HTML containing a subject and
    /// a group (status) label out.
    #[test]
    fn render_board_groups_by_status_and_renders_subjects() {
        let statuses = vec![status(1, "New", 1), status(2, "In Progress", 2)];
        let wps = vec![
            work_package(101, "Fix the login bug", 10, 1, 0),
            work_package(102, "Write the board page", 10, 2, 40),
        ];
        let projects = vec![project(10, "Nexgen")];

        let html = render_board(&statuses, &wps, &projects);

        assert!(html.contains("Fix the login bug"), "{html}");
        assert!(html.contains("Write the board page"), "{html}");
        assert!(html.contains("In Progress"), "{html}");
        assert!(html.contains("New"), "{html}");
        assert!(html.contains("Nexgen"), "{html}");
    }

    /// Proves the detail-view render path (`render_detail` /
    /// `HtmlDetailView`) works end-to-end without a DB for a work
    /// package: fake row + resolved family-edge names in, HTML
    /// containing the subject, resolved labels, and (crucially) the
    /// rewritten `/work_packages/…` + `/projects/…` hrefs out — proving
    /// the board's link rewrite lands on a page that actually resolves.
    #[test]
    fn render_wp_detail_contains_fields() {
        let mut wp = work_package(101, "Fix the login bug", 10, 2, 40);
        wp.description = Some("Some rich description".to_string());
        wp.priority_id = Some(5);
        wp.assigned_to_id = Some(7);
        wp.estimated_hours = Some(3.5);

        let html = render_wp_detail(
            &wp,
            "In Progress",
            "Nexgen",
            "Bug",
            "High",
            "Jane Doe",
            "John Smith",
        );

        // Headline + subtitle
        assert!(html.contains("Fix the login bug"), "{html}");
        assert!(html.contains("In Progress"), "{html}");
        // Resolved family-edge labels
        assert!(html.contains("Nexgen"), "{html}");
        assert!(html.contains("Bug"), "{html}");
        assert!(html.contains("High"), "{html}");
        assert!(html.contains("Jane Doe"), "{html}");
        assert!(html.contains("John Smith"), "{html}");
        // Rich-text block (escaped, wrapped)
        assert!(html.contains("Some rich description"), "{html}");
        // Numeric fields
        assert!(html.contains("3.50"), "{html}");
        assert!(html.contains("aria-valuenow=\"40\""), "{html}");
        // The link rewrite: project cell links to /projects/{id}, NOT
        // /api/v3/projects/{id}.
        assert!(html.contains("href=\"/projects/10\""), "{html}");
        assert!(!html.contains("/api/v3/projects/10"), "{html}");
        // The record's own id link points at /work_packages/{id}.
        assert!(html.contains("/work_packages/101"), "{html}");
    }

    /// Same proof for the project detail page: fake `ProjectRow` in, HTML
    /// containing name/identifier/public/active/description out.
    #[test]
    fn render_project_detail_contains_fields() {
        let mut row = project(10, "Nexgen");
        row.description = Some("A cool project".to_string());
        row.public = false;

        let html = render_project_detail(&row);

        assert!(html.contains("Nexgen"), "{html}");
        assert!(html.contains("nexgen"), "{html}"); // identifier
        assert!(html.contains("A cool project"), "{html}");
        assert!(html.contains("no"), "{html}"); // public = false
        assert!(html.contains("yes"), "{html}"); // active = true (default)
        assert!(html.contains("/projects/10"), "{html}");
    }
}
