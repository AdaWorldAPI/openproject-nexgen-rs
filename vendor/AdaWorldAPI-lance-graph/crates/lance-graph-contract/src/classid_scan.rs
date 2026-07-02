//! `classid_scan` — the D-V3-W6a adoption-scan COUNTING LOGIC (zero-dep).
//!
//! Two governance metrics — V3 classid adoption% and the pre-flip corpus-proof
//! count — are, per `.claude/v3/soa_layout/routing.md` §5 ("Monitor routing —
//! adoption is a range count"), **the same key-range scan** over the DECODED
//! `classid: u32`, never a raw LE key-byte prefix walk (routing.md §1's
//! byte-order caveat, codex #629). This module is that scanner's counting
//! logic: [`classify_form`] buckets one decoded classid into a [`ClassidForm`],
//! [`count_adoption`] folds an iterator of classids into [`AdoptionCounts`].
//!
//! `classify_form` mirrors [`classid_canon_compat`](crate::ogar_codebook::classid_canon_compat)'s
//! own branch structure exactly — same two decisions, in the same order — so a
//! row this scanner buckets as [`ClassidForm::CanonHigh`] is precisely a row
//! `classid_canon_compat` resolves WITHOUT falling back to the legacy
//! [`ClassidOrder::CanonLow`](crate::ogar_codebook::ClassidOrder::CanonLow)
//! split, and a row bucketed as one of the three legacy variants is precisely
//! a row that fallback resolves. No bit math is performed on the composed
//! `u32` here — every classification reads the two `u16` halves returned by
//! [`split_classid`](crate::ogar_codebook::split_classid), the one sanctioned
//! decomposition helper (`ogar_codebook.rs` D-CCF-0).
//!
//! The three legacy shapes counted as `old_form` are exactly the set
//! `classid_canon_compat` routes through its `CanonLow` fallback (per
//! routing.md §5, corpus-proof MUST count all three or it can falsely report
//! a clean corpus while un-rebaked render rows remain):
//!
//! - [`ClassidForm::LegacyZeroPrefixHigh`] — `0x0000_DDCC` (legacy core form;
//!   worked example `ogar_codebook::tests::classid_canon_compat_reads_both_stored_forms`
//!   line `classid_canon_compat(0x0000_0901)` and `NodeGuid::CLASSID_OSINT_LEGACY`
//!   / `CLASSID_FMA_LEGACY` / `CLASSID_PROJECT_LEGACY` / `CLASSID_ERP_LEGACY`).
//! - [`ClassidForm::LegacyV3MarkerHigh`] — `0x1000_DDCC` (pre-flip V3-marker
//!   form; worked example `classid_canon_compat(0x1000_0700)` and
//!   `NodeGuid::CLASSID_OSINT_V3_LEGACY` / `CLASSID_FMA_V3_LEGACY` /
//!   `CLASSID_CPIC_V3_LEGACY`).
//! - [`ClassidForm::LegacyRenderPrefixHigh`] — `0xAAAA_DDCC` (legacy
//!   app/render-prefix-high form; worked example
//!   `classid_canon_compat(0x0005_0901)` — MedCare's pre-flip Healthcare
//!   render pair).

use crate::ogar_codebook::split_classid;

