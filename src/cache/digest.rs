//! Clause-set AdHash digest calculation for the rIC3 cache layer.
//!
//! # Specification (docs/cache-design.md §2, §3)
//!
//! We compute two distinct cryptographic digests via `portable_algebraic_aotjit`:
//! - `region_key`: AdHash fold of transition relation $T$ and environment constraints.
//! - `whole_digest`: AdHash fold of $T \uplus \text{constraints} \uplus \neg P$.
//!
//! The AdHash fold is an exact multiset homomorphism:
//! `combine_fold(fold(A), fold(B)) = fold(A ⊎ B)`.

use logicrs::LitVec;
use portable_algebraic_aotjit::digest::{
    clause_name_hash, combine_fold, fold_to_digest, ClauseFold, EMPTY_FOLD,
};

use crate::cache::identity::{litvec_to_atom_literals, IdentityMap};
use crate::transys::{Transys, TransysIf};

/// Result of digesting a model's transition relation and query property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDigests {
    /// 32-byte digest of T + constraints (region key for prelude reuse)
    pub region_key: [u8; 32],
    /// 32-byte digest of T + constraints + bad (verdict short-circuit key)
    pub whole_digest: [u8; 32],
    /// The intermediate fold of the prelude (T + constraints)
    pub prelude_fold: ClauseFold,
}

/// Compute the canonical AdHash fold for a single clause given atom identities.
pub fn fold_clause(clause: &[logicrs::Lit], identities: &IdentityMap) -> ClauseFold {
    let lits = litvec_to_atom_literals(clause, identities);
    let hash = clause_name_hash(lits);
    (hash, 1)
}

/// Fold an iterator of clauses into a cumulative ClauseFold.
pub fn fold_clauses<'a, I>(clauses: I, identities: &IdentityMap) -> ClauseFold
where
    I: IntoIterator<Item = &'a LitVec>,
{
    let mut acc = EMPTY_FOLD;
    for c in clauses {
        let single = fold_clause(c.as_slice(), identities);
        acc = combine_fold(acc, single);
    }
    acc
}

/// Compute both `region_key` and `whole_digest` for a `Transys`.
pub fn compute_model_digests(ts: &Transys, identities: &IdentityMap) -> ModelDigests {
    // 1. Fold transition relation clauses T
    let mut prelude_fold = fold_clauses(ts.trans(), identities);

    // 2. Fold environment constraints C
    for c in ts.constraint() {
        // Each constraint is treated as a unit clause
        let single = fold_clause(&[c], identities);
        prelude_fold = combine_fold(prelude_fold, single);
    }

    let region_key = fold_to_digest(prelude_fold);

    // 3. Fold bad state / negated property ¬P
    // In Transys, bad is LitVec where each literal represents a bad state condition
    let mut whole_fold = prelude_fold;
    for &b in &ts.bad {
        let single = fold_clause(&[b], identities);
        whole_fold = combine_fold(whole_fold, single);
    }

    let whole_digest = fold_to_digest(whole_fold);

    ModelDigests {
        region_key,
        whole_digest,
        prelude_fold,
    }
}
