//! Canonical SoA node — LOCKED minimal layout + zero-fallback ladder.
//!
//! Decisions pinned here (everything else comes after):
//!   * key byte/print order: classid · HEEL · HIP · TWIG · family · identity (LE)
//!   * family + identity are the CONTIGUOUS TRAILING 6 BYTES → the basin-local
//!     key you can use alone after an HHTL radix walk (skip the prefix).
//!   * edge block = 12 in-family + 4 out-of-family, one byte per slot (canonical,
//!     not mandatory — always reserved, never shrunk; opt-out is registry-resolved).
//!   * node = 4096 bit = 512 byte = key(16) | edges(16) | value(480).
//!
//! ## Zero-fallback ladder (monotonic: zero = fall through to the broader default)
//!   * classid  == 0x0000_0000  → default class,  no prefix routing   (dormant)
//!   * family   == 0x00_0000     → default basin,  no neighborhood grouping (dormant)
//!   * ⇒ while both are zero, `identity` (3 bytes / 24 bits) ALONE discriminates.
//!
//! RESERVE, DON'T RECLAIM: a zero tier means "not consulted", never "compacted
//! away". classid(4B) and family(3B) keep their fixed offsets so a non-zero mint
//! later wakes routing/basin binding with ZERO layout change.
//!
//! No UUID ceremony: no version nibble, no variant bits, no namespace/kind framing.
//! Little-endian throughout so the trailing-6-byte local key is a single masked load.

/// 16-byte canonical instance key.
///
/// ```text
///   0..4   classid   (u32)   ← 8 hex, prefix-routable; default 0x0000_0000
///   4..6   HEEL      (u16)   ┐
///   6..8   HIP       (u16)   ├ 3 cascade tiers (HHTL path)
///   8..10  TWIG      (u16)   ┘
///  10..13  family    (u24)   ┐ trailing 6 bytes = basin-local key
///  13..16  identity  (u24)   ┘ (usable alone once the prefix is trie-resolved)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C, align(16))]
pub struct NodeGuid([u8; 16]);

impl NodeGuid {
    /// Reserved canonical default class (implicit fallback; no prefix routing).
    pub const CLASSID_DEFAULT: u32 = 0x0000_0000;
    /// Reserved canonical default basin (implicit fallback; no neighborhood grouping).
    pub const FAMILY_DEFAULT: u32 = 0x00_0000;

    // ── classids follow OGAR `ogar-vocab`'s domain-encoded `0xDDCC` codebook ──
    // (DD = domain high byte, CC = concept slot; CC=0x00 = domain root, reserved).
    // The `0xDDCC` codebook id is the classid's CANON half — the HIGH u16 since
    // the 2026-07-02 half-order flip (`classid-canon-custom-flip-v1` P1;
    // `crate::ogar_codebook::classid_canon`). Realigned 2026-06-20
    // (ISS-CLASSID-OGAR-DRIFT): OSINT was 0x0007 (OGAR Reserved domain) → 0x0700;
    // FMA was 0x0008 (OGAR OCR block) → 0x0901. Re-realigned 2026-06-24
    // (ISS-CLASSID-OGAR-DRIFT cont.): FMA 0x0901 **collided with OGAR `patient`
    // (0x0901)** — both Health. FMA now routes to the new **Anatomy** domain root
    // 0x0A01 (`anatomical_structure`); anatomy is public reference, not Health
    // PHI. Surfaced by OGAR `docs/NODEGUID-CANON-AUDIT.md` F-1. Migration:
    // `.claude/plans/ogar-vocab-contract-codebook-migration-v1.md`.
    //
    // MINT-FORWARD BOUNDARY (flip P1/P3): the named constants below are the
    // MINT surface — they carry the new canon-HIGH form. The pre-flip stored
    // forms (`0x0000_DDCC` v1 / `0x1000_DDCC` V3) stay behind as
    // `CLASSID_*_LEGACY` read-only aliases so persisted rows keep resolving
    // through `BUILTIN_READ_MODES`; they retire only on corpus proof that zero
    // old-form rows remain (codex P2 #627 — RESERVE, DON'T RECLAIM).

    /// **OSINT / Palantir-Gotham** domain root (`0x07` = OSINT domain, `0x00` =
    /// root — "applied domain-wise" per the 2026-07-02 ruling). The
    /// neo4j-emulation entity graph (people / orgs / systems / events,
    /// family-grouped). Canon `0x0700` HIGH, custom `0x0000`. Resolves to
    /// [`ReadMode::OSINT`] (hot `Cognitive` value + `CoarseOnly` adjacency).
    pub const CLASSID_OSINT: u32 = 0x0700_0000;
    /// Pre-flip stored form of [`CLASSID_OSINT`] (canon in the LOW half) —
    /// read-only legacy alias for persisted rows; do NOT mint with this.
    pub const CLASSID_OSINT_LEGACY: u32 = 0x0000_0700;
    /// **FMA anatomy** — `anatomical_structure` (`0x01`) in the **Anatomy** domain
    /// (`0x0A`); `0x0A00` is the Anatomy root. The Foundational Model of Anatomy
    /// (~70k structural entities, family = body region, bones = stability anchors).
    /// Anatomy is **public reference, not Health PHI** — moved off `0x0901` to
    /// clear the collision with OGAR `patient`. Canon `0x0A01` HIGH. Resolves to
    /// [`ReadMode::FMA`] (cold `Compressed` reference + `CoarseOnly`).
    pub const CLASSID_FMA: u32 = 0x0A01_0000;
    /// Pre-flip stored form of [`CLASSID_FMA`] — read-only legacy alias.
    pub const CLASSID_FMA_LEGACY: u32 = 0x0000_0A01;
    /// **Project-management** domain root (`0x01`) — OpenProject ↔ Redmine
    /// (work items, members, versions, …). OGAR codebook `0x01XX`. Canon
    /// `0x0100` HIGH. Resolves to [`ReadMode::PROJECT`].
    pub const CLASSID_PROJECT: u32 = 0x0100_0000;
    /// Pre-flip stored form of [`CLASSID_PROJECT`] — read-only legacy alias.
    pub const CLASSID_PROJECT_LEGACY: u32 = 0x0000_0100;
    /// **Commerce / ERP** domain root (`0x02`) — Odoo ↔ OSB (invoices, taxes,
    /// partners, payments, …). OGAR codebook `0x02XX`. Canon `0x0200` HIGH.
    /// Resolves to [`ReadMode::ERP`].
    pub const CLASSID_ERP: u32 = 0x0200_0000;
    /// Pre-flip stored form of [`CLASSID_ERP`] — read-only legacy alias.
    pub const CLASSID_ERP_LEGACY: u32 = 0x0000_0200;

    // ── V3 cascade-key classids (feature `guid-v3-tail`) ───────────────────────
    // Since the 2026-07-02 half-order flip (P1): the CANON (`domain:appid`,
    // e.g. `0x0701` = OSINT domain `0x07`, appid `0x01` = q2) sits in the HIGH
    // u16 and the V3 generation marker `0x1000` in the LOW/custom u16 — stored
    // `0x0701_1000`, human-readable `0x07:01::1000` (the operator's mnemonic).
    // The marker is temporary by declaration (a "hard reminder" of the V3
    // migration); its retirement is the plan's P4 operator checkpoint. The
    // appid byte normalizes to `:01` (q2, the consumer app) for OSINT and
    // CPIC per the ruling ("0701 is q2 as the OSINT appid, our consumer";
    // "same for cpic also under q2"); FMA was already `:01`. Pre-flip stored
    // forms (`0x1000_DDCC`) remain as `_LEGACY` read-only alias keys.

    /// **OSINT-V3** — OSINT on a [`TailVariant::V3`] cascade tail, minted for
    /// q2 (appid `0x01`). Canon `0x0701` HIGH; the V3 marker `0x1000` in the
    /// LOW/custom u16 — `0x07:01::1000`.
    /// [`classid_concept_domain`](crate::ogar_codebook::classid_concept_domain)
    /// routes [`Osint`](crate::ogar_codebook::ConceptDomain::Osint) off the
    /// canon half. Resolves to [`ReadMode::OSINT_V3`].
    #[cfg(feature = "guid-v3-tail")]
    pub const CLASSID_OSINT_V3: u32 = 0x0701_1000;
    /// Pre-flip stored form of [`CLASSID_OSINT_V3`] (marker HIGH, canon
    /// `0x0700` LOW — note the pre-normalization appid `:00`) — read-only
    /// legacy alias for persisted rows; do NOT mint with this.
    #[cfg(feature = "guid-v3-tail")]
    pub const CLASSID_OSINT_V3_LEGACY: u32 = 0x1000_0700;

    /// **FMA-V3** — FMA anatomy on a [`TailVariant::V3`] cascade tail, minted
    /// for q2. Canon `0x0A01` HIGH (Anatomy domain `0x0A`, appid `0x01`); the
    /// V3 marker `0x1000` in the LOW/custom u16 — `0x0A:01::1000`.
    /// [`classid_concept_domain`](crate::ogar_codebook::classid_concept_domain)
    /// routes [`Anatomy`](crate::ogar_codebook::ConceptDomain::Anatomy).
    /// Resolves to [`ReadMode::FMA_V3`] (same cold `Compressed` model as legacy FMA).
    #[cfg(feature = "guid-v3-tail")]
    pub const CLASSID_FMA_V3: u32 = 0x0A01_1000;
    /// Pre-flip stored form of [`CLASSID_FMA_V3`] — read-only legacy alias.
    #[cfg(feature = "guid-v3-tail")]
    pub const CLASSID_FMA_V3_LEGACY: u32 = 0x1000_0A01;

    /// **CPIC-V3** — CPIC pharmacogenomics (gene–drug guidelines) on a
    /// [`TailVariant::V3`] cascade tail, in the **Genetics** domain (`0x0E`,
    /// operator-allocated 2026-06-26 — `0x0D` was already HR), minted for q2
    /// (appid `0x01`, normalized from the pre-flip domain-root `:00` per the
    /// ruling "same for cpic also under q2"). Canon `0x0E01` HIGH; the V3
    /// marker `0x1000` in the LOW/custom u16 — `0x0E:01::1000`.
    /// [`classid_concept_domain`](crate::ogar_codebook::classid_concept_domain)
    /// routes [`Genetics`](crate::ogar_codebook::ConceptDomain::Genetics). Resolves
    /// to [`ReadMode::CPIC_V3`].
    ///
    /// **The 6 V3 basins are genomic MEREOLOGY, not labels** (operator directive
    /// 2026-06-26; `I-VSA-IDENTITIES` Test-0, register-laziness): a gene's identity
    /// is its *position* in the part-of hierarchy (genome → chromosome → region →
    /// locus → gene), readable as HHTL `(X;Y)` coordinates per `(part_of:is_a)`
    /// tile — never a flat type tag a `HashMap` would carry. The 6-basin + relative
    /// location is a substantial address; spending it on labels wastes it. The human
    /// genome is the **fixed schema view** the position is taken against, which is
    /// why the value model is [`ValueSchema::Compressed`] (a fixed reference frame,
    /// not a hot lifecycle); Phase 2 shapes the V3 tenants — gene expression as the
    /// coordinate *value* — on top.
    #[cfg(feature = "guid-v3-tail")]
    pub const CLASSID_CPIC_V3: u32 = 0x0E01_1000;
    /// Pre-flip stored form of [`CLASSID_CPIC_V3`] (marker HIGH, canon
    /// `0x0E00` LOW — pre-normalization appid `:00`) — read-only legacy alias.
    #[cfg(feature = "guid-v3-tail")]
    pub const CLASSID_CPIC_V3_LEGACY: u32 = 0x1000_0E00;

    /// Construct from the six canonical groups. `family`/`identity` use their low 3 bytes.
    ///
    /// Panics (incl. const-eval) when `family` or `identity` exceed 24 bits — the
    /// silent-truncation footgun: distinct u32 inputs would otherwise collapse
    /// to the same stored key.
    pub const fn new(
        classid: u32,
        heel: u16,
        hip: u16,
        twig: u16,
        family: u32,
        identity: u32,
    ) -> Self {
        assert!(family <= 0x00FF_FFFF, "family must fit in 24 bits");
        assert!(identity <= 0x00FF_FFFF, "identity must fit in 24 bits");
        let c = classid.to_le_bytes();
        let h = heel.to_le_bytes();
        let p = hip.to_le_bytes();
        let t = twig.to_le_bytes();
        let f = family.to_le_bytes(); // low 3 bytes
        let i = identity.to_le_bytes(); // low 3 bytes
        Self([
            c[0], c[1], c[2], c[3], //  0..4  classid
            h[0], h[1], //  4..6  HEEL
            p[0], p[1], //  6..8  HIP
            t[0], t[1], //  8..10 TWIG
            f[0], f[1], f[2], // 10..13 family
            i[0], i[1], i[2], // 13..16 identity
        ])
    }

    /// Default-class, default-basin node: only `identity` discriminates.
    /// This is the bootstrap address while classid and family are zero.
    pub const fn local(identity: u32) -> Self {
        Self::new(
            Self::CLASSID_DEFAULT,
            0,
            0,
            0,
            Self::FAMILY_DEFAULT,
            identity,
        )
    }

    #[inline]
    pub const fn classid(&self) -> u32 {
        u32::from_le_bytes([self.0[0], self.0[1], self.0[2], self.0[3]])
    }

    #[inline]
    pub const fn family(&self) -> u32 {
        u32::from_le_bytes([self.0[10], self.0[11], self.0[12], 0])
    }

    #[inline]
    pub const fn identity(&self) -> u32 {
        u32::from_le_bytes([self.0[13], self.0[14], self.0[15], 0])
    }

    /// HEEL — HHT cascade tier 1 (bytes 4..6, LE `u16`).
    #[inline]
    pub const fn heel(&self) -> u16 {
        u16::from_le_bytes([self.0[4], self.0[5]])
    }

    /// HIP — HHT cascade tier 2 (bytes 6..8, LE `u16`).
    #[inline]
    pub const fn hip(&self) -> u16 {
        u16::from_le_bytes([self.0[6], self.0[7]])
    }

    /// TWIG — HHT cascade tier 3 (bytes 8..10, LE `u16`).
    #[inline]
    pub const fn twig(&self) -> u16 {
        u16::from_le_bytes([self.0[8], self.0[9]])
    }

    /// Decode the whole key in one read — every canon group as its native
    /// LE-decoded integer. This is the "read the GUID as a GUID" surface: a
    /// consumer or OGAR gets `classid + HHT (HEEL/HIP/TWIG) + family + identity`
    /// from one call instead of re-deriving each group from raw bytes. The six
    /// fields ARE the canon print order — nothing invented, nothing dropped (cf.
    /// [`Display`](NodeGuid#impl-Display-for-NodeGuid), which renders the same six).
    #[inline]
    pub const fn decode(&self) -> GuidParts {
        GuidParts {
            classid: self.classid(),
            heel: self.heel(),
            hip: self.hip(),
            twig: self.twig(),
            family: self.family(),
            identity: self.identity(),
        }
    }

    /// The [`ReadMode`] this node's `classid` resolves to — which value tenants
    /// to materialise + how to read the edge block. The carrier-method form (the
    /// object speaks for itself): a consumer reads `guid.read_mode()`, OGAR reads
    /// [`classid_read_mode`]`(guid.classid())`; both inherit the SAME answer from
    /// the one [`LazyLock`] registry, so the LE interpretation of the node's bytes
    /// is single-sourced. Not `const` — it consults the runtime registry.
    #[inline]
    pub fn read_mode(&self) -> ReadMode {
        classid_read_mode(self.classid())
    }

    /// Basin-local key: trailing 6 bytes (family ++ identity), zero-padded to u64.
    /// After an HHTL radix walk has bound classid+HEEL+HIP+TWIG, this is the only
    /// part that still discriminates — a single masked load, no gather.
    #[inline]
    pub const fn local_key(&self) -> u64 {
        u64::from_le_bytes([
            self.0[10], self.0[11], self.0[12], self.0[13], self.0[14], self.0[15], 0, 0,
        ])
    }