/// The decoded shape of a stored `classid: u32`, per the decision procedure
/// in [`classid_canon_compat`](crate::ogar_codebook::classid_canon_compat)
/// (`ogar_codebook.rs` lines 361-387) and the shape catalogue in
/// `.claude/v3/soa_layout/routing.md` §5.
///
/// `classify_form` mirrors `classid_canon_compat`'s two-branch structure:
///
/// 1. `canon >= 0x0100 && canon != 0x1000` → the id resolves natively under
///    the active [`ClassidOrder::CanonHigh`](crate::ogar_codebook::ClassidOrder::CanonHigh)
///    order, no fallback needed → [`ClassidForm::CanonHigh`]. The degenerate
///    default-class case (`0x0000_0000`, `canon == 0x0000 && custom == 0x0000`)
///    also resolves without a fallback (`classid_canon_compat`'s final `else`
///    returns `canon` directly, same value either interpretation) and is
///    folded into this variant too — it was never a pre-flip form to migrate
///    away from, so it does not belong in `old_form`.
/// 2. Otherwise, if `custom != 0`, the id needed the legacy `CanonLow`
///    fallback — one of the three [`old_form`](AdoptionCounts::old_form)
///    shapes, distinguished by which value `canon` (the HIGH half) carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ClassidForm {
    /// The classid resolves natively under the active `CanonHigh` order
    /// (`canon >= 0x0100 && canon != 0x1000`), or is the degenerate default
    /// class (`0x0000_0000`) — no legacy fallback consulted.
    CanonHigh,
    /// `0x0000_DDCC` — legacy core form (pre-flip `compose_classid_with(CanonLow, concept, 0)`).
    /// `canon == 0x0000 && custom != 0x0000`.
    LegacyZeroPrefixHigh,
    /// `0x1000_DDCC` — pre-flip V3-marker-high form (pre-flip
    /// `compose_classid_with(CanonLow, concept, 0x1000)`).
    /// `canon == 0x1000 && custom != 0x0000`.
    ///
    /// **Documented ambiguity bound** (`ogar_codebook.rs` lines 372-375, the
    /// `classid_canon_compat` doc comment "Documented limitation"): a future
    /// GENUINE canon-high id whose concept equals `0x1000` exactly (the
    /// domain-root slot of the currently-`Unassigned` domain `0x10`) is
    /// bit-identical to this legacy shape and is therefore
    /// indistinguishable from it under `classid_canon_compat`'s heuristic —
    /// see [`ClassidForm::Ambiguous`]. `classify_form` follows
    /// `classid_canon_compat` and routing.md §5's corpus-proof convention:
    /// it classifies every `canon == 0x1000 && custom != 0` id as THIS
    /// variant (conservatively legacy), never as `Ambiguous`, because no
    /// [`CODEBOOK`](crate::ogar_codebook::CODEBOOK) entry occupies domain
    /// `0x10` today — see [`ClassidForm::Ambiguous`] for when that changes.
    LegacyV3MarkerHigh,
    /// `0xAAAA_DDCC` — legacy app/render-prefix-high form (pre-flip
    /// `compose_classid_with(CanonLow, concept, app_prefix)`, e.g. MedCare's
    /// `0x0005_0901`). `canon` is nonzero, `< 0x0100`, and not `0x1000`
    /// (automatically excluded since `0x1000 >= 0x0100`); `custom != 0x0000`.
    LegacyRenderPrefixHigh,
    /// Reserved for the case documented at [`ClassidForm::LegacyV3MarkerHigh`]:
    /// a classid whose true canon is genuinely `0x1000` (domain `0x10` root)
    /// composed under the active `CanonHigh` order is bit-for-bit identical
    /// to a legacy V3-marker-high id, so no purely bit-level classifier can
    /// tell them apart. `classify_form` never constructs this variant today
    /// — see the `LegacyV3MarkerHigh` doc comment for why it resolves that
    /// bit pattern to `LegacyV3MarkerHigh` instead. Kept as a distinct,
    /// `#[non_exhaustive]`-safe variant so a future classifier that DOES gain
    /// the information to disambiguate (e.g. cross-checking
    /// [`CODEBOOK`](crate::ogar_codebook::CODEBOOK) once domain `0x10` mints
    /// a concept) has somewhere to route the genuinely-undecidable case
    /// without a breaking enum change.
    Ambiguous,
}

/// Classify a decoded `classid: u32` into its [`ClassidForm`] — the counting
/// primitive both the adoption% and corpus-proof monitors scan with (per
/// `.claude/v3/soa_layout/routing.md` §5, "ONE two-metric scanner"). Reads
/// only the two `u16` halves from [`split_classid`](crate::ogar_codebook::split_classid);
/// performs no bit math on the composed `u32` itself (`/v3-audit` check 1).
#[inline]
#[must_use]
pub fn classify_form(classid: u32) -> ClassidForm {
    let (canon, custom) = split_classid(classid);
    if canon >= 0x0100 && canon != 0x1000 {
        // Mirrors classid_canon_compat's native branch. Also captures the
        // degenerate default class (canon == 0x0000, custom == 0x0000) via
        // the fallthrough below — see the second branch.
        ClassidForm::CanonHigh
    } else if custom != 0 {
        // Mirrors classid_canon_compat's CanonLow-fallback branch: this id
        // needed the legacy split to resolve its true canon. Distinguish the
        // three routing.md §5 shapes by which value `canon` (the HIGH half)
        // carries.
        match canon {
            0x0000 => ClassidForm::LegacyZeroPrefixHigh,
            0x1000 => ClassidForm::LegacyV3MarkerHigh,
            _ => ClassidForm::LegacyRenderPrefixHigh,
        }
    } else {
        // custom == 0x0000 && canon < 0x0100: in practice only the default
        // class (0x0000_0000) — classid_canon_compat's final `else` returns
        // `canon` (0) directly here, identical to the CanonHigh reading, so
        // this is not a pre-flip form to migrate away from.
        ClassidForm::CanonHigh
    }
}

