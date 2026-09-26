import importlib.util
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Never

import pytest
from pytest_mock import MockerFixture

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "performance_shapes", ROOT / "tools/performance_shapes.py"
)
shapes = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shapes)


def test_registry_covers_all_dimensions_and_rejects_duplicates() -> None:
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
    duplicate = json.loads(json.dumps(registry))
    duplicate["shapes"].append(duplicate["shapes"][0])
    with pytest.raises(ValueError, match="duplicate"):
        shapes.validate_registry(duplicate)


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("sizes", [2, 2, 4], "sizes must be bounded N/2N/4N"),
        ("metric", "memory", "invalid metric/status"),
        ("status", "passing", "invalid metric/status"),
    ],
)
def test_registry_rejects_invalid_growth(
    field: str, value: object, message: str
) -> None:
    registry = shapes.load_registry(ROOT / "docs/performance/shapes.json")
    registry["shapes"][0][field] = value
    with pytest.raises(ValueError, match=message):
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
    assert shapes.executed_tests("test result: ok. 1 passed; 0 failed; 0 ignored;") == 1


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


def test_output_validation_does_not_accept_empty_candidates() -> None:
    fixture = {"mode": "plan", "expected_candidates": 1, "truncated": True}
    with pytest.raises(ValueError, match="unexpected candidate count"):
        shapes.validate_output({"candidates": [], "truncated": True}, fixture)
    assert (
        shapes.validate_output(
            {"candidates": [{"id": "m1"}], "truncated": True}, fixture
        )["candidates"]
        == 1
    )
    with pytest.raises(ValueError, match="unexpected truncation"):
        shapes.validate_output(
            {"candidates": [{"id": "m1"}], "truncated": False}, fixture
        )


@pytest.mark.parametrize(
    ("name", "size"), [("ast", 0), ("ast", -1), ("ast", 129), ("unknown", 1)]
)
def test_size_bounds_and_unknown_shapes_fail_before_writing(
    tmp_path: Path, name: str, size: int
) -> None:
    root = tmp_path / "project"
    with pytest.raises(ValueError, match="unknown shape or out-of-bounds size"):
        shapes.make_fixture(name, size, root)
    assert not root.exists()


def test_unobserved_rss_is_null() -> None:
    assert shapes.observed_rss({"reason": "child_exit", "peak_rss_kib": 0}) is None
    assert shapes.observed_rss({"reason": "child_exit", "peak_rss_kib": 12}) == 12288


@pytest.mark.parametrize("reason", ["timeout", "rss_limit", "monitor_error"])
def test_monitor_failure_is_not_success(reason: str) -> None:
    with pytest.raises(ValueError, match=f"resource guard failed: {reason}"):
        shapes.observed_rss({"reason": reason, "peak_rss_kib": 12})


def test_comparisons_include_elapsed_and_nullable_rss_medians() -> None:
    medians = [
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


def test_large_flat_inputs_do_not_raise_the_ast_depth_limit(tmp_path: Path) -> None:
    fixture = shapes.make_fixture("long-line", 1000, tmp_path / "flat")
    assert fixture["size"] == 1000
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
    def timeout(command: list[str], **kwargs: object) -> Never:
        output = kwargs["stdout"]
        if hasattr(output, "write"):
            output.write("partial compiler diagnostic\n")
            output.flush()
        raise subprocess.TimeoutExpired(
            command, 300, output="partial compiler diagnostic\n"
        )

    artifact = tmp_path
    registry = {"gates": [{"id": "blocked", "status": "active", "args": ["test"]}]}
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


def test_added_axes_selection_controls(tmp_path: Path) -> None:
    parent = tmp_path
    for name in (
        "target-files",
        "fingerprint-bytes",
        "workspace-bytes",
        "unicode-long-line",
        "unicode-many-lines",
    ):
        shapes.make_fixture(name, 4, parent / name)
    assert len(list((parent / "target-files").glob("selected*.py"))) == 4
    assert (parent / "fingerprint-bytes/config.toml").stat().st_size == 4096
    assert (parent / "workspace-bytes/data.bin").stat().st_size == 4 * 65536
    assert (parent / "unicode-long-line/case.py").stat().st_size == (
        parent / "unicode-many-lines/case.py"
    ).stat().st_size
    assert "雪" in (parent / "unicode-long-line/case.py").read_text()
    assert shapes.make_fixture("workspace-workers", 2, parent / "workers")["jobs"] == 2
    symbols = shapes.make_fixture("symbol-selectors", 4, parent / "symbols")
    assert symbols["selectors"] == ["--symbol", "case:subject"] * 4
    assert symbols["options"] == ["--source", "."]
    assert (
        shapes.make_fixture("line-selectors", 4, parent / "lines")["selectors"]
        == ["--line", "case.py:1-1"] * 4
    )
    assert (
        shapes.make_fixture("file-selectors", 4, parent / "files")["selectors"]
        == ["--file", "case.py"] * 4
    )


def test_partial_verify_has_explicit_incomplete_contract() -> None:
    fixture = {"mode": "verify", "expected_candidates": 2, "truncated": True}
    observed = {"mutants": [{}, {}], "summary": {"complete": False}}
    assert shapes.validate_output(observed, fixture)["candidates"] == 2
    with pytest.raises(shapes.SemanticMismatch):
        shapes.validate_output({**observed, "summary": {"complete": True}}, fixture)
    with pytest.raises(shapes.SemanticMismatch):
        shapes.validate_output(observed, {**fixture, "truncated": False})


def test_growth_compares_each_binary_across_n_2n_4n_separately() -> None:
    medians = [
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
    growth = shapes.summarize_growth(medians)
    assert len(growth) == 4
    baseline = [r for r in growth if r["label"] == "baseline"]
    candidate = [r for r in growth if r["label"] == "candidate"]
    assert [r["elapsed_ratio"] for r in baseline] == [2, 4]
    assert [r["output_document_ratio"] for r in candidate] == [2, 4]
    assert all(r["sampled_tree_rss_ratio"] is None for r in candidate)
    with pytest.raises(ValueError, match="missing N/2N/4N medians"):
        shapes.summarize_growth(medians[:-1])
