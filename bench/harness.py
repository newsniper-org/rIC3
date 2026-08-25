#!/usr/bin/env python3
"""
rIC3 fork — Stage 1 benchmark harness (AGENTS.md work item 1.2).

Scoring is solved-count and PAR-2 only. Wall clock on a hand-picked instance is
not a result and this harness deliberately makes it awkward to report one: the
per-instance JSONL is a record, the summary is the result.

Facts about the pinned upstream (docs/UPSTREAM.md §4) that this harness encodes:

  * The verdict is NOT carried by the exit code. Both SAT and UNSAT exit 0, so
    a harness keyed on exit status would score every instance identically.
    The verdict is parsed from the trailing line of output.
  * rIC3 writes its log, its statistics AND its verdict all to stdout; stderr
    is empty. We therefore merge stderr into stdout and parse the tail.
  * `--ui true` is the default and pollutes machine-readable output, so
    `--ui false` is always passed.
  * The engine is a required subcommand; there is no default to rely on.
  * /usr/bin/time is absent on this host, so peak RSS is collected via
    os.wait4(2) rusage rather than by wrapping the child.

Requires only the Python standard library. Run it through uv:

    uv run --python 3.14 bench/harness.py --help
"""

from __future__ import annotations

import argparse
import concurrent.futures
import dataclasses
import datetime
import json
import os
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

#: Recognised model file extensions. AIGER is the bit-level track, BTOR2 the
#: word-level track (AGENTS.md §1.2).
MODEL_EXTENSIONS = (".aig", ".aag", ".btor", ".btor2")

#: The trailing verdict tokens rIC3 emits.
VERDICT_RE = re.compile(r"^(SAT|UNSAT)\s*$", re.MULTILINE)

#: Corroborating log lines, used only to cross-check the trailing token.
PROVED_RE = re.compile(r"proved the property")
CEX_RE = re.compile(r"found a counterexample|counterexample found")

#: Allocation-failure signatures. Rust's allocator error handler prints
#: "memory allocation of N bytes failed" and then aborts; the kernel OOM path
#: and C++ allocators produce the other forms.
ALLOC_FAIL_RE = re.compile(
    r"memory allocation of \d+ bytes failed"
    r"|out of memory"
    r"|Cannot allocate memory"
    r"|std::bad_alloc"
    r"|MemoryError",
    re.IGNORECASE,
)

#: An ordinary Rust panic. This MUST be distinguished from an allocation
#: failure: the release profile sets panic = "abort", so *every* panic exits
#: via SIGABRT, exactly like an allocation failure does. Classifying on the
#: signal alone therefore reports genuine bugs and bad CLI combinations as
#: memory-outs. It did: an early version of this harness scored 25 of 25
#: instances as memout when rIC3 was in fact rejecting a flag combination
#: ("cannot enable both dynamic and drop-po").
PANIC_RE = re.compile(r"panicked at|explicit panic|RUST_BACKTRACE")

#: How many bytes of the tail of the output we read back to find the verdict.
#: rIC3's statistics block is a few KiB; 64 KiB is generous.
TAIL_BYTES = 64 * 1024

#: Verdict values.
V_SAFE = "unsat"      # property holds; an inductive invariant was found
V_UNSAFE = "sat"      # counterexample exists
V_UNKNOWN = "unknown"

#: Run status values.
S_SOLVED = "solved"
S_TIMEOUT = "timeout"
S_ERROR = "error"
S_MEMOUT = "memout"

#: The upstream verification-cache directory name (`ric3 clean` manages it).
#: Stage 1's cache work must decide whether to extend this or sit beside it;
#: see docs/UPSTREAM.md §4.4.
RIC3PROJ = "ric3proj"


# ---------------------------------------------------------------------------
# Records
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class RunRecord:
    """One (instance, engine, seed, cache-state, repetition) measurement."""

    instance: str
    instance_name: str
    engine: str
    seed: int
    cache_state: str          # "cold" | "warm"
    repetition: int
    verdict: str              # V_SAFE | V_UNSAFE | V_UNKNOWN
    status: str               # S_SOLVED | S_TIMEOUT | S_MEMOUT | S_ERROR
    wall_s: float
    peak_rss_kb: int
    exit_code: int
    timeout_s: float
    memory_limit_mb: int
    argv: list[str]
    stdout_tail: str
    started_at: str

    def to_json(self) -> str:
        return json.dumps(dataclasses.asdict(self), ensure_ascii=False)


