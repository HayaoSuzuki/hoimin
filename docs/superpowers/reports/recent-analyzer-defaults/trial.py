"""Compare default discovery with the previous50 set; no tests are executed.

Usage: .venv/bin/python trial.py target/debug/hoimin observations.json
"""
import hashlib
import importlib.metadata
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import iniconfig
import packaging.version

binary = str(Path(sys.argv[1]).resolve())
recent = {"method_call_remove", "function_body_return_constant"}
observations = []
for package, module in [("packaging", packaging.version), ("iniconfig", iniconfig)]:
    source = Path(module.__file__).read_bytes()
    with tempfile.TemporaryDirectory(prefix="hoimin-recent-defaults-trial-") as tmp:
        root = Path(tmp)
        shutil.copytree(Path(module.__file__).parent, root / package,
                        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        selected = str(Path(package) / Path(module.__file__).name)

        def discover(options):
            command = [binary, "plan", "--root", tmp, "--file", selected,
                       "--allow-best-effort-memory", "--max-candidates", "10000",
                       *options, "--", sys.executable, "-B", "-c", "pass"]
            output = subprocess.run(command, capture_output=True, text=True, timeout=30)
            assert output.returncode == 0, output.stderr
            return json.loads(output.stdout)

        current = discover([])
        previous_names = sorted(set(current["normalized_config"]["operators"]) - recent)
        assert len(previous_names) == 50
        previous = discover(["--operators", ",".join(previous_names)])
        opt_out = discover(["--exclude-operators", ",".join(sorted(recent))])
        assert previous["candidates"] == opt_out["candidates"]
        assert not any(p["truncated"] for p in (current, previous, opt_out))
        counts = {name: sum(c["operator"] == name for c in current["candidates"])
                  for name in sorted(recent)}
        observations.append({"module": module.__name__,
                             "version": importlib.metadata.version(package),
                             "source_sha256": hashlib.sha256(source).hexdigest(),
                             "previous50": len(previous["candidates"]),
                             "default52": len(current["candidates"]),
                             "opt_out_both": len(opt_out["candidates"]),
                             "recent_candidates": counts,
                             "exact_opt_out_candidate_equality": True,
                             "truncated": False})
Path(sys.argv[2]).write_text(json.dumps(observations, indent=2) + "\n")
