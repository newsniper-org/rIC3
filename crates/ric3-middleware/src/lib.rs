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
pub mod driver;

pub use capability::EngineCapability;
pub use driver::MiddlewareDriver;
pub use lemma::{ContentLemma, ContentLit, ContentLemmaBridge, SurvivalMeter};
