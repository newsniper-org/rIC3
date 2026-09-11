//! Riemann Subsystem: Non-uniform step size & accelerated transitions.
//!
//! # Specification (docs/STAGE2_RIEMANN.md)
//!
//! Stage 2 innovations:
//! - `counter`: Affine counter/timer sub-circuit identifier.
//! - `accelerator`: Multi-step accelerated transition relation synthesizer.
//! - `cascade`: Cascaded / inter-dependent counter acceleration engine.
//! - `modulo`: Arbitrary modulo counter boundary exclusion synthesizer.
//! - `engine`: IC3 Riemann accelerated frame engine orchestrator.

pub mod counter;
pub mod accelerator;
pub mod cascade;
pub mod modulo;
pub mod engine;

pub use counter::{AffineCounter, CounterDetector};
pub use accelerator::{AcceleratedStep, AcceleratedStepBuilder};
pub use cascade::{CascadeDetector, CascadedCounterPair, CascadedJumpBuilder};
pub use modulo::{ModuloCounter, ModuloDetector, ModuloJumpBuilder};
pub use engine::{RiemannEngine, RiemannLemmaExtractor};
