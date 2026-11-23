//! Read-only pages served from the desk (`AppState::desk`): one static
//! [`DeskView`] spec per concept, rendered through the same
//! `ogar_render_askama` list/detail templates and [`skin`] masks as the
//! PostgreSQL-backed pages.
//!
//! Column orders follow OpenProject's own views where they exist (cited per
//! spec), cut to the columns the desk table holds.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use ogar_vocab::canonical_concept_id;
use op_desk::desk::Rows;
use serde_json::{json, Value as J};

use super::{esc, page_shell, skin, BREADCRUMB};
use crate::desk_pages::cell_text;
use crate::health::AppState;

/// One desk-backed page pair (`<route>` list, `<route>/:id` detail).
pub struct DeskView {
    /// The nav concept (`nav::route_for` resolves it to `route`).
    pub concept: &'static str,
    /// ogar-vocab canonical concept, for the template's class id.
    pub canonical: &'static str,
    pub title: &'static str,
    pub singular: &'static str,
    pub route: &'static str,
    pub table: &'static str,
    /// An STI subset: only rows whose column equals the value.
    pub only: Option<(&'static str, &'static str)>,
    /// The headline column (the list's link text, the detail's title).
    pub headline: &'static str,
    pub list: &'static [&'static str],
    pub detail: &'static [&'static str],
    /// Foreign-key columns rendered as links to their target's page.
    pub links: &'static [DeskLink],
    /// Columns whose integer values have names (`users.status`).
    pub labels: &'static [(&'static str, &'static [(i64, &'static str)])],
}

/// A foreign-key column shown as a link: `<route>/<value>`, to `concept`'s
/// page. With `when`, only on rows whose column holds that value (a
/// polymorphic key such as `journable_id` links only for `WorkPackage`).
pub struct DeskLink {
    pub col: &'static str,
    pub route: &'static str,
    pub concept: &'static str,
    pub when: Option<(&'static str, &'static str)>,
}

const WORK_PACKAGE: (&str, &str) = ("/work_packages", "ProjectWorkItem");
const USER: (&str, &str) = ("/users", "ProjectActor");

const fn link(col: &'static str, to: (&'static str, &'static str)) -> DeskLink {
    DeskLink {
        col,
        route: to.0,
        concept: to.1,
        when: None,
    }
}

const fn link_when(
    col: &'static str,
    to: (&'static str, &'static str),
    when: (&'static str, &'static str),
) -> DeskLink {
    DeskLink {
        col,
        route: to.0,
        concept: to.1,
        when: Some(when),
    }
}

/// Statuses: list per `app/components/statuses/table_component.rb:40-48`
/// (done ratio shown unconditionally, no workflows column, raw values), detail
/// per `lib/api/v3/statuses/status_representer.rb:36-49` (minus the derived
/// `is_closed` and `color`).
pub const STATUSES: DeskView = DeskView {
    concept: "ProjectStatus",
    canonical: "project_status",
    title: "Statuses",
    singular: "Status",
    route: "/statuses",
    table: "statuses",
    only: None,
    headline: "name",
    list: &["default_done_ratio", "category", "is_readonly"],
    detail: &[
        "category",
        "is_default",
        "is_readonly",
        "excluded_from_totals",
        "default_done_ratio",
        "position",
    ],
    links: &[],
    labels: &[],
};

/// Types: OpenProject has no flat types table to follow (its admin index is
/// the work-package type variants UI), so the list is `name, position,
/// is_milestone`; detail per `lib/api/v3/types/type_representer.rb:39-59`
/// (minus `color` and the derived `is_default`).
pub const TYPES: DeskView = DeskView {
    concept: "ProjectType",
    canonical: "project_type",
    title: "Types",
    singular: "Type",
    route: "/types",
    table: "types",
    only: None,
    headline: "name",
    list: &["position", "is_milestone"],
    detail: &[
        "description",
        "position",
        "is_milestone",
        "created_at",
        "updated_at",
    ],
    links: &[],
    labels: &[],
};

