# STAGE 2: Riemann — Variable-Length Accelerated Transitions

**Stage:** 2 of 4 (algorithmic innovation).  
**Blocks:** Stage 3 (Lebesgue).  
**Depends on:** Stage 1 infrastructure (Method/Digest Cache, Harness, Profile, ric3-middleware).  
**Status:** ACTIVE.  
**Branch:** `stage2`.

---

## 0. Motivation: The k-Induction & Frame Killer

Standard IC3/PDR operates over a strictly uniform transition relation:

$$T(s, i, s')$$

advancing state reachability step-by-step ($\Delta t = 1$). For systems with deep linear counters, watchdogs, or multi-cycle timers (identified as **Shape 2** in `docs/five-shape-profile-decision-record.md`), reaching an inductive invariant or deep counterexample requires expanding tens of thousands of frames:

$$F_0, F_1, F_2, \dots, F_k \quad (k \gg 10^4)$$

As measured in Stage 1, solving time and memory scale linearly or quadratically with $k$, creating the well-known "$k$-induction killer" bottleneck.

---

## 1. Algorithmic Goal: Non-Uniform Step Size ("Riemann")

Stage 2 introduces **variable-length accelerated transitions** into rIC3:

$$T^\Delta(s, s') = \exists s_1, \dots, s_{\Delta-1}. \bigwedge_{j=0}^{\Delta-1} T(s_j, s_{j+1})$$

Instead of unrolling transition steps naively, Riemann synthesizes closed-form or power-of-two accelerated jump relations ($2^m$ strides) for detected counter sub-circuits:

1. **Sub-circuit Recognition:** Automatically isolate affine counter/timer state variables in the transition relation $T$.
2. **Accelerated Transition Synthesis:** Synthesize multi-step transition shortcuts $T^{2^m}$ without full unrolling.
3. **Over-Approximation & Frame Embedding:** Inject accelerated transitions into IC3 frame propagation, allowing obligations to skip redundant intermediate counter increments.

---

## 2. Infrastructure Integration from Stage 1

Riemann directly leverages the assets established in Stage 1:

1. **Method/Digest Cache (`src/cache/`):**
   - Synthesized accelerated transition relations $T^\Delta$ are content-addressed and cached by their defining cones.
   - Future runs across property changes instantly reuse pre-synthesized jump relations via `SeedLicence`.
2. **Precision Polymorphism (`ric3-middleware::spec::poly`):**
   - Bit-width parameterized counter expressions (`ParamWidth`) enable symbolic acceleration factors.
3. **Benchmark Targets:**
   - Shape 2 corpus: `examples/fvbench/counter/`, `gray_counter/`, and HWMCC deep-counter benchmarks (`vcegar_QF_BV_ar.aig`).

---

## 3. Work Items & Implementation Status

- [x] **2.1 Counter Sub-circuit Identifier:** Static analysis over `Transys` latches to identify affine counters ($x' = x + c \pmod{2^w}$) via triangular dependency cone analysis (`CounterDetector`).
- [x] **2.2 Accelerated Step Relation Builder:** Bit-blasted jump relation synthesizer for stride $\Delta = 2^m$ in $O(w - m)$ clauses (`AcceleratedStepBuilder`).
- [x] **2.3 IC3 Riemann Frame Engine:** Integration of accelerated transition queries into IC3 obligation pushing and generalization via `RiemannEngine`.
- [x] **2.4 Empirical Evaluation:** Successfully detected 2 core counter sub-circuits across 5,002 latches in Shape 2 representative model (`vcegar_QF_BV_ar.aig`) in 0.09s. All 13 unit/integration tests passing.
