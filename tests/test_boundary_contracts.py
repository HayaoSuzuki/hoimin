import hashlib
import json
import os
import signal
import subprocess
from pathlib import Path
from typing import TextIO
from unittest.mock import Mock

import pytest
from pytest_mock import MockerFixture

from tools.boundary_contracts import (
    REGISTRY,
    classify,
    execute,
    load_registry,
    main,
    observations,
)


@pytest.mark.parametrize(
    ("returncode", "log", "expected"),
    [
        (0, "test wanted ... ok\ntest result: ok. 1 passed;", "match"),
        (0, "test result: ok. 0 passed;", "infrastructure-error"),
        (0, "test other ... ok", "infrastructure-error"),
    ],
    ids=["exact-test", "zero-tests", "different-test"],
)
def test_only_exact_executed_test_is_a_match(
    returncode: int, log: str, expected: str
) -> None:
    assert classify(returncode, log, "wanted") == expected


@pytest.mark.parametrize(
    ("returncode", "log", "expected"),
    [
        (0, "SKIP: backend unavailable\ntest wanted ... ok", "unexecuted"),
        (
            0,
            (
                'BOUNDARY_OBSERVATION {"status":"unexecuted"}\n'
                "test wanted ... ok\ntest result: ok. 1 passed;"
            ),
            "unexecuted",
        ),
        (-9, "", "infrastructure-error"),
        (101, "test wanted ... FAILED", "mismatch"),
        (101, "error: compilation failed", "infrastructure-error"),
    ],
    ids=[
        "skip-message",
        "unexecuted-observation",
        "signal",
        "test-failure",
        "compilation-failure",
    ],
)
def test_skip_and_crash_are_not_semantic_success(
    returncode: int, log: str, expected: str
) -> None:
    assert classify(returncode, log, "wanted") == expected


def test_semantic_matrix_errors_override_outer_test_failure() -> None:
    assert (
        classify(
            101,
            'BOUNDARY_OBSERVATION {"status":"infrastructure-error"}\n'
            "test wanted ... FAILED",
            "wanted",
        )
        == "infrastructure-error"
    )


def test_captured_case_output_can_interrupt_the_named_test_line() -> None:
    log = (
        'test wanted ... BOUNDARY_OBSERVATION {"status":"match"}\n'
        "ok\ntest result: ok. 1 passed;"
    )
    assert classify(0, log, "wanted") == "match"


def test_report_mode_preserves_failure_and_complete_unexecuted_rows(
    tmp_path: Path,
) -> None:
    rows = [
        {
            "id": case["id"],
            "mode": case["mode"],
            "status": "mismatch" if case["mode"] == "strict" else "unexecuted",
        }
        for case in load_registry()["cases"]
    ]
    report = {
        "schema": 1,
        "registry_sha256": hashlib.sha256(REGISTRY.read_bytes()).hexdigest(),
        "rows": rows,
        "extra_evidence": {"retained": [1, True, None]},
    }
    source = tmp_path / "captured.json"
    source.write_text(json.dumps(report))
    assert (
        main(["report", "--from-results", str(source), "--output", str(tmp_path)]) == 0
    )
    assert json.loads((tmp_path / "report.json").read_text()) == report


def test_nested_mismatch_cannot_be_laundered_by_outer_success() -> None:
    log = (
        'test wanted ... BOUNDARY_OBSERVATION {"status":"mismatch"}\n'
        "ok\ntest result: ok. 1 passed;"
    )
    assert classify(0, log, "wanted") == "mismatch"


def test_missing_strict_results_are_reported_as_unexecuted(tmp_path: Path) -> None:
    assert main(["report", "--output", str(tmp_path)]) == 0
    rows = json.loads((tmp_path / "report.json").read_text())["rows"]
    assert rows
    assert all(row["status"] == "unexecuted" for row in rows)


def test_fixture_setup_failure_is_infrastructure_not_mismatch() -> None:
    assert (
        classify(
            101,
            "test wanted ... BOUNDARY_INFRASTRUCTURE: "
            "permission fixture unavailable\nFAILED",
            "wanted",
        )
        == "infrastructure-error"
    )


@pytest.mark.parametrize("invalid_case", ["duplicate", "unknown-status"])
def test_report_rejects_duplicate_or_unknown_status_rows(
    tmp_path: Path, invalid_case: str
) -> None:
    rows = [
        {"id": case["id"], "mode": case["mode"], "status": "unexecuted"}
        for case in load_registry()["cases"]
    ]
    source = tmp_path / "captured.json"
    invalid = (
        [*rows, rows[0]]
        if invalid_case == "duplicate"
        else [rows[0] | {"status": "unknown"}, *rows[1:]]
    )
    source.write_text(
        json.dumps(
            {
                "registry_sha256": hashlib.sha256(REGISTRY.read_bytes()).hexdigest(),
                "rows": invalid,
            }
        )
    )
    with pytest.raises(SystemExit) as error:
        main(["report", "--from-results", str(source), "--output", str(tmp_path)])
    assert error.value.code == 2
    assert not (tmp_path / "report.json").exists()


