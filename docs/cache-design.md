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

## 6. Storage

Content-addressed by `region_key` (32 bytes). One entry holds:

|field|purpose|
|---|---|
|`format_version`|**invalidate, never reinterpret**|
|`upstream_commit`|pin at write time; mismatch ⇒ ignore|
|`identity_scheme`|`symbol` or `structural`, plus depth bound|
|`region_key`|digest of $T$ + constraints|
|`whole_digest`|digest including $\neg P$ — the verdict licence key|
|`verdict`|`McResult`|
|`clauses`|`Vec<LitVec>` in atom-identity terms, not variable numbers|
|`collision`|flag from §5.4|

Preprocessed artifacts (swept AIG, CNF, COI, GipSAT base clause DB) are listed
by AGENTS.md §1.5 as candidates. **Deferred**: their serialisation cost is
unmeasured, and the cold-path budget is 1 %. Add them only with a measurement.

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

## 11. Open decisions

1. **Fork patch vs upstream PR** for IC3 lemma injection (§4.3).
2. **Clause storage form** — atom-identity tuples vs a serialised `LitVec` plus
   a variable-to-identity side table. The former is self-describing, the latter
   is smaller; decide against the 1 % budget.
3. **Whether to store preprocessed artifacts** at all (§6).
