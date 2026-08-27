# Cache layer design — licences, atom identity, and where wrapping stops

**Status:** DESIGN. No code yet. **Date:** 2026-08-26.
**Covers:** plan items A2 (cache design record) and A3 (atom identity).
**Pin:** `7149d56`. **Prereq for:** C2, C3, C4.

Written during the 840-case baseline run, so every claim here is from reading
the pinned tree, not from experiment. Items needing measurement are marked.

---

## 1. Insertion point

`src/lib.rs` exposes the factory:

```rust
pub fn create_bl_engine(cfg: EngineConfig, ts: Transys, sym: VarSymbols) -> Box<dyn BlEngine>
```

The cache is a wrapper around this call. Nothing else in upstream needs to move
for the **verdict** path (§3). The **seed** path does need a small upstream
change, and §4 is honest about that.

---

## 2. Two licences, two types

AGENTS.md §1.4(b) requires these be different code paths, "not a boolean flag".

|licence|precondition|permits|
|---|---|---|
|**verdict**|digest of $T \uplus \text{constraints} \uplus \neg P$ matches byte-for-byte|return the stored verdict immediately|
|**seed**|only `region_key` ($T$ + constraints) matches|inject stored clauses as *candidates*, then run IC3 normally|

A `region_key` match alone is **not** a verdict licence: the property differs,
so $\mathit{Inv} \Rightarrow P$ may fail.

Encoded as distinct types so the compiler forbids confusing them:

```rust
/// Whole-formula digest matched. Sound to return without solving.
struct VerdictLicence { verdict: McResult, whole_digest: [u8; 32] }

/// Only region_key matched. Sound ONLY as candidate clauses.
struct SeedLicence { clauses: Vec<LitVec>, region_key: [u8; 32] }
```

There is deliberately no `enum Licence { Verdict, Seed }` and no
`fn get(..) -> (bool, ..)`. The two are produced by two functions with
different return types and consumed at different points in the flow.

### 2.1 Only the verdict path is gated on the digest

Per AGENTS.md §1.4(c), seeding is unconditionally sound: stored clauses enter as
candidates, survivors of relative induction stay, the rest are dropped. So:

- **Never gate seeding on a digest match.** Attempt it on a *near* miss too
  (same design, one assertion added). A failed attempt costs one check.
- **Always gate the verdict short-circuit** on the whole-formula digest.

---

## 3. Verdict path — pure wrapper, no upstream change

    digest(T ⊎ constraints ⊎ ¬P) ──hit──► return stored McResult
                                └─miss──► create_bl_engine(...) as usual

Confirmed feasible: the wrapper owns `cfg`, `ts` and `sym` before the factory
runs, which is everything the digest needs. On a hit no engine is constructed at
all, which is where the ≥100× requirement (AGENTS.md §3.3b) comes from — the
saving is the entire derivation, not a faster derivation.

---

## 4. Seed path — wrapping is not sufficient. Measured obstacle.

This is the main finding of this design pass.

### 4.1 Storing the clauses

The artifact worth caching is the clause list. Its source is:

```rust
// src/ic3/frame.rs:337
pub fn inner_invariant(&mut self) -> Vec<LitVec>
```

but it hangs off `Frames`, reached through `IC3`'s field `frame`, and
**every field of `IC3` is private** (`src/ic3/mod.rs:166`). The only public way
out is `BlEngine::proof()`, which does not return clauses:

```rust
pub struct BlProof { pub proof: Transys }   // src/transys/certify.rs:162
```

`IC3::proof()` takes the clause list, restores literals to the original
variable space via `self.rst.restore(..)`, encodes them as a DNF of AIG nodes
(`rel.new_and` / `rel.new_or`), and ORs the result into `proof.bad` — the shape
**certifaiger** expects. Recovering clauses from that encoding is possible in
principle and a bad idea in practice: it re-derives structure we already had,
and any mismatch is silent.

### 4.2 Injecting the clauses

There is a lemma-injection interface, and IC3 does not implement it:

```rust
// src/tracer.rs:207
pub trait ExtractorIf: Send { fn extract_lemma(&mut self) -> Option<(Option<usize>, LitVec)>; }
```

|site|fact|
|---|---|
|`src/lib.rs:123`|`fn set_extractor(&mut self, _extractor: Box<dyn ExtractorIf>) {}` — default is a **no-op**|
|`src/bmc.rs:230`|the **only** override|
|`src/bmc.rs:135`|the **only** consumer: `while let Some((k, lemma)) = extractor.extract_lemma()`|
|`src/portfolio/mod.rs:111`|`extractor.map(\|e\| engine.set_extractor(Box::new(e)))`|

