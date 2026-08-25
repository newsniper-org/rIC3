# AGENTS.md — Stage 1: Fork, Harness, and the Method/Digest Cache

**Stage:** 1 of 4 (infrastructure). **Blocks:** stages 2, 3, 4.
**Status:** active. **Standalone:** this file is self-contained; do not assume
the other stage briefs have been read.

---

## 0. What this project is

We are building a **fork of rIC3**, the IC3/PDR hardware model checker
(This repo is a local clone of the fork. Upstream: `https://github.com/gipsyh/rIC3`; first place in
the bit-level and word-level bit-vector tracks of HWMCC'24 and '25; see
*The rIC3 Hardware Model Checker*, arXiv:2502.13605, CAV 2025).

The fork pursues a four-stage research program:

| stage | axis | one-line goal |
|---|---|---|
| **1 (this file)** | infrastructure | fork, reproducible baseline, profile, and a model+invariant cache |
| 2 | non-uniform **step size** | variable-length accelerated transitions ("Riemann") |
| 3 | non-uniform **partition basis** | well-founded / ranking-function induction, targeting liveness ("Lebesgue") |
| 4 | parallelism | heterogeneous lemma sharing, SIMD, and a narrowly-scoped GPU tier |

Stage 1 builds **only infrastructure**. It introduces no new proof technique.
Its job is to make stages 2–4 measurable and to harvest the one cheap win that
needs no algorithmic change: **reuse across runs**.

### The motivating observation

IC3 emits an **inductive invariant** as its certificate. For a transition
system $(I, T)$ and property $P$, a set $\mathit{Inv}$ is a valid certificate iff

$$I \Rightarrow \mathit{Inv}, \qquad
  \mathit{Inv} \wedge T \Rightarrow \mathit{Inv}', \qquad
  \mathit{Inv} \Rightarrow P .$$

**Deriving $\mathit{Inv}$ is the entire cost of the run. Re-checking a stored
$\mathit{Inv}$ is three SAT queries.** Any workload that re-runs a proof over a
mostly-unchanged design — CI re-verification, multi-property runs sharing one
transition relation, `[tasks]`/`chparam` sweeps, and the invariant-development
loop where a human adds one helper assertion per iteration — pays the full
derivation cost every time. Stage 1 stops that.

---

## 1. The reuse substrate: `portable-algebraic-aotjit`

We reuse an existing in-house crate rather than inventing a cache format.

**Repository:** `https://codeberg.org/newsniper-org/portable-algebraic-aotjit`
(Rust, BSD-2-Clause-Patent, pure-Rust and portable by design — it runs under
`wasmi`; it contains **no native codegen and must not acquire any**).

Read `docs/algebraic-aotjit-codegen-rejected.md` and
`docs/algebraic-deegen-meta-generator-design.md` before touching it.

What it provides:

- **`digest`** — an AdHash clause-set fold. `clause_name_hash` hashes one
  clause from its sorted, de-duplicated `(name, polarity)` literals;
  `combine_fold` adds `(sum, count)` pairs; `fold_to_digest` collapses to 32
  bytes via K12-256. The fold is an **exact multiset homomorphism**:

  $$\texttt{combine\_fold}(\mathrm{fold}(A), \mathrm{fold}(B)) = \mathrm{fold}(A \uplus B)$$

  so a stable half can be precomputed once and a delta folded in
  $O(|\text{delta}|)$.
- **`method::Method<A>`** — a precompiled, reusable "prelude" unit: its
  clause-fold (its identity), a handle→atom resolver, and a collision flag.
  `Method::region_key()` is the prelude-only digest, routed through the *same*
  `compose_digest` expression the live verdict digest uses.
- **`guard`** — finite-field-free algebraic guards (`EquivClass`,
  `SkeletonShape`) checked before reuse fires.
- **`replay`** — a CDCL trace-event interpreter. **We do not use this layer.**
  See §5.

### Adapting the vocabulary to a model checker

| crate concept | our binding |
|---|---|
| prelude clause set | the transition relation $T$ + environment constraints |
| per-query delta | the property/bad-state clauses ($\neg P$) |
| `region_key` | digest of $T$ + constraints alone |
| `Method` | preprocessed model artifacts **+ the stored inductive invariant** |
| `Guard::SkeletonShape` | structural hash of the property's fan-in cone (COI) |
| `diverged` → fall-through | certificate re-check fails → seed and run normal IC3 |

---

## 2. Work items

### 1.1 Fork hygiene

- Fork rIC3 (already done). Keep an `upstream` remote (already done) and **pin the exact upstream commit** in
  `docs/UPSTREAM.md` with its HWMCC'24/'25 result claims.
