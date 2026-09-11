//! Middleware driver and runner.
//!
//! # Specification (docs/toolchain-interop-patterns.md §6)
//!
//! Encapsulates front-end parsing, capability validation, and backend invocation
//! without polluting upstream core crates.

use std::path::Path;
use anyhow::{bail, Result};
use log::info;

use rIC3::config::EngineConfig;
use rIC3::frontend::frontend_from_model;
use rIC3::{BlEngine, McResult, create_bl_engine};

use crate::capability::EngineCapability;

/// High-level middleware execution context.
pub struct MiddlewareDriver;

impl MiddlewareDriver {
    /// Inspect capabilities and execute model checking safely.
    pub fn run(model_path: &Path, cfg: EngineConfig) -> Result<McResult> {
        let cap = EngineCapability::query(&cfg);
        info!("Engine {:?} capabilities: supports_proof={}, supports_cex={}",
              cfg, cap.supports_proof, cap.supports_cex);

        let mut frontend = frontend_from_model(&model_path.to_path_buf())?;

        if cap.is_word_level {
            bail!("Word-level engine driver not yet wrapped in middleware");
        }

        let (ts, symbols) = frontend.ts();
        let mut engine: Box<dyn BlEngine> = create_bl_engine(cfg, ts, symbols);

        let res = engine.check();
        engine.statistic();

        Ok(res)
    }
}
