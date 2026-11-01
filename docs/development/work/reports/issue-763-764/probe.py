# ruff: noqa: INP001 -- Standalone, unsupported research probe.
"""Finite reviewed pure-function experiment; not a product mutation runner."""

from __future__ import annotations

import ast
import gzip
import hashlib
import io
import itertools
import json
import random
import time
import tokenize
from collections.abc import Callable
from dataclasses import asdict, dataclass
from fractions import Fraction
from pathlib import Path
from typing import cast

ROOT = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
SOURCES = ROOT / ".superpowers/issue-763-764-sources"
MAX_EDITS = 16
ARM_SECONDS = 60
TOTAL_SECONDS = 540
SEEDS = (763, 764, 765)
SWAPS = {
    "*": ("/",),
    "/": ("*",),
    "+": ("-",),
    "-": ("+",),
    "==": ("!=",),
    "and": ("or",),
    "or": ("and",),
    "in": ("not in",),
    "min": ("max",),
}


@dataclass(frozen=True)
class Edit:
    start: int
    end: int
    original: str
    replacement: str
    family: str


@dataclass(frozen=True)
class Case:
    args: tuple[object, ...]
    expected: object
    label: str
    threshold: float | None = None


@dataclass
class Subject:
    source: str
    name: str
    namespace: dict[str, object]
    cases: list[Case]
    validation: list[Case]
    edits: list[Edit]


@dataclass
class Observation:
    edits: list[int]
    killed: list[int]
    validation_killed: list[int]
    raised: dict[str, str]
    elapsed_seconds: float
    stable: bool
    source_sha256: str


def relations(left: set[int], right: set[int], combined: set[int]) -> dict[str, bool]:
    intersection = left & right
    return {
        "ssh": bool(combined) and combined <= intersection,
        "strict": bool(combined) and combined < intersection,
    }


def apply_edits(source: str, edits: list[Edit]) -> str:
    raw = source.encode("utf-8")
    ordered = sorted(edits, key=lambda edit: edit.start)
    previous = 0
    pieces = []
    for edit in ordered:
        if edit.start < previous:
            msg = "overlapping edits"
            raise ValueError(msg)
        if not 0 <= edit.start < edit.end <= len(raw):
            msg = "invalid source span"
            raise ValueError(msg)
        if raw[edit.start : edit.end] != edit.original.encode("utf-8"):
            msg = "stale source span"
            raise ValueError(msg)
        pieces.extend((raw[previous : edit.start], edit.replacement.encode("utf-8")))
        previous = edit.end
    pieces.append(raw[previous:])
    return b"".join(pieces).decode("utf-8")


def function_source(path: Path, name: str) -> str:
    text = path.read_text(encoding="utf-8")
    node = next(
        node
        for node in ast.parse(text).body
        if isinstance(node, ast.FunctionDef) and node.name == name
    )
    assert node.end_lineno is not None
    return "".join(text.splitlines(keepends=True)[node.lineno - 1 : node.end_lineno])


def candidates(source: str) -> list[Edit]:
    lines = source.splitlines(keepends=True)
    edits = []
    for token in tokenize.generate_tokens(io.StringIO(source).readline):
        replacements = SWAPS.get(token.string, ())
        if token.type == tokenize.NUMBER and token.string.isdecimal():
            value = int(token.string)
            replacements = (str(value + 1), str(value - 1))
        start = sum(len(line.encode()) for line in lines[: token.start[0] - 1])
        start += (
            len(lines[token.start[0] - 1][: token.start[1]].encode())
            if token.start[0] <= len(lines)
            else 0
        )
        edits.extend(
            Edit(
                start,
                start + len(token.string.encode()),
                token.string,
                replacement,
                token.string,
            )
            for replacement in replacements
        )
    assert len(edits) <= MAX_EDITS, "candidate cap exceeded; do not silently truncate"
    return edits


def load_function(
    source: str, name: str, namespace: dict[str, object]
) -> Callable[..., object]:
    scope = dict(namespace)
    # Only the two reviewed, fixed functions and our finite arithmetic examples.
    exec(compile(source, "<reviewed-research-function>", "exec"), scope)  # noqa: S102 -- No arbitrary source input or product ingress.
    return cast("Callable[..., object]", scope[name])


def outcome(function: Callable[..., object], case: Case) -> tuple[bool, str | None]:
    try:
        value = function(*case.args)
        if case.threshold is not None:
            value = cast("float", value) > case.threshold
    except ArithmeticError as error:
        # A function raising instead of returning violates the explicit assertion.
        return False, type(error).__name__
    return value == case.expected, None


