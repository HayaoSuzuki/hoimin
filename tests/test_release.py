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
from pathlib import Path

import pytest
import yaml

from tools import release as automation

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools/release.py"
VERSION_FILES = ("Cargo.toml", "Cargo.lock", "pyproject.toml", "uv.lock")


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
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "v0.1.0"
    assert git(repository, "ls-remote", "--tags", "origin").startswith(first)
    git(repository, "commit", "--allow-empty", "-m", "second")
    second = git(repository, "rev-parse", "HEAD")
    assert release(repository, "tag", "--commit", second).stdout.strip() == "v0.1.1"
    assert release(repository, "tag", "--commit", first).stdout.strip() == "v0.1.0"


def test_tags_ignore_previews_and_peel_annotated_tags(repository: Path) -> None:
    commit = git(repository, "rev-parse", "HEAD")
    git(repository, "tag", "-a", "v2.3.9", "-m", "release")
    git(repository, "tag", "v99.0.0-dev.1")
    git(repository, "push", "origin", "--tags")
    assert release(repository, "tag", "--commit", commit).stdout.strip() == "v2.3.9"
    git(repository, "commit", "--allow-empty", "-m", "next")
    commit = git(repository, "rev-parse", "HEAD")
    assert release(repository, "tag", "--commit", commit).stdout.strip() == "v2.3.10"


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


