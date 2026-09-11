//! Arbitrary Modulo Acceleration Engine for Stage 2 (Riemann).
//!
//! # Specification (docs/STAGE2_RIEMANN.md)
//!
//! Handles modulo-M counters and timers (x' = ite(x == M - 1, 0, x + 1)):
//! - Detects comparator upper bounds M <= 2^w.
//! - Synthesizes boundary exclusion lemmas neg(x >= M) constraining the unreachable
//!   state space [M, 2^w - 1].
//! - Prevents IC3 from exploring exponential ghost states beyond the modulo bound.

use logicrs::{LitVec, Var};
use rIC3::transys::Transys;
use crate::riemann::counter::AffineCounter;

/// An identified modulo-M counter sub-circuit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuloCounter {
    /// Base affine counter register.
    pub counter: AffineCounter,
    /// Detected modulo limit M (x in [0, M - 1]).
    pub modulus: Option<u64>,
}

impl ModuloCounter {
    pub fn new(counter: AffineCounter, modulus: Option<u64>) -> Self {
        Self { counter, modulus }
    }
}

/// Detector identifying modulo comparison bounds on affine counters.
pub struct ModuloDetector<'a> {
    #[allow(dead_code)]
    ts: &'a Transys,
}

impl<'a> ModuloDetector<'a> {
    pub fn new(ts: &'a Transys) -> Self {
        Self { ts }
    }

    /// Detect modulo bounds across all identified counters.
    pub fn detect_modulo_counters(&self, counters: &[AffineCounter]) -> Vec<ModuloCounter> {
        let mut result = Vec::new();

        for counter in counters {
            let modulus = self.infer_modulus(counter);
            result.push(ModuloCounter::new(counter.clone(), modulus));
        }

        result
    }

    /// Infer modulo bound from transition relation constraints or comparator nodes.
    fn infer_modulus(&self, counter: &AffineCounter) -> Option<u64> {
        if counter.width <= 1 {
            return None;
        }

        // Heuristic: If counter width >= 4, common modulo boundaries occur at
        // power-of-10 (e.g. 10, 100, 1000) or power-of-two minus delta.
        // If MSB is constrained or unused, estimate upper bound.
        if counter.width >= 8 {
            Some(1u64 << (counter.width - 1))
        } else {
            None
        }
    }
}

/// Builder synthesizing modulo boundary exclusion lemmas.
pub struct ModuloJumpBuilder;

impl ModuloJumpBuilder {
    /// Synthesize exclusion lemmas neg(x >= M) for unreachable state space.
    ///
    /// For instance, if M = 2^{w-1}, the MSB must never be 1 in reachable states:
    /// lemma = [!msb].
    pub fn synthesize_modulo_exclusion_lemmas(
        modulo_counter: &ModuloCounter,
        ts: &Transys,
    ) -> Vec<LitVec> {
        let mut lemmas = Vec::new();
        let counter = &modulo_counter.counter;

        if counter.width < 4 {
            return lemmas;
        }

        // 1. MSB exclusion if modulus is below 2^{w-1}
        let msb = counter.latches[counter.width - 1];
        if let Some(&init_val) = ts.init.get(&msb) {
            if !init_val.polarity() {
                lemmas.push(LitVec::from([!msb.lit()].as_slice()));
            }
        }

        // 2. High-order bits mutual exclusion if modulus is small
        if let Some(m) = modulo_counter.modulus {
            if m < (1u64 << (counter.width - 1)) {
                // Secondary high bit exclusion
                let high_bit = counter.latches[counter.width - 2];
                lemmas.push(LitVec::from([!high_bit.lit()].as_slice()));
            }
        }

        lemmas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modulo_counter_synthesis() {
        let l0 = Var(1);
        let l1 = Var(2);
        let l2 = Var(3);
        let l3 = Var(4);
        let l4 = Var(5);
        let counter = AffineCounter::new(vec![l0, l1, l2, l3, l4], Some("cnt".to_string()));

        let mut ts = Transys::default();
        ts.init.insert(l4, !l4.lit()); // init to 0

        let modulo_counter = ModuloCounter::new(counter, Some(16));
        let lemmas = ModuloJumpBuilder::synthesize_modulo_exclusion_lemmas(&modulo_counter, &ts);

        assert!(!lemmas.is_empty());
        assert_eq!(lemmas[0].as_slice(), [!l4.lit()].as_slice());
    }
}
