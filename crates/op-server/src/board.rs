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

use a2ui_layout::{LayoutMode, Region, RIGHT_SLIM_PX, ZONES_CSS};
use axum::extract::{Extension, Form, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};

use ogar_render_askama::{
    render_detail, render_field_view, render_form, render_list, CellData, CellSource, ColumnKind,
    FieldView, FormFieldSource, FormSource, GroupHeader, InputData, RenderColumn, RowSource,
    SelectOptionOwned,
};
// `project` (the ogar_vocab canonical-class constructor) is aliased to
// `project_class` — the tests module below defines its own `fn project(...)`
// fake-row helper, and a bare `project` import would collide with it (same
// value namespace, same name, `use super::*;` would pull both in).
use ogar_vocab::{canonical_concept_id, project as project_class, project_work_item, Class};

use lance_graph_contract::class_view::WideFieldMask;

use op_db::repository::Repository;
use op_db::{
    PriorityRepository, PriorityRow, ProjectRepository, ProjectRow, StatusRepository, StatusRow,
    TypeRepository, TypeRow, UpdateProjectDto, UpdateWorkPackageDto, UserRepository, UserRow,
};

use crate::health::AppState;
use crate::layout;
use crate::nav;
use crate::signin::Who;
use crate::viewfilter;
use crate::write_guard::{
    self, actor_key, EditOptions, PageStore, PgWpStore, Principal, StoreError, WpStore, CSRF_FIELD,
    EDIT_PROJECT,
};
use op_services::work_packages::mutation::{self, MutationError};

// ── Field basis + FieldMask-driven column projection ─────────────────
//
// A skin's column set is no longer hand-listed: it is a
// `WideFieldMask` over a per-class `basis` (position 0 = the class's
// first attribute, counting up through attributes then associations —
// mirroring `ClassView::fields`'s position convention), plus a small
// pragmatic tail of leaf fields (`subject`, `done_ratio`, the `_id`
// form-select keys, …) that exist on the DB row / form but are not
// (yet) modeled as `ogar_vocab::Attribute`s on the synthetic canonical
// classes (`project_work_item()` carries zero `Attribute`s today — see
// its doc comment; `project()` carries only `name`/`identifier`). The
// tail is appended, never interleaved, so the attributes-then-
// associations prefix stays a faithful `ClassView` mirror and every
// position is still a stable, addressable bit.

/// The field-order source for a class: `attributes[].name` then
/// `associations[].name`, in declaration order, plus `extra` leaf
/// field names appended (skipping any already present). THIS is the
/// position index a `WideFieldMask` bit refers to for this class.
fn basis(class: &Class, extra: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = class.attributes.iter().map(|a| a.name.clone()).collect();
    out.extend(class.associations.iter().map(|a| a.name.clone()));
    for name in extra {
        if !out.iter().any(|b| b == name) {
            out.push((*name).to_string());
        }
    }
    out
}

/// The extra leaf fields `project_work_item()` needs beyond its
/// (currently empty) `attributes` + its 6 direct family edges: the
/// DB-row scalars every skin renders (`subject` .. `description`), and
/// the raw FK-select keys the edit form posts under (`status_id` ..
/// `assigned_to_id`) — distinct positions from the association roles
/// (`status` .. `assignee`) because the form's `name=` attribute must
/// be the literal POST key `parse_wp_update` reads.
const WP_EXTRA_FIELDS: &[&str] = &[
    "subject",
    "start_date",
    "due_date",
    "estimated_hours",
    "done_ratio",
    "description",
    "status_id",
    "type_id",
    "priority_id",
    "assigned_to_id",
    "lock_version",
];

/// The extra leaf fields `project()` needs beyond its `name`/
/// `identifier` attributes: the remaining DB-row scalars.
const PROJECT_EXTRA_FIELDS: &[&str] = &["public", "active", "created_at", "description"];

fn wp_basis() -> Vec<String> {
    basis(&project_work_item(), WP_EXTRA_FIELDS)
}

fn project_basis() -> Vec<String> {
    basis(&project_class(), PROJECT_EXTRA_FIELDS)
}

/// Resolve the up-link href for a `belongs_to`-style association
/// (`role`) on `class`, IF its target concept has a live route (per
/// [`nav::route_for`] through [`nav::nav_edges`]) — `None` for a "dead
/// lane" association (a known, undeferred-but-unrouted target such as
/// `status`/`type`/`priority`/`author`/`assignee` today). This is the
/// single source of truth [`kind_for`] also consults, so the column
/// kind and the cell's actual href never disagree.
fn nav_href(class: &Class, role: &str, fk_id: i64) -> Option<String> {
    nav::nav_edges(class)
        .into_iter()
        .find(|e| e.role == role)
        .and_then(|e| e.route)
        .map(|route| format!("{route}/{fk_id}"))
}

/// Field order + `WideFieldMask` for a skin. Order is what the skin
/// author writes (and drives display order via [`columns_for`]); the
/// mask (built from the same names' basis positions) is what actually
/// gates membership — the two are kept in one struct so a skin is
/// defined once.
struct Skin {
    order: &'static [&'static str],
    mask: WideFieldMask,
}

fn skin(basis: &[String], order: &'static [&'static str]) -> Skin {
    // Canonical mask minting (lance-graph #669): `from_universe_present`
    // sets bit `i` iff `basis[i] ∈ order` — the SAME sort-agnostic
    // membership rule odoo-rs's `mint_wide_mask` and the Rails/ERB
    // `ViewFieldSet` use, so op-nexgen mints the IDENTICAL mask as every
    // other ClassView consumer (the DTO-Lego-brick interchange guarantee).
    // The basis is far under the 256-field SOC-split cap, so the mint
    // cannot error here — a >256 universe would be a "split this class"
    // signal, not a mask to widen (the rolling-bucket overflow path for
    // un-split God objects lives in `crate::viewfilter::bucketized_masks`).
    let universe: Vec<&str> = basis.iter().map(String::as_str).collect();
    let view = WideFieldMask::from_universe_present(&universe, order)
        .expect("class basis is well under the 256-field SOC cap");
    // The facet-presence gate: every basis field exists in memory today,
    // so `present` = the full basis — minted on-brick, and the seam where
    // a disabled facet would narrow what a tenant's ClassView carries.
    let present = WideFieldMask::from_universe_present(&universe, &universe)
        .expect("class basis is well under the 256-field SOC cap");
    // The ViewFilter (one brick, three sources): rbac ∩ present ∩ view.
    // Anonymous demo posture → every addressable position today; when a
    // role store lands, columns drop per role with no change here.
    Skin {
        order,
        mask: viewfilter::view_filter(&viewfilter::current_rbac_field_mask(), &present, &view),
    }
}

/// Board (list, standalone) skin: the kanban card columns.
const WP_BOARD_ORDER: &[&str] = &["subject", "project", "done_ratio"];

/// Detail skin: the fuller work-package detail page.
const WP_DETAIL_ORDER: &[&str] = &[
    "status",
    "project",
    "type",
    "priority",
    "author",
    "assignee",
    "start_date",
    "due_date",
    "estimated_hours",
    "done_ratio",
    "description",
];

/// Form skin: the editable work-package fields (posted under their raw
/// `_id` FK keys, not the association role names — see
/// [`WP_EXTRA_FIELDS`]).
const WP_FORM_ORDER: &[&str] = &[
    "subject",
    "status_id",
    "type_id",
    "priority_id",
    "assigned_to_id",
    "start_date",
    "due_date",
    "estimated_hours",
    "done_ratio",
    "description",
    "lock_version",
];

/// Stacked skin: the board mask, minus the `project` column — used
/// when a work-package list is embedded UNDER a project detail page
/// (the parent context already names the project, so repeating it in
/// every row is noise). Built by intersecting `WP_BOARD_ORDER`'s mask
/// with a "context" mask of every basis field EXCEPT `project` —
/// demonstrating `WideFieldMask` composition (the stud algebra) rather
/// than hand-editing a second literal field list. Both operands are
/// minted through the canonical `from_universe_present` brick — no raw
/// `from_positions`, so the SOC-cap guard applies to every mask here.
fn wp_stacked_skin(basis: &[String]) -> Skin {
    let board = skin(basis, WP_BOARD_ORDER);
    // The "in-project context" universe: present = every basis field but
    // `project`. Minted on-brick (identical rule to odoo-rs/ViewFieldSet).
    let universe: Vec<&str> = basis.iter().map(String::as_str).collect();
    let context_present: Vec<&str> = universe
        .iter()
        .copied()
        .filter(|&n| n != "project")
        .collect();
    let without_project = WideFieldMask::from_universe_present(&universe, &context_present)
        .expect("class basis is well under the 256-field SOC cap");
    Skin {
        order: WP_BOARD_ORDER,
        mask: board.mask.intersect(&without_project),
    }
}

/// Project detail skin.
const PROJECT_DETAIL_ORDER: &[&str] = &[
    "identifier",
    "public",
    "active",
    "created_at",
    "description",
];

/// Project form skin.
const PROJECT_FORM_ORDER: &[&str] = &["name", "public", "active", "description"];

/// Project index (list) skin.
const PROJECT_INDEX_ORDER: &[&str] = &["identifier", "public", "active"];

/// Intern the app's REAL route skins into a [`viewfilter::ViewRegistry`]
/// — every route arm's `(concept, mask)` becomes an interned node, so two
/// operations minting the same projection resolve to ONE node (route-arm
/// dedup + constructor amortization), and the one genuine stack today
/// (`/projects/:id` composing the stacked work-package list) is a child
/// edge interned children-first. Called at boot (`main`) where the
/// once-pass construction-order check and the forward/backward bijective
/// round trip run over exactly what the routes serve.
pub fn view_registry() -> viewfilter::ViewRegistry {
    let wp_basis = wp_basis();
    let project_basis = project_basis();
    let mut reg = viewfilter::ViewRegistry::new();
    let leaf = |reg: &mut viewfilter::ViewRegistry, concept, mask| {
        reg.intern(concept, mask, vec![])
            .expect("leaf nodes have no children — cannot violate construction order")
    };
    // Leaf skins first (children before parents — the construction order
    // IS the topological order, so the stack is acyclic by construction).
    leaf(
        &mut reg,
        "ProjectWorkItem",
        skin(&wp_basis, WP_BOARD_ORDER).mask,
    ); // `/`
    let wp_stacked = leaf(&mut reg, "ProjectWorkItem", wp_stacked_skin(&wp_basis).mask);
    leaf(
        &mut reg,
        "ProjectWorkItem",
        skin(&wp_basis, WP_DETAIL_ORDER).mask,
    ); // `/work_packages/:id`
    leaf(
        &mut reg,
        "ProjectWorkItem",
        skin(&wp_basis, WP_FORM_ORDER).mask,
    ); // `…/edit`
    leaf(
        &mut reg,
        "Project",
        skin(&project_basis, PROJECT_INDEX_ORDER).mask,
    ); // `/projects`
    leaf(
        &mut reg,
        "Project",
        skin(&project_basis, PROJECT_FORM_ORDER).mask,
    ); // `…/edit`
       // The real nesting: project detail stacks the WP list (child already
       // interned above — children-first, checked by `intern` itself).
    reg.intern(
        "Project",
        skin(&project_basis, PROJECT_DETAIL_ORDER).mask,
        vec![wp_stacked],
    )
    .expect("wp_stacked is interned above — construction order holds");
    reg
}

/// Boot-time proof that the rolling-bucket overflow path is equivalent to
/// the direct mint on the app's REAL bases (both fit one bucket today) —
/// so the God-object path is already proven before any basis grows past
/// the 256-field cap.
pub fn bases_bucket_roundtrip() -> bool {
    let wp_basis = wp_basis();
    let project_basis = project_basis();
    let wp_universe: Vec<&str> = wp_basis.iter().map(String::as_str).collect();
    let project_universe: Vec<&str> = project_basis.iter().map(String::as_str).collect();
    viewfilter::buckets_match_direct(&wp_universe, WP_BOARD_ORDER)
        && viewfilter::buckets_match_direct(&wp_universe, WP_DETAIL_ORDER)
        && viewfilter::buckets_match_direct(&project_universe, PROJECT_DETAIL_ORDER)
}

/// Human caption for a basis field name — mirrors the captions the
/// hand-listed `RenderColumn` vecs used to carry verbatim.
fn caption_for(name: &str) -> &'static str {
    match name {
        "subject" => "Subject",
        "project" => "Project",
        "status" | "status_id" => "Status",
        "type" | "type_id" => "Type",
        "priority" | "priority_id" => "Priority",
        "author" => "Author",
        "assignee" | "assigned_to_id" => "Assignee",
        "start_date" => "Start date",
        "due_date" => "Due date",
        "estimated_hours" => "Estimated hours",
        "done_ratio" => "% Done",
        "description" => "Description",
        "lock_version" => "Lock version",
        "name" => "Name",
        "identifier" => "Identifier",
        "public" => "Public",
        "active" => "Active",
        "created_at" => "Created",
        "work_items" => "Work items",
        _ => "Field",
    }
}

