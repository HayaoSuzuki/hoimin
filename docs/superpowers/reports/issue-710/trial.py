"""Equal-budget authored examples; not a representative project benchmark.

Usage: .venv/bin/python trial.py target/debug/hoimin observations.json
Uses only stdlib; temporary projects are deleted after each scenario.
"""

import hashlib
import json
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

BINARY = str(Path(sys.argv[1]).resolve())
POLICIES = ("strict", "diverse", "line-diverse")


def invoke(args, accepted):
    start = time.monotonic()
    result = subprocess.run([BINARY, *args], text=True, capture_output=True, timeout=35)
    assert result.returncode in accepted, (result.returncode, result.stderr)
    return json.loads(result.stdout), round(time.monotonic() - start, 6)


def experiment(split):
    with tempfile.TemporaryDirectory(prefix="hoimin-710-") as tmp:
        root = Path(tmp) / "project"
        root.mkdir()
        dense = (
            "def dense(x):\n    return "
            + " or ".join(f"x == {n}" for n in range(1, 11))
            + "\n"
        )
        other = (
            "def ignored(x):\n    return x == 0\ndef another(x):\n    return x == 1\n"
        )
        sources = {"a.py": dense, "b.py": other} if split else {"a.py": dense + other}
        for name, source in sources.items():
            (root / name).write_text(source)
        imports = (
            "from a import dense\nfrom b import ignored, another\n"
            if split
            else "from a import dense, ignored, another\n"
        )
        weak = (
            imports
            + "for x in range(12):\n    assert dense(x) == (1 <= x <= 10)\nignored(0)\nanother(1)\n"
        )
        strong = weak + "assert ignored(0) is True\nassert another(1) is True\n"
        results = {}
        for strength, command in [("weak", weak), ("strong", strong)]:
            plan, _ = invoke(
                [
                    "plan",
                    "--root",
                    str(root),
                    "--source",
                    ".",
                    "--operators",
                    "compare_eq_ne",
                    "--max-mutants",
                    "3",
                    "--jobs",
                    "1",
                    "--min-free-space",
                    "1B",
                    "--baseline-timeout",
                    "5s",
                    "--mutant-timeout",
                    "5s",
                    "--total-timeout",
                    "30s",
                    "--allow-best-effort-memory",
                    "--",
                    sys.executable,
                    "-B",
                    "-c",
                    command,
                ],
                (0,),
            )
            assert len(plan["candidates"]) == 12 and not plan["truncated"]
            manifest = Path(tmp) / f"{strength}.json"
            manifest.write_text(json.dumps(plan))
            rows = {policy: [] for policy in POLICIES}
            for repetition in range(5 if strength == "weak" else 1):
                # Rotate policy order to reduce systematic first-run effects.
                for policy in POLICIES[repetition % 3 :] + POLICIES[: repetition % 3]:
                    report, elapsed = invoke(
                        [
                            "verify",
                            str(manifest),
                            "--top",
                            "3",
                            "--selection-policy",
                            policy,
                            "--format",
                            "json",
                        ],
                        (0, 1),
                    )
                    assert report["summary"]["complete"] and report["baseline"][
                        "termination"
                    ] == {"Exit": 0}
                    mutants = [
                        {
                            "id": m["candidate"]["id"],
                            "path": Path(m["candidate"]["path"]).name,
                            "line": m["candidate"]["line"],
                            "status": m["status"],
                        }
                        for m in report["mutants"]
                    ]
                    assert len(mutants) == 3 and all(
                        m["status"] in ("killed", "survived") for m in mutants
                    )
                    rows[policy].append(
                        {
                            "elapsed_seconds": elapsed,
                            "distinct_start_lines": len(
                                {(m["path"], m["line"]) for m in mutants}
                            ),
                            "mutants": mutants,
                        }
                    )
                    assert all(
                        (root / name).read_text() == source
                        for name, source in sources.items()
                    )
            results[strength] = rows
        for policy in POLICIES:
            weak_rows = results["weak"][policy]
            assert all(row["mutants"] == weak_rows[0]["mutants"] for row in weak_rows)
            strong_row = results["strong"][policy][0]
            assert [m["id"] for m in strong_row["mutants"]] == [
                m["id"] for m in weak_rows[0]["mutants"]
            ]
            assert all(m["status"] == "killed" for m in strong_row["mutants"])
        return {
            "fixture": "two_files" if split else "one_file",
            "sources": sources,
            "sha256": {
                n: hashlib.sha256(s.encode()).hexdigest() for n, s in sources.items()
            },
            "weak_command": weak,
            "strong_command": strong,
            "results": results,
            "median_weak_seconds": {
                p: statistics.median(r["elapsed_seconds"] for r in results["weak"][p])
                for p in POLICIES
            },
        }


result = {
    "scope": "authored deterministic comparison; no prevalence or real-defect claim",
    "python": sys.version,
    "budget": 3,
    "repetitions_weak": 5,
    "repetitions_strong": 1,
    "scenarios": [experiment(False), experiment(True)],
}
Path(sys.argv[2]).write_text(json.dumps(result, indent=2) + "\n")
