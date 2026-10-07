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

import iniconfig
import packaging.version

binary = str(Path(sys.argv[1]).resolve())
rows = []
for package, module, command in [
    ("packaging", packaging.version, "from packaging.version import Version; assert str(Version(' 1.2 ')) == '1.2'; assert Version('1.2') < Version('2.0')"),
    ("iniconfig", iniconfig, "from iniconfig import IniConfig; c=IniConfig('sample.ini', data='[section]\\nkey = value\\n'); assert c['section']['key'] == 'value'"),
]:
    source = Path(module.__file__).read_bytes()
    with tempfile.TemporaryDirectory(prefix="hoimin-method-probe-") as tmp:
        root = Path(tmp)
        shutil.copytree(Path(module.__file__).parent, root / package, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        selected = str(Path(package) / Path(module.__file__).name)
        args = [binary, "plan", "--root", tmp, "--file", selected, "--operators", "method_call_remove", "--jobs", "1", "--min-free-space", "1B", "--baseline-timeout", "5s", "--mutant-timeout", "5s", "--total-timeout", "30s", "--allow-best-effort-memory", "--", sys.executable, "-B", "-c", command]
        start = time.monotonic()
        result = subprocess.run(args, capture_output=True, text=True, timeout=35)
        assert result.returncode == 0, result.stderr
        plan = json.loads(result.stdout)
        manifest = root / "plan.json"
        manifest.write_text(json.dumps(plan))
        result = subprocess.run([binary, "verify", str(manifest), "--top", "3", "--format", "json"], capture_output=True, text=True, timeout=35)
        assert result.returncode in (0, 1), result.stderr
        report = json.loads(result.stdout)
        rows.append({"package": package, "version": importlib.metadata.version(package), "sha256": hashlib.sha256(source).hexdigest(), "candidate_count": len(plan['candidates']), "truncated": plan['truncated'], "command": command, "baseline": report['baseline']['termination'], "counts": report['summary']['counts'], "complete": report['summary']['complete'], "elapsed_seconds": round(time.monotonic()-start, 6), "mutants": [{"line": m['candidate']['line'], "original": m['candidate']['original'], "replacement": m['candidate']['replacement'], "status": m['status']} for m in report['mutants']]})
Path(sys.argv[2]).write_text(json.dumps(rows, indent=2)+"\n")
