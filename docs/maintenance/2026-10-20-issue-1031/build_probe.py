"""Build the common probe against one exact checkout and isolated Cargo target.

Example (run baseline and changed sequentially):
  python3 build_probe.py --revision baseline --worktree BASE --target TARGET --output PROBE
Only the protocol adapter uses --cfg baseline. The copied production store gets
one identical read counter hook; the original sources are never edited.
"""
import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import time


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def capture_sources(root):
    paths = [
        *sorted((root / "crates/hoimin-core/src").rglob("*.rs")),
        root / "crates/hoimin-core/Cargo.toml",
        root / "crates/hoimin-cli/src/analyzer/store.rs",
        root / "crates/hoimin-cli/src/plan/sampling.rs",
    ]
    return {str(path.relative_to(root)): sha256(path) for path in paths}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", choices=["baseline", "changed"], required=True)
    parser.add_argument("--worktree", type=pathlib.Path, required=True)
    parser.add_argument("--target", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--profile", choices=["release", "debug"], default="release")
    args = parser.parse_args()
    root, target, output = (path.resolve() for path in (args.worktree, args.target, args.output))
    output.parent.mkdir(parents=True, exist_ok=True)
    source_dir = pathlib.Path(__file__).resolve().parent
    source = source_dir / "probe.rs"
    before = capture_sources(root)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if args.revision == "baseline":
        assert head == "3054427a1b1b9a7d2f5cdd1b6ce62053e6c417ac", head
        assert not subprocess.check_output(
            ["git", "diff", "--", "crates/hoimin-core", "crates/hoimin-cli/src/analyzer/store.rs",
             "crates/hoimin-cli/src/plan/sampling.rs"], cwd=root
        ), "baseline production sources must remain immutable"

    env = dict(os.environ, CARGO_TARGET_DIR=str(target))
    cargo = ["cargo", "build", "--offline", "--locked", "-p", "hoimin-cli", "--lib",
             "--message-format=json-render-diagnostics"]
    if args.profile == "release":
        cargo.append("--release")
    cargo_stdout = output.with_suffix(".cargo.jsonl")
    cargo_stderr = output.with_suffix(".cargo.log")
    started = time.monotonic()
    with cargo_stdout.open("w") as stdout, cargo_stderr.open("w") as stderr:
        subprocess.run(cargo, cwd=root, env=env, stdout=stdout, stderr=stderr, check=True)
    artifacts = {}
    for line in cargo_stdout.read_text().splitlines():
        item = json.loads(line)
        if item.get("reason") == "compiler-artifact":
            rlibs = [pathlib.Path(path) for path in item["filenames"] if path.endswith(".rlib")]
            if rlibs:
                assert len(rlibs) == 1, item
                artifacts[item["target"]["name"]] = rlibs[0]
    assert capture_sources(root) == before, "production sources changed during Cargo compilation"

    copied_store = output.with_suffix(".store.rs")
    store_original = root / "crates/hoimin-cli/src/analyzer/store.rs"
    store_text = store_original.read_text()
    anchor = "    if read == 0 {\n"
    assert store_text.count(anchor) == 1, "record-read hook anchor must be unique"
    hook = "    crate::io_counts::record_read(read);\n"
    copied_store.write_text(store_text.replace(anchor, hook + anchor))
    sampling = root / "crates/hoimin-cli/src/plan/sampling.rs"
    modules = output.with_suffix(".modules.rs")
    modules.write_text(
        "#[allow(dead_code)]\n"
        f"#[path = {json.dumps(str(copied_store))}]\nmod store;\n"
        f"#[path = {json.dumps(str(sampling))}]\nmod sampling;\n"
    )
    env["HOIMIN_PROBE_MODULES"] = str(modules)
    dependencies = target / args.profile / "deps"
    command = ["rustc", "--edition=2024", str(source), "-L", f"dependency={dependencies}"]
    if args.profile == "release":
        command += ["-O", "-C", "lto=thin", "-C", "embed-bitcode=yes"]
    else:
        command += ["-C", "debuginfo=1"]
    if args.revision == "baseline":
        command += ["--cfg", "baseline"]
    selected_artifacts = {}
    for name in ["hoimin_core", "hoimin_python", "thiserror", "tempfile", "serde_json", "blake3"]:
        path = artifacts[name]
        command += ["--extern", f"{name}={path}"]
        selected_artifacts[str(path)] = sha256(path)
    command += ["-o", str(output)]
    compile_log = output.with_suffix(".rustc.log")
    with compile_log.open("w") as log:
        subprocess.run(command, cwd=root, env=env, stdout=log, stderr=log, check=True)
    assert capture_sources(root) == before, "production sources changed during probe compilation"
    metadata = {
        "revision": args.revision,
        "profile": args.profile,
        "head": head,
        "worktree": str(root),
        "cargo_target_dir": str(target),
        "cargo_command": cargo,
        "rustc_command": command,
        "rustc": subprocess.check_output(["rustc", "-vV"], text=True),
        "build_elapsed_seconds": time.monotonic() - started,
        "production_sources_sha256": before,
        "probe_sources_sha256": {
            str(path): sha256(path)
            for path in [source, source_dir / "requested_heap.rs", pathlib.Path(__file__).resolve()]
        },
        "instrumentation": {
            "source": str(store_original),
            "original_sha256": sha256(store_original),
            "copy": str(copied_store),
            "instrumented_sha256": sha256(copied_store),
            "anchor": anchor,
            "inserted_before_anchor": hook,
            "semantics": "count actual successful nonempty read_until records and logical JSONL bytes, excluding EOF",
        },
        "rlib_sha256": selected_artifacts,
        "binary_sha256": sha256(output),
    }
    output.with_suffix(".build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(output, flush=True)


if __name__ == "__main__":
    main()
