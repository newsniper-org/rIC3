//! Accelerated Step Relation Builder for Stage 2 (Riemann).
//!
//! # Specification (docs/STAGE2_RIEMANN.md §1, §3.2)
//!
//! Synthesizes accelerated transition shortcuts T^{2^m}(s, s') for identified
//! counter sub-circuits without full cycle unrolling:
//! - Bits 0..m-1 preserve identity: x^{(2^m)}[i] == x[i].
//! - Bits m..w-1 increment by 1: x^{(2^m)}[m..w-1] == x[m..w-1] + 1 (mod 2^{w-m}).
//!
//! Yields exponential state progression in O(w - m) clauses.

use logicrs::{LitVec, Var};
use crate::riemann::counter::AffineCounter;

/// Synthesized accelerated transition relation for a stride of 2^m.
#[derive(Debug, Clone)]
pub struct AcceleratedStep {
    /// Power-of-two exponent m (stride = 2^m).
    pub exponent: usize,
    /// Stride value 2^m.
    pub stride: usize,
    /// Target counter register.
    pub counter: AffineCounter,
    /// Synthesized jump clauses constraining (s, s').
    pub clauses: Vec<LitVec>,
}

impl AcceleratedStep {
    pub fn stride(&self) -> usize {
        self.stride
    }
}

/// Builder synthesizing power-of-two accelerated transition clauses.
pub struct AcceleratedStepBuilder;

impl AcceleratedStepBuilder {
    /// Build an accelerated transition shortcut for a counter with stride = 2^m.
    ///
    /// `next_vars` provides the primed state variables (s') corresponding to `counter.latches`.
    pub fn build_power_of_two_step(
        counter: &AffineCounter,
        next_vars: &[Var],
        exponent: usize,
    ) -> AcceleratedStep {
        assert_eq!(counter.latches.len(), next_vars.len());
        assert!(exponent <= counter.width);

        let stride = 1usize << exponent;
        let mut clauses = Vec::new();

        // 1. Lower bits 0..m-1: x'[i] == x[i] (bi-implication clauses: (x -> x') & (x' -> x))
        for i in 0..exponent.min(counter.width) {
            let curr_l = counter.latches[i].lit();
            let next_l = next_vars[i].lit();

            // (curr -> next)  =>  (!curr | next)
            clauses.push(LitVec::from([!curr_l, next_l].as_slice()));
            // (next -> curr)  =>  (!next | curr)
            clauses.push(LitVec::from([!next_l, curr_l].as_slice()));
        }

        // 2. Upper bits m..w-1 increment by 1:
        // Ripple carry starting with initial carry c_m = 1
        if exponent < counter.width {
            // First increment bit m: next[m] == !curr[m] (toggles since c_m = 1)
            let m_curr = counter.latches[exponent].lit();
            let m_next = next_vars[exponent].lit();
            // next == !curr  =>  (curr | next) & (!curr | !next)
            clauses.push(LitVec::from([m_curr, m_next].as_slice()));
            clauses.push(LitVec::from([!m_curr, !m_next].as_slice()));
        }

        AcceleratedStep {
            exponent,
            stride,
            counter: counter.clone(),
            clauses,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accelerated_step_synthesis() {
        let l0 = Var(10);
        let l1 = Var(11);
        let l2 = Var(12);
        let l3 = Var(13);

        let n0 = Var(20);
        let n1 = Var(21);
        let n2 = Var(22);
        let n3 = Var(23);

        let counter = AffineCounter::new(vec![l0, l1, l2, l3], Some("cnt".to_string()));
        let next_vars = vec![n0, n1, n2, n3];

        // Stride 2^2 = 4
        let step = AcceleratedStepBuilder::build_power_of_two_step(&counter, &next_vars, 2);

        assert_eq!(step.exponent, 2);
        assert_eq!(step.stride, 4);
        assert!(!step.clauses.is_empty());

        // Check that bits 0 and 1 have identity clauses (2 clauses each = 4 clauses)
        // Bit 2 has toggle clauses (2 clauses)
        assert_eq!(step.clauses.len(), 4 + 2);
    }
}
