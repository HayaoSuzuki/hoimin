"""Bounded paired release CLI observations for the six performance fixes."""

import hashlib
import json
import os
import platform
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path.cwd()
BASE, CANDIDATE, OUTPUT = map(lambda p: Path(p).resolve(), sys.argv[1:])
PYTHON = REPO / ".venv/bin/python"
GUARD = REPO / "formal/HoiminOracle/tools/lean_resource_guard.py"
OUTPUT.mkdir(parents=True, exist_ok=False)
RUNS = []


def normalized(value):
    if isinstance(value, list):
        return [normalized(v) for v in value]
    if isinstance(value, dict):
        return {
            k: normalized(v)
            for k, v in value.items()
            if k
            not in {
                "elapsed_ms",
                "duration_ms",
                "run_id",
                "worker_id",
                "workspace",
                "started_at",
                "finished_at",
                "timestamp",
                "process_id",
                "pid",
                "token",
                "elapsed",
                "disk",
                "memory",
                "output_spool",
                "stdout_spool",
                "stderr_spool",
            }
        }
    return value


def paired(name, argv, expected_candidates):
    documents = []
    for repeat in range(3):
        for label, binary in (
            [("baseline", BASE), ("candidate", CANDIDATE)]
            if repeat % 2 == 0
            else [("candidate", CANDIDATE), ("baseline", BASE)]
        ):
            destination = OUTPUT / f"{name}-{repeat}-{label}"
            destination.mkdir()
            env = dict(os.environ, TMPDIR=str(destination))
            command = [str(binary), *map(str, argv)]
            with (
                (destination / "stdout.json").open("wb") as out,
                (destination / "stderr.txt").open("wb") as err,
            ):
                result = subprocess.run(
                    [
                        str(PYTHON),
                        str(GUARD),
                        "--timeout-seconds",
                        "30",
                        "--rss-limit-mib",
                        "2048",
                        "--sample-ms",
                        "25",
                        "--stats",
                        str(destination / "resource.json"),
                        "--",
                        *command,
                    ],
                    stdout=out,
                    stderr=err,
                    env=env,
                    timeout=40,
                    check=False,
                )
            stats = json.loads((destination / "resource.json").read_text())
            if stats["reason"] != "child_exit" or result.returncode not in [0, 1]:
                raise RuntimeError(
                    (
                        name,
                        result.returncode,
                        stats,
                        (destination / "stderr.txt").read_text(),
                    )
                )
            document = json.loads((destination / "stdout.json").read_bytes())
            if document.get("kind") == "plan":
                assert len(document["candidates"]) == expected_candidates, (
                    name,
                    len(document["candidates"]),
                )
                assert document["truncated"] is False, name
            else:
                assert len(document["mutants"]) == expected_candidates, (
                    name,
                    len(document["mutants"]),
                )
                assert document["summary"]["complete"] is True, name
            documents.append(normalized(document))
            RUNS.append(
                dict(
                    case=name,
                    repeat=repeat,
                    label=label,
                    exit_code=result.returncode,
                    elapsed_ms=stats["elapsed_ms"],
                    sampled_tree_rss_bytes=(stats["peak_rss_kib"] * 1024 or None),
                    sample_ms=25,
                    argv=command,
                )
            )
    if any(doc != documents[0] for doc in documents):
        (OUTPUT / f"{name}-normalized.json").write_text(json.dumps(documents, indent=2))
        raise AssertionError(f"{name}: normalized outputs differ; inspect artifact")
    print(name, "equivalent", flush=True)


def common(root, operator):
    return [
        "--root",
        str(root),
        "--file",
        "sample.py",
        "--operators",
        operator,
        "--allow-best-effort-memory",
        "--max-candidates",
        "10000",
        "--min-free-space",
        "1GiB",
    ]


