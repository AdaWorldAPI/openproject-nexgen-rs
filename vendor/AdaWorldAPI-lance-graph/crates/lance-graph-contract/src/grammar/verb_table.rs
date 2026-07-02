//! 144-cell verb-role lookup table â 12 semantic families Ã 12 tense/aspect/mood.
//!
//! Each cell holds a TEKAMOLO slot prior: which slots a verb of this family
//! and tense expects to be filled. Parsing reduces to (family, tense) â
//! row â fill slots from morphology â NARS-revise truth.
//!
//! Slot priors seeded from grammar-landscape.md Â§3 TEKAMOLO semantics.
//! Starter values â tune empirically with corpus statistics.
//!
//! See PR #279 outlook E3 + grammar-landscape.md Â§9.
//!
//! META-AGENT: `pub mod verb_table;` to mod.rs.
//!
//! ## Tense modulation (G4 loose end)
//!
//! Earlier seed broadcast 12 family priors across all 12 tenses, producing a
//! degenerate 12-unique-value table with zero tense x family interaction. The
//! refactor introduces `SlotPriorDelta` + `SlotPrior::combine` and a
//! `tense_modifier(Tense)` function so each cell becomes
//! `final = base.combine(tense_modifier(tense))`.
//!
//! Modifiers are linguistically grounded in standard English grammar
//! (Quirk, Greenbaum, Leech & Svartvik, *A Comprehensive Grammar of the
//! English Language*, Longman 1985, sections 4.21-4.27 on tense / aspect /
//! mood):
//!
//! - Perfect aspects (Perfect, Pluperfect, FuturePerfect) emphasise
//!   completion and therefore temporal anchoring -> `temporal +0.15`.
//! - Continuous (progressive) aspects emphasise an ongoing process ->
//!   `temporal +0.10`, `modal -0.05` (less anchored, less modal weight).
//! - Imperative is a timeless directive command -> `temporal -0.20`,
//!   `modal +0.20`.
//! - Potential (irrealis / possibility mood; this enum's stand-in for the
//!   Subjunctive) emphasises possibility -> `temporal -0.10`, `modal +0.25`,
//!   `kausal -0.05` (cause is hypothetical).
//! - Habitual is recurring-as-timeless -> `temporal -0.10`, `modal +0.05`.
//! - Default (Present, Past, Future) leaves the base prior untouched.
//!
//! All resulting axes are clamped to [0.0, 1.0] in `SlotPrior::combine`.

use crate::grammar::role_keys::Tense;

/// Twelve top-level semantic families. The naming is deliberately
/// process-oriented (verbs as transformations on configurations of
/// the world) rather than syntax-oriented â these are the "roles a
/// predicate plays" that disambiguate which TEKAMOLO slots get filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerbFamily {
    Becomes,
    Causes,
    Supports,
    Contradicts,
    Refines,
    Grounds,
    Abstracts,
    Enables,
    Prevents,
    Transforms,
    Mirrors,
    Dissolves,
}

impl VerbFamily {
    pub const ALL: [Self; 12] = [
        Self::Becomes,
        Self::Causes,
        Self::Supports,
        Self::Contradicts,
        Self::Refines,
        Self::Grounds,
        Self::Abstracts,
        Self::Enables,
        Self::Prevents,
        Self::Transforms,
        Self::Mirrors,
        Self::Dissolves,
    ];
}

/// Slot prior per TEKAMOLO axis. Cells in [0.0, 1.0]: 0 = slot rarely filled,
/// 1 = slot always filled.
#[derive(Debug, Clone, Copy)]
pub struct SlotPrior {
    pub temporal: f32,
    pub kausal: f32,
    pub modal: f32,
    pub lokal: f32,
    pub instrument: f32,
}

impl SlotPrior {
    pub const fn uniform() -> Self {
        Self {
            temporal: 0.5,
            kausal: 0.5,
            modal: 0.5,
            lokal: 0.5,
            instrument: 0.5,
        }
    }