    // ── fallback-ladder dispatch guards ─────────────────────────────────────
    /// `true` while the classid is the implicit default (no prefix routing).
    #[inline]
    pub const fn is_default_class(&self) -> bool {
        self.classid() == Self::CLASSID_DEFAULT
    }
    /// `true` while the family is the implicit default basin (no grouping).
    #[inline]
    pub const fn is_unbasined(&self) -> bool {
        self.family() == Self::FAMILY_DEFAULT
    }
    /// `true` when both tiers fall through and only `identity` discriminates.
    #[inline]
    pub const fn is_bootstrap_address(&self) -> bool {
        self.is_default_class() && self.is_unbasined()
    }

    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Mint-path guard: while in the default basin, `identity` (24 bits) is the
    /// ONLY discriminator, so the mint path MUST guarantee its uniqueness. Call
    /// on insert with whatever set/bitmap the mint path keeps; this centralises
    /// the invariant so it can't be forgotten while family is still a no-op.
    #[inline]
    pub fn debug_assert_identity_unique(&self, already_present: bool) {
        if self.is_bootstrap_address() {
            debug_assert!(
                !already_present,
                "identity collision in default basin: 24-bit identity space exhausted \
                 or reused — mint a non-zero family to expand before this fires in prod"
            );
        }
    }
}

// ── GUID v2 tail (leaf·family·identity, 3×u16) — D-GV2-1, feature-gated ────────
//
// The v2 basin tail repartitions bytes 10..16: leaf(u16) 10..12 (the 4th HHTL
// tier), family(u16) 12..14 (the basin / episodic hub), identity(u16) 14..16
// (the instance). Bytes 0..10 (classid·HEEL·HIP·TWIG) are IDENTICAL to v1.
// Additive and NON-breaking: v1 `new`/`family`/`identity` are untouched; these
// v2 accessors coexist behind `guid-v2-tail` until cutover (D-GV2-5). Per
// I-LEGACY-API-FEATURE-GATED the v2 names are distinct (`leaf`/`*_v2`), so no
// function silently changes semantics, and `GUID_TAIL_LAYOUT_VERSION_V2` is the
// version gate marking a v2-tail packet.
#[cfg(feature = "guid-v2-tail")]
impl NodeGuid {
    /// Construct a v2-tail GUID: `classid·HEEL·HIP·TWIG` identical to v1, then the
    /// 3×u16 basin tail `leaf·family·identity`. Each tail field is a full `u16` —
    /// no 24-bit truncation footgun (the point of v2).
    #[allow(clippy::too_many_arguments)]
    pub const fn new_v2(
        classid: u32,
        heel: u16,
        hip: u16,
        twig: u16,
        leaf: u16,
        family: u16,
        identity: u16,
    ) -> Self {
        let c = classid.to_le_bytes();
        let h = heel.to_le_bytes();
        let p = hip.to_le_bytes();
        let t = twig.to_le_bytes();
        let l = leaf.to_le_bytes();
        let f = family.to_le_bytes();
        let i = identity.to_le_bytes();
        Self([
            c[0], c[1], c[2], c[3], //  0..4  classid
            h[0], h[1], //  4..6  HEEL
            p[0], p[1], //  6..8  HIP
            t[0], t[1], //  8..10 TWIG
            l[0], l[1], // 10..12 leaf   (4th HHTL tier)
            f[0], f[1], // 12..14 family (basin / episodic hub)
            i[0], i[1], // 14..16 identity (instance)
        ])
    }

    /// Mint a node by its **tail variant** — the carrier form of the Phase-1
    /// symmetric spine (`soa-value-tenant-migration-v2.md` §2.1): a consumer
    /// mints with `mint_for(classid_read_mode(c).tail_variant, …)`, NEVER by
    /// hardcoding `new` vs `new_v2`. The key-side analog of the value-side
    /// `to_node_row(classid_read_mode(c).value_schema, …)` — same
    /// [`classid_read_mode`] lookup, sibling field. Migrating a class's identity
    /// to V3 is then a one-line flip of its `tail_variant` in the registry, with
    /// zero consumer rewrite (the "extend the one `ReadMode`, never a public
    /// `new_v3`" litmus).
    ///
    /// Dispatch (all three [`TailVariant`] arms exist unconditionally as enum
    /// values; only the constructors they call are gated):
    /// - [`V1`](TailVariant::V1) → [`new`](NodeGuid::new): the canonical
    ///   `family(u24)·identity(u24)` tail. `leaf` is not part of the V1 tail and
    ///   is intentionally ignored (the V1 cascade is HEEL·HIP·TWIG only).
    /// - [`V2`](TailVariant::V2) / [`V3`](TailVariant::V3) → [`new_v2`](NodeGuid::new_v2):
    ///   the shared `leaf·family·identity` 3×u16 tail bytes. V3 differs from V2
    ///   only in how those bytes are *read* (the `(part_of:is_a)` cascade tile),
    ///   not how they are *stored* — so it mints through the same constructor.
    ///
    /// **No silent truncation** (the footgun v2 exists to remove): the V2/V3 arm
    /// asserts `family`/`identity` fit `u16`, mirroring [`new`](NodeGuid::new)'s
    /// own 24-bit guard. An out-of-range value is a loud panic, never a wrong key.
    #[allow(clippy::too_many_arguments)]
    pub const fn mint_for(
        tail_variant: TailVariant,
        classid: u32,
        heel: u16,
        hip: u16,
        twig: u16,
        leaf: u16,
        family: u32,
        identity: u32,
    ) -> Self {
        match tail_variant {
            TailVariant::V1 => Self::new(classid, heel, hip, twig, family, identity),
            TailVariant::V2 | TailVariant::V3 => {
                assert!(
                    family <= 0xFFFF,
                    "v2/v3 family must fit in 16 bits (no silent truncation)"
                );
                assert!(
                    identity <= 0xFFFF,
                    "v2/v3 identity must fit in 16 bits (no silent truncation)"
                );
                Self::new_v2(
                    classid,
                    heel,
                    hip,
                    twig,
                    leaf,
                    family as u16,
                    identity as u16,
                )
            }
        }
    }

    /// v2 `leaf` — bytes 10..12, the 4th HHTL routing tier (cascade terminal).
    #[inline]
    pub const fn leaf(&self) -> u16 {
        u16::from_le_bytes([self.0[10], self.0[11]])
    }

    /// v2 `family` — bytes 12..14, the basin / episodic-hub tier (the codebook
    /// selector). Distinct from v1 [`family`](NodeGuid::family) (u24 at 10..13):
    /// different name, different bytes — no silent semantic swap.
    #[inline]
    pub const fn family_v2(&self) -> u16 {
        u16::from_le_bytes([self.0[12], self.0[13]])
    }

    /// v2 `identity` — bytes 14..16, the instance tier (full `u16`).
    #[inline]
    pub const fn identity_v2(&self) -> u16 {
        u16::from_le_bytes([self.0[14], self.0[15]])
    }

    /// v2 basin-local key: trailing 4 bytes (family ++ identity), zero-padded to
    /// `u32` — the discriminator once the HHTL prefix (incl. leaf) is bound.
    #[inline]
    pub const fn local_key_v2(&self) -> u32 {
        u32::from_le_bytes([self.0[12], self.0[13], self.0[14], self.0[15]])
    }

    /// v2 decode — every tier (`classid·HEEL·HIP·TWIG·leaf·family·identity`) as a
    /// native integer. The "read the GUID as a GUID" surface for v2.
    #[inline]
    pub const fn decode_v2(&self) -> GuidPartsV2 {
        GuidPartsV2 {
            classid: self.classid(),
            heel: self.heel(),
            hip: self.hip(),
            twig: self.twig(),
            leaf: self.leaf(),
            family: self.family_v2(),
            identity: self.identity_v2(),
        }
    }

    /// v2 self-describing hex: `classid-heel-hip-twig-leaf-family-identity`,
    /// uniform 4-hex groups (classid as 8) — the v2 Display shape.
    pub fn to_hex_v2(&self) -> String {
        let p = self.decode_v2();
        format!(
            "{:08x}-{:04x}-{:04x}-{:04x}-{:04x}-{:04x}-{:04x}",
            p.classid, p.heel, p.hip, p.twig, p.leaf, p.family, p.identity
        )
    }
}

/// The v2-tail GUID decoded — `classid · HEEL · HIP · TWIG · leaf · family ·
/// identity`, every tier a native integer (no `u24`). The v2 counterpart of
/// [`GuidParts`]. (D-GV2-1; feature `guid-v2-tail`.)
#[cfg(feature = "guid-v2-tail")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GuidPartsV2 {
    /// 0..4 — prefix-routable class id.
    pub classid: u32,
    /// 4..6 — HEEL (HHT cascade tier 1).
    pub heel: u16,
    /// 6..8 — HIP (HHT cascade tier 2).
    pub hip: u16,
    /// 8..10 — TWIG (HHT cascade tier 3).
    pub twig: u16,
    /// 10..12 — leaf, the 4th HHTL tier.
    pub leaf: u16,
    /// 12..14 — family, the basin / episodic hub.
    pub family: u16,
    /// 14..16 — identity, the instance.
    pub identity: u16,
}

/// v2 layout-version marker: a v2-tail packet is layout version 2. A v1 reader
/// MUST refuse a v2 blob (and vice-versa) — the version gate per
/// `I-LEGACY-API-FEATURE-GATED`. Wired into the `SoaEnvelope` version at cutover
/// (D-GV2-5).
#[cfg(feature = "guid-v2-tail")]
pub const GUID_TAIL_LAYOUT_VERSION_V2: u16 = 2;

/// The whole canonical key decoded in one shot — `classid · HEEL · HIP · TWIG ·
/// family · identity`, each as its native LE-decoded integer.
///
/// This is the "read the GUID as a GUID and return classid + HHT + Leaf +
/// identity" contract: one decode, six fields, in canon print order. It invents
/// nothing — it is exactly [`NodeGuid::decode`] of the existing 16-byte key, the
/// same six groups [`NodeGuid`]'s `Display` renders. `family` is the basin
/// "Leaf" and `family ++ identity` is the trailing-6-byte [`NodeGuid::local_key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GuidParts {
    /// 0..4 — prefix-routable class id (default `0x0000_0000`).
    pub classid: u32,
    /// 4..6 — HEEL (HHT cascade tier 1).
    pub heel: u16,
    /// 6..8 — HIP (HHT cascade tier 2).
    pub hip: u16,
    /// 8..10 — TWIG (HHT cascade tier 3).
    pub twig: u16,
    /// 10..13 — family (u24, the basin "Leaf").
    pub family: u32,
    /// 13..16 — identity (u24).
    pub identity: u32,
}

/// Canonical self-describing print: `classid-HEEL-HIP-TWIG-family·identity`.
///
/// The dash-groups ARE the semantic delimiters — every printed GUID is
/// self-describing at sight (OGAR canon, P0). `{:08x}-{:04x}-{:04x}-{:04x}-{:06x}{:06x}`
/// renders the canonical 8-4-4-4-12 hex layout regardless of in-memory byte
/// order (the field accessors fold LE bytes into u32/u16/u24 first).
impl core::fmt::Display for NodeGuid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:06x}{:06x}",
            self.classid(),
            self.heel(),
            self.hip(),
            self.twig(),
            self.family(),
            self.identity(),
        )
    }
}

/// 16-byte canonical edge block: 12 in-family + 4 out-of-family.
///
/// Canonical, not mandatory: the 16 bytes are ALWAYS reserved (zeroed when unused).
/// A class never shrinks this block — opting out of edges is resolved via
/// classid → ClassView in the registry, never by changing the row stride.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C, align(16))]
pub struct EdgeBlock {
    /// 12 local adjacency slots (basin-local), one byte each.
    pub in_family: [u8; 12],
    /// 4 inherited adapter slots (out-of-family interfaces), one byte each.
    pub out_family: [u8; 4],
}

/// Which edge-codec flavor a class uses to *read* its node's edge block.
///
/// The flavor is an INTERPRETATION of the canonical 16-byte [`EdgeBlock`] (plus
/// an optional value-slab residue), selected per class via
/// [`ClassView::edge_codec_flavor`](crate::class_view::ClassView::edge_codec_flavor)
/// — never a change to [`NodeRow`]'s 512-byte layout. Every variant leaves
/// [`NODE_ROW_STRIDE`] untouched (the canon "registry-resolved via
/// `classid → ClassView`" rule), so adopting a flavor needs NO
/// `ENVELOPE_LAYOUT_VERSION` bump.
///
/// Encode/reconstruct kernels live in `ndarray::hpc::edge_codec`; per-flavor
/// fidelity is measured by `ndarray::hpc::reliability` (see the
/// `edge_codec_compare` example — CoarseResidue dominates on agreement, Pq32x4
/// preserves rank but not absolute distance). Default is [`CoarseOnly`], the
/// zero-fallback reading that matches the canon all-zero bootstrap default.
///
/// [`CoarseOnly`]: EdgeCodecFlavor::CoarseOnly
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum EdgeCodecFlavor {
    /// 1 byte/vector: each edge byte is a palette/centroid index — the
    /// [`EdgeBlock`] read literally. The canon zero-fallback default.
    #[default]
    CoarseOnly = 0,
    /// 1 + ⌈D/2⌉ bytes: coarse index + a per-dimension signed-4-bit residue
    /// carried in the reserved value slab. Highest fidelity / agreement.
    CoarseResidue = 1,
    /// 16 bytes: the edge block read as 32 × 4-bit product-quantizer codes (the
    /// turbovec PQ model). Preserves neighbour *rank* better than absolute
    /// distance (low ICC, decent Spearman).
    Pq32x4 = 2,
}

impl EdgeCodecFlavor {
    /// Per-vector byte cost for dimensionality `dim` (D even for the residue's
    /// nibble packing; `⌈D/2⌉` is used so odd D rounds up).
    #[inline]
    pub const fn bytes_per_vector(self, dim: usize) -> usize {
        match self {
            EdgeCodecFlavor::CoarseOnly => 1,
            EdgeCodecFlavor::CoarseResidue => 1 + dim.div_ceil(2),
            EdgeCodecFlavor::Pq32x4 => 16,
        }
    }

    /// Every flavor re-interprets the SAME 512-byte node row — none changes
    /// [`NODE_ROW_STRIDE`], so no flavor requires a layout-version bump. This is
    /// the canon invariant, encoded so a regression test can assert it.
    #[inline]
    pub const fn is_layout_preserving(self) -> bool {
        true
    }
}

/// One node = 4096 bit = 512 byte: key(16) | edges(16) | value(480).
///
/// The 480-byte value is deferred — energy/meta/qualia/entity_type, materialized
/// CausalEdge64, helix residue, fingerprint, class extensions all land here later,
/// Lance-compressible. This is the row the MailboxSoA owns and the MailboxSoaView reads.
///
/// **Two doctrines (operator 2026-06-29), neither a blocker:**
/// 1. **Clean ⇒ expansion is `classid`-inherited.** When a clean class's field
///    set / capacity grows, the `classid` selects the (expanded) shape — no
///    global layout change (cf. RESERVE-DON'T-RECLAIM + the class-conditioned
///    [`CascadeShape`](crate::facet::CascadeShape)). Expansion is never a blocker.
/// 2. **Bulk raw data lives out-of-line — a *separate* Lance table, not this
///    480-byte value.** The value slab is for structured/compressible columns; a
///    raw payload that can't fit even compressed (a ~3.2 Gbp genome; the
///    FMA / BodyParts3D anatomy mesh at 4M vertices / 6M triangles) is its own
///    table, referenced by `key`/`classid` — and still not a blocker (the
///    anatomy mesh baked cleanly as a SoA release). The node stays 512 B; bulk
///    is addressed, not inlined.
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct NodeRow {
    pub key: NodeGuid,    //  0..16
    pub edges: EdgeBlock, // 16..32
    pub value: [u8; 480], // 32..512  (reserved — comes after)
}

// Sizes are part of the lock.
const _: () = assert!(core::mem::size_of::<NodeGuid>() == 16);
const _: () = assert!(core::mem::size_of::<EdgeBlock>() == 16);
const _: () = assert!(core::mem::size_of::<NodeRow>() == 512);

// ── SoaEnvelope binding for [NodeRow] ────────────────────────────────────────

