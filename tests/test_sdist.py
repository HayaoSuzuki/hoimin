"""Check source distribution inputs against the actual generated archive."""

import subprocess
import sys
import tarfile
from pathlib import Path
from tempfile import TemporaryDirectory

import pytest

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture(scope="module")
def source_archive(tmp_path_factory: pytest.TempPathFactory) -> Path:
    output = tmp_path_factory.mktemp("sdist")
    with TemporaryDirectory(
        prefix="hoimin-sdist-excluded-", dir=ROOT / "vendor/ruff_python_parser"
    ) as excluded:
        for relative in ("target/build.bin", "cache/state.bin", "scratch/output.rs"):
            path = Path(excluded) / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"untracked build/cache output")
        subprocess.run(  # noqa: S603 -- Fixed build tool and argv, no shell.
            [sys.executable, "-m", "maturin", "sdist", "--out", str(output)],
            cwd=ROOT,
            check=True,
            capture_output=True,
            timeout=120,
        )
    archives = list(output.glob("hoimin-*.tar.gz"))
    assert len(archives) == 1
    return archives[0]


@pytest.mark.parametrize(
    "member",
    [
        "crates/hoimin-python/Cargo.toml",
        "crates/hoimin-python/src/lib.rs",
        "vendor/ruff_python_parser/Cargo.toml",
        "vendor/ruff_python_parser/README.hoimin.md",
        "vendor/ruff_python_parser/LICENSE",
        *sorted(
            path.relative_to(ROOT).as_posix()
            for path in (ROOT / "vendor/ruff_python_parser/src").rglob("*.rs")
        ),
    ],
)
def test_sdist_retains_internal_python_api_and_patched_parser_bytes(
    source_archive: Path, member: str
) -> None:
    expected = (ROOT / member).read_bytes()

    with tarfile.open(source_archive) as archive:
        matches = [name for name in archive.getnames() if name.endswith("/" + member)]
        assert len(matches) == 1, f"missing or duplicate sdist member: {member}"
        source = archive.extractfile(matches[0])
        assert source is not None
        actual = source.read()

    assert actual == expected


def test_sdist_excludes_untracked_vendor_build_and_cache_outputs(
    source_archive: Path,
) -> None:
    with tarfile.open(source_archive) as archive:
        names = archive.getnames()

    assert not any("/hoimin-sdist-excluded-" in name for name in names)
