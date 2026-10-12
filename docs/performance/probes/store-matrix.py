"""Bounded candidate-store measurements; see the accompanying report.

Usage: python store-matrix.py EXE OUTPUT_DIRECTORY LABEL
Run on an otherwise idle build tree, once per version. Never compare build time
with the timed inner loop printed by the Rust driver.
"""
import json
import subprocess
import sys
from pathlib import Path

executable, output, label = sys.argv[1:]
directory = Path(output)
directory.mkdir(parents=True, exist_ok=True)
scenarios = [("small", 100_000, 32, "a"),
             ("escaped", 50_000, 32, '\n\t"\\雪🧪'),
             ("large", 256, 262_144, "a")]
for scenario, count, size, text in scenarios:
    for mode in ("count", "encode", "push", "replay"):
        for repetition in range(3):
            destination = directory / f"{label}-{scenario}-{mode}-{repetition}.json"
            result = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("measure-windows.py")),
                 str(destination), "90", "0", executable, mode, str(count), str(size), text],
                capture_output=True, text=True, check=True, timeout=100,
            )
            observation = json.loads(result.stdout)
            print(destination.name, observation["returncode"], observation["peak_rss_bytes"])
