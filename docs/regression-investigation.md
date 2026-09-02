# Regression investigation — 23 cases the reference solves and we do not

**Status:** PLANNED, blocked on the baseline run completing (640/840 at the time
of writing, ≈2.5 days remaining). **Date:** 2026-08-27.
**Decision:** option C — finish the run, then re-measure only the regressions
against `v1.5.2`.

---

## 1. What was observed

At 640/840 the per-instance diff against the CAV'25 artifact shows:

|metric|value|
|---|---|
|compared|640|
|**regressions** (reference solved, we did not)|**23**|
|improvements (we solved, reference did not)|16|

All 23 are `ours=timeout`. Seventeen of them are the `picorv32_mut{A,B,C}X_nomem-p*`
family; the rest are `counter_bit_width_large`, `gcd_2+newton_3_7`, `h_RCU`,
`mul7`, `rocket_1951`, `rushhour.4.prop1-func-interl`.

Regressions were **0 through the first 206 cases**, so this is not a property of
the whole run.

## 2. Causes ruled out, with evidence

|hypothesis|verdict|evidence|
|---|---|---|
|memory pressure / zram|**ruled out**|peak RSS median 0.6 GiB, max 2.7 GiB against a 32 GB cap|
|external swap contamination|**ruled out**|`sda3_kb=0` for the whole run; watchdog flagged 0 events|
|preprocessing cost|**ruled out**|`mul7`: scorr 14.1 s + frts 2.6 s, then 3601 s timeout. `picorv32_mutBX_nomem-p8`: scorr 2.4 s + frts 0.05 s|
|configuration mismatch|**ruled out**|see §3|

### 2.1 What the logs do show

Lemma counts per frame, at timeout:

    picorv32_mutBX_nomem-p8:  1985 67 8 34 128 2785 17826 54285 117999 80008 105
    rocket_1951:              2955 31 63 66 112 343 372 365 920 2070 2528 5578 7477 7692 8653 9985 ...
    mul7:                     reached depth 1 in 3601 s

Frame 8 holding 117,999 lemmas is a generalisation failure, not a resource
limit. The engine is doing work, just not converging.

## 3. Configuration is not the cause — checked against the paper's tag

The obvious suspicion was that `--drop-po=false` (which we are forced into,
since `IC3Config::validate()` panics on `dynamic && drop_po`) differs from what
the paper ran. It does not. `v1.5.2`, the release contemporary with the paper:

```rust
// v1.5.2:src/config.rs
if self.dynamic && self.drop_po {
    error!("cannot enable both ic3-dynamic and ic3-drop-po");
    panic!();
}
```

Identical logic, and `drop_po` defaults to `true` in both. So the artifact
README's `-e ic3 --ic3-dynamic` **panics as written**, and the actual
experiments must have disabled drop-po exactly as we do. Our reproduction
configuration matches.

Independent check: running `gcd_2+newton_3_7` with plain `ic3` (no `--dynamic`,
drop-po at its default) also failed to finish inside 150 s, against the
reference's 18 s. So the gap is not specific to the DynAMic path either.

## 4. Leading hypothesis

The pin `7149d56` is **67 commits past `v1.5.2`** and postdates the paper. A
performance regression somewhere in those 67 commits is the live hypothesis —
exactly the possibility `docs/BASELINE.md` §6 was written to keep open:

> record the divergence rather than assuming local misconfiguration … an
> upstream behaviour change is a live hypothesis alongside hardware

Competing hypothesis: this host's microarchitecture is genuinely bad for these
instances. Weak, because a 200× gap on `gcd_2+newton_3_7` (18 s → >3600 s) is
not a cache-size effect.

---

## 5. Procedure

### 5.1 Finish first (blocking)

Complete the 840-case run. The regression list is **not final** — 200 cases
remain, and more regressions may appear. Fix the list only from the completed
`verdicts.json`.

### 5.2 Re-measure the regressions against `v1.5.2`

```sh
# separate worktree, so the pinned tree and its target/ stay untouched
just git worktree add ../ric3-v152 v1.5.2
cd ../ric3-v152 && git submodule update --init --recursive
cargo build --release            # its own target/, no conflict

# regression list, extracted from the completed run
uv run --python 3.14 bench/compare.py bench/results/baseline-840 \
    bench/corpus/reference/rIC3-ic3-cav25.txt --show 999 \
    | sed -n '/--- regressions/,/--- improvements/p' \
    | awk '/ours=/ {print $1}' > bench/corpus/reference/regressions.txt

# re-measure only those, same limits, v1.5.2 binary
uv run --python 3.14 bench/harness.py bench/corpus \
    --instance-list bench/corpus/reference/regressions.txt \
    --ext aig --dedup-by-stem \
    --engine ic3 --engine-arg=--dynamic --engine-arg=--drop-po=false \
    --timeout 3600 --memory-limit-mb 32768 \
    --binary ../ric3-v152/target/release/ric3 \
    --out bench/results/v152-regressions
```

Cost: 23 cases, most of which the reference solved in under 2000 s, so ≈12 h
worst case.

Outcomes:

|result|conclusion|
|---|---|
|`v1.5.2` solves them|**upstream regression confirmed** → §5.3|
|`v1.5.2` also times out|host/microarchitecture, or the artifact's numbers are not reproducible on any machine we have. Record as divergence and stop|
|mixed|split the list and treat each group separately|

### 5.3 Bisect — cheaper than it looks

If confirmed, `git bisect` over the 67 commits needs ⌈log₂ 67⌉ = **7 steps**.
Each step is a build plus one decisive instance, and the decisive instance is
already identified:

- **`gcd_2+newton_3_7`** — reference 18 s, we exceed 3600 s. A 200× gap makes
  the good/bad call unambiguous, and a 60 s cutoff decides it in under a minute.

So a step costs ≈2 min of build plus ≈1 min of measurement: **under 25 minutes
total** to name the commit. This is the cheapest part of the whole
investigation, which is why it is worth doing rather than filing a vague
"slower than the paper" report.

    just git bisect start 7149d56 v1.5.2
    # per step:
    cargo build --release && \
      timeout 60 ./target/release/ric3 check --ui false \
        bench/corpus/.../gcd_2+newton_3_7.aig ic3 --rseed 0 \
        --dynamic --drop-po=false | tail -1
    # UNSAT within 60 s -> good ; timeout -> bad

### 5.4 Report

If a commit is named, this becomes an upstream issue with a reproducer, in the
same shape as `docs/upstream-pr-ic3-lemma-inject.md`: the instance, the two
timings, the commit, and the measurement conditions.

---

## 6. Constraints that still apply

- **Nothing CPU-intensive until the run finishes.** That includes the `v1.5.2`
  build. `docs/BASELINE.md` §4.0.
- **Never rebuild `target/release/ric3`** while the harness is live; it launches
  that binary per case. The worktree in §5.2 exists for this reason.
- Environment must stay frozen: no swap changes, no additional workloads.

## 7. What this does not change

The baseline itself remains valid and is still worth completing. It measures
**our pinned fork on our hardware**, which is what every later stage-1 claim is
a delta against. The 23 regressions are a finding *about the pin*, not a defect
in the measurement — and `regressions = 0` as a **release gate** applies to
changes we make against this baseline, not to the baseline versus the paper.