    /// Apply a tense-driven delta to each axis and clamp the result to
    /// `[0.0, 1.0]`. This is how the broadcast-flat 12 priors per family
    /// gain tense x family interaction (G4 loose end).
    pub fn combine(self, delta: SlotPriorDelta) -> Self {
        fn clamp(x: f32) -> f32 {
            x.clamp(0.0, 1.0)
        }
        Self {
            temporal: clamp(self.temporal + delta.temporal),
            kausal: clamp(self.kausal + delta.kausal),
            modal: clamp(self.modal + delta.modal),
            lokal: clamp(self.lokal + delta.lokal),
            instrument: clamp(self.instrument + delta.instrument),
        }
    }
}

/// Additive delta applied to a `SlotPrior` per tense. Each axis is summed
/// with the base prior and clamped via `SlotPrior::combine`. Default = no
/// change (all zeros).
#[derive(Debug, Clone, Copy, Default)]
pub struct SlotPriorDelta {
    pub temporal: f32,
    pub kausal: f32,
    pub modal: f32,
    pub lokal: f32,
    pub instrument: f32,
}

/// Tense-driven modifier table. Linguistic grounding: Quirk et al.
/// *Comprehensive Grammar of the English Language* sections 4.21-4.27.
/// See module-level doc comment for the per-tense rationale.
pub fn tense_modifier(tense: Tense) -> SlotPriorDelta {
    use Tense::*;
    match tense {
        // Perfect aspects emphasise completion -> temporal anchoring.
        Perfect | Pluperfect | FuturePerfect => SlotPriorDelta {
            temporal: 0.15,
            kausal: 0.0,
            modal: 0.0,
            lokal: 0.0,
            instrument: 0.0,
        },
        // Continuous (progressive) aspects emphasise ongoing process.
        PresentContinuous | PastContinuous | FutureContinuous => SlotPriorDelta {
            temporal: 0.10,
            kausal: 0.0,
            modal: -0.05,
            lokal: 0.0,
            instrument: 0.0,
        },
        // Imperative: timeless directive -> suppresses temporal, amplifies modal.
        Imperative => SlotPriorDelta {
            temporal: -0.20,
            kausal: 0.0,
            modal: 0.20,
            lokal: 0.0,
            instrument: 0.0,
        },
        // Potential (irrealis / subjunctive role): possibility -> modal up,
        // kausal slightly down (cause is hypothetical), temporal slightly down.
        Potential => SlotPriorDelta {
            temporal: -0.10,
            kausal: -0.05,
            modal: 0.25,
            lokal: 0.0,
            instrument: 0.0,
        },
        // Habitual: recurring-as-timeless.
        Habitual => SlotPriorDelta {
            temporal: -0.10,
            kausal: 0.0,
            modal: 0.05,
            lokal: 0.0,
            instrument: 0.0,
        },
        // Present, Past, Future: unmarked tense, no modifier.
        Present | Past | Future => SlotPriorDelta::default(),
    }
}

/// 144-cell lookup: rows = `VerbFamily`, columns = `Tense`. Indexing is
/// by enum discriminant (`as usize`), so any future reordering of either
/// enum must keep `#[repr(u8)]` (or equivalent) and contiguous indices.
pub struct VerbRoleTable {
    cells: [[SlotPrior; 12]; 12],
}

impl VerbRoleTable {
    pub fn new_uniform() -> Self {
        Self {
            cells: [[SlotPrior::uniform(); 12]; 12],
        }
    }
    pub fn lookup(&self, family: VerbFamily, tense: Tense) -> SlotPrior {
        self.cells[family as usize][tense as usize]
    }
    pub fn set(&mut self, family: VerbFamily, tense: Tense, prior: SlotPrior) {
        self.cells[family as usize][tense as usize] = prior;
    }
}

