# Toolchain interop patterns — and why an IR is the wrong unit

**Status:** DRAFT. Blocked on the §1.3 profile (the 840-case baseline run is in
flight; see `docs/BASELINE.md`). Do not treat the conclusions as settled.
**Created:** 2026-08-26. **Stage:** 1 (infrastructure), spanning 2–4.
**Style:** per AGENTS.md §6, mirroring
`portable-algebraic-aotjit/docs/algebraic-aotjit-codegen-rejected.md`.

---

## 0. What was proposed and what is concluded

**Proposed:** introduce a language specialised for hardware verification —
initially described as an "IR" — motivated by six separate concerns:

1. vocabulary for ranking functions / liveness
2. word-level information lost in bit-blasting
3. exchange format for lemmas between engines
4. targeting reprogrammable devices beyond FPGA (CPLD, CGRA, FPAA)
5. splitting rIC3 into a front-end (a drop-in replacement for upstream) and a
   middleware that drives back-ends
6. handling jitter/noise **polymorphically**, in a language whose proper subset
   corresponds 1:1 to the existing semantics

### 0.1 Terminology: this is a specification language, not an IR

The naming matters more than it looks, because it decides how much risk the
proposal carries:

|  |IR|specification language|
|---|---|---|
|position|internal representation, tool-to-tool exchange|user-facing input surface|
|relation to verdict path|**in the middle of it**|an *addition* beside existing inputs|
|release-gate exposure|maximal — every model flows through it|none, while existing AIGER/BTOR2 paths are untouched|

Concerns 1, 4 and 6 are about **what a user can express**, so they are
specification-language questions. Concerns 2, 3 and 5 are about internal
representation and tool boundaries, and none of them wants a new format. The
original framing conflated the two; the rest of this document keeps them apart.

### 0.2 Conclusion

**Concluded (provisionally):** no new *IR* is warranted. Concerns 2 and 3 are
already served by existing mappings and are identifier problems in disguise;
4 is barred by the decision procedure rather than by representation; 5 is worth
doing on its own merits and needs no new format.

Concerns 1 and 6 are **specification-language** questions and are not rejected.
Concern 6 in particular carries its own gate — see §1.3(6) — and is the only
part of the original proposal with a mechanically checkable soundness
criterion. It is therefore the one to prototype first, behind that gate.

What must be written down regardless is the **discipline for wiring tools
together**, because that recurs in every later stage while a language would be
built once. That discipline is §2–§7.

---

## 1. Why not an IR

### 1.1 rIC3 already has three IR layers

|layer|location|size|role|
|---|---|---|---|
|word-level terms|`logicrs::fol` (`Term`, `TermType`, `OpTerm`, `Sort`, `term_mgr`)|6,664 lines|BTOR2-class IR with hash-consing|
|DAG / CNF|`logicrs::{DagCnf, cstdagcnf}`|part of 10,797|AIG and CNF encoding, BVA|
|transition system|`TransysIf` trait; `Transys`, `TransysCtx`, `NoDepTransys`, `WlTransys`|3,737 lines|verification abstraction; `bitblast.rs` lowers|

rIC3 proper is 17,219 lines and `logicrs` alone is 10,797. A fourth layer needs
to justify itself against a base that is already more than half IR.

### 1.2 Interop argues *against* a new IR

The currency of interoperability is not our IR but **the formats the ecosystem
already reads**. AIGER and BTOR2 are read by ABC, yosys, btor2tools, AVR, Pono,
nuXmv, certifaiger and cerbtora. A new IR is read by us alone.

This is not abstract. Two things were possible today *because* AIGER is
standard: reusing the 840-case corpus unchanged, and diffing our results
against the CAV'25 artifact's per-instance file. A bespoke IR forfeits both.

### 1.3 The six concerns, resolved individually

**(1) Ranking functions.** `fol::Sort` carries only `Bv(usize)` and
`Array(i,e)`, but order lives in the operator layer, which already has `Ult`,
`Ugt`, `Slt`, `Sgt`, `Add`, `Sub`, `Mul`, `Ite`, `Eq`, `Neq`. So "this measure
decreases" is expressible today.

More importantly, **`Bv(n)` is a finite domain, so well-foundedness is free.**
The hard part of ranking functions in infinite-state systems is a descent with
no floor; a bit-vector descent must terminate. A user-supplied rank therefore
reduces to a *safety* property ("it decreases"), which existing IC3 checks.
This lowers the stage-3 burden substantially.

