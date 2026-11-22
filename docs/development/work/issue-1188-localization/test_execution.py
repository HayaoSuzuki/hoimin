"""Exercise research boundaries with real subprocesses and byte-level sources."""

import sys
import tempfile
import time
from pathlib import Path

import blake3
import pytest
from evaluate import applied, command, observe
from hypothesis import given
from hypothesis import strategies as st


def test_collector_records_independent_test_outcomes(tmp_path):
    # Arrange
    (tmp_path / "subject.py").write_text(
        "def value():\n    return 2\n", encoding="utf-8"
    )
    (tmp_path / "test_subject.py").write_text(
        "from subject import value\ndef test_pass(): assert value() == 2\n"
        "def test_fail(): assert value() == 1\n",
        encoding="utf-8",
    )
    # Act
    result = observe(
        tmp_path,
        ["test_subject.py"],
        ["subject.py"],
        time.monotonic() + 20,
        coverage=True,
    )
    # Assert
    assert {name: item["call"] for name, item in result["tests"].items()} == {
        "test_subject.py::test_pass": "passed",
        "test_subject.py::test_fail": "failed",
    }
    assert result["coverage"]["subject.py"]["2"] == [
        "test_subject.py::test_fail",
        "test_subject.py::test_pass",
    ]


@pytest.mark.parametrize(
    "source",
    [
        "import pytest\n@pytest.mark.skip\ndef test_case(): pass\n",
        "import pytest\n@pytest.mark.xfail\ndef test_case(): assert False\n",
        "import pytest\n@pytest.mark.xfail\ndef test_case(): pass\n",
        "import pytest\n@pytest.fixture\ndef broken(): raise ValueError('setup')\n"
        "def test_case(broken): pass\n",
        "def test_case(): pass\ndef teardown_function(): raise ValueError('teardown')\n",
    ],
)
def test_collector_rejects_unsupported_test_results(tmp_path, source):
    # Arrange
    (tmp_path / "test_subject.py").write_text(source, encoding="utf-8")
    # Act
    result = observe(tmp_path, ["test_subject.py"], [], time.monotonic() + 20)
    # Assert
    assert result["complete"] is False


def test_collection_error_is_incomplete(tmp_path):
    # Arrange
    (tmp_path / "test_subject.py").write_text(
        "raise ValueError('collection')", encoding="utf-8"
    )
    # Act
    result = observe(tmp_path, ["test_subject.py"], [], time.monotonic() + 20)
    # Assert
    assert result["complete"] is False


def candidate(source, original, replacement):
    start = source.index(original.encode())
    return {
        "span": {"start": start, "length": len(original.encode())},
        "original": original,
        "replacement": replacement,
        "file_hash": blake3.blake3(source).hexdigest(),
    }


@given(st.text(alphabet="日本語abc", min_size=1, max_size=20))
def test_mutation_offsets_are_bytes_and_source_is_restored(prefix):
    # Arrange
    source = ("# " + prefix + "\nvalue = 10\n").encode()
    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder) / "source.py"
        path.write_bytes(source)
        mutation = candidate(source, "10", "11")
        # Act
        with applied(path, mutation):
            actual = path.read_bytes()
        restored = path.read_bytes()
    # Assert
    assert actual == source.replace(b"10", b"11")
    assert restored == source


def test_stale_source_is_rejected_without_writing(tmp_path):
    # Arrange
    path = tmp_path / "source.py"
    mutation = candidate(b"x = 1\n", "1", "2")
    changed = b"x = 3\n"
    path.write_bytes(changed)
    # Act / Assert
    with pytest.raises(ValueError):
        with applied(path, mutation):
            pytest.fail("stale source was used")
    assert path.read_bytes() == changed


def test_mutation_is_restored_after_execution_error(tmp_path):
    # Arrange
    path = tmp_path / "source.py"
    original = b"x = 1\n"
    path.write_bytes(original)
    # Act
    with pytest.raises(TimeoutError):
        with applied(path, candidate(original, "1", "2")):
            raise TimeoutError
    # Assert
    assert path.read_bytes() == original


def test_expired_budget_starts_no_process(tmp_path):
    # Arrange
    marker = tmp_path / "started"
    argv = [sys.executable, "-c", f"open({str(marker)!r}, 'w').close()"]
    # Act
    result = command(argv, tmp_path, time.monotonic() - 1)
    # Assert
    assert result["termination"] == "budget_exhausted"
    assert not marker.exists()


def test_timeout_stops_descendant_before_returning(tmp_path):
    # Arrange
    marker = tmp_path / "late-write"
    child = "import time; time.sleep(3); " + f"open({str(marker)!r}, 'w').close()"
    parent = (
        "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',"
        + repr(child)
        + "]); print('started',flush=True); time.sleep(20)"
    )
    # Act
    result = command(
        [sys.executable, "-c", parent], tmp_path, time.monotonic() + 10, timeout=1
    )
    time.sleep(3)
    # Assert
    assert result["termination"] == "timeout"
    assert "started" in result["stdout"]
    assert not marker.exists()


