#!/usr/bin/env python3
"""
Resolve case-identifier collisions in the assembled HWMCC corpus.

Why this exists
---------------
The benchmark suite is keyed by *basename minus extension* -- that is the
normalisation the CAV'25 artifact uses (utils/evaluatee.py), so it is the one we
must match. But basenames are not unique across the competition archives:

    hwmcc24/aiger/2019/mann/safe/analog_estimation_convergence.aig
    hwmcc24/aiger/2019/mann/unsafe/analog_estimation_convergence.aig

Two *different* models, same basename, distinguished only by the `safe/` vs
`unsafe/` directory. The artifact's own result file resolves this by suffixing:

    analog_estimation_convergence_safe.aig    0.52
    analog_estimation_convergence_unsafe.aig  0.17
    analog_estimation_convergence.aig         0.84   <- the HWMCC'19 one

Without a fix, a basename-keyed harness silently drops one of each colliding
pair and reports 838 of 840 cases. That is precisely the AGENTS.md §1.4(a)
failure mode -- an identifier that is not actually an identifier -- reappearing
one layer down, in the corpus rather than in the clause encoding.

What it does
------------
For every group of files sharing a case identifier, if the group can be told
apart by a `safe`/`unsafe` path component, create a sibling symlink named
`<stem>_<safe|unsafe><ext>`. Originals are left untouched, so identifiers that
were already unique keep matching. Symlinks are used rather than copies because
the AIGER corpus is ~3 GB.

Idempotent: re-running makes no further changes.

Usage:
    uv run --python 3.14 bench/prepare_corpus.py bench/corpus
    uv run --python 3.14 bench/prepare_corpus.py bench/corpus --check \
        --instance-list bench/corpus/reference/cases-840.txt
"""

from __future__ import annotations

import argparse
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from harness import MODEL_EXTENSIONS, instance_stem  # noqa: E402

#: Path components that discriminate an otherwise-colliding basename.
DISCRIMINATORS = ("safe", "unsafe")


def model_files(root: Path) -> list[Path]:
    """Every model file under `root`, excluding archives and symlinks."""
    out: list[Path] = []
    for ext in MODEL_EXTENSIONS:
        for p in root.rglob(f"*{ext}"):
            if p.is_symlink():
                continue
            if "archives" in p.parts:
                continue
            out.append(p)
    return sorted(out, key=str)


def discriminator_of(path: Path) -> str | None:
    """Return 'safe' or 'unsafe' if a path component says so.

    `unsafe` is checked first: a naive membership test would let the substring
    `safe` inside `unsafe` win, mislabelling every unsafe instance. The parts
    are matched exactly, and scanned from the leaf upward so that the closest
    qualifier to the file wins.
    """
    for part in reversed(path.parts[:-1]):
        lowered = part.lower()
        if lowered == "unsafe":
            return "unsafe"
        if lowered == "safe":
            return "safe"
    return None


def extension_of(path: Path) -> str:
    lowered = path.name.lower()
    for ext in MODEL_EXTENSIONS:
        if lowered.endswith(ext):
            return path.name[-len(ext):]
    return path.suffix


