"""Developer-only, fixed-domain Django mutation research; not a code sandbox."""

from __future__ import annotations

import argparse
import datetime
import importlib
import json
import sys
import time
from collections.abc import Callable
from pathlib import Path
from typing import cast

from hypothesis import Phase, find, seed, settings
from hypothesis import strategies as st
from hypothesis.errors import NoSuchExample

type Args = list[int | str | bool]
type Scalar = int | str
type Function = Callable[..., object]

TARGETS = {
    "numberformat": ("django.utils.numberformat", "format"),
    "timesince": ("django.utils.timesince", "timesince"),
    "duration": ("django.utils.duration", "duration_microseconds"),
}


def domain(target: str) -> st.SearchStrategy[Args]:
    if target == "numberformat":
        return st.tuples(
            st.integers(-1000000, 1000000).map(
                lambda value: f"{value // 100}.{value % 100:02d}"
            ),
            st.integers(0, 6),
            st.integers(1, 6),
            st.booleans(),
        ).map(list)
    if target == "timesince":
        return st.tuples(
            st.integers(0, 366 * 86400 * 4),
            st.integers(0, 366 * 86400 * 4),
            st.integers(1, 3),
        ).map(list)
    if target == "duration":
        return st.integers(-(10**15), 10**15).map(lambda value: [value])
    raise ValueError(target)


def invoke(target: str, function: Function, args: Args) -> object:
    if target == "numberformat":
        return function(
            args[0],
            ".",
            decimal_pos=args[1],
            grouping=args[2],
            thousand_sep=",",
            force_grouping=args[3],
            use_l10n=False,
        )
    if target == "timesince":
        origin = datetime.datetime(2000, 1, 1)  # noqa: DTZ001 -- intentionally fixed naive dates on both sides.
        return function(
            origin + datetime.timedelta(seconds=cast("int", args[0])),
            origin + datetime.timedelta(seconds=cast("int", args[1])),
            depth=args[2],
        )
    if target == "duration":
        return function(datetime.timedelta(microseconds=cast("int", args[0])))
    raise ValueError(target)


def observe(target: str, function: Function, args: Args) -> Scalar:
    value = invoke(target, function, args)
    if isinstance(value, str):
        return str(value)
    if type(value) is int:
        return value
    raise TypeError(type(value).__name__)


def find_witness(
    target: str, original: Function, mutant: Function, seed_value: int, seconds: float
) -> dict[str, object]:
    started = time.monotonic()
    invalid = False
    attempts = 0
    observations: dict[tuple[int | str | bool, ...], tuple[Scalar, Scalar]] = {}

    def different(args: Args) -> bool:
        nonlocal invalid, attempts
        key = tuple(args)
        if key in observations:
            left, right = observations[key]
            return type(left) is not type(right) or left != right
        if time.monotonic() - started >= seconds:
            return False
        attempts += 1
        try:
            left, right = observe(target, original, args), observe(target, mutant, args)
        except Exception:  # noqa: BLE001 -- mutant exceptions are unsupported observations, never witnesses.
            invalid = True
            return False
        observations[key] = (left, right)
        return type(left) is not type(right) or left != right

    result: dict[str, object] = {"status": "no_witness_within_budget", "args": []}
    try:
        args = seed(seed_value)(find)(
            domain(target),
            different,
            settings=settings(
                max_examples=128,
                deadline=None,
                database=None,
                phases=(Phase.generate, Phase.shrink),
            ),
        )
    except NoSuchExample:
        pass
    else:
        left, right = observations[tuple(args)]
        result.update(
            status="witness",
            args=args,
            original=left,
            mutant=right,
        )
    if invalid:
        result.update(status="unsupported_observation", args=[])
    result.update(attempts=attempts, elapsed=time.monotonic() - started)
    return result


def render_regression(target: str, args: Args, expected: Scalar) -> str:
    module, symbol = TARGETS[target]
    if not (type(expected) is int or isinstance(expected, str)):
        raise TypeError
    if target == "numberformat":
        call = (
            f"target({args[0]!r}, '.', decimal_pos={args[1]!r}, "
            f"grouping={args[2]!r}, thousand_sep=',', "
            f"force_grouping={args[3]!r}, use_l10n=False)"
        )
    elif target == "timesince":
        call = (
            "target(datetime.datetime(2000, 1, 1) + "
            f"datetime.timedelta(seconds={args[0]!r}), "
            "datetime.datetime(2000, 1, 1) + "
            f"datetime.timedelta(seconds={args[1]!r}), depth={args[2]!r})"
        )
    else:
        call = f"target(datetime.timedelta(microseconds={args[0]!r}))"
    return (
        "# Characterization candidate: review against the intended specification.\n"
        "import datetime\nimport unittest\nfrom django.conf import settings\n"
        f"from {module} import {symbol} as target\n\n"
        "if not settings.configured:\n"
        "    settings.configure(USE_I18N=False, USE_THOUSAND_SEPARATOR=False)\n\n"
        "class GeneratedRegression(unittest.TestCase):\n"
        "    def test_characterization(self):\n"
        f"        self.assertEqual({call}, {expected!r})\n"
    )


def configure() -> None:
    settings = importlib.import_module("django.conf").settings

    if not settings.configured:
        settings.configure(USE_I18N=False, USE_THOUSAND_SEPARATOR=False)


def load_source(path: Path, target: str) -> Function:
    module, symbol = TARGETS[target]
    namespace: dict[str, object] = {"__name__": module}
    # Fixed pinned trusted source, with resource limits imposed by Rust parent.
    exec(compile(path.read_text(encoding="utf-8"), str(path), "exec"), namespace)  # noqa: S102 -- developer-owned local research only.
    return cast("Function", namespace[symbol])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["search", "observe", "regression"])
    parser.add_argument("target", choices=list(TARGETS))
    parser.add_argument("--original", type=Path)
    parser.add_argument("--mutant", type=Path)
    parser.add_argument("--seed", type=int, default=790)
    parser.add_argument("--seconds", type=float, default=1.0)
    parser.add_argument("--args", default="[]")
    parser.add_argument("--expected", default="0")
    args = parser.parse_args()
    values = json.loads(args.args)
    if args.mode == "regression":
        sys.stdout.write(
            render_regression(args.target, values, json.loads(args.expected))
        )
        return
    configure()
    if args.mode == "search":
        result = find_witness(
            args.target,
            load_source(args.original, args.target),
            load_source(args.mutant, args.target),
            args.seed,
            args.seconds,
        )
    else:
        module_name, symbol = TARGETS[args.target]
        module = importlib.import_module(module_name)
        if module.__file__ is None:
            raise ValueError(module_name)
        result = {
            "value": observe(args.target, getattr(module, symbol), values),
            "path": str(Path(module.__file__).resolve()),
        }
    print(json.dumps(result, ensure_ascii=True, allow_nan=False))  # noqa: T201 -- machine-readable research evidence.


if __name__ == "__main__":
    main()
