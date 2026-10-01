"""Reserve release tags and prepare verified GitHub Release assets."""

from __future__ import annotations

import argparse
import hashlib
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SEMVER = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
PLATFORMS = ("windows-x86_64", "linux-x86_64", "macos-aarch64")
TAG_ATTEMPTS = 5
RELEASE_STATE_REF = "refs/heads/hoimin-release-state"


def version_tuple(version: str) -> tuple[int, ...]:
    match = re.fullmatch(SEMVER, version)
    if match is None:
        msg = f"expected MAJOR.MINOR.PATCH, got {version!r}"
        raise ValueError(msg)
    return tuple(map(int, match.groups()))


def validate_version(version: str) -> None:
    if re.fullmatch(SEMVER + r"(?:-dev\.(0|[1-9][0-9]*))?", version) is None:
        msg = f"invalid release version: {version!r}"
        raise ValueError(msg)


def git(root: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    # Only fixed git operations and validated release refs reach this boundary.
    return subprocess.run(  # noqa: S603
        ["git", "-C", str(root), *args],  # noqa: S607 -- Git is installed by CI.
        capture_output=True,
        text=True,
        check=check,
        timeout=120,
    )


def remote_release_state(root: Path) -> tuple[dict[str, str], str]:
    refs: dict[str, str] = {}
    for line in git(
        root, "ls-remote", "origin", "refs/tags/v*", RELEASE_STATE_REF
    ).stdout.splitlines():
        commit, ref = line.split()
        refs[ref] = commit
    tags = {}
    for ref, commit in refs.items():
        name = ref.removeprefix("refs/tags/")
        if re.fullmatch("v" + SEMVER, name):
            tags[name] = refs.get(ref + "^{}", commit)
    return tags, refs.get(RELEASE_STATE_REF, "")


def remote_tags(root: Path) -> dict[str, str]:
    return remote_release_state(root)[0]


def follows_commit(root: Path, commit: str, ancestor: str) -> bool:
    # Another run may have tagged a commit absent from this checkout. Fetch its
    # history before comparing; shallow boundaries cannot establish ancestry.
    if git(root, "rev-parse", "--is-shallow-repository").stdout.strip() == "true":
        git(root, "fetch", "--unshallow", "--no-tags", "origin")
    git(root, "fetch", "--no-tags", "origin", ancestor)
    result = git(root, "merge-base", "--is-ancestor", ancestor, commit, check=False)
    if result.returncode == 1:
        return False
    result.check_returncode()
    return True


def reserve_tag(root: Path, commit: str) -> str | None:
    if re.fullmatch(r"[0-9a-f]{40}", commit) is None:
        msg = "a full commit SHA is required"
        raise ValueError(msg)
    git(root, "cat-file", "-e", commit + "^{commit}")
    base = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"][
        "version"
    ]
    base_tuple = version_tuple(base)
    for _ in range(TAG_ATTEMPTS):
        tags, state = remote_release_state(root)
        existing = [tag for tag, sha in tags.items() if sha == commit]
        if existing:
            return max(existing, key=lambda tag: version_tuple(tag[1:]))
        ancestors = {state} if state else set()
        if tags:
            latest = max(tags, key=lambda tag: version_tuple(tag[1:]))
            ancestors.add(tags[latest])
        if any(not follows_commit(root, commit, ancestor) for ancestor in ancestors):
            return None
        versions = [version_tuple(tag[1:]) for tag in tags]
        next_version = base_tuple
        if versions:
            major, minor, patch = max(versions)
            next_version = max(base_tuple, (major, minor, patch + 1))
        tag = "v" + ".".join(map(str, next_version))
        # A shared compare-and-swap serializes reservations even when their
        # manifest version floors differ. The lease applies only to the state
        # branch; immutable version tags are never force-pushed. Both refs must
        # update together, or neither is written.
        pushed = git(
            root,
            "push",
            "--atomic",
            f"--force-with-lease={RELEASE_STATE_REF}:{state}",
            "origin",
            f"{commit}:refs/tags/{tag}",
            f"{commit}:{RELEASE_STATE_REF}",
            check=False,
        )
        if pushed.returncode == 0:
            return tag
        # Re-read remote state, including a successful push whose response was lost.
    tags = remote_tags(root)
    existing = [tag for tag, sha in tags.items() if sha == commit]
    if existing:
        return max(existing, key=lambda tag: version_tuple(tag[1:]))
    msg = "could not reserve a release tag after five attempts"
    raise RuntimeError(msg)


