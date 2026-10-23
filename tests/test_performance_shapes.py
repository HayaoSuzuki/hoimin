import json
import os
import subprocess
import sys
import tempfile
from copy import deepcopy
from pathlib import Path
from typing import BinaryIO, Never, TextIO

import pytest
from pytest_mock import MockerFixture

from tools import performance_shapes as shapes

ROOT = Path(__file__).resolve().parents[1]


def test_registry_covers_all_dimensions() -> None:
    registry = shapes.load_registry(ROOT / "docs/performance/shapes.json")

    assert {row["dimension"] for row in registry["shapes"]} == {
        "discovery",
        "fingerprint",
        "layout",
        "ast",
        "analysis-state",
        "verify",
        "output",
        "workspace",
    }


def test_registry_rejects_duplicate_shapes() -> None:
    registry = shapes.load_registry(ROOT / "docs/performance/shapes.json")
    duplicate = deepcopy(registry)
    duplicate["shapes"].append(duplicate["shapes"][0])

    with pytest.raises(ValueError, match="duplicate"):
        shapes.validate_registry(duplicate)


def test_registry_rejects_invalid_sizes() -> None:
    registry = shapes.load_registry(ROOT / "docs/performance/shapes.json")
    value = [2, 2, 4]
    assert isinstance(value, list)
    assert all(isinstance(item, int) for item in value)
    registry["shapes"][0]["sizes"] = value

    with pytest.raises(ValueError, match="sizes must be bounded N/2N/4N"):
        shapes.validate_registry(registry)


@pytest.mark.parametrize("field", ["metric", "status"])
def test_registry_rejects_invalid_metric_or_status(field: str) -> None:
    registry = shapes.load_registry(ROOT / "docs/performance/shapes.json")
    value = {"metric": "memory", "status": "passing"}[field]
    assert isinstance(value, str)
    registry["shapes"][0].update({field: value})

    with pytest.raises(ValueError, match="invalid metric/status"):
        shapes.validate_registry(registry)


def test_registry_rejects_untraceable_pending_gate() -> None:
    registry = shapes.load_registry(ROOT / "docs/performance/shapes.json")
    registry["gates"].append(
        {"id": "untraceable", "status": "pending", "metric": "operations", "args": []}
    )

    with pytest.raises(ValueError, match="pending dependency required"):
        shapes.validate_registry(registry)


def test_registry_rejects_invalid_gate_metric() -> None:
    registry = shapes.load_registry(ROOT / "docs/performance/shapes.json")
    registry["gates"][0]["metric"] = "memory"

    with pytest.raises(ValueError, match="gate metric"):
        shapes.validate_registry(registry)


@pytest.mark.parametrize(
    "output",
    [
        "test result: ok. 0 passed; 0 failed; 0 ignored;",
        "test result: ok. 0 passed; 0 failed; 1 ignored;",
        "test result: FAILED. 1 passed; 1 failed; 0 ignored;",
    ],
)
def test_test_gate_rejects_empty_ignored_and_failed_runs(output: str) -> None:
    with pytest.raises(ValueError, match="gate did not execute a passing test"):
        shapes.executed_tests(output)


def test_test_gate_accepts_executed_tests() -> None:
    executed = shapes.executed_tests("test result: ok. 1 passed; 0 failed; 0 ignored;")

    assert executed == 1


@pytest.mark.parametrize(
    ("name", "expected"),
    [
        ("discovery", 1),
        ("fingerprint", 1),
        ("long-line", 1),
        ("many-lines", 1),
        ("ast", 1),
        ("imports", 0),
        ("bindings", 0),
        ("verify-top1", 1),
        ("verify-topn", 4),
        ("output", 4),
        ("workspace", 1),
    ],
)
def test_fixtures_have_independent_expected_outputs_and_cleanup(
    name: str, expected: int
) -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / name

        fixture = shapes.make_fixture(name, 4, root)

        assert fixture["expected_candidates"] == expected
        assert (root / "case.py").is_file()
        compile((root / "case.py").read_text(), "case.py", "exec")

    assert not Path(directory).exists()


def test_line_layout_fixtures_have_equal_size(tmp_path: Path) -> None:
    for name in ("long-line", "many-lines"):
        shapes.make_fixture(name, 4, tmp_path / name)

    assert (tmp_path / "long-line/case.py").stat().st_size == (
        tmp_path / "many-lines/case.py"
    ).stat().st_size