/// Cell-kind dispatch for a basis field name, in the context of `class`.
/// A field that is one of `class`'s associations renders as
/// [`ColumnKind::RecordRef`] IF the association's target concept
/// resolves to a live route via [`nav::route_for`] (through
/// [`nav::nav_edges`]) — otherwise it stays [`ColumnKind::Plain`] (a
/// "dead lane": the label shows, but there's nowhere to link to yet).
/// This is the topology-driven half of (B): a new page landing for a
/// concept that already has an association edge lights the link up
/// without touching this function. Non-association fields keep the
/// small fixed kind table the hand-listed columns used to carry.
fn kind_for(name: &str, class: &Class) -> ColumnKind {
    match name {
        "id" => ColumnKind::IdLink,
        "subject" => ColumnKind::PrimaryLink,
        "done_ratio" => ColumnKind::ProgressBar,
        "description" => ColumnKind::RichText,
        _ => {
            let is_routed_assoc = nav::nav_edges(class)
                .into_iter()
                .find(|e| e.role == name)
                .is_some_and(|e| e.route.is_some());
            if is_routed_assoc {
                ColumnKind::RecordRef
            } else {
                ColumnKind::Plain
            }
        }
    }
}

/// Build the `RenderColumn` list a mask selects out of `basis`,
/// ordered per `order` (a skin's own field-order list) rather than raw
/// basis-position order — `order` is what a skin author writes (and
/// what the previous hand-listed `RenderColumn` vecs encoded
/// implicitly via `vec![...]` element order); `mask` is what actually
/// gates inclusion. A name in `order` that the mask doesn't select
/// (e.g. `stacked_mask` dropping `project`) is silently skipped — same
/// as a name not present in `basis` at all.
fn columns_for(
    basis: &[String],
    mask: &WideFieldMask,
    order: &[&str],
    class: &Class,
) -> Vec<RenderColumn> {
    order
        .iter()
        .filter_map(|name| {
            let pos = basis.iter().position(|b| b == name)?;
            if !mask.has(pos as u8) {
                return None;
            }
            let mut col = RenderColumn::new(
                (*name).to_string(),
                caption_for(name),
                kind_for(name, class),
            );
            if *name == "subject" {
                col = col.sortable();
            }
            if *name == "description" {
                col = col.block();
            }
            Some(col)
        })
        .collect()
}

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
    let class = project_work_item();
    let basis = wp_basis();
    let board = skin(&basis, WP_BOARD_ORDER);
    let mut inline_columns: Vec<RenderColumn> =
        vec![RenderColumn::new("id", "#", ColumnKind::IdLink)];
    inline_columns.extend(columns_for(&basis, &board.mask, board.order, &class));
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

/// Breadcrumb + "Edit" link, used above the two detail pages (work
/// package / project) so the reader lands directly on the render-bake
/// `HtmlForm` edit route for that record.
fn detail_toolbar(edit_href: &str) -> String {
    format!(
        "<p class=\"breadcrumb\"><a href=\"/\">← Board</a> · <a href=\"{edit_href}\" class=\"edit-link\">✎ Edit</a></p>"
    )
}

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
) -> Response {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("Work package", "this page").into_response();
    };

    // `Ok(None)` is 404; a database error is 500 (never collapsed into "not found").
    let wp = match lookup(
        op_db::WorkPackageRepository::new(pool.clone())
            .find_by_id(id)
            .await,
        "Work package",
        id,
    ) {
        Ok(wp) => wp,
        Err(resp) => return resp,
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

    let toolbar = detail_toolbar(&format!("/work_packages/{id}/edit"));
    Html(page_shell(
        &format!("Work package #{id} — {}", esc(&wp.subject)),
        &format!("{toolbar}<main>{fragment}</main>"),
    ))
    .into_response()
}

/// Which part of the detail grid a field lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Spans both columns, in its own row (Redmine has no slot for it
    /// in the split, or — like `description` — renders it full width).
    Full,
    /// Redmine's `splitcontentleft`.
    Left,
    /// Redmine's `splitcontentright`.
    Right,
}

/// Declarative layout of the work-package field view, mirroring
/// Redmine's `issues/show.html.erb` (lines 73-108) /
/// `IssueFieldsRows` (`issues_helper.rb` 326-400): `(wp_basis field
/// name, side, Redmine css class of the `div.attribute`)`. Table order
/// is row order within a side; a `Full` entry starts a new row below
/// everything placed so far.
///
/// * Left column: status, priority, assigned_to (op: `assignee`).
/// * Right column: start_date, due_date, done_ratio, estimated_hours.
/// * Redmine fields op's basis does not model, hence skipped:
///   `category` and `fixed_version` (left column) and `spent_time`
///   (right column). `WorkPackageRow` carries `category_id`/`version_id`
///   but `wp_basis()` has no such positions, so there is no address to
///   gate or render them at.
/// * Op fields Redmine has no split slot for (`type`, `project`,
///   `author`) are a full-width group ABOVE the split; `description`
///   is full width BELOW it (Redmine also renders it under the split).
///   `subject` is the section title, as in Redmine.
///
/// Redmine's `disabled_core_fields` is the `WideFieldMask` of the detail
/// skin: a field whose position bit is off is never emitted.
const WP_FIELD_SPLIT: &[(&str, Side, &str)] = &[
    ("type", Side::Full, "tracker"),
    ("project", Side::Full, "project"),
    ("author", Side::Full, "author"),
    ("status", Side::Left, "status"),
    ("priority", Side::Left, "priority"),
    ("assignee", Side::Left, "assigned-to"),
    ("start_date", Side::Right, "start-date"),
    ("due_date", Side::Right, "due-date"),
    ("done_ratio", Side::Right, "progress"),
    ("estimated_hours", Side::Right, "estimated-hours"),
    ("description", Side::Full, "description"),
];

/// One placed field: its basis position (the `data-field-pos` address),
/// name, side, redmine css class and 1-based grid row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PlacedField {
    pos: u8,
    name: &'static str,
    side: Side,
    css: &'static str,
    row: u32,
}

/// Place every `WP_FIELD_SPLIT` entry that exists in `basis` AND whose
/// position bit is on in `mask`. Pure; rows are assigned per column so
/// masked-out fields leave no gap.
fn wp_split_layout(basis: &[String], mask: &WideFieldMask) -> Vec<PlacedField> {
    let (mut left, mut right) = (1u32, 1u32);
    let mut out = Vec::new();
    for &(name, side, css) in WP_FIELD_SPLIT {
        let Some(pos) = basis.iter().position(|b| b == name) else {
            continue;
        };
        let Ok(pos) = u8::try_from(pos) else { continue };
        if !mask.has(pos) {
            continue;
        }
        let row = match side {
            Side::Left => {
                left += 1;
                left - 1
            }
            Side::Right => {
                right += 1;
                right - 1
            }
            Side::Full => {
                let r = left.max(right);
                left = r + 1;
                right = r + 1;
                r
            }
        };
        out.push(PlacedField {
            pos,
            name,
            side,
            css,
            row,
        });
    }
    out
}

/// Scoped `<style>` placing each `[data-field-pos="N"]` in its grid
/// column/row. Layout is a renderer concern: the fieldview markup
/// stays one flat `<dl>`; this derives from `placed` only.
fn wp_split_css(placed: &[PlacedField]) -> String {
    let mut css = String::from(
        ".ogar-fieldview .fieldview-fields{display:grid;grid-template-columns:1fr 1fr;gap:.25rem 2rem}\
.ogar-fieldview .fieldview-field{display:flex;gap:.5rem}\
.ogar-fieldview .fieldview-label{font-weight:600;min-width:9rem}\
.ogar-fieldview .fieldview-label::after{content:\":\"}\
.ogar-fieldview .fieldview-value{margin:0;white-space:pre-wrap}\n",
    );
    for p in placed {
        let col = match p.side {
            Side::Left => "1",
            Side::Right => "2",
            Side::Full => "1 / -1",
        };
        css.push_str(&format!(
            "/* .attribute.{} */.ogar-fieldview .fieldview-field[data-field-pos=\"{}\"]{{grid-column:{col};grid-row:{}}}\n",
            p.css, p.pos, p.row
        ));
    }
    css.push_str(
        "@media (max-width:640px){.ogar-fieldview .fieldview-fields{grid-template-columns:1fr}\
.ogar-fieldview .fieldview-field{grid-column:1 !important;grid-row:auto !important}}\n",
    );
    css
}

/// Render the work-package detail fragment (pure — no DB access), given
/// the row plus the already-resolved display names for its family-edge
/// fields (status/project/type/priority/author/assignee). Kept separate
/// from the async handler so it's testable without a live DB pool
/// (mirrors [`render_board`]'s split).
///
/// Renders through OGAR's addressed field view
/// ([`ogar_render_askama::render_field_view`]): one section, one key,
/// every field at its basis position. The Redmine two-column ergonomics
/// come from [`WP_FIELD_SPLIT`] + a scoped grid `<style>`; the record's
/// own link and the project link (which a fieldview cannot carry — it
/// has no `|safe` content) sit in a context line above it.
fn render_wp_detail(
    wp: &op_db::work_packages::WorkPackageRow,
    status_name: &str,
    project_name: &str,
    type_name: &str,
    priority_name: &str,
    author_name: &str,
    assignee_name: &str,
) -> String {
    let class = project_work_item();
    let basis = wp_basis();
    let detail = skin(&basis, WP_DETAIL_ORDER);
    let placed = wp_split_layout(&basis, &detail.mask);

    let dash = || "—".to_string();
    let value_for = |name: &str| -> String {
        match name {
            "status" => status_name.to_string(),
            "project" => project_name.to_string(),
            "type" => type_name.to_string(),
            "priority" => priority_name.to_string(),
            "author" => author_name.to_string(),
            "assignee" => assignee_name.to_string(),
            "start_date" => wp.start_date.map(|d| d.to_string()).unwrap_or_else(dash),
            "due_date" => wp.due_date.map(|d| d.to_string()).unwrap_or_else(dash),
            "estimated_hours" => wp
                .estimated_hours
                .map(|h| format!("{h:.2}"))
                .unwrap_or_else(dash),
            "done_ratio" => format!("{}%", wp.done_ratio.clamp(0, 100)),
            "description" => wp.description.clone().unwrap_or_else(dash),
            _ => dash(),
        }
    };

    let concept = class.canonical_concept.clone().unwrap_or_default();
    let class_id = canonical_concept_id(&concept).unwrap_or(0);

    let fields: Vec<FieldView> = placed
        .iter()
        .map(|p| FieldView {
            position: p.pos,
            label: caption_for(p.name).to_string(),
            predicate: format!("{concept}:{}", p.name),
            value: value_for(p.name),
        })
        .collect();

    // INTERIM node key: class id (8 hex) ++ work-package id (24 hex) —
    // deterministic and stable, but NOT the canonical OGAR GUID mint
    // (op-nexgen has no `NodeGuid` minter yet). Replace when it does.
    let key_hex = format!("{:08x}{:024x}", class_id, wp.id as u64);

    // Title is raw: `render_field_view` escapes every interpolation.
    let title = wp.subject.as_str();
    let view = render_field_view(class_id, &concept, &key_hex, title, &fields, &[])
        .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>"));

    let wp_href = format!("/work_packages/{}", wp.id);
    let project_href = nav_href(&class, "project", wp.project_id)
        .unwrap_or_else(|| format!("/projects/{}", wp.project_id));
    let context = format!(
        "<p class=\"wp-context\"><a href=\"{wp_href}\">#{}</a> · <a href=\"{project_href}\">{}</a></p>",
        wp.id,
        esc(project_name),
    );

    format!("{context}<style>{}</style>{view}", wp_split_css(&placed))
}

// ── Work package edit ────────────────────────────────────────────────

/// GET `/work_packages/:id/edit` — the render-bake `HtmlForm` edit page,
/// pre-filled from the record. `<select>` options are sourced from the
/// same family-edge lookup tables [`work_package_detail`] resolves names
/// from, but kept as `(id, label)` pairs here instead of collapsing to a
/// single resolved string.
///
/// Only someone who may edit the work package gets the form (403 otherwise);
/// the form carries a CSRF token bound to this browser and actor.
pub async fn work_package_edit_form(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    who: Option<Extension<Who>>,
) -> Response {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("Edit work package", "this page").into_response();
    };
    let who = who.map(|Extension(w)| w);
    let ctx = WriteCtx::from_request(who.as_ref(), &headers);
    edit_form_core(&PgWpStore { pool }, id, &ctx).await
}

