# STAGE 4: Parallelism — SIMD BCP, Narrow-Scope GPU Tier, and Heterogeneous Lemma Sharing

**Stage:** 4 of 4 (hardware acceleration & parallelism).  
**Depends on:** Stage 1 (`docs/five-shape-profile-decision-record.md`, `lemma::ContentLemmaBridge`), Stage 2 (Riemann), Stage 3 (Lebesgue).  
**Status:** ACTIVE.  
**Branch:** `stage4`.

---

## 0. Prerequisite Mandate: Empirical Justification

Per AGENTS.md §1.3 and §4, **no performance work on the solver core is permitted without the profile decision record in hand**.

In `docs/five-shape-profile-decision-record.md`, `perf` self-time was measured across five structurally distinct workloads. The empirical findings were unequivocal:

| Workload Shape | Dominant Function | Self-Time Share | Structural Finding |
|---|---|---|---|
| 1. Control FSM | `propagate_domain` (GipSAT BCP) | **56.8%** | BCP dominates; watcher traversal critical |
| 2. Deep Counter | `solve_with_param` & BCP | **41.0%** | SAT queries dominate |
| 3. Wide Datapath | `propagate_domain` (GipSAT BCP) | **65.2%** | BCP completely saturates cycles |
| 4. Large Memory | `propagate_domain` (GipSAT BCP) | **50.5%** | Memory latency in watcher cache lines |
| 5. Hard HWMCC | `propagate_domain` (GipSAT BCP) | **65.2%** | Front-end < 0.1%; BCP is the entire run |

**Conclusion:** GipSAT's Boolean Constraint Propagation (BCP) loop accounts for **50–65% of all execution cycles**. Optimizing BCP via SIMD and parallel architectures is now mathematically and empirically authorized.

---

## 1. Technical Architecture

### 1.1 Vectorized Watcher List Scanning (SIMD / AVX2)
In GipSAT (`src/gipsat/propagate.rs`), the inner BCP loop traverses watcher lists:
```rust
struct Watcher {
    blocker: Lit,
    cref: ClauseRef,
}
```
Currently, watchers are inspected sequentially. In a 256-bit SIMD register, 8 blocking literals (`8 x u32`) can be compared simultaneously against the solver's assignment vector. Non-blocking watchers are skipped in bulk, eliminating branch mispredictions and memory stalls.

### 1.2 Heterogeneous Lemma Sharing (`ric3-middleware::lemma`)
In 16-thread portfolio runs (`src/portfolio/mod.rs`), workers often diverge in internal variable numbering due to preprocessing.
- Stage 4 replaces positional `LitVec` IPC broadcasts with `ContentLemmaBridge`.
- `SurvivalMeter` tracks the survival rate of shared lemmas per worker, dynamically throttling ineffective workers and boosting high-yield sources.

### 1.3 Pluggable Dynamic-Loading Multi-Backend GPU Tier
Per architectural decision, the GPU acceleration tier adheres to **strict dynamic loading (dlopen / dynamic dispatch)** without hard compile-time shared-object linking, ensuring the binary never aborts on machines lacking specific vendor toolchains:

1. **Default Backend (Vendor-Agnostic):**
   - **Vulkan Compute / WGPU:** Automatically addresses both AMD Radeon 780M (Zero-Copy UMA host-shared memory) and NVIDIA RTX 5050 (high-throughput GDDR6 dGPU).
2. **Optional Specialized Backends:**
   - **NVIDIA CUDA:** Dynamically probes `libcuda.so.1`.
   - **AMD ROCm / HIP:** Dynamically probes `libamdhip64.so`.
   - **Vendor-Agnostic OpenCL:** Dynamically probes `libOpenCL.so.1`.
3. **Graceful Fallback:** If dynamic loading fails or no compatible device is detected, execution transparently falls back to CPU AVX2 SIMD (`SimdWatcherScanner`).

---
## 2. Work Items & Implementation Status

- [x] **4.1 SIMD BCP Watcher Scanner:** Vectorized batch-checking of blocking literals in GipSAT (`SimdWatcherScanner`).
- [x] **4.2 Heterogeneous Lemma Exchange Pipeline:** Wires `ContentLemmaBridge` and `SurvivalMeter` into `AdaptiveLemmaRouter`.
- [x] **4.3 Narrow-Scope Dynamic Multi-Backend GPU Tier:** Dynamic loading prober (`DynamicBackendProber`), vendor-agnostic Vulkan default, optional CUDA/ROCm/OpenCL, and zero-panic CPU fallback (`GpuOffloadManager`).
- [x] **4.4 Full Portfolio PAR-2 Evaluation:** Measured 16-thread portfolio speedup (1.77× on `fifo.btor`) and documented in `docs/stage4-parallelism-report.md`. All 27 unit/integration tests passing.
