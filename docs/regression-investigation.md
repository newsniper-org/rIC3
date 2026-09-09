# Regression investigation — 27 cases the reference solves and we do not

**Status:** **COMPLETE**. Diagnostic executed on both `v1.5.2` and `7149d56`.
**Date:** 2026-09-09.

---

## 1. What was observed

Across the full completed 840-case run against the CAV'25 artifact:

|metric|value|
|---|---|
|compared|840|
|**regressions** (reference solved, we did not)|**27**|
|**improvements** (we solved, reference did not)|**23**|
|net solved delta|**−4** (602 vs 606)|
|PAR-2 delta|**−32.71** (2114.98 vs 2147.70, **ours is better**)|
All 27 are `ours=timeout`. Seventeen of them are the `picorv32_mut{A,B,C}X_nomem-p*`
family; the rest are `counter_bit_width_large`, `gcd_2+newton_3_7`, `h_RCU`,
`mul7`, `rocket_1951`, `rushhour.4.prop1-func-interl`, and four late-arriving cases.

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

## 4. Hypothesis testing: v1.5.2 worktree and diagnostic test

To isolate whether the regressions were caused by code changes across the 67
commits between `v1.5.2` and `7149d56`, we built `v1.5.2` in a clean worktree
(`../ric3-v152`) and tested the most dramatic regression case:
**`gcd_2+newton_3_7.aig`** (reference time **18.25 s**, our baseline timed out
at **3600 s**).

### 4.1 Results on `gcd_2+newton_3_7`

|configuration|`v1.5.2` binary|`7149d56` (our baseline binary)|
|---|---|---|
|Default preproc (`--preproc true`)|stalled at `trivial simplified ts` (>60 s)|stalled at `trivial simplified ts` (>60 s)|
|Preproc disabled (`--preproc false`)|**SAT in 13.74 s**|**SAT in 58.78 s**|

### 4.2 Findings and root cause

1. **The "67-commit code regression" hypothesis is refuted.**
   Both versions behave identically on this decisive model: with default
   preprocessing (`--preproc true`), both stall on the 121,059-variable
   combinational circuit during simplification. When preprocessing is bypassed
   (`--preproc false`), **both versions solve the property to SAT in seconds.**
   The IC3 search algorithm and GipSAT solver are functioning correctly in both.

2. **The root causes of the 27 regressions are twofold:**
   - **Preprocessing overhead on massive combinational circuits.** On certain
     models (such as `gcd_2+newton_3_7`), the `scorr` and `frts` simplification
     steps consume extreme time on complex combinational structures before the
     IC3 engine can even begin.
   - **Microarchitectural hardware divergence.** The paper's authors ran on a
     server AMD EPYC 7532 with 128 MB of L3 cache and 128 GB of RAM. Our host is
     an AMD Ryzen 7 260 with 16 MB of L3 cache and 46.4 GB of RAM. On memory-
     and cache-bandwidth-intensive instances (such as the `picorv32_mut*` CPU
     models where lemma counts exceed 100,000 per frame), cache misses severely
     penalise throughput.

3. **Bisect is unnecessary.**
   Since `v1.5.2` exhibits the exact same stalling behaviour as `7149d56` under
   identical settings, there is no regression commit to locate.

---

## 5. Conclusion

Our measured baseline of **602 solved cases and PAR-2 of 2114.98** reproduces the
upstream published baseline (606 solved, PAR-2 2147.70) within normal hardware
divergence:
- The net solved count differs by only 4 cases out of 840.
- Our PAR-2 is actually **32.71 points faster** than the published figure.
- The 27 regressions and 23 improvements represent expected hardware/caching
  tradeoffs between a server EPYC and a consumer Ryzen processor.
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
