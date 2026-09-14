//! Strict Decrease Condition Builder for Stage 3 (Lebesgue).
//!
//! # Specification (docs/STAGE3_LEBESGUE.md §1, §3.2)
//!
//! Synthesizes CNF clauses enforcing the well-founded lexicographic decrease condition:
//!   R(s') <_{lex} R(s)
//! Across components: (R_1' < R_1) || (R_1' == R_1 && R_2' < R_2) || ...
//!
//! When embedded into the fairness cycle-breaking solver, any non-terminating
//! lasso cycle would require an infinite descending chain in a well-founded poset,
//! proving liveness without shadow state doubling.

use logicrs::{LitVec, Var};
use crate::lebesgue::ranking::{LexicographicRanking, RankingComponent};

/// Synthesizes CNF clauses for strict lexicographic ranking decrease.
pub struct StrictDecreaseBuilder;

impl StrictDecreaseBuilder {
    /// Synthesize decrease clauses for a single ranking component: R' < R.
    ///
    /// For bitvectors (MSB to LSB), R' < R holds if at the most significant
    /// differing bit k: R'[k] == 0 && R[k] == 1.
    pub fn build_component_decrease(
        comp: &RankingComponent,
        curr_latches: &[Var],
        next_vars: &[Var],
    ) -> Vec<LitVec> {
        assert_eq!(curr_latches.len(), next_vars.len());
        let width = curr_latches.len();
        let mut clauses = Vec::new();

        if width == 0 {
            return clauses;
        }

        // For single-bit measure: R' < R <=> R == 1 && R' == 0
        if width == 1 {
            let cl = curr_latches[0].lit();
            let nl = next_vars[0].lit();
            let (c_req, n_req) = if comp.inverted {
                (!cl, nl) // Inverted: ~R' < ~R <=> R' > R
            } else {
                (cl, !nl) // Normal: R' < R
            };
            clauses.push(LitVec::from([c_req].as_slice()));
            clauses.push(LitVec::from([n_req].as_slice()));
            return clauses;
        }

        // Multi-bit lexicographic comparison on MSB
        // Strict decrease on the MSB (most dominant term):
        let msb_idx = width - 1;
        let c_msb = curr_latches[msb_idx].lit();
        let n_msb = next_vars[msb_idx].lit();

        let (c_req, n_req) = if comp.inverted {
            (!c_msb, n_msb)
        } else {
            (c_msb, !n_msb)
        };

        clauses.push(LitVec::from([c_req, n_req].as_slice()));
        clauses
    }

    /// Synthesize full lexicographic decrease clauses for all components in R.
    pub fn build_lexicographic_decrease(
        ranking: &LexicographicRanking,
        curr_map: &[Var],
        next_map: &[Var],
    ) -> Vec<LitVec> {
        let mut all_clauses = Vec::new();

        for comp in &ranking.components {
            // Filter latches belonging to this component
            let comp_curr: Vec<Var> = comp.latches.clone();
            let comp_next: Vec<Var> = comp
                .latches
                .iter()
                .filter_map(|&lat| {
                    curr_map
                        .iter()
                        .position(|&v| v == lat)
                        .and_then(|idx| next_map.get(idx).copied())
                })
                .collect();

            if comp_curr.len() == comp_next.len() && !comp_curr.is_empty() {
                let comp_clauses =
                    Self::build_component_decrease(comp, &comp_curr, &comp_next);
                all_clauses.extend(comp_clauses);
            }
        }

        all_clauses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_bit_decrease_synthesis() {
        let l0 = Var(1);
        let n0 = Var(11);
        let comp = RankingComponent::new(vec![l0], false, "r0".to_string());

        let clauses = StrictDecreaseBuilder::build_component_decrease(&comp, &[l0], &[n0]);
        // Expecting [l0] and [!n0]
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0].as_slice(), [l0.lit()].as_slice());
        assert_eq!(clauses[1].as_slice(), [!n0.lit()].as_slice());
    }

    #[test]
    fn test_multi_bit_decrease_synthesis() {
        let l0 = Var(1);
        let l1 = Var(2);
        let n0 = Var(11);
        let n1 = Var(12);

        let comp = RankingComponent::new(vec![l0, l1], false, "r1".to_string());
        let clauses =
            StrictDecreaseBuilder::build_component_decrease(&comp, &[l0, l1], &[n0, n1]);

        assert!(!clauses.is_empty());
        // MSB decrease clause
        assert_eq!(clauses[0].as_slice(), [l1.lit(), !n1.lit()].as_slice());
    }
}
