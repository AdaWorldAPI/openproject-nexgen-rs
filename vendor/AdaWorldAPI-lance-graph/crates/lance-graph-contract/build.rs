// build.rs — lance-graph-contract
// Reads modules/*/manifest.yaml from the workspace root and
// emits two files into OUT_DIR:
//   ogit_namespace.rs   — pub mod OGIT { pub const *_V*: (u32,u32) }
//                         pub const ALL_G_SLOTS: &[u32]
//   manifest_metadata.rs — pub static MANIFEST_METADATA: &[ManifestEntry]
//                          sorted ascending by g_slot (binary-search safe)
//
// Zero runtime deps added — serde_yaml is a build-dep only.
// The emitted ManifestEntry type is defined in src/manifest.rs.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Canonical slot table (build-time only)
// ---------------------------------------------------------------------------

const CANONICAL_SLOTS: &[(&str, u32)] = &[
    ("DOLCE", 0),
    ("MED", 1),
    ("HEALTHCARE", 2),
    ("GOTHAM", 3),
    ("SMB", 4),
    ("FMA", 5),
    ("CRM", 6),
    // L2 universal upper-bridge ontologies (PR-bO-2 .. bO-5/bO-8).
    // Each declares `inherits_from: dolce` in its manifest.
    ("TIME", 10),
    ("PROVO", 11),
    ("QUDT", 12),
    ("SCHEMAORG", 13),
    ("SKOS", 14),
    // L3 finance / business ontologies (PR-bO-6 / bO-7).
    // FIBO Foundations and FIBO Business Entities.
    ("FIBOFND", 20),
    ("FIBOBE", 21),
    // L3 e-invoicing schemas (PR-bO-16). ZUGFeRD/Factur-X is the
    // German hybrid PDF/A-3+XML invoice format aligned with EN 16931.
    // Hydrated as IRI-interning over UN/CEFACT CII XSDs via XsdHydrator.
    ("ZUGFERD", 30),
    // L3 e-invoicing business rules (PR-bO-15). Schematron assertion /
    // report IDs from the ZUGFeRD validator config, plus the bracketed
    // EN16931 / PEPPOL / CO / DE business-rule IDs from the message
    // bodies. Hydrated via SchematronHydrator.
    ("ZUGFERDRULES", 31),
    // L3 German chart of accounts (PR-bO-13). DATEV SKR is the de-facto
    // canonical bookkeeping scheme for HGB-compliant German SMEs.
    // SKR 03 uses process-oriented family numbering; SKR 04 uses
    // balance-sheet-oriented. Each is hydrated as a separate G slot
    // because account numbers DO NOT mean the same thing across the two
    // schemes (e.g. account 1000 is "Roh-, Hilfs- und Betriebsstoffe"
    // in SKR 04 but "Kasse" in SKR 03).
    ("SKR03", 40),
    ("SKR04", 41),
    // SKR 03 Bau und Handwerk (Branchenpaket 19606) — trade-specific
    // 6-digit extensions on top of canonical SKR 03 (Sand- und
    // Kiesausbeute, Bauliche Anlagen, etc.). Hydrates into its OWN G
    // slot rather than the canonical SKR03_V1 slot so mixed consumers
    // can hold both account sets in one OntologyRegistry.
    ("SKR03BAU", 42),
    // L1 odoo extraction source (four-way alignment seam). Odoo-extracted
    // business models (res.partner, account.move, product.template, …) as
    // OWL classes interned via OwlHydrator. Declares `inherits_from: fibofnd`
    // and reaches the financial ontology via the `owl:equivalentClass`
    // alignment axioms in data/ontologies/odoo/alignment/ (Seam decision 1 /
    // Option B: odoo inherits existing FIBO/SKR slots, it does NOT get its
    // own CAM codebook family).
    ("ODOO", 50),
];

fn canonical_slot(token: &str) -> Option<u32> {
    CANONICAL_SLOTS
        .iter()
        .find(|(t, _)| *t == token)
        .map(|(_, s)| *s)
}

