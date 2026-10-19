# ruff: noqa: INP001
# Standalone audit script/test, not an importable package.
"""Optional sampled process-tree RSS; separate from uninstrumented timing."""

import json
import subprocess
from pathlib import Path

import measure

ROOT = Path("/tmp/hoimin-priority-six-task6")  # noqa: S108 - Authorized task-owned artifact directory.
GUARD = Path.cwd() / "formal/HoiminOracle/tools/lean_resource_guard.py"
measure.verify_fixtures(ROOT)
rows = []
for n in measure.SIZES:
    for repeat in range(-1, 5):
        for shape in ("dag", "control") if repeat % 2 else ("control", "dag"):
            for label in (
                ("baseline", "candidate") if repeat % 2 else ("candidate", "baseline")
            ):
                prefix = ROOT / f"rss-{label}-{shape}-{n}-{repeat}"
                fixture = ROOT / "fixtures" / f"{shape}-{n}"
                command = [
                    str(ROOT / f"{label}-hoimin"),
                    "plan",
                    "--root",
                    str(fixture),
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
                guard = [
                    "python3",
                    str(GUARD),
                    "--timeout-seconds",
                    "30",
                    "--rss-limit-mib",
                    "2048",
                    "--sample-ms",
                    "10",
                    "--stats",
                    str(prefix.with_suffix(".stats.json")),
                    "--",
                    *command,
                ]
                with (
                    prefix.with_suffix(".stdout.json").open("wb") as out,
                    prefix.with_suffix(".stderr.txt").open("wb") as err,
                ):
                    result = subprocess.run(  # noqa: S603 - Trusted saved binaries/artifact argv; no shell.
                        guard, stdout=out, stderr=err, timeout=40, check=False
                    )
                stats = json.loads(prefix.with_suffix(".stats.json").read_text())
                assert result.returncode == 0, (prefix, stats)
                assert stats["reason"] == "child_exit", (prefix, stats)
                candidate_sha = measure.validate(
                    json.loads(prefix.with_suffix(".stdout.json").read_text())
                )
                rows.append(
                    {
                        "label": label,
                        "shape": shape,
                        "n": n,
                        "repeat": repeat,
                        "argv": command,
                        "sample_ms": 10,
                        "stats": stats,
                        "candidates_sha256": candidate_sha,
                    }
                )
assert len({r["candidates_sha256"] for r in rows}) == 1
(ROOT / "rss-measurements.json").write_text(json.dumps(rows, indent=2) + "\n")
print(  # noqa: T201 - Measurement CLI output.
    f"{len(rows)} sampled RSS observations; includes warmups; "
    "no timing comparison to unguarded runs"
)
