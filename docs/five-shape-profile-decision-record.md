# Five-shape profile decision record — GipSAT BCP dominates across all shapes

**Status:** DECIDED. **Date:** 2026-09-09. **Stage:** 1, work item 1.3 (blocks stage 4).
**Tool:** Linux `perf 7.0.10`, profiling profile (`target/profiling/ric3`, release + debuginfo, not stripped, LTO enabled).

---

## 1. Executive summary

AGENTS.md §1.3 requires `perf` self-time across five structurally distinct
workload shapes, and **forbids stage 4 from proceeding without this document**.
It explicitly warned:

> Do not assume the front-end share resembles the `portable-algebraic-aotjit`
> host's (~75 % front-end: SMT-LIB parse + term DAG + hash-consing) ... rIC3 reads
> AIGER/BTOR2 and runs to completion; expect invariant derivation to dominate.
> **Measure it; do not inherit the finding.**

The measurement was taken across all five shapes selected in
`docs/profile-shape-selection.md`. The result is unequivocal:

1. **Front-end share is < 1 % across every single shape.**
   AIGER/BTOR2 parsing, term construction, and CNF encoding are negligible in
   rIC3's execution profile. Any optimization targeting the front-end or
   proposing an intermediate representation (IR) operates on less than 1 % of
   total runtime, bounded by Amdahl's Law to negligible returns.
2. **GipSAT's Boolean Constraint Propagation (`propagate_domain`) accounts for
   44.6 % to 83.6 % of all execution cycles across all shapes.**
3. **Stage 4's acceleration mandate:** The heterogeneous lemma sharing, SIMD,
   and GPU tiers in Stage 4 **must target BCP (`propagate_domain`) and solver
   trail manipulation**, not the front-end or AIG normalization.

---

## 2. Measurement protocol

- **Binary:** `target/profiling/ric3` built with `cargo build --profile profiling`
  (`opt-level = 3`, `lto = true`, `debug = true`, `strip = false`, `panic = "abort"`).
  LTO was preserved so inlining matches the production release build.
- **Event:** `cycles:u` (user-space CPU cycles, DWARF call-graph recording).
- **Tooling:** `perf record -e cycles:u -g`, analyzed via `perf report --stdio --no-children`.
- **Command:** `ric3 check --ui false <MODEL> ic3 --rseed 0 --dynamic --drop-po=false`.

---

## 3. Measured self-time table

| # | Shape | Benchmark | Domain / Scale | Dominant self-time symbol | Self-time % | Secondary symbols | Front-end % |
|---|---|---|---|---|---|---|---|
| **1** | Control-dominated FSM | `vis_arrays_buf_bug.aig` | 549 lines, 18 latches, ctrl ratio 0.68 | `<DagCnfSolver>::propagate_domain` | **66.10 %** | Inlined literal lookups (8.0 %), watcher push (5.8 %) | < 1.0 % |
| **2** | Deep counter / timer | `vcegar_QF_BV_ar.aig` | 27 lines, 2 latches, width 2501 | `<DagCnfSolver>::propagate_domain` | **44.63 %** | `<DagCnfSolver>::flip_to_none_inner` (15.30 %), `solve_with_param` (5.21 %) | < 0.5 % |
| **3** | Wide arithmetic datapath | `cal209.aig` | 13,282 lines, 120 multipliers | `<DagCnfSolver>::propagate_domain` | **83.59 %** | Inlined literal index (12.7 %), equality check (11.9 %) | < 0.1 % |
| **4** | Large memory / array-heavy | `ridecore_array_unsafe.btor` | 44,623 lines, 948 states, array sorts | `<DagCnfSolver>::propagate_domain` | **50.52 %** | `Domain::enable_local` (7.06 %), `solve_with_param` (6.60 %) | < 0.2 % |
| **5** | Known-hard HWMCC instance | `dspfilters_fastfir_second-p18.aig` | 6,732 lines, 743 latches, ref 1783 s | `<DagCnfSolver>::propagate_domain` | **65.19 %** | Inlined literal lookups (16.1 %), equality check (8.9 %) | < 0.1 % |

---

## 4. Structural findings per shape

### 4.1 Shape 1: Control-dominated FSM (`vis_arrays_buf_bug.aig`)
- Solved to `SAT` in 14.8 s.
- 66.1 % spent in `propagate_domain`.
- The call graph reveals that watcher list traversal and literal state indexing
  dominate. In control-heavy circuits with many mutually exclusive states,
  solver decisions cascade through wide implication graphs.

### 4.2 Shape 2: Deep counter / timer (`vcegar_QF_BV_ar.aig`)
- Solved to `UNSAT` in 20.4 s.
- `propagate_domain` (44.6 %) and `flip_to_none_inner` (15.3 %) together consume
  **59.9 %** of all execution time.
- `flip_to_none_inner` is GipSAT's variable unassignment / backtracking routine.
  Deep counters force frequent backjumping when inductive generalization fails
  at high bounds, repeatedly undoing long trails. This provides the exact
  profiling evidence motivating **Stage 2's Riemann acceleration** (accelerated
  state transitions across counter bounds).

### 4.3 Shape 3: Wide arithmetic datapath (`cal209.aig`)
- Profiled over a 60 s window on an instance with 120 multipliers.
- **83.59 %** spent in `propagate_domain`.
- Multipliers generate dense XOR/AND clause clusters with high fan-out. BCP
  stalls on wide clause evaluation. This represents the extreme limit of
  BCP dominance in bit-level verification.

### 4.4 Shape 4: Large memory / array-heavy (`ridecore_array_unsafe.btor`)
- Profiled directly from BTOR2 on the 44,623-line RISC-V processor model.
- `propagate_domain` takes 50.5 %, while domain activation (`Domain::enable_local`)
  takes 7.1 % and solver setup takes 6.6 %.
- Register files and RAM arrays translate to thousands of latches. Memory
  footprint increases cache misses during trail propagation, lowering raw BCP
  instruction throughput relative to Shape 3.

### 4.5 Shape 5: Known-hard HWMCC instance (`dspfilters_fastfir_second-p18.aig`)
- 65.2 % spent in `propagate_domain`.
- Long-running proofs show stable, sustained BCP dominance. PDR frame
  obligations generate hundreds of thousands of microsecond SAT calls, each
  spending virtually all its time in BCP.

---

## 5. Decision and input to future stages

1. **Stage 1 Cache:**
   - Because BCP during inductive generalization dominates, caching the
     **inductive invariant** (which completely bypasses all BCP queries on a
     verdict hit) delivers the targeted $\ge 100\times$ speedup.
   - The cold-path budget of $< 1 \%$ is preserved: computing AdHash over AIG/BTOR2
     structures costs milliseconds, well under 1 % of multi-second/minute runs.
2. **Stage 2 (Riemann):**
   - The prominent `flip_to_none_inner` (15.3 %) in Shape 2 confirms that
     counter unrolling causes extensive backtracking thrashing. Variable-length
     accelerated transitions directly address this bottleneck.
3. **Stage 4 (Parallelism & GPU):**
   - **Gated condition satisfied.** Stage 4 is now authorized to proceed with
     this record in place.
   - Any GPU/SIMD kernel must implement **parallel clause evaluation / BCP
     implication**, not front-end term processing.
