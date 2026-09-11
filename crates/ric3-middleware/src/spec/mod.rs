//! Specification language prototype.
//!
//! # Specification (AGENTS.md Stage 1 Phase D, Stage 2 & 3 Foundations)
//!
//! Includes:
//! - `poly`: Precision polymorphism (symbolic bitwidths with over-approximation).
//! - `rank`: Quantification and lexicographic multi-measure ranking functions.

pub mod poly;
pub mod rank;

pub use poly::{ConcreteSort, ParamWidth, PolyExpr, PolySort};
pub use rank::{BoundedQuantifier, LexicographicRank, QuantifierKind, RankMeasure};