/// Priorities (`IssuePriority`, an STI subset of `enumerations`): list per
/// `app/components/enumerations/table_component.rb:47-55` (its "sort" column is
/// reorder controls; this read-only page shows the `position` it edits, and
/// drops `color`), detail per `lib/api/v3/priorities/priority_representer.rb:39-46`.
pub const PRIORITIES: DeskView = DeskView {
    concept: "Priority",
    canonical: "priority",
    title: "Priorities",
    singular: "Priority",
    route: "/priorities",
    table: "enumerations",
    only: Some(("type", "IssuePriority")),
    headline: "name",
    list: &["is_default", "active", "position"],
    detail: &["position", "is_default", "active"],
    links: &[],
    labels: &[],
};

/// Users (ProjectActor): list per `app/components/users/table_component.rb:32`
/// (`login firstname lastname mail admin created_at last_login_on`), detail per
/// `lib/api/v3/users/user_representer.rb:107-171`. `status` is labelled per
/// `app/models/principal.rb:38-44`. Principal's default scope hides deleted
/// users (status 5).
pub const USERS: DeskView = DeskView {
    concept: "ProjectActor",
    canonical: "project_actor",
    title: "Users",
    singular: "User",
    route: "/users",
    table: "users",
    only: None,
    headline: "login",
    list: &[
        "firstname",
        "lastname",
        "mail",
        "admin",
        "created_at",
        "last_login_on",
    ],
    detail: &[
        "admin",
        "firstname",
        "lastname",
        "mail",
        "status",
        "identity_url",
        "language",
    ],
    links: &[],
    labels: &[(
        "status",
        &[
            (1, "active"),
            (2, "registered"),
            (3, "locked"),
            (4, "invited"),
            (5, "deleted"),
        ],
    )],
};

/// Journals (ProjectJournal): OpenProject shows journals only as a work
/// package's activity; columns per `lib/api/v3/activities/activity_representer.rb:75-92`
/// (`notes`, `version`, the journable and its user, `created_at`).
pub const JOURNALS: DeskView = DeskView {
    concept: "ProjectJournal",
    canonical: "project_journal",
    title: "Journals",
    singular: "Journal",
    route: "/journals",
    table: "journals",
    only: None,
    headline: "created_at",
    list: &["journable_type", "journable_id", "version", "user_id"],
    detail: &[
        "notes",
        "version",
        "journable_type",
        "journable_id",
        "user_id",
    ],
    links: &[
        link_when(
            "journable_id",
            WORK_PACKAGE,
            ("journable_type", "WorkPackage"),
        ),
        link("user_id", USER),
    ],
    labels: &[],
};

/// Relations (ProjectRelation): columns per
/// `lib/api/v3/relations/relation_representer.rb:54-79`.
pub const RELATIONS: DeskView = DeskView {
    concept: "ProjectRelation",
    canonical: "project_relation",
    title: "Relations",
    singular: "Relation",
    route: "/relations",
    table: "relations",
    only: None,
    headline: "relation_type",
    list: &["from_id", "to_id", "lag"],
    detail: &["description", "from_id", "to_id", "lag"],
    links: &[link("from_id", WORK_PACKAGE), link("to_id", WORK_PACKAGE)],
    labels: &[],
};

/// Time entries (BillableWorkEntry, costs module): columns per
/// `modules/costs/lib/api/v3/time_entries/time_entry_representer.rb:74-128`.
pub const TIME_ENTRIES: DeskView = DeskView {
    concept: "BillableWorkEntry",
    canonical: "billable_work_entry",
    title: "Time entries",
    singular: "Time entry",
    route: "/time_entries",
    table: "time_entries",
    only: None,
    headline: "spent_on",
    list: &["user_id", "activity_id", "entity_id", "hours", "comments"],
    detail: &[
        "ongoing",
        "comments",
        "hours",
        "entity_type",
        "entity_id",
        "project_id",
        "user_id",
        "activity_id",
        "start_time",
        "created_at",
        "updated_at",
    ],
    links: &[
        link_when("entity_id", WORK_PACKAGE, ("entity_type", "WorkPackage")),
        link("project_id", ("/projects", "Project")),
        link("user_id", USER),
    ],
    labels: &[],
};

