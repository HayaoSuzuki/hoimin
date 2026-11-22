"""Export the frozen public cases; never check out or modify the supplied repository."""

import argparse
import hashlib
import io
import json
import subprocess
import tarfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent


def prepare(repository, destination):
    cases = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))
    for case in cases:
        started = time.monotonic()
        root = destination / case["name"]
        root.mkdir(parents=True, exist_ok=False)
        tests = (
            "tests/test_querymatcher.py"
            if case["name"] == "empty-query"
            else "tests/test_readiness.py"
        )
        paths = ["pytest_httpserver", tests, "LICENSE"]
        if case["name"] == "readiness":
            paths.append("tests/conftest.py")
        archive = subprocess.check_output(
            ["git", "-C", str(repository), "archive", case["fixed_revision"], *paths],
            timeout=30,
        )
        with tarfile.open(fileobj=io.BytesIO(archive)) as members:
            members.extractall(root, filter="data")
        (root / "fixed-source.bin").write_bytes((root / case["source"]).read_bytes())
        buggy = subprocess.check_output(
            [
                "git",
                "-C",
                str(repository),
                "show",
                case["buggy_revision"] + ":" + case["source"],
            ],
            timeout=30,
        )
        (root / "buggy-source.bin").write_bytes(buggy)
        (root / "pytest.ini").write_text("[pytest]\n", encoding="utf-8")
        files = {
            path.relative_to(root).as_posix(): hashlib.sha256(
                path.read_bytes()
            ).hexdigest()
            for path in sorted(root.rglob("*"))
            if path.is_file()
        }
        receipt = {
            "case": case,
            "files": files,
            "archive_sha256": hashlib.sha256(archive).hexdigest(),
            "export_seconds": time.monotonic() - started,
        }
        (root / "prepared.json").write_text(
            json.dumps(receipt, indent=2), encoding="utf-8"
        )
        print(json.dumps({"case": case["name"], "prepared_files": len(files)}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--projects", type=Path, required=True)
    args = parser.parse_args()
    prepare(args.repository.resolve(), args.projects.resolve())