/// Default table with hand-set families per the plan's table and
/// grammar-landscape.md Â§3 TEKAMOLO slot semantics.
///
/// Semantic profiles â starter â tune empirically:
///   BECOMES    â Change verb: high Temporal + Modal
///   CAUSES     â Action verb: high Kausal + Instrument
///   SUPPORTS   â State verb:  high Modal, low Temporal
///   CONTRADICTS â State verb: high Modal + Kausal
///   REFINES    â State verb:  high Modal, moderate Kausal
///   GROUNDS    â State verb:  high Lokal + Modal
///   ABSTRACTS  â Change verb: high Modal + Temporal
///   ENABLES    â Discovery verb: high Kausal + Lokal
///   PREVENTS   â Action verb: high Kausal + Temporal
///   TRANSFORMS â Action verb: high Kausal + Temporal + Instrument
///   MIRRORS    â Change verb: high Temporal + Modal + Lokal
///   DISSOLVES  â Change verb: high Temporal + Modal
///
/// The numbers are *priors*, not facts: a future PR replaces them
/// with corpus-derived statistics. Mark this `// starter â tune empirically`
/// in any consumer that depends on specific values.
/// Base prior for a `VerbFamily` (pre-tense-modulation). The full per-cell
/// prior is `base_prior(family).combine(tense_modifier(tense))`.
pub fn base_prior(family: VerbFamily) -> SlotPrior {
    match family {
        // --- Change verbs: high Temporal + Modal ---
        VerbFamily::Becomes => SlotPrior {
            temporal: 0.9,
            kausal: 0.2,
            modal: 0.7,
            lokal: 0.3,
            instrument: 0.2,
        },
        VerbFamily::Dissolves => SlotPrior {
            temporal: 0.85,
            kausal: 0.3,
            modal: 0.7,
            lokal: 0.25,
            instrument: 0.2,
        },
        VerbFamily::Abstracts => SlotPrior {
            temporal: 0.7,
            kausal: 0.25,
            modal: 0.85,
            lokal: 0.15,
            instrument: 0.2,
        },
        VerbFamily::Mirrors => SlotPrior {
            temporal: 0.75,
            kausal: 0.2,
            modal: 0.7,
            lokal: 0.6,
            instrument: 0.15,
        },
        // --- Action verbs: high Kausal + Temporal ---
        VerbFamily::Causes => SlotPrior {
            temporal: 0.4,
            kausal: 0.95,
            modal: 0.4,
            lokal: 0.3,
            instrument: 0.5,
        },
        VerbFamily::Prevents => SlotPrior {
            temporal: 0.7,
            kausal: 0.9,
            modal: 0.4,
            lokal: 0.25,
            instrument: 0.35,
        },
        VerbFamily::Transforms => SlotPrior {
            temporal: 0.8,
            kausal: 0.85,
            modal: 0.35,
            lokal: 0.3,
            instrument: 0.6,
        },
        // --- State verbs: high Modal, low Temporal ---
        VerbFamily::Supports => SlotPrior {
            temporal: 0.2,
            kausal: 0.35,
            modal: 0.85,
            lokal: 0.2,
            instrument: 0.3,
        },
        VerbFamily::Contradicts => SlotPrior {
            temporal: 0.15,
            kausal: 0.7,
            modal: 0.9,
            lokal: 0.15,
            instrument: 0.1,
        },
        VerbFamily::Refines => SlotPrior {
            temporal: 0.3,
            kausal: 0.4,
            modal: 0.8,
            lokal: 0.2,
            instrument: 0.35,
        },
        VerbFamily::Grounds => SlotPrior {
            temporal: 0.25,
            kausal: 0.3,
            modal: 0.75,
            lokal: 0.85,
            instrument: 0.2,
        },
        // --- Discovery / enablement: high Kausal + Lokal ---
        VerbFamily::Enables => SlotPrior {
            temporal: 0.35,
            kausal: 0.8,
            modal: 0.4,
            lokal: 0.7,
            instrument: 0.45,
        },
    }
}

