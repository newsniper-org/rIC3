//! Distinct licence types for the rIC3 cache layer.
//!
//! # Specification (AGENTS.md §1.4(b), docs/cache-design.md §2)
//!
//! Two distinct licences, separated in the type system:
//! - `VerdictLicence`: Whole-formula digest matches byte-for-byte. Returns the
//!   stored verdict immediately without running any engine.
//! - `SeedLicence`: Only the `region_key` (T + constraints) matches. Injects
//!   stored invariant clauses as candidate lemmas, then runs IC3 normally.
//!
//! A `region_key` match alone is NOT a verdict licence.

use logicrs::LitVec;
use std::collections::VecDeque;

use crate::McResult;
use crate::tracer::ExtractorIf;

/// Licence permitting instant return of a stored verdict.
///
/// Precondition: The digest of T ⊎ constraints ⊎ ¬P matches byte-for-byte.
#[derive(Debug, Clone, PartialEq)]
pub struct VerdictLicence {
    pub verdict: McResult,
    pub whole_digest: [u8; 32],
}

/// Licence permitting candidate lemma injection into an engine.
///
/// Precondition: The region_key (T ⊎ constraints) matches.
/// The candidate clauses must survive relative induction in IC3 before
/// being retained (soundness per AGENTS.md §1.4(c)).
#[derive(Debug, Clone)]
pub struct SeedLicence {
    pub clauses: Vec<LitVec>,
    pub region_key: [u8; 32],
}

/// Extractor implementation feeding stored seed clauses to an IC3 engine.
pub struct CacheExtractor {
    queue: VecDeque<LitVec>,
}

impl CacheExtractor {
    pub fn new(clauses: Vec<LitVec>) -> Self {
        Self {
            queue: VecDeque::from(clauses),
        }
    }
}

impl ExtractorIf for CacheExtractor {
    fn extract_lemma(&mut self) -> Option<(Option<usize>, LitVec)> {
        // None frame index indicates an inductive invariant clause valid across all frames
        self.queue.pop_front().map(|lemma| (None, lemma))
    }
}
