//! Ontology contract — the unifying layer that composes PropertySchema,
//! LinkSpec, ActionSpec, and model integration into a Foundry-equivalent
//! typed object model.
//!
//! Covers Palantir Foundry stages 3-5:
//! - Stage 3 (Model Integration): ModelBinding connects external model
//!   I/O to ontology properties via PropertySpec.
//! - Stage 4 (Model Ops): ModelHealth tracks prediction quality via
//!   NARS truth values per model-property pair.
//! - Stage 5 (Decisions / Learning): SimulationSpec parameterises
//!   World::fork() what-if scenarios.
//!
//! Zero-dep. All types are trait-shape or plain structs.

// `PrefetchDepth` and `PropertyKind` retained for wiring the prefetch hint
// from ontology.action_spec into Schema lookups (TD-ONTO-1). Currently the
// fetch path uses ActionSpec.fetch_max_rows and LinkSpec only.
use crate::cam::CodecRoute;
#[allow(unused_imports)]
use crate::property::{
    ActionSpec, LinkSpec, Marking, PrefetchDepth, PropertyKind, Schema, SemanticType,
};

// ═══════════════════════════════════════════════════════════════════════════
// Locale + Label — bilingual ontology support (DE/EN)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Locale {
    En,
    De,
}

impl Locale {
    pub const fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::De => "de",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub key: &'static str,
    pub en: &'static str,
    pub de: &'static str,
}

impl Label {
    pub const fn new(key: &'static str, en: &'static str, de: &'static str) -> Self {
        Self { key, en, de }
    }

    pub const fn en_only(key: &'static str) -> Self {
        Self {
            key,
            en: key,
            de: key,
        }
    }

