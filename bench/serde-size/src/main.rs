//! Measure RON versus binary serialisation for the shapes a cache entry holds.
//!
//! # Why
//!
//! `docs/cache-design.md` §11.2 leaves the `preproc` storage threshold open,
//! and the deciding comparison is deserialisation time against preprocessing
//! time. Before that can be settled, the encoding has to be chosen — and
//! upstream's existing `ric3proj` cache uses `ron::to_string`
//! (`src/cli/rproj.rs:74`), i.e. a *text* format, for `ts.ron`, `wts.ron` and
//! friends. Text encodings of large clause databases are the obvious way to
//! blow a 1 % cold-path budget, so the ratio needs a number rather than an
//! assumption.
//!
//! # What is measured, and what is approximated
//!
//! This does **not** depend on rIC3: doing so would build
//! cadical/kissat/bitwuzla, and the question is about the encoding, not about
//! model checking. Clause lists are structurally `Vec<Vec<u32>>` (a `LitVec` is
//! a vector of `u32`-backed `Lit`), so the size and time ratios carry over.
//! `LitVec` could define a custom Serde impl that differs in constant factors;
//! the ratio between encodings would not change.
//!
//! Scales are taken from the baseline run:
//!
//! - invariant clause list, fifo scale: 5607 clauses x 12 literals
//! - preprocessed transition relation: `a07-p14.aig` reported 2,502,409 clauses
//!   at 1,867,680 vars; AIG clauses are short, so 3 literals each
//! - side table: `ILA_Ridecore_*` reported 23,592 latches
//!
//! The largest case is measured at a reduced size and extrapolated, because
//! materialising a multi-hundred-megabyte RON string next to a live measurement
//! is exactly the kind of memory pressure `docs/BASELINE.md` §4.0 warns about.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Clause list, structurally equivalent to `Vec<LitVec>`.
#[derive(Serialize, Deserialize)]
struct Clauses {
    clauses: Vec<Vec<u32>>,
}

/// Variable-number to atom-identity side table (§6.1 of the cache design).
#[derive(Serialize, Deserialize)]
struct SideTable {
    map: Vec<(u32, [u8; 32])>,
}

/// Deterministic synthetic data; content does not affect encoding ratios, but
/// keeping it deterministic makes repeated runs comparable.
fn synth_clauses(n: usize, len: usize) -> Clauses {
    let mut clauses = Vec::with_capacity(n);
    let mut x: u32 = 12345;
    for _ in 0..n {
        let mut c = Vec::with_capacity(len);
        for _ in 0..len {
            // xorshift, so literals look like real variable numbers rather than
            // a run of small integers that a text format would compress well.
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            c.push(x % 4_000_000);
        }
        clauses.push(c);
    }
    Clauses { clauses }
}

fn synth_side_table(n: usize) -> SideTable {
    let mut map = Vec::with_capacity(n);
    for i in 0..n {
        let mut id = [0u8; 32];
        for (j, b) in id.iter_mut().enumerate() {
            *b = ((i * 31 + j * 7) % 251) as u8;
        }
        map.push((i as u32, id));
    }
    SideTable { map }
}

struct Result {
    label: String,
    ron_bytes: usize,
    ron_ser_ms: f64,
    ron_de_ms: f64,
    bin_bytes: usize,
    bin_ser_ms: f64,
    bin_de_ms: f64,
}