So `set_extractor` on an IC3 engine is silently discarded. Two consequences:

1. **The cache cannot seed IC3 through existing public API.**
2. **Portfolio lemma sharing does not reach IC3 workers either** — they can
   only be *sources*, never *sinks*. Worth confirming by measurement (see
   §9), because if true it bounds how much stage 4 can expect from the
   existing sharing mechanism.

### 4.3 What must change, and how small it can be

Injecting clauses by adding them to `ts` as constraints would be **unsound** —
that restricts the transition relation instead of proposing candidates. The
correct target is the frame, via the existing internal `add_lemma`.

Minimal change, mirroring what `BMC` already does:

- implement `set_extractor` on `IC3` (store the box)
- drain it at an existing safe point in the main loop, feeding each
  `(frame_idx, LitVec)` to the internal `add_lemma` as a candidate
- optionally expose `invariant_clauses(&self) -> &[LitVec]` for the store side

This is a **fork patch to upstream source**, which §6.1 of
`docs/toolchain-interop-patterns.md` argued we should avoid because rebases cost
a full re-measurement. The mitigating facts: it is small, it duplicates an
existing accepted pattern (`BMC`), and it is a plausible upstream PR — which
would remove the fork delta entirely.

**Decision required from the plan owner** (recorded, not taken here): patch the
fork, or upstream first and wait.

---

## 5. Atom identity (plan item A3)

### 5.1 Requirement

AGENTS.md §1.4(a): atom identity must be content-derived, never positional. AIG
node indices and CNF variable numbers shift when the design changes — precisely
the situation the cache exists for.

Note the trap is *already present* in this codebase: `IC3::proof()` calls
`self.rst.restore(..)` to map literals back to the **original variable space**.
That makes preprocessing-induced renumbering invisible, which is necessary but
not sufficient — the original numbering is still AIGER file position, so it
moves when the design is edited.

### 5.2 Scope: latches, not all nodes

Clauses from `inner_invariant()` are cubes over **latch** literals. So only
latches (and inputs appearing in constraints) need stable identity, not every
AIG node. This is a large simplification and should be asserted in one place so
it cannot drift.

### 5.3 Scheme

One function, imported by every consumer — the discipline of §5 of
`docs/toolchain-interop-patterns.md`, where three silent failures came from
duplicating identity logic.

Priority order per latch:

1. **Declared symbol name**, when the front end has one. AGENTS.md §1.4(a)
   prefers this for BTOR2, and the machinery exists: `WlTsSymbol`,
   `link_wts_by_symbol()`, and `VarSymbols` (already threaded into
   `create_bl_engine`).
2. **Structural hash of the defining cone** otherwise.

**Do not mix within one model.** Record per entry which scheme was used, and
refuse reuse across schemes rather than guessing.

Structural hash normalisation, to be specified precisely before coding:

- canonical child order for commutative nodes (sort by child hash)
- polarity as a bit, not as a distinct node
- constants folded to fixed tags
- traversal depth-bounded, with the bound recorded in the entry (an unbounded
  cone hash on a large design is a cold-path cost, and §3.4 allows 1 %)

### 5.4 Collisions

The fold is `AdHash` over clause-name hashes, so a name collision silently
merges distinct atoms. Carry an explicit collision flag, as
`portable-algebraic-aotjit`'s `Method` does, and **fall through to a full run**
when set. Never resolve a collision by preferring one side.

### 5.5 Reuse for lemma exchange

The same function fixes the `LitVec` positional-identity problem in
`portfolio/lemma_mgr.rs` (§1.3(3) of the interop document). Deliberately the
same specification, two consumers — that is the point of A3 being separate from
A2.

---

## 6. Storage and representation across layers

Content-addressed by `region_key` (32 bytes). One entry holds:

|field|purpose|
|---|---|
|`format_version`|**invalidate, never reinterpret**|
|`upstream_commit`|pin at write time; mismatch ⇒ ignore|
|`identity_scheme`|`symbol` or `structural`, plus depth bound|
|`region_key`|digest of $T$ + constraints|
|`whole_digest`|digest including $\neg P$ — the verdict licence key|
|`verdict`|`McResult`|
|`var_identity`|side table: variable number → atom identity, for this region|
|`clauses`|`Vec<LitVec>` in the region's own variable numbering|
|`preproc`|**optional** preprocessed `Transys` (§6.2)|
|`collision`|flag from §5.4|

