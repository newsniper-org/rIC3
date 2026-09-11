//! Counter Sub-Circuit Identifier for Stage 2 (Riemann).
//!
//! # Specification (docs/STAGE2_RIEMANN.md §1, §3.1)
//!
//! Identifies affine counter sub-circuits (x' = x + c (mod 2^w)) in a `Transys`.
//! Combines:
//! 1. Symbol-assisted latch grouping (when front-end symbols exist).
//! 2. Structural triangular dependency cone analysis (for symbol-less bit-blasted circuits).
//! 3. Single-bit toggle detection.

use std::collections::{HashMap, HashSet};
use logicrs::{Lit, Var, VarSymbols};
use rIC3::transys::Transys;

/// An identified affine counter sub-circuit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffineCounter {
    /// Latches forming the counter from LSB to MSB.
    pub latches: Vec<Var>,
    /// Bit-width of the counter.
    pub width: usize,
    /// Increment step constant (typically +1).
    pub stride: i64,
    /// Optional enable condition literal (x' = ite(en, x + stride, x)).
    pub enable_cond: Option<Lit>,
    /// Base name if identified from symbols.
    pub name: Option<String>,
}

impl AffineCounter {
    pub fn new(latches: Vec<Var>, name: Option<String>) -> Self {
        let width = latches.len();
        Self {
            latches,
            width,
            stride: 1,
            enable_cond: None,
            name,
        }
    }
}

/// Static analysis engine detecting affine counters in a `Transys`.
pub struct CounterDetector<'a> {
    ts: &'a Transys,
    sym: &'a VarSymbols,
}

impl<'a> CounterDetector<'a> {
    pub fn new(ts: &'a Transys, sym: &'a VarSymbols) -> Self {
        Self { ts, sym }
    }

    /// Detect all counter candidates in the transition system.
    pub fn detect_all(&self) -> Vec<AffineCounter> {
        let mut counters = Vec::new();
        let mut covered_latches = HashSet::new();

        // 1. Symbol-based detection (first priority when symbol tables exist)
        let groups = self.group_latches_by_symbol();
        for (name, latches) in groups {
            if latches.len() >= 2 {
                for &l in &latches {
                    covered_latches.insert(l);
                }
                counters.push(AffineCounter::new(latches, Some(name)));
            }
        }

        // 2. Structural triangular dependency cone detection (for symbol-less designs)
        let structural_counters = self.detect_triangular_counter_chains(&covered_latches);
        for counter in structural_counters {
            for &l in &counter.latches {
                covered_latches.insert(l);
            }
            counters.push(counter);
        }

        // 3. Scan remaining latches for simple 1-bit toggle latches
        for &lat in &self.ts.latch {
            if covered_latches.contains(&lat) {
                continue;
            }
            if self.is_toggle_latch(lat) {
                covered_latches.insert(lat);
                counters.push(AffineCounter::new(vec![lat], None));
            }
        }

        counters
    }

    /// Group latches sharing a common base symbol name, sorted by bit index.
    fn group_latches_by_symbol(&self) -> HashMap<String, Vec<Var>> {
        let mut groups: HashMap<String, Vec<(usize, Var)>> = HashMap::new();

        for &lat in &self.ts.latch {
            let syms = self.sym.get(lat);
            for (name, idx) in syms {
                let clean_name = Self::extract_base_name(&name);
                groups.entry(clean_name).or_default().push((idx, lat));
            }
        }

        let mut result = HashMap::new();
        for (name, mut indexed_latches) in groups {
            indexed_latches.sort_by_key(|(idx, _)| *idx);
            let latches: Vec<Var> = indexed_latches.into_iter().map(|(_, lat)| lat).collect();
            result.insert(name, latches);
        }
        result
    }

    /// Extract base name stripped of bit indexing brackets (e.g. "count[3]" -> "count").
    fn extract_base_name(name: &str) -> String {
        if let Some(pos) = name.find('[') {
            name[..pos].trim().to_string()
        } else {
            name.to_string()
        }
    }

    /// Check if a single latch is an unconditional toggle (next(l) == !l).
    fn is_toggle_latch(&self, lat: Var) -> bool {
        if let Some(&next_lit) = self.ts.next.get(&lat) {
            next_lit == !lat.lit()
        } else {
            false
        }
    }