/// Every desk view, in side-menu order.
pub const VIEWS: &[&DeskView] = &[
    &STATUSES,
    &TYPES,
    &PRIORITIES,
    &USERS,
    &JOURNALS,
    &RELATIONS,
    &TIME_ENTRIES,
];

/// The desk table's column names: the basis the skin masks index.
fn basis(view: &DeskView) -> Vec<String> {
    op_desk::desk::table(view.table)
        .map(|t| t.columns.iter().map(|c| c.name.to_string()).collect())
        .unwrap_or_default()
}

/// The list and detail skin masks, for the view registry.
pub fn masks(view: &DeskView) -> [lance_graph_contract::class_view::WideFieldMask; 2] {
    let b = basis(view);
    [skin(&b, view.list).mask, skin(&b, view.detail).mask]
}

/// `default_done_ratio` → `Default done ratio`.
fn caption(name: &str) -> String {
    let mut s = name.replace('_', " ");
    if let Some(first) = s.get(0..1) {
        let upper = first.to_uppercase();
        s.replace_range(0..1, &upper);
    }
    s
}

/// The rows of `view` (all, or one id), through the desk's scoped reads.
fn read(state: &AppState, view: &DeskView, id: Option<i64>) -> Result<Rows, String> {
    let desk = state
        .desk
        .lock()
        .map_err(|_| "desk lock poisoned".to_string())?;
    let rows = match (view.only, id) {
        (Some((col, value)), None) => desk.read(view.table, col, json!(value)),
        (_, Some(id)) => desk.read(view.table, "id", json!(id)),
        (None, None) => desk.read_all(view.table),
    }
    .map_err(|e| e.to_string())?;
    drop(desk);
    // A detail read by id must still honour the STI subset.
    match (view.only, id) {
        (Some((col, value)), Some(_)) => {
            let at = rows.columns.iter().position(|c| c == col);
            Ok(Rows {
                rows: rows
                    .rows
                    .into_iter()
                    .filter(|r| at.is_some_and(|i| r[i] == json!(value)))
                    .collect(),
                columns: rows.columns,
            })
        }
        _ => Ok(rows),
    }
}

fn cell(rows: &Rows, row: &[J], col: &str) -> String {
    rows.columns
        .iter()
        .position(|c| c == col)
        .map_or_else(|| "—".to_string(), |i| cell_text(&row[i]))
}

/// A shown cell: its text, and a link when the column is a [`DeskLink`] that
/// applies to this row (the key is set, and any `when` condition holds).
struct Shown {
    text: String,
    link: Option<(String, &'static str)>,
}

fn raw<'r>(rows: &Rows, row: &'r [J], col: &str) -> Option<&'r J> {
    rows.columns.iter().position(|c| c == col).map(|i| &row[i])
}

fn shown(view: &DeskView, rows: &Rows, row: &[J], col: &str) -> Shown {
    let value = raw(rows, row, col);
    let label = view
        .labels
        .iter()
        .find(|(c, _)| *c == col)
        .and_then(|(_, names)| {
            let n = value?.as_i64()?;
            names
                .iter()
                .find(|(k, _)| *k == n)
                .map(|(_, l)| (*l).to_string())
        });
    let text = label.unwrap_or_else(|| cell(rows, row, col));
    let link = view.links.iter().find(|l| l.col == col).and_then(|l| {
        if let Some((c, v)) = l.when {
            if raw(rows, row, c).and_then(J::as_str) != Some(v) {
                return None;
            }
        }
        let id = value?.as_u64()?;
        Some((format!("{}/{id}", l.route), l.concept))
    });
    Shown { text, link }
}

fn kind(view: &DeskView, col: &str) -> ColumnKind {
    if view.links.iter().any(|l| l.col == col) {
        ColumnKind::RecordRef
    } else {
        ColumnKind::Plain
    }
}

