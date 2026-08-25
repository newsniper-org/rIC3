# BASELINE.md — measured upstream baseline on our hardware

**Status:** measurement IN PROGRESS, started 2026-08-25. **Stage:** 1, work
item 1.2. **Pin:** see `docs/UPSTREAM.md` (`7149d56`).

Every later performance claim in this project is a delta against this file
(AGENTS.md §1.2). Until the "Measured result" table below is filled in, no
stage-2/3/4 performance claim may be made.

---

## 1. What is being reproduced

The rIC3 paper (arXiv:2502.13605, CAV 2025) reports, for the single-thread
`rIC3-ic3` engine over the combined HWMCC'19+'20+'24 suite:

|figure|published value|
|---|---|
|cases|840|
|solved|606|
|timeouts (TO)|225|
|memory-outs (MO)|9|
|PAR-2|2147.70|

### 1.1 The published numbers are internally consistent — verified

The paper's artifact (`https://github.com/gipsyh/rIC3-CAV25`) ships the
per-instance result file `result/rIC3-ic3.txt`. We copied it to
`bench/corpus/reference/rIC3-ic3-cav25.txt` and recomputed the aggregate with
the artifact's own formula (`utils/evaluatee.py`):

$$\mathrm{PAR}\text{-}2 = \frac{\sum_{\text{solved}} t_i + N_{\text{unsolved}}
\cdot 2T}{N}, \qquad T = 3600\,\text{s}$$

Recomputation reproduces the paper **exactly**: 840 cases, 606 solved, 225 TO,
9 MO, PAR-2 2147.70. Two consequences:

1. The published aggregate is not a rounding artefact, and the per-instance
   file is the authoritative reference for a per-case diff.
2. `bench/harness.py`'s `score()` implements the same formula, so our number
   and theirs are computed identically. `bench/compare.py` re-derives the
   reference over whatever subset we measured, so a partial run can never be
   compared against the 840-case figure by accident.

`Failed` in the artifact's result file means memory-out; `Timeout` means
timeout. This is how the 9 MO / 225 TO split was recovered.

---

## 2. Corpus construction

The paper: "the complete benchmark suites from the bit-level track (AIGER
format) and the word-level bit-vector track (Btor2 format) of ... HWMCC'19,
HWMCC'20, and HWMCC'24 ... After removing duplicates, the combined suite
comprised 840 unique cases, each available in both AIGER and Btor2 formats."

The **bit-vector** track only; the array track is excluded.

|archive|source|AIGER|btor2 bv|btor2 array|
|---|---|---|---|---|
|HWMCC'19|`fmv.jku.at/hwmcc19/hwmcc19-single-benchmarks.tar.xz`|317|317|312|
|HWMCC'20|`fmv.jku.at/hwmcc20/hwmcc20benchmarks.tar.xz`|324|324|315|
|HWMCC'24|Zenodo record **14156844**, `benchmarks_aiger.tar.gz`|319|—|—|

AIGER and btor2-bv counts match 1:1 within each archive, consistent with the
bit-level set being a bit-blasting of the word-level bit-vector set.

The de-duplication arithmetic closes exactly:

    HWMCC'19 ∪ HWMCC'20  (by case identifier)  = 536
    840 − 536                                  = 304  unique to HWMCC'24
    HWMCC'24 total                             = 319  (15 shared)

Reproduce with:

```sh
uv run --python 3.14 bench/prepare_corpus.py bench/corpus \
    --instance-list bench/corpus/reference/cases-840.txt
# -> requested cases : 840 / resolvable : 840
```

### 2.1 Two traps in the corpus, both resolved

**(a) Case identifiers are not unique across directories.** HWMCC'24 ships two
*different* models under one basename:

    hwmcc24/aiger/2019/mann/safe/analog_estimation_convergence.aig
    hwmcc24/aiger/2019/mann/unsafe/analog_estimation_convergence.aig

The artifact separates them as `analog_estimation_convergence_safe` (0.52 s)
and `..._unsafe` (0.17 s), while `analog_estimation_convergence` (0.84 s) is
the HWMCC'19 file. `bench/prepare_corpus.py` creates the two suffixed aliases,
scoped to collisions *within one archive* — scoping corpus-wide would relabel
the HWMCC'19 file and leave the unsuffixed identifier unmatched. Same-stem
different-format files (`.aig` vs `.btor2`) are not collisions, by design of
the suite.

**(b) `Path.resolve()` destroys the aliases.** Those aliases are symlinks, so
resolving them collapses the distinct identifier back onto its target and the
case disappears. This turned an 840-case run into 838 silently. The harness
now uses `abs_no_symlink()`.

Both traps are the AGENTS.md §1.4(a) hazard — an identifier that is not an
identifier — appearing in the corpus layer rather than the clause layer. Case
identity is defined once, in `harness.instance_stem()`, and every consumer
(`compare.py`, `prepare_corpus.py`) imports it.

---

## 3. Configuration under measurement

