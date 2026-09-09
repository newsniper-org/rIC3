//! Content-derived atom identity for the rIC3 cache layer.
//!
//! # Specification (docs/cache-design.md §5)
//!
//! Atom identity must be content-derived, never positional (AGENTS.md §1.4(a)).
//! Priority order:
//! 1. Declared symbol name from front-end (`VarSymbols`), e.g. `in:clk:0`.
//! 2. Structural hash of the defining latch signature (init + next).
//! 3. Canonical index for inputs where no symbol exists.

use giputils::hash::GHashMap;
use logicrs::{Lit, Var, VarSymbols};
use sha2::{Digest, Sha256};
use std::fmt::Write;

use crate::transys::Transys;

/// Map from local variable to content-derived string identity.
pub type IdentityMap = GHashMap<Var, String>;

/// Resolve content-derived identities for all latches and inputs in `ts`.
pub fn resolve_identities(ts: &Transys, sym: &VarSymbols) -> IdentityMap {
    let mut map = GHashMap::default();

    // 1. Inputs
    for &inp in &ts.input {
        let symbols = sym.get(inp);
        if let Some((name, idx)) = symbols.first() {
            map.insert(inp, format!("in:{}:{}", name, idx));
        } else {
            map.insert(inp, format!("in:idx_{}", inp.0));
        }
    }

    // 2. Latches
    for &lat in &ts.latch {
        let symbols = sym.get(lat);
        if let Some((name, idx)) = symbols.first() {
            map.insert(lat, format!("lat:{}:{}", name, idx));
        } else {
            let next_lit = ts.next.get(&lat).copied().unwrap_or_else(|| lat.lit());
            let cone_hash = compute_latch_sig(ts, lat, next_lit);
            map.insert(lat, format!("lat:sig_{}", cone_hash));
        }
    }

    map
}

/// Compute canonical structural signature for a latch (lat, next, init).
fn compute_latch_sig(ts: &Transys, lat: Var, next_lit: Lit) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"latch_sig_v1:");
    hasher.update(lat.0.to_le_bytes());
    hasher.update(next_lit.var().0.to_le_bytes());
    hasher.update([if next_lit.polarity() { 1u8 } else { 0u8 }]);

    if let Some(init) = ts.init.get(&lat) {
        hasher.update(b"init:");
        hasher.update(init.var().0.to_le_bytes());
        hasher.update([if init.polarity() { 1u8 } else { 0u8 }]);
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(32);
    for b in &digest[..16] {
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

/// Convert a LitVec slice into (atom_name, polarity) pairs suitable for clause_name_hash.
pub fn litvec_to_atom_literals<'a>(
    clause: &'a [Lit],
    identities: &'a IdentityMap,
) -> Vec<(&'a str, bool)> {
    clause
        .iter()
        .map(|l| {
            let name = identities
                .get(&l.var())
                .map(|s| s.as_str())
                .unwrap_or("unknown_atom");
            (name, l.polarity())
        })
        .collect()
}