use crate::class_view::FieldMask;
use crate::kanban::{ExecTarget, KanbanColumn};
use crate::qualia::QualiaI4_16D;
use crate::soa_envelope::{ColumnDescriptor, ColumnKind, SoaEnvelope};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Stable column-id ordinals for [`NodeRow`]'s three top-level slots.
/// `name_id` in the [`ColumnDescriptor`] table; the registry-resolved value
/// carve-out (per `classid → ClassView`) lives *inside* `Value` and is not
/// surfaced as its own envelope column — the canon contract is at this level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum NodeRowColumn {
    Key = 0,
    Edges = 1,
    Value = 2,
}

/// Canonical [`ColumnDescriptor`] table for [`NodeRow`].
///
/// Three columns, all `ColumnKind::U8` byte-arrays (their internal structure
/// is canon-described elsewhere — `NodeGuid` decomposes the key, `EdgeBlock`
/// the edges, registry `ClassView` carves the value side). The envelope
/// contract is at the row-stride level: bytes 0..16 are the key, 16..32 are
/// the edges, 32..512 are the class-resolved value slab. Sum = 512 = stride.
pub const NODE_ROW_COLUMNS: &[ColumnDescriptor] = &[
    ColumnDescriptor {
        name_id: NodeRowColumn::Key as u16,
        kind: ColumnKind::U8,
        elems_per_row: 16,
        row_offset: 0,
    },
    ColumnDescriptor {
        name_id: NodeRowColumn::Edges as u16,
        kind: ColumnKind::U8,
        elems_per_row: 16,
        row_offset: 16,
    },
    ColumnDescriptor {
        name_id: NodeRowColumn::Value as u16,
        kind: ColumnKind::U8,
        elems_per_row: 480,
        row_offset: 32,
    },
];

/// Row stride for [`NodeRow`] in bytes — equal to `size_of::<NodeRow>()`.
pub const NODE_ROW_STRIDE: usize = 512;

/// The node viewed as fixed-size **GUID slots**: `NODE_ROW_STRIDE /
/// size_of::<NodeGuid>()` = `512 / 16` = **32**. A 512-byte SoA row can carry up
/// to 32 GUID-sized (16-byte) entries; `key` + `edges` occupy 2, leaving the
/// 480-byte value slab = 30 slots for the class-resolved layout.
///
/// **Doctrine — clean / SoC over packed (operator, 2026-06-29).** When a class
/// needs more structure than fits cleanly in one slot, *Tetris it across the
/// slots* — give each concern its own 16-byte slot — rather than cram two
/// *distinct concerns* into one. The 32-slot capacity is *why* that cramming is
/// almost never needed — separation-of-concerns layout is the default, packing
/// the rare last resort. (This is also the headroom that lets a ClassView
/// *rotate* and lets the rare classid-stacking-entropy case spread to a fresh
/// slot instead of minting another classid.) The per-class *shape* of one
/// facet — [`CascadeShape`](crate::facet::CascadeShape) `6×2`/`4×3`/`3×4`,
/// selected by `classid` — is a separate, class-conditioned choice (a `4×3`
/// class is clean); this doctrine is about not mixing concerns, not about shape.
pub const GUIDS_PER_NODE: usize = NODE_ROW_STRIDE / 16;

const _: () = assert!(
    GUIDS_PER_NODE == 32 && GUIDS_PER_NODE * core::mem::size_of::<NodeGuid>() == NODE_ROW_STRIDE,
    "512-byte node = 32 × 16-byte GUID slots"
);

// ── Value-slab schema presets: which tenants a class materialises ─────────────

/// Full-row byte offset of the value slab (key 16 + edges 16).
pub const VALUE_SLAB_ROW_OFFSET: usize = 32;
/// Bytes available in the [`NodeRow::value`] slab.
pub const VALUE_SLAB_LEN: usize = 480;

/// A named tenant of the 480-byte [`NodeRow::value`] slab.
///
/// Stable, append-only positions — the canon "reserve, don't reclaim" rule and
/// the [`FieldMask`] N3 contract: a tenant's presence bit and its byte offset in
/// [`VALUE_TENANTS`] never move once instances persist, and retired tenants are
/// never reused. **The discriminant IS the [`FieldMask`] bit position** and the
/// index into [`VALUE_TENANTS`] (asserted at compile time below).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ValueTenant {
    /// `MetaWord` — thinking / awareness / NARS / free-energy bits.
    Meta = 0,
    /// `QualiaI4_16D` — 16 signed-4-bit chroma channels.
    Qualia = 1,
    /// The 4 out-of-family edges materialised as full `CausalEdge64`.
    MaterializedEdges = 2,
    /// `Fingerprint<256>` — 32-byte identity print.
    Fingerprint = 3,
    /// helix golden-spiral Place/Residue — signed full-sphere `Signed360`,
    /// 48-bit = 6 B (2× the 24-bit equal-area hemisphere; produced by the `helix`
    /// crate's `Signed360`, written here zero-copy).
    HelixResidue = 4,
    /// turbovec PQ residue ([`EdgeCodecFlavor::Pq32x4`], 16 B).
    TurbovecResidue = 5,
    /// Spatio-temporal accumulator (`f32`).
    Energy = 6,
    /// Hebbian plasticity counter + last-active stamp.
    Plasticity = 7,
    /// OGIT entity-type / class discriminator (`u16`).
    EntityType = 8,
    /// kanban×Rubicon phase cursor (8 B): `phase` (KanbanColumn) + `exec`
    /// (ExecTarget) + reserved + `cycle` (u32). The per-node Rubicon lifecycle
    /// column — owner-advanced (the `MailboxSoaOwner` split), surreal-read (the
    /// Rubicon "view never mutates" projection), q2 renders columns by `phase`.
    /// Pins SoA↔kanban in the LE blob at value-slab `[112,120)` (subsumes the
    /// envelope-pointer G1: the node carries its own phase + cycle).
    Kanban = 9,
}

impl ValueTenant {
    /// This tenant's byte offset **within the 480-byte value slab** (its row
    /// offset minus [`VALUE_SLAB_ROW_OFFSET`]). The companion to its
    /// [`VALUE_TENANTS`] descriptor — lets a transcode write into
    /// [`NodeRow::value`] without hardcoding the carve. Not a new property: a
    /// derived accessor over the already-locked, compile-asserted carve.
    #[inline]
    pub const fn value_offset(self) -> usize {
        VALUE_TENANTS[self as usize].row_offset as usize - VALUE_SLAB_ROW_OFFSET
    }

    /// This tenant's byte length in the slab (from its [`VALUE_TENANTS`] descriptor).
    #[inline]
    pub const fn byte_len(self) -> usize {
        VALUE_TENANTS[self as usize].col_bytes_per_row()
    }
}

/// Stable byte carve of the value slab. Offsets are **row-relative** (within one
/// row packet, in the value region `[32, 512)`) — consistent with
/// [`NODE_ROW_COLUMNS`], one level finer. Contiguous, in [`ValueTenant`]
/// discriminant order, no gaps; the Full set fits the slab (all asserted at
/// compile time below). This is the per-class carve the canon defers to
/// `ClassView`; it is NOT surfaced as its own top-level envelope column.
pub const VALUE_TENANTS: &[ColumnDescriptor] = &[
    ColumnDescriptor {
        name_id: ValueTenant::Meta as u16,
        kind: ColumnKind::U64,
        elems_per_row: 1,
        row_offset: 32,
    },
    ColumnDescriptor {
        name_id: ValueTenant::Qualia as u16,
        kind: ColumnKind::U64,
        elems_per_row: 1,
        row_offset: 40,
    },
    ColumnDescriptor {
        name_id: ValueTenant::MaterializedEdges as u16,
        kind: ColumnKind::U64,
        elems_per_row: 4,
        row_offset: 48,
    },
    ColumnDescriptor {
        name_id: ValueTenant::Fingerprint as u16,
        kind: ColumnKind::U8,
        elems_per_row: 32,
        row_offset: 80,
    },
    ColumnDescriptor {
        name_id: ValueTenant::HelixResidue as u16,
        kind: ColumnKind::U8,
        // 6 B = 48 bit = 2× the 24-bit equal-area hemisphere (helix `Signed360`,
        // signed full sphere). Was 48 B — a bits→bytes slip; right-sized 2026-06-15.
        elems_per_row: 6,
        row_offset: 112,
    },
    ColumnDescriptor {
        name_id: ValueTenant::TurbovecResidue as u16,
        kind: ColumnKind::U8,
        elems_per_row: 16,
        row_offset: 118,
    },
    ColumnDescriptor {
        name_id: ValueTenant::Energy as u16,
        kind: ColumnKind::F32,
        elems_per_row: 1,
        row_offset: 134,
    },
    ColumnDescriptor {
        name_id: ValueTenant::Plasticity as u16,
        kind: ColumnKind::U32,
        elems_per_row: 1,
        row_offset: 138,
    },
    ColumnDescriptor {
        name_id: ValueTenant::EntityType as u16,
        kind: ColumnKind::U16,
        elems_per_row: 1,
        row_offset: 142,
    },
    ColumnDescriptor {
        // kanban×Rubicon cursor: 8 B contiguous at row_offset 144 (value-slab
        // [112,120)); reserve-don't-reclaim, layout-preserving (Full ends 152 ≤ 480).
        name_id: ValueTenant::Kanban as u16,
        kind: ColumnKind::U64,
        elems_per_row: 1,
        row_offset: 144,
    },
];

// Compile-time canon: VALUE_TENANTS is discriminant-ordered, contiguous within the
// value slab, and the Full carve fits the 480-byte slab.
const _: () = {
    let mut i = 0usize;
    let mut prev_end = VALUE_SLAB_ROW_OFFSET;
    while i < VALUE_TENANTS.len() {
        let c = &VALUE_TENANTS[i];
        assert!(
            c.name_id as usize == i,
            "ValueTenant discriminant must equal its VALUE_TENANTS index"
        );
        assert!(
            c.row_offset as usize == prev_end,
            "VALUE_TENANTS must be contiguous within the value slab (no gaps/overlap)"
        );
        prev_end = c.row_offset as usize + c.col_bytes_per_row();
        i += 1;
    }
    assert!(
        prev_end <= NODE_ROW_STRIDE,
        "value tenants must fit within the 512-byte row"
    );
    assert!(
        prev_end - VALUE_SLAB_ROW_OFFSET <= VALUE_SLAB_LEN,
        "value tenants must fit the 480-byte slab"
    );
};

/// Which value-slab schema a class materialises — the value-side analog of
/// [`EdgeCodecFlavor`]. A preset is a presence [`FieldMask`] over [`ValueTenant`]
/// positions; a class selects it via
/// [`ClassView::value_schema`](crate::class_view::ClassView::value_schema).
///
/// **Layout-preserving:** every preset carves WITHIN the reserved 480-byte value
/// slab, so the choice never changes [`NODE_ROW_STRIDE`] (no
/// `ENVELOPE_LAYOUT_VERSION` bump — canon "registry-resolved via
/// `classid → ClassView`", never a stride change). [`Bootstrap`] is the
/// zero-fallback default: value all zero, only key + edges meaningful.
///
/// [`Bootstrap`]: ValueSchema::Bootstrap
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ValueSchema {
    /// Empty value slab — the canon zero-fallback (key + edges only).
    #[default]
    Bootstrap = 0,
    /// Hot self-thinking set: Meta + Qualia + Fingerprint + Energy + Plasticity +
    /// EntityType. No materialised edges, no codec residues.
    Cognitive = 1,
    /// Cold / compressed codec stack: Fingerprint + Helix `Signed360` (6 B) +
    /// turbovec residue + EntityType. No hot lifecycle columns.
    Compressed = 2,
    /// Every [`ValueTenant`] materialised — the densest node.
    Full = 3,
}

impl ValueSchema {
    /// The presence [`FieldMask`] over [`ValueTenant`] positions for this preset.
    pub const fn field_mask(self) -> FieldMask {
        match self {
            ValueSchema::Bootstrap => FieldMask::EMPTY,
            ValueSchema::Cognitive => FieldMask::from_positions(&[
                ValueTenant::Meta as u8,
                ValueTenant::Qualia as u8,
                ValueTenant::Fingerprint as u8,
                ValueTenant::Energy as u8,
                ValueTenant::Plasticity as u8,
                ValueTenant::EntityType as u8,
                ValueTenant::Kanban as u8,
            ]),
            ValueSchema::Compressed => FieldMask::from_positions(&[
                ValueTenant::Fingerprint as u8,
                ValueTenant::HelixResidue as u8,
                ValueTenant::TurbovecResidue as u8,
                ValueTenant::EntityType as u8,
            ]),
            ValueSchema::Full => FieldMask::from_positions(&[
                ValueTenant::Meta as u8,
                ValueTenant::Qualia as u8,
                ValueTenant::MaterializedEdges as u8,
                ValueTenant::Fingerprint as u8,
                ValueTenant::HelixResidue as u8,
                ValueTenant::TurbovecResidue as u8,
                ValueTenant::Energy as u8,
                ValueTenant::Plasticity as u8,
                ValueTenant::EntityType as u8,
                ValueTenant::Kanban as u8,
            ]),
        }
    }

    /// Does this preset materialise `tenant`?
    #[inline]
    pub const fn has(self, tenant: ValueTenant) -> bool {
        self.field_mask().has(tenant as u8)
    }

    /// Total bytes this preset occupies in the value slab (Σ present tenants).
    pub const fn tenant_bytes(self) -> usize {
        let mask = self.field_mask();
        let mut total = 0usize;
        let mut i = 0usize;
        while i < VALUE_TENANTS.len() {
            let c = &VALUE_TENANTS[i];
            if mask.has(c.name_id as u8) {
                total += c.col_bytes_per_row();
            }
            i += 1;
        }
        total
    }

    /// Every preset carves within the reserved 480-byte slab — none changes
    /// [`NODE_ROW_STRIDE`], so none forces an `ENVELOPE_LAYOUT_VERSION` bump.
    #[inline]
    pub const fn is_layout_preserving(self) -> bool {
        true
    }
}

// Compile-time canon: the densest preset fits the slab; Full covers every tenant;
// Bootstrap is empty.
const _: () = assert!(ValueSchema::Full.tenant_bytes() <= VALUE_SLAB_LEN);
const _: () = assert!(ValueSchema::Full.field_mask().count() as usize == VALUE_TENANTS.len());
const _: () = assert!(ValueSchema::Bootstrap.field_mask().is_empty());

// ── classid → read-mode: the LE contract both the consumer and OGAR inherit ────

/// Which tail / identity shape a class uses to *read* its node's 16-byte key —
/// the key-side analog of [`ValueSchema`] (value slab) and [`EdgeCodecFlavor`]
/// (edge block). It is the THIRD axis of OGAR #128's reusable envelope parser
/// (`E-CLASSID-ENVELOPE-PARSER`): `classid → {tail_variant, value_schema,
/// edge_codec}`, resolved by the SAME [`classid_read_mode`] registry, so the
/// key-side read is symmetric with the value-side (minting consults
/// `tail_variant`; [`NodeRow`] value transcode consults `value_schema`).
///
/// **Layout-preserving:** every variant re-interprets the SAME 16 key bytes —
/// `family·identity` vs `leaf·family·identity` vs the cascade `(part_of:is_a)`
/// tile are all readings of bytes 10..16, never a stride change (no
/// `ENVELOPE_LAYOUT_VERSION` bump — canon "registry-resolved via
/// `classid → ClassView`"). [`V1`] is the zero-fallback default: the canonical
/// original tail that [`NodeGuid::new`] mints.
///
/// [`V1`]: TailVariant::V1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum TailVariant {
    /// The canonical original tail `family(u24)·identity(u24)` (bytes 10..16) —
    /// what [`NodeGuid::new`] mints. The canon zero-fallback default.
    #[default]
    V1 = 0,
    /// The v2 basin tail `leaf(u16)·family(u16)·identity(u16)` — what
    /// [`NodeGuid::new_v2`] mints (feature `guid-v2-tail`, leaf as the 4th HHTL
    /// tier).
    V2 = 1,
    /// The new-generation `cascade_key` tail: the `(part_of:is_a)` 8:8 tile (the
    /// two-axis cascade key). Feature `guid-v3-tail`.
    V3 = 2,
}