    pub const fn display(&self, locale: Locale) -> &str {
        match locale {
            Locale::En => self.en,
            Locale::De => self.de,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EntityTypeId — Foundry Object Type equivalent (Column H in BindSpace SoA)
// ═══════════════════════════════════════════════════════════════════════════

/// Numeric entity type identifier for per-row BindSpace typing.
/// 0 = untyped. Non-zero = index into `Ontology.schemas` (1-based).
///
/// This is the Palantir Vertex "Object Type" equivalent. Every row in
/// BindSpace can be typed, enabling Object Explorer scrolling, property
/// view selection (LF-22 ObjectView), and type-filtered search (LF-40).
pub type EntityTypeId = u16;

/// Look up the EntityTypeId for a named entity type within an Ontology.
/// Returns 0 if the name doesn't match any schema.
pub fn entity_type_id(ontology: &Ontology, name: &str) -> EntityTypeId {
    ontology
        .schemas
        .iter()
        .position(|s| s.name == name)
        .map(|idx| (idx + 1) as EntityTypeId)
        .unwrap_or(0)
}

// ═══════════════════════════════════════════════════════════════════════════
// Ontology — the composed object model
// ═══════════════════════════════════════════════════════════════════════════

/// A complete ontology definition: schemas + links + actions.
/// This is the Foundry Ontology equivalent — the "semantic model
/// representing the enterprise as business objects."
#[derive(Clone, Debug)]
pub struct Ontology {
    pub name: &'static str,
    pub label: Label,
    pub locale: Locale,
    pub schemas: Vec<Schema>,
    pub links: Vec<LinkSpec>,
    pub actions: Vec<ActionSpec>,
}

impl Ontology {
    pub fn builder(name: &'static str) -> OntologyBuilder {
        OntologyBuilder {
            name,
            label: Label::en_only(name),
            locale: Locale::En,
            schemas: Vec::new(),
            links: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub fn schema(&self, entity_type: &str) -> Option<&Schema> {
        self.schemas.iter().find(|s| s.name == entity_type)
    }

    pub fn links_from(&self, subject_type: &str) -> Vec<&LinkSpec> {
        self.links
            .iter()
            .filter(|l| l.subject_type == subject_type)
            .collect()
    }

    pub fn links_to(&self, object_type: &str) -> Vec<&LinkSpec> {
        self.links
            .iter()
            .filter(|l| l.object_type == object_type)
            .collect()
    }

    pub fn actions_for(&self, entity_type: &str) -> Vec<&ActionSpec> {
        self.actions
            .iter()
            .filter(|a| a.entity_type == entity_type)
            .collect()
    }
}

pub struct OntologyBuilder {
    name: &'static str,
    label: Label,
    locale: Locale,
    schemas: Vec<Schema>,
    links: Vec<LinkSpec>,
    actions: Vec<ActionSpec>,
}

impl OntologyBuilder {
    pub fn label(mut self, label: Label) -> Self {
        self.label = label;
        self
    }

    pub fn locale(mut self, locale: Locale) -> Self {
        self.locale = locale;
        self
    }

    pub fn schema(mut self, schema: Schema) -> Self {
        self.schemas.push(schema);
        self
    }

    pub fn link(mut self, link: LinkSpec) -> Self {
        self.links.push(link);
        self
    }

    pub fn action(mut self, action: ActionSpec) -> Self {
        self.actions.push(action);
        self
    }

    pub fn build(self) -> Ontology {
        Ontology {
            name: self.name,
            label: self.label,
            locale: self.locale,
            schemas: self.schemas,
            links: self.links,
            actions: self.actions,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Model Binding — Foundry Stage 3 (connect model I/O to ontology)
// ═══════════════════════════════════════════════════════════════════════════

/// Binds an external model's input/output to ontology properties.
/// When a model predicts "industry" for a customer, the binding
/// tells the system: read these input properties, write to this
/// output property, track quality via NARS truth on the output.
#[derive(Clone, Debug)]
pub struct ModelBinding {
    pub model_id: &'static str,
    pub entity_type: &'static str,
    /// Properties read as model input features.
    pub input_properties: &'static [&'static str],
    /// Property written with model output.
    pub output_property: &'static str,
    /// Expected codec route for the output (CamPq for embeddings,
    /// Passthrough for classifications).
    pub output_codec: CodecRoute,
}

impl ModelBinding {
    pub const fn new(
        model_id: &'static str,
        entity_type: &'static str,
        inputs: &'static [&'static str],
        output: &'static str,
        codec: CodecRoute,
    ) -> Self {
        Self {
            model_id,
            entity_type,
            input_properties: inputs,
            output_property: output,
            output_codec: codec,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Model Health — Foundry Stage 4 (NARS-based monitoring)
// ═══════════════════════════════════════════════════════════════════════════

/// Per-model, per-property health tracking via NARS truth values.
/// frequency = prediction accuracy (how often the model is right).
/// confidence = sample size (how many predictions have been evaluated).
///
/// When frequency drops below the PropertySpec's nars_floor, the
/// system generates a FailureTicket — same as a missing Required
/// property, but caused by model drift rather than absence.
#[derive(Clone, Copy, Debug)]
pub struct ModelHealth {
    pub model_id_hash: u64,
    pub property_hash: u64,
    pub frequency: u8,
    pub confidence: u8,
    pub predictions_total: u32,
    pub predictions_correct: u32,
}

impl ModelHealth {
    pub const fn new(model_id_hash: u64, property_hash: u64) -> Self {
        Self {
            model_id_hash,
            property_hash,
            frequency: 0,
            confidence: 0,
            predictions_total: 0,
            predictions_correct: 0,
        }
    }

    /// Update health after a prediction is evaluated.
    pub fn record(&mut self, correct: bool) {
        self.predictions_total = self.predictions_total.saturating_add(1);
        if correct {
            self.predictions_correct = self.predictions_correct.saturating_add(1);
        }
        if self.predictions_total > 0 {
            self.frequency =
                ((self.predictions_correct as u64 * 255) / self.predictions_total as u64) as u8;
        }
        self.confidence = match self.predictions_total {
            0..=9 => (self.predictions_total as u8) * 25,
            10..=99 => 250,
            _ => 255,
        };
    }

    pub const fn is_healthy(&self, min_frequency: u8, min_confidence: u8) -> bool {
        self.frequency >= min_frequency && self.confidence >= min_confidence
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Simulation — Foundry Stage 5 (what-if via World::fork())
// ═══════════════════════════════════════════════════════════════════════════

/// Parameters for a what-if simulation. Feeds into World::fork()
/// to create a branched dataset where hypothetical changes are
/// applied, models re-run, and outcomes compared.
#[derive(Clone, Debug)]
pub struct SimulationSpec {
    pub name: &'static str,
    /// Entity type being simulated.
    pub entity_type: &'static str,
    /// Hypothetical property overrides: (predicate, new_value_hash).
    /// The actual values live in the forked dataset; the spec only
    /// names which properties change.
    pub overrides: Vec<(&'static str, u64)>,
    /// Maximum simulation ticks before termination.
    pub max_ticks: u32,
    /// Properties to compare between base and fork.
    pub outcome_properties: &'static [&'static str],
}

impl SimulationSpec {
    pub fn new(name: &'static str, entity_type: &'static str) -> Self {
        Self {
            name,
            entity_type,
            overrides: Vec::new(),
            max_ticks: 100,
            outcome_properties: &[],
        }
    }

    pub fn with_override(mut self, predicate: &'static str, value_hash: u64) -> Self {
        self.overrides.push((predicate, value_hash));
        self
    }

    pub fn with_max_ticks(mut self, ticks: u32) -> Self {
        self.max_ticks = ticks;
        self
    }

    pub fn with_outcomes(mut self, properties: &'static [&'static str]) -> Self {
        self.outcome_properties = properties;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Schema → SPO expansion (Phase 1 of the SQL↔SPO bridge)
// ═══════════════════════════════════════════════════════════════════════════

/// One SPO triple expanded from an Ontology Schema/LinkSpec entry.
/// Zero-dep DTO — the receiving SpoStore implementation hashes labels
/// into Fingerprints and writes the actual triple.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpandedTriple {
    pub subject_label: String,
    pub predicate: &'static str,
    pub object_label: String,
    pub truth: (f32, f32), // (frequency, confidence)
    pub property_kind: PropertyKind,
    pub marking: Marking,
    pub semantic_type: SemanticType,
    pub entity_type_id: EntityTypeId,
}

/// Trait for expanding ontology elements into SPO triples.
pub trait SchemaExpander {
    /// Expand a single entity row's properties into triples.
    /// `properties` is a list of (predicate, value_bytes) pairs.
    fn expand_entity(
        &self,
        entity_type: &str,
        entity_id: u64,
        properties: &[(&str, &[u8])],
    ) -> Vec<ExpandedTriple>;

    /// Expand a typed link between two entities into a single edge triple.
    fn expand_link(&self, link: &LinkSpec, subject_id: u64, object_id: u64) -> ExpandedTriple;
}

impl SchemaExpander for Ontology {
    fn expand_entity(
        &self,
        entity_type: &str,
        entity_id: u64,
        properties: &[(&str, &[u8])],
    ) -> Vec<ExpandedTriple> {
        let etype_id = entity_type_id(self, entity_type);
        let schema = match self.schema(entity_type) {
            Some(s) => s,
            None => return Vec::new(),
        };

        let subject_label = format!("entity:{entity_type}:{entity_id}");

        properties
            .iter()
            .map(|(predicate, value_bytes)| {
                let spec = schema.get(predicate);
                let (kind, marking, semantic_type, truth) = match spec {
                    Some(s) => {
                        // Required: start at nars_floor; Optional/Free: unknown
                        let truth = match s.kind {
                            PropertyKind::Required => s
                                .nars_floor
                                .map(|(f, c)| (f as f32 / 255.0, c as f32 / 255.0))
                                .unwrap_or((1.0, 0.5)),
                            _ => (0.5, 0.01), // unknown
                        };
                        (s.kind, s.marking, s.semantic_type.clone(), truth)
                    }
                    None => (
                        PropertyKind::Free,
                        Marking::Internal,
                        SemanticType::PlainText,
                        (0.5, 0.01),
                    ),
                };

                // Predicate must be &'static — schema's predicates are &'static
                let predicate_static: &'static str = spec.map(|s| s.predicate).unwrap_or("unknown");

                ExpandedTriple {
                    subject_label: subject_label.clone(),
                    predicate: predicate_static,
                    object_label: format!("value:{:016x}", crate::hash::fnv1a(value_bytes)),
                    truth,
                    property_kind: kind,
                    marking,
                    semantic_type,
                    entity_type_id: etype_id,
                }
            })
            .collect()
    }

    fn expand_link(&self, link: &LinkSpec, subject_id: u64, object_id: u64) -> ExpandedTriple {
        let subject_type_id = entity_type_id(self, link.subject_type);
        ExpandedTriple {
            subject_label: format!("entity:{}:{subject_id}", link.subject_type),
            predicate: link.predicate,
            object_label: format!("entity:{}:{object_id}", link.object_type),
            truth: (1.0, 0.9), // links are by-construction true
            property_kind: PropertyKind::Required,
            marking: Marking::Internal,
            semantic_type: SemanticType::PlainText,
            entity_type_id: subject_type_id,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ObjectView + NotificationSpec — Foundry parity primitives (D-PARITY-V2-4)
//
// LF-22/23 surface for Q2 Object Explorer. CONTRACT primitives only — POD
// shapes consumed by the future D-PARITY-V2-7 renderer. No logic here.
//
// **Zone classification**: Zone 1 (BindSpace SoA, inside the BBB).
// MUST NOT carry `serde::Serialize` — matches `MulThresholdProfile` pattern
// (Wave 2). See `.claude/knowledge/soa-dto-dependency-ledger.md`.
// ═══════════════════════════════════════════════════════════════════════════

/// Which Q2 panel template renders an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayTemplate {
    Card,
    Detail,
    Summary,
}

/// One predicate column projected into an `ObjectView`.
/// `predicate_iri` matches the predicate string on `ExpandedTriple`/`MappingRow`;
/// `label` is the display string (English; locale resolution happens in Q2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldRef {
    pub predicate_iri: String,
    pub label: String,
}

impl FieldRef {
    pub fn new(predicate_iri: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            predicate_iri: predicate_iri.into(),
            label: label.into(),
        }
    }
}

/// Foundry "Object View" — a per-Schema render spec for the Object Explorer.
/// `fields` enumerates which `MappingRow` predicates to surface, in order.
/// `primary_label` names the predicate that becomes the row's headline
/// (e.g. `"name"`, `"title"`); `None` falls back to the first field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectView {
    pub display_template: DisplayTemplate,
    pub fields: Vec<FieldRef>,
    pub primary_label: Option<String>,
}

impl ObjectView {
    pub fn new(display_template: DisplayTemplate, fields: Vec<FieldRef>) -> Self {
        Self {
            display_template,
            fields,
            primary_label: None,
        }
    }
}

/// What event fires a notification.
/// `ThresholdCrossed` is the Foundry "metric crossed" trigger; the threshold
/// value lives in the consumer (D-PARITY-V2-7), not in this contract surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationTrigger {
    Created,
    Updated,
    Deleted,
    ThresholdCrossed,
}

/// Where the notification body is delivered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationChannel {
    Inline,
    Webhook,
    Email,
}

/// Foundry "Notification" — one trigger × channel × body template.
/// `template` is a free-form string (e.g. handlebars-style); rendering is
/// the consumer's job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationSpec {
    pub trigger: NotificationTrigger,
    pub channel: NotificationChannel,
    pub template: String,
}

impl NotificationSpec {
    pub fn new(
        trigger: NotificationTrigger,
        channel: NotificationChannel,
        template: impl Into<String>,
    ) -> Self {
        Self {
            trigger,
            channel,
            template: template.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property::Cardinality;

    #[test]
    fn ontology_builder_composes() {
        let customer = Schema::builder("Customer")
            .required("customer_name")
            .required("tax_id")
            .searchable("industry")
            .free("note")
            .build();

        let invoice = Schema::builder("Invoice")
            .required("invoice_number")
            .required("customer_ref")
            .optional("due_date")
            .build();

        let ontology = Ontology::builder("SMB")
            .schema(customer)
            .schema(invoice)
            .link(LinkSpec::one_to_many("Customer", "issued", "Invoice"))
            .action(ActionSpec::manual("approve", "Invoice", "status"))
            .action(ActionSpec::auto("classify", "Customer", "industry"))
            .build();

        assert_eq!(ontology.name, "SMB");
        assert_eq!(ontology.schemas.len(), 2);
        assert_eq!(ontology.links.len(), 1);
        assert_eq!(ontology.actions.len(), 2);
    }

    #[test]
    fn ontology_schema_lookup() {
        let ontology = Ontology::builder("Test")
            .schema(Schema::builder("Customer").required("name").build())
            .build();
        assert!(ontology.schema("Customer").is_some());
        assert!(ontology.schema("Unknown").is_none());
    }

    #[test]
    fn ontology_links_from() {
        let ontology = Ontology::builder("Test")
            .link(LinkSpec::one_to_many("Customer", "issued", "Invoice"))
            .link(LinkSpec::one_to_many("Customer", "filed", "TaxDeclaration"))
            .link(LinkSpec::many_to_many("Invoice", "references", "Invoice"))
            .build();
        assert_eq!(ontology.links_from("Customer").len(), 2);
        assert_eq!(ontology.links_to("Invoice").len(), 2);
    }

    #[test]
    fn ontology_actions_for() {
        let ontology = Ontology::builder("Test")
            .action(ActionSpec::manual("approve", "Invoice", "status"))
            .action(ActionSpec::suggested("flag", "Invoice", "flagged"))
            .action(ActionSpec::auto("classify", "Customer", "industry"))
            .build();
        assert_eq!(ontology.actions_for("Invoice").len(), 2);
        assert_eq!(ontology.actions_for("Customer").len(), 1);
    }

    #[test]
    fn link_spec_cardinality() {
        let link = LinkSpec::one_to_many("Customer", "issued", "Invoice");
        assert_eq!(link.cardinality, Cardinality::OneToMany);
        assert_eq!(link.codec_route, CodecRoute::Passthrough);
    }

    #[test]
    fn model_binding_fields() {
        let binding = ModelBinding::new(
            "industry_classifier",
            "Customer",
            &["customer_name", "description"],
            "industry",
            CodecRoute::CamPq,
        );
        assert_eq!(binding.input_properties.len(), 2);
        assert_eq!(binding.output_property, "industry");
        assert_eq!(binding.output_codec, CodecRoute::CamPq);
    }

    #[test]
    fn model_health_tracking() {
        let mut health = ModelHealth::new(0xABCD, 0x1234);
        assert_eq!(health.frequency, 0);
        assert_eq!(health.confidence, 0);

        health.record(true);
        health.record(true);
        health.record(false);
        // 2/3 correct ≈ 170/255
        assert!(health.frequency > 150);
        assert_eq!(health.predictions_total, 3);
        assert_eq!(health.predictions_correct, 2);
    }

    #[test]
    fn model_health_confidence_ramps() {
        let mut health = ModelHealth::new(0, 0);
        for _ in 0..10 {
            health.record(true);
        }
        assert_eq!(health.confidence, 250); // 10-99 range
        for _ in 0..90 {
            health.record(true);
        }
        assert_eq!(health.confidence, 255); // 100+ range
    }

    #[test]
    fn simulation_spec_builder() {
        let sim = SimulationSpec::new("price_increase", "Invoice")
            .with_override("total_amount", 0xDEAD)
            .with_override("currency", 0xBEEF)
            .with_max_ticks(50)
            .with_outcomes(&["payment_status", "days_to_pay"]);
        assert_eq!(sim.overrides.len(), 2);
        assert_eq!(sim.max_ticks, 50);
        assert_eq!(sim.outcome_properties.len(), 2);
    }

    #[test]
    fn prefetch_depth_ordering() {
        assert!(PrefetchDepth::Identity < PrefetchDepth::Detail);
        assert!(PrefetchDepth::Detail < PrefetchDepth::Similar);
        assert!(PrefetchDepth::Similar < PrefetchDepth::Full);
    }

    #[test]
    fn label_bilingual_display() {
        let l = Label::new("customer", "Customer", "Kunde");
        assert_eq!(l.display(Locale::En), "Customer");
        assert_eq!(l.display(Locale::De), "Kunde");
        assert_eq!(l.key, "customer");
    }

    #[test]
    fn label_en_only_fallback() {
        let l = Label::en_only("invoice");
        assert_eq!(l.display(Locale::De), "invoice");
        assert_eq!(l.display(Locale::En), "invoice");
    }

    #[test]
    fn locale_code() {
        assert_eq!(Locale::En.code(), "en");
        assert_eq!(Locale::De.code(), "de");
    }

    #[test]
    fn ontology_builder_bilingual() {
        let ontology = Ontology::builder("SMB")
            .label(Label::new("smb", "SMB Practice", "Steuerberatungskanzlei"))
            .locale(Locale::De)
            .schema(Schema::builder("Customer").build())
            .build();
        assert_eq!(ontology.label.display(Locale::De), "Steuerberatungskanzlei");
        assert_eq!(ontology.label.display(Locale::En), "SMB Practice");
        assert_eq!(ontology.locale, Locale::De);
    }

    #[test]
    fn entity_type_id_returns_1_based_index() {
        use crate::property::Schema;
        let ont = Ontology {
            name: "test",
            label: Label::en_only("test"),
            locale: Locale::En,
            schemas: vec![
                Schema::builder("Customer").build(),
                Schema::builder("Invoice").build(),
                Schema::builder("Product").build(),
            ],
            links: Vec::new(),
            actions: Vec::new(),
        };
        assert_eq!(entity_type_id(&ont, "Customer"), 1);
        assert_eq!(entity_type_id(&ont, "Invoice"), 2);
        assert_eq!(entity_type_id(&ont, "Product"), 3);
        assert_eq!(entity_type_id(&ont, "Unknown"), 0);
    }

    #[test]
    fn expanded_triple_construction() {
        let t = ExpandedTriple {
            subject_label: "entity:Customer:42".into(),
            predicate: "tax_id",
            object_label: "value:DE123456".into(),
            truth: (1.0, 0.9),
            property_kind: PropertyKind::Required,
            marking: Marking::Financial,
            semantic_type: SemanticType::TaxId,
            entity_type_id: 1,
        };
        assert_eq!(t.predicate, "tax_id");
        assert_eq!(t.entity_type_id, 1);
    }
}
