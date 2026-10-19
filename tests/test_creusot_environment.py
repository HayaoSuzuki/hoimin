"""Reject drift even when a tool prints a successful version response."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "tools/creusot_environment.py"
NAMES = (
    "rustc",
    "cargo",
    "creusot-rustc",
    "cargo-creusot",
    "ocamlc",
    "why3",
    "why3find",
    "z3",
    "cvc5",
    "timeout",
    "prlimit",
)


@pytest.fixture
def environment(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    monkeypatch.setenv("CREUSOT_DATA_HOME", str(tmp_path))
    tools = {}
    hashes = {}
    for name in NAMES:
        path = tmp_path / name
        path.write_text("#!/bin/sh\nprintf '%s\\n' 'approved 1.0'\n")
        path.chmod(0o755)
        tools[name] = {
            "path": str(path),
            "args": ["--version"],
            "version": "approved 1.0",
        }
        hashes[str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
    config = tmp_path / "why3.conf"
    config.write_text("[main]\nrunning_provers_max = 2\nmemlimit = 2048\n")
    project_config = tmp_path / "why3find.json"
    project_config.write_text(
        json.dumps(
            {
                "provers": ["z3@5.1.0", "cvc5@1.4.1"],
                "time": 1.0,
                "tactics": ["compute_specified", "split_vc"],
            }
        )
    )
    project_root = tmp_path / "project"
    project_root.mkdir()
    (project_root / "why3find.json").write_bytes(project_config.read_bytes())
    manifest = tmp_path / "toolchain.json"
    manifest.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "environment": {"CREUSOT_DATA_HOME": str(tmp_path)},
                "tools": tools,
                "files": {
                    str(config): hashlib.sha256(config.read_bytes()).hexdigest(),
                    str(project_config): hashlib.sha256(
                        project_config.read_bytes()
                    ).hexdigest(),
                },
                "project_config": str(project_config),
                "receipt": str(tmp_path / "receipt.json"),
                "artifacts": {
                    "source": {
                        "url": "https://example.com/pinned.tar.gz",
                        "sha256": "a" * 64,
                    }
                },
            }
        )
    )
    (tmp_path / "receipt.json").write_text(
        json.dumps(
            {
                "manifest_sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
                "files": hashes,
            }
        )
    )
    return manifest


def check(manifest: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(  # noqa: S603 -- Fixed Python interpreter and script.
        [
            sys.executable,
            str(SCRIPT),
            "check",
            "--manifest",
            str(manifest),
            "--project-root",
            str(manifest.parent / "project"),
        ],
        capture_output=True,
        text=True,
        check=False,
        env={**os.environ, "PATH": str(manifest.parent) + ":/usr/bin:/bin"},
    )


def test_requires_project_root_for_proof_check(environment: Path) -> None:
    result = subprocess.run(  # noqa: S603 -- Fixed Python interpreter and script.
        [sys.executable, str(SCRIPT), "check", "--manifest", str(environment)],
        capture_output=True,
        text=True,
        check=False,
        env={**os.environ, "PATH": str(environment.parent) + ":/usr/bin:/bin"},
    )

    assert result.returncode != 0
    assert "project-root" in result.stderr


def test_installation_only_check_does_not_need_a_project(environment: Path) -> None:
    (environment.parent / "project" / "why3find.json").unlink()

    result = subprocess.run(  # noqa: S603 -- Fixed Python interpreter and script.
        [
            sys.executable,
            str(SCRIPT),
            "check-installation",
            "--manifest",
            str(environment),
        ],
        capture_output=True,
        text=True,
        check=False,
        env={**os.environ, "PATH": str(environment.parent) + ":/usr/bin:/bin"},
    )

    assert result.returncode == 0, result.stderr


def test_accepts_matching_executables_and_configuration(environment: Path) -> None:
    result = check(environment)
    assert result.returncode == 0, result.stderr


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("provers", ["z3@5.1.0"]),
        ("time", 99.0),
        ("tactics", []),
    ],
)
def test_rejects_selected_project_configuration_drift(
    environment: Path, field: str, value: object
) -> None:
    selected = environment.parent / "project" / "why3find.json"
    content = json.loads(selected.read_text())
    content[field] = value
    selected.write_text(json.dumps(content))

    result = check(environment)

    assert result.returncode != 0
    assert "why3find.json" in result.stderr


def test_rejects_missing_selected_project_configuration(environment: Path) -> None:
    (environment.parent / "project" / "why3find.json").unlink()

    result = check(environment)

    assert result.returncode != 0
    assert "why3find.json" in result.stderr


@pytest.mark.parametrize(
    "name", ["z3", "cvc5", "why3", "creusot-rustc", "timeout", "prlimit"]
)
def test_rejects_missing_required_executable(environment: Path, name: str) -> None:
    (environment.parent / name).unlink()
    result = check(environment)
    assert result.returncode != 0
    assert name in result.stderr


@pytest.mark.parametrize("response", ["wrong 1.0", "approved 1.00", ""])
def test_rejects_wrong_version_even_with_matching_binary_hash(
    environment: Path, response: str
) -> None:
    tool = environment.parent / "z3"
    tool.write_text(f"#!/bin/sh\nprintf '%s\\n' '{response}'\n")
    receipt_path = environment.parent / "receipt.json"
    receipt = json.loads(receipt_path.read_text())
    receipt["files"][str(tool)] = hashlib.sha256(tool.read_bytes()).hexdigest()
    receipt_path.write_text(json.dumps(receipt))
    result = check(environment)
    assert result.returncode != 0
    assert "version" in result.stderr


def test_rejects_changed_content_printing_correct_version(environment: Path) -> None:
    tool = environment.parent / "z3"
    tool.write_text(tool.read_text() + "# different executable\n")
    result = check(environment)
    assert result.returncode != 0
    assert "hash" in result.stderr


def test_rejects_wrong_configuration(environment: Path) -> None:
    (environment.parent / "why3.conf").write_text("[main]\nrunning_provers_max = 4\n")
    result = check(environment)
    assert result.returncode != 0
    assert "why3.conf" in result.stderr


@pytest.mark.parametrize(
    "content", ["{", "[]", "{}", '{"schema_version": 1, "tools": {}}']
)
def test_rejects_malformed_manifest(tmp_path: Path, content: str) -> None:
    manifest = tmp_path / "toolchain.json"
    manifest.write_text(content)
    result = check(manifest)
    assert result.returncode != 0
    assert "manifest" in result.stderr


def test_rejects_missing_build_receipt(environment: Path) -> None:
    (environment.parent / "receipt.json").unlink()
    result = check(environment)
    assert result.returncode != 0
    assert "receipt" in result.stderr


@pytest.mark.parametrize(
    ("section", "key", "value"),
    [
        ("root", "schema_version", True),
        ("root", "tools", {}),
        ("root", "files", {}),
        ("root", "artifacts", {}),
        ("root", "receipt", "relative.json"),
        ("z3", "path", "relative-z3"),
        ("z3", "args", "--version"),
        ("z3", "version", ""),
        ("source", "sha256", "a" * 63),
        ("source", "url", "http://example.com/unverified.tar.gz"),
    ],
)
def test_rejects_invalid_manifest_fields(
    environment: Path, section: str, key: str, value: object
) -> None:
    manifest = json.loads(environment.read_text())
    target = {
        "root": manifest,
        "z3": manifest["tools"]["z3"],
        "source": manifest["artifacts"]["source"],
    }[section]
    target[key] = value
    environment.write_text(json.dumps(manifest))
    receipt_path = environment.parent / "receipt.json"
    receipt = json.loads(receipt_path.read_text())
    receipt["manifest_sha256"] = hashlib.sha256(environment.read_bytes()).hexdigest()
    receipt_path.write_text(json.dumps(receipt))

    result = check(environment)

    assert result.returncode != 0
    assert "manifest" in result.stderr


def test_rejects_failed_tool_printing_correct_version(environment: Path) -> None:
    tool = environment.parent / "z3"
    tool.write_text(tool.read_text() + "exit 1\n")
    receipt_path = environment.parent / "receipt.json"
    receipt = json.loads(receipt_path.read_text())
    receipt["files"][str(tool)] = hashlib.sha256(tool.read_bytes()).hexdigest()
    receipt_path.write_text(json.dumps(receipt))

    result = check(environment)

    assert result.returncode != 0
    assert "version" in result.stderr


def test_rejects_changed_manifest_against_build_receipt(environment: Path) -> None:
    environment.write_text(environment.read_text() + "\n")

    result = check(environment)

    assert result.returncode != 0
    assert "manifest hash mismatch" in result.stderr


def test_rejects_executable_absent_from_receipt(environment: Path) -> None:
    receipt_path = environment.parent / "receipt.json"
    receipt = json.loads(receipt_path.read_text())
    del receipt["files"][str(environment.parent / "z3")]
    receipt_path.write_text(json.dumps(receipt))

    result = check(environment)

    assert result.returncode != 0
    assert "receipt hash for z3" in result.stderr


def test_rejects_environment_selecting_different_configuration(
    environment: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("CREUSOT_DATA_HOME", "/different/configuration")

    result = check(environment)

    assert result.returncode != 0
    assert "CREUSOT_DATA_HOME" in result.stderr


def test_rejects_path_selecting_different_executable(environment: Path) -> None:
    different = environment.parent / "different" / "z3"
    different.parent.mkdir()
    different.write_bytes((environment.parent / "z3").read_bytes())
    different.chmod(0o755)
    manifest = json.loads(environment.read_text())
    manifest["tools"]["z3"]["path"] = str(different)
    environment.write_text(json.dumps(manifest))
    receipt_path = environment.parent / "receipt.json"
    receipt = json.loads(receipt_path.read_text())
    receipt["manifest_sha256"] = hashlib.sha256(environment.read_bytes()).hexdigest()
    receipt["files"][str(different)] = hashlib.sha256(
        different.read_bytes()
    ).hexdigest()
    receipt_path.write_text(json.dumps(receipt))

    result = check(environment)

    assert result.returncode != 0
    assert "PATH" in result.stderr
