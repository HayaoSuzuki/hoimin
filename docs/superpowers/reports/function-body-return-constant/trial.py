"""Bounded local source probes, not upstream test-suite effectiveness claims.

Usage: .venv/bin/python trial.py target/debug/hoimin observations.json
"""
import hashlib
import importlib.metadata
import json
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import packaging.markers
import packaging.specifiers
import packaging.version

binary = str(Path(sys.argv[1]).resolve())
rows = []
for package, module, command in [
    ("packaging", packaging.markers, "from packaging.markers import Marker; assert Marker('python_version >= \"3.0\"').evaluate(); assert not Marker('python_version < \"3.0\"').evaluate()"),
    ("packaging", packaging.version, "from packaging.version import Version; assert str(Version(' 1.2 ')) == '1.2'; assert Version('1.2') < Version('2.0')"),
    ("packaging", packaging.specifiers, "from packaging.specifiers import Specifier; assert Specifier('>=1.0').contains('2.0'); assert not Specifier('>=1.0').contains('0.9')"),
]:
    source = Path(module.__file__).read_bytes()
    with tempfile.TemporaryDirectory(prefix="hoimin-return-probe-") as tmp:
        root = Path(tmp)
        shutil.copytree(Path(module.__file__).parent, root / package, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        selected = str(Path(package) / Path(module.__file__).name)
        args = [binary, "plan", "--root", tmp, "--file", selected, "--operators", "function_body_return_constant", "--jobs", "1", "--min-free-space", "1B", "--baseline-timeout", "5s", "--mutant-timeout", "5s", "--total-timeout", "30s", "--allow-best-effort-memory", "--", sys.executable, "-B", "-c", command]
        start = time.monotonic()
        result = subprocess.run(args, capture_output=True, text=True, timeout=35)
        if result.returncode != 0:
            # Preserve discovery failures separately from mutation outcomes.
            legacy_args = args.copy()
            legacy_args[legacy_args.index("function_body_return_constant")] = "function_body_erase"
            legacy = subprocess.run(legacy_args, capture_output=True, text=True, timeout=35)
            rows.append({"module": module.__name__, "version": importlib.metadata.version(package), "sha256": hashlib.sha256(source).hexdigest(), "discovery_exit": result.returncode, "diagnostic": result.stderr, "existing_operator_exit": legacy.returncode, "existing_operator_diagnostic": legacy.stderr})
            continue
        plan = json.loads(result.stdout)
        manifest = root / "plan.json"
        manifest.write_text(json.dumps(plan))
        result = subprocess.run([binary, "verify", str(manifest), "--top", "3", "--format", "json"], capture_output=True, text=True, timeout=35)
        assert result.returncode in (0, 1), result.stderr
        report = json.loads(result.stdout)
        rows.append({"module": module.__name__, "package": package, "version": importlib.metadata.version(package), "sha256": hashlib.sha256(source).hexdigest(), "candidate_count": len(plan['candidates']), "truncated": plan['truncated'], "command": command, "baseline": report['baseline']['termination'], "counts": report['summary']['counts'], "complete": report['summary']['complete'], "elapsed_seconds": round(time.monotonic()-start, 6), "mutants": [{"line": m['candidate']['line'], "original": m['candidate']['original'], "replacement": m['candidate']['replacement'], "status": m['status']} for m in report['mutants']]})
Path(sys.argv[2]).write_text(json.dumps(rows, indent=2)+"\n")
