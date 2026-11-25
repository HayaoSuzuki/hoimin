"""Bounded TLC verification and reproducible ITF oracles for BudgetLedger.

The existing POSIX process-tree guard owns termination and resource accounting.
Only an explicit semantic violation counts as a successful negative control.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]
FORMAL = ROOT / "formal/quint"
GUARD = ROOT / "formal/HoiminOracle/tools/lean_resource_guard.py"
QUINT = FORMAL / "node_modules/.bin/quint"


def validate_result(code: int, output: str, *, counterexample: bool) -> None:
    if counterexample:
        if code != 1 or "[violation]" not in output or "counterexample" not in output:
            msg = f"Expected a semantic counterexample, got exit {code}:\n{output}"
            raise RuntimeError(msg)
    elif code != 0:
        msg = f"Quint verification failed with exit {code}:\n{output}"
        raise RuntimeError(msg)


def guarded(arguments: list[str], directory: Path, name: str) -> tuple[int, str]:
    env = {**os.environ, "NO_COLOR": "1", "JAVA_TOOL_OPTIONS": "-Xmx1G"}
    result = subprocess.run(  # noqa: S603 -- repository-owned executable and argv, no shell
        [
            sys.executable,
            str(GUARD),
            "--timeout-seconds",
            "20",
            "--rss-limit-mib",
            "2048",
            "--sample-ms",
            "50",
            "--stats",
            str(directory / f"{name}.stats.json"),
            "--",
            str(QUINT),
            *arguments,
        ],
        cwd=directory,
        env=env,
        capture_output=True,
        text=True,
        check=False,
        timeout=25,
    )
    output = result.stdout + result.stderr
    (directory / f"{name}.log").write_text(output, encoding="utf-8")
    print(output, end="")  # noqa: T201 -- developer command diagnostics
    return result.returncode, output


def canonical_trace(path: Path) -> str:
    """Remove generator timestamps/absolute paths, preserving all expected states."""
    trace = json.loads(path.read_text(encoding="utf-8"))
    trace["#meta"] = {"format": "ITF", "status": trace["#meta"]["status"]}
    return json.dumps(trace, sort_keys=True, indent=2) + "\n"


def verify(*, update: bool, simulate_only: bool = False) -> None:
    if not QUINT.is_file():
        msg = (
            "Install the pinned tool first: "
            "npm ci --prefix formal/quint --ignore-scripts"
        )
        raise RuntimeError(msg)
    build = FORMAL / "_build"
    build.mkdir(exist_ok=True)
    model = str(FORMAL / "budget.qnt")
    code, output = guarded(["typecheck", model], build, "typecheck")
    validate_result(code, output, counterexample=False)
    if not simulate_only:
        for module in ("Budget", "BudgetOverdraft"):
            code, output = guarded(
                [
                    "verify",
                    model,
                    "--backend=tlc",
                    "--apalache-version=0.62.4",
                    f"--tlc-config={FORMAL / 'tlc.json'}",
                    f"--main={module}",
                    "--invariant=Safety",
                    "--verbosity=3",
                ],
                build,
                module,
            )
            validate_result(code, output, counterexample=module != "Budget")
    with TemporaryDirectory(dir=build) as temporary:
        directory = Path(temporary)
        code, output = guarded(
            [
                "run",
                model,
                "--backend=typescript",
                "--main=Budget",
                "--invariant=Safety",
                "--seed=1172",
                "--max-steps=24",
                "--max-samples=20",
                "--n-traces=4",
                f"--out-itf={directory}/trace_{{seq}}.itf.json",
                "--verbosity=1",
            ],
            build,
            "simulation",
        )
        validate_result(code, output, counterexample=False)
        expected_paths = [directory / f"trace_{i}.itf.json" for i in range(4)]
        for source in expected_paths:
            content = canonical_trace(source)
            target = FORMAL / "traces" / source.name
            if update:
                target.parent.mkdir(exist_ok=True)
                target.write_text(content, encoding="utf-8")
            elif not target.is_file() or target.read_text(encoding="utf-8") != content:
                msg = (
                    f"Stale Quint oracle: {target}; run tools/verify_quint.py --update"
                )
                raise RuntimeError(msg)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--update", action="store_true", help="replace committed ITF oracles"
    )
    parser.add_argument(
        "--simulate-only",
        action="store_true",
        help="skip TLC for local oracle development",
    )
    arguments = parser.parse_args()
    verify(update=arguments.update, simulate_only=arguments.simulate_only)


if __name__ == "__main__":
    main()