impl TailVariant {
    /// Every variant re-interprets the SAME 16 key bytes (the tail is a reading
    /// of bytes 10..16, never a stride change), so no variant forces an
    /// `ENVELOPE_LAYOUT_VERSION` bump — the canon invariant, encoded so a
    /// regression test can assert it.
    #[inline]
    pub const fn is_layout_preserving(self) -> bool {
        true
    }
}

/// The **read mode** a `classid` resolves to: the trio of *already-existing*
/// read-mode axes — [`TailVariant`] (which key/identity shape to read),
/// [`ValueSchema`] (which value tenants to materialise) and [`EdgeCodecFlavor`]
/// (how to read the 16-byte edge block).
///
/// It is NOT a new node property and NOT a SoA column — nothing is stored on the
/// row. This is the *resolution result* (the lens): the value-side analog of
/// "which XSD parses this document". §0 anti-invention — it bundles the three
/// read-mode enums that already exist, adding zero new fields to the node.
/// There is NO public `new_v3` dispatch — the [`tail_variant`](ReadMode::tail_variant)
/// registry field IS the mechanism, symmetric with
/// [`value_schema`](ReadMode::value_schema).
///
/// Both consumers and OGAR resolve `classid → ReadMode` through the one
/// [`LazyLock`] registry ([`classid_read_mode`]), so the LE interpretation of a
/// node's bytes is single-sourced: a consumer transcoding a [`NodeRow`] and OGAR
/// minting/projecting the same class read the identical schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReadMode {
    /// Which tail / identity shape this class reads from its 16-byte key (the
    /// FIRST axis — resolved upstream of the value slab per OGAR #128's parse
    /// order: `classid → {tail_variant, value_schema, edge_codec}`).
    pub tail_variant: TailVariant,
    /// Which value-slab tenants this class materialises.
    pub value_schema: ValueSchema,
    /// How this class reads its 16-byte edge block.
    pub edge_codec: EdgeCodecFlavor,
}

impl ReadMode {
    /// The zero-fallback / POC default an *unconfigured* classid resolves to.
    ///
    /// `tail_variant = V1` is the conservative legacy default (L1: every
    /// un-minted classid stays [`TailVariant::V1`] → zero re-mint of the V1/V2
    /// corpus, RESERVE-DON'T-RECLAIM). It is **latent, not behavioural** here —
    /// nothing reads `tail_variant` yet, so V1 is a pin, not a key reformat. The
    /// precise per-classid legacy tail (e.g. an OSINT class that mints via
    /// [`NodeGuid::new_v2`] = [`TailVariant::V2`]) is fixed when the reusable
    /// envelope parser dispatches per classid — not by this conservative default.
    ///
    /// **TEMPORARY (2026-06-15 POC):** `value_schema = Full` mirrors the
    /// [`ClassView::value_schema`](crate::class_view::ClassView::value_schema)
    /// POC default so an unconfigured class materialises the whole slab for
    /// transcode; `edge_codec = CoarseOnly` is the canon zero-fallback edge
    /// reading. When the POC ends, flip `value_schema` back to
    /// [`ValueSchema::Bootstrap`] HERE and in `ClassView` together (one revert,
    /// two sites — the test `read_mode_default_is_full_poc` guards the pairing).
    pub const DEFAULT: ReadMode = ReadMode {
        tail_variant: TailVariant::V1,
        value_schema: ValueSchema::Full,
        edge_codec: EdgeCodecFlavor::CoarseOnly,
    };

    /// The **OSINT / Palantir-Gotham** read-mode ([`NodeGuid::CLASSID_OSINT`]):
    /// a *hot* entity graph — [`ValueSchema::Cognitive`] (Meta + Qualia +
    /// Fingerprint + Energy + Plasticity + EntityType, for live NARS reasoning)
    /// over [`EdgeCodecFlavor::CoarseOnly`] adjacency (the 12 in-family + 4
    /// out-of-family slots read literally as the neo4j-emulation edges).
    pub const OSINT: ReadMode = ReadMode {
        tail_variant: TailVariant::V1,
        value_schema: ValueSchema::Cognitive,
        edge_codec: EdgeCodecFlavor::CoarseOnly,
    };

    /// The **FMA anatomy** read-mode ([`NodeGuid::CLASSID_FMA`]): a *cold*
    /// structural reference graph — [`ValueSchema::Compressed`] (Fingerprint +
    /// Helix + Turbovec + EntityType; no hot lifecycle columns, it is static
    /// reference data) over [`EdgeCodecFlavor::CoarseOnly`] part-of adjacency.
    pub const FMA: ReadMode = ReadMode {
        tail_variant: TailVariant::V1,
        value_schema: ValueSchema::Compressed,
        edge_codec: EdgeCodecFlavor::CoarseOnly,
    };

    /// The **project-management** read-mode ([`NodeGuid::CLASSID_PROJECT`],
    /// OpenProject ↔ Redmine): a *hot* work-item graph — [`ValueSchema::Cognitive`]
    /// (live lifecycle: status / assignee / version edges queried + reasoned over)
    /// with [`EdgeCodecFlavor::CoarseOnly`] adjacency (parent / blocks / relates).
    pub const PROJECT: ReadMode = ReadMode {
        tail_variant: TailVariant::V1,
        value_schema: ValueSchema::Cognitive,
        edge_codec: EdgeCodecFlavor::CoarseOnly,
    };

    /// The **commerce / ERP** read-mode ([`NodeGuid::CLASSID_ERP`], Odoo ↔ OSB):
    /// a *hot* transactional graph — [`ValueSchema::Cognitive`] (invoices / taxes /
    /// partners / payments queried live) with [`EdgeCodecFlavor::CoarseOnly`]
    /// adjacency (partner-of / line-of / paid-by).
    pub const ERP: ReadMode = ReadMode {
        tail_variant: TailVariant::V1,
        value_schema: ValueSchema::Cognitive,
        edge_codec: EdgeCodecFlavor::CoarseOnly,
    };

    /// The **OSINT-V3** read-mode ([`NodeGuid::CLASSID_OSINT_V3`]): the same hot
    /// [`ValueSchema::Cognitive`] value model as legacy [`OSINT`](ReadMode::OSINT),
    /// read through the new-generation [`TailVariant::V3`] cascade tail. The first
    /// V3 exemplar; [`FMA_V3`](ReadMode::FMA_V3) + [`CPIC_V3`](ReadMode::CPIC_V3)
    /// complete the Phase-1 V3 set.
    #[cfg(feature = "guid-v3-tail")]
    pub const OSINT_V3: ReadMode = ReadMode {
        tail_variant: TailVariant::V3,
        value_schema: ValueSchema::Cognitive,
        edge_codec: EdgeCodecFlavor::CoarseOnly,
    };

    /// The **FMA-V3** read-mode ([`NodeGuid::CLASSID_FMA_V3`]): the same cold
    /// [`ValueSchema::Compressed`] value model as legacy [`FMA`](ReadMode::FMA),
    /// read through the new-generation [`TailVariant::V3`] cascade tail.
    #[cfg(feature = "guid-v3-tail")]
    pub const FMA_V3: ReadMode = ReadMode {
        tail_variant: TailVariant::V3,
        value_schema: ValueSchema::Compressed,
        edge_codec: EdgeCodecFlavor::CoarseOnly,
    };

    /// The **CPIC-V3** read-mode ([`NodeGuid::CLASSID_CPIC_V3`], Genetics domain):
    /// CPIC pharmacogenomics on a [`TailVariant::V3`] cascade tail. The value model
    /// [`ValueSchema::Compressed`] is the **Phase-1 provisional** (biomedical
    /// reference, mirroring FMA's cold treatment); Phase 2 pins the V3-shaped
    /// tenants. Edges [`EdgeCodecFlavor::CoarseOnly`].
    #[cfg(feature = "guid-v3-tail")]
    pub const CPIC_V3: ReadMode = ReadMode {
        tail_variant: TailVariant::V3,
        value_schema: ValueSchema::Compressed,
        edge_codec: EdgeCodecFlavor::CoarseOnly,
    };

    /// All three axes are layout-preserving (a tail-variant/preset/flavor
    /// re-interprets reserved bytes, never a stride change), so adopting any
    /// read-mode needs no `ENVELOPE_LAYOUT_VERSION` bump.
    #[inline]
    pub const fn is_layout_preserving(self) -> bool {
        self.tail_variant.is_layout_preserving()
            && self.value_schema.is_layout_preserving()
            && self.edge_codec.is_layout_preserving()
    }
}

/// Structural parity fuse — names all THREE [`ReadMode`] axes so a field
/// add/remove is a compile error. This is the structural-against-canon guard
/// vs OGAR #128's (`E-CLASSID-ENVELOPE-PARSER`) `{tail_variant, value_schema,
/// edge_codec}` tuple. OGAR #128 is doc-only today, so there is no runtime OGAR
/// struct to compare against yet; this upgrades to a runtime fuse when OGAR
/// codes its registry's `tail_variant`.
const _: ReadMode = ReadMode {
    tail_variant: TailVariant::V1,
    value_schema: ValueSchema::Bootstrap,
    edge_codec: EdgeCodecFlavor::CoarseOnly,
};

/// Builtin `classid → ReadMode` registry, built once on first use.
///
/// Immutable after init — the canon "already-immutable ontology registry" shape,
/// the same [`LazyLock`] pattern `lance-graph-ontology` uses for its seed
/// namespace registry. Holds only the canon builtins; a minted class's read-mode
/// is layered in by OGAR one level up. Any classid NOT in the map falls through
/// to [`ReadMode::DEFAULT`] — the same zero-fallback ladder as the key itself
/// (`classid 0 ⇒ default class`).
static BUILTIN_READ_MODES: LazyLock<HashMap<u32, ReadMode>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // The canon default class materialises the POC-Full slab (see ReadMode::DEFAULT).
    m.insert(NodeGuid::CLASSID_DEFAULT, ReadMode::DEFAULT);
    // OSINT/Gotham (hot entity graph) + FMA anatomy (cold structural reference) —
    // the two registered graph domains (see `soa_graph`). Both read edges as
    // CoarseOnly adjacency; they differ in the value schema (hot vs cold).
    m.insert(NodeGuid::CLASSID_OSINT, ReadMode::OSINT);
    m.insert(NodeGuid::CLASSID_FMA, ReadMode::FMA);
    // Project-management (OpenProject ↔ Redmine) + commerce/ERP (Odoo ↔ OSB) —
    // the OGAR `0x01XX` / `0x02XX` domains; both hot business graphs (Cognitive).
    m.insert(NodeGuid::CLASSID_PROJECT, ReadMode::PROJECT);
    m.insert(NodeGuid::CLASSID_ERP, ReadMode::ERP);
    // LEGACY ALIASES (flip P1/P3, codex P2 #627): the pre-flip stored forms
    // (canon in the LOW half) resolve to the SAME read modes so persisted
    // pre-flip rows keep reading correctly — mint-forward, never blanket
    // reinterpretation. Read-only: do NOT mint with these keys. Retirement is
    // a later step gated on a corpus proof that zero old-form rows remain.
    m.insert(NodeGuid::CLASSID_OSINT_LEGACY, ReadMode::OSINT);
    m.insert(NodeGuid::CLASSID_FMA_LEGACY, ReadMode::FMA);
    m.insert(NodeGuid::CLASSID_PROJECT_LEGACY, ReadMode::PROJECT);
    m.insert(NodeGuid::CLASSID_ERP_LEGACY, ReadMode::ERP);
    // V3 cascade-key classes (feature `guid-v3-tail`): same value model as their
    // legacy domain, on a TailVariant::V3 tail. Since the P1 flip the canon
    // (`domain:appid`) is the HIGH u16 (`0x0701_1000`-form), so
    // `classid_concept_domain` routes Osint / Anatomy / Genetics directly off
    // the canon half; the pre-flip `0x1000_DDCC` forms stay as aliases.
    #[cfg(feature = "guid-v3-tail")]
    {
        m.insert(NodeGuid::CLASSID_OSINT_V3, ReadMode::OSINT_V3);
        m.insert(NodeGuid::CLASSID_FMA_V3, ReadMode::FMA_V3);
        m.insert(NodeGuid::CLASSID_CPIC_V3, ReadMode::CPIC_V3);
        m.insert(NodeGuid::CLASSID_OSINT_V3_LEGACY, ReadMode::OSINT_V3);
        m.insert(NodeGuid::CLASSID_FMA_V3_LEGACY, ReadMode::FMA_V3);
        m.insert(NodeGuid::CLASSID_CPIC_V3_LEGACY, ReadMode::CPIC_V3);
    }
    m
});

/// Resolve a `classid` to its [`ReadMode`] — the single source both consumers
/// and OGAR inherit. Reads the [`BUILTIN_READ_MODES`] registry, falling through
/// to [`ReadMode::DEFAULT`] for any unconfigured classid (the key's own
/// zero-fallback ladder). [`NodeGuid::read_mode`] is the carrier-method form.
#[inline]
pub fn classid_read_mode(classid: u32) -> ReadMode {
    BUILTIN_READ_MODES
        .get(&classid)
        .copied()
        .unwrap_or(ReadMode::DEFAULT)
}

/// Zero-copy [`SoaEnvelope`] wrapper over a contiguous slice of [`NodeRow`].
///
/// `NodeRow` is `#[repr(C, align(64))]` with the locked 16/16/480 byte
/// layout, so a `&[NodeRow]` IS already a row-strided LE packet at stride
/// 512 — no allocation, no copy. This wrapper just attaches the cycle stamp
/// and exposes the slice through the [`SoaEnvelope`] trait so Lance's
/// columnar I/O reads it directly.
///
/// The envelope's column table ([`NODE_ROW_COLUMNS`]) names the three
/// top-level slots (key / edges / value). Internal structure within each
/// slot is the canon's concern (`NodeGuid` for the key, `EdgeBlock` for the
/// edges, registry `ClassView` for the value carve-out).
#[derive(Clone, Copy)]
pub struct NodeRowPacket<'a> {
    rows: &'a [NodeRow],
    cycle: u32,
}

impl<'a> core::fmt::Debug for NodeRowPacket<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NodeRowPacket")
            .field("n_rows", &self.rows.len())
            .field("cycle", &self.cycle)
            .field("row_stride", &NODE_ROW_STRIDE)
            .finish()
    }
}

impl<'a> NodeRowPacket<'a> {
    /// Wrap a contiguous slice of [`NodeRow`] with a cycle stamp.
    #[inline]
    pub const fn new(rows: &'a [NodeRow], cycle: u32) -> Self {
        Self { rows, cycle }
    }

    /// The underlying rows.
    #[inline]
    pub const fn rows(&self) -> &'a [NodeRow] {
        self.rows
    }
}

impl<'a> SoaEnvelope for NodeRowPacket<'a> {
    fn columns(&self) -> &[ColumnDescriptor] {
        NODE_ROW_COLUMNS
    }
    fn row_stride(&self) -> usize {
        NODE_ROW_STRIDE
    }
    fn n_rows(&self) -> usize {
        self.rows.len()
    }
    fn cycle(&self) -> u32 {
        self.cycle
    }
    fn as_le_bytes(&self) -> &[u8] {
        // SAFETY: NodeRow is #[repr(C, align(64))] with size_of::<NodeRow>() ==
        // 512 (checked by the const _: () asserts above). A &[NodeRow] is a
        // contiguous array of #[repr(C)] structs; viewing it as &[u8] of
        // length len * 512 is a standard column-store packing operation, and
        // every byte position is valid for reads (no padding past size_of,
        // alignment of NodeRow (64) ⊇ alignment of u8 (1)).
        //
        // The NodeGuid and EdgeBlock fields hold their bytes in canon-LE
        // order (NodeGuid::new uses to_le_bytes; EdgeBlock is plain [u8;_]),
        // so the resulting byte slice IS the envelope's LE packet — no
        // translation needed at the boundary.
        unsafe {
            core::slice::from_raw_parts(
                self.rows.as_ptr().cast::<u8>(),
                self.rows.len() * NODE_ROW_STRIDE,
            )
        }
    }
}

