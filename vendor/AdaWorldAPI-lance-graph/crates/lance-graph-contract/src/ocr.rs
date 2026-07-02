//! OCR contract. Zero-dep.

use crate::canonical_node::{
    classid_read_mode, EdgeBlock, NodeGuid, NodeRow, ValueTenant, VALUE_SLAB_LEN,
};
use core::future::Future;

pub trait OcrProvider: Send + Sync {
    type Doc;
    type Error: core::fmt::Debug + Send + Sync + 'static;

    fn recognize<'a>(
        &'a self,
        image: PageImage<'a>,
        opts: OcrOpts<'a>,
    ) -> impl Future<Output = Result<Self::Doc, Self::Error>> + Send + 'a;
}

pub struct PageImage<'a> {
    pub bytes: &'a [u8],
    pub mime: &'a str,
    pub page_index: u32,
    pub dpi_hint: Option<u16>,
}

pub struct OcrOpts<'a> {
    /// Expected languages, BCP-47. OCR engine may or may not honor.
    pub languages: &'a [&'a str],
    /// If true, the implementation should emit full layout blocks
    /// (paragraphs, tables) rather than just text.
    pub layout: bool,
    /// Confidence threshold below which tokens are dropped.
    pub min_confidence: f32,
}

/// Bounding box in image pixel space.
#[derive(Clone, Copy, Debug)]
pub struct Bbox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Semantic classification of a layout block.
#[derive(Clone, Copy, Debug)]
pub enum BlockKind {
    Text,
    Heading,
    Table,
    Figure,
    Signature,
    Stamp,
    Other,
}

impl BlockKind {
    /// Stable OGIT entity-type discriminant for this block kind — the value
    /// written into the [`ValueTenant::EntityType`] slot. Append-only (a mini
    /// class table); `0` is the `Other`/unknown sentinel (matches the registry's
    /// "id 0 is unknown" convention). Reusing the existing EntityType tenant —
    /// no invented "ocr_kind" property (§0 anti-invention).
    pub const fn entity_type(self) -> u16 {
        match self {
            BlockKind::Other => 0,
            BlockKind::Text => 1,
            BlockKind::Heading => 2,
            BlockKind::Table => 3,
            BlockKind::Figure => 4,
            BlockKind::Signature => 5,
            BlockKind::Stamp => 6,
        }
    }
}

pub struct LayoutBlock<'a> {
    pub kind: BlockKind,
    pub bbox: Bbox,
    pub text: &'a str,
    pub confidence: f32,
}

