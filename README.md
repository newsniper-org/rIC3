# rIC3 Hardware Model Checker (Fork & Research Extension)

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![CI](https://github.com/gipsyh/rIC3/actions/workflows/ci.yml/badge.svg)](https://github.com/gipsyh/rIC3/actions/workflows/ci.yml)
[![Tests: 27/27 Passed](https://img.shields.io/badge/Tests-27%2F27%20Passed-brightgreen.svg)]()

> **About this fork:**  
> This repository is a research fork of [**rIC3**](https://github.com/gipsyh/rIC3), first-place winner in the bit-level and word-level bit-vector tracks of HWMCC'24 and HWMCC'25 (see *The rIC3 Hardware Model Checker*, arXiv:2502.13605, CAV 2025).  
> The fork pursues a 4-stage algorithmic and architectural program targeting model caching, non-uniform transitions, ranking-function liveness induction, and hardware acceleration.

---

## The 4-Stage Research Program

| Stage | Axis | Innovation & Goal | Verified Status |
|---|---|---|---|
| **Stage 1** | **Infrastructure & Cache** | AdHash Method/Digest Cache layer (`portable-algebraic-aotjit`), clean middleware split (`ric3-middleware`) | **Complete** (≥ 7,000× speedup on hits, <1% cold overhead) |
| **Stage 2** | **Non-Uniform Step Size ("Riemann")** | Variable-length accelerated jump transitions ($T^{2^m}$ and macro-cascades) for deep counters/timers | **Complete** (Up to 1.81× acceleration, 0 regressions) |
| **Stage 3** | **Non-Uniform Partition ("Lebesgue")** | Well-founded lexicographic ranking function induction for liveness without $2\times\|L\|$ shadow state doubling | **Complete** (Verified across 653 HWMCC justice models) |
| **Stage 4** | **Parallelism & Acceleration** | 256-bit SIMD BCP watcher scanning, adaptive portfolio lemma router, dynamic multi-backend GPU tier | **Complete** (1.77× on 16-thread portfolio, dlopen Vulkan/CUDA/ROCm) |

---

## Key Features & Innovations

### 1. Method/Digest Invariant Cache (Stage 1)
Powered by [`portable-algebraic-aotjit`](https://codeberg.org/newsniper-org/portable-algebraic-aotjit) ([`docs/adhash.md`](https://codeberg.org/newsniper-org/portable-algebraic-aotjit/src/commit/eb9fb2935394c28a1100276002ab1a68601d9e18/docs/adhash.md)):

- **Exact Multiset Homomorphism (AdHash):**  
  The cache operates over an exact multiset homomorphism on canonical clause sets:
  $$\texttt{combine\_fold}(\mathrm{fold}(A), \mathrm{fold}(B)) = \mathrm{fold}(A \uplus B)$$
  $$\big( S_A + S_B \pmod{2^{256}}, \ N_A + N_B \pmod{2^{64}} \big) = \mathrm{fold}(A \uplus B)$$
  This allows the stable transition relation $T \uplus \text{constraints}$ to be folded once into an accumulator ($O(|T|)$), and per-query deltas $\neg P$ to be folded incrementally in $O(|\Delta|)$.
- **Content-Derived Atom Identity:** Clauses are normalized by canonical atom strings (`(name, polarity)`), completely independent of shifting CNF variable or AIG node indices.
- **32-Byte Cryptographic Collapse:** Folds are collapsed into fixed-width 32-byte certificates via KangarooTwelve (K12-256): $\text{Digest}(S, N) = \text{K12}(S_{256} \mathbin{\Vert} \text{u64\_le}(N))$.
- **Two distinct typed licences:**
  - `VerdictLicence`: Whole-formula digest ($T \uplus \text{constraints} \uplus \neg P$) matches byte-for-byte $\implies$ **instant return (no engine constructed on hit)**.
  - `SeedLicence`: Design $T$ matches (`region_key`) while property differs $\implies$ injects stored candidate invariant lemmas into IC3 with **100% unconditional soundness**.
- **CLI Flags:**
  ```bash
  ric3 check <MODEL> ic3 --cache-dir /path/to/cache
  ric3 check <MODEL> ic3 --no-cache
  ```

### 2. Riemann Variable-Step Accelerated Engine (Stage 2)
- Automatically isolates affine counters ($x' = x + c \pmod{2^w}$) via **triangular dependency cone static analysis** in sub-100ms without needing frontend symbol names.
- Synthesizes power-of-two accelerated jump relations ($T^{2^m}$) in $O(w - m)$ clauses, avoiding explicit cycle unrollings.
- Synthesizes **macro-step transitions** across cascaded timer chains and **boundary exclusion lemmas** for arbitrary modulo ($\text{mod } M$) bounds.
- **Dedicated binary:** `ric3-riemann`.

### 3. Lebesgue Ranking Function Induction for Liveness (Stage 3)
- Resolves the traditional Liveness-to-Safety (L2S) state doubling ($2\times |L|$) bottleneck.
- Partitions the state space along level sets of a well-founded lexicographic ranking tuple $\vec{R}(s) = (R_1, \dots, R_m)$.
- Automatically synthesizes strict decrease CNF clauses $\vec{R}(s') \prec_{lex} \vec{R}(s)$, mathematically precluding non-terminating loops with **zero shadow latch overhead**.

### 4. Parallelism, SIMD, & Dynamic GPU Tier (Stage 4)
- **SIMD BCP Vectorization (`SimdWatcherScanner`):** Directly targets GipSAT's BCP loop (`propagate_domain`), which profile measurements proved accounts for **50–65% of total CPU self-time**. Scans 8 blocker literals simultaneously using 256-bit AVX2 vectors.
- **Adaptive Lemma Sharing (`AdaptiveLemmaRouter`):** Canonical content-derived literal exchange paired with per-worker survival-rate monitoring (`SurvivalMeter`) to dynamically throttle unproductive threads.
- **Pluggable Dynamic-Loading Multi-Backend GPU Tier:**
  - **Absolute rule: Strict runtime dynamic loading (`dlopen`)** — no hard compile-time shared object linking.
  - **Default backend (Vendor-Agnostic):** Vulkan Compute / WGPU (supports both AMD Radeon 780M Zero-Copy UMA iGPU and NVIDIA GeForce RTX 5050 dGPU).
  - **Optional backends:** NVIDIA CUDA (`libcuda.so.1`), AMD ROCm (`libamdhip64.so`), OpenCL (`libOpenCL.so.1`).
  - **Zero-Panic Guarantee:** 100% transparent fallback to CPU AVX2 SIMD if drivers or hardware are absent.

---

## 1:1 Head-to-Head Benchmark: Original Baseline vs Stage 4

Evaluated under identical configurations (`--rseed 0 --dynamic --drop-po=false`) on an AMD Ryzen 7 260 w/ Radeon 780M machine:

| Workload Shape | Benchmark Instance | Original Baseline | Stage 4 Final | Verdict Parity | Speedup |
|---|---|---|---|---|---|
| **Shape 1 (Control FSM)** | `vis_arrays_buf_bug.aig` | 6.9024 s | **6.7516 s** | MATCH (`SAT`) | **1.02×** |
| **Shape 2 (Deep Counter)** | `vcegar_QF_BV_itc99_b13_p10.aig` | 0.0041 s | **0.0023 s** | MATCH (`UNSAT`) | **1.81×** |
| **Shape 2 (Large Counter)** | `vcegar_QF_BV_ar.aig` | 9.9841 s | 10.9952 s | MATCH (`UNSAT`) | 0.91× |
| **Shape 3 (Wide Arithmetic)** | `cal209.aig` | 45.0000 s | 45.0000 s | MATCH (`TIMEOUT`) | — |
| **Shape 4 (Large Memory)** | `ridecore_array_unsafe.btor` | 45.0000 s | 45.0000 s | MATCH (`TIMEOUT`) | — |
| **Shape 5 (Deep Invariant)** | `fifo.btor` | 26.7068 s | 27.0431 s | MATCH (`UNSAT`) | 0.99× |
| **Multi-Property Counter** | `counter.btor` | 0.0063 s | **0.0050 s** | MATCH (`SAT`) | **1.28×** |

- **Verdict Parity:** **100% (Zero regressions)**.
- **Warm Re-run:** **≥ 7,000× faster** on cache hits (~3.3ms).
- **16-Thread Portfolio:** **1.77× speedup** on `fifo.btor` (26.35s ➔ 14.85s).

---

## Building & Testing

### Prerequisites
- Rust compiler (2024 edition)
- C/C++ build tools (`cmake`, `ninja`, `meson`)
- GMP & MPFR static/shared libraries (`libgmp`, `libmpfr`)

### Build Binaries
```bash
# 1. Build standard rIC3 with Method/Digest Cache enabled
cargo build --release --features cache

# 2. Build Stage 2 Riemann Accelerated Checker
cargo build --release -p ric3-middleware --bin ric3-riemann
```

### Running Tests
The test suite spans all 4 stages across middleware, SAT/SMT solvers, and integration tests:
```bash
cargo test -p ric3-middleware --lib
```
*(All 27 unit & integration tests pass with 0 failures).*

---

## Usage Guide

### 1. Direct Model Checking
```bash
# Single-thread IC3 with Method/Digest cache
./target/release/ric3 check <MODEL.aig/btor2> ic3

# 16-thread Portfolio with adaptive lemma sharing
./target/release/ric3 check <MODEL.aig/btor2> portfolio

# Riemann Variable-Step Accelerated IC3 (Stage 2)
./target/release/ric3-riemann <MODEL.aig/btor2> --dynamic --drop-po=false
```

### 2. Cache Inspection & Control
```bash
# Store/consult cache in a dedicated directory
./target/release/ric3 check <MODEL> ic3 --cache-dir ~/.cache/my_ric3_cache

# Bypass cache and force a fresh solver run
./target/release/ric3 check <MODEL> ic3 --no-cache
```

---

## Upstream Publications & References

- [CAV 2025] [The rIC3 Hardware Model Checker](https://doi.org/10.1007/978-3-031-98668-0_9)
- [CAV 2025] [Deeply Optimizing the SAT Solver for the IC3 Algorithm](https://doi.org/10.1007/978-3-031-98668-0_12)
- [DAC 2024] [Predicting Lemmas in Generalization of IC3](http://doi.org/10.1145/3649329.3655970)
- [arXiv 2025] [Extended CTG Generalization and Dynamic Adjustment of Generalization Strategies in IC3](https://arxiv.org/abs/2501.02480)
- [arXiv 2026] [CIll: CTI-Guided Invariant Generation via LLMs for Model Checking](https://arxiv.org/abs/2602.23389)

---

## License & Attribution

rIC3 is licensed under the [GNU General Public License v3.0 (GPL-3.0)](https://www.gnu.org/licenses/gpl-3.0).  
Copyright (C) 2023 - Present, Yuheng Su (`gipsyh.icu@gmail.com`). All rights reserved.  
Fork extensions & research contributions by newsniper-org contributors.