Two genuine gaps remain, and both are **annotation surface syntax**, not `fol`:

- **No quantifiers.** SystemVerilog has none; CIll emulates with a symbolic
  index and avoids `generate` because it replicates the assertion $W$ times.
  A rank over an array or memory ("entries still pending") is awkward.
- **No lexicographic ranks.** A tuple of measures with its ordering cannot be
  expressed by the `h_`-prefix naming convention.

**(2) Word-level information in bit-blasting.** Already preserved:

```rust
fn bitblast(&self) -> (Self, GHashMap<Term, TermVec>, GHashMap<Term, (Term, usize)>)
```

Both directions are returned — word→bits and bit→(term, bit position). On top
of that, `WlTsSymbol` and `link_wts_by_symbol()` connect by declared symbol
name, which is exactly what AGENTS.md §1.4(a) asks for ("prefer declared symbol
names where they exist"). This is **an unconsumed mapping, not a loss**; the
work is code that reads it.

**(3) Lemma exchange.** Already implemented — `portfolio/lemma_mgr.rs`
broadcasts all-to-all over `ipc_channel`. But the format is:

```rust
pub type LemmaIpcTx = IpcSender<(Option<usize>, LitVec)>;
```

`LitVec` is variable-number-based, i.e. **positional identity** — the thing
AGENTS.md §1.4(a) forbids, and the same class as the
`portable-algebraic-aotjit` §3.5.J drift (recorder wrote a content hash, replay
indexed a pool position). So this is an *identifier* problem, not a format
problem, and it is **the same problem stage 1's cache already has to solve**.
Build content-derived atom identity once and both are served.

Soundness is probably not at risk: IC3 admits foreign lemmas as candidates and
keeps only those surviving relative induction, which is the §1.4(c) seeding
argument. The likely cost is wasted exchange, i.e. a ceiling on sharing
efficiency — worth measuring in stage 4 as "survival rate of received lemmas".

**(4) Devices beyond FPGA.**

|device|theoretical barrier|what it actually needs|
|---|---|---|
|CPLD|none|nothing; product-term macrocells are a synthesis/P&R concern, and RTL→netlist→AIGER is the same synchronous finite-state system|
|CGRA|none|word-level reasoning (`wltransys`, `wl-*`, `cegar` exist) plus **quantification** over parameterised PE arrays — the same gap as (1). Configuration correctness is translation validation, reducible to a miter, which a safety checker handles|
|FPAA|**yes**|a real-arithmetic decision procedure|

The FPAA case is the load-bearing one, and it is worth stating the principle
plainly:

> **Changing the IR does not change what the decision procedure can decide.**
> The IR fixes what is expressible; capability is fixed by the solver.

Concretely: add `Sort::Real` and `bitblast()` cannot lower it, so no `DagCnf`
is produced, so the SAT back-end receives nothing. The whole pipeline is
vacuous. Continuous/hybrid verification is a different tool family (reachable
set methods — Flow\*, CORA, SpaceEx; dReach; barrier certificates; Lyapunov
functions). The IC3-flavoured precedent exists — IC3 Modulo Theories / HyComp,
by the nuXmv authors — but it requires an LRA/NRA SMT back-end, so it is a
**back-end replacement, not an IR addition**.

Note also that the free lunch of §1.3(1) disappears here: in a continuous
domain a descent need not terminate (1/2, 1/4, 1/8, …), so a floor and a
convergence rate must be proven separately. The reason stage 3 looks cheap *is*
finiteness.

**(5) The front-end/middleware split.** See §6 — worth doing, no IR needed.

**(6) Jitter/noise, polymorphically, over a 1:1 proper subset.** This is the
strongest form of the proposal and the only one that is *not* rejected here.

The requirement "a proper subset corresponds 1:1 to the existing semantics" is
a **conservative extension**: on sentences of the original language, the
extended system proves exactly what the original proves. The hardware domain
already has precedent for the shape — Verilog-AMS ⊃ Verilog, VHDL-AMS ⊃ VHDL.

What makes it different from concerns 1–5 is that **it brings its own gate**.
`btor-rs` exposes `Deparser::deparse(&Btor) -> String` plus a `Display` impl,
so a round trip is already constructible:

    BTOR2 --parse--> (subset representation) --deparse--> BTOR2'

and the 1:1 claim becomes a measurable property: run the 840-case suite through
the round trip and require **verdict invariance**, which `verdicts.json` and
`bench/compare.py` already diff. Stronger still, check equivalence of original
against round-tripped model with a miter. Wherever the round trip is not the
identity is exactly where the subset boundary lies.

So the verification frame can be built *before* the language is designed. This
neutralises the objection that sank concerns 1–5 as IR candidates, namely that
a new representation is the hardest possible thing to push through a
"zero verdict changes" gate.

**"Polymorphic" splits three ways, and only two are cheap:**

|reading|example|needs|verdict|
|---|---|---|---|
|**nondeterminism**|"within ±10 ps, any value"|`input()` + `constraint()`|**already inside the subset** — expressible today|
|**precision**|one model at 8/16/32-bit fixed point|parameterise $n$ in `Sort::Bv(n)`|**most valuable**; composes with `cegar`|
|**probability**|"Gaussian distributed"|probabilistic model checking (PRISM, Storm)|different decision procedure → a §7 contract edge|

The first row deserves emphasis: **abstracting jitter as interval
nondeterminism puts it inside what rIC3 already decides.** This is how CDC
verification models metastability. The gain is then *notation*, not capability
— a legitimate goal for a language, but it must not be sold as new power.

The second row is where real leverage sits. Real Number Modeling quantised to
fixed point (§7.4) needs exactly precision polymorphism, and "start coarse,
refine when the counterexample is spurious" is CEGAR — for which `cegar`
already exists.

**Mandatory soundness discipline.** The extension must compile to an
**over-approximation only**. Widening jitter to nondeterminism keeps safety
proofs sound and makes only counterexamples possibly spurious, which CEGAR
handles. An under-approximation would make a *safety proof itself* unsound.
This is the same class of prohibition as CIll's ban on `assume`, and if the
type system can enforce the direction, it should.

**Cost, stated honestly.** The cost of a language is not its grammar but its
diagnostics and documentation — error messages, type checking, version
compatibility, user education. CIll needs a 7.5 KB skill document for a
convention as small as an `h_` prefix.

Sequencing that follows from the above: build the **round-trip gate first**
(cheap, language-agnostic, useful on its own as a BTOR2 fidelity check), then
prototype with precision polymorphism alone, and add quantification,
lexicographic order and contracts only when a consumer demands them.

#### 1.3.6.1 The round-trip gate was built. It changed the plan.

`bench/roundtrip` implements it (depends only on `btor`, so no C/C++ solver
toolchain and cheap enough to compile beside a running measurement). Result
over HWMCC'19's word-level bit-vector track:

|outcome|count|share|
|---|---|---|
|instances|317|—|
|raw text identical after one round trip|**10**|3 %|
|raw differed, meaning preserved (commutative operand order)|**201**|63 %|
|**differed after normalisation**|**106**|33 %|

Every one of the 106 has an *identical* line count and an *identical* keyword
histogram. So nothing is lost or invented — the representation is simply not
canonical. Two distinct causes were isolated:

1. **Commutative operand order.** `counter.btor` differed only in lines like
   `16 eq 1 8 15` vs `16 eq 1 15 8`. Normalising commutative operands fixes
   these 201 cases. The order tracks internal term interning order, which
   depends on how many terms the process has already seen — stable within a
   run (`giputils` pins `RandomState::with_seeds(0,0,0,0)`, verified: three
   runs produced byte-identical output) but not across parse positions.
2. **Node renumbering.** `fifo.btor` swapped two *independent* declarations:

       149 concat 148 48 79      149 const  148 10
       150 const  148 10    ->   150 concat 148 48 79

   Line-local normalisation cannot repair this, because the node ids change and
   every reference to them changes with it.

**Consequence for the proposal.** The 1:1 claim **cannot be established
textually.** Two routes remain:

- **Verdict invariance** over the 840-case suite (plan item B5). This is the
  operative gate, and the harness already computes it.
- **A structural canonical form** — topological order plus content-derived
  numbering — which would make textual comparison meaningful.

The second route is *the same work* as the cache's content-derived atom
identity (`docs/cache-design.md` §5.3, which independently specified "canonical
child order for commutative nodes, sort by child hash"). That specification was
written before this measurement and is now confirmed as necessary rather than
precautionary.

So the specification language and the cache need one shared piece of
infrastructure. That is the third time in this document that two apparently
unrelated concerns reduced to a single identity problem — the first two being
lemma exchange (§1.3(3)) and corpus case naming (§5).

**Status of this item:** the gate exists and is a useful diagnostic; it is
*not* a pass/fail oracle for 1:1. Do not treat the 106 as failures of BTOR2
fidelity — treat them as the measured size of the canonicalisation gap.

### 1.4 What would reopen this

- The §1.3 profile shows representation/front-end work dominating rather than
  invariant derivation (AGENTS.md §1.3 explicitly warns against inheriting the
  `portable-algebraic-aotjit` host's ~75% front-end finding — measure it).
- A stage-3 design shows quantification and lexicographic order cannot be
  carried by an annotation surface language over `fol`.
- `Engine`/`TransysIf` prove too unstable across upstream rebases to wrap.

---

## 2. Pattern: the layer is chosen by exchange frequency, not by format

rIC3 already spans five interop layers. They are not interchangeable; the
selector is how often the boundary is crossed.

|layer|used for|language reach|suited frequency|isolation|
|---|---|---|---|---|
|FFI (C ABI)|cadical, kissat, bitwuzla, aiger (all `build.rs` compile+link)|C-ABI languages only|microseconds|**none** — dies together|
|process + IPC|`portfolio` (`ipc-channel`, `fork`)|Rust-leaning (bincode)|milliseconds|process|
|subprocess + file|`Command::new("yosys")`|any|seconds|process|
|container + file|`certifaiger_check`, `cerbtora_check` via `Command::new("docker")`, volume mounts, exit code|**any**|seconds|container **and licence**|
|MCP (JSON-RPC)|`ric3_trace_tools` in `src/cli/trace.rs` (`rmcp`)|**any**|ms–seconds|process|

**Measured consequence.** One fifo case produced `num_mic: 17882` in 9.65 s —
about 1,850 lemmas/second. All-to-all across 16 workers is tens of thousands of
messages per second. JSON-RPC and containers are impossible at that rate and
bincode IPC is already a burden.

AGENTS.md §5 rejected the `replay` layer on exactly this reasoning
(microsecond-scale, hundreds of thousands to millions of events → pure
overhead). The same conclusion applies to cross-language lemma sharing:
**reduce the frequency first** (filtering, batching, quality thresholds); what
survives is low-frequency and the existing layers already carry it.

Rule of thumb, by boundary:

|boundary|frequency|prescribed layer|
|---|---|---|
|contract exchange with another domain tool|once per refinement|BTOR2 `constraint` file, or MCP|
|preprocessing / synthesis delegation|once per run|subprocess + file (already done)|
|certificate validation|once per run|container + file (already done)|
|lemma sharing|10³–10⁴/s|**redesign the frequency**, then FFI or shared-memory IPC|

---

## 3. Pattern: pin everything, by content

An interop edge is reproducible only if the thing on the far side is pinned.
Current state is inconsistent:

|artifact|pinned by|status|
|---|---|---|
|upstream rIC3|commit SHA `7149d56…`|correct (`docs/UPSTREAM.md`)|
|submodules|commit SHAs|correct|
|HWMCC'24 archives|Zenodo MD5, verified|correct|
|JKU archives|our recorded SHA-256|correct|
|`ghcr.io/gipsyh/certifaiger`|image **tag**|**gap**|
|`ghcr.io/gipsyh/cerbtora:latest`|`:latest` **tag**|**gap**|

`--pull=never` is a good instinct — it forbids silent network drift — but a tag
does not fix content. Certificate validation is slated to become the reason we
trust a *cached* invariant, so this edge must be pinned by digest
(`@sha256:…`). Cheap to fix; deferred until the baseline run completes because
it changes behaviour.

**Rule:** every external tool edge is pinned by content hash, and the hash is
recorded in the doc that quotes results depending on it.

---

## 4. Pattern: isolation is a trust and licence boundary, not just a crash boundary

Three forces push the same way:

1. **`panic = "abort"`.** The release profile aborts on panic, so an
   in-process tool's failed assertion kills the whole verification. Measured
   today: a rejected flag combination (`cannot enable both dynamic and
   drop-po`) surfaced as SIGABRT, indistinguishable by signal from an
   allocation failure. C/C++ verification tools use internal asserts freely.
2. **AGENTS.md §1.5** requires a corrupted cache entry to fall through to a
   full run, "never a verdict and never a panic". With `panic = "abort"`,
   `catch_unwind` is not available, so deserialisation must be total —
   `Result`-returning, with no panicking indexing or `unwrap` on cache data.
3. **Licence.** rIC3 is **GPL-3.0**. Linking propagates it, so an
   academic non-commercial or proprietary tool cannot be *linked* and
   redistributed. Process and container separation creates a separate-program
   boundary. That certifaiger and cerbtora are containers is right on this axis
   too. (`portable-algebraic-aotjit` is BSD-2-Clause-Patent, so absorption into
   GPL is fine; the reverse is not.)

**Rule:** a tool we do not control, or whose licence is not GPL-compatible,
runs behind a process or container boundary — not linked.

---

## 5. Pattern: identity must be content-derived at every layer

AGENTS.md §1.4(a) states this for cache atoms. It generalises, and the failures
are cheap to reproduce. Three instances were hit in one day:

|site|positional/naive key|failure|fix|
|---|---|---|---|
|case identifier|strip "text after last dot", applied twice|`93.c.aig` → `93.c` → `93`, 29 cases silently unmatched|strip only a known model extension; **idempotent**|
|corpus paths|basename alone|HWMCC'24 ships two different models as `.../safe/x.aig` and `.../unsafe/x.aig`; 840 became 838|suffix aliases scoped to collisions *within one archive*|
|path normalisation|`Path.resolve()`|resolved the alias back onto its target, alias vanished, 840 became 838 again|`abs_no_symlink()`|
|lemma exchange|`LitVec` variable numbers|(open) different preprocessing per worker ⇒ different numbering|content-derived atom identity — same work as the cache|

Note the shape: each failure was **silent and produced a plausible number**.
That is the §3.5.J signature.

**Rule:** identity is defined in exactly one function; every consumer imports
it. Here that is `harness.instance_stem()`, imported by `compare.py` and
`prepare_corpus.py`.

---

## 6. Pattern: wrap, do not fork — the front-end/middleware split

The proposal is to split rIC3 into a front-end (drop-in replacement for
upstream) and a middleware that drives back-ends. The structure already exists
as trait boundaries:

|layer|current location|contract|
|---|---|---|
|front-end|`src/cli/`, `src/frontend/{aig,btor}`|AIGER/BTOR2 files|
|middleware|`src/portfolio/`, `src/polynexus/`, `src/mp/`|`create_bl_engine(cfg, ts, sym) -> Box<dyn BlEngine>`|
|back-end|`src/ic3/`, `bmc.rs`, `kind.rs`, `rlive/`, `cegar/`, `wl*`|`Engine` / `BlEngine` / `WlEngine`|
|solver|`src/gipsat/`, cadical/kissat/bitwuzla|FFI|

`Engine` already carries nearly everything a middleware needs:

```rust
fn check(&mut self) -> McResult;                    // run + verdict
fn proof(&mut self) -> BlProof;                     // the invariant to cache
fn cex(&mut self) -> BlCex;
fn certificate(&mut self, res) -> McBlCertificate;
fn set_extractor(&mut self, Box<dyn ExtractorIf>);  // lemma-exchange hook
fn add_tracer(&mut self, Box<dyn TracerIf>);
fn get_ctrl(&self) -> Arc<dyn TerminateCtrl>;
```

### 6.1 The real payoff is rebase cost

`src/lib.rs` exports everything (`pub mod ic3`, `pub trait Engine`,
`pub fn create_bl_engine`), so **rIC3 can be consumed as a library**.

AGENTS.md §1.1 requires `main` to track a *rebase* onto upstream, and every
upstream rebase to carry a full benchmark re-run — which this baseline shows is
roughly ten days of machine time. The more upstream source we edit, the more
often we pay that. If our code lives beside upstream and depends on it, **the
edit count is zero and rebasing degenerates to updating a pin.**

§1.1 already mandates this discipline for another dependency: *"Do not vendor
`portable-algebraic-aotjit`. Depend on it by git ref, pinned."* Applying the
same rule to rIC3 itself is consistent, not novel.

### 6.2 The gate already exists

Architecture changes are usually hard to validate. Here "zero verdict changes
across 840 cases" *is* the drop-in compatibility test, and `bench/compare.py`
diffs per instance. This is an unusually favourable condition and argues for
doing the split while that gate is fresh.

### 6.3 Boundary formats already exist — no IR needed

|boundary|existing format|evidence|
|---|---|---|
|front-end → middleware|AIGER / BTOR2|industry standard|
|middleware → Rust back-end|`Transys`/`WlTransys` + `EngineConfig`|`Transys` derives `Serialize, Deserialize`|
|middleware → foreign back-end|**BTOR2 round-trip**|`btor-rs/deparse.rs` can *write* BTOR2|

### 6.4 Obstacle: no capability query

Default trait methods panic:

```rust
fn proof(&mut self) -> BlProof { panic!("unsupport proof"); }
```

Not every engine produces an invariant, there is no `supports_proof() -> bool`,
and `panic = "abort"` means it cannot be probed speculatively either. A
middleware must therefore keep a capability whitelist, which silently rots when
upstream adds an engine. The better fix is a capability query upstream, which
is a decision outside this fork.

Related: `create_bl_engine` is not exhaustive (`_ => unreachable!()`);
`Portfolio` and `PolyNexus` take separate paths and need special-casing.

#### 6.4.1 Measured capability table

Surveyed on the pinned tree. This is the whitelist a cache wrapper needs.

|engine|trait|`proof()`|site|
|---|---|---|---|
|`IC3`|`BlEngine`|yes|`ic3/mod.rs:404`|
|`Kind`|`BlEngine`|**conditional**|`kind.rs:219`|
|`MultiProp`|`BlEngine`|yes|`mp/mod.rs:138`|
|`Portfolio`|`BlEngine`|**conditional**|`portfolio/mod.rs:409`|
|`PolyNexus`|—|yes|`polynexus/mod.rs`|
|`CIllKind`|`BlEngine`|yes|`cli/cill/kind.rs:105`|
|`WlKind`|`WlEngine`|yes|`wlkind.rs:136`|
|`Cegar`|`WlEngine`|yes|`cegar/mod.rs:93`|
|`BMC`|`BlEngine`|**no** — inherits `panic!`|`bmc.rs`|
|`Rlive`|`BlEngine`|**no** — inherits `panic!`|`rlive/mod.rs`|
|`WlBMC`|`WlEngine`|**no** — inherits `panic!`|`wlbmc.rs`|

**The important finding: capability is not static, it is state-dependent.**
Two engines advertise `proof()` and then refuse at call time:

- `Kind::proof()` panics outright when `cfg.simple_path` is set — the body is
  `error!("k-induction with simple path constraint not support certifaiger");
  panic!();`. Note that `portfolio.toml` ships
  `kind = "kind --step 1 --simple-path"`, so the *configured* worker is
  precisely the failing case.
- `Portfolio::proof()` panics with `"no proof available"` unless its stored
  certificate is already `UNSAT`.

So a whitelist keyed on engine type alone is insufficient; the key must be
**(engine, configuration)**, and with `panic = "abort"` there is no recovery
from getting it wrong. The narrowest safe start for the cache is therefore
**`IC3` only**, widening per engine only once its refusal conditions have been
enumerated the way these two were.

### 6.5 Sequencing

Build the **cache as a wrapper around `create_bl_engine`** and let the
middleware grow out of it, rather than designing a middleware first:

- **verdict licence** — consult the digest *before* the factory; on a hit
  return `McResult` without constructing an engine
- **seed licence** — inject stored lemmas into `ts`, then call the factory
- **store** — `proof() -> BlProof` is the artifact

Only the boundaries an actual consumer demands get built.

---

## 7. Pattern: cross-domain edges exchange contracts, not representations

For a mixed-signal system — an FPAA tool alongside rIC3 — a shared IR would
have to express both continuous dynamics and discrete transitions, and each
tool would understand only its half: representation shared, reasoning not.
A contract is agreed **only at the boundary** and each side keeps its own
language.

The rIC3 side of that boundary already exists in both directions:

|direction|channel|evidence|
|---|---|---|
|receive assumptions|BTOR2 `constraint` node|`btor-rs` `parse.rs`/`deparse.rs` both ways; `ywb.rs` maps 1:1 to Yosys `assume`|
|emit guarantees|proven invariant + `--cert`/`--certify`|certifaiger / cerbtora|
|project onto boundary signals|`filter_map_var(Fn(Var) -> Option<Var>)`|`transys/certify.rs`|

Measured: `fifo.btor` carries three `constraint` nodes in live use.

### 7.1 Contract refinement is exactly the §1.4(d) licence

Contract-based verification is iterative: the analog side narrows an
assumption, which *adds a constraint* on the digital side. AGENTS.md §1.4(d):
with $T' = T \wedge C$ and $T' \Rightarrow T$, a previously valid $\mathit{Inv}$
still satisfies $\mathit{Inv} \wedge T' \Rightarrow \mathit{Inv}'$.

So **every refinement round can reuse the previous invariant** — stage 1's
cache directly accelerates a mixed-signal contract loop, the same shape as the
"human adds one helper assertion per iteration" workflow in §0.

### 7.2 Worked boundary: PLL radio

|direction|signals|contract|checked by|
|---|---|---|---|
|D→A|frequency control word, divide ratio, charge-pump code, band select|"FCW always within $[a,b]$", "divide ratio in the valid set"|**rIC3** (safety)|
|D→A|sigma-delta accumulator|"no overflow"|**rIC3** (word-level: `wl-*`, `cegar`)|
|D→A|calibration FSM|"terminates"|**rIC3** (liveness → stage 3)|
|A→D|`lock_detect`, divided clock|"FCW in $[a,b]$ ⇒ lock time < $T_{lock}$"|FPAA tool|
|A→D|VCO frequency|"FCW in $[a,b]$ ⇒ $f_{VCO} \in [f_1,f_2]$"|FPAA tool|

Two hard parts, both landing on this project's existing roadmap:

- **Circular dependency.** The digital side assumes lock while the analog side
  assumes FCW range. Closing that soundly needs a **well-founded argument over
  time** (Abadi–Lamport-style composition) — the stage-3 tool.
- **Time-scale mismatch.** Analog settling is µs–ms; the digital clock is ns.
  Unrolling lock acquisition cycle-by-cycle explodes. AGENTS.md §1.3 names
  "deep counter or timer (the k-induction killer; the stage-2 target)" as
  profile shape #2 — a PLL divider and lock timeout counter are precisely that.
  Mixed-signal interop independently strengthens the stage-2 motivation.

### 7.3 What a contract must not promise

Phase noise, jitter and spurs are spectral/statistical properties and are not
formal-verification targets. Contracts carry only assertable properties —
ranges, timing bounds. Blurring this puts unverifiable promises in a contract.

### 7.4 A closer practical path

Industry practice is Real Number Modeling: the analog block becomes a
discrete-time SystemVerilog `real` model. `real` obstructs formal verification,
but **quantised to fixed point it lands inside `Sort::Bv(n)`** — then rIC3
checks the boundary contract directly, with no separate FPAA tool. The
refinement loop has existing assets: `--abs-cst`, `--abs-trans`, and `cegar`'s
multiplier UF abstraction.

---

## 8. Open items

|item|action|blocked on|
|---|---|---|
|container pins|`@sha256:` digests for certifaiger/cerbtora|baseline run (behaviour change)|
|lemma identity|content-derived atom identity; shared with the cache|stage 1 cache design|
|lemma survival rate|instrument received-lemma acceptance in `portfolio`|stage 4|
|capability query|whitelist now; consider an upstream PR for `supports_*`|—|
|annotation surface language|quantification + lexicographic order over `fol`|stage 3 design|
|`fol` sufficiency for ranks|confirm operator layer suffices|stage 3 design|
|**round-trip gate**|BTOR2 → parse → deparse → BTOR2, verdict invariance over 840|**buildable now**; language-agnostic|
|precision polymorphism|parameterise `Sort::Bv(n)` in a spec-language prototype|round-trip gate|
|over-approximation only|enforce widening direction in the extension|spec-language design|

---

## 9. Reopening conditions

This document is a draft. It becomes a decision record when the §1.3 profile
lands and either confirms or refutes §1.4.

Reopen the **IR** question if the profile shows representation cost dominating,
or if `Engine`/`TransysIf` churn makes wrapping upstream more expensive than
editing it.

The **specification-language** question (§1.3(6)) is not closed and does not
wait on the profile in the same way: its gate is the round trip, which can be
built independently. Close it negatively only if the round trip cannot be made
the identity on the 840-case suite — that would mean the intended subset is not
actually 1:1, which is the whole premise.