def test_sampling_visits_distinct_eligible_lines_before_second_mutants():
    # Arrange
    from evaluate import select_candidates

    candidates = [
        {"id": name, "line": line}
        for name, line in [("a", 1), ("b", 1), ("c", 2), ("d", 3), ("e", 9)]
    ]
    # Act
    actual = select_candidates(candidates, {1, 2, 3}, 3)
    # Assert
    assert {item["line"] for item in actual} == {1, 2, 3}


@given(st.permutations([("a", 1), ("b", 1), ("c", 2)]))
def test_sampling_is_independent_of_plan_order(entries):
    # Arrange
    from evaluate import select_candidates

    candidates = [{"id": name, "line": line} for name, line in entries]
    # Act
    actual = select_candidates(candidates, {1, 2}, 3)
    # Assert
    assert [item["id"] for item in actual] == ["c", "a", "b"]


def test_excess_output_is_incomplete(tmp_path):
    # Arrange
    argv = [
        sys.executable,
        "-c",
        "import sys,time; print('x'*10000,flush=True); time.sleep(5)",
    ]
    # Act
    result = command(argv, tmp_path, time.monotonic() + 10, output_limit=1000)
    # Assert
    assert result["termination"] == "output_limit"
    assert len(result["stdout"]) <= 1000


def test_last_repeat_exhaustion_keeps_case_incomplete():
    # Arrange
    from evaluate import completion

    variants = [
        {
            "observations": [
                {"process": {"termination": "exit"}},
                {"process": {"termination": "budget_exhausted"}},
            ]
        }
    ]
    # Act
    result = completion(variants, 1)
    # Assert
    assert result == (False, "budget_exhausted")


def test_unconfirmed_cleanup_does_not_restore_a_live_mutated_copy(tmp_path):
    # Arrange
    from evaluate import ContainmentError

    path = tmp_path / "source.py"
    original = b"x = 1\n"
    path.write_bytes(original)
    # Act
    with pytest.raises(ContainmentError):
        with applied(path, candidate(original, "1", "2")):
            raise ContainmentError("unconfirmed")
    # Assert
    assert path.read_bytes() == b"x = 2\n"


def test_project_copy_keeps_caller_source_immutable(tmp_path):
    # Arrange
    from evaluate import project_copy

    original = tmp_path / "source.py"
    original.write_text("x = 1", encoding="utf-8")
    # Act
    with project_copy(tmp_path) as copied:
        (copied / "source.py").write_text("x = 2", encoding="utf-8")
        observed = original.read_text(encoding="utf-8")
    # Assert
    assert observed == "x = 1"
    assert not copied.exists()


def test_existing_timestamp_bytecode_cannot_hide_changed_source(tmp_path):
    # Arrange
    import os
    import py_compile

    path = tmp_path / "subject.py"
    path.write_text("value = 1\n", encoding="utf-8")
    stamp = path.stat().st_mtime_ns
    py_compile.compile(str(path), doraise=True)
    path.write_text("value = 2\n", encoding="utf-8")
    os.utime(path, ns=(stamp, stamp))
    (tmp_path / "test_subject.py").write_text(
        "from subject import value\ndef test_value(): assert value == 2\n",
        encoding="utf-8",
    )
    # Act
    actual = observe(
        tmp_path, ["test_subject.py"], ["subject.py"], time.monotonic() + 20
    )
    # Assert
    assert actual["tests"]["test_subject.py::test_value"]["call"] == "passed"


def test_modified_prepared_test_is_rejected(tmp_path):
    # Arrange
    import hashlib
    import json

    from evaluate import validate_prepared

    test = tmp_path / "test_subject.py"
    original = b"def test_original(): pass\n"
    test.write_bytes(b"def test_changed(): pass\n")
    (tmp_path / "prepared.json").write_text(
        json.dumps(
            {
                "case": {"name": "demo"},
                "files": {"test_subject.py": hashlib.sha256(original).hexdigest()},
            }
        )
    )
    # Act / Assert
    with pytest.raises(ValueError, match="prepared input changed"):
        validate_prepared(tmp_path, {"name": "demo"})


def test_fast_excess_output_is_still_incomplete(tmp_path):
    # Arrange
    argv = [sys.executable, "-c", "print('x'*10000)"]
    # Act
    result = command(argv, tmp_path, time.monotonic() + 10, output_limit=1000)
    # Assert
    assert result["termination"] == "output_limit"


def test_quarantined_copy_is_retained_on_unconfirmed_cleanup(tmp_path):
    # Arrange
    from evaluate import ContainmentError, project_copy

    (tmp_path / "source.py").write_text("x=1", encoding="utf-8")
    # Act
    with pytest.raises(ContainmentError):
        with project_copy(tmp_path) as copied:
            raise ContainmentError("not reaped")
    # Assert
    assert copied.is_dir()
    # Test-only cleanup: no process was launched in this test.
    (copied / "source.py").unlink()
    copied.rmdir()
