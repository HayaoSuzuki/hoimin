"""Disposable research probes; not a product operator or general-purpose runner."""

import argparse
import ast
import hashlib
import io
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import tokenize
from dataclasses import asdict, dataclass
from enum import StrEnum
from pathlib import Path


class Verdict(StrEnum):
    PASSED = "passed"
    ASSERTION = "assertion_failed"
    ERROR = "error"


@dataclass(frozen=True)
class Candidate:
    id: str
    start: int
    length: int
    original: str
    replacement: str
    line: int
    operator: str


@dataclass(frozen=True)
class Example:
    name: str
    issue: int
    source: str
    weak: str
    strong: str
    requirement: str


EXAMPLES = (
    Example(
        "window",
        1121,
        "def cut(xs, a, b):\n    return xs[a:b]\n",
        "assert cut([], 1, 3) == []",
        "assert cut([10, 20, 30, 40], 1, 3) == [20, 30]",
        "Return exactly the half-open requested window; exclude both outside regions.",
    ),
    Example(
        "prefix",
        1121,
        "def cut(xs, n):\n    return xs[:n]\n",
        "assert cut([], 2) == []",
        "assert cut([10, 20, 30, 40], 2) == [10, 20]",
        "Return the first n records in order, not the remaining records.",
    ),
    Example(
        "suffix",
        1121,
        "def cut(xs, n):\n    return xs[n:]\n",
        "assert cut([], 2) == []",
        "assert cut([10, 20, 30, 40], 2) == [30, 40]",
        "Skip the first n records and retain the remaining records.",
    ),
    Example(
        "literal_window",
        1121,
        "def cut(xs):\n    return xs[1:3]\n",
        "assert cut([]) == []",
        "assert cut([10, 20, 30, 40]) == [20, 30]",
        "Return the second and third records; existing numeric neighbors may suffice.",
    ),
    Example(
        "clipped_full",
        1121,
        "def cut(xs):\n    return xs[0:99]\n",
        "assert cut([]) == []",
        "assert cut([10, 20, 30]) == [10, 20, 30]",
        "For input lists of length at most 3, return every record (bounded domain).",
    ),
    Example(
        "true_division",
        1124,
        "def divide(a, b):\n    return a / b\n",
        "assert divide(12, 3) == 4",
        "assert divide(7, 2) == 3.5\nassert divide(-7, 2) == -3.5",
        "Return the signed real quotient, retaining fractional parts.",
    ),
    Example(
        "floor_division",
        1124,
        "def divide(a, b):\n    return a // b\n",
        "assert divide(12, 3) == 4",
        "assert divide(7, 2) == 3\nassert divide(-7, 2) == -4",
        "Count complete units by flooring toward negative infinity, not truncating.",
    ),
    Example(
        "nonnegative_truncation",
        1124,
        "def divide(a, b):\n    return int(a / b)\n",
        "assert divide(12, 3) == 4",
        "assert divide(7, 2) == 3\nassert divide(0, 2) == 0",
        "Inputs are nonnegative small integers and positive divisors; return complete units.",
    ),
    Example(
        "overload_unsupported",
        1124,
        "class OnlyTrue:\n    def __truediv__(self, other):\n        return 2.5\n"
        "def divide(a, b):\n    return a / b\n",
        "assert divide(OnlyTrue(), 2) == 2.5",
        "assert divide(OnlyTrue(), 3) == 2.5",
        "The input protocol requires only __truediv__; __floordiv__ is not promised.",
    ),
)


def offset(source: str, line: int, column: int) -> int:
    # This independent ASCII corpus keeps AST byte columns and token columns equal.
    assert source.isascii()
    return (
        sum(len(part) for part in source.splitlines(keepends=True)[: line - 1]) + column
    )


def bounds(source: str, node: ast.AST) -> tuple[int, int]:
    return (
        offset(source, node.lineno, node.col_offset),
        offset(source, node.end_lineno, node.end_col_offset),
    )


