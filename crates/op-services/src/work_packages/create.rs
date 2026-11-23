//! Create Service for Work Packages
//!
//! Mirrors: app/services/work_packages/create_service.rb

use op_contracts::base::UserContext;
use op_contracts::work_packages::{admit_create, AdmissionFacts, CreateAdmission, Verdict};
use op_core::error::ValidationErrors;
use op_core::traits::Id;

use crate::result::ServiceResult;
use super::set_attributes::{SetAttributesService, WorkPackageEntity};
use super::WorkPackageParams;

/// Service for creating work packages
///
/// # Example
/// ```ignore
/// let service = CreateWorkPackageService::new(&user);
/// let params = WorkPackageParams::new()
///     .with_subject("New Feature")
///     .with_project_id(1)
///     .with_type_id(1);
/// let result = service.call(params);
/// ```
pub struct CreateWorkPackageService<'a, U: UserContext> {
    user: &'a U,
    send_notifications: bool,
    facts: Option<&'a dyn AdmissionFacts>,
}

/// No model facts supplied: every fact is unavailable, so a guard that reads one is
/// unresolved. Nothing is assumed true.
struct NoFacts;

impl AdmissionFacts for NoFacts {
    fn fact(&self, _name: &str) -> Option<bool> {
        None
    }
}

impl<'a, U: UserContext> CreateWorkPackageService<'a, U> {
    pub fn new(user: &'a U) -> Self {
        Self {
            user,
            send_notifications: true,
            facts: None,
        }
    }

    pub fn without_notifications(user: &'a U) -> Self {
        Self {
            user,
            send_notifications: false,
            facts: None,
        }
    }

    /// Supply the model facts the admission guards read (`leaf?`, status closed, ...).
    /// Without them a submitted attribute whose guard reads a fact is unresolved and
    /// `call` refuses (`admission_unresolved`); nothing is read as true.
    pub fn with_facts(mut self, facts: &'a dyn AdmissionFacts) -> Self {
        self.facts = Some(facts);
        self
    }

    /// Admission of a create under `WorkPackages::CreateContract`, evaluated by
    /// `op_contracts::work_packages::admit_create` from the table generated out of
    /// the OpenProject contracts: the authorization rule plus the read-only check of
    /// each submitted attribute. `submitted` names the attributes the caller sends
    /// (as the API receives them); no attribute values are read.
    ///
    /// [`CreateWorkPackageService::call`] runs this first and acts on the verdict.
    pub fn admit(
        &self,
        project_id: Option<Id>,
        submitted: &[&str],
        facts: &dyn AdmissionFacts,
    ) -> CreateAdmission {
        admit_create(self.user, project_id, submitted, facts)
    }

    /// Execute the create operation
    pub fn call(self, params: WorkPackageParams) -> ServiceResult<WorkPackageEntity> {
        // Admission under the generated CreateContract rules comes first. A denial
        // ends the call with the contract's error keys and codes. A verdict that
        // cannot be made (a fact the rules read was not supplied, or the rule is not
        // extractable) is a typed refusal, never an admission.
        let names = submitted_attributes(&params);
        let facts: &dyn AdmissionFacts = self.facts.unwrap_or(&NoFacts);
        let admission = self.admit(params.project_id, &names, facts);
        // `WorkPackage` validates `project` presence (app/models/work_package/
        // validations.rb:34). The model and contract validations run together,
        // so a denied request without a project reports both. No project is
        // ever chosen for the caller: the project admitted is the project used.
        let missing_project = params.project_id.is_none();
        match &admission.verdict {
            Verdict::Denied => {
                let mut errors = ValidationErrors::new();
                if missing_project {
                    errors.add("project", "blank");
                }
                for (key, codes) in &admission.errors {
                    for code in codes {
                        if key == "base" {
                            errors.add_base(*code);
                        } else {
                            errors.add(key.clone(), *code);
                        }
                    }
                }
                return ServiceResult::failure(errors);
            }
            Verdict::Unresolved(reason) => {
                return ServiceResult::failure_with_base_error(format!(
                    "admission_unresolved: {reason:?}"
                ));
            }
            Verdict::Admitted => {}
        }
        let Some(project_id) = params.project_id else {
            let mut errors = ValidationErrors::new();
            errors.add("project", "blank");
            return ServiceResult::failure(errors);
        };
        // Without a type, Rails assigns the project's first enabled type
        // (`SetAttributesService#assign_default_type`). Project types are not
        // modeled here, so the default is a typed refusal, never a guessed id.
        let Some(type_id) = params.type_id else {
            return ServiceResult::failure_with_base_error(
                "type_default_unresolved: the project's first enabled type is not modeled",
            );
        };

        // HAND-WRITTEN PATH, only for what the generated table has no rule for: the
        // other contract validators (`not_evaluated_validators()`) and the model
        // validations, as `CreateWorkPackageContract` encodes them by hand.
        let work_package = WorkPackageEntity::new(project_id, type_id, self.user.id());

        // Set attributes and validate
        let set_attrs_service = SetAttributesService::new(self.user, work_package);
        let result = set_attrs_service.call(&params);

        if result.is_failure() {
            return result;
        }

        let mut work_package = result.unwrap();

        // In a real implementation, this would persist to database
        // For now, we simulate assigning an ID
        work_package.id = Some(self.next_id());

        // Create result
        let mut service_result = ServiceResult::success(work_package.clone());

        // Handle notifications (would be implemented with actual notification service)
        if self.send_notifications && params.send_notifications {
            // Would send notifications here
        }

        // Handle ancestor updates (would be implemented with UpdateAncestors service)
        // self.update_ancestors(&work_package, &mut service_result);

        // Handle scheduling (would be implemented with SetScheduleService)
        // self.reschedule_related(&work_package, &mut service_result);

        // Add user as watcher (would be implemented with Watchers service)
        // self.set_user_as_watcher(&work_package);

        service_result
    }

