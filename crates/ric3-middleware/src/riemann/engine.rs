//! IC3 Riemann Accelerated Frame Engine Integration.
//!
//! # Specification (docs/STAGE2_RIEMANN.md §1, §3.3)
//!
//! Integrates accelerated step transitions with IC3 via candidate lemma injection:
//! 1. Analyzes transition system for affine counters using `CounterDetector`.
//! 2. Synthesizes power-of-two accelerated lemmas constraining upper counter bits.
//! 3. Injects synthesized lemmas into IC3 via the `lemma-inject` / `ExtractorIf` interface.
//! 4. IC3's relative induction filters and embeds valid accelerated lemmas into higher frames.
use logicrs::{LitVec, VarSymbols};
use rIC3::config::EngineConfig;
use rIC3::tracer::ExtractorIf;
use rIC3::transys::Transys;
use rIC3::{BlEngine, create_bl_engine};

use crate::riemann::cascade::{CascadeDetector, CascadedJumpBuilder};
use crate::riemann::counter::CounterDetector;
use crate::riemann::modulo::{ModuloDetector, ModuloJumpBuilder};
/// An extractor that yields Riemann accelerated candidate lemmas to IC3.
pub struct RiemannLemmaExtractor {
    lemmas: Vec<LitVec>,
    cursor: usize,
}

impl RiemannLemmaExtractor {
    pub fn new(lemmas: Vec<LitVec>) -> Self {
        Self { lemmas, cursor: 0 }
    }
}

impl ExtractorIf for RiemannLemmaExtractor {
    fn extract_lemma(&mut self) -> Option<(Option<usize>, LitVec)> {
        if self.cursor < self.lemmas.len() {
            let lemma = self.lemmas[self.cursor].clone();
            self.cursor += 1;
            // Inductive invariants valid across all frames
            Some((None, lemma))
        } else {
            None
        }
    }
}

/// Riemann Accelerated Verification Orchestrator.
pub struct RiemannEngine;

impl RiemannEngine {
    /// Detect counter circuits and synthesize accelerated candidate lemmas for IC3.
    pub fn synthesize_accelerated_lemmas(
        ts: &Transys,
        sym: &VarSymbols,
    ) -> Vec<LitVec> {
        let detector = CounterDetector::new(ts, sym);
        let counters = detector.detect_all();

        let mut accelerated_lemmas = Vec::new();

        for counter in &counters {
            // For multi-bit counters, synthesize jump bounds on the MSB / overflow bit
            // if initial state is zero or known.
            if counter.width >= 4 {
                let msb = counter.latches[counter.width - 1];
                if let Some(&init_val) = ts.init.get(&msb) {
                    // If MSB is initialized to 0, constrain initial safety frame
                    if !init_val.polarity() {
                        accelerated_lemmas.push(LitVec::from([!msb.lit()].as_slice()));
                    }
                }
            }
        }

        // Synthesize macro-step jump lemmas for cascaded counter architectures
        let cascade_detector = CascadeDetector::new(ts);
        let cascades = cascade_detector.detect_cascades(&counters);
        for pair in &cascades {
            let macro_lemmas = CascadedJumpBuilder::synthesize_cascaded_lemmas(pair, ts);
            accelerated_lemmas.extend(macro_lemmas);
        }

        // Synthesize modulo boundary exclusion lemmas
        let modulo_detector = ModuloDetector::new(ts);
        let modulo_counters = modulo_detector.detect_modulo_counters(&counters);
        for mc in &modulo_counters {
            let mod_lemmas = ModuloJumpBuilder::synthesize_modulo_exclusion_lemmas(mc, ts);
            accelerated_lemmas.extend(mod_lemmas);
        }

        // Deduplicate identical candidate lemmas
        accelerated_lemmas.sort();
        accelerated_lemmas.dedup();

        accelerated_lemmas
    }

    /// Construct an accelerated IC3 engine instance with synthesized Riemann lemmas.
    pub fn create_accelerated_bl_engine(
        cfg: EngineConfig,
        ts: Transys,
        sym: VarSymbols,
    ) -> Box<dyn BlEngine> {
        let lemmas = Self::synthesize_accelerated_lemmas(&ts, &sym);
        let num_lemmas = lemmas.len();

        let mut engine = create_bl_engine(cfg, ts, sym);

        if num_lemmas > 0 {
            log::info!(
                "Riemann accelerated engine: injecting {} counter acceleration lemmas into IC3.",
                num_lemmas
            );
            engine.set_extractor(Box::new(RiemannLemmaExtractor::new(lemmas)));
        }

        engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_riemann_lemma_synthesis() {
        let mut ts = Transys::default();
        let sym = VarSymbols::new();

        // 4-bit counter latches 1..4
        let l1 = logicrs::Var(1);
        let l2 = logicrs::Var(2);
        let l3 = logicrs::Var(3);
        let l4 = logicrs::Var(4);

        ts.latch.extend([l1, l2, l3, l4]);
        ts.next.insert(l1, !l1.lit());
        ts.next.insert(l2, l2.lit());
        ts.next.insert(l3, l3.lit());
        ts.next.insert(l4, l4.lit());
        ts.init.insert(l4, !l4.lit()); // MSB init to 0

        let lemmas = RiemannEngine::synthesize_accelerated_lemmas(&ts, &sym);
        assert_eq!(lemmas.len(), 1);
        assert_eq!(lemmas[0].as_slice(), [!l4.lit()].as_slice());
    }
}
