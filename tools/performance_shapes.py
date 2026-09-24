"""Declared performance gates and bounded release CLI measurements."""

import argparse
import hashlib
import json
import platform
import re
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GROWTH_SIZE_COUNT = 3
MIN_REPEATS = 3
MAX_REPEATS = 10
MIN_SAMPLE_MS = 10
MAX_SAMPLE_MS = 1000

type JsonValue = (
    bool | int | float | str | list[JsonValue] | dict[str, JsonValue] | None
)

DIMENSIONS = {
    "discovery",
    "fingerprint",
    "layout",
    "ast",
    "analysis-state",
    "verify",
    "output",
    "workspace",
}
SHAPES = {
    "discovery",
    "fingerprint",
    "long-line",
    "many-lines",
    "ast",
    "imports",
    "bindings",
    "verify-top1",
    "verify-topn",
    "output",
    "workspace",
}
ADDITIONAL_SHAPES = {
    "target-files",
    "file-selectors",
    "symbol-selectors",
    "line-selectors",
    "fingerprint-files",
    "fingerprint-bytes",
    "fingerprint-mixed",
    "unicode-long-line",
    "unicode-many-lines",
    "ast-wide",
    "ast-left",
    "large-literal",
    "imports-active",
    "verify-multifile",
    "verify-partial",
    "output-record",
    "workspace-bytes",
    "workspace-workers",
}
SHAPES |= ADDITIONAL_SHAPES
LIMITS = {
    name: (
        128
        if name in {"ast", "verify-top1", "verify-topn", "output", "workspace"}
        else 65536
    )
    for name in SHAPES
}
LIMITS.update(
    {
        "ast-left": 128,
        "verify-multifile": 128,
        "verify-partial": 128,
        "workspace-workers": 4,
    }
)
METRICS = {"operations", "retained_heap", "peak_heap", "sampled_tree_rss", "time"}


# Keep the complete registry contract together for auditability.
def validate_registry(registry: dict[str, JsonValue]) -> dict[str, JsonValue]:  # noqa: C901, PLR0912
    if registry.get("schema_version") != 1:
        msg = "unsupported schema"
        raise ValueError(msg)
    ids = set()
    for row in registry["shapes"]:
        if row["id"] in ids:
            msg = "duplicate shape"
            raise ValueError(msg)
        ids.add(row["id"])
        sizes = row["sizes"]
        if (
            len(sizes) != GROWTH_SIZE_COUNT
            or any(type(n) is not int for n in sizes)
            or not sizes[0] >= 1
            or sizes != [sizes[0], 2 * sizes[0], 4 * sizes[0]]
            or sizes[-1] > LIMITS.get(row["fixture"], 0)
        ):
            msg = "sizes must be bounded N/2N/4N"
            raise ValueError(msg)
        if row["metric"] not in METRICS or row["status"] not in {
            "active",
            "measurement-only",
            "pending",
        }:
            msg = "invalid metric/status"
            raise ValueError(msg)
        if row["fixture"] not in SHAPES or not row["growth_model"]:
            msg = "missing fixture/model"
            raise ValueError(msg)
    if {r["dimension"] for r in registry["shapes"]} != DIMENSIONS:
        msg = "all eight dimensions required"
        raise ValueError(msg)
    ids = set()
    for gate in registry["gates"]:
        if gate["id"] in ids:
            msg = "duplicate gate"
            raise ValueError(msg)
        ids.add(gate["id"])
        if gate.get("metric") not in METRICS:
            msg = "invalid gate metric"
            raise ValueError(msg)
        if gate["status"] == "pending":
            if not gate.get("issue", "").startswith(
                "https://github.com/tokyogas-tech/hoimin/issues/"
            ):
                msg = "pending dependency required"
                raise ValueError(msg)
        elif gate["status"] == "active":
            args = gate["args"]
            if (
                not args
                or args[0] != "test"
                or "--exact" not in args
                or "--ignored" in args
            ):
                msg = "exact non-ignored Rust test required"
                raise ValueError(msg)
        else:
            msg = "unknown gate status"
            raise ValueError(msg)
    return registry


def load_registry(path: Path) -> dict[str, JsonValue]:
    return validate_registry(json.loads(path.read_text()))


def executed_tests(output: str) -> int:
    results = re.findall(
        r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;", output
    )
    if (
        not results
        or any(s != "ok" or int(f) for s, _, f, _ in results)
        or sum(int(p) for _, p, _, _ in results) == 0
    ):
        msg = "gate did not execute a passing test"
        raise ValueError(msg)
    return sum(int(p) for _, p, _, _ in results)