### 6.1 Three layers, three encodings — and the consumer fixes one of them

The choice of encoding is not free at every layer, because the *consumption*
point is already decided by upstream's API:

```rust
fn extract_lemma(&mut self) -> Option<(Option<usize>, LitVec)>;   // tracer.rs:208
pub(super) fn add_lemma(&mut self, frame: usize, lemma: LitVec, ..) // frame.rs:244
```

Both take `LitVec`. So the in-memory form at the point of use is **not a design
choice** — it is `LitVec` over the *current* variable numbering. Everything else
arranges itself around that:

|layer|encoding|why|
|---|---|---|
|**on-wire** (lemma IPC, cross-process)|atom-identity tuples|must be self-describing; the peer has a different variable space, and a side table cannot ride along on every message at 10³/s|
|**on-disk**|side table + `LitVec`|6–8× smaller (measured §6.1.1); the table is valid exactly when `region_key` matches|
|**in-memory (hot)**|`LitVec`|forced by `extract_lemma` / `add_lemma`|
|**in-memory (translation only)**|atom-identity tuples|transient, only on a `region_key` near-miss|

**The rule that falls out:** *identity is boundary currency, `LitVec` is the
working form.* Identity appears when crossing a process or a design change, and
nowhere else. The hot path never materialises it.

This keeps §5.5 honest — one identity specification, two consumers — while
letting each layer encode it for its own cost profile. It is a logical/physical
split, not a compromise.

#### 6.1.1 Size, measured

latch counts over 536 cases: min 2, **p50 171**, p90 329, max 6194. Observed
`avg_mic_cube_len`: 4.5–11.0 literals.

For 1000 clauses of 6 literals at 171 latches:

|encoding|clause body|side table|total|
|---|---|---|---|
|identity tuples|1000 × 6 × 33 B = 198 KB|—|**198 KB**|
|side table + `LitVec`|1000 × 6 × 4 B = 24 KB|171 × 36 B = 6.2 KB|**30 KB**|

6.5× at that size, 8× at fifo scale (5607 clauses × 12 literals: 2.2 MB vs
275 KB).

#### 6.1.2 Two load paths

|situation|path|
|---|---|
|`region_key` matches|side table is valid → deserialise `LitVec` and use **directly**; no identity materialised|
|near-miss (design changed)|translate through identity: side table → identity → current model's identity→var map → `LitVec`|

Only the second path pays the translation cost, and only it needs the identity
scheme at run time. Both end in `LitVec`, as they must.

#### 6.1.3 The cache does not stay resident

A verdict hit needs one lookup; a seed hit needs one entry's clauses. Neither
wants an LRU or a warm pool. Entries are loaded once and dropped.

This is not a micro-optimisation on this host: zram occupancy was observed at
25–38 GB against 46.4 GB of RAM during the baseline run (`docs/BASELINE.md`
§4.0). A resident cache would compete with the very instances it is meant to
accelerate.

### 6.2 Preprocessed artifacts — store conditionally

AGENTS.md §1.5 lists these as candidates. An earlier revision of this document
deferred them on the grounds that the cost was unmeasured. **The measurement
arrived and reverses that.**

Observed `frts`/`scorr` times from the baseline run:

|model size|preprocessing|
|---|---|
|1,258,074 vars|**1003.32 s**|
|1,257,889 vars|1002.76 s|
|738,926 vars|898.61 s|
|555,353 vars|520.46 s|
|247,516 vars|172.63 s|

`--frts-tl` defaults to 1000 s and `--scorr-tl` to 200 s, so preprocessing can
consume up to a third of the 3600 s budget. The `fifo` observation of
`frts: ... in 0.00s` that motivated deferring was simply not representative.

Who benefits:

- **verdict hit: nobody.** The wrapper returns before `create_bl_engine`, and
  preprocessing happens at or after that point (`portfolio/mod.rs:140` calls
  `ts.preproc(..)` ahead of the factory). So the ≥100× target does not depend
  on storing this.
- **seed hit: potentially 1003 s.** Preprocessing depends only on $T$, and
  `region_key` *is* the digest of $T$ + constraints. So a `region_key` match
  means the stored preprocessing result is valid. Discarding it means paying up
  to 1003 s again on every seed hit, which can cancel the benefit seeding is
  supposed to deliver.

