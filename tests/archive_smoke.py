"""Run a verified release archive outside the checkout; private CI helper."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import stat
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

from wheel_smoke import exercise_cli, isolated_environment, run


def extract_binary(archive: Path, root: Path, binary_name: str) -> Path:
    expected = {
        binary_name,
        "README.md",
        "LICENSE",
        "vendor/ruff_python_parser/LICENSE",
    }
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as zipped:
            members = zipped.infolist()
            assert len(members) == len(expected)
            assert {m.filename for m in members} == expected
            assert all(
                stat.S_IFMT(m.external_attr >> 16) in {0, stat.S_IFREG} for m in members
            )
            data = zipped.read(binary_name)
    else:
        with tarfile.open(archive, "r:gz") as tarred:
            entries = tarred.getmembers()
            assert len(entries) == len(expected)
            assert {m.name for m in entries} == expected
            assert all(m.isfile() for m in entries)
            stream = tarred.extractfile(binary_name)
            assert stream is not None
            with stream:
                data = stream.read()
    assert data, "empty packaged executable"
    # No archive path is extracted. Only the validated regular executable's bytes
    # are copied to a fixed local name; notices are checked but not materialized.
    root.mkdir()
    binary = root / binary_name
    binary.write_bytes(data)
    binary.chmod(0o755)
    return binary


def main() -> int:
    archive = Path(os.environ["ARCHIVE"]).resolve(strict=True)
    version = os.environ["VERSION"]
    with tempfile.TemporaryDirectory(prefix="hoimin-archive-smoke-") as temporary:
        root = Path(temporary)
        binary = extract_binary(
            archive, root / "unpacked", "hoimin.exe" if os.name == "nt" else "hoimin"
        )
        exercise_cli(binary, Path(sys.executable), root / "project", version)
        evidence = {
            "archive": archive.name,
            "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "version": version,
            "system": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "libc": platform.libc_ver(),
        }
        if sys.platform.startswith("linux"):
            abi = run(
                ["readelf", "--version-info", str(binary)],
                cwd=root,
                env=isolated_environment(os.environ),
            ).stdout
            symbols = {
                (int(major), int(minor))
                for major, minor in re.findall(r"GLIBC_(\d+)\.(\d+)", abi)
            }
            assert symbols, abi
            assert max(symbols) <= (2, 35), abi
            evidence["glibc_symbols"] = sorted(symbols)
        Path(os.environ["SMOKE_REPORT"]).write_text(
            json.dumps(evidence, indent=2) + "\n", encoding="utf-8"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
