"""Bounded decision experiment, not an implementation of --repetitions.

Run with the repository .venv Python and an already built hoimin binary:
  .venv/bin/python docs/superpowers/reports/issue-709/probe.py target/debug/hoimin RESULT.json
Each existing verify invocation has its own baseline/budget; this does NOT model
a single invocation sharing one budget. All experiment roots are temporary.
"""

import hashlib
import importlib.metadata
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

BINARY = str(Path(sys.argv[1]).resolve())
PYTHON = sys.executable


def invoke(args):
    started = time.monotonic()
    result = subprocess.run([BINARY, *args], capture_output=True, text=True, timeout=35)
    if result.returncode not in (0, 1, 3, 4):
        raise RuntimeError(
            f"CLI infrastructure exit {result.returncode}: {result.stderr}"
        )
    return json.loads(result.stdout), round(time.monotonic() - started, 6)


def plan(root, path, command, count, operators):
    document, _ = invoke(
        [
            "plan",
            "--root",
            str(root),
            "--file",
            path,
            "--operators",
            operators,
            "--max-mutants",
            str(count),
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
            PYTHON,
            "-B",
            "-c",
            command,
        ]
    )
    assert len(document["candidates"]) >= count
    destination = root.parent / "plan.json"
    destination.write_text(json.dumps(document))
    return destination


def repeated(manifest, count, repetitions):
    rows = []
    for _ in range(repetitions):
        report, elapsed = invoke(
            ["verify", str(manifest), "--top", str(count), "--format", "json"]
        )
        rows.append(
            {
                "elapsed_seconds": elapsed,
                "baseline": report["baseline"]["termination"],
                "complete": report["summary"]["complete"],
                "mutants": [
                    {"id": m["candidate"]["id"], "status": m["status"]}
                    for m in report["mutants"]
                ],
            }
        )
    return rows


def control(mode):
    with tempfile.TemporaryDirectory(prefix="hoimin-709-control-") as tmp:
        base = Path(tmp)
        root = base / "project"
        root.mkdir()
        original = "def value():\n    return 1 + 2\n"
        (root / "subject.py").write_text(original)
        counter = base / "counter"
        outcomes = base / "outcomes.jsonl"
        command = f"""from subject import value
from pathlib import Path
import json, sys
mutated = value() != 3
if not mutated:
    sys.exit(0)
p = Path({str(counter)!r})
n = int(p.read_text()) if p.exists() else 0
p.write_text(str(n + 1))
mode = {mode!r}
first_fails = mode == 'fail' or (mode in ('alternate', 'masked') and n % 2 == 0)
second_fails = mode == 'masked'
with Path({str(outcomes)!r}).open('a') as out:
    out.write(json.dumps([first_fails, second_fails]) + '\\n')
sys.exit(int(first_fails or second_fails))
"""
        manifest = plan(root, "subject.py", command, 1, "binary_add_sub")
        rows = repeated(manifest, 1, 6)
        observed = [r["mutants"][0]["status"] for r in rows]
        expected = {
            "pass": ["survived"] * 6,
            "fail": ["killed"] * 6,
            "alternate": ["killed", "survived"] * 3,
            "masked": ["killed"] * 6,
        }[mode]
        assert observed == expected, (mode, rows)
        assert all(r["baseline"] == {"Exit": 0} and r["complete"] for r in rows)
        assert (root / "subject.py").read_text() == original
        return {
            "mode": mode,
            "observations": rows,
            "per_test_failure_controls": [
                json.loads(s) for s in outcomes.read_text().splitlines()
            ],
        }


def baseline_control():
    with tempfile.TemporaryDirectory(prefix="hoimin-709-baseline-") as tmp:
        base = Path(tmp)
        root = base / "project"
        root.mkdir()
        (root / "subject.py").write_text("value = 1 + 2\n")
        manifest = plan(root, "subject.py", "raise SystemExit(1)", 1, "binary_add_sub")
        rows = repeated(manifest, 1, 2)
        assert all(r["baseline"] == {"Exit": 1} and not r["mutants"] for r in rows)
        return rows


def package_probe(name, target, command):
    source = Path(importlib.util.find_spec(name).origin).parent
    with tempfile.TemporaryDirectory(prefix="hoimin-709-package-") as tmp:
        root = Path(tmp) / "project"
        root.mkdir()
        shutil.copytree(
            source, root / name, ignore=shutil.ignore_patterns("__pycache__", "*.pyc")
        )
        raw = (root / target).read_bytes()
        manifest = plan(
            root,
            target,
            command,
            4,
            "compare_eq_ne,compare_order,binary_add_sub,identity,membership",
        )
        rows = repeated(manifest, 4, 5)
        assert all(
            r["baseline"] == {"Exit": 0} and r["complete"] and len(r["mutants"]) == 4
            for r in rows
        ), rows
        assert (root / target).read_bytes() == raw
        ids = {m["id"] for m in rows[0]["mutants"]}
        assert all({m["id"] for m in r["mutants"]} == ids for r in rows)
        flips = sum(
            len({m["status"] for r in rows for m in r["mutants"] if m["id"] == key}) > 1
            for key in ids
        )
        return {
            "package": name,
            "version": importlib.metadata.version(name),
            "target": target,
            "sha256": hashlib.sha256(raw).hexdigest(),
            "authored_check": command,
            "observations": rows,
            "changed_classifications": flips,
            "first_round_seconds": rows[0]["elapsed_seconds"],
            "five_round_seconds": round(sum(r["elapsed_seconds"] for r in rows), 6),
        }


result = {
    "scope": "Existing independent verify calls; controlled stimuli and narrow authored package checks, not upstream suites or prevalence estimation.",
    "python": sys.version,
    "controls": [control(mode) for mode in ("pass", "fail", "alternate", "masked")],
    "failed_baseline": baseline_control(),
    "packages": [
        package_probe(
            "packaging",
            "packaging/version.py",
            "from packaging.version import Version; assert Version('1.0') < Version('2.0'); assert Version('1.0') == Version('1.0.0'); assert Version('1.0a1') < Version('1.0')",
        ),
        package_probe(
            "iniconfig",
            "iniconfig/__init__.py",
            "from iniconfig import IniConfig; c = IniConfig('example.ini', data='[main]\\na=1\\n'); assert c['main']['a'] == '1'; assert list(c.sections) == ['main']",
        ),
    ],
}
Path(sys.argv[2]).write_text(json.dumps(result, indent=2) + "\n")
print(
    json.dumps(
        {
            "controlled_cases": len(result["controls"]),
            "packages": [
                {
                    k: p[k]
                    for k in (
                        "package",
                        "changed_classifications",
                        "first_round_seconds",
                        "five_round_seconds",
                    )
                }
                for p in result["packages"]
            ],
        },
        indent=2,
    )
)