def observe(
    subject: Subject,
    indices: list[int],
) -> Observation:
    started = time.perf_counter()
    mutated = apply_edits(subject.source, [subject.edits[index] for index in indices])
    function = load_function(mutated, subject.name, subject.namespace)
    results = [
        outcome(function, case) for case in [*subject.cases, *subject.validation]
    ]
    repeated = [
        outcome(function, case) for case in [*subject.cases, *subject.validation]
    ]
    return Observation(
        indices,
        [
            index
            for index, result in enumerate(results[: len(subject.cases)])
            if not result[0]
        ],
        [
            index
            for index, result in enumerate(results[len(subject.cases) :])
            if not result[0]
        ],
        {
            str(index): error
            for index, (_, error) in enumerate(results)
            if error is not None
        },
        time.perf_counter() - started,
        results == repeated,
        hashlib.sha256(mutated.encode()).hexdigest(),
    )


def eligible_pairs(edits: list[Edit]) -> list[tuple[int, int]]:
    return [
        (left, right)
        for left, right in itertools.combinations(range(len(edits)), 2)
        if edits[left].end <= edits[right].start
    ]


def run_arm(
    mode: str,
    seed: int,
    subject: Subject,
    deadline: float,
) -> dict[str, object]:
    started = time.perf_counter()
    limit = min(started + ARM_SECONDS, deadline)
    first = []
    for index in range(len(subject.edits)):
        assert time.perf_counter() < limit, "arm budget exhausted"
        first.append(observe(subject, [index]))
    guidance_seconds = time.perf_counter() - started
    pairs = eligible_pairs(subject.edits)
    rng = random.Random(seed)  # noqa: S311 -- Reproducible research ordering, not security.
    rng.shuffle(pairs)
    if mode == "guided":
        pairs.sort(
            key=lambda pair: (
                -len(set(first[pair[0]].killed) & set(first[pair[1]].killed)),
                subject.edits[pair[1]].start - subject.edits[pair[0]].end,
            )
        )
    if mode == "first":
        pairs = []
    second = []
    for left, right in pairs:
        assert time.perf_counter() < limit, "arm budget exhausted"
        second.append(observe(subject, [left, right]))
    assert all(row.stable for row in [*first, *second]), "unstable observations"
    classified = []
    for row in second:
        left, right = row.edits
        flags = relations(
            set(first[left].killed), set(first[right].killed), set(row.killed)
        )
        classified.append(
            {
                "edits": row.edits,
                **flags,
                "survived": not row.killed,
                "validation_witnesses": row.validation_killed,
                "both_constituents_killed": bool(first[left].killed)
                and bool(first[right].killed),
            }
        )
    return {
        "mode": mode,
        "seed": seed,
        "first_order_seconds_including_validation": guidance_seconds,
        "elapsed_seconds_including_validation": time.perf_counter() - started,
        "exhausted": True,
        "first": [asdict(row) for row in first],
        "second": [asdict(row) for row in second],
        "classification": classified,
    }


def path_cases() -> tuple[list[Case], list[Case]]:
    all_categories = ("python", "rust", "formal", "distribution")
    existing = [
        ("README.md", ("distribution",)),
        ("docs/usage.md", ()),
        ("docs/cli-reference.md", ("rust",)),
        ("docs/audits/note.md", ()),
        ("crates/hoimin-cli/src/main.rs", ("rust", "formal", "distribution")),
        ("vendor/ruff_python_parser/src/lib.rs", ("rust", "formal", "distribution")),
        ("tests/test_example.py", ("python", "rust", "formal")),
        ("formal/HoiminOracle/Main.lean", ("formal", "rust")),
        ("tools/release.py", ("python", "distribution")),
        ("tests/wheel_smoke.py", all_categories),
        ("LICENSE", ("distribution",)),
        ("docs/json-schema/report.json", all_categories),
        ("docs/audits/repro.py", all_categories),
        ("tests/fixtures/case.json", all_categories),
        (".github/workflows/ci.yml", all_categories),
        ("Cargo.lock", all_categories),
        ("rust-toolchain.toml", all_categories),
        ("uv.lock", all_categories),
        ("unknown-file", all_categories),
        ("tools/docs_site.py", ()),
        ("tests/test_docs_site.py", ()),
        ("mkdocs.yml", ()),
        (".github/workflows/documentation.yml", ()),
    ]
    validation = []
    groups = [
        ("docs/", "md", ()),
        ("docs/", "py", all_categories),
        ("docs/json-schema/", "md", all_categories),
        ("tests/fixtures/", "py", all_categories),
        ("tests/", "py", ("python", "rust", "formal")),
        ("tests/", "md", all_categories),
        ("crates/", "md", ("rust", "formal", "distribution")),
        ("vendor/", "py", ("rust", "formal", "distribution")),
        ("formal/", "md", ("formal", "rust")),
        ("unclassified/", "md", all_categories),
    ]
    for prefix, suffix, expected in groups:
        for stem in ("a", "nested/a", "α", "cli-reference"):
            path = f"{prefix}{stem}.{suffix}"
            # This exact reference path has an explicit Rust-check policy.
            required = ("rust",) if path == "docs/cli-reference.md" else expected
            validation.append(Case((path,), required, path))
    validation.append(Case(("CONTRIBUTING.md",), (), "CONTRIBUTING.md"))
    return [Case((path,), expected, path) for path, expected in existing], validation