# ---------------------------------------------------------------------------
# Corpus discovery
# ---------------------------------------------------------------------------


def abs_no_symlink(path: Path) -> Path:
    """Absolute, normalised path that does NOT resolve symlinks.

    Symlinks are load-bearing in the corpus. bench/prepare_corpus.py creates
    `<stem>_safe.aig -> <stem>.aig` aliases so that two different models
    sharing one basename can be addressed as distinct cases. `Path.resolve()`
    collapses such an alias back onto its target, the alias name disappears,
    and the case silently vanishes from the suite -- that is exactly how an
    840-case run came out as 838.
    """
    return Path(os.path.normpath(path.absolute()))


def instance_stem(path: Path) -> str:
    """The canonical case identifier for a model file.

    This must agree with the CAV'25 artifact's normalisation, because our
    numbers are compared against its per-instance results. utils/evaluatee.py
    does basename-then-strip-one-extension:

        case = case.rsplit("/", 1)[-1]
        case = case.rsplit(".", 1)[0]

    We strip only a *known model extension* instead of "whatever follows the
    last dot". On real corpus files the two agree, because every file ends in
    .aig/.aag/.btor/.btor2 -- but ours is idempotent, and theirs is not.

    That distinction is not academic. Cases like `93.c.aig` carry a dot inside
    the identifier: the artifact reduces it to `93.c`, and applying the naive
    rule a second time (to an already-normalised list entry) yields `93`, which
    matches nothing. That silent mismatch cost 29 cases in a first run of this
    harness -- the AGENTS.md §1.4(a) failure mode exactly, where one side keys
    on a content hash and the other on a position. Identity is defined here,
    once, and every consumer derives from it.
    """
    name = path.name
    lowered = name.lower()
    for ext in MODEL_EXTENSIONS:
        if lowered.endswith(ext):
            return name[: -len(ext)]
    return name


def discover_instances(
    roots: list[str],
    limit: int | None = None,
    extensions: tuple[str, ...] = MODEL_EXTENSIONS,
    allow_stems: set[str] | None = None,
    dedup_by_stem: bool = False,
) -> list[Path]:
    """Recursively collect model files under `roots`, deterministically ordered.

    A stable order matters: PAR-2 over a truncated corpus is only comparable
    across runs if the truncation picks the same instances.

    `extensions` restricts the formats considered. The published rIC3-ic3
    baseline ran on AIGER, so reproducing it means passing ("aig",).

    `allow_stems` keeps only the named cases -- the mechanism for reproducing
    the artifact's exact 840-case suite instead of an approximation of it.

    `dedup_by_stem` keeps one file per case identifier. HWMCC'20 re-ships a
    subset of HWMCC'19 under a `2019/` subtree, so the union of competition
    archives contains the same benchmark more than once; the paper's suite is
    de-duplicated ("After removing duplicates ... 840 unique cases"). Without
    this, a case would be measured twice and PAR-2 would silently weight it
    double.
    """
    exts = tuple(e if e.startswith(".") else "." + e for e in extensions)
    found: list[Path] = []
    for root in roots:
        p = Path(root)
        if p.is_file():
            if p.suffix.lower() in exts:
                found.append(abs_no_symlink(p))
            else:
                print(
                    f"warning: {p} does not match the selected extensions "
                    f"{exts}, skipping",
                    file=sys.stderr,
                )
            continue
        if not p.is_dir():
            print(f"warning: {p} does not exist, skipping", file=sys.stderr)
            continue
        for ext in exts:
            found.extend(abs_no_symlink(q) for q in p.rglob(f"*{ext}"))

    # Deduplicate identical paths while keeping determinism.
    unique = sorted(set(found), key=lambda q: str(q))

    if allow_stems is not None:
        unique = [q for q in unique if instance_stem(q) in allow_stems]
        missing = allow_stems - {instance_stem(q) for q in unique}
        if missing:
            print(
                f"warning: {len(missing)} requested case(s) absent from the "
                f"corpus; the suite is INCOMPLETE and its solved count is not "
                f"comparable to the published figure. First few: "
                f"{sorted(missing)[:5]}",
                file=sys.stderr,
            )

    if dedup_by_stem:
        by_stem: dict[str, Path] = {}
        for q in unique:
            by_stem.setdefault(instance_stem(q), q)
        unique = [by_stem[s] for s in sorted(by_stem)]

    if limit is not None:
        unique = unique[:limit]
    return unique


