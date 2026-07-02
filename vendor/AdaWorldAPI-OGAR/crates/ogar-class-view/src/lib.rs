//! `ogar-class-view` — the bridge from
//! [`ogar_vocab::Class`] (the calcified canonical AR shape, codebook-keyed)
//! onto [`lance_graph_contract::ClassView`] (the presence-bitmask + render-row
//! resolver).
//!
//! # What this crate is
//!
//! The seam between the *inner* design (canonical concept + typed attributes +
//! family edges, minted in [`ogar_vocab::CODEBOOK`]) and the *outer* design
//! (askama/jinja templates that materialize the same shape into Rust / TS /
//! SurrealQL / OpenAPI / …). The "ClassView" of
//! `lance-graph-contract::class_view` already supplies the *contract*:
//!
//! ```text
//!   SoA row          =  the XML document      (agnostic bytes)
//!   class            =  the XSD schema        (ordered field set)
//!   ClassView        =  parser+schema         (projects row → typed view)
//!   FieldMask        =  optional-elements presence
//!   askama template  =  the XSLT              (renders the projection)
//! ```
//!
//! This crate is the **adapter**: it builds an
//! [`ObjectView`](lance_graph_contract::ObjectView) per promoted canonical
//! concept by walking `ogar-vocab`'s class fns, keys them by
//! [`canonical_concept_id`](ogar_vocab::canonical_concept_id) (= the
//! `lance_graph_contract::ClassId`), and exposes the whole set through one
//! [`ClassView`](lance_graph_contract::ClassView) impl ([`OgarClassView`]).
//!
//! Once a renderer holds an `OgarClassView`, `class_view.render_rows(id, mask)`
//! returns typed rows the askama template iterates — no codegen output is
//! emitted *here*; that is the deferred render crate's job.
//!
//! # N3 stable field order (the bit basis)
//!
//! `FieldMask` bit `n` corresponds to the `n`-th `FieldRef` in the
//! [`ObjectView::fields`](lance_graph_contract::ObjectView::fields) slice.
//! Stability is contractual: once instances persist, bit positions are
//! append-only ([`lance_graph_contract::FieldMask`] doc).
//!
//! This crate's chosen order:
//!
//! 1. **Attributes first**, in declaration order from the class fn
//!    (`name`, `position`, `permissions`, …).
//! 2. **Family edges (`Association`s) after**, in declaration order
//!    (e.g. `belongs_to :project`, `has_many :work_items`, …).
//!
//! Tests in this crate pin that order for every promoted concept. A regen
//! that drops or reorders a slot trips them.
//!
//! # Why this crate has no `serde`, no I/O
//!
//! It is a *pure* in-process adapter. The registry is constructed at startup
//! by calling the promoted class fns; nothing reads files or parses JSON.
//! Renderers that *do* need persistence (templating output, etc.) sit
//! downstream.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;

use lance_graph_contract::{
    class_view::{ClassId, ClassView},
    ontology::{DisplayTemplate, FieldRef, ObjectView},
};
use ogar_vocab::{
    Class,
    accounting_account,
    // 0x0CXX — automation (HIRO MARS CMDB + DO-arm actuators)
    action_applicability,
    action_handler,
    anatomical_structure,
    auth_ory_keto,
    auth_store,
    auth_zanzibar,
    auth_zitadel,
    automation_trigger,
    billable_work_entry,
    billing_party,
    bone,
    canonical_concept_id,
    commercial_document,
    commercial_line_item,
    currency_policy,
    diagnosis,
    hr_department,
    hr_employee,
    hr_employment_contract,
    hr_job,
    joint,
    knowledge_item,
    lab_value,
    mars_application,
    mars_machine,
    mars_node_template,
    mars_resource,
    mars_software,
    medication,
    patient,
    payment_record,
    pricelist,
    pricelist_rule,
    priority,
    product,
    project,
    project_actor,
    project_attachment,
    project_changeset,
    project_comment,
    project_custom_field,
    project_custom_value,
    project_enabled_module,
    project_forum,
    project_journal,
    project_member_role,
    project_membership,
    project_message,
    project_news,
    project_query,
    project_relation,
    project_repository,
    project_role,
    project_status,
    project_type,
    project_version,
    project_watcher,
    project_wiki_page,
    project_work_item,
    skeleton,
    tax_policy,
    treatment,
    unit_of_measure,
    visit,
    vital_sign,
};