`rIC3-ic3` is not the default CLI configuration. Per the paper (§7.1) it "utilizes
the DynAMic heuristic method and the specifically optimized SAT solver.
Internal signals and localization abstraction are excluded", and the artifact
README runs it as `-e ic3 --ic3-dynamic <AIGER>`.

On the pinned CLI that is `ic3 --dynamic`, plus one non-obvious requirement:
`IC3Config::validate()` (`src/ic3/mod.rs`) **panics** if `dynamic` and
`drop_po` are both enabled, and `--drop-po` defaults to `true`. So the
reproduction configuration is:

```sh
just bench-baseline
# = bench/harness.py bench/corpus
#     --instance-list bench/corpus/reference/cases-840.txt
#     --ext aig --dedup-by-stem
#     --engine ic3 --engine-arg=--dynamic --engine-arg=--drop-po=false
#     --timeout 3600 --memory-limit-mb 32768
```

|constraint|paper|ours|
|---|---|---|
|time limit|3600 s wall clock|3600 s wall clock|
|memory limit|32 GB|32768 MiB via `prlimit --as`|
|threads|single|single (`--jobs 1`)|
|input format|AIGER|AIGER (`--ext aig`)|
|seed|not stated|`--rseed 0`, recorded|

**Memory-limit caveat.** `prlimit --as` bounds virtual address space, not RSS,
whereas competition harnesses use cgroups. An mmap-heavy process can trip the
cap with a much smaller RSS. Every record stores the cap next to the measured
peak RSS so a suspicious MO can be re-classified without re-running. If our MO
count diverges materially from the published 9, this is the first thing to
suspect — not the solver.

---

## 4. Hardware and environment

|field|paper|ours|
|---|---|---|
|CPU|AMD EPYC 7532 @ 2.4 GHz|AMD Ryzen 7 260 (Radeon 780M)|
|OS|Ubuntu 24.10|Linux 7.0.9-1-cachyos-**rt**-bore|
|RAM|(32 GB cap enforced)|46 GB total, **no swap**|
|toolchain|—|rustc/cargo 1.96.0|

Two environmental facts that must be quoted alongside any number from this
file:

- **`PREEMPT_RT` + BORE scheduler.** Not a typical HWMCC evaluation
  environment; expect wider timing variance than a stock server kernel.
- **No swap.** With a 32 GB cap on a 46 GB machine, an instance approaching the
  cap has no swap to fall back on. Combined with other resident workloads this
  is a plausible source of MO divergence from the paper.

### 4.1 Measured run-to-run noise

On `examples/fvbench/fifo.btor` (ic3, 4 repetitions, cold and warm):

    9.07 s, 9.32 s, 9.32 s, 9.82 s     spread ≈ 8 % of the median

A cold/warm split measured at PAR-2 4.9174 vs 4.6673 on a two-instance set is
**entirely within this band**. There is no cache in the tree yet, so the
cold/warm axis currently measures noise and nothing else. Do not read it as a
cache result.

**Noise floor rule:** an improvement smaller than ~8 % on a handful of
instances is not distinguishable from noise on this host. Report solved count
and PAR-2 over the full suite, never a wall clock on a hand-picked instance
(AGENTS.md §1.2).

---

## 5. Measured result

> **NOT YET AVAILABLE.** The 840-case run started 2026-08-25 and is expected to
> take ≈10 days. Do not quote or infer these numbers until this section is
> filled in and this Status line is changed to `complete`.

Runtime is dominated by timeouts, which no amount of CPU speed reduces:
225 unsolved × 3600 s ≈ 225 h, against ≈33 h (paper hardware) of solved-case
time. A faster host converting timeouts into solves would *shorten* the run.

|figure|published|measured here|delta|
|---|---|---|---|
|cases|840|_pending_||
|solved|606|_pending_||
|TO|225|_pending_||
|MO|9|_pending_||
|PAR-2|2147.70|_pending_||

Artifacts, once complete, live in `bench/results/baseline-840/`:
`results.jsonl` (per instance), `summary.json`, `verdicts.json` (the diffable
input to the "zero verdict changes" gate), `environment.json`.

Per-case diff against the paper:

```sh
just bench-compare bench/results/baseline-840
```

### 5.1 Preliminary observation (NOT the baseline)

A 25-case subset chosen from the fastest reference instances (all solved by
both) gave a runtime ratio of **median 0.28×** ours/reference — this host is
roughly 3–4× faster per case than the paper's EPYC 7532 on short instances.
Regressions: 0. This is a sanity check on the harness, not a result: 25 cases
of 840, all trivially fast, and the ratio says nothing about hard instances
where memory bandwidth dominates.

---

## 6. Divergence policy

If the measured solved count differs from 606 beyond run-to-run noise, record
the divergence here rather than assuming local misconfiguration. The pinned
commit is 67 commits past tag `v1.5.2` and **postdates the paper**, so an
upstream behaviour change is a live hypothesis alongside hardware and the
address-space-vs-RSS caveat of §3. Rank the candidates and test the cheapest
first; do not silently tune the configuration until the number matches.
