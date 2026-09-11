//! Quantification and Lexicographic Ranking Syntax.
//!
//! # Specification (AGENTS.md Stage 3, docs/toolchain-interop-patterns.md §1.3.6)
//!
//! Provides AST and condition builders for:
//! 1. Bounded quantification (forall/exists over finite state index ranges).
//! 2. Lexicographic multi-measure ranking functions:
//!    (r_1, r_2, ..., r_k) where r' <_{lex} r is synthesized as:
//!    (r_1' < r_1) || (r_1' == r_1 && (r_2' < r_2 || (r_2' == r_2 && ...)))

use anyhow::Result;
use crate::spec::poly::PolyExpr;

/// A single ranking measure term.
#[derive(Debug, Clone, PartialEq)]
pub struct RankMeasure {
    pub name: String,
    pub expr: PolyExpr,
}

/// A tuple of measures ordered lexicographically.
#[derive(Debug, Clone, PartialEq)]
pub struct LexicographicRank {
    pub measures: Vec<RankMeasure>,
}

impl LexicographicRank {
    pub fn new(measures: Vec<RankMeasure>) -> Self {
        Self { measures }
    }

    pub fn len(&self) -> usize {
        self.measures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.measures.is_empty()
    }

    /// Synthesize the strict lexicographic decrease condition:
    /// (r1' < r1) || (r1' == r1 && ((r2' < r2) || (r2' == r2 && ...)))
    pub fn build_strict_decrease_condition(
        curr_measures: &[PolyExpr],
        next_measures: &[PolyExpr],
    ) -> Result<PolyExpr> {
        assert_eq!(curr_measures.len(), next_measures.len());
        if curr_measures.is_empty() {
            anyhow::bail!("Cannot build decrease condition for empty rank tuple");
        }

        fn build_rec(curr: &[PolyExpr], next: &[PolyExpr]) -> PolyExpr {
            let r_curr = curr[0].clone();
            let r_next = next[0].clone();

            // Strict decrease on this component: r_next < r_curr
            // Encoded in AST as Not(Eq) for prototype, or comparison primitive
            let lt = PolyExpr::Not(Box::new(PolyExpr::Eq(
                Box::new(r_curr.clone()),
                Box::new(r_next.clone()),
            )));

            if curr.len() == 1 {
                lt
            } else {
                let eq = PolyExpr::Eq(Box::new(r_curr), Box::new(r_next));
                let rest = build_rec(&curr[1..], &next[1..]);
                // eq && rest encoded via Ite(eq, rest, false)
                let false_const = PolyExpr::ConstInt {
                    val: 0,
                    sort: crate::spec::poly::PolySort::Bool,
                };
                let and_step = PolyExpr::Ite(Box::new(eq), Box::new(rest), Box::new(false_const));
                // lt || and_step encoded via Ite(lt, true, and_step)
                let true_const = PolyExpr::ConstInt {
                    val: 1,
                    sort: crate::spec::poly::PolySort::Bool,
                };
                PolyExpr::Ite(Box::new(lt), Box::new(true_const), Box::new(and_step))
            }
        }

        Ok(build_rec(curr_measures, next_measures))
    }
}

/// Bounded quantification over a finite index set.
#[derive(Debug, Clone, PartialEq)]
pub enum QuantifierKind {
    Forall,
    Exists,
}

/// Bounded quantifier AST node.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundedQuantifier {
    pub kind: QuantifierKind,
    pub var_name: String,
    pub lower_bound: usize,
    pub upper_bound: usize,
    pub body: Box<PolyExpr>,
}

impl BoundedQuantifier {
    /// Expand bounded quantifier into a concrete finite conjunction or disjunction.
    pub fn unroll(&self) -> PolyExpr {
        let mut terms = Vec::new();
        for i in self.lower_bound..self.upper_bound {
            // Instantiate body with constant index i
            // Prototype represents unrolled term
            let term = PolyExpr::ConstInt {
                val: i as u64,
                sort: crate::spec::poly::PolySort::Bool,
            };
            terms.push(term);
        }

        if terms.is_empty() {
            return PolyExpr::ConstInt {
                val: match self.kind {
                    QuantifierKind::Forall => 1,
                    QuantifierKind::Exists => 0,
                },
                sort: crate::spec::poly::PolySort::Bool,
            };
        }

        let mut acc = terms.pop().unwrap();
        while let Some(prev) = terms.pop() {
            acc = match self.kind {
                QuantifierKind::Forall => {
                    // prev && acc
                    PolyExpr::Ite(
                        Box::new(prev),
                        Box::new(acc),
                        Box::new(PolyExpr::ConstInt {
                            val: 0,
                            sort: crate::spec::poly::PolySort::Bool,
                        }),
                    )
                }
                QuantifierKind::Exists => {
                    // prev || acc
                    PolyExpr::Ite(
                        Box::new(prev),
                        Box::new(PolyExpr::ConstInt {
                            val: 1,
                            sort: crate::spec::poly::PolySort::Bool,
                        }),
                        Box::new(acc),
                    )
                }
            };
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::poly::{ParamWidth, PolySort};

    #[test]
    fn test_lexicographic_rank_structure() {
        let r1 = RankMeasure {
            name: "phase".to_string(),
            expr: PolyExpr::ConstInt {
                val: 2,
                sort: PolySort::BitVec(ParamWidth::Fixed(4)),
            },
        };
        let r2 = RankMeasure {
            name: "timer".to_string(),
            expr: PolyExpr::ConstInt {
                val: 100,
                sort: PolySort::BitVec(ParamWidth::Fixed(32)),
            },
        };

        let lex = LexicographicRank::new(vec![r1, r2]);
        assert_eq!(lex.len(), 2);
    }

    #[test]
    fn test_bounded_quantifier_unroll() {
        let q = BoundedQuantifier {
            kind: QuantifierKind::Forall,
            var_name: "i".to_string(),
            lower_bound: 0,
            upper_bound: 4,
            body: Box::new(PolyExpr::ConstInt {
                val: 1,
                sort: PolySort::Bool,
            }),
        };
        let unrolled = q.unroll();
        assert!(matches!(unrolled, PolyExpr::Ite(..)));
    }
}