`Transys` derives `Serialize`/`Deserialize` (`src/transys/mod.rs:123`), so this
is mechanically available.

**Policy: store only when it pays.** Write the preprocessed `Transys` when
preprocessing took longer than a threshold *and* the serialised size is under a
cap; otherwise store clauses alone. Both numbers must be set from measurement,
and the deciding comparison is **deserialisation time vs. preprocessing time**
— storing is pointless if loading is not decisively cheaper.

### 6.3 Encoding: measured, and it is not RON

`bench/serde-size` measures the two candidate encodings on structurally
equivalent data (clause lists are `Vec<Vec<u32>>` in shape; a `LitVec` is a
vector of `u32`-backed `Lit`). Scales taken from the baseline run.

|shape|RON|bincode|size ratio|**deserialise ratio**|
|---|---|---|---|---|
|invariant clauses, fifo scale (5607 × 12)|0.51 MB, de 8.7 ms|0.32 MB, de 0.4 ms|1.6×|**22.6×**|
|preproc clauses (250241 × 3)|6.01 MB, de 110.1 ms|3.79 MB, de 11.8 ms|1.6×|9.3×|
|side table (23592 latches)|2.78 MB, de 60.6 ms|0.79 MB, de 0.8 ms|3.5×|**78.0×**|

Extrapolated to `a07-p14` (2,502,409 clauses, the largest observed):
**RON 60.05 MB / 1100 ms** versus **bincode 37.95 MB / 118 ms**.

Two corrections to earlier guesses in this document:

1. **Size is not the problem.** RON costs only 1.6× on clause lists, not the
   3–10× a text format suggests — large variable numbers cost about as many
   characters as bytes. The side table is the exception at 3.5×, because
   `[u8; 32]` becomes a parenthesised list of 32 decimals.
2. **Time is the problem.** Deserialisation is 9–78× slower, worst on exactly
   the structure atom identity needs. That is the figure that matters, since
   the cold path pays it on every hit.

**Decision: binary encoding (bincode or postcard), not RON.** Concretely this
means *not* reusing `RicProj::save_serde_obj` / `load_serde_obj`
(`src/cli/rproj.rs:74`), which hard-code `ron::to_string`. The storage
directory layout and hashing discipline there are still worth borrowing; the
encoding is not.

### 6.4 The threshold, now decidable

With binary encoding, loading the largest preprocessed clause database costs
**≈118 ms** against preprocessing that was measured at **1003 s** — a ratio of
about **0.012 %**. Storage is therefore overwhelmingly worth it whenever
preprocessing was non-trivial at all.

|parameter|value|basis|
|---|---|---|
|store `preproc` when preprocessing exceeded|**10 s**|two orders of magnitude above the 118 ms load, so the decision is never marginal|
|size cap|**256 MB** serialised|38 MB at the largest observed instance; the cap only guards pathological inputs such as `Problem17.aig` (2.29 GB source)|
|write cost on a miss|34 ms at that scale|against per-case budgets of seconds to 3600 s, comfortably inside the 1 % cold-path allowance|

Still unmeasured:

1. the distribution of preprocessing time across all 840 cases — the current
   sample is a handful of solved ones, so the 10 s threshold's *hit rate* is
   unknown even though its correctness is not;
2. real `Transys` serialisation, as opposed to structurally equivalent data.
   The encoding ratio will hold; absolute constants may differ if `LitVec` or
   `DagCnf` define custom Serde impls.

---

## 7. Untrusted input, under `panic = "abort"`

AGENTS.md §1.5: a corrupted entry must fall through to a full run, "never a
verdict and never a panic". The release profile sets `panic = "abort"`, so
`catch_unwind` is **not available** — this was verified indirectly today when a
rejected CLI flag combination surfaced as SIGABRT.

Therefore deserialisation must be *total*:

- every read returns `Result`; no `unwrap`, `expect`, indexing or slicing on
  cache-derived data
- length fields validated against remaining input before allocation
- a failed parse is a miss, logged once, never an error to the caller
- fuzz the deserialiser as a unit test; a panic there is a release blocker

---

## 8. Monotonicity licences (AGENTS.md §1.4(d))

To be encoded as documented preconditions with tests:

|change|licence|
|---|---|
|$T' = T \wedge C$ (constraints added)|$T' \Rightarrow T$, so a valid $\mathit{Inv}$ still satisfies $\mathit{Inv} \wedge T' \Rightarrow \mathit{Inv}'$; if also $I' \Rightarrow I$, the initiation condition survives|
|$P \to P \wedge Q$ (property strengthened)|$\mathit{Inv} \Rightarrow P \wedge Q$ breaks, but $\mathit{Inv}$ remains a sound over-approximation ⇒ valid **seed** material|
|$T$ changed|nothing guaranteed; just run the three checks, they are cheap|

The first row is what makes the mixed-signal contract loop (§7.1 of the interop
document) and the CIll helper-assertion loop cheap: each refinement adds
constraints, so the previous invariant is reusable.

---

## 9. Acceptance and measurement

Mapped to AGENTS.md §3:

|#|criterion|how|
|---|---|---|
|1|zero verdict changes, cold and warm, over 840|`verdicts.json` diff; a single change blocks merge|
|2|warm identical re-run ≥100× faster|verdict licence hit constructs no engine|
|3|same $T$, changed property: measurable gain + **seeded-clause survival rate**|instrument candidate acceptance; a negative result is published (§3.3c)|
|4|cold-path overhead < 1 %|digest-on-miss timing against `docs/BASELINE.md`|

Additional measurement this design implies:

- **Do IC3 workers currently receive any shared lemma?** §4.2 suggests no.
  Instrument before stage 4 assumes otherwise.
- **Structural-hash cost** on the largest 840 instances, against the 1 % budget.

---

## 10. Scope limit for the first cut

Per the capability survey (§6.4.1 of the interop document), `proof()` is
state-dependent: `Kind` panics under `--simple-path` (which `portfolio.toml`
actually ships) and `Portfolio` panics unless its certificate is already UNSAT.
With no capability query and no `catch_unwind`, a wrong guess aborts the run.

**First cut supports `IC3` only.** Widen per engine only after enumerating that
engine's refusal conditions the way those two were.

---

## 11. Decisions

### 11.1 Settled

1. **Fork patch *and* upstream PR** for IC3 lemma injection (§4.3). Patch is
   `879ee74` behind the `lemma-inject` feature; PR body is
   `docs/upstream-pr-ic3-lemma-inject.md`, held until it has measurements.
2. **Clause storage form — both, at different layers** (§6.1). Identity is
   boundary currency, `LitVec` is the working form: identity tuples on the wire
   and for near-miss translation, side table + `LitVec` on disk, `LitVec` in
   memory because `extract_lemma`/`add_lemma` require it. The question was
   mis-framed as a single choice; the consumer had already fixed one layer.
3. **Preprocessed artifacts — store conditionally** (§6.2). Reversed by
   measurement: preprocessing reaches 1003 s on the largest instances, and a
   `region_key` match makes the stored result valid, so discarding it would
   charge every seed hit up to 1003 s. Verdict hits are unaffected either way.

### 11.2 Settled by measurement since

4. **Encoding is binary, not RON** (§6.3). Measured 9–78× slower
   deserialisation for RON, worst (78×) on the side table — exactly the
   structure atom identity needs. Size was only 1.6×, so the earlier
   size-driven reasoning was wrong; time is the cost. Consequence:
   `RicProj::save_serde_obj` is **not** reusable, since it hard-codes
   `ron::to_string`.
5. **`preproc` threshold: 10 s of preprocessing; cap 256 MB serialised**
   (§6.4). Loading the largest observed clause database costs ≈118 ms against
   1003 s of preprocessing — 0.012 %, so the decision is never marginal.

### 11.3 Still open, and what would settle them

|question|settled by|
|---|---|
|hit rate of the 10 s threshold|distribution of preprocessing time over all 840 — the baseline run itself. Correctness of the threshold is settled; how often it fires is not|
|absolute constants for real `Transys`|serialising an actual `Transys` rather than structurally equivalent data. The encoding *ratio* will hold; constants may shift if `LitVec`/`DagCnf` define custom Serde impls|
|whether `mmap` is worth it|only if cold-path deserialisation shows against the 1 % budget. At 118 ms for the largest instance it currently does not; first cut uses plain deserialisation|
|identity hash width (32 B vs truncated)|collision rate over the corpus. §5.4 already requires a collision flag, so a shorter hash trades size for fall-through frequency — and §6.3 shows the side table is the most encoding-sensitive structure, so this is where width matters most|
