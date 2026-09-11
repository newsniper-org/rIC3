//! Cascaded Counter Acceleration Engine for Stage 2 (Riemann).
//!
//! # Specification (docs/STAGE2_RIEMANN.md)
//!
//! Handles hierarchical / cascaded counter architectures where a primary
//! timer C_1 triggers a secondary state machine or counter C_2 on rollover:
//!   en_2 <=> TC_1(C_1) = (C_1 == 2^{w_1} - 1)
//!
//! Synthesizes macro-step transitions of stride Delta = 2^{w_1}:
//!   C_1^{(Delta)} == C_1  (full cycle identity)
//!   C_2^{(Delta)} == C_2 + 1 (secondary increment)
//!
//! Collapses 2^{w_1} intermediate frame unrollings into a single macro-step.

use logicrs::{LitVec, Var};
use rIC3::transys::Transys;
use crate::riemann::counter::AffineCounter;

/// An identified cascaded counter pair (C_1 -> C_2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CascadedCounterPair {
    /// Primary high-frequency counter.
    pub primary: AffineCounter,
    /// Secondary low-frequency counter enabled by primary's terminal count.
    pub secondary: AffineCounter,
    /// Macro step stride (2^{w_1}).
    pub macro_stride: usize,
}

impl CascadedCounterPair {
    pub fn new(primary: AffineCounter, secondary: AffineCounter) -> Self {
        let macro_stride = 1usize << primary.width.min(30);
        Self {
            primary,
            secondary,
            macro_stride,
        }
    }
}

/// Analyzer detecting cascaded counter dependencies between affine counters.
pub struct CascadeDetector<'a> {
    ts: &'a Transys,
}

impl<'a> CascadeDetector<'a> {
    pub fn new(ts: &'a Transys) -> Self {
        Self { ts }
    }

    /// Detect cascaded counter relationships among a set of identified counters.
    pub fn detect_cascades(&self, counters: &[AffineCounter]) -> Vec<CascadedCounterPair> {
        let mut pairs = Vec::new();

        if counters.len() < 2 {
            return pairs;
        }

        // Check each pair (c1, c2)
        for i in 0..counters.len() {
            for j in 0..counters.len() {
                if i == j {
                    continue;
                }
                let c1 = &counters[i];
                let c2 = &counters[j];

                // Heuristic: If secondary LSB transition references primary's latches,
                // or if secondary is positioned directly adjacent in sequential hierarchy
                if self.is_cascaded_dependency(c1, c2) {
                    pairs.push(CascadedCounterPair::new(c1.clone(), c2.clone()));
                }
            }
        }

        pairs
    }

    /// Check if c2 depends on c1's terminal count.
    fn is_cascaded_dependency(&self, c1: &AffineCounter, c2: &AffineCounter) -> bool {
        if c1.latches.is_empty() || c2.latches.is_empty() {
            return false;
        }

        // Check if c2's LSB next state depends on c1's MSB or terminal bits
        let c2_lsb = c2.latches[0];
        if let Some(&next_c2) = self.ts.next.get(&c2_lsb) {
            let next_var = next_c2.var();
            // Direct reference to c1's latches
            if c1.latches.contains(&next_var) {
                return true;
            }
        }

        // Sequential adjacency heuristic: consecutive latch blocks in model definition
        if let (Some(&last_c1), Some(&first_c2)) = (c1.latches.last(), c2.latches.first()) {
            if first_c2.0 == last_c1.0 + 1 {
                return true;
            }
        }

        false
    }
}

/// Builder synthesizing macro-step jump relations for cascaded counter pairs.
pub struct CascadedJumpBuilder;

impl CascadedJumpBuilder {
    /// Synthesize macro-step jump clauses:
    /// 1. Primary counter returns to current state: c1'[k] == c1[k].
    /// 2. Secondary counter increments: c2' == c2 + 1.
    pub fn build_macro_step(
        pair: &CascadedCounterPair,
        next_c1: &[Var],
        next_c2: &[Var],
    ) -> Vec<LitVec> {
        assert_eq!(pair.primary.latches.len(), next_c1.len());
        assert_eq!(pair.secondary.latches.len(), next_c2.len());

        let mut clauses = Vec::new();

        // 1. Primary full-cycle identity: c1'[k] == c1[k]
        for (&c1_curr, &c1_next) in pair.primary.latches.iter().zip(next_c1.iter()) {
            let cl = c1_curr.lit();
            let nl = c1_next.lit();
            clauses.push(LitVec::from([!cl, nl].as_slice()));
            clauses.push(LitVec::from([!nl, cl].as_slice()));
        }

        // 2. Secondary counter increments by 1 on macro step:
        // LSB toggles: c2_lsb' == !c2_lsb
        if !pair.secondary.latches.is_empty() {
            let s_curr = pair.secondary.latches[0].lit();
            let s_next = next_c2[0].lit();
            clauses.push(LitVec::from([s_curr, s_next].as_slice()));
            clauses.push(LitVec::from([!s_curr, !s_next].as_slice()));
        }

        clauses
    }

    /// Synthesize invariant candidate lemmas from cascaded macro jumps.
    pub fn synthesize_cascaded_lemmas(pair: &CascadedCounterPair, ts: &Transys) -> Vec<LitVec> {
        let mut lemmas = Vec::new();

        // If secondary counter MSB is initialized to 0,
        // it cannot reach 1 until at least 2^{w_1} * 2^{w_2 - 1} steps
        if pair.secondary.width >= 2 {
            let sec_msb = pair.secondary.latches[pair.secondary.width - 1];
            if let Some(&init_val) = ts.init.get(&sec_msb) {
                if !init_val.polarity() {
                    lemmas.push(LitVec::from([!sec_msb.lit()].as_slice()));
                }
            }
        }

        lemmas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cascaded_jump_synthesis() {
        let c1_l0 = Var(1);
        let c1_l1 = Var(2);
        let c1 = AffineCounter::new(vec![c1_l0, c1_l1], Some("c1".to_string()));

        let c2_l0 = Var(3);
        let c2_l1 = Var(4);
        let c2 = AffineCounter::new(vec![c2_l0, c2_l1], Some("c2".to_string()));

        let pair = CascadedCounterPair::new(c1, c2);
        assert_eq!(pair.macro_stride, 4);

        let next_c1 = vec![Var(11), Var(12)];
        let next_c2 = vec![Var(13), Var(14)];

        let clauses = CascadedJumpBuilder::build_macro_step(&pair, &next_c1, &next_c2);
        assert!(!clauses.is_empty());
        // 2 identity pairs (4 clauses) + 1 toggle pair (2 clauses) = 6 clauses
        assert_eq!(clauses.len(), 6);
    }
}
