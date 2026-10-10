from __future__ import annotations

import io
import os
import subprocess
import sys
import tarfile
import venv
import zipfile
from pathlib import Path

import pytest
from archive_smoke import extract_binary
from pytest_mock import MockerFixture
from wheel_smoke import exercise_cli


@pytest.mark.parametrize("kind", ["zip", "tar.gz"])
def test_extracts_exact_binary_without_using_archive_paths(
    tmp_path: Path, kind: str
) -> None:
    archive = tmp_path / f"release.{kind}"
    names = ["hoimin", "README.md", "LICENSE", "vendor/ruff_python_parser/LICENSE"]
    if kind == "zip":
        with zipfile.ZipFile(archive, "w") as stream:
            for name in names:
                stream.writestr(
                    name, b"artifact binary" if name == "hoimin" else b"notice"
                )
    else:
        with tarfile.open(archive, "w:gz") as stream:
            for name in names:
                data = b"artifact binary" if name == "hoimin" else b"notice"
                info = tarfile.TarInfo(name)
                info.size = len(data)
                stream.addfile(info, io.BytesIO(data))
    result = extract_binary(archive, tmp_path / "unpacked", "hoimin")
    assert result.read_bytes() == b"artifact binary"
    assert list(result.parent.iterdir()) == [result]


@pytest.mark.parametrize("bad_member", ["../hoimin", "hoimin", "link"])
def test_rejects_unexpected_duplicate_and_link_members(
    tmp_path: Path, bad_member: str
) -> None:
    archive = tmp_path / "release.tar.gz"
    with tarfile.open(archive, "w:gz") as stream:
        names = [
            "hoimin",
            "README.md",
            "LICENSE",
            "vendor/ruff_python_parser/LICENSE",
        ]
        if bad_member == "hoimin":
            names.append(bad_member)
        elif bad_member == "../hoimin":
            names[0] = bad_member
        for name in names:
            info = tarfile.TarInfo(name)
            if bad_member == "link" and name == "hoimin":
                info.type = tarfile.SYMTYPE
                info.linkname = "elsewhere"
            stream.addfile(info, io.BytesIO(b""))
    with pytest.raises(AssertionError):
        extract_binary(archive, tmp_path / "unpacked", "hoimin")
    assert not (tmp_path / "unpacked").exists()


def test_missing_executable_cannot_fall_back_to_path(tmp_path: Path) -> None:
    with pytest.raises(FileNotFoundError):
        exercise_cli(
            tmp_path / "missing-hoimin",
            Path(sys.executable),
            tmp_path / "fixture",
            "0.3.0",
        )


def test_version_mismatch_stops_before_mutation(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    command = mocker.patch(
        "wheel_smoke.run",
        return_value=subprocess.CompletedProcess([], 0, "hoimin 99.0.0", ""),
    )
    with pytest.raises(AssertionError):
        exercise_cli(
            Path(sys.executable), Path(sys.executable), tmp_path / "fixture", "0.3.0"
        )
    assert command.call_count == 1


def test_preview_version_matches_python_metadata_spelling(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    mocker.patch(
        "wheel_smoke.run",
        side_effect=[
            subprocess.CompletedProcess([], 0, "hoimin 0.3.0-dev.123\n", ""),
            subprocess.CompletedProcess([], 0, "Usage: hoimin", ""),
            subprocess.CompletedProcess([], 0, '{"mutants":[{"status":"killed"}]}', ""),
        ],
    )
    exercise_cli(
        Path(sys.executable), Path(sys.executable), tmp_path / "fixture", "0.3.0.dev123"
    )


def test_zip_symlink_is_not_executed(tmp_path: Path) -> None:
    archive = tmp_path / "release.zip"
    with zipfile.ZipFile(archive, "w") as stream:
        for name in [
            "hoimin",
            "README.md",
            "LICENSE",
            "vendor/ruff_python_parser/LICENSE",
        ]:
            info = zipfile.ZipInfo(name)
            info.create_system = 3
            info.external_attr = (0o120777 if name == "hoimin" else 0o100644) << 16
            stream.writestr(info, b"not an executable")
    with pytest.raises(AssertionError):
        extract_binary(archive, tmp_path / "unpacked", "hoimin")
    assert not (tmp_path / "unpacked").exists()


@pytest.mark.skipif(os.name == "nt", reason="Unix venv interpreter symlink contract")
def test_venv_interpreter_symlink_keeps_its_environment(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    environment = tmp_path / "venv"
    venv.EnvBuilder(symlinks=True).create(environment)
    python = environment / "bin/python"
    assert python.is_symlink()
    command = mocker.patch(
        "wheel_smoke.run",
        side_effect=[
            subprocess.CompletedProcess([], 0, "hoimin 0.3.0", ""),
            subprocess.CompletedProcess([], 0, "Usage: hoimin", ""),
            subprocess.CompletedProcess([], 0, '{"mutants":[{"status":"killed"}]}', ""),
        ],
    )
    exercise_cli(Path(sys.executable), python, tmp_path / "fixture", "0.3.0")
    assert command.call_args.args[0][-4] == str(python.absolute())
