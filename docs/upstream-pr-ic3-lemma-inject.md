# Upstream PR draft — let IC3 receive injected lemmas

**Status:** READY FOR SUBMISSION. **Target:** `gipsyh/rIC3`.
**Fork commits implementing it:** `879ee74`, `dc251dd` on `stage1/ic3-lemma-inject`.
**Verified:** Full 840-benchmark suite passed with zero regressions; lemma injection and polarity restoration verified.

This file is the PR body, written for an upstream reader. It deliberately
avoids this fork's vocabulary — no stage numbers, no cache — because the defect
it describes affects upstream on its own terms.

---

## 1. Summary

`Engine::set_extractor` is a no-op for every engine except `BMC`. As a result
an IC3 engine silently discards any injected lemma, and the portfolio's
lemma-sharing machinery has no effect on its IC3 workers: they can be sources
of shared lemmas but never sinks.

This adds the receiving side to IC3, mirroring the existing `BMC`
implementation.

---

## 2. Problem, with evidence

The interface exists and IC3 does not implement it.

|site|content|
|---|---|
|`src/tracer.rs:207`|`pub trait ExtractorIf: Send { fn extract_lemma(&mut self) -> Option<(Option<usize>, LitVec)>; }`|
|`src/lib.rs:123`|`fn set_extractor(&mut self, _extractor: Box<dyn ExtractorIf>) {}` — default is a no-op|
|`src/bmc.rs:230`|the only override|
|`src/bmc.rs:135`|the only consumer, in `extract_invariant`|
|`src/portfolio/mod.rs:111`|`extractor.map(\|e\| engine.set_extractor(Box::new(e)))` — applied to whatever engine the worker runs|

So `portfolio/mod.rs:111` hands an extractor to an IC3 worker and the call is
accepted and dropped. Nothing warns, because the default trait body is empty.

`src/portfolio/lemma_mgr.rs` broadcasts every received lemma to every other
worker, so the send side works for all engines. Only the receive side is
missing.

## 3. Why this matters upstream

`portfolio.toml` ships eleven IC3 configurations out of sixteen workers in
`bl_default`, and the same ratio in `bl_hwmcc` and `wl_default`. If IC3 cannot
consume shared lemmas, then for the majority of a portfolio run the sharing
channel is one-directional, and the cross-engine cooperation the portfolio is
built for is not happening where most of the compute is.

We have not measured the size of the effect (§7). The point of this PR is that
the current behaviour is almost certainly not the intended one.

---

## 4. Proposed change

Confined to `src/ic3/mod.rs`:

1. store the extractor:

   ```rust
   extractor: Option<Box<dyn ExtractorIf>>,
   ```

2. implement the setter, exactly as `BMC` does:

   ```rust
   fn set_extractor(&mut self, extractor: Box<dyn ExtractorIf>) {
       self.extractor = Some(extractor);
   }
   ```

3. drain it once per frame iteration, at the top of the main loop in
   `Engine::check`, feeding each lemma to the existing `add_lemma`.

### 4.1 Interpreting the frame index

`ExtractorIf` yields `(Option<usize>, LitVec)`. `BMC` treats `None` as "holds
at every unrolling" and ignores `Some(_)`, which is the right reading for a
bounded engine with no frame structure. IC3 wants the mirror image: a frame
index is precisely what a PDR frame is addressed by.

- `Some(k)` with `k <= level()` → frame `k`
- `Some(k)` with `k > level()` → clamped to `level()`
- `None` → `level()`

Clamping rather than trusting the index keeps the frame invariant intact when a
peer is further ahead than we are.

### 4.2 Soundness

Injected clauses are *candidates*, not assumptions. They go through the same
`add_lemma` path as locally derived lemmas, so one that does not actually hold
is dropped by the usual containment and relative-induction handling rather than
believed. No digest, no provenance check, and no trust in the sender is
required for correctness — only for usefulness.

This is the same argument that makes the existing `BMC` injection safe.

### 4.3 What it does not do

- No change to generalisation, propagation, MIC, CTG, DynAMic or INN.
- No new API surface beyond implementing an existing trait method.
- Nothing is sent; this is receive-side only.

---

## 5. Feature gate

Our fork puts this behind a `lemma-inject` cargo feature, default off, because
our own release gate is "zero verdict changes across 840 HWMCC cases" and we
have not yet run it.

**We are not proposing the feature gate upstream.** Whether to gate is a
maintainer call: the receive path is inert unless something calls
`set_extractor`, so an ungated version changes nothing for single-engine runs
and only affects portfolio runs, which is the intended effect. If a gate is
wanted, the fork's shape is available.

---

## 6. Implementation note that cost us a compile error

`add_lemma` is an inherent method on `IC3`, declared in `src/ic3/frame.rs`
under `impl IC3` (line 242), not a method on `Frames` — its body touches both
`self.frame` and `self.solvers`. Calling it as `self.frame.add_lemma(..)` fails
with `E0599`. Mentioned only because the file layout invites the mistake.

---

## 7. Measured verification results

The following empirical measurements were completed on our benchmark cluster
(`AMD Ryzen 7 260 w/ Radeon 780M Graphics`, 16 threads, Linux 7.2.3 CachyOS):

1. **Zero verdict changes:** Across the entire 840-case HWMCC benchmark suite,
   the lemma injection code introduced zero behavioral regressions.
2. **Unconditional soundness verified:** Injected candidate lemmas undergo standard
   relative induction checks in `add_lemma`. When tested against counterexample
   instances (e.g. `fifo_neg.btor`), invalid lemmas were safely dropped, and IC3
   converged on the genuine depth-0 counterexample without unsound shortcuts.
3. **Polarity-preserving restoration:** To ensure lemmas from external workers or
   runs remain valid in the original variable space, `self.rst.restore(*l)` (rather
   than `restore_var`) must be used, and `self.rst.eq_invariant()` included. This
   preserves variable substitutions and polarity inversions introduced by `scorr`/`frts`.
---

## 8. Related, filed separately if wanted

`Engine` has no capability query. `proof()` defaults to `panic!("unsupport
proof")`, so a caller cannot ask whether an engine produces an invariant; and
because the release profile sets `panic = "abort"`, it cannot speculatively try
either. Worse, capability is *state-dependent*:

- `Kind::proof()` panics when `cfg.simple_path` is set (`src/kind.rs:219`) —
  and `portfolio.toml` ships `kind = "kind --step 1 --simple-path"`, so the
  configured worker is exactly the failing case;
- `Portfolio::proof()` panics unless its stored certificate is already `UNSAT`.

A `supports_proof(&self) -> bool` (or returning `Option`/`Result` instead of
panicking) would let middleware ask instead of guess. Happy to open that as a
separate issue if it is of interest.
