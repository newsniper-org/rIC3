//! SIMD Vectorized Acceleration Subsystem for Stage 4.
//!
//! # Specification (docs/STAGE4_PARALLELISM.md §1.1)
//!
//! Stage 4 innovations:
//! - `watcher`: SIMD batch scanner for GipSAT BCP watcher lists.

pub mod watcher;

pub use watcher::{SimdWatcherScanner, WatcherItem};
