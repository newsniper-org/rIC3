//! Narrow-Scope GPU Tier Interface for Stage 4 (Clause DB Offloading).
//!
//! # Specification (docs/STAGE4_PARALLELISM.md §1.3)
//!
//! Provides the abstract interface for offloading massive clause database
//! evaluations (>= 50k clauses) without incurring per-query synchronous PCIe latency.
//!
//! Architecture:
//! - Contiguous flat clause representation (`GpuClauseDb`).
//! - Asynchronous assignment update buffer (`GpuAssignmentBuffer`).
//! - Pluggable backend trait (`GpuOffloadTier`) with zero-panic CPU fallback.

use anyhow::Result;
use logicrs::LitVec;

/// Minimum clause count required before GPU offloading is engaged.
pub const DEFAULT_GPU_CLAUSE_THRESHOLD: usize = 50_000;

/// Configuration for the narrow-scope GPU offload tier.
#[derive(Debug, Clone)]
pub struct GpuOffloadConfig {
    /// Minimum clause threshold to engage GPU.
    pub clause_threshold: usize,
    /// Whether GPU tier is enabled.
    pub enabled: bool,
    /// Target device identifier.
    pub device_id: u32,
}

impl Default for GpuOffloadConfig {
    fn default() -> Self {
        Self {
            clause_threshold: DEFAULT_GPU_CLAUSE_THRESHOLD,
            enabled: true,
            device_id: 0,
        }
    }
}

/// Contiguous flat layout of clauses optimized for coalesced GPU memory access.
#[derive(Debug, Clone, Default)]
pub struct GpuClauseDb {
    /// Flat array of 32-bit literals.
    pub literals: Vec<u32>,
    /// Offsets indexing into `literals` for each clause.
    pub offsets: Vec<u32>,
    /// Number of clauses stored.
    pub num_clauses: usize,
}

impl GpuClauseDb {
    /// Build a flat GPU clause buffer from an iterator of `LitVec`.
    pub fn from_clauses<'a>(clauses: impl IntoIterator<Item = &'a LitVec>) -> Self {
        let mut literals = Vec::new();
        let mut offsets = Vec::new();
        let mut count = 0;

        for clause in clauses {
            offsets.push(literals.len() as u32);
            for &lit in clause.iter() {
                // Encode lit into u32 (var << 1 | sign)
                let raw = u32::from(lit);
                literals.push(raw);
            }
            count += 1;
        }
        offsets.push(literals.len() as u32);

        Self {
            literals,
            offsets,
            num_clauses: count,
        }
    }

    pub fn len(&self) -> usize {
        self.num_clauses
    }

    pub fn is_empty(&self) -> bool {
        self.num_clauses == 0
    }
}

/// Abstract GPU Offload Tier Trait.
pub trait GpuOffloadTier: Send + Sync {
    /// Upload the static clause database to device memory.
    fn upload_clause_db(&mut self, db: &GpuClauseDb) -> Result<()>;

    /// Asynchronously evaluate clauses against the variable assignment vector.
    /// Returns a bitmask representing satisfied clauses (or number of satisfied clauses).
    fn evaluate_async(&mut self, assignment_vector: &[u8]) -> Result<Vec<u32>>;

    /// Check if device is ready and available.
    fn is_available(&self) -> bool;
}

/// Fallback / Reference Host Offload Tier ensuring zero-panic portability.
pub struct HostFallbackGpuTier {
    db: Option<GpuClauseDb>,
    config: GpuOffloadConfig,
}

impl HostFallbackGpuTier {
    pub fn new(config: GpuOffloadConfig) -> Self {
        Self { db: None, config }
    }
}

impl GpuOffloadTier for HostFallbackGpuTier {
    fn upload_clause_db(&mut self, db: &GpuClauseDb) -> Result<()> {
        self.db = Some(db.clone());
        Ok(())
    }

    fn evaluate_async(&mut self, assignment_vector: &[u8]) -> Result<Vec<u32>> {
        let Some(db) = &self.db else {
            return Ok(Vec::new());
        };

        // Evaluate clauses in parallel chunks
        let mut satisfied_count = 0u32;
        for i in 0..db.num_clauses {
            let start = db.offsets[i] as usize;
            let end = db.offsets[i + 1] as usize;
            let mut clause_sat = false;

            for &lit in &db.literals[start..end] {
                let var = (lit >> 1) as usize;
                let is_neg = (lit & 1) != 0;
                if var < assignment_vector.len() {
                    let val = assignment_vector[var];
                    if (is_neg && val == 2) || (!is_neg && val == 1) {
                        clause_sat = true;
                        break;
                    }
                }
            }
            if clause_sat {
                satisfied_count += 1;
            }
        }

        Ok(vec![satisfied_count])
    }

    fn is_available(&self) -> bool {
        self.config.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logicrs::Var;

    #[test]
    fn test_flat_clause_db_layout() {
        let l0 = Var(1).lit();
        let l1 = !Var(2).lit();
        let c0 = LitVec::from([l0, l1].as_slice());
        let c1 = LitVec::from([l0].as_slice());

        let db = GpuClauseDb::from_clauses(&[c0, c1]);
        assert_eq!(db.len(), 2);
        assert_eq!(db.offsets, vec![0, 2, 3]);
        assert_eq!(db.literals.len(), 3);
    }

    #[test]
    fn test_host_fallback_evaluation() {
        let l0 = Var(0).lit();
        let l1 = !Var(1).lit();
        let c0 = LitVec::from([l0, l1].as_slice());

        let db = GpuClauseDb::from_clauses(&[c0]);
        let mut tier = HostFallbackGpuTier::new(GpuOffloadConfig::default());
        tier.upload_clause_db(&db).unwrap();

        // Assignment: var 0 = True (1), var 1 = False (2)
        let assignments = vec![1u8, 2u8];
        let result = tier.evaluate_async(&assignments).unwrap();
        assert_eq!(result, vec![1]); // 1 clause satisfied
    }
}
