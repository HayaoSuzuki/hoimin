"""Execute the checked-in boundary registry and preserve evidence, including gaps."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "tests/fixtures/boundary-contracts.json"
MARKER = "BOUNDARY_OBSERVATION "


def observations(log):
    result = []
    for line in log.splitlines():
        if MARKER in line:
            result.append(json.loads(line.split(MARKER, 1)[1]))
    return result


def classify(code, log, test):
    if "BOUNDARY_INFRASTRUCTURE:" in log or any(row["status"] == "infrastructure-error" for row in observations(log)):
        return "infrastructure-error"
    if any(row["status"] == "mismatch" for row in observations(log)):
        return "mismatch"
    if "SKIP:" in log or any(row["status"] == "unexecuted" for row in observations(log)):
        return "unexecuted"
    if code == 0 and f"test {test} ..." in log and "test result: ok. 1 passed;" in log:
        return "match"
    if code != 0 and f"test {test} ..." in log and "FAILED" in log:
        return "mismatch"
    return "infrastructure-error"


def load_registry(path=REGISTRY):
    registry = json.loads(path.read_text())
    assert registry["schema"] == 1
    ids = set()
    for case in registry["cases"]:
        assert case["id"] not in ids
        ids.add(case["id"])
        assert case["mode"] in ("strict", "report")
        assert case["evidence"] in ("public-handler", "public-handler-sqlite", "real-cli", "real-cli-sqlite", "native-backend")
        assert case["premise"] and case["observation"] and case["sources"]
        if case["mode"] == "report":
            assert case["reason"]
        else:
            assert 0 < case["timeout_seconds"] <= 180
            source = ROOT / "crates" / case["package"] / "tests" / (case["target"] + ".rs")
            assert ("fn " + case["test"] + "(") in source.read_text(), case["id"]
    return registry


def execute(case, directory):
    row = {key: case[key] for key in ("id", "boundary", "mode", "evidence", "premise", "observation", "sources")}
    if case["mode"] == "report" or sys.platform not in case["platforms"]:
        return row | {"status": "unexecuted", "reason": case.get("reason", "native platform unavailable")}
    argv = ["cargo", "test", "-p", case["package"], "--test", case["target"], case["test"], "--", "--exact", "--nocapture"]
    log_path = directory / (case["id"] + ".log")
    start = time.monotonic()
    code = None
    environment = os.environ.copy()
    for selector in ("HOIMIN_BOUNDARY_CASE", "HOIMIN_SESSION_ORACLE_CASE"):
        environment.pop(selector, None)
    try:
        with log_path.open("w") as log:
            child = subprocess.Popen(argv, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, start_new_session=True, env=environment)
            try:
                code = child.wait(timeout=case["timeout_seconds"])
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=5)
                return row | {"status": "infrastructure-error", "reason": "external deadline expired; process group killed", "command": argv, "log": str(log_path)}
        log = log_path.read_text(errors="replace")
        status = classify(code, log, case["test"])
        return row | {"status": status, "returncode": code, "command": argv, "seconds": time.monotonic() - start, "log": str(log_path), "cases": observations(log)}
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        return row | {"status": "infrastructure-error", "reason": str(error), "command": argv, "returncode": code, "log": str(log_path) if log_path.exists() else None}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["strict", "report"])
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--from-results", type=Path)
    args = parser.parse_args(argv)
    registry = load_registry()
    digest = hashlib.sha256(REGISTRY.read_bytes()).hexdigest()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.mode == "strict":
        rows = [execute(case, args.output) for case in registry["cases"]]
        report = {"schema": 1, "registry_sha256": digest, "platform": sys.platform, "rows": rows}
    else:
        if args.from_results is None or not args.from_results.exists():
            report = {"schema": 1, "registry_sha256": digest, "platform": sys.platform,
                      "rows": [case | {"status": "unexecuted", "reason": "strict results unavailable; earlier gate did not execute"} for case in registry["cases"]]}
        else:
            report = json.loads(args.from_results.read_text())
        if (report["registry_sha256"] != digest
                or len(report["rows"]) != len(registry["cases"])
                or {row["id"] for row in report["rows"]} != {case["id"] for case in registry["cases"]}
                or any(row["status"] not in ("match", "mismatch", "infrastructure-error", "unexecuted") for row in report["rows"])):
            parser.error("captured results do not match the current complete registry")
    destination = args.output / (args.mode + ".json")
    destination.write_text(json.dumps(report, indent=2) + "\n")
    counts = {status: sum(row["status"] == status for row in report["rows"]) for status in ["match", "mismatch", "infrastructure-error", "unexecuted"]}
    print(json.dumps({"report": str(destination), "counts": counts}))
    return int(args.mode == "strict" and any(row["mode"] == "strict" and row["status"] != "match" for row in report["rows"]))


if __name__ == "__main__":
    raise SystemExit(main())