def experimental_candidates(source: str, issue: int) -> list[Candidate]:
    tree = ast.parse(source)
    result = []
    annotation_ranges = [
        bounds(source, n.annotation)
        for n in ast.walk(tree)
        if isinstance(n, ast.AnnAssign)
    ]

    def add(start: int, end: int, replacement: str, line: int, operator: str) -> None:
        modified = source[:start] + replacement + source[end:]
        ast.parse(modified)
        result.append(
            Candidate(
                f"probe-{len(result) + 1}",
                start,
                end - start,
                source[start:end],
                replacement,
                line,
                operator,
            )
        )

    def simple(node: ast.AST | None) -> bool:
        return (
            isinstance(node, ast.Name)
            or isinstance(node, ast.Constant)
            and type(node.value) is int
            or isinstance(node, ast.UnaryOp)
            and isinstance(node.op, ast.USub)
            and isinstance(node.operand, ast.Constant)
            and type(node.operand.value) is int
        )

    for node in ast.walk(tree):
        if issue == 1121 and isinstance(node, ast.Subscript):
            start, end = bounds(source, node)
            if any(a <= start and end <= b for a, b in annotation_ranges):
                continue
            if not isinstance(node.ctx, ast.Load) or not isinstance(
                node.slice, ast.Slice
            ):
                continue
            s = node.slice
            if s.step is not None or any(
                x is not None and not simple(x) for x in (s.lower, s.upper)
            ):
                continue
            start, end = bounds(source, s)
            lo = ast.get_source_segment(source, s.lower) if s.lower else ""
            hi = ast.get_source_segment(source, s.upper) if s.upper else ""
            if lo and hi:
                add(start, end, f":{hi}", node.lineno, "omit_lower")
                add(start, end, f"{lo}:", node.lineno, "omit_upper")
            elif lo or hi:
                add(start, end, f":{lo}" if lo else f"{hi}:", node.lineno, "move_bound")
        elif (
            issue == 1124
            and isinstance(node, ast.BinOp)
            and isinstance(node.op, (ast.Div, ast.FloorDiv))
        ):
            left_end, right_start = (
                bounds(source, node.left)[1],
                bounds(source, node.right)[0],
            )
            for token in tokenize.generate_tokens(io.StringIO(source).readline):
                start, end = offset(source, *token.start), offset(source, *token.end)
                if (
                    left_end <= start < end <= right_start
                    and token.type == tokenize.OP
                    and token.string in ("/", "//")
                ):
                    add(
                        start,
                        end,
                        "//" if token.string == "/" else "/",
                        node.lineno,
                        "exchange_division",
                    )
    return result


def invoke(
    argv: list[str], cwd: Path, timeout: int = 5
) -> tuple[subprocess.CompletedProcess, float]:
    started = time.perf_counter()
    result = subprocess.run(
        argv,
        check=False,
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=timeout,
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )
    return result, time.perf_counter() - started


def check(root: Path, source: str, assertions: str) -> dict:
    (root / "subject.py").write_text(source)
    code = (
        "import json\ntry:\n    from subject import *\n"
        f"    exec({assertions!r})\n"
        "except AssertionError:\n    result = {'verdict': 'assertion_failed'}\n"
        "except Exception as error:\n    result = {'verdict': 'error', 'exception': type(error).__name__}\n"
        "else:\n    result = {'verdict': 'passed'}\nprint(json.dumps(result))\n"
    )
    process, elapsed = invoke([sys.executable, "-B", "-c", code], root)
    if process.returncode:
        raise RuntimeError(process.stderr)
    return {**json.loads(process.stdout), "elapsed_seconds": elapsed}


def plan(binary: Path, root: Path, source: str, operators: str | None) -> dict:
    (root / "subject.py").write_text(source)
    command = [
        str(binary),
        "plan",
        "--root",
        str(root),
        "--file",
        "subject.py",
        "--allow-best-effort-memory",
        "--jobs",
        "1",
        "--max-workspace-size",
        "8GiB",
        "--min-free-space",
        "10GiB",
        "--max-mutants",
        "100",
        "--max-candidates",
        "1000",
        "--baseline-timeout",
        "10s",
        "--mutant-timeout",
        "10s",
        "--total-timeout",
        "60s",
    ]
    if operators:
        command.extend(["--operators", operators])
    process, _ = invoke([*command, "--", sys.executable, "-B", "check.py"], root, 45)
    if process.returncode:
        raise RuntimeError(process.stderr)
    saved = json.loads(process.stdout)
    assert not saved["truncated"]
    (root / "plan.json").write_text(process.stdout)
    return saved