- All fork work lands on topic branches off `main`; `main` tracks a rebase onto
  upstream, never a merge-commit tangle. Upstream rebases are their own PRs
  with a full benchmark re-run attached.
- Do not vendor `portable-algebraic-aotjit`. Depend on it by git ref, pinned.

### 1.2 Benchmark harness — build this before anything else

- Corpus: HWMCC'19–'24 (AIGER bit-level and BTOR2 word-level tracks). Record
  per-instance: wall clock, verdict, peak RSS, engine, and the seed/config.
- Scoring: **solved count and PAR-2**, per the rIC3 paper's protocol. Wall
  clock on a hand-picked instance is not a result and must never be reported
  as one.
- **Gate: reproduce the upstream baseline before changing a single line.** The
  paper reports rIC3-ic3 solving 606 of 840 combined HWMCC'19–'24 cases
  (PAR-2 2147.70) against nuXmv-cav23 533, ABC-pdr 516, Avy 488, IC3ref 486;
  and rIC3-portfolio 245 of 319 HWMCC'24 cases against ABC-superprove's 226.
  Reproduce the single-thread number on our hardware within run-to-run noise
  and write the measured figure into `docs/BASELINE.md`. Every later claim is
  a delta against that file.
- The harness must support a **cold/warm** distinction (cache empty vs. cache
  populated) — this is the axis stage 1 is measured on.

### 1.3 Profiling — produce a decision record, not a vibe

Profile `perf` self-time (release + debuginfo) across at least five workload
shapes, chosen to be structurally distinct:

1. control-dominated FSM / arbiter / handshake
2. **deep counter or timer** (the k-induction killer; the stage-2 target)
3. wide arithmetic datapath
4. large memory / array-heavy
5. a known-hard HWMCC instance that runs for minutes

Write the result as a dated decision record in `docs/`, in the style of
`algebraic-aotjit-codegen-rejected.md`: a table, the dominant self-time per
case, and the structural conclusions. **This document is the input to stage 4
and the thing stage 4 is forbidden to proceed without.**

Do not assume the front-end share resembles the `portable-algebraic-aotjit`
host's. That host measured ~75 % front-end (SMT-LIB parse + term DAG +
hash-consing) and bailed to `unknown` on every non-trivial input, so its heavy
solving never ran in-process at all. rIC3 reads AIGER/BTOR2 and runs to
completion; expect invariant derivation to dominate. **Measure it; do not
inherit the finding.**

### 1.4 The cache layer

Key design decisions, in order of how badly getting them wrong hurts:

**(a) Atom identity must be content-derived, never positional.**

The `portable-algebraic-aotjit` host has already been burned by exactly this
(§3.5.J: the recorder wrote an atom *content hash* while the replay indexed a
*pool position*, so every consult diverged and the reuse silently never
fired). AIG node indices and CNF variable numbers **shift when the design
changes** — which is precisely the situation the cache exists to handle. Key
every atom by a canonical structural hash of its defining cone. For BTOR2,
prefer declared symbol names where they exist, but fall back to the structural
hash rather than mixing schemes. Assert the choice in one place and derive
every consumer from it.

**(b) Two distinct licenses. Separate them in the type system.**

| license | precondition | what it permits |
|---|---|---|
| **verdict** | digest of the *whole* formula $T \uplus \text{constraints} \uplus \neg P$ matches byte-for-byte | return the stored verdict immediately |
| **seed** | only the `region_key` ($T$ + constraints) matches | inject stored invariant clauses as candidate lemmas, then run IC3 normally |

A `region_key` match alone is **not** a verdict license — the property differs,
so $\mathit{Inv} \Rightarrow P$ may fail. Make the two paths different types or
different functions; do not let a boolean flag decide it.

**(c) The seeding path is unconditionally sound — exploit that.**

Seeding cannot produce a wrong answer. Stored clauses enter as *candidates*;
those that survive relative-induction stay, the rest are dropped, and IC3
continues as normal. So:

- On a `region_key` miss, seeding is still worth attempting when a *near*
  match exists (same design, one assertion added). Fall-through costs one
  failed check.
- Never gate seeding on the digest. Gate only the verdict short-circuit.

**(d) Monotonicity licenses worth encoding.**