impl<'a> LayoutBlock<'a> {
    /// Transcode one OCR layout block into a canonical SoA [`NodeRow`] — the
    /// keystone end-to-end, the reference transcode any [`OcrProvider`]
    /// (tesseract-rs and others) reuses.
    ///
    /// `classid` resolves (via [`classid_read_mode`]) to the [`ReadMode`] that
    /// says WHICH value tenants to materialise; this writes only the tenants the
    /// OCR block populates AND the resolved schema includes, each at its canon
    /// byte offset ([`ValueTenant::value_offset`]):
    /// - [`ValueTenant::EntityType`] ← [`BlockKind::entity_type`] (the semantic class).
    /// - [`ValueTenant::Energy`] ← `confidence` (POC: the OCR confidence seeded as
    ///   the node's `f32` energy scalar; a Qualia *certainty* channel is the
    ///   richer follow-up).
    ///
    /// **`text` and `bbox` are NOT bundled into the node** (`I-VSA-IDENTITIES`:
    /// the node carries identity + typed scalars; the recognised string and pixel
    /// geometry live in an external content store keyed by `identity`). The node
    /// is the *identity that points to* the OCR content, never the content's
    /// register.
    ///
    /// [`ReadMode`]: crate::canonical_node::ReadMode
    pub fn to_node_row(&self, classid: u32, identity: u32) -> NodeRow {
        let schema = classid_read_mode(classid).value_schema;
        let mut value = [0u8; VALUE_SLAB_LEN];

        if schema.has(ValueTenant::EntityType) {
            let o = ValueTenant::EntityType.value_offset();
            value[o..o + 2].copy_from_slice(&self.kind.entity_type().to_le_bytes());
        }
        if schema.has(ValueTenant::Energy) {
            let o = ValueTenant::Energy.value_offset();
            value[o..o + 4].copy_from_slice(&self.confidence.to_le_bytes());
        }

        NodeRow {
            // HHT unbound (0) and default basin for the POC — only `identity`
            // discriminates (the canon bootstrap address); `classid` still selects
            // the read-mode. Minting HEEL/HIP/TWIG + family is the OGAR follow-up.
            key: NodeGuid::new(classid, 0, 0, 0, NodeGuid::FAMILY_DEFAULT, identity),
            edges: EdgeBlock::default(),
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_node::{NodeRowPacket, ValueSchema};
    use crate::soa_envelope::SoaEnvelope;

    fn heading(text: &str, conf: f32) -> LayoutBlock<'_> {
        LayoutBlock {
            kind: BlockKind::Heading,
            bbox: Bbox {
                x: 10,
                y: 20,
                w: 100,
                h: 30,
            },
            text,
            confidence: conf,
        }
    }

    #[test]
    fn block_kind_entity_types_are_stable_and_other_is_zero() {
        // 0 is the unknown sentinel (registry convention); the rest are distinct.
        assert_eq!(BlockKind::Other.entity_type(), 0);
        let ids = [
            BlockKind::Text.entity_type(),
            BlockKind::Heading.entity_type(),
            BlockKind::Table.entity_type(),
            BlockKind::Figure.entity_type(),
            BlockKind::Signature.entity_type(),
            BlockKind::Stamp.entity_type(),
        ];
        for (i, a) in ids.iter().enumerate() {
            assert_ne!(*a, 0, "non-Other kind must not be the sentinel");
            for b in &ids[i + 1..] {
                assert_ne!(a, b, "entity types must be distinct");
            }
        }
    }

    #[test]
    fn layout_block_transcodes_to_node_row_via_keystone() {
        let blk = heading("Invoice", 0.97);
        let row = blk.to_node_row(NodeGuid::CLASSID_DEFAULT, 0x00_0042);

        // Identity is preserved in the key; classid selects the read-mode.
        assert_eq!(row.key.identity(), 0x00_0042);
        assert_eq!(row.key.classid(), NodeGuid::CLASSID_DEFAULT);
        assert!(row.key.is_bootstrap_address());

        // The keystone: classid → read-mode → Full (POC) → tenants materialised.
        let rm = row.key.read_mode();
        assert_eq!(rm.value_schema, ValueSchema::Full);

        // EntityType ← BlockKind::Heading (2), at its canon slab offset.
        let o = ValueTenant::EntityType.value_offset();
        assert_eq!(u16::from_le_bytes([row.value[o], row.value[o + 1]]), 2);

        // Energy ← OCR confidence 0.97, at its canon slab offset.
        let o = ValueTenant::Energy.value_offset();
        let energy = f32::from_le_bytes(row.value[o..o + 4].try_into().unwrap());
        assert!((energy - 0.97).abs() < 1e-6, "energy ← confidence");
    }

    #[test]
    fn transcoded_row_packs_zero_copy_through_envelope() {
        // The transcoded row is a plain NodeRow → it rides the SoaEnvelope with
        // no special-casing (512-byte stride, zero-copy LE view, verifies).
        let rows = [
            heading("Invoice", 0.9).to_node_row(NodeGuid::CLASSID_DEFAULT, 1),
            heading("Total", 0.8).to_node_row(NodeGuid::CLASSID_DEFAULT, 2),
        ];
        let pkt = NodeRowPacket::new(&rows, 0);
        assert_eq!(pkt.n_rows(), 2);
        assert_eq!(pkt.as_le_bytes().len(), 2 * 512);
        assert_eq!(
            pkt.as_le_bytes().as_ptr() as usize,
            rows.as_ptr() as usize,
            "transcoded rows pack zero-copy"
        );
        assert!(pkt.verify_layout().is_ok());
    }

    #[test]
    fn transcode_is_schema_gated_only_present_tenants_written() {
        // The transcode honors the resolved schema: it writes a tenant ONLY if
        // the read-mode includes it. Under the POC Full default both EntityType
        // and Energy are present, so both slots are populated; tenants the schema
        // omits would stay zero. (No classid resolves to Bootstrap today — when
        // one is minted, the same `schema.has()` gate leaves its slab empty.)
        let row = heading("x", 1.0).to_node_row(NodeGuid::CLASSID_DEFAULT, 7);
        let schema = row.key.read_mode().value_schema;
        assert!(schema.has(ValueTenant::EntityType) && schema.has(ValueTenant::Energy));
        let et = ValueTenant::EntityType.value_offset();
        assert_ne!(
            row.value[et], 0,
            "present EntityType is written (Heading=2)"
        );
        // A slab byte that belongs to NO tenant the transcode writes stays zero
        // (e.g. the Meta tenant at offset 0 — present in Full but the OCR transcode
        // doesn't populate it, so it remains the zero default).
        assert_eq!(
            row.value[ValueTenant::Meta.value_offset()],
            0,
            "a tenant the transcode doesn't populate stays zero"
        );
    }

    #[test]
    fn ocr_schema_fit_rides_existing_preset_no_new_variant() {
        // Probe OCR-SCHEMA (.claude/plans/ocr-probes-v1.md): the OCR value tenants
        // fit an EXISTING ValueSchema preset, so a 5th `ValueSchema::Ocr` enum variant
        // is NOT needed (#496 §0 anti-invention). `Compressed` carries the codec
        // residues — but OCR also writes confidence→Energy + repair→Plasticity, which
        // `Compressed` LACKS, so OCR rides `Full` (the only preset with residues AND
        // the hot lifecycle columns), not `Compressed` (codex P2 on #500).
        let compressed = ValueSchema::Compressed;
        for t in [
            ValueTenant::HelixResidue,
            ValueTenant::TurbovecResidue,
            ValueTenant::EntityType,
            ValueTenant::Fingerprint,
        ] {
            assert!(
                compressed.has(t),
                "Compressed carries the codec residue {t:?}"
            );
        }
        // ...but NOT the hot columns OCR's writeback needs — Compressed alone drops them.
        assert!(
            !compressed.has(ValueTenant::Energy),
            "Compressed lacks Energy"
        );
        assert!(
            !compressed.has(ValueTenant::Plasticity),
            "Compressed lacks Plasticity"
        );
        // OCR rides `Full`, which carries every tenant OCR touches (residues + Meta
        // anchor + Energy confidence + Plasticity provenance + EntityType).
        let full = ValueSchema::Full;
        for t in [
            ValueTenant::HelixResidue,
            ValueTenant::TurbovecResidue,
            ValueTenant::EntityType,
            ValueTenant::Meta,
            ValueTenant::Energy,
            ValueTenant::Plasticity,
        ] {
            assert!(full.has(t), "Full POC default carries {t:?}");
        }
        // Both presets are layout-preserving — riding either needs no ENVELOPE_LAYOUT_VERSION bump.
        assert!(compressed.is_layout_preserving() && full.is_layout_preserving());
    }
}