def incumbent(saved: dict) -> list[Candidate]:
    return [
        Candidate(
            c["id"],
            c["span"]["start"],
            c["span"]["length"],
            c["original"],
            c["replacement"],
            c["line"],
            c["operator"],
        )
        for c in saved["candidates"]
    ]


def apply(source: str, candidate: Candidate) -> str:
    assert (
        source[candidate.start : candidate.start + candidate.length]
        == candidate.original
    )
    return (
        source[: candidate.start]
        + candidate.replacement
        + source[candidate.start + candidate.length :]
    )


def compare(binary: Path, root: Path, example: Example) -> dict:
    originals = {
        kind: check(root, example.source, checks)
        for kind, checks in (("weak", example.weak), ("strong", example.strong))
    }
    assert all(item["verdict"] == Verdict.PASSED for item in originals.values())
    operators = (
        "structure_slice_neighbor"
        if example.issue == 1121
        else "binary_mul_div,binary_floor_mod"
    )
    saved = plan(binary, root, example.source, operators)
    full_saved = plan(binary, root, example.source, None)
    rows = {}
    for family, pool in (
        ("existing", incumbent(saved)),
        ("existing_full", incumbent(full_saved)),
        ("proposed", experimental_candidates(example.source, example.issue)),
    ):
        results = []
        for c in pool[:2]:
            modified = apply(example.source, c)
            results.append(
                {
                    "candidate": asdict(c),
                    "weak": check(root, modified, example.weak),
                    "strong": check(root, modified, example.strong),
                }
            )
        rows[family] = {
            "pool_count": len(pool),
            "executed": len(results),
            "unexecuted": len(pool) - len(results),
            "results": results,
        }
    return {
        "name": example.name,
        "issue": example.issue,
        "requirement": example.requirement,
        "source": example.source,
        "weak_check": example.weak,
        "strong_check": example.strong,
        "budget_per_family": 2,
        "originals": originals,
        **rows,
    }


SELECTION_SOURCE = (
    "def pair(n):\n    return (n + 1, n * 2)\n\n"
    "def floor_units(n):\n    return n // 2\n\n"
    "def offset_value(n):\n    return n + 10\n"
)
SELECTION_WEAK = (
    "assert pair(0)[1] == 0\nassert floor_units(4) == 2\nassert offset_value(0) == 10\n"
)
REQUIREMENTS = {
    "increment": "assert pair(3)[0] == 4",
    "double": "assert pair(3)[1] == 6",
    "negative_floor": "assert floor_units(-7) == -4",
    "offset": "assert offset_value(3) == 13",
}


def policy_trace(
    order: list[Candidate], outcomes: dict, budget: int, grouping: str | None
) -> dict:
    blocked = set()
    selected, skipped = [], []
    for c in order:
        if len(selected) >= budget:
            break
        key = (c.line,) if grouping == "line" else (c.start, c.length)
        if grouping and key in blocked:
            skipped.append(c.id)
            continue
        selected.append(c.id)
        if grouping and outcomes[c.id]["weak"] == "survived":
            blocked.add(key)
    useful = sorted(
        {
            gap
            for id_ in selected
            if outcomes[id_]["weak"] == "survived"
            for gap in outcomes[id_]["requirements"]
        }
    )
    return {
        "selected_ids": selected,
        "deferred_ids": skipped,
        "unexecuted_ids": [c.id for c in order if c.id not in selected],
        "executed_count": len(selected),
        "useful_requirements": useful,
        "useful_survivors": [
            i
            for i in selected
            if outcomes[i]["weak"] == "survived" and outcomes[i]["requirements"]
        ],
    }


