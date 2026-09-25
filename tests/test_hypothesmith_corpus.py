import hashlib
import json
import subprocess
import sys
from pathlib import Path

import pytest

from tools import hypothesmith_corpus as corpus

ROOT = Path(__file__).resolve().parents[1]
USAGE_ERROR = 2
MAX_CORPUS_BYTES = 4096


def test_preserves_bytes_and_deduplicates(tmp_path: Path) -> None:
    source = "value = '日本語'\r\n"
    encoded = source.encode()
    assert corpus.store_source(source, tmp_path, max_bytes=len(encoded)) == "written"
    files = list(tmp_path.iterdir())
    assert len(files) == 1
    assert files[0].name == hashlib.sha256(encoded).hexdigest()
    assert files[0].read_bytes() == encoded
    assert corpus.store_source(source, tmp_path, max_bytes=len(encoded)) == "existing"
    assert (
        corpus.store_source(source, tmp_path, max_bytes=len(encoded) - 1) == "oversize"
    )
    assert list(tmp_path.iterdir()) == files


@pytest.mark.parametrize(
    ("source", "expected"),
    [
        ("", "empty"),
        (" \n\t", "empty"),
        ("def f(:", "invalid"),
        ("return 1", "invalid"),
        ("x = '\ud800'", "invalid"),
    ],
)
def test_rejects_unusable_sources(tmp_path: Path, source: str, expected: str) -> None:
    assert corpus.store_source(source, tmp_path, max_bytes=4096) == expected
    assert not list(tmp_path.iterdir())


def test_only_compiles_generated_code(tmp_path: Path) -> None:
    sentinel = tmp_path / "must-not-exist"
    output = tmp_path / "corpus"
    output.mkdir()
    source = f"from pathlib import Path\nPath({str(sentinel)!r}).touch()\n"
    assert corpus.store_source(source, output, max_bytes=4096) == "written"
    assert not sentinel.exists()


@pytest.mark.parametrize(("options", "expected"), [(["--help"], 0), ([], USAGE_ERROR)])
def test_cli_without_optional_dependencies(
    tmp_path: Path,
    options: list[str],
    expected: int,
) -> None:
    # -S excludes site-packages, exercising the real missing-dependency path.
    result = subprocess.run(  # noqa: S603 -- Fixed interpreter and checked-in script.
        [
            sys.executable,
            "-S",
            str(ROOT / "tools/hypothesmith_corpus.py"),
            "--output",
            str(tmp_path / "corpus"),
            *options,
        ],
        check=False,
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert result.returncode == expected
    assert not (tmp_path / "corpus").exists()
    if expected == USAGE_ERROR:
        assert "install fuzz dependencies" in result.stderr


@pytest.mark.parametrize(
    ("option", "limit"),
    [
        ("--max-bytes", 0),
        ("--max-bytes", -1),
        ("--max-bytes", 4097),
        ("--examples", 0),
        ("--examples", -1),
    ],
)
def test_cli_rejects_invalid_limits(tmp_path: Path, option: str, limit: int) -> None:
    # Run only the checked-in CLI with fixed arguments, never generated code.
    result = subprocess.run(  # noqa: S603
        [
            sys.executable,
            str(ROOT / "tools/hypothesmith_corpus.py"),
            "--output",
            str(tmp_path / "corpus"),
            option,
            str(limit),
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    assert result.returncode == USAGE_ERROR
    assert not (tmp_path / "corpus").exists()


@pytest.mark.parametrize("strategy", ["grammar", "libcst"])
def test_real_generator_is_reproducible_and_reports_existing_inputs(
    tmp_path: Path,
    strategy: str,
) -> None:
    pytest.importorskip(
        "hypothesmith", reason="install the optional fuzz dependency group"
    )
    command = [
        sys.executable,
        str(ROOT / "tools/hypothesmith_corpus.py"),
        "--examples",
        "10",
        "--seed",
        "20260926",
        "--strategy",
        strategy,
    ]
    outputs = [tmp_path / "first", tmp_path / "second", tmp_path / "first"]
    reports = []
    for output in outputs:
        # The executable and script are fixed; output is a pytest-owned directory.
        result = subprocess.run(  # noqa: S603
            [*command, "--output", str(output)],
            check=True,
            capture_output=True,
            text=True,
            timeout=60,
        )
        reports.append(json.loads(result.stdout))
    first = {p.name: p.read_bytes() for p in outputs[0].iterdir()}
    second = {p.name: p.read_bytes() for p in outputs[1].iterdir()}
    assert first == second
    assert first
    assert reports[0]["written"] == len(first)
    assert reports[2]["written"] == 0
    assert reports[2]["existing"] > 0
    for data in first.values():
        assert 0 < len(data) <= MAX_CORPUS_BYTES
        compile(data, "<generated>", "exec", dont_inherit=True)