fn data(s: &Shown) -> CellData<'_> {
    match &s.link {
        Some((href, concept)) => CellData::RecordRef {
            label: &s.text,
            href,
            target_concept: concept,
        },
        None => CellData::Plain { value: &s.text },
    }
}

fn id_of(rows: &Rows, row: &[J]) -> u64 {
    rows.columns
        .iter()
        .position(|c| c == "id")
        .and_then(|i| row[i].as_u64())
        .unwrap_or(0)
}

/// The visible tail columns of a skin, in order.
fn visible(view: &DeskView, order: &'static [&'static str]) -> Vec<&'static str> {
    let b = basis(view);
    let mask = skin(&b, order).mask;
    order
        .iter()
        .copied()
        .filter(|name| {
            b.iter()
                .position(|x| x == name)
                .is_some_and(|p| mask.has(p as u8))
        })
        .collect()
}

/// The list fragment (pure: rows in, HTML out).
pub fn render_view_list(view: &DeskView, rows: &Rows) -> String {
    let tail = visible(view, view.list);
    let mut columns = vec![
        RenderColumn::new("id", "#", ColumnKind::IdLink),
        RenderColumn::new(
            view.headline,
            caption(view.headline),
            ColumnKind::PrimaryLink,
        ),
    ];
    columns.extend(
        tail.iter()
            .map(|c| RenderColumn::new(*c, caption(c), kind(view, c))),
    );

    struct Owned {
        id: u64,
        href: String,
        headline: String,
        cells: Vec<Shown>,
    }
    let owned: Vec<Owned> = rows
        .rows
        .iter()
        .map(|r| {
            let id = id_of(rows, r);
            Owned {
                id,
                href: format!("{}/{id}", view.route),
                headline: shown(view, rows, r, view.headline).text,
                cells: tail.iter().map(|c| shown(view, rows, r, c)).collect(),
            }
        })
        .collect();
    let sources: Vec<RowSource> = owned
        .iter()
        .map(|o| {
            let mut inline = vec![
                CellSource {
                    column: &columns[0],
                    css_classes: "",
                    data: CellData::IdLink {
                        id: o.id,
                        href: &o.href,
                    },
                },
                CellSource {
                    column: &columns[1],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &o.headline,
                        href: &o.href,
                    },
                },
            ];
            inline.extend(o.cells.iter().enumerate().map(|(i, v)| CellSource {
                column: &columns[i + 2],
                css_classes: "",
                data: data(v),
            }));
            RowSource {
                record_id: o.id,
                css_classes: "",
                group: None,
                inline,
                block: vec![],
            }
        })
        .collect();
    let class_id = canonical_concept_id(view.canonical).unwrap_or(0);
    render_list(
        view.title,
        class_id,
        view.canonical,
        &columns,
        &Vec::new(),
        &sources,
    )
    .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>"))
}

/// The detail fragment for the single row of `rows`.
pub fn render_view_detail(view: &DeskView, rows: &Rows) -> Option<String> {
    let row = rows.rows.first()?;
    let id = id_of(rows, row);
    let tail = visible(view, view.detail);
    let mut columns = vec![RenderColumn::new("id", "#", ColumnKind::IdLink)];
    columns.extend(
        tail.iter()
            .map(|c| RenderColumn::new(*c, caption(c), kind(view, c))),
    );
    let href = format!("{}/{id}", view.route);
    let values: Vec<Shown> = tail.iter().map(|c| shown(view, rows, row, c)).collect();
    let mut cells = vec![CellSource {
        column: &columns[0],
        css_classes: "",
        data: CellData::IdLink { id, href: &href },
    }];
    cells.extend(values.iter().enumerate().map(|(i, v)| CellSource {
        column: &columns[i + 1],
        css_classes: "",
        data: data(v),
    }));
    let class_id = canonical_concept_id(view.canonical).unwrap_or(0);
    let headline = esc(&shown(view, rows, row, view.headline).text);
    Some(
        render_detail(
            class_id,
            view.canonical,
            id,
            &headline,
            view.singular,
            &columns,
            &cells,
        )
        .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>")),
    )
}

