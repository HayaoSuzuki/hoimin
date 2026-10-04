"""Execute the checked-in boundary registry and preserve evidence, including gaps."""

import argparse
import hashlib
import json
import os
import signal
import subprocess
import sys
import time
from collections.abc import Sequence
from pathlib import Path
from typing import Literal, NotRequired, TypedDict, TypeGuard

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "tests/fixtures/boundary-contracts.json"
MARKER = "BOUNDARY_OBSERVATION "
MAX_TIMEOUT_SECONDS = 180


class Observation(TypedDict):
    status: str


class CaseMetadata(TypedDict):
    id: str
    boundary: list[str]
    evidence: str
    premise: str
    observation: str
    sources: list[str]


class StrictCase(CaseMetadata):
    mode: Literal["strict"]
    package: str
    target: str
    test: str
    platforms: list[str]
    timeout_seconds: float
    reason: NotRequired[str]


class ReportCase(CaseMetadata):
    mode: Literal["report"]
    reason: str


type RegistryCase = StrictCase | ReportCase


class Registry(TypedDict):
    schema: int
    cases: list[RegistryCase]


class ResultSummary(TypedDict):
    id: str
    status: str
    mode: NotRequired[str]


class ResultRow(ResultSummary):
    boundary: NotRequired[list[str]]
    evidence: NotRequired[str]
    premise: NotRequired[str]
    observation: NotRequired[str]
    sources: NotRequired[list[str]]
    reason: NotRequired[str]
    command: NotRequired[list[str]]
    log: NotRequired[str | None]
    returncode: NotRequired[int | None]
    seconds: NotRequired[float]
    cases: NotRequired[list[Observation]]


class Report(TypedDict):
    registry_sha256: str
    rows: Sequence[ResultSummary]
    schema: NotRequired[int]
    platform: NotRequired[str]


def _is_string_list(value: object) -> TypeGuard[list[str]]:
    return isinstance(value, list) and all(isinstance(item, str) for item in value)


def _is_registry_case(value: object) -> TypeGuard[RegistryCase]:
    if not isinstance(value, dict):
        return False
    if not all(
        isinstance(value.get(key), str)
        for key in ("id", "evidence", "premise", "observation")
    ) or not all(_is_string_list(value.get(key)) for key in ("boundary", "sources")):
        return False
    if value.get("mode") == "report":
        return isinstance(value.get("reason"), str)
    return (
        value.get("mode") == "strict"
        and all(
            isinstance(value.get(key), str) for key in ("package", "target", "test")
        )
        and _is_string_list(value.get("platforms"))
        and isinstance(value.get("timeout_seconds"), (int, float))
        and ("reason" not in value or isinstance(value.get("reason"), str))
    )


def _is_registry(value: object) -> TypeGuard[Registry]:
    if not isinstance(value, dict):
        return False
    cases = value.get("cases")
    return (
        isinstance(value.get("schema"), int)
        and isinstance(cases, list)
        and all(_is_registry_case(case) for case in cases)
    )


def _is_result_summary(value: object) -> TypeGuard[ResultSummary]:
    return (
        isinstance(value, dict)
        and isinstance(value.get("id"), str)
        and isinstance(value.get("status"), str)
        and ("mode" not in value or isinstance(value.get("mode"), str))
    )


def _is_report(value: object) -> TypeGuard[Report]:
    if not isinstance(value, dict):
        return False
    rows = value.get("rows")
    return (
        isinstance(value.get("registry_sha256"), str)
        and isinstance(rows, list)
        and all(_is_result_summary(row) for row in rows)
        and ("schema" not in value or isinstance(value.get("schema"), int))
        and ("platform" not in value or isinstance(value.get("platform"), str))
    )


def _is_observation(value: object) -> TypeGuard[Observation]:
    return isinstance(value, dict) and isinstance(value.get("status"), str)


def observations(log: str) -> list[Observation]:
    rows: list[Observation] = []
    for line in log.splitlines():
        if MARKER not in line:
            continue
        value: object = json.loads(line.split(MARKER, 1)[1])
        if not _is_observation(value):
            message = "boundary observation must be an object with a string status"
            raise ValueError(message)
        rows.append(value)
    return rows


def classify(code: int, log: str, test: str) -> str:
    if "BOUNDARY_INFRASTRUCTURE:" in log or any(
        row["status"] == "infrastructure-error" for row in observations(log)
    ):
        return "infrastructure-error"
    if any(row["status"] == "mismatch" for row in observations(log)):
        return "mismatch"
    if "SKIP:" in log or any(
        row["status"] == "unexecuted" for row in observations(log)
    ):
        return "unexecuted"
    if code == 0 and f"test {test} ..." in log and "test result: ok. 1 passed;" in log:
        return "match"
    if code != 0 and f"test {test} ..." in log and "FAILED" in log:
        return "mismatch"
    return "infrastructure-error"