    fn next_id(&self) -> Id {
        // In real implementation, this would come from database
        // For now, use a simple counter simulation
        1000
    }
}

/// The attribute names a create submits, as the API names them. Values are not read.
fn submitted_attributes(p: &WorkPackageParams) -> Vec<&'static str> {
    let present = [
        (p.subject.is_some(), "subject"),
        (p.description.is_some(), "description"),
        (p.project_id.is_some(), "project"),
        (p.type_id.is_some(), "type"),
        (p.status_id.is_some(), "status"),
        (p.priority_id.is_some(), "priority"),
        (p.assigned_to_id.is_some(), "assigned_to"),
        (p.responsible_id.is_some(), "responsible"),
        (p.start_date.is_some(), "start_date"),
        (p.due_date.is_some(), "due_date"),
        (p.estimated_hours.is_some(), "estimated_hours"),
        (p.done_ratio.is_some(), "done_ratio"),
        (p.parent_id.is_some(), "parent"),
        (p.version_id.is_some(), "target_versions"),
        (p.category_id.is_some(), "category"),
    ];
    present.iter().filter(|(on, _)| *on).map(|(_, n)| *n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct MockUser {
        id: Id,
        admin: bool,
        project_permissions: std::collections::HashMap<Id, HashSet<String>>,
    }

    impl UserContext for MockUser {
        fn id(&self) -> Id {
            self.id
        }

        fn is_admin(&self) -> bool {
            self.admin
        }

        fn is_anonymous(&self) -> bool {
            false
        }

        fn allowed_in_project(&self, permission: &str, project_id: Id) -> bool {
            self.project_permissions
                .get(&project_id)
                .map(|perms| perms.contains(permission))
                .unwrap_or(false)
        }

        fn allowed_globally(&self, _permission: &str) -> bool {
            false
        }

        fn allowed_in_any_project(&self, permission: &str) -> bool {
            self.project_permissions
                .values()
                .any(|perms| perms.contains(permission))
        }
    }

    fn create_admin_user() -> MockUser {
        MockUser {
            id: 1,
            admin: true,
            project_permissions: std::collections::HashMap::new(),
        }
    }

    fn create_user_with_permission() -> MockUser {
        let mut permissions = std::collections::HashMap::new();
        let mut project_perms = HashSet::new();
        project_perms.insert("add_work_packages".to_string());
        permissions.insert(1, project_perms);

        MockUser {
            id: 2,
            admin: false,
            project_permissions: permissions,
        }
    }

    #[test]
    fn test_create_work_package_as_admin() {
        let user = create_admin_user();
        let service = CreateWorkPackageService::new(&user);

        let params = WorkPackageParams::new()
            .with_subject("New Feature")
            .with_project_id(1)
            .with_type_id(1);

        let result = service.call(params);
        assert!(result.is_success());

        let wp = result.result().unwrap();
        assert_eq!(wp.subject, "New Feature");
        assert!(wp.id.is_some());
        assert_eq!(wp.author_id, user.id);
    }

    #[test]
    fn test_create_work_package_with_permission() {
        let user = create_user_with_permission();
        let service = CreateWorkPackageService::new(&user);

        let params = WorkPackageParams::new()
            .with_subject("Bug Fix")
            .with_project_id(1)
            .with_type_id(1);

        let result = service.call(params);
        assert!(result.is_success());
    }

    #[test]
    fn test_create_work_package_without_permission() {
        let user = MockUser {
            id: 3,
            admin: false,
            project_permissions: std::collections::HashMap::new(),
        };
        let service = CreateWorkPackageService::new(&user);

        let params = WorkPackageParams::new()
            .with_subject("Unauthorized")
            .with_project_id(1)
            .with_type_id(1);

        let result = service.call(params);
        assert!(result.is_failure());
    }

    #[test]
    fn test_create_work_package_validation_failure() {
        let user = create_admin_user();
        let service = CreateWorkPackageService::new(&user);

        // Missing subject
        let params = WorkPackageParams::new()
            .with_project_id(1)
            .with_type_id(1);

        let result = service.call(params);
        assert!(result.is_failure());
        assert!(result.errors().has_error("subject"));
    }

    #[test]
    fn a_guard_that_reads_an_unsupplied_fact_refuses_instead_of_admitting() {
        let user = create_user_with_permission();
        let params = WorkPackageParams::new()
            .with_subject("Needs a status fact")
            .with_project_id(1)
            .with_type_id(1)
            .with_status_id(1);
        let result = CreateWorkPackageService::new(&user).call(params);
        assert!(result.is_failure());
        assert!(result.errors().base_errors[0].starts_with("admission_unresolved"));
    }

    #[test]
    fn a_supplied_fact_resolves_the_guard() {
        struct Open;
        impl AdmissionFacts for Open {
            fn fact(&self, name: &str) -> Option<bool> {
                match name {
                    "model.status_id_change" => Some(true),
                    "model.status&.closed?" | "model.status.closed?" => Some(false),
                    "model.effective_target_versions.any?(&:closed?)" => Some(false),
                    _ => None,
                }
            }
        }
        let user = create_user_with_permission();
        let params = WorkPackageParams::new()
            .with_subject("Has facts")
            .with_project_id(1)
            .with_type_id(1)
            .with_status_id(1);
        let result = CreateWorkPackageService::new(&user).with_facts(&Open).call(params);
        assert!(result.is_success());
    }

    #[test]
    fn test_create_without_notifications() {
        let user = create_admin_user();
        let service = CreateWorkPackageService::without_notifications(&user);

        let params = WorkPackageParams::new()
            .with_subject("Silent Feature")
            .with_project_id(1)
            .with_type_id(1);

        let result = service.call(params);
        assert!(result.is_success());
    }

    fn user_in_project_5_only() -> MockUser {
        let mut permissions = std::collections::HashMap::new();
        permissions.insert(5, HashSet::from(["add_work_packages".to_string()]));
        MockUser {
            id: 3,
            admin: false,
            project_permissions: permissions,
        }
    }

    #[test]
    fn a_create_without_a_project_never_picks_one() {
        let user = user_in_project_5_only();
        // The any-project rule admits this user, so only the missing project
        // can stop the create.
        assert!(user.allowed_in_any_project("add_work_packages"));
        let params = WorkPackageParams::new()
            .with_subject("No project")
            .with_type_id(1);
        let result = CreateWorkPackageService::new(&user).call(params);
        assert!(result.is_failure());
        assert!(result.result().is_none(), "no entity may be produced");
        assert!(result.errors().has_error("project"));
    }

    #[test]
    fn the_project_admitted_is_the_project_assigned() {
        let user = user_in_project_5_only();
        let params = WorkPackageParams::new()
            .with_subject("In five")
            .with_project_id(5)
            .with_type_id(1);
        let result = CreateWorkPackageService::new(&user).call(params);
        assert!(result.is_success());
        assert_eq!(result.result().map(|wp| wp.project_id), Some(5));
        // Project 1 is not granted, and is never reached by default.
        let params = WorkPackageParams::new()
            .with_subject("In one")
            .with_project_id(1)
            .with_type_id(1);
        assert!(CreateWorkPackageService::new(&user).call(params).is_failure());
    }

    #[test]
    fn a_missing_type_is_a_typed_refusal_not_type_1() {
        let user = user_in_project_5_only();
        let params = WorkPackageParams::new()
            .with_subject("No type")
            .with_project_id(5);
        let result = CreateWorkPackageService::new(&user).call(params);
        assert!(result.is_failure());
        assert!(result.result().is_none());
    }
}
