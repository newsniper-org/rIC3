//! Method and digest cache layer for rIC3.
//!
//! # Specification (AGENTS.md §1.4, docs/cache-design.md)
//!
//! Implements a content-addressed cache over transition systems and properties:
//! - Content-addressed by `region_key` = digest(T ⊎ constraints).
//! - Short-circuit on `whole_digest` = digest(T ⊎ constraints ⊎ ¬P).
//! - Two licences: `VerdictLicence` (instant return) and `SeedLicence` (candidate injection).
//! - Total deserialisation discipline (no panics on corrupt cache).

pub mod digest;
pub mod engine;
pub mod identity;
pub mod licence;
pub mod storage;

use std::path::PathBuf;
use std::sync::Arc;

use giputils::TerminateCtrl;
use logicrs::{LitVec, VarSymbols};

use crate::cache::digest::compute_model_digests;
use crate::cache::engine::CachedVerdictEngine;
use crate::cache::identity::resolve_identities;
use crate::cache::licence::{CacheExtractor, SeedLicence, VerdictLicence};
use crate::cache::storage::{default_cache_dir, load_entry, store_entry, CacheEntry, FORMAT_VERSION, PINNED_UPSTREAM_COMMIT};
use crate::config::EngineConfig;
use crate::transys::certify::{BlCex, BlProof};
use crate::transys::Transys;
use crate::{BlEngine, Engine, McResult};

/// Attempt to intercept engine creation with a cached verdict or seed candidate lemmas.
pub fn try_cached_bl_engine(
    cfg: &EngineConfig,
    ts: &Transys,
    sym: &VarSymbols,
) -> Option<Box<dyn BlEngine>> {
    // Check if caching is enabled (enabled by default when feature `cache` is compiled,
    // can be disabled via RIC3_CACHE_DISABLE=1).
    if std::env::var("RIC3_CACHE_DISABLE").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false) {
        return None;
    }

    let cache_dir = default_cache_dir();
    let identities = resolve_identities(ts, sym);
    let digests = compute_model_digests(ts, &identities);

    // Consult the on-disk cache by region_key
    if let Some(entry) = load_entry(&cache_dir, &digests.region_key) {
        // 1. Verdict Licence Path: Whole-formula digest matches byte-for-byte
        if entry.whole_digest == digests.whole_digest {
            log::info!(
                "Method/digest cache HIT: whole digest matched. Returning verdict {:?} immediately.",
                entry.verdict
            );
            let licence = VerdictLicence {
                verdict: entry.verdict,
                whole_digest: entry.whole_digest,
            };
            return Some(Box::new(CachedVerdictEngine::new(licence)));
        }

        // 2. Seed Licence Path: Only region_key matches (T + constraints)
        log::info!(
            "Method/digest cache SEED HIT: region_key matched. Injecting candidate lemmas."
        );
        let candidate_clauses = reconstruct_clauses(&entry, ts, &identities);
        let _seed = SeedLicence {
            clauses: candidate_clauses.clone(),
            region_key: entry.region_key,
        };

        // Construct underlying engine and inject candidate lemmas via set_extractor
        let mut inner = create_uncached_bl_engine(cfg.clone(), ts.clone(), sym.clone());
        inner.set_extractor(Box::new(CacheExtractor::new(candidate_clauses)));

        let side_table = build_side_table(ts, &identities);
        return Some(Box::new(CachingBlEngine {
            inner,
            cache_dir,
            region_key: digests.region_key,
            whole_digest: digests.whole_digest,
            side_table,
            already_stored: false,
        }));
    }

    // 3. Cache Miss: Construct engine and wrap to record results on completion
    let inner = create_uncached_bl_engine(cfg.clone(), ts.clone(), sym.clone());
    let side_table = build_side_table(ts, &identities);

    Some(Box::new(CachingBlEngine {
        inner,
        cache_dir,
        region_key: digests.region_key,
        whole_digest: digests.whole_digest,
        side_table,
        already_stored: false,
    }))
}

/// Fallback factory creating the raw un-cached engine.
fn create_uncached_bl_engine(
    cfg: EngineConfig,
    ts: Transys,
    sym: VarSymbols,
) -> Box<dyn BlEngine> {
    match cfg {
        EngineConfig::IC3(cfg) => Box::new(crate::ic3::IC3::new(cfg, ts, sym)),
        EngineConfig::Kind(cfg) => Box::new(crate::kind::Kind::new(cfg, ts)),
        EngineConfig::BMC(cfg) => Box::new(crate::bmc::BMC::new(cfg, ts)),
        EngineConfig::MultiProp(cfg) => Box::new(crate::mp::MultiProp::new(cfg, ts)),
        EngineConfig::Rlive(cfg) => Box::new(crate::rlive::Rlive::new(cfg, ts)),
        _ => unreachable!(),
    }
}

