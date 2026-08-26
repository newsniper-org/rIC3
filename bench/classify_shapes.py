#!/usr/bin/env python3
"""
Classify BTOR2 instances into the five profiling shapes of AGENTS.md §1.3.

§1.3 requires `perf` self-time across five *structurally distinct* workload
shapes, and forbids stage 4 from proceeding without the resulting decision
record. Choosing those five by name or by hunch would undermine the exercise,
so this derives them from the models themselves:

    1. control-dominated FSM / arbiter / handshake
    2. deep counter or timer            (the k-induction killer; stage-2 target)
    3. wide arithmetic datapath
    4. large memory / array-heavy
    5. a known-hard HWMCC instance that runs for minutes

Method: BTOR2 is line-oriented (`<id> <op> <sort> <operands...>`), so operator
histograms and declared bit widths are recoverable by text scanning alone — no
solver, no term construction, negligible CPU. That matters because this runs
while the 840-case baseline measurement holds the machine.

Shape 5 is not structural, so it comes from the reference timing file.

Caveat worth stating up front: the paper's suite is the **bit-vector** track, so
if array sorts are absent from the scanned set then shape 4 has no true
representative and the script says so rather than silently substituting one.

Usage:
    uv run --python 3.14 bench/classify_shapes.py bench/corpus/hwmcc19/btor2/bv
    uv run --python 3.14 bench/classify_shapes.py <roots>... \
        --restrict bench/corpus/reference/cases-840.txt --json shapes.json
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from harness import instance_stem  # noqa: E402  single definition of identity

# --- operator groups -------------------------------------------------------
# Taken from the operators actually present in the corpus rather than from the
# BTOR2 spec, so an unused operator cannot skew a ratio.

ARITH_OPS = frozenset({"add", "mul", "sub", "sdiv", "udiv", "srem", "urem", "neg"})
CMP_OPS = frozenset(
    {"eq", "neq", "ugt", "ugte", "ult", "ulte", "sgt", "sgte", "slt", "slte"}
)
CONTROL_OPS = frozenset({"ite", "implies"})
MEM_OPS = frozenset({"read", "write"})

#: Lines that declare structure rather than logic; excluded from ratio totals.
STRUCTURAL = frozenset(
    {
        "sort", "input", "output", "bad", "constraint", "fair", "justice",
        "const", "constd", "consth", "zero", "one", "ones", "end",
        "state", "next", "init",
    }
)


@dataclass
class Instance:
    path: Path
    stem: str
    lines: int = 0
    ops: Counter = field(default_factory=Counter)
    n_states: int = 0
    n_inputs: int = 0
    max_width: int = 0
    total_state_width: int = 0
    has_array: bool = False
    ref_time: float | None = None
    ref_status: str | None = None

    @property
    def logic_total(self) -> int:
        return sum(c for op, c in self.ops.items() if op not in STRUCTURAL)

    def ratio(self, group: frozenset[str]) -> float:
        total = self.logic_total
        return sum(self.ops.get(op, 0) for op in group) / total if total else 0.0

    @property
    def arith(self) -> float:
        return self.ratio(ARITH_OPS)

    @property
    def control(self) -> float:
        return self.ratio(CONTROL_OPS)

    @property
    def cmp(self) -> float:
        return self.ratio(CMP_OPS)

    @property
    def mem(self) -> float:
        return self.ratio(MEM_OPS)

    def summary(self) -> dict:
        return {
            "stem": self.stem,
            "path": str(self.path),
            "lines": self.lines,
            "states": self.n_states,
            "inputs": self.n_inputs,
            "max_width": self.max_width,
            "total_state_width": self.total_state_width,
            "has_array": self.has_array,
            "arith_ratio": round(self.arith, 4),
            "control_ratio": round(self.control, 4),
            "cmp_ratio": round(self.cmp, 4),
            "mem_ratio": round(self.mem, 4),
            "mul": self.ops.get("mul", 0),
            "add": self.ops.get("add", 0),
            "ite": self.ops.get("ite", 0),
            "ref_time": self.ref_time,
            "ref_status": self.ref_status,
        }


def scan(path: Path) -> Instance:
    """Extract operator and width statistics from one BTOR2 file."""
    inst = Instance(path=path, stem=instance_stem(path))
    sort_width: dict[str, int] = {}

    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        print(f"warning: cannot read {path}: {exc}", file=sys.stderr)
        return inst

    for raw in text.splitlines():
        line = raw.split(";", 1)[0].strip()
        if not line:
            continue
        fields = line.split()
        if len(fields) < 2:
            continue
        inst.lines += 1
        node_id, op = fields[0], fields[1]
        inst.ops[op] += 1

        if op == "sort" and len(fields) >= 3:
            if fields[2] == "bitvec" and len(fields) >= 4:
                try:
                    w = int(fields[3])
                except ValueError:
                    w = 0
                sort_width[node_id] = w
                inst.max_width = max(inst.max_width, w)
            elif fields[2] == "array":
                inst.has_array = True
                if len(fields) >= 5:
                    iw = sort_width.get(fields[3], 0)
                    ew = sort_width.get(fields[4], 0)
                    # Addressable bits, index capped so a 32-bit index does not
                    # produce an astronomical figure that swamps the ranking.
                    inst.max_width = max(inst.max_width, min(iw, 24) + ew)
        elif op == "state":
            inst.n_states += 1
            if len(fields) >= 3:
                inst.total_state_width += sort_width.get(fields[2], 0)
        elif op == "input":
            inst.n_inputs += 1

    return inst


def load_reference(path: Path) -> dict[str, tuple[float | None, str]]:
    """stem -> (time, status) from an artifact result file."""
    out: dict[str, tuple[float | None, str]] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) < 2:
            continue
        stem = instance_stem(Path(parts[0]))
        token = parts[1]
        if token == "Timeout":
            out[stem] = (None, "timeout")
        elif token == "Failed":
            out[stem] = (None, "memout")
        else:
            try:
                out[stem] = (float(token), "solved")
            except ValueError:
                continue
    return out


def collect(roots: list[str]) -> list[Path]:
    files: list[Path] = []
    for root in roots:
        p = Path(root)
        if p.is_file():
            files.append(p)
            continue
        for ext in ("btor", "btor2"):
            files.extend(q for q in p.rglob(f"*.{ext}") if not q.is_symlink())
    return sorted(set(files), key=str)


def pick(instances: list[Instance], n: int) -> dict[str, tuple[str, list[Instance]]]:
    """Rank candidates per shape, returning the criterion alongside them.

    The criterion string is printed so a reviewer can disagree with a weighting
    instead of with an opaque verdict.
    """
    out: dict[str, tuple[str, list[Instance]]] = {}

    out["1-control-fsm"] = (
        "arith_ratio < 0.02 and states >= 8, ranked by control_ratio then states",
        sorted(
            (i for i in instances if i.arith < 0.02 and i.n_states >= 8),
            key=lambda i: (-i.control, -i.n_states),
        )[:n],
    )

    # Few states but wide ones, with arithmetic and comparison present, is the
    # signature of a counter rather than a datapath.
    out["2-deep-counter"] = (
        "add >= 1, cmp > 0, max_width >= 16, states <= 32; ranked by width",
        sorted(
            (
                i
                for i in instances
                if i.ops.get("add", 0) >= 1
                and i.cmp > 0
                and i.max_width >= 16
                and i.n_states <= 32
            ),
            key=lambda i: (-i.max_width, i.n_states),
        )[:n],
    )

    out["3-wide-arith"] = (
        "mul >= 1 or arith_ratio >= 0.05; ranked by mul count then width",
        sorted(
            (i for i in instances if i.ops.get("mul", 0) >= 1 or i.arith >= 0.05),
            key=lambda i: (-i.ops.get("mul", 0), -i.max_width, -i.arith),
        )[:n],
    )

    array_backed = [i for i in instances if i.has_array or i.mem > 0]
    if array_backed:
        out["4-memory-array"] = (
            "array sort present; ranked by model size then mem_ratio, because "
            "§1.3 wants a workload that runs for minutes and the smallest "
            "array models are toy-sized",
            sorted(array_backed, key=lambda i: (-i.lines, -i.mem))[:n],
        )
    else:
        out["4-memory-array"] = (
            "NO array sorts in scope (bit-vector track) — falling back to "
            "largest aggregate state width, which is a bit-blasted memory at "
            "best. Consider scanning the array track for a true representative",
            sorted(instances, key=lambda i: -i.total_state_width)[:n],
        )

    # "Runs for minutes" per §1.3. A timeout yields no self-time distribution to
    # compare against, so solved-but-slow is strictly better for profiling.
    out["5-known-hard"] = (
        "reference solved in 60-1800 s; ranked by descending reference time",
        sorted(
            (
                i
                for i in instances
                if i.ref_status == "solved" and i.ref_time and 60.0 <= i.ref_time <= 1800.0
            ),
            key=lambda i: -(i.ref_time or 0),
        )[:n],
    )

    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        prog="classify_shapes.py",
        description="Pick the five §1.3 profiling shapes from BTOR2 structure.",
    )
    ap.add_argument("roots", nargs="+", help="BTOR2 files or directories")
    ap.add_argument(
        "--reference",
        default="bench/corpus/reference/rIC3-ic3-cav25.txt",
        help="artifact result file, for shape 5 (default: %(default)s)",
    )
    ap.add_argument(
        "--restrict",
        default=None,
        help="only consider stems listed here (e.g. cases-840.txt)",
    )
    ap.add_argument("--json", default=None, help="write full statistics here")
    ap.add_argument(
        "--show", type=int, default=3, help="candidates per shape (default: %(default)s)"
    )
    args = ap.parse_args(argv)

    allow: set[str] | None = None
    if args.restrict:
        allow = {
            instance_stem(Path(line.split()[0]))
            for line in Path(args.restrict).read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.startswith("#")
        }

    files = collect(args.roots)
    if not files:
        print("error: no BTOR2 files found", file=sys.stderr)
        return 2

    ref: dict[str, tuple[float | None, str]] = {}
    ref_path = Path(args.reference)
    if ref_path.exists():
        ref = load_reference(ref_path)
    else:
        print(f"warning: {ref_path} missing; shape 5 unavailable", file=sys.stderr)

    instances: list[Instance] = []
    # One entry per case identifier. HWMCC'20 re-ships a large part of
    # HWMCC'19, so scanning both archives yields the same benchmark twice and
    # every ranking below would list it twice. Files are already sorted, so the
    # first occurrence wins deterministically.
    seen: set[str] = set()
    duplicates = 0
    for f in files:
        inst = scan(f)
        if allow is not None and inst.stem not in allow:
            continue
        if inst.stem in seen:
            duplicates += 1
            continue
        seen.add(inst.stem)
        if inst.stem in ref:
            inst.ref_time, inst.ref_status = ref[inst.stem]
        instances.append(inst)

    print(f"scanned {len(files)} file(s), {len(instances)} unique case(s) in scope")
    if allow is not None:
        print(f"  restricted to {len(allow)} listed case(s)")
    if duplicates:
        print(f"  dropped {duplicates} duplicate(s) by case identifier")
    n_array = sum(1 for i in instances if i.has_array)
    print(f"  instances with array sorts: {n_array}")
    print()

    for name, (criterion, cands) in pick(instances, args.show).items():
        print(f"=== {name} ===")
        print(f"  criterion: {criterion}")
        if not cands:
            print("  NO CANDIDATE — shape not represented in the scanned set")
            print()
            continue
        for i in cands:
            ref_s = i.ref_status or "-"
            ref_t = f"/{i.ref_time:.0f}s" if i.ref_time else ""
            print(
                f"  {i.stem[:42]:42s} ln={i.lines:6d} st={i.n_states:4d} "
                f"w={i.max_width:5d} ar={i.arith:.3f} ct={i.control:.3f} "
                f"mul={i.ops.get('mul', 0):4d} {ref_s}{ref_t}"
            )
        print()

    if args.json:
        Path(args.json).write_text(
            json.dumps([i.summary() for i in instances], indent=2) + "\n",
            encoding="utf-8",
        )
        print(f"full statistics written to {args.json}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