pub fn default_table() -> VerbRoleTable {
    let mut t = VerbRoleTable::new_uniform();
    for family in VerbFamily::ALL {
        let base = base_prior(family);
        for tense in Tense::ALL {
            t.set(family, tense, base.combine(tense_modifier(tense)));
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_144_cells() {
        let t = VerbRoleTable::new_uniform();
        let mut count = 0;
        for f in VerbFamily::ALL.iter() {
            for tense_idx in 0..12 {
                let _ = t.cells[*f as usize][tense_idx];
                count += 1;
            }
        }
        assert_eq!(count, 144);
    }

    #[test]
    fn lookup_returns_uniform_for_unset_cell() {
        let t = VerbRoleTable::new_uniform();
        let p = t.lookup(VerbFamily::Mirrors, Tense::Pluperfect);
        assert!((p.temporal - 0.5).abs() < 1e-6);
    }

    #[test]
    fn default_table_overrides_some_cells() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Causes, Tense::Present);
        assert!(p.kausal > 0.8);
    }

    // --- Per-family tests: verify priors are non-zero for at least 2 TEKAMOLO slots ---

    /// Helper: count slots that are non-uniform (differ from 0.5 by > 0.05).
    fn count_non_uniform(p: &SlotPrior) -> usize {
        let slots = [p.temporal, p.kausal, p.modal, p.lokal, p.instrument];
        slots.iter().filter(|&&v| (v - 0.5).abs() > 0.05).count()
    }

    #[test]
    fn becomes_change_verb_temporal_modal() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Becomes, Tense::Present);
        assert!(p.temporal > 0.7, "Becomes should have high temporal");
        assert!(p.modal > 0.6, "Becomes should have high modal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn causes_action_verb_kausal_instrument() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Causes, Tense::Past);
        assert!(p.kausal > 0.8, "Causes should have high kausal");
        assert!(p.instrument > 0.4, "Causes should have elevated instrument");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn supports_state_verb_modal_high() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Supports, Tense::Present);
        assert!(p.modal > 0.7, "Supports should have high modal");
        assert!(p.temporal < 0.4, "Supports should have low temporal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn contradicts_state_verb_modal_kausal() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Contradicts, Tense::Future);
        assert!(p.modal > 0.8, "Contradicts should have high modal");
        assert!(p.kausal > 0.6, "Contradicts should have elevated kausal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn refines_state_verb_modal() {
        // Tense::Present is unmarked (no modifier) so the family-level base
        // prior is preserved. (Under tense modulation, Perfect adds +0.15 to
        // temporal, which would push Refines.temporal from 0.3 to 0.45.)
        let t = default_table();
        let p = t.lookup(VerbFamily::Refines, Tense::Present);
        assert!(p.modal > 0.7, "Refines should have high modal");
        assert!(p.temporal < 0.4, "Refines should have low temporal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn grounds_state_verb_lokal_modal() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Grounds, Tense::Habitual);
        assert!(p.lokal > 0.7, "Grounds should have high lokal");
        assert!(p.modal > 0.6, "Grounds should have elevated modal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn abstracts_change_verb_modal_temporal() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Abstracts, Tense::PresentContinuous);
        assert!(p.modal > 0.7, "Abstracts should have high modal");
        assert!(p.temporal > 0.6, "Abstracts should have elevated temporal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn enables_discovery_verb_kausal_lokal() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Enables, Tense::Potential);
        assert!(p.kausal > 0.7, "Enables should have high kausal");
        assert!(p.lokal > 0.6, "Enables should have elevated lokal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn prevents_action_verb_kausal_temporal() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Prevents, Tense::Past);
        assert!(p.kausal > 0.8, "Prevents should have high kausal");
        assert!(p.temporal > 0.6, "Prevents should have elevated temporal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn transforms_action_verb_kausal_temporal_instrument() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Transforms, Tense::FuturePerfect);
        assert!(p.kausal > 0.7, "Transforms should have high kausal");
        assert!(p.temporal > 0.7, "Transforms should have high temporal");
        assert!(
            p.instrument > 0.5,
            "Transforms should have elevated instrument"
        );
        assert!(count_non_uniform(&p) >= 3);
    }

    #[test]
    fn mirrors_change_verb_temporal_modal_lokal() {
        let t = default_table();
        let p = t.lookup(VerbFamily::Mirrors, Tense::Pluperfect);
        assert!(p.temporal > 0.6, "Mirrors should have elevated temporal");
        assert!(p.modal > 0.6, "Mirrors should have elevated modal");
        assert!(p.lokal > 0.5, "Mirrors should have elevated lokal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn dissolves_change_verb_temporal_modal() {
        // Use Tense::Present (unmarked) so the family base prior is preserved.
        // Imperative would suppress temporal by 0.20 (0.85 -> 0.65 < 0.7) and
        // amplify modal — those are tested in `test_imperative_suppresses_temporal`.
        let t = default_table();
        let p = t.lookup(VerbFamily::Dissolves, Tense::Present);
        assert!(p.temporal > 0.7, "Dissolves should have high temporal");
        assert!(p.modal > 0.6, "Dissolves should have elevated modal");
        assert!(count_non_uniform(&p) >= 2);
    }

    #[test]
    fn all_families_have_non_uniform_priors() {
        let t = default_table();
        for family in VerbFamily::ALL {
            let p = t.lookup(family, Tense::Present);
            assert!(
                count_non_uniform(&p) >= 2,
                "{:?} should have at least 2 non-uniform TEKAMOLO slots",
                family
            );
        }
    }

    // --- Tense modulation tests (G4 loose end: priors must vary across tenses
    // within a family; broadcast-flat 12-priors-across-12-tenses produces
    // zero tense×family interaction). ---

    /// Failing-test-first: Perfect aspect (completion → temporal anchoring
    /// per Quirk et al. CGEL §4.21–4.27) must yield strictly higher temporal
    /// prior than the unmarked Past for the same family.
    #[test]
    fn test_perfect_amplifies_temporal_within_family() {
        let t = default_table();
        let perfect = t.lookup(VerbFamily::Causes, Tense::Perfect);
        let past = t.lookup(VerbFamily::Causes, Tense::Past);
        assert!(
            perfect.temporal > past.temporal,
            "Perfect should amplify temporal over Past for Causes; got perfect={} past={}",
            perfect.temporal,
            past.temporal
        );
    }

    /// Imperative (timeless command) suppresses temporal in favour of modal.
    #[test]
    fn test_imperative_suppresses_temporal() {
        let t = default_table();
        let imperative = t.lookup(VerbFamily::Causes, Tense::Imperative);
        let present = t.lookup(VerbFamily::Causes, Tense::Present);
        assert!(
            imperative.temporal < present.temporal,
            "Imperative should suppress temporal vs Present for Causes; got imp={} pres={}",
            imperative.temporal,
            present.temporal
        );
        assert!(
            imperative.modal > present.modal,
            "Imperative should amplify modal vs Present for Causes; got imp={} pres={}",
            imperative.modal,
            present.modal
        );
    }

    /// Subjunctive equivalent — this enum has Potential (irrealis/possibility
    /// mood), which fills the Subjunctive role. Potential should amplify modal
    /// over Present.
    #[test]
    fn test_subjunctive_amplifies_modal() {
        let t = default_table();
        let potential = t.lookup(VerbFamily::Supports, Tense::Potential);
        let present = t.lookup(VerbFamily::Supports, Tense::Present);
        assert!(
            potential.modal > present.modal,
            "Potential (subjunctive role) should amplify modal vs Present for Supports; \
             got pot={} pres={}",
            potential.modal,
            present.modal
        );
    }

    /// Sanity: continuous aspects amplify temporal but less than perfect.
    /// Use `Causes` (temporal base 0.4) so neither modifier saturates at 1.0.
    #[test]
    fn test_continuous_amplifies_temporal_less_than_perfect() {
        let t = default_table();
        let cont = t.lookup(VerbFamily::Causes, Tense::PresentContinuous);
        let perf = t.lookup(VerbFamily::Causes, Tense::Perfect);
        let pres = t.lookup(VerbFamily::Causes, Tense::Present);
        assert!(
            cont.temporal > pres.temporal,
            "Continuous > Present temporal"
        );
        assert!(
            perf.temporal > cont.temporal,
            "Perfect > Continuous temporal"
        );
    }

    /// Sanity: clamp to [0,1] holds even when base prior is near saturation.
    #[test]
    fn test_combine_clamps_to_unit_interval() {
        let t = default_table();
        // Causes has kausal=0.95 base; no tense modifier touches kausal,
        // but Perfect adds +0.15 to temporal where Causes.temporal=0.4 → 0.55.
        let p = t.lookup(VerbFamily::Causes, Tense::Perfect);
        assert!(p.temporal >= 0.0 && p.temporal <= 1.0);
        assert!(p.kausal >= 0.0 && p.kausal <= 1.0);
        assert!(p.modal >= 0.0 && p.modal <= 1.0);
        assert!(p.lokal >= 0.0 && p.lokal <= 1.0);
        assert!(p.instrument >= 0.0 && p.instrument <= 1.0);
    }
}
