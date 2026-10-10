"""Capture Cargo SBOM in the build environment (stdlib only, Python 3.9+)."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

GENERATOR_VERSION = "0.5.7"


def run(root: Path, *args: str) -> str:
    return subprocess.run(  # noqa: S603 -- Fixed build tools, argv without a shell.
        args, cwd=root, check=True, capture_output=True, encoding="utf-8", timeout=600
    ).stdout.strip()


def capture(root: Path, target: str, output: Path) -> None:
    root = root.resolve()
    lock = root / "Cargo.lock"
    before = lock.read_bytes()
    # The generator emits one document per workspace member even with a manifest.
    generated = [
        root / "crates" / member / "hoimin-release-sbom.json"
        for member in ("hoimin-cli", "hoimin-core")
    ]
    for path in generated:
        path.unlink(missing_ok=True)
    try:
        version = run(root, "cargo", "cyclonedx", "--version")
        if version != f"cargo-cyclonedx-cyclonedx {GENERATOR_VERSION}":
            msg = f"unexpected SBOM generator: {version}"
            raise ValueError(msg)
        run(
            root,
            "cargo",
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--no-default-features",
            "--filter-platform",
            target,
        )
        run(
            root,
            "cargo",
            "cyclonedx",
            "--manifest-path",
            "crates/hoimin-cli/Cargo.toml",
            "--format",
            "json",
            "--spec-version",
            "1.5",
            "--no-default-features",
            "--target",
            target,
            "--override-filename",
            "hoimin-release-sbom",
        )
        result = {
            "bom": json.loads(generated[0].read_text(encoding="utf-8")),
            "lock_sha256": hashlib.sha256(before).hexdigest(),
            "rustc": run(root, "rustc", "-vV"),
        }
    finally:
        changed = not lock.exists() or lock.read_bytes() != before
        if changed:
            lock.write_bytes(before)
        for path in generated:
            path.unlink(missing_ok=True)
    if changed:
        msg = (
            "SBOM generation changed Cargo.lock; restored original and rejected output"
        )
        raise ValueError(msg)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    capture(args.root, args.target, args.output)


if __name__ == "__main__":
    main()
