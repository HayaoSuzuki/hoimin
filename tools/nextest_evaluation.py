from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import TypedDict, cast

type TestIdentity = tuple[str, str, str, str, bool]
SAMPLE_SECONDS = 0.2
COMMAND_TIMEOUT_SECONDS = 900
ROOT = Path(__file__).resolve().parents[1]


class Case(TypedDict):
    ignored: bool


Suite = TypedDict(
    "Suite",
    {
        "package-name": str,
        "binary-name": str,
        "kind": str,
        "binary-path": str,
        "status": str,
        "testcases": dict[str, Case],
    },
)


class Target(TypedDict):
    name: str
    kind: list[str]


class Profile(TypedDict):
    test: bool


class Artifact(TypedDict):
    reason: str
    executable: str | None
    package_id: str
    target: Target
    profile: Profile


class Package(TypedDict):
    id: str
    name: str


class Metadata(TypedDict):
    packages: list[Package]


NextestList = TypedDict("NextestList", {"rust-suites": dict[str, Suite]})


def tree_usage(rows: list[tuple[int, int, int]], root: int) -> tuple[int, int]:
    members = {root}
    while True:
        children = {pid for pid, parent, _ in rows if parent in members}
        if children <= members:
            break
        members |= children
    return (
        sum(pid in members for pid, _, _ in rows),
        sum(rss for pid, _, rss in rows if pid in members),
    )


def check_inventory(cargo: set[TestIdentity], nextest: set[TestIdentity]) -> None:
    assert cargo == nextest, (
        f"inventory mismatch: Cargo-only={sorted(cargo - nextest)!r}, "
        f"nextest-only={sorted(nextest - cargo)!r}"
    )


def output(argv: list[str]) -> str:
    # Commands originate only in this repository-private evaluation, never a shell.
    return subprocess.check_output(  # noqa: S603 -- fixed local probe commands
        argv,
        text=True,
        timeout=120,
        cwd=ROOT,
    )


def measure(directory: Path, name: str, argv: list[str]) -> None:
    started = time.monotonic()
    peak_processes = peak_rss_kib = samples = 0
    with (
        (directory / f"{name}.log").open("w", encoding="utf-8") as log,
        subprocess.Popen(  # noqa: S603 -- fixed probe commands, no shell
            ["/usr/bin/time", "-v", "-o", str(directory / f"{name}.time"), *argv],
            cwd=ROOT,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        ) as child,
    ):
        try:
            while child.poll() is None:
                if time.monotonic() - started > COMMAND_TIMEOUT_SECONDS:
                    raise subprocess.TimeoutExpired(argv, COMMAND_TIMEOUT_SECONDS)
                rows = [
                    tuple(map(int, line.split()))
                    for line in output(
                        ["/bin/ps", "-eo", "pid=,ppid=,rss="]
                    ).splitlines()
                ]
                typed_rows = [(row[0], row[1], row[2]) for row in rows]
                processes, rss = tree_usage(typed_rows, child.pid)
                peak_processes = max(peak_processes, processes)
                peak_rss_kib = max(peak_rss_kib, rss)
                samples += 1
                time.sleep(SAMPLE_SECONDS)
            code = child.returncode
        finally:
            if child.poll() is None:
                subprocess.run(  # noqa: S603 -- terminate only this probe's process group
                    ["/bin/kill", "-KILL", "--", f"-{child.pid}"],
                    check=False,
                )
                child.wait()
    result = {
        "argv": argv,
        "exit_code": code,
        "wall_seconds": time.monotonic() - started,
        "sample_interval_seconds": SAMPLE_SECONDS,
        "samples": samples,
        "sampled_peak_tree_processes": peak_processes,
        "sampled_peak_tree_rss_kib": peak_rss_kib,
    }
    (directory / f"{name}.json").write_text(
        json.dumps(result, indent=2) + "\n", encoding="utf-8"
    )
    sys.stdout.write(json.dumps({"name": name, **result}) + "\n")
    sys.stdout.flush()
    if code:
        raise subprocess.CalledProcessError(code, argv)


