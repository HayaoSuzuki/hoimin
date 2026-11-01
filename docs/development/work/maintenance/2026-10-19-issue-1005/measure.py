# ruff: noqa: INP001
# Standalone audit script/test, not an importable package.
"""One-off #1005 fixtures and release measurements; no timing regression gate."""

import argparse
import hashlib
import json
import platform
import statistics
import subprocess
import time
from pathlib import Path
from typing import TypedDict

SIZES = (16, 32, 64, 128)
EXPECTED_BYTES = dict(zip(SIZES, (17695, 70127, 279439, 1115855), strict=True))
TARGET = (
    "class Root(Exception): pass\nclass Child(Root): pass\n"
    "def f():\n    raise Child()\n"
)
MAX_OUTPUT_BYTES = 8 * 1024 * 1024


class Measurement(TypedDict):
    label: str
    shape: str
    n: int
    repeat: int
    argv: list[str]
    elapsed_ns: int
    stdout_bytes: int
    stderr_bytes: int
    candidates_sha256: str
    heap: dict[str, int]


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fixtures(directory: Path) -> None:
    rows = []
    for n in SIZES:
        for shape in ("dag", "control"):
            root = directory / "fixtures" / f"{shape}-{n}"
            root.mkdir(parents=True, exist_ok=True)
            for i in range(n):
                for layer, dest in (("a", "m"), ("m", "l")):
                    origin = {"m": "x", "l": "y"}[dest] if shape == "control" else dest
                    (root / f"{layer}{i:03}.py").write_text(
                        "".join(
                            f"from {origin}{j:03} import sentinel as s{j:03}\n"
                            for j in range(n)
                        )
                    )
                (root / f"l{i:03}.py").write_text("sentinel = 0\n")
            (root / "target.py").write_text(TARGET)
            files = sorted(root.glob("*.py"))
            for p in files:
                compile(p.read_text(), str(p), "exec")
            manifest = [
                {
                    "path": p.name,
                    "bytes": p.stat().st_size,
                    "sha256": sha(p.read_bytes()),
                }
                for p in files
            ]
            assert len(files) == 3 * n + 1
            assert sum(p.stat().st_size for p in files) == EXPECTED_BYTES[n]
            rows.append(
                {
                    "n": n,
                    "shape": shape,
                    "files": manifest,
                    "source_bytes": EXPECTED_BYTES[n],
                    "graph_edges": 2 * n * n if shape == "dag" else 0,
                }
            )
    (directory / "fixtures.json").write_text(json.dumps(rows, indent=2) + "\n")


def validate(document: dict[str, object]) -> str:
    assert document["truncated"] is False
    candidates = document["candidates"]
    assert isinstance(candidates, list)
    assert len(candidates) == 1
    assert document["diagnostics"] == []
    return sha(
        json.dumps(
            document["candidates"], sort_keys=True, separators=(",", ":")
        ).encode()
    )


def verify_fixtures(directory: Path) -> None:
    for row in json.loads((directory / "fixtures.json").read_text()):
        root = directory / "fixtures" / f"{row['shape']}-{row['n']}"
        actual = [
            {"path": p.name, "bytes": p.stat().st_size, "sha256": sha(p.read_bytes())}
            for p in sorted(root.glob("*.py"))
        ]
        assert actual == row["files"], root