# Each branch materializes one registered benchmark shape.
def make_fixture(name: str, size: int, root: Path) -> dict[str, JsonValue]:  # noqa: C901, PLR0912, PLR0915
    if name not in SHAPES or type(size) is not int or not 1 <= size <= LIMITS[name]:
        msg = "unknown shape or out-of-bounds size"
        raise ValueError(msg)
    root.mkdir(parents=True)
    source, operator, count, truncated, mode = (
        "x = 1 + 2\n",
        "binary_add_sub",
        1,
        False,
        "plan",
    )
    options = []
    selectors = ["--file", "case.py"]
    jobs, candidate_limit, plan_count = 1, None, None
    if name == "discovery":
        (root / "unrelated").mkdir()
        for i in range(size):
            (root / "unrelated" / f"{i}.txt").write_text("")
    elif name == "fingerprint":
        (root / "config.toml").write_text("value = 1\n")
        options = ["--fingerprint-include", "*.toml"] * size
    elif name in {"long-line", "many-lines"}:
        source = (
            "x = ["
            + (" " if name == "long-line" else "\n").join(["True,"] * size)
            + "]\n"
        )
        operator, truncated = "boolean_literal", size > 1
    elif name == "ast":
        source = "x = " + "[" * size + "1 + 2" + "]" * size + "\n"
    elif name == "imports":
        source = "".join(f"from typing import Optional as A{i}\n" for i in range(size))
        source += "".join(f"x{i}: A{i}[int]\n" for i in range(size))
        operator, count = "boolean_literal", 0
    elif name == "bindings":
        source, operator, count = (
            "list = tuple\nx = list()\n" * size,
            "collection_list_tuple",
            0,
        )
    elif name in {"verify-top1", "verify-topn", "output"}:
        source, mode = "x = 1 + 2\n" * size, "run" if name == "output" else "verify"
        count = 1 if name == "verify-top1" else size
    elif name == "workspace":
        mode = "run"
        (root / "data").mkdir()
        for i in range(size):
            (root / "data" / f"{i}.txt").write_bytes(b"x" * 65536)
    elif name == "target-files":
        source = "pass\n"
        selectors = ["--source", "."]
        for i in range(size):
            (root / f"selected{i}.py").write_text("value = 1 + 2\n")
        truncated = size > 1
    elif name in {"file-selectors", "symbol-selectors", "line-selectors"}:
        if name == "symbol-selectors":
            source = "def subject():\n    return 1 + 2\n"
            selectors = ["--symbol", "case:subject"] * size
            options = ["--source", "."]
        else:
            selectors = (
                ["--file", "case.py"]
                if name == "file-selectors"
                else ["--line", "case.py:1-1"]
            ) * size
    elif name == "fingerprint-files":
        for i in range(size):
            (root / f"config{i}.toml").write_text("value = 1\n")
        options = ["--fingerprint-include", "*.toml"]
    elif name == "fingerprint-bytes":
        (root / "config.toml").write_bytes(b"#" * (size * 1024))
        options = ["--fingerprint-file", "config.toml"]
    elif name == "fingerprint-mixed":
        (root / "config.toml").write_text("value = 1\n")
        options = ["--fingerprint-file", "config.toml"] + [
            "--fingerprint-include",
            "*.toml",
        ] * size
    elif name in {"unicode-long-line", "unicode-many-lines"}:
        separator = " " if name == "unicode-long-line" else "\n"
        source = "x = [" + separator.join(["('雪', True),"] * size) + "]\n"
        operator, truncated = "boolean_literal", size > 1
    elif name == "ast-wide":
        source = "x = [" + "1," * size + "]\ny = 1 + 2\n"
    elif name == "ast-left":
        source = "x = " + " + ".join(["1"] * (size + 1)) + "\n"
        truncated = size > 1
    elif name == "large-literal":
        source = "x = ['" + "x" * (size * 256) + "']\ny = 1 + 2\n"
    elif name == "imports-active":
        source = "from typing import Sequence\n"
        source += "".join(f"from typing import Optional as A{i}\n" for i in range(size))
        source += "".join(f"x{i}: list[int]\n" for i in range(size))
        operator, truncated = "type_list_sequence", size > 1
    elif name == "verify-multifile":
        source, mode, count = "pass\n", "verify", size
        selectors = ["--source", "."]
        for i in range(size):
            (root / f"selected{i}.py").write_text("value = 1 + 2\n")
    elif name == "verify-partial":
        source = "value = 1 + 2\n" * (size + 1)
        mode, count, candidate_limit, plan_count, truncated = (
            "verify",
            size,
            size,
            size,
            True,
        )
    elif name == "output-record":
        source = "x = ['" + "x" * (size * 256) + "']\n"
        mode, operator = "run", "collection_list_tuple"
    elif name == "workspace-bytes":
        mode = "run"
        (root / "data.bin").write_bytes(b"x" * (size * 65536))
    elif name == "workspace-workers":
        source, mode, count, jobs = "value = 1 + 2\n" * 4, "run", 4, size
        (root / "data.bin").write_bytes(b"x" * 65536)
    (root / "case.py").write_text(source)
    return {
        "name": name,
        "size": size,
        "mode": mode,
        "operator": operator,
        "expected_candidates": count,
        "truncated": truncated,
        "options": options,
        "selectors": selectors,
        "jobs": jobs,
        "candidate_limit": candidate_limit or (1 if mode == "plan" else 128),
        "plan_count": plan_count if plan_count is not None else size,
        "source_bytes": len(source.encode()),
        "python_source_bytes": sum(p.stat().st_size for p in root.rglob("*.py")),
        "input_files": sum(1 for p in root.rglob("*") if p.is_file()),
        "input_bytes": sum(p.stat().st_size for p in root.rglob("*") if p.is_file()),
    }