# ---------------------------------------------------------------------------
# Verdict parsing
# ---------------------------------------------------------------------------


def read_tail(path: Path, nbytes: int = TAIL_BYTES) -> str:
    """Read at most the last `nbytes` bytes of `path` as lossy UTF-8."""
    try:
        size = path.stat().st_size
    except OSError:
        return ""
    try:
        with path.open("rb") as fh:
            if size > nbytes:
                fh.seek(size - nbytes)
            data = fh.read()
    except OSError:
        return ""
    return data.decode("utf-8", errors="replace")


def parse_verdict(tail: str) -> str:
    """Extract the verdict from rIC3's output tail.

    The authoritative signal is the trailing SAT/UNSAT token. We take the LAST
    match, because multi-property runs may emit several. Log lines are used
    only to detect disagreement, which we surface rather than silently resolve.
    """
    matches = VERDICT_RE.findall(tail)
    if not matches:
        return V_UNKNOWN
    token = matches[-1].strip().upper()
    verdict = V_SAFE if token == "UNSAT" else V_UNSAFE

    # Cross-check against the log narrative. Disagreement is a harness bug or
    # an upstream output change; either way it must not pass silently.
    if verdict == V_SAFE and CEX_RE.search(tail) and not PROVED_RE.search(tail):
        print(
            "warning: trailing token says UNSAT but log reports a counterexample",
            file=sys.stderr,
        )
    return verdict


# ---------------------------------------------------------------------------
# Single run
# ---------------------------------------------------------------------------


def clear_cache(workdir: Path) -> None:
    """Make the run cold by removing the upstream verification cache.

    Stage 1 has not yet added the method/digest cache, so today this only
    removes `ric3proj`. The cold/warm axis exists in the harness now so that
    the cache work lands against an already-validated measurement path
    (AGENTS.md §1.2: "must support a cold/warm distinction").
    """
    target = workdir / RIC3PROJ
    if target.exists():
        shutil.rmtree(target, ignore_errors=True)


def build_argv(
    binary: Path,
    model: Path,
    engine: str,
    seed: int,
    engine_args: list[str],
    memory_limit_mb: int = 0,
) -> list[str]:
    """Assemble the rIC3 command line.

    Note the ordering imposed by the CLI: global options and the model precede
    the engine subcommand, and engine options follow it.

    When `memory_limit_mb` is positive the command is wrapped in prlimit(1) so
    the child runs under an address-space ceiling. The rIC3 paper's
    single-thread evaluation used a 32 GB cap and reported 9 memory-outs for
    rIC3-ic3 (arXiv:2502.13605 §7.1), so an uncapped run is NOT comparable to
    the published 606/PAR-2 2147.70 baseline: an instance that the paper counts
    as a memory-out would, uncapped, either solve or swap.

    Caveat worth recording: RLIMIT_AS bounds virtual address space, not
    resident set size, so an mmap-heavy process can trip it while its RSS sits
    well under the cap. Competition harnesses use cgroups instead. We store the
    limit next to the measured peak RSS in every record so that a suspicious
    memory-out can be re-classified after the fact rather than re-run.
    """
    argv: list[str] = []
    if memory_limit_mb > 0:
        argv += ["prlimit", f"--as={memory_limit_mb * 1024 * 1024}", "--"]
    argv += [
        str(binary),
        "check",
        "--ui",
        "false",
        str(model),
        engine,
    ]
    # Not every engine accepts --rseed (e.g. the word-level ones differ), so
    # the caller can suppress it by passing seed < 0.
    if seed >= 0:
        argv += ["--rseed", str(seed)]
    argv += engine_args
    return argv


