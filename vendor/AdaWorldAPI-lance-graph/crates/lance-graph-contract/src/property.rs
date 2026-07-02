//! Property classification for AriGraph SPO predicates.
//!
//! Each predicate in the triple store carries a `PropertySpec` that
//! determines: (1) whether absence triggers a `FailureTicket` (Required),
//! (2) how the object value is stored — lossless Index or compressed
//! CAM-PQ Argmax, and (3) the NARS truth floor below which the system
//! escalates.
//!
//! The bardioc Required/Optional/Free concept maps to the I1 Codec
//! Regime Split (ADR-0002): Required = Passthrough (identity must
//! round-trip), Optional = configurable, Free = CamPq (similarity
//! search over schema-free attributes).

use crate::cam::CodecRoute;

/// Classification of an SPO predicate's cardinality and schema obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyKind {
    /// MUST exist for the entity to be valid. Absence triggers
    /// FailureTicket via FreeEnergy escalation. Always Index regime
    /// (lossless, exact match). Examples: tax_id, customer_name, IBAN.
    Required,
    /// MAY exist. Adds value when present but absence does not
    /// escalate. Codec route is configurable per predicate —
    /// address = Index, industry_description = CamPq.
    Optional,
    /// Schema-free. Any predicate name accepted. Default codec
    /// route is CamPq (Argmax) for similarity search across
    /// tenants. User-defined tags, notes, custom fields.
    Free,
}

/// Specification for a single predicate in the AriGraph SPO store.
///
/// Ties the predicate name to its property kind, codec route, and
/// NARS truth floor. The truth floor is the minimum (frequency,
/// confidence) below which the system treats the property as
/// "effectively absent" — for Required properties, this triggers
/// a FailureTicket.
#[derive(Clone, Debug)]
pub struct PropertySpec {
    /// Predicate name in the SPO triple (e.g. "tax_id", "address", "note").
    pub predicate: &'static str,
    /// Required / Optional / Free classification.
    pub kind: PropertyKind,
    /// How the object value is stored/searched. Derived from kind
    /// by default but overridable per predicate.
    pub codec_route: CodecRoute,
    /// Minimum (frequency, confidence) as u8 pair (0..255 each).
    /// Below this floor, Required properties trigger FailureTicket.
    /// None = no floor check (typical for Free properties).
    pub nars_floor: Option<(u8, u8)>,
    /// What kind of value this property holds (LF-21).
    pub semantic_type: SemanticType,
    /// GDPR / data-protection classification (LF-6 marking).
    /// Default = `Marking::Internal` (GDPR-safe baseline).
    /// Override per-predicate via `.with_marking(...)`.
    pub marking: Marking,
}

impl PropertySpec {
    /// Create a Required property spec. Default codec: Passthrough (Index).
    /// Default NARS floor: (128, 128) — moderate confidence required.
    /// Default marking: `Marking::Internal` (GDPR-safe).
    pub const fn required(predicate: &'static str) -> Self {
        Self {
            predicate,
            kind: PropertyKind::Required,
            codec_route: CodecRoute::Passthrough,
            nars_floor: Some((128, 128)),
            semantic_type: SemanticType::PlainText,
            marking: Marking::Internal,
        }
    }

    /// Create an Optional property spec. Caller must specify codec route.
    /// No NARS floor by default (absence doesn't escalate).
    /// Default marking: `Marking::Internal` (GDPR-safe).
    pub const fn optional(predicate: &'static str, codec_route: CodecRoute) -> Self {
        Self {
            predicate,
            kind: PropertyKind::Optional,
            codec_route,
            nars_floor: None,
            semantic_type: SemanticType::PlainText,
            marking: Marking::Internal,
        }
    }

    /// Create a Free property spec. Default codec: CamPq (Argmax).
    /// No NARS floor (schema-free, always accepted).
    /// Default marking: `Marking::Internal` (GDPR-safe).
    pub const fn free(predicate: &'static str) -> Self {
        Self {
            predicate,
            kind: PropertyKind::Free,
            codec_route: CodecRoute::CamPq,
            nars_floor: None,
            semantic_type: SemanticType::PlainText,
            marking: Marking::Internal,
        }
    }

    pub const fn with_semantic_type(mut self, st: SemanticType) -> Self {
        self.semantic_type = st;
        self
    }

