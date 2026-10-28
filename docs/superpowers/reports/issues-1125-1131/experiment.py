"""Disposable research probes; not a product operator or general-purpose runner."""

import argparse
import ast
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
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
        "continue_rejection",
        1125,
        "def run(values):\n    events = []\n    for value in values:\n        if value < 0:\n            events.append((value, 'rejected'))\n            continue\n        events.append((value, 'accepted'))\n    return events\n",
        "assert (-2, 'rejected') in run([-2,3])\nassert (3, 'accepted') in run([-2,3])",
        "assert run([-2,3]) == [(-2,'rejected'),(3,'accepted')]",
        "Rejected records must never be accepted; later valid records must still run.",
    ),
    Example(
        "continue_tail_equivalent",
        1125,
        "def run(values):\n    seen=[]\n    for value in values:\n        seen.append(value)\n        continue\n    return seen\n",
        "assert run([])==[]",
        "assert run([2,5])==[2,5]",
        "Retain all records; removing a tail continue has no effect.",
    ),
    Example(
        "factory_isolation",
        1126,
        "from dataclasses import dataclass, field\n@dataclass\nclass Bag:\n    entries: list = field(default_factory=list)\n",
        "assert Bag().entries == []",
        "a,b=Bag(),Bag()\na.entries.append('added')\nassert b.entries == []",
        "Each bag owns its entries; changes must not leak to another instance.",
    ),
    Example(
        "factory_read_only_control",
        1126,
        "from dataclasses import dataclass, field\n@dataclass\nclass Bag:\n    entries: list = field(default_factory=list)\n",
        "assert Bag().entries == []",
        "assert [Bag().entries for _ in range(3)] == [[],[],[]]",
        "Read-only bags need empty entries; identity/independent mutation is not an API promise.",
    ),
    Example(
        "sort_priority",
        1127,
        "def run(rows):\n    return sorted(rows, key=lambda r: (r[0], r[1]))\n",
        "assert run([(2,2),(1,1)]) == [(1,1),(2,2)]",
        "assert run([(2,1),(1,9),(1,3)]) == [(1,3),(1,9),(2,1)]",
        "Sort by first column, then second column for equal first values.",
    ),
    Example(
        "sort_correlated_control",
        1127,
        "def run(rows):\n    return sorted(rows, key=lambda r: (r[0], r[1]))\n",
        "assert run([])==[]",
        "assert run([(3,3),(1,1),(2,2)])==[(1,1),(2,2),(3,3)]",
        "Allowed rows have equal column values; either key order is equivalent on this domain.",
    ),
    Example(
        "lambda_capture",
        1128,
        "def run(values):\n    callbacks=[]\n    for value in values:\n        callbacks.append(lambda value=value: value)\n    return [callback() for callback in callbacks]\n",
        "assert run([4])==[4]",
        "assert run([4,9])==[4,9]",
        "Callbacks retain the distinct value at construction, even after loop completion.",
    ),
    Example(
        "lambda_explicit_argument",
        1128,
        "def run(values):\n    callbacks=[]\n    for value in values:\n        callbacks.append(lambda value=value: value)\n    return [callback(7) for callback in callbacks]\n",
        "assert run([])==[]",
        "assert run([4,9])==[7,7]",
        "Callbacks accept explicit overrides; dropping the parameter breaks this call interface.",
    ),
    Example(
        "comparison_delegation",
        1129,
        "class Token:\n    def __eq__(self, other):\n        if isinstance(other, Token):\n            return True\n        return NotImplemented\nclass Compatible:\n    def __eq__(self, other):\n        return isinstance(other, Token)\n",
        "assert Token()==Token()",
        "assert Token()==Compatible()",
        "Unsupported comparisons delegate to the compatible right operand.",
    ),
    Example(
        "comparison_no_peer_control",
        1129,
        "class Token:\n    def __eq__(self, other):\n        if isinstance(other, Token):\n            return True\n        return NotImplemented\n",
        "assert Token()==Token()",
        "assert (Token()==object()) is False",
        "Unrelated objects compare unequal; no peer supports this type.",
    ),
    Example(
        "cause_diagnostic",
        1130,
        "def run():\n    try:\n        int('invalid')\n    except ValueError as source:\n        raise LookupError('invalid record') from source\n",
        "try:\n    run()\nexcept LookupError as e:\n    assert str(e)=='invalid record'\nelse:\n    assert False",
        "try:\n    run()\nexcept LookupError as e:\n    assert isinstance(e.__cause__,ValueError)\nelse:\n    assert False",
        "The public diagnostic API preserves the original parse failure as an explicit cause.",
    ),
    Example(
        "cause_private_control",
        1130,
        "def run():\n    try:\n        int('invalid')\n    except ValueError as source:\n        raise LookupError('invalid record') from source\n",
        "try:\n    run()\nexcept LookupError:\n    pass\nelse:\n    assert False",
        "try:\n    run()\nexcept LookupError as e:\n    assert e.args==('invalid record',)\nelse:\n    assert False",
        "Only public error type and message are contractual; exception chaining is private.",
    ),
    Example(
        "encoding_roundtrip",
        1131,
        "def run(data):\n    return data.decode('utf-8', errors='surrogateescape').encode('utf-8', errors='surrogateescape')\n",
        "assert run(b'abc')==b'abc'",
        "assert run(b'a\\xffb')==b'a\\xffb'",
        "Arbitrary byte payloads round-trip without data loss, including undecodable bytes.",
    ),
    Example(
        "encoding_ascii_control",
        1131,
        "def run(data):\n    return data.decode('utf-8', errors='surrogateescape').encode('utf-8', errors='surrogateescape')\n",
        "assert run(b'abc')==b'abc'",
        "assert run(b'012 xyz')==b'012 xyz'",
        "Inputs are restricted to ASCII; the error policy has no observable effect.",
    ),
)
INCUMBENTS = {
    1125: "break_continue",
    1126: "mutable_default_share",
    1127: "return_tuple_swap,container_element_delete",
    1128: "nonlocal_remove,mutable_default_share",
    1129: "compare_eq_ne,function_body_return_constant",
    1130: "exception_type_pair",
    1131: "string_literal_empty",
}


