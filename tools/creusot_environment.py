"""Check a pinned Creusot installation without trusting version exit codes."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

REQUIRED_TOOLS = frozenset(
    {
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
    }
)


class PreflightError(ValueError):
    """An incomplete or drifted verification installation."""


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def object_from_file(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        message = f"expected JSON object: {path}"
        raise PreflightError(message)
    return value


def load_manifest(path: Path) -> dict[str, Any]:  # noqa: C901, PLR0912 -- Schema checks stay together.
    try:
        data = object_from_file(path)
        if type(data.get("schema_version")) is not int or data["schema_version"] != 1:
            message = "unsupported schema_version"
            raise PreflightError(message)
        if (
            not isinstance(data.get("tools"), dict)
            or set(data["tools"]) != REQUIRED_TOOLS
        ):
            message = "tools must contain every required executable"
            raise PreflightError(message)
        for name, tool in data["tools"].items():
            if not isinstance(tool, dict) or not all(
                isinstance(tool.get(key), str) and tool[key]
                for key in ("path", "version")
            ):
                message = f"invalid tool {name}"
                raise PreflightError(message)
            if not Path(tool["path"]).is_absolute():
                message = f"tool path must be absolute: {name}"
                raise PreflightError(message)
            if not isinstance(tool.get("args"), list) or not all(
                isinstance(arg, str) for arg in tool["args"]
            ):
                message = f"invalid arguments for {name}"
                raise PreflightError(message)
        environment = data.get("environment")
        if (
            not isinstance(environment, dict)
            or not environment
            or not all(
                isinstance(key, str) and isinstance(value, str)
                for key, value in environment.items()
            )
        ):
            message = "invalid environment selection"
            raise PreflightError(message)
        for key in ("files", "artifacts"):
            if not isinstance(data.get(key), dict) or not data[key]:
                message = f"missing {key}"
                raise PreflightError(message)
        for name, digest in data["files"].items():
            if not isinstance(name, str) or not valid_hash(digest):
                message = "invalid configuration hash"
                raise PreflightError(message)
        for artifact in data["artifacts"].values():
            if (
                not isinstance(artifact, dict)
                or not isinstance(artifact.get("url"), str)
                or not artifact["url"].startswith("https://")
                or not valid_hash(artifact.get("sha256"))
            ):
                message = "invalid pinned artifact"
                raise PreflightError(message)
        if (
            not isinstance(data.get("receipt"), str)
            or not Path(data["receipt"]).is_absolute()
        ):
            message = "missing receipt path"
            raise PreflightError(message)
    except (OSError, ValueError) as error:
        message = f"manifest {path}: {error}"
        raise PreflightError(message) from error
    return data


def valid_hash(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == hashlib.sha256().digest_size * 2
        and all(char in "0123456789abcdef" for char in value)
    )


def check_environment(path: Path) -> None:
    data = load_manifest(path)
    for name, expected in data["environment"].items():
        if os.environ.get(name) != expected:
            message = (
                f"configuration environment mismatch: {name}; expected {expected!r}"
            )
            raise PreflightError(message)
    try:
        receipt = object_from_file(Path(data["receipt"]))
        if receipt.get("manifest_sha256") != sha256(path):
            message = "manifest hash mismatch"
            raise PreflightError(message)
        hashes = receipt.get("files")
        if not isinstance(hashes, dict) or not hashes:
            message = "missing executable hashes"
            raise PreflightError(message)
    except (OSError, ValueError) as error:
        message = f"receipt {data['receipt']}: {error}"
        raise PreflightError(message) from error
    for filename, expected in {**hashes, **data["files"]}.items():
        file = Path(filename)
        if not valid_hash(expected) or sha256(file) != expected:
            message = f"hash mismatch: {file}; rebuild the pinned environment"
            raise PreflightError(message)
    for name, tool in data["tools"].items():
        check_tool(name, tool, hashes)


def check_tool(name: str, tool: dict[str, Any], hashes: dict[str, Any]) -> None:
    executable = Path(tool["path"])
    if str(executable) not in hashes:
        message = f"missing receipt hash for {name}: {executable}"
        raise PreflightError(message)
    if not os.access(executable, os.X_OK):
        message = f"missing executable {name}: {executable}"
        raise PreflightError(message)
    selected = shutil.which(name)
    if selected is None or Path(selected).resolve() != executable.resolve():
        message = f"PATH selects wrong {name}: {selected}; expected {executable}"
        raise PreflightError(message)
    result = subprocess.run(  # noqa: S603 -- Executable hash was verified above.
        [str(executable), *tool["args"]],
        capture_output=True,
        text=True,
        check=False,
        timeout=15,
    )
    first_line = result.stdout.splitlines()[0] if result.stdout.splitlines() else ""
    if result.returncode != 0 or first_line != tool["version"]:
        message = (
            f"version mismatch for {name}: expected {tool['version']!r}, "
            f"got {first_line!r} (exit {result.returncode}); "
            f"{result.stderr.strip()}"
        )
        raise PreflightError(message)
    sys.stdout.write(f"{name}: {first_line}\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check"])
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    try:
        check_environment(args.manifest)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.stderr.write(f"Creusot environment check failed: {error}\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