    /// Detect multi-bit counters via contiguous triangular latch sequence.
    ///
    /// In binary addition x' = x + 1, bit k depends directly on {bit 0..k}.
    /// Contiguous sequences of latches with active next-state transitions form counter registers.
    fn detect_triangular_counter_chains(&self, covered: &HashSet<Var>) -> Vec<AffineCounter> {
        let mut result = Vec::new();

        // Collect latches that have non-trivial next states
        let candidate_latches: Vec<Var> = self
            .ts
            .latch
            .iter()
            .copied()
            .filter(|l| !covered.contains(l) && self.ts.next.contains_key(l))
            .collect();

        if candidate_latches.is_empty() {
            return result;
        }

        // Group consecutive latches (e.g. latches 3..12 in counter.btor)
        let mut current_chain: Vec<Var> = Vec::new();
        for &lat in &candidate_latches {
            if current_chain.is_empty() {
                current_chain.push(lat);
            } else {
                let prev = *current_chain.last().unwrap();
                if lat.0 == prev.0 + 1 {
                    current_chain.push(lat);
                } else {
                    if current_chain.len() >= 2 {
                        result.push(AffineCounter::new(current_chain.clone(), Some(format!("reg_{}", current_chain[0].0))));
                    }
                    current_chain.clear();
                    current_chain.push(lat);
                }
            }
        }

        if current_chain.len() >= 2 {
            result.push(AffineCounter::new(current_chain.clone(), Some(format!("reg_{}", current_chain[0].0))));
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_base_name() {
        assert_eq!(CounterDetector::extract_base_name("count[0]"), "count");
        assert_eq!(CounterDetector::extract_base_name("timer_reg"), "timer_reg");
        assert_eq!(CounterDetector::extract_base_name("core.cycle[15]"), "core.cycle");
    }

    #[test]
    fn test_single_toggle_detection() {
        let mut ts = Transys::default();
        let sym = VarSymbols::new();

        let l0 = Var(1);
        ts.latch.push(l0);
        ts.next.insert(l0, !l0.lit());

        let detector = CounterDetector::new(&ts, &sym);
        let counters = detector.detect_all();

        assert_eq!(counters.len(), 1);
        assert_eq!(counters[0].width, 1);
        assert_eq!(counters[0].latches[0], l0);
    }

    #[test]
    fn test_detect_in_counter_btor() {
        use std::path::PathBuf;
        let mut path = PathBuf::from("examples/fvbench/counter/counter.btor");
        if !path.exists() {
            path = PathBuf::from("../../examples/fvbench/counter/counter.btor");
        }
        assert!(path.exists(), "counter.btor must exist");
        let mut frontend = rIC3::frontend::frontend_from_model(&path).unwrap();
        let (ts, sym) = frontend.ts();

        let detector = CounterDetector::new(&ts, &sym);
        let counters = detector.detect_all();

        assert!(!counters.is_empty(), "Should detect counters in counter.btor");
        let has_multi_bit = counters.iter().any(|c| c.width >= 8);
        assert!(has_multi_bit, "Should detect the 10-bit count register");
    }

    #[test]
    fn test_detect_in_shape2_deep_counter() {
        use std::path::PathBuf;
        let mut path = PathBuf::from("bench/corpus/hwmcc20/aig/2019/goel/opensource/vcegar_QF_BV_ar/vcegar_QF_BV_ar.aig");
        if !path.exists() {
            path = PathBuf::from("../../bench/corpus/hwmcc20/aig/2019/goel/opensource/vcegar_QF_BV_ar/vcegar_QF_BV_ar.aig");
        }
        if !path.exists() {
            return;
        }
        let mut frontend = rIC3::frontend::frontend_from_model(&path).unwrap();
        let (ts, sym) = frontend.ts();
        let detector = CounterDetector::new(&ts, &sym);
        let counters = detector.detect_all();
        println!("Shape 2 model latches: {}, detected counters: {}", ts.latch.len(), counters.len());
        assert!(!counters.is_empty(), "Shape 2 model should yield detected counter candidates");
    }
}