def offset(source, line, column):
    assert source.isascii()
    return sum(map(len, source.splitlines(keepends=True)[: line - 1])) + column


def bounds(source, node):
    return offset(source, node.lineno, node.col_offset), offset(
        source, node.end_lineno, node.end_col_offset
    )


def experimental_candidates(source, issue):
    # Fixed corpus probes, not production eligibility or name-resolution logic.
    result = []

    def add(node, replacement, operator):
        start, end = bounds(source, node)
        ast.parse(source[:start] + replacement + source[end:])
        result.append(
            Candidate(
                f"probe-{len(result) + 1}",
                start,
                end - start,
                source[start:end],
                replacement,
                node.lineno,
                operator,
            )
        )

    for node in ast.walk(ast.parse(source)):
        if issue == 1125 and isinstance(node, ast.Continue):
            add(node, "pass", "continue_remove")
        elif issue == 1126 and isinstance(node, ast.Call):
            for keyword in node.keywords:
                if (
                    keyword.arg == "default_factory"
                    and isinstance(keyword.value, ast.Name)
                    and keyword.value.id == "list"
                ):
                    add(
                        keyword.value,
                        "lambda shared=[]: shared",
                        "dataclass_factory_share",
                    )
        elif (
            issue == 1127
            and isinstance(node, ast.Lambda)
            and isinstance(node.body, ast.Tuple)
            and len(node.body.elts) == 2
        ):
            a, b = node.body.elts
            add(
                node.body,
                f"({ast.get_source_segment(source, b)}, {ast.get_source_segment(source, a)})",
                "sort_key_swap",
            )
        elif (
            issue == 1128
            and isinstance(node, ast.Lambda)
            and len(node.args.args) == len(node.args.defaults) == 1
        ):
            add(
                node,
                f"lambda: {ast.get_source_segment(source, node.body)}",
                "lambda_capture_remove",
            )
        elif (
            issue == 1129
            and isinstance(node, ast.Return)
            and isinstance(node.value, ast.Name)
            and node.value.id == "NotImplemented"
        ):
            add(node.value, "False", "comparison_not_implemented")
        elif issue == 1130 and isinstance(node, ast.Raise) and node.cause is not None:
            add(node.cause, "None", "exception_cause_remove")
        elif issue == 1131 and isinstance(node, ast.Call):
            for keyword in node.keywords:
                if (
                    keyword.arg == "errors"
                    and isinstance(keyword.value, ast.Constant)
                    and keyword.value.value == "surrogateescape"
                ):
                    add(keyword.value, "'ignore'", "encoding_errors_ignore")
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
    operators = INCUMBENTS[example.issue]
    started = time.perf_counter()
    saved = plan(binary, root, example.source, operators)
    direct_seconds = time.perf_counter() - started
    started = time.perf_counter()
    full_saved = plan(binary, root, example.source, None)
    full_seconds = time.perf_counter() - started
    started = time.perf_counter()
    proposed = experimental_candidates(example.source, example.issue)
    prototype_seconds = time.perf_counter() - started
    rows = {}
    for family, pool in (
        ("existing", incumbent(saved)),
        ("existing_full", incumbent(full_saved)),
        ("proposed", proposed),
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
        "generation_elapsed_seconds": {
            "existing": direct_seconds,
            "existing_full": full_seconds,
            "proposed": prototype_seconds,
        },
        "originals": originals,
        **rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hoimin", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binary = args.hoimin.resolve()
    started = time.perf_counter()
    with binary.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    with tempfile.TemporaryDirectory(prefix="hoimin-1125-1131-") as directory:
        root = Path(directory)
        assert (
            min(shutil.disk_usage(root).free, shutil.disk_usage(Path.cwd()).free)
            > 10 * 1024**3
        )
        (root / "check.py").write_text("import subject\n")
        result = {
            "python": sys.version,
            "platform": platform.platform(),
            "source_revision": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], text=True
            ).strip(),
            "binary_sha256": digest,
            "operator_comparisons": [compare(binary, root, e) for e in EXAMPLES],
        }
        result["temporary_file_bytes_at_end"] = sum(
            p.stat().st_size for p in root.rglob("*") if p.is_file()
        )
    result["temporary_directory_removed"] = not root.exists()
    result["elapsed_seconds"] = time.perf_counter() - started
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