/// Request-derived inputs of a guarded write: the signed-in identity, the request headers (for
/// the CSRF cookie), and the process-level policy (anonymous opt-in, CSRF secret, cookie flags).
struct WriteCtx<'a> {
    who: Option<&'a Who>,
    headers: &'a HeaderMap,
    allow_anonymous: bool,
    secret: &'a [u8],
    secure_cookie: bool,
}

impl<'a> WriteCtx<'a> {
    fn from_request(who: Option<&'a Who>, headers: &'a HeaderMap) -> Self {
        WriteCtx {
            who,
            headers,
            allow_anonymous: crate::env_flag("OP_ALLOW_ANONYMOUS"),
            secret: write_guard::secret(),
            secure_cookie: write_guard::deploy_is_https(),
        }
    }

    /// Does the submitted form carry a token issued to this browser for this actor?
    fn csrf_ok(&self, form: &HashMap<String, String>) -> bool {
        write_guard::token_valid(
            self.secret,
            write_guard::cookie_from_headers(self.headers).as_deref(),
            form.get(CSRF_FIELD).map(String::as_str),
            &actor_key(self.who),
        )
    }
}

/// A plain status page (the body text is escaped).
fn status_page(status: StatusCode, title: &str, message: &str) -> Response {
    (
        status,
        Html(page_shell(
            title,
            &format!("<main><p class=\"empty\">{}</p></main>", esc(message)),
        )),
    )
        .into_response()
}

fn forbidden(message: &str) -> Response {
    status_page(StatusCode::FORBIDDEN, "Forbidden", message)
}

/// Log a store failure and answer 500 without echoing the database error to the page.
fn internal_error(e: impl std::fmt::Display) -> Response {
    tracing::error!("write path: store error: {e}");
    status_page(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal error",
        "Something went wrong on the server.",
    )
}

/// Turn a repository lookup into the record or the right error page: `Ok(None)` is 404,
/// `Err(_)` is 500. (A database failure must never read as "not found".)
#[expect(
    clippy::result_large_err,
    reason = "the Err is the ready-to-send error page, returned straight to the handler"
)]
fn lookup<T, E: std::fmt::Display>(
    r: Result<Option<T>, E>,
    what: &str,
    id: i64,
) -> Result<T, Response> {
    match r {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(status_page(
            StatusCode::NOT_FOUND,
            &format!("{what} not found"),
            &format!("{what} #{id} not found."),
        )),
        Err(e) => Err(internal_error(e)),
    }
}

/// The page answer to a work-package mutation the boundary refused or could not perform.
///
/// Authorization is decided by `op_services::work_packages::mutation` (the same decision the JSON
/// API makes): an anonymous or unauthorized caller is 403 here, an unknown record 404, a store
/// fault 500 without the database error echoed. Nothing else reaches a page from the boundary.
fn mutation_error_page(e: MutationError, id: i64) -> Response {
    match e {
        MutationError::Unauthenticated | MutationError::Forbidden => {
            forbidden("You are not allowed to edit this work package.")
        }
        MutationError::NotFound => status_page(
            StatusCode::NOT_FOUND,
            "Work package not found",
            &format!("Work package #{id} not found."),
        ),
        MutationError::Invalid(v) => internal_error(format!("{v:?}")),
        MutationError::Unresolved(m) | MutationError::Conflict(m) | MutationError::Store(m) => {
            internal_error(m)
        }
    }
}

/// Core of [`work_package_edit_form`], generic over the store so it is testable without a DB.
async fn edit_form_core<S: WpStore>(store: &S, id: i64, ctx: &WriteCtx<'_>) -> Response {
    let caller = write_guard::caller_of(ctx.who, ctx.allow_anonymous);
    let wp = match mutation::authorize_update(&PageStore(store), &caller, id).await {
        Ok(ticket) => ticket.existing().clone(),
        Err(e) => return mutation_error_page(e, id),
    };

    let EditOptions {
        statuses,
        types,
        priorities,
        users,
    } = store.edit_options().await;

    // Reuse the browser's CSRF cookie when it has one; otherwise issue one with this response.
    let existing = write_guard::cookie_from_headers(ctx.headers);
    let (cookie, fresh) = match existing {
        Some(c) => (c, false),
        None => (write_guard::new_cookie_value(), true),
    };
    let token = write_guard::token_for(ctx.secret, &cookie, &actor_key(ctx.who));

    let fragment = render_wp_edit(&wp, &statuses, &types, &priorities, &users, &token);
    let mut resp = Html(page_shell(
        &format!("Edit work package #{id}"),
        &format!("{BREADCRUMB}<main>{fragment}</main>"),
    ))
    .into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if fresh {
        if let Ok(v) =
            HeaderValue::from_str(&write_guard::set_cookie_value(&cookie, ctx.secure_cookie))
        {
            resp.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    resp
}

/// Render the work-package edit form fragment (pure — no DB access),
/// given the row plus the family-edge lookup tables (status/type/
/// priority/user) it renders as `<select>` options. Kept separate from
/// the async handler so it's testable without a live DB pool (mirrors
/// [`render_wp_detail`]).
///
/// `status_id`/`type_id` are required `<select>`s (the row always has a
/// valid FK); `priority_id`/`assigned_to_id` are optional `<select>`s —
/// left `required: false` so `dispatch/input/select.askama` (OGAR
/// `templates/dispatch/input/select.askama:3-5`) auto-emits the blank
/// `<option value="">—</option>` itself; a hand-added blank option here
/// would double it up.
fn render_wp_edit(
    wp: &op_db::work_packages::WorkPackageRow,
    statuses: &[StatusRow],
    types: &[TypeRow],
    priorities: &[PriorityRow],
    users: &[UserRow],
    csrf_token: &str,
) -> String {
    let class = project_work_item();
    let basis = wp_basis();
    let form = skin(&basis, WP_FORM_ORDER);
    // `columns_for` sets `subject`/`description` to Sortable/Block —
    // both irrelevant to a form's `RenderColumn` (form dispatch reads
    // `name`/`caption`, not `sortable`/`block`), so they're harmless
    // here; kept identical to the list/detail skins for one shared
    // helper rather than a second near-duplicate.
    let columns: Vec<RenderColumn> = columns_for(&basis, &form.mask, form.order, &class);

    let status_options: Vec<SelectOptionOwned> = statuses
        .iter()
        .map(|s| SelectOptionOwned {
            value: s.id.to_string(),
            label: s.name.clone(),
        })
        .collect();
    let type_options: Vec<SelectOptionOwned> = types
        .iter()
        .map(|t| SelectOptionOwned {
            value: t.id.to_string(),
            label: t.name.clone(),
        })
        .collect();
    let priority_options: Vec<SelectOptionOwned> = priorities
        .iter()
        .map(|p| SelectOptionOwned {
            value: p.id.to_string(),
            label: p.name.clone(),
        })
        .collect();
    let assignee_options: Vec<SelectOptionOwned> = users
        .iter()
        .map(|u| SelectOptionOwned {
            value: u.id.to_string(),
            label: u.full_name(),
        })
        .collect();

    let action = format!("/work_packages/{}/edit", wp.id);
    let legend = format!("Edit work package #{}", wp.id);
    let cancel_href = format!("/work_packages/{}", wp.id);
    let priority_value = wp.priority_id.map(|id| id.to_string()).unwrap_or_default();
    let assignee_value = wp
        .assigned_to_id
        .map(|id| id.to_string())
        .unwrap_or_default();
    let start_date = wp.start_date.map(|d| d.to_string()).unwrap_or_default();
    let due_date = wp.due_date.map(|d| d.to_string()).unwrap_or_default();
    let estimated_hours = wp
        .estimated_hours
        .map(|h| h.to_string())
        .unwrap_or_default();
    let description = wp.description.clone().unwrap_or_default();

    let fields: Vec<FormFieldSource<'_>> = vec![
        FormFieldSource {
            column: &columns[0],
            css_classes: "",
            hint: "",
            data: InputData::Text {
                value: wp.subject.clone(),
                required: true,
                placeholder: String::new(),
            },
        },
        FormFieldSource {
            column: &columns[1],
            css_classes: "",
            hint: "",
            data: InputData::Select {
                value: wp.status_id.to_string(),
                required: true,
                options: status_options,
            },
        },
        FormFieldSource {
            column: &columns[2],
            css_classes: "",
            hint: "",
            data: InputData::Select {
                value: wp.type_id.to_string(),
                required: true,
                options: type_options,
            },
        },
        FormFieldSource {
            column: &columns[3],
            css_classes: "",
            hint: "",
            data: InputData::Select {
                value: priority_value,
                required: false,
                options: priority_options,
            },
        },
        FormFieldSource {
            column: &columns[4],
            css_classes: "",
            hint: "",
            data: InputData::Select {
                value: assignee_value,
                required: false,
                options: assignee_options,
            },
        },
        FormFieldSource {
            column: &columns[5],
            css_classes: "",
            hint: "",
            data: InputData::Date {
                value: start_date,
                required: false,
            },
        },
        FormFieldSource {
            column: &columns[6],
            css_classes: "",
            hint: "",
            data: InputData::Date {
                value: due_date,
                required: false,
            },
        },
        FormFieldSource {
            column: &columns[7],
            css_classes: "",
            hint: "",
            data: InputData::Number {
                value: estimated_hours,
                required: false,
                step: "0.01".to_string(),
            },
        },
        FormFieldSource {
            column: &columns[8],
            css_classes: "",
            hint: "",
            data: InputData::Range {
                value: wp.done_ratio.clamp(0, 100).to_string(),
                min: 0,
                max: 100,
                step: 5,
                suffix: "%".to_string(),
            },
        },
        FormFieldSource {
            column: &columns[9],
            css_classes: "",
            hint: "",
            data: InputData::TextArea {
                value: description,
                rows: 6,
                required: false,
                placeholder: String::new(),
            },
        },
        FormFieldSource {
            column: &columns[10],
            css_classes: "",
            hint: "",
            data: InputData::Hidden {
                value: wp.lock_version.to_string(),
            },
        },
    ];

    let src = FormSource {
        method: "post",
        action: &action,
        // Signed double-submit token (see `write_guard`): the POST handler
        // rejects a missing or wrong `authenticity_token` with 403. An empty
        // token would make the form emit no field at all, so callers always
        // pass a real one.
        csrf_token,
        record_id: Some(wp.id as u64),
        legend: &legend,
        submit_label: "Save",
        cancel_label: "Cancel",
        cancel_href: &cancel_href,
        fields,
    };

    let concept = class.canonical_concept.clone().unwrap_or_default();
    let class_id = canonical_concept_id(&concept).unwrap_or(0);

    render_form(class_id, &concept, &src)
        .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>"))
}

/// POST `/work_packages/:id/edit` — apply the submitted edit form and
/// redirect back to the detail page. `existing` is re-fetched (rather
/// than trusted from the form) both to 404 cleanly on a deleted record
/// and to supply [`parse_wp_update`] with the family-edge fields the
/// edit form doesn't surface.
///
/// Guards, in order, each answering before anything is written: missing
/// record 404 / lookup failure 500, not allowed to edit 403, bad CSRF token
/// 403, malformed field 422.
pub async fn work_package_update(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    who: Option<Extension<Who>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("Update work package", "this action").into_response();
    };
    let who = who.map(|Extension(w)| w);
    let ctx = WriteCtx::from_request(who.as_ref(), &headers);
    update_core(&PgWpStore { pool }, id, &ctx, &form).await
}

/// Core of [`work_package_update`], generic over the store so every guard is testable without a
/// DB. `store.update_unit_of_work` is reached only after all of them pass.
async fn update_core<S: WpStore>(
    store: &S,
    id: i64,
    ctx: &WriteCtx<'_>,
    form: &HashMap<String, String>,
) -> Response {
    // Authorization first, through the boundary the JSON API uses; the write is reachable only
    // through the ticket it returns, and only after CSRF and parsing pass.
    let caller = write_guard::caller_of(ctx.who, ctx.allow_anonymous);
    let page_store = PageStore(store);
    let ticket = match mutation::authorize_update(&page_store, &caller, id).await {
        Ok(ticket) => ticket,
        Err(e) => return mutation_error_page(e, id),
    };
    if !ctx.csrf_ok(form) {
        return forbidden("Missing or invalid form token. Reload the edit page and try again.");
    }
    let dto = match parse_wp_update(form, ticket.existing()) {
        Ok(dto) => dto,
        Err(e) => return unprocessable(id, &e),
    };

    match ticket.commit(&page_store, dto).await {
        Ok(_) => Redirect::to(&format!("/work_packages/{id}")).into_response(),
        Err(e) => mutation_error_page(e, id),
    }
}

/// A submitted field that is present but unreadable.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldError {
    /// The form key, e.g. `start_date`.
    field: &'static str,
    /// What was submitted.
    value: String,
    /// What the field accepts, in words.
    expected: &'static str,
}