def test_strict_execution_removes_inherited_corpus_filters(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mocker: MockerFixture
) -> None:
    case = load_registry()["cases"][0]

    assert case["mode"] == "strict"

    def spawn(
        _argv: list[str],
        *,
        env: dict[str, str],
        stdout: TextIO,
        **_kwargs: object,
    ) -> Mock:
        assert "HOIMIN_BOUNDARY_CASE" not in env
        assert "HOIMIN_SESSION_ORACLE_CASE" not in env
        assert env["PATH"] == os.environ["PATH"]
        stdout.write(f"test {case['test']} ... ok\ntest result: ok. 1 passed;")
        stdout.flush()
        return Mock(wait=Mock(return_value=0))

    monkeypatch.setenv("HOIMIN_BOUNDARY_CASE", "one")
    monkeypatch.setenv("HOIMIN_SESSION_ORACLE_CASE", "one")
    mocker.patch("tools.boundary_contracts.subprocess.Popen", side_effect=spawn)
    assert execute(case, tmp_path)["status"] == "match"


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("id", 1),
        ("mode", "unknown"),
        ("boundary", "boundary"),
        ("sources", [1]),
        ("package", None),
        ("platforms", [1]),
        ("timeout_seconds", "180"),
        ("reason", 1),
    ],
)
def test_registry_rejects_malformed_case_fields(
    tmp_path: Path, field: str, value: object
) -> None:
    case = dict(load_registry()["cases"][0]) | {field: value}
    source = tmp_path / "registry.json"
    source.write_text(json.dumps({"schema": 1, "cases": [case]}))
    with pytest.raises(AssertionError):
        load_registry(source)


@pytest.mark.parametrize(
    "value",
    [None, [], {"schema": 1, "cases": {}}, {"schema": "1", "cases": []}],
)
def test_registry_rejects_malformed_document(tmp_path: Path, value: object) -> None:
    source = tmp_path / "registry.json"
    source.write_text(json.dumps(value))
    with pytest.raises(AssertionError):
        load_registry(source)


@pytest.mark.parametrize(
    "rows",
    [None, {}, [None], [{"id": 1, "status": "match"}], [{"id": "x"}]],
)
def test_report_rejects_malformed_rows(tmp_path: Path, rows: object) -> None:
    source = tmp_path / "captured.json"
    source.write_text(
        json.dumps(
            {
                "registry_sha256": hashlib.sha256(REGISTRY.read_bytes()).hexdigest(),
                "rows": rows,
            }
        )
    )
    with pytest.raises(SystemExit) as error:
        main(["report", "--from-results", str(source), "--output", str(tmp_path)])
    assert error.value.code == 2
    assert not (tmp_path / "report.json").exists()


def test_timeout_kills_process_group_and_reports_infrastructure_error(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    if not hasattr(os, "killpg") or not hasattr(signal, "SIGKILL"):
        pytest.skip("process-group execution requires POSIX")
    case = load_registry()["cases"][0]
    child = Mock(pid=123, wait=Mock(side_effect=[subprocess.TimeoutExpired([], 1), 0]))
    spawn = mocker.patch(
        "tools.boundary_contracts.subprocess.Popen", return_value=child
    )
    kill = mocker.patch("tools.boundary_contracts.os.killpg")
    row = execute(case, tmp_path)
    spawn.assert_called_once()
    kill.assert_called_once_with(123, signal.SIGKILL)
    assert child.wait.call_args_list[-1].kwargs == {"timeout": 5}
    assert row["status"] == "infrastructure-error"
    assert row["reason"] == "external deadline expired; process group killed"


def test_execution_without_process_group_support_is_unexecuted(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mocker: MockerFixture
) -> None:
    case = load_registry()["cases"][0]
    monkeypatch.delattr("tools.boundary_contracts.os.killpg", raising=False)
    spawn = mocker.patch("tools.boundary_contracts.subprocess.Popen")
    row = execute(case, tmp_path)
    assert row["status"] == "unexecuted"
    assert row["reason"] == "native platform unavailable"
    spawn.assert_not_called()


@pytest.mark.parametrize("value", [None, [], {}, {"status": 1}])
def test_observations_reject_malformed_records(value: object) -> None:
    with pytest.raises(ValueError, match="boundary observation must be an object"):
        observations("BOUNDARY_OBSERVATION " + json.dumps(value))


def test_observations_preserve_additional_evidence() -> None:
    value = {"status": "match", "evidence": {"nested": [None, 1, "detail"]}}
    assert observations(
        "test wanted ... BOUNDARY_OBSERVATION " + json.dumps(value)
    ) == [value]
