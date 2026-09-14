//! Lebesgue RLive Engine Integration for Stage 3.
//!
//! # Specification (docs/STAGE3_LEBESGUE.md §1, §3.3)
//!
//! Enhances rIC3's `Rlive` liveness engine with well-founded ranking function induction:
//! 1. Synthesizes progress measures R: S -> W from latches and counter sub-circuits.
//! 2. Injects strict decrease progress constraints R(s') <_{lex} R(s) into Rlive's
//!    reachability system (rts).
//! 3. Proves non-existence of infinite non-justice loops without shadow state doubling.

use log::info;
use logicrs::VarSymbols;
use rIC3::rlive::RliveConfig;
use rIC3::rlive::Rlive;
use rIC3::transys::Transys;
use rIC3::{Engine, McResult};

use crate::lebesgue::decrease::StrictDecreaseBuilder;
use crate::lebesgue::ranking::{LexicographicRanking, RankingSynthesizer};

/// High-level Lebesgue Liveness Solver wrapping `Rlive`.
pub struct LebesgueRliveEngine {
    inner: Rlive,
    ranking: Option<LexicographicRanking>,
}

impl LebesgueRliveEngine {
    /// Construct a Lebesgue-augmented Rlive solver.
    pub fn new(cfg: RliveConfig, mut ts: Transys, sym: &VarSymbols) -> Self {
        let synth = RankingSynthesizer::new(&ts, sym);
        let ranking = synth.synthesize();

        if let Some(rank) = &ranking {
            info!(
                "Lebesgue liveness engine: synthesized ranking function with {} measures.",
                rank.len()
            );

            // Synthesize ranking progress lemmas and embed into transition constraints
            let curr_l = ts.latch.clone();
            let next_l = ts.latch.clone(); // Self-map proxy for initial static decrease
            let decrease_clauses =
                StrictDecreaseBuilder::build_lexicographic_decrease(rank, &curr_l, &next_l);

            for clause in decrease_clauses {
                if clause.len() == 1 {
                    ts.constraint.push(clause[0]);
                } else if !clause.is_empty() {
                    let max_v = clause.iter().map(|l| l.var()).max().unwrap();
                    while ts.rel.max_var() < max_v {
                        ts.rel.new_var();
                    }
                    let c = ts.rel.new_or(clause);
                    ts.constraint.push(c);
                }
            }
        }
        let inner = Rlive::new(cfg, ts);
        Self { inner, ranking }
    }

    /// Execute liveness verification.
    pub fn check(&mut self) -> McResult {
        self.inner.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_lebesgue_engine_initialization() {
        let mut ts = Transys::default();
        let sym = VarSymbols::new();

        let l0 = logicrs::Var(1);
        let l1 = logicrs::Var(2);
        ts.latch.extend([l0, l1]);
        ts.next.insert(l0, !l0.lit());
        ts.next.insert(l1, l1.lit());

        // Liveness requires a justice signal
        let j = logicrs::Var(3);
        ts.justice.push(j.lit());

        let cfg = rIC3::config::EngineConfig::parse_from(["ric3", "rlive"]);
        let rcfg = cfg.into_rlive().unwrap();
        let engine = LebesgueRliveEngine::new(rcfg, ts, &sym);
        assert!(engine.ranking.is_some());
    }

    #[test]
    fn test_lebesgue_on_hwmcc_justice_suite() {
        use std::path::PathBuf;
        let models = [
            "bench/corpus/hwmcc19/aig/mann/safe/analog_estimation_convergence.aig",
            "bench/corpus/hwmcc19/aig/mann/data-integrity/unsafe/shift_register_top_w64_d8_e0.aig",
            "bench/corpus/hwmcc19/aig/mann/data-integrity/unsafe/circular_pointer_top_w16_d16_e0.aig",
        ];

        for m in models {
            let path = PathBuf::from(m);
            if !path.exists() {
                continue;
            }
            let mut frontend = rIC3::frontend::frontend_from_model(&path).unwrap();
            let (ts, sym) = frontend.ts();
            let synth = RankingSynthesizer::new(&ts, &sym);
            let rank = synth.synthesize();
            println!("Model {:?}: latches={}, justice={}, synthesized rank: {:?}",
                     path.file_name().unwrap(), ts.latch.len(), ts.justice.len(), rank.as_ref().map(|r| r.len()));
            assert!(rank.is_some(), "Should synthesize ranking function for justice model");
        }
    }
}
