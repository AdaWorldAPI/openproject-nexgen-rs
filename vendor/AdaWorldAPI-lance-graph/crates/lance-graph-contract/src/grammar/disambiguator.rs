//! Generalized disambiguation trait.
//!
//! PR #279 outlook E5: ContextChain::disambiguate is a domain-agnostic
//! primitive. Generalize so the same machinery serves:
//!   - coreference (current usage)
//!   - RLS predicate selection over LogicalPlan
//!   - AriGraph triplet completion
//!   - thinking-style dispatch
//!
//! META-AGENT: `pub mod disambiguator;` in mod.rs.

/// Anything that can carry a similarity / coherence score against a
/// surrounding context. The score function is domain-specific.
pub trait Disambiguatable: Sized {
    /// Score this candidate against the focal item + its surrounding context.
    /// Higher = better fit.
    fn score_against(&self, focal: &Self, context: &[Self]) -> f32;
}

#[derive(Debug, Clone)]
pub struct GeneralizedResult<T> {
    pub winner: T,
    pub winner_index: usize,
    pub margin: f32,     // score of winner - score of runner-up
    pub dispersion: f32, // mean pairwise distance among top candidates
    pub candidate_count: usize,
}

/// Generalized disambiguation. Iterate candidates, score each against
/// (focal, context), pick the winner with non-trivial margin.
pub fn disambiguate_general<T: Disambiguatable + Clone, I: IntoIterator<Item = T>>(
    focal: &T,
    context: &[T],
    candidates: I,
) -> GeneralizedResult<T> {
    let scored: Vec<(T, f32)> = candidates
        .into_iter()
        .map(|c| {
            let s = c.score_against(focal, context);
            (c, s)
        })
        .collect();
    let candidate_count = scored.len();
    if candidate_count == 0 {
        return GeneralizedResult {
            winner: focal.clone(),
            winner_index: usize::MAX,
            margin: 0.0,
            dispersion: 0.0,
            candidate_count: 0,
        };
    }
    let mut sorted: Vec<(usize, T, f32)> = scored
        .into_iter()
        .enumerate()
        .map(|(i, (t, s))| (i, t, s))
        .collect();
    sorted.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let winner = sorted[0].clone();
    let margin = if sorted.len() > 1 {
        winner.2 - sorted[1].2
    } else {
        winner.2
    };
    let dispersion = if sorted.len() >= 2 {
        let top: Vec<f32> = sorted
            .iter()
            .take(3.min(sorted.len()))
            .map(|x| x.2)
            .collect();
        let mean = top.iter().sum::<f32>() / top.len() as f32;
        let var = top.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / top.len() as f32;
        var.sqrt()
    } else {
        0.0
    };
    GeneralizedResult {
        winner: winner.1,
        winner_index: winner.0,
        margin,
        dispersion,
        candidate_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct ScalarItem(f32);
    impl Disambiguatable for ScalarItem {
        fn score_against(&self, focal: &Self, _ctx: &[Self]) -> f32 {
            -(self.0 - focal.0).abs()
        }
    }

    #[test]
    fn picks_closest_to_focal() {
        let focal = ScalarItem(5.0);
        let candidates = vec![ScalarItem(1.0), ScalarItem(4.5), ScalarItem(10.0)];
        let r = disambiguate_general(&focal, &[], candidates);
        assert_eq!(r.winner_index, 1);
        assert!(r.margin > 0.0);
    }

    #[test]
    fn empty_candidates_returns_focal() {
        let focal = ScalarItem(5.0);
        let r: GeneralizedResult<ScalarItem> =
            disambiguate_general(&focal, &[], std::iter::empty());
        assert_eq!(r.candidate_count, 0);
    }

    #[test]
    fn single_candidate_has_full_margin() {
        let focal = ScalarItem(0.0);
        let r = disambiguate_general(&focal, &[], vec![ScalarItem(2.0)]);
        assert_eq!(r.candidate_count, 1);
        assert_eq!(r.winner_index, 0);
        // With a single candidate, margin equals its raw score.
        assert!((r.margin - (-2.0)).abs() < 1e-6);
    }

    #[test]
    fn dispersion_is_zero_for_identical_top_candidates() {
        let focal = ScalarItem(0.0);
        let r = disambiguate_general(
            &focal,
            &[],
            vec![ScalarItem(1.0), ScalarItem(1.0), ScalarItem(1.0)],
        );
        assert!(r.dispersion.abs() < 1e-6);
    }
}
