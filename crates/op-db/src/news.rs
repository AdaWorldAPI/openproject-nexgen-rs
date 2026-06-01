// SPRINT C0 · openproject-nexgen-rs · agent-emitted, NOT pipeline-generated.
// Follows target-spec/rust-axum-sqlx.toml + the op-db seed repository idiom.
// Pipeline cannot yet emit this (axum-seaorm engine; no Ruby frontend — see
// extraction-gap-proposals.md). Source of truth: target-spec, not this file.
// Rails source: app/models/news.rb (+ the news controller queries)

//! News repository
//!
//! Database operations for news entries (project-scoped announcements).
//!
//! Mirrors the seed repository idiom (see `work_packages.rs` / `projects.rs`):
//! owned `PgPool`, `new(pool)` constructor, runtime `sqlx::query_as` (no
//! compile-time macros so `cargo check` stays DB-free), `RepositoryError`
//! mapping with the `NotFound` idiom, and `PaginatedResult::new` for listings.

use op_core::traits::Id;
use sqlx::PgPool;

use op_models::news::News;

use crate::repository::{Pagination, PaginatedResult, RepositoryError, RepositoryResult};

/// News repository implementation.
///
/// Holds an owned [`PgPool`] exactly like the sibling repositories
/// (`WorkPackageRepository`, `ProjectRepository`); construct it with a cloned
/// pool: `NewsRepository::new(db.pool().clone())`.
pub struct NewsRepository {
    pool: PgPool,
}

impl NewsRepository {
    /// Create a new repository over the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find a single news entry by its ID.
    ///
    /// Returns [`RepositoryError::NotFound`] when no row matches — mirroring the
    /// `NotFound` idiom used by the sibling repositories' mutating queries.
    pub async fn find_by_id(&self, id: Id) -> Result<News, RepositoryError> {
        let news = sqlx::query_as::<_, News>(
            r#"
            SELECT id, project_id, title, summary, description,
                   author_id, comments_count, created_at, updated_at
            FROM news
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::NotFound(format!("News with id {} not found", id)))?;

        Ok(news)
    }

    /// List news entries for a project (tenant column: `project_id`), paginated
    /// and ordered most-recent-first — matching the news controller queries.
    pub async fn list_by_project(
        &self,
        project_id: Id,
        pagination: &Pagination,
    ) -> Result<PaginatedResult<News>, RepositoryError> {
        let items = sqlx::query_as::<_, News>(
            r#"
            SELECT id, project_id, title, summary, description,
                   author_id, comments_count, created_at, updated_at
            FROM news
            WHERE project_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(project_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(&self.pool)
        .await?;

        let total =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM news WHERE project_id = $1")
                .bind(project_id)
                .fetch_one(&self.pool)
                .await?;

        Ok(PaginatedResult::new(items, total, *pagination))
    }
}

// Note: `RepositoryResult<T>` (= `Result<T, RepositoryError>`) is imported to
// keep parity with the sibling repos' public error alias; the two public method
// signatures above are spelled out as `Result<_, RepositoryError>` per the
// W1-R interface contract. Reference the alias so the import is not dead.
#[allow(dead_code)]
type _NewsRepositoryResult<T> = RepositoryResult<T>;
