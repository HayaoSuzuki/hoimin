import hashlib
import json
import os
from pathlib import Path

import pytest
from pytest_mock import MockerFixture, MockType

from tools.boundary_contracts import REGISTRY, classify, execute, load_registry, main


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

    def spawn(_argv: list[str], **kwargs: object) -> MockType:
        assert "HOIMIN_BOUNDARY_CASE" not in kwargs["env"]
        assert "HOIMIN_SESSION_ORACLE_CASE" not in kwargs["env"]
        assert kwargs["env"]["PATH"] == os.environ["PATH"]
        kwargs["stdout"].write(
            f"test {case['test']} ... ok\ntest result: ok. 1 passed;"
        )
        kwargs["stdout"].flush()
        return mocker.Mock(wait=mocker.Mock(return_value=0))

    monkeypatch.setenv("HOIMIN_BOUNDARY_CASE", "one")
    monkeypatch.setenv("HOIMIN_SESSION_ORACLE_CASE", "one")
    mocker.patch("tools.boundary_contracts.subprocess.Popen", side_effect=spawn)
    assert execute(case, tmp_path)["status"] == "match"
