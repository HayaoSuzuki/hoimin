from __future__ import annotations

import io
import os
import subprocess
import sys
import tarfile
import venv
import zipfile
from collections.abc import Callable
from pathlib import Path

import pytest
from archive_smoke import extract_binary
from pytest_mock import MockerFixture
from wheel_smoke import exercise_cli

ARCHIVE_MEMBERS = {
    "hoimin": b"artifact binary",
    "README.md": b"notice",
    "LICENSE": b"notice",
    "vendor/ruff_python_parser/LICENSE": b"notice",
}


def write_zip(archive: Path) -> None:
    with zipfile.ZipFile(archive, "w") as stream:
        for name, data in ARCHIVE_MEMBERS.items():
            stream.writestr(name, data)


def write_tar(archive: Path) -> None:
    with tarfile.open(archive, "w:gz") as stream:
        for name, data in ARCHIVE_MEMBERS.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            stream.addfile(info, io.BytesIO(data))


@pytest.mark.parametrize(
    ("kind", "write_archive"), [("zip", write_zip), ("tar.gz", write_tar)]
)
def test_extracts_exact_binary_without_using_archive_paths(
    tmp_path: Path, kind: str, write_archive: Callable[[Path], None]
) -> None:
    archive = tmp_path / f"release.{kind}"
    write_archive(archive)

    result = extract_binary(archive, tmp_path / "unpacked", "hoimin")

    assert result.read_bytes() == b"artifact binary"
    assert list(result.parent.iterdir()) == [result]


@pytest.mark.parametrize(
    ("binary_names", "binary_type", "linkname"),
    [
        pytest.param(["../hoimin"], tarfile.REGTYPE, "", id="traversal"),
        pytest.param(["hoimin", "hoimin"], tarfile.REGTYPE, "", id="duplicate"),
        pytest.param(["hoimin"], tarfile.SYMTYPE, "elsewhere", id="link"),
    ],
)
def test_rejects_unexpected_duplicate_and_link_members(
    tmp_path: Path, binary_names: list[str], binary_type: bytes, linkname: str
) -> None:
    archive = tmp_path / "release.tar.gz"
    with tarfile.open(archive, "w:gz") as stream:
        for name in binary_names:
            info = tarfile.TarInfo(name)
            info.type = binary_type
            info.linkname = linkname
            stream.addfile(info, io.BytesIO(b""))
        for name in list(ARCHIVE_MEMBERS)[1:]:
            stream.addfile(tarfile.TarInfo(name), io.BytesIO(b""))

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
        for name, mode in [
            ("hoimin", 0o120777),
            ("README.md", 0o100644),
            ("LICENSE", 0o100644),
            ("vendor/ruff_python_parser/LICENSE", 0o100644),
        ]:
            info = zipfile.ZipInfo(name)
            info.create_system = 3
            info.external_attr = mode << 16
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
