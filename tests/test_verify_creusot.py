"""Failure boundaries for the mandatory production Creusot proof gate."""

from __future__ import annotations

import ast
import json
import os
import shutil
import sys
import time
from pathlib import Path
from typing import Any

import pytest

from tools import verify_creusot

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "tests/fixtures/creusot" / "x86_64-unknown-linux-gnu/first"


def test_runner_source_parses_for_pinned_container_python() -> None:
    source = (ROOT / "tools/verify_creusot.py").read_text(encoding="utf-8")

    ast.parse(source, filename="tools/verify_creusot.py", feature_version=(3, 12))


@pytest.fixture
def proof_project(tmp_path: Path) -> tuple[Path, dict[str, Any]]:
    """Use the real generated proof fixture and current production source."""
    manifest: dict[str, Any] = json.loads(
        (ROOT / "formal/creusot/obligations.json").read_text()
    )
    for source in {item["source"] for item in manifest["functions"]} | {
        item["source"] for item in manifest["logical_definitions"]
    }:
        destination = tmp_path / source
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / source, destination)
    shutil.copytree(EVIDENCE / "verif", tmp_path / "verif")
    return tmp_path, manifest


@pytest.mark.parametrize(
    ("function_id", "old", "new"),
    [
        (
            "projection",
            "ensures(result.jobs@ == jobs@)",
            "ensures(result.jobs@ >= jobs@)",
        ),
        ("reconstruction", "requires(nanos@ <= duration_max_nanos())", ""),
        ("projection", "pub fn project_top_budget(", "pub fn changed_top_budget("),
    ],
)
def test_source_scope_rejects_missing_or_weakened_contract(
    proof_project: tuple[Path, dict[str, Any]],
    function_id: str,
    old: str,
    new: str,
) -> None:
    root, manifest = proof_project
    function = next(item for item in manifest["functions"] if item["id"] == function_id)
    source = root / function["source"]
    original = source.read_text()
    assert old in original
    source.write_text(original.replace(old, new, 1))

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_source_scope(root, manifest)


@pytest.mark.parametrize(
    "addition",
    [
        "#[cfg_attr(creusot, requires(selected@ < 100))]\n",
        "#[cfg_attr(creusot, ensures(result.waves@ >= 0))]\n",
    ],
)
def test_source_scope_rejects_extra_contract(
    proof_project: tuple[Path, dict[str, Any]], addition: str
) -> None:
    root, manifest = proof_project
    source = root / "crates/hoimin-budget-math/src/projection.rs"
    source.write_text(
        source.read_text().replace(
            "pub fn project_top_budget(", addition + "pub fn project_top_budget(", 1
        )
    )

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_source_scope(root, manifest)