- **Adding constraints** ($T' = T \wedge C$, transitions restricted): since
  $T' \Rightarrow T$, a previously valid $\mathit{Inv}$ still satisfies
  $\mathit{Inv} \wedge T' \Rightarrow \mathit{Inv}'$; and if $I' \Rightarrow I$
  the first condition survives too. This is the model-checking analogue of the
  crate's prelude monotonicity license, and it directly covers the staged-proof
  workflow (prove lemma $A$, then assume $A$ while proving $B$).
- **Strengthening the property** ($P \to P \wedge Q$): $\mathit{Inv} \Rightarrow
  P \wedge Q$ breaks, but $\mathit{Inv}$ remains a valid over-approximation of
  the reachable states, so it stays sound **as seed material**.
- **Changing $T$**: nothing is guaranteed. Do not reason about it — just run
  the three checks. They are cheap.

Encode these as documented preconditions on the cache API, with tests.

### 1.5 Cache storage

- Content-addressed by `region_key` (32 bytes). One entry: preprocessed model
  artifacts (ABC-swept AIG, CNF encoding, COI data, the GipSAT base clause DB
  if it is cheaply serializable), the stored invariant clauses, the verdict,
  and the whole-formula digest.
- Version every entry with a **format version** and the pinned upstream commit.
  A format or upstream change invalidates entries; never silently reinterpret
  an old entry.
- Assume the cache is untrusted input. A corrupted entry must cause a
  fall-through to a full run, never a verdict and never a panic.

---

## 3. Definition of done

1. `docs/BASELINE.md` exists and its numbers are reproducible on our hardware.
2. `docs/` contains the dated five-shape profile decision record.
3. Cache layer merged, with:
   - **Zero verdict changes across the full HWMCC regression suite**, cold and
     warm. This is the release gate; a single changed verdict blocks the merge.
   - Warm re-run of an *identical* problem: verdict-license hit, ≥100× faster.
   - Warm run with the same $T$ and a changed property: measurable improvement
     on a stated number of benchmarks, with the seeded-clause survival rate
     reported. **A negative result here is a publishable finding — record it,
     do not bury it.**
4. Cold-path overhead of the cache (digest computation on a miss) is under 1 %
   of the baseline; if it is not, the fold implementation is wrong.

---

## 4. Non-goals (hard)

- **No native codegen, dynasm, or LLVM stencils anywhere in
  `portable-algebraic-aotjit`.** Its portability (pure-Rust, `wasmi`) is its
  identity, and the codegen reading was formally rejected with a profile behind
  it. If you believe you have a reason to revisit this, write a decision record
  first; do not write code.
- No algorithmic change to IC3 in this stage. Frames, MIC, CTG/EXCTG, DynAMic,
  IC3-INN, and localization abstraction are untouched. Stage 2 opens them.
- No new proof technique. No acceleration, no ranking functions.
- No performance work on the solver core. Stage 4, gated.

---

## 5. Why the crate's `replay` layer is not used

`portable-algebraic-aotjit`'s "JIT" half replays a recorded CDCL event stream
(`Decide`/`Propagate`/`Backjump`/`Restart`/`Conflict`) to reconstruct a prior
solve's trail. **This is the wrong granularity for IC3 and must not be wired
in.** IC3 is not a linear trace; it is a search over proof obligations across
frames, and its inner SAT queries are microsecond-scale and number in the
hundreds of thousands to millions. Recording and replaying at that level is
pure overhead.

The artifact worth reusing in IC3 is the **inductive invariant**, not the
search that found it. That is what §1.4 caches. The `digest`, `method`, and
`guard` modules carry over; `replay` does not.

Record this in a decision record so it is not re-proposed.

---

## 6. Conventions

- Rust 2024. No `unsafe` in new code without a decision record naming the
  invariant it upholds and a test that would fail if it were violated.
- Any capability that changes results is behind a cargo feature, default off,
  until it has a benchmark run behind it.
- Every rejected design gets a dated decision record in `docs/` with a
  **Status:** line, the profile or measurement that decided it, and what would
  have to change to reopen it. Mirror the style of
  `algebraic-aotjit-codegen-rejected.md`.
- Tests: property tests for the fold's homomorphism and order-independence
  (the crate already has these — extend them to our atom encoding), plus a
  regression test asserting cross-implementation digest equality if the digest
  is ever computed in two places.

---

## 7. Read before starting

- *The rIC3 Hardware Model Checker*, arXiv:2502.13605 (CAV 2025) — the
  algorithm, GipSAT, DynAMic, EXCTG, IC3-INN, localization abstraction, the
  16-thread portfolio, and the ablations.
- `portable-algebraic-aotjit/docs/algebraic-aotjit-codegen-rejected.md` — the
  two-prerequisite argument ((i) the target phase dominates ∧ (ii) it recurs)
  that governs every reuse decision in this project.
- `portable-algebraic-aotjit/docs/algebraic-deegen-meta-generator-design.md` —
  the §3.5.J drift class and the single-spec discipline that prevents it.
- Bradley's original IC3 paper and the Hassan–Bradley–Somenzi CTG
  generalization paper, for frame and MIC semantics.
