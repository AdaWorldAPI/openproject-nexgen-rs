//! Work package service
//!
//! Mirrors: `app/services/work_packages/create_service.rb` (+ the set-attributes
//! write path). A thin `CreateService` over a pluggable [`WorkPackageStore`]:
//! deserialize a [`NewWorkPackage`] write-model, validate, build the domain
//! [`WorkPackage`], persist, return it with its assigned id.
//!
//! Structure follows the `op-journals` house pattern: an error enum +
//! `Result` alias, an async `Store` trait, the service, and an in-memory
//! store for tests.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use op_core::traits::Id;
use op_core::types::Formattable;
use serde::Deserialize;
use thiserror::Error;

use crate::work_package::WorkPackage;

/// Work package service errors.
#[derive(Debug, Error)]
pub enum WorkPackageError {
    /// No work package with the given id.
    #[error("work package not found: {0}")]
    NotFound(Id),
    /// The write-model failed validation (e.g. blank subject).
    #[error("validation failed: {0}")]
    Validation(String),
    /// The underlying store failed.
    #[error("database error: {0}")]
    Database(String),
}

/// Result alias for the service surface.
pub type WorkPackageResult<T> = Result<T, WorkPackageError>;

/// Create write-model — the input DTO for `POST /work_packages`.
///
/// Only the NOT NULL columns are required (`subject`, `project_id`,
/// `type_id`, `status_id`, `author_id`); everything else is optional,
/// matching the persisted shape (`op-db` carries the rest as `Option`).
/// Deserializable so an API layer can parse a request body straight into it.
#[derive(Debug, Clone, Deserialize)]
pub struct NewWorkPackage {
    /// One-line headline. Required (non-blank — see [`WorkPackageService::create`]).
    pub subject: String,
    /// Owning project FK.
    pub project_id: Id,
    /// Work-package type FK.
    pub type_id: Id,
    /// Status FK.
    pub status_id: Id,
    /// Author FK.
    pub author_id: Id,
    /// Priority FK (optional — nullable column).
    #[serde(default)]
    pub priority_id: Option<Id>,
    /// Assignee FK (optional).
    #[serde(default)]
    pub assigned_to_id: Option<Id>,
    /// Parent WP FK for the hierarchy (optional).
    #[serde(default)]
    pub parent_id: Option<Id>,
    /// Description body as markdown source (optional).
    #[serde(default)]
    pub description: Option<String>,
}

impl NewWorkPackage {
    /// The minimal required create input; optionals default to `None`.
    pub fn new(
        subject: impl Into<String>,
        project_id: Id,
        type_id: Id,
        status_id: Id,
        author_id: Id,
    ) -> Self {
        Self {
            subject: subject.into(),
            project_id,
            type_id,
            status_id,
            author_id,
            priority_id: None,
            assigned_to_id: None,
            parent_id: None,
            description: None,
        }
    }

    /// Map the write-model onto a fresh (unpersisted) domain entity.
    pub fn to_work_package(&self) -> WorkPackage {
        let mut wp = WorkPackage::new(
            self.subject.clone(),
            self.project_id,
            self.type_id,
            self.status_id,
            self.author_id,
        );
        wp.priority_id = self.priority_id;
        wp.assigned_to_id = self.assigned_to_id;
        wp.parent_id = self.parent_id;
        if let Some(body) = &self.description {
            wp.description = Formattable::markdown(body.clone());
        }
        wp
    }
}

/// Persistence port for work packages (mirrors OpenProject's repository).
#[async_trait]
pub trait WorkPackageStore: Send + Sync {
    /// Insert a new work package; returns the assigned id.
    async fn insert(&self, wp: &WorkPackage) -> WorkPackageResult<Id>;
    /// Fetch by id.
    async fn get(&self, id: Id) -> WorkPackageResult<Option<WorkPackage>>;
    /// All work packages in a project.
    async fn list_for_project(&self, project_id: Id) -> WorkPackageResult<Vec<WorkPackage>>;
}

/// Create/read service over a [`WorkPackageStore`].
pub struct WorkPackageService {
    store: Arc<dyn WorkPackageStore>,
}

impl WorkPackageService {
    /// Build the service over a store.
    pub fn new(store: Arc<dyn WorkPackageStore>) -> Self {
        Self { store }
    }

    /// Create a work package from the write-model: validate, build, persist.
    /// Returns the persisted entity with its assigned id.
    ///
    /// # Errors
    ///
    /// [`WorkPackageError::Validation`] if `subject` is blank;
    /// [`WorkPackageError::Database`] if the store fails.
    pub async fn create(&self, input: NewWorkPackage) -> WorkPackageResult<WorkPackage> {
        if input.subject.trim().is_empty() {
            return Err(WorkPackageError::Validation(
                "subject can't be blank".to_string(),
            ));
        }
        let wp = input.to_work_package();
        let id = self.store.insert(&wp).await?;
        let mut created = wp;
        created.id = Some(id);
        Ok(created)
    }

    /// Fetch a work package by id, erroring [`WorkPackageError::NotFound`] if
    /// absent.
    ///
    /// # Errors
    ///
    /// [`WorkPackageError::NotFound`] / [`WorkPackageError::Database`].
    pub async fn find(&self, id: Id) -> WorkPackageResult<WorkPackage> {
        self.store
            .get(id)
            .await?
            .ok_or(WorkPackageError::NotFound(id))
    }

