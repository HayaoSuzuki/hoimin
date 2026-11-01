"""Temporary logging at real raw-hash call sites; restores exact bytes."""

import os
import subprocess
import sys
from pathlib import Path

mode = sys.argv[1]
assert mode in ("before", "after")
cli = Path("crates/hoimin-cli/src/workspace/mutation.rs")
core = Path("crates/hoimin-core/src/candidate.rs")
original = {cli: cli.read_bytes(), core: core.read_bytes()}
source = (
    subprocess.check_output(["git", "show", f"12b34294:{cli}"])
    if mode == "before"
    else original[cli]
).decode()
if mode == "before":
    old = "        let actual_hash = blake3::hash(&bytes);"
    new = '        eprintln!("mutation-raw-hash bytes={}", bytes.len());\n' + old
else:
    old = "        Err(_) => blake3::hash(source) == expected_hash,"
    new = '        Err(_) => { eprintln!("mutation-raw-hash bytes={}", source.len()); blake3::hash(source) == expected_hash },'
assert source.count(old) == 1
source = source.replace(old, new, 1)
core_source = (
    subprocess.check_output(["git", "show", f"12b34294:{core}"])
    if mode == "before"
    else original[core]
).decode()
old = "            file_hash: blake3::hash(source).to_hex().to_string(),"
new = '            file_hash: { eprintln!("mutation-raw-hash bytes={}", source.len()); blake3::hash(source).to_hex().to_string() },'
assert core_source.count(old + "\n            source_index:") == 1
core_source = core_source.replace(old, new, 1)
log = Path(f".superpowers/implementation-1077-1078-1079/mutation-hashes-{mode}.log")
log.parent.mkdir(parents=True, exist_ok=True)
try:
    cli.write_text(source)
    core.write_text(core_source)
    env = os.environ | {
        "CARGO_BUILD_JOBS": "2",
        "CARGO_PROFILE_DEV_DEBUG": "0",
        "CARGO_INCREMENTAL": "0",
    }
    with log.open("w") as stream:
        result = subprocess.run(
            [
                "cargo",
                "test",
                "--locked",
                "-p",
                "hoimin-cli",
                "--test",
                "mutation_source_heap",
                "--",
                "--nocapture",
            ],
            check=False,
            env=env,
            stdout=stream,
            stderr=subprocess.STDOUT,
        )
finally:
    for path, data in original.items():
        path.write_bytes(data)
    print("Restored exact original source bytes", flush=True)
windows = []
current = None
for line in log.read_text().splitlines():
    if line.startswith("mutation-window begin"):
        current = {"window": line, "raw_hashes": 0}
    elif line.startswith("mutation-raw-hash") and current is not None:
        current["raw_hashes"] += 1
    elif line.startswith("mutation-window end") and current is not None:
        windows.append(current)
        current = None
assert len(windows) == 9, log
for observation in windows:
    print(observation)
assert all(x["raw_hashes"] == (2 if mode == "before" else 1) for x in windows), windows
# The before implementation deliberately fails the separate heap bound.
assert result.returncode == (101 if mode == "before" else 0), result.returncode