// ---------------------------------------------------------------------------
// Serde types — manifest deserialization
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestRaw {
    ogit_g: String,
    version: u32,
    domain_name: String,
    inert_when_consumer_absent: bool,
    /// Map from entity name to "u16=NNN" string.
    entity_types: BTreeMap<String, String>,
    rbac_policy: Option<String>,
    stack_profile: Option<StackProfileRaw>,
    #[allow(dead_code)] // reserved for future capability extraction
    action_capabilities: BTreeMap<String, String>,
    actor: Option<ActorRaw>,
    inherits_from: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StackProfileRaw {
    audit_retention_days: Option<u32>,
    requires_fail_closed: Option<bool>,
    escalation: Option<String>,
    // Soft-accept unknown sub-keys via flattening.
    #[serde(flatten)]
    _extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorRaw {
    #[serde(rename = "crate")]
    crate_name: String,
    #[serde(rename = "type")]
    type_name: String,
    #[allow(dead_code)] // reserved for future actor-message-type codegen
    message_type: String,
}

// ---------------------------------------------------------------------------
// Internal representation after validation
// ---------------------------------------------------------------------------

struct Manifest {
    g_slot: u32,
    version: u32,
    domain_name: String,
    inert: bool,
    rbac_policy: Option<String>,
    audit_days: u32,
    fail_closed: bool,
    escalation: String, // "Llm" | "Human" | "Deny"
    actor_crate: Option<String>,
    actor_type: Option<String>,
    entity_count: usize,
}

// ---------------------------------------------------------------------------
// Entity-code parsing
// ---------------------------------------------------------------------------

fn parse_entity_code(s: &str) -> Result<u16, String> {
    let stripped = s
        .strip_prefix("u16=")
        .ok_or_else(|| format!("entity_type code must be 'u16=NNN', got '{s}'"))?;
    stripped
        .parse::<u16>()
        .map_err(|e| format!("entity_type code '{s}': {e}"))
}

// ---------------------------------------------------------------------------
// Escalation parsing
// ---------------------------------------------------------------------------

fn parse_escalation(s: &str) -> Result<&'static str, String> {
    match s {
        "llm" => Ok("Llm"),
        "human" => Ok("Human"),
        "deny" => Ok("Deny"),
        other => Err(format!(
            "unknown escalation mode '{other}'; expected llm | human | deny"
        )),
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    // Two levels up: crates/lance-graph-contract → workspace root
    let workspace_root = manifest_dir
        .parent()
        .expect("contract crate must have a parent dir")
        .parent()
        .expect("crates/ dir must have a parent (workspace root)");

    let modules_glob = workspace_root
        .join("modules")
        .join("*")
        .join("manifest.yaml");

    // Emit rerun triggers
    let workspace_cargo = workspace_root.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", workspace_cargo.display());

    // Collect manifest paths, sort lexicographically for determinism
    let mut paths: Vec<PathBuf> = glob::glob(modules_glob.to_str().unwrap())
        .unwrap_or_else(|e| panic!("invalid glob pattern: {e}"))
        .filter_map(|r| r.ok())
        .collect();
    paths.sort();

    for p in &paths {
        println!("cargo:rerun-if-changed={}", p.display());
    }

    // Parse + collect raw manifests
    let mut raw_manifests: Vec<(PathBuf, ManifestRaw)> = Vec::new();
    for path in &paths {
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let raw: ManifestRaw = serde_yaml::from_str(&src)
            .unwrap_or_else(|e| panic!("manifest parse error in {}:\n  {e}", path.display()));
        raw_manifests.push((path.clone(), raw));
    }

    // -----------------------------------------------------------------------
    // Cross-manifest validation pass
    // -----------------------------------------------------------------------

    // Validate domain_name matches directory name; collect g_slot by domain
    let mut seen_slots: HashMap<u32, PathBuf> = HashMap::new();
    let mut seen_domains: HashMap<String, PathBuf> = HashMap::new();
    let mut seen_entity_codes: HashMap<u16, (String, PathBuf)> = HashMap::new();

    // Sort by g_slot ascending so inherits_from resolution works (DOLCE first)
    // Parse slots first, then sort
    let mut slot_order: Vec<(u32, usize)> = Vec::new();
    for (i, (path, raw)) in raw_manifests.iter().enumerate() {
        let slot = canonical_slot(&raw.ogit_g).unwrap_or_else(|| {
            panic!(
                "{}: unknown ogit_g token '{}'; valid tokens: {:?}",
                path.display(),
                raw.ogit_g,
                CANONICAL_SLOTS.iter().map(|(t, _)| t).collect::<Vec<_>>()
            )
        });
        slot_order.push((slot, i));
    }
    slot_order.sort_by_key(|(s, _)| *s);

    let mut known_domains: Vec<String> = Vec::new();
    let mut validated: Vec<Manifest> = Vec::new();

    for (slot, idx) in &slot_order {
        let (path, raw) = &raw_manifests[*idx];

        // Validate version >= 1
        if raw.version < 1 {
            panic!(
                "{}: version must be >= 1, got {}",
                path.display(),
                raw.version
            );
        }

        // Validate domain_name matches directory name
        let expected_dir = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if raw.domain_name != expected_dir {
            panic!(
                "{}: domain_name '{}' does not match directory name '{}'",
                path.display(),
                raw.domain_name,
                expected_dir
            );
        }

        // Duplicate G slot check
        if let Some(prev) = seen_slots.get(slot) {
            panic!(
                "duplicate G slot: {} claimed by both\n  {}\n  AND {}\n\
                 (if adding a new version, bump `version:` in the existing manifest)",
                raw.ogit_g,
                prev.display(),
                path.display()
            );
        }
        seen_slots.insert(*slot, path.clone());

        // Duplicate domain_name check
        if let Some(prev) = seen_domains.get(&raw.domain_name) {
            panic!(
                "duplicate domain_name '{}' in\n  {}\n  AND {}",
                raw.domain_name,
                prev.display(),
                path.display()
            );
        }
        seen_domains.insert(raw.domain_name.clone(), path.clone());

        // Parse entity type codes + check global uniqueness
        for (entity_name, code_str) in &raw.entity_types {
            let code =
                parse_entity_code(code_str).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
            if let Some((prev_name, prev_path)) = seen_entity_codes.get(&code) {
                panic!(
                    "entity-type code collision: u16={} is declared by\n\
                     {}: {}\n  AND\n  {}: {}\n\
                     Entity-type codes must be globally unique across all G slots.",
                    code,
                    prev_path.display(),
                    prev_name,
                    path.display(),
                    entity_name
                );
            }
            seen_entity_codes.insert(code, (entity_name.clone(), path.clone()));
        }

        // inherits_from validation
        if let Some(parent) = &raw.inherits_from {
            if !known_domains.contains(parent) {
                panic!(
                    "{}: inherits_from '{}' does not resolve to a known domain_name.\n\
                     Known domains at this point (sorted by slot): {:?}",
                    path.display(),
                    parent,
                    known_domains
                );
            }
        } else {
            // null inherits_from is only valid for DOLCE (slot 0)
            if *slot != 0 {
                panic!(
                    "{}: inherits_from is null but ogit_g='{}' is not DOLCE (slot 0).\n\
                     Only the DOLCE root manifest may have inherits_from: ~",
                    path.display(),
                    raw.ogit_g
                );
            }
        }

        // Active consumer check (non-inert + no actor → error unless feature set)
        if !raw.inert_when_consumer_absent && raw.actor.is_none() {
            panic!(
                "{}: inert_when_consumer_absent=false but no actor block is specified.\n\
                 Either provide an actor: block or set inert_when_consumer_absent: true.",
                path.display()
            );
        }

        // Feature-flag gating for non-inert consumers
        // (We don't panic here — we just note whether the feature is set.
        //  The supervisor (PR-G2) is responsible for the hard fail at startup.)
        // For now: emit all entries unconditionally; consumer registration
        // is via inventory::submit! in the consumer crates themselves.

        // Parse stack profile
        let (audit_days, fail_closed, escalation_str) = if let Some(sp) = &raw.stack_profile {
            let esc_raw = sp.escalation.as_deref().unwrap_or("deny");
            let esc =
                parse_escalation(esc_raw).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
            (
                sp.audit_retention_days.unwrap_or(0),
                sp.requires_fail_closed.unwrap_or(false),
                esc,
            )
        } else {
            (0, false, "Deny")
        };

        let entity_count = raw.entity_types.len();

        validated.push(Manifest {
            g_slot: *slot,
            version: raw.version,
            domain_name: raw.domain_name.clone(),
            inert: raw.inert_when_consumer_absent,
            rbac_policy: raw.rbac_policy.clone(),
            audit_days,
            fail_closed,
            escalation: escalation_str.to_string(),
            actor_crate: raw.actor.as_ref().map(|a| a.crate_name.clone()),
            actor_type: raw.actor.as_ref().map(|a| a.type_name.clone()),
            entity_count,
        });

        known_domains.push(raw.domain_name.clone());
    }

    // validated is already sorted by g_slot (inherited from slot_order sort)

    // -----------------------------------------------------------------------
    // Code generation
    // -----------------------------------------------------------------------

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // --- ogit_namespace.rs ---
    emit_ogit_namespace(&out_dir, &validated);

    // --- manifest_metadata.rs ---
    emit_manifest_metadata(&out_dir, &validated);
}

// ---------------------------------------------------------------------------
// Emitter: ogit_namespace.rs
// ---------------------------------------------------------------------------

fn emit_ogit_namespace(out_dir: &Path, manifests: &[Manifest]) {
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by crates/lance-graph-contract/build.rs\n");
    out.push_str("// Source: modules/*/manifest.yaml\n");
    out.push_str("// DO NOT EDIT — regenerated when any manifest changes.\n\n");
    out.push_str("/// OGIT G-slot constants. Each constant is (g_slot, manifest_version).\n");
    out.push_str("/// Import as `use lance_graph_contract::manifest::OGIT;`\n");
    out.push_str("#[allow(non_snake_case)]\n");
    out.push_str("pub mod OGIT {\n");

    for m in manifests {
        // Derive constant name: e.g. HEALTHCARE_V1
        let const_name = format!("{}_V{}", slot_token(m.g_slot), m.version);
        out.push_str(&format!(
            "    pub const {}: (u32, u32) = ({}, {});\n",
            const_name, m.g_slot, m.version
        ));
    }

    out.push_str("}\n\n");

    // ALL_G_SLOTS
    let slots: Vec<String> = manifests.iter().map(|m| m.g_slot.to_string()).collect();
    out.push_str(
        "/// All G slots registered across all manifests (inert + active), sorted ascending.\n",
    );
    out.push_str(&format!(
        "pub const ALL_G_SLOTS: &[u32] = &[{}];\n",
        slots.join(", ")
    ));

    let path = out_dir.join("ogit_namespace.rs");
    std::fs::write(&path, &out).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
}

// ---------------------------------------------------------------------------
// Emitter: manifest_metadata.rs
// ---------------------------------------------------------------------------

fn emit_manifest_metadata(out_dir: &Path, manifests: &[Manifest]) {
    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by crates/lance-graph-contract/build.rs\n");
    out.push_str("// Source: modules/*/manifest.yaml\n");
    out.push_str("// DO NOT EDIT — regenerated when any manifest changes.\n\n");
    out.push_str("// Sorted ascending by g_slot — safe for binary_search_by_key.\n");
    out.push_str("pub static MANIFEST_METADATA: &[crate::manifest::ManifestEntry] = &[\n");

    for m in manifests {
        let rbac = match &m.rbac_policy {
            Some(p) => format!("Some(\"{}\")", p),
            None => "None".to_string(),
        };
        let actor_crate = match &m.actor_crate {
            Some(c) => format!("Some(\"{}\")", c),
            None => "None".to_string(),
        };
        let actor_type = match &m.actor_type {
            Some(t) => format!("Some(\"{}\")", t),
            None => "None".to_string(),
        };

        out.push_str(&format!(
            "    crate::manifest::ManifestEntry {{\n\
             \x20\x20\x20\x20    g_slot:       {g_slot},\n\
             \x20\x20\x20\x20    version:      {version},\n\
             \x20\x20\x20\x20    domain_name:  \"{domain_name}\",\n\
             \x20\x20\x20\x20    inert:        {inert},\n\
             \x20\x20\x20\x20    rbac_policy:  {rbac},\n\
             \x20\x20\x20\x20    stack:        crate::manifest::StackProfile {{\n\
             \x20\x20\x20\x20        audit_days:  {audit_days},\n\
             \x20\x20\x20\x20        fail_closed: {fail_closed},\n\
             \x20\x20\x20\x20        escalation:  crate::manifest::Escalation::{escalation},\n\
             \x20\x20\x20\x20    }},\n\
             \x20\x20\x20\x20    actor_crate:  {actor_crate},\n\
             \x20\x20\x20\x20    actor_type:   {actor_type},\n\
             \x20\x20\x20\x20    entity_count: {entity_count},\n\
             \x20\x20\x20\x20}},\n",
            g_slot = m.g_slot,
            version = m.version,
            domain_name = m.domain_name,
            inert = m.inert,
            rbac = rbac,
            audit_days = m.audit_days,
            fail_closed = m.fail_closed,
            escalation = m.escalation,
            actor_crate = actor_crate,
            actor_type = actor_type,
            entity_count = m.entity_count,
        ));
    }

    out.push_str("];\n");

    let path = out_dir.join("manifest_metadata.rs");
    std::fs::write(&path, &out).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
}

// ---------------------------------------------------------------------------
// Helper: reverse-map slot → canonical token name
// ---------------------------------------------------------------------------

fn slot_token(slot: u32) -> &'static str {
    CANONICAL_SLOTS
        .iter()
        .find(|(_, s)| *s == slot)
        .map(|(t, _)| *t)
        .unwrap_or("UNKNOWN")
}
