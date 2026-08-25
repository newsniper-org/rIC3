# UPSTREAM.md — Pinned upstream baseline

**Status:** active. **Created:** 2026-08-25. **Stage:** 1, work item 1.1 (fork hygiene).

This file is the single source of truth for *which* upstream rIC3 our fork is
measured against. Every performance or verdict claim made anywhere in this
project is a delta against the commit pinned here. Changing this pin requires
its own PR with a full benchmark re-run attached (AGENTS.md §1.1).

---

## 1. The pin

|field|value|
|---|---|
|upstream repository|`https://github.com/gipsyh/rIC3`|
|upstream remote branch|`upstream/master`|
|**pinned commit**|**`7149d568785b039134f0b2baa58358c8af63e70d`**|
|abbreviated|`7149d56`|
|commit subject|`Bug fix of Gptr (#164)`|
|commit author|Yuheng Su `<gipsyh.icu@gmail.com>`|
|commit date|`2026-06-28 16:09:40 +0800`|
|`git describe --tags`|`v1.5.2-67-g7149d56`|
|nearest release tag|`v1.5.2` (`02c12ff1407b3474b7eb85c4531a06ad2eb86933`, 2025-12-07)|
|crate version|`rIC3` 1.5.2|
|crate edition|Rust 2024|
|upstream license|GPL-3.0|

### Fork position at time of pinning

    fork remote (origin) : https://github.com/newsniper-org/ric3
    local branch         : master
    commits ahead of upstream/master  : 0
    commits behind upstream/master    : 0

The fork is **byte-identical to the upstream tip** as of this pin. This is the
ideal starting condition for stage 1: the baseline measured in
`docs/BASELINE.md` is unambiguously *upstream's* number, not a number
contaminated by fork changes. Any future divergence is therefore attributable.

Verify the pin at any time with:

```sh
git merge-base HEAD upstream/master   # must print the pinned commit
```

### Submodule pins

`Cargo.toml` consumes all seven solver/IR dependencies by **path**, not by
version from crates.io, so the submodule SHAs below are part of the baseline
and must be recorded with it. `examples/fvbench` is the local example corpus.

|path|pinned SHA|describe|
|---|---|---|
|`deps/aig-rs`|`aa7ac1b4f0b8ebf55e5a0981b15eb317c24561f6`|`v0.4.6-1-gaa7ac1b`|
|`deps/bitwuzla-rs`|`3743b786048a2f1fb06306b54e890f4f3bdb3b88`|`v0.1.0-9-g3743b78`|
|`deps/btor-rs`|`4eaa050a4a207f73a7dabf9fce214dced553154d`|`v0.2.2-14-g4eaa050`|
|`deps/cadical-rs`|`9cdc994b0c90ecdb797b0bc9a26f3ca7f3551742`|`v0.2.2-5-g9cdc994`|
|`deps/giputils`|`16a16e7254fe91537c6ef2cd98509d6dec345d54`|`v0.3.6-23-g16a16e7`|
|`deps/kissat-rs`|`0ce9de57fb8ff1028a1d84d559d0bf2f59b58d86`|`v0.5.2-6-g0ce9de5`|
|`deps/logicrs`|`236e94c236aa5c4887fa9f6eb5be5936b27012f4`|`v0.6.1-94-g236e94c`|
|`examples/fvbench`|`c77b0287698324bf036abb3cbcb1a5ce76143d1e`|`heads/master`|

Nested (third-party C) submodules resolved transitively:

|path|pinned SHA|
|---|---|
|`deps/aig-rs/aiger`|`322d70b799ffc5e4570093183198abb27cae042a`|
|`deps/bitwuzla-rs/bitwuzla`|`95e71f3b9322f4892bb54510e72765d32b10b9b1`|
|`deps/cadical-rs/cadical`|`f13d74439a5b5c963ac5b02d05ce93a8098018b8`|
|`deps/kissat-rs/kissat`|`8af8e56f174b778aef3aa45af9f739b2a5f492c2`|

Restore the full tree with:

```sh
git submodule update --init --recursive
```

---

## 2. Upstream's published result claims

These are the numbers the pinned upstream lineage claims, taken from *The rIC3
Hardware Model Checker* (arXiv:2502.13605, CAV 2025). They are reproduced here
as the **claims to be reproduced**, not as measurements of our hardware. The
measured counterpart lives in `docs/BASELINE.md`.

### HWMCC'19–'24 combined, 840 cases, single engine, single thread

|checker|solved|PAR-2|
|---|---|---|
|**rIC3-ic3**|**606**|**2147.70**|
|nuXmv-cav23|533|—|
|ABC-pdr|516|—|
|Avy|488|—|
|IC3ref|486|—|

### HWMCC'24, 319 cases, portfolio

|checker|solved|
|---|---|
|**rIC3-portfolio** (16 threads)|**245**|
|ABC-superprove|226|

**Caveat on attribution.** The paper describes the rIC3 lineage, not
necessarily commit `7149d56` specifically; the pinned commit is 67 commits
past tag `v1.5.2` and postdates the paper. Treat the table above as the
*target*, and `docs/BASELINE.md` as the authoritative measurement. If our
measured single-thread number diverges from 606 beyond run-to-run noise,
record the divergence rather than assuming a local misconfiguration — the
delta may be a genuine upstream change.

