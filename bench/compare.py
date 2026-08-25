#!/usr/bin/env python3
"""
Compare a harness run against the rIC3 CAV'25 artifact's per-instance results.

`docs/UPSTREAM.md` records the *claim* (606 solved, PAR-2 2147.70). This script
turns that claim into a per-instance diff, which is what the stage-1 gate
actually needs: AGENTS.md §3.3 makes a single changed verdict a merge blocker,
and a whole-suite aggregate cannot tell you which instance moved.

The one thing this script insists on: when our run covers only part of the
840-case suite, the reference is re-scored **over exactly the same subset**.
Comparing a 507-case PAR-2 against the published 840-case PAR-2 is meaningless,
and doing it by accident is easy.

Usage:

    uv run --python 3.14 bench/compare.py <results-dir-or-jsonl> <reference.txt>
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

# Reuse the single definition of case identity (AGENTS.md §1.4(a)).
from harness import S_MEMOUT, S_SOLVED, S_TIMEOUT, instance_stem  # noqa: E402

#: The artifact hard-codes this in utils/evaluatee.py.
REFERENCE_TIMEOUT = 3600.0


# ---------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------


def load_reference(path: Path) -> tuple[dict[str, float], set[str], set[str]]:
    """Parse an artifact result file into (solved_times, timeouts, memouts).

    Mirrors utils/evaluatee.py: a bare float is a solved time, `Timeout` is a
    timeout, `Failed` is a memory-out, and a time above the limit is folded
    back into the timeout set.
    """
    solved: dict[str, float] = {}
    timeouts: set[str] = set()
    memouts: set[str] = set()

    for lineno, raw in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) < 2:
            print(
                f"warning: {path}:{lineno}: cannot parse {line!r}, skipping",
                file=sys.stderr,
            )
            continue
        case = instance_stem(Path(parts[0]))
        token = parts[1]
        if token == "Timeout":
            timeouts.add(case)
        elif token == "Failed":
            memouts.add(case)
        else:
            try:
                value = float(token)
            except ValueError:
                print(
                    f"warning: {path}:{lineno}: unrecognised result {token!r}",
                    file=sys.stderr,
                )
                continue
            if value > REFERENCE_TIMEOUT:
                timeouts.add(case)
            else:
                solved[case] = value
    return solved, timeouts, memouts


def load_run(target: Path) -> dict[str, dict]:
    """Load our own results.jsonl, collapsing repetitions per case.

    When a case was measured several times we keep the median-by-wall-clock
    solved run, matching the harness's own scoring, so this script and
    `score()` cannot disagree.
    """
    jsonl = target
    if target.is_dir():
        jsonl = target / "results.jsonl"
    if not jsonl.exists():
        print(f"error: {jsonl} not found", file=sys.stderr)
        sys.exit(2)

    grouped: dict[str, list[dict]] = {}
    for raw in jsonl.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line:
            continue
        rec = json.loads(line)
        case = instance_stem(Path(rec["instance"]))
        grouped.setdefault(case, []).append(rec)

    collapsed: dict[str, dict] = {}
    for case, recs in grouped.items():
        ok = [r for r in recs if r["status"] == S_SOLVED]
        if ok:
            ok.sort(key=lambda r: r["wall_s"])
            collapsed[case] = ok[len(ok) // 2]
        else:
            # Prefer a timeout over an error when describing an unsolved case.
            recs.sort(key=lambda r: 0 if r["status"] == S_TIMEOUT else 1)
            collapsed[case] = recs[0]
    return collapsed


# ---------------------------------------------------------------------------
# Scoring
# ---------------------------------------------------------------------------


def par2_over(
    cases: set[str],
    solved_times: dict[str, float],
    timeout_s: float,
) -> tuple[int, float]:
    """(solved_count, PAR-2) restricted to `cases`.

    Same formula as the artifact and as harness.score():
        (sum of solved times + unsolved * 2 * T) / N
    """
    if not cases:
        return 0, float("nan")
    total = 0.0
    solved = 0
    for case in cases:
        t = solved_times.get(case)
        if t is None:
            total += 2.0 * timeout_s
        else:
            total += t
            solved += 1
    return solved, round(total / len(cases), 4)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        prog="compare.py",
        description="Diff a harness run against the CAV'25 artifact results.",
    )
    ap.add_argument("results", help="results directory or results.jsonl")
    ap.add_argument("reference", help="artifact result file, e.g. rIC3-ic3.txt")
    ap.add_argument(
        "--timeout",
        type=float,
        default=None,
        help="timeout used for PAR-2; default is taken from our own records",
    )
    ap.add_argument(
        "--show",
        type=int,
        default=15,
        help="how many instances to list per category (default: %(default)s)",
    )
    args = ap.parse_args(argv)

    ref_solved, ref_to, ref_mo = load_reference(Path(args.reference))
    ref_cases = set(ref_solved) | ref_to | ref_mo
    run = load_run(Path(args.results))

    if not run:
        print("error: no records in the run", file=sys.stderr)
        return 2

    timeout_s = args.timeout
    if timeout_s is None:
        timeouts = {r["timeout_s"] for r in run.values()}
        if len(timeouts) > 1:
            print(
                f"warning: run mixes timeouts {sorted(timeouts)}; PAR-2 is not "
                "well defined. Pass --timeout explicitly",
                file=sys.stderr,
            )
        timeout_s = max(timeouts)

    ours_solved = {
        c: r["wall_s"] for c, r in run.items() if r["status"] == S_SOLVED
    }

    # The comparison set: cases present in BOTH, so neither side is charged for
    # instances the other never saw.
    common = set(run) & ref_cases
    only_ours = set(run) - ref_cases
    missing = ref_cases - set(run)

    print("=== coverage ===")
    print(f"  reference cases      : {len(ref_cases)}")
    print(f"  measured cases       : {len(run)}")
    print(f"  compared (common)    : {len(common)}")
    if missing:
        print(f"  reference-only       : {len(missing)}  (not measured here)")
    if only_ours:
        print(f"  ours-only            : {len(only_ours)}  (absent from reference)")
    if len(common) < len(ref_cases):
        print(
            "  NOTE: partial suite. Both sides are re-scored over the common\n"
            "        subset below; these figures are NOT the published 840-case\n"
            "        numbers and must not be quoted as such."
        )

    our_n, our_par2 = par2_over(common, ours_solved, timeout_s)
    ref_n, ref_par2 = par2_over(common, ref_solved, timeout_s)

    print()
    print(f"=== score over {len(common)} common case(s), timeout {timeout_s}s ===")
    print(f"  {'':22s}{'solved':>8s}{'PAR-2':>12s}")
    print(f"  {'ours':22s}{our_n:>8d}{our_par2:>12.2f}")
    print(f"  {'reference (CAV25)':22s}{ref_n:>8d}{ref_par2:>12.2f}")
    delta = our_n - ref_n
    print(f"  {'delta':22s}{delta:>+8d}{our_par2 - ref_par2:>+12.2f}")

    # Categorise.
    regressions = []   # reference solved, we did not
    improvements = []  # we solved, reference did not
    both_solved = []
    neither = []
    for case in sorted(common):
        rec = run[case]
        we_ok = rec["status"] == S_SOLVED
        ref_ok = case in ref_solved
        if we_ok and ref_ok:
            both_solved.append((case, rec["wall_s"], ref_solved[case]))
        elif ref_ok and not we_ok:
            regressions.append((case, rec["status"]))
        elif we_ok and not ref_ok:
            kind = "Timeout" if case in ref_to else "Failed"
            improvements.append((case, rec["wall_s"], kind))
        else:
            neither.append(case)

    print()
    print("=== categories ===")
    print(f"  both solved          : {len(both_solved)}")
    print(f"  regressions          : {len(regressions)}   (reference solved, we did not)")
    print(f"  improvements         : {len(improvements)}   (we solved, reference did not)")
    print(f"  neither solved       : {len(neither)}")

    if regressions:
        print()
        print(f"  --- regressions (first {args.show}) ---")
        for case, status in regressions[: args.show]:
            print(f"    {case}   ours={status}")
    if improvements:
        print()
        print(f"  --- improvements (first {args.show}) ---")
        for case, wall, kind in improvements[: args.show]:
            print(f"    {case}   ours={wall:.2f}s  reference={kind}")

    # Runtime ratio on the cases both solved. Hardware differs from the paper's
    # EPYC 7532, so this is a sanity band, not a claim.
    if both_solved:
        ratios = sorted(
            (ours / ref) for _c, ours, ref in both_solved if ref > 0.0
        )
        if ratios:
            mid = ratios[len(ratios) // 2]
            p90 = ratios[min(len(ratios) - 1, int(len(ratios) * 0.9))]
            print()
            print("=== runtime ratio ours/reference, cases both solved ===")
            print(f"  median : {mid:.2f}x")
            print(f"  p90    : {p90:.2f}x")
            print(f"  min    : {ratios[0]:.2f}x     max: {ratios[-1]:.2f}x")
            print(
                "  (different hardware from the paper's EPYC 7532; treat as a\n"
                "   sanity band, not a speed claim)"
            )

    # Verdict-consistency check. The reference carries no verdict, only a time,
    # so the strongest available statement is about solvedness. Any case we
    # report as an error rather than a timeout is a harness/build problem.
    errors = [
        c
        for c, r in run.items()
        if r["status"] not in (S_SOLVED, S_TIMEOUT, S_MEMOUT)
    ]
    if errors:
        print()
        print(f"=== errors: {len(errors)} case(s) failed without a verdict ===")
        for case in errors[: args.show]:
            print(f"    {case}   exit={run[case]['exit_code']}")
        print("  these are build/harness failures, not results")

    return 1 if (regressions or errors) else 0


if __name__ == "__main__":
    sys.exit(main())