/// HTTP 422 naming the offending field; nothing was saved.
fn unprocessable(id: i64, e: &FieldError) -> Response {
    let body = format!(
        "<main><p class=\"empty\">Could not save: <strong>{caption}</strong> (<code>{field}</code>) \
is not valid. Expected {expected}; got “{value}”. No changes were saved.</p>\
<p><a href=\"/work_packages/{id}/edit\">← Back to the form</a></p></main>",
        caption = esc(caption_for(e.field)),
        field = esc(e.field),
        expected = esc(e.expected),
        value = esc(&e.value),
    );
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Html(page_shell("Invalid work package field", &body)),
    )
        .into_response()
}

/// Read one optional form field. Blank (or absent) is `Ok(None)`; a non-blank value that `p`
/// cannot read is an error naming the field — never silently `None`.
fn parse_field<T>(
    form: &HashMap<String, String>,
    key: &'static str,
    expected: &'static str,
    p: fn(&str) -> Option<T>,
) -> Result<Option<T>, FieldError> {
    match form.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(raw) => p(raw).map(Some).ok_or_else(|| FieldError {
            field: key,
            value: raw.to_string(),
            expected,
        }),
    }
}

/// Parse a submitted work-package edit form into an
/// [`UpdateWorkPackageDto`] (pure — no DB access). `existing` supplies
/// the family-edge fields the edit form does NOT surface
/// (`responsible_id` / `parent_id` / `version_id` / `category_id`):
/// unlike `subject`/`status_id`/`type_id`/`priority_id`/`done_ratio`,
/// `WorkPackageRepository::update`'s `UPDATE` does **not** `COALESCE`
/// those four columns (op-db `work_packages.rs::update`, roughly
/// lines 339-391 — `assigned_to_id`/`responsible_id`/`start_date`/
/// `due_date`/`estimated_hours`/`parent_id`/`version_id`/`category_id`
/// are bound straight into the `SET` list), so a bare `None` in the DTO
/// would silently NULL them out on every save. Carrying the current
/// value forward for the ones this form doesn't expose avoids that;
/// `assigned_to_id`/`start_date`/`due_date`/`estimated_hours` (which
/// ARE on the form) intentionally pass their parsed `Option` straight
/// through, since an empty field there is meant to clear the value.
fn parse_wp_update(
    form: &HashMap<String, String>,
    existing: &op_db::work_packages::WorkPackageRow,
) -> Result<UpdateWorkPackageDto, FieldError> {
    let non_empty = |k: &str| form.get(k).map(|s| s.trim()).filter(|s| !s.is_empty());
    let int: fn(&str) -> Option<i64> = |s| s.parse().ok();
    let date: fn(&str) -> Option<chrono::NaiveDate> =
        |s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();

    let subject = non_empty("subject").map(str::to_string);
    let description = non_empty("description").map(str::to_string);
    let type_id = parse_field(form, "type_id", "a whole number", int)?;
    let status_id = parse_field(form, "status_id", "a whole number", int)?;
    let priority_id = parse_field(form, "priority_id", "a whole number", int)?;
    let assigned_to_id = parse_field(form, "assigned_to_id", "a whole number", int)?;
    let start_date = parse_field(form, "start_date", "a date as YYYY-MM-DD", date)?;
    let due_date = parse_field(form, "due_date", "a date as YYYY-MM-DD", date)?;
    let estimated_hours = parse_field(form, "estimated_hours", "a number", |s| {
        s.parse::<f64>().ok().filter(|h| h.is_finite())
    })?;
    let done_ratio = parse_field(form, "done_ratio", "a whole number", |s| {
        s.parse::<i32>().ok()
    })?;
    let lock_version = parse_field(form, "lock_version", "a whole number", |s| {
        s.parse::<i32>().ok()
    })?
    .unwrap_or(existing.lock_version);

    Ok(UpdateWorkPackageDto {
        subject,
        description,
        type_id,
        status_id,
        priority_id,
        assigned_to_id,
        responsible_id: existing.responsible_id,
        start_date,
        due_date,
        estimated_hours,
        done_ratio,
        parent_id: existing.parent_id,
        version_id: existing.version_id,
        category_id: existing.category_id,
        lock_version,
    })
}

// ── Project index ────────────────────────────────────────────────────

/// GET `/projects` — the project list page. Closes the `nav::menu()`
/// "Projects" link's connectivity gap: without this route, the
/// menu-derived link points at a 404.
pub async fn project_index(State(state): State<Arc<AppState>>) -> Html<String> {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("Projects", "the project list");
    };

    let projects = ProjectRepository::new(pool)
        .find_all(200, 0)
        .await
        .unwrap_or_default();

    let fragment = render_project_index(&projects);

    Html(page_shell(
        "Projects",
        &format!("{BREADCRUMB}<main>{fragment}</main>"),
    ))
}

/// Render the project index fragment (pure — no DB access). `name`
/// (the headline column) is prepended by hand, same as `id` — neither
/// is mask-selected because both are structural (record identity +
/// primary label), not optional display fields; the mask drives the
/// `identifier`/`public`/`active` tail per [`PROJECT_INDEX_ORDER`].
fn render_project_index(projects: &[ProjectRow]) -> String {
    let class = project_class();
    let basis = project_basis();
    let index = skin(&basis, PROJECT_INDEX_ORDER);
    let mut inline_columns: Vec<RenderColumn> = vec![
        RenderColumn::new("id", "#", ColumnKind::IdLink),
        RenderColumn::new("name", caption_for("name"), ColumnKind::PrimaryLink).sortable(),
    ];
    inline_columns.extend(columns_for(&basis, &index.mask, index.order, &class));

    struct OwnedProjectRow {
        id: u64,
        name: String,
        href: String,
        identifier: String,
        public: &'static str,
        active: &'static str,
    }

    let owned: Vec<OwnedProjectRow> = projects
        .iter()
        .map(|p| OwnedProjectRow {
            id: p.id as u64,
            name: p.name.clone(),
            href: format!("/projects/{}", p.id),
            identifier: p.identifier.clone(),
            public: if p.public { "yes" } else { "no" },
            active: if p.active { "yes" } else { "no" },
        })
        .collect();

    let rows: Vec<RowSource> = owned
        .iter()
        .map(|o| RowSource {
            record_id: o.id,
            css_classes: "",
            group: None,
            inline: vec![
                CellSource {
                    column: &inline_columns[0],
                    css_classes: "",
                    data: CellData::IdLink {
                        id: o.id,
                        href: &o.href,
                    },
                },
                CellSource {
                    column: &inline_columns[1],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &o.name,
                        href: &o.href,
                    },
                },
                CellSource {
                    column: &inline_columns[2],
                    css_classes: "",
                    data: CellData::Plain {
                        value: &o.identifier,
                    },
                },
                CellSource {
                    column: &inline_columns[3],
                    css_classes: "",
                    data: CellData::Plain { value: o.public },
                },
                CellSource {
                    column: &inline_columns[4],
                    css_classes: "",
                    data: CellData::Plain { value: o.active },
                },
            ],
            block: vec![],
        })
        .collect();

    let concept = class.canonical_concept.clone().unwrap_or_default();
    let class_id = canonical_concept_id(&concept).unwrap_or(0);

    render_list(
        "Projects",
        class_id,
        &concept,
        &inline_columns,
        &Vec::new(),
        &rows,
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

    let Some(row) = ProjectRepository::new(pool.clone())
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

    let mut fragment = render_project_detail(&row);

    // Topology (B.2): `work_items` is a `has_many` edge on `project()`
    // whose target (`ProjectWorkItem`) resolves to a live route — stack
    // the child list view under the parent detail, Lego-style, instead
    // of leaving the reader to navigate away and filter by hand.
    if nav::route_for("ProjectWorkItem").is_some() {
        let items = op_db::WorkPackageRepository::new(pool)
            .find_by_project(id, op_db::repository::Pagination::new(500, 0))
            .await
            .map(|p| p.items)
            .unwrap_or_default();
        fragment.push_str(&render_project_work_items_stack(&items));
    }

    let toolbar = detail_toolbar(&format!("/projects/{id}/edit"));
    Html(page_shell(
        &format!("Project — {}", esc(&row.name)),
        &format!("{toolbar}<main>{fragment}</main>"),
    ))
}

/// Render the "stacked" child list of a project's work packages —
/// embedded below the project's own detail fields (see
/// [`project_detail`]). Uses the [`wp_stacked_skin`] mask (the board
/// skin minus its `project` column, since the parent context already
/// names the project). Pure — no DB access — so it's testable with
/// fake rows, mirroring every other render fn in this module.
fn render_project_work_items_stack(wps: &[op_db::work_packages::WorkPackageRow]) -> String {
    struct StackedRow {
        id: u64,
        subject: String,
        wp_href: String,
        done: u8,
    }

    let class = project_work_item();
    let basis = wp_basis();
    let stacked = wp_stacked_skin(&basis);
    let mut inline_columns: Vec<RenderColumn> =
        vec![RenderColumn::new("id", "#", ColumnKind::IdLink)];
    inline_columns.extend(columns_for(&basis, &stacked.mask, stacked.order, &class));

    let owned: Vec<StackedRow> = wps
        .iter()
        .map(|wp| StackedRow {
            id: wp.id as u64,
            subject: wp.subject.clone(),
            wp_href: format!("/work_packages/{}", wp.id),
            done: wp.done_ratio.clamp(0, 100) as u8,
        })
        .collect();

    let rows: Vec<RowSource> = owned
        .iter()
        .map(|o| RowSource {
            record_id: o.id,
            css_classes: "",
            group: None,
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
                    data: CellData::ProgressBar { pct: o.done },
                },
            ],
            block: vec![],
        })
        .collect();

    let concept = class.canonical_concept.clone().unwrap_or_default();
    let class_id = canonical_concept_id(&concept).unwrap_or(0);

    let list_html = render_list(
        "Work items",
        class_id,
        &concept,
        &inline_columns,
        &Vec::new(),
        &rows,
    )
    .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>"));

    format!("<section class=\"stacked-list\">{list_html}</section>")
}

/// Render the project detail fragment (pure — no DB access). Kept
/// separate from the async handler so it's testable without a live DB
/// pool (mirrors [`render_wp_detail`] / [`render_board`]).
fn render_project_detail(row: &ProjectRow) -> String {
    let class = project_class();
    let basis = project_basis();
    let detail = skin(&basis, PROJECT_DETAIL_ORDER);
    let mut columns: Vec<RenderColumn> = vec![RenderColumn::new("id", "#", ColumnKind::IdLink)];
    columns.extend(columns_for(&basis, &detail.mask, detail.order, &class));

    let project_href = format!("/projects/{}", row.id);
    let public_label = if row.public { "yes" } else { "no" };
    let active_label = if row.active { "yes" } else { "no" };
    let created_label = row.created_at.to_rfc3339();
    let description_html = format!("<p>{}</p>", esc(row.description.as_deref().unwrap_or("—")));

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

// ── Project edit ─────────────────────────────────────────────────────

/// GET `/projects/:id/edit` — the render-bake `HtmlForm` edit page,
/// pre-filled from the record. Requires `edit_project` in this project (403
/// otherwise); the form carries a CSRF token bound to this browser and actor.
pub async fn project_edit_form(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    who: Option<Extension<Who>>,
) -> Response {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("Edit project", "this page").into_response();
    };
    let who = who.map(|Extension(w)| w);
    let ctx = WriteCtx::from_request(who.as_ref(), &headers);

    let row = match lookup(
        ProjectRepository::new(pool.clone()).find_by_id(id).await,
        "Project",
        id,
    ) {
        Ok(row) => row,
        Err(resp) => return resp,
    };
    if let Err(resp) = authorize_project(&pool, &ctx, &row, EDIT_PROJECT).await {
        return resp;
    }

    let existing = write_guard::cookie_from_headers(ctx.headers);
    let (cookie, fresh) = match existing {
        Some(c) => (c, false),
        None => (write_guard::new_cookie_value(), true),
    };
    let token = write_guard::token_for(ctx.secret, &cookie, &actor_key(ctx.who));

    let fragment = render_project_edit(&row, &token);
    let mut resp = Html(page_shell(
        &format!("Edit project #{id}"),
        &format!("{BREADCRUMB}<main>{fragment}</main>"),
    ))
    .into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if fresh {
        if let Ok(v) =
            HeaderValue::from_str(&write_guard::set_cookie_value(&cookie, ctx.secure_cookie))
        {
            resp.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    resp
}

/// Require `permission` in the project (OpenProject's project-scoped `allowed_in_project?`).
#[expect(
    clippy::result_large_err,
    reason = "the Err is the ready-to-send error page, returned straight to the handler"
)]
async fn authorize_project(
    pool: &sqlx::PgPool,
    ctx: &WriteCtx<'_>,
    project: &ProjectRow,
    permission: &str,
) -> Result<(), Response> {
    let principal: Principal =
        write_guard::load_principal(pool, ctx.who, project.id, ctx.allow_anonymous)
            .await
            .map_err(|e: StoreError| internal_error(e))?;
    if write_guard::may(&principal, project.active, permission) {
        Ok(())
    } else {
        Err(forbidden("You are not allowed to edit this project."))
    }
}

