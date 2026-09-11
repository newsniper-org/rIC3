//! Content-Derived Lemma Exchange and Survival Rate Instrumentation.
//!
//! # Specification (AGENTS.md Stage 1 Phase D, docs/stage1-architecture-review.md §5.1)
//!
//! Replaces fragile positional `LitVec` IPC sharing with content-derived atom identities:
//! - Literals are represented as `(atom_name: String, polarity: bool)`.
//! - Translates across engines with distinct variable allocations.
//! - Tracks candidate lemma survival rates during relative induction.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use logicrs::{LitVec, Var};

/// A literal identified by its content-derived canonical atom name and polarity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContentLit {
    pub atom_name: String,
    pub polarity: bool,
}

/// A lemma represented canonically via content-derived literals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentLemma {
    pub frame: Option<usize>,
    pub lits: Vec<ContentLit>,
}

/// Thread-safe tracker measuring lemma survival rate across relative induction.
#[derive(Debug, Default)]
pub struct SurvivalMeter {
    injected: AtomicUsize,
    survived: AtomicUsize,
    dropped: AtomicUsize,
}

impl SurvivalMeter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn record_injected(&self, count: usize) {
        self.injected.fetch_add(count, Ordering::Relaxed);
    }

    pub fn record_survived(&self, count: usize) {
        self.survived.fetch_add(count, Ordering::Relaxed);
    }

    pub fn record_dropped(&self, count: usize) {
        self.dropped.fetch_add(count, Ordering::Relaxed);
    }

    pub fn injected_count(&self) -> usize {
        self.injected.load(Ordering::Relaxed)
    }

    pub fn survived_count(&self) -> usize {
        self.survived.load(Ordering::Relaxed)
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Calculate survival rate as a percentage [0.0, 100.0].
    pub fn survival_rate(&self) -> f64 {
        let total = self.survived_count() + self.dropped_count();
        if total == 0 {
            0.0
        } else {
            (self.survived_count() as f64 / total as f64) * 100.0
        }
    }
}

/// Bridge converting between engine-local `LitVec` and canonical `ContentLemma`.
pub struct ContentLemmaBridge {
    var_to_name: HashMap<Var, String>,
    name_to_var: HashMap<String, Var>,
}

impl ContentLemmaBridge {
    pub fn new(var_to_name: HashMap<Var, String>) -> Self {
        let mut name_to_var = HashMap::new();
        for (&v, name) in &var_to_name {
            name_to_var.insert(name.clone(), v);
        }
        Self {
            var_to_name,
            name_to_var,
        }
    }

    /// Export engine-local `LitVec` to canonical `ContentLemma`.
    pub fn export_lemma(&self, frame: Option<usize>, lemma: &LitVec) -> ContentLemma {
        let mut lits = Vec::with_capacity(lemma.len());
        for &lit in lemma.iter() {
            let atom_name = self
                .var_to_name
                .get(&lit.var())
                .cloned()
                .unwrap_or_else(|| format!("anon:{}", lit.var().0));
            lits.push(ContentLit {
                atom_name,
                polarity: lit.polarity(),
            });
        }
        ContentLemma { frame, lits }
    }

    /// Import canonical `ContentLemma` into engine-local `LitVec`.
    /// Returns `None` if any atom cannot be mapped to a local variable.
    pub fn import_lemma(&self, content_lemma: &ContentLemma) -> Option<LitVec> {
        let mut lits = Vec::with_capacity(content_lemma.lits.len());
        for clit in &content_lemma.lits {
            if let Some(&var) = self.name_to_var.get(&clit.atom_name) {
                lits.push(if clit.polarity { var.lit() } else { !var.lit() });
            } else {
                return None;
            }
        }
        Some(LitVec::from(lits.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_survival_meter() {
        let meter = SurvivalMeter::new();
        meter.record_injected(10);
        meter.record_survived(7);
        meter.record_dropped(3);

        assert_eq!(meter.injected_count(), 10);
        assert_eq!(meter.survived_count(), 7);
        assert_eq!(meter.dropped_count(), 3);
        assert!((meter.survival_rate() - 70.0).abs() < 1e-6);
    }

    #[test]
    fn test_content_lemma_roundtrip() {
        let mut var_map = HashMap::new();
        let v1 = Var(1);
        let v2 = Var(2);
        var_map.insert(v1, "lat:cnt:0".to_string());
        var_map.insert(v2, "lat:cnt:1".to_string());

        let bridge = ContentLemmaBridge::new(var_map);
        let original_litvec = LitVec::from([v1.lit(), !v2.lit()].as_slice());

        let exported = bridge.export_lemma(Some(2), &original_litvec);
        assert_eq!(exported.frame, Some(2));
        assert_eq!(exported.lits.len(), 2);

        let imported = bridge.import_lemma(&exported).unwrap();
        assert_eq!(imported.as_slice(), original_litvec.as_slice());
    }
}