/// Range-count tallies over a scanned set of classids, per
/// `.claude/v3/soa_layout/routing.md` §5's "ONE two-metric scanner":
/// `canon_high` feeds the adoption% metric, `old_form` feeds the
/// corpus-proof metric (all three legacy shapes summed), `ambiguous` is
/// reserved for [`ClassidForm::Ambiguous`] (currently always `0` — see that
/// variant's doc comment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdoptionCounts {
    /// Rows classified as [`ClassidForm::CanonHigh`] (native new form,
    /// including the degenerate default class).
    pub canon_high: u64,
    /// Rows classified as any of the three legacy shapes
    /// ([`ClassidForm::LegacyZeroPrefixHigh`],
    /// [`ClassidForm::LegacyV3MarkerHigh`],
    /// [`ClassidForm::LegacyRenderPrefixHigh`]) — the corpus-proof count.
    /// Zero across all three ⇒ alias retirement unlocks (routing.md §5).
    pub old_form: u64,
    /// Rows classified as [`ClassidForm::Ambiguous`]. Always `0` under the
    /// current [`classify_form`] (see that function's doc comment); carried
    /// so callers never need to special-case its absence.
    pub ambiguous: u64,
    /// Total rows scanned (`canon_high + old_form + ambiguous`).
    pub total: u64,
}

impl AdoptionCounts {
    /// Adoption percentage: `canon_high / total`, in `[0.0, 1.0]`. `0.0` for
    /// an empty scan (`total == 0`) rather than `NaN` — an empty corpus is
    /// vacuously not-yet-adopted, not undefined.
    #[inline]
    #[must_use]
    pub fn adoption_pct(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.canon_high as f64 / self.total as f64
        }
    }
}

