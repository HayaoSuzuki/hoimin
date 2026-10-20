"""Link a probe against exact release artifacts and record its inputs.

Usage: python3 build_probe.py TARGET_DIR SOURCE_RS OUTPUT_BINARY
Unique release dependency artifacts are required, so stale variants fail closed.
The JSON sidecar records the actual command, toolchain, source and rlib hashes.
"""

import hashlib
import json
import pathlib
import subprocess
import sys


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    target, source, output = (pathlib.Path(arg).resolve() for arg in sys.argv[1:])
    deps = target / "release" / "deps"
    command = [
        "rustc",
        "--edition=2024",
        "-O",
        "-C",
        "lto=thin",
        "-C",
        "embed-bitcode=yes",
        str(source),
        "-L",
        f"dependency={deps}",
    ]
    artifacts = {}
    for crate in ["camino", "hoimin_core", "hoimin_python", "blake3", "serde_json"]:
        matches = list(deps.glob(f"lib{crate}-*.rlib"))
        if len(matches) != 1:
            raise RuntimeError(f"{crate}: expected one release artifact, found {matches}")
        artifacts[str(matches[0])] = digest(matches[0])
        command += ["--extern", f"{crate}={matches[0]}"]
    output.parent.mkdir(parents=True, exist_ok=True)
    command += ["-o", str(output)]
    subprocess.run(command, check=True, timeout=180)
    metadata = {
        "command": command,
        "rustc": subprocess.check_output(["rustc", "-vV"], text=True),
        "source_sha256": {
            str(path): digest(path)
            for path in [source, source.with_name("requested_heap.rs"), pathlib.Path(__file__).resolve()]
        },
        "rlib_sha256": artifacts,
        "binary_sha256": digest(output),
    }
    output.with_suffix(".build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
