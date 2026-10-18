from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tomllib
import zipfile
from collections.abc import Callable
from pathlib import Path
from typing import Any, NamedTuple

import pytest
import yaml
from sbom_fixtures import attach_sboms

from tools import release as automation

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools/release.py"
VERSION_FILES = ("Cargo.toml", "Cargo.lock", "pyproject.toml", "uv.lock")


@pytest.mark.parametrize(
    ("job", "entrypoint"),
    [
        ("reserve", "Reserve release tag"),
        ("windows-wheel", "Set release version in manifests and lockfiles"),
        ("linux-wheel", "Set release version in manifests and lockfiles"),
        ("macos-wheel", "Set release version in manifests and lockfiles"),
    ],
)
def test_release_bootstrap_installs_tooling_before_use(
    job: str, entrypoint: str
) -> None:
    workflow = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())
    steps = workflow["jobs"][job]["steps"]
    names = [step.get("name") for step in steps]

    installation = names.index("Install internal Python tooling")
    invocation = names.index(entrypoint)

    assert installation < invocation
    assert steps[installation]["run"] == (
        "uv sync --frozen --only-group internal-tooling --no-install-project"
    )
    assert (
        "uv run --frozen --no-sync python tools/release.py" in steps[invocation]["run"]
    )


def command(*args: str, cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(  # noqa: S603
        args, cwd=cwd, capture_output=True, text=True, check=False, timeout=30
    )


def release(root: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return command(sys.executable, str(SCRIPT), "--root", str(root), *args, cwd=root)


def git(root: Path, *args: str) -> str:
    result = command("git", *args, cwd=root)
    assert result.returncode == 0, result.stderr
    return result.stdout.strip()


@pytest.fixture
def repository(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    monkeypatch.setenv("GIT_CONFIG_NOSYSTEM", "1")
    monkeypatch.setenv("GIT_CONFIG_GLOBAL", os.devnull)
    root = tmp_path / "checkout"
    root.mkdir()
    for name in VERSION_FILES:
        shutil.copyfile(ROOT / name, root / name)
    # Keep tag-allocation scenarios independent of the release being prepared.
    automation.set_version(root, "0.1.0")
    git(root, "init", "--initial-branch=main")
    git(root, "config", "user.name", "Release test")
    git(root, "config", "user.email", "release@example.invalid")
    git(root, "add", ".")
    git(root, "commit", "-m", "initial")
    remote = tmp_path / "remote.git"
    git(tmp_path, "init", "--bare", str(remote))
    git(root, "remote", "add", "origin", str(remote))
    git(root, "push", "origin", "main")
    return root


def test_tags_are_remote_reserved_monotonic_and_reused(repository: Path) -> None:
    first = git(repository, "rev-parse", "HEAD")

    result = release(repository, "tag", "--commit", first)
    remote_tags = git(repository, "ls-remote", "--tags", "origin")
    git(repository, "commit", "--allow-empty", "-m", "second")
    second = git(repository, "rev-parse", "HEAD")
    second_release = release(repository, "tag", "--commit", second)
    reused_release = release(repository, "tag", "--commit", first)

    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "v0.1.0"
    assert remote_tags.startswith(first)
    assert second_release.stdout.strip() == "v0.1.1"
    assert reused_release.stdout.strip() == "v0.1.0"


def test_tags_ignore_previews_and_peel_annotated_tags(repository: Path) -> None:
    commit = git(repository, "rev-parse", "HEAD")
    git(repository, "tag", "-a", "v2.3.9", "-m", "release")
    git(repository, "tag", "v99.0.0-dev.1")
    git(repository, "push", "origin", "--tags")

    reused_release = release(repository, "tag", "--commit", commit)
    git(repository, "commit", "--allow-empty", "-m", "next")
    commit = git(repository, "rev-parse", "HEAD")
    next_release = release(repository, "tag", "--commit", commit)

    assert reused_release.stdout.strip() == "v2.3.9"
    assert next_release.stdout.strip() == "v2.3.10"


def test_failed_tag_push_cannot_claim_success(repository: Path) -> None:
    remote = Path(git(repository, "remote", "get-url", "origin"))
    hook = remote / "hooks/pre-receive"
    hook.write_text("#!/bin/sh\nexit 1\n")
    hook.chmod(0o755)
    result = release(
        repository, "tag", "--commit", git(repository, "rev-parse", "HEAD")
    )

    assert result.returncode != 0
    assert not result.stdout.strip()
    assert not git(repository, "ls-remote", "--tags", "origin")


@pytest.fixture
def push_competitor(
    monkeypatch: pytest.MonkeyPatch,
) -> Callable[[Callable[[], None]], None]:
    def install(compete: Callable[[], None]) -> None:
        original_git = automation.git
        effects = iter([compete])

        def no_competition() -> None:
            pass

        def competing_git(
            root: Path, *args: str, check: bool = True
        ) -> subprocess.CompletedProcess[str]:
            def compete_once() -> None:
                next(effects, no_competition)()

            effects_for_command = {"push": compete_once}
            effects_for_command.get(args[0], no_competition)()
            return original_git(root, *args, check=check)

        monkeypatch.setattr(automation, "git", competing_git)

    return install


def test_tag_collision_retries_without_replacing_other_commit(
    repository: Path, push_competitor: Callable[[Callable[[], None]], None]
) -> None:
    first = git(repository, "rev-parse", "HEAD")
    git(repository, "commit", "--allow-empty", "-m", "second")
    second = git(repository, "rev-parse", "HEAD")
    original_git = automation.git

    def compete() -> None:
        original_git(repository, "push", "origin", f"{first}:refs/tags/v0.1.0")

    push_competitor(compete)

    tag = automation.reserve_tag(repository, second)

    assert tag == "v0.1.1"

    tags = automation.remote_tags(repository)

    assert tags == {"v0.1.0": first, "v0.1.1": second}


def test_delayed_older_commit_is_skipped_without_creating_a_tag(
    repository: Path,
) -> None:
    older = git(repository, "rev-parse", "HEAD")
    git(repository, "commit", "--allow-empty", "-m", "newer")
    newer = git(repository, "rev-parse", "HEAD")

    tag = automation.reserve_tag(repository, newer)

    assert tag == "v0.1.0"
    result = release(repository, "tag", "--commit", older)

    assert result.returncode == 0, result.stderr
    assert not result.stdout.strip()
    assert "Skipping release" in result.stderr

    tags = automation.remote_tags(repository)

    assert tags == {"v0.1.0": newer}


def test_newer_commit_winning_tag_race_prevents_older_retry(
    repository: Path, push_competitor: Callable[[Callable[[], None]], None]
) -> None:
    older = git(repository, "rev-parse", "HEAD")
    git(repository, "commit", "--allow-empty", "-m", "newer")
    newer = git(repository, "rev-parse", "HEAD")
    original_git = automation.git

    def compete() -> None:
        original_git(repository, "push", "origin", f"{newer}:refs/tags/v0.1.0")

    push_competitor(compete)

    tag = automation.reserve_tag(repository, older)

    assert tag is None

    tags = automation.remote_tags(repository)

    assert tags == {"v0.1.0": newer}


class ReservationRace(NamedTuple):
    competing_role: str
    reserving_role: str
    competing_tag: str
    expected_tag: str | None
    expected_roles: dict[str, str]


@pytest.mark.parametrize(
    "race",
    [
        pytest.param(
            ReservationRace("newer", "older", "v1.0.0", None, {"v1.0.0": "newer"}),
            id="newer-wins",
        ),
        pytest.param(
            ReservationRace(
                "older",
                "newer",
                "v2.0.0",
                "v2.0.1",
                {"v2.0.0": "older", "v2.0.1": "newer"},
            ),
            id="older-wins",
        ),
    ],
)
def test_different_version_floors_share_atomic_reservation(
    repository: Path,
    tmp_path: Path,
    push_competitor: Callable[[Callable[[], None]], None],
    race: ReservationRace,
) -> None:
    automation.set_version(repository, "2.0.0")
    git(repository, "commit", "-am", "raise floor")
    older = git(repository, "rev-parse", "HEAD")

    automation.set_version(repository, "1.0.0")
    git(repository, "commit", "-am", "lower floor")
    newer = git(repository, "rev-parse", "HEAD")
    other = tmp_path / "older-checkout"
    git(tmp_path, "clone", str(repository), str(other))
    git(other, "checkout", older)
    git(
        other,
        "remote",
        "set-url",
        "origin",
        git(repository, "remote", "get-url", "origin"),
    )
    checkouts = {"older": other, "newer": repository}
    commits = {"older": older, "newer": newer}

    def compete() -> None:
        reserved = automation.reserve_tag(
            checkouts[race.competing_role], commits[race.competing_role]
        )
        assert reserved == race.competing_tag

    push_competitor(compete)

    tag = automation.reserve_tag(
        checkouts[race.reserving_role], commits[race.reserving_role]
    )

    assert tag == race.expected_tag
    tags = automation.remote_tags(repository)
    assert tags == {tag: commits[role] for tag, role in race.expected_roles.items()}

    assert (
        git(
            repository, "ls-remote", "origin", "refs/heads/hoimin-release-state"
        ).split()[0]
        == newer
    )


def test_rejected_state_update_does_not_leave_a_version_tag(repository: Path) -> None:
    remote = Path(git(repository, "remote", "get-url", "origin"))
    hook = remote / "hooks/update"
    hook.write_text(
        '#!/bin/sh\nif [ "$1" = "refs/heads/hoimin-release-state" ]; then exit 1; fi\n'
    )
    hook.chmod(0o755)
    result = release(
        repository, "tag", "--commit", git(repository, "rev-parse", "HEAD")
    )

    assert result.returncode != 0

    release_state = automation.remote_release_state(repository)

    assert release_state == ({}, "")


def test_divergent_history_cannot_get_newer_stable_tag(repository: Path) -> None:
    base = git(repository, "rev-parse", "HEAD")
    git(repository, "commit", "--allow-empty", "-m", "released")
    released = git(repository, "rev-parse", "HEAD")

    tag = automation.reserve_tag(repository, released)

    assert tag == "v0.1.0"
    git(repository, "checkout", "-b", "divergent", base)
    git(repository, "commit", "--allow-empty", "-m", "different history")

    next_tag = automation.reserve_tag(repository, git(repository, "rev-parse", "HEAD"))

    assert next_tag is None

    tags = automation.remote_tags(repository)

    assert tags == {"v0.1.0": released}


def test_tag_ancestry_fetches_new_remote_objects(
    repository: Path, tmp_path: Path
) -> None:
    older = git(repository, "rev-parse", "HEAD")
    other = tmp_path / "other"
    git(tmp_path, "clone", "--branch", "main", str(repository), str(other))
    git(other, "config", "user.name", "Release test")
    git(other, "config", "user.email", "release@example.invalid")
    git(
        other,
        "remote",
        "set-url",
        "origin",
        git(repository, "remote", "get-url", "origin"),
    )
    git(other, "commit", "--allow-empty", "-m", "newer elsewhere")
    newer = git(other, "rev-parse", "HEAD")

    tag = automation.reserve_tag(other, newer)

    assert tag == "v0.1.0"

    next_tag = automation.reserve_tag(repository, older)

    assert next_tag is None

    tags = automation.remote_tags(repository)

    assert tags == {"v0.1.0": newer}


def test_ancestry_errors_are_not_reported_as_successful_skips(
    repository: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    tag = automation.reserve_tag(repository, git(repository, "rev-parse", "HEAD"))

    assert tag == "v0.1.0"
    git(repository, "commit", "--allow-empty", "-m", "next")
    original_git = automation.git

    def broken_ancestry(
        root: Path, *args: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        commands = {
            "merge-base": lambda: subprocess.CompletedProcess(
                args, 128, "", "corrupt history"
            )
        }
        return commands.get(args[0], lambda: original_git(root, *args, check=check))()

    monkeypatch.setattr(automation, "git", broken_ancestry)

    with pytest.raises(subprocess.CalledProcessError):
        automation.reserve_tag(repository, git(repository, "rev-parse", "HEAD"))


def test_shallow_checkout_is_deepened_before_ancestry_check(
    repository: Path, tmp_path: Path
) -> None:
    first = git(repository, "rev-parse", "HEAD")

    tag = automation.reserve_tag(repository, first)

    assert tag == "v0.1.0"
    git(repository, "commit", "--allow-empty", "-m", "newer")
    newer = git(repository, "rev-parse", "HEAD")
    git(repository, "push", "origin", "main")
    remote = Path(git(repository, "remote", "get-url", "origin"))
    shallow = tmp_path / "shallow"
    git(
        tmp_path,
        "clone",
        "--depth=1",
        "--branch",
        "main",
        remote.as_uri(),
        str(shallow),
    )

    assert git(shallow, "rev-parse", "--is-shallow-repository") == "true"

    next_tag = automation.reserve_tag(shallow, newer)

    assert next_tag == "v0.1.1"
    assert git(shallow, "rev-parse", "--is-shallow-repository") == "false"


@pytest.fixture
def prepare_workflow(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> dict[str, Any]:
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    (fake_bin / "python").symlink_to(sys.executable)
    (tmp_path / "tools").mkdir()
    (tmp_path / "tools/release.py").write_text(
        "import os, sys\n"
        "mode = os.environ['RELEASE_TEST_MODE']\n"
        "if mode == 'release': print('v1.2.3')\n"
        "if mode == 'error': sys.exit(23)\n"
    )
    (tmp_path / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\n')
    monkeypatch.setenv("PATH", str(fake_bin) + os.pathsep + os.environ["PATH"])
    monkeypatch.setenv("PUBLISH", "true")
    monkeypatch.setenv("COMMIT", "a" * 40)
    monkeypatch.setenv("GITHUB_RUN_ID", "123")
    return yaml.safe_load(
        (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    )


@pytest.fixture
def reserve_script(prepare_workflow: dict[str, Any]) -> str:
    script = next(
        step["run"]
        for step in prepare_workflow["jobs"]["reserve"]["steps"]
        if step.get("id") == "tag"
    )
    assert isinstance(script, str)
    return script


@pytest.fixture
def prepare_script(prepare_workflow: dict[str, Any]) -> str:
    script = next(
        step["run"]
        for step in prepare_workflow["jobs"]["prepare"]["steps"]
        if step.get("id") == "version"
    )
    assert isinstance(script, str)
    return script


@pytest.mark.skipif(os.name != "posix", reason="GitHub prepare job uses Ubuntu bash")
def test_reservation_error_stops_before_preparation(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, reserve_script: str
) -> None:
    monkeypatch.setenv("RELEASE_TEST_MODE", "error")
    reserve_output = tmp_path / "reserve-output"
    monkeypatch.setenv("GITHUB_OUTPUT", str(reserve_output))

    result = command("bash", "-e", "-o", "pipefail", "-c", reserve_script, cwd=tmp_path)

    assert result.returncode != 0
    assert not reserve_output.exists()
    assert not (tmp_path / "output").exists()


@pytest.mark.skipif(os.name != "posix", reason="GitHub prepare job uses Ubuntu bash")
def test_skipped_reservation_prevents_preparation(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    prepare_workflow: dict[str, Any],
    reserve_script: str,
) -> None:
    monkeypatch.setenv("RELEASE_TEST_MODE", "skip")
    reserve_output = tmp_path / "reserve-output"
    monkeypatch.setenv("GITHUB_OUTPUT", str(reserve_output))

    result = command("bash", "-e", "-o", "pipefail", "-c", reserve_script, cwd=tmp_path)

    assert result.returncode == 0, result.stderr
    reserved_tag = dict(
        line.split("=", 1) for line in reserve_output.read_text().splitlines()
    )["tag"]
    assert reserved_tag == ""
    assert (
        "needs.reserve.outputs.tag != ''" in prepare_workflow["jobs"]["prepare"]["if"]
    )
    assert not (tmp_path / "output").exists()


@pytest.mark.skipif(os.name != "posix", reason="GitHub prepare job uses Ubuntu bash")
def test_release_preparation_uses_reserved_tag(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    reserve_script: str,
    prepare_script: str,
) -> None:
    monkeypatch.setenv("RELEASE_TEST_MODE", "release")
    reserve_output = tmp_path / "reserve-output"
    monkeypatch.setenv("GITHUB_OUTPUT", str(reserve_output))

    reserved = command(
        "bash", "-e", "-o", "pipefail", "-c", reserve_script, cwd=tmp_path
    )

    assert reserved.returncode == 0, reserved.stderr
    reserved_tag = dict(
        line.split("=", 1) for line in reserve_output.read_text().splitlines()
    )["tag"]
    assert reserved_tag == "v1.2.3"
    monkeypatch.setenv("RESERVED_TAG", reserved_tag)
    output = tmp_path / "output"
    monkeypatch.setenv("GITHUB_OUTPUT", str(output))

    result = command("bash", "-e", "-o", "pipefail", "-c", prepare_script, cwd=tmp_path)

    assert result.returncode == 0, result.stderr
    values = dict(line.split("=", 1) for line in output.read_text().splitlines())
    assert values["publish"] == "true"
    assert values["commit"] == "a" * 40
    assert values["tag"] == "v1.2.3"
    assert values["version"] == "1.2.3"


@pytest.mark.skipif(os.name != "posix", reason="GitHub prepare job uses Ubuntu bash")
def test_preview_preparation_uses_run_version(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, prepare_script: str
) -> None:
    monkeypatch.setenv("RELEASE_TEST_MODE", "preview")
    monkeypatch.setenv("PUBLISH", "false")
    monkeypatch.setenv("RESERVED_TAG", "")
    output = tmp_path / "output"
    monkeypatch.setenv("GITHUB_OUTPUT", str(output))

    result = command("bash", "-e", "-o", "pipefail", "-c", prepare_script, cwd=tmp_path)

    assert result.returncode == 0, result.stderr
    values = dict(line.split("=", 1) for line in output.read_text().splitlines())
    assert values["publish"] == "false"
    assert values["commit"] == "a" * 40
    assert values["tag"] == "v0.1.0-dev.123"
    assert values["version"] == "0.1.0-dev.123"


def test_tag_push_with_lost_response_is_recovered(
    repository: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    commit = git(repository, "rev-parse", "HEAD")
    original_git = automation.git

    def lost_response(
        root: Path, *args: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        result = original_git(root, *args, check=check)
        responses = {
            "push": subprocess.CompletedProcess(result.args, 1, "", "lost response")
        }
        return responses.get(args[0], result)

    monkeypatch.setattr(automation, "git", lost_response)

    tag = automation.reserve_tag(repository, commit)

    assert tag == "v0.1.0"

    tags = automation.remote_tags(repository)

    assert tags == {"v0.1.0": commit}


@pytest.mark.parametrize("version", ["0.2.0", "1.0.0"])
def test_workspace_version_can_raise_release_floor(
    repository: Path, version: str
) -> None:
    git(repository, "tag", "v0.1.19")
    git(repository, "push", "origin", "--tags")

    assert release(repository, "set-version", version).returncode == 0
    git(repository, "commit", "-am", "raise release floor")
    commit = git(repository, "rev-parse", "HEAD")

    assert (
        release(repository, "tag", "--commit", commit).stdout.strip() == f"v{version}"
    )


@pytest.mark.parametrize("version", ["1.2.3", "1.2.3-dev.123"])
def test_version_updates_manifests_and_both_lockfiles(
    repository: Path, version: str
) -> None:
    result = release(repository, "set-version", version)
    documents = {
        name: tomllib.loads((repository / name).read_text(encoding="utf-8"))
        for name in VERSION_FILES
    }

    assert result.returncode == 0, result.stderr
    assert documents["Cargo.toml"]["workspace"]["package"]["version"] == version
    assert documents["pyproject.toml"]["project"]["version"] == version.replace(
        "-dev.", ".dev"
    )
    for name, packages, expected_version in (
        ("Cargo.lock", {"hoimin-core", "hoimin-cli", "hoimin-python"}, version),
        ("uv.lock", {"hoimin"}, version.replace("-dev.", ".dev")),
    ):
        original = tomllib.loads((ROOT / name).read_text(encoding="utf-8"))["package"]
        paired = list(zip(original, documents[name]["package"], strict=True))
        for before, after in [
            (before, after) for before, after in paired if before["name"] in packages
        ]:
            assert after == {**before, "version": expected_version}
        for before, after in [
            (before, after)
            for before, after in paired
            if before["name"] not in packages
        ]:
            assert after == before


@pytest.mark.parametrize("version", ["v1.2.3", "01.2.3", "1.2", "1.2.3\n", "1.2.3;id"])
def test_invalid_version_leaves_files_untouched(repository: Path, version: str) -> None:
    before = {name: (repository / name).read_bytes() for name in VERSION_FILES}

    result = release(repository, "set-version", version)

    assert result.returncode != 0
    assert {name: (repository / name).read_bytes() for name in VERSION_FILES} == before


def test_incomplete_metadata_is_rejected_before_any_write(repository: Path) -> None:
    (repository / "uv.lock").write_text("version = 1\n")
    before = {name: (repository / name).read_bytes() for name in VERSION_FILES}

    result = release(repository, "set-version", "1.2.3")

    assert result.returncode != 0
    assert {name: (repository / name).read_bytes() for name in VERSION_FILES} == before


def package_inputs(
    tmp_path: Path, platform: str, binary_name: str
) -> tuple[Path, dict[str, bytes], subprocess.CompletedProcess[str]]:
    binary = tmp_path / binary_name
    binary.write_bytes(b"executable payload")
    binary.chmod(0o755)
    (tmp_path / "README.md").write_text("usage")
    notices = {
        "LICENSE": b"project license terms\n",
        "vendor/ruff_python_parser/LICENSE": b"third-party license terms\n",
    }
    for name, content in notices.items():
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    result = release(
        tmp_path,
        "package",
        "--binary",
        str(binary),
        "--platform",
        platform,
        "--version",
        "1.2.3",
        "--directory",
        str(tmp_path / "dist"),
    )

    return binary, notices, result


def test_zip_archive_contains_executable_readme_and_licenses(tmp_path: Path) -> None:
    binary, notices, result = package_inputs(tmp_path, "windows-x86_64", "hoimin.exe")

    assert result.returncode == 0, result.stderr
    (archive,) = (tmp_path / "dist").iterdir()
    prefix = "hoimin-v1.2.3-windows-x86_64"
    assert archive.name == prefix + ".zip"
    with zipfile.ZipFile(archive) as zipped:
        assert set(zipped.namelist()) == {binary.name, "README.md", *notices}
        assert zipped.read(binary.name) == binary.read_bytes()
        for name, content in notices.items():
            assert zipped.read(name) == content


@pytest.mark.parametrize("platform", ["linux-x86_64", "macos-aarch64"])
def test_tar_archive_contains_executable_readme_and_licenses(
    tmp_path: Path, platform: str
) -> None:
    binary, notices, result = package_inputs(tmp_path, platform, "hoimin")

    assert result.returncode == 0, result.stderr
    (archive,) = (tmp_path / "dist").iterdir()
    prefix = f"hoimin-v1.2.3-{platform}"
    assert archive.name == prefix + ".tar.gz"
    with tarfile.open(archive) as packed:
        assert set(packed.getnames()) == {binary.name, "README.md", *notices}
        assert packed.getmember(binary.name).mode & 0o111
        contents = packed.extractfile(binary.name)
        assert contents is not None
        assert contents.read() == binary.read_bytes()
        for name, content in notices.items():
            notice = packed.extractfile(name)
            assert notice is not None
            assert notice.read() == content


def release_assets(directory: Path) -> list[Path]:
    names = [
        "hoimin-v1.2.3-windows-x86_64.zip",
        "hoimin-v1.2.3-linux-x86_64.tar.gz",
        "hoimin-v1.2.3-macos-aarch64.tar.gz",
        "hoimin-1.2.3-py3-none-win_amd64.whl",
        "hoimin-1.2.3-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl",
        "hoimin-1.2.3-py3-none-macosx_11_0_arm64.whl",
    ]
    paths = [directory / name for name in names]
    for path in paths:
        path.write_bytes(path.name.encode())
    return paths


def test_checksums_cover_exactly_all_platform_assets(tmp_path: Path) -> None:
    assets = release_assets(tmp_path)
    assets += attach_sboms(assets)
    result = release(
        tmp_path,
        "checksums",
        "--directory",
        str(tmp_path),
        "--version",
        "1.2.3",
        "--commit",
        "a" * 40,
    )

    assert result.returncode == 0, result.stderr
    assert (tmp_path / "SHA256SUMS").read_text() == "".join(
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
        for path in sorted(assets)
    )


@pytest.mark.parametrize(
    ("damage_checksums", "succeeds", "diagnostic"),
    [
        pytest.param(lambda _sums: None, True, "", id="none"),
        pytest.param(
            lambda sums: sums.unlink(), False, "SHA256SUMS does not match", id="missing"
        ),
        pytest.param(
            lambda sums: sums.write_bytes(b"0" + sums.read_bytes()[1:]),
            False,
            "SHA256SUMS does not match",
            id="digest",
        ),
        pytest.param(
            lambda sums: sums.write_bytes(sums.read_bytes().replace(b"\n", b"\r\n")),
            False,
            "SHA256SUMS does not match",
            id="crlf",
        ),
        pytest.param(
            lambda sums: sums.write_bytes(sums.read_bytes() + b"unexpected\n"),
            False,
            "SHA256SUMS does not match",
            id="extra",
        ),
    ],
)
def test_verifying_checksums_never_repairs_downloaded_bytes(
    tmp_path: Path,
    damage_checksums: Callable[[Path], object],
    *,
    succeeds: bool,
    diagnostic: str,
) -> None:
    assets = release_assets(tmp_path)
    attach_sboms(assets)
    arguments = (
        "checksums",
        "--directory",
        str(tmp_path),
        "--version",
        "1.2.3",
        "--commit",
        "a" * 40,
    )

    assert release(tmp_path, *arguments).returncode == 0
    sums = tmp_path / "SHA256SUMS"
    damage_checksums(sums)
    before = {path.name: path.read_bytes() for path in tmp_path.iterdir()}
    result = release(tmp_path, *arguments, "--verify")

    assert (result.returncode == 0) == succeeds, result.stderr
    assert diagnostic in result.stderr

    assert {path.name: path.read_bytes() for path in tmp_path.iterdir()} == before


@pytest.mark.parametrize(
    "damage_assets",
    [
        pytest.param(lambda assets: assets[0].unlink(), id="missing"),
        pytest.param(lambda assets: assets[0].write_bytes(b""), id="empty"),
        pytest.param(
            lambda assets: (assets[0].parent / "unexpected.txt").write_text("extra"),
            id="extra",
        ),
        pytest.param(
            lambda assets: assets[-1].rename(
                assets[-1].with_name(assets[-1].name.replace("1.2.3", "1.2.4"))
            ),
            id="wrong-version",
        ),
        pytest.param(
            lambda assets: assets[-1].rename(
                assets[-1].parent / "hoimin-1.2.3-py3-none-manylinux2014_x86_64.whl"
            ),
            id="duplicate-platform",
        ),
    ],
)
def test_incomplete_or_mixed_assets_cannot_be_published(
    tmp_path: Path, damage_assets: Callable[[list[Path]], object]
) -> None:
    assets = release_assets(tmp_path)
    damage_assets(assets)

    assert (
        release(
            tmp_path,
            "checksums",
            "--directory",
            str(tmp_path),
            "--version",
            "1.2.3",
            "--commit",
            "a" * 40,
        ).returncode
        != 0
    )
    assert not (tmp_path / "SHA256SUMS").exists()


@pytest.fixture
def publication(
    request: pytest.FixtureRequest, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> tuple[subprocess.CompletedProcess[str], list[list[str]]]:
    assert isinstance(request.param, str)
    state = request.param
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    (fake_bin / "python").symlink_to(sys.executable)
    gh = fake_bin / "gh"
    gh.write_text(
        f"#!{sys.executable}\n"
        "import json, os, pathlib, sys\n"
        "args = sys.argv[1:]\n"
        "with pathlib.Path('calls.jsonl').open('a') as stream:\n"
        "    stream.write(json.dumps(args) + '\\n')\n"
        "state = os.environ['RELEASE_TEST_STATE']\n"
        "if args[:2] == ['release', 'view']:\n"
        "    if state == 'missing' and 'databaseId' not in args: sys.exit(1)\n"
        "    if 'databaseId' in args: print(42)\n"
        "    else: print(json.dumps({'isDraft': state != 'published'}))\n"
        "if args[:2] == ['release', 'upload'] and state == 'upload-fails':\n"
        "    sys.exit(1)\n"
    )
    gh.chmod(0o755)
    monkeypatch.setenv("PATH", str(fake_bin) + os.pathsep + os.environ["PATH"])
    monkeypatch.setenv("RELEASE_TEST_STATE", state)
    monkeypatch.setenv("TAG", "v1.2.3")
    monkeypatch.setenv("GH_REPO", "owner/project")
    dist = tmp_path / "dist"
    dist.mkdir()
    (dist / "payload.zip").write_bytes(b"archive")
    workflow = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())
    step = next(
        step
        for step in workflow["jobs"]["publish"]["steps"]
        if step.get("name") == "Publish complete release"
    )
    result = command("bash", "-e", "-o", "pipefail", "-c", step["run"], cwd=tmp_path)
    calls: list[list[str]] = [
        json.loads(line) for line in (tmp_path / "calls.jsonl").read_text().splitlines()
    ]
    return result, calls


@pytest.mark.skipif(os.name != "posix", reason="GitHub publish job uses Ubuntu bash")
@pytest.mark.parametrize("publication", ["published"], indirect=True)
def test_published_release_preserves_assets(
    publication: tuple[subprocess.CompletedProcess[str], list[list[str]]],
) -> None:
    result, calls = publication

    assert result.returncode == 0, result.stderr
    assert [args[1] for args in calls] == ["view"]


@pytest.mark.skipif(os.name != "posix", reason="GitHub publish job uses Ubuntu bash")
@pytest.mark.parametrize("publication", ["upload-fails"], indirect=True)
def test_failed_upload_leaves_release_unpublished(
    publication: tuple[subprocess.CompletedProcess[str], list[list[str]]],
) -> None:
    result, calls = publication

    assert result.returncode != 0
    assert [args[1] for args in calls] == ["view", "upload"]


@pytest.mark.skipif(os.name != "posix", reason="GitHub publish job uses Ubuntu bash")
@pytest.mark.parametrize(
    ("publication", "expected_operations", "expected_create_flags"),
    [
        ("missing", ["view", "create", "upload", "view", "api"], [(True, True)]),
        ("draft", ["view", "upload", "view", "api"], []),
    ],
    indirect=["publication"],
)
def test_publishing_resumes_drafts(
    publication: tuple[subprocess.CompletedProcess[str], list[list[str]]],
    expected_operations: list[str],
    expected_create_flags: list[tuple[bool, bool]],
) -> None:
    result, calls = publication
    operations = [{"release": args[1], "api": "api"}[args[0]] for args in calls]

    assert result.returncode == 0, result.stderr
    assert operations == expected_operations
    assert calls[-1] == [
        "api",
        "--method",
        "PATCH",
        "repos/owner/project/releases/42",
        "-F",
        "draft=false",
        "-f",
        "make_latest=legacy",
    ]
    assert [
        ("--draft" in args, "--verify-tag" in args)
        for args in calls
        if args[:2] == ["release", "create"]
    ] == expected_create_flags


@pytest.mark.skipif(os.name != "posix", reason="GitHub publish job uses Ubuntu bash")
@pytest.mark.parametrize("order", [("v1.2.3", "v1.2.4"), ("v1.2.4", "v1.2.3")])
def test_publication_uses_server_version_selection_in_both_finish_orders(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, order: tuple[str, str]
) -> None:
    # This double models the documented API contract, not GitHub internals.
    # Its default promotes the last publisher; legacy selects by version.
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    (fake_bin / "python").symlink_to(sys.executable)
    gh = fake_bin / "gh"
    gh.write_text(
        f"#!{sys.executable}\n"
        "import json, os, pathlib, sys\n"
        "args = sys.argv[1:]\n"
        "tag = os.environ['TAG']\n"
        "path = pathlib.Path('server.json')\n"
        "state = json.loads(path.read_text())\n"
        "if args[:2] == ['release', 'view']:\n"
        "    if 'databaseId' in args: print(int(tag.rsplit('.', 1)[1]))\n"
        "    else: print(json.dumps({'isDraft': tag not in state['published']}))\n"
        "elif args[:2] == ['release', 'edit'] or args[0] == 'api':\n"
        "    state['published'].append(tag)\n"
        "    if 'make_latest=legacy' in args:\n"
        "        state['latest'] = max(state['published'],\n"
        "            key=lambda v: tuple(map(int, v[1:].split('.'))))\n"
        "    else: state['latest'] = tag\n"
        "    path.write_text(json.dumps(state))\n"
    )
    gh.chmod(0o755)
    monkeypatch.setenv("PATH", str(fake_bin) + os.pathsep + os.environ["PATH"])
    monkeypatch.setenv("GH_REPO", "owner/project")
    (tmp_path / "server.json").write_text('{"published": [], "latest": null}')
    (tmp_path / "dist").mkdir()
    (tmp_path / "dist/payload.zip").write_bytes(b"archive")
    workflow = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())
    step = next(
        step
        for step in workflow["jobs"]["publish"]["steps"]
        if step.get("name") == "Publish complete release"
    )
    for tag in order:
        monkeypatch.setenv("TAG", tag)
        result = command(
            "bash", "-e", "-o", "pipefail", "-c", step["run"], cwd=tmp_path
        )

        assert result.returncode == 0, result.stderr
    state = json.loads((tmp_path / "server.json").read_text())

    assert state["published"] == list(order)
    assert state["latest"] == "v1.2.4"


def test_release_lock_bytes_match_with_windows_default_newlines(
    repository: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    original = Path.write_text

    def windows_write(
        path: Path,
        data: str,
        *,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
    ) -> int:
        # Reproduce TextIOWrapper's Windows default on any test host.
        return original(
            path,
            data,
            encoding=encoding,
            errors=errors,
            newline={None: "\r\n"}.get(newline, newline),
        )

    monkeypatch.setattr(Path, "write_text", windows_write)

    automation.set_version(repository, "1.2.3")
    lock = (repository / "Cargo.lock").read_bytes()

    assert b'version = "1.2.3"' in lock
    assert b"\r\n" not in lock
