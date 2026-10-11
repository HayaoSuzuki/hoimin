"""Development-only experiment for the pinned proofreadtools length-ratio function.

Extracted functions accelerate exploration; only independent real-module executions
through the Rust worker establish evidence. This is not a Python security sandbox.
"""

from __future__ import annotations

import argparse
import ast
import importlib
import json
import math
import sys
import time
from collections.abc import Callable
from pathlib import Path
from typing import TypedDict, cast

from hypothesis import Phase, given, seed, settings
from hypothesis import strategies as st

SYMBOL = "_length_ratio_upper_bound"
MAX_LENGTH = 32
ARGUMENT_COUNT = 2
MODULE = "proofreadtools.checks._variant_candidates"


class SearchResult(TypedDict):
    status: str
    args: list[str]
    original: float
    mutant: float
    attempts: int
    elapsed: float


class BudgetExpiredError(Exception):
    pass


class InvalidObservationError(Exception):
    pass


def extracted(source: str) -> Callable[[str, str], float]:
    tree = ast.parse(source)
    function = next(
        node
        for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name == SYMBOL
    )
    if function.decorator_list or len(function.args.args) != ARGUMENT_COUNT:
        msg = "only the fixed undecorated two-argument function is supported"
        raise ValueError(msg)
    namespace: dict[str, object] = {
        "__builtins__": {
            "len": len,
            "min": min,
            "max": max,
            "float": float,
            "str": str,
        },
    }
    # Only developer-owned source is executed, under Rust ProcessHandler limits.
    exec(  # noqa: S102 -- fixed developer-owned source, isolated bounded experiment.
        compile(
            ast.Module(body=[function], type_ignores=[]), "<extracted-probe>", "exec"
        ),
        namespace,
    )
    return cast("Callable[[str, str], float]", namespace[SYMBOL])


def observation(function: Callable[[str, str], float], x: str, y: str) -> float:
    try:
        value = function(x, y)
    except (ArithmeticError, ValueError, TypeError, NameError) as error:
        raise InvalidObservationError from error
    if type(value) is not float or not math.isfinite(value):
        raise InvalidObservationError
    return value


def find_witness(
    original_source: str, mutant_source: str, seed_value: int, seconds: float
) -> SearchResult:
    started = time.monotonic()
    original, mutant = extracted(original_source), extracted(mutant_source)
    result: SearchResult = {
        "status": "no_witness_within_budget",
        "args": [],
        "original": 0.0,
        "mutant": 0.0,
        "attempts": 0,
        "elapsed": 0.0,
    }

    @seed(seed_value)
    @settings(
        max_examples=256,
        deadline=None,
        database=None,
        phases=(Phase.generate, Phase.shrink),
    )
    @given(
        st.text(alphabet="ab", min_size=1, max_size=MAX_LENGTH),
        st.text(alphabet="ab", min_size=1, max_size=MAX_LENGTH),
    )
    def compare(x: str, y: str) -> None:
        if time.monotonic() - started >= seconds:
            raise BudgetExpiredError
        result["attempts"] += 1
        left, right = observation(original, x, y), observation(mutant, x, y)
        if left != right:
            result.update(status="witness", args=[x, y], original=left, mutant=right)
        assert left == right

    try:
        compare()
    except AssertionError:
        pass
    except BudgetExpiredError:
        pass
    except InvalidObservationError:
        result.update(status="unsupported_observation", args=[])
    result["elapsed"] = time.monotonic() - started
    return result


def render_regression(args: tuple[str, str], expected: float) -> str:
    if not all(
        type(arg) is str and arg.isascii() and 1 <= len(arg) <= MAX_LENGTH
        for arg in args
    ):
        msg = "arguments must be within the fixed ASCII domain"
        raise ValueError(msg)
    if type(expected) is not float or not math.isfinite(expected):
        raise InvalidObservationError
    return (
        "# Characterization candidate: review against the intended specification.\n"
        "from proofreadtools.checks._variant_candidates import "
        "_length_ratio_upper_bound\n\n"
        "def test_generated_length_ratio_regression():\n"
        f"    assert _length_ratio_upper_bound({args[0]!r}, {args[1]!r}) "
        f"== {expected!r}\n"
    )


def write_evidence(path: Path, evidence: object) -> None:
    with path.open("x", encoding="utf-8") as stream:
        json.dump(evidence, stream, ensure_ascii=True, allow_nan=False)
        stream.write("\n")


def crosshair_probe(original_source: str, mutant_source: str) -> str:
    functions = []
    for name, source in [("original", original_source), ("mutant", mutant_source)]:
        tree = ast.parse(source)
        function = next(
            node
            for node in tree.body
            if isinstance(node, ast.FunctionDef) and node.name == SYMBOL
        )
        function.name = name
        # ASCII content is immaterial to this fixed function; CrossHair's broader
        # character domain is projected back to "a" * len(s) before real replay.
        function.body.insert(
            0,
            ast.Expr(
                value=ast.Constant(
                    value="pre: 1 <= len(x) <= 32\npre: 1 <= len(y) <= 32"
                )
            ),
        )
        functions.append(ast.unparse(ast.fix_missing_locations(function)))
    return "\n\n".join(functions) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["search", "observe", "crosshair-probe"])
    parser.add_argument("--original", type=Path)
    parser.add_argument("--mutant", type=Path)
    parser.add_argument("--seed", type=int, default=790)
    parser.add_argument("--seconds", type=float, default=2.0)
    parser.add_argument("--x", default="a")
    parser.add_argument("--y", default="aa")
    args = parser.parse_args()
    if args.mode == "observe":
        module = importlib.import_module(MODULE)
        sys.stdout.write(
            json.dumps(
                {
                    "value": getattr(module, SYMBOL)(args.x, args.y),
                    "path": module.__file__,
                },
                allow_nan=False,
            )
        )
        return
    if args.original is None or args.mutant is None:
        parser.error("--original and --mutant are required")
    original = args.original.read_text(encoding="utf-8")
    mutant = args.mutant.read_text(encoding="utf-8")
    if args.mode == "crosshair-probe":
        sys.stdout.write(crosshair_probe(original, mutant))
    else:
        sys.stdout.write(
            json.dumps(
                find_witness(original, mutant, args.seed, args.seconds), allow_nan=False
            )
        )


if __name__ == "__main__":
    main()