/// Fold an iterator of decoded `classid: u32` values into [`AdoptionCounts`]
/// by [`classify_form`]. The one counting pass both the adoption% and
/// corpus-proof monitors share (routing.md §5).
#[must_use]
pub fn count_adoption(ids: impl Iterator<Item = u32>) -> AdoptionCounts {
    let mut counts = AdoptionCounts::default();
    for id in ids {
        match classify_form(id) {
            ClassidForm::CanonHigh => counts.canon_high += 1,
            ClassidForm::LegacyZeroPrefixHigh
            | ClassidForm::LegacyV3MarkerHigh
            | ClassidForm::LegacyRenderPrefixHigh => counts.old_form += 1,
            ClassidForm::Ambiguous => counts.ambiguous += 1,
        }
        counts.total += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ogar_codebook::{
        canonical_concept_id, classid_canon_compat, compose_classid_with,
        render_classid_for_concept, AppPrefix, ClassidOrder, CODEBOOK,
    };
    use crate::NodeGuid;

    // Every test classid below is built through a sanctioned composer
    // (`compose_classid_with`, `render_classid_for_concept`) or a documented
    // public constant (`NodeGuid::CLASSID_*` / `CLASSID_*_LEGACY`) — never a
    // hand-rolled composed-`u32` hex literal (footgun F12). Component-level
    // literals (the `0x1000` V3 marker, `0x0000` core-form custom, AppPrefix
    // values) are used as composer ARGUMENTS, matching the idiom already
    // established by `ogar_codebook.rs`'s own tests (e.g.
    // `classid_route_through_matrix_under_active_and_legacy_order`'s
    // `for prefix in [0x0000u16, 0x0001, 0x0005, 0x1000]`).
    //
    // `NodeGuid::CLASSID_*_V3` / `CLASSID_*_V3_LEGACY` are gated behind the
    // (default-OFF) `guid-v3-tail` feature (Cargo.toml), so this module
    // reproduces their bit patterns unconditionally via the composers
    // instead of depending on that feature.

    /// A CODEBOOK concept id, used as the CANON half in composed test ids.
    fn a_concept() -> u16 {
        canonical_concept_id("patient").expect("patient is a CODEBOOK entry")
    }

    // ── classify_form: one id at a time, worked examples from ogar_codebook.rs ──

    #[test]
    fn classify_form_canon_high_native() {
        // NodeGuid post-flip constants — canon in the HIGH half, native.
        assert_eq!(
            classify_form(NodeGuid::CLASSID_OSINT),
            ClassidForm::CanonHigh
        );
        assert_eq!(classify_form(NodeGuid::CLASSID_FMA), ClassidForm::CanonHigh);
        assert_eq!(
            classify_form(NodeGuid::CLASSID_PROJECT),
            ClassidForm::CanonHigh
        );
        assert_eq!(classify_form(NodeGuid::CLASSID_ERP), ClassidForm::CanonHigh);

        // Post-flip V3-marked forms: canon high, custom == 0x1000 — still
        // native (the marker lives in custom, not canon). Reproduces the
        // `CLASSID_*_V3` bit pattern via the unconditional composer instead
        // of the `guid-v3-tail`-gated constants.
        for &(_, concept) in CODEBOOK {
            let v3_marked = compose_classid_with(ClassidOrder::CanonHigh, concept, 0x1000);
            assert_eq!(classify_form(v3_marked), ClassidForm::CanonHigh);
        }

        // render_classid_for_concept — a real (non-legacy) render classid
        // composed via the sanctioned composer, e.g. MedCare Healthcare/patient.
        let pat = render_classid_for_concept(AppPrefix::Healthcare, "patient").unwrap();
        assert_eq!(classify_form(pat), ClassidForm::CanonHigh);
    }

    #[test]
    fn classify_form_default_class_is_canon_high() {
        assert_eq!(
            classify_form(NodeGuid::CLASSID_DEFAULT),
            ClassidForm::CanonHigh
        );
    }

    #[test]
    fn classify_form_legacy_zero_prefix_high() {
        // The documented public legacy aliases (canonical_node.rs) — the
        // 0x0000_DDCC shape (worked example: `classid_canon_compat_reads_both_stored_forms`
        // pins `classid_canon_compat(0x0000_0901) = 0x0901`, same shape).
        assert_eq!(
            classify_form(NodeGuid::CLASSID_OSINT_LEGACY),
            ClassidForm::LegacyZeroPrefixHigh
        );
        assert_eq!(
            classify_form(NodeGuid::CLASSID_FMA_LEGACY),
            ClassidForm::LegacyZeroPrefixHigh
        );
        assert_eq!(
            classify_form(NodeGuid::CLASSID_PROJECT_LEGACY),
            ClassidForm::LegacyZeroPrefixHigh
        );
        assert_eq!(
            classify_form(NodeGuid::CLASSID_ERP_LEGACY),
            ClassidForm::LegacyZeroPrefixHigh
        );
        // Composed directly via the sanctioned CanonLow composer: custom=0,
        // canon=concept — reproduces the same shape for any codebook entry.
        for &(_, concept) in CODEBOOK {
            let legacy = compose_classid_with(ClassidOrder::CanonLow, concept, 0x0000);
            assert_eq!(classify_form(legacy), ClassidForm::LegacyZeroPrefixHigh);
        }
    }

    #[test]
    fn classify_form_legacy_v3_marker_high() {
        // 0x1000_DDCC shape (worked example: `classid_canon_compat(0x1000_0700) = 0x0700`,
        // same shape as `NodeGuid::CLASSID_OSINT_V3_LEGACY`, `guid-v3-tail`-gated).
        // Composed via the sanctioned CanonLow composer with custom=0x1000
        // (the pre-flip V3-marker position) for every codebook entry.
        for &(_, concept) in CODEBOOK {
            let legacy = compose_classid_with(ClassidOrder::CanonLow, concept, 0x1000);
            assert_eq!(classify_form(legacy), ClassidForm::LegacyV3MarkerHigh);
        }
    }

    #[test]
    fn classify_form_legacy_render_prefix_high() {
        // 0xAAAA_DDCC shape (worked example: `classid_canon_compat(0x0005_0901) = 0x0901`,
        // MedCare's pre-flip Healthcare render pair). Every allocated
        // AppPrefix, composed via the sanctioned CanonLow composer, against
        // every codebook concept.
        for app in [
            AppPrefix::OpenProject,
            AppPrefix::Odoo,
            AppPrefix::Woa,
            AppPrefix::Smb,
            AppPrefix::Healthcare,
            AppPrefix::Redmine,
        ] {
            for &(_, concept) in CODEBOOK {
                let legacy = compose_classid_with(ClassidOrder::CanonLow, concept, app.prefix());
                assert_eq!(
                    classify_form(legacy),
                    ClassidForm::LegacyRenderPrefixHigh,
                    "prefix {:#06x} concept {concept:#06x}",
                    app.prefix()
                );
            }
        }
    }

    #[test]
    fn classify_form_agrees_with_classid_canon_compat_fallback_decision() {
        // classify_form's CanonHigh/old_form split must agree exactly with
        // whether classid_canon_compat needed the CanonLow fallback: when it
        // does NOT need the fallback, canon == classid_canon_compat(id); the
        // legacy shapes are exactly where it DOES need the fallback and the
        // compat answer differs from the naive canon-high read.
        let concept = a_concept();
        let native_ids = [
            NodeGuid::CLASSID_OSINT,
            NodeGuid::CLASSID_FMA,
            NodeGuid::CLASSID_PROJECT,
            NodeGuid::CLASSID_ERP,
            NodeGuid::CLASSID_DEFAULT,
            compose_classid_with(ClassidOrder::CanonHigh, concept, 0x1000), // V3-marked, native
        ];
        for id in native_ids {
            assert_eq!(classify_form(id), ClassidForm::CanonHigh);
            let (canon, _custom) = split_classid(id);
            assert_eq!(
                classid_canon_compat(id),
                canon,
                "CanonHigh-classified id must not need the legacy fallback"
            );
        }

        let legacy_ids = [
            (
                compose_classid_with(ClassidOrder::CanonLow, concept, 0x0000),
                ClassidForm::LegacyZeroPrefixHigh,
            ),
            (
                compose_classid_with(ClassidOrder::CanonLow, concept, 0x1000),
                ClassidForm::LegacyV3MarkerHigh,
            ),
            (
                compose_classid_with(
                    ClassidOrder::CanonLow,
                    concept,
                    AppPrefix::Healthcare.prefix(),
                ),
                ClassidForm::LegacyRenderPrefixHigh,
            ),
        ];
        for (id, expected_form) in legacy_ids {
            assert_eq!(classify_form(id), expected_form);
            let (canon, _custom) = split_classid(id);
            assert_ne!(
                classid_canon_compat(id),
                canon,
                "old_form-classified id must have needed the legacy fallback \
                 (compat answer differs from the naive canon-high read)"
            );
        }
    }

    // ── count_adoption / AdoptionCounts ──

    #[test]
    fn count_adoption_all_canon_high_is_full_adoption() {
        let concept = a_concept();
        let ids = [
            NodeGuid::CLASSID_OSINT,
            NodeGuid::CLASSID_FMA,
            compose_classid_with(ClassidOrder::CanonHigh, concept, 0x1000), // V3-marked, native
        ];
        let counts = count_adoption(ids.into_iter());
        assert_eq!(
            counts,
            AdoptionCounts {
                canon_high: 3,
                old_form: 0,
                ambiguous: 0,
                total: 3,
            }
        );
        assert!((counts.adoption_pct() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn count_adoption_all_old_form_is_zero_adoption() {
        let concept = a_concept();
        let ids = [
            NodeGuid::CLASSID_OSINT_LEGACY,
            compose_classid_with(ClassidOrder::CanonLow, concept, 0x1000),
            compose_classid_with(
                ClassidOrder::CanonLow,
                concept,
                AppPrefix::Healthcare.prefix(),
            ),
        ];
        let counts = count_adoption(ids.into_iter());
        assert_eq!(
            counts,
            AdoptionCounts {
                canon_high: 0,
                old_form: 3,
                ambiguous: 0,
                total: 3,
            }
        );
        assert_eq!(counts.adoption_pct(), 0.0);
    }

    #[test]
    fn count_adoption_mixed_produces_correct_totals_and_pct() {
        // 3 native (one of which is the default class), 3 old_form (one of
        // each legacy shape), 0 ambiguous — the mixed-corpus case.
        let concept = a_concept();
        let ids = [
            NodeGuid::CLASSID_OSINT,                                        // CanonHigh
            compose_classid_with(ClassidOrder::CanonHigh, concept, 0x1000), // CanonHigh (V3-marked)
            NodeGuid::CLASSID_DEFAULT,      // CanonHigh (degenerate)
            NodeGuid::CLASSID_OSINT_LEGACY, // LegacyZeroPrefixHigh
            compose_classid_with(ClassidOrder::CanonLow, concept, 0x1000), // LegacyV3MarkerHigh
            compose_classid_with(
                ClassidOrder::CanonLow,
                concept,
                AppPrefix::Healthcare.prefix(),
            ), // LegacyRenderPrefixHigh
        ];
        let counts = count_adoption(ids.into_iter());
        assert_eq!(
            counts,
            AdoptionCounts {
                canon_high: 3,
                old_form: 3,
                ambiguous: 0,
                total: 6,
            }
        );
        assert!((counts.adoption_pct() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn count_adoption_empty_iterator_is_zero_not_nan() {
        let counts = count_adoption(std::iter::empty());
        assert_eq!(counts, AdoptionCounts::default());
        assert_eq!(counts.adoption_pct(), 0.0);
    }
}