with tempfile.TemporaryDirectory(prefix="six-perf-") as temporary:
    parent = Path(temporary)
    axes = [
        (8, 8),
        (64, 8),
        (256, 8),
        (8, 64),
        (8, 256),
        (256, 256),
        (1000, 1000),
        (2000, 2000),
    ]
    for shape in ["qualified", "private", "related"]:
        for left, right in axes:
            root = parent / f"{shape}-{left}-{right}"
            root.mkdir()
            if shape == "qualified":
                source = "".join(
                    f"import typing as t{i}\n" for i in range(left)
                ) + "".join(f"x{i}: t0.Sequence[int]\n" for i in range(right))
                operator = "type_sequence_iterable"
            elif shape == "private":
                (root / "errors.py").write_text(
                    "class Base(Exception): pass\nclass Child(Base): pass\n"
                )
                source = (
                    "from errors import Base\n"
                    + "".join(
                        f"from errors import Child as a{i}\n" for i in range(left)
                    )
                    + "".join(
                        f"class C{i}:\n def f(self):\n  raise a0()\n"
                        for i in range(right)
                    )
                )
                operator = "exception_hierarchy"
            else:
                (root / "errors.py").write_text(
                    "class Base(Exception): pass\n"
                    + "".join(f"class C{i}(Base): pass\n" for i in range(left))
                )
                source = (
                    "from errors import C0, C1\ndef f():\n" + " raise C0()\n" * right
                )
                operator = "exception_hierarchy"
            (root / "sample.py").write_text(source)
            paired(
                root.name,
                ["plan", *common(root, operator), "--", str(PYTHON), "-c", "pass"],
                right,
            )
    for payload in [4096, 131072]:
        root = parent / f"verify-{payload}"
        root.mkdir()
        text = "x" * payload
        (root / "sample.py").write_text(
            "".join(f"x{i} = ['{text}']\n" for i in range(16))
        )
        plan = subprocess.run(
            [
                str(BASE),
                "plan",
                *common(root, "collection_list_tuple"),
                "--",
                str(PYTHON),
                "-c",
                "pass",
            ],
            capture_output=True,
            timeout=30,
            check=True,
        )
        manifest = parent / f"verify-{payload}.json"
        manifest.write_bytes(plan.stdout)
        for selected in [1, 16]:
            paired(
                f"verify-{payload}-{selected}",
                ["verify", str(manifest), "--top", str(selected)],
                selected,
            )
    for depth, files in [(1, 8), (1, 256), (32, 8), (32, 256)]:
        root = parent / f"reset-{depth}-{files}"
        folder = root.joinpath(*(["nested"] * depth))
        folder.mkdir(parents=True)
        for i in range(files):
            (folder / f"f{i}").write_bytes(b"x")
        (root / "sample.py").write_text("x = True\ny = True\nz = True\n")
        paired(
            root.name,
            [
                "run",
                *common(root, "boolean_literal"),
                "--jobs",
                "1",
                "--max-mutants",
                "3",
                "--",
                str(PYTHON),
                "-c",
                "from pathlib import Path; assert Path('sample.py').read_text().count('True') == 3",
            ],
            3,
        )
    for emitted in [0, 8192, 16 * 1024 * 1024, 64 * 1024 * 1024]:
        root = parent / f"output-{emitted}"
        root.mkdir()
        (root / "sample.py").write_text("x = True\n")
        code = f"import sys; n={emitted}; b=b'x'*8192; [sys.stdout.buffer.write(b[:min(8192,n-i)]) for i in range(0,n,8192)]"
        paired(
            root.name,
            [
                "run",
                *common(root, "boolean_literal"),
                "--jobs",
                "1",
                "--max-mutants",
                "1",
                "--max-output",
                "1MiB",
                "--",
                str(PYTHON),
                "-c",
                code,
            ],
            1,
        )

summary = []
for case in dict.fromkeys(r["case"] for r in RUNS):
    row = {"case": case}
    for label in ["baseline", "candidate"]:
        runs = [r for r in RUNS if r["case"] == case and r["label"] == label]
        row[label + "_median_ms"] = statistics.median(r["elapsed_ms"] for r in runs)
        row[label + "_max_sampled_tree_rss_bytes"] = max(
            (
                r["sampled_tree_rss_bytes"]
                for r in runs
                if r["sampled_tree_rss_bytes"] is not None
            ),
            default=None,
        )
    summary.append(row)
record = dict(
    platform=platform.platform(),
    machine=platform.machine(),
    candidate_revision=subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True
    ).strip(),
    rust=subprocess.check_output(["rustc", "--version"], text=True).strip(),
    python=sys.version,
    lock_sha256=hashlib.sha256((REPO / "Cargo.lock").read_bytes()).hexdigest(),
    probe_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    baseline_sha256=hashlib.sha256(BASE.read_bytes()).hexdigest(),
    candidate_sha256=hashlib.sha256(CANDIDATE.read_bytes()).hexdigest(),
    baseline_revision="a892eee0",
    runs=RUNS,
    summary=summary,
    note="Three alternating pairs; 25ms sampled process-tree RSS, 2GiB/30s guards. Plan JSON compared exactly; run JSON compared after normalization of nondeterministic/resource fields. Allocation gates are separate.",
)
(OUTPUT / "measurements.json").write_text(json.dumps(record, indent=2) + "\n")
print(json.dumps(summary, indent=2))
