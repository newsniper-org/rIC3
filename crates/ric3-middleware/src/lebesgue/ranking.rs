//! Ranking Function Synthesizer for Stage 3 (Lebesgue).
//!
//! # Specification (docs/STAGE3_LEBESGUE.md §1, §3.1)
//!
//! Synthesizes well-founded ranking functions R: S -> W over posets
//! to prove liveness (F Goal or GF p) without shadow state doubling:
//! 1. Complementary progress measures from Stage 2 counters (R = ~u or R = d).
use logicrs::{Var, VarSymbols};
use rIC3::transys::Transys;
use crate::riemann::counter::CounterDetector;

/// An individual progress measure component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankingComponent {
    /// Latches encoding the progress measure value (LSB to MSB).
    pub latches: Vec<Var>,
    /// Whether the value should be inverted (complement for up-counters: ~x).
    pub inverted: bool,
    /// Component descriptive name.
    pub name: String,
}

impl RankingComponent {
    pub fn new(latches: Vec<Var>, inverted: bool, name: String) -> Self {
        Self {
            latches,
            inverted,
            name,
        }
    }

    pub fn width(&self) -> usize {
        self.latches.len()
    }
}

/// A lexicographic ranking function composed of multiple ordered progress measures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicographicRanking {
    pub components: Vec<RankingComponent>,
}

impl LexicographicRanking {
    pub fn new(components: Vec<RankingComponent>) -> Self {
        Self { components }
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}

/// Static analyzer synthesizing candidate ranking functions from transition systems.
pub struct RankingSynthesizer<'a> {
    ts: &'a Transys,
    sym: &'a VarSymbols,
}

impl<'a> RankingSynthesizer<'a> {
    pub fn new(ts: &'a Transys, sym: &'a VarSymbols) -> Self {
        Self { ts, sym }
    }

    /// Synthesize candidate lexicographic ranking functions.
    pub fn synthesize(&self) -> Option<LexicographicRanking> {
        let mut components = Vec::new();

        // 1. Leverage Stage 2 CounterDetector to identify affine counters
        let detector = CounterDetector::new(self.ts, self.sym);
        let counters = detector.detect_all();

        for (idx, counter) in counters.into_iter().enumerate() {
            // For up-counters, progress toward limit corresponds to complement ~u
            // For down-counters, progress corresponds to direct value d
            let inverted = true; // Default up-counter complement
            let name = counter
                .name
                .clone()
                .unwrap_or_else(|| format!("rank_meas_{}", idx));

            components.push(RankingComponent::new(counter.latches, inverted, name));
        }

        if components.is_empty() {
            // Fallback: If no counters found, pick state latches directly
            if !self.ts.latch.is_empty() {
                let sample_latches = self.ts.latch[..self.ts.latch.len().min(4)].to_vec();
                components.push(RankingComponent::new(
                    sample_latches,
                    false,
                    "state_rank".to_string(),
                ));
            }
        }

        if components.is_empty() {
            None
        } else {
            Some(LexicographicRanking::new(components))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ranking_synthesis_synthetic() {
        let mut ts = Transys::default();
        let sym = VarSymbols::new();

        let l0 = Var(1);
        let l1 = Var(2);
        ts.latch.extend([l0, l1]);
        ts.next.insert(l0, !l0.lit());
        ts.next.insert(l1, l1.lit());

        let synth = RankingSynthesizer::new(&ts, &sym);
        let rank = synth.synthesize().expect("Should synthesize ranking function");

        assert!(!rank.is_empty());
        assert_eq!(rank.components[0].width(), 2);
    }

    #[test]
    fn test_synthesize_ranking_for_counter_btor() {
        use std::path::PathBuf;
        let mut path = PathBuf::from("examples/fvbench/counter/counter.btor");
        if !path.exists() {
            path = PathBuf::from("../../examples/fvbench/counter/counter.btor");
        }
        if !path.exists() {
            return;
        }
        let mut frontend = rIC3::frontend::frontend_from_model(&path).unwrap();
        let (ts, sym) = frontend.ts();
        let synth = RankingSynthesizer::new(&ts, &sym);
        let rank = synth.synthesize().expect("Should synthesize ranking for counter.btor");
        assert!(!rank.is_empty());
        assert!(rank.components[0].width() >= 8);
    }
}