/// All promoted canonical concepts: `(canonical_concept_name, Class)`.
///
/// Walked at startup by [`OgarClassView::new`]. The list is exhaustive against
/// [`ogar_vocab::CODEBOOK`] — a test in this crate fails if a codebook entry
/// is missing from here (or vice versa). Adding a new canonical concept means
/// appending one line in OGAR (the class fn + codebook id) and one line here.
fn all_canonical_classes() -> Vec<(&'static str, Class)> {
    vec![
        // ── 0x01XX — project-mgmt ──
        ("project", project()),
        ("project_work_item", project_work_item()),
        ("billable_work_entry", billable_work_entry()),
        ("project_actor", project_actor()),
        ("project_status", project_status()),
        ("project_type", project_type()),
        ("priority", priority()),
        ("project_membership", project_membership()),
        ("project_journal", project_journal()),
        ("project_repository", project_repository()),
        ("project_version", project_version()),
        ("project_wiki_page", project_wiki_page()),
        ("project_query", project_query()),
        ("project_attachment", project_attachment()),
        ("project_comment", project_comment()),
        ("project_custom_field", project_custom_field()),
        ("project_relation", project_relation()),
        ("project_changeset", project_changeset()),
        ("project_watcher", project_watcher()),
        ("project_news", project_news()),
        ("project_message", project_message()),
        ("project_forum", project_forum()),
        ("project_role", project_role()),
        ("project_member_role", project_member_role()),
        ("project_custom_value", project_custom_value()),
        ("project_enabled_module", project_enabled_module()),
        // ── 0x02XX — commerce ──
        ("commercial_line_item", commercial_line_item()),
        ("commercial_document", commercial_document()),
        ("tax_policy", tax_policy()),
        ("billing_party", billing_party()),
        ("payment_record", payment_record()),
        ("currency_policy", currency_policy()),
        ("product", product()),
        ("accounting_account", accounting_account()),
        ("pricelist", pricelist()),
        ("pricelist_rule", pricelist_rule()),
        ("unit_of_measure", unit_of_measure()),
        // ── 0x09XX — health (OGIT Healthcare) ──
        ("patient", patient()),
        ("diagnosis", diagnosis()),
        ("lab_value", lab_value()),
        ("medication", medication()),
        ("treatment", treatment()),
        ("visit", visit()),
        ("vital_sign", vital_sign()),
        // ── 0x0AXX — anatomy (FMA reference kinds) ──
        ("anatomical_structure", anatomical_structure()),
        ("skeleton", skeleton()),
        ("bone", bone()),
        ("joint", joint()),
        // ── 0x0BXX — auth (the AuthStore class family, keystone §7) ──
        ("auth_store", auth_store()),
        ("auth_zitadel", auth_zitadel()),
        ("auth_zanzibar", auth_zanzibar()),
        ("auth_ory_keto", auth_ory_keto()),
        // ── 0x0DXX — HR cluster (closes the final 4-of-11 odoo-rs #14 gap) ──
        ("hr_employee", hr_employee()),
        ("hr_department", hr_department()),
        ("hr_job", hr_job()),
        ("hr_employment_contract", hr_employment_contract()),
        // ── 0x0CXX — automation (HIRO MARS CMDB + DO-arm actuators) ──
        ("mars_application", mars_application()),
        ("mars_resource", mars_resource()),
        ("mars_software", mars_software()),
        ("mars_machine", mars_machine()),
        ("knowledge_item", knowledge_item()),
        ("mars_node_template", mars_node_template()),
        ("action_handler", action_handler()),
        ("action_applicability", action_applicability()),
        ("automation_trigger", automation_trigger()),
    ]
}