/// Build side table mapping (atom identity string -> local variable ID).
fn build_side_table(ts: &Transys, identities: &crate::cache::identity::IdentityMap) -> Vec<(String, u32)> {
    let mut table = Vec::new();
    for &lat in &ts.latch {
        if let Some(id) = identities.get(&lat) {
            table.push((id.clone(), lat.0));
        }
    }
    table
}

/// Reconstruct local LitVec clauses from a stored CacheEntry.
fn reconstruct_clauses(
    entry: &CacheEntry,
    ts: &Transys,
    identities: &crate::cache::identity::IdentityMap,
) -> Vec<LitVec> {
    // Map from atom identity string to current model's local variable
    let mut name_to_var = std::collections::HashMap::new();
    for &lat in &ts.latch {
        if let Some(id) = identities.get(&lat) {
            name_to_var.insert(id.clone(), lat);
        }
    }

    // Map stored variable indices to current model's local variable
    let mut stored_idx_to_curr_var = std::collections::HashMap::new();
    for (name, stored_var) in &entry.side_table {
        if let Some(&curr_var) = name_to_var.get(name) {
            stored_idx_to_curr_var.insert(*stored_var, curr_var);
        }
    }

    let mut result = Vec::new();
    for clause in &entry.clauses {
        let mut lits = Vec::new();
        let mut valid = true;
        for &raw in clause {
            let stored_var_id = raw.unsigned_abs();
            let is_pos = raw > 0;
            if let Some(&var) = stored_idx_to_curr_var.get(&stored_var_id) {
                lits.push(if is_pos { var.lit() } else { !var.lit() });
            } else {
                valid = false;
                break;
            }
        }
        if valid && !lits.is_empty() {
            result.push(LitVec::from(lits.as_slice()));
        }
    }
    result
}

/// Wrapper engine that records verdicts and learned invariant clauses to disk.
pub struct CachingBlEngine {
    inner: Box<dyn BlEngine>,
    cache_dir: PathBuf,
    region_key: [u8; 32],
    whole_digest: [u8; 32],
    side_table: Vec<(String, u32)>,
    already_stored: bool,
}

impl Engine for CachingBlEngine {
    fn check(&mut self) -> McResult {
        let res = self.inner.check();

        if !self.already_stored && (res.is_unsat() || res.is_sat()) {
            self.save_result(res);
            self.already_stored = true;
        }

        res
    }

    fn add_tracer(&mut self, tracer: Box<dyn crate::tracer::TracerIf>) {
        self.inner.add_tracer(tracer);
    }

    fn set_extractor(&mut self, extractor: Box<dyn crate::tracer::ExtractorIf>) {
        self.inner.set_extractor(extractor);
    }

    fn set_ui(&mut self, renderer: crate::ui::UiRenderer) {
        self.inner.set_ui(renderer);
    }

    fn statistic(&mut self) {
        self.inner.statistic();
    }

    fn get_ctrl(&self) -> Arc<dyn TerminateCtrl> {
        self.inner.get_ctrl()
    }
}

impl BlEngine for CachingBlEngine {
    fn proof(&mut self) -> BlProof {
        self.inner.proof()
    }

    fn cex(&mut self) -> BlCex {
        self.inner.cex()
    }

    fn invariant(&mut self) -> Vec<logicrs::LitVec> {
        self.inner.invariant()
    }
}

impl CachingBlEngine {
    fn save_result(&mut self, verdict: McResult) {
        let mut stored_clauses = Vec::new();
        if verdict.is_unsat() {
            let invs = self.inner.invariant();
            for clause in invs {
                let mut c = Vec::with_capacity(clause.len());
                for l in clause.iter() {
                    let var_id = l.var().0 as i32;
                    c.push(if l.polarity() { var_id } else { -var_id });
                }
                if !c.is_empty() {
                    stored_clauses.push(c);
                }
            }
            log::info!("Captured {} inductive invariant clauses for reuse/seeding.", stored_clauses.len());
        }

        let entry = CacheEntry {
            format_version: FORMAT_VERSION,
            upstream_commit: PINNED_UPSTREAM_COMMIT.to_string(),
            identity_scheme: "symbol_or_cone_v1".to_string(),
            region_key: self.region_key,
            whole_digest: self.whole_digest,
            verdict,
            side_table: self.side_table.clone(),
            clauses: stored_clauses,
            collision: false,
        };

        if let Err(e) = store_entry(&self.cache_dir, &entry) {
            log::warn!("Failed to store cache entry: {e}");
        } else {
            log::info!("Cached verification result for region.");
        }
    }
}
