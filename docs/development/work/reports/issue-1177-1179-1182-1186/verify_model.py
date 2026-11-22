"""Check the model and two incorrect variants with bounded native Lean calls.

On Windows this uses Lean's internal memory limit and a wall deadline, not the
POSIX aggregate RSS guard. The model imports Std only and starts no subprocesses.
"""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time


HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lean", required=True, help="Native Lean 4.32.2 executable")
    arguments = parser.parse_args()
    version = subprocess.run([arguments.lean, "--version"], capture_output=True,
                             text=True, timeout=10, check=True).stdout
    assert "version 4.32.2," in version, version
    original = (HERE / "Evaluation.lean").read_text(encoding="utf-8")
    variants = [
        ("original", original, 0),
        ("gate_drops_last", original.replace("x.a && x.b && x.c && x.d", "x.a && x.b && x.c"), 1),
        ("rows_are_exhaustive", original.replace("allRows.filter fun x => !(x.a ^^ x.b ^^ x.c ^^ x.d)", "allRows"), 1),
    ]
    records = []
    for name, source, expected in variants:
        with tempfile.TemporaryDirectory(prefix="hoimin-research-model-") as directory:
            path = Path(directory) / "Evaluation.lean"
            path.write_text(source, encoding="utf-8")
            command = [arguments.lean, "-j1", "-M2048", "-DElab.async=false", str(path)]
            begin = time.monotonic()
            result = subprocess.run(command, capture_output=True, text=True, encoding="utf-8",
                                    timeout=30, check=False)
        records.append({"name": name, "seconds": time.monotonic() - begin,
                        "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
        assert result.returncode == expected, records[-1]
        if name == "original":
            assert "sorryAx" not in result.stdout
        else:
            assert source != original and "error:" in result.stdout
    summary = {"version": version, "limits": {"wall_seconds_per_call": 30,
                "lean_internal_memory_mib": 2048, "threads": 1, "aggregate_rss_guard": False},
               "checks": records}
    (HERE / "evidence/model-checks.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