def set_version(root: Path, version: str) -> None:
    validate_version(version)
    python_version = version.replace("-dev.", ".dev")
    edits = (
        ("Cargo.toml", r'(\[workspace\.package\]\nversion = ")[^"]+("\n)', version),
        (
            "pyproject.toml",
            r'(\[project\]\nname = "hoimin"\nversion = ")[^"]+("\n)',
            python_version,
        ),
        (
            "Cargo.lock",
            r'(\[\[package\]\]\nname = "hoimin-cli"\nversion = ")[^"]+("\n)',
            version,
        ),
        (
            "Cargo.lock",
            r'(\[\[package\]\]\nname = "hoimin-core"\nversion = ")[^"]+("\n)',
            version,
        ),
        (
            "uv.lock",
            r'(\[\[package\]\]\nname = "hoimin"\nversion = ")[^"]+("\n)',
            python_version,
        ),
    )
    updated: dict[str, str] = {}
    for name, pattern, replacement in edits:
        contents = updated.get(name, (root / name).read_text(encoding="utf-8"))
        contents, count = re.subn(
            pattern, lambda m, value=replacement: m[1] + value + m[2], contents
        )
        if count != 1:
            msg = f"expected exactly one package version in {name}"
            raise ValueError(msg)
        updated[name] = contents
    # Validate every edit before changing any file. No dependency resolution needed.
    for name, contents in updated.items():
        (root / name).write_text(contents, encoding="utf-8")


def package(
    root: Path, binary: Path, platform: str, version: str, directory: Path
) -> None:
    validate_version(version)
    if platform not in PLATFORMS:
        msg = f"unsupported platform: {platform}"
        raise ValueError(msg)
    if not binary.stat().st_size:
        msg = "empty executable"
        raise ValueError(msg)
    directory.mkdir(parents=True, exist_ok=True)
    prefix = f"hoimin-v{version}-{platform}"
    files = {
        "hoimin.exe" if platform.startswith("windows") else "hoimin": binary,
        "README.md": root / "README.md",
    }
    if platform.startswith("windows"):
        with zipfile.ZipFile(
            directory / (prefix + ".zip"), "w", zipfile.ZIP_DEFLATED
        ) as archive:
            for name, path in files.items():
                archive.write(path, name)
    else:
        with tarfile.open(directory / (prefix + ".tar.gz"), "w:gz") as archive:
            for name, path in files.items():
                archive.add(path, arcname=name)


def checksums(directory: Path, version: str) -> None:
    validate_version(version)
    expected = {
        f"hoimin-v{version}-{platform}."
        + ("zip" if platform.startswith("windows") else "tar.gz")
        for platform in PLATFORMS
    }
    python_version = re.escape(version.replace("-dev.", ".dev"))
    for platform in (r"win_amd64", r"manylinux[^/]*x86_64", r"macosx_[^/]*arm64"):
        pattern = rf"hoimin-{python_version}-[^-]+-[^-]+-{platform}\.whl"
        matches = [
            path.name
            for path in directory.iterdir()
            if re.fullmatch(pattern, path.name)
        ]
        if len(matches) != 1:
            msg = f"expected exactly one wheel for {platform}, got {matches}"
            raise ValueError(msg)
        expected.update(matches)
    actual = {path.name for path in directory.iterdir()} - {"SHA256SUMS"}
    if actual != expected:
        msg = (
            f"unexpected release assets: missing={expected - actual}, "
            f"extra={actual - expected}"
        )
        raise ValueError(msg)
    lines = []
    for name in sorted(expected):
        path = directory / name
        if not path.is_file() or not path.stat().st_size:
            msg = f"empty or non-file release asset: {name}"
            raise ValueError(msg)
        with path.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        lines.append(f"{digest}  {name}\n")
    (directory / "SHA256SUMS").write_text("".join(lines), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    commands = parser.add_subparsers(dest="command", required=True)
    tag = commands.add_parser("tag")
    tag.add_argument("--commit", required=True)
    version = commands.add_parser("set-version")
    version.add_argument("version")
    archive = commands.add_parser("package")
    archive.add_argument("--binary", type=Path, required=True)
    archive.add_argument("--platform", choices=PLATFORMS, required=True)
    archive.add_argument("--version", required=True)
    archive.add_argument("--directory", type=Path, required=True)
    sums = commands.add_parser("checksums")
    sums.add_argument("--directory", type=Path, required=True)
    sums.add_argument("--version", required=True)
    args = parser.parse_args()
    if args.command == "tag":
        tag = reserve_tag(args.root, args.commit)
        if tag is None:
            print(  # noqa: T201 -- Skip diagnostic; stdout stays empty.
                "Skipping release: commit does not follow the latest stable tag.",
                file=sys.stderr,
            )
        else:
            print(tag)  # noqa: T201 -- GitHub output.
    elif args.command == "set-version":
        set_version(args.root, args.version)
    elif args.command == "package":
        package(args.root, args.binary, args.platform, args.version, args.directory)
        for wheel in (args.root / "target/wheels").glob("*.whl"):
            shutil.copyfile(wheel, args.directory / wheel.name)
    else:
        checksums(args.directory, args.version)


if __name__ == "__main__":
    main()
