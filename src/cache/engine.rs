//! Short-circuit engine that returns a cached verdict immediately.
//!
//! # Specification (AGENTS.md §3.3(b), docs/cache-design.md §3)
//!
//! When a `VerdictLicence` is issued, no solver or search engine is constructed.
//! This dummy engine immediately returns the stored verdict on `check()`,
//! satisfying the ≥ 100× speedup requirement.

use giputils::TerminateCtrl;
use std::sync::Arc;

use crate::cache::licence::VerdictLicence;
use crate::transys::{Transys, certify::{BlCex, BlProof}};
use crate::{BlEngine, Engine, McResult};

/// An engine that immediately returns a stored verdict from a `VerdictLicence`.
pub struct CachedVerdictEngine {
    licence: VerdictLicence,
}

impl CachedVerdictEngine {
    pub fn new(licence: VerdictLicence) -> Self {
        Self { licence }
    }

    pub fn whole_digest(&self) -> [u8; 32] {
        self.licence.whole_digest
    }
}

struct NoopCtrl;
impl TerminateCtrl for NoopCtrl {
    fn is_terminated(&self) -> bool {
        false
    }
    fn terminate(&self) {}
}

impl Engine for CachedVerdictEngine {
    fn check(&mut self) -> McResult {
        self.licence.verdict
    }

    fn add_tracer(&mut self, _tracer: Box<dyn crate::tracer::TracerIf>) {}

    fn set_extractor(&mut self, _extractor: Box<dyn crate::tracer::ExtractorIf>) {}

    fn set_ui(&mut self, _renderer: crate::ui::UiRenderer) {}

    fn statistic(&mut self) {
        log::info!("Verdict returned instantly from method/digest cache (verdict licence hit).");
    }

    fn get_ctrl(&self) -> Arc<dyn TerminateCtrl> {
        Arc::new(NoopCtrl)
    }
}

impl BlEngine for CachedVerdictEngine {
    fn proof(&mut self) -> BlProof {
        BlProof {
            proof: Transys::default(),
        }
    }

    fn cex(&mut self) -> BlCex {
        BlCex::new()
    }

    fn invariant(&mut self) -> Vec<logicrs::LitVec> {
        Vec::new()
    }
}