    /// All work packages in a project.
    ///
    /// # Errors
    ///
    /// [`WorkPackageError::Database`] on store failure.
    pub async fn list_for_project(&self, project_id: Id) -> WorkPackageResult<Vec<WorkPackage>> {
        self.store.list_for_project(project_id).await
    }
}

/// In-memory [`WorkPackageStore`] for tests and prototyping.
pub struct MemoryWorkPackageStore {
    rows: RwLock<Vec<WorkPackage>>,
    next_id: AtomicI64,
}

impl Default for MemoryWorkPackageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryWorkPackageStore {
    /// Empty store, ids starting at 1.
    pub fn new() -> Self {
        Self {
            rows: RwLock::new(Vec::new()),
            next_id: AtomicI64::new(1),
        }
    }
}

#[async_trait]
impl WorkPackageStore for MemoryWorkPackageStore {
    async fn insert(&self, wp: &WorkPackage) -> WorkPackageResult<Id> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut stored = wp.clone();
        stored.id = Some(id);
        self.rows
            .write()
            .map_err(|e| WorkPackageError::Database(e.to_string()))?
            .push(stored);
        Ok(id)
    }

    async fn get(&self, id: Id) -> WorkPackageResult<Option<WorkPackage>> {
        let rows = self
            .rows
            .read()
            .map_err(|e| WorkPackageError::Database(e.to_string()))?;
        Ok(rows.iter().find(|wp| wp.id == Some(id)).cloned())
    }

    async fn list_for_project(&self, project_id: Id) -> WorkPackageResult<Vec<WorkPackage>> {
        let rows = self
            .rows
            .read()
            .map_err(|e| WorkPackageError::Database(e.to_string()))?;
        Ok(rows
            .iter()
            .filter(|wp| wp.project_id == project_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> WorkPackageService {
        WorkPackageService::new(Arc::new(MemoryWorkPackageStore::new()))
    }

    // subject, project, type, status, author
    fn input() -> NewWorkPackage {
        NewWorkPackage::new("Fix the thing", 1, 2, 3, 10)
    }

    #[tokio::test]
    async fn create_persists_and_assigns_id() {
        let svc = service();
        let created = svc.create(input()).await.unwrap();
        assert!(created.id.is_some(), "persisted row gets an id");
        assert_eq!(created.subject, "Fix the thing");
        // Round-trips through the store.
        let found = svc.find(created.id.unwrap()).await.unwrap();
        assert_eq!(found.subject, "Fix the thing");
        assert_eq!(found.project_id, 1);
    }

    #[tokio::test]
    async fn create_rejects_blank_subject() {
        let svc = service();
        let mut bad = input();
        bad.subject = "   ".to_string();
        let err = svc.create(bad).await.unwrap_err();
        assert!(
            matches!(err, WorkPackageError::Validation(_)),
            "blank subject → Validation, got {err:?}"
        );
    }

    #[tokio::test]
    async fn create_maps_optional_write_model_fields() {
        let svc = service();
        let mut inp = input();
        inp.priority_id = Some(5);
        inp.assigned_to_id = Some(20);
        inp.parent_id = Some(7);
        inp.description = Some("**body**".to_string());
        let wp = svc.create(inp).await.unwrap();
        assert_eq!(wp.priority_id, Some(5));
        assert_eq!(wp.assigned_to_id, Some(20));
        assert_eq!(wp.parent_id, Some(7));
        assert_eq!(wp.description.raw, "**body**");
    }

    #[tokio::test]
    async fn find_unknown_id_is_not_found() {
        let svc = service();
        let err = svc.find(999).await.unwrap_err();
        assert!(matches!(err, WorkPackageError::NotFound(999)), "{err:?}");
    }

    #[tokio::test]
    async fn list_for_project_filters_by_project() {
        let svc = service();
        // Two in project 1, one in project 2.
        svc.create(NewWorkPackage::new("a", 1, 2, 3, 10))
            .await
            .unwrap();
        svc.create(NewWorkPackage::new("b", 1, 2, 3, 10))
            .await
            .unwrap();
        svc.create(NewWorkPackage::new("c", 2, 2, 3, 10))
            .await
            .unwrap();
        assert_eq!(svc.list_for_project(1).await.unwrap().len(), 2);
        assert_eq!(svc.list_for_project(2).await.unwrap().len(), 1);
        assert_eq!(svc.list_for_project(99).await.unwrap().len(), 0);
    }

    #[test]
    fn write_model_deserializes_with_optional_fields_defaulting() {
        // An API body with only the required fields parses; optionals default.
        let json = r#"{"subject":"S","project_id":1,"type_id":2,"status_id":3,"author_id":10}"#;
        let nwp: NewWorkPackage = serde_json::from_str(json).unwrap();
        assert_eq!(nwp.subject, "S");
        assert!(nwp.priority_id.is_none());
        assert!(nwp.description.is_none());
        // And it maps to a fresh, unpersisted entity.
        let wp = nwp.to_work_package();
        assert!(wp.id.is_none());
        assert_eq!(wp.status_id, 3);
    }
}
