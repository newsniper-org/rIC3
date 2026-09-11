//! Riemann Subsystem: Non-uniform step size & accelerated transitions.
//!
//! # Specification (docs/STAGE2_RIEMANN.md)
//!
//! Stage 2 innovations:
//! - `counter`: Affine counter/timer sub-circuit identifier.
//! - `accelerator`: Multi-step accelerated transition relation synthesizer.
//! - `engine`: IC3 Riemann accelerated frame engine orchestrator.

pub mod counter;
pub mod accelerator;
pub mod engine;

pub use counter::{AffineCounter, CounterDetector};
pub use accelerator::{AcceleratedStep, AcceleratedStepBuilder};
pub use engine::{RiemannEngine, RiemannLemmaExtractor};
