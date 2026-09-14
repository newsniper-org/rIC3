//! GPU Offload Tier Subsystem for Stage 4 (Massive Clause DBs).
//!
//! # Specification (docs/STAGE4_PARALLELISM.md §1.3)
//!
//! Stage 4 innovations:
//! - `tier`: Contiguous flat clause database and asynchronous evaluation interface.
//! - `backend`: Dynamic multi-backend loader (Vulkan default, CUDA/ROCm/OpenCL optional).

pub mod tier;
pub mod backend;

pub use tier::{
    DEFAULT_GPU_CLAUSE_THRESHOLD, GpuClauseDb, GpuOffloadConfig, GpuOffloadTier,
    HostFallbackGpuTier,
};
pub use backend::{DetectedDrivers, GpuBackendKind, GpuOffloadManager};
