"""Fixed specification comparisons; seeded weak tests are experimental controls."""

import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from unittest.mock import patch

import pytest

from tools import ci_metrics, ci_selection

CORPUS = json.loads(Path(__file__).with_name("corpus.json").read_text(encoding="utf-8"))


@pytest.mark.parametrize("domain,oracle,values", [
    pytest.param(domain, oracle, values, id=f"{domain}-{oracle}-{i}")
    for domain in ("narrow", "broad", "heldout")
    for oracle in ("weak", "strong")
    for i, values in enumerate(CORPUS[domain])
])
def test_amplification(domain, oracle, values):
    # Arrange: total is defined by integer addition, not by recorded subject output.
    expected = 0
    for value in values:
        expected += value

    # Act
    result = ci_metrics.total(values)

    # Assert
    if oracle == "weak":
        assert isinstance(result, (int, float)) and result >= 0
    else:
        assert result == expected


@pytest.mark.parametrize("values", CORPUS["broad"] + CORPUS["heldout"])
def test_mr_permutation(values):
    # Arrange: exact bounded integers exclude floating-point reassociation errors.
    transformed = list(reversed(values))

    # Act: both calls use the same mutated function.
    before = ci_metrics.total(values)
    after = ci_metrics.total(transformed)

    # Assert
    assert after == before


@pytest.mark.parametrize("values", CORPUS["broad"] + CORPUS["heldout"])
def test_mr_concatenation(values):
    # Arrange
    midpoint = len(values) // 2
    left, right = values[:midpoint], values[midpoint:]

    # Act
    combined = ci_metrics.total(values)
    separate = ci_metrics.total(left) + ci_metrics.total(right)

    # Assert
    assert combined == separate


@pytest.mark.parametrize("values", CORPUS["broad"])
def test_mr_missing_absorbs(values):
    # Arrange
    transformed = values + [None]

    # Act
    result = ci_metrics.total(transformed)

    # Assert: missingness is a documented contract, independent of the subject.
    assert result is None


@pytest.mark.parametrize("suite,row", [
    pytest.param(suite, row, id=f"{suite}-{i}")
    for suite, rows in CORPUS["combinations"].items()
    for i, row in enumerate(rows)
])
def test_combinations(tmp_path, suite, row):
    # Arrange: the gate is tested through main; only the preceding Git planner is replaced.
    push, main, delegated, rust = row
    event = tmp_path / "event.json"
    event.write_text("{}", encoding="utf-8")
    environment = {
        "GITHUB_EVENT_NAME": "push" if push else "workflow_dispatch",
        "GITHUB_REF": "refs/heads/main" if main else "refs/heads/feature",
        "GITHUB_EVENT_PATH": str(event), "CI_CGROUP": str(delegated).lower(),
    }
    flags = dict.fromkeys(ci_selection.CATEGORIES, False) | {"rust": rust}
    output = io.StringIO()
    expected = all(row)

    # Act
    with patch.dict(os.environ, environment, clear=True), \
            patch.object(sys, "argv", ["ci_selection", "plan"]), \
            patch.object(ci_selection, "plan", return_value=flags), \
            contextlib.redirect_stdout(output):
        status = ci_selection.main()
    actual = json.loads(output.getvalue())

    # Assert
    assert status == 0 and actual["cgroup"] is expected


@pytest.fixture
def repository(tmp_path):
    """A local rename with non-ASCII names exercises NUL and rename contracts."""
    executable = shutil.which("git")
    assert executable
    def git(*args):
        return subprocess.check_output(
            [executable, "-C", str(tmp_path), *args], text=True,
            encoding="utf-8", stderr=subprocess.PIPE, timeout=10,
        ).strip()
    git("init")
    git("config", "user.name", "Research test")
    git("config", "user.email", "research@example.invalid")
    old, new = "old name.txt", "日本語.py"
    (tmp_path / old).write_text("content\n", encoding="utf-8")
    git("add", ".")
    git("commit", "-m", "base")
    base = git("rev-parse", "HEAD")
    (tmp_path / old).rename(tmp_path / new)
    git("add", "-A")
    git("commit", "-m", "rename")
    return tmp_path, base, {old, new}


@pytest.mark.parametrize("backend", ["fixed", "strict", "real"])
def test_dependency_contract(repository, backend):
    # Arrange: fixed/strict are planted experimental fakes, not existing product mocks.
    root, base, expected = repository
    git = shutil.which("git")
    argv = [git, "-C", str(root), "diff", "--no-renames", "--name-only", "-z", f"{base}..HEAD", "--"]
    def fake(args, **kwargs):
        if backend == "strict":
            assert args == argv
        return subprocess.CompletedProcess(args, 0, stdout=b"old name.txt\0" + "日本語.py\0".encode())
    backend_context = contextlib.nullcontext() if backend == "real" else patch.object(ci_selection.subprocess, "run", side_effect=fake)

    # Act
    with backend_context:
        result = ci_selection.changed_paths(root, base, "HEAD", merge_base=False)

    # Assert: ordering is not part of the contract.
    assert set(result) == expected


@pytest.mark.parametrize("removed", ["--no-renames", "-z", "--"])
def test_known_fake_drift_without_mutation(repository, removed):
    # Arrange: ordinary contract testing, with explicitly changed requests.
    root, base, paths = repository
    argv = [shutil.which("git"), "-C", str(root), "diff", "--no-renames", "--name-only", "-z", f"{base}..HEAD", "--"]
    argv.remove(removed)
    fixed = b"old name.txt\0" + "日本語.py\0".encode()

    # Act
    real = subprocess.run(argv, check=True, capture_output=True, timeout=10).stdout

    # Assert: two seeded drifts; omitting the final separator is harmless here.
    if removed == "--":
        assert set(real.split(b"\0")) == set(fixed.split(b"\0"))
    else:
        assert set(real.split(b"\0")) != set(fixed.split(b"\0"))