def load_registry(path: Path = REGISTRY) -> Registry:
    registry: object = json.loads(path.read_text())
    assert _is_registry(registry)
    assert registry["schema"] == 1
    ids = set()
    for case in registry["cases"]:
        assert case["id"] not in ids
        ids.add(case["id"])
        assert case["mode"] in ("strict", "report")
        assert case["evidence"] in (
            "public-handler",
            "public-handler-sqlite",
            "real-cli",
            "real-cli-sqlite",
            "native-backend",
        )
        assert case["premise"]
        assert case["observation"]
        assert case["sources"]
        if case["mode"] == "report":
            assert case["reason"]
        else:
            assert 0 < case["timeout_seconds"] <= MAX_TIMEOUT_SECONDS
            source = (
                ROOT / "crates" / case["package"] / "tests" / (case["target"] + ".rs")
            )
            assert ("fn " + case["test"] + "(") in source.read_text(encoding="utf-8"), (
                case["id"]
            )
    return registry


def execute(case: RegistryCase, directory: Path) -> ResultRow:
    row: CaseMetadata = {
        "id": case["id"],
        "boundary": case["boundary"],
        "evidence": case["evidence"],
        "premise": case["premise"],
        "observation": case["observation"],
        "sources": case["sources"],
    }
    if (
        case["mode"] == "report"
        or sys.platform not in case["platforms"]
        or not hasattr(os, "killpg")
        or not hasattr(signal, "SIGKILL")
    ):
        return {
            **row,
            "mode": case["mode"],
            "status": "unexecuted",
            "reason": case.get("reason", "native platform unavailable"),
        }
    argv = [
        "cargo",
        "test",
        "-p",
        case["package"],
        "--test",
        case["target"],
        case["test"],
        "--",
        "--exact",
        "--nocapture",
    ]
    log_path = directory / (case["id"] + ".log")
    start = time.monotonic()
    code = None
    environment = os.environ.copy()
    for selector in ("HOIMIN_BOUNDARY_CASE", "HOIMIN_SESSION_ORACLE_CASE"):
        environment.pop(selector, None)
    try:
        with log_path.open("w") as log:
            child = subprocess.Popen(  # noqa: S603 - Trusted CLI/test arguments; no shell execution.
                argv,
                cwd=ROOT,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
                env=environment,
            )
            try:
                code = child.wait(timeout=case["timeout_seconds"])
            except subprocess.TimeoutExpired:
                assert hasattr(os, "killpg")
                assert hasattr(signal, "SIGKILL")
                os.killpg(child.pid, signal.SIGKILL)

                child.wait(timeout=5)
                return {
                    **row,
                    "mode": case["mode"],
                    "status": "infrastructure-error",
                    "reason": "external deadline expired; process group killed",
                    "command": argv,
                    "log": str(log_path),
                }
        log = log_path.read_text(errors="replace")
        status = classify(code, log, case["test"])
        return {
            **row,
            "mode": case["mode"],
            "status": status,
            "returncode": code,
            "command": argv,
            "seconds": time.monotonic() - start,
            "log": str(log_path),
            "cases": observations(log),
        }
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        return {
            **row,
            "mode": case["mode"],
            "status": "infrastructure-error",
            "reason": str(error),
            "command": argv,
            "returncode": code,
            "log": str(log_path) if log_path.exists() else None,
        }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["strict", "report"])
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--from-results", type=Path)
    args = parser.parse_args(argv)
    registry = load_registry()
    digest = hashlib.sha256(REGISTRY.read_bytes()).hexdigest()
    args.output.mkdir(parents=True, exist_ok=True)
    report: Report
    if args.mode == "strict":
        rows = [execute(case, args.output) for case in registry["cases"]]
        report = {
            "schema": 1,
            "registry_sha256": digest,
            "platform": sys.platform,
            "rows": rows,
        }
    else:
        if args.from_results is None or not args.from_results.exists():
            unexecuted_rows: list[ResultSummary] = []
            for case in registry["cases"]:
                unexecuted = {
                    **case,
                    "status": "unexecuted",
                    "reason": (
                        "strict results unavailable; earlier gate did not execute"
                    ),
                }
                assert _is_result_summary(unexecuted)
                unexecuted_rows.append(unexecuted)
            report = {
                "schema": 1,
                "registry_sha256": digest,
                "platform": sys.platform,
                "rows": unexecuted_rows,
            }
        else:
            captured: object = json.loads(args.from_results.read_text())
            if not _is_report(captured):
                parser.error(
                    "captured results do not match the current complete registry"
                )
            report = captured
        if (
            report["registry_sha256"] != digest
            or len(report["rows"]) != len(registry["cases"])
            or {row["id"] for row in report["rows"]}
            != {case["id"] for case in registry["cases"]}
            or any(
                row["status"]
                not in ("match", "mismatch", "infrastructure-error", "unexecuted")
                for row in report["rows"]
            )
        ):
            parser.error("captured results do not match the current complete registry")
    destination = args.output / (args.mode + ".json")
    destination.write_text(json.dumps(report, indent=2) + "\n")
    counts = {
        status: sum(row["status"] == status for row in report["rows"])
        for status in ["match", "mismatch", "infrastructure-error", "unexecuted"]
    }
    print(json.dumps({"report": str(destination), "counts": counts}))  # noqa: T201 - CLI status or failure diagnostics.
    return int(
        args.mode == "strict"
        and any(
            row.get("mode") == "strict" and row["status"] != "match"
            for row in report["rows"]
        )
    )


if __name__ == "__main__":
    raise SystemExit(main())
