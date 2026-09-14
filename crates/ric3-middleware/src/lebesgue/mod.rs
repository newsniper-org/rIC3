//! Lebesgue Subsystem: Well-founded ranking function induction for liveness.
//!
//! # Specification (docs/STAGE3_LEBESGUE.md)
//!
//! Stage 3 innovations:
//! - `ranking`: Automatic synthesis of progress measures and lexicographic ranking functions.
//! - `decrease`: Strict lexicographic decrease CNF clause synthesis.
//! - `engine`: Lebesgue RLive engine integration.

pub mod ranking;
pub mod decrease;
pub mod engine;

pub use ranking::{LexicographicRanking, RankingComponent, RankingSynthesizer};
pub use decrease::StrictDecreaseBuilder;
pub use engine::LebesgueRliveEngine;
