//! Engine Capability Query Interface.
//!
//! # Specification (docs/toolchain-interop-patterns.md §6.4)
//!
//! Because default trait methods in rIC3 panic with `unsupport proof` or
//! `unsupport counterexample` and rIC3 compiles under `panic = "abort"`,
//! capabilities cannot be probed speculatively at runtime.
//!
//! This middleware capability interface provides a safe, non-panicking query
//! over engine configurations before invoking methods.

use rIC3::config::EngineConfig;

/// Capabilities supported by a model checking engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineCapability {
    /// Whether the engine can generate an inductive proof/certificate on UNSAT.
    pub supports_proof: bool,
    /// Whether the engine can generate a counterexample trace on SAT.
    pub supports_cex: bool,
    /// Whether the engine operates on word-level (BTOR2/SMT) or bit-level (AIGER).
    pub is_word_level: bool,
}

impl EngineCapability {
    /// Query the capabilities of an engine from its configuration.
    pub fn query(cfg: &EngineConfig) -> Self {
        match cfg {
            EngineConfig::IC3(_) => Self {
                supports_proof: true,
                supports_cex: true,
                is_word_level: false,
            },
            EngineConfig::BMC(_) => Self {
                supports_proof: false,
                supports_cex: true,
                is_word_level: false,
            },
            EngineConfig::Kind(_) => Self {
                supports_proof: true,
                supports_cex: true,
                is_word_level: false,
            },
            EngineConfig::Cegar(_) => Self {
                supports_proof: true,
                supports_cex: true,
                is_word_level: true,
            },
            EngineConfig::Rlive(_) => Self {
                supports_proof: true,
                supports_cex: true,
                is_word_level: false,
            },
            EngineConfig::WlBMC(_) => Self {
                supports_proof: false,
                supports_cex: true,
                is_word_level: true,
            },
            EngineConfig::WlKind(_) => Self {
                supports_proof: true,
                supports_cex: true,
                is_word_level: true,
            },
            EngineConfig::MultiProp(_) => Self {
                supports_proof: true,
                supports_cex: true,
                is_word_level: false,
            },
            EngineConfig::Portfolio(_) => Self {
                supports_proof: true,
                supports_cex: true,
                is_word_level: false,
            },
            EngineConfig::Polynexus(_) => Self {
                supports_proof: true,
                supports_cex: true,
                is_word_level: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rIC3::ic3::IC3Config;

    #[test]
    fn test_ic3_capabilities() {
        let cfg = EngineConfig::IC3(IC3Config::default());
        let cap = EngineCapability::query(&cfg);
        assert!(cap.supports_proof);
        assert!(cap.supports_cex);
        assert!(!cap.is_word_level);
    }
}