@pytest.mark.parametrize("name", sorted(shapes.SHAPES))
def test_minimum_size_is_a_valid_fixture_for_every_shape(
    tmp_path: Path, name: str
) -> None:
    root = tmp_path / name

    fixture = shapes.make_fixture(name, 1, root)

    assert fixture["size"] == 1
    assert fixture["truncated"] == (name == "verify-partial")
    compile((root / "case.py").read_text(), "case.py", "exec")


@pytest.mark.parametrize(
    ("candidates", "truncated", "message"),
    [
        ([], True, "unexpected candidate count"),
        ([{"id": "m1"}], False, "unexpected truncation"),
    ],
    ids=["empty-candidates", "incorrect-truncation"],
)
def test_output_validation_rejects_incorrect_plan(
    candidates: list[dict[str, str]],
    *,
    truncated: bool,
    message: str,
) -> None:
    fixture: shapes.ExpectedOutput = {
        "mode": "plan",
        "expected_candidates": 1,
        "truncated": True,
    }

    with pytest.raises(ValueError, match=message):
        shapes.validate_output(
            {"candidates": candidates, "truncated": truncated}, fixture
        )


def test_output_validation_accepts_expected_plan() -> None:
    fixture: shapes.ExpectedOutput = {
        "mode": "plan",
        "expected_candidates": 1,
        "truncated": True,
    }
    output: dict[str, object] = {"candidates": [{"id": "m1"}], "truncated": True}

    result = shapes.validate_output(output, fixture)

    assert result["candidates"] == 1


@pytest.mark.parametrize("size", [0, -1, 129])
def test_size_bound_regressions_fail_before_writing(tmp_path: Path, size: int) -> None:
    root = tmp_path / "project"

    with pytest.raises(ValueError, match="unknown shape or out-of-bounds size"):
        shapes.make_fixture("ast", size, root)

    assert not root.exists()


def test_unknown_shape_fails_before_writing(tmp_path: Path) -> None:
    root = tmp_path / "project"

    with pytest.raises(ValueError, match="unknown shape or out-of-bounds size"):
        shapes.make_fixture("unknown", 1, root)

    assert not root.exists()


def test_observed_rss_preserves_missing_measurements() -> None:
    observation: shapes.ResourceObservation = {
        "reason": "child_exit",
        "peak_rss_kib": 0,
    }

    result = shapes.observed_rss(observation)

    assert result is None


def test_observed_rss_preserves_positive_measurement_regression() -> None:
    observation: shapes.ResourceObservation = {
        "reason": "child_exit",
        "peak_rss_kib": 12,
    }

    result = shapes.observed_rss(observation)

    assert result == 12288


@pytest.mark.parametrize("reason", ["timeout", "rss_limit", "monitor_error"])
def test_monitor_failure_is_not_success(reason: str) -> None:
    with pytest.raises(ValueError, match=f"resource guard failed: {reason}"):
        shapes.observed_rss({"reason": reason, "peak_rss_kib": 12})


def test_comparisons_include_elapsed_and_nullable_rss_medians() -> None:
    medians: list[shapes.Median] = [
        {
            "shape": "ast",
            "size": 16,
            "label": "baseline",
            "elapsed_ms": 10,
            "sampled_tree_rss_bytes": 100,
        },
        {
            "shape": "ast",
            "size": 16,
            "label": "candidate",
            "elapsed_ms": 8,
            "sampled_tree_rss_bytes": 90,
        },
        {
            "shape": "output",
            "size": 4,
            "label": "baseline",
            "elapsed_ms": 0,
            "sampled_tree_rss_bytes": None,
        },
        {
            "shape": "output",
            "size": 4,
            "label": "candidate",
            "elapsed_ms": 1,
            "sampled_tree_rss_bytes": 80,
        },
    ]

    ast, output = shapes.summarize_comparisons(medians)

    assert ast["elapsed_ms_delta"] == -2
    assert ast["elapsed_ratio"] == 0.8
    assert ast["sampled_tree_rss_bytes_delta"] == -10
    assert ast["sampled_tree_rss_ratio"] == 0.9
    assert output["elapsed_ms_delta"] == 1
    assert output["elapsed_ratio"] is None
    assert output["sampled_tree_rss_bytes_delta"] is None
    assert output["sampled_tree_rss_ratio"] is None