/// Render the project edit form fragment (pure — no DB access). Kept
/// separate from the async handler so it's testable without a live DB
/// pool (mirrors [`render_wp_edit`] / [`render_project_detail`]).
fn render_project_edit(row: &ProjectRow, csrf_token: &str) -> String {
    let class = project_class();
    let basis = project_basis();
    let form = skin(&basis, PROJECT_FORM_ORDER);
    let columns: Vec<RenderColumn> = columns_for(&basis, &form.mask, form.order, &class);

    let action = format!("/projects/{}/edit", row.id);
    let legend = format!("Edit project #{}", row.id);
    let cancel_href = format!("/projects/{}", row.id);
    let description = row.description.clone().unwrap_or_default();

    let fields: Vec<FormFieldSource<'_>> = vec![
        FormFieldSource {
            column: &columns[0],
            css_classes: "",
            hint: "",
            data: InputData::Text {
                value: row.name.clone(),
                required: true,
                placeholder: String::new(),
            },
        },
        FormFieldSource {
            column: &columns[1],
            css_classes: "",
            hint: "",
            data: InputData::Checkbox {
                checked: row.public,
            },
        },
        FormFieldSource {
            column: &columns[2],
            css_classes: "",
            hint: "",
            data: InputData::Checkbox {
                checked: row.active,
            },
        },
        FormFieldSource {
            column: &columns[3],
            css_classes: "",
            hint: "",
            data: InputData::TextArea {
                value: description,
                rows: 6,
                required: false,
                placeholder: String::new(),
            },
        },
    ];

    let src = FormSource {
        method: "post",
        action: &action,
        // Signed double-submit token — see `render_wp_edit` and `write_guard`.
        csrf_token,
        record_id: Some(row.id as u64),
        legend: &legend,
        submit_label: "Save",
        cancel_label: "Cancel",
        cancel_href: &cancel_href,
        fields,
    };

    let concept = class.canonical_concept.clone().unwrap_or_default();
    let class_id = canonical_concept_id(&concept).unwrap_or(0);

    render_form(class_id, &concept, &src)
        .unwrap_or_else(|e| format!("<pre>render error: {e}</pre>"))
}

/// POST `/projects/:id/edit` — apply the submitted edit form and
/// redirect back to the detail page. Guards, each before any write: missing
/// project 404 / lookup failure 500, no `edit_project` 403, bad CSRF token 403.
pub async fn project_update(
    Path(id): Path<i64>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    who: Option<Extension<Who>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let Some(pool) = state.db.clone() else {
        return not_connected_page("Update project", "this action").into_response();
    };
    let who = who.map(|Extension(w)| w);
    let ctx = WriteCtx::from_request(who.as_ref(), &headers);

    let repo = ProjectRepository::new(pool.clone());
    let existing = match lookup(repo.find_by_id(id).await, "Project", id) {
        Ok(row) => row,
        Err(resp) => return resp,
    };
    if let Err(resp) = authorize_project(&pool, &ctx, &existing, EDIT_PROJECT).await {
        return resp;
    }
    if !ctx.csrf_ok(&form) {
        return forbidden("Missing or invalid form token. Reload the edit page and try again.");
    }
    let dto = parse_project_update(&form);

    match repo.update(id, dto).await {
        Ok(_) => Redirect::to(&format!("/projects/{id}")).into_response(),
        Err(e) => internal_error(e),
    }
}

/// Parse a submitted project edit form into an [`UpdateProjectDto`]
/// (pure — no DB access).
///
/// Checkbox fields ride the "hidden zero" idiom
/// (`dispatch/input/checkbox.askama:1-4`): the render-bake checkbox
/// input always emits a paired `<input type="hidden" name="{name}"
/// value="0">` immediately before the real `<input type="checkbox">`,
/// so an unchecked box still POSTs `<name>=0` (last-value-wins with the
/// checkbox's `1` when checked) — the field is therefore always present
/// in `form`, and the value (not mere key presence) tells checked vs
/// unchecked.
///
/// `parent_id` is intentionally left `None`: `ProjectRepository::update`
/// (op-db `projects.rs::update`, lines 398-422) never reads
/// `UpdateProjectDto::parent_id` in its `SET` list, so there's nothing
/// to preserve or clear here.
fn parse_project_update(form: &HashMap<String, String>) -> UpdateProjectDto {
    let name = form
        .get("name")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let description = form
        .get("description")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let public = form.get("public").map(|v| v == "1");
    let active = form.get("active").map(|v| v == "1");

    UpdateProjectDto {
        name,
        description,
        public,
        parent_id: None,
        active,
    }
}

/// Wrap a rendered body fragment in a full, self-contained HTML page —
/// no external assets, readable in light and dark, kanban-ish styling on
/// top of the `html_list_view` spine markup (`.ogar-list`, `table.list`,
/// `tr.group`, `.progress`).
fn page_shell(title: &str, body_html: &str) -> String {
    page_shell_with(title, body_html, &layout::default_region_mask())
}

/// [`page_shell`] with an explicit region mask. Layer-0 region composition:
/// which chrome regions render is a masked ClassView (crate::layout). The
/// center zone is always `body_html`; the header, side menu, right menu, and
/// footer are gated by the mask. Zones use the shared `a2ui-layout`
/// vocabulary: op `header` = `TopBar`, `left_menu` = `LeftNav`, `main` =
/// `Center`, `right_menu` = `RightPanel`, `footer` = `BottomBar`.
/// Theme rules on top of the shared `a2ui-layout` grid. Pages use
/// [`LayoutMode::MainWide`] (210px nav | center | 70px quick menu; at 1920px
/// the center is 2 x 820). The shared grid only covers widths that fit the
/// chrome; below that a wrap breakpoint is the theme's job (`Budget::for_viewport`
/// docs): at 640px and below the side menu is hidden and the center is the
/// only column, the top bar carrying the same live links ([`nav::menu`]). A
/// page without the side menu would otherwise put its center into the 210px
/// slot, so its grid drops that column.
fn body_grid_css(regions: &WideFieldMask) -> String {
    let mut css = String::new();
    if !layout::region_on(regions, "left_menu") {
        css.push_str(&format!(
            "  .a2-body.{} {{ grid-template-columns: minmax(0, 1fr) {RIGHT_SLIM_PX}px; }}\n",
            LayoutMode::MainWide.class()
        ));
    }
    css.push_str(
        "  @media (max-width: 640px) { .a2-body, .a2-body.mode-main-wide { grid-template-columns: minmax(0, 1fr); } \
         aside.sidemenu-region { display: none; } }\n",
    );
    css
}

