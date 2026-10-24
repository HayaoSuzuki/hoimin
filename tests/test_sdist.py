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
        "crates/hoimin-python/src/lib.rs",
        "crates/hoimin-budget-math/Cargo.toml",
        "crates/hoimin-budget-math/src/lib.rs",
        "crates/hoimin-budget-math/src/duration.rs",
        "crates/hoimin-budget-math/src/projection.rs",
        "vendor/ruff_python_parser/Cargo.toml",
        "vendor/ruff_python_parser/README.hoimin.md",
        "vendor/ruff_python_parser/LICENSE",
        *sorted(
            path.relative_to(ROOT).as_posix()
            for crate in (ROOT / "vendor").glob("ruff_*")
            for path in crate.rglob("*")
            if path.is_file()
            and (
                path.name
                in {"Cargo.toml", "build.rs", "UnicodeData.txt", "NameAliases.txt"}
                or path.name.startswith("LICENSE")
                or (path.suffix == ".rs" and "src" in path.relative_to(crate).parts)
            )
            and path.relative_to(ROOT).as_posix()
            not in {
                "vendor/ruff_python_parser/Cargo.toml",
                "vendor/ruff_python_parser/LICENSE",
            }
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

    if member.endswith("Cargo.toml"):
        # Maturin rewrites local dependency manifests with LF, including CRLF checkouts.
        actual = actual.replace(b"\r\n", b"\n")
        expected = expected.replace(b"\r\n", b"\n")
    assert actual == expected


def test_sdist_excludes_untracked_vendor_build_and_cache_outputs(
    source_archive: Path,
) -> None:
    with tarfile.open(source_archive) as archive:
        names = archive.getnames()

    assert not any("/hoimin-sdist-excluded-" in name for name in names)


@pytest.mark.parametrize("line_ending", [b"\n", b"\r\n"], ids=["lf", "crlf"])
def test_sdist_retains_workspace_manifest_text(
    source_archive: Path, line_ending: bytes
) -> None:
    member = "crates/hoimin-python/Cargo.toml"
    expected_lf = (ROOT / member).read_bytes().replace(b"\r\n", b"\n")
    expected = expected_lf.replace(b"\n", line_ending)

    with tarfile.open(source_archive) as archive:
        matches = [name for name in archive.getnames() if name.endswith("/" + member)]
        assert len(matches) == 1, f"missing or duplicate sdist member: {member}"
        source = archive.extractfile(matches[0])
        assert source is not None
        actual = source.read()

    # Maturin rewrites workspace manifests with LF, including CRLF checkouts.
    assert actual.replace(b"\r\n", b"\n") == expected.replace(b"\r\n", b"\n")
