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
};

/// Every desk view, in side-menu order.
pub const VIEWS: &[&DeskView] = &[&STATUSES, &TYPES, &PRIORITIES];

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
            .map(|c| RenderColumn::new(*c, caption(c), ColumnKind::Plain)),
    );

    struct Owned {
        id: u64,
        href: String,
        headline: String,
        cells: Vec<String>,
    }
    let owned: Vec<Owned> = rows
        .rows
        .iter()
        .map(|r| {
            let id = id_of(rows, r);
            Owned {
                id,
                href: format!("{}/{id}", view.route),
                headline: cell(rows, r, view.headline),
                cells: tail.iter().map(|c| cell(rows, r, c)).collect(),
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
                data: CellData::Plain { value: v },
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
            .map(|c| RenderColumn::new(*c, caption(c), ColumnKind::Plain)),
    );
    let href = format!("{}/{id}", view.route);
    let values: Vec<String> = tail.iter().map(|c| cell(rows, row, c)).collect();
    let mut cells = vec![CellSource {
        column: &columns[0],
        css_classes: "",
        data: CellData::IdLink { id, href: &href },
    }];
    cells.extend(values.iter().enumerate().map(|(i, v)| CellSource {
        column: &columns[i + 1],
        css_classes: "",
        data: CellData::Plain { value: v },
    }));
    let class_id = canonical_concept_id(view.canonical).unwrap_or(0);
    let headline = esc(&cell(rows, row, view.headline));
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