fn page_shell_with(title: &str, body_html: &str, regions: &WideFieldMask) -> String {
    let header_region = if layout::region_on(regions, "header") {
        format!(
            "<nav class=\"{} topnav\"><a href=\"/\" class=\"topnav-brand\">OpenProject RS</a>{}</nav>",
            Region::TopBar.marker(),
            topnav_menu_links(),
        )
    } else {
        String::new()
    };
    let left_menu_region = if layout::region_on(regions, "left_menu") {
        format!(
            "<aside class=\"{} sidemenu-region\">{}</aside>",
            Region::LeftNav.marker(),
            layout::left_menu_html()
        )
    } else {
        String::new()
    };
    let right_menu_region = if layout::region_on(regions, "right_menu") {
        format!(
            "<aside class=\"{} sidemenu-region rightmenu\"></aside>",
            Region::RightPanel.marker()
        )
    } else {
        String::new()
    };
    let footer_region = if layout::region_on(regions, "footer") {
        format!(
            "<footer class=\"{} page-footer\">{}</footer>",
            Region::BottomBar.marker(),
            layout::footer_html()
        )
    } else {
        String::new()
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
{zones_css}
{body_grid}
  :root {{ color-scheme: light dark; }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    background: #f4f5f7;
    color: #172b4d;
  }}
  nav.topnav {{
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 1.25rem;
    background: #091e42;
    color: #b3bac5;
    font-size: 0.85rem;
  }}
  nav.topnav a {{ color: #b3bac5; }}
  nav.topnav a.topnav-brand {{ color: #fff; font-weight: 700; }}
  header {{ padding: 1rem 1.5rem; background: #0747a6; color: #fff; }}
  header h1 {{ margin: 0 0 0.25rem; font-size: 1.35rem; }}
  header p {{ margin: 0; font-size: 0.85rem; opacity: 0.85; }}
  main {{ padding: 1.25rem; overflow-x: auto; }}
  .empty {{ padding: 3rem 1rem; text-align: center; color: #6b778c; }}
  .breadcrumb {{ margin: 0; padding: 0.6rem 1.25rem 0; font-size: 0.85rem; }}
  .breadcrumb .edit-link {{ margin-left: 0.25rem; }}
  .ogar-form fieldset.form-fields {{ border: none; padding: 0; margin: 0 0 1rem; }}
  .ogar-form legend {{ font-weight: 700; font-size: 1.1rem; padding: 0; margin-bottom: 0.5rem; }}
  .ogar-form .form-field {{ margin-bottom: 0.85rem; display: flex; flex-direction: column; max-width: 32rem; }}
  .ogar-form label {{ font-weight: 600; font-size: 0.8rem; margin-bottom: 0.25rem; }}
  .ogar-form input, .ogar-form select, .ogar-form textarea {{
    font: inherit;
    padding: 0.4rem 0.5rem;
    border: 1px solid #dfe1e6;
    border-radius: 3px;
    background: #fff;
    color: inherit;
  }}
  .ogar-form .hint {{ color: #6b778c; font-size: 0.75rem; margin-top: 0.2rem; }}
  .ogar-form .form-actions {{ display: flex; gap: 0.75rem; align-items: center; margin-top: 1rem; }}
  .ogar-form .btn-primary {{
    background: #0052cc; color: #fff; border: none; border-radius: 3px;
    padding: 0.45rem 1rem; font-weight: 600; cursor: pointer;
  }}
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
  aside.sidemenu-region {{ flex: 0 0 13rem; background: #fff; border-right: 1px solid #dfe1e6; padding: 0.75rem 0; }}
  .sidemenu ul {{ list-style: none; margin: 0; padding: 0; }}
  .sidemenu-primary a {{ display: block; padding: 0.5rem 1.25rem; color: #172b4d; font-weight: 600; }}
  .sidemenu-primary a:hover {{ background: #ebecf0; text-decoration: none; }}
  .sidemenu-soon-label {{ padding: 0.75rem 1.25rem 0.25rem; font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; color: #6b778c; }}
  .sidemenu-soon li span {{ display: block; padding: 0.35rem 1.25rem; color: #a5adba; font-size: 0.85rem; cursor: not-allowed; }}
  footer.page-footer {{ padding: 0.75rem 1.5rem; background: #091e42; color: #7a869a; font-size: 0.75rem; }}
  @media (prefers-color-scheme: dark) {{
    aside.sidemenu-region {{ background: #22272b; border-right-color: #3a3f45; }}
    .sidemenu-primary a {{ color: #b6c2cf; }}
    .sidemenu-primary a:hover {{ background: #2c333a; }}
  }}
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
<div class="a2-shell">{header}<div class="a2-body {mode}">{left_menu}<div class="{center}">{body_html}</div>{right_menu}</div>{footer}</div>
</body>
</html>"#,
        zones_css = ZONES_CSS,
        body_grid = body_grid_css(regions),
        mode = LayoutMode::MainWide.class(),
        center = Region::Center.marker(),
        header = header_region,
        left_menu = left_menu_region,
        right_menu = right_menu_region,
        footer = footer_region,
    )
}

/// The top-nav link list, built from [`nav::menu()`] — every link the
/// menu carries is guaranteed routable (see nav's own
/// `menu_targets_are_all_routable` test), so a page can never link to
/// a 404 through here.
fn topnav_menu_links() -> String {
    nav::menu()
        .iter()
        .map(|(label, href)| format!("<a href=\"{href}\">{}</a>", esc(label)))
        .collect::<String>()
}

#[cfg(test)]
mod executed_db;

#[cfg(test)]
mod tests {
    use super::*;

    /// Pages use the shared main-wide layout: 210px nav, the center, a 70px
    /// quick-menu rail (1920 = 210 + 2 x 820 + 70). No page-local column
    /// override while the side menu renders; without it the center does not
    /// fall into the 210px slot.
    #[test]
    fn pages_use_the_main_wide_layout_and_own_no_columns_while_the_menu_renders() {
        let html = page_shell("t", "<p>x</p>");
        assert!(
            html.contains("<div class=\"a2-body mode-main-wide\">"),
            "{html}"
        );
        assert!(html.contains(
            ".a2-body.mode-main-wide { grid-template-columns: 210px minmax(0, 1fr) 70px; }"
        ));
        let default = body_grid_css(&layout::default_region_mask());
        assert!(
            !default.contains("grid-template-columns: 210px"),
            "{default}"
        );
        assert!(!default.contains("270px"), "{default}");

        let no_left = body_grid_css(&layout::region_mask(&["global", "header", "main"]));
        assert!(
            no_left.contains(
                ".a2-body.mode-main-wide { grid-template-columns: minmax(0, 1fr) 70px; }"
            ),
            "{no_left}"
        );
    }

    /// At phone width the side menu is hidden and the center is the only
    /// column; the rule comes after `ZONES_CSS` so it overrides the shared grid.
    #[test]
    fn a_phone_width_page_is_one_column_and_the_override_follows_the_shared_grid() {
        let html = page_shell("t", "<p>x</p>");
        let phone = "@media (max-width: 640px) { .a2-body, .a2-body.mode-main-wide { grid-template-columns: minmax(0, 1fr); } aside.sidemenu-region { display: none; } }";
        assert!(html.contains(phone), "{html}");
        let zones = html.find(ZONES_CSS).expect("shared grid present");
        let ours = html.find(phone).expect("phone rule present");
        assert!(
            ours > zones,
            "the override must come after the shared grid to win"
        );
    }
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
        assert!(html.contains(">40%<"), "{html}");
        // The link rewrite: project cell links to /projects/{id}, NOT
        // /api/v3/projects/{id}.
        assert!(html.contains("href=\"/projects/10\""), "{html}");
        assert!(!html.contains("/api/v3/projects/10"), "{html}");
        // The record's own id link points at /work_packages/{id}.
        assert!(html.contains("/work_packages/101"), "{html}");
    }

    fn detail_html() -> String {
        let mut wp = work_package(101, "Fix the login bug", 10, 2, 40);
        wp.estimated_hours = Some(3.5);
        render_wp_detail(
            &wp,
            "In Progress",
            "Nexgen",
            "Bug",
            "High",
            "Jane Doe",
            "John Smith",
        )
    }

    fn basis_pos(name: &str) -> usize {
        wp_basis()
            .iter()
            .position(|b| b == name)
            .unwrap_or_else(|| panic!("{name} not in wp basis"))
    }

    /// The detail page is OGAR's addressed field view: one section, one
    /// `data-field-pos` per emitted field, equal to its basis index.
    #[test]
    fn detail_is_an_addressed_fieldview_at_basis_positions() {
        let html = detail_html();
        assert_eq!(
            html.matches("class=\"ogar-fieldview\"").count(),
            1,
            "{html}"
        );
        for name in ["status", "estimated_hours", "start_date", "description"] {
            let pos = basis_pos(name);
            assert!(
                html.contains(&format!("data-field-pos=\"{pos}\"")),
                "{name} at {pos} missing: {html}"
            );
        }
        // Exactly the fields of the split table, no more.
        assert_eq!(
            html.matches("class=\"fieldview-field\" data-field-pos=")
                .count(),
            WP_FIELD_SPLIT.len(),
            "{html}"
        );
    }

    /// Left fields land in grid column 1, right fields in column 2.
    #[test]
    fn detail_split_css_places_left_and_right_columns() {
        let html = detail_html();
        let status = basis_pos("status");
        let hours = basis_pos("estimated_hours");
        assert!(
            html.contains(&format!(
                "[data-field-pos=\"{status}\"]{{grid-column:1;grid-row:4}}"
            )),
            "{html}"
        );
        assert!(
            html.contains(&format!(
                "[data-field-pos=\"{hours}\"]{{grid-column:2;grid-row:7}}"
            )),
            "{html}"
        );
        // Three full-width rows (type/project/author) precede the split, so
        // status is row 4 and estimated_hours (4th right field) row 7.
        // Full-width group (type) spans both columns in row 1.
        let ty = basis_pos("type");
        assert!(
            html.contains(&format!(
                "[data-field-pos=\"{ty}\"]{{grid-column:1 / -1;grid-row:1}}"
            )),
            "{html}"
        );
    }

    /// The mask is the gate (Redmine's `disabled_core_fields`): a field
    /// whose bit is off is not placed.
    #[test]
    fn detail_mask_gates_fields() {
        let basis = wp_basis();
        let pos = basis_pos("priority");
        assert!(WP_FIELD_SPLIT.iter().any(|(n, _, _)| *n == "priority"));
        let universe: Vec<&str> = basis.iter().map(String::as_str).collect();
        let without: Vec<&str> = WP_DETAIL_ORDER
            .iter()
            .copied()
            .filter(|n| *n != "priority")
            .collect();
        let mask = WideFieldMask::from_universe_present(&universe, &without).unwrap();
        let placed = wp_split_layout(&basis, &mask);
        assert!(placed.iter().all(|p| p.pos as usize != pos), "{placed:?}");
        let css = wp_split_css(&placed);
        assert!(!css.contains(&format!("data-field-pos=\"{pos}\"")), "{css}");
        // And with the full mask it IS placed (anti-vacuity).
        let full = skin(&basis, WP_DETAIL_ORDER);
        assert!(wp_split_layout(&basis, &full.mask)
            .iter()
            .any(|p| p.pos as usize == pos));
    }

    /// Untrusted subject / description text is escaped, never raw.
    #[test]
    fn detail_escapes_script_in_subject_and_description() {
        let mut wp = work_package(7, "<script>alert(1)</script>", 10, 2, 0);
        wp.description = Some("<script>x()</script>".to_string());
        let html = render_wp_detail(&wp, "New", "P", "Bug", "High", "A", "B");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;alert(1)"), "{html}");
        assert!(html.contains("&lt;script&gt;x()"), "{html}");
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

    fn wp_type(id: i64, name: &str) -> TypeRow {
        TypeRow {
            id,
            name: name.to_string(),
            position: 1,
            is_default: false,
            is_in_roadmap: true,
            is_milestone: false,
            is_standard: true,
            color_id: None,
            description: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn priority(id: i64, name: &str) -> PriorityRow {
        PriorityRow {
            id,
            name: name.to_string(),
            position: 1,
            is_default: false,
            active: true,
            color_id: None,
            project_id: None,
            parent_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn user(id: i64, firstname: &str, lastname: &str) -> UserRow {
        UserRow {
            id,
            login: format!("{firstname}.{lastname}").to_lowercase(),
            firstname: firstname.to_string(),
            lastname: lastname.to_string(),
            mail: format!("{firstname}@example.com").to_lowercase(),
            admin: false,
            status: 1,
            language: None,
            hashed_password: None,
            salt: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_login_on: None,
        }
    }

    /// Proves the edit-form render path (`render_form` / `HtmlForm`)
    /// works end-to-end without a DB: fake row + family-edge lookup
    /// tables in, HTML containing a `<form>`, a `<select>` with the
    /// current status pre-selected, the subject's current value, a
    /// `<textarea>` for description, and the `lock_version` round-trip
    /// hidden field out.
    #[test]
    fn render_wp_edit_form_has_selects_and_values() {
        let mut wp = work_package(101, "Fix the login bug", 10, 2, 40);
        wp.description = Some("Some rich description".to_string());
        wp.lock_version = 3;

        let statuses = vec![status(1, "New", 1), status(2, "In Progress", 2)];
        let types = vec![wp_type(1, "Bug"), wp_type(2, "Feature")];
        let priorities = vec![priority(4, "Low"), priority(5, "High")];
        let users = vec![user(7, "Jane", "Doe"), user(8, "John", "Smith")];

        let html = render_wp_edit(&wp, &statuses, &types, &priorities, &users, "tok");

        assert!(html.contains("<form"), "{html}");
        assert!(html.contains("method=\"post\""), "{html}");
        assert!(
            html.contains("action=\"/work_packages/101/edit\""),
            "{html}"
        );
        assert!(html.contains("<select"), "{html}");
        // Current status (id 2, "In Progress") is pre-selected.
        assert!(
            html.contains("<option value=\"2\" selected>In Progress</option>"),
            "{html}"
        );
        // The other status is present but not selected.
        assert!(html.contains("<option value=\"1\">New</option>"), "{html}");
        // Subject carries its current value.
        assert!(html.contains("value=\"Fix the login bug\""), "{html}");
        // Description renders as a <textarea>, not a plain input.
        assert!(html.contains("<textarea"), "{html}");
        assert!(html.contains("Some rich description"), "{html}");
        // lock_version round-trips as a hidden field for the optimistic-
        // locking WHERE clause in `WorkPackageRepository::update`.
        assert!(
            html.contains("<input type=\"hidden\" name=\"lock_version\" value=\"3\">"),
            "{html}"
        );
        // No priority/assignee selected (both None) — the select
        // template's own blank option is selected instead.
        assert!(
            html.contains("<option value=\"\" selected>—</option>"),
            "{html}"
        );
    }

    /// Given a submitted form + the existing row, `parse_wp_update`
    /// builds the exact `UpdateWorkPackageDto` the repository expects:
    /// parsed edit-form fields on the fields the form exposes, and the
    /// existing row's values carried through untouched on the family-edge
    /// fields (`responsible_id`/`parent_id`/`version_id`/`category_id`)
    /// the form does NOT expose — see the doc comment on
    /// `parse_wp_update` for why (op-db `work_packages.rs::update` does
    /// not `COALESCE` those columns).
    #[test]
    fn parse_wp_update_round_trips_form_fields() {
        let mut existing = work_package(101, "Old subject", 10, 1, 0);
        existing.lock_version = 5;
        existing.responsible_id = Some(9);
        existing.parent_id = Some(50);
        existing.version_id = Some(3);
        existing.category_id = Some(2);

        let mut form: HashMap<String, String> = HashMap::new();
        form.insert("subject".to_string(), "New subject".to_string());
        form.insert("status_id".to_string(), "2".to_string());
        form.insert("type_id".to_string(), "1".to_string());
        form.insert("priority_id".to_string(), "".to_string());
        form.insert("assigned_to_id".to_string(), "7".to_string());
        form.insert("start_date".to_string(), "2026-08-01".to_string());
        form.insert("due_date".to_string(), "".to_string());
        form.insert("estimated_hours".to_string(), "3.5".to_string());
        form.insert("done_ratio".to_string(), "60".to_string());
        form.insert("description".to_string(), "New body".to_string());
        form.insert("lock_version".to_string(), "5".to_string());

        let dto = parse_wp_update(&form, &existing).expect("valid form parses");

        assert_eq!(dto.subject.as_deref(), Some("New subject"));
        assert_eq!(dto.description.as_deref(), Some("New body"));
        assert_eq!(dto.type_id, Some(1));
        assert_eq!(dto.status_id, Some(2));
        assert_eq!(dto.priority_id, None); // blank select -> None
        assert_eq!(dto.assigned_to_id, Some(7));
        assert_eq!(
            dto.start_date,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap())
        );
        assert_eq!(dto.due_date, None); // blank date -> cleared
        assert_eq!(dto.estimated_hours, Some(3.5));
        assert_eq!(dto.done_ratio, Some(60));
        assert_eq!(dto.lock_version, 5);
        // Fields the form doesn't expose are carried through from
        // `existing`, not silently nulled.
        assert_eq!(dto.responsible_id, Some(9));
        assert_eq!(dto.parent_id, Some(50));
        assert_eq!(dto.version_id, Some(3));
        assert_eq!(dto.category_id, Some(2));
    }

    /// Proves the project edit-form render path works end-to-end without
    /// a DB: current name/public/active/description all present in the
    /// rendered HTML, checkboxes carrying their `checked` state.
    #[test]
    fn render_project_edit_form_has_checkboxes_and_values() {
        let mut row = project(10, "Nexgen");
        row.description = Some("A cool project".to_string());
        row.public = false;
        row.active = true;

        let html = render_project_edit(&row, "tok");

        assert!(html.contains("<form"), "{html}");
        assert!(html.contains("action=\"/projects/10/edit\""), "{html}");
        assert!(html.contains("value=\"Nexgen\""), "{html}");
        assert!(html.contains("A cool project"), "{html}");
        // public = false -> checkbox not checked.
        assert!(
            html.contains("name=\"public\" id=\"field-public\" value=\"1\">"),
            "{html}"
        );
        // active = true -> checkbox carries `checked`.
        assert!(
            html.contains("name=\"active\" id=\"field-active\" value=\"1\" checked>"),
            "{html}"
        );
    }

    /// `parse_project_update` reads the checkbox's real value (not mere
    /// key presence) — see the doc comment on `parse_project_update` for
    /// why the "hidden zero" idiom makes that necessary.
    #[test]
    fn parse_project_update_round_trips_form_fields() {
        let mut form: HashMap<String, String> = HashMap::new();
        form.insert("name".to_string(), "Renamed".to_string());
        form.insert("description".to_string(), "New description".to_string());
        form.insert("public".to_string(), "1".to_string());
        form.insert("active".to_string(), "0".to_string());

        let dto = parse_project_update(&form);

        assert_eq!(dto.name.as_deref(), Some("Renamed"));
        assert_eq!(dto.description.as_deref(), Some("New description"));
        assert_eq!(dto.public, Some(true));
        assert_eq!(dto.active, Some(false));
        assert_eq!(dto.parent_id, None);
    }

    // ── FieldMask-driven columns + topology links (this refactor) ────

    /// The board's columns are exactly the mask-selected subset of the
    /// work-package basis, in the skin's declared order — proving the
    /// mask (not a hand-written `vec![...]`) drives which columns
    /// appear.
    #[test]
    fn board_columns_come_from_mask() {
        let basis = wp_basis();
        let board = skin(&basis, WP_BOARD_ORDER);
        let class = project_work_item();
        let columns = columns_for(&basis, &board.mask, board.order, &class);

        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["subject", "project", "done_ratio"]);
    }

    /// The composed page shell carries the Layer-0 regions: the top-nav
    /// header, the left side menu (primary links + the "Coming soon"
    /// deferred lanes), the content, and the footer — the chrome that was
    /// "completely missing" from the read-only preview. `right_menu` is off
    /// by default, so it must NOT appear.
    #[test]
    fn page_shell_composes_layout_regions() {
        let html = page_shell("T", "<main><p>content here</p></main>");
        // The zones CSS names every marker, so check the emitted class
        // attribute (`class="<marker> ..."`), not bare substrings.
        let zone = |html: &str, r: Region| html.contains(&format!("class=\"{} ", r.marker()));
        assert!(html.contains("class=\"a2-shell\""), "shell");
        assert!(html.contains("class=\"a2-body "), "body grid");
        assert!(zone(&html, Region::TopBar), "top bar zone");
        assert!(zone(&html, Region::LeftNav), "left nav zone");
        assert!(
            html.contains(&format!("class=\"{}\"", Region::Center.marker())),
            "center zone"
        );
        assert!(zone(&html, Region::BottomBar), "bottom bar zone");
        assert!(
            html.contains("Coming soon"),
            "deferred lanes shown in side menu"
        );
        assert!(html.contains("content here"), "main content region");
        assert!(
            !zone(&html, Region::RightPanel),
            "right_menu is off by default"
        );
    }

    /// The region mask gates the right panel in both directions: turning
    /// `right_menu` on emits the RightPanel zone; turning `left_menu` off
    /// drops the LeftNav zone.
    #[test]
    fn page_shell_region_mask_gates_right_panel() {
        let zone = |html: &str, r: Region| html.contains(&format!("class=\"{} ", r.marker()));
        let on = layout::region_mask(&["global", "header", "right_menu", "main"]);
        let html = page_shell_with("T", "<p>x</p>", &on);
        assert!(zone(&html, Region::RightPanel), "right panel on");
        assert!(!zone(&html, Region::LeftNav), "left nav off");
        assert!(!zone(&html, Region::BottomBar), "footer off");
        assert!(zone(&html, Region::TopBar), "header on");
    }

    /// `stacked_mask` (via `wp_stacked_skin`) drops `project` from the
    /// board mask — proving the `WideFieldMask::intersect` composition
    /// actually removes the bit rather than just being unused.
    #[test]
    fn stacked_mask_drops_project_via_intersect() {
        let basis = wp_basis();
        let stacked = wp_stacked_skin(&basis);
        let class = project_work_item();
        let columns = columns_for(&basis, &stacked.mask, stacked.order, &class);

        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["subject", "done_ratio"]);

        let board = skin(&basis, WP_BOARD_ORDER);
        assert_eq!(board.mask.count(), 3);
        assert_eq!(stacked.mask.count(), 2);
    }

    /// The app's REAL route skins intern into a ViewRegistry whose
    /// nesting construction order and forward/backward bijective round
    /// trip both hold — the compile/test-time form of the boot self-check
    /// in `main`. Seven distinct nodes today (six leaves + the project
    /// detail stacking the WP list), each route arm's projection its own
    /// interned node (no accidental collapse, no duplicate).
    #[test]
    fn real_skin_registry_is_ordered_and_bijective() {
        let mut reg = view_registry();
        assert_eq!(reg.len(), 7, "6 leaf skins + 1 stacking parent");
        assert!(reg.verify_stack_order(), "children strictly before parents");
        assert!(reg.verify_bijective(), "resolve∘intern round trip");
        assert!(
            bases_bucket_roundtrip(),
            "rolling-bucket path must equal the direct mint on today's bases"
        );
    }

    /// The work-package detail page's `project` cell is a live
    /// `RecordRef` link (derived from `nav::route_for("Project")`
    /// being `Some`), while `status` — a known dead lane per
    /// `nav::NOT_YET_NAVIGABLE` — renders as plain resolved text with
    /// no href anywhere in the fragment. Both are DERIVED from
    /// `nav::nav_edges`, not hand-wired.
    #[test]
    fn detail_project_link_is_derived_not_hardwired() {
        let wp = work_package(101, "Fix the login bug", 10, 2, 40);

        let html = render_wp_detail(
            &wp,
            "In Progress",
            "Nexgen",
            "Bug",
            "High",
            "Jane Doe",
            "John Smith",
        );

        assert!(html.contains("href=\"/projects/10\""), "{html}");
        assert!(!html.contains("/statuses/"), "{html}");
        // The status label still shows (resolved text), just unlinked.
        assert!(html.contains("In Progress"), "{html}");
    }

    /// A project detail page stacks its work packages' list view
    /// (the `has_many` `work_items` topology edge) below the project's
    /// own fields.
    #[test]
    fn project_detail_stacks_its_work_packages() {
        let wps = vec![
            work_package(201, "Stacked item one", 10, 1, 0),
            work_package(202, "Stacked item two", 10, 2, 50),
        ];

        let html = render_project_work_items_stack(&wps);

        assert!(html.contains("Work items"), "{html}");
        assert!(html.contains("Stacked item one"), "{html}");
        assert!(html.contains("Stacked item two"), "{html}");
        // Stacked rows use the narrower (project-less) column set.
        assert!(!html.contains("href=\"/projects/10\""), "{html}");
    }
}

/// The write-path guards, exercised through the same `edit_form_core` / `update_core` the
/// handlers call, over a fake [`WpStore`] that records every write it is asked to make.
#[cfg(test)]
mod write_path_tests {
    use super::*;
    use crate::write_guard::EDIT_WORK_PACKAGES;
    use chrono::Utc;
    use http_body_util::BodyExt;
    use op_services::work_packages::mutation::Caller;
    use std::collections::HashSet;
    use std::sync::Mutex;

    const SECRET: &[u8] = b"write-path-test-secret-0123456789";
    /// The project the work package lives in.
    const PROJECT: i64 = 10;
    /// Another project the signed-in member belongs to.
    const OTHER_PROJECT: i64 = 20;

    fn wp() -> op_db::work_packages::WorkPackageRow {
        op_db::work_packages::WorkPackageRow {
            id: 101,
            subject: "Old subject".into(),
            description: None,
            project_id: PROJECT,
            type_id: 1,
            status_id: 1,
            priority_id: Some(1),
            author_id: 1,
            assigned_to_id: None,
            responsible_id: None,
            start_date: None,
            due_date: None,
            estimated_hours: None,
            done_ratio: 0,
            parent_id: None,
            version_id: None,
            category_id: None,
            lock_version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    struct Fake {
        wp: Result<Option<op_db::work_packages::WorkPackageRow>, StoreError>,
        active: Result<bool, StoreError>,
        principal_fails: bool,
        /// Permissions of the signed-in user, per project (membership is project-scoped).
        memberships: HashMap<i64, HashSet<String>>,
        admin: bool,
        updates: Mutex<Vec<(i64, UpdateWorkPackageDto)>>,
        update_fails: bool,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                wp: Ok(Some(wp())),
                active: Ok(true),
                principal_fails: false,
                memberships: HashMap::new(),
                admin: false,
                updates: Mutex::new(Vec::new()),
                update_fails: false,
            }
        }
        fn member_of(mut self, project: i64, perms: &[&str]) -> Self {
            self.memberships
                .insert(project, perms.iter().map(|p| (*p).to_string()).collect());
            self
        }
        fn writes(&self) -> usize {
            self.updates.lock().unwrap().len()
        }
    }

    impl WpStore for Fake {
        async fn find_wp(
            &self,
            _id: i64,
        ) -> Result<Option<op_db::work_packages::WorkPackageRow>, StoreError> {
            self.wp.clone()
        }
        async fn project_active(&self, _project_id: i64) -> Result<bool, StoreError> {
            self.active.clone()
        }
        async fn principal(
            &self,
            who: Option<&Who>,
            project_id: i64,
            allow_anonymous: bool,
        ) -> Result<Principal, StoreError> {
            if self.principal_fails {
                return Err(StoreError("boom".into()));
            }
            Ok(match who {
                None => Principal::Anonymous {
                    allowed: allow_anonymous,
                },
                Some(Who::Caller { .. }) => Principal::User {
                    admin: self.admin,
                    active: true,
                    // Scoped to the project the handler asked about.
                    permissions: self
                        .memberships
                        .get(&project_id)
                        .cloned()
                        .unwrap_or_default(),
                },
                Some(_) => Principal::Anonymous { allowed: false },
            })
        }
        async fn edit_options(&self) -> EditOptions {
            EditOptions::default()
        }
        async fn update_unit_of_work(
            &self,
            _caller: &Caller,
            id: i64,
            dto: UpdateWorkPackageDto,
        ) -> Result<op_db::work_packages::WorkPackageRow, MutationError> {
            if self.update_fails {
                return Err(MutationError::Store("db down".into()));
            }
            self.updates.lock().unwrap().push((id, dto));
            Ok(wp())
        }
    }

    fn alice() -> Who {
        Who::Caller {
            actor: "oidc:alice".into(),
            role: "Member".into(),
        }
    }

    fn headers_with_cookie(cookie: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            format!("{}={cookie}", write_guard::CSRF_COOKIE)
                .parse()
                .unwrap(),
        );
        h
    }

    fn ctx<'a>(who: Option<&'a Who>, headers: &'a HeaderMap, anon: bool) -> WriteCtx<'a> {
        WriteCtx {
            who,
            headers,
            allow_anonymous: anon,
            secret: SECRET,
            secure_cookie: false,
        }
    }

    fn good_form(cookie: &str, actor: &str) -> HashMap<String, String> {
        let mut f = HashMap::new();
        f.insert("subject".into(), "New subject".into());
        f.insert(
            CSRF_FIELD.into(),
            write_guard::token_for(SECRET, cookie, actor),
        );
        f
    }

    async fn body(resp: Response) -> String {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn cookie() -> String {
        write_guard::new_cookie_value()
    }

    // ── positive ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn an_authorized_member_with_a_valid_token_updates_and_is_redirected() {
        let store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let resp = update_core(
            &store,
            101,
            &ctx(Some(&who), &headers, false),
            &good_form(&c, "oidc:alice"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(resp.headers()[header::LOCATION], "/work_packages/101");
        let updates = store.updates.lock().unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, 101);
        assert_eq!(updates[0].1.subject.as_deref(), Some("New subject"));
    }

    #[tokio::test]
    async fn an_admin_without_a_membership_may_edit() {
        let mut store = Fake::new();
        store.admin = true;
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let resp = update_core(
            &store,
            101,
            &ctx(Some(&who), &headers, false),
            &good_form(&c, "oidc:alice"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(store.writes(), 1);
    }

    #[tokio::test]
    async fn anonymous_may_not_write_even_with_the_demo_opt_in() {
        let c = cookie();
        let headers = headers_with_cookie(&c);
        let form = good_form(&c, "anonymous");

        let store = Fake::new();
        let denied = update_core(&store, 101, &ctx(None, &headers, false), &form).await;
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);

        // The demo opt-in does not open editing: an anonymous edit could not be journaled
        // (docs/decisions/0001-journaled-edits.md §2).
        let store = Fake::new();
        let demo = update_core(&store, 101, &ctx(None, &headers, true), &form).await;
        assert_eq!(demo.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    // ── authorization ────────────────────────────────────────────────

    #[tokio::test]
    async fn a_member_without_the_edit_permission_gets_403_and_no_write() {
        let store = Fake::new().member_of(PROJECT, &["view_work_packages"]);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let resp = update_core(
            &store,
            101,
            &ctx(Some(&who), &headers, false),
            &good_form(&c, "oidc:alice"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn a_user_with_no_membership_at_all_gets_403_and_no_write() {
        let store = Fake::new();
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let resp = update_core(
            &store,
            101,
            &ctx(Some(&who), &headers, false),
            &good_form(&c, "oidc:alice"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn a_member_of_another_project_may_not_edit_this_projects_work_package() {
        // Full edit rights, but in OTHER_PROJECT; the work package is in PROJECT.
        let store = Fake::new().member_of(OTHER_PROJECT, &[EDIT_WORK_PACKAGES]);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let resp = update_core(
            &store,
            101,
            &ctx(Some(&who), &headers, false),
            &good_form(&c, "oidc:alice"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn an_archived_project_allows_no_edit_even_for_an_admin() {
        let mut store = Fake::new();
        store.admin = true;
        store.active = Ok(false);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let resp = update_core(
            &store,
            101,
            &ctx(Some(&who), &headers, false),
            &good_form(&c, "oidc:alice"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn the_edit_form_is_not_rendered_for_someone_who_cannot_edit() {
        let store = Fake::new().member_of(PROJECT, &["view_work_packages"]);
        let who = alice();
        let headers = HeaderMap::new();
        let resp = edit_form_core(&store, 101, &ctx(Some(&who), &headers, false)).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(resp.headers().get(header::SET_COOKIE).is_none());
        let html = body(resp).await;
        assert!(!html.contains("<form"), "{html}");
        assert!(!html.contains(CSRF_FIELD), "{html}");
    }

    // ── CSRF ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_missing_csrf_token_gets_403_and_no_write() {
        let store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let mut form = good_form(&c, "oidc:alice");
        form.remove(CSRF_FIELD);
        let resp = update_core(&store, 101, &ctx(Some(&who), &headers, false), &form).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn a_missing_csrf_cookie_gets_403_and_no_write() {
        let store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        let (who, c) = (alice(), cookie());
        let headers = HeaderMap::new(); // token is right for `c`, but the browser sent no cookie
        let resp = update_core(
            &store,
            101,
            &ctx(Some(&who), &headers, false),
            &good_form(&c, "oidc:alice"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn a_wrong_csrf_token_gets_403_and_no_write() {
        let store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let mut form = good_form(&c, "oidc:alice");
        form.insert(CSRF_FIELD.into(), "0".repeat(64));
        let resp = update_core(&store, 101, &ctx(Some(&who), &headers, false), &form).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn another_actors_token_does_not_authorize_this_actors_write() {
        let store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let form = good_form(&c, "oidc:mallory"); // minted for someone else
        let resp = update_core(&store, 101, &ctx(Some(&who), &headers, false), &form).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn the_edit_form_embeds_a_token_that_the_update_then_accepts() {
        let store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        let who = alice();
        let no_cookie = HeaderMap::new();
        let resp = edit_form_core(&store, 101, &ctx(Some(&who), &no_cookie, false)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let set_cookie = resp.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_owned();
        assert!(set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Strict"));
        let cookie_value = set_cookie
            .strip_prefix("op_csrf=")
            .and_then(|r| r.split(';').next())
            .unwrap()
            .to_owned();
        let html = body(resp).await;
        let token = write_guard::token_for(SECRET, &cookie_value, "oidc:alice");
        assert!(
            html.contains(&format!("name=\"{CSRF_FIELD}\" value=\"{token}\"")),
            "{html}"
        );

        // Round trip: the browser returns the cookie and posts the embedded token.
        let headers = headers_with_cookie(&cookie_value);
        let mut form = HashMap::new();
        form.insert("subject".to_string(), "Round trip".to_string());
        form.insert(CSRF_FIELD.to_string(), token);
        let resp = update_core(&store, 101, &ctx(Some(&who), &headers, false), &form).await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(store.writes(), 1);

        // A browser that already holds a cookie is not issued another.
        let again = edit_form_core(&store, 101, &ctx(Some(&who), &headers, false)).await;
        assert!(again.headers().get(header::SET_COOKIE).is_none());
    }

    // ── 422 on malformed fields ──────────────────────────────────────

    #[tokio::test]
    async fn a_malformed_date_gets_422_naming_the_field_and_no_write() {
        for (field, bad) in [
            ("start_date", "2026-13-45"),
            ("start_date", "tomorrow"),
            ("due_date", "31/12/2026"),
            ("due_date", "2026-02-30"),
        ] {
            let store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
            let (who, c) = (alice(), cookie());
            let headers = headers_with_cookie(&c);
            let mut form = good_form(&c, "oidc:alice");
            form.insert(field.into(), bad.into());
            let resp = update_core(&store, 101, &ctx(Some(&who), &headers, false), &form).await;
            assert_eq!(
                resp.status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{field}={bad}"
            );
            let html = body(resp).await;
            assert!(html.contains(field), "names the field: {html}");
            assert!(html.contains(bad), "echoes the value: {html}");
            assert_eq!(store.writes(), 0, "{field}={bad}: nothing may be saved");
        }
    }

    #[tokio::test]
    async fn a_blank_date_still_clears_it_and_is_not_an_error() {
        let store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let mut form = good_form(&c, "oidc:alice");
        form.insert("start_date".into(), "".into());
        form.insert("due_date".into(), "2026-12-31".into());
        let resp = update_core(&store, 101, &ctx(Some(&who), &headers, false), &form).await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let updates = store.updates.lock().unwrap();
        assert_eq!(updates[0].1.start_date, None);
        assert_eq!(
            updates[0].1.due_date,
            chrono::NaiveDate::from_ymd_opt(2026, 12, 31)
        );
    }

    #[test]
    fn malformed_numeric_fields_are_errors_too_not_silently_dropped() {
        let existing = wp();
        for (field, bad) in [
            ("estimated_hours", "lots"),
            ("estimated_hours", "NaN"),
            ("done_ratio", "half"),
            ("assigned_to_id", "bob"),
            ("status_id", "x"),
            ("lock_version", "-"),
        ] {
            let mut form = HashMap::new();
            form.insert(field.to_string(), bad.to_string());
            let err = parse_wp_update(&form, &existing).unwrap_err();
            assert_eq!(err.field, field, "{field}={bad}");
        }
    }

    // ── 404 vs 500 ───────────────────────────────────────────────────

    #[tokio::test]
    async fn a_missing_work_package_is_404_on_form_and_update_and_never_writes() {
        let mut store = Fake::new();
        store.wp = Ok(None);
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let cx = ctx(Some(&who), &headers, false);
        assert_eq!(
            edit_form_core(&store, 101, &cx).await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            update_core(&store, 101, &cx, &good_form(&c, "oidc:alice"))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn a_lookup_failure_is_500_not_404_and_never_writes() {
        let mut store = Fake::new();
        store.wp = Err(StoreError("connection reset".into()));
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let cx = ctx(Some(&who), &headers, false);
        let form_resp = edit_form_core(&store, 101, &cx).await;
        assert_eq!(form_resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // The database error text is logged, not shown.
        assert!(!body(form_resp).await.contains("connection reset"));
        assert_eq!(
            update_core(&store, 101, &cx, &good_form(&c, "oidc:alice"))
                .await
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn an_authorization_lookup_failure_is_500_and_never_writes() {
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let cx = ctx(Some(&who), &headers, false);

        let mut store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        store.principal_fails = true;
        assert_eq!(
            update_core(&store, 101, &cx, &good_form(&c, "oidc:alice"))
                .await
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(store.writes(), 0);

        let mut store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        store.active = Err(StoreError("timeout".into()));
        assert_eq!(
            update_core(&store, 101, &cx, &good_form(&c, "oidc:alice"))
                .await
                .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(store.writes(), 0);
    }

    #[tokio::test]
    async fn a_failed_save_is_500_not_a_200_page() {
        let mut store = Fake::new().member_of(PROJECT, &[EDIT_WORK_PACKAGES]);
        store.update_fails = true;
        let (who, c) = (alice(), cookie());
        let headers = headers_with_cookie(&c);
        let resp = update_core(
            &store,
            101,
            &ctx(Some(&who), &headers, false),
            &good_form(&c, "oidc:alice"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// The detail route's lookup (it needs the whole DB for display names, so its 404/500
    /// decision is the shared `lookup` helper, tested directly).
    #[tokio::test]
    async fn lookup_maps_none_to_404_and_errors_to_500_and_found_to_the_record() {
        let found: Result<Option<i32>, StoreError> = Ok(Some(7));
        assert_eq!(lookup(found, "Work package", 1).ok(), Some(7));
        let none: Result<Option<i32>, StoreError> = Ok(None);
        assert_eq!(
            lookup(none, "Work package", 1).unwrap_err().status(),
            StatusCode::NOT_FOUND
        );
        let err: Result<Option<i32>, StoreError> = Err(StoreError("x".into()));
        assert_eq!(
            lookup(err, "Work package", 1).unwrap_err().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn the_rendered_forms_carry_the_token_field() {
        let html = render_wp_edit(&wp(), &[], &[], &[], &[], "TOKEN-WP");
        assert!(
            html.contains("name=\"authenticity_token\" value=\"TOKEN-WP\""),
            "{html}"
        );
    }

    // ── one rule for pages and API ───────────────────────────────────

    /// What a request was told, reduced to the decision.
    #[derive(Debug, PartialEq, Eq)]
    enum Decision {
        Allowed,
        Refused,
        NotFound,
    }

    fn api_patch() -> op_api::handlers::work_packages::UpdateWorkPackageDto {
        op_api::handlers::work_packages::UpdateWorkPackageDto {
            subject: Some("New subject".into()),
            description: None,
            type_id: None,
            status_id: None,
            priority_id: None,
            assigned_to_id: None,
            estimated_hours: None,
            done_ratio: None,
            lock_version: 1,
        }
    }

    /// The page and the JSON API answer the same decision for the same principal and record: the
    /// page's `update_core` and the API's `update_work_package_core` run over the SAME store
    /// (the page store seen through the boundary), with the same sign-in outcome.
    #[tokio::test]
    async fn the_page_and_the_api_decide_the_same_for_the_same_principal_and_record() {
        let signed_in = alice();
        let refused = Who::Refused {
            actor: "oidc:eve".into(),
        };
        let whos: [(&str, Option<&Who>, bool); 5] = [
            ("anonymous", None, false),
            ("anonymous, demo opt-in", None, true),
            ("signed in", Some(&signed_in), false),
            ("signed in, demo opt-in", Some(&signed_in), true),
            ("refused", Some(&refused), false),
        ];
        let rights: [(&str, bool, &[&str]); 4] = [
            ("editor", false, &[EDIT_WORK_PACKAGES]),
            ("viewer", false, &["view_work_packages"]),
            ("no membership", false, &[]),
            ("admin", true, &[]),
        ];
        let (mut allowed, mut refused_n, mut not_found) = (0, 0, 0);
        for (who_name, who, anon) in whos {
            for (rights_name, admin, perms) in rights {
                for (project_active, record_exists) in [(true, true), (false, true), (true, false)]
                {
                    let label = format!("{who_name} / {rights_name} / active={project_active} / exists={record_exists}");
                    let mut store = Fake::new().member_of(PROJECT, perms);
                    store.admin = admin;
                    store.active = Ok(project_active);
                    if !record_exists {
                        store.wp = Ok(None);
                    }

                    // The page.
                    let c = cookie();
                    let headers = headers_with_cookie(&c);
                    let actor = actor_key(who);
                    let resp = update_core(
                        &store,
                        101,
                        &ctx(who, &headers, anon),
                        &good_form(&c, &actor),
                    )
                    .await;
                    let page = match resp.status() {
                        StatusCode::SEE_OTHER => Decision::Allowed,
                        StatusCode::FORBIDDEN => Decision::Refused,
                        StatusCode::NOT_FOUND => Decision::NotFound,
                        other => panic!("{label}: page answered {other}"),
                    };

                    // The API, over the same store and the same sign-in outcome.
                    let caller = write_guard::caller_of(who, anon);
                    let api = match op_api::handlers::work_packages::update_work_package_core(
                        &PageStore(&store),
                        &caller,
                        101,
                        api_patch(),
                    )
                    .await
                    {
                        Ok(_) => Decision::Allowed,
                        Err(e) => match e.status_code().as_u16() {
                            401 | 403 => Decision::Refused,
                            404 => Decision::NotFound,
                            other => panic!("{label}: api answered {other}"),
                        },
                    };
                    assert_eq!(page, api, "{label}");
                    match page {
                        Decision::Allowed => allowed += 1,
                        Decision::Refused => refused_n += 1,
                        Decision::NotFound => not_found += 1,
                    }
                }
            }
        }
        // The matrix is not vacuous: every outcome occurs.
        assert!(
            allowed > 0 && refused_n > 0 && not_found > 0,
            "{allowed}/{refused_n}/{not_found}"
        );
    }
}