def test_source_scope_accepts_current_production_contracts(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project

    verify_creusot.check_source_scope(root, manifest)


@pytest.mark.parametrize("damage", ["function", "contract", "logical", "target"])
def test_manifest_cannot_drop_required_proof_scope(
    proof_project: tuple[Path, dict[str, Any]], damage: str
) -> None:
    root, manifest = proof_project
    if damage == "function":
        manifest["functions"] = [
            item for item in manifest["functions"] if item["id"] != "projection"
        ]
    elif damage == "contract":
        projection = next(
            item for item in manifest["functions"] if item["id"] == "projection"
        )
        projection["contracts"] = [
            item
            for item in projection["contracts"]
            if item["id"] != "mathematical_ceiling"
        ]
        source = root / projection["source"]
        source.write_text(
            source.read_text().replace(
                "#[cfg_attr(creusot, ensures(result.waves@ == "
                "(selected@ + jobs@ - 1) / jobs@))]",
                "",
            )
        )
    elif damage == "logical":
        manifest["logical_definitions"] = []
    else:
        manifest["targets"] = manifest["targets"][:2]

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_source_scope(root, manifest)


def test_manifest_and_source_cannot_weaken_contract_together(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    projection = next(
        item for item in manifest["functions"] if item["id"] == "projection"
    )
    contract = next(
        item for item in projection["contracts"] if item["id"] == "jobs_preserved"
    )
    source = root / projection["source"]
    old = contract["source"]
    new = old.replace("==", ">=")
    source.write_text(source.read_text().replace(old, new, 1))
    contract["source"] = new

    with pytest.raises(verify_creusot.ProofError, match="approved scope"):
        verify_creusot.check_source_scope(root, manifest)


@pytest.mark.parametrize("damage", ["missing", "empty", "old", "marker"])
def test_generated_scope_rejects_incomplete_or_stale_coma(
    proof_project: tuple[Path, dict[str, Any]], damage: str
) -> None:
    root, manifest = proof_project
    path = root / "verif/hoimin_budget_math_rlib/projection/project_top_budget.coma"
    if damage == "missing":
        path.unlink()
    elif damage == "empty":
        path.write_text("")
    elif damage == "old":
        os.utime(path, ns=(1, 1))
    else:
        path.write_text(
            path.read_text().replace(
                "project_top_budget ensures #5", "changed ensures #5"
            )
        )

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_generated_scope(root, manifest, generated_after_ns=2)


def test_generated_scope_accepts_current_named_files(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project

    verify_creusot.check_generated_scope(root, manifest, generated_after_ns=0)


def test_saved_strategies_attach_only_to_current_generated_files(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    target = "x86_64-unknown-linux-gnu"
    strategy_root = Path(manifest["strategy_roots"][target])
    shutil.copytree(ROOT / strategy_root, root / strategy_root)
    for prior in (root / "verif").rglob("proof.json"):
        prior.unlink()

    count = verify_creusot.install_strategies(root, manifest, target)

    assert count == 25
    assert len(list((root / "verif").rglob("proof.json"))) == 25


@pytest.mark.parametrize(
    "damage", ["missing_goal", "unproved", "missing_target", "empty"]
)
def test_proof_status_rejects_incomplete_targets(
    proof_project: tuple[Path, dict[str, Any]], damage: str
) -> None:
    root, manifest = proof_project
    log = (EVIDENCE / "proof.log").read_text(encoding="utf-8")
    if damage == "missing_goal":
        log = log.replace("  Goal vc_project_top_budget: ✔ (1)\n", "", 1)
    elif damage == "unproved":
        log = log.replace(
            "  Goal vc_project_top_budget: ✔ (1)",
            "  Goal vc_project_top_budget: ✘ (0)",
            1,
        )
    elif damage == "missing_target":
        log = log.replace("Proved (25 files) ✔", "Proved (24 files) ✔")
    else:
        log = ""

    with pytest.raises(verify_creusot.ProofError):
        verify_creusot.check_proof_status(root, manifest, log)


def test_successful_alternative_may_recover_solver_timeout(
    proof_project: tuple[Path, dict[str, Any]],
) -> None:
    root, manifest = proof_project
    log = "Z3 attempt timed out; CVC5 alternative succeeded\n" + (
        EVIDENCE / "proof.log"
    ).read_text(encoding="utf-8")

    result = verify_creusot.check_proof_status(root, manifest, log)

    assert result["files"] == 25
    assert result["goals"] == 59


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX process groups required")
def test_command_enforces_overall_deadline(tmp_path: Path) -> None:
    deadline = time.monotonic() + 0.05

    with pytest.raises(verify_creusot.ProofError, match="deadline"):
        verify_creusot.run_command(
            ["python3", "-c", "import time; time.sleep(5)"],
            cwd=tmp_path,
            log_path=tmp_path / "sleep.log",
            deadline=deadline,
        )


@pytest.mark.parametrize("failure", ["exit", "missing"])
def test_command_rejects_failed_or_missing_tool(tmp_path: Path, failure: str) -> None:
    command = (
        ["python3", "-c", "raise SystemExit(3)"]
        if failure == "exit"
        else [str(tmp_path / "missing-tool")]
    )

    with pytest.raises((verify_creusot.ProofError, FileNotFoundError)):
        verify_creusot.run_command(
            command,
            cwd=tmp_path,
            log_path=tmp_path / "failed.log",
            deadline=time.monotonic() + 10,
        )


def test_mismatched_environment_preflight_stops_before_generation(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    checker = tmp_path / "checker.py"
    checker.write_text("raise SystemExit('tool hash mismatch')\n")
    manifest = tmp_path / "toolchain.json"
    manifest.write_text("{}")
    monkeypatch.setattr(verify_creusot, "ENVIRONMENT", checker)
    monkeypatch.setattr(verify_creusot, "TOOLCHAIN", manifest)

    with pytest.raises(verify_creusot.ProofError, match="command failed"):
        verify_creusot.proof_preflight(
            tmp_path, tmp_path, time.monotonic() + 10, "mismatch"
        )

    assert "tool hash mismatch" in (tmp_path / "mismatch-preflight.log").read_text()


@pytest.mark.skipif(
    sys.platform != "linux", reason="Linux session ownership is required"
)
def test_deadline_kills_child_in_separate_process_group(tmp_path: Path) -> None:
    pid_file = tmp_path / "child.pid"
    script = (
        "import os, pathlib, time\n"
        "pid = os.fork()\n"
        "if pid == 0:\n"
        "    os.setpgid(0, 0)\n"
        f"    pathlib.Path({str(pid_file)!r}).write_text(str(os.getpid()))\n"
        "    time.sleep(20)\n"
        "else:\n"
        f"    path = pathlib.Path({str(pid_file)!r})\n"
        "    while not path.exists(): time.sleep(0.001)\n"
        "    time.sleep(20)\n"
    )

    with pytest.raises(verify_creusot.ProofError, match="deadline"):
        verify_creusot.run_command(
            [sys.executable, "-c", script],
            cwd=tmp_path,
            log_path=tmp_path / "separate-group.log",
            deadline=time.monotonic() + 0.5,
        )

    child = int(pid_file.read_text())
    state_file = Path(f"/proc/{child}/stat")
    assert not state_file.exists() or state_file.read_text().split(") ", 1)[1][0] == "Z"


@pytest.mark.skipif(sys.platform == "win32", reason="Linux session ownership required")
def test_session_cleanup_stops_command_group_before_solver_groups(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls: list[int] = []
    monkeypatch.setattr(verify_creusot.sys, "platform", "linux")
    monkeypatch.setattr(verify_creusot.os, "getsid", lambda _: 999)
    monkeypatch.setattr(verify_creusot, "owned_session_groups", lambda _: {100, 101})
    monkeypatch.setattr(
        verify_creusot.os, "killpg", lambda group, _: calls.append(group)
    )

    verify_creusot.terminate_session(100)

    assert calls[0] == 100
    assert 101 in calls


@pytest.mark.parametrize("unresolved_target", [None, "x86_64-pc-windows-msvc"])
def test_runner_requires_three_fresh_target_results(
    proof_project: tuple[Path, dict[str, Any]],
    monkeypatch: pytest.MonkeyPatch,
    unresolved_target: str | None,
) -> None:
    root, manifest = proof_project
    for name in (
        "Cargo.toml",
        "Cargo.lock",
        "why3find.json",
        "crates/hoimin-budget-math/Cargo.toml",
        "formal/creusot/obligations.json",
    ):
        destination = root / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, destination)
    environment = root / "environment.py"
    environment.write_text("# preflight fixture\n")
    toolchain = root / "toolchain.json"
    toolchain.write_text("{}")
    monkeypatch.setattr(verify_creusot, "ENVIRONMENT", environment)
    monkeypatch.setattr(verify_creusot, "TOOLCHAIN", toolchain)
    monkeypatch.setattr(verify_creusot, "install_strategies", lambda *_: 25)
    calls: list[list[str]] = []

    def fixture_command(
        command: list[str], *, cwd: Path, log_path: Path, deadline: float
    ) -> dict[str, Any]:
        del deadline
        calls.append(command)
        target = log_path.parent.name
        if "--only=coma" in command:
            shutil.copytree(EVIDENCE / "verif", cwd / "verif", dirs_exist_ok=True)
            for path in (cwd / "verif").rglob("*.coma"):
                os.utime(path, None)
        elif "--only=prove" in command:
            text = (EVIDENCE.parents[1] / target / "first" / "proof.log").read_text(
                encoding="utf-8"
            )
            if target == unresolved_target:
                text = text.replace(
                    "  Goal vc_project_top_budget: ✔ (1)",
                    "  Goal vc_project_top_budget: ✘ (0)",
                    1,
                )
            log_path.write_text(text, encoding="utf-8")
        else:
            log_path.write_text("fixture subprocess completed\n")
        return {"command": command, "elapsed_seconds": 0.01, "log": str(log_path)}

    monkeypatch.setattr(verify_creusot, "run_command", fixture_command)
    output = root / "report"
    output.mkdir()

    if unresolved_target is not None:
        with pytest.raises(verify_creusot.ProofError, match="unproved goal"):
            verify_creusot.verify(root, output, manifest, time.monotonic() + 10)
    else:
        results = verify_creusot.verify(root, output, manifest, time.monotonic() + 10)
        assert [result["target"] for result in results] == list(verify_creusot.TARGETS)
        assert all(result["proved"] == {"files": 25, "goals": 59} for result in results)
    proves = [call for call in calls if "--only=prove" in call]
    assert len(proves) == 3
    assert all(
        "--no-cache" in call and "-j" in call and call[call.index("-j") + 1] == "2"
        for call in proves
    )
    assert sum("check" in call for call in calls) == 6