def test_environment_probe_failure_writes_infrastructure_artifact(
    tmp_path: Path,
) -> None:
    output = tmp_path / "artifacts"
    registry = json.loads((ROOT / "docs/performance/shapes.json").read_text())
    for gate in registry["gates"]:
        gate.setdefault("metric", "operations")
    registry_path = tmp_path / "registry.json"
    registry_path.write_text(json.dumps(registry))

    completed = subprocess.run(  # noqa: S603 - Trusted CLI/test arguments; no shell execution.
        [
            sys.executable,
            str(ROOT / "tools/performance_shapes.py"),
            "measure",
            "--output",
            str(output),
            "--registry",
            str(registry_path),
            "--baseline",
            "missing-baseline",
            "--candidate",
            "missing-candidate",
        ],
        cwd=ROOT,
        env={**os.environ, "PATH": ""},
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )

    assert completed.returncode == 1
    result = json.loads((output / "result.json").read_text())

    assert result["status"] == "infrastructure-error"
    assert result["environment"] == {}
    assert "rustc" in result["error"]


def test_large_flat_input_is_allowed(tmp_path: Path) -> None:
    fixture = shapes.make_fixture("long-line", 1000, tmp_path / "flat")

    assert fixture["size"] == 1000


def test_large_nested_input_is_rejected(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="unknown shape or out-of-bounds size"):
        shapes.make_fixture("ast", 129, tmp_path / "deep")


@pytest.mark.skipif(
    os.name != "posix", reason="executable fixture uses a POSIX shebang"
)
def test_top1_checks_available_manifest_size_before_measurement(tmp_path: Path) -> None:
    parent = tmp_path

    fixture = shapes.make_fixture("verify-top1", 4, parent / "project")
    artifact = parent / "artifact"
    artifact.mkdir()
    binary = parent / "fake-hoimin"
    binary.write_text(
        f"#!{sys.executable}\nimport json\n"
        'print(json.dumps({"candidates": [{"id": "m1"}], '
        '"truncated": False}))\n'
    )
    binary.chmod(0o755)

    with pytest.raises(shapes.SemanticMismatch):
        shapes.measure_once(binary, fixture, parent / "project", artifact, 25)

    assert not (artifact / "resource.json").exists()