fn error_page(status: StatusCode, title: &str, message: &str) -> Response {
    (
        status,
        Html(page_shell(
            title,
            &format!(
                "{BREADCRUMB}<main><p class=\"empty\">{}</p></main>",
                esc(message)
            ),
        )),
    )
        .into_response()
}

/// GET `<route>`.
pub async fn list(view: &'static DeskView, state: Arc<AppState>) -> Response {
    match read(&state, view, None) {
        Ok(rows) => Html(page_shell(
            view.title,
            &format!("{BREADCRUMB}<main>{}</main>", render_view_list(view, &rows)),
        ))
        .into_response(),
        Err(e) => {
            tracing::error!(table = view.table, error = %e, "desk read refused");
            error_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                view.title,
                &format!("{} could not be read.", view.title),
            )
        }
    }
}

/// GET `<route>/:id`.
pub async fn detail(view: &'static DeskView, id: i64, state: Arc<AppState>) -> Response {
    let not_found = || {
        error_page(
            StatusCode::NOT_FOUND,
            &format!("{} not found", view.singular),
            &format!("{} #{id} not found.", view.singular),
        )
    };
    // Quack's integer lane holds ids as u32: an id outside it (negative, or
    // above u32::MAX) cannot name a row, and the desk would refuse the read.
    if u32::try_from(id).is_err() {
        return not_found();
    }
    match read(&state, view, Some(id)) {
        Ok(rows) => match render_view_detail(view, &rows) {
            Some(fragment) => Html(page_shell(
                view.singular,
                &format!("{BREADCRUMB}<main>{fragment}</main>"),
            ))
            .into_response(),
            None => not_found(),
        },
        Err(e) => {
            tracing::error!(table = view.table, error = %e, "desk read refused");
            error_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                view.singular,
                &format!("{} #{id} could not be read.", view.singular),
            )
        }
    }
}

/// Handlers for one view, as axum route functions.
macro_rules! desk_routes {
    ($list:ident, $detail:ident, $view:expr) => {
        pub async fn $list(State(state): State<Arc<AppState>>) -> Response {
            list(&$view, state).await
        }
        pub async fn $detail(Path(id): Path<i64>, State(state): State<Arc<AppState>>) -> Response {
            detail(&$view, id, state).await
        }
    };
}
desk_routes!(statuses_list, statuses_detail, STATUSES);
desk_routes!(types_list, types_detail, TYPES);
desk_routes!(priorities_list, priorities_detail, PRIORITIES);
desk_routes!(users_list, users_detail, USERS);
desk_routes!(journals_list, journals_detail, JOURNALS);
desk_routes!(relations_list, relations_detail, RELATIONS);
desk_routes!(time_entries_list, time_entries_detail, TIME_ENTRIES);

#[cfg(test)]
mod tests {
    use super::*;

    fn journal_rows(journable_type: &str) -> Rows {
        Rows {
            columns: vec![
                "journable_type".into(),
                "journable_id".into(),
                "user_id".into(),
            ],
            rows: vec![vec![json!(journable_type), json!(4), json!(2)]],
        }
    }

    /// A polymorphic id links only when its type column names the target:
    /// a journal of a Project is not a work-package link.
    #[test]
    fn a_polymorphic_link_needs_its_type() {
        let rows = journal_rows("WorkPackage");
        let s = shown(&JOURNALS, &rows, &rows.rows[0], "journable_id");
        assert_eq!(s.link.map(|(h, _)| h), Some("/work_packages/4".to_string()));

        let rows = journal_rows("Project");
        let s = shown(&JOURNALS, &rows, &rows.rows[0], "journable_id");
        assert!(s.link.is_none(), "a Project journal is not a work package");
        assert_eq!(s.text, "4", "the id is still shown");
        // An unconditional link is unaffected by the type column.
        let s = shown(&JOURNALS, &rows, &rows.rows[0], "user_id");
        assert_eq!(s.link.map(|(h, _)| h), Some("/users/2".to_string()));
    }
}
