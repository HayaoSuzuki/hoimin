"""Conservative CI planning and strict validation of GitHub job results."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from collections.abc import Mapping
from pathlib import Path
from typing import TypeGuard

CATEGORIES = ("python", "rust", "formal", "distribution")
RELEASE_TOOLS = {"tools/release.py", "tools/sbom.py", "tools/capture_sbom.py"}


def classify(paths: list[str]) -> dict[str, bool]:
    flags = dict.fromkeys(CATEGORIES, False)
    for path in paths:
        for category in categories_for(path):
            flags[category] = True
    return flags


def categories_for(path: str) -> tuple[str, ...]:  # noqa: PLR0911 -- explicit path taxonomy
    if path.startswith(("docs/json-schema/", "tests/fixtures/")):
        return CATEGORIES
    if path.endswith(".md") and (path == "CONTRIBUTING.md" or path.startswith("docs/")):
        return ()
    if path.startswith(("crates/", "vendor/")):
        return ("rust", "formal", "distribution")
    if path == "tests/wheel_smoke.py":
        return CATEGORIES
    if path in RELEASE_TOOLS:
        return ("python", "distribution")
    if path.startswith("tests/") and path.endswith(".py"):
        return ("python", "rust", "formal")
    if path.startswith("formal/"):
        return ("formal", "rust")
    if path in {"LICENSE", "README.md"}:
        return ("distribution",)
    return CATEGORIES


def changed_paths(root: Path, base: str, head: str, *, merge_base: bool) -> list[str]:
    # Revision arguments come from event SHAs, never from branch names or shells.
    if any(not value or value.startswith("-") for value in (base, head)):
        msg = "Missing or invalid revision"
        raise ValueError(msg)
    git = shutil.which("git")
    if git is None:
        msg = "Git is required"
        raise FileNotFoundError(msg)
    result = subprocess.run(  # noqa: S603 -- resolved executable and argv, no shell
        [
            git,
            "-C",
            str(root),
            "diff",
            "--no-renames",
            "--name-only",
            "-z",
            f"{base}{'...' if merge_base else '..'}{head}",
            "--",
        ],
        check=True,
        capture_output=True,
        timeout=60,
    )
    return [os.fsdecode(path) for path in result.stdout.split(b"\0") if path]


def plan(root: Path, event_name: str, event: dict[str, object]) -> dict[str, bool]:
    try:
        if event_name == "pull_request":
            pr = object_mapping(event["pull_request"])
            base = object_mapping(pr["base"])["sha"]
            head = object_mapping(pr["head"])["sha"]
        elif event_name == "merge_group":
            group = object_mapping(event["merge_group"])
            base, head = group["base_sha"], group["head_sha"]
        elif event_name == "push":
            base, head = event["before"], event["after"]
        else:
            return dict.fromkeys(CATEGORIES, True)
        return classify(
            changed_paths(
                root,
                revision(base),
                revision(head),
                merge_base=event_name == "pull_request",
            )
        )
    except (
        KeyError,
        TypeError,
        ValueError,
        OSError,
        subprocess.SubprocessError,
    ) as error:
        print(f"CI diff unavailable; selecting all checks: {error}", file=sys.stderr)  # noqa: T201 -- CLI output contract
        return dict.fromkeys(CATEGORIES, True)


def selected_jobs(flags: dict[str, bool], *, cgroup: bool) -> dict[str, bool]:
    return {
        "quality": True,
        "workflow-lint": True,
        "rust": flags["rust"],
        "rust-shuffle": flags["rust"],
        "contracts": flags["rust"],
        "core-dependency-purity": flags["rust"],
        "linux-best-effort": flags["rust"],
        "fuzz": flags["rust"],
        "lean-audit": flags["formal"],
        "boundary-contracts": flags["formal"],
        "wheel-smoke": flags["distribution"] or flags["python"],
        "linux-cgroup-v2-hard": cgroup,
    }


def revision(value: object) -> str:
    if not isinstance(value, str):
        raise TypeError
    return value


def is_object_mapping(value: object) -> TypeGuard[dict[str, object]]:
    return isinstance(value, dict) and all(isinstance(key, str) for key in value)


def object_mapping(value: object) -> dict[str, object]:
    if not is_object_mapping(value):
        raise TypeError
    return dict(value)


def aggregate(needs: Mapping[str, object]) -> list[str]:
    try:
        changes = object_mapping(needs["changes"])
        if changes["result"] != "success":
            return ["changes did not succeed"]
        outputs = object_mapping(changes["outputs"])
        if any(
            outputs.get(key) not in {"true", "false"} for key in (*CATEGORIES, "cgroup")
        ):
            return ["changes outputs are missing or invalid"]
        flags = {key: outputs[key] == "true" for key in CATEGORIES}
        expected = selected_jobs(flags, cgroup=outputs["cgroup"] == "true")
        errors = []
        for job, required in expected.items():
            result = object_mapping(needs.get(job, {})).get("result")
            allowed = {"success"} if required else {"success", "skipped"}
            if result not in allowed:
                errors.append(f"{job}: expected {sorted(allowed)}, got {result!r}")
    except KeyError, TypeError, ValueError:
        return ["Malformed CI results"]
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "aggregate"))
    args = parser.parse_args()
    if args.command == "aggregate":
        try:
            errors = aggregate(object_mapping(json.load(sys.stdin)))
        except ValueError, TypeError:
            errors = ["Malformed CI results JSON"]
        print("\n".join(errors) if errors else "All required CI jobs succeeded")  # noqa: T201 -- CLI output contract
        return int(bool(errors))
    event_name = os.environ.get("GITHUB_EVENT_NAME", "workflow_dispatch")
    try:
        event = object_mapping(
            json.loads(
                Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8")
            )
        )
        flags = plan(Path.cwd(), event_name, event)
    except KeyError, OSError, ValueError, TypeError:
        flags = dict.fromkeys(CATEGORIES, True)
    cgroup = (
        event_name == "push"
        and os.environ.get("GITHUB_REF") == "refs/heads/main"
        and os.environ.get("CI_CGROUP") == "true"
        and flags["rust"]
    )
    outputs = flags | {"cgroup": cgroup}
    print(json.dumps(outputs, sort_keys=True))  # noqa: T201 -- CLI output contract
    if output := os.environ.get("GITHUB_OUTPUT"):
        with Path(output).open("a", encoding="utf-8") as stream:
            stream.writelines(
                f"{key}={str(value).lower()}\n" for key, value in outputs.items()
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
