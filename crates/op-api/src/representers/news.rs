// SPRINT C0 · openproject-nexgen-rs · agent-emitted, NOT pipeline-generated.
// Follows target-spec/rust-axum-sqlx.toml + the op-api representer seed idiom.
// Pipeline cannot yet emit this (axum-seaorm engine; no Ruby frontend — see
// extraction-gap-proposals.md). Source of truth: target-spec, not this file.
// Rails source: lib/api/v3/news/news_representer.rb

//! News HAL Representer
//!
//! Converts news models to HAL+JSON format compatible with OpenProject API v3.
//! Mirrors `API::V3::News::NewsRepresenter` (`_type` "News"; properties id,
//! title, summary, description{format,raw,html}, createdAt, updatedAt; links
//! self/project/author + permission-gated updateImmediately/delete).

use chrono::{DateTime, Utc};
use op_core::traits::Id;
use serde::Serialize;

use super::hal::{HalCollection, HalLink, HalLinks, HalResource, rels};

/// News representation for API responses
#[derive(Debug, Clone, Serialize)]
pub struct NewsRepresentation {
    pub id: Id,
    pub title: String,
    pub summary: String,
    pub description: FormattableText,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

/// Formattable text (mirrors the Rails `formattable_property` shape)
#[derive(Debug, Clone, Serialize)]
pub struct FormattableText {
    pub format: String,
    pub raw: String,
    pub html: String,
}

/// News representer
pub struct NewsRepresenter;

impl NewsRepresenter {
    /// Create a HAL resource for a single news entry.
    ///
    /// `can_manage` reflects `manage_news` permission in the news' project and
    /// gates the `updateImmediately`/`delete` links, matching the Rails
    /// representer's `cache_if: current_user.allowed_in_project?(:manage_news, ...)`.
    pub fn represent(news: NewsData, can_manage: bool) -> HalResource<NewsRepresentation> {
        let description = news.description.clone().unwrap_or_default();
        let rep = NewsRepresentation {
            id: news.id,
            title: news.title.clone(),
            summary: news.summary.clone(),
            description: FormattableText {
                format: "markdown".to_string(),
                raw: description.clone(),
                html: format!("<p class=\"op-uc-p\">{}</p>", html_escape(&description)),
            },
            created_at: news.created_at,
            updated_at: news.updated_at,
        };

        let links = Self::build_links(&news, can_manage);

        HalResource::new("News", rep).with_links(links)
    }

    /// Create a HAL collection of news entries.
    pub fn represent_collection(
        news: Vec<NewsData>,
        total: i64,
        offset: i64,
        page_size: i64,
        base_url: &str,
        can_manage: bool,
    ) -> HalCollection<HalResource<NewsRepresentation>> {
        let page = (offset / page_size) + 1;
        let elements: Vec<HalResource<NewsRepresentation>> = news
            .into_iter()
            .map(|n| Self::represent(n, can_manage))
            .collect();

        HalCollection::new("NewsCollection", elements, total, page_size, offset)
            .with_pagination_links(base_url, page, page_size)
    }

    /// Build links for a news entry.
    fn build_links(news: &NewsData, can_manage: bool) -> HalLinks {
        let base = format!("/api/v3/news/{}", news.id);

        // self carries the news title (Rails: self_link title_getter -> represented.title)
        let mut links = HalLinks::new()
            .with(rels::SELF, HalLink::with_title(&base, news.title.as_str()))
            .with(
                "project",
                HalLink::with_title(
                    format!("/api/v3/projects/{}", news.project_id),
                    news.project_name.as_deref().unwrap_or(""),
                ),
            )
            .with(
                "author",
                HalLink::with_title(
                    format!("/api/v3/users/{}", news.author_id),
                    news.author_name.as_deref().unwrap_or(""),
                ),
            );

        // Permission-gated mutations (manage_news in the news' project).
        if can_manage {
            links.add(
                rels::UPDATE_IMMEDIATELY,
                HalLink::new(&base).method("PATCH"),
            );
            links.add(rels::DELETE, HalLink::new(&base).method("DELETE"));
        }

        links
    }
}

/// News data for representation.
///
/// Mirrors `op_models::news::News` (id, project_id, title, summary, description,
/// author_id, created_at, updated_at, comments_count) and additionally carries
/// the denormalized `project_name`/`author_name` needed for the titled
/// `project`/`author` HAL links (same precedent as `ProjectData::parent_name`).
#[derive(Debug, Clone)]
pub struct NewsData {
    pub id: Id,
    pub project_id: Id,
    pub project_name: Option<String>,
    pub title: String,
    pub summary: String,
    pub description: Option<String>,
    pub author_id: Id,
    pub author_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub comments_count: i64,
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_news() -> NewsData {
        NewsData {
            id: 1,
            project_id: 7,
            project_name: Some("Test Project".to_string()),
            title: "Test News".to_string(),
            summary: "A short summary".to_string(),
            description: Some("A test news body".to_string()),
            author_id: 3,
            author_name: Some("John Doe".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            comments_count: 0,
        }
    }

    #[test]
    fn test_news_representation() {
        let news = create_test_news();
        let hal = NewsRepresenter::represent(news, false);

        let json = serde_json::to_value(&hal).unwrap();
        assert_eq!(json["_type"], "News");
        assert_eq!(json["id"], 1);
        assert_eq!(json["title"], "Test News");
        assert_eq!(json["summary"], "A short summary");
        assert_eq!(json["description"]["format"], "markdown");
        assert_eq!(json["description"]["raw"], "A test news body");
        assert_eq!(
            json["description"]["html"],
            "<p class=\"op-uc-p\">A test news body</p>"
        );
    }

    #[test]
    fn test_news_links() {
        let news = create_test_news();
        let hal = NewsRepresenter::represent(news, false);

        let json = serde_json::to_value(&hal).unwrap();
        assert_eq!(json["_links"]["self"]["href"], "/api/v3/news/1");
        assert_eq!(json["_links"]["self"]["title"], "Test News");
        assert_eq!(json["_links"]["project"]["href"], "/api/v3/projects/7");
        assert_eq!(json["_links"]["project"]["title"], "Test Project");
        assert_eq!(json["_links"]["author"]["href"], "/api/v3/users/3");
        assert_eq!(json["_links"]["author"]["title"], "John Doe");
    }

    #[test]
    fn test_news_management_links_gated() {
        let news = create_test_news();

        let unmanaged = serde_json::to_value(NewsRepresenter::represent(news.clone(), false)).unwrap();
        assert!(unmanaged["_links"]["updateImmediately"].is_null());
        assert!(unmanaged["_links"]["delete"].is_null());

        let managed = serde_json::to_value(NewsRepresenter::represent(news, true)).unwrap();
        assert_eq!(managed["_links"]["updateImmediately"]["href"], "/api/v3/news/1");
        assert_eq!(managed["_links"]["updateImmediately"]["method"], "PATCH");
        assert_eq!(managed["_links"]["delete"]["method"], "DELETE");
    }

    #[test]
    fn test_news_collection() {
        let news = vec![create_test_news()];
        let collection =
            NewsRepresenter::represent_collection(news, 1, 0, 20, "/api/v3/news", false);

        let json = serde_json::to_value(&collection).unwrap();
        assert_eq!(json["_type"], "NewsCollection");
        assert_eq!(json["count"], 1);
        assert_eq!(json["total"], 1);
        assert_eq!(json["_embedded"]["elements"][0]["_type"], "News");
    }
}
