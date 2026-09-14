# STAGE 3: Lebesgue — Well-Founded Ranking Function Induction for Liveness

**Stage:** 3 of 4 (algorithmic innovation).  
**Blocks:** Stage 4 (Parallelism & GPU Tier).  
**Depends on:** Stage 1 (`ric3-middleware::spec::rank`, Method/Digest Cache) & Stage 2 (Riemann).  
**Status:** ACTIVE.  
**Branch:** `stage3`.

---

## 0. Motivation: The Liveness-to-Safety Bottleneck

Standard model checking of liveness properties (e.g. $\mathbf{G}\mathbf{F} p$ or $\mathbf{F} \text{Goal}$):

1. **The L2S (Liveness to Safety) Transformation Penalty:**  
   Traditional tools double the state latch count by introducing a shadow copy of the state vector to detect non-terminating cycles:
   $$s_{\text{saved}} = s \land \text{loop\_detected}$$
   Doubling the latch count ($2 \times |L|$) squares the state space, causing IC3's frame propagation and SAT solving to degrade drastically.

2. **The Lebesgue Alternative (Non-Uniform Partition Basis):**  
   Instead of uniform time unrolling, Stage 3 partitions the state space along the level sets of a **well-founded ranking function** $R: S \to \mathcal{W}$ ("Lebesgue" integration analogy):
   $$\mathcal{S}_w = \{ s \in S \mid R(s) = w \}$$
   If $R(s)$ decreases strictly across transitions under fair paths:
   $$T(s, s') \land \neg \text{Goal}(s) \implies R(s') \prec_{lex} R(s)$$
   then the well-foundedness of $(\mathcal{W}, \prec_{lex})$ mathematically guarantees termination and liveness **without state doubling**.

---

## 1. Algorithmic Architecture

1. **Lexicographic Ranking Function Basis (`ric3-middleware::spec::rank`):**
   - Supports tuples of ranking measures $\vec{r} = (r_1, r_2, \dots, r_m)$.
   - Automatically synthesizes strict decrease clauses:
     $$\vec{r}' \prec_{lex} \vec{r} \iff (r_1' < r_1) \lor (r_1' = r_1 \land r_2' < r_2) \lor \dots$$

2. **Ranking Function Candidate Generator (`lebesgue::rank_gen`):**
   - Extracts progress measures from counter sub-circuits (from Stage 2 `CounterDetector`) and FSM distance-to-goal metrics.

3. **RLive Lebesgue Engine Integration (`src/rlive/`):**
   - Couples synthesized ranking invariants directly into the fairness cycle-breaking solver.

---

## 2. Work Items & Implementation Status

- [x] **3.1 Ranking Function Synthesizer:** Automatic progress measure extraction from `Transys` latches and counters (`RankingSynthesizer`).
- [x] **3.2 Strict Decrease Condition Builder:** CNF synthesis of $\vec{r}' \prec_{lex} \vec{r}$ integrated into transition relation constraints (`StrictDecreaseBuilder`).
- [x] **3.3 RLive Lebesgue Solver Integration:** Connect ranking function induction into `src/rlive/` without shadow state doubling (`LebesgueRliveEngine`).
- [x] **3.4 HWMCC Liveness Empirical Evaluation:** Evaluated on HWMCC justice benchmarks (`analog_estimation_convergence`, `shift_register_top`, `circular_pointer_top`) in `docs/lebesgue-liveness-report.md`.