class SemanticMismatch(ValueError):  # noqa: N818 - Preserve the public exception name.
    """The CLI completed but its observable result violates the fixture."""


def validate_output(
    document: dict[str, JsonValue], fixture: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    if fixture["mode"] == "plan":
        records = document.get("candidates")
        if document.get("truncated") is not fixture["truncated"]:
            msg = "unexpected truncation"
            raise SemanticMismatch(msg)
    else:
        records = document.get("mutants")
        if document.get("summary", {}).get("complete") is not (
            not fixture["truncated"]
        ):
            msg = "unexpected execution completeness"
            raise SemanticMismatch(msg)
    if not isinstance(records, list) or len(records) != fixture["expected_candidates"]:
        msg = "unexpected candidate count"
        raise SemanticMismatch(msg)
    return {"candidates": len(records), "truncated": fixture["truncated"]}


def observed_rss(stats: dict[str, JsonValue]) -> int | None:
    if stats.get("reason") != "child_exit":
        msg = f"resource guard failed: {stats.get('reason')}"
        raise ValueError(msg)
    return stats["peak_rss_kib"] * 1024 if stats["peak_rss_kib"] else None


def measure_once(
    binary: Path,
    fixture: dict[str, JsonValue],
    root: Path,
    artifact: Path,
    sample_ms: int,
) -> dict[str, JsonValue]:
    common = [
        "--root",
        str(root),
        *fixture["selectors"],
        "--operators",
        fixture["operator"],
        "--jobs",
        str(fixture["jobs"]),
        "--max-mutants",
        "128",
        "--max-candidates",
        str(fixture["candidate_limit"]),
        "--allow-best-effort-memory",
        "--max-workspace-size",
        "8GiB",
        "--min-free-space",
        "10GiB",
    ] + fixture["options"]
    test = ["--", sys.executable, "-c", "pass"]
    command = [str(binary), "plan", *common, *test]
    if fixture["mode"] == "verify":
        manifest = artifact / "plan.json"
        with manifest.open("wb") as out, (artifact / "plan.stderr").open("wb") as err:
            prepared = subprocess.run(  # noqa: S603 - Trusted CLI/test arguments; no shell execution.
                command, stdout=out, stderr=err, timeout=30, check=False
            )
        if prepared.returncode != (4 if fixture["truncated"] else 0):
            msg = "verify fixture plan failed"
            raise ValueError(msg)
        validate_output(
            json.loads(manifest.read_text()),
            {
                "mode": "plan",
                "expected_candidates": fixture["plan_count"],
                "truncated": fixture["truncated"],
            },
        )
        command = [
            str(binary),
            "verify",
            str(manifest),
            "--top",
            str(fixture["expected_candidates"]),
            "--format",
            "json",
        ]
    elif fixture["mode"] == "run":
        command = [str(binary), "run", *common, "--format", "json", *test]
    stats_path = artifact / "resource.json"
    guard = [
        sys.executable,
        str(ROOT / "formal/HoiminOracle/tools/lean_resource_guard.py"),
        "--timeout-seconds",
        "30",
        "--rss-limit-mib",
        "2048",
        "--sample-ms",
        str(sample_ms),
        "--stats",
        str(stats_path),
        "--",
        *command,
    ]
    with (
        (artifact / "stdout.json").open("wb") as out,
        (artifact / "stderr.txt").open("wb") as err,
    ):
        completed = subprocess.run(  # noqa: S603 - Trusted CLI/test arguments; no shell execution.
            guard, stdout=out, stderr=err, timeout=40, check=False
        )
    stats = json.loads(stats_path.read_text())
    rss = observed_rss(stats)
    expected_exit = (
        4 if fixture["truncated"] else (0 if fixture["mode"] == "plan" else 1)
    )
    if completed.returncode != expected_exit:
        msg = f"CLI exit {completed.returncode}; expected {expected_exit}"
        raise ValueError(msg)
    observation = validate_output(
        json.loads((artifact / "stdout.json").read_text()), fixture
    )
    return dict(
        argv=command,
        elapsed_ms=stats["elapsed_ms"],
        sampled_tree_rss_bytes=rss,
        sample_ms=sample_ms,
        exit_code=completed.returncode,
        output_document_bytes=(artifact / "stdout.json").stat().st_size,
        **observation,
    )


def run_gate(
    registry: dict[str, JsonValue], artifact: Path
) -> list[dict[str, JsonValue]]:
    outcomes = []
    for gate in registry["gates"]:
        if gate["status"] == "pending":
            outcomes.append(
                {"id": gate["id"], "status": "pending", "issue": gate["issue"]}
            )
            continue
        log = artifact / f"{gate['id']}.log"
        with log.open("w") as output:
            completed = subprocess.run(  # noqa: S603 - Trusted CLI/test arguments; no shell execution.
                ["cargo", *gate["args"]],  # noqa: S607 - Resolve the developer tool from PATH.
                cwd=ROOT,
                stdout=output,
                stderr=subprocess.STDOUT,
                text=True,
                timeout=300,
                check=False,
            )
        output = log.read_text()
        if completed.returncode:
            print(output, file=sys.stderr)  # noqa: T201 - CLI status or failure diagnostics.
            msg = f"gate failed: {gate['id']}"
            raise ValueError(msg)
        outcomes.append(
            {"id": gate["id"], "status": "passed", "tests": executed_tests(output)}
        )
    return outcomes


def summarize_comparisons(
    medians: list[dict[str, JsonValue]],
) -> list[dict[str, JsonValue]]:
    """Compare candidate medians with their matching baseline observations."""
    grouped = {}
    for item in medians:
        grouped.setdefault((item["shape"], item["size"]), {})[item["label"]] = item
    comparisons = []
    for (shape, size), labels in grouped.items():
        if set(labels) != {"baseline", "candidate"}:
            msg = f"missing baseline or candidate median: {shape}/{size}"
            raise ValueError(msg)
        baseline, candidate = labels["baseline"], labels["candidate"]
        elapsed_base, elapsed_candidate = (
            baseline["elapsed_ms"],
            candidate["elapsed_ms"],
        )
        rss_base = baseline["sampled_tree_rss_bytes"]
        rss_candidate = candidate["sampled_tree_rss_bytes"]
        comparisons.append(
            {
                "shape": shape,
                "size": size,
                "elapsed_ms_delta": elapsed_candidate - elapsed_base,
                "elapsed_ratio": (
                    elapsed_candidate / elapsed_base if elapsed_base else None
                ),
                "sampled_tree_rss_bytes_delta": (
                    rss_candidate - rss_base
                    if rss_base is not None and rss_candidate is not None
                    else None
                ),
                "sampled_tree_rss_ratio": (
                    rss_candidate / rss_base
                    if rss_base not in {None, 0} and rss_candidate is not None
                    else None
                ),
            }
        )
    return comparisons


def summarize_growth(
    medians: list[dict[str, JsonValue]],
) -> list[dict[str, JsonValue]]:
    """Report N-relative growth within each binary; RSS is not allocator peak."""
    grouped = {}
    for item in medians:
        grouped.setdefault((item["shape"], item["label"]), []).append(item)
    growth = []
    for (shape, label), items in grouped.items():
        items.sort(key=lambda item: item["size"])
        sizes = [item["size"] for item in items]
        if len(sizes) != GROWTH_SIZE_COUNT or sizes != [
            sizes[0],
            sizes[0] * 2,
            sizes[0] * 4,
        ]:
            msg = f"missing N/2N/4N medians: {shape}/{label}"
            raise ValueError(msg)
        first = items[0]
        for item in items[1:]:
            row = {
                "shape": shape,
                "label": label,
                "base_size": first["size"],
                "size": item["size"],
            }
            for source, field in [
                ("elapsed_ms", "elapsed_ratio"),
                ("sampled_tree_rss_bytes", "sampled_tree_rss_ratio"),
                ("output_document_bytes", "output_document_ratio"),
            ]:
                base, observed = first.get(source), item.get(source)
                row[field] = (
                    observed / base
                    if base not in (None, 0) and observed is not None
                    else None
                )
            growth.append(row)
    return growth


# Preserve the existing CLI orchestration and artifact failure handling.
def main() -> int:  # noqa: C901, PLR0912, PLR0915
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["check", "gate", "measure"])
    parser.add_argument(
        "--registry", type=Path, default=ROOT / "docs/performance/shapes.json"
    )
    parser.add_argument("--output", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--shape", action="append", choices=sorted(SHAPES))
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--sample-ms", type=int, default=25)
    args = parser.parse_args()
    if args.mode == "check":
        registry = load_registry(args.registry)
        print(f"validated {len(registry['shapes'])} shapes")  # noqa: T201 - CLI status or failure diagnostics.
        return 0
    if args.output is None:
        parser.error("--output is required")
    if (
        not MIN_REPEATS <= args.repeats <= MAX_REPEATS
        or not MIN_SAMPLE_MS <= args.sample_ms <= MAX_SAMPLE_MS
    ):
        parser.error("repeats must be 3..10 and sample-ms 10..1000")
    args.output.mkdir(parents=True, exist_ok=False)
    result = {"schema_version": 1, "environment": {}, "runs": []}
    try:
        registry = load_registry(args.registry)
        result["environment"] = {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "cpu": platform.processor(),
            "python": sys.version,
            "rust": subprocess.run(
                ["rustc", "--version"],  # noqa: S607 - Developer tool from PATH.
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip(),
        }
        result["registry_sha256"] = hashlib.sha256(
            args.registry.read_bytes()
        ).hexdigest()
        if args.mode == "gate":
            result["gates"] = run_gate(registry, args.output)
        else:
            if args.baseline is None or args.candidate is None:
                msg = "--baseline and --candidate required"
                raise ValueError(msg)  # noqa: TRY301 - Record validation failures in the artifact.
            result["binaries"] = {
                name: {
                    "path": str(path.resolve(strict=True)),
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
                for name, path in [
                    ("baseline", args.baseline),
                    ("candidate", args.candidate),
                ]
            }
            for row in registry["shapes"]:
                if args.shape and row["fixture"] not in args.shape:
                    continue
                for size in row["sizes"]:
                    for label, info in result["binaries"].items():
                        elapsed = []
                        rss_samples = []
                        document_bytes = []
                        for repeat in range(args.repeats):
                            artifact = (
                                args.output / f"{row['id']}-{size}-{label}-{repeat}"
                            )
                            artifact.mkdir()
                            with tempfile.TemporaryDirectory(
                                prefix="hoimin-perf-"
                            ) as temporary:
                                root = Path(temporary) / "project"
                                fixture = make_fixture(row["fixture"], size, root)
                                run = measure_once(
                                    Path(info["path"]),
                                    fixture,
                                    root,
                                    artifact,
                                    args.sample_ms,
                                )
                            result["runs"].append(
                                dict(
                                    shape=row["id"],
                                    label=label,
                                    repeat=repeat,
                                    fixture=fixture,
                                    **run,
                                )
                            )
                            elapsed.append(run["elapsed_ms"])
                            rss_samples.append(run["sampled_tree_rss_bytes"])
                            document_bytes.append(run["output_document_bytes"])
                        result.setdefault("medians", []).append(
                            {
                                "shape": row["id"],
                                "size": size,
                                "label": label,
                                "elapsed_ms": statistics.median(elapsed),
                                "output_document_bytes": statistics.median(
                                    document_bytes
                                ),
                                "sampled_tree_rss_bytes": (
                                    statistics.median(rss_samples)
                                    if all(value is not None for value in rss_samples)
                                    else None
                                ),
                            }
                        )
            result["comparisons"] = summarize_comparisons(result.get("medians", []))
            result["growth"] = summarize_growth(result.get("medians", []))
        result["status"] = "passed"
    except SemanticMismatch as error:
        result["status"], result["error"] = "mismatch", str(error)
    except (KeyError, OSError, ValueError, subprocess.SubprocessError) as error:
        result["status"], result["error"] = "infrastructure-error", str(error)
    finally:
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(  # noqa: T201 - CLI status or failure diagnostics.
        json.dumps(
            {"status": result["status"], "artifact": str(args.output / "result.json")}
        )
    )
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