/// Zero-copy **read** of a [`NodeRow`] slice out of an external LE byte buffer —
/// the inverse of [`NodeRowPacket::as_le_bytes`] and the load-bearing primitive
/// for "a store hands lance-graph its SoA view without a copy."
///
/// This is the **LE contract a backing store satisfies**: hand a byte slice that
/// is (a) a whole number of 512-byte rows and (b) aligned to
/// `align_of::<NodeRow>()` (64), and you get back a `&[NodeRow]` viewing the SAME
/// bytes — no allocation, no deserialize. Returns `None` if either invariant
/// fails (a sliced/offset buffer that lost 64-byte alignment, or a length that
/// isn't a multiple of the stride), so the caller can fall back to a copy rather
/// than risk UB.
///
/// Intended consumer: a Lance-backed key-value store (e.g. surrealdb's `kv-lance`)
/// that persists each node as a fixed-size 512-byte LE blob
/// (`arrow::FixedSizeBinary(512)`, whose value buffer arrow-rs allocates 64-byte
/// aligned). The store's value buffer is then directly a `&[NodeRow]` the
/// cognitive shader reads in place — surrealdb's bytes ARE the SoA. (A
/// *variable-length* `Binary` column does NOT qualify: it has no fixed stride and
/// no alignment guarantee; the store must use `FixedSizeBinary(512)` for the SoA
/// value path. And the buffer must be uncompressed for the read to be literally
/// zero-copy — a Lance-compressed column decodes to a contiguous buffer first,
/// which is one copy, still no per-field deserialize.)
///
/// The bytes are interpreted in canon-LE order exactly as [`NodeGuid`]/[`EdgeBlock`]
/// wrote them, so no endianness translation happens at the boundary.
#[inline]
#[must_use]
pub fn node_rows_from_le_bytes(bytes: &[u8]) -> Option<&[NodeRow]> {
    if bytes.is_empty() {
        return Some(&[]);
    }
    if !bytes.len().is_multiple_of(NODE_ROW_STRIDE) {
        return None;
    }
    if !(bytes.as_ptr() as usize).is_multiple_of(core::mem::align_of::<NodeRow>()) {
        return None;
    }
    let n = bytes.len() / NODE_ROW_STRIDE;
    // SAFETY: NodeRow is #[repr(C, align(64))], size_of == 512 == NODE_ROW_STRIDE
    // (const-asserted above). We checked (1) bytes.len() is an exact multiple of
    // the stride, so n rows span the whole slice with no trailing bytes, and (2)
    // the pointer is aligned to align_of::<NodeRow>() (64). Every bit pattern in
    // the 512 bytes is a valid NodeRow (NodeGuid is bytes, EdgeBlock is [u8;16],
    // value is [u8;480] — no niche/enum to invalidate), so the reinterpretation
    // is sound. The returned slice borrows `bytes` for its lifetime (no copy).
    Some(unsafe { core::slice::from_raw_parts(bytes.as_ptr().cast::<NodeRow>(), n) })
}

// ── kanban×Rubicon value tenant (the per-node phase cursor) ───────────────────

/// The kanban×Rubicon phase cursor stored in [`ValueTenant::Kanban`] — 8 bytes at
/// value-slab `[112, 120)`, LE: `phase(u8) | exec(u8) | reserved(u16) | cycle(u32)`.
///
/// Per-node Rubicon lifecycle: `phase` advances along the [`KanbanColumn`] DAG
/// (**owner-only** write — the `MailboxSoaOwner`/`View` split is what makes "the
/// view never mutates the SoA" a compile-time guarantee), `exec` names the
/// dispatch backend ([`ExecTarget`]), `cycle` is the owner's `current_cycle`
/// stamp. A `Copy` microcopy: read zero-copy from the slab, written only on phase
/// advance. surrealdb projects it as the kanban view (read-only, Rubicon); q2
/// renders columns by `phase`. Because it lives IN the node, SoA↔kanban is pinned
/// in the 512-byte LE blob — no separate envelope pointer needed (subsumes G1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KanbanTenant {
    /// Rubicon lifecycle column.
    pub phase: KanbanColumn,
    /// Dispatch backend the planner selected.
    pub exec: ExecTarget,
    /// Owner `current_cycle` stamp at the last phase write.
    pub cycle: u32,
}

impl KanbanTenant {
    /// Decode from the 8 tenant bytes (LE; unknown phase/exec discriminants fall
    /// back to their zero defaults via [`KanbanColumn::from_u8`]/[`ExecTarget::from_u8`]).
    #[inline]
    #[must_use]
    pub fn from_bytes(b: [u8; 8]) -> Self {
        Self {
            phase: KanbanColumn::from_u8(b[0]),
            exec: ExecTarget::from_u8(b[1]),
            // b[2..4] reserved
            cycle: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        }
    }

    /// Encode to the 8 tenant bytes (LE).
    #[inline]
    #[must_use]
    pub fn to_bytes(self) -> [u8; 8] {
        let c = self.cycle.to_le_bytes();
        [
            self.phase as u8,
            self.exec as u8,
            0,
            0,
            c[0],
            c[1],
            c[2],
            c[3],
        ]
    }
}

// `FacetTier` / `FacetCascade` live in the dedicated [`crate::facet`] module — a
// reusable, content-blind 8:8 substrate (a *reading* over borrowed bytes, NOT part of
// the locked node layout). Re-exported here for the historical `canonical_node` path.
pub use crate::facet::{FacetCascade, FacetTier};