    /// Override the GDPR / data-protection marking for this predicate (LF-6).
    /// Default is `Marking::Internal`. SMB customer schema overrides:
    /// `iban` → Financial, `geburtsdatum` → Pii, etc.
    pub const fn with_marking(mut self, marking: Marking) -> Self {
        self.marking = marking;
        self
    }

    /// Override the NARS truth floor.
    pub const fn with_nars_floor(mut self, frequency: u8, confidence: u8) -> Self {
        self.nars_floor = Some((frequency, confidence));
        self
    }

    /// Override the codec route.
    pub const fn with_codec_route(mut self, route: CodecRoute) -> Self {
        self.codec_route = route;
        self
    }

    /// Check whether a given (frequency, confidence) pair is below this
    /// property's truth floor. Returns true if escalation is warranted.
    pub const fn below_floor(&self, frequency: u8, confidence: u8) -> bool {
        match self.nars_floor {
            Some((min_f, min_c)) => frequency < min_f || confidence < min_c,
            None => false,
        }
    }
}

/// A property schema — a collection of PropertySpecs for a given entity type.
/// Used by AriGraph to validate triples on insert and to route codec
/// decisions per predicate.
#[derive(Clone, Debug)]
pub struct PropertySchema {
    /// Entity type name (e.g. "Customer", "Invoice", "TaxDeclaration").
    pub entity_type: &'static str,
    /// Ordered list of property specs. Required properties come first
    /// by convention (not enforced).
    pub properties: &'static [PropertySpec],
}

impl PropertySchema {
    /// Look up a property spec by predicate name.
    pub fn get(&self, predicate: &str) -> Option<&PropertySpec> {
        self.properties.iter().find(|p| p.predicate == predicate)
    }

    /// Return all Required properties.
    pub fn required(&self) -> impl Iterator<Item = &PropertySpec> {
        self.properties
            .iter()
            .filter(|p| p.kind == PropertyKind::Required)
    }

    /// Return all predicates that are missing from a given set of
    /// predicate names. Only checks Required properties.
    /// Returns predicate names that should trigger FailureTicket.
    pub fn missing_required<'a>(
        &'a self,
        present: &'a [&str],
    ) -> impl Iterator<Item = &'static str> + 'a {
        self.required()
            .filter(move |p| !present.contains(&p.predicate))
            .map(|p| p.predicate)
    }

    /// Determine the codec route for a predicate. If the predicate is
    /// not in the schema, it's treated as Free (CamPq).
    pub fn codec_route_for(&self, predicate: &str) -> CodecRoute {
        self.get(predicate)
            .map(|p| p.codec_route)
            .unwrap_or(CodecRoute::CamPq)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Schema builder — declarative API for SMB tenants
// ═══════════════════════════════════════════════════════════════════════════

/// Owned property schema built at runtime via the builder API.
/// Complement to `PropertySchema` (which is `&'static`-only for const schemas).
#[derive(Clone, Debug)]
pub struct Schema {
    pub name: &'static str,
    pub properties: Vec<PropertySpec>,
    pub view: Option<ObjectView>,
}

impl Schema {
    pub fn builder(name: &'static str) -> SchemaBuilder {
        SchemaBuilder {
            name,
            properties: Vec::new(),
            view: None,
        }
    }

    pub fn get(&self, predicate: &str) -> Option<&PropertySpec> {
        self.properties.iter().find(|p| p.predicate == predicate)
    }

    pub fn required_props(&self) -> impl Iterator<Item = &PropertySpec> {
        self.properties
            .iter()
            .filter(|p| p.kind == PropertyKind::Required)
    }

    pub fn missing_required<'a>(
        &'a self,
        present: &'a [&str],
    ) -> impl Iterator<Item = &'static str> + 'a {
        self.required_props()
            .filter(move |p| !present.contains(&p.predicate))
            .map(|p| p.predicate)
    }

    pub fn codec_route_for(&self, predicate: &str) -> CodecRoute {
        self.get(predicate)
            .map(|p| p.codec_route)
            .unwrap_or(CodecRoute::CamPq)
    }

    /// Validate a set of present predicates. Returns a list of missing
    /// Required predicate names. Empty = valid.
    pub fn validate(&self, present: &[&str]) -> Vec<&'static str> {
        self.missing_required(present).collect()
    }
}

pub struct SchemaBuilder {
    name: &'static str,
    properties: Vec<PropertySpec>,
    view: Option<ObjectView>,
}

