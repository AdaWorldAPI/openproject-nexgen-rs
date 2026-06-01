// SPRINT C0 · openproject-nexgen-rs · agent-emitted, NOT pipeline-generated.
// Follows target-spec/rust-axum-sqlx.toml + the op-api handler seed idiom
// (repository-backed, ajax_json). Pipeline cannot yet emit this (axum-seaorm
// engine; no Ruby frontend — see extraction-gap-proposals.md). Source of truth:
// target-spec, not this file. Rails source: lib/api/v3/news/news_api.rb

//! News API handlers
//!
//! Mirrors: lib/api/v3/news/* (the `ajax_json` emit kind per target-spec).
//!
//! Repository-backed (op-db), HAL+JSON responses — built with the same inline
//! `Response`/`from_row` idiom the seed handlers use (`projects.rs`,
//! `work_packages.rs`). The News listing in op-db is project-scoped
//! (`NewsRepository::list_by_project`), matching the Rails v3 `/projects/:id/news`
//! nesting, so `list_news` takes the project id via `Path` like the sibling
//! project-scoped collections (`versions::list_project_versions`).

use axum::{
    extract::{Path, State},
    response::IntoResponse,
};
use op_core::traits::Id;
use serde::Serialize;

use op_db::news::NewsRepository;
use op_models::news::News;

use crate::error::{ApiError, ApiResult};
use crate::extractors::{AppState, AuthenticatedUser, HalResponse, Pagination};

/// GET /api/v3/projects/:id/news
///
/// Lists the news entries for a project, most-recent-first, paginated.
pub async fn list_news(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(project_id): Path<Id>,
    pagination: Pagination,
) -> ApiResult<impl IntoResponse> {
    let pool = state.pool()?;
    let repo = NewsRepository::new(pool.clone());

    let result = repo
        .list_by_project(
            project_id,
            &op_db::Pagination {
                limit: pagination.page_size as i64,
                offset: pagination.offset as i64,
            },
        )
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let total = result.total;
    let elements: Vec<NewsResponse> = result
        .items
        .into_iter()
        .map(NewsResponse::from_row)
        .collect();

    let collection = NewsCollection {
        type_name: "Collection".into(),
        total: total as usize,
        count: elements.len(),
        page_size: pagination.page_size,
        offset: pagination.offset,
        elements,
    };
    Ok(HalResponse(collection))
}

/// GET /api/v3/news/:id
pub async fn get_news(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(id): Path<Id>,
) -> ApiResult<impl IntoResponse> {
    let pool = state.pool()?;
    let repo = NewsRepository::new(pool.clone());

    // `find_by_id` surfaces a missing row as `RepositoryError::NotFound`
    // (it returns `News`, not `Option<News>`), so map that variant explicitly —
    // mirroring the error-mapping idiom in `work_packages.rs`.
    let row = repo
        .find_by_id(id)
        .await
        .map_err(|e| match e {
            op_db::RepositoryError::NotFound(_) => ApiError::not_found("News", id),
            _ => ApiError::internal(format!("Database error: {}", e)),
        })?;

    Ok(HalResponse(NewsResponse::from_row(row)))
}

// DTOs
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewsCollection {
    #[serde(rename = "_type")]
    type_name: String,
    total: usize,
    count: usize,
    page_size: usize,
    offset: usize,
    #[serde(rename = "_embedded")]
    elements: Vec<NewsResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewsResponse {
    #[serde(rename = "_type")]
    type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Id>,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<Id>,
    author_id: Id,
    comments_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<String>,
    #[serde(rename = "_links")]
    links: NewsLinks,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NewsLinks {
    #[serde(rename = "self")]
    self_link: Link,
    author: Link,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<Link>,
}

#[derive(Debug, Serialize)]
struct Link {
    href: String,
}

impl NewsResponse {
    fn from_row(row: News) -> Self {
        let project_link = row.project_id.map(|pid| Link {
            href: format!("/api/v3/projects/{}", pid),
        });

        NewsResponse {
            type_name: "News".into(),
            id: row.id,
            title: row.title,
            summary: row.summary,
            description: row.description,
            project_id: row.project_id,
            author_id: row.author_id,
            comments_count: row.comments_count,
            created_at: row.created_at.map(|d| d.to_rfc3339()),
            updated_at: row.updated_at.map(|d| d.to_rfc3339()),
            links: NewsLinks {
                self_link: Link {
                    href: format!("/api/v3/news/{}", row.id.unwrap_or(0)),
                },
                author: Link {
                    href: format!("/api/v3/users/{}", row.author_id),
                },
                project: project_link,
            },
        }
    }
}

// EXTRACTOR-GAP: create/update/delete News handlers are intentionally omitted.
// Source: crates/op-db/src/news.rs:32-90 | Resolve: the sibling NewsRepository
// exposes ONLY `find_by_id` + `list_by_project` (no `create`/`update`/`delete`),
// so a faithful, repository-backed mutation handler cannot be emitted without a
// fabricated service/repo method. The target-spec emit kind for News is
// `ajax_json` read access (list + show); mutations are out of scope for this
// agent and belong to a follow-up once op-db grows the write surface. Per the
// sprint HARD RULES, no `todo!()` and no raw sqlx — list + show are emitted
// faithfully and the rest is gapped here rather than stubbed inertly.