def test_timed_out_gate_keeps_partial_process_log(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    def timeout(command: list[str], *, stdout: TextIO, **_kwargs: object) -> Never:
        stdout.write("partial compiler diagnostic\n")
        stdout.flush()
        raise subprocess.TimeoutExpired(
            command, 300, output="partial compiler diagnostic\n"
        )

    artifact = tmp_path
    registry: shapes.GateRegistry = {
        "gates": [{"id": "blocked", "status": "active", "args": ["test"]}]
    }
    mocker.patch.object(shapes.subprocess, "run", side_effect=timeout)

    with pytest.raises(subprocess.TimeoutExpired):
        shapes.run_gate(registry, artifact)

    assert (artifact / "blocked.log").read_text() == "partial compiler diagnostic\n"


@pytest.mark.parametrize(
    ("name", "count"),
    [
        ("target-files", 1),
        ("file-selectors", 1),
        ("symbol-selectors", 1),
        ("line-selectors", 1),
        ("fingerprint-files", 1),
        ("fingerprint-bytes", 1),
        ("fingerprint-mixed", 1),
        ("unicode-long-line", 1),
        ("unicode-many-lines", 1),
        ("ast-wide", 1),
        ("ast-left", 1),
        ("large-literal", 1),
        ("imports-active", 1),
        ("verify-multifile", 4),
        ("verify-partial", 4),
        ("output-record", 1),
        ("workspace-bytes", 1),
        ("workspace-workers", 4),
    ],
)
def test_added_axes_materialize_independent_sources(
    tmp_path: Path, name: str, count: int
) -> None:
    root = tmp_path / name

    fixture = shapes.make_fixture(name, 4, root)

    assert fixture["expected_candidates"] == count
    for path in root.glob("*.py"):
        compile(path.read_bytes(), str(path), "exec")


@pytest.mark.parametrize(
    ("name", "relative_path", "expected_bytes"),
    [
        ("fingerprint-bytes", "config.toml", 4096),
        ("workspace-bytes", "data.bin", 4 * 65536),
    ],
)
def test_byte_axis_materializes_requested_size(
    tmp_path: Path,
    name: str,
    relative_path: str,
    expected_bytes: int,
) -> None:
    root = tmp_path / name

    shapes.make_fixture(name, 4, root)

    assert (root / relative_path).stat().st_size == expected_bytes


def test_target_files_axis_materializes_requested_count(tmp_path: Path) -> None:
    root = tmp_path / "project"

    shapes.make_fixture("target-files", 4, root)

    assert len(list(root.glob("selected*.py"))) == 4


def test_unicode_layout_axes_preserve_equal_byte_size(tmp_path: Path) -> None:
    long_line = tmp_path / "unicode-long-line"
    many_lines = tmp_path / "unicode-many-lines"

    shapes.make_fixture("unicode-long-line", 4, long_line)

    shapes.make_fixture("unicode-many-lines", 4, many_lines)

    assert (long_line / "case.py").stat().st_size == (
        many_lines / "case.py"
    ).stat().st_size
    assert "雪" in (long_line / "case.py").read_text(encoding="utf-8")


def test_worker_axis_sets_requested_jobs(tmp_path: Path) -> None:
    root = tmp_path / "project"

    fixture = shapes.make_fixture("workspace-workers", 2, root)

    assert fixture["jobs"] == 2


@pytest.mark.parametrize(
    ("name", "selector", "value", "options"),
    [
        ("symbol-selectors", "--symbol", "case:subject", ["--source", "."]),
        ("line-selectors", "--line", "case.py:1-1", []),
        ("file-selectors", "--file", "case.py", []),
    ],
)
def test_selector_axis_repeats_requested_selector(
    tmp_path: Path,
    name: str,
    selector: str,
    value: str,
    options: list[str],
) -> None:
    root = tmp_path / "project"

    fixture = shapes.make_fixture(name, 4, root)

    assert fixture["selectors"] == [selector, value] * 4
    assert fixture["options"] == options


def test_partial_verify_has_explicit_incomplete_contract() -> None:
    fixture: shapes.ExpectedOutput = {
        "mode": "verify",
        "expected_candidates": 2,
        "truncated": True,
    }
    observed = shapes.ExecutionOutput(
        [{}, {}], {"kind": "run_finished", "complete": False, "exit_code": 4}
    )

    validated = shapes.validate_output(observed, fixture)

    assert validated["candidates"] == 2


@pytest.mark.parametrize(
    ("complete", "truncated"),
    [(True, True), (False, False)],
    ids=["complete-report", "untruncated-fixture"],
)
def test_partial_verify_rejects_inconsistent_completion(
    *, complete: bool, truncated: bool
) -> None:
    fixture: shapes.ExpectedOutput = {
        "mode": "verify",
        "expected_candidates": 2,
        "truncated": truncated,
    }
    observed = shapes.ExecutionOutput(
        [{}, {}], {"kind": "run_finished", "complete": complete, "exit_code": 4}
    )

    with pytest.raises(shapes.SemanticMismatch):
        shapes.validate_output(observed, fixture)


def growth_medians() -> list[shapes.Median]:
    return [
        {
            "shape": "source",
            "label": label,
            "size": n,
            "elapsed_ms": n * factor,
            "sampled_tree_rss_bytes": n * 100 if label == "baseline" else None,
            "output_document_bytes": n * 20,
        }
        for label, factor in [("baseline", 10), ("candidate", 7)]
        for n in [2, 4, 8]
    ]


def test_growth_compares_each_binary_across_n_2n_4n_separately() -> None:
    medians = growth_medians()

    growth = shapes.summarize_growth(medians)

    baseline = [r for r in growth if r["label"] == "baseline"]
    candidate = [r for r in growth if r["label"] == "candidate"]

    assert len(growth) == 4
    assert [r["elapsed_ratio"] for r in baseline] == [2, 4]
    assert [r["output_document_ratio"] for r in candidate] == [2, 4]
    assert all(r["sampled_tree_rss_ratio"] is None for r in candidate)


def test_growth_rejects_missing_medians() -> None:
    medians = growth_medians()[:-1]

    with pytest.raises(ValueError, match="missing N/2N/4N medians"):
        shapes.summarize_growth(medians)


@pytest.mark.parametrize(
    "document",
    [None, [], {"schema_version": 1, "shapes": [], "gates": [None]}],
)
def test_registry_load_rejects_invalid_document_structure(
    tmp_path: Path, document: object
) -> None:
    path = tmp_path / "registry.json"
    path.write_text(json.dumps(document))

    with pytest.raises(ValueError, match="invalid registry structure"):
        shapes.load_registry(path)


@pytest.mark.parametrize(
    ("field", "value"),
    [("sizes", [1, "2", 4]), ("fixture", None)],
)
def test_registry_load_rejects_invalid_shape_fields(
    tmp_path: Path, field: str, value: object
) -> None:
    document = json.loads((ROOT / "docs/performance/shapes.json").read_text())
    document["shapes"][0][field] = value
    path = tmp_path / "registry.json"
    path.write_text(json.dumps(document))

    with pytest.raises(ValueError, match="invalid registry structure"):
        shapes.load_registry(path)


@pytest.mark.parametrize(
    ("field", "value"),
    [("id", None), ("args", ["test", 1]), ("issue", 42)],
)
def test_registry_load_rejects_invalid_gate_fields(
    tmp_path: Path, field: str, value: object
) -> None:
    document = json.loads((ROOT / "docs/performance/shapes.json").read_text())
    document["gates"][0][field] = value
    path = tmp_path / "registry.json"
    path.write_text(json.dumps(document))

    with pytest.raises(ValueError, match="invalid registry structure"):
        shapes.load_registry(path)


@pytest.mark.parametrize("document", [[], None, "text"])
def test_cli_document_must_be_an_object(tmp_path: Path, document: object) -> None:
    path = tmp_path / "stdout.json"
    path.write_text(json.dumps(document))

    with pytest.raises(ValueError, match="expected a JSON object"):
        shapes.load_object(path)


@pytest.mark.parametrize(
    "stats",
    [
        None,
        {"reason": 1, "peak_rss_kib": 12, "elapsed_ms": 10},
        {"reason": "child_exit", "peak_rss_kib": "12", "elapsed_ms": 10},
        {"reason": "child_exit", "peak_rss_kib": 12, "elapsed_ms": "10"},
    ],
)
def test_measurement_rejects_malformed_resource_observations(
    tmp_path: Path, mocker: MockerFixture, stats: object
) -> None:
    fixture = shapes.make_fixture("ast", 1, tmp_path / "project")
    artifact = tmp_path / "artifact"
    artifact.mkdir()

    def run_guard(
        command: list[str], *, stdout: BinaryIO, **_kwargs: object
    ) -> subprocess.CompletedProcess[bytes]:
        stats_path = Path(command[command.index("--stats") + 1])
        stats_path.write_text(json.dumps(stats))
        stdout.write(b'{"candidates": [{}], "truncated": false}')
        return subprocess.CompletedProcess(command, 0)

    mocker.patch.object(shapes.subprocess, "run", side_effect=run_guard)

    with pytest.raises(ValueError, match="invalid resource observation"):
        shapes.measure_once(Path("hoimin"), fixture, tmp_path / "project", artifact, 25)


@pytest.mark.parametrize(
    "events",
    [
        [{"kind": "run_started"}],
        [{"kind": "run_finished", "complete": True, "exit_code": 0}],
        [
            {"kind": "run_started"},
            {"kind": "run_finished", "complete": "yes", "exit_code": 0},
        ],
        [
            {"kind": "run_started"},
            {"kind": "run_finished", "complete": True, "exit_code": 0},
            {"kind": "diagnostic"},
        ],
        [{"schema_version": 10, "run": {}, "mutants": [], "summary": {}}],
    ],
)
def test_execution_reader_rejects_incomplete_or_nonstream_output(
    tmp_path: Path, events: list[dict[str, object]]
) -> None:
    source = tmp_path / "run.jsonl"
    source.write_text("\n".join(json.dumps(event) for event in events))
    with pytest.raises(shapes.SemanticMismatch):
        shapes.load_execution_output(source)


def test_execution_reader_collects_finished_events_and_final_summary(
    tmp_path: Path,
) -> None:
    source = tmp_path / "run.jsonl"
    events = [
        {"kind": "run_started"},
        {"kind": "mutant_started"},
        {"kind": "mutant_finished", "status": "survived"},
        {"kind": "run_finished", "complete": True, "exit_code": 0},
    ]
    source.write_text("\n".join(json.dumps(event) for event in events))
    result = shapes.load_execution_output(source)
    assert result.mutants == [events[2]]
    assert result.summary == events[3]