impl SchemaBuilder {
    /// Add a Required property (Passthrough codec, NARS floor 128/128).
    pub fn required(mut self, predicate: &'static str) -> Self {
        self.properties.push(PropertySpec::required(predicate));
        self
    }

    /// Add an Optional property with Passthrough (exact match) codec.
    pub fn optional(mut self, predicate: &'static str) -> Self {
        self.properties
            .push(PropertySpec::optional(predicate, CodecRoute::Passthrough));
        self
    }

    /// Add an Optional property with CamPq (similarity search) codec.
    pub fn searchable(mut self, predicate: &'static str) -> Self {
        self.properties
            .push(PropertySpec::optional(predicate, CodecRoute::CamPq));
        self
    }

    /// Add a Free property (CamPq codec, no NARS floor).
    pub fn free(mut self, predicate: &'static str) -> Self {
        self.properties.push(PropertySpec::free(predicate));
        self
    }

    /// Add a custom PropertySpec directly.
    pub fn property(mut self, spec: PropertySpec) -> Self {
        self.properties.push(spec);
        self
    }

    /// Attach an ObjectView for outside-BBB rendering (LF-22).
    pub fn view(mut self, view: ObjectView) -> Self {
        self.view = Some(view);
        self
    }

    pub fn build(self) -> Schema {
        Schema {
            name: self.name,
            properties: self.properties,
            view: self.view,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Link types — typed edges between ontology objects (Foundry Stage 1)
// ═══════════════════════════════════════════════════════════════════════════

/// Typed edge between two ontology object types. An SPO triple
/// `(Customer:123, issued, Invoice:456)` is governed by a LinkSpec
/// that constrains subject_type, predicate, and object_type.
#[derive(Clone, Debug)]
pub struct LinkSpec {
    pub subject_type: &'static str,
    pub predicate: &'static str,
    pub object_type: &'static str,
    pub cardinality: Cardinality,
    pub codec_route: CodecRoute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToMany,
}

impl LinkSpec {
    pub const fn one_to_many(
        subject_type: &'static str,
        predicate: &'static str,
        object_type: &'static str,
    ) -> Self {
        Self {
            subject_type,
            predicate,
            object_type,
            cardinality: Cardinality::OneToMany,
            codec_route: CodecRoute::Passthrough,
        }
    }

    pub const fn many_to_many(
        subject_type: &'static str,
        predicate: &'static str,
        object_type: &'static str,
    ) -> Self {
        Self {
            subject_type,
            predicate,
            object_type,
            cardinality: Cardinality::ManyToMany,
            codec_route: CodecRoute::Passthrough,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Prefetch depth — Object Explorer property loading tiers (Foundry Stage 5)
// ═══════════════════════════════════════════════════════════════════════════

/// Graph prefetch depth for progressive property loading.
/// Maps to PropertyKind + CodecRoute: the ontology metadata
/// determines what loads at each scroll/expansion level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrefetchDepth {
    /// Node visible — Required properties only (identity).
    /// All Passthrough (Index regime), instant lookup.
    Identity = 0,
    /// Node selected — + Optional/Passthrough (exact-match fields).
    Detail = 1,
    /// Node expanded — + Optional/CamPq (similarity-searchable).
    /// CAM-PQ distance queries fire at this level.
    Similar = 2,
    /// Node deep-dived — + Free properties + episodic memory.
    /// Full CamPq sweep + Markov ±5 temporal window.
    Full = 3,
}

impl Schema {
    /// Return properties visible at a given prefetch depth.
    pub fn properties_at_depth(&self, depth: PrefetchDepth) -> Vec<&PropertySpec> {
        self.properties
            .iter()
            .filter(|p| match depth {
                PrefetchDepth::Identity => p.kind == PropertyKind::Required,
                PrefetchDepth::Detail => {
                    p.kind == PropertyKind::Required
                        || (p.kind == PropertyKind::Optional
                            && p.codec_route == CodecRoute::Passthrough)
                }
                PrefetchDepth::Similar => {
                    p.kind == PropertyKind::Required || p.kind == PropertyKind::Optional
                }
                PrefetchDepth::Full => true,
            })
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Action specs — Application Builder actions on objects (Foundry Stage 5)
// ═══════════════════════════════════════════════════════════════════════════

/// An action that can be taken on an ontology object. Maps a user
/// gesture (approve invoice, flag customer, submit declaration) to
/// a predicate change routed through OrchestrationBridge.
///
/// In active-inference terms: an Action IS a Commit with side effects.
/// The action fires when FreeEnergy drops below threshold (auto) or
/// when a human explicitly triggers it (manual).
#[derive(Clone, Debug)]
pub struct ActionSpec {
    pub name: &'static str,
    pub entity_type: &'static str,
    /// The predicate this action modifies (e.g. "status", "approved_by").
    pub target_predicate: &'static str,
    pub trigger: ActionTrigger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionTrigger {
    /// User must explicitly trigger (button click, approval).
    Manual,
    /// System triggers when FreeEnergy < threshold (auto-commit).
    Auto,
    /// System suggests, user confirms (semi-auto).
    Suggested,
}

impl ActionSpec {
    pub const fn manual(
        name: &'static str,
        entity_type: &'static str,
        target: &'static str,
    ) -> Self {
        Self {
            name,
            entity_type,
            target_predicate: target,
            trigger: ActionTrigger::Manual,
        }
    }

    pub const fn auto(name: &'static str, entity_type: &'static str, target: &'static str) -> Self {
        Self {
            name,
            entity_type,
            target_predicate: target,
            trigger: ActionTrigger::Auto,
        }
    }

    pub const fn suggested(
        name: &'static str,
        entity_type: &'static str,
        target: &'static str,
    ) -> Self {
        Self {
            name,
            entity_type,
            target_predicate: target,
            trigger: ActionTrigger::Suggested,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Example schemas — SMB domain (const)
// ═══════════════════════════════════════════════════════════════════════════

/// Customer entity property schema.
pub const CUSTOMER_SCHEMA: PropertySchema = PropertySchema {
    entity_type: "Customer",
    properties: &[
        // Required — identity, lossless
        PropertySpec::required("customer_name"),
        PropertySpec::required("tax_id"),
        // Optional — exact match
        PropertySpec::optional("address", CodecRoute::Passthrough),
        PropertySpec::optional("iban", CodecRoute::Passthrough),
        PropertySpec::optional("phone", CodecRoute::Passthrough),
        PropertySpec::optional("email", CodecRoute::Passthrough),
        // Optional — similarity search
        PropertySpec::optional("industry", CodecRoute::CamPq),
        PropertySpec::optional("description", CodecRoute::CamPq),
        // Free — anything goes, similarity indexed
        PropertySpec::free("tag"),
        PropertySpec::free("note"),
    ],
};

/// Invoice entity property schema.
pub const INVOICE_SCHEMA: PropertySchema = PropertySchema {
    entity_type: "Invoice",
    properties: &[
        PropertySpec::required("invoice_number"),
        PropertySpec::required("date"),
        PropertySpec::required("total_amount"),
        PropertySpec::required("currency"),
        PropertySpec::required("customer_ref"),
        PropertySpec::optional("due_date", CodecRoute::Passthrough),
        PropertySpec::optional("payment_terms", CodecRoute::Passthrough),
        PropertySpec::optional("line_items_hash", CodecRoute::Passthrough),
        PropertySpec::free("note"),
        PropertySpec::free("tag"),
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_defaults() {
        let p = PropertySpec::required("tax_id");
        assert_eq!(p.kind, PropertyKind::Required);
        assert_eq!(p.codec_route, CodecRoute::Passthrough);
        assert!(p.nars_floor.is_some());
    }

    #[test]
    fn optional_inherits_codec() {
        let p = PropertySpec::optional("industry", CodecRoute::CamPq);
        assert_eq!(p.kind, PropertyKind::Optional);
        assert_eq!(p.codec_route, CodecRoute::CamPq);
        assert!(p.nars_floor.is_none());
    }

    #[test]
    fn free_defaults_to_campq() {
        let p = PropertySpec::free("note");
        assert_eq!(p.kind, PropertyKind::Free);
        assert_eq!(p.codec_route, CodecRoute::CamPq);
        assert!(p.nars_floor.is_none());
    }

    /// LF-6: every PropertySpec defaults to `Marking::Internal` (GDPR-safe).
    #[test]
    fn property_spec_marking_defaults_to_internal() {
        assert_eq!(PropertySpec::required("kdnr").marking, Marking::Internal);
        assert_eq!(
            PropertySpec::optional("note", CodecRoute::CamPq).marking,
            Marking::Internal
        );
        assert_eq!(PropertySpec::free("free").marking, Marking::Internal);
    }

    /// SMB schema marking pattern: chain `with_marking` per predicate.
    #[test]
    fn property_spec_with_marking_overrides() {
        let iban = PropertySpec::required("iban").with_marking(Marking::Financial);
        let dob = PropertySpec::required("geburtsdatum").with_marking(Marking::Pii);
        let note = PropertySpec::free("note"); // stays Internal

        assert_eq!(iban.marking, Marking::Financial);
        assert_eq!(dob.marking, Marking::Pii);
        assert_eq!(note.marking, Marking::Internal);

        // Per-row fold (W-2): `most_restrictive` over a row's markings.
        let row_markings = [iban.marking, dob.marking, note.marking];
        assert_eq!(Marking::most_restrictive(&row_markings), Marking::Financial);
    }

    /// `with_marking` is const and chains with `with_semantic_type` (LF-21).
    #[test]
    fn property_spec_with_marking_chains_with_semantic_type() {
        const SPEC: PropertySpec = PropertySpec::required("iban")
            .with_semantic_type(SemanticType::Iban)
            .with_marking(Marking::Financial);
        assert_eq!(SPEC.predicate, "iban");
        assert_eq!(SPEC.semantic_type, SemanticType::Iban);
        assert_eq!(SPEC.marking, Marking::Financial);
    }

    #[test]
    fn below_floor_required() {
        let p = PropertySpec::required("tax_id");
        // Default floor is (128, 128)
        assert!(p.below_floor(100, 200)); // frequency too low
        assert!(p.below_floor(200, 100)); // confidence too low
        assert!(!p.below_floor(200, 200)); // both above
    }

    #[test]
    fn below_floor_free_always_false() {
        let p = PropertySpec::free("note");
        assert!(!p.below_floor(0, 0)); // no floor = never below
    }

    #[test]
    fn schema_missing_required() {
        let present = ["customer_name", "address", "tag"];
        let missing: Vec<_> = CUSTOMER_SCHEMA.missing_required(&present).collect();
        assert!(missing.contains(&"tax_id"));
        assert!(!missing.contains(&"customer_name"));
    }

    #[test]
    fn schema_codec_route_known_predicate() {
        assert_eq!(
            CUSTOMER_SCHEMA.codec_route_for("tax_id"),
            CodecRoute::Passthrough
        );
        assert_eq!(
            CUSTOMER_SCHEMA.codec_route_for("industry"),
            CodecRoute::CamPq
        );
    }

    #[test]
    fn schema_codec_route_unknown_predicate_defaults_to_campq() {
        assert_eq!(
            CUSTOMER_SCHEMA.codec_route_for("unknown_field"),
            CodecRoute::CamPq
        );
    }

    #[test]
    fn invoice_schema_has_five_required() {
        let count = INVOICE_SCHEMA.required().count();
        assert_eq!(count, 5);
    }

    #[test]
    fn with_nars_floor_override() {
        let p = PropertySpec::free("note").with_nars_floor(50, 50);
        assert!(p.below_floor(40, 60));
        assert!(!p.below_floor(60, 60));
    }

    // ── Schema builder tests ──

    #[test]
    fn schema_builder_declarative() {
        let s = Schema::builder("Customer")
            .required("customer_name")
            .required("tax_id")
            .optional("address")
            .searchable("industry")
            .free("note")
            .build();
        assert_eq!(s.name, "Customer");
        assert_eq!(s.properties.len(), 5);
    }

    #[test]
    fn schema_validate_missing_required() {
        let s = Schema::builder("Customer")
            .required("customer_name")
            .required("tax_id")
            .optional("address")
            .build();
        let missing = s.validate(&["customer_name", "address"]);
        assert_eq!(missing, vec!["tax_id"]);
    }

    #[test]
    fn schema_validate_all_present() {
        let s = Schema::builder("Customer")
            .required("customer_name")
            .required("tax_id")
            .build();
        let missing = s.validate(&["customer_name", "tax_id"]);
        assert!(missing.is_empty());
    }

    #[test]
    fn schema_searchable_is_campq() {
        let s = Schema::builder("Test").searchable("description").build();
        assert_eq!(s.codec_route_for("description"), CodecRoute::CamPq);
    }

    #[test]
    fn schema_unknown_predicate_defaults_campq() {
        let s = Schema::builder("Test").build();
        assert_eq!(s.codec_route_for("anything"), CodecRoute::CamPq);
    }

    #[test]
    fn schema_optional_is_passthrough() {
        let s = Schema::builder("Test").optional("address").build();
        assert_eq!(s.codec_route_for("address"), CodecRoute::Passthrough);
    }

    // ── Prefetch depth tests ──

    #[test]
    fn prefetch_identity_only_required() {
        let s = Schema::builder("Customer")
            .required("name")
            .required("tax_id")
            .optional("address")
            .searchable("industry")
            .free("note")
            .build();
        let props = s.properties_at_depth(PrefetchDepth::Identity);
        assert_eq!(props.len(), 2);
        assert!(props.iter().all(|p| p.kind == PropertyKind::Required));
    }

    #[test]
    fn prefetch_detail_adds_optional_passthrough() {
        let s = Schema::builder("Customer")
            .required("name")
            .optional("address")
            .searchable("industry")
            .free("note")
            .build();
        let props = s.properties_at_depth(PrefetchDepth::Detail);
        assert_eq!(props.len(), 2); // name + address
    }

    #[test]
    fn prefetch_similar_adds_campq_optional() {
        let s = Schema::builder("Customer")
            .required("name")
            .optional("address")
            .searchable("industry")
            .free("note")
            .build();
        let props = s.properties_at_depth(PrefetchDepth::Similar);
        assert_eq!(props.len(), 3); // name + address + industry
    }

    #[test]
    fn prefetch_full_includes_everything() {
        let s = Schema::builder("Customer")
            .required("name")
            .optional("address")
            .searchable("industry")
            .free("note")
            .build();
        let props = s.properties_at_depth(PrefetchDepth::Full);
        assert_eq!(props.len(), 4);
    }

    // ── Link spec tests ──

    #[test]
    fn link_one_to_many_defaults() {
        let link = LinkSpec::one_to_many("Customer", "issued", "Invoice");
        assert_eq!(link.subject_type, "Customer");
        assert_eq!(link.object_type, "Invoice");
        assert_eq!(link.cardinality, Cardinality::OneToMany);
        assert_eq!(link.codec_route, CodecRoute::Passthrough);
    }

    #[test]
    fn link_many_to_many() {
        let link = LinkSpec::many_to_many("Tag", "applied_to", "Customer");
        assert_eq!(link.cardinality, Cardinality::ManyToMany);
    }

    // ── Action spec tests ──

    #[test]
    fn action_manual() {
        let a = ActionSpec::manual("approve", "Invoice", "status");
        assert_eq!(a.trigger, ActionTrigger::Manual);
        assert_eq!(a.target_predicate, "status");
    }

    #[test]
    fn action_auto() {
        let a = ActionSpec::auto("classify", "Customer", "industry");
        assert_eq!(a.trigger, ActionTrigger::Auto);
    }

    #[test]
    fn action_suggested() {
        let a = ActionSpec::suggested("flag", "Invoice", "flagged");
        assert_eq!(a.trigger, ActionTrigger::Suggested);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MARKING (GDPR data classification)
// ═══════════════════════════════════════════════════════════════════════════

/// Data classification marking for GDPR compliance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Marking {
    /// No restriction; safe to expose externally.
    Public,
    /// Default — internal use only, not for general sharing.
    #[default]
    Internal,
    /// Personally Identifiable Information; GDPR-protected.
    Pii,
    /// Financial data; bookkeeping or tax-relevant.
    Financial,
    /// Highest restriction; access requires explicit grant.
    Restricted,
}

// ═══════════════════════════════════════════════════════════════════════════
// SEMANTIC TYPE (LF-21 — SMB REQUEST)
// ═══════════════════════════════════════════════════════════════════════════

/// Semantic type annotation on a property. Tells the outside-BBB surface
/// what kind of value this property holds, enabling format-aware validation,
/// display formatting, and search indexing without inspecting the raw bytes.
///
/// SMB use cases: `iban` (DE89370400440532013000), `currency` (EUR 1234.56),
/// `email`, `phone`, `date` (geburtsdatum), `address`, `tax_id` (umsatzsteuer-id).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum SemanticType {
    /// Default — opaque text, no semantic interpretation.
    #[default]
    PlainText,
    /// International Bank Account Number (IBAN).
    Iban,
    /// Currency value with ISO 4217 currency code (e.g., "EUR", "USD").
    Currency(&'static str),
    /// Email address.
    Email,
    /// Phone number.
    Phone,
    /// Date with explicit precision.
    Date(DatePrecision),
    /// Geographic coordinate in the named format.
    Geo(GeoFormat),
    /// Postal address (free-form).
    Address,
    /// File reference with MIME type.
    File(&'static str),
    /// Image reference (any format).
    Image,
    /// HTTP/HTTPS URL.
    Url,
    /// Tax identification number (e.g., German USt-ID).
    TaxId,
    /// Customer identifier (per-tenant).
    CustomerId,
    /// Invoice number (per-tenant).
    InvoiceNumber,
}

/// Date granularity for `SemanticType::Date`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DatePrecision {
    Day,
    Month,
    Year,
    DateTime,
}

/// Geo coordinate format for `SemanticType::Geo`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GeoFormat {
    LatLon,
    Wgs84,
    PlusCode,
}

// ═══════════════════════════════════════════════════════════════════════════
// OBJECT VIEW (LF-22 — SMB REQUEST)
// ═══════════════════════════════════════════════════════════════════════════

/// Rendering descriptor for entity views outside the BBB.
/// Tells a UI which properties to show at each zoom level
/// without bespoke per-entity-type rendering code.
#[derive(Clone, Debug)]
pub struct ObjectView {
    pub card: &'static [&'static str],
    pub detail: &'static [&'static str],
    pub summary_template: &'static str,
}

impl ObjectView {
    pub const fn new(
        card: &'static [&'static str],
        detail: &'static [&'static str],
        summary_template: &'static str,
    ) -> Self {
        Self {
            card,
            detail,
            summary_template,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDIT ENTRY (LF-90 — SMB REQUEST)
// ═══════════════════════════════════════════════════════════════════════════

/// Append-only audit trail entry. Outside the BBB this is a compliance
/// record; inside it feeds CausalEdge64 provenance bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEntry {
    pub actor: u64,
    pub action_id: u64,
    pub action_kind: AuditAction,
    pub timestamp_ms: u64,
    pub predicate_target: &'static str,
    pub signature: [u8; 64],
}

/// What kind of mutation the audit entry records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuditAction {
    Create,
    Update,
    Delete,
    Read,
    Export,
    Import,
    Approve,
    Reject,
}

/// Trait for an append-only audit log. Implementations back this with
/// Lance versioned dataset, Arrow Flight, or any persistent store.
pub trait AuditLog: Send + Sync {
    type Error: Send + 'static;

    fn append(&self, entry: AuditEntry) -> Result<(), Self::Error>;

    fn entries_for_entity(
        &self,
        entity_type: &str,
        entity_id: u64,
    ) -> Result<Vec<AuditEntry>, Self::Error>;

    fn entries_by_actor(&self, actor: u64, since_ms: u64) -> Result<Vec<AuditEntry>, Self::Error>;
}

impl Marking {
    pub fn most_restrictive(markings: &[Marking]) -> Marking {
        markings.iter().copied().max().unwrap_or(Marking::Public)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LINEAGE HANDLE
// ═══════════════════════════════════════════════════════════════════════════

/// Opaque handle to an entity's lineage chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineageHandle {
    pub entity_type: &'static str,
    pub entity_id: u64,
    pub version: u64,
    pub source_system: &'static str,
    pub timestamp_ms: u64,
}

impl LineageHandle {
    pub const fn new(
        entity_type: &'static str,
        entity_id: u64,
        version: u64,
        source_system: &'static str,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            entity_type,
            entity_id,
            version,
            source_system,
            timestamp_ms,
        }
    }

    /// Merge two handles. Takes higher version, newer source_system, max timestamp.
    pub fn merge(self, other: Self) -> Self {
        debug_assert_eq!(self.entity_type, other.entity_type);
        debug_assert_eq!(self.entity_id, other.entity_id);
        let (newer, older) = if self.version >= other.version {
            (self, other)
        } else {
            (other, self)
        };
        Self {
            entity_type: newer.entity_type,
            entity_id: newer.entity_id,
            version: newer.version,
            source_system: newer.source_system,
            timestamp_ms: newer.timestamp_ms.max(older.timestamp_ms),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ENTITY STORE + WRITER TRAITS
// ═══════════════════════════════════════════════════════════════════════════

/// Streaming-capable entity scan API for tables exceeding ~50K rows.
pub trait EntityStore: Send + Sync {
    type RowBatch: Send;
    type Error: Send + 'static;
    type ScanStream: Iterator<Item = Result<Self::RowBatch, Self::Error>> + Send;

    fn scan_stream(&self, entity_type: &str) -> Result<Self::ScanStream, Self::Error>;
}

/// Writer trait with provenance tracking via LineageHandle.
pub trait EntityWriter: Send + Sync {
    type Error: Send + 'static;
    type Row: Send;

    fn upsert_with_lineage(
        &self,
        entity_type: &'static str,
        entity_id: u64,
        row: Self::Row,
        source_system: &'static str,
    ) -> Result<LineageHandle, Self::Error>;
}

// ═══════════════════════════════════════════════════════════════════════════
// MOCK STORE (test-only template)
// ═══════════════════════════════════════════════════════════════════════════

/// In-memory test store implementing EntityStore + EntityWriter.
pub mod mock_store {
    use super::*;
    use std::sync::RwLock;

    /// In-memory test store: tuple of (entity_id, encoded_row_bytes).
    pub struct VecStore {
        /// Stored rows.
        pub rows: RwLock<Vec<(u64, Vec<u8>)>>,
        version_counter: RwLock<u64>,
    }

    impl Default for VecStore {
        fn default() -> Self {
            Self::new()
        }
    }

    impl VecStore {
        /// Create an empty test store.
        pub fn new() -> Self {
            Self {
                rows: RwLock::new(Vec::new()),
                version_counter: RwLock::new(0),
            }
        }
    }

    impl EntityStore for VecStore {
        type RowBatch = Vec<(u64, Vec<u8>)>;
        type Error = &'static str;
        type ScanStream = std::vec::IntoIter<Result<Self::RowBatch, Self::Error>>;

        fn scan_stream(&self, _entity_type: &str) -> Result<Self::ScanStream, Self::Error> {
            let batch = self.rows.read().map_err(|_| "lock poisoned")?.clone();
            Ok(vec![Ok(batch)].into_iter())
        }
    }

    impl EntityWriter for VecStore {
        type Error = &'static str;
        type Row = Vec<u8>;

        fn upsert_with_lineage(
            &self,
            entity_type: &'static str,
            entity_id: u64,
            row: Self::Row,
            source_system: &'static str,
        ) -> Result<LineageHandle, Self::Error> {
            let mut ver = self.version_counter.write().map_err(|_| "lock poisoned")?;
            *ver += 1;
            let version = *ver;
            self.rows
                .write()
                .map_err(|_| "lock poisoned")?
                .push((entity_id, row));
            Ok(LineageHandle::new(
                entity_type,
                entity_id,
                version,
                source_system,
                0,
            ))
        }
    }
}

#[cfg(test)]
mod smb_tests {
    use super::*;

    #[test]
    fn marking_most_restrictive() {
        assert_eq!(Marking::most_restrictive(&[]), Marking::Public);
        assert_eq!(
            Marking::most_restrictive(&[Marking::Internal, Marking::Pii]),
            Marking::Pii
        );
        assert_eq!(
            Marking::most_restrictive(&[Marking::Restricted, Marking::Public]),
            Marking::Restricted
        );
    }

    #[test]
    fn lineage_merge_takes_higher_version() {
        let a = LineageHandle::new("Customer", 1, 3, "mongo", 100);
        let b = LineageHandle::new("Customer", 1, 5, "imap", 50);
        let merged = a.merge(b);
        assert_eq!(merged.version, 5);
        assert_eq!(merged.source_system, "imap");
        assert_eq!(merged.timestamp_ms, 100);
    }

    #[test]
    fn vec_store_upsert_and_scan() {
        use mock_store::VecStore;
        let store = VecStore::new();
        let handle = store
            .upsert_with_lineage("Customer", 42, vec![1, 2, 3], "test")
            .unwrap();
        assert_eq!(handle.entity_id, 42);
        assert_eq!(handle.version, 1);
        let mut stream = store.scan_stream("Customer").unwrap();
        let batch = stream.next().unwrap().unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].0, 42);
    }
}