def terminate_group(pid: int) -> tuple[bool, int, object | None]:
    """Kill the child's whole process group, politely then not.

    rIC3 forks (the `fork` and `ipc-channel` crates back the portfolio engine),
    so killing only the direct child can leak workers that then contend with
    subsequent measurements and corrupt timings.

    Returns ``(reaped, raw_status, rusage)``. When the child dies during the
    grace period we reap it here, and its rusage is handed back to the caller
    instead of being discarded. Peak RSS is a required per-instance figure
    (AGENTS.md §1.2), and a timed-out instance is precisely the case where the
    memory number matters most -- an earlier version of this function dropped
    it and every timeout recorded rss=-1.
    """
    try:
        os.killpg(pid, signal.SIGTERM)
    except ProcessLookupError:
        return (False, 0, None)
    except PermissionError:
        pass

    deadline = time.monotonic() + 2.0
    while time.monotonic() < deadline:
        try:
            done, raw_status, rusage = os.wait4(pid, os.WNOHANG)
        except ChildProcessError:
            return (True, 0, None)
        if done != 0:
            return (True, raw_status, rusage)
        time.sleep(0.02)

    try:
        os.killpg(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    return (False, 0, None)


def run_one(
    binary: Path,
    model: Path,
    engine: str,
    seed: int,
    timeout_s: float,
    cache_state: str,
    repetition: int,
    workdir: Path,
    engine_args: list[str],
    keep_output: Path | None,
    memory_limit_mb: int,
) -> RunRecord:
    """Run one instance under a wall-clock timeout, collecting rusage."""
    if cache_state == "cold":
        clear_cache(workdir)

    argv = build_argv(binary, model, engine, seed, engine_args, memory_limit_mb)
    started_at = datetime.datetime.now(datetime.UTC).isoformat()

    out_fd, out_name = tempfile.mkstemp(prefix="ric3-bench-", suffix=".log")
    out_path = Path(out_name)
    timed_out = False
    rusage = None
    raw_status = 0

    try:
        with os.fdopen(out_fd, "wb") as out_fh:
            t0 = time.monotonic()
            proc = subprocess.Popen(
                argv,
                stdout=out_fh,
                stderr=subprocess.STDOUT,
                stdin=subprocess.DEVNULL,
                cwd=str(workdir),
                start_new_session=True,  # own process group, for group kill
            )

            # We reap with os.wait4 to obtain rusage, which Popen.wait() does
            # not expose. Because we reap the pid ourselves, we must tell Popen
            # the child is gone (below) or its destructor will warn and its
            # own wait() would raise ChildProcessError.
            deadline = t0 + timeout_s
            poll = 0.005
            while True:
                try:
                    done_pid, raw_status, rusage = os.wait4(proc.pid, os.WNOHANG)
                except ChildProcessError:
                    done_pid, raw_status, rusage = proc.pid, 0, None
                    break
                if done_pid != 0:
                    break
                if time.monotonic() >= deadline:
                    timed_out = True
                    reaped, raw_status, rusage = terminate_group(proc.pid)
                    if not reaped:
                        try:
                            _, raw_status, rusage = os.wait4(proc.pid, 0)
                        except ChildProcessError:
                            rusage = None
                    break
                time.sleep(poll)
                # Back off so that hour-long instances do not spin.
                poll = min(poll * 1.5, 0.25)
            wall_s = time.monotonic() - t0

            # Mark the Popen object as already-reaped.
            proc.returncode = (
                os.waitstatus_to_exitcode(raw_status) if not timed_out else -9
            )

        tail = read_tail(out_path)
        if keep_output is not None:
            keep_output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(out_path, keep_output)
    finally:
        out_path.unlink(missing_ok=True)

    peak_rss_kb = int(rusage.ru_maxrss) if rusage is not None else -1

    try:
        exit_code = os.waitstatus_to_exitcode(raw_status)
    except ValueError:
        exit_code = -1

    if timed_out:
        verdict, status = V_UNKNOWN, S_TIMEOUT
    else:
        verdict = parse_verdict(tail)
        if verdict != V_UNKNOWN:
            status = S_SOLVED
        elif ALLOC_FAIL_RE.search(tail):
            # Positive evidence of an allocation failure in the output.
            status = S_MEMOUT
        elif PANIC_RE.search(tail):
            # A real panic: a bug, an unsupported model, or a rejected option
            # combination. Never a memory-out.
            status = S_ERROR
        elif memory_limit_mb > 0 and exit_code == -signal.SIGKILL:
            # SIGKILL with a cap in force and no panic text is the OOM-killer
            # signature; the process had no chance to print anything.
            status = S_MEMOUT
        else:
            status = S_ERROR

    return RunRecord(
        instance=str(model),
        instance_name=model.name,
        engine=engine,
        seed=seed,
        cache_state=cache_state,
        repetition=repetition,
        verdict=verdict,
        status=status,
        wall_s=round(wall_s, 6),
        peak_rss_kb=peak_rss_kb,
        exit_code=exit_code,
        timeout_s=timeout_s,
        memory_limit_mb=memory_limit_mb,
        argv=argv,
        stdout_tail=tail[-2000:],
        started_at=started_at,
    )


# ---------------------------------------------------------------------------
# Scoring
# ---------------------------------------------------------------------------


def score(records: list[RunRecord], timeout_s: float) -> dict:
    """Solved count and PAR-2, the only two figures this project reports.

    PAR-2 (penalised average runtime, factor 2) charges a solved instance its
    wall clock and an unsolved instance twice the timeout, then averages over
    all instances:

        PAR-2 = ( sum_solved t_i  +  sum_unsolved 2*T ) / N

    A run is 'solved' only when a verdict was parsed. Timeouts and errors are
    both unsolved, but they are counted separately because an error is a
    harness or build problem while a timeout is a result.
    """
    # Collapse repetitions by taking, per (instance, cache_state), the median
    # wall time of solved runs; a single instance must contribute once.
    by_key: dict[tuple[str, str], list[RunRecord]] = {}
    for r in records:
        by_key.setdefault((r.instance, r.cache_state), []).append(r)

    n = 0
    solved = 0
    timeouts = 0
    memouts = 0
    errors = 0
    safe = 0
    unsafe = 0
    par2_total = 0.0
    solved_times: list[float] = []
    peak_rss_max = 0

    for (_instance, _state), runs in sorted(by_key.items()):
        n += 1
        ok = [r for r in runs if r.status == S_SOLVED]
        peak_rss_max = max(peak_rss_max, max(r.peak_rss_kb for r in runs))
        if ok:
            solved += 1
            times = sorted(r.wall_s for r in ok)
            mid = times[len(times) // 2]
            par2_total += mid
            solved_times.append(mid)
            if ok[0].verdict == V_SAFE:
                safe += 1
            else:
                unsafe += 1
        else:
            par2_total += 2.0 * timeout_s
            if any(r.status == S_TIMEOUT for r in runs):
                timeouts += 1
            elif any(r.status == S_MEMOUT for r in runs):
                memouts += 1
            else:
                errors += 1

    return {
        "instances": n,
        "solved": solved,
        "unsolved": n - solved,
        "timeouts": timeouts,
        "memouts": memouts,
        "errors": errors,
        "safe_unsat": safe,
        "unsafe_sat": unsafe,
        "par2": round(par2_total / n, 4) if n else None,
        "total_solved_wall_s": round(sum(solved_times), 3),
        "peak_rss_kb_max": peak_rss_max,
        "timeout_s": timeout_s,
    }


def verdict_map(records: list[RunRecord]) -> dict[str, str]:
    """instance -> verdict, for the 'zero verdict changes' release gate.

    AGENTS.md §3.3 makes a single changed verdict across the regression suite a
    merge blocker. That comparison needs a stable, diffable artifact.
    """
    out: dict[str, str] = {}
    for r in sorted(records, key=lambda x: (x.instance, x.cache_state, x.repetition)):
        if r.status != S_SOLVED:
            continue
        prior = out.get(r.instance)
        if prior is not None and prior != r.verdict:
            print(
                f"error: inconsistent verdicts for {r.instance}: {prior} vs {r.verdict}",
                file=sys.stderr,
            )
        out[r.instance] = r.verdict
    return out


# ---------------------------------------------------------------------------
# Environment capture
# ---------------------------------------------------------------------------


def git_output(args: list[str], repo: Path) -> str:
    try:
        res = subprocess.run(
            ["git", *args],
            cwd=str(repo),
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (OSError, subprocess.SubprocessError):
        return ""
    return res.stdout.strip() if res.returncode == 0 else ""


def tool_version(argv: list[str]) -> str:
    try:
        res = subprocess.run(argv, capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return ""
    return (res.stdout or res.stderr).strip().splitlines()[0] if res.stdout or res.stderr else ""


def capture_environment(repo: Path, binary: Path) -> dict:
    """Everything needed to attribute a number to a build and a machine."""
    return {
        "recorded_at": datetime.datetime.now(datetime.UTC).isoformat(),
        "repo": str(repo),
        "git_head": git_output(["rev-parse", "HEAD"], repo),
        "git_describe": git_output(["describe", "--tags", "--always", "--dirty"], repo),
        "git_upstream_merge_base": git_output(
            ["merge-base", "HEAD", "upstream/master"], repo
        ),
        # Only *tracked* modifications make a measurement unattributable. An
        # untracked local file (editor state, a workspace config directory) does
        # not change what was built, and letting it raise the dirty flag would
        # mean no measurement on a working machine is ever attributable.
        # Untracked files are still counted, so the record is not silent.
        "git_dirty": bool(
            git_output(["status", "--porcelain", "--untracked-files=no"], repo)
        ),
        "git_untracked_count": len(
            git_output(["ls-files", "--others", "--exclude-standard"], repo)
            .splitlines()
        ),
        "submodules": git_output(["submodule", "status"], repo).splitlines(),
        "binary": str(binary),
        "binary_version": tool_version([str(binary), "--version"]),
        "rustc": tool_version(["rustc", "--version"]),
        "cargo": tool_version(["cargo", "--version"]),
        "platform": platform.platform(),
        "processor": platform.processor(),
        "cpu_count": os.cpu_count(),
        "python": sys.version.split()[0],
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    ap = argparse.ArgumentParser(
        prog="harness.py",
        description="rIC3 stage-1 benchmark harness (solved count + PAR-2).",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument(
        "corpus",
        nargs="+",
        help="model files or directories to scan for .aig/.aag/.btor/.btor2",
    )
    ap.add_argument(
        "--binary",
        default="target/release/ric3",
        help="path to the ric3 binary (default: %(default)s)",
    )
    ap.add_argument(
        "--engine",
        default="ic3",
        help="engine subcommand; required by the CLI, recorded per run "
        "(default: %(default)s)",
    )
    ap.add_argument(
        "--engine-arg",
        action="append",
        default=[],
        dest="engine_args",
        help="extra argument forwarded to the engine; repeatable",
    )
    ap.add_argument(
        "--timeout",
        type=float,
        default=3600.0,
        help="per-instance wall-clock limit in seconds (default: %(default)s)",
    )
    ap.add_argument(
        "--memory-limit-mb",
        type=int,
        default=32768,
        help="address-space cap per instance via prlimit, in MiB; 0 disables. "
        "The published baseline used 32 GB, so changing this makes the run "
        "incomparable to it (default: %(default)s)",
    )
    ap.add_argument(
        "--seed",
        type=int,
        default=0,
        help="value for --rseed; negative suppresses the flag (default: %(default)s)",
    )
    ap.add_argument(
        "--cache-state",
        choices=("cold", "warm", "both"),
        default="cold",
        help="cold clears %s before each run (default: %%(default)s)" % RIC3PROJ,
    )
    ap.add_argument(
        "--repeat",
        type=int,
        default=1,
        help="repetitions per instance, for noise estimation (default: %(default)s)",
    )
    ap.add_argument(
        "--jobs",
        type=int,
        default=1,
        help="parallel instances. Keep at 1 for reportable timings; >1 "
        "distorts wall clock and PAR-2 (default: %(default)s)",
    )
    ap.add_argument(
        "--limit",
        type=int,
        default=None,
        help="use only the first N instances in deterministic order",
    )
    ap.add_argument(
        "--instance-list",
        default=None,
        help="file of case identifiers (one per line, extension optional); "
        "only these cases run. Use bench/corpus/reference/cases-840.txt to "
        "reproduce the published suite exactly",
    )
    ap.add_argument(
        "--ext",
        action="append",
        default=None,
        dest="extensions",
        choices=("aig", "aag", "btor", "btor2"),
        help="restrict model formats; repeatable. The published rIC3-ic3 "
        "baseline used aig (default: all)",
    )
    ap.add_argument(
        "--dedup-by-stem",
        action="store_true",
        help="keep one file per case identifier. Required when scanning "
        "several competition archives, which re-ship overlapping benchmarks",
    )
    ap.add_argument(
        "--out",
        default=None,
        help="results directory (default: bench/results/<timestamp>)",
    )
    ap.add_argument(
        "--keep-logs",
        action="store_true",
        help="retain each run's full output under <out>/logs/",
    )
    ap.add_argument(
        "--dry-run",
        action="store_true",
        help="list the discovered instances and the command line, then exit",
    )
    return ap.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    repo = Path(__file__).resolve().parent.parent

    binary = Path(args.binary)
    if not binary.is_absolute():
        binary = (repo / binary).resolve()

    allow_stems: set[str] | None = None
    if args.instance_list:
        list_path = Path(args.instance_list)
        if not list_path.exists():
            print(f"error: {list_path} not found", file=sys.stderr)
            return 2
        allow_stems = set()
        for raw in list_path.read_text(encoding="utf-8").splitlines():
            entry = raw.strip()
            if not entry or entry.startswith("#"):
                continue
            # Accept both "foo" and "foo.aig", and tolerate a trailing
            # whitespace-separated result column so the artifact's own result
            # files can be used directly as an instance list.
            entry = entry.split()[0]
            allow_stems.add(instance_stem(Path(entry)))

    extensions = (
        tuple(args.extensions) if args.extensions else MODEL_EXTENSIONS
    )
    instances = discover_instances(
        args.corpus,
        args.limit,
        extensions=extensions,
        allow_stems=allow_stems,
        dedup_by_stem=args.dedup_by_stem,
    )
    if not instances:
        print("error: no model files found in the given corpus", file=sys.stderr)
        return 2

    if args.dry_run:
        print(f"binary : {binary}")
        print(f"engine : {args.engine}")
        print(f"timeout: {args.timeout}s   seed: {args.seed}   jobs: {args.jobs}")
        print(f"example: {' '.join(build_argv(binary, instances[0], args.engine, args.seed, args.engine_args, args.memory_limit_mb))}")
        print(f"{len(instances)} instance(s):")
        for p in instances:
            print(f"  {p}")
        return 0

    if not binary.exists():
        print(
            f"error: {binary} not found; run `cargo build --release` first",
            file=sys.stderr,
        )
        return 2

    if args.jobs > 1:
        print(
            f"warning: --jobs {args.jobs} distorts wall clock and therefore PAR-2; "
            "these numbers are not reportable as a baseline",
            file=sys.stderr,
        )

    stamp = datetime.datetime.now(datetime.UTC).strftime("%Y%m%dT%H%M%SZ")
    out_dir = Path(args.out) if args.out else repo / "bench" / "results" / stamp
    out_dir.mkdir(parents=True, exist_ok=True)

    states = ["cold", "warm"] if args.cache_state == "both" else [args.cache_state]

    env = capture_environment(repo, binary)
    env["invocation"] = {
        "corpus": args.corpus,
        "engine": args.engine,
        "engine_args": args.engine_args,
        "timeout_s": args.timeout,
        "seed": args.seed,
        "cache_states": states,
        "repeat": args.repeat,
        "jobs": args.jobs,
        "instance_count": len(instances),
    }
    (out_dir / "environment.json").write_text(
        json.dumps(env, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    if env["git_dirty"]:
        print(
            "warning: working tree is dirty; this measurement is not attributable "
            "to a commit",
            file=sys.stderr,
        )

    # Build the work list.
    work: list[tuple[Path, str, int]] = []
    for state in states:
        for rep in range(args.repeat):
            for inst in instances:
                work.append((inst, state, rep))

    total = len(work)
    records: list[RunRecord] = []
    jsonl_path = out_dir / "results.jsonl"

    def do(item: tuple[Path, str, int]) -> RunRecord:
        inst, state, rep = item
        keep = None
        if args.keep_logs:
            keep = out_dir / "logs" / f"{inst.stem}.{state}.r{rep}.log"
        return run_one(
            binary=binary,
            model=inst,
            engine=args.engine,
            seed=args.seed,
            timeout_s=args.timeout,
            cache_state=state,
            repetition=rep,
            workdir=repo,
            engine_args=args.engine_args,
            keep_output=keep,
            memory_limit_mb=args.memory_limit_mb,
        )

    print(
        f"running {total} measurement(s) over {len(instances)} instance(s), "
        f"engine={args.engine}, timeout={args.timeout}s -> {out_dir}"
    )

    t_start = time.monotonic()
    with jsonl_path.open("w", encoding="utf-8") as jf:
        if args.jobs <= 1:
            for i, item in enumerate(work, 1):
                rec = do(item)
                records.append(rec)
                jf.write(rec.to_json() + "\n")
                jf.flush()
                print(
                    f"[{i}/{total}] {rec.instance_name} {rec.cache_state} "
                    f"-> {rec.verdict}/{rec.status} {rec.wall_s:.2f}s "
                    f"rss={rec.peak_rss_kb}kB"
                )
        else:
            with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as ex:
                futs = {ex.submit(do, item): item for item in work}
                for i, fut in enumerate(
                    concurrent.futures.as_completed(futs), 1
                ):
                    rec = fut.result()
                    records.append(rec)
                    jf.write(rec.to_json() + "\n")
                    jf.flush()
                    print(
                        f"[{i}/{total}] {rec.instance_name} {rec.cache_state} "
                        f"-> {rec.verdict}/{rec.status} {rec.wall_s:.2f}s "
                        f"rss={rec.peak_rss_kb}kB"
                    )
    elapsed = time.monotonic() - t_start

    summary = {
        "environment": env,
        "wall_clock_of_whole_run_s": round(elapsed, 3),
        "overall": score(records, args.timeout),
        "by_cache_state": {
            state: score([r for r in records if r.cache_state == state], args.timeout)
            for state in states
        },
    }
    (out_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    (out_dir / "verdicts.json").write_text(
        json.dumps(verdict_map(records), indent=2, sort_keys=True, ensure_ascii=False)
        + "\n",
        encoding="utf-8",
    )

    ov = summary["overall"]
    print()
    print(f"=== result ({out_dir}) ===")
    print(f"  instances : {ov['instances']}")
    print(f"  solved    : {ov['solved']}  (unsat/safe {ov['safe_unsat']}, sat/unsafe {ov['unsafe_sat']})")
    print(f"  unsolved  : {ov['unsolved']}  (timeout {ov['timeouts']}, memout {ov['memouts']}, error {ov['errors']})")
    print(f"  PAR-2     : {ov['par2']}   [timeout {ov['timeout_s']}s, mem cap {args.memory_limit_mb} MiB]")
    print(f"  peak RSS  : {ov['peak_rss_kb_max']} kB (max over runs)")
    if len(states) > 1:
        for state in states:
            s = summary["by_cache_state"][state]
            print(f"  {state:4s}: solved {s['solved']}/{s['instances']}, PAR-2 {s['par2']}")

    # A non-zero exit signals a harness/build problem, never a verdict.
    return 1 if ov["errors"] else 0


if __name__ == "__main__":
    sys.exit(main())