def plan_aliases(
    files: list[Path], root: Path
) -> tuple[list[tuple[Path, Path]], list[list[Path]]]:
    """Compute (link, target) pairs for colliding identifiers.

    The collision that matters is *within a single competition archive*. Across
    archives, a repeated basename is just the same benchmark re-shipped (HWMCC'20
    re-ships much of HWMCC'19), and de-duplication is the right answer -- adding
    a suffix there would invent case names the reference never had.

    Concretely, for `analog_estimation_convergence` the reference expects three
    distinct cases:

        analog_estimation_convergence          0.84   <- HWMCC'19, safe only
        analog_estimation_convergence_safe     0.52   <- HWMCC'24 safe/
        analog_estimation_convergence_unsafe   0.17   <- HWMCC'24 unsafe/

    Only HWMCC'24 holds two different models under that one basename, so only
    HWMCC'24 gets suffixed aliases. Scoping by archive produces exactly that,
    where scoping by the whole corpus would wrongly relabel the HWMCC'19 file
    as `_safe` and leave the plain identifier unmatched.

    Returns the alias plan plus any collision group that safe/unsafe could not
    separate, which the caller reports rather than silently ignoring.
    """
    def archive_of(path: Path) -> str:
        rel = path.relative_to(root)
        return rel.parts[0] if rel.parts else ""

    groups: dict[tuple[str, str], list[Path]] = defaultdict(list)
    for p in files:
        groups[(archive_of(p), instance_stem(p))].append(p)

    plan: list[tuple[Path, Path]] = []
    unresolved: list[list[Path]] = []

    for (_archive, stem), members in sorted(groups.items()):
        if len(members) < 2:
            continue

        # Split by extension first. The same benchmark shipped as both .aig and
        # .btor2 shares a stem by design -- the paper's suite is "each available
        # in both AIGER and Btor2 formats" -- so that is not a collision and
        # must not be reported as one. Only two files of the SAME format under
        # one identifier are ambiguous.
        by_ext: dict[str, list[Path]] = defaultdict(list)
        for m in members:
            by_ext[extension_of(m).lower()].append(m)

        for _ext_key, same_ext in sorted(by_ext.items()):
            if len(same_ext) < 2:
                continue

            labelled: dict[str, list[Path]] = defaultdict(list)
            for m in same_ext:
                d = discriminator_of(m)
                if d is not None:
                    labelled[d].append(m)

            # Act only when the discriminators actually separate the group. A
            # group that is entirely `safe/` is a duplicate, not a collision.
            if len(labelled) < 2:
                unresolved.append(same_ext)
                continue

            for label, members_with_label in sorted(labelled.items()):
                target = sorted(members_with_label, key=str)[0]
                ext = extension_of(target)
                link = target.with_name(f"{stem}_{label}{ext}")
                if link.exists() or link.is_symlink():
                    continue
                plan.append((link, target))

    return plan, unresolved


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        prog="prepare_corpus.py",
        description="Create disambiguating aliases for colliding case names.",
    )
    ap.add_argument("corpus", help="corpus root, e.g. bench/corpus")
    ap.add_argument(
        "--check",
        action="store_true",
        help="report only; make no changes",
    )
    ap.add_argument(
        "--instance-list",
        default=None,
        help="verify that every case in this list is now resolvable",
    )
    args = ap.parse_args(argv)

    root = Path(args.corpus)
    if not root.is_dir():
        print(f"error: {root} is not a directory", file=sys.stderr)
        return 2

    files = model_files(root)
    print(f"scanned {len(files)} model file(s) under {root}")

    plan, unresolved = plan_aliases(files, root)

    if not plan:
        print("no new aliases required")
    for link, target in plan:
        rel_link = link.relative_to(root)
        rel_target = target.relative_to(root)
        if args.check:
            print(f"  would link {rel_link} -> {rel_target.name}")
            continue
        try:
            link.symlink_to(target.name)
        except OSError as exc:
            print(f"error: cannot create {rel_link}: {exc}", file=sys.stderr)
            return 1
        print(f"  linked {rel_link} -> {rel_target.name}")

    for group in unresolved:
        print(
            "warning: identifier collision that safe/unsafe cannot separate: "
            + ", ".join(str(p.relative_to(root)) for p in group),
            file=sys.stderr,
        )

    if args.instance_list:
        wanted: set[str] = set()
        for raw in Path(args.instance_list).read_text(encoding="utf-8").splitlines():
            entry = raw.strip()
            if not entry or entry.startswith("#"):
                continue
            wanted.add(instance_stem(Path(entry.split()[0])))

        # Recompute including symlinks, which is what the harness will see.
        present: set[str] = set()
        for ext in MODEL_EXTENSIONS:
            for p in root.rglob(f"*{ext}"):
                if "archives" in p.parts:
                    continue
                present.add(instance_stem(p))

        missing = sorted(wanted - present)
        print()
        print(f"requested cases : {len(wanted)}")
        print(f"resolvable      : {len(wanted) - len(missing)}")
        if missing:
            print(f"still missing   : {len(missing)}")
            for case in missing[:20]:
                print(f"    {case}")
            return 1
        print("all requested cases are present")

    return 0


if __name__ == "__main__":
    sys.exit(main())