def selection(binary: Path, root: Path) -> dict:
    (root / "check.py").write_text("from subject import *\n" + SELECTION_WEAK)
    saved = plan(
        binary,
        root,
        SELECTION_SOURCE,
        "binary_add_sub,binary_mul_div,binary_floor_mod,integer_literal_neighbor",
    )
    process, _ = invoke(
        [
            str(binary),
            "verify",
            str(root / "plan.json"),
            "--top",
            "100",
            "--selection-policy",
            "line-diverse",
            "--dry-run",
        ],
        root,
        45,
    )
    if process.returncode:
        raise RuntimeError(process.stderr)
    preview = json.loads(process.stdout)
    # Match the public preview IDs to the immutable saved candidates.
    ids = [c["id"] for c in preview["candidates"]]
    by_id = {c.id: c for c in incumbent(saved)}
    order = [by_id[id_] for id_ in ids]
    outcomes = {}
    for name, checks in REQUIREMENTS.items():
        assert check(root, SELECTION_SOURCE, checks)["verdict"] == Verdict.PASSED, name
    assert check(root, SELECTION_SOURCE, SELECTION_WEAK)["verdict"] == Verdict.PASSED
    for c in order:
        modified = apply(SELECTION_SOURCE, c)
        weak = check(root, modified, SELECTION_WEAK)
        outcomes[c.id] = {
            "weak": "survived"
            if weak["verdict"] == Verdict.PASSED
            else weak["verdict"],
            "requirements": [
                name
                for name, checks in REQUIREMENTS.items()
                if check(root, modified, checks)["verdict"] == Verdict.ASSERTION
            ],
        }
    comparisons = {
        str(budget): {
            name: policy_trace(order, outcomes, budget, grouping)
            for name, grouping in (
                ("line_diverse", None),
                ("stop_line", "line"),
                ("stop_span", "span"),
            )
        }
        for budget in (4, 8, 12)
    }
    # Replay through the real CLI. Adaptive decisions use only completed CLI
    # outcomes, not the exhaustive direct-probe oracle above.
    (root / "subject.py").write_text(SELECTION_SOURCE)

    def verify(arguments: list[str]) -> dict:
        process, elapsed = invoke(
            [str(binary), "verify", str(root / "plan.json"), *arguments], root, 75
        )
        if process.returncode:
            raise RuntimeError(process.stderr)
        events = [json.loads(line) for line in process.stdout.splitlines()]
        finished = next(e for e in events if e["kind"] == "run_finished")
        assert finished["complete"] and finished["counts"]["not_run"] == 0
        mutants = [e for e in events if e["kind"] == "mutant_finished"]
        for e in mutants:
            expected = outcomes[e["candidate"]["id"]]["weak"]
            assert e["status"] == (
                "killed" if expected == Verdict.ASSERTION else expected
            )
        return {
            "elapsed_seconds": elapsed,
            "counts": finished["counts"],
            "disk": finished["disk"],
            "complete": finished["complete"],
            "mutants": [
                {"id": e["candidate"]["id"], "status": e["status"]} for e in mutants
            ],
        }

    cli = {
        "line_diverse": [verify(["--top", "12", "--selection-policy", "line-diverse"])]
    }
    for name, grouping in (("stop_line", "line"), ("stop_span", "span")):
        blocked, runs = set(), []
        for c in order:
            key = (c.line,) if grouping == "line" else (c.start, c.length)
            if key in blocked:
                continue
            run = verify(["--candidate", c.id])
            assert len(run["mutants"]) == 1
            runs.append(run)
            if run["mutants"][0]["status"] == "survived":
                blocked.add(key)
        cli[name] = runs
    for name, runs in cli.items():
        assert [m["id"] for run in runs for m in run["mutants"]] == comparisons["12"][
            name
        ]["selected_ids"]
    return {
        "source": SELECTION_SOURCE,
        "weak_check": SELECTION_WEAK,
        "requirements": REQUIREMENTS,
        "pool_count": len(order),
        "ordered_candidates": [asdict(c) for c in order],
        "outcomes": outcomes,
        "comparisons": comparisons,
        "cli_verify_runs_at_budget_12": cli,
    }


