"""Alternate fixed before/after CLI binaries on a bounded synthetic project.

Usage: python cli-pairs.py BEFORE.exe AFTER.exe OUTPUT_DIRECTORY
"""
import json
import subprocess
import sys
from pathlib import Path

before, after, output = sys.argv[1:]
directory = Path(output).resolve()
directory.mkdir(parents=True, exist_ok=True)
project = directory / "project"
project.mkdir(exist_ok=True)
(project / "calc.py").write_text(
    "\n".join(f"def calc_{i}(x):\n    return x + {i} if x > {i} else x - {i}\n" for i in range(120)),
    encoding="utf-8",
)
for mode in ("plan", "run"):
    for repetition in range(3):
        for label, executable in (("before", before), ("after", after)):
            destination = directory / f"{label}-{mode}-{repetition}.json"
            command = [str(Path(executable).resolve()), mode, "--root", str(project),
                       "--file", "calc.py", "--max-mutants", "3", "--jobs", "1",
                       "--max-candidates", "10000", "--total-timeout", "30s",
                       "--max-memory", "256MiB", "--max-copy-size", "1MiB",
                       "--max-workspace-size", "64MiB"]
            if mode == "run":
                command += ["--format", "json"]
            command += ["--", sys.executable, "-c", "import calc; assert calc.calc_0(3) == 3"]
            result = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("measure-windows.py")),
                 str(destination), "45", "4" if mode == "run" else "0", *command],
                capture_output=True, text=True, check=True, timeout=50,
            )
            observation = json.loads(result.stdout)
            document = json.loads(observation["stdout"])
            if mode == "run":
                assert document["baseline"]["termination"] == {"Exit": 0}
                counts = document["summary"]["counts"]
                assert counts["killed"] + counts["survived"] == 3
                assert all(counts[key] == 0 for key in ("error", "timeout", "out_of_memory", "process_limit"))
                assert document["summary"]["disk"]["stop"] is None
            print(destination.name, observation["wall_seconds"], observation["peak_rss_bytes"])