def test_tag_collision_retries_without_replacing_other_commit(
    repository: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    first = git(repository, "rev-parse", "HEAD")
    git(repository, "commit", "--allow-empty", "-m", "second")
    second = git(repository, "rev-parse", "HEAD")
    original_git = automation.git
    collided = False

    def competing_git(
        root: Path, *args: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        nonlocal collided
        if args[0] == "push" and not collided:
            collided = True
            original_git(root, "push", "origin", f"{first}:refs/tags/v0.1.0")
        return original_git(root, *args, check=check)

    monkeypatch.setattr(automation, "git", competing_git)
    assert automation.reserve_tag(repository, second) == "v0.1.1"
    assert automation.remote_tags(repository) == {"v0.1.0": first, "v0.1.1": second}


def test_delayed_older_commit_is_skipped_without_creating_a_tag(
    repository: Path,
) -> None:
    older = git(repository, "rev-parse", "HEAD")
    git(repository, "commit", "--allow-empty", "-m", "newer")
    newer = git(repository, "rev-parse", "HEAD")
    assert automation.reserve_tag(repository, newer) == "v0.1.0"
    result = release(repository, "tag", "--commit", older)
    assert result.returncode == 0, result.stderr
    assert not result.stdout.strip()
    assert "Skipping release" in result.stderr
    assert automation.remote_tags(repository) == {"v0.1.0": newer}


def test_newer_commit_winning_tag_race_prevents_older_retry(
    repository: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    older = git(repository, "rev-parse", "HEAD")
    git(repository, "commit", "--allow-empty", "-m", "newer")
    newer = git(repository, "rev-parse", "HEAD")
    original_git = automation.git
    collided = False

    def competing_git(
        root: Path, *args: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        nonlocal collided
        if args[0] == "push" and not collided:
            collided = True
            original_git(root, "push", "origin", f"{newer}:refs/tags/v0.1.0")
        return original_git(root, *args, check=check)

    monkeypatch.setattr(automation, "git", competing_git)
    assert automation.reserve_tag(repository, older) is None
    assert automation.remote_tags(repository) == {"v0.1.0": newer}


@pytest.mark.parametrize("winner", ["older", "newer"])
def test_different_version_floors_share_atomic_reservation(
    repository: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, winner: str
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
    original_git = automation.git
    competed = False

    def competing_git(
        root: Path, *args: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        nonlocal competed
        if args[0] == "push" and not competed:
            competed = True
            if winner == "newer":
                assert automation.reserve_tag(repository, newer) == "v1.0.0"
            else:
                assert automation.reserve_tag(other, older) == "v2.0.0"
        return original_git(root, *args, check=check)

    monkeypatch.setattr(automation, "git", competing_git)
    if winner == "newer":
        assert automation.reserve_tag(other, older) is None
        assert automation.remote_tags(repository) == {"v1.0.0": newer}
    else:
        assert automation.reserve_tag(repository, newer) == "v2.0.1"
        assert automation.remote_tags(repository) == {"v2.0.0": older, "v2.0.1": newer}
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
    assert automation.remote_release_state(repository) == ({}, "")


def test_divergent_history_cannot_get_newer_stable_tag(repository: Path) -> None:
    base = git(repository, "rev-parse", "HEAD")
    git(repository, "commit", "--allow-empty", "-m", "released")
    released = git(repository, "rev-parse", "HEAD")
    assert automation.reserve_tag(repository, released) == "v0.1.0"
    git(repository, "checkout", "-b", "divergent", base)
    git(repository, "commit", "--allow-empty", "-m", "different history")
    assert (
        automation.reserve_tag(repository, git(repository, "rev-parse", "HEAD")) is None
    )
    assert automation.remote_tags(repository) == {"v0.1.0": released}


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
    assert automation.reserve_tag(other, newer) == "v0.1.0"
    assert automation.reserve_tag(repository, older) is None
    assert automation.remote_tags(repository) == {"v0.1.0": newer}


def test_ancestry_errors_are_not_reported_as_successful_skips(
    repository: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    assert (
        automation.reserve_tag(repository, git(repository, "rev-parse", "HEAD"))
        == "v0.1.0"
    )
    git(repository, "commit", "--allow-empty", "-m", "next")
    original_git = automation.git

    def broken_ancestry(
        root: Path, *args: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        if args[0] == "merge-base":
            return subprocess.CompletedProcess(args, 128, "", "corrupt history")
        return original_git(root, *args, check=check)

    monkeypatch.setattr(automation, "git", broken_ancestry)
    with pytest.raises(subprocess.CalledProcessError):
        automation.reserve_tag(repository, git(repository, "rev-parse", "HEAD"))


def test_shallow_checkout_is_deepened_before_ancestry_check(
    repository: Path, tmp_path: Path
) -> None:
    first = git(repository, "rev-parse", "HEAD")
    assert automation.reserve_tag(repository, first) == "v0.1.0"
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
    assert automation.reserve_tag(shallow, newer) == "v0.1.1"
    assert git(shallow, "rev-parse", "--is-shallow-repository") == "false"


@pytest.mark.skipif(os.name != "posix", reason="GitHub prepare job uses Ubuntu bash")
@pytest.mark.parametrize("mode", ["release", "skip", "preview", "error"])
def test_prepare_propagates_skip_and_error_without_publishing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mode: str
) -> None:
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
    shutil.copyfile(ROOT / "Cargo.toml", tmp_path / "Cargo.toml")
    monkeypatch.setenv("PATH", str(fake_bin) + os.pathsep + os.environ["PATH"])
    monkeypatch.setenv("RELEASE_TEST_MODE", mode)
    monkeypatch.setenv("PUBLISH", "false" if mode == "preview" else "true")
    monkeypatch.setenv("COMMIT", "a" * 40)
    monkeypatch.setenv("GITHUB_RUN_ID", "123")
    output = tmp_path / "output"
    monkeypatch.setenv("GITHUB_OUTPUT", str(output))
    workflow = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())
    step = next(
        step
        for step in workflow["jobs"]["prepare"]["steps"]
        if step.get("id") == "version"
    )
    result = command("bash", "-e", "-o", "pipefail", "-c", step["run"], cwd=tmp_path)
    if mode == "error":
        assert result.returncode != 0
        assert not output.exists()
        return
    assert result.returncode == 0, result.stderr
    values = dict(line.split("=", 1) for line in output.read_text().splitlines())
    assert values["publish"] == ("true" if mode == "release" else "false")
    assert values["commit"] == "a" * 40
    expected = {"release": "v1.2.3", "skip": "", "preview": "v0.1.0-dev.123"}[mode]
    assert values["tag"] == expected
    assert values["version"] == expected.removeprefix("v")


def test_tag_push_with_lost_response_is_recovered(
    repository: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    commit = git(repository, "rev-parse", "HEAD")
    original_git = automation.git

    def lost_response(
        root: Path, *args: str, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        result = original_git(root, *args, check=check)
        if args[0] == "push":
            return subprocess.CompletedProcess(result.args, 1, "", "lost response")
        return result

    monkeypatch.setattr(automation, "git", lost_response)
    assert automation.reserve_tag(repository, commit) == "v0.1.0"
    assert automation.remote_tags(repository) == {"v0.1.0": commit}


def test_workspace_version_can_raise_release_floor(repository: Path) -> None:
    git(repository, "tag", "v0.1.8")
    git(repository, "push", "origin", "--tags")
    assert release(repository, "set-version", "1.0.0").returncode == 0
    git(repository, "commit", "-am", "major")
    commit = git(repository, "rev-parse", "HEAD")
    assert release(repository, "tag", "--commit", commit).stdout.strip() == "v1.0.0"


@pytest.mark.parametrize("version", ["1.2.3", "1.2.3-dev.123"])
def test_version_updates_manifests_and_both_lockfiles(
    repository: Path, version: str
) -> None:
    result = release(repository, "set-version", version)
    assert result.returncode == 0, result.stderr
    documents = {
        name: tomllib.loads((repository / name).read_text()) for name in VERSION_FILES
    }
    assert documents["Cargo.toml"]["workspace"]["package"]["version"] == version
    assert documents["pyproject.toml"]["project"]["version"] == version.replace(
        "-dev.", ".dev"
    )
    for name, packages in (
        ("Cargo.lock", {"hoimin-core", "hoimin-cli"}),
        ("uv.lock", {"hoimin"}),
    ):
        original = tomllib.loads((ROOT / name).read_text())["package"]
        for before, after in zip(original, documents[name]["package"], strict=True):
            expected = dict(before)
            if expected["name"] in packages:
                expected["version"] = (
                    version
                    if name == "Cargo.lock"
                    else version.replace("-dev.", ".dev")
                )
            assert after == expected


@pytest.mark.parametrize("version", ["v1.2.3", "01.2.3", "1.2", "1.2.3\n", "1.2.3;id"])
def test_invalid_version_leaves_files_untouched(repository: Path, version: str) -> None:
    before = {name: (repository / name).read_bytes() for name in VERSION_FILES}
    assert release(repository, "set-version", version).returncode != 0
    assert {name: (repository / name).read_bytes() for name in VERSION_FILES} == before


def test_incomplete_metadata_is_rejected_before_any_write(repository: Path) -> None:
    (repository / "uv.lock").write_text("version = 1\n")
    before = {name: (repository / name).read_bytes() for name in VERSION_FILES}
    assert release(repository, "set-version", "1.2.3").returncode != 0
    assert {name: (repository / name).read_bytes() for name in VERSION_FILES} == before


@pytest.mark.parametrize(
    "platform", ["windows-x86_64", "linux-x86_64", "macos-aarch64"]
)
def test_archives_contain_executable_and_readme(tmp_path: Path, platform: str) -> None:
    binary = tmp_path / ("hoimin.exe" if platform.startswith("windows") else "hoimin")
    binary.write_bytes(b"executable payload")
    binary.chmod(0o755)
    (tmp_path / "README.md").write_text("usage")
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
    assert result.returncode == 0, result.stderr
    (archive,) = (tmp_path / "dist").iterdir()
    prefix = f"hoimin-v1.2.3-{platform}"
    if platform.startswith("windows"):
        assert archive.name == prefix + ".zip"
        with zipfile.ZipFile(archive) as zipped:
            assert set(zipped.namelist()) == {binary.name, "README.md"}
            assert zipped.read(binary.name) == binary.read_bytes()
    else:
        assert archive.name == prefix + ".tar.gz"
        with tarfile.open(archive) as packed:
            assert set(packed.getnames()) == {binary.name, "README.md"}
            assert packed.getmember(binary.name).mode & 0o111
            contents = packed.extractfile(binary.name)
            assert contents is not None
            assert contents.read() == binary.read_bytes()


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
    result = release(
        tmp_path, "checksums", "--directory", str(tmp_path), "--version", "1.2.3"
    )
    assert result.returncode == 0, result.stderr
    assert (tmp_path / "SHA256SUMS").read_text() == "".join(
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
        for path in sorted(assets)
    )


@pytest.mark.parametrize(
    "damage", ["missing", "empty", "extra", "wrong-version", "duplicate-platform"]
)
def test_incomplete_or_mixed_assets_cannot_be_published(
    tmp_path: Path, damage: str
) -> None:
    assets = release_assets(tmp_path)
    if damage == "missing":
        assets[0].unlink()
    elif damage == "empty":
        assets[0].write_bytes(b"")
    elif damage == "extra":
        (tmp_path / "unexpected.txt").write_text("extra")
    elif damage == "wrong-version":
        assets[-1].rename(tmp_path / assets[-1].name.replace("1.2.3", "1.2.4"))
    else:
        assets[-1].rename(tmp_path / "hoimin-1.2.3-py3-none-manylinux2014_x86_64.whl")
    assert (
        release(
            tmp_path, "checksums", "--directory", str(tmp_path), "--version", "1.2.3"
        ).returncode
        != 0
    )
    assert not (tmp_path / "SHA256SUMS").exists()


@pytest.mark.skipif(os.name != "posix", reason="GitHub publish job uses Ubuntu bash")
@pytest.mark.parametrize("state", ["missing", "draft", "published", "upload-fails"])
def test_publishing_resumes_drafts_and_preserves_published_assets(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, state: str
) -> None:
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
    calls = [
        json.loads(line) for line in (tmp_path / "calls.jsonl").read_text().splitlines()
    ]
    operations = [args[1] if args[0] == "release" else "api" for args in calls]
    if state == "published":
        assert result.returncode == 0, result.stderr
        assert operations == ["view"]
    elif state == "upload-fails":
        assert result.returncode != 0
        assert operations == ["view", "upload"]
    else:
        assert result.returncode == 0, result.stderr
        assert operations == (
            ["view", "create", "upload", "view", "api"]
            if state == "missing"
            else ["view", "upload", "view", "api"]
        )
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
        if state == "missing":
            assert "--draft" in calls[1]
            assert "--verify-tag" in calls[1]


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