def ratio_cases() -> tuple[list[Case], list[Case]]:
    cases = [
        Case(("abc", "abcdef"), 2 / 3, "direct_total_length"),
        Case(
            ("サーバ", "サーバー"),
            expected=True,
            label="known_variant_not_pruned",
            threshold=0.85,
        ),
        Case(
            ("a" * 17, "a" * 17 + "b" * 6),
            expected=False,
            label="exact_threshold_skipped",
            threshold=0.85,
        ),
        Case(
            ("a" * 17, "a" * 16 + "c"),
            expected=True,
            label="later_match_not_pruned",
            threshold=0.85,
        ),
    ]
    validation = [
        Case(
            ("a" * left, "b" * right),
            float(Fraction(left + right - abs(left - right), left + right)),
            f"lengths_{left}_{right}",
        )
        for left in range(10)
        for right in range(10)
        if left + right
    ]
    return cases, validation


def artificial() -> dict[str, object]:
    source = "def count(x, y):\n    return int(x == 0) + int(y == 0)\n"
    edits = [
        edit
        for edit in candidates(source)
        if edit.original == "0" and edit.replacement == "1"
    ]
    cases = [Case((0, 1), 1, "left"), Case((1, 0), 1, "right")]
    validation = [Case((0, 0), 2, "both")]
    subject = Subject(source, "count", {}, cases, validation, edits)
    rows = [observe(subject, indices) for indices in ([0], [1], [0, 1])]
    initial = relations(set(rows[0].killed), set(rows[1].killed), set(rows[2].killed))
    expanded_sets = [
        set(row.killed) | {len(cases) + index for index in row.validation_killed}
        for row in rows
    ]
    expanded = relations(*expanded_sets)
    assert initial == {"ssh": False, "strict": False}
    assert expanded == {"ssh": True, "strict": True}
    assert rows[2].killed == []
    assert rows[2].validation_killed == [0]
    cancellation = "def cancel(x):\n    return x + 1 - 1\n"
    constant_edits = [
        edit
        for edit in candidates(cancellation)
        if edit.original == "1" and edit.replacement == "2"
    ]
    cancel_subject = Subject(
        cancellation,
        "cancel",
        {},
        [Case((x,), x, str(x)) for x in range(-4, 5)],
        [],
        constant_edits,
    )
    cancel_rows = [observe(cancel_subject, indices) for indices in ([0], [1], [0, 1])]
    assert cancel_rows[2].killed == []
    return {
        "count_source": source,
        "count_edits": [asdict(edit) for edit in edits],
        "count_rows": [asdict(row) for row in rows],
        "initial_relation": initial,
        "expanded_relation": expanded,
        "cancel_source": cancellation,
        "cancel_edits": [asdict(edit) for edit in constant_edits],
        "cancel_rows": [asdict(row) for row in cancel_rows],
    }


def main() -> None:
    # Standalone root setup occurs before main; import only trusted constants.
    from tools import ci_selection  # noqa: PLC0415

    started = time.perf_counter()
    targets: list[
        tuple[Path, str, dict[str, object], tuple[list[Case], list[Case]]]
    ] = [
        (
            SOURCES / "src__proofreadtools__checks___variant_candidates.py",
            "_length_ratio_upper_bound",
            {},
            ratio_cases(),
        ),
        (
            ROOT / "tools/ci_selection.py",
            "categories_for",
            {
                "CATEGORIES": ci_selection.CATEGORIES,
                "DOCUMENTATION_INPUTS": ci_selection.DOCUMENTATION_INPUTS,
                "RELEASE_TOOLS": ci_selection.RELEASE_TOOLS,
            },
            path_cases(),
        ),
    ]
    evidence: dict[str, object] = {"artificial": artificial(), "targets": []}
    target_results = []
    for path, name, namespace, (cases, validation) in targets:
        generation_started = time.perf_counter()
        source = function_source(path, name)
        edits = candidates(source)
        subject = Subject(source, name, namespace, cases, validation, edits)
        generation_seconds = time.perf_counter() - generation_started
        baseline = observe(subject, [])
        assert baseline.stable
        assert not baseline.killed
        assert not baseline.validation_killed
        arms = [
            run_arm(mode, seed, subject, started + TOTAL_SECONDS)
            for seed in SEEDS
            for mode in ("first", "random", "guided")
        ]
        target_results.append(
            {
                "name": name,
                "file_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "source": source,
                "edits": [asdict(edit) for edit in edits],
                "cases": [asdict(case) for case in cases],
                "validation_cases": [asdict(case) for case in validation],
                "generation_seconds": generation_seconds,
                "baseline": asdict(baseline),
                "arms": arms,
            }
        )
    evidence.update(
        targets=target_results, elapsed_seconds=time.perf_counter() - started
    )
    (HERE / "evidence.json.gz").write_bytes(
        gzip.compress(
            (json.dumps(evidence, ensure_ascii=False, indent=2) + "\n").encode(),
            mtime=0,
        )
    )


if __name__ == "__main__":
    import sys

    sys.path.insert(0, str(ROOT))
    main()
