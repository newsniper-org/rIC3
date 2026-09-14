//! `ric3-middleware`: Middleware crate separating front-end parsing,
//! capability checking, and engine driving from the core solver.
//!
//! # Specification (AGENTS.md Stage 1 Phase D, docs/toolchain-interop-patterns.md §6)
//!
//! Features:
//! - `capability`: Engine capability whitelist query (safe probing without aborting).
//! - `driver`: Model verification driver wrapping front-end and back-end.

pub mod capability;
pub mod spec;
pub mod lemma;
pub mod riemann;
pub mod lebesgue;
pub mod simd;
pub mod gpu;
pub mod driver;

pub use capability::EngineCapability;
pub use riemann::{AcceleratedStep, AcceleratedStepBuilder, AffineCounter, CounterDetector, RiemannEngine};
pub use driver::MiddlewareDriver;
pub use lemma::{AdaptiveLemmaRouter, ContentLemma, ContentLit, ContentLemmaBridge, SurvivalMeter};
pub use lebesgue::{LebesgueRliveEngine, LexicographicRanking, RankingComponent, RankingSynthesizer, StrictDecreaseBuilder};
pub use simd::{SimdWatcherScanner, WatcherItem};
pub use gpu::{
    DEFAULT_GPU_CLAUSE_THRESHOLD, DetectedDrivers, GpuBackendKind, GpuClauseDb,
    GpuOffloadConfig, GpuOffloadManager, GpuOffloadTier, HostFallbackGpuTier,
};