Scoring protocol (per AGENTS.md §1.2): **solved count and PAR-2 only**. Wall
clock on a hand-picked instance is not a result.

---

## 3. Build environment used for the baseline

|field|value|
|---|---|
|host CPU|AMD Ryzen 7 260 w/ Radeon 780M Graphics|
|OS / kernel|Linux 7.0.9-1-cachyos-rt-bore, `SMP PREEMPT_RT`|
|`rustc`|1.96.0 (`ac68faa20`, 2026-05-25)|
|`cargo`|1.96.0 (`30a34c682`, 2026-05-25)|
|build command|`cargo build --release`|
|build result|success, exit 0|
|binary|`target/release/ric3`, reports `rIC3 1.5.2`|

**`PREEMPT_RT` note.** The host runs a real-time preemption kernel with the
BORE scheduler. This is not a typical HWMCC evaluation environment and may
affect timing variance and PAR-2 stability, especially for the 16-thread
portfolio configuration. Quantify run-to-run noise before quoting any figure,
and state the noise band alongside every number in `docs/BASELINE.md`.

---

## 4. Consequences for stage 1 work items

Facts about the pinned upstream that stage-1 infrastructure must account for.
Recorded here because they were discovered while establishing the pin; none of
them are changed by this document.

1. **The release profile is unprofilable as shipped.** `Cargo.toml` sets
   `[profile.release]` with `lto = true`, `strip = true`, and
   `panic = "abort"`. Work item 1.3 requires `perf` self-time on *release +
   debuginfo*; `strip = true` removes the symbols that requires. A separate
   profile (inheriting release, with `debug = true` and `strip = false`) is
   needed before profiling, and the profiling profile must be reported
   alongside the numbers since LTO changes inlining and therefore self-time
   attribution.

2. **`panic = "abort"` constrains the cache's untrusted-input requirement.**
   AGENTS.md §1.5 requires that a corrupted cache entry "must cause a
   fall-through to a full run, never a verdict and never a panic." With
   `panic = "abort"`, `catch_unwind` is not a usable recovery mechanism in
   release builds. Deserialization of cache entries must therefore be
   total — fallible-by-return-value (`Result`), with no panicking indexing,
   slicing, or `unwrap` on cache-derived data.

3. **Verdict is not carried by the exit code.** Measured on the pinned build:
   both a SAT (unsafe) and an UNSAT (safe) result exit with status `0`.

       ric3 check --ui false examples/fvbench/counter/counter.btor ic3  -> "SAT",   exit 0
       ric3 check --ui false examples/fvbench/fifo/fifo.btor    ic3  -> "UNSAT", exit 0

   The benchmark harness must parse the verdict from stdout and must treat
   exit status only as a crash/timeout signal. A harness keyed on exit codes
   would silently score every instance identically and invalidate the release
   gate ("zero verdict changes").

4. **Upstream already has a cache concept named `ric3proj`.** The CLI exposes
   `ric3 build` ("Build ric3proj with ric3.toml") and `ric3 clean` ("Clean up
   verification cache (ric3proj)"). Work items 1.4/1.5 must establish whether
   the method/digest cache extends `ric3proj` or sits beside it, and must not
   assume the name is free.

5. **Upstream already emits certificates.** `ric3 check` accepts
   `--cert <CERT>` and `--certify` (certifaiger / cerbtora). Since the
   artifact stage 1 caches *is* the inductive invariant, this existing path is
   the natural source and validator of the stored invariant and should be
   evaluated before a bespoke serialization format is designed.

6. **The engine is a required subcommand.** `ric3 check <MODEL>` alone exits
   with status 2 and a usage error; an engine (`ic3`, `bmc`, `portfolio`, …)
   must be supplied. The harness must set the engine explicitly and record it
   per AGENTS.md §1.2, rather than relying on a default.

7. **`--ui true` is the default and pollutes machine-readable output.** The
   harness must pass `--ui false`.

8. **`examples/fvbench` is not the HWMCC corpus.** It holds 11 small
   SystemVerilog designs (`arbiter`, `counter`, `fifo`, `frame_proc`,
   `gray_counter`, `multiplier`, `onehot_fsm`, `priority_enc`, `stall_pipe`,
   `xor_flip`) with per-design `ric3.toml`, but only three pre-generated
   BTOR2 files (`counter`, `fifo`, `multiplier`); the rest require a
   Yosys/SymbiYosys flow to lower `.sv` to `.btor`. HWMCC'19–'24 must be
   provisioned separately for the 840-case baseline. fvbench remains useful as
   a fast smoke corpus and as raw material for the five profile shapes of
   work item 1.3.

---

## 5. Rebase policy

Per AGENTS.md §1.1: `main`/`master` tracks a **rebase** onto upstream, never a
merge-commit tangle. All fork work lands on topic branches. An upstream rebase
is its own PR, updates the pin in §1 of this file, and carries a full
benchmark re-run against the new pin.