/// Lift one canonical [`Class`] into its `ObjectView` (the field basis the
/// `FieldMask` bits index).
///
/// Field order — **N3 stable, append-only**:
/// 1. Typed attributes, in declaration order on the class fn.
/// 2. Family-edge associations, in declaration order on the class fn.
///
/// `predicate_iri` is the slot's name as written (snake_case); `label`
/// mirrors it (display localisation happens downstream). The
/// `DisplayTemplate` is left as the default
/// [`DisplayTemplate::Detail`] — per-class template selection is the
/// renderer's job, not this adapter's.
fn lift_object_view(class: &Class) -> ObjectView {
    let mut fields: Vec<FieldRef> =
        Vec::with_capacity(class.attributes.len() + class.associations.len());
    for attr in &class.attributes {
        fields.push(FieldRef::new(attr.name.clone(), attr.name.clone()));
    }
    for assoc in &class.associations {
        fields.push(FieldRef::new(assoc.name.clone(), assoc.name.clone()));
    }
    let mut view = ObjectView::new(DisplayTemplate::Detail, fields);
    // Convention: primary_label is the first typed attribute named "name"
    // when present, falling back to None (the contract treats None as
    // "first field" — see `ObjectView` doc).
    if class.attributes.iter().any(|a| a.name == "name") {
        view.primary_label = Some("name".to_string());
    }
    view
}

/// [`ClassView`] implementation backed by [`ogar_vocab`]'s promoted
/// canonical concepts.
///
/// Construct once at startup with [`OgarClassView::new`]; the registry is
/// then read-only for the rest of the process. Per-class `ObjectView`s
/// (the field basis, ordered, N3 stable) and the empty-field-list fallback
/// outlive the view.
pub struct OgarClassView {
    by_id: HashMap<ClassId, ObjectView>,
    /// The fallback returned by [`ClassView::fields`] when a class id is
    /// not in the registry — empty slice, so a `FieldMask` over an unknown
    /// class projects to zero rows (consumer skips it).
    empty_fields: Vec<FieldRef>,
}

impl OgarClassView {
    /// Build the registry by walking every promoted class fn in
    /// [`ogar_vocab`] and lifting it onto an [`ObjectView`]. Pure
    /// construction; no I/O.
    #[must_use]
    pub fn new() -> Self {
        let mut by_id = HashMap::new();
        for (concept, class) in all_canonical_classes() {
            let id = canonical_concept_id(concept).unwrap_or_else(|| {
                panic!("{concept} is in all_canonical_classes() but not in OGAR CODEBOOK")
            });
            by_id.insert(id, lift_object_view(&class));
        }
        Self {
            by_id,
            empty_fields: Vec::new(),
        }
    }

    /// The promoted class ids the registry currently exposes, in **stable
    /// codebook order** (matches [`ogar_vocab::class_ids::ALL`]).
    ///
    /// Codex P2 on PR #77: the previous shape borrowed `HashMap`'s
    /// randomized iteration, so downstream renderers using this for bulk
    /// emission could reorder generated structs / drift-guard snapshots
    /// across process runs without any schema change. Stable order is
    /// load-bearing for diff hygiene of generated artifacts.
    pub fn known_class_ids(&self) -> impl Iterator<Item = ClassId> + '_ {
        ogar_vocab::class_ids::ALL
            .iter()
            .filter_map(|(_, id)| self.by_id.contains_key(id).then_some(*id))
    }

    /// Look up the [`ObjectView`] (the full per-class render spec —
    /// fields + display template + primary label) for a class id, if
    /// known. Lower-level than [`ClassView`]; useful when a renderer
    /// needs the `primary_label` or `DisplayTemplate` directly.
    pub fn object_view(&self, class: ClassId) -> Option<&ObjectView> {
        self.by_id.get(&class)
    }
}

impl Default for OgarClassView {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassView for OgarClassView {
    fn fields(&self, class: ClassId) -> &[FieldRef] {
        self.by_id
            .get(&class)
            .map(|v| v.fields.as_slice())
            .unwrap_or(self.empty_fields.as_slice())
    }

    fn template(&self, class: ClassId) -> DisplayTemplate {
        self.by_id
            .get(&class)
            .map(|v| v.display_template.clone())
            .unwrap_or(DisplayTemplate::Detail)
    }

