# ruff: noqa: INP001
# Standalone audit script/test, not an importable package.
import json
import subprocess
import sys
from pathlib import Path

root = Path("/tmp/hoimin-priority-six-task6")  # noqa: S108 - Authorized task-owned artifact directory.
label = sys.argv[1]
records = [
    json.loads(line)
    for line in (root / f"{label}-library.jsonl").read_text().splitlines()
]
artifacts = {
    record["target"]["name"]: record
    for record in records
    if record.get("reason") == "compiler-artifact"
}
externs = []
for name in ("hoimin_python", "camino"):
    files = [p for p in artifacts[name]["filenames"] if p.endswith(".rlib")]
    assert len(files) == 1, (name, files)
    externs.extend(["--extern", name + "=" + files[0]])
command = [
    "rustc",
    "--edition=2024",
    "-O",
    "-C",
    "lto=thin",
    "-L",
    "dependency=/tmp/hoimin-priority-six-target/release/deps",
    *externs,
    str(root / "heap_probe.rs"),
    "-o",
    str(root / f"{label}-heap"),
]
(root / f"{label}-probe-command.json").write_text(json.dumps(command, indent=2) + "\n")
subprocess.run(command, check=True)  # noqa: S603 - Trusted exact Cargo artifact argv; no shell.
