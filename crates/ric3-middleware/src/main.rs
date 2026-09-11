//! `ric3-riemann`: CLI Driver for Riemann Accelerated Model Checking.
//!
//! # Specification (docs/STAGE2_RIEMANN.md)

use std::path::PathBuf;
use clap::Parser;
use log::info;
use anyhow::{bail, Result};

use rIC3::config::EngineConfig;
use rIC3::frontend::frontend_from_model;
use rIC3::ic3::IC3Config;
use rIC3::McResult;
use ric3_middleware::RiemannEngine;

#[derive(Parser, Debug)]
#[command(name = "ric3-riemann", about = "rIC3 with Riemann variable-step accelerated transitions")]
struct Cli {
    /// Model file path (.aig, .aag, .btor, .btor2)
    model: PathBuf,

    /// Disable Riemann acceleration (fall back to standard IC3)
    #[arg(long = "no-riemann", default_value_t = false)]
    no_riemann: bool,

    /// Random seed
    #[arg(long, default_value_t = 0)]
    rseed: u64,
}

fn main() -> Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }
    env_logger::init();

    let cli = Cli::parse();
    if !cli.model.exists() {
        bail!("Model file not found: {}", cli.model.display());
    }

    info!("Riemann model checker: analyzing {}", cli.model.display());

    let mut frontend = frontend_from_model(&cli.model)?;
    let (ts, sym) = frontend.ts();

    let mut ic3_cfg = IC3Config::default();
    ic3_cfg.rseed = cli.rseed;
    let cfg = EngineConfig::IC3(ic3_cfg);

    let mut engine = if cli.no_riemann {
        info!("Running with standard single-step IC3 (acceleration disabled).");
        rIC3::create_bl_engine(cfg, ts, sym)
    } else {
        info!("Running with Riemann variable-step accelerated IC3 engine.");
        RiemannEngine::create_accelerated_bl_engine(cfg, ts, sym)
    };

    let res = engine.check();
    engine.statistic();

    match res {
        McResult::UNSAT => println!("UNSAT"),
        McResult::SAT(_) => println!("SAT"),
        McResult::Unknown(_) => println!("UNKNOWN"),
    }

    Ok(())
}
