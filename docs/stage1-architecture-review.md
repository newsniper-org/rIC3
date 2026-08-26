# Stage 1 — architecture review and integrated plan

**Workflow:** `architecture_review`. **Status:** ACTIVE.
**Date:** 2026-08-26. **Pin:** `7149d56` (see `docs/UPSTREAM.md`).
**Branch:** `stage1/benchmark-harness`.

Executed on `claude_opus_primary` at the user's direction because
`claude_fable_primary` — which `.omp/workspace.yml` assigns to `planCreation` —
is currently failing. This deviates from the configured role split and is
recorded here rather than left implicit.

---

## 1. Scope and method

Reviewed against AGENTS.md §3 ("Definition of done") and the conclusions
recorded in `docs/toolchain-interop-patterns.md`. Every claim below is either
measured on this host or cited to a file in the tree; nothing is inherited from
the paper without saying so.

The organising constraint of this plan is not priority but **contention**: a
840-case measurement holds the CPU for ~9.3 more days, and any CPU-intensive
work invalidates it. So the plan is split by whether an item can proceed
*during* the run.

---

## 2. Gap analysis against the definition of done

|AGENTS.md §3 requirement|state|evidence|
|---|---|---|
|1. `docs/BASELINE.md` exists, numbers reproducible|**partial** — methodology and corpus construction recorded; §5 figures pending|30/840 measured, ETA 9.3 d|
|2. dated five-shape profile decision record|**not started** — blocked by CPU contention|—|
|3. cache layer merged, zero verdict changes cold+warm|**not started**|—|
|3b. warm identical re-run: verdict hit, ≥100× faster|**not started**|—|
|3c. same $T$, changed property: measurable gain + seeded-clause survival rate|**not started**|—|
|4. cold-path overhead < 1 % of baseline|**not started**|—|

Infrastructure completed beyond the checklist (all committed):

|item|artifact|
|---|---|
|upstream pinned, fork verified identical|`docs/UPSTREAM.md`, 0 ahead / 0 behind|
|harness: solved count, PAR-2, cold/warm, peak RSS, MO classification|`bench/harness.py`|
|reference diff, subset-aware re-scoring|`bench/compare.py`|
|corpus 840/840, identifier collisions resolved|`bench/prepare_corpus.py`|
|PAR-2 formula cross-verified against the artifact|reproduces 840/606/225/9/2147.70 exactly|
|toolchain interop discipline; IR provisionally rejected|`docs/toolchain-interop-patterns.md`|

### 2.1 Early validation of the run (30 cases)

|metric|ours|reference|
|---|---|---|
|solved|12|9|
|PAR-2|4433.23|5082.40|
|**regressions**|**0**|—|
|improvements|3|—|

All four memouts matched the reference's `Failed`, with peak RSS 24.7–30.2 GB
against a 32 GB cap — real exhaustion, not an `RLIMIT_AS` false positive. This
validates the MO classifier and the memory-limit reproduction simultaneously.

**Projection:** with a ~0.35× runtime ratio, some reference timeouts will
resolve, so the final solved count will likely **exceed 606**. Per
`docs/BASELINE.md` §6 this is recorded as divergence with its cause (faster
hardware), not tuned away.

---

## 3. Architecture decisions — current standing

|decision|standing|basis|
|---|---|---|
|No new **IR**|provisionally rejected|three IR layers already exist; interop currency is AIGER/BTOR2; ecosystem diff capability would be forfeited|
|**Specification language** (jitter/noise polymorphic, 1:1 proper subset)|**open, gated**|conservative extension; brings its own round-trip gate; only proposal with a mechanical soundness criterion|
|Front-end / middleware split|**endorsed on its own merits**|`lib.rs` exports everything; upstream edits → 0 collapses rebase cost, which this run prices at ~10 days each|
|Cache as a wrapper over `create_bl_engine`|endorsed as the sequencing|verdict licence before the factory; seed licence injects into `ts`; `proof()` is the artifact|
|Cross-domain edges exchange **contracts**|endorsed|BTOR2 `constraint` in/out already exists; §1.4(d) makes refinement reuse sound|
|`replay` layer from the crate|rejected (AGENTS.md §5)|microsecond-scale, 10⁵–10⁶ events|

### 3.1 Load-bearing constraints discovered, not assumed

1. **`panic = "abort"`** — no `catch_unwind`. Cache deserialisation must be
   total (`Result`, no panicking index/slice/`unwrap`). Also blocks speculative
   capability probing on engines.
2. **Verdict is not in the exit code** — both SAT and UNSAT exit 0.
3. **`strip = true` + `lto = true`** in release — the profile required by §1.3
   needs a separate profiling profile, and LTO changes inlining, so the profile
   used must be reported with the numbers.
4. **rIC3-ic3 ≠ default CLI** — needs `--dynamic` *and* `--drop-po=false`;
   `IC3Config::validate()` panics otherwise.
5. **Identity must be content-derived** — three silent failures in one day
   (840→811, 840→838 twice).
6. **No capability query** on `Engine`; `proof()` defaults to `panic!`.

---

## 4. Risk register