def controls(root: Path) -> dict:
    excluded = {
        "step": "xs[1:3:2]",
        "multidimensional": "xs[1:3, 2:4]",
        "call_bound": "xs[start():3]",
        "empty": "xs[:]",
        "store": "xs[1:3] = []",
        "delete": "del xs[1:3]",
        "annotation": "x: xs[1:3]",
    }
    for name, source in excluded.items():
        assert experimental_candidates(source, 1121) == [], name
    # Statuses other than a survivor must not suppress a group.
    group = [Candidate(str(i), 0, 1, "+", "-", 1, "control") for i in range(2)]
    for verdict in (
        "timeout",
        "error",
        "cancelled",
        "resource_stop",
        "assertion_failed",
    ):
        outcomes = {c.id: {"weak": verdict, "requirements": []} for c in group}
        assert policy_trace(group, outcomes, 2, "line")["executed_count"] == 2
    evaluation = (
        "calls = []\ndef operand(name, value):\n    calls.append(name)\n    return value\n"
        "def divide():\n    return operand('left', -7) / operand('right', 2)\n"
    )
    exchanged = experimental_candidates(evaluation, 1124)
    assert len(exchanged) == 1
    for source, value in ((evaluation, -3.5), (apply(evaluation, exchanged[0]), -4)):
        checks = f"assert divide() == {value!r}\nassert calls == ['left', 'right']"
        assert check(root, source, checks)["verdict"] == Verdict.PASSED
    assert (
        check(root, "", "assert type(7 / 2) is float\nassert type(7 // 2) is int")[
            "verdict"
        ]
        == Verdict.PASSED
    )
    for example in EXAMPLES:
        if example.name not in ("true_division", "floor_division"):
            continue
        expected = -3.5 if example.name == "true_division" else -4
        negative_check = f"assert divide(-7, 2) == {expected}"
        assert check(root, example.source, negative_check)["verdict"] == Verdict.PASSED
        c = experimental_candidates(example.source, 1124)[0]
        assert (
            check(root, apply(example.source, c), negative_check)["verdict"]
            == Verdict.ASSERTION
        )
    negative = "def cut(xs):\n    return xs[-2:]\n"
    c = experimental_candidates(negative, 1121)[0]
    negative_check = "assert cut([1, 2, 3, 4]) == [3, 4]"
    assert check(root, negative, negative_check)["verdict"] == Verdict.PASSED
    assert (
        check(root, apply(negative, c), negative_check)["verdict"] == Verdict.ASSERTION
    )
    return {
        "excluded_slices": list(excluded),
        "non_survivor_statuses_do_not_suppress": True,
        "division_evaluates_operands_once_left_to_right": True,
        "division_result_types_differ": True,
        "negative_fraction_checks_kill_each_exchange": True,
        "negative_slice_bound_checked": True,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hoimin", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.hoimin.resolve()
    with binary.open("rb") as stream:
        binary_hash = hashlib.file_digest(stream, "sha256").hexdigest()
    started = time.perf_counter()
    with tempfile.TemporaryDirectory(
        prefix="hoimin-issues-1121-1122-1124-"
    ) as directory:
        root = Path(directory)
        assert all(shutil.disk_usage(p).free > 10 * 1024**3 for p in (root, Path.cwd()))
        result = {
            "python": sys.version,
            "platform": platform.platform(),
            "binary_sha256": binary_hash,
            "source_revision": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], text=True
            ).strip(),
            "controls": controls(root),
            "operator_comparisons": [compare(binary, root, e) for e in EXAMPLES],
            "selection": selection(binary, root),
        }
        result["elapsed_seconds"] = time.perf_counter() - started
        result["temporary_file_bytes_at_end"] = sum(
            p.stat().st_size for p in root.rglob("*") if p.is_file()
        )
    assert not root.exists()
    result["temporary_directory_removed"] = True
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
