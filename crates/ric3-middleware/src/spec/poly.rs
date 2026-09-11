//! Precision Polymorphism Prototype for Hardware Specifications.
//!
//! # Specification (docs/toolchain-interop-patterns.md §1.3.6, §7)
//!
//! Replaces fixed `Sort::Bv(n)` with parameterized widths `ParamWidth`.
//! Allows formal properties and transition relations to be specified across
//! arbitrary or scalable bit-widths, compiling down to concrete BTOR2 bitvectors
//! via monomorphization under the mandatory over-approximation discipline.

use std::collections::HashMap;
use anyhow::{bail, Result};

/// Width of a bitvector, either concrete or symbolically parameterized.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParamWidth {
    /// Fixed concrete width (e.g. 8, 32, 64 bits).
    Fixed(usize),
    /// Symbolic width parameter (e.g. "DATA_WIDTH", "ADDR_WIDTH").
    Param(String),
    /// Linear expression over a parameter (e.g. "N + 1", "2 * N").
    Add(Box<ParamWidth>, usize),
}

impl ParamWidth {
    /// Resolve to a concrete usize given an environment mapping parameter names.
    pub fn resolve(&self, env: &HashMap<String, usize>) -> Result<usize> {
        match self {
            ParamWidth::Fixed(w) => Ok(*w),
            ParamWidth::Param(name) => {
                if let Some(&w) = env.get(name) {
                    if w == 0 {
                        bail!("Bitvector width cannot be zero (parameter {})", name);
                    }
                    Ok(w)
                } else {
                    bail!("Unbound width parameter: {}", name);
                }
            }
            ParamWidth::Add(inner, offset) => {
                let base = inner.resolve(env)?;
                Ok(base + offset)
            }
        }
    }
}

/// Sort with polymorphic precision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolySort {
    Bool,
    BitVec(ParamWidth),
}

impl PolySort {
    pub fn resolve(&self, env: &HashMap<String, usize>) -> Result<ConcreteSort> {
        match self {
            PolySort::Bool => Ok(ConcreteSort::Bool),
            PolySort::BitVec(pw) => Ok(ConcreteSort::BitVec(pw.resolve(env)?)),
        }
    }
}

/// Concrete sort after monomorphization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcreteSort {
    Bool,
    BitVec(usize),
}

/// Polymorphic specification expression.
#[derive(Debug, Clone, PartialEq)]
pub enum PolyExpr {
    Var { name: String, sort: PolySort },
    ConstInt { val: u64, sort: PolySort },
    Not(Box<PolyExpr>),
    Add(Box<PolyExpr>, Box<PolyExpr>),
    Eq(Box<PolyExpr>, Box<PolyExpr>),
    Ite(Box<PolyExpr>, Box<PolyExpr>, Box<PolyExpr>),
}

impl PolyExpr {
    /// Check that expression sorts are consistent when instantiated.
    pub fn type_check(&self, env: &HashMap<String, usize>) -> Result<ConcreteSort> {
        match self {
            PolyExpr::Var { sort, .. } => sort.resolve(env),
            PolyExpr::ConstInt { sort, .. } => sort.resolve(env),
            PolyExpr::Not(e) => {
                let s = e.type_check(env)?;
                Ok(s)
            }
            PolyExpr::Add(a, b) => {
                let sa = a.type_check(env)?;
                let sb = b.type_check(env)?;
                if sa != sb {
                    bail!("Sort mismatch in Add: {:?} vs {:?}", sa, sb);
                }
                Ok(sa)
            }
            PolyExpr::Eq(a, b) => {
                let sa = a.type_check(env)?;
                let sb = b.type_check(env)?;
                if sa != sb {
                    bail!("Sort mismatch in Eq: {:?} vs {:?}", sa, sb);
                }
                Ok(ConcreteSort::Bool)
            }
            PolyExpr::Ite(c, t, e) => {
                let sc = c.type_check(env)?;
                if sc != ConcreteSort::Bool {
                    bail!("Condition in Ite must be Bool, got {:?}", sc);
                }
                let st = t.type_check(env)?;
                let se = e.type_check(env)?;
                if st != se {
                    bail!("Branch sort mismatch in Ite: {:?} vs {:?}", st, se);
                }
                Ok(st)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_polymorphic_width_resolution() {
        let mut env = HashMap::new();
        env.insert("W".to_string(), 32);

        let p1 = ParamWidth::Param("W".to_string());
        assert_eq!(p1.resolve(&env).unwrap(), 32);

        let p2 = ParamWidth::Add(Box::new(p1), 1);
        assert_eq!(p2.resolve(&env).unwrap(), 33);
    }

    #[test]
    fn test_polymorphic_type_check() {
        let mut env = HashMap::new();
        env.insert("DATA_WIDTH".to_string(), 16);

        let sort = PolySort::BitVec(ParamWidth::Param("DATA_WIDTH".to_string()));
        let x = PolyExpr::Var {
            name: "x".to_string(),
            sort: sort.clone(),
        };
        let c = PolyExpr::ConstInt {
            val: 10,
            sort: sort.clone(),
        };
        let add = PolyExpr::Add(Box::new(x), Box::new(c));

        assert_eq!(add.type_check(&env).unwrap(), ConcreteSort::BitVec(16));
    }
}