|risk|likelihood|impact|mitigation|
|---|---|---|---|
|Measurement invalidated by concurrent CPU load|high if unmanaged|restart = 10 days|§5 partitions work by contention; no benchmark or profiling until done|
|Final solved ≠ 606|**likely** (already +3/30)|baseline claim weakened|record divergence with cause per BASELINE §6; regressions (0 so far) are the real gate|
|Cache returns a wrong verdict|low|**release blocker**|two distinct licences as distinct code paths, never a bool; verdict short-circuit gated on whole-formula digest only|
|Corrupted cache entry panics|medium|abort kills run|total deserialisation; no `catch_unwind` available|
|`Engine`/`TransysIf` churn breaks the wrapper|medium|rework|compile-time breakage is loud; cheaper than rebase conflicts|
|Container image drifts under `:latest`|medium|certificate trust undermined|pin by `@sha256:`|
|Spec-language subset not actually 1:1|unknown|premise collapses|round-trip gate answers it before any language work|
|Host RAM 46 GB with 32 GB cap, no swap|medium|MO divergence|already observed 30.2 GB peaks; keep host otherwise idle|

---

## 5. Integrated plan

### Phase A — during the measurement (~9.3 days, no CPU-intensive work)

|#|item|why now|acceptance|
|---|---|---|---|
|A1|Round-trip gate **implementation** (BTOR2 → parse → deparse → BTOR2)|`Deparser` exists; language-agnostic; also a BTOR2 fidelity check|tool runs on one model, byte- or structure-level diff reported|
|A2|Cache layer **design record**: two licences, digest boundary, storage format, version field|no CPU; unblocks C2 immediately after the run|written to `docs/`; API preconditions for §1.4(d) monotonicity stated with intended tests|
|A3|Atom identity scheme (content-derived) design|shared root of the cache **and** lemma exchange|one specification, both consumers named|
|A4|Profiling profile in `Cargo.toml` (`debug`, no `strip`)|§1.3 cannot start without it|`cargo build --profile profiling` produces symbols; release untouched|
|A5|Config hygiene: `mcp_sqlite_server.py`, `../shared_data`, failover order, `allowedCommands`|found during review; unrelated to CPU|MCP servers start, or the stanza is removed|
|A6|Capability whitelist for engines (`proof()` support)|needed by the cache wrapper|table in code with a comment naming the panic hazard|

**Explicitly not in Phase A:** any `cargo build --release` benchmark run, any
`perf` session, any 840-case re-measurement, container digest pinning
(behaviour change needing verification).

### Phase B — immediately after the run completes

|#|item|acceptance|
|---|---|---|
|B1|Fill `docs/BASELINE.md` §5; flip Status to `complete`|figures present; divergence from 606 explained, not tuned|
|B2|`just bench-compare` full 840 diff; publish regressions/improvements|**regressions = 0** is the gate|
|B3|Noise band: repeat a stratified subset (`--repeat 3`)|per-instance spread quoted alongside every later claim|
|B4|Container digest pinning + certifaiger/cerbtora re-verify|certificates still validate|
|B5|Round-trip gate **execution** over 840|verdict invariance, or the subset boundary is characterised|

### Phase C — stage-1 substance

|#|item|acceptance (AGENTS.md §3)|
|---|---|---|
|C1|Five-shape `perf` profile → dated decision record|table + dominant self-time per shape + structural conclusions; **stage 4 is forbidden without this**|
|C2|Cache layer behind a default-off cargo feature|zero verdict changes cold+warm across 840|
|C3|Verdict licence path|warm identical re-run ≥100× faster|
|C4|Seed licence path|measurable gain on a stated benchmark count + **seeded-clause survival rate reported**; a negative result is published, not buried|
|C5|Cold-path overhead measurement|< 1 % of baseline, else the fold is wrong|

### Phase D — deferred, gated

|#|item|gate|
|---|---|---|
|D1|Front-end/middleware split as a separate crate|C2 merged; `Engine` stability observed across one upstream rebase|
|D2|Spec-language prototype: precision polymorphism only|B5 green|
|D3|Quantification, lexicographic ranks, contract syntax|D2 plus a stage-3 consumer|
|D4|Lemma exchange over content-derived identity; survival-rate instrumentation|A3 plus C1 (frequency evidence)|

### 5.1 Dependency order

    A4 ─────────────► C1 ──► (stage 4 unlocked)
    A2 ─┬─► C2 ─┬─► C3
    A3 ─┘       └─► C4 ──► D4
    A1 ─────────────► B5 ──► D2 ──► D3
    B1,B2,B3 ────────► (all later performance claims)
    C2 ─────────────► D1

---

## 6. What this plan deliberately does not do

- **No algorithmic change to IC3** (AGENTS.md §4). Frames, MIC, CTG/EXCTG,
  DynAMic, IC3-INN, localization abstraction are untouched.
- **No native codegen** anywhere in `portable-algebraic-aotjit`.
- **No solver-core performance work** — stage 4, gated on C1.
- **No configuration tuning to make the measured number match 606.** Divergence
  is data.
- **No new IR.** Revisit only under §9 of `docs/toolchain-interop-patterns.md`.

---

## 7. Open questions for the plan owner

1. **Upstream PRs.** A capability query on `Engine` (`supports_proof()`) and a
   `--drop-po` default that does not conflict with `--dynamic` would both be
   better fixed upstream than worked around. Out of fork scope — decide whether
   to raise them.
2. **Spec-language scope.** Phase D2 assumes precision polymorphism first. If
   the driving use case is really contracts (mixed-signal), the order flips.
3. **`role` assignment.** The assignment referenced by the user is not present
   in `.omp/*.yml`, `.env`, or the IPC directories; it appears not to have been
   saved. Re-apply before relying on role-based routing.