    fn dolce_category_id(&self, _class: ClassId) -> u8 {
        // DOLCE upper-category classification is the OGIT cache's job
        // (`OD-DOLCE: use the ontology cache`). This adapter is below
        // OGIT; the renderer that wires us with a DOLCE source supplies
        // its own mapping. Default `0` = unclassified.
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::class_view::FieldMask;

    #[test]
    fn registry_carries_every_codebook_concept() {
        // Forward gate: every concept in `all_canonical_classes` resolves
        // to a known codebook id and lands in the registry.
        let v = OgarClassView::new();
        for (concept, _) in all_canonical_classes() {
            let id = canonical_concept_id(concept)
                .unwrap_or_else(|| panic!("{concept} missing from OGAR CODEBOOK"));
            assert!(
                v.object_view(id).is_some(),
                "{concept} ({id:#06x}) absent from OgarClassView registry"
            );
        }
        // The registry knows the full promoted concept set.
        assert_eq!(v.known_class_ids().count(), all_canonical_classes().len());
    }

    #[test]
    fn known_class_ids_iterates_in_stable_codebook_order() {
        // Codex P2 on #77: the iterator must be deterministic across
        // process runs (no HashMap randomization leaking through), and
        // its order must match `ogar_vocab::class_ids::ALL` — the same
        // order downstream consumers' drift-guard snapshots are pinned
        // against.
        let v = OgarClassView::new();
        let got: Vec<ClassId> = v.known_class_ids().collect();
        let expected: Vec<ClassId> = ogar_vocab::class_ids::ALL
            .iter()
            .filter_map(|(_, id)| {
                // class_ids::ALL spans all domains (project-mgmt +
                // commerce); the registry holds every promoted concept,
                // so the filter is a no-op today but stays robust if
                // OGAR ever adds a concept this crate doesn't lift.
                canonical_concept_id(
                    ogar_vocab::class_ids::ALL
                        .iter()
                        .find(|(_, i)| i == id)
                        .map(|(n, _)| *n)
                        .unwrap_or(""),
                )
                .map(|_| *id)
            })
            .collect();
        assert_eq!(got, expected, "known_class_ids drifted from codebook order");
        // A second call returns the same sequence — no HashMap pollution.
        let again: Vec<ClassId> = v.known_class_ids().collect();
        assert_eq!(got, again);
    }

    #[test]
    fn every_codebook_id_appears_in_class_ids_all() {
        // Reverse gate: every (name, id) in `ogar_vocab::class_ids::ALL`
        // must have a registry entry — guards against a CODEBOOK promotion
        // landing in OGAR without being added here.
        let v = OgarClassView::new();
        for (concept, id) in ogar_vocab::class_ids::ALL {
            assert!(
                v.object_view(*id).is_some(),
                "{concept} ({id:#06x}) in class_ids::ALL but missing from OgarClassView registry"
            );
        }
    }

    #[test]
    fn field_basis_fits_in_one_u64_mask() {
        // FieldMask is a u64 — bit positions >= 64 are silently dropped
        // (`lance_graph_contract::FieldMask::from_positions` doc). No
        // promoted canonical class may have more than 64 slots.
        let v = OgarClassView::new();
        for (concept, _) in all_canonical_classes() {
            let id = canonical_concept_id(concept).unwrap();
            let n = v.field_count(id);
            assert!(
                n <= FieldMask::MAX_FIELDS as usize,
                "{concept} has {n} fields, exceeds FieldMask::MAX_FIELDS ({})",
                FieldMask::MAX_FIELDS
            );
        }
    }

    #[test]
    fn field_order_is_attributes_then_associations() {
        // The N3 bit-basis convention: typed attributes first (in
        // declaration order), then family-edge associations (in
        // declaration order). Pinned here so any reordering trips this
        // test before existing FieldMask producers silently misalign.
        let v = OgarClassView::new();
        let id = canonical_concept_id("billable_work_entry").unwrap();
        let class = billable_work_entry();
        let fields = v.fields(id);

        // First `class.attributes.len()` positions are attributes, in
        // declaration order.
        for (i, attr) in class.attributes.iter().enumerate() {
            assert_eq!(
                fields[i].predicate_iri, attr.name,
                "billable_work_entry field[{i}] should be attribute {}",
                attr.name
            );
        }
        // Then `class.associations.len()` positions are associations.
        for (i, assoc) in class.associations.iter().enumerate() {
            let pos = class.attributes.len() + i;
            assert_eq!(
                fields[pos].predicate_iri, assoc.name,
                "billable_work_entry field[{pos}] should be association {}",
                assoc.name
            );
        }
        assert_eq!(
            fields.len(),
            class.attributes.len() + class.associations.len()
        );
    }

    #[test]
    fn render_rows_skips_off_bits_under_a_real_mask() {
        // The full path: class_id + FieldMask -> Vec<RenderRow>. Pin the
        // render_rows behaviour the askama template will iterate against.
        let v = OgarClassView::new();
        let id = canonical_concept_id("project_work_item").unwrap();
        let n = v.field_count(id);
        assert!(n > 0, "project_work_item must have at least one slot");

        // Empty mask -> no rows.
        let empty = v.render_rows(id, FieldMask::EMPTY);
        assert!(empty.is_empty());

        // Mask with only bit 0 set -> exactly one row, matching field[0]'s label.
        let only_first = FieldMask::EMPTY.with(0);
        let rows = v.render_rows(id, only_first);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, v.fields(id)[0].label.as_str());
    }