def measure(directory: Path, labels: list[str], repeats: int) -> None:
    verify_fixtures(directory)
    rows: list[Measurement] = []
    for n in SIZES:
        for repeat in range(-1, repeats):
            for shape in ("dag", "control") if repeat % 2 else ("control", "dag"):
                for label in labels if repeat % 2 else labels[::-1]:
                    root = directory / "fixtures" / f"{shape}-{n}"
                    prefix = directory / f"{label}-{shape}-{n}-{repeat}"
                    command = [
                        str(directory / f"{label}-hoimin"),
                        "plan",
                        "--root",
                        str(root),
                        "--file",
                        "target.py",
                        "--operators",
                        "exception_hierarchy",
                        "--max-candidates",
                        "1",
                        "--analyzer-timeout",
                        "5s",
                        "--allow-best-effort-memory",
                        "--",
                        "true",
                    ]
                    # Pipes let communicate wait for readiness instead of the
                    # timed wait polling used with regular-file redirection.
                    # Persist bounded output after stopping the wall-time clock.
                    start = time.perf_counter_ns()
                    result = subprocess.run(  # noqa: S603 - Trusted saved binaries/artifact argv; no shell.
                        command, capture_output=True, timeout=30, check=False
                    )
                    elapsed = time.perf_counter_ns() - start
                    assert len(result.stdout) <= MAX_OUTPUT_BYTES, (
                        "unexpected stdout size"
                    )
                    assert len(result.stderr) <= MAX_OUTPUT_BYTES, (
                        "unexpected stderr size"
                    )
                    prefix.with_suffix(".stdout.json").write_bytes(result.stdout)
                    prefix.with_suffix(".stderr.txt").write_bytes(result.stderr)
                    assert result.returncode == 0, (command, result.returncode, prefix)
                    document = json.loads(
                        prefix.with_suffix(".stdout.json").read_text()
                    )
                    candidate_sha = validate(document)
                    heap = subprocess.run(  # noqa: S603 - Trusted saved binaries/artifact argv; no shell.
                        [str(directory / f"{label}-heap"), str(root)],
                        capture_output=True,
                        text=True,
                        timeout=30,
                        check=True,
                    )
                    prefix.with_suffix(".heap.json").write_text(heap.stdout)
                    rows.append(
                        {
                            "label": label,
                            "shape": shape,
                            "n": n,
                            "repeat": repeat,
                            "argv": command,
                            "elapsed_ns": elapsed,
                            "stdout_bytes": len(result.stdout),
                            "stderr_bytes": len(result.stderr),
                            "candidates_sha256": candidate_sha,
                            "heap": json.loads(heap.stdout),
                        }
                    )
    assert len({r["candidates_sha256"] for r in rows}) == 1
    output: dict[str, object] = {
        "environment": {
            "platform": platform.platform(),
            "python": platform.python_version(),
        },
        "binaries": {
            f"{label}-{kind}": sha((directory / f"{label}-{kind}").read_bytes())
            for label in labels
            for kind in ("hoimin", "heap")
        },
        "probe_sha256": sha((directory / "heap_probe.rs").read_bytes()),
        "runner_sha256": sha(Path(__file__).read_bytes()),
        "fixtures_manifest_sha256": sha((directory / "fixtures.json").read_bytes()),
        "rows": rows,
    }
    output["medians"] = [
        {
            "label": label,
            "shape": shape,
            "n": n,
            "elapsed_ms": statistics.median(
                r["elapsed_ns"]
                for r in rows
                if r["label"] == label
                and r["shape"] == shape
                and r["n"] == n
                and r["repeat"] >= 0
            )
            / 1e6,
            "heap": {
                key: statistics.median(
                    r["heap"][key]
                    for r in rows
                    if r["label"] == label
                    and r["shape"] == shape
                    and r["n"] == n
                    and r["repeat"] >= 0
                )
                for key in rows[0]["heap"]
            },
        }
        for label in labels
        for shape in ("dag", "control")
        for n in SIZES
    ]
    (directory / ("-".join(labels) + "-measurements.json")).write_text(
        json.dumps(output, indent=2) + "\n"
    )
    print(json.dumps(output["medians"], indent=2))  # noqa: T201 - Measurement CLI output.


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--fixtures", action="store_true")
    parser.add_argument("--labels", nargs="+", default=["baseline", "candidate"])
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if args.fixtures:
        fixtures(args.directory)
    else:
        measure(args.directory, args.labels, args.repeats)
