"""Supported release targets and version syntax, without tooling dependencies."""

from typing import NamedTuple

SEMVER = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"


class ReleasePlatform(NamedTuple):
    name: str
    rust_target: str
    release_wheel_pattern: str
    published_wheel_tag: str


# Release assembly accepts compatible build tags; PyPI staging requires the
# exact tags built by the stable release workflow.
RELEASE_PLATFORMS = (
    ReleasePlatform(
        "windows-x86_64", "x86_64-pc-windows-msvc", r"win_amd64", "win_amd64"
    ),
    ReleasePlatform(
        "linux-x86_64",
        "x86_64-unknown-linux-gnu",
        r"manylinux[^/]*x86_64",
        "manylinux_2_17_x86_64.manylinux2014_x86_64",
    ),
    ReleasePlatform(
        "macos-aarch64",
        "aarch64-apple-darwin",
        r"macosx_[^/]*arm64",
        "macosx_11_0_arm64",
    ),
)
PLATFORM_NAMES = tuple(platform.name for platform in RELEASE_PLATFORMS)
