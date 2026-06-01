// SPRINT C0 · openproject-nexgen-rs · agent-emitted, NOT pipeline-generated.
// Follows target-spec/rust-axum-sqlx.toml + the op-models seed idiom. The ruff
// codegen pipeline cannot yet emit this (engine target is axum-seaorm; no
// Ruby/Rails frontend — see extraction-gap-proposals.md). Source of truth is
// target-spec, not this file. Rails source: app/models/news.rb

//! News model
//!
//! Mirrors: app/models/news.rb
//! Table: news
//!
//! Schema source of truth: db/migrate/tables/news.rb
//! ```ruby
//! create_table do |t|
//!   t.bigint   :project_id
//!   t.string   :title,          default: "", null: false
//!   t.string   :summary,        default: ""
//!   t.text     :description
//!   t.bigint   :author_id,      null: false
//!   t.datetime :created_at,     precision: nil
//!   t.integer  :comments_count, default: 0,  null: false
//!   t.datetime :updated_at,     precision: nil
//! end
//! ```

use chrono::{DateTime, Utc};
use op_core::traits::{Entity, Id, Identifiable, ProjectScoped, Timestamped, HalRepresentable};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// News entity
///
/// A project news entry (headline + summary + body) authored by a user.
///
/// # Ruby equivalent
/// ```ruby
/// class News < ApplicationRecord
///   belongs_to :project
///   belongs_to :author, class_name: "User"
///   has_many :comments, as: :commented, dependent: :delete_all
///   validates :project, presence: true
///   validates :title, presence: true, length: { maximum: 256 }
///   validates :summary, length: { maximum: 255 }
///   acts_as_journalized
///   acts_as_watchable
/// end
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, Validate)]
#[serde(rename_all = "camelCase")]
pub struct News {
    pub id: Option<Id>,

    /// Owning project. Nullable in the schema (`t.bigint :project_id` carries no
    /// `null: false`); the Rails model adds `validates :project, presence: true`
    /// at the application layer. Modeled as `Option<Id>` to stay faithful to the
    /// column, mirroring `Member::project_id`.
    pub project_id: Option<Id>,

    /// The headline of the news (NOT NULL, defaults to "").
    #[validate(length(max = 256))]
    pub title: String,

    /// A short summary. Nullable column (defaults to ""), capped at 255 chars.
    #[validate(length(max = 255))]
    pub summary: Option<String>,

    /// The main body of the news with all the details (`text`, nullable).
    pub description: Option<String>,

    /// The user having created the news (`author_id`, NOT NULL).
    pub author_id: Id,

    /// Cached count of attached comments (`comments_count`, NOT NULL, default 0).
    #[serde(default)]
    pub comments_count: i32,

    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Default for News {
    fn default() -> Self {
        Self {
            id: None,
            project_id: None,
            title: String::new(),
            summary: None,
            description: None,
            author_id: 0,
            comments_count: 0,
            created_at: None,
            updated_at: None,
        }
    }
}

impl Identifiable for News {
    fn id(&self) -> Option<Id> {
        self.id
    }
}

impl Timestamped for News {
    fn created_at(&self) -> Option<DateTime<Utc>> {
        self.created_at
    }

    fn updated_at(&self) -> Option<DateTime<Utc>> {
        self.updated_at
    }
}

impl ProjectScoped for News {
    fn project_id(&self) -> Option<Id> {
        self.project_id
    }
}

impl Entity for News {
    const TABLE_NAME: &'static str = "news";
    const TYPE_NAME: &'static str = "News";
}

impl HalRepresentable for News {
    fn hal_type(&self) -> &'static str {
        "News"
    }

    fn self_href(&self) -> String {
        format!("/api/v3/news/{}", self.id.unwrap_or(0))
    }

    fn hal_links(&self) -> serde_json::Value {
        let mut links = serde_json::json!({
            "self": { "href": self.self_href() },
            "author": { "href": format!("/api/v3/users/{}", self.author_id) }
        });

        if let Some(project_id) = self.project_id {
            links["project"] =
                serde_json::json!({ "href": format!("/api/v3/projects/{}", project_id) });
        }

        links
    }
}

impl News {
    /// Create a new news entry with a headline, owning project and author.
    pub fn new(title: impl Into<String>, project_id: Id, author_id: Id) -> Self {
        Self {
            title: title.into(),
            project_id: Some(project_id),
            author_id,
            ..Default::default()
        }
    }
}

/// DTO for creating a news entry.
///
/// Mirrors `lib/api/v3/news/news_payload_representer.rb` / the writable subset
/// of the Rails model: `title`, `summary`, `description` (author + project are
/// supplied by the surrounding service/context).
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateNewsDto {
    pub project_id: Id,

    #[validate(length(min = 1, max = 256))]
    pub title: String,

    #[validate(length(max = 255))]
    pub summary: Option<String>,

    pub description: Option<String>,
}

impl From<CreateNewsDto> for News {
    fn from(dto: CreateNewsDto) -> Self {
        Self {
            project_id: Some(dto.project_id),
            title: dto.title,
            summary: dto.summary,
            description: dto.description,
            ..Default::default()
        }
    }
}

/// DTO for updating a news entry (all fields optional / partial update).
#[derive(Debug, Clone, Deserialize, Default, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNewsDto {
    #[validate(length(min = 1, max = 256))]
    pub title: Option<String>,

    #[validate(length(max = 255))]
    pub summary: Option<String>,

    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_news_new() {
        let news = News::new("Release 1.0 is out", 7, 2);
        assert_eq!(news.title, "Release 1.0 is out");
        assert_eq!(news.project_id, Some(7));
        assert_eq!(news.author_id, 2);
        assert_eq!(news.comments_count, 0);
        assert!(news.id.is_none());
    }

    #[test]
    fn test_entity_table_name() {
        assert_eq!(News::TABLE_NAME, "news");
        assert_eq!(News::TYPE_NAME, "News");
    }

    #[test]
    fn test_project_scoped() {
        let news = News::new("Hello", 42, 1);
        assert_eq!(ProjectScoped::project_id(&news), Some(42));
    }

    #[test]
    fn test_hal_links_include_project_and_author() {
        let mut news = News::new("Hello", 42, 9);
        news.id = Some(3);
        let links = news.hal_links();
        assert_eq!(links["self"]["href"], "/api/v3/news/3");
        assert_eq!(links["author"]["href"], "/api/v3/users/9");
        assert_eq!(links["project"]["href"], "/api/v3/projects/42");
    }

    #[test]
    fn test_create_dto_into_news() {
        let dto = CreateNewsDto {
            project_id: 5,
            title: "Headline".to_string(),
            summary: Some("Short".to_string()),
            description: None,
        };
        let news: News = dto.into();
        assert_eq!(news.title, "Headline");
        assert_eq!(news.project_id, Some(5));
        assert_eq!(news.summary.as_deref(), Some("Short"));
    }

    #[test]
    fn test_title_length_validation() {
        let mut news = News::new("ok", 1, 1);
        assert!(news.validate().is_ok());

        news.title = "x".repeat(257);
        assert!(news.validate().is_err());
    }
}