impl NodeRow {
    /// Read the [`KanbanTenant`] phase cursor from the [`ValueTenant::Kanban`]
    /// slab bytes — zero-copy decode, `Copy` result. The per-node Rubicon phase.
    #[inline]
    #[must_use]
    pub fn kanban(&self) -> KanbanTenant {
        let o = ValueTenant::Kanban.value_offset();
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.value[o..o + 8]);
        KanbanTenant::from_bytes(b)
    }

    /// Write the [`KanbanTenant`] into the slab. **Owner-only by convention** (the
    /// `MailboxSoaOwner` phase-advance path); reads stay zero-copy. Bumps the
    /// `Kanban` per-tenant update counter (no-op unless `tenant-counters`).
    #[inline]
    pub fn set_kanban(&mut self, k: KanbanTenant) {
        let o = ValueTenant::Kanban.value_offset();
        self.value[o..o + 8].copy_from_slice(&k.to_bytes());
        crate::tenant_counter::tenant_update(ValueTenant::Kanban);
    }

    /// Read the [`ValueTenant::Qualia`] slab bytes as a [`QualiaI4_16D`] — the 16
    /// signed-4-bit chroma channels (flow / trust / coherence …), zero-copy decode
    /// (`QualiaI4_16D` is a `u64`; the tenant is its 8 LE bytes).
    #[inline]
    #[must_use]
    pub fn qualia(&self) -> QualiaI4_16D {
        let o = ValueTenant::Qualia.value_offset();
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.value[o..o + 8]);
        QualiaI4_16D(u64::from_le_bytes(b))
    }

    /// **S2 — the MUL → phase seam** (capstone `cognitive-loop-wiring` plan).
    /// Reads this node's `Qualia` tenant + current `Kanban` phase, runs the
    /// zero-dep MUL gate ([`mul::i4_eval::gate_decision_i4`](crate::mul::i4_eval::gate_decision_i4),
    /// flow-vs-mismatch over the qualia + the signed `mantissa`), and returns the
    /// **target [`KanbanColumn`]** to advance to — or `None` to hold.
    ///
    /// Returns the *phase decision only*, NOT a full `KanbanTenant`: the owner
    /// stamps `cycle` + `exec` at write time, i.e.
    /// `set_kanban(KanbanTenant { phase, exec, cycle: <owner current_cycle> })`.
    /// It must use the OWNER'S current cycle — reusing the node's stored `cycle`
    /// would mark a later-cycle advance as stale (codex P2 #566). **Pure read**
    /// (no `&mut` during compute, per the borrow-strategy rule). `mantissa` is the
    /// inference strength (e.g. `causal_edge::InferenceType::to_mantissa()` / a
    /// Meta signal).
    #[inline]
    #[must_use]
    pub fn mul_phase_step(&self, mantissa: i8) -> Option<KanbanColumn> {
        let gate = crate::mul::i4_eval::gate_decision_i4(&self.qualia(), mantissa);
        self.kanban().phase.advance_on_gate(&gate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanban_tenant_round_trip_and_field_isolation() {
        let mut row = NodeRow {
            key: NodeGuid::local(1),
            edges: EdgeBlock::default(),
            value: [0xABu8; 480],
        };
        let before = row.value;
        let k = KanbanTenant {
            phase: KanbanColumn::CognitiveWork,
            exec: ExecTarget::SurrealQl,
            cycle: 0xDEAD_BEEF,
        };
        row.set_kanban(k);
        assert_eq!(row.kanban(), k, "kanban tenant round-trips");
        let o = ValueTenant::Kanban.value_offset();
        assert_eq!(o, 112, "kanban tenant at value-slab [112,120)");
        for (i, (&now, &was)) in row.value.iter().zip(before.iter()).enumerate() {
            if (o..o + 8).contains(&i) {
                continue;
            }
            assert_eq!(
                now, was,
                "byte {i} outside the kanban tenant must not change"
            );
        }
    }

    #[test]
    fn kanban_in_cognitive_and_full_schemas() {
        assert!(ValueSchema::Cognitive.has(ValueTenant::Kanban));
        assert!(ValueSchema::Full.has(ValueTenant::Kanban));
        assert!(!ValueSchema::Bootstrap.has(ValueTenant::Kanban));
        assert!(ValueSchema::Full.tenant_bytes() <= VALUE_SLAB_LEN);
        assert_eq!(ValueTenant::Kanban.byte_len(), 8);
    }

    #[test]
    fn s2_mul_phase_step_flow_advances_block_prunes_hold_stays() {
        // S2 seam probe (capstone): node Qualia tenant → MUL gate → kanban phase.
        let qo = ValueTenant::Qualia.value_offset();
        let mk = |q: QualiaI4_16D, phase: KanbanColumn| {
            let mut row = NodeRow {
                key: NodeGuid::local(1),
                edges: EdgeBlock::default(),
                value: [0u8; 480],
            };
            row.value[qo..qo + 8].copy_from_slice(&q.0.to_le_bytes());
            row.set_kanban(KanbanTenant {
                phase,
                ..Default::default()
            });
            row
        };
        // Flow qualia (warmth+groundedness high, low tension, calibrated) + mantissa>0
        // → GateDecision::Flow → forward advance Planning → CognitiveWork.
        // (Returns the target phase only; the owner stamps cycle+exec at write.)
        let flow_q = QualiaI4_16D(0).with(3, 4).with(14, 3).with(9, 4).with(1, 2);
        assert_eq!(
            mk(flow_q, KanbanColumn::Planning).mul_phase_step(4),
            Some(KanbanColumn::CognitiveWork),
        );
        // Block qualia (coherence very low + tension high → Uncertain) → Block →
        // Prune (the legal Libet veto edge at Planning).
        let block_q = QualiaI4_16D(0).with(9, -4).with(2, 4);
        assert_eq!(
            mk(block_q, KanbanColumn::Planning).mul_phase_step(0),
            Some(KanbanColumn::Prune),
        );
        // Block mid-CognitiveWork: no Prune successor in the DAG → hold (None).
        assert!(mk(block_q, KanbanColumn::CognitiveWork)
            .mul_phase_step(0)
            .is_none());
        // Neutral qualia + mantissa 0 → Boredom/Calibrated → Hold → None (stay).
        assert!(mk(QualiaI4_16D(0), KanbanColumn::Planning)
            .mul_phase_step(0)
            .is_none());
    }

    #[test]
    fn node_rows_le_bytes_round_trip_zero_copy() {
        // Build a small SoA, view it as LE bytes (the write path), then read it
        // back as &[NodeRow] (the inverse) — same bytes, no copy.
        let rows = vec![
            NodeRow {
                key: NodeGuid::new(NodeGuid::CLASSID_OSINT, 1, 2, 3, 0xAB, 0xCD),
                edges: EdgeBlock::default(),
                value: [7u8; 480],
            },
            NodeRow {
                key: NodeGuid::new(NodeGuid::CLASSID_PROJECT, 4, 5, 6, 0x11, 0x22),
                edges: EdgeBlock::default(),
                value: [9u8; 480],
            },
        ];
        let packet = NodeRowPacket::new(&rows, 0);
        let bytes = packet.as_le_bytes();
        assert_eq!(bytes.len(), 2 * NODE_ROW_STRIDE);

        let view = node_rows_from_le_bytes(bytes).expect("aligned, 512-multiple");
        assert_eq!(view.len(), 2);
        assert_eq!(view[0].key.classid(), NodeGuid::CLASSID_OSINT);
        assert_eq!(view[1].key.classid(), NodeGuid::CLASSID_PROJECT);
        assert_eq!(view[0].value, [7u8; 480]);
        // Truly zero-copy: the view aliases the SAME backing store as `rows`.
        assert_eq!(view.as_ptr().cast::<u8>(), rows.as_ptr().cast::<u8>());
    }

    #[test]
    fn node_rows_from_le_bytes_rejects_bad_inputs() {
        let rows = vec![
            NodeRow {
                key: NodeGuid::local(1),
                edges: EdgeBlock::default(),
                value: [0u8; 480],
            },
            NodeRow {
                key: NodeGuid::local(2),
                edges: EdgeBlock::default(),
                value: [0u8; 480],
            },
        ];
        let packet = NodeRowPacket::new(&rows, 0);
        let bytes = packet.as_le_bytes(); // 1024 bytes, 64-aligned
                                          // empty → Some(empty)
        assert_eq!(node_rows_from_le_bytes(&[]).map(<[_]>::len), Some(0));
        // not a whole number of rows → None (length check)
        assert!(node_rows_from_le_bytes(&bytes[..NODE_ROW_STRIDE - 1]).is_none());
        // a 512-length window offset by 1 off the 64-aligned base: correct length
        // but misaligned → None via the alignment check (no UB cast).
        let misaligned = &bytes[1..1 + NODE_ROW_STRIDE];
        assert_eq!(misaligned.len(), NODE_ROW_STRIDE);
        assert!(node_rows_from_le_bytes(misaligned).is_none());
    }

    #[test]
    fn defaults_are_zero_and_bootstrap() {
        let g = NodeGuid::local(0x00_00CD);
        assert_eq!(g.classid(), 0x0000_0000);
        assert_eq!(g.family(), 0x00_0000);
        assert!(g.is_default_class());
        assert!(g.is_unbasined());
        assert!(g.is_bootstrap_address());
    }

    #[test]
    fn nonzero_family_wakes_basin_binding() {
        let g = NodeGuid::new(0, 0, 0, 0, 0x00_00AB, 0x00_00CD);
        assert!(g.is_default_class());
        assert!(!g.is_unbasined()); // family != 0 ⇒ basin binding active
        assert!(!g.is_bootstrap_address());
    }

    #[test]
    fn family_identity_are_the_trailing_six_bytes() {
        let g = NodeGuid::new(0xDEAD_BEEF, 0x1111, 0x2222, 0x3333, 0x00_00AB, 0x00_00CD);
        assert_eq!(g.family(), 0x00_00AB);
        assert_eq!(g.identity(), 0x00_00CD);
        let lk = g.local_key();
        assert_eq!(lk & 0xFF_FFFF, 0x00_00AB);
        assert_eq!((lk >> 24) & 0xFF_FFFF, 0x00_00CD);
        assert_eq!(&g.as_bytes()[10..16], &[0xAB, 0x00, 0x00, 0xCD, 0x00, 0x00]);
    }

    #[test]
    fn edge_block_is_twelve_plus_four() {
        let e = EdgeBlock::default();
        assert_eq!(e.in_family.len(), 12);
        assert_eq!(e.out_family.len(), 4);
        assert_eq!(core::mem::size_of_val(&e), 16);
    }

    #[test]
    fn edge_codec_flavor_default_is_coarse_only() {
        // Zero-fallback default: the all-zero reading is the canon bootstrap.
        assert_eq!(EdgeCodecFlavor::default(), EdgeCodecFlavor::CoarseOnly);
        assert_eq!(EdgeCodecFlavor::CoarseOnly as u8, 0);
    }

    #[test]
    fn edge_codec_flavor_byte_costs() {
        // D = 128: coarse 1 B, residue 1 + 64 = 65 B, PQ fixed 16 B.
        assert_eq!(EdgeCodecFlavor::CoarseOnly.bytes_per_vector(128), 1);
        assert_eq!(EdgeCodecFlavor::CoarseResidue.bytes_per_vector(128), 65);
        assert_eq!(EdgeCodecFlavor::Pq32x4.bytes_per_vector(128), 16);
        // Odd D rounds the residue nibble count up.
        assert_eq!(EdgeCodecFlavor::CoarseResidue.bytes_per_vector(7), 1 + 4);
    }

    #[test]
    fn every_flavor_preserves_node_layout() {
        // The canon invariant: a flavor is an interpretation, never a stride
        // change — so no flavor forces an ENVELOPE_LAYOUT_VERSION bump.
        for f in [
            EdgeCodecFlavor::CoarseOnly,
            EdgeCodecFlavor::CoarseResidue,
            EdgeCodecFlavor::Pq32x4,
        ] {
            assert!(f.is_layout_preserving());
        }
        assert_eq!(NODE_ROW_STRIDE, core::mem::size_of::<NodeRow>());
    }

    #[test]
    fn guids_per_node_is_32_slots_clean_soc_over_packed() {
        // The 512-byte node is 32 × 16-byte GUID-sized slots.
        assert_eq!(GUIDS_PER_NODE, 32);
        assert_eq!(
            GUIDS_PER_NODE * core::mem::size_of::<NodeGuid>(),
            NODE_ROW_STRIDE
        );
        // key + edges occupy 2 slots; the value slab is the remaining 30 to
        // Tetris a class's concerns into (SoC over packed — no straddle needed).
        let key_edges_slots =
            (core::mem::size_of::<NodeGuid>() + core::mem::size_of::<EdgeBlock>()) / 16;
        assert_eq!(key_edges_slots, 2);
        assert_eq!(
            GUIDS_PER_NODE - key_edges_slots,
            30,
            "value slab = 30 slots"
        );
    }

    #[test]
    fn uniqueness_guard_is_noop_outside_bootstrap() {
        // family != 0 ⇒ no longer the bootstrap address: the guard is a no-op
        // even when `already_present` is true.
        let g = NodeGuid::new(0, 0, 0, 0, 0x00_0001, 0x00_0001);
        g.debug_assert_identity_unique(true);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "identity collision in default basin")]
    fn uniqueness_guard_panics_on_bootstrap_collision() {
        let g = NodeGuid::local(1);
        g.debug_assert_identity_unique(true);
    }

    #[test]
    #[should_panic(expected = "family must fit in 24 bits")]
    fn new_panics_on_family_overflow() {
        let _ = NodeGuid::new(0, 0, 0, 0, 0x0100_0000, 0);
    }

    #[test]
    #[should_panic(expected = "identity must fit in 24 bits")]
    fn new_panics_on_identity_overflow() {
        let _ = NodeGuid::new(0, 0, 0, 0, 0, 0x0100_0000);
    }

    #[test]
    fn display_is_canonical_self_describing() {
        // Canon (OGAR P0): xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx (8-4-4-4-12 hex);
        // groups = classid · HEEL · HIP · TWIG · family·identity.
        let g = NodeGuid::new(0xDEAD_BEEF, 0x1111, 0x2222, 0x3333, 0x00_00AB, 0x00_00CD);
        let s = g.to_string();
        assert_eq!(s, "deadbeef-1111-2222-3333-0000ab0000cd");
        assert_eq!(s.len(), 36, "8-4-4-4-12 + 4 hyphens");
        for i in [8usize, 13, 18, 23] {
            assert_eq!(s.as_bytes()[i], b'-', "hyphen at {i}");
        }
    }

    #[test]
    fn display_zero_default_is_all_zeros() {
        // Zero-fallback ladder visible at sight: classid + family == 0 prints
        // as ...0...-...0... with identity-only discrimination.
        let g = NodeGuid::local(0x00_00CD);
        assert_eq!(g.to_string(), "00000000-0000-0000-0000-0000000000cd");
    }

    // ── SoaEnvelope binding for NodeRowPacket ────────────────────────────────

    fn sample_row(classid: u32, identity: u32) -> NodeRow {
        NodeRow {
            key: NodeGuid::new(classid, 0x1111, 0x2222, 0x3333, 0x00_00AB, identity),
            edges: EdgeBlock::default(),
            value: [0u8; 480],
        }
    }

    #[test]
    fn node_row_column_table_sums_to_row_stride() {
        let total: usize = NODE_ROW_COLUMNS.iter().map(|c| c.col_bytes_per_row()).sum();
        assert_eq!(total, NODE_ROW_STRIDE);
        assert_eq!(NODE_ROW_STRIDE, core::mem::size_of::<NodeRow>());
    }

    #[test]
    fn node_row_column_table_is_in_offset_order_without_gaps() {
        // The contract: columns are contiguous (key 0..16, edges 16..32,
        // value 32..512) — no gaps, no overlap, in offset order.
        let mut prev_end = 0usize;
        for c in NODE_ROW_COLUMNS {
            assert_eq!(c.row_offset as usize, prev_end, "no gap before {c:?}");
            prev_end = c.row_offset as usize + c.col_bytes_per_row();
        }
        assert_eq!(prev_end, NODE_ROW_STRIDE);
    }

    #[test]
    fn empty_packet_verifies() {
        let rows: &[NodeRow] = &[];
        let pkt = NodeRowPacket::new(rows, 0);
        assert_eq!(pkt.n_rows(), 0);
        assert_eq!(pkt.as_le_bytes().len(), 0);
        assert!(pkt.verify_layout().is_ok(), "empty packet must verify");
    }

    #[test]
    fn single_row_packet_verifies_and_byte_view_is_zero_copy() {
        let rows = [sample_row(0xDEAD_BEEF, 0x00_00CD)];
        let pkt = NodeRowPacket::new(&rows, 7);
        assert_eq!(pkt.n_rows(), 1);
        assert_eq!(pkt.cycle(), 7);
        assert_eq!(pkt.row_stride(), 512);
        assert_eq!(pkt.as_le_bytes().len(), 512);
        // Zero-copy: the byte view's pointer is the slice's pointer.
        assert_eq!(
            pkt.as_le_bytes().as_ptr() as usize,
            rows.as_ptr() as usize,
            "as_le_bytes must be zero-copy"
        );
        assert!(pkt.verify_layout().is_ok());
    }

    #[test]
    fn multi_row_packet_byte_length_is_stride_times_rows() {
        let rows = [
            sample_row(0xDEAD_BEEF, 0x00_00CD),
            sample_row(0xCAFE_BABE, 0x00_0001),
            sample_row(0x0000_0000, 0x00_0042),
        ];
        let pkt = NodeRowPacket::new(&rows, 42);
        assert_eq!(pkt.n_rows(), 3);
        assert_eq!(pkt.as_le_bytes().len(), 3 * 512);
        assert!(pkt.verify_layout().is_ok());
    }

    #[test]
    fn row_le_view_returns_one_full_row() {
        let rows = [sample_row(1, 2), sample_row(3, 4), sample_row(5, 6)];
        let pkt = NodeRowPacket::new(&rows, 0);
        for (i, row) in rows.iter().enumerate() {
            let row_bytes = pkt.row_le(i).expect("row in range");
            assert_eq!(row_bytes.len(), 512);
            // First 4 bytes are the classid in canon-LE order.
            assert_eq!(
                u32::from_le_bytes(row_bytes[..4].try_into().unwrap()),
                row.key.classid()
            );
        }
        assert!(pkt.row_le(3).is_none(), "out of range");
    }

    #[test]
    fn column_le_view_returns_the_named_slot() {
        // Place a recognisable byte pattern in the value side; verify the
        // value column-view picks it up at the right offset.
        let mut row = sample_row(0xDEAD_BEEF, 0x00_00CD);
        row.value[0] = 0xAB;
        row.value[479] = 0xCD;
        let rows = [row];
        let pkt = NodeRowPacket::new(&rows, 0);
        let value_col = pkt
            .column_le(0, &NODE_ROW_COLUMNS[NodeRowColumn::Value as usize])
            .expect("value column in range");
        assert_eq!(value_col.len(), 480);
        assert_eq!(value_col[0], 0xAB);
        assert_eq!(value_col[479], 0xCD);
        // Key column is at offset 0, length 16 — first byte = LE byte 0 of
        // classid = 0xEF (low byte of 0xDEAD_BEEF).
        let key_col = pkt
            .column_le(0, &NODE_ROW_COLUMNS[NodeRowColumn::Key as usize])
            .expect("key column in range");
        assert_eq!(key_col.len(), 16);
        assert_eq!(key_col[0], 0xEF);
        assert_eq!(key_col[3], 0xDE);
    }

    #[test]
    fn key_bytes_in_canon_le_order() {
        // Round-trip: pack a NodeRow with known fields, read the bytes back
        // through the envelope, parse each canon group by its LE byte range,
        // confirm values match. Proves the SoA envelope view stays canon-LE
        // end-to-end without any field-accessor intermediation.
        let row = sample_row(0xDEAD_BEEF, 0x00_00CD);
        let rows = [row];
        let pkt = NodeRowPacket::new(&rows, 0);
        let bytes = pkt.as_le_bytes();
        // Per OGAR/CLAUDE.md P0: classid · HEEL · HIP · TWIG · family · identity.
        assert_eq!(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            0xDEAD_BEEF,
            "classid at [0..4]"
        );
        assert_eq!(
            u16::from_le_bytes([bytes[4], bytes[5]]),
            0x1111,
            "HEEL at [4..6]"
        );
        assert_eq!(
            u16::from_le_bytes([bytes[6], bytes[7]]),
            0x2222,
            "HIP at [6..8]"
        );
        assert_eq!(
            u16::from_le_bytes([bytes[8], bytes[9]]),
            0x3333,
            "TWIG at [8..10]"
        );
        // family is u24 LE in bytes [10..13]: 0xAB, 0x00, 0x00.
        assert_eq!(&bytes[10..13], &[0xAB, 0x00, 0x00], "family at [10..13]");
        // identity is u24 LE in bytes [13..16]: 0xCD, 0x00, 0x00.
        assert_eq!(&bytes[13..16], &[0xCD, 0x00, 0x00], "identity at [13..16]");
    }

    #[test]
    fn envelope_layout_version_matches_envelope_default() {
        // The wrapper does not override LAYOUT_VERSION, so verify_layout
        // checks against the envelope-crate default (ENVELOPE_LAYOUT_VERSION).
        let rows = [sample_row(0, 1)];
        let pkt = NodeRowPacket::new(&rows, 0);
        assert_eq!(
            <NodeRowPacket<'_> as SoaEnvelope>::LAYOUT_VERSION,
            crate::soa_envelope::ENVELOPE_LAYOUT_VERSION
        );
        // verify_layout exercises that gate.
        assert!(pkt.verify_layout().is_ok());
    }

    // ── Value-slab schema presets ────────────────────────────────────────────

    #[test]
    fn value_tenants_contiguous_within_slab() {
        let mut prev_end = VALUE_SLAB_ROW_OFFSET;
        for (i, c) in VALUE_TENANTS.iter().enumerate() {
            assert_eq!(c.name_id as usize, i, "discriminant == index");
            assert_eq!(c.row_offset as usize, prev_end, "no gap before {c:?}");
            prev_end = c.row_offset as usize + c.col_bytes_per_row();
        }
        assert!(prev_end <= NODE_ROW_STRIDE);
        assert_eq!(
            prev_end - VALUE_SLAB_ROW_OFFSET,
            120,
            "current Full carve uses 120 of 480 B (helix 6 + kanban×Rubicon tenant 8)"
        );
        assert!(prev_end - VALUE_SLAB_ROW_OFFSET <= VALUE_SLAB_LEN);
    }

    #[test]
    fn value_schema_default_is_bootstrap_empty() {
        assert_eq!(ValueSchema::default(), ValueSchema::Bootstrap);
        assert!(ValueSchema::Bootstrap.field_mask().is_empty());
        assert_eq!(ValueSchema::Bootstrap.tenant_bytes(), 0);
    }

    #[test]
    fn value_schema_full_covers_every_tenant() {
        let full = ValueSchema::Full;
        assert_eq!(full.field_mask().count() as usize, VALUE_TENANTS.len());
        for t in [
            ValueTenant::Meta,
            ValueTenant::Qualia,
            ValueTenant::MaterializedEdges,
            ValueTenant::Fingerprint,
            ValueTenant::HelixResidue,
            ValueTenant::TurbovecResidue,
            ValueTenant::Energy,
            ValueTenant::Plasticity,
            ValueTenant::EntityType,
            ValueTenant::Kanban,
        ] {
            assert!(full.has(t), "Full must materialise {t:?}");
        }
    }

    #[test]
    fn value_schema_byte_budgets_are_locked() {
        assert_eq!(ValueSchema::Bootstrap.tenant_bytes(), 0);
        // Cognitive 58 + Kanban 8 = 66; Full 112 + Kanban 8 = 120 (kanban×Rubicon
        // tenant added — reserve-don't-reclaim, still ≤ 480, stride unchanged).
        assert_eq!(ValueSchema::Cognitive.tenant_bytes(), 66);
        assert_eq!(ValueSchema::Compressed.tenant_bytes(), 56);
        assert_eq!(ValueSchema::Full.tenant_bytes(), 120);
        for s in [
            ValueSchema::Bootstrap,
            ValueSchema::Cognitive,
            ValueSchema::Compressed,
            ValueSchema::Full,
        ] {
            assert!(s.tenant_bytes() <= VALUE_SLAB_LEN);
            assert!(s.is_layout_preserving());
        }
    }

    #[test]
    fn value_schema_presets_carry_expected_tenants() {
        // Cognitive: hot columns, no codec residues, no materialised edges.
        let c = ValueSchema::Cognitive;
        assert!(c.has(ValueTenant::Meta) && c.has(ValueTenant::Qualia));
        assert!(c.has(ValueTenant::Energy) && c.has(ValueTenant::EntityType));
        assert!(!c.has(ValueTenant::HelixResidue));
        assert!(!c.has(ValueTenant::TurbovecResidue));
        assert!(!c.has(ValueTenant::MaterializedEdges));
        // Compressed: codec residues, no hot lifecycle.
        let z = ValueSchema::Compressed;
        assert!(z.has(ValueTenant::HelixResidue) && z.has(ValueTenant::TurbovecResidue));
        assert!(z.has(ValueTenant::Fingerprint));
        assert!(!z.has(ValueTenant::Energy) && !z.has(ValueTenant::Meta));
    }

    #[test]
    fn value_schema_preserves_node_stride() {
        // A preset is an interpretation of the reserved value slab, never a
        // stride change — same canon invariant as EdgeCodecFlavor.
        assert_eq!(NODE_ROW_STRIDE, core::mem::size_of::<NodeRow>());
        assert_eq!(VALUE_SLAB_ROW_OFFSET + VALUE_SLAB_LEN, NODE_ROW_STRIDE);
    }

    // ── GUID decode + classid → read-mode (the keystone) ─────────────────────

    #[test]
    fn decode_returns_all_six_canon_groups() {
        // One read yields classid + HHT (HEEL/HIP/TWIG) + family + identity, in
        // canon print order — the "read the GUID as a GUID" contract.
        let g = NodeGuid::new(0xDEAD_BEEF, 0x1111, 0x2222, 0x3333, 0x00_00AB, 0x00_00CD);
        let p = g.decode();
        assert_eq!(p.classid, 0xDEAD_BEEF);
        assert_eq!(p.heel, 0x1111);
        assert_eq!(p.hip, 0x2222);
        assert_eq!(p.twig, 0x3333);
        assert_eq!(p.family, 0x00_00AB);
        assert_eq!(p.identity, 0x00_00CD);
        // decode() is exactly the field accessors, no field invented/dropped.
        assert_eq!(p.classid, g.classid());
        assert_eq!(p.family, g.family());
        assert_eq!(p.identity, g.identity());
    }

    #[test]
    fn hht_accessors_match_display_groups() {
        // The new HEEL/HIP/TWIG accessors fold the same LE bytes Display renders.
        let g = NodeGuid::new(0xDEAD_BEEF, 0xA1B2, 0xC3D4, 0xE5F6, 0x12_3456, 0x78_9ABC);
        assert_eq!(g.heel(), 0xA1B2);
        assert_eq!(g.hip(), 0xC3D4);
        assert_eq!(g.twig(), 0xE5F6);
        // Display's middle three groups are exactly heel-hip-twig in hex.
        let s = g.to_string();
        let groups: Vec<&str> = s.split('-').collect();
        assert_eq!(groups[1], format!("{:04x}", g.heel()));
        assert_eq!(groups[2], format!("{:04x}", g.hip()));
        assert_eq!(groups[3], format!("{:04x}", g.twig()));
    }

    #[test]
    fn read_mode_default_is_full_poc() {
        // The default classid resolves to the POC read-mode: Full value slab +
        // CoarseOnly edges. This GUARDS the ClassView pairing — ReadMode::DEFAULT
        // .value_schema MUST equal the ClassView POC default (Full). When the POC
        // ends, both flip to Bootstrap together and this test flips with them.
        let rm = classid_read_mode(NodeGuid::CLASSID_DEFAULT);
        assert_eq!(rm, ReadMode::DEFAULT);
        assert_eq!(rm.value_schema, ValueSchema::Full);
        assert_eq!(rm.edge_codec, EdgeCodecFlavor::CoarseOnly);
        assert!(rm.is_layout_preserving());
    }

    #[test]
    fn read_mode_zero_fallback_for_unconfigured_classid() {
        // Any classid NOT in the builtin registry falls through to DEFAULT — the
        // key's own zero-fallback ladder (classid 0 ⇒ default class), extended to
        // read-mode resolution.
        assert_eq!(classid_read_mode(0xDEAD_BEEF), ReadMode::DEFAULT);
        assert_eq!(classid_read_mode(0x0000_0001), ReadMode::DEFAULT);
        assert_eq!(classid_read_mode(u32::MAX), ReadMode::DEFAULT);
    }

    #[test]
    fn guid_read_mode_method_delegates_to_registry() {
        // The carrier method (guid.read_mode()) and the free resolver
        // (classid_read_mode(classid)) are the SAME answer — consumer and OGAR
        // inherit one source.
        let g = NodeGuid::new(0xCAFE_BABE, 1, 2, 3, 0x00_0001, 0x00_0002);
        assert_eq!(g.read_mode(), classid_read_mode(g.classid()));
        // A default-class node reads the Full POC slab.
        assert_eq!(NodeGuid::local(0x00_00CD).read_mode(), ReadMode::DEFAULT);
    }

    #[test]
    fn default_class_node_materialises_full_slab() {
        // End-to-end connect: a bootstrap NodeRow → its classid resolves to Full →
        // the Full preset covers every tenant and uses the locked 120-byte carve.
        let row = sample_row(NodeGuid::CLASSID_DEFAULT, 0x00_00CD);
        let rm = row.key.read_mode();
        assert_eq!(rm.value_schema, ValueSchema::Full);
        assert_eq!(
            rm.value_schema.field_mask().count() as usize,
            VALUE_TENANTS.len(),
            "Full read-mode materialises every value tenant"
        );
        assert_eq!(rm.value_schema.tenant_bytes(), 120);
        // The slab has room (120 ≤ 480) and the choice never grows the stride.
        assert!(rm.value_schema.tenant_bytes() <= VALUE_SLAB_LEN);
        assert!(rm.is_layout_preserving());
    }

    #[test]
    fn osint_and_fma_classids_resolve_to_their_read_modes() {
        // The two registered graph domains (see `soa_graph`): OSINT/Gotham is a
        // hot entity graph (Cognitive value), FMA anatomy is a cold structural
        // reference (Compressed value); both read edges as CoarseOnly adjacency.
        let osint = classid_read_mode(NodeGuid::CLASSID_OSINT);
        assert_eq!(osint, ReadMode::OSINT);
        assert_eq!(osint.value_schema, ValueSchema::Cognitive);
        assert_eq!(osint.edge_codec, EdgeCodecFlavor::CoarseOnly);

        let fma = classid_read_mode(NodeGuid::CLASSID_FMA);
        assert_eq!(fma, ReadMode::FMA);
        assert_eq!(fma.value_schema, ValueSchema::Compressed);
        assert_eq!(fma.edge_codec, EdgeCodecFlavor::CoarseOnly);

        // The classids follow OGAR `0xDDCC` in the CANON (high-since-P1) half
        // (ISS-CLASSID-OGAR-DRIFT realign): OSINT domain root `0x0700`; FMA =
        // `anatomical_structure` `0x0A01` in the **Anatomy** domain — re-realigned
        // off `0x0901` to clear the OGAR `patient` collision. Never the
        // pre-realign 0x0007 / 0x0008, nor the colliding 0x0901.
        use crate::ogar_codebook::classid_canon;
        assert_eq!(NodeGuid::CLASSID_OSINT, 0x0700_0000);
        assert_eq!(NodeGuid::CLASSID_FMA, 0x0A01_0000);
        assert_ne!(
            classid_canon(NodeGuid::CLASSID_FMA),
            0x0901,
            "must not alias `patient`"
        );
        assert_eq!(
            classid_canon(NodeGuid::CLASSID_OSINT) >> 8,
            0x07,
            "OSINT domain byte"
        );
        assert_eq!(
            classid_canon(NodeGuid::CLASSID_FMA) >> 8,
            0x0A,
            "Anatomy domain byte"
        );
        assert_eq!(
            NodeGuid::new(NodeGuid::CLASSID_OSINT, 1, 2, 3, 0xAB, 0xCD).read_mode(),
            ReadMode::OSINT
        );
        // Mint-forward: the persisted pre-flip forms resolve to the SAME read
        // modes through their legacy alias keys (read-only, never minted).
        assert_eq!(classid_read_mode(NodeGuid::CLASSID_OSINT_LEGACY), osint);
        assert_eq!(classid_read_mode(NodeGuid::CLASSID_FMA_LEGACY), fma);
        assert!(osint.is_layout_preserving() && fma.is_layout_preserving());
    }

    #[test]
    fn project_and_erp_classids_resolve_to_their_read_modes() {
        // OGAR `0x01XX` (project-mgmt: OpenProject ↔ Redmine) + `0x02XX`
        // (commerce/ERP: Odoo ↔ OSB) — both hot business graphs (Cognitive).
        let project = classid_read_mode(NodeGuid::CLASSID_PROJECT);
        assert_eq!(project, ReadMode::PROJECT);
        assert_eq!(project.value_schema, ValueSchema::Cognitive);
        assert_eq!(project.edge_codec, EdgeCodecFlavor::CoarseOnly);

        let erp = classid_read_mode(NodeGuid::CLASSID_ERP);
        assert_eq!(erp, ReadMode::ERP);
        assert_eq!(erp.value_schema, ValueSchema::Cognitive);
        assert_eq!(erp.edge_codec, EdgeCodecFlavor::CoarseOnly);

        // Domain roots in the CANON (high-since-P1) half: project `0x0100`,
        // ERP `0x0200`; concept byte `0x00` = the domain root (reserved).
        use crate::ogar_codebook::classid_canon;
        assert_eq!(NodeGuid::CLASSID_PROJECT, 0x0100_0000);
        assert_eq!(NodeGuid::CLASSID_ERP, 0x0200_0000);
        assert_eq!(classid_canon(NodeGuid::CLASSID_PROJECT) >> 8, 0x01);
        assert_eq!(classid_canon(NodeGuid::CLASSID_ERP) >> 8, 0x02);
        // Mint-forward: pre-flip forms keep resolving via the legacy aliases.
        assert_eq!(classid_read_mode(NodeGuid::CLASSID_PROJECT_LEGACY), project);
        assert_eq!(classid_read_mode(NodeGuid::CLASSID_ERP_LEGACY), erp);
        assert!(project.is_layout_preserving() && erp.is_layout_preserving());
    }

    // ── ReadMode tail_variant (P-A) — the V3 identity axis ────────────────────

    #[test]
    fn read_mode_tail_variant_is_v1_legacy_default() {
        // The default classid resolves to the conservative legacy tail (V1): every
        // un-minted classid stays V1 → zero re-mint of the V1/V2 corpus (L1,
        // RESERVE-DON'T-RECLAIM). V1 is latent (nothing reads tail_variant yet),
        // and the whole DEFAULT read-mode stays layout-preserving across all three
        // axes — adopting tail_variant never bumps ENVELOPE_LAYOUT_VERSION.
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_DEFAULT).tail_variant,
            TailVariant::V1
        );
        assert_eq!(ReadMode::DEFAULT.tail_variant, TailVariant::V1);
        assert!(TailVariant::default() == TailVariant::V1);
        assert!(TailVariant::V1.is_layout_preserving());
        assert!(ReadMode::DEFAULT.is_layout_preserving());
        // The mechanism axis exists for all three variants (V3 is the new-gen key).
        // The Phase-1 V3 set is COMPLETE: OSINT-V3 + FMA-V3 + CPIC-V3 (Genetics) are
        // all wired (see read_mode_fma_v3_and_cpic_v3_route_their_domains).
        assert!(TailVariant::V3.is_layout_preserving());
        assert!(TailVariant::V2.is_layout_preserving());
    }

    #[cfg(feature = "guid-v3-tail")]
    #[test]
    fn read_mode_osint_v3_routes_v3_tail_and_osint_domain() {
        // The wired V3 exemplar proves BOTH facts at once — the canon/custom
        // split scheme (canon HIGH since the P1 flip):
        //   (1) the third axis IS the registry field — classid_read_mode resolves
        //       CLASSID_OSINT_V3 to TailVariant::V3 (never a public new_v3 dispatch);
        //   (2) the domain router reads the CANON half (0x0701 = OSINT:q2), so the
        //       gen-marker 0x1000 in the custom half never perturbs domain routing.
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_OSINT_V3).tail_variant,
            TailVariant::V3
        );
        assert_eq!(
            crate::ogar_codebook::classid_concept_domain(NodeGuid::CLASSID_OSINT_V3),
            crate::ogar_codebook::ConceptDomain::Osint
        );
        // The whole read-mode resolves to the V3 exemplar (same hot value model as
        // legacy OSINT) and stays layout-preserving on all three axes.
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_OSINT_V3),
            ReadMode::OSINT_V3
        );
        assert_eq!(ReadMode::OSINT_V3.value_schema, ValueSchema::Cognitive);
        assert!(ReadMode::OSINT_V3.is_layout_preserving());
        // Concretely: canon (domain:appid) in the HIGH half, marker in the LOW —
        // stored `0x0701_1000`, human-readable `0x07:01::1000`. The appid
        // normalizes to `:01` (q2) per the 2026-07-02 ruling, so the V3 canon is
        // 0x0701, not the v1 domain root 0x0700.
        use crate::ogar_codebook::{classid_canon, classid_custom};
        assert_eq!(NodeGuid::CLASSID_OSINT_V3, 0x0701_1000);
        assert_eq!(
            classid_custom(NodeGuid::CLASSID_OSINT_V3),
            0x1000,
            "gen-marker in the custom (low) half"
        );
        assert_eq!(
            classid_canon(NodeGuid::CLASSID_OSINT_V3),
            0x0701,
            "canon == OSINT domain 0x07, appid 0x01 (q2)"
        );
        // Mint-forward: the persisted pre-flip form resolves the same read mode.
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_OSINT_V3_LEGACY),
            ReadMode::OSINT_V3
        );
        assert_eq!(NodeGuid::CLASSID_OSINT_V3_LEGACY, 0x1000_0700);
    }

    #[cfg(feature = "guid-v3-tail")]
    #[test]
    fn read_mode_fma_v3_and_cpic_v3_route_their_domains() {
        use crate::ogar_codebook::{
            classid_canon, classid_concept_domain, classid_custom, ConceptDomain,
        };
        // Phase-1 V3 set completion: FMA-V3 + CPIC-V3 resolve to the V3 tail AND
        // their domain routes off the CANON (high) half, unperturbed by the
        // gen-marker in the custom half — the same scheme proven for OSINT-V3.

        // FMA-V3: Anatomy domain (0x0A) intact; cold Compressed model (mirrors FMA).
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_FMA_V3).tail_variant,
            TailVariant::V3
        );
        assert_eq!(
            classid_concept_domain(NodeGuid::CLASSID_FMA_V3),
            ConceptDomain::Anatomy
        );
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_FMA_V3),
            ReadMode::FMA_V3
        );
        assert_eq!(ReadMode::FMA_V3.value_schema, ValueSchema::Compressed);
        assert_eq!(NodeGuid::CLASSID_FMA_V3, 0x0A01_1000);
        assert_eq!(
            classid_custom(NodeGuid::CLASSID_FMA_V3),
            0x1000,
            "gen-marker in the custom (low) half"
        );
        assert_eq!(
            classid_canon(NodeGuid::CLASSID_FMA_V3),
            classid_canon(NodeGuid::CLASSID_FMA),
            "canon == FMA concept (0x0A01), shared with v1 FMA"
        );
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_FMA_V3_LEGACY),
            ReadMode::FMA_V3,
            "pre-flip form resolves via the legacy alias"
        );

        // CPIC-V3: the operator-allocated Genetics domain (0x0E); Compressed = the
        // fixed human-genome schema view (basins = genomic mereology, not labels).
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_CPIC_V3).tail_variant,
            TailVariant::V3
        );
        assert_eq!(
            classid_concept_domain(NodeGuid::CLASSID_CPIC_V3),
            ConceptDomain::Genetics
        );
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_CPIC_V3),
            ReadMode::CPIC_V3
        );
        assert_eq!(ReadMode::CPIC_V3.value_schema, ValueSchema::Compressed);
        assert_eq!(NodeGuid::CLASSID_CPIC_V3, 0x0E01_1000);
        assert_eq!(
            classid_custom(NodeGuid::CLASSID_CPIC_V3),
            0x1000,
            "gen-marker in the custom (low) half"
        );
        assert_eq!(
            classid_canon(NodeGuid::CLASSID_CPIC_V3),
            0x0E01,
            "canon == Genetics domain 0x0E, appid 0x01 (q2 — normalized from \
             the pre-flip domain-root :00 per the ruling)"
        );
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_CPIC_V3_LEGACY),
            ReadMode::CPIC_V3,
            "pre-flip form (canon 0x0E00) resolves via the legacy alias"
        );

        // The three V3 classes are mutually distinct, all V3 + layout-preserving.
        assert!(ReadMode::FMA_V3.is_layout_preserving());
        assert!(ReadMode::CPIC_V3.is_layout_preserving());
        assert_ne!(NodeGuid::CLASSID_FMA_V3, NodeGuid::CLASSID_CPIC_V3);
        assert_ne!(NodeGuid::CLASSID_FMA_V3, NodeGuid::CLASSID_OSINT_V3);
    }

    #[cfg(feature = "guid-v3-tail")]
    #[test]
    fn mint_for_osint_v3_is_end_to_end_routable() {
        // Phase-1 end-to-end (soa-value-tenant-migration-v2.md §2): mint a class's
        // identity BY ITS CLASSID's tail_variant — the symmetric spine
        // `mint_for(classid_read_mode(c).tail_variant, …)` — and confirm the minted
        // address is V3-routable (the Codex-P2 EMPTY-fold is GONE).
        use crate::hhtl::{NiblePath, MAX_DEPTH};

        // (1) Resolve the tail shape from the classid — consumers never hardcode
        //     v1/v2/v3; the registry says which tail OSINT-V3 reads.
        let tv = classid_read_mode(NodeGuid::CLASSID_OSINT_V3).tail_variant;
        assert_eq!(
            tv,
            TailVariant::V3,
            "OSINT-V3 classid resolves to the V3 tail"
        );

        // (2) Mint through the carrier. tv == V3 ⇒ mint_for dispatches to new_v2,
        //     laying the tail down as leaf·family·identity (3×u16).
        let node = NodeGuid::mint_for(
            tv,
            NodeGuid::CLASSID_OSINT_V3,
            0xAB12, // HEEL  (part_of:is_a tile)
            0xCD34, // HIP
            0xEF56, // TWIG
            0x789A, // LEAF
            0xBCDE, // family (basin)
            0xF012, // identity (instance)
        );

        // (3) The generation marker (custom/low half since P1) round-trips in
        //     the stored classid…
        assert_eq!(node.classid(), NodeGuid::CLASSID_OSINT_V3);
        assert_eq!(
            crate::ogar_codebook::classid_custom(node.classid()),
            0x1000,
            "gen-marker preserved in the key"
        );
        // …and the node's OWN read_mode() (carrier form) agrees it is V3.
        assert_eq!(node.read_mode().tail_variant, TailVariant::V3);

        // (4) THE FIX, both directions:
        //   - the v1 fold REFUSES this address (both classid halves nonzero —
        //     a marked classid under every order) → the latent EMPTY fold
        //     Codex flagged on #613;
        assert_eq!(
            NiblePath::from_guid_prefix(&node),
            None,
            "v1 fold still refuses the marked classid"
        );
        //   - the v3 fold ROUTES it: HEEL·HIP·TWIG·LEAF in full (both bytes per
        //     8:8 tile), depth 16, classid NOT folded → never EMPTY.
        let p = NiblePath::from_guid_prefix_v3(&node);
        assert_ne!(p, NiblePath::EMPTY, "V3 address must route, not collapse");
        let expected = (0xAB12u64 << 48) | (0xCD34u64 << 32) | (0xEF56u64 << 16) | 0x789Au64;
        assert_eq!(
            p.packed(),
            (expected, MAX_DEPTH),
            "the full HEEL·HIP·TWIG·LEAF cascade is the routing prefix"
        );

        // (5) The tail reads back through the v2 decode (V3 shares the v2 bytes —
        //     family/identity are the basin tail, preserved not dropped).
        let d = node.decode_v2();
        assert_eq!(
            (d.heel, d.hip, d.twig, d.leaf, d.family, d.identity),
            (0xAB12, 0xCD34, 0xEF56, 0x789A, 0xBCDE, 0xF012)
        );
    }

    #[cfg(feature = "guid-v3-tail")]
    #[test]
    fn osint_v3_cognitive_tenant_carve_field_isolation_matrix() {
        // Phase-2 seam (the CPIC doc's "Phase 2 shapes the V3 tenants … on top"):
        // a V3 node's VALUE side is the tenant carve its registry read-mode names.
        // The V3 exemplar OSINT-V3 resolves to ValueSchema::Cognitive — the
        // AriGraph-hot set the mailbox SoA view reads (Meta/Energy/EntityType are
        // the `meta_raw`/`energy`/`entity_type` columns of
        // `crate::soa_view::MailboxSoaView`; EntityType IS the `class_id` alias).
        // The I-LEGACY mandatory field-isolation matrix existed only for the
        // Kanban tenant (kanban_tenant_round_trip_and_field_isolation); this
        // extends it to EVERY tenant of the Cognitive carve: writing one tenant's
        // byte lane changes NO byte outside it, and never touches key or edges.
        // Nothing invented — key minted by registry dispatch (`mint_for`), lanes
        // read from the compile-asserted `ValueTenant` carve.

        // (1) Registry says: OSINT-V3 = {V3 tail, Cognitive value, CoarseOnly edges}.
        let rm = classid_read_mode(NodeGuid::CLASSID_OSINT_V3);
        assert_eq!(rm.value_schema, ValueSchema::Cognitive);

        // (2) Pin the carve: exactly the 7 hot tenants, none of the codec residues.
        let hot = [
            ValueTenant::Meta,
            ValueTenant::Qualia,
            ValueTenant::Fingerprint,
            ValueTenant::Energy,
            ValueTenant::Plasticity,
            ValueTenant::EntityType,
            ValueTenant::Kanban,
        ];
        for t in hot {
            assert!(rm.value_schema.has(t), "Cognitive materialises {t:?}");
        }
        for t in [
            ValueTenant::MaterializedEdges,
            ValueTenant::HelixResidue,
            ValueTenant::TurbovecResidue,
        ] {
            assert!(!rm.value_schema.has(t), "Cognitive must NOT carry {t:?}");
        }
        assert_eq!(rm.value_schema.field_mask().count() as usize, hot.len());

        // (3) Mint the key by registry dispatch (consumers never hardcode a
        //     constructor) and build the canonical row.
        let key = NodeGuid::mint_for(
            rm.tail_variant,
            NodeGuid::CLASSID_OSINT_V3,
            0x0101,
            0x0202,
            0x0303,
            0x0404,
            0x0505,
            0x0606,
        );
        let mut row = NodeRow {
            key,
            edges: EdgeBlock::default(),
            value: [0u8; 480],
        };

        // (4) THE MATRIX: flip every byte of one tenant's lane; assert the lane
        //     changed and every byte OUTSIDE it did not — per tenant, in carve order.
        assert_value_lane_isolation(&mut row, &hot);
        // Value-slab writes never move the key or the edge block.
        assert_eq!(row.key, key, "key untouched by value-tenant writes");
        assert_eq!(
            row.edges,
            EdgeBlock::default(),
            "edge block untouched by value-tenant writes"
        );

        // (5) EntityType tenant ↔ SoA class column: the u16 the slab carries is the
        //     same discriminator `MailboxSoaView::class_id()` (alias of
        //     `entity_type()`) exposes per row. Stamp the CANON half and read it
        //     back — on the V3 class it is the OSINT:q2 canon 0x0701 (appid
        //     normalized per the ruling; the gen-marker in the custom half
        //     never leaks into the entity discriminator).
        let o = ValueTenant::EntityType.value_offset();
        row.value[o..o + 2].copy_from_slice(
            &crate::ogar_codebook::classid_canon(NodeGuid::CLASSID_OSINT_V3).to_le_bytes(),
        );
        let et = u16::from_le_bytes([row.value[o], row.value[o + 1]]);
        assert_eq!(
            et, 0x0701,
            "EntityType tenant carries the canon Osint concept"
        );

        // (6) Typed accessors stay live on the V3 row: the kanban round-trip and
        //     the qualia read decode the SAME slab this matrix certified.
        row.set_kanban(KanbanTenant {
            phase: KanbanColumn::CognitiveWork,
            exec: ExecTarget::Native,
            cycle: 0x0701_1000,
        });
        assert_eq!(row.kanban().cycle, 0x0701_1000);
        assert_eq!(row.qualia().0, {
            let qo = ValueTenant::Qualia.value_offset();
            let mut b = [0u8; 8];
            b.copy_from_slice(&row.value[qo..qo + 8]);
            u64::from_le_bytes(b)
        });
    }

    /// Shared body of the tenant-carve matrix: flip every byte of one tenant's
    /// lane; assert the lane changed and every byte OUTSIDE it did not — per
    /// tenant, in carve order (the I-LEGACY mandatory field-isolation test).
    #[cfg(feature = "guid-v3-tail")]
    fn assert_value_lane_isolation(row: &mut NodeRow, lanes: &[ValueTenant]) {
        for &t in lanes {
            let before = row.value;
            let (o, n) = (t.value_offset(), t.byte_len());
            for b in &mut row.value[o..o + n] {
                *b ^= 0xFF;
            }
            for (i, (&now, &was)) in row.value.iter().zip(before.iter()).enumerate() {
                if (o..o + n).contains(&i) {
                    assert_ne!(now, was, "{t:?} lane byte {i} must have flipped");
                } else {
                    assert_eq!(now, was, "byte {i} outside {t:?} lane must not change");
                }
            }
        }
    }

    #[cfg(feature = "guid-v3-tail")]
    #[test]
    fn fma_cpic_v3_compressed_tenant_carve_field_isolation_matrix() {
        // The cold half of the Phase-1 V3 set: FMA-V3 and CPIC-V3 both resolve to
        // ValueSchema::Compressed — the codec-stack carve (no hot lifecycle
        // columns). Same certification as the OSINT-V3/Cognitive matrix above:
        // registry-dispatched mint, per-tenant lane isolation, key+edges untouched.
        // With this, EVERY carve a Phase-1 V3 class materialises is matrix-covered.
        let rm = classid_read_mode(NodeGuid::CLASSID_FMA_V3);
        assert_eq!(rm.value_schema, ValueSchema::Compressed);
        // CPIC-V3 shares the SAME cold carve — one matrix certifies both.
        assert_eq!(
            classid_read_mode(NodeGuid::CLASSID_CPIC_V3).value_schema,
            ValueSchema::Compressed
        );

        // Pin the carve: exactly the 4 codec tenants, none of the hot lifecycle set.
        let cold = [
            ValueTenant::Fingerprint,
            ValueTenant::HelixResidue,
            ValueTenant::TurbovecResidue,
            ValueTenant::EntityType,
        ];
        for t in cold {
            assert!(rm.value_schema.has(t), "Compressed materialises {t:?}");
        }
        for t in [
            ValueTenant::Meta,
            ValueTenant::Qualia,
            ValueTenant::MaterializedEdges,
            ValueTenant::Energy,
            ValueTenant::Plasticity,
            ValueTenant::Kanban,
        ] {
            assert!(!rm.value_schema.has(t), "Compressed must NOT carry {t:?}");
        }
        assert_eq!(rm.value_schema.field_mask().count() as usize, cold.len());

        // Registry-dispatched mint + the matrix over the cold lanes.
        let key = NodeGuid::mint_for(
            rm.tail_variant,
            NodeGuid::CLASSID_FMA_V3,
            0x1111,
            0x2222,
            0x3333,
            0x4444,
            0x5555,
            0x6666,
        );
        let mut row = NodeRow {
            key,
            edges: EdgeBlock::default(),
            value: [0u8; 480],
        };
        assert_value_lane_isolation(&mut row, &cold);
        assert_eq!(row.key, key, "key untouched by value-tenant writes");
        assert_eq!(
            row.edges,
            EdgeBlock::default(),
            "edge block untouched by value-tenant writes"
        );

        // The EntityType discriminator carries the CANON half on the cold
        // classes too — Anatomy 0x0A01 / Genetics:q2 0x0E01 (appid normalized
        // per the ruling), never the 0x1000 gen-marker.
        let o = ValueTenant::EntityType.value_offset();
        row.value[o..o + 2].copy_from_slice(
            &crate::ogar_codebook::classid_canon(NodeGuid::CLASSID_FMA_V3).to_le_bytes(),
        );
        assert_eq!(
            u16::from_le_bytes([row.value[o], row.value[o + 1]]),
            0x0A01,
            "EntityType tenant carries the canon Anatomy concept"
        );
        row.value[o..o + 2].copy_from_slice(
            &crate::ogar_codebook::classid_canon(NodeGuid::CLASSID_CPIC_V3).to_le_bytes(),
        );
        assert_eq!(
            u16::from_le_bytes([row.value[o], row.value[o + 1]]),
            0x0E01,
            "EntityType tenant carries the canon Genetics root"
        );
    }

    #[cfg(feature = "guid-v2-tail")]
    #[test]
    fn mint_for_dispatches_to_the_right_constructor_per_tail() {
        // The carrier is exactly `new` (V1) / `new_v2` (V2 & V3) — no new layout,
        // just a classid-driven choice of the existing constructors. V3 shares the
        // V2 *bytes* (it only reads them differently), so it mints identically.
        let c = 0xDEAD_BEEF;
        let (h, hp, t) = (0x1111u16, 0x2222u16, 0x3333u16);

        // V1 arm == new(...): the u24 family·identity tail; `leaf` is not a V1 tier
        // and is ignored (pass a sentinel to prove it is dropped).
        assert_eq!(
            NodeGuid::mint_for(TailVariant::V1, c, h, hp, t, 0xFFFF, 0x00_00AB, 0x00_00CD),
            NodeGuid::new(c, h, hp, t, 0x00_00AB, 0x00_00CD),
            "V1 arm is `new`, leaf ignored"
        );

        // V2 arm == new_v2(...): the leaf·family·identity 3×u16 tail.
        assert_eq!(
            NodeGuid::mint_for(TailVariant::V2, c, h, hp, t, 0x4444, 0x5555, 0x6666),
            NodeGuid::new_v2(c, h, hp, t, 0x4444, 0x5555, 0x6666),
            "V2 arm is `new_v2`"
        );

        // V3 arm == new_v2(...): identical stored bytes to V2 (the (part_of:is_a)
        // reading is a *lens*, not a re-carve) — same constructor, same key.
        assert_eq!(
            NodeGuid::mint_for(TailVariant::V3, c, h, hp, t, 0x4444, 0x5555, 0x6666),
            NodeGuid::new_v2(c, h, hp, t, 0x4444, 0x5555, 0x6666),
            "V3 stores the same bytes as V2"
        );
    }

    // ── GUID v2 tail (D-GV2-1) — field-isolation matrix + coexistence ─────────

    #[cfg(feature = "guid-v2-tail")]
    #[test]
    fn v2_field_isolation_matrix() {
        // Each tier carries a distinct value; every accessor reads back exactly
        // its own, and varying ONE tier changes ONLY that accessor (the
        // mandatory layout-bit-boundary test for a reclaim, I-LEGACY).
        let base = NodeGuid::new_v2(0x1111_2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777, 0x8888);
        assert_eq!(base.classid(), 0x1111_2222);
        assert_eq!(base.heel(), 0x3333);
        assert_eq!(base.hip(), 0x4444);
        assert_eq!(base.twig(), 0x5555);
        assert_eq!(base.leaf(), 0x6666);
        assert_eq!(base.family_v2(), 0x7777);
        assert_eq!(base.identity_v2(), 0x8888);

        // vary ONLY leaf
        let l = NodeGuid::new_v2(0x1111_2222, 0x3333, 0x4444, 0x5555, 0xAAAA, 0x7777, 0x8888);
        assert_eq!(l.leaf(), 0xAAAA);
        assert_eq!(l.family_v2(), base.family_v2());
        assert_eq!(l.identity_v2(), base.identity_v2());
        assert_eq!(l.twig(), base.twig());
        // vary ONLY family
        let f = NodeGuid::new_v2(0x1111_2222, 0x3333, 0x4444, 0x5555, 0x6666, 0xBBBB, 0x8888);
        assert_eq!(f.family_v2(), 0xBBBB);
        assert_eq!(f.leaf(), base.leaf());
        assert_eq!(f.identity_v2(), base.identity_v2());
        // vary ONLY identity
        let i = NodeGuid::new_v2(0x1111_2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777, 0xCCCC);
        assert_eq!(i.identity_v2(), 0xCCCC);
        assert_eq!(i.leaf(), base.leaf());
        assert_eq!(i.family_v2(), base.family_v2());

        // local_key_v2 = family ++ identity (LE)
        assert_eq!(base.local_key_v2(), 0x8888_7777);
        // decode_v2 round-trips the tail
        let d = base.decode_v2();
        assert_eq!((d.leaf, d.family, d.identity), (0x6666, 0x7777, 0x8888));
        // Display is uniform 4-hex groups (classid 8).
        assert_eq!(base.to_hex_v2(), "11112222-3333-4444-5555-6666-7777-8888");
    }

    #[cfg(feature = "guid-v2-tail")]
    #[test]
    fn v1_and_v2_share_prefix_differ_in_tail() {
        // v1 and v2 agree on the prefix (classid·HEEL·HIP·TWIG)…
        let v1 = NodeGuid::new(0xDEAD_BEEF, 0x1111, 0x2222, 0x3333, 0x00_00AB, 0x00_00CD);
        let v2 = NodeGuid::new_v2(0xDEAD_BEEF, 0x1111, 0x2222, 0x3333, 0, 0xABCD, 0);
        assert_eq!(v1.classid(), v2.classid());
        assert_eq!(v1.heel(), v2.heel());
        assert_eq!(v1.hip(), v2.hip());
        assert_eq!(v1.twig(), v2.twig());
        // …but the tail bytes are interpreted differently — which is exactly why
        // the version gate is mandatory before reading a tail.
        assert_eq!(GUID_TAIL_LAYOUT_VERSION_V2, 2);
        // v1 accessors remain UNTOUCHED under the feature (additive, non-breaking).
        assert_eq!(v1.family(), 0x00_00AB);
        assert_eq!(v1.identity(), 0x00_00CD);
    }
}