impl Result {
    fn report(&self) {
        let size_ratio = self.ron_bytes as f64 / self.bin_bytes as f64;
        let de_ratio = if self.bin_de_ms > 0.0 {
            self.ron_de_ms / self.bin_de_ms
        } else {
            f64::NAN
        };
        println!("=== {} ===", self.label);
        println!(
            "  RON     {:>10.2} MB   ser {:>8.1} ms   de {:>8.1} ms",
            self.ron_bytes as f64 / 1_048_576.0,
            self.ron_ser_ms,
            self.ron_de_ms
        );
        println!(
            "  bincode {:>10.2} MB   ser {:>8.1} ms   de {:>8.1} ms",
            self.bin_bytes as f64 / 1_048_576.0,
            self.bin_ser_ms,
            self.bin_de_ms
        );
        println!(
            "  ratio   {:>10.1}x size          {:>8.1}x deserialise",
            size_ratio, de_ratio
        );
        println!();
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn measure_clauses(label: &str, n: usize, len: usize) -> Result {
    let data = synth_clauses(n, len);
    let cfg = bincode::config::standard();

    let t = Instant::now();
    let ron_s = ron::to_string(&data).expect("ron serialise");
    let ron_ser_ms = ms(t);
    let ron_bytes = ron_s.len();

    let t = Instant::now();
    let _: Clauses = ron::from_str(&ron_s).expect("ron deserialise");
    let ron_de_ms = ms(t);
    drop(ron_s);

    let t = Instant::now();
    let bin = bincode::serde::encode_to_vec(&data, cfg).expect("bincode serialise");
    let bin_ser_ms = ms(t);
    let bin_bytes = bin.len();

    let t = Instant::now();
    let _: (Clauses, usize) =
        bincode::serde::decode_from_slice(&bin, cfg).expect("bincode deserialise");
    let bin_de_ms = ms(t);

    Result {
        label: label.to_string(),
        ron_bytes,
        ron_ser_ms,
        ron_de_ms,
        bin_bytes,
        bin_ser_ms,
        bin_de_ms,
    }
}

fn measure_side_table(label: &str, n: usize) -> Result {
    let data = synth_side_table(n);
    let cfg = bincode::config::standard();

    let t = Instant::now();
    let ron_s = ron::to_string(&data).expect("ron serialise");
    let ron_ser_ms = ms(t);
    let ron_bytes = ron_s.len();

    let t = Instant::now();
    let _: SideTable = ron::from_str(&ron_s).expect("ron deserialise");
    let ron_de_ms = ms(t);
    drop(ron_s);

    let t = Instant::now();
    let bin = bincode::serde::encode_to_vec(&data, cfg).expect("bincode serialise");
    let bin_ser_ms = ms(t);
    let bin_bytes = bin.len();

    let t = Instant::now();
    let _: (SideTable, usize) =
        bincode::serde::decode_from_slice(&bin, cfg).expect("bincode deserialise");
    let bin_de_ms = ms(t);

    Result {
        label: label.to_string(),
        ron_bytes,
        ron_ser_ms,
        ron_de_ms,
        bin_bytes,
        bin_ser_ms,
        bin_de_ms,
    }
}

fn main() {
    println!("Serialisation cost: RON (upstream ric3proj uses it) vs bincode");
    println!("Synthetic data structurally equal to Vec<LitVec> / side table.");
    println!();

    // What the cache stores on a seed hit: the invariant clause list.
    let inv = measure_clauses("invariant clauses, fifo scale (5607 x 12)", 5607, 12);
    inv.report();

    // Preprocessed transition relation, measured at 1/10 scale and extrapolated
    // to avoid a multi-hundred-MB RON string beside a live measurement.
    let pre = measure_clauses(
        "preproc clauses, 1/10 of a07-p14 scale (250241 x 3)",
        250_241,
        3,
    );
    pre.report();

    // Side table at the largest observed latch count.
    let st = measure_side_table("side table, 23592 latches", 23_592);
    st.report();

    println!("=== extrapolation to full a07-p14 (2502409 clauses x 3) ===");
    println!(
        "  RON     {:>10.2} MB   de {:>8.1} ms",
        pre.ron_bytes as f64 * 10.0 / 1_048_576.0,
        pre.ron_de_ms * 10.0
    );
    println!(
        "  bincode {:>10.2} MB   de {:>8.1} ms",
        pre.bin_bytes as f64 * 10.0 / 1_048_576.0,
        pre.bin_de_ms * 10.0
    );
    println!("  (linear in clause count; verify before quoting)");
    println!();

    println!("=== what this decides ===");
    println!("  Threshold for storing `preproc` is worth it only when load time");
    println!("  is decisively below preprocessing time. Observed preprocessing on");
    println!("  the largest instances: 1003 s (frts-tl default is 1000 s).");
    println!("  Compare the bincode deserialise figure above against that.");
}