    #[test]
    fn unknown_class_id_projects_to_no_rows() {
        // A class id outside the registry must NOT panic — it projects
        // to an empty field set, so the renderer skips the row safely.
        let v = OgarClassView::new();
        assert!(v.fields(0xFFFF).is_empty());
        let rows = v.render_rows(0xFFFF, FieldMask::EMPTY.with(0).with(3));
        assert!(rows.is_empty());
    }

    #[test]
    fn billable_work_entry_carries_all_12_family_edges() {
        // The 12 family edges of billable_work_entry are part of its
        // promoted contract (see `ogar_vocab::billable_work_entry` doc).
        // The bridge must surface every one as a FieldRef.
        let v = OgarClassView::new();
        let id = canonical_concept_id("billable_work_entry").unwrap();
        let class = billable_work_entry();
        let assoc_names: std::collections::HashSet<&str> =
            class.associations.iter().map(|a| a.name.as_str()).collect();
        let field_names: std::collections::HashSet<&str> = v
            .fields(id)
            .iter()
            .map(|f| f.predicate_iri.as_str())
            .collect();
        for assoc in &assoc_names {
            assert!(
                field_names.contains(assoc),
                "billable_work_entry family edge `{assoc}` missing from FieldRef set"
            );
        }
        assert!(class.associations.len() >= 12);
    }

    #[test]
    fn primary_label_is_set_for_concepts_with_a_name_attribute() {
        // Convention: concepts that have a typed `name` attribute carry
        // `primary_label = Some("name")` so a renderer can pull the
        // headline without scanning the field list.
        let v = OgarClassView::new();
        let id = canonical_concept_id("project").unwrap();
        let view = v.object_view(id).unwrap();
        // project() has a `name` attribute, so primary_label is set.
        assert_eq!(view.primary_label.as_deref(), Some("name"));
    }

    #[test]
    fn health_concepts_are_registered_with_their_fields() {
        // The 7 OGIT Healthcare concepts resolve to registry entries —
        // this is what makes `every_codebook_id_appears_in_class_ids_all`
        // green for the 0x09XX block.
        let v = OgarClassView::new();
        for concept in [
            "patient",
            "diagnosis",
            "lab_value",
            "medication",
            "treatment",
            "visit",
            "vital_sign",
        ] {
            let id = canonical_concept_id(concept).unwrap();
            let view = v
                .object_view(id)
                .unwrap_or_else(|| panic!("{concept} not registered"));
            assert!(!view.fields.is_empty(), "{concept} has no field basis");
        }
    }

    #[test]
    fn diagnosis_field_basis_is_ten_attributes_then_two_edges() {
        // The 0x0902 worked example, projected: attributes first (icd_code
        // leads), then the two family edges, in declaration order.
        let v = OgarClassView::new();
        let id = canonical_concept_id("diagnosis").unwrap();
        let fields = v.fields(id);
        assert_eq!(fields.len(), 12); // 10 attributes + 2 edges
        assert_eq!(fields[0].predicate_iri, "icd_code");
        assert_eq!(fields[10].predicate_iri, "patient");
        assert_eq!(fields[11].predicate_iri, "encounter");
    }
}