def inventories(directory: Path) -> None:
    metadata = cast(
        "Metadata",
        json.loads(
            output(
                ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"]
            )
        ),
    )
    names = {package["id"]: package["name"] for package in metadata["packages"]}
    artifacts = [
        cast("Artifact", json.loads(line))
        for line in (directory / "cargo-artifacts.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    ]
    cargo: set[TestIdentity] = set()
    binaries: set[str] = set()
    for artifact in artifacts:
        if artifact["reason"] != "compiler-artifact" or not artifact["profile"]["test"]:
            continue
        executable = artifact["executable"]
        if executable is None:
            continue
        binaries.add(executable)
        listed = output([executable, "--list", "--format", "terse"])
        ignored = output([executable, "--list", "--ignored", "--format", "terse"])
        for line in listed.splitlines():
            assert line.endswith(": test"), f"unsupported harness output: {line!r}"
            cargo.add(
                (
                    names[artifact["package_id"]],
                    artifact["target"]["kind"][0],
                    artifact["target"]["name"],
                    line.removesuffix(": test"),
                    line in ignored.splitlines(),
                )
            )
    nextest_json = output(
        [
            "cargo",
            "nextest",
            "list",
            "--workspace",
            "--locked",
            "--message-format",
            "json",
        ]
    )
    (directory / "nextest-list.json").write_text(nextest_json, encoding="utf-8")
    suites = cast("NextestList", json.loads(nextest_json))["rust-suites"]
    assert binaries == {suite["binary-path"] for suite in suites.values()}, (
        "binary inventory mismatch"
    )
    nextest: set[TestIdentity] = set()
    for suite in suites.values():
        assert suite["status"] == "listed", suite
        for name, case in suite["testcases"].items():
            nextest.add(
                (
                    suite["package-name"],
                    suite["kind"],
                    suite["binary-name"],
                    name,
                    case["ignored"],
                )
            )
    check_inventory(cargo, nextest)
    (directory / "inventory.json").write_text(
        json.dumps(sorted(cargo), indent=2) + "\n", encoding="utf-8"
    )
    sys.stdout.write(
        f"Inventory matched: {len(cargo)} tests, {len(binaries)} binaries\n"
    )
    sys.stdout.flush()


def main() -> None:
    directory = Path(os.environ["RUNNER_TEMP"]) / "nextest-evaluation"
    directory.mkdir(exist_ok=True)
    (directory / "environment.txt").write_text(
        output(["rustc", "--version", "--verbose"])
        + output(["cargo", "nextest", "--version"])
        + output(["/usr/bin/nproc"])
        + "RUST_TEST_THREADS="
        + os.environ.get("RUST_TEST_THREADS", "libtest default")
        + "\n",
        encoding="utf-8",
    )
    inventories(directory)
    base = [
        "--locked",
        "-p",
        "hoimin-cli",
        "--test",
        "run_e2e",
        "--test",
        "process_handler",
        "timeout",
    ]
    cargo = ["cargo", "test", *base, "--", "--show-output"]
    nextest = ["cargo", "nextest", "run", "--profile", "ci", *base]
    for name, argv in [
        ("cargo-probe-1", cargo),
        ("nextest-probe-1", nextest),
        ("nextest-probe-2", nextest),
        ("cargo-probe-2", cargo),
    ]:
        measure(directory, name, argv)
    measure(
        directory,
        "cargo-workspace",
        ["cargo", "test", "--workspace", "--locked", "--", "--show-output"],
    )
    measure(
        directory,
        "nextest-workspace",
        ["cargo", "nextest", "run", "--workspace", "--locked", "--profile", "ci"],
    )
    measure(
        directory, "doctests", ["cargo", "test", "--workspace", "--locked", "--doc"]
    )


if __name__ == "__main__":
    main()
