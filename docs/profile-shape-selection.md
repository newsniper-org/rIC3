# Profiling shape selection for §1.3

**Status:** SELECTED, not yet profiled. **Date:** 2026-08-26.
**Plan item:** C1 prerequisite. **Tool:** `bench/classify_shapes.py`.

AGENTS.md §1.3 requires `perf` self-time across five structurally distinct
workload shapes, and **forbids stage 4 from proceeding without the resulting
decision record**. Picking the five by name would undermine that, so they are
derived from the models.

---

## 1. Method

BTOR2 is line-oriented (`<id> <op> <sort> <operands...>`), so operator
histograms and declared bit widths come out of a text scan — no solver, no term
construction. That let this run while the 840-case baseline holds the machine.

Ratios are over *logic* lines only; `sort`, `input`, `state`, `next`, `init`,
`bad`, `constraint` and constants are excluded so that declaring more state
cannot dilute an operator share.

De-duplication by case identifier is applied: scanning HWMCC'19 and '20
together yielded 641 files but **536 unique cases**, matching the independently
computed union in `docs/BASELINE.md` §2 — a useful cross-check that the
identifier function agrees across tools.

Reproduce:

```sh
uv run --python 3.14 bench/classify_shapes.py \
    bench/corpus/hwmcc19/btor2/bv bench/corpus/hwmcc20/btor2/bv \
    --restrict bench/corpus/reference/cases-840.txt
```

---

## 2. Selection

`ln` = logic lines, `st` = states, `w` = max declared width, `ar` = arithmetic
ratio, `ct` = control ratio, `ref` = reference outcome.

|#|shape|instance|evidence|
|---|---|---|---|
|1|control-dominated FSM|**`vis_arrays_buf_bug`**|ln 549, st 18, ar 0.004, **ct 0.676**, ref solved/93 s|
|2|deep counter / timer|**`vcegar_QF_BV_ar`**|ln 27, st 2, **w 2501**, ref solved/15 s|
|3|wide arithmetic datapath|**`cal209`**|ln 13282, st 113, **mul 120**, ref timeout|
|4|large memory / array-heavy|**`ridecore_array_unsafe`**|ln 44623, st 948, **array sorts present**, w 137|
|5|known-hard, minutes|**`dspfilters_fastfir_second-p18`**|ln 6732, st 743, ref **solved/1783 s**|

Runners-up, in case one of the above turns out unrepresentative once profiled:
`vis_arrays_bufferAlloc` (1), `stack-p1` w 1029 (2), `cal210` (3),
`ridecore_array` (4), `picorv32_mutBX_nomem-p0` solved/1705 s (5).

### 2.1 Why these

**Shape 1** — control ratio 0.68 with arithmetic at 0.004 is as cleanly
control-dominated as the corpus gets. Chosen over `vis_arrays_bufferAlloc`
(ct 0.679, marginally higher) because the reference *solves* it in 93 s, so
there is a complete self-time distribution rather than a truncated one.

**Shape 2** — 2501-bit state across only 2 state variables in a 27-line model
is the counter signature: almost no logic, enormous width. This is the shape
AGENTS.md §1.3 calls "the k-induction killer" and names as the stage-2 target.

**Shape 3** — 120 multipliers is decisive; nothing else in the bit-vector track
comes close. Note the reference times out, which is acceptable here: profiling
a hard instance for a bounded window still yields a self-time distribution, and
hard arithmetic is precisely the interesting case.

**Shape 4 — this one is outside the 840-case suite, deliberately.** The paper's
suite is the *bit-vector* track and it contains **zero array sorts**, so no
member of it is genuinely memory/array-heavy. The tool reports this rather than
substituting a lookalike. `ridecore_array_unsafe` comes from the array track
(569 unique instances, all with array sorts). The bit-vector fallback ranking
independently put plain `ridecore` first by aggregate state width, so the two
methods agree on which design is memory-dominated.

**Shape 5** — 1783 s in the reference is squarely "runs for minutes" and it is
solved, so the profile is of a completed proof rather than a truncation.

---

## 3. Consequences to carry into the profile

1. **Shape 4 is off-suite.** Any claim derived from it must say so; it is not
   part of the baseline and never enters the verdict gate.
2. **Shapes 3 and 4 are reference-timeouts.** Fix a wall-clock window and
   report it with the numbers, since self-time shares from a truncated run are
   only comparable against equally truncated runs.
3. **Use the `profiling` profile**, not `release`: `release` sets
   `strip = true`, so `perf` sees no symbols. `lto` is inherited deliberately,
   so inlining matches the shipped build — and the profile used must be quoted
   with every figure.

       cargo build --profile profiling
       perf record -g --call-graph dwarf target/profiling/ric3 check --ui false \
           <model> ic3 --dynamic --drop-po=false

4. **Do not run this until the baseline completes.** `perf` is CPU-intensive
   and would corrupt the in-flight measurement.

---

## 4. What the profile must answer

AGENTS.md §1.3 warns against inheriting the `portable-algebraic-aotjit`
finding (~75 % front-end) and says to expect invariant derivation to dominate
instead — but to **measure it**. So the decision record must state, per shape:

- dominant self-time function and its share;
- the split between front-end (parse, bit-blast, CNF encoding) and search
  (SAT queries, generalisation, propagation);
- whether the dominant cost differs *between* shapes, which is the whole point
  of using five.

That output is what stage 4 is gated on, and it also settles the open question
in `docs/toolchain-interop-patterns.md` §1.4: whether representation cost is
large enough to reopen the specification-language question.
