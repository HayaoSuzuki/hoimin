"""Generate bounded Python source inputs for cargo-fuzz without executing them."""

import argparse
import hashlib
import json
import platform
import warnings
from importlib.metadata import version
from pathlib import Path

MAX_SOURCE_BYTES = 4096


def store_source(source: str, output: Path, *, max_bytes: int) -> str:
    if not source.strip():
        return "empty"
    try:
        encoded = source.encode("utf-8")
    except UnicodeError:
        return "invalid"
    if len(encoded) > max_bytes:
        return "oversize"
    try:
        with warnings.catch_warnings(action="ignore", category=SyntaxWarning):
            compile(encoded, "<hypothesmith>", "exec", dont_inherit=True)
    except SyntaxError, ValueError:
        return "invalid"
    destination = output / hashlib.sha256(encoded).hexdigest()
    try:
        with destination.open("xb") as stream:
            stream.write(encoded)
    except FileExistsError:
        return "existing"
    return "written"


def generate(
    output: Path,
    *,
    examples: int,
    random_seed: int,
    strategy: str,
    max_bytes: int,
) -> dict[str, int]:
    if examples < 1 or not 1 <= max_bytes <= MAX_SOURCE_BYTES:
        msg = "examples must be positive; max-bytes must be between 1 and 4096"
        raise ValueError(msg)
    if strategy not in {"grammar", "libcst"}:
        msg = "strategy must be grammar or libcst"
        raise ValueError(msg)

    # Optional dependencies: normal unit tests and --help need no fuzz extras.
    import hypothesmith  # noqa: PLC0415
    from hypothesis import HealthCheck, Phase, given, seed, settings  # noqa: PLC0415

    source_strategy = (
        hypothesmith.from_grammar()
        if strategy == "grammar"
        else hypothesmith.from_node()
    )
    output.mkdir(parents=True, exist_ok=True)
    counts = dict.fromkeys(("written", "existing", "empty", "oversize", "invalid"), 0)

    @seed(random_seed)
    @settings(
        max_examples=examples,
        database=None,
        deadline=None,
        phases=(Phase.generate, Phase.target),
        suppress_health_check=(HealthCheck.filter_too_much,),
    )
    @given(source_strategy)
    def collect(source: str) -> None:
        counts[store_source(source, output, max_bytes=max_bytes)] += 1

    # Hypothesmith also compiles during generation; these are source warnings,
    # not executions or generator failures. Keep the CLI's diagnostics readable.
    with warnings.catch_warnings(action="ignore", category=SyntaxWarning):
        collect()
    return counts


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, default=Path("fuzz/corpus/python_analyzer")
    )
    parser.add_argument("--examples", type=int, default=100)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--strategy", choices=("grammar", "libcst"), default="grammar")
    parser.add_argument("--max-bytes", type=int, default=MAX_SOURCE_BYTES)
    args = parser.parse_args()
    try:
        counts = generate(
            args.output,
            examples=args.examples,
            random_seed=args.seed,
            strategy=args.strategy,
            max_bytes=args.max_bytes,
        )
    except ModuleNotFoundError:
        parser.error(
            "install fuzz dependencies: "
            "uv sync --frozen --group fuzz --no-install-project"
        )
    except ValueError as error:
        parser.error(str(error))
    print(  # noqa: T201 -- The CLI emits its generation report as JSON on stdout.
        json.dumps(
            {
                "output": str(args.output),
                "seed": args.seed,
                "strategy": args.strategy,
                "max_examples": args.examples,
                "max_bytes": args.max_bytes,
                "python": platform.python_version(),
                "hypothesmith": version("hypothesmith"),
                "hypothesis": version("hypothesis"),
                "libcst": version("libcst"),
                "lark": version("lark"),
                **counts,
            },
            sort_keys=True,
        )
    )
    return 0 if counts["written"] + counts["existing"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
